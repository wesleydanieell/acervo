//! Disposable directory listings, never hydrated metadata or mutation authority.
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};
const VERSION: u32 = 2;
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 200_000;
const CACHE: &str = "library-discovery-v2.json";

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Stamp {
    device: u64,
    inode: u64,
    seconds: i64,
    nanos: i64,
    changed_seconds: i64,
    changed_nanos: i64,
}
fn stamp(path: &Path) -> Option<Stamp> {
    let meta = fs::symlink_metadata(path).ok()?;
    if !meta.is_dir() || meta.file_type().is_symlink() {
        return None;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        Some(Stamp {
            device: meta.dev(),
            inode: meta.ino(),
            seconds: meta.mtime(),
            nanos: meta.mtime_nsec(),
            changed_seconds: meta.ctime(),
            changed_nanos: meta.ctime_nsec(),
        })
    }
    // Without a stable directory identity, enumerate instead of trusting mtime.
    #[cfg(not(unix))]
    {
        None
    }
}
#[derive(Serialize, Deserialize)]
struct Listing {
    stamp: Stamp,
    names: Vec<String>,
}
#[derive(Serialize, Deserialize)]
pub(super) struct Index {
    version: u32,
    root: PathBuf,
    directories: BTreeMap<String, Listing>,
    #[serde(skip)]
    count: usize,
    #[serde(skip)]
    seen: BTreeSet<String>,
}
fn component(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', ':', '\0'])
        && matches!(
            Path::new(name).components().next(),
            Some(std::path::Component::Normal(_))
        )
}
fn relative(path: &str) -> bool {
    path.is_empty() || (path.split('/').count() <= 81 && path.split('/').all(component))
}
impl Index {
    pub(super) fn empty(root: &Path) -> Self {
        Self {
            version: VERSION,
            root: root.into(),
            directories: BTreeMap::new(),
            count: 0,
            seen: BTreeSet::new(),
        }
    }
    pub(super) fn load(dir: &Path, root: &Path) -> Self {
        let empty = || Self::empty(root);
        let read = || -> Option<Self> {
            let path = dir.join(CACHE);
            let meta = fs::symlink_metadata(&path).ok()?;
            if !meta.is_file() || meta.len() > MAX_BYTES {
                return None;
            }
            let mut bytes = Vec::new();
            crate::library_editions::open_regular(&path)
                .ok()?
                .take(MAX_BYTES + 1)
                .read_to_end(&mut bytes)
                .ok()?;
            if bytes.len() as u64 > MAX_BYTES {
                return None;
            }
            let mut index: Self = serde_json::from_slice(&bytes).ok()?;
            if index.version != VERSION
                || index.root != root
                || index.directories.len() > MAX_ENTRIES
            {
                return None;
            }
            let mut count = index.directories.len();
            for (path, listing) in &index.directories {
                count = count.checked_add(listing.names.len())?;
                if count > MAX_ENTRIES
                    || !relative(path)
                    || !listing.names.iter().all(|n| component(n))
                {
                    return None;
                }
                let unique: std::collections::BTreeSet<_> = listing.names.iter().collect();
                if unique.len() != listing.names.len() {
                    return None;
                }
            }
            index.count = count;
            Some(index)
        };
        read().unwrap_or_else(empty)
    }
    pub(super) fn save(&mut self, dir: &Path) {
        self.directories.retain(|key, _| self.seen.contains(key));
        // Cache failure must never fail discovery. A unique create_new temporary
        // file and rename avoid following an existing cache file or partial JSON.
        let write = || -> io::Result<()> {
            fs::create_dir_all(dir)?;
            let bytes = serde_json::to_vec(self)?;
            if bytes.len() as u64 > MAX_BYTES {
                return Ok(());
            }
            let temp = dir.join(format!(
                ".library-{}-{}.tmp",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos()
            ));
            let result = (|| {
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temp)?;
                file.write_all(&bytes)?;
                file.sync_all()?;
                drop(file);
                fs::rename(&temp, dir.join(CACHE))
            })();
            let _ = fs::remove_file(temp);
            result
        };
        let _ = write();
    }
}
// Live entries retain the cheap file_type fast path of read_dir.
// Cached names require fresh metadata (including symlink checks).
pub(super) enum Entry {
    Live(fs::DirEntry),
    Cached(PathBuf),
}
impl Entry {
    pub(super) fn path(&self) -> PathBuf {
        match self {
            Self::Live(entry) => entry.path(),
            Self::Cached(path) => path.clone(),
        }
    }
    pub(super) fn file_type(&self) -> io::Result<fs::FileType> {
        match self {
            Self::Live(entry) => entry.file_type(),
            Self::Cached(path) => fs::symlink_metadata(path).map(|meta| meta.file_type()),
        }
    }
    pub(super) fn metadata(&self) -> io::Result<fs::Metadata> {
        match self {
            Self::Live(entry) => entry.metadata(),
            Self::Cached(path) => fs::symlink_metadata(path),
        }
    }
}
type Entries = Box<dyn Iterator<Item = io::Result<Entry>>>;
fn enumerate(current: &Path) -> io::Result<Entries> {
    #[cfg(test)]
    crate::library_index::record_read(current);
    Ok(Box::new(
        fs::read_dir(current)?.map(|entry| entry.map(Entry::Live)),
    ))
}
pub(super) fn listing(
    root: &Path,
    current: &Path,
    disk: Option<&mut Index>,
) -> io::Result<Entries> {
    let Some(index) = disk else {
        return enumerate(current);
    };
    let key = current.strip_prefix(root).ok().and_then(|relative| {
        relative
            .components()
            .map(|part| part.as_os_str().to_str().filter(|name| component(name)))
            .collect::<Option<Vec<_>>>()
            .map(|parts| parts.join("/"))
    });
    let (Some(key), Some(before)) = (key, stamp(current)) else {
        return enumerate(current);
    };
    index.seen.insert(key.clone());
    if let Some(old) = index
        .directories
        .get(&key)
        .filter(|old| old.stamp == before)
    {
        let paths: Vec<_> = old
            .names
            .iter()
            .map(|name| Ok(Entry::Cached(current.join(name))))
            .collect();
        return Ok(Box::new(paths.into_iter()));
    }
    if let Some(old) = index.directories.remove(&key) {
        index.count -= old.names.len() + 1;
    }
    // No cache capacity: enumerate normally, without truncating discovery.
    let remaining = MAX_ENTRIES.saturating_sub(index.count + 1);
    if remaining == 0 {
        return enumerate(current);
    }
    let mut entries = enumerate(current)?;
    let mut paths = Vec::new();
    let mut names = Vec::new();
    let mut cacheable = true;
    while let Some(entry) = entries.next() {
        if paths.len() >= remaining {
            // Return the already-enumerated prefix plus the remaining stream.
            // The cache cap is not a library discovery cap.
            return Ok(Box::new(
                paths
                    .into_iter()
                    .chain(std::iter::once(entry))
                    .chain(entries),
            ));
        }
        match &entry {
            Ok(entry) => match entry
                .path()
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| component(name))
            {
                Some(name) => names.push(name.to_owned()),
                None => cacheable = false,
            },
            Err(_) => cacheable = false,
        }
        paths.push(entry);
    }
    if cacheable && stamp(current).as_ref() == Some(&before) {
        index.count += names.len() + 1;
        index.directories.insert(
            key,
            Listing {
                stamp: before,
                names,
            },
        );
    }
    Ok(Box::new(paths.into_iter()))
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
    fn cache_capacity_never_truncates_discovery() {
        let temp = Fixture(std::env::temp_dir().join(format!(
                "acervo-cache-cap-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        fs::create_dir(&temp.0).unwrap();
        for file in ["One.pdf", "Two.epub", "Three.pdf"] {
            fs::write(temp.0.join(file), b"book").unwrap();
        }
        let root = fs::canonicalize(&temp.0).unwrap();
        let mut index = Index::load(&root, &root);
        index.count = MAX_ENTRIES - 2;
        let found = listing(&root, &root, Some(&mut index))
            .unwrap()
            .collect::<io::Result<Vec<_>>>()
            .unwrap();
        assert_eq!(found.len(), 3);
        assert!(index.directories.is_empty());
    }

    #[test]
    fn offline_changes_and_invalid_caches_match_full_discovery() {
        let temp = Fixture(std::env::temp_dir().join(format!(
                "acervo-disk-{}-{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            )));
        let root = temp.0.join("library");
        let cache = temp.0.join("cache");
        fs::create_dir_all(root.join("A/deep")).unwrap();
        fs::create_dir_all(root.join("B")).unwrap();
        fs::write(root.join("A/deep/one.pdf"), b"one").unwrap();
        fs::write(root.join("B/two.epub"), b"two").unwrap();
        let root = fs::canonicalize(root).unwrap();
        let scan = || {
            crate::library_index::initial(root.to_str().unwrap().into(), Some(cache.clone()))
                .unwrap()
        };
        let equal = |value| {
            assert_eq!(
                serde_json::to_value(value).unwrap(),
                serde_json::to_value(
                    crate::scan_collection(root.to_str().unwrap().into()).unwrap()
                )
                .unwrap()
            )
        };
        equal(scan());
        crate::library_index::READS.with(|r| r.borrow_mut().clear());
        let unchanged = scan();
        #[cfg(unix)]
        crate::library_index::READS.with(|r| {
            assert!(
                r.borrow().is_empty(),
                "unchanged tree must not use read_dir"
            )
        });
        equal(unchanged);
        // Force an unambiguous directory mtime change even on coarse filesystems.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        fs::write(root.join("A/deep/new.pdf"), b"new").unwrap();
        fs::rename(root.join("A/deep/one.pdf"), root.join("A/deep/renamed.pdf")).unwrap();
        fs::write(root.join("B/two.epub"), b"resized contents").unwrap();
        crate::library_index::READS.with(|r| r.borrow_mut().clear());
        let changed = scan();
        #[cfg(unix)]
        crate::library_index::READS.with(|r| {
            assert!(r.borrow().contains(&root.join("A/deep")));
            assert!(!r.borrow().contains(&root.join("B")));
        });
        equal(changed);
        fs::remove_file(root.join("A/deep/new.pdf")).unwrap();
        equal(scan());
        fs::write(cache.join(CACHE), b"{broken").unwrap();
        equal(scan());
        let mut wrong = Index::load(&cache, &root);
        wrong.root = temp.0.join("other");
        wrong.seen = wrong.directories.keys().cloned().collect();
        wrong.save(&cache);
        assert!(Index::load(&cache, &root).directories.is_empty());
        equal(scan());
        #[cfg(unix)]
        {
            let mut invalid = Index::load(&cache, &root);
            invalid
                .directories
                .get_mut("")
                .unwrap()
                .names
                .push("../escape.pdf".into());
            invalid.seen = invalid.directories.keys().cloned().collect();
            invalid.save(&cache);
            assert!(Index::load(&cache, &root).directories.is_empty());
            equal(scan());
            let mut stale = Index::load(&cache, &root);
            stale
                .directories
                .get_mut("")
                .unwrap()
                .names
                .push("Ghost.pdf".into());
            stale.seen = stale.directories.keys().cloned().collect();
            stale.save(&cache);
            equal(scan());
        }
        #[cfg(not(unix))]
        assert!(Index::load(&cache, &root).directories.is_empty());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
            equal(scan());
        }
        let file = fs::File::create(cache.join(CACHE)).unwrap();
        file.set_len(MAX_BYTES + 1).unwrap();
        assert!(Index::load(&cache, &root).directories.is_empty());
    }
}
