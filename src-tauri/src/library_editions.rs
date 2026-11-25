//! Per-library edition provenance, independent of download history.
use crate::downloads::DownloadEdition;
use crate::import_fingerprint::{FileFingerprint, fingerprint};
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{MapAccess, Visitor},
};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    Mutex,
    atomic::{AtomicU64, Ordering},
};

const SERVICE: &str = "99 - Catálogo";
const INDEX: &str = "Acervo_Edicoes.json";
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
static MUTATIONS: Mutex<()> = Mutex::new(());
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EditionRecord {
    pub path: String,
    pub source_name: String,
    pub metadata: DownloadEdition,
    pub fingerprint: FileFingerprint,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    version: u32,
    #[serde(deserialize_with = "read_entries")]
    entries: BTreeMap<String, EditionRecord>,
}

fn read_entries<'de, D: Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, EditionRecord>, D::Error> {
    struct Entries;
    impl<'de> Visitor<'de> for Entries {
        type Value = BTreeMap<String, EditionRecord>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("a bounded map of unique edition paths")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
            let mut result = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                if result.len() >= MAX_ENTRIES || result.contains_key(&key) {
                    return Err(serde::de::Error::custom(
                        "excessive or duplicate edition entries",
                    ));
                }
                result.insert(key, map.next_value()?);
            }
            Ok(result)
        }
    }
    d.deserialize_map(Entries)
}

pub(crate) fn valid_path(path: &str) -> Result<(), String> {
    if path.is_empty()
        || path.len() > 855
        || Path::new(path).is_absolute()
        || path.as_bytes().get(1) == Some(&b':')
        || path.split('/').count() > 81
        || path.split('/').any(|part| {
            let normalized = part
                .trim()
                .to_lowercase()
                .replace(['–', '—'], "-")
                .replace('á', "a");
            part.is_empty()
                || part.len() > 255
                || part.trim() != part
                || part.starts_with('.')
                || part.contains('\\')
                || (cfg!(windows) && part.contains(':'))
                || part.chars().any(char::is_control)
                || part == "node_modules"
                || normalized
                    .split_once('-')
                    .is_some_and(|(a, b)| a.trim() == "99" && b.trim() == "catalogo")
        })
    {
        return Err("Caminho de edição inválido ou protegido.".into());
    }
    Ok(())
}

fn validate(entry: &EditionRecord) -> Result<(), String> {
    valid_path(&entry.path)?;
    if entry.source_name.trim().is_empty()
        || entry.source_name.len() > 500
        || entry.source_name.chars().any(char::is_control)
    {
        return Err("Nome da fonte inválido.".into());
    }
    entry.metadata.validate()?;
    entry.fingerprint.validate()
}

pub(crate) fn canonical_root(root: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(root).map_err(|e| e.to_string())?;
    if !metadata.file_type().is_dir()
        || fs::canonicalize(root)
            .map_err(|e| e.to_string())?
            .as_os_str()
            != root.as_os_str()
    {
        return Err("A raiz deve ser uma pasta canônica, sem link simbólico.".into());
    }
    Ok(())
}

