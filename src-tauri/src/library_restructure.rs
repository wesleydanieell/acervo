//! Read-only structural planning. Execution deliberately lives outside this module.
mod snapshot;
use serde::{Deserialize, Serialize};
use snapshot::Observations;
use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

const MAX_ENTRIES: usize = 50_000;
const MAX_OPERATIONS: usize = 100;

#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub enum StructuralOperation {
    CreateFolder { path: String },
    MoveFolder { source: String, target: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderImpact {
    source: String,
    target: String,
    folders: usize,
    files: usize,
    books: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuralPreview {
    version: u8,
    filesystem_revision: String,
    created_folders: Vec<String>,
    moved_folders: Vec<FolderImpact>,
    execution_supported: bool,
    reference_impact: crate::library_reference_impact::ReferenceImpact,
    download_impact: Option<crate::downloads::restructure::DownloadImpact>,
    rule_impact: Option<Vec<crate::smart_organization::RuleDestinationImpact>>,
}

fn relative(value: &str) -> Result<PathBuf, String> {
    if value.is_empty()
        || crate::downloads::destination_parts(value).is_err()
        || value.split('/').any(|part| {
            part.is_empty()
                || part == "."
                || part == ".."
                || part.starts_with('.')
                || part == "99 - Catálogo"
                || part == "node_modules"
                || part.len() > 255
                || part
                    .chars()
                    .any(|c| c.is_control() || c == '\\' || c == ':')
        })
    {
        return Err(format!("Caminho inválido ou protegido: {value}"));
    }
    Ok(PathBuf::from(value))
}

fn inspect(
    root: &Path,
    path: &Path,
    observations: &mut Observations,
) -> Result<Option<fs::Metadata>, String> {
    let mut current = root.to_path_buf();
    let mut last = None;
    for component in path.components() {
        current.push(component);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(format!("Link simbólico: {}", current.display()));
            }
            Ok(meta) => {
                observations.observe(root, &current, Some(&meta))?;
                last = Some(meta);
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                observations.observe(root, &current, None)?;
                return Ok(None);
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(last)
}

fn overlapping(a: &Path, b: &Path) -> bool {
    a.starts_with(b) || b.starts_with(a)
}

fn inventory(
    root: &Path,
    source: &Path,
    target: &Path,
    remaining: &mut usize,
    observations: &mut Observations,
) -> Result<FolderImpact, String> {
    let mut impact = FolderImpact {
        source: source.to_string_lossy().into_owned(),
        target: target.to_string_lossy().into_owned(),
        folders: 1,
        files: 0,
        books: 0,
    };
    let mut pending = vec![root.join(source)];
    while let Some(folder) = pending.pop() {
        let before = fs::symlink_metadata(&folder).map_err(|e| e.to_string())?;
        if !before.file_type().is_dir() {
            return Err("Pasta mudou durante o inventário.".into());
        }
        observations.observe(root, &folder, Some(&before))?;
        for entry in fs::read_dir(&folder).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            *remaining = remaining
                .checked_sub(1)
                .ok_or("Inventário excede o limite de 50.000 entradas.")?;
            let path = entry.path();
            let meta = fs::symlink_metadata(&path).map_err(|e| e.to_string())?;
            observations.observe(root, &path, Some(&meta))?;
            let kind = meta.file_type();
            if kind.is_symlink() || (!kind.is_dir() && !kind.is_file()) {
                return Err(format!(
                    "Entrada especial não suportada: {}",
                    entry.path().display()
                ));
            }
            if kind.is_dir() {
                impact.folders += 1;
                pending.push(entry.path());
            } else {
                impact.files += 1;
                if crate::is_book(&entry.path()) {
                    impact.books += 1;
                }
            }
        }
        let after = fs::symlink_metadata(&folder).map_err(|e| e.to_string())?;
        observations.observe(root, &folder, Some(&after))?;
    }
    Ok(impact)
}

fn plan(root: &Path, operations: Vec<StructuralOperation>) -> Result<StructuralPreview, String> {
    if operations.is_empty() || operations.len() > MAX_OPERATIONS {
        return Err("Use de 1 a 100 operações estruturais.".into());
    }
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("Biblioteca inválida.".into());
    }
    let mut observations = Observations::default();
    observations.observe(
        &root,
        &root,
        Some(&fs::symlink_metadata(&root).map_err(|e| e.to_string())?),
    )?;
    let ordered_operations = operations.clone();
    let mut created = HashSet::<PathBuf>::new();
    let mut reserved = Vec::<PathBuf>::new();
    let mut preview = StructuralPreview {
        version: 1,
        filesystem_revision: String::new(),
        created_folders: vec![],
        moved_folders: vec![],
        execution_supported: false,
        reference_impact: Default::default(),
        download_impact: None,
        rule_impact: None,
    };
    let mut remaining = MAX_ENTRIES;
    for operation in operations {
        let (source, target) = match operation {
            StructuralOperation::CreateFolder { path } => (None, relative(&path)?),
            StructuralOperation::MoveFolder { source, target } => {
                (Some(relative(&source)?), relative(&target)?)
            }
        };
        if reserved.iter().any(|p| overlapping(p, &target)) || created.contains(&target) {
            return Err("Destino conflitante com outra operação.".into());
        }
        if inspect(&root, &target, &mut observations)?.is_some() {
            return Err(format!("Destino existente: {}", target.display()));
        }
        let parent = target.parent().ok_or("Destino inválido.")?;
        if !parent.as_os_str().is_empty()
            && !created.contains(parent)
            && !inspect(&root, parent, &mut observations)?.is_some_and(|m| m.is_dir())
        {
            return Err(format!("Pasta pai inexistente: {}", parent.display()));
        }
        if let Some(source) = source {
            if overlapping(&source, &target)
                || reserved.iter().any(|p| overlapping(p, &source))
                || created.iter().any(|p| overlapping(p, &source))
            {
                return Err("Sobreposição de subárvores não suportada.".into());
            }
            if !inspect(&root, &source, &mut observations)?.is_some_and(|m| m.is_dir()) {
                return Err(format!("Origem não é uma pasta: {}", source.display()));
            }
            preview.moved_folders.push(inventory(
                &root,
                &source,
                &target,
                &mut remaining,
                &mut observations,
            )?);
            reserved.push(source);
            reserved.push(target);
        } else {
            preview
                .created_folders
                .push(target.to_string_lossy().into_owned());
            created.insert(target);
        }
    }
    let remap = crate::library_reference_impact::PathRemap::new(
        preview
            .moved_folders
            .iter()
            .map(|m| (m.source.as_str(), m.target.as_str()))
            .collect(),
    );
    preview.reference_impact = crate::library_reference_impact::preview(&root, &remap)?;
    observations.observe(
        &root,
        &root,
        Some(&fs::symlink_metadata(&root).map_err(|e| e.to_string())?),
    )?;
    preview.filesystem_revision = observations.revision(&root, &ordered_operations)?;
    Ok(preview)
}

/// Preview only: no persistent token or authorization to execute is returned.
#[tauri::command]
pub async fn preview_library_restructure(
    root: String,
    operations: Vec<StructuralOperation>,
    manager: tauri::State<'_, crate::downloads::DownloadManager>,
    rules: Option<crate::smart_organization::Rules>,
    expected_filesystem_revision: Option<String>,
) -> Result<StructuralPreview, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
        let mut preview = plan(&root, operations)?;
        snapshot::verify_revision(
            &preview.filesystem_revision,
            expected_filesystem_revision.as_deref(),
        )?;
        let remap = crate::library_reference_impact::PathRemap::new(
            preview
                .moved_folders
                .iter()
                .map(|m| (m.source.as_str(), m.target.as_str()))
                .collect(),
        );
        preview.download_impact = Some(crate::downloads::restructure::preview(
            &root, &remap, &manager,
        )?);
        preview.rule_impact = rules
            .as_ref()
            .map(|rules| crate::smart_organization::preview_restructure_rules(rules, &remap))
            .transpose()?;
        Ok(preview)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root = std::env::temp_dir().join(format!(
                "acervo-restructure-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            for path in ["A/Vazia", "AB", "99 - Catálogo"] {
                fs::create_dir_all(root.join(path)).unwrap();
            }
            fs::write(root.join("A/livro.epub"), b"book").unwrap();
            fs::write(root.join("A/notas.txt"), b"notes").unwrap();
            Self(root)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn mv(source: &str, target: &str) -> StructuralOperation {
        StructuralOperation::MoveFolder {
            source: source.into(),
            target: target.into(),
        }
    }
    #[test]
    fn filesystem_revision_detects_external_edits_and_plan_changes() {
        let f = Fixture::new();
        let revision = |destination| {
            plan(&f.0, vec![mv("A", destination)])
                .unwrap()
                .filesystem_revision
        };
        let first = revision("New");
        assert_eq!(first.len(), 64);
        assert_eq!(first, revision("New"));
        assert_ne!(first, revision("Other"));
        assert!(snapshot::verify_revision(&first, Some(&first)).is_ok());
        fs::write(f.0.join("A/livro.epub"), b"changed contents").unwrap();
        let edited = revision("New");
        assert_ne!(edited, first);
        assert!(snapshot::verify_revision(&edited, Some(&first)).is_err());
        fs::create_dir(f.0.join("A/Vazia/Nova")).unwrap();
        let added = revision("New");
        assert_ne!(added, edited);
        fs::remove_dir(f.0.join("A/Vazia/Nova")).unwrap();
        assert_ne!(revision("New"), added);
        fs::create_dir(f.0.join("New")).unwrap();
        assert!(plan(&f.0, vec![mv("A", "New")]).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn filesystem_revision_detects_equal_size_replacement_and_destination_parent() {
        let f = Fixture::new();
        let revision = || {
            plan(&f.0, vec![mv("A", "AB/New")])
                .unwrap()
                .filesystem_revision
        };
        let first = revision();
        fs::write(f.0.join("A/replacement.epub"), b"book").unwrap();
        fs::rename(f.0.join("A/replacement.epub"), f.0.join("A/livro.epub")).unwrap();
        let replaced = revision();
        assert_ne!(first, replaced);
        fs::rename(f.0.join("AB"), f.0.join("AB-old")).unwrap();
        fs::create_dir(f.0.join("AB")).unwrap();
        assert_ne!(replaced, revision());
    }

    #[test]
    fn previews_nested_destination_and_auxiliary_files_without_writing() {
        let f = Fixture::new();
        let p = plan(
            &f.0,
            vec![
                StructuralOperation::CreateFolder {
                    path: "Assuntos".into(),
                },
                mv("A", "Assuntos/Álgebra"),
            ],
        )
        .unwrap();
        assert_eq!(p.moved_folders[0].folders, 2);
        assert_eq!(p.moved_folders[0].files, 2);
        assert_eq!(p.moved_folders[0].books, 1);
        assert!(!p.execution_supported);
        assert!(f.0.join("A/Vazia").is_dir());
        assert!(!f.0.join("Assuntos").exists());
    }
    #[test]
    fn previews_catalog_and_index_references_including_missing_works_without_writes() {
        let f = Fixture::new();
        let root = fs::canonicalize(&f.0).unwrap();
        let service = root.join("99 - Catálogo");
        let list = service.join("Acervo_Listas.json");
        fs::write(&list, r#"{"versao":1,"obras":[{"titulo":"Present","local_indicado":"AB","arquivos_encontrados":["A/livro.epub","AB/other.pdf"]},{"titulo":"Future","local_indicado":"A/Future"},{"titulo":"Unchanged","local_indicado":"AB"}]}"#).unwrap();
        let db = service.join("Catalogo_Conhecimento.db");
        let connection = rusqlite::Connection::open(&db).unwrap();
        connection.execute_batch(r#"CREATE TABLE livros (id INTEGER PRIMARY KEY,titulo TEXT,autor TEXT,editoras_indicadas TEXT,local_indicado TEXT,arquivos_encontrados TEXT,prioridade_alta INTEGER);
            INSERT INTO livros VALUES(42,'Legacy future','','','A/Not yet',NULL,1);"#).unwrap();
        drop(connection);
        let manual = service.join("Acervo_Metadados_Manuais.json");
        fs::write(&manual, r#"{"version":1,"revision":1,"entries":{"A/livro.epub":{"metadata":{"title":"Manual"},"identity":{"bytes":"4","modifiedNs":"1","createdNs":"1","device":"1","inode":"1"}}}}"#).unwrap();
        crate::library_editions::record(
            &root,
            crate::library_editions::EditionRecord {
                path: "A/livro.epub".into(),
                source_name: "Fixture".into(),
                metadata: crate::downloads::DownloadEdition {
                    title: "Book".into(),
                    ..Default::default()
                },
                fingerprint: crate::import_fingerprint::fingerprint(&root.join("A/livro.epub"))
                    .unwrap(),
            },
        )
        .unwrap();
        let edition = service.join("Acervo_Edicoes.json");
        let paths = [&list, &db, &manual, &edition];
        let before: Vec<_> = paths.iter().map(|p| fs::read(p).unwrap()).collect();
        let p = plan(&root, vec![mv("A", "New")]).unwrap();
        let impact = p.reference_impact;
        assert_eq!(impact.works.len(), 3);
        let present = impact.works.iter().find(|w| w.work_id == -1).unwrap();
        assert_eq!(present.associated_files[0].after, "New/livro.epub");
        assert_eq!(present.associated_files.len(), 1);
        assert!(present.planned_destination.is_none());
        let future = impact.works.iter().find(|w| w.work_id == -2).unwrap();
        assert_eq!(
            future.planned_destination.as_ref().unwrap().after,
            "New/Future"
        );
        let legacy = impact.works.iter().find(|w| w.work_id == 42).unwrap();
        assert_eq!(
            legacy.planned_destination.as_ref().unwrap().after,
            "New/Not yet"
        );
        assert_eq!(impact.manual_metadata[0].after, "New/livro.epub");
        assert_eq!(impact.editions[0].after, "New/livro.epub");
        for (path, expected) in paths.iter().zip(before) {
            assert_eq!(fs::read(path).unwrap(), expected);
        }
        assert!(root.join("A/livro.epub").exists());
        assert!(!root.join("New").exists());
    }

    #[test]
    fn rejects_ambiguous_catalog_identity_and_malformed_associations() {
        let f = Fixture::new();
        let list = f.0.join("99 - Catálogo/Acervo_Listas.json");
        fs::write(
            &list,
            r#"[{"titulo":"Same","local_indicado":"A"},{"titulo":"Same","local_indicado":"New"}]"#,
        )
        .unwrap();
        assert!(plan(&f.0, vec![mv("A", "New")]).is_err());
        fs::write(
            &list,
            r#"[{"titulo":"Malformed","arquivos_encontrados":[5]}]"#,
        )
        .unwrap();
        assert!(plan(&f.0, vec![mv("A", "New")]).is_err());
    }

    #[test]
    fn rejects_conflicts_cycles_protected_paths_and_missing_parents() {
        let f = Fixture::new();
        for (a, b) in [
            ("A", "AB"),
            ("A", "A/New"),
            ("A", "../Other"),
            ("99 - Catálogo", "C"),
            ("A", "Unknown/C"),
            ("A", "/outside"),
            ("A", "A"),
        ] {
            assert!(plan(&f.0, vec![mv(a, b)]).is_err(), "{a} -> {b}");
        }
        assert!(plan(&f.0, vec![mv("A", "New"), mv("AB", "New")]).is_err());
        assert!(plan(&f.0, vec![mv("A", "New"), mv("A/Vazia", "Other")]).is_err());
        assert!(plan(&f.0, vec![mv("A", "New"), mv("AB", "Other")]).is_ok());
    }
    #[cfg(unix)]
    #[test]
    fn rejects_symlink_ancestors_and_descendants() {
        let f = Fixture::new();
        std::os::unix::fs::symlink(f.0.join("AB"), f.0.join("Link")).unwrap();
        assert!(plan(&f.0, vec![mv("A", "Link/New")]).is_err());
        std::os::unix::fs::symlink(f.0.join("AB"), f.0.join("A/Link")).unwrap();
        assert!(plan(&f.0, vec![mv("A", "New")]).is_err());
    }
}
