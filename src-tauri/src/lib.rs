mod catalog_lists;
mod covers;
mod download_naming;
mod download_resolver;
mod download_validation;
mod downloads;
mod embedded_metadata;
mod extension_api;
mod extension_host;
mod extension_network;
mod extension_repositories;
mod html_parser;
mod html_source;
mod import_fingerprint;
mod library_duplicates;
mod library_editions;
mod library_index;
mod library_index_disk;
mod library_monitor;
mod manual_metadata;
mod metadata_cache;
mod noctalia;
mod opds_parser;
mod opds_source;
mod organization;
mod library_restructure;
mod repository_index;
mod smart_organization;
mod source_browser;
mod source_covers;
mod source_search;
mod sources;

use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BookFile {
    id: String,
    title: String,
    path: String,
    folder: String,
    format: String,
    bytes: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    edition: Option<library_editions::EditionRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    manual: Option<manual_metadata::ManualRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    embedded: Option<metadata_cache::CachedMetadata>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CatalogWork {
    id: i64,
    title: String,
    author: String,
    publishers: String,
    destination: String,
    present: bool,
    priority: bool,
    associated_files: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Collection {
    root: String,
    files: Vec<BookFile>,
    folders: Vec<String>,
    works: Vec<CatalogWork>,
    warnings: Vec<String>,
}

fn is_book(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| {
            matches!(
                extension.to_ascii_lowercase().as_str(),
                "pdf" | "epub" | "mobi" | "azw3" | "djvu"
            )
        })
        .unwrap_or(false)
}

fn display_title(filename: &str) -> String {
    let bare = Path::new(filename)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(filename);
    if let Some(rest) = bare.strip_prefix("[L") {
        if let Some((digits, title)) = rest.split_once(']') {
            if digits.len() == 6 && digits.bytes().all(|c| c.is_ascii_digit()) {
                return title.trim().to_string();
            }
        }
    }
    bare.trim().to_string()
}

fn file_id(filename: &str, relative: &str) -> String {
    if let Some(rest) = filename.strip_prefix("[L") {
        if let Some((digits, _)) = rest.split_once(']') {
            if digits.len() == 6 && digits.bytes().all(|c| c.is_ascii_digit()) {
                return format!("L{digits}");
            }
        }
    }
    format!("path:{relative}")
}

fn scan_dir(
    root: &Path,
    current: &Path,
    files: &mut Vec<BookFile>,
    folders: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
    depth: usize,
) {
    scan_selected(root, current, files, folders, warnings, depth, None);
}

fn scan_selected(
    root: &Path,
    current: &Path,
    files: &mut Vec<BookFile>,
    folders: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
    depth: usize,
    selected: Option<&HashSet<String>>,
) {
    scan_selected_disk(
        root, current, files, folders, warnings, depth, selected, &mut None,
    );
}

#[allow(clippy::too_many_arguments)]
fn scan_selected_disk(
    root: &Path,
    current: &Path,
    files: &mut Vec<BookFile>,
    folders: &mut BTreeSet<String>,
    warnings: &mut Vec<String>,
    depth: usize,
    selected: Option<&HashSet<String>>,
    disk: &mut Option<library_index_disk::Index>,
) {
    if depth > 80 || files.len() > 150_000 {
        warnings.push(
            "Limite de profundidade ou arquivos atingido; algumas pastas não foram indexadas."
                .into(),
        );
        return;
    }
    let entries = match library_index_disk::listing(root, current, disk.as_mut()) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!("Pasta inacessível {}: {error}", current.display()));
            return;
        }
    };
    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                warnings.push(format!("Falha ao ler pasta: {error}"));
                continue;
            }
        };
        let path = entry.path();
        let name = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();
        if selected.is_some_and(|names| !names.contains(&name)) {
            continue;
        }
        let kind = match entry.file_type() {
            Ok(kind) => kind,
            Err(error) => {
                warnings.push(format!("Tipo inacessível {}: {error}", path.display()));
                continue;
            }
        };
        if kind.is_symlink() {
            continue;
        }
        if kind.is_dir() {
            if name.starts_with('.') || name == "99 - Catálogo" || name == "node_modules" {
                continue;
            }
            if let Ok(relative) = path.strip_prefix(root) {
                folders.insert(relative.to_string_lossy().replace('\\', "/"));
            }
            scan_selected_disk(root, &path, files, folders, warnings, depth + 1, None, disk);
        } else if kind.is_file() && is_book(&path) {
            let Ok(relative) = path.strip_prefix(root) else {
                continue;
            };
            let relative = relative.to_string_lossy().replace('\\', "/");
            let folder = Path::new(&relative)
                .parent()
                .unwrap_or(Path::new(""))
                .to_string_lossy()
                .replace('\\', "/");
            let format = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("")
                .to_ascii_uppercase();
            files.push(BookFile {
                id: file_id(&name, &relative),
                title: display_title(&name),
                path: relative,
                folder,
                format,
                bytes: match entry.metadata() {
                    Ok(meta) if meta.file_type().is_file() => meta.len(),
                    Ok(_) => continue,
                    Err(error) => {
                        warnings.push(format!("Arquivo inacessível {}: {error}", path.display()));
                        continue;
                    }
                },
                manual: None,
                edition: None,
                embedded: None,
            });
        }
    }
}

