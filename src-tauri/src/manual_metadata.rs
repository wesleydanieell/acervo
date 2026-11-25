//! Durable user overrides. Never repaired, pruned, or reassociated automatically.
use crate::library_editions::{canonical_root, open_regular, persist_json, service};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::UNIX_EPOCH,
};

const INDEX: &str = "Acervo_Metadados_Manuais.json";
const MAX_BYTES: u64 = 16 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
pub(crate) static MUTATIONS: Mutex<()> = Mutex::new(());

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualMetadata {
    pub title: Option<String>,
    pub author: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub isbn: Option<String>,
    pub description: Option<String>,
}
impl ManualMetadata {
    fn validate(&self) -> Result<bool, String> {
        let mut any = false;
        for (value, limit) in [
            (&self.title, 1200),
            (&self.author, 1200),
            (&self.language, 100),
            (&self.publisher, 1200),
            (&self.isbn, 100),
            (&self.description, 16000),
        ] {
            if let Some(value) = value {
                if value.is_empty()
                    || value.trim() != value
                    || value.len() > limit
                    || value.chars().any(char::is_control)
                {
                    return Err(
                        "Metadado manual inválido: use texto aparado, não vazio e sem controles."
                            .into(),
                    );
                }
                any = true;
            }
        }
        Ok(any)
    }
}

