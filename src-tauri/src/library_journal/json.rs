//! Structural rewrites of app-owned JSON. Existing files only; no deletion.
use super::{Image, Images, Resource, Step, images};
use crate::library_reference_impact::PathRemap;
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Write,
    path::Path,
    sync::atomic::Ordering,
};

fn validate(resource: Resource, bytes: &[u8]) -> Result<(), String> {
    Image::of(Some(bytes))?;
    match resource {
        Resource::Lists => {
            crate::catalog_lists::parse_list(
                std::str::from_utf8(bytes).map_err(|e| e.to_string())?,
            )?;
        }
        Resource::ManualMetadata => {
            crate::manual_metadata::decode_index(bytes)?;
        }
        Resource::Editions => {
            crate::library_editions::decode_index(bytes)?;
        }
        _ => return Err("Recurso JSON não suportado.".into()),
    }
    Ok(())
}
fn lists(bytes: &[u8], remap: &PathRemap<'_>) -> Result<Option<Vec<u8>>, String> {
    validate(Resource::Lists, bytes)?;
    let mut data: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let works = if data.is_array() {
        data.as_array_mut()
    } else {
        data.get_mut("obras").and_then(Value::as_array_mut)
    }
    .ok_or("Lista inválida.")?;
    let mut changed = false;
    let mut identities = HashMap::new();
    for work in works {
        let obj = work.as_object_mut().ok_or("Ficha inválida.")?;
        let mut destination_changed = false;
        if let Some(value) = obj.get_mut("local_indicado") {
            if !value.is_null() {
                let path = value.as_str().ok_or("Destino inválido.")?;
                if let Some(change) = remap.map(path)? {
                    *value = Value::String(change.after);
                    changed = true;
                    destination_changed = true;
                }
            }
        }
        if let Some(value) = obj.get_mut("arquivos_encontrados") {
            if !value.is_null() {
                let paths = value.as_array_mut().ok_or("Associações inválidas.")?;
                let mut seen = HashMap::new();
                for value in paths {
                    let old = value.as_str().ok_or("Associação inválida.")?.to_owned();
                    let new = remap.map(&old)?.map_or_else(|| old.clone(), |c| c.after);
                    if seen
                        .insert(new.clone(), old.clone())
                        .is_some_and(|p| p != old)
                    {
                        return Err("Associações conflitantes.".into());
                    }
                    if old != new {
                        *value = Value::String(new);
                        changed = true;
                    }
                }
            }
        }
        let identity = ["titulo", "autor", "local_indicado"].map(|k| {
            obj.get(k)
                .and_then(Value::as_str)
                .unwrap_or("")
                .trim()
                .to_lowercase()
        });
        if let Some(previous) = identities.insert(identity, destination_changed) {
            if previous || destination_changed {
                return Err("Identidade ambígua de ficha.".into());
            }
        }
    }
    if !changed {
        return Ok(None);
    }
    let result = serde_json::to_vec_pretty(&data).map_err(|e| e.to_string())?;
    validate(Resource::Lists, &result)?;
    Ok(Some(result))
}
fn proposed(
    resource: Resource,
    bytes: &[u8],
    remap: &PathRemap<'_>,
) -> Result<Option<Vec<u8>>, String> {
    match resource {
        Resource::Lists => lists(bytes, remap),
        Resource::ManualMetadata => {
            let mut index = crate::manual_metadata::decode_index(bytes)?;
            let mut entries = BTreeMap::new();
            let mut changed = false;
            for (old, record) in std::mem::take(&mut index.entries) {
                let new = remap.map(&old)?.map_or_else(|| old.clone(), |c| c.after);
                changed |= old != new;
                if entries.insert(new, record).is_some() {
                    return Err("Conflito de metadados manuais.".into());
                }
            }
            if !changed {
                return Ok(None);
            }
            index.entries = entries;
            index.revision = index.revision.checked_add(1).ok_or("Revisão esgotada.")?;
            let bytes = serde_json::to_vec(&index).map_err(|e| e.to_string())?;
            validate(resource, &bytes)?;
            Ok(Some(bytes))
        }
        Resource::Editions => {
            let mut index = crate::library_editions::decode_index(bytes)?;
            let mut entries = BTreeMap::new();
            let mut changed = false;
            for (old, mut record) in std::mem::take(&mut index.entries) {
                let new = remap.map(&old)?.map_or_else(|| old.clone(), |c| c.after);
                changed |= old != new;
                record.path = new.clone();
                if entries.insert(new, record).is_some() {
                    return Err("Conflito de edições.".into());
                }
            }
            if !changed {
                return Ok(None);
            }
            index.entries = entries;
            let bytes = serde_json::to_vec(&index).map_err(|e| e.to_string())?;
            validate(resource, &bytes)?;
            Ok(Some(bytes))
        }
        _ => Err("Recurso JSON não suportado.".into()),
    }
}
#[allow(dead_code)]
pub(super) fn plan(
    root: &Path,
    resource: Resource,
    remap: &PathRemap<'_>,
    images: &mut Images,
) -> Result<Option<Step>, String> {
    let Some(before) = images::capture_json(root, resource)? else {
        return Ok(None);
    };
    let Some(after) = proposed(resource, &before, remap)? else {
        return Ok(None);
    };
    Ok(Some(Step::RewriteReferences {
        resource,
        before: images.insert(Some(before))?,
        after: images.insert(Some(after))?,
    }))
}