fn read_catalog(root: &Path, files: &[BookFile], warnings: &mut Vec<String>) -> Vec<CatalogWork> {
    use rusqlite::{Connection, OpenFlags};
    let db = root.join("99 - Catálogo").join("Catalogo_Conhecimento.db");
    if !db.is_file() {
        return Vec::new();
    }
    let connection = match Connection::open_with_flags(&db, OpenFlags::SQLITE_OPEN_READ_ONLY) {
        Ok(connection) => connection,
        Err(error) => {
            warnings.push(format!("Não foi possível ler o catálogo: {error}"));
            return Vec::new();
        }
    };
    let mut statement = match connection.prepare(
        "SELECT id,titulo,autor,editoras_indicadas,local_indicado,arquivos_encontrados,prioridade_alta FROM livros ORDER BY titulo COLLATE NOCASE"
    ) {
        Ok(statement) => statement,
        Err(error) => {
            warnings.push(format!("Estrutura do catálogo incompatível: {error}"));
            return Vec::new();
        }
    };
    let known: HashSet<&str> = files.iter().map(|book| book.path.as_str()).collect();
    let rows = match statement.query_map([], |row| {
        let associated: String = row.get::<_, Option<String>>(5)?.unwrap_or_default();
        let paths: Vec<String> = if associated.trim().is_empty() || associated.trim() == "null" {
            vec![]
        } else {
            serde_json::from_str(&associated).map_err(|error| {
                rusqlite::Error::FromSqlConversionFailure(
                    5,
                    rusqlite::types::Type::Text,
                    Box::new(error),
                )
            })?
        };
        let present = paths.iter().any(|path| known.contains(path.as_str()));
        Ok(CatalogWork {
            id: row.get(0)?,
            title: row.get::<_, Option<String>>(1)?.unwrap_or_default(),
            author: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
            publishers: row.get::<_, Option<String>>(3)?.unwrap_or_default(),
            destination: row.get::<_, Option<String>>(4)?.unwrap_or_default(),
            present,
            priority: row.get::<_, Option<i64>>(6)?.unwrap_or(0) != 0,
            associated_files: paths,
        })
    }) {
        Ok(rows) => rows,
        Err(error) => {
            warnings.push(format!("Falha ao consultar catálogo: {error}"));
            return Vec::new();
        }
    };
    rows.filter_map(|row| match row {
        Ok(work) => Some(work),
        Err(error) => {
            if !warnings
                .iter()
                .any(|w| w.starts_with("Ficha inválida no catálogo"))
            {
                warnings.push(format!("Ficha inválida no catálogo: {error}"));
            }
            None
        }
    })
    .collect()
}

#[tauri::command]
async fn load_initial_collection(
    app: tauri::AppHandle,
    root: String,
) -> Result<Collection, String> {
    let cache = app.path().app_cache_dir().ok();
    tauri::async_runtime::spawn_blocking(move || library_index::initial(root, cache))
        .await
        .map_err(|error| format!("Falha ao carregar a biblioteca: {error}"))?
}

#[tauri::command]
async fn load_collection(root: String) -> Result<Collection, String> {
    tauri::async_runtime::spawn_blocking(move || library_index::refresh(root, None))
        .await
        .map_err(|error| format!("Falha ao carregar a biblioteca: {error}"))?
}

#[tauri::command]
async fn refresh_indexed_collection(
    root: String,
    changes: Option<Vec<String>>,
) -> Result<Collection, String> {
    tauri::async_runtime::spawn_blocking(move || library_index::refresh(root, changes))
        .await
        .map_err(|error| format!("Falha ao carregar a biblioteca: {error}"))?
}

fn scan_collection(root: String) -> Result<Collection, String> {
    let path = fs::canonicalize(&root).map_err(|e| format!("Pasta inválida: {e}"))?;
    if !path.is_dir() {
        return Err("Escolha uma pasta de biblioteca válida.".into());
    }
    let mut files = Vec::new();
    let mut folders = BTreeSet::new();
    let mut warnings = Vec::new();
    scan_dir(&path, &path, &mut files, &mut folders, &mut warnings, 0);
    hydrate_collection(&path, files, folders, warnings)
}