// IPC must preserve integers larger than JavaScript's exact integer range.
mod decimal_u64 {
    use serde::{Deserialize, Deserializer, Serializer};
    pub fn serialize<S: Serializer>(value: &u64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&value.to_string())
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<u64, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Stable across a rename, conservative across replacement. Bounded hash fallback.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct FileIdentity {
    #[serde(with = "decimal_u64")]
    bytes: u64,
    modified_ns: String,
    created_ns: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    content_sha256: Option<String>,
    #[serde(with = "decimal_u64")]
    device: u64,
    #[serde(with = "decimal_u64")]
    inode: u64,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManualRecord {
    pub metadata: ManualMetadata,
    pub identity: FileIdentity,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct EditToken {
    pub identity: FileIdentity,
    #[serde(with = "decimal_u64")]
    pub revision: u64,
    pub root: String,
    pub path: String,
}
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditSnapshot {
    pub metadata: ManualMetadata,
    pub expected: EditToken,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Index {
    version: u32,
    revision: u64,
    #[serde(deserialize_with = "read_entries")]
    pub entries: BTreeMap<String, ManualRecord>,
}
fn read_entries<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<BTreeMap<String, ManualRecord>, D::Error> {
    struct Entries;
    impl<'de> serde::de::Visitor<'de> for Entries {
        type Value = BTreeMap<String, ManualRecord>;
        fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
            f.write_str("bounded unique manual paths")
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut entries = BTreeMap::new();
            while let Some(key) = map.next_key::<String>()? {
                if entries.len() >= MAX_ENTRIES || entries.contains_key(&key) {
                    return Err(serde::de::Error::custom(
                        "Duplicate or excessive manual records",
                    ));
                }
                entries.insert(key, map.next_value()?);
            }
            Ok(entries)
        }
    }
    d.deserialize_map(Entries)
}
fn valid_path(relative: &str) -> Result<(), String> {
    if relative.is_empty()
        || relative.len() > 855
        || relative.split('/').count() > 81
        || !crate::is_book(Path::new(relative))
    {
        return Err("Caminho de livro inválido.".into());
    }
    let parts: Vec<_> = relative.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        let normalized = part
            .to_lowercase()
            .replace(['–', '—'], "-")
            .replace('á', "a");
        if part.is_empty()
            || *part == "."
            || *part == ".."
            || part.len() > 255
            || part.trim() != *part
            || part.contains(['\\', ':'])
            || part.chars().any(char::is_control)
            || *part == "node_modules"
            || (part.starts_with('.') && i + 1 != parts.len())
            || normalized
                .split_once('-')
                .is_some_and(|(a, b)| a.trim() == "99" && b.trim() == "catalogo")
        {
            return Err("Caminho protegido ou inválido.".into());
        }
    }
    Ok(())
}
fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    canonical_root(root)?;
    valid_path(relative)?;
    let mut current = root.to_path_buf();
    let parts: Vec<_> = relative.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        current.push(part);
        let kind = fs::symlink_metadata(&current)
            .map_err(|e| e.to_string())?
            .file_type();
        if if i + 1 == parts.len() {
            !kind.is_file()
        } else {
            !kind.is_dir()
        } {
            return Err("Caminho simbólico ou inválido.".into());
        }
    }
    if fs::canonicalize(&current).map_err(|e| e.to_string())? != current {
        return Err("Caminho não canônico.".into());
    }
    Ok(current)
}
#[cfg(windows)]
fn windows_file_id(file: &fs::File) -> Result<(u64, u64), String> {
    use std::os::windows::io::AsRawHandle;
    // BY_HANDLE_FILE_INFORMATION consists entirely of 32-bit words:
    // attributes, 3 FILETIMEs, volume, size, links, file index (high/low).
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetFileInformationByHandle(file: *mut std::ffi::c_void, info: *mut u32) -> i32;
    }
    let mut info = [0u32; 13];
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let inode = (u64::from(info[11]) << 32) | u64::from(info[12]);
    if inode == 0 {
        return Err("Sistema de arquivos sem identidade estável.".into());
    }
    Ok((u64::from(info[7]), inode))
}
fn ns(time: std::io::Result<std::time::SystemTime>) -> Result<String, String> {
    Ok(time
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos()
        .to_string())
}
// A small content-based fallback for file systems without creation time.
// Only manually edited files use this during hydration, never all books.
const MAX_FALLBACK_BYTES: u64 = 16 * 1024 * 1024;
fn fallback_hash(file: &fs::File, meta: &fs::Metadata) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    if meta.len() > MAX_FALLBACK_BYTES {
        return Err(
            "Sem data de criação: edição manual disponível para arquivos de até 16 MiB.".into(),
        );
    }
    let mut hasher = Sha256::new();
    let mut reader = file.take(meta.len());
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            value => value.map_err(|e| e.to_string())?,
        };
        if read == 0 {
            break;
        }
        total += read as u64;
        hasher.update(&buffer[..read]);
    }
    let after = file.metadata().map_err(|e| e.to_string())?;
    if total != meta.len()
        || after.len() != meta.len()
        || after.modified().ok() != meta.modified().ok()
    {
        return Err("Arquivo alterado durante a verificação.".into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec())
            != (after.dev(), after.ino(), after.ctime(), after.ctime_nsec())
        {
            return Err("Arquivo alterado durante a verificação.".into());
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}
pub(crate) fn identity(root: &Path, relative: &str) -> Result<FileIdentity, String> {
    let path = checked_path(root, relative)?;
    let file = open_regular(&path)?;
    let meta = file.metadata().map_err(|e| e.to_string())?;
    let created_ns = ns(meta.created()).ok();
    #[cfg(unix)]
    let (device, inode) = {
        use std::os::unix::fs::MetadataExt;
        (meta.dev(), meta.ino())
    };
    #[cfg(windows)]
    let (device, inode) = windows_file_id(&file)?;
    #[cfg(not(any(unix, windows)))]
    let (device, inode) = (0, 0);
    if inode == 0 {
        return Err("Sistema de arquivos sem identidade estável.".into());
    }
    let content_sha256 = if created_ns.is_none() {
        Some(fallback_hash(&file, &meta)?)
    } else {
        None
    };
    Ok(FileIdentity {
        bytes: meta.len(),
        modified_ns: ns(meta.modified())?,
        created_ns,
        content_sha256,
        device,
        inode,
    })
}
fn valid_ns(value: &str) -> bool {
    !value.is_empty() && value.len() <= 39 && value.bytes().all(|b| b.is_ascii_digit())
}
pub(crate) fn load(root: &Path) -> Result<Index, String> {
    canonical_root(root)?;
    let empty = || Index {
        version: 1,
        revision: 0,
        entries: BTreeMap::new(),
    };
    let Some(directory) = service(root)? else {
        return Ok(empty());
    };
    let path = directory.join(INDEX);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(empty()),
        Err(e) => return Err(e.to_string()),
        Ok(_) => {}
    }
    let mut bytes = Vec::new();
    open_regular(&path)?
        .take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Índice manual excede 16 MiB.".into());
    }
    let index: Index = serde_json::from_slice(&bytes)
        .map_err(|e| format!("Índice manual corrompido; recuperação explícita necessária: {e}"))?;
    if index.version != 1 {
        return Err("Versão de índice manual não suportada.".into());
    }
    for (path, record) in &index.entries {
        valid_path(path)?;
        if !record.metadata.validate()?
            || !valid_ns(&record.identity.modified_ns)
            || record
                .identity
                .created_ns
                .as_ref()
                .is_some_and(|s| !valid_ns(s))
            || record.identity.inode == 0
            || match (&record.identity.created_ns, &record.identity.content_sha256) {
                (Some(_), None) => false,
                (None, Some(hash)) => {
                    record.identity.bytes > MAX_FALLBACK_BYTES
                        || hash.len() != 64
                        || !hash
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                }
                _ => true,
            }
        {
            return Err("Registro manual inválido.".into());
        }
    }
    Ok(index)
}
fn persist(root: &Path, mut index: Index) -> Result<(), String> {
    if index.entries.len() > MAX_ENTRIES {
        return Err("Excesso de registros manuais.".into());
    }
    index.revision = index.revision.checked_add(1).ok_or("Revisão esgotada.")?;
    persist_json(root, INDEX, &index)
}
fn snapshot(root: &Path, path: &str, index: &Index) -> Result<EditSnapshot, String> {
    let identity = identity(root, path)?;
    let record = index.entries.get(path);
    if record.is_some_and(|r| r.identity != identity) {
        return Err(
            "Arquivo substituído; registro manual preservado para recuperação explícita.".into(),
        );
    }
    Ok(EditSnapshot {
        metadata: record.map(|r| r.metadata.clone()).unwrap_or_default(),
        expected: EditToken {
            identity,
            revision: index.revision,
            root: root.to_string_lossy().into_owned(),
            path: path.into(),
        },
    })
}
fn get(root: &Path, path: &str) -> Result<EditSnapshot, String> {
    snapshot(root, path, &load(root)?)
}
fn save(
    root: &Path,
    path: &str,
    expected: EditToken,
    metadata: ManualMetadata,
) -> Result<EditSnapshot, String> {
    let _guard = MUTATIONS
        .lock()
        .map_err(|_| "Índice manual indisponível.")?;
    let any = metadata.validate()?;
    let mut index = load(root)?;
    let current = snapshot(root, path, &index)?;
    if current.expected != expected {
        return Err("Arquivo ou metadados alterados; recarregue antes de salvar.".into());
    }
    if any {
        index.entries.insert(
            path.into(),
            ManualRecord {
                metadata,
                identity: current.expected.identity,
            },
        );
    } else {
        index.entries.remove(path);
    }
    // Recheck immediately before committing; all application moves share this lock.
    if identity(root, path)? != expected.identity {
        return Err("Arquivo alterado durante a edição.".into());
    }
    persist(root, index)?;
    get(root, path)
}
/// Caller holds MUTATIONS across the entire move and rollback.
pub(crate) fn sync_move(root: &Path, old: &str, new: &str) -> Result<Option<Index>, String> {
    let mut index = load(root)?;
    if index.entries.contains_key(new) {
        return Err("Registro manual conflitante no destino.".into());
    }
    let Some(record) = index.entries.get(old).cloned() else {
        return Ok(None);
    };
    if identity(root, new)? != record.identity {
        return Err("Identidade manual divergente; recuperação explícita necessária.".into());
    }
    let undo = index.clone();
    index.entries.remove(old);
    index.entries.insert(new.into(), record);
    persist(root, index)?;
    Ok(Some(undo))
}
pub(crate) fn rollback_move(root: &Path, undo: Option<Index>) -> Result<(), String> {
    if let Some(mut undo) = undo {
        undo.revision = load(root)?.revision;
        persist(root, undo)?;
    }
    Ok(())
}
#[tauri::command]
pub async fn get_manual_book_metadata(root: String, path: String) -> Result<EditSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let _guard = MUTATIONS
            .lock()
            .map_err(|_| "Índice manual indisponível.")?;
        get(Path::new(&root), &path)
    })
    .await
    .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn save_manual_book_metadata(
    root: String,
    path: String,
    expected: EditToken,
    metadata: ManualMetadata,
) -> Result<EditSnapshot, String> {
    tauri::async_runtime::spawn_blocking(move || save(Path::new(&root), &path, expected, metadata))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    struct Library(PathBuf);
    impl Library {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "acervo-manual-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
        fn book(&self, name: &str) {
            fs::write(self.0.join(name), b"book").unwrap();
        }
        fn index(&self) -> PathBuf {
            self.0.join("99 - Catálogo").join(INDEX)
        }
    }
    impl Drop for Library {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn metadata() -> ManualMetadata {
        ManualMetadata {
            title: Some("Manual title".into()),
            author: Some("Manual author".into()),
            ..Default::default()
        }
    }
    #[test]
    fn content_fallback_is_bounded_and_detects_different_contents() {
        let lib = Library::new();
        lib.book("book.pdf");
        let path = lib.0.join("book.pdf");
        let file = open_regular(&path).unwrap();
        let meta = file.metadata().unwrap();
        let first = fallback_hash(&file, &meta).unwrap();
        drop(file);
        fs::write(&path, vec![b'x'; meta.len() as usize]).unwrap();
        let file = open_regular(&path).unwrap();
        assert_ne!(
            first,
            fallback_hash(&file, &file.metadata().unwrap()).unwrap()
        );
        drop(file);
        fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_FALLBACK_BYTES + 1)
            .unwrap();
        let file = open_regular(&path).unwrap();
        assert!(fallback_hash(&file, &file.metadata().unwrap()).is_err());
    }

    #[test]
    fn roundtrip_clear_and_stale_edits_all_formats_including_hidden() {
        let lib = Library::new();
        for name in [
            ".hidden.pdf",
            "book.epub",
            "book.mobi",
            "book.azw3",
            "book.djvu",
        ] {
            lib.book(name);
            let first = get(&lib.0, name).unwrap();
            let saved = save(&lib.0, name, first.expected.clone(), metadata()).unwrap();
            assert_eq!(saved.metadata, metadata());
            assert_eq!(get(&lib.0, name).unwrap().expected, saved.expected);
            assert!(save(&lib.0, name, first.expected, metadata()).is_err());
            let cleared = save(
                &lib.0,
                name,
                saved.expected.clone(),
                ManualMetadata::default(),
            )
            .unwrap();
            assert_eq!(cleared.metadata, ManualMetadata::default());
            assert!(save(&lib.0, name, saved.expected, metadata()).is_err());
            assert!(!load(&lib.0).unwrap().entries.contains_key(name));
        }
        assert!(lib.index().is_file());
    }
    #[test]
    fn replacement_preserves_record_and_never_attaches_or_edits() {
        let lib = Library::new();
        lib.book("book.pdf");
        let first = get(&lib.0, "book.pdf").unwrap();
        let saved = save(&lib.0, "book.pdf", first.expected, metadata()).unwrap();
        let before = fs::read(lib.index()).unwrap();
        lib.book("replacement.pdf");
        fs::rename(lib.0.join("replacement.pdf"), lib.0.join("book.pdf")).unwrap();
        assert!(get(&lib.0, "book.pdf").is_err());
        assert!(
            save(
                &lib.0,
                "book.pdf",
                saved.expected,
                ManualMetadata::default()
            )
            .is_err()
        );
        let collection = crate::scan_collection(lib.0.to_string_lossy().into()).unwrap();
        assert!(collection.files[0].manual.is_none());
        assert_eq!(fs::read(lib.index()).unwrap(), before);
    }
    #[test]
    fn app_move_preserves_manual_and_edition_failure_rolls_it_back() {
        let lib = Library::new();
        lib.book("book.pdf");
        fs::create_dir(lib.0.join("dest")).unwrap();
        let first = get(&lib.0, "book.pdf").unwrap();
        save(&lib.0, "book.pdf", first.expected, metadata()).unwrap();
        // A valid but conflicting edition destination fails at the final commit.
        let edition = crate::library_editions::EditionRecord {
            path: "dest/book.pdf".into(),
            source_name: "source".into(),
            metadata: crate::downloads::DownloadEdition {
                title: "Book".into(),
                ..Default::default()
            },
            fingerprint: crate::import_fingerprint::fingerprint(&lib.0.join("book.pdf")).unwrap(),
        };
        fs::write(
            lib.0.join("99 - Catálogo/Acervo_Edicoes.json"),
            serde_json::to_vec(
                &serde_json::json!({"version":1,"entries":{"dest/book.pdf":edition}}),
            )
            .unwrap(),
        )
        .unwrap();
        let root = lib.0.to_string_lossy().to_string();
        assert!(crate::move_book(root.clone(), "book.pdf".into(), "dest".into()).is_err());
        assert!(lib.0.join("book.pdf").is_file());
        assert_eq!(get(&lib.0, "book.pdf").unwrap().metadata, metadata());
        fs::remove_file(lib.0.join("99 - Catálogo/Acervo_Edicoes.json")).unwrap();
        crate::move_book(root.clone(), "book.pdf".into(), "dest".into()).unwrap();
        assert_eq!(get(&lib.0, "dest/book.pdf").unwrap().metadata, metadata());
        let collection = crate::scan_collection(root).unwrap();
        assert_eq!(
            collection.files[0].manual.as_ref().unwrap().metadata,
            metadata()
        );
    }
    #[test]
    fn bad_paths_and_values_do_not_write() {
        let lib = Library::new();
        lib.book("book.pdf");
        for path in [
            "",
            "../book.pdf",
            "a/../book.pdf",
            "/book.pdf",
            "./book.pdf",
            "a//book.pdf",
            "99 - Catálogo/book.pdf",
            "99—Catalogo/book.pdf",
            "node_modules/book.pdf",
            ".dir/book.pdf",
            "a\\book.pdf",
            "C:/book.pdf",
            "book.txt",
        ] {
            assert!(get(&lib.0, path).is_err(), "{path}");
        }
        let first = get(&lib.0, "book.pdf").unwrap();
        for title in [
            "".into(),
            " ".into(),
            " padded".into(),
            "line\nbreak".into(),
            "x".repeat(1201),
        ] {
            assert!(
                save(
                    &lib.0,
                    "book.pdf",
                    first.expected.clone(),
                    ManualMetadata {
                        title: Some(title),
                        ..Default::default()
                    }
                )
                .is_err()
            );
        }
        assert!(!lib.index().exists());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(lib.0.join("book.pdf"), lib.0.join("link.pdf")).unwrap();
            assert!(get(&lib.0, "link.pdf").is_err());
            std::os::unix::fs::symlink(&lib.0, lib.0.join("alias")).unwrap();
            assert!(get(&lib.0, "alias/book.pdf").is_err());
        }
    }
    #[test]
    fn corrupt_index_blocks_edits_and_moves_without_changing_data() {
        let lib = Library::new();
        lib.book("book.pdf");
        fs::create_dir(lib.0.join("dest")).unwrap();
        let first = get(&lib.0, "book.pdf").unwrap();
        save(&lib.0, "book.pdf", first.expected.clone(), metadata()).unwrap();
        let valid = fs::read_to_string(lib.index()).unwrap();
        let duplicate = valid.replace("\"entries\":{", "\"entries\":{\"book.pdf\":{},");
        for raw in [
            "{".to_string(),
            "{\"version\":2,\"revision\":0,\"entries\":{}}".into(),
            duplicate,
        ] {
            fs::write(lib.index(), &raw).unwrap();
            assert!(get(&lib.0, "book.pdf").is_err());
            assert!(save(&lib.0, "book.pdf", first.expected.clone(), metadata()).is_err());
            assert!(
                crate::move_book(
                    lib.0.to_string_lossy().into(),
                    "book.pdf".into(),
                    "dest".into()
                )
                .is_err()
            );
            assert_eq!(fs::read_to_string(lib.index()).unwrap(), raw);
            assert!(lib.0.join("book.pdf").is_file());
        }
    }
}
