//! Deterministic metadata observations for a read-only structural plan.
//! Not a content hash, lock, or authorization to perform filesystem writes.
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Identity {
    directory: bool,
    bytes: u64,
    modified_ns: i128,
    created_ns: Option<i128>,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
}

fn nanos(time: SystemTime) -> i128 {
    match time.duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_nanos() as i128,
        Err(error) => -(error.duration().as_nanos() as i128),
    }
}

impl Identity {
    fn from_metadata(meta: &fs::Metadata) -> Result<Self, String> {
        if !meta.is_file() && !meta.is_dir() || meta.file_type().is_symlink() {
            return Err("Entrada especial ou simbólica no inventário.".into());
        }
        #[cfg(unix)]
        use std::os::unix::fs::MetadataExt;
        Ok(Self {
            directory: meta.is_dir(),
            bytes: meta.len(),
            modified_ns: nanos(meta.modified().map_err(|e| e.to_string())?),
            created_ns: meta.created().ok().map(nanos),
            #[cfg(unix)]
            device: meta.dev(),
            #[cfg(unix)]
            inode: meta.ino(),
            #[cfg(unix)]
            changed_seconds: meta.ctime(),
            #[cfg(unix)]
            changed_nanoseconds: meta.ctime_nsec(),
        })
    }
}

#[derive(Default)]
pub(super) struct Observations {
    entries: BTreeMap<String, Option<Identity>>,
}

fn key(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "Caminho fora da raiz.")?;
    relative
        .components()
        .map(|c| {
            c.as_os_str()
                .to_str()
                .ok_or_else(|| "Nome de arquivo não UTF-8.".to_string())
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
}

impl Observations {
    pub(super) fn observe(
        &mut self,
        root: &Path,
        path: &Path,
        meta: Option<&fs::Metadata>,
    ) -> Result<(), String> {
        let key = key(root, path)?;
        let identity = meta.map(Identity::from_metadata).transpose()?;
        if let Some(previous) = self.entries.get(&key) {
            if previous != &identity {
                return Err(format!("A biblioteca mudou durante a prévia: {key}"));
            }
        } else {
            self.entries.insert(key, identity);
        }
        Ok(())
    }

    /// Digest includes operation order and canonical root, as well as all observed
    /// identities and absent targets. Sorted paths avoid read_dir ordering effects.
    pub(super) fn revision(
        &self,
        root: &Path,
        operations: &impl Serialize,
    ) -> Result<String, String> {
        let root = root.to_str().ok_or("Raiz não UTF-8.")?;
        let encoded = serde_json::to_vec(&(1u8, root, operations, &self.entries))
            .map_err(|e| e.to_string())?;
        Ok(format!("{:x}", Sha256::digest(encoded)))
    }
}

pub(super) fn verify_revision(actual: &str, expected: Option<&str>) -> Result<(), String> {
    if expected.is_some_and(|expected| expected != actual) {
        return Err(
            "A estrutura física mudou ou o plano foi editado. Gere uma nova prévia.".into(),
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn observation_order_does_not_affect_revision_and_plan_order_does() {
        let root = std::env::temp_dir();
        let meta = fs::metadata(&root).unwrap();
        let mut a = Observations::default();
        let mut b = Observations::default();
        a.observe(&root, &root, Some(&meta)).unwrap();
        a.observe(&root, &root.join("missing"), None).unwrap();
        b.observe(&root, &root.join("missing"), None).unwrap();
        b.observe(&root, &root, Some(&meta)).unwrap();
        let rev = a.revision(&root, &["a", "b"]).unwrap();
        assert_eq!(rev, b.revision(&root, &["a", "b"]).unwrap());
        assert_ne!(rev, b.revision(&root, &["b", "a"]).unwrap());
        assert!(
            a.observe(&root, &root.join("missing"), Some(&meta))
                .is_err()
        );
    }
}
