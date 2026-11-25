//! A disposable, bounded metadata index, independent of catalog and edition records.
use crate::{
    embedded_metadata::{self, EmbeddedMetadata},
    library_editions::{canonical_root, open_regular, service, valid_path},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::UNIX_EPOCH,
};

const INDEX: &str = "Acervo_Metadados.json";
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const MAX_ENTRIES: usize = 20_000;
const MAX_BATCH: usize = 10;
static MUTATIONS: Mutex<()> = Mutex::new(());
static TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Signature {
    bytes: u64,
    modified_ns: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct CachedMetadata {
    pub metadata: Option<EmbeddedMetadata>,
    pub error: Option<String>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Record {
    pub signature: Signature,
    pub result: CachedMetadata,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Envelope {
    version: u32,
    entries: BTreeMap<String, Record>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchReport {
    extracted: usize,
    empty: usize,
    failed: usize,
    skipped: usize,
    errors: Vec<String>,
}
fn checked_path(root: &Path, relative: &str) -> Result<PathBuf, String> {
    valid_path(relative)?;
    let mut current = root.to_path_buf();
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        current.push(part);
        let kind = fs::symlink_metadata(&current)
            .map_err(|e| e.to_string())?
            .file_type();
        if if parts.peek().is_some() {
            !kind.is_dir()
        } else {
            !kind.is_file()
        } {
            return Err("Arquivo ou pasta simbólica/inválida.".into());
        }
    }
    if fs::canonicalize(&current).map_err(|e| e.to_string())? != current {
        return Err("Caminho não canônico.".into());
    }
    if !matches!(
        current
            .extension()
            .and_then(|x| x.to_str())
            .map(|x| x.to_ascii_uppercase())
            .as_deref(),
        Some("EPUB" | "PDF")
    ) {
        return Err("Formato sem extrator de metadados.".into());
    }
    Ok(current)
}
pub(crate) fn current_signature(root: &Path, relative: &str) -> Result<Signature, String> {
    let path = checked_path(root, relative)?;
    let meta = fs::symlink_metadata(path).map_err(|e| e.to_string())?;
    if !meta.file_type().is_file() {
        return Err("Arquivo inválido.".into());
    }
    let ns = meta
        .modified()
        .map_err(|e| e.to_string())?
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    Ok(Signature {
        bytes: meta.len(),
        modified_ns: ns.to_string(),
    })
}
fn has_metadata(m: &EmbeddedMetadata) -> bool {
    [
        &m.title,
        &m.author,
        &m.language,
        &m.publisher,
        &m.isbn,
        &m.description,
    ]
    .iter()
    .any(|value| !value.is_empty())
}
fn valid_record(record: &Record) -> bool {
    record.signature.modified_ns.parse::<u128>().is_ok()
        && (record.result.metadata.is_none() || record.result.error.is_none())
        && record
            .result
            .error
            .as_ref()
            .is_none_or(|e| e.len() <= 1000 && !e.chars().any(char::is_control))
        && record.result.metadata.as_ref().is_none_or(|m| {
            [
                (&m.title, 500),
                (&m.author, 500),
                (&m.language, 25),
                (&m.publisher, 500),
                (&m.isbn, 64),
                (&m.description, 2000),
            ]
            .iter()
            .all(|(v, limit)| v.len() <= *limit && !v.chars().any(char::is_control))
        })
}
pub(crate) fn load(root: &Path) -> Result<BTreeMap<String, Record>, String> {
    canonical_root(root)?;
    let Some(dir) = service(root)? else {
        return Ok(BTreeMap::new());
    };
    let path = dir.join(INDEX);
    match fs::symlink_metadata(&path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
        Err(e) => return Err(e.to_string()),
        _ => {}
    }
    let file = open_regular(&path)?;
    let mut data = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut data)
        .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_BYTES {
        return Err("Cache excede 32 MiB.".into());
    }
    let value: Envelope = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    if value.version != 1 || value.entries.len() > MAX_ENTRIES {
        return Err("Versão ou tamanho de cache inválido.".into());
    }
    for (path, record) in &value.entries {
        valid_path(path)?;
        if !valid_record(record) {
            return Err("Registro de metadados inválido.".into());
        }
    }
    Ok(value.entries)
}
fn save(root: &Path, entries: BTreeMap<String, Record>) -> Result<(), String> {
    if entries.len() > MAX_ENTRIES {
        return Err("Limite de 20 mil registros de metadados.".into());
    }
    let data = serde_json::to_vec(&Envelope {
        version: 1,
        entries,
    })
    .map_err(|e| e.to_string())?;
    if data.len() as u64 > MAX_BYTES {
        return Err("Cache excede 32 MiB.".into());
    }
    canonical_root(root)?;
    if service(root)?.is_none() {
        match fs::create_dir(root.join("99 - Catálogo")) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    let directory = service(root)?.ok_or("Pasta de serviço ausente.")?;
    let path = directory.join(INDEX);
    match fs::symlink_metadata(&path) {
        Ok(m) if !m.file_type().is_file() => return Err("Cache não é um arquivo regular.".into()),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
        _ => {}
    }
    let temp = directory.join(format!(
        ".acervo-metadata-{}-{}-{}.tmp",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos(),
        TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let result = (|| {
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        Ok::<(), std::io::Error>(())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result.map_err(|e| e.to_string())
}
/// Best effort: cache is derived data, never roll back a successful book move
/// because a cache write failed. A subsequent extraction can recreate it.
pub(crate) fn after_move(root: &Path, old: &str, new: &str, before: Option<Signature>) {
    let Ok(_guard) = MUTATIONS.try_lock() else {
        return;
    };
    let Ok(mut records) = load(root) else {
        return;
    };
    let old_record = records.remove(old);
    let previous_target = records.remove(new);
    if old_record.is_none() && previous_target.is_none() {
        return;
    }
    if let (Some(record), Some(before)) = (old_record, before) {
        if record.signature == before && current_signature(root, new).ok().as_ref() == Some(&before)
        {
            records.insert(new.to_owned(), record);
        }
    }
    let _ = save(root, records);
}

fn clean_error(error: String) -> String {
    let mut value = String::new();
    for ch in error.chars() {
        let ch = if ch.is_control() { ' ' } else { ch };
        if value.len() + ch.len_utf8() > 1000 {
            break;
        }
        value.push(ch);
    }
    value
}
fn extract_batch(root: &Path, paths: Vec<String>, retry: bool) -> Result<BatchReport, String> {
    if paths.is_empty() || paths.len() > MAX_BATCH {
        return Err("Selecione de 1 a 10 livros.".into());
    }
    canonical_root(root)?;
    let mut seen = HashSet::new();
    for path in &paths {
        valid_path(path)?;
        if !seen.insert(path) {
            return Err("Arquivo repetido.".into());
        }
    }
    let _guard = MUTATIONS
        .try_lock()
        .map_err(|_| "Outra extração está em execução.")?;
    let mut records = load(root)?;
    records.retain(|path,_|!matches!(fs::symlink_metadata(root.join(path)),Err(e) if e.kind()==std::io::ErrorKind::NotFound));

    let mut report = BatchReport {
        extracted: 0,
        empty: 0,
        failed: 0,
        skipped: 0,
        errors: vec![],
    };
    // Reserve capacity before reading books; do not discard older valid records.
    if records.len() + paths.iter().filter(|p| !records.contains_key(*p)).count() > MAX_ENTRIES {
        return Err("Cache atingiu o limite de 20 mil registros.".into());
    }
    for relative in paths {
        let result = (|| {
            let before = current_signature(root, &relative)?;
            if !retry
                && records
                    .get(&relative)
                    .is_some_and(|record| record.signature == before)
            {
                report.skipped += 1;
                return Ok(());
            }
            let path = checked_path(root, &relative)?;
            let format = path
                .extension()
                .and_then(|x| x.to_str())
                .unwrap_or_default()
                .to_ascii_uppercase();
            let parsed = embedded_metadata::extract(&path, &format);
            if current_signature(root, &relative)? != before {
                return Err("Arquivo mudou durante a extração.".into());
            }
            let entry = match parsed {
                Ok(m) if has_metadata(&m) => {
                    report.extracted += 1;
                    CachedMetadata {
                        metadata: Some(m),
                        error: None,
                    }
                }
                Ok(_) => {
                    report.empty += 1;
                    CachedMetadata {
                        metadata: None,
                        error: None,
                    }
                }
                Err(e) => {
                    let error = clean_error(e);
                    report.failed += 1;
                    report.errors.push(format!("{relative}: {error}"));
                    CachedMetadata {
                        metadata: None,
                        error: Some(error),
                    }
                }
            };
            let record = Record {
                signature: before,
                result: entry,
            };
            if !valid_record(&record) {
                return Err("Metadados excedem os limites.".into());
            }
            records.insert(relative.clone(), record);
            Ok::<(), String>(())
        })();
        if let Err(error) = result {
            report.failed += 1;
            report.errors.push(format!("{relative}: {error}"));
        }
    }
    save(root, records)?;
    Ok(report)
}
fn quarantine_invalid_cache(root: &Path) -> Result<String, String> {
    canonical_root(root)?;
    let _guard = MUTATIONS
        .try_lock()
        .map_err(|_| "Outra extração está em execução.")?;
    if load(root).is_ok() {
        return Err("O cache atual não está inválido.".into());
    }
    let dir = service(root)?.ok_or("Pasta de serviço ausente.")?;
    let path = dir.join(INDEX);
    // Never rename a symlink, directory or other non-regular entry.
    if !fs::symlink_metadata(&path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_file()
    {
        return Err("Cache inválido não é um arquivo regular.".into());
    }
    let name = format!(
        "Acervo_Metadados.invalid-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| e.to_string())?
            .as_nanos()
    );
    super::rename_without_replace(&path, &dir.join(&name)).map_err(|e| e.to_string())?;
    Ok(name)
}

#[tauri::command]
pub async fn reset_invalid_metadata_cache(root: String) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || quarantine_invalid_cache(Path::new(&root)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub async fn extract_book_metadata(
    root: String,
    paths: Vec<String>,
    retry: Option<bool>,
) -> Result<BatchReport, String> {
    tauri::async_runtime::spawn_blocking(move || {
        extract_batch(Path::new(&root), paths, retry.unwrap_or(false))
    })
    .await
    .map_err(|e| e.to_string())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cache_roundtrip_and_changes_invalidate_records() {
        let root = std::env::temp_dir().join(format!(
            "acervo-meta-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let book = root.join("Book.epub");
        fs::write(&book, b"epub").unwrap();
        let signature = current_signature(&root, "Book.epub").unwrap();
        let mut records = BTreeMap::new();
        records.insert(
            "Book.epub".into(),
            Record {
                signature: signature.clone(),
                result: CachedMetadata {
                    metadata: Some(EmbeddedMetadata {
                        title: "Title".into(),
                        ..Default::default()
                    }),
                    error: None,
                },
            },
        );
        save(&root, records).unwrap();
        assert_eq!(load(&root).unwrap()["Book.epub"].signature, signature);
        let report = extract_batch(&root, vec!["Book.epub".into()], false).unwrap();
        assert_eq!(report.skipped, 1);
        fs::rename(&book, root.join("Moved.epub")).unwrap();
        after_move(&root, "Book.epub", "Moved.epub", Some(signature.clone()));
        assert_eq!(load(&root).unwrap()["Moved.epub"].signature, signature);
        fs::rename(root.join("Moved.epub"), &book).unwrap();
        after_move(&root, "Moved.epub", "Book.epub", Some(signature.clone()));
        fs::write(&book, b"changed book").unwrap();
        let current = current_signature(&root, "Book.epub").unwrap();
        assert_ne!(current, signature);
        fs::rename(&book, root.join("Moved.epub")).unwrap();
        after_move(&root, "Book.epub", "Moved.epub", Some(current));
        assert!(load(&root).unwrap().is_empty());

        assert!(current_signature(&root, "../Book.epub").is_err());
        assert!(extract_batch(&root, vec!["Book.epub".into(), "Book.epub".into()], false).is_err());
        assert!(quarantine_invalid_cache(&root).is_err());
        let path = root.join("99 - Catálogo").join(INDEX);
        fs::write(&path, b"invalid cache").unwrap();
        assert!(load(&root).is_err());
        let backup = quarantine_invalid_cache(&root).unwrap();
        assert_eq!(
            fs::read(root.join("99 - Catálogo").join(backup)).unwrap(),
            b"invalid cache"
        );
        assert!(load(&root).unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