fn hydrate_collection(
    path: &Path,
    mut files: Vec<BookFile>,
    folders: BTreeSet<String>,
    mut warnings: Vec<String>,
) -> Result<Collection, String> {
    match library_editions::load(&path) {
        Ok(mut editions) => {
            for file in &mut files {
                if let Some(edition) = editions.remove(&file.path) {
                    // Fast scan: size is a cheap stale-data guard, not content verification.
                    if edition.fingerprint.bytes == file.bytes {
                        file.edition = Some(edition);
                    } else {
                        warnings.push(format!(
                            "Metadados de edição não exibidos: tamanho mudou em {}",
                            file.path
                        ));
                    }
                }
            }
        }
        Err(error) => warnings.push(format!("Registro de edições: {error}")),
    }
    match metadata_cache::load(&path) {
        Ok(mut records) => {
            for file in &mut files {
                if let Some(record) = records.remove(&file.path) {
                    if metadata_cache::current_signature(&path, &file.path)
                        .ok()
                        .as_ref()
                        == Some(&record.signature)
                    {
                        file.embedded = Some(record.result);
                    }
                }
            }
        }
        Err(error) => warnings.push(format!("Cache de metadados: {error}")),
    }
    for file in &mut files {
        file.manual = None;
    }
    match manual_metadata::load(path) {
        Ok(index) => {
            for file in &mut files {
                if let Some(record) = index.entries.get(&file.path) {
                    if manual_metadata::identity(path, &file.path).ok().as_ref()
                        == Some(&record.identity)
                    {
                        file.manual = Some(record.clone());
                    } else {
                        warnings.push(format!(
                            "Metadados manuais preservados, identidade divergente: {}",
                            file.path
                        ));
                    }
                }
            }
        }
        Err(error) => warnings.push(format!("Metadados manuais: {error}")),
    }
    files.sort_by_cached_key(|file| (file.title.to_lowercase(), file.path.clone()));
    let mut works = read_catalog(&path, &files, &mut warnings);
    works.extend(catalog_lists::load(&path, &files, &mut warnings));
    Ok(Collection {
        root: path.to_string_lossy().to_string(),
        files,
        folders: folders.into_iter().collect(),
        works,
        warnings,
    })
}

fn resolve_inside(root: &Path, relative: &str) -> Result<PathBuf, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                std::path::Component::ParentDir | std::path::Component::Prefix(_)
            )
        })
    {
        return Err("Caminho fora da biblioteca não é permitido.".into());
    }
    let canonical =
        fs::canonicalize(root.join(path)).map_err(|e| format!("Destino inválido: {e}"))?;
    if !canonical.starts_with(root) {
        return Err("O caminho aponta para fora da biblioteca.".into());
    }
    Ok(canonical)
}

#[tauri::command]
fn create_folder(root: String, parent: String, name: String) -> Result<String, String> {
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("Biblioteca inválida.".into());
    }
    let folder = name.trim();
    if folder.is_empty()
        || folder.len() > 100
        || folder == "."
        || folder == ".."
        || folder
            .chars()
            .any(|c| c == '/' || c == '\\' || c.is_control())
    {
        return Err("Nome de pasta inválido.".into());
    }
    let parent = resolve_inside(&root, &parent)?;
    if parent.starts_with(root.join("99 - Catálogo"))
        || (parent == root && folder == "99 - Catálogo")
    {
        return Err("A pasta de serviço do catálogo é protegida.".into());
    }
    let destination = parent.join(folder);
    if destination.exists() {
        return Err("Uma pasta com esse nome já existe.".into());
    }
    fs::create_dir(&destination).map_err(|e| format!("Não foi possível criar a pasta: {e}"))?;
    Ok(destination
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/"))
}

#[derive(Debug)]
struct LegacyMoveUndo {
    id: i64,
    previous_paths: Option<String>,
    previous_destination: Option<String>,
    updated_paths: String,
    updated_destination: Option<String>,
}

fn sync_catalog_move(
    root: &Path,
    old: &str,
    new: &str,
    preserve_destination: bool,
) -> Result<Vec<LegacyMoveUndo>, String> {
    use rusqlite::Connection;
    let db = root.join("99 - Catálogo").join("Catalogo_Conhecimento.db");
    if !db.exists() {
        return Ok(vec![]);
    }
    let mut connection = Connection::open(db).map_err(|e| e.to_string())?;
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    let mut undo = Vec::new();
    {
        let mut stmt = tx
            .prepare("SELECT id,arquivos_encontrados,local_indicado FROM livros")
            .map_err(|e| e.to_string())?;
        let associations = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                ))
            })
            .map_err(|e| e.to_string())?;
        for association in associations {
            let (id, raw, previous_destination) = association.map_err(|e| e.to_string())?;
            let Ok(mut paths) =
                serde_json::from_str::<Vec<String>>(raw.as_deref().unwrap_or_default())
            else {
                continue;
            };
            if !paths.iter().any(|path| path == old) {
                continue;
            }
            for path in &mut paths {
                if path == old {
                    *path = new.to_string();
                }
            }
            let first_parent = Path::new(&paths[0])
                .parent()
                .unwrap_or(Path::new("."))
                .to_string_lossy()
                .to_string();
            let updated_destination = if preserve_destination {
                previous_destination.clone()
            } else {
                Some(first_parent)
            };
            let updated_paths = serde_json::to_string(&paths).map_err(|e| e.to_string())?;
            tx.execute(
                "UPDATE livros SET arquivos_encontrados=?1, local_indicado=?2 WHERE id=?3",
                (&updated_paths, &updated_destination, id),
            )
            .map_err(|e| e.to_string())?;
            undo.push(LegacyMoveUndo {
                id,
                previous_paths: raw,
                previous_destination,
                updated_paths,
                updated_destination,
            });
        }
    }
    tx.commit().map_err(|e| e.to_string())?;
    Ok(undo)
}