// Missing service data is an empty index; malformed or redirected data is an error.
pub(crate) fn service(root: &Path) -> Result<Option<PathBuf>, String> {
    let path = root.join(SERVICE);
    match fs::symlink_metadata(&path) {
        Ok(m) if m.file_type().is_dir() => Ok(Some(path)),
        Ok(_) => Err("Pasta de serviço inválida.".into()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn open_regular(path: &Path) -> Result<File, String> {
    if !fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err("O índice deve ser um arquivo regular, sem link simbólico.".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    if !file
        .metadata()
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err("Índice não regular.".into());
    }
    Ok(file)
}

pub(crate) fn load(root: &Path) -> Result<BTreeMap<String, EditionRecord>, String> {
    canonical_root(root)?;
    let Some(directory) = service(root)? else {
        return Ok(BTreeMap::new());
    };
    let path = directory.join(INDEX);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let file = open_regular(&path)?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("Índice de edições excede 16 MiB.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Índice de edições excede 16 MiB.".into());
    }
    let envelope: Envelope = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if envelope.version != 1 || envelope.entries.len() > MAX_ENTRIES {
        return Err("Versão ou tamanho de índice inválido.".into());
    }
    for (key, entry) in &envelope.entries {
        validate(entry)?;
        if key != &entry.path {
            return Err("Chave de edição divergente do caminho.".into());
        }
    }
    Ok(envelope.entries)
}

fn verify_book(root: &Path, relative: &str, expected: &FileFingerprint) -> Result<(), String> {
    valid_path(relative)?;
    let mut path = root.to_path_buf();
    let parts: Vec<_> = relative.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        path.push(part);
        let kind = fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type();
        if (i + 1 == parts.len() && !kind.is_file()) || (i + 1 != parts.len() && !kind.is_dir()) {
            return Err("Caminho físico de edição inválido ou simbólico.".into());
        }
    }
    if fs::canonicalize(&path).map_err(|e| e.to_string())? != path
        || fingerprint(&path)? != *expected
    {
        return Err("Arquivo de edição não corresponde à impressão digital.".into());
    }
    Ok(())
}

struct LimitedBuffer(Vec<u8>);
impl Write for LimitedBuffer {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > MAX_BYTES as usize - self.0.len() {
            return Err(std::io::Error::other("Índice de edições excede 16 MiB."));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub(crate) fn persist_json<T: Serialize>(
    root: &Path,
    index: &str,
    envelope: &T,
) -> Result<(), String> {
    let mut buffer = LimitedBuffer(Vec::new());
    serde_json::to_writer(&mut buffer, envelope).map_err(|e| e.to_string())?;
    canonical_root(root)?;
    if service(root)?.is_none() {
        match fs::create_dir(root.join(SERVICE)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let directory = service(root)?.ok_or("Pasta de serviço ausente.")?;
    let target = directory.join(index);
    let check_target = || -> Result<(), String> {
        match fs::symlink_metadata(&target) {
            Ok(m) if m.file_type().is_file() => Ok(()),
            Ok(_) => Err("Índice não regular ou simbólico.".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    };
    check_target()?;
    let (temp, mut file) = loop {
        let temp = directory.join(format!(
            ".{index}-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        match OpenOptions::new().write(true).create_new(true).open(&temp) {
            Ok(file) => break (temp, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let result = (|| {
        file.write_all(&buffer.0).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        canonical_root(root)?;
        service(root)?.ok_or("Pasta de serviço ausente.")?;
        check_target()?;
        fs::rename(&temp, &target).map_err(|e| e.to_string())?;
        // Once rename succeeds the visible index is committed. A directory sync
        // failure must not cause the caller to undo a successful file move.
        #[cfg(unix)]
        let _ = File::open(&directory).and_then(|f| f.sync_all());
        Ok(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn persist(root: &Path, entries: BTreeMap<String, EditionRecord>) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err("Índice excede 20000 edições.".into());
    }
    persist_json(
        root,
        INDEX,
        &Envelope {
            version: 1,
            entries,
        },
    )
}

pub(crate) fn record(root: &Path, entry: EditionRecord) -> Result<(), String> {
    let _guard = MUTATIONS
        .lock()
        .map_err(|_| "Índice de edições indisponível.")?;
    validate(&entry)?;
    let mut entries = load(root)?;
    verify_book(root, &entry.path, &entry.fingerprint)?;
    if let Some(existing) = entries.get(&entry.path) {
        return if existing == &entry {
            Ok(())
        } else {
            Err("Edição conflitante no destino.".into())
        };
    }
    entries.insert(entry.path.clone(), entry);
    persist(root, entries)
}

/// Called only after the caller has physically renamed the book.
pub(crate) fn sync_move(root: &Path, old: &str, new: &str) -> Result<(), String> {
    let _guard = MUTATIONS
        .lock()
        .map_err(|_| "Índice de edições indisponível.")?;
    let mut entries = load(root)?;
    let Some(mut entry) = entries.get(old).cloned() else {
        if entries.contains_key(new) {
            return Err("Edição conflitante no destino.".into());
        }
        return Ok(());
    };
    valid_path(old)?;
    valid_path(new)?;
    verify_book(root, new, &entry.fingerprint)?;
    if old == new {
        return Ok(());
    }
    if entries.contains_key(new) {
        return Err("Edição conflitante no destino.".into());
    }
    entries.remove(old);
    entry.path = new.into();
    entries.insert(new.into(), entry);
    persist(root, entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Library(PathBuf);
    impl Library {
        fn new() -> Self {
            loop {
                let path = std::env::temp_dir().join(format!(
                    "acervo-editions-test-{}-{}",
                    std::process::id(),
                    NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(fs::canonicalize(path).unwrap()),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => panic!("{e}"),
                }
            }
        }
        fn book(&self, name: &str, bytes: &[u8]) -> EditionRecord {
            let path = self.0.join(name);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, bytes).unwrap();
            EditionRecord {
                path: name.into(),
                source_name: "Test source".into(),
                metadata: DownloadEdition {
                    title: "Test book".into(),
                    ..Default::default()
                },
                fingerprint: fingerprint(&path).unwrap(),
            }
        }
        fn index(&self) -> PathBuf {
            self.0.join(SERVICE).join(INDEX)
        }
        fn raw(&self, bytes: &[u8]) {
            fs::create_dir_all(self.0.join(SERVICE)).unwrap();
            fs::write(self.index(), bytes).unwrap();
        }
    }
    impl Drop for Library {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn persistence_and_idempotence_without_download_history() {
        let lib = Library::new();
        assert!(load(&lib.0).unwrap().is_empty());
        assert!(!lib.0.join(SERVICE).exists());
        sync_move(&lib.0, "missing.epub", "other.epub").unwrap();
        assert!(!lib.0.join(SERVICE).exists());
        let entry = lib.book("Books/one.epub", b"edition one");
        record(&lib.0, entry.clone()).unwrap();
        assert_eq!(load(&lib.0).unwrap().get(&entry.path), Some(&entry));
        let before = fs::read(lib.index()).unwrap();
        let modified = fs::metadata(lib.index()).unwrap().modified().unwrap();
        record(&lib.0, entry.clone()).unwrap();
        sync_move(&lib.0, &entry.path, &entry.path).unwrap();
        sync_move(&lib.0, "missing.epub", "other.epub").unwrap();
        assert_eq!(fs::read(lib.index()).unwrap(), before);
        assert_eq!(
            fs::metadata(lib.index()).unwrap().modified().unwrap(),
            modified
        );
        // Loading provenance must not depend on the continued presence of books.
        fs::remove_file(lib.0.join(&entry.path)).unwrap();
        assert_eq!(load(&lib.0).unwrap()[&entry.path], entry);
    }

    #[test]
    fn untracked_moves_preserve_existing_rules_but_reject_stale_destination_record() {
        let lib = Library::new();
        sync_move(&lib.0, ".old.epub", ".new.epub").unwrap();
        let entry = lib.book("target.epub", b"book");
        record(&lib.0, entry).unwrap();
        assert!(sync_move(&lib.0, "untracked.epub", "target.epub").is_err());
    }

    #[test]
    fn move_checks_contents_and_preserves_metadata() {
        let lib = Library::new();
        let original = lib.book("old.epub", b"original");
        record(&lib.0, original.clone()).unwrap();
        let before = fs::read(lib.index()).unwrap();
        assert!(sync_move(&lib.0, "old.epub", "new.epub").is_err());
        lib.book("new.epub", b"modified");
        assert!(sync_move(&lib.0, "old.epub", "new.epub").is_err());
        assert_eq!(fs::read(lib.index()).unwrap(), before);
        fs::remove_file(lib.0.join("new.epub")).unwrap();
        fs::rename(lib.0.join("old.epub"), lib.0.join("new.epub")).unwrap();
        sync_move(&lib.0, "old.epub", "new.epub").unwrap();
        let mut expected = original;
        expected.path = "new.epub".into();
        assert_eq!(
            load(&lib.0).unwrap(),
            BTreeMap::from([("new.epub".into(), expected)])
        );
        assert_eq!(fs::read(lib.0.join("new.epub")).unwrap(), b"original");
    }

    #[test]
    fn collisions_and_hash_mismatches_never_overwrite() {
        let lib = Library::new();
        let first = lib.book("one.epub", b"same");
        let second = lib.book("two.epub", b"same");
        record(&lib.0, first.clone()).unwrap();
        record(&lib.0, second).unwrap();
        let before = fs::read(lib.index()).unwrap();
        let mut conflict = first.clone();
        conflict.source_name = "Other source".into();
        assert!(record(&lib.0, conflict).is_err());
        fs::remove_file(lib.0.join("two.epub")).unwrap();
        fs::rename(lib.0.join("one.epub"), lib.0.join("two.epub")).unwrap();
        assert!(sync_move(&lib.0, "one.epub", "two.epub").is_err());
        assert_eq!(fs::read(lib.index()).unwrap(), before);
        assert_eq!(fs::read(lib.0.join("two.epub")).unwrap(), b"same");
        fs::write(lib.0.join("one.epub"), b"diff").unwrap();
        assert!(record(&lib.0, first).is_err());
        assert_eq!(fs::read(lib.index()).unwrap(), before);
    }

    #[test]
    fn validates_paths_metadata_and_fingerprints_before_writing() {
        let lib = Library::new();
        let valid = lib.book("book.epub", b"book");
        for path in [
            "",
            ".",
            "..",
            "./book",
            "a/../book",
            "/book",
            "a//book",
            "a/",
            "a\\book",
            "C:/book",
            "a/\nbook",
            ".hidden/book",
            "a/.hidden/book",
            "99 - Catálogo/book",
            "99 — Catalogo/book",
            "99–CATÁLOGO/book",
            "node_modules/book",
        ] {
            let mut entry = valid.clone();
            entry.path = path.into();
            assert!(record(&lib.0, entry).is_err(), "{path:?}");
        }
        for name in ["".into(), " ".into(), "x".repeat(501), "bad\n".into()] {
            let mut entry = valid.clone();
            entry.source_name = name;
            assert!(record(&lib.0, entry).is_err());
        }
        let mut entry = valid.clone();
        entry.metadata.title = "x".repeat(1201);
        assert!(record(&lib.0, entry).is_err());
        let mut entry = valid;
        entry.fingerprint.sha256 = "A".repeat(64);
        assert!(record(&lib.0, entry).is_err());
        assert!(!lib.0.join(SERVICE).exists());
        assert_eq!(fs::read(lib.0.join("book.epub")).unwrap(), b"book");
    }

    #[test]
    fn corruption_fails_closed() {
        let lib = Library::new();
        let entry = lib.book("book.epub", b"book");
        let record_json = serde_json::to_string(&entry).unwrap();
        let cases = vec![
            "".into(),
            "{".into(),
            r#"{"version":2,"entries":{}}"#.into(),
            r#"{"version":1,"entries":{},"unknown":true}"#.into(),
            r#"{"version":1,"version":1,"entries":{}}"#.into(),
            r#"{"version":1,"entries":{}} trailing"#.into(),
            format!(r#"{{"version":1,"entries":{{"wrong":{record_json}}}}}"#),
            format!(
                r#"{{"version":1,"entries":{{"book.epub":{record_json},"book.epub":{record_json}}}}}"#
            ),
            format!(
                r#"{{"version":1,"entries":{{"book.epub":{}}}}}"#,
                record_json.replace("Test source", "")
            ),
            format!(
                r#"{{"version":1,"entries":{{"book.epub":{}}}}}"#,
                record_json.replace("Test book", &"x".repeat(1201))
            ),
        ];
        for raw in cases {
            lib.raw(raw.as_bytes());
            assert!(load(&lib.0).is_err(), "{raw}");
            assert!(record(&lib.0, entry.clone()).is_err());
            assert!(sync_move(&lib.0, "missing", "other").is_err());
            assert_eq!(fs::read(lib.index()).unwrap(), raw.as_bytes());
            assert_eq!(fs::read(lib.0.join("book.epub")).unwrap(), b"book");
        }
    }

    #[test]
    fn rejects_nonregular_service_data_and_noncanonical_roots() {
        let lib = Library::new();
        let entry = lib.book("book.epub", b"book");
        assert!(load(&lib.0.join(".")).is_err());
        assert!(load(&lib.0.join("absent")).is_err());
        fs::write(lib.0.join(SERVICE), b"not a directory").unwrap();
        assert!(load(&lib.0).is_err());
        assert!(record(&lib.0, entry.clone()).is_err());
        fs::remove_file(lib.0.join(SERVICE)).unwrap();
        fs::create_dir(lib.0.join(SERVICE)).unwrap();
        fs::create_dir(lib.index()).unwrap();
        assert!(load(&lib.0).is_err());
        assert!(record(&lib.0, entry).is_err());
        assert!(lib.index().is_dir());
        assert_eq!(fs::read(lib.0.join("book.epub")).unwrap(), b"book");
    }

    #[test]
    fn enforces_read_write_and_entry_bounds() {
        let lib = Library::new();
        lib.raw(b"{}");
        OpenOptions::new()
            .write(true)
            .open(lib.index())
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(load(&lib.0).is_err());
        let mut buffer = LimitedBuffer(Vec::new());
        buffer.write_all(&vec![b' '; MAX_BYTES as usize]).unwrap();
        assert!(buffer.write_all(b"x").is_err());
        assert_eq!(buffer.0.len() as u64, MAX_BYTES);
        let entry = lib.book("book.epub", b"book");
        let mut entries = BTreeMap::new();
        for n in 0..=MAX_ENTRIES {
            let path = format!("{n}.epub");
            entries.insert(
                path.clone(),
                EditionRecord {
                    path,
                    ..entry.clone()
                },
            );
        }
        let bytes = serde_json::to_vec(&Envelope {
            version: 1,
            entries: entries.clone(),
        })
        .unwrap();
        assert!(bytes.len() < MAX_BYTES as usize);
        lib.raw(&bytes);
        assert!(load(&lib.0).is_err());
        assert!(persist(&lib.0, entries).is_err());
        assert_eq!(fs::read(lib.index()).unwrap(), bytes);
        let mut large = entry;
        large.metadata.title = "x".repeat(1200);
        large.metadata.author = "x".repeat(1200);
        let entries = (0..8000)
            .map(|n| {
                let path = format!("{n}.epub");
                (
                    path.clone(),
                    EditionRecord {
                        path,
                        ..large.clone()
                    },
                )
            })
            .collect();
        assert!(persist(&lib.0, entries).is_err());
        assert_eq!(fs::read(lib.index()).unwrap(), bytes);
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_roots_books_parents_service_and_index() {
        use std::os::unix::fs::symlink;
        let lib = Library::new();
        let other = Library::new();
        let entry = lib.book("book.epub", b"book");
        symlink(&lib.0, other.0.join("root")).unwrap();
        assert!(load(&other.0.join("root")).is_err());
        symlink(lib.0.join("book.epub"), lib.0.join("link.epub")).unwrap();
        let linked = EditionRecord {
            path: "link.epub".into(),
            ..entry.clone()
        };
        assert!(record(&lib.0, linked).is_err());
        fs::create_dir(lib.0.join("Books")).unwrap();
        fs::write(lib.0.join("Books/book.epub"), b"book").unwrap();
        symlink(lib.0.join("Books"), lib.0.join("alias")).unwrap();
        assert!(
            record(
                &lib.0,
                EditionRecord {
                    path: "alias/book.epub".into(),
                    ..entry.clone()
                }
            )
            .is_err()
        );
        symlink(&other.0, lib.0.join(SERVICE)).unwrap();
        assert!(load(&lib.0).is_err());
        assert!(record(&lib.0, entry.clone()).is_err());
        assert!(fs::read_dir(&other.0).unwrap().count() == 1);
        fs::remove_file(lib.0.join(SERVICE)).unwrap();
        fs::create_dir(lib.0.join(SERVICE)).unwrap();
        fs::write(other.0.join("sentinel"), b"unchanged").unwrap();
        symlink(other.0.join("sentinel"), lib.index()).unwrap();
        assert!(load(&lib.0).is_err());
        assert!(record(&lib.0, entry.clone()).is_err());
        assert_eq!(fs::read(other.0.join("sentinel")).unwrap(), b"unchanged");
        fs::remove_file(lib.index()).unwrap();
        record(&lib.0, entry).unwrap();
        assert!(sync_move(&lib.0, "book.epub", "link.epub").is_err());
        assert!(sync_move(&lib.0, "book.epub", "alias/book.epub").is_err());
        fs::remove_file(lib.index()).unwrap();
        symlink(other.0.join("missing"), lib.index()).unwrap();
        assert!(load(&lib.0).is_err());
    }
}
