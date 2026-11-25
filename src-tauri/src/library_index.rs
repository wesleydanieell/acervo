//! Disposable raw discovery for one canonical root. Hydration is always fresh.
use crate::{BookFile, Collection, hydrate_collection, scan_dir, scan_selected};
use std::{
    collections::{BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

const MAX_CHANGES: usize = 1024;
const MAX_FILES: usize = 150_000;
static INDEX: Mutex<Option<Snapshot>> = Mutex::new(None);

#[derive(Clone)]
struct Snapshot {
    root: PathBuf,
    files: Vec<BookFile>,
    folders: BTreeSet<String>,
    warnings: Vec<String>,
}

pub(super) fn refresh(root: String, changes: Option<Vec<String>>) -> Result<Collection, String> {
    // Hold through discovery AND hydration/publication: an older full scan must
    // never overwrite a newer incremental result (or vice versa).
    let mut cache = INDEX
        .lock()
        .map_err(|_| "Índice da biblioteca indisponível.")?;
    refresh_with(&mut cache, root, changes)
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0', ':'])
        && Path::new(name).components().count() == 1
        && matches!(
            Path::new(name).components().next(),
            Some(std::path::Component::Normal(_))
        )
}

fn full(root: PathBuf) -> Snapshot {
    let mut result = Snapshot {
        root,
        files: Vec::new(),
        folders: BTreeSet::new(),
        warnings: Vec::new(),
    };
    scan_dir(
        &result.root,
        &result.root,
        &mut result.files,
        &mut result.folders,
        &mut result.warnings,
        0,
    );
    result
}

fn refresh_with(
    cache: &mut Option<Snapshot>,
    root: String,
    changes: Option<Vec<String>>,
) -> Result<Collection, String> {
    // Invalidate even on canonicalization failure; never retain a snapshot from
    // a failed request for another root.
    let previous = cache.take();
    let root = fs::canonicalize(root).map_err(|e| format!("Pasta inválida: {e}"))?;
    if !root.is_dir() {
        return Err("Escolha uma pasta de biblioteca válida.".into());
    }
    let names = changes
        .filter(|names| names.len() <= MAX_CHANGES && names.iter().all(|n| valid_name(n)))
        .map(|names| names.into_iter().collect::<HashSet<_>>());
    let mut raw = match (previous, names) {
        (Some(mut old), Some(names)) if old.root == root && old.warnings.is_empty() => {
            let affected = |path: &str| names.contains(path.split('/').next().unwrap_or(path));
            old.files.retain(|file| !affected(&file.path));
            old.folders.retain(|folder| !affected(folder));
            if !names.is_empty() {
                // Enumerate the root to use precisely the same entry handling as
                // full scans, including vanished names, exclusions and symlinks.
                scan_selected(
                    &root,
                    &root,
                    &mut old.files,
                    &mut old.folders,
                    &mut old.warnings,
                    0,
                    Some(&names),
                );
            }
            if !old.warnings.is_empty() || old.files.len() > MAX_FILES {
                full(root)
            } else {
                old
            }
        }
        _ => full(root),
    };
    // Stable ordering also makes equal-title results independent of event order.
    raw.files.sort_by(|a, b| a.path.cmp(&b.path));
    let result = hydrate_collection(
        &raw.root,
        raw.files.clone(),
        raw.folders.clone(),
        raw.warnings.clone(),
    )?;
    // A warning may indicate incomplete discovery or invalid sidecar hydration.
    // Neither partial results nor errors become a future incremental baseline.
    if result.warnings.is_empty() && raw.files.len() <= MAX_FILES {
        *cache = Some(raw);
    }
    Ok(result)
}

#[cfg(test)]
thread_local! {
    static READS: std::cell::RefCell<Vec<PathBuf>> = const { std::cell::RefCell::new(Vec::new()) };
}
#[cfg(test)]
pub(super) fn record_read(path: &Path) {
    READS.with(|reads| reads.borrow_mut().push(path.to_owned()));
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn disposable_incremental_discovery_matches_full() {
        let temp = Fixture(std::env::temp_dir().join(format!(
                "acervo-index-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        let root = temp.0.join("one");
        let other = temp.0.join("two");
        for dir in [
            root.join("A/nested"),
            root.join("B"),
            other.clone(),
            root.join(".hidden"),
            root.join("node_modules"),
            root.join("99 - Catálogo"),
        ] {
            fs::create_dir_all(dir).unwrap();
        }
        for file in [
            "A/nested/a.pdf",
            "B/b.pdf",
            "root.epub",
            ".hidden/no.pdf",
            "node_modules/no.pdf",
        ] {
            fs::write(root.join(file), b"book").unwrap();
        }
        let mut cache = None;
        let request = |cache: &mut Option<Snapshot>, root: &Path, names: Option<Vec<&str>>| {
            refresh_with(
                cache,
                root.to_str().unwrap().into(),
                names.map(|v| v.into_iter().map(String::from).collect()),
            )
            .unwrap()
        };
        request(&mut cache, &root, None);
        fs::write(root.join("A/nested/new.pdf"), b"new").unwrap();
        READS.with(|r| r.borrow_mut().clear());
        let updated = request(&mut cache, &root, Some(vec!["A"]));
        READS.with(|r| {
            let reads = r.borrow();
            assert!(reads.contains(&root.join("A/nested")));
            assert!(!reads.contains(&root.join("B")));
        });
        let equal_full = |value: Collection, root: &Path| {
            assert_eq!(
                serde_json::to_value(value).unwrap(),
                serde_json::to_value(
                    crate::scan_collection(root.to_str().unwrap().into()).unwrap()
                )
                .unwrap()
            )
        };
        equal_full(updated, &root);
        fs::remove_dir_all(root.join("A")).unwrap();
        fs::rename(root.join("B"), root.join("Renamed")).unwrap();
        fs::remove_file(root.join("root.epub")).unwrap();
        fs::write(root.join("added.pdf"), b"added").unwrap();
        equal_full(
            request(
                &mut cache,
                &root,
                Some(vec![
                    "A",
                    "B",
                    "Renamed",
                    "root.epub",
                    "added.pdf",
                    ".hidden",
                    "node_modules",
                ]),
            ),
            &root,
        );
        READS.with(|r| r.borrow_mut().clear());
        let catalog_only = request(&mut cache, &root, Some(vec![]));
        READS.with(|r| assert!(r.borrow().is_empty()));
        equal_full(catalog_only, &root);
        fs::write(other.join("other.pdf"), b"other").unwrap();
        equal_full(request(&mut cache, &other, Some(vec![])), &other);
        equal_full(request(&mut cache, &root, Some(vec!["../outside"])), &root);
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&other, root.join("link")).unwrap();
            equal_full(request(&mut cache, &root, Some(vec!["link"])), &root);
        }
    }
}