fn rollback_catalog_move(root: &Path, undo: Vec<LegacyMoveUndo>) -> Result<(), String> {
    if undo.is_empty() {
        return Ok(());
    }
    let db = root.join("99 - Catálogo").join("Catalogo_Conhecimento.db");
    let mut connection =
        rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
            .map_err(|e| e.to_string())?;
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    for row in undo {
        let changed=tx.execute("UPDATE livros SET arquivos_encontrados=?1,local_indicado=?2 WHERE id=?3 AND arquivos_encontrados IS ?4 AND local_indicado IS ?5",
          (row.previous_paths,row.previous_destination,row.id,row.updated_paths,row.updated_destination)).map_err(|e|e.to_string())?;
        if changed != 1 {
            return Err("O catálogo mudou durante a movimentação; reversão não aplicada.".into());
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

// The destination existence check is only a preview. The rename itself must
// refuse replacement, including rollback after a catalog failure.
fn rename_without_replace(source: &Path, target: &Path) -> std::io::Result<()> {
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        use std::{ffi::CString, os::unix::ffi::OsStrExt};
        let source = CString::new(source.as_os_str().as_bytes()).map_err(std::io::Error::other)?;
        let target = CString::new(target.as_os_str().as_bytes()).map_err(std::io::Error::other)?;
        #[cfg(target_os = "linux")]
        let result = unsafe {
            libc::renameat2(
                libc::AT_FDCWD,
                source.as_ptr(),
                libc::AT_FDCWD,
                target.as_ptr(),
                libc::RENAME_NOREPLACE,
            )
        };
        #[cfg(target_os = "macos")]
        let result =
            unsafe { libc::renamex_np(source.as_ptr(), target.as_ptr(), libc::RENAME_EXCL) };
        if result == 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::ffi::OsStrExt;
        #[link(name = "kernel32")]
        unsafe extern "system" {
            fn MoveFileW(existing: *const u16, new: *const u16) -> i32;
        }
        let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
        let target: Vec<u16> = target.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { MoveFileW(source.as_ptr(), target.as_ptr()) } != 0 {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        let _ = (source, target);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Movimentação sem sobrescrita não suportada.",
        ))
    }
}

#[tauri::command]
fn move_book(root: String, relative_path: String, destination: String) -> Result<String, String> {
    move_book_with_policy(root, relative_path, destination, false)
}
fn move_book_with_policy(
    root: String,
    relative_path: String,
    destination: String,
    preserve_destination: bool,
) -> Result<String, String> {
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    if !root.is_dir() {
        return Err("Biblioteca inválida.".into());
    }
    let source = resolve_inside(&root, &relative_path)?;
    let target_dir = resolve_inside(&root, &destination)?;
    if !source.is_file() || !is_book(&source) || !target_dir.is_dir() {
        return Err("Selecione um livro e uma pasta válidos.".into());
    }
    if source.starts_with(root.join("99 - Catálogo"))
        || target_dir.starts_with(root.join("99 - Catálogo"))
    {
        return Err("Arquivos e pastas de serviço do catálogo são protegidos.".into());
    }
    let filename = source.file_name().ok_or("Nome de arquivo inválido.")?;
    let target = target_dir.join(filename);
    if source == target {
        return Err("O livro já está nesta pasta.".into());
    }
    if target.exists() {
        return Err(
            "Já existe um arquivo com este nome no destino. Nenhum arquivo foi substituído.".into(),
        );
    }
    let old_relative = source
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let new_relative = target
        .strip_prefix(&root)
        .unwrap()
        .to_string_lossy()
        .replace('\\', "/");
    let _manual_guard = manual_metadata::MUTATIONS
        .lock()
        .map_err(|_| "Índice manual indisponível.")?;
    manual_metadata::load(&root)?;
    let embedded_signature = metadata_cache::current_signature(&root, &old_relative).ok();
    // Fail before touching the filesystem if the edition index is already invalid.
    library_editions::load(&root)?;
    rename_without_replace(&source, &target).map_err(|e| format!("Falha ao mover o livro: {e}"))?;
    let legacy_undo =
        match sync_catalog_move(&root, &old_relative, &new_relative, preserve_destination) {
            Ok(undo) => undo,
            Err(error) => {
                let file = rename_without_replace(&target, &source);
                return Err(format!(
                    "Falha no catálogo: {error}; reversão de arquivo: {file:?}."
                ));
            }
        };
    let lists_undo = match catalog_lists::sync_move_refs(
        &root,
        &old_relative,
        &new_relative,
        preserve_destination,
    ) {
        Ok(undo) => undo,
        Err(error) => {
            let catalog = rollback_catalog_move(&root, legacy_undo);
            let file = rename_without_replace(&target, &source);
            return Err(format!(
                "Falha nas listas: {error}; reversão de catálogo: {catalog:?}, arquivo: {file:?}."
            ));
        }
    };
    let manual_undo = match manual_metadata::sync_move(&root, &old_relative, &new_relative) {
        Ok(undo) => undo,
        Err(error) => {
            let lists = catalog_lists::rollback_move(&root, lists_undo);
            let catalog = rollback_catalog_move(&root, legacy_undo);
            let file = rename_without_replace(&target, &source);
            return Err(format!(
                "Falha nos metadados manuais: {error}; reversão: {lists:?}, {catalog:?}, {file:?}."
            ));
        }
    };
    if let Err(error) = library_editions::sync_move(&root, &old_relative, &new_relative) {
        let manual = manual_metadata::rollback_move(&root, manual_undo);
        let lists = catalog_lists::rollback_move(&root, lists_undo);
        let catalog = rollback_catalog_move(&root, legacy_undo);
        let file = rename_without_replace(&target, &source);
        return Err(format!(
            "Falha ao mover registro de edição: {error}; reversão manual: {manual:?}, listas: {lists:?}, catálogo: {catalog:?}, arquivo: {file:?}."
        ));
    }
    metadata_cache::after_move(&root, &old_relative, &new_relative, embedded_signature);
    Ok(new_relative)
}

#[tauri::command]
fn move_books(
    root: String,
    relative_paths: Vec<String>,
    destination: String,
) -> Result<Vec<String>, String> {
    if relative_paths.is_empty() || relative_paths.len() > 100 {
        return Err("Selecione entre 1 e 100 livros por operação.".into());
    }
    let canonical_root = fs::canonicalize(&root).map_err(|error| error.to_string())?;
    if !canonical_root.is_dir() {
        return Err("Biblioteca inválida.".into());
    }
    let target_dir = resolve_inside(&canonical_root, &destination)?;
    if !target_dir.is_dir() || target_dir.starts_with(canonical_root.join("99 - Catálogo")) {
        return Err("Selecione uma pasta de destino válida, fora do catálogo.".into());
    }
    let mut sources = HashSet::new();
    let mut filenames = HashSet::new();
    let mut prepared = Vec::new();
    for relative in relative_paths {
        let source = resolve_inside(&canonical_root, &relative)?;
        if !source.is_file()
            || !is_book(&source)
            || source.starts_with(canonical_root.join("99 - Catálogo"))
        {
            return Err(format!("Arquivo não pode ser movido: {relative}"));
        }
        let filename = source
            .file_name()
            .ok_or("Nome de arquivo inválido.")?
            .to_os_string();
        let target = target_dir.join(&filename);
        if source == target {
            return Err(format!("O livro já está nessa pasta: {relative}"));
        }
        if target.exists() {
            return Err(format!(
                "Conflito: já existe {} no destino. Nada foi movido.",
                filename.to_string_lossy()
            ));
        }
        if !sources.insert(source.clone()) || !filenames.insert(filename) {
            return Err("Há livros repetidos ou nomes iguais na seleção. Nada foi movido.".into());
        }
        let old = source
            .strip_prefix(&canonical_root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        prepared.push(old);
    }
    let mut done: Vec<(String, String)> = Vec::new();
    let root_text = canonical_root.to_string_lossy().to_string();
    for old in prepared {
        match move_book(root_text.clone(), old.clone(), destination.clone()) {
            Ok(new) => done.push((old, new)),
            Err(error) => {
                let mut rollback_errors = Vec::new();
                for (original, moved) in done.iter().rev() {
                    let original_parent = Path::new(original).parent().unwrap_or(Path::new(""));
                    if let Err(recovery) = move_book(
                        root_text.clone(),
                        moved.clone(),
                        original_parent.to_string_lossy().to_string(),
                    ) {
                        rollback_errors.push(format!("{moved}: {recovery}"));
                    }
                }
                if rollback_errors.is_empty() {
                    return Err(format!("{error} A operação foi revertida."));
                }
                return Err(format!(
                    "{error} Reversão parcial: {}. Verifique esses arquivos.",
                    rollback_errors.join("; ")
                ));
            }
        }
    }
    Ok(done.into_iter().map(|(_, new)| new).collect())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let handler: fn(tauri::ipc::Invoke<tauri::Wry>) -> bool = tauri::generate_handler![
        load_collection,
        library_duplicates::scan_duplicate_books,
        manual_metadata::get_manual_book_metadata,
        manual_metadata::save_manual_book_metadata,
        load_initial_collection,
        refresh_indexed_collection,
        library_monitor::set_library_monitor,
        metadata_cache::extract_book_metadata,
        metadata_cache::reset_invalid_metadata_cache,
        catalog_lists::import_catalog_json,
        catalog_lists::link_catalog_file,
        catalog_lists::save_catalog_csv,
        covers::get_book_cover,
        create_folder,
        move_book,
        move_books,
        smart_organization::validate_organization_rules,
        smart_organization::preview_smart_organization,
        smart_organization::apply_smart_organization,
        organization::preview_organization,
        library_restructure::preview_library_restructure,
        organization::apply_organization,
        sources::list_sources,
        source_browser::open_source_webview,
        source_covers::get_source_cover,
        download_resolver::resolve_source_downloads,
        extension_repositories::list_repositories,
        extension_repositories::add_repository,
        extension_repositories::remove_repository,
        extension_repositories::browse_repository,
        extension_repositories::preview_repository_extension,
        extension_repositories::install_repository_extension,
        sources::install_source,
        sources::install_demo_source,
        sources::install_example_source,
        sources::set_source_enabled,
        sources::set_source_preferences,
        sources::remove_source,
        extension_host::search_sources,
        extension_host::browse_source,
        extension_host::list_source_descriptors,
        downloads::list_downloads,
        downloads::queue_download,
        download_naming::preview_download_filename,
        downloads::start_waiting_downloads,
        downloads::pause_download,
        downloads::cancel_download,
        downloads::retry_download,
        downloads::dismiss_download,
        downloads::save_download,
        downloads::rename_ready_download,
        downloads::recover_download_import,
        noctalia::read_noctalia_colors
    ];
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(downloads::DownloadManager::restore(app.handle()));
            app.manage(library_monitor::LibraryMonitor::default());
            if let Some(watcher) = noctalia::PaletteWatcher::start(app.handle()) {
                app.manage(watcher);
            }
            #[cfg(target_os = "macos")]
            source_browser::setup_macos_menu(app.handle()).map_err(std::io::Error::other)?;
            Ok(())
        })
        .invoke_handler(move |invoke: tauri::ipc::Invoke<tauri::Wry>| {
            if invoke.message.webview_ref().label() != "main" {
                invoke
                    .resolver
                    .reject("Esta janela não tem acesso aos comandos do Acervo.");
                return true;
            }
            handler(invoke)
        })
        .run(tauri::generate_context!())
        .expect("Falha ao iniciar o Acervo");
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rename_never_replaces_a_destination_even_during_rollback() {
        let dir = std::env::temp_dir().join(format!(
            "acervo-no-clobber-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&dir).unwrap();
        let source = dir.join("source");
        let target = dir.join("target");
        fs::write(&source, b"source").unwrap();
        fs::write(&target, b"existing").unwrap();
        assert!(rename_without_replace(&source, &target).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"source");
        assert_eq!(fs::read(&target).unwrap(), b"existing");
        fs::remove_file(&target).unwrap();
        rename_without_replace(&source, &target).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"source");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn move_book_updates_app_owned_catalog_links() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-sidecar-{}-{seed}", std::process::id()));
        for folder in ["A", "B", "99 - Catálogo"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("A/Obra.epub"), b"book").unwrap();
        fs::write(
            root.join("99 - Catálogo/Acervo_Listas.json"),
            r#"{"versao":1,"obras":[{"titulo":"Obra","local_indicado":"A","arquivos_encontrados":["A/Obra.epub"]}]}"#,
        ).unwrap();
        let new = move_book(
            root.to_string_lossy().to_string(),
            "A/Obra.epub".into(),
            "B".into(),
        )
        .unwrap();
        assert_eq!(new, "B/Obra.epub");
        assert!(root.join("B/Obra.epub").exists());
        assert!(!root.join("A/Obra.epub").exists());
        let data: serde_json::Value = serde_json::from_slice(
            &fs::read(root.join("99 - Catálogo/Acervo_Listas.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(data["obras"][0]["arquivos_encontrados"][0], "B/Obra.epub");
        assert_eq!(data["obras"][0]["local_indicado"], "B");
        assert!(root.join("99 - Catálogo/Acervo_Listas.json.bak").exists());
        let scanned = scan_collection(root.to_string_lossy().to_string()).unwrap();
        assert!(
            scanned
                .works
                .iter()
                .any(|item| item.title == "Obra" && item.present)
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn edition_index_survives_scan_and_app_moves_independently_of_download_history() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "acervo-editions-integration-{}-{seed}",
            std::process::id()
        ));
        fs::create_dir_all(root.join("A")).unwrap();
        fs::create_dir_all(root.join("B")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let book = root.join("A/Book.pdf");
        fs::write(&book, b"%PDF-1.7 edition").unwrap();
        let record = library_editions::EditionRecord {
            path: "A/Book.pdf".into(),
            source_name: "Fixture".into(),
            metadata: downloads::DownloadEdition {
                title: "Original".into(),
                author: "Author".into(),
                publisher: "Publisher".into(),
                language: "pt".into(),
                edition_label: "Second edition".into(),
                source_id: "fixture".into(),
                ..Default::default()
            },
            fingerprint: import_fingerprint::fingerprint(&book).unwrap(),
        };
        library_editions::record(&root, record.clone()).unwrap();
        let root_text = root.to_str().unwrap().to_owned();
        let first = scan_collection(root_text.clone()).unwrap();
        assert_eq!(first.files[0].edition.as_ref(), Some(&record));
        assert_eq!(
            move_book(root_text.clone(), "A/Book.pdf".into(), "B".into()).unwrap(),
            "B/Book.pdf"
        );
        let second = scan_collection(root_text.clone()).unwrap();
        let moved = second.files[0].edition.as_ref().unwrap();
        assert_eq!(moved.path, "B/Book.pdf");
        assert_eq!(moved.metadata, record.metadata);
        assert_eq!(moved.fingerprint, record.fingerprint);
        fs::write(
            root.join("B/Book.pdf"),
            b"%PDF-1.7 changed content and length",
        )
        .unwrap();
        let changed = scan_collection(root_text.clone()).unwrap();
        assert!(changed.files[0].edition.is_none());
        assert!(changed.warnings.iter().any(|w| w.contains("tamanho mudou")));
        // Stale fingerprint must block a move rather than move the record to unrelated bytes.
        assert!(move_book(root_text, "B/Book.pdf".into(), "A".into()).is_err());
        assert!(root.join("B/Book.pdf").is_file());
        assert!(!root.join("A/Book.pdf").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn failed_edition_move_restores_original_planned_catalog_destinations() {
        let root = std::env::temp_dir().join(format!(
            "acervo-move-undo-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for folder in ["A", "B", "99 - Catálogo"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        let root = fs::canonicalize(root).unwrap();
        let book = root.join("A/Book.pdf");
        fs::write(&book, b"%PDF-1.7 original").unwrap();
        let list = root.join("99 - Catálogo/Acervo_Listas.json");
        fs::write(&list,r#"{"versao":1,"obras":[{"titulo":"Book","local_indicado":"Planned","arquivos_encontrados":["A/Book.pdf"]}]}"#).unwrap();
        let before: serde_json::Value = serde_json::from_slice(&fs::read(&list).unwrap()).unwrap();
        let db = rusqlite::Connection::open(root.join("99 - Catálogo/Catalogo_Conhecimento.db"))
            .unwrap();
        db.execute_batch(r#"CREATE TABLE livros (id INTEGER PRIMARY KEY, arquivos_encontrados TEXT, local_indicado TEXT); INSERT INTO livros VALUES(1,'["A/Book.pdf"]','Legacy planned');"#).unwrap();
        library_editions::record(
            &root,
            library_editions::EditionRecord {
                path: "A/Book.pdf".into(),
                source_name: "Source".into(),
                metadata: downloads::DownloadEdition {
                    title: "Book".into(),
                    ..Default::default()
                },
                fingerprint: import_fingerprint::fingerprint(&book).unwrap(),
            },
        )
        .unwrap();
        fs::write(&book, b"%PDF-1.7 changed").unwrap();
        assert!(
            move_book(
                root.to_str().unwrap().into(),
                "A/Book.pdf".into(),
                "B".into()
            )
            .is_err()
        );
        assert!(book.is_file());
        assert!(!root.join("B/Book.pdf").exists());
        let after: serde_json::Value = serde_json::from_slice(&fs::read(list).unwrap()).unwrap();
        assert_eq!(before, after);
        let legacy: (String, String) = db
            .query_row(
                "SELECT arquivos_encontrados,local_indicado FROM livros WHERE id=1",
                [],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            legacy,
            (r#"["A/Book.pdf"]"#.into(), "Legacy planned".into())
        );
        drop(db);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn malformed_app_catalog_cancels_and_reverts_move() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-bad-sidecar-{}-{seed}", std::process::id()));
        for folder in ["A", "B", "99 - Catálogo"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("A/Obra.epub"), b"book").unwrap();
        fs::write(
            root.join("99 - Catálogo/Acervo_Listas.json"),
            b"invalid json",
        )
        .unwrap();
        assert!(
            move_book(
                root.to_string_lossy().to_string(),
                "A/Obra.epub".into(),
                "B".into()
            )
            .is_err()
        );
        assert!(root.join("A/Obra.epub").exists());
        assert!(!root.join("B/Obra.epub").exists());
        fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn strip_legacy_id_without_touching_filename() {
        assert_eq!(display_title("[L000024] O Banquete.epub"), "O Banquete");
        assert_eq!(
            file_id("[L000024] O Banquete.epub", "Filosofia/O Banquete.epub"),
            "L000024"
        );
        assert_eq!(display_title("Frankenstein.pdf"), "Frankenstein");
    }
    #[test]
    fn only_supported_books() {
        assert!(is_book(Path::new("Obra.EPUB")));
        assert!(!is_book(Path::new("settings.json")));
        assert!(!is_book(Path::new("notas.md")));
    }
    #[test]
    fn move_file_updates_legacy_catalog_without_overwriting() {
        use rusqlite::Connection;
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-fixture-{}-{seed}", std::process::id()));
        fs::create_dir_all(root.join("Origem")).unwrap();
        fs::create_dir(root.join("Destino")).unwrap();
        fs::create_dir(root.join("99 - Catálogo")).unwrap();
        fs::write(root.join("Origem").join("Livro.epub"), b"fixture").unwrap();
        let db = root.join("99 - Catálogo").join("Catalogo_Conhecimento.db");
        let conn = Connection::open(db).unwrap();
        conn.execute_batch("CREATE TABLE livros (id INTEGER PRIMARY KEY, arquivos_encontrados TEXT, local_indicado TEXT);").unwrap();
        conn.execute(
            "INSERT INTO livros VALUES (1, ?1, 'Origem')",
            [r#"["Origem/Livro.epub"]"#],
        )
        .unwrap();
        drop(conn);
        let path = root.to_string_lossy().to_string();
        let moved = move_book(path.clone(), "Origem/Livro.epub".into(), "Destino".into()).unwrap();
        assert_eq!(moved, "Destino/Livro.epub");
        assert!(!root.join("Origem/Livro.epub").exists());
        assert_eq!(
            fs::read(root.join("Destino/Livro.epub")).unwrap(),
            b"fixture"
        );
        let conn = Connection::open(root.join("99 - Catálogo/Catalogo_Conhecimento.db")).unwrap();
        let (links, destination): (String, String) = conn
            .query_row(
                "SELECT arquivos_encontrados, local_indicado FROM livros WHERE id=1",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(links, r#"["Destino/Livro.epub"]"#);
        assert_eq!(destination, "Destino");
        let result = move_book(path, "Origem/Livro.epub".into(), "Destino".into());
        assert!(result.is_err());
        drop(conn);
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn bulk_move_rejects_collisions_before_touching_files() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-bulk-fixture-{}-{seed}", std::process::id()));
        for folder in ["A", "B", "Destino"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("A/Livro.pdf"), b"first").unwrap();
        fs::write(root.join("B/Livro.pdf"), b"second").unwrap();
        let result = move_books(
            root.to_string_lossy().to_string(),
            vec!["A/Livro.pdf".into(), "B/Livro.pdf".into()],
            "Destino".into(),
        );
        assert!(result.unwrap_err().contains("nomes iguais"));
        assert_eq!(fs::read(root.join("A/Livro.pdf")).unwrap(), b"first");
        assert_eq!(fs::read(root.join("B/Livro.pdf")).unwrap(), b"second");
        assert!(!root.join("Destino/Livro.pdf").exists());
        fs::remove_dir_all(&root).unwrap();
    }
    #[test]
    fn bulk_move_updates_all_catalog_references() {
        use rusqlite::Connection;
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "acervo-multi-fixture-{}-{seed}",
            std::process::id()
        ));
        for folder in ["A", "B", "Destino", "99 - Catálogo"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("A/Um.epub"), b"one").unwrap();
        fs::write(root.join("B/Dois.pdf"), b"two").unwrap();
        let conn = Connection::open(root.join("99 - Catálogo/Catalogo_Conhecimento.db")).unwrap();
        conn.execute_batch("CREATE TABLE livros (id INTEGER PRIMARY KEY, arquivos_encontrados TEXT, local_indicado TEXT);").unwrap();
        conn.execute(
            "INSERT INTO livros VALUES(1, ?1, 'A')",
            [r#"["A/Um.epub"]"#],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO livros VALUES(2, ?1, 'B')",
            [r#"["B/Dois.pdf"]"#],
        )
        .unwrap();
        drop(conn);
        let paths = move_books(
            root.to_string_lossy().to_string(),
            vec!["A/Um.epub".into(), "B/Dois.pdf".into()],
            "Destino".into(),
        )
        .unwrap();
        assert_eq!(paths, vec!["Destino/Um.epub", "Destino/Dois.pdf"]);
        assert!(!root.join("A/Um.epub").exists());
        assert!(!root.join("B/Dois.pdf").exists());
        assert_eq!(fs::read(root.join("Destino/Um.epub")).unwrap(), b"one");
        assert_eq!(fs::read(root.join("Destino/Dois.pdf")).unwrap(), b"two");
        let conn = Connection::open(root.join("99 - Catálogo/Catalogo_Conhecimento.db")).unwrap();
        for (id, expected) in [(1, "Destino/Um.epub"), (2, "Destino/Dois.pdf")] {
            let (linked, location): (String, String) = conn
                .query_row(
                    "SELECT arquivos_encontrados, local_indicado FROM livros WHERE id=?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .unwrap();
            assert_eq!(
                serde_json::from_str::<Vec<String>>(&linked).unwrap(),
                vec![expected]
            );
            assert_eq!(location, "Destino");
        }
        drop(conn);
        fs::remove_dir_all(&root).unwrap();
    }
}