/// Internal only. Caller holds the root reservation. External concurrent writes
/// are not fully excluded by the final compare; executor must account for this.
#[allow(dead_code)]
pub(super) fn replace(
    root: &Path,
    resource: Resource,
    expected: &[u8],
    desired: &[u8],
) -> Result<(), String> {
    validate(resource, expected)?;
    validate(resource, desired)?;
    let current = images::capture_json(root, resource)?.ok_or("Recurso ausente.")?;
    if current == desired {
        return Ok(());
    }
    if current != expected {
        return Err("Recurso divergiu do snapshot esperado.".into());
    }
    let dir = crate::library_editions::service(root)?.ok_or("Pasta de serviço ausente.")?;
    super::sync_directory(&dir)?;
    let target = dir.join(images::json_name(resource)?);
    let permissions = fs::symlink_metadata(&target)
        .map_err(|e| e.to_string())?
        .permissions();
    let (temp, mut file) = loop {
        let temp = dir.join(format!(
            ".acervo-rewrite-{}-{}.tmp",
            std::process::id(),
            super::NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(file) => break (temp, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let result = (|| {
        file.write_all(desired).map_err(|e| e.to_string())?;
        file.set_permissions(permissions)
            .map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        if images::capture_json(root, resource)?.as_deref() != Some(expected) {
            return Err("Recurso mudou durante preparação.".into());
        }
        fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        super::sync_directory(&dir)?;
        if images::capture_json(root, resource)?.as_deref() != Some(desired) {
            return Err("Pós-condição JSON divergente.".into());
        }
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::super::tests::Fixture;
    use super::*;
    #[cfg(unix)]
    #[test]
    fn exact_lists_roundtrip_preserves_envelope_and_unknown_fields() {
        let f = Fixture::new();
        let root = &f.0;
        let service = root.join("99 - Catálogo");
        fs::create_dir(&service).unwrap();
        let path = service.join("Acervo_Listas.json");
        let before=br#"{ "extra":{"keep":true},"obras":[{"titulo":"A","local_indicado":"A/Future","arquivos_encontrados":["A/book.pdf","AB/book.pdf"],"tag":"keep"},{"titulo":"Absent","local_indicado":"A","arquivos_encontrados":null}] }"#;
        fs::write(&path, before).unwrap();
        let map = PathRemap::new(vec![("A", "New")]);
        let after = proposed(Resource::Lists, before, &map).unwrap().unwrap();
        let v: Value = serde_json::from_slice(&after).unwrap();
        assert_eq!(v["extra"]["keep"], true);
        assert_eq!(v["obras"][0]["tag"], "keep");
        assert_eq!(v["obras"][0]["arquivos_encontrados"][0], "New/book.pdf");
        assert_eq!(v["obras"][0]["arquivos_encontrados"][1], "AB/book.pdf");
        assert_eq!(v["obras"][1]["local_indicado"], "New");
        assert!(v["obras"][1]["arquivos_encontrados"].is_null());
        let guard = crate::library_mutations::Reservation::acquire(root).unwrap();
        let mut images = Images::default();
        assert!(
            plan(guard.root(), Resource::Lists, &map, &mut images)
                .unwrap()
                .is_some()
        );
        assert_eq!(fs::read(&path).unwrap(), before);
        replace(root, Resource::Lists, before, &after).unwrap();
        replace(root, Resource::Lists, before, &after).unwrap();
        replace(root, Resource::Lists, &after, before).unwrap();
        assert_eq!(fs::read(&path).unwrap(), before);
        fs::write(&path, br#"[{"titulo":"external"}]"#).unwrap();
        assert!(replace(root, Resource::Lists, before, &after).is_err());
        assert_eq!(fs::read(&path).unwrap(), br#"[{"titulo":"external"}]"#);
    }
    #[test]
    fn indexes_remap_keys_and_edition_path_without_losing_identity() {
        let f = Fixture::new();
        let root = &f.0;
        fs::write(root.join("A/book.pdf"), b"book").unwrap();
        let identity = crate::manual_metadata::identity(root, "A/book.pdf").unwrap();
        let manual=serde_json::to_vec(&serde_json::json!({"version":1,"revision":9,"entries":{"A/book.pdf":{"identity":identity,"metadata":{"title":"Manual"}}}})).unwrap();
        let map = PathRemap::new(vec![("A", "New")]);
        let after = proposed(Resource::ManualMetadata, &manual, &map)
            .unwrap()
            .unwrap();
        let m = crate::manual_metadata::decode_index(&after).unwrap();
        assert_eq!(m.revision, 10);
        assert_eq!(m.entries["New/book.pdf"].identity, identity);
        let fingerprint = crate::import_fingerprint::fingerprint(&root.join("A/book.pdf")).unwrap();
        let editions=serde_json::to_vec(&serde_json::json!({"version":1,"entries":{"A/book.pdf":{"path":"A/book.pdf","sourceName":"Fixture","metadata":{"title":"Book"},"fingerprint":fingerprint}}})).unwrap();
        let after = proposed(Resource::Editions, &editions, &map)
            .unwrap()
            .unwrap();
        let e = crate::library_editions::decode_index(&after).unwrap();
        assert_eq!(e.entries["New/book.pdf"].path, "New/book.pdf");
        assert_eq!(e.entries["New/book.pdf"].fingerprint, fingerprint);
    }
    #[test]
    fn conflicts_and_unaffected_resources_are_not_rewritten() {
        let map = PathRemap::new(vec![("A", "New")]);
        assert!(proposed(Resource::Lists,br#"[{"titulo":"same","local_indicado":"A"},{"titulo":"same","local_indicado":"New"}]"#,&map).is_err());
        assert!(
            proposed(
                Resource::Lists,
                br#"[{"titulo":"same","arquivos_encontrados":["A/book.pdf","New/book.pdf"]}]"#,
                &map
            )
            .is_err()
        );
        assert!(
            proposed(
                Resource::Lists,
                br#"[{"titulo":"same","local_indicado":"AB"}]"#,
                &map
            )
            .unwrap()
            .is_none()
        );
    }
}
