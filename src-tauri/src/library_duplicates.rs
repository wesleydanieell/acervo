//! Read-only duplicate discovery. Never changes files or catalog records.
use crate::{library_editions::canonical_root, scan_collection};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, HashMap},
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_READ: u64 = 8 * 1024 * 1024 * 1024;
const MAX_CANDIDATES: usize = 4000;
static SCAN: Mutex<()> = Mutex::new(());

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateFile {
    path: String,
    format: String,
    associations: usize,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DuplicateGroup {
    bytes: u64,
    sha256: String,
    files: Vec<DuplicateFile>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Report {
    groups: Vec<DuplicateGroup>,
    files: usize,
    candidates: usize,
    hashed: usize,
    skipped: usize,
    failed: usize,
    warnings: Vec<String>,
}
#[derive(PartialEq, Eq)]
struct Stamp {
    bytes: u64,
    modified: SystemTime,
    #[cfg(unix)]
    identity: (u64, u64, i64, i64),
}
fn stamp(path: &Path) -> Result<Stamp, String> {
    stamp_metadata(fs::symlink_metadata(path).map_err(|e| e.to_string())?)
}
fn stamp_metadata(meta: fs::Metadata) -> Result<Stamp, String> {
    if !meta.file_type().is_file() {
        return Err("Não é um arquivo regular.".into());
    }
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(Stamp {
        bytes: meta.len(),
        modified: meta.modified().map_err(|e| e.to_string())?,
        #[cfg(unix)]
        identity: (meta.dev(), meta.ino(), meta.ctime(), meta.ctime_nsec()),
    })
}
fn checked(root: &Path, relative: &str) -> Result<PathBuf, String> {
    if relative.is_empty()
        || relative.contains(['\\', '\0', ':'])
        || relative.split('/').count() > 81
        || !relative.split('/').all(|part| {
            !part.is_empty()
                && part != "."
                && part != ".."
                && matches!(
                    Path::new(part).components().next(),
                    Some(std::path::Component::Normal(_))
                )
        })
    {
        return Err("Caminho de livro inválido.".into());
    }
    let mut path = root.to_path_buf();
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        path.push(part);
        let kind = fs::symlink_metadata(&path)
            .map_err(|e| e.to_string())?
            .file_type();
        if if parts.peek().is_some() {
            !kind.is_dir()
        } else {
            !kind.is_file()
        } {
            return Err("Caminho inválido ou simbólico.".into());
        }
    }
    if fs::canonicalize(&path).map_err(|e| e.to_string())? != path {
        return Err("Caminho não canônico.".into());
    }
    Ok(path)
}
// Unix: anchor traversal to a directory descriptor; never follow a symlink
// in any component between the selected root and the book.
#[cfg(unix)]
fn open_book(root: &Path, relative: &str, _path: &Path) -> Result<fs::File, String> {
    use std::{
        ffi::CString,
        os::fd::{AsRawFd, FromRawFd},
        os::unix::fs::OpenOptionsExt,
    };
    let mut file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(root)
        .map_err(|e| e.to_string())?;
    let mut parts = relative.split('/').peekable();
    while let Some(part) = parts.next() {
        let part = CString::new(part).map_err(|e| e.to_string())?;
        let flags = libc::O_RDONLY
            | libc::O_CLOEXEC
            | libc::O_NOFOLLOW
            | libc::O_NONBLOCK
            | if parts.peek().is_some() {
                libc::O_DIRECTORY
            } else {
                0
            };
        let fd = unsafe { libc::openat(file.as_raw_fd(), part.as_ptr(), flags) };
        if fd < 0 {
            return Err(std::io::Error::last_os_error().to_string());
        }
        file = unsafe { fs::File::from_raw_fd(fd) };
    }
    Ok(file)
}
#[cfg(not(unix))]
fn open_book(_root: &Path, _relative: &str, path: &Path) -> Result<fs::File, String> {
    crate::library_editions::open_regular(path)
}
fn hash_book(root: &Path, relative: &str, bytes: u64) -> Result<(String, Stamp), String> {
    let path = checked(root, relative)?;
    let before = stamp(&path)?;
    if before.bytes != bytes {
        return Err("Tamanho mudou desde a varredura.".into());
    }
    let mut file = open_book(root, relative, &path)?;
    if stamp_metadata(file.metadata().map_err(|e| e.to_string())?)? != before {
        return Err("Arquivo mudou ao abrir.".into());
    }
    let mut hasher = Sha256::new();
    let mut reader = (&mut file).take(bytes);
    let mut buffer = [0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match reader.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            value => value.map_err(|e| e.to_string())?,
        };
        if n == 0 {
            break;
        }
        total += n as u64;
        hasher.update(&buffer[..n]);
    }
    if total != bytes
        || stamp_metadata(file.metadata().map_err(|e| e.to_string())?)? != before
        || stamp(&checked(root, relative)?)? != before
    {
        return Err("Arquivo mudou durante a leitura.".into());
    }
    Ok((format!("{:x}", hasher.finalize()), before))
}
fn warn(report: &mut Report, message: String) {
    if report.warnings.len() < 50 {
        report.warnings.push(message);
    }
}
fn discover(root: &str, folder: Option<&str>) -> Result<Report, String> {
    let _guard = SCAN
        .try_lock()
        .map_err(|_| "Uma busca de duplicatas já está em andamento.")?;
    let root_path = Path::new(root);
    canonical_root(root_path)?;
    let collection = scan_collection(root.into())?;
    if collection.root != root {
        return Err("Biblioteca alterada; atualize antes de buscar.".into());
    }
    let folder = folder.unwrap_or("");
    if !folder.is_empty() && !collection.folders.iter().any(|f| f == folder) {
        return Err("Pasta não encontrada.".into());
    }
    let prefix = format!("{folder}/");
    let mut associations: HashMap<&str, usize> = HashMap::new();
    for work in &collection.works {
        for path in &work.associated_files {
            *associations.entry(path).or_default() += 1;
        }
    }
    let mut report = Report {
        groups: vec![],
        files: 0,
        candidates: 0,
        hashed: 0,
        skipped: 0,
        failed: 0,
        warnings: collection.warnings.iter().take(50).cloned().collect(),
    };
    let mut by_size: BTreeMap<u64, Vec<_>> = BTreeMap::new();
    for file in &collection.files {
        if folder.is_empty() || file.folder == folder || file.folder.starts_with(&prefix) {
            report.files += 1;
            by_size.entry(file.bytes).or_default().push(file);
        }
    }
    let mut budget = 0u64;
    let mut attempts = 0usize;
    for (bytes, files) in by_size {
        if files.len() < 2 {
            continue;
        }
        report.candidates += files.len();
        let cost = bytes.saturating_mul(files.len() as u64);
        if bytes == 0
            || bytes > MAX_FILE
            || attempts + files.len() > MAX_CANDIDATES
            || budget.saturating_add(cost) > MAX_READ
        {
            report.skipped += files.len();
            continue;
        }
        attempts += files.len();
        budget += cost;
        let mut matches: BTreeMap<String, Vec<(DuplicateFile, Stamp)>> = BTreeMap::new();
        for file in files {
            let result = hash_book(root_path, &file.path, bytes);
            match result {
                Ok((sha, before)) => {
                    report.hashed += 1;
                    matches.entry(sha).or_default().push((
                        DuplicateFile {
                            path: file.path.clone(),
                            format: file.format.clone(),
                            associations: associations
                                .get(file.path.as_str())
                                .copied()
                                .unwrap_or(0),
                        },
                        before,
                    ));
                }
                Err(error) => {
                    report.failed += 1;
                    warn(&mut report, format!("{}: {error}", file.path));
                }
            }
        }
        for (sha, entries) in matches {
            if entries.len() < 2 {
                continue;
            }
            let mut stable = vec![];
            for (file, before) in entries {
                if checked(root_path, &file.path)
                    .and_then(|path| stamp(&path))
                    .ok()
                    .as_ref()
                    == Some(&before)
                {
                    stable.push(file);
                } else {
                    report.hashed -= 1;
                    report.failed += 1;
                    warn(
                        &mut report,
                        format!("{}: arquivo alterado após a leitura.", file.path),
                    );
                }
            }
            if stable.len() > 1 {
                stable.sort_by(|a, b| a.path.cmp(&b.path));
                report.groups.push(DuplicateGroup {
                    bytes,
                    sha256: sha,
                    files: stable,
                });
            }
        }
    }
    report
        .groups
        .sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.sha256.cmp(&b.sha256)));
    if report.skipped > 0 {
        warn(&mut report,"Busca parcial: arquivos vazios, acima de 512 MiB ou grupos que excedem o limite de 4.000 candidatos/8 GiB. Escolha uma pasta menor para investigar.".into());
    }
    Ok(report)
}
#[tauri::command]
pub async fn scan_duplicate_books(root: String, folder: Option<String>) -> Result<Report, String> {
    tauri::async_runtime::spawn_blocking(move || discover(&root, folder.as_deref()))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn compares_contents_not_titles_or_sizes_and_preserves_files() {
        let dir = std::env::temp_dir().join(format!(
            "acervo-duplicates-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for name in ["A", "B", "99 - Catálogo"] {
            fs::create_dir_all(dir.join(name)).unwrap();
        }
        for (name, content) in [
            ("A/First.pdf", b"same".as_slice()),
            ("B/.Copy.epub", b"same"),
            ("A/Different.pdf", b"diff"),
            ("A/Unique.pdf", b"unique"),
            ("B/Empty.pdf", b""),
            ("A/Empty.pdf", b""),
        ] {
            fs::write(dir.join(name), content).unwrap();
        }
        fs::write(dir.join("99 - Catálogo/Acervo_Listas.json"),r#"{"versao":1,"obras":[{"titulo":"First","local_indicado":"A","arquivos_encontrados":["A/First.pdf"]}]}"#).unwrap();
        let dir = fs::canonicalize(dir).unwrap();
        let root = dir.to_str().unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("A/First.pdf"), dir.join("B/Link.pdf")).unwrap();
        let result = discover(root, None).unwrap();
        assert_eq!(result.files, 6);
        assert_eq!(result.candidates, 5);
        assert_eq!(result.hashed, 3);
        assert_eq!(result.skipped, 2);
        assert_eq!(result.groups.len(), 1);
        let group = &result.groups[0];
        assert_eq!(group.files.len(), 2);
        assert_eq!(group.files[0].associations, 1);
        assert!(discover(root, Some("A")).unwrap().groups.is_empty());
        assert!(discover(root, Some("../outside")).is_err());
        assert!(hash_book(&dir, "A/First.pdf", 3).is_err());
        assert!(checked(&dir, "../escape.pdf").is_err());
        assert!(hash_book(&dir, "B/.Copy.epub", 4).is_ok());
        assert_eq!(fs::read(dir.join("A/First.pdf")).unwrap(), b"same");
        fs::remove_dir_all(dir).unwrap();
    }
}
