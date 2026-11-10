mod catalog_lists;
mod covers;
mod downloads;
mod organization;
mod source_search;
mod sources;

use serde::Serialize;
use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BookFile {
    id: String,
    title: String,
    path: String,
    folder: String,
    format: String,
    bytes: u64,
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
    if depth > 80 || files.len() > 150_000 {
        warnings.push(
            "Limite de profundidade ou arquivos atingido; algumas pastas não foram indexadas."
                .into(),
        );
        return;
    }
    let entries = match fs::read_dir(current) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!("Pasta inacessível {}: {error}", current.display()));
            return;
        }
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(kind) = entry.file_type() else {
            continue;
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
            scan_dir(root, &path, files, folders, warnings, depth + 1);
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
                bytes: entry.metadata().map(|meta| meta.len()).unwrap_or(0),
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
        let paths: Vec<String> = serde_json::from_str(&associated).unwrap_or_default();
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
    rows.filter_map(Result::ok).collect()
}

#[tauri::command]
fn load_collection(root: String) -> Result<Collection, String> {
    let path = fs::canonicalize(&root).map_err(|e| format!("Pasta inválida: {e}"))?;
    if !path.is_dir() {
        return Err("Escolha uma pasta de biblioteca válida.".into());
    }
    let mut files = Vec::new();
    let mut folders = BTreeSet::new();
    let mut warnings = Vec::new();
    scan_dir(&path, &path, &mut files, &mut folders, &mut warnings, 0);
    files.sort_by(|a, b| a.title.to_lowercase().cmp(&b.title.to_lowercase()));
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

fn sync_catalog_move(root: &Path, old: &str, new: &str) -> Result<(), String> {
    use rusqlite::Connection;
    let db = root.join("99 - Catálogo").join("Catalogo_Conhecimento.db");
    if !db.exists() {
        return Ok(());
    }
    let mut connection = Connection::open(db).map_err(|e| e.to_string())?;
    let tx = connection.transaction().map_err(|e| e.to_string())?;
    {
        let mut stmt = tx
            .prepare("SELECT id,arquivos_encontrados FROM livros")
            .map_err(|e| e.to_string())?;
        let associations = stmt
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                ))
            })
            .map_err(|e| e.to_string())?;
        for association in associations {
            let (id, raw) = association.map_err(|e| e.to_string())?;
            let Ok(mut paths) = serde_json::from_str::<Vec<String>>(&raw) else {
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
            tx.execute(
                "UPDATE livros SET arquivos_encontrados=?1, local_indicado=?2 WHERE id=?3",
                (
                    serde_json::to_string(&paths).map_err(|e| e.to_string())?,
                    first_parent,
                    id,
                ),
            )
            .map_err(|e| e.to_string())?;
        }
    }
    tx.commit().map_err(|e| e.to_string())
}

#[tauri::command]
fn move_book(root: String, relative_path: String, destination: String) -> Result<String, String> {
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
    fs::rename(&source, &target).map_err(|e| format!("Falha ao mover o livro: {e}"))?;
    if let Err(error) = sync_catalog_move(&root, &old_relative, &new_relative) {
        let undo = fs::rename(&target, &source);
        return Err(if let Err(rollback) = undo {
            format!(
                "Falha no catálogo: {error}; reversão também falhou: {rollback}. Verifique manualmente o arquivo."
            )
        } else {
            format!("Catálogo não atualizado; a movimentação foi revertida: {error}")
        });
    }
    if let Err(error) = catalog_lists::sync_move_refs(&root, &old_relative, &new_relative) {
        let catalog_rollback = sync_catalog_move(&root, &new_relative, &old_relative);
        let file_rollback = fs::rename(&target, &source);
        return Err(match (catalog_rollback, file_rollback) {
            (Ok(()), Ok(())) => {
                format!("Falha ao atualizar listas; movimentação revertida: {error}")
            }
            (catalog, file) => format!(
                "Falha ao atualizar listas: {error}; reversão de catálogo: {catalog:?}; reversão de arquivo: {file:?}. Verifique a biblioteca."
            ),
        });
    }
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

#[tauri::command]
fn read_noctalia_colors() -> Option<serde_json::Value> {
    let base = std::env::var("XDG_CONFIG_HOME")
        .ok()
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var("HOME")
                .ok()
                .map(|home| PathBuf::from(home).join(".config"))
        })?;
    let content = fs::read_to_string(base.join("acervo").join("noctalia-colors.json")).ok()?;
    let map: serde_json::Map<String, serde_json::Value> = serde_json::from_str(&content).ok()?;
    let allowed = [
        "primary",
        "on_primary",
        "surface",
        "on_surface",
        "surface_container",
        "outline_variant",
    ];
    if !allowed.iter().all(|key| {
        map.get(*key)
            .and_then(|value| value.as_str())
            .map(|color| {
                color.starts_with('#')
                    && (color.len() == 7 || color.len() == 9)
                    && color[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
            })
            .unwrap_or(false)
    }) {
        return None;
    }
    Some(serde_json::Value::Object(map))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            app.manage(downloads::DownloadManager::restore(app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            load_collection,
            catalog_lists::import_catalog_json,
            covers::get_book_cover,
            create_folder,
            move_book,
            move_books,
            organization::preview_organization,
            organization::apply_organization,
            sources::list_sources,
            sources::install_source,
            sources::install_demo_source,
            sources::set_source_enabled,
            sources::remove_source,
            source_search::search_sources,
            downloads::list_downloads,
            downloads::queue_download,
            downloads::pause_download,
            downloads::cancel_download,
            downloads::retry_download,
            downloads::dismiss_download,
            downloads::save_download,
            read_noctalia_colors
        ])
        .run(tauri::generate_context!())
        .expect("Falha ao iniciar o Acervo");
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let scanned = load_collection(root.to_string_lossy().to_string()).unwrap();
        assert!(
            scanned
                .works
                .iter()
                .any(|item| item.title == "Obra" && item.present)
        );
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
