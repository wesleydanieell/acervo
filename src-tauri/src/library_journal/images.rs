//! Exact byte snapshots for journal resources. No resource restoration here.
use super::{Resource, Step};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::Read,
    path::Path,
};

pub(super) const DIRECTORY: &str = "snapshots";
const MAX_BLOB: u64 = 16 * 1024 * 1024;
const MAX_TOTAL: u64 = 64 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Image {
    Missing,
    Bytes { length: u64, sha256: String },
}
impl Image {
    pub(crate) fn of(bytes: Option<&[u8]>) -> Result<Self, String> {
        match bytes {
            None => Ok(Self::Missing),
            Some(bytes) if bytes.len() as u64 <= MAX_BLOB => Ok(Self::Bytes {
                length: bytes.len() as u64,
                sha256: format!("{:x}", Sha256::digest(bytes)),
            }),
            _ => Err("Snapshot excede 16 MiB.".into()),
        }
    }
    pub(super) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Missing => Ok(()),
            Self::Bytes { length, sha256 }
                if *length <= MAX_BLOB && super::digest_valid(sha256) =>
            {
                Ok(())
            }
            _ => Err("Descritor de snapshot inválido.".into()),
        }
    }
}

#[derive(Default)]
pub(crate) struct Images(BTreeMap<String, Vec<u8>>);
impl Images {
    #[allow(dead_code)]
    pub(crate) fn insert(&mut self, bytes: Option<Vec<u8>>) -> Result<Image, String> {
        let image = Image::of(bytes.as_deref())?;
        if let (Image::Bytes { sha256, .. }, Some(bytes)) = (&image, bytes) {
            let total: u64 = self.0.values().map(|b| b.len() as u64).sum();
            if !self.0.contains_key(sha256) && total + bytes.len() as u64 > MAX_TOTAL {
                return Err("Snapshots excedem 64 MiB.".into());
            }
            self.0.entry(sha256.clone()).or_insert(bytes);
        }
        Ok(image)
    }
}

fn required(steps: &[Step]) -> Result<BTreeMap<String, u64>, String> {
    let mut required = BTreeMap::new();
    let mut resources = BTreeSet::new();
    for step in steps {
        if let Step::RewriteReferences {
            resource,
            before,
            after,
        } = step
        {
            if !resources.insert(resource) {
                return Err("Recurso repetido no journal.".into());
            }
            for image in [before, after] {
                image.validate()?;
                if let Image::Bytes { length, sha256 } = image {
                    if required
                        .insert(sha256.clone(), *length)
                        .is_some_and(|old| old != *length)
                    {
                        return Err("Tamanhos de snapshot divergentes.".into());
                    }
                }
            }
        }
    }
    if required.values().sum::<u64>() > MAX_TOTAL {
        return Err("Snapshots excedem 64 MiB.".into());
    }
    Ok(required)
}

pub(super) fn validate_inputs(steps: &[Step], images: &Images) -> Result<(), String> {
    let required = required(steps)?;
    if required.len() != images.0.len() {
        return Err("Conjunto de snapshots incompleto ou excedente.".into());
    }
    for (hash, length) in required {
        let bytes = images.0.get(&hash).ok_or("Snapshot ausente.")?;
        if Image::of(Some(bytes))?
            != (Image::Bytes {
                length,
                sha256: hash,
            })
        {
            return Err("Snapshot divergente.".into());
        }
    }
    Ok(())
}

pub(super) fn persist(dir: &Path, images: &Images) -> Result<(), String> {
    if images.0.is_empty() {
        return Ok(());
    }
    let snapshots = dir.join(DIRECTORY);
    fs::create_dir(&snapshots).map_err(|e| e.to_string())?;
    super::sync_directory(dir)?;
    for (hash, bytes) in &images.0 {
        super::publish(&snapshots, &format!("{hash}.bin"), bytes)?;
    }
    Ok(())
}

pub(super) fn verify(dir: &Path, steps: &[Step]) -> Result<(), String> {
    let required = required(steps)?;
    let snapshots = dir.join(DIRECTORY);
    match fs::symlink_metadata(&snapshots) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound && required.is_empty() => return Ok(()),
        Ok(m) if m.file_type().is_dir() => {}
        _ => return Err("Diretório de snapshots ausente ou inválido.".into()),
    }
    let mut seen = BTreeSet::new();
    for entry in fs::read_dir(&snapshots).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Nome de snapshot inválido.")?;
        let hash = name
            .strip_suffix(".bin")
            .ok_or("Nome de snapshot inválido.")?;
        let length = *required.get(hash).ok_or("Snapshot não referenciado.")?;
        if !seen.insert(hash.to_owned()) {
            return Err("Snapshot repetido.".into());
        }
        let file = crate::library_editions::open_regular(&entry.path())?;
        let mut bytes = Vec::new();
        file.take(length + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if Image::of(Some(&bytes))?
            != (Image::Bytes {
                length,
                sha256: hash.into(),
            })
        {
            return Err("Snapshot corrompido.".into());
        }
    }
    if seen.len() != required.len() {
        return Err("Snapshot ausente.".into());
    }
    Ok(())
}

/// Exact bytes of supported library-owned JSON resources. Never copy a live SQLite
/// database file: WAL/transaction semantics need a dedicated logical snapshot.
#[allow(dead_code)]
pub(crate) fn capture_json(root: &Path, resource: Resource) -> Result<Option<Vec<u8>>, String> {
    let name = match resource {
        Resource::Lists => "Acervo_Listas.json",
        Resource::ManualMetadata => "Acervo_Metadados_Manuais.json",
        Resource::Editions => "Acervo_Edicoes.json",
        _ => return Err("Recurso exige adaptador de snapshot próprio.".into()),
    };
    let Some(service) = crate::library_editions::service(root)? else {
        return Ok(None);
    };
    let path = service.join(name);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Ok(m) if m.file_type().is_file() && m.len() <= MAX_BLOB => {}
        _ => return Err("Recurso inválido ou excede limite.".into()),
    }
    let mut bytes = Vec::new();
    crate::library_editions::open_regular(&path)?
        .take(MAX_BLOB + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Image::of(Some(&bytes))?;
    Ok(Some(bytes))
}

pub(super) fn capture_resource(root: &Path, resource: Resource) -> Result<Option<Vec<u8>>, String> {
    match resource {
        Resource::LegacyCatalog => super::sqlite::capture_bytes(root),
        _ => capture_json(root, resource),
    }
}
