//! App-owned wishlists live beside the legacy SQLite catalog, never inside it.
use super::{BookFile, CatalogWork};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

const LIST_FILE: &str = "Acervo_Listas.json";
static LIST_WRITE_LOCK: Mutex<()> = Mutex::new(());

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    imported: usize,
    skipped: usize,
    total: usize,
}

fn field<'a>(book: &'a Value, key: &str) -> &'a str {
    book.get(key).and_then(Value::as_str).unwrap_or("")
}

fn valid_relative_folder(path: &str) -> bool {
    let candidate = Path::new(path);
    !candidate.is_absolute()
        && path.len() <= 600
        && !path.chars().any(char::is_control)
        && !candidate.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
}

fn validate_book(book: &Value) -> Result<(), String> {
    let title = field(book, "titulo").trim();
    if title.is_empty() || title.len() > 500 {
        return Err("Cada obra deve ter um título de até 500 caracteres.".into());
    }
    if field(book, "autor").len() > 500 {
        return Err(format!("Autor longo demais: {title}"));
    }
    if !valid_relative_folder(field(book, "local_indicado")) {
        return Err(format!(
            "Destino inválido para {title}: não use caminhos absolutos ou '..'."
        ));
    }
    if let Some(publishers) = book.get("editoras_indicadas") {
        if !publishers.is_array() && !publishers.is_string() && !publishers.is_null() {
            return Err(format!("Editoras inválidas para {title}."));
        }
    }
    Ok(())
}

fn parse_list(raw: &str) -> Result<Vec<Value>, String> {
    let data: Value =
        serde_json::from_str(raw).map_err(|error| format!("JSON inválido: {error}"))?;
    let works = if data.is_array() {
        &data
    } else {
        data.get("obras").ok_or("Falta a chave 'obras'.")?
    };
    let works = works.as_array().ok_or("'obras' deve ser uma lista.")?;
    if works.len() > 20_000 {
        return Err("O limite é 20 mil obras por importação.".into());
    }
    for book in works {
        validate_book(book)?;
    }
    Ok(works.to_vec())
}

fn identity(book: &Value) -> String {
    ["titulo", "autor", "local_indicado"]
        .iter()
        .map(|key| field(book, key).trim().to_lowercase())
        .collect::<Vec<_>>()
        .join("\u{1f}")
}

pub(crate) fn list_path(root: &Path) -> std::path::PathBuf {
    root.join("99 - Catálogo").join(LIST_FILE)
}

pub(crate) fn load_values(path: &Path) -> Result<Vec<Value>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    if path
        .symlink_metadata()
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("O arquivo de listas não pode ser um link simbólico.".into());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("Não foi possível ler listas: {e}"))?;
    parse_list(&raw)
}

fn save_values(root: &Path, works: Vec<Value>) -> Result<(), String> {
    let catalog_dir = root.join("99 - Catálogo");
    fs::create_dir_all(&catalog_dir)
        .map_err(|error| format!("Falha ao criar pasta do catálogo: {error}"))?;
    if fs::canonicalize(&catalog_dir).map_err(|error| error.to_string())? != catalog_dir {
        return Err("Catálogo fora da biblioteca.".into());
    }

    let destination = list_path(root);
    for path in [&destination, &catalog_dir.join("Acervo_Listas.json.bak")] {
        match fs::symlink_metadata(path) {
            Ok(meta) if !meta.is_file() || meta.file_type().is_symlink() => {
                return Err("Arquivo de catálogo ou backup inválido.".into());
            }
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
                return Err(error.to_string());
            }
            _ => {}
        }
    }
    let payload = serde_json::to_vec_pretty(&json!({ "versao": 1, "obras": works }))
        .map_err(|error| error.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let staging = catalog_dir.join(format!(".acervo-save-{}-{stamp}.tmp", std::process::id()));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .map_err(|error| format!("Falha ao preparar gravação: {error}"))?;
        if let Err(error) = file.write_all(&payload).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&staging);
            return Err(format!("Falha ao gravar lista: {error}"));
        }
    }
    if destination.exists() {
        if let Err(error) = fs::copy(&destination, catalog_dir.join("Acervo_Listas.json.bak")) {
            let _ = fs::remove_file(&staging);
            return Err(format!("Backup não criado; operação cancelada: {error}"));
        }
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_file(&staging);
        return Err(format!("Falha ao salvar listas: {error}"));
    }
    Ok(())
}

/// Keep app-owned work associations in sync with file operations.
pub(super) struct MoveUndo {
    before: Vec<Value>,
    after: Vec<Value>,
}

pub(super) fn sync_move_refs(
    root: &Path,
    old: &str,
    new: &str,
    preserve_destination: bool,
) -> Result<Option<MoveUndo>, String> {
    let _guard = LIST_WRITE_LOCK
        .lock()
        .map_err(|_| "Catálogo indisponível.")?;
    let destination = list_path(root);
    let mut values = load_values(&destination)?;
    let before = values.clone();
    let mut changed = false;
    for work in &mut values {
        let mut found = false;
        if let Some(paths) = work
            .get_mut("arquivos_encontrados")
            .and_then(Value::as_array_mut)
        {
            for path in paths {
                if path.as_str() == Some(old) {
                    *path = Value::String(new.to_string());
                    found = true;
                }
            }
        }
        if found {
            let parent = Path::new(new).parent().unwrap_or(Path::new(""));
            if !preserve_destination && let Some(map) = work.as_object_mut() {
                map.insert(
                    "local_indicado".into(),
                    Value::String(parent.to_string_lossy().replace('\\', "/")),
                );
            }
            changed = true;
        }
    }
    if changed {
        save_values(root, values.clone())?;
        return Ok(Some(MoveUndo {
            before,
            after: values,
        }));
    }
    Ok(None)
}

pub(super) fn rollback_move(root: &Path, undo: Option<MoveUndo>) -> Result<(), String> {
    let Some(undo) = undo else {
        return Ok(());
    };
    let _guard = LIST_WRITE_LOCK
        .lock()
        .map_err(|_| "Catálogo indisponível.")?;
    if load_values(&list_path(root))? != undo.after {
        return Err("As listas mudaram durante a movimentação; não foi possível reverter sem sobrescrever outras alterações.".into());
    }
    save_values(root, undo.before)
}

pub fn load(root: &Path, files: &[BookFile], warnings: &mut Vec<String>) -> Vec<CatalogWork> {
    let path = list_path(root);
    let items = match load_values(&path) {
        Ok(items) => items,
        Err(error) => {
            warnings.push(format!("Listas do Acervo: {error}"));
            return Vec::new();
        }
    };
    works_from_values(items, files, warnings)
}

pub(crate) fn works_from_values(
    items: Vec<Value>,
    files: &[BookFile],
    warnings: &mut Vec<String>,
) -> Vec<CatalogWork> {
    let known: HashSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
    items
        .into_iter()
        .enumerate()
        .map(|(index, book)| {
            if book.get("arquivos_encontrados").is_some_and(|value| {
                !value.is_null()
                    && !value
                        .as_array()
                        .is_some_and(|items| items.iter().all(Value::is_string))
            }) && !warnings
                .iter()
                .any(|w| w.starts_with("Associações inválidas nas listas"))
            {
                warnings.push("Associações inválidas nas listas do Acervo.".into());
            }
            let associated: Vec<String> = book
                .get("arquivos_encontrados")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .filter_map(Value::as_str)
                        .map(str::to_owned)
                        .collect()
                })
                .unwrap_or_default();
            let publishers = match book.get("editoras_indicadas") {
                Some(Value::String(text)) => text.clone(),
                Some(value) => value.to_string(),
                None => String::new(),
            };
            CatalogWork {
                id: -(index as i64) - 1, // Safe for JavaScript; SQLite catalog IDs are positive.
                title: field(&book, "titulo").to_owned(),
                author: field(&book, "autor").to_owned(),
                publishers,
                destination: field(&book, "local_indicado").to_owned(),
                present: associated.iter().any(|path| known.contains(path.as_str())),
                priority: book
                    .get("prioridade_alta")
                    .map(|value| value == 1 || value == true)
                    .unwrap_or(false),
                associated_files: associated,
            }
        })
        .collect()
}

#[tauri::command]
pub fn import_catalog_json(root: String, json_text: String) -> Result<ImportReport, String> {
    let reservation = crate::library_mutations::Reservation::acquire(Path::new(&root))?;
    let _guard = LIST_WRITE_LOCK
        .lock()
        .map_err(|_| "Catálogo indisponível.")?;
    let root = reservation.root().to_path_buf();
    if !root.is_dir() {
        return Err("A raiz deve ser uma pasta.".into());
    }
    let incoming = parse_list(&json_text)?;
    let destination = list_path(&root);
    let mut existing = load_values(&destination)?;
    let mut ids: HashSet<String> = existing.iter().map(identity).collect();
    for work in super::read_catalog(&root, &[], &mut Vec::new()) {
        ids.insert(
            [work.title, work.author, work.destination]
                .iter()
                .map(|value| value.trim().to_lowercase())
                .collect::<Vec<_>>()
                .join("\u{1f}"),
        );
    }
    let mut imported = 0usize;
    let mut skipped = 0usize;
    for work in incoming {
        if ids.insert(identity(&work)) {
            existing.push(work);
            imported += 1;
        } else {
            skipped += 1;
        }
    }
    if imported == 0 {
        return Ok(ImportReport {
            imported,
            skipped,
            total: existing.len(),
        });
    }

    let total = existing.len();
    save_values(&root, existing)?;
    Ok(ImportReport {
        imported,
        skipped,
        total,
    })
}

/// A conservative persistent reference: a canonical library root and exact, unique work identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CatalogTarget {
    pub root: String,
    pub title: String,
    pub author: String,
    pub destination: String,
    #[serde(default)]
    pub legacy_id: Option<i64>,
}

impl CatalogTarget {
    pub fn validate(&self) -> Result<(), String> {
        if self.root.len() > 4096
            || self.title.trim().is_empty()
            || self.title.len() > 500
            || self.author.len() > 500
            || !valid_relative_folder(&self.destination)
        {
            return Err("Referência de catálogo inválida.".into());
        }
        let root = fs::canonicalize(&self.root).map_err(|error| error.to_string())?;
        if root.to_str() != Some(self.root.as_str()) || !root.is_dir() {
            return Err("Biblioteca de origem inválida.".into());
        }
        self.resolve_id(&root).map(|_| ())
    }

    fn resolve_id(&self, root: &Path) -> Result<i64, String> {
        let catalog_dir = root.join("99 - Catálogo");
        if fs::canonicalize(&catalog_dir).map_err(|error| error.to_string())? != catalog_dir {
            return Err("Diretório de catálogo inválido ou simbólico.".into());
        }
        if let Some(id) = self.legacy_id {
            let db = catalog_dir.join("Catalogo_Conhecimento.db");
            if id <= 0
                || !fs::symlink_metadata(&db)
                    .is_ok_and(|m| m.is_file() && !m.file_type().is_symlink())
            {
                return Err("Banco legado inválido.".into());
            }
            let conn = rusqlite::Connection::open_with_flags(
                db,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )
            .map_err(|e| e.to_string())?;
            self.check_legacy_identity(&conn, id)?;
            return Ok(id);
        }
        let works = load_values(&list_path(root))?;
        let matches: Vec<_> = works
            .iter()
            .enumerate()
            .filter(|(_, work)| {
                field(work, "titulo") == self.title
                    && field(work, "autor") == self.author
                    && field(work, "local_indicado") == self.destination
            })
            .collect();
        if matches.len() != 1 {
            return Err(
                "A ficha foi alterada, removida ou não é única; associe manualmente.".into(),
            );
        }
        Ok(-(matches[0].0 as i64) - 1)
    }

    fn check_legacy_identity(&self, conn: &rusqlite::Connection, id: i64) -> Result<(), String> {
        let current: (String, String, String) = conn.query_row(
            "SELECT COALESCE(titulo,''), COALESCE(autor,''), COALESCE(local_indicado,'') FROM livros WHERE id=?1",
            [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        ).map_err(|e| e.to_string())?;
        if current
            != (
                self.title.clone(),
                self.author.clone(),
                self.destination.clone(),
            )
        {
            return Err("Ficha legada alterada; confirme a associação manualmente.".into());
        }
        Ok(())
    }

    fn link_legacy(&self, root: &Path, id: i64, relative: &str) -> Result<(), String> {
        let file = super::resolve_inside(root, relative)?;
        let catalog_dir = root.join("99 - Catálogo");
        if file != root.join(relative)
            || !file.is_file()
            || !super::is_book(&file)
            || file.starts_with(&catalog_dir)
        {
            return Err("Arquivo de livro inválido.".into());
        }
        let db = catalog_dir.join("Catalogo_Conhecimento.db");
        if !fs::symlink_metadata(&db).is_ok_and(|m| m.is_file() && !m.file_type().is_symlink()) {
            return Err("Banco legado inválido.".into());
        }
        let works = load_values(&list_path(root))?;
        if works.iter().any(|work| {
            work.get("arquivos_encontrados")
                .and_then(Value::as_array)
                .is_some_and(|paths| paths.iter().any(|p| p.as_str() == Some(relative)))
        }) {
            return Err("Arquivo já associado a uma ficha JSON.".into());
        }
        let mut conn =
            rusqlite::Connection::open_with_flags(db, rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE)
                .map_err(|e| e.to_string())?;
        let tx = conn
            .transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)
            .map_err(|e| e.to_string())?;
        self.check_legacy_identity(&tx, id)?;
        let mut selected = None;
        {
            let mut stmt = tx
                .prepare("SELECT id, arquivos_encontrados FROM livros")
                .map_err(|e| e.to_string())?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?))
                })
                .map_err(|e| e.to_string())?;
            for row in rows {
                let (other, raw) = row.map_err(|e| e.to_string())?;
                let paths: Vec<String> = match raw.as_deref().map(str::trim) {
                    None | Some("") => vec![],
                    Some(text) => serde_json::from_str(text)
                        .map_err(|_| "Associações legadas inválidas; banco preservado.")?,
                };
                if other == id {
                    selected = Some(paths);
                } else if paths.iter().any(|p| p == relative) {
                    return Err("Arquivo já associado a outra obra.".into());
                }
            }
        }
        let mut paths = selected.ok_or("Ficha legada ausente.")?;
        if !paths.iter().any(|p| p == relative) {
            paths.push(relative.into());
            tx.execute(
                "UPDATE livros SET arquivos_encontrados=?1 WHERE id=?2",
                rusqlite::params![
                    serde_json::to_string(&paths).map_err(|e| e.to_string())?,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.commit().map_err(|e| e.to_string())
    }

    pub fn link(&self, root: &Path, relative: &str) -> Result<(), String> {
        if root.to_str() != Some(self.root.as_str()) {
            return Err("O download pertence a outra biblioteca.".into());
        }
        let _guard = LIST_WRITE_LOCK
            .lock()
            .map_err(|_| "Catálogo indisponível.")?;
        let id = self.resolve_id(root)?;
        if id > 0 {
            return self.link_legacy(root, id, relative);
        }
        link_catalog_file_locked(
            self.root.clone(),
            id,
            self.title.clone(),
            self.author.clone(),
            self.destination.clone(),
            relative.into(),
        )
    }
}

/// Associate an existing physical book with an explicitly selected app-owned work.
#[tauri::command]
pub fn link_catalog_file(
    root: String,
    work_id: i64,
    title: String,
    author: String,
    destination: String,
    relative_path: String,
) -> Result<(), String> {
    let reservation = crate::library_mutations::Reservation::acquire(Path::new(&root))?;
    let root = reservation
        .root()
        .to_str()
        .ok_or("Raiz não UTF-8.")?
        .to_owned();
    let _guard = LIST_WRITE_LOCK
        .lock()
        .map_err(|_| "Catálogo indisponível.")?;
    link_catalog_file_locked(root, work_id, title, author, destination, relative_path)
}

fn link_catalog_file_locked(
    root: String,
    work_id: i64,
    title: String,
    author: String,
    destination: String,
    relative_path: String,
) -> Result<(), String> {
    let root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let index = work_id
        .checked_neg()
        .and_then(|id| id.checked_sub(1))
        .and_then(|id| usize::try_from(id).ok())
        .ok_or("Ficha não pertence às listas do Acervo.")?;
    let catalog_dir = root.join("99 - Catálogo");
    if fs::canonicalize(&catalog_dir).map_err(|error| error.to_string())? != catalog_dir {
        return Err("Diretório de catálogo inválido ou simbólico.".into());
    }
    let backup = catalog_dir.join("Acervo_Listas.json.bak");
    if fs::symlink_metadata(&backup)
        .is_ok_and(|meta| !meta.is_file() || meta.file_type().is_symlink())
    {
        return Err("Backup do catálogo inválido.".into());
    }
    let file = super::resolve_inside(&root, &relative_path)?;
    if file != root.join(&relative_path)
        || !file.is_file()
        || !super::is_book(&file)
        || file.starts_with(&catalog_dir)
        || relative_path.contains('\\')
        || relative_path
            .split('/')
            .any(|part| part.is_empty() || part.starts_with('.') || part == "node_modules")
    {
        return Err("Selecione um arquivo de livro regular na biblioteca.".into());
    }
    let mut works = load_values(&list_path(&root))?;
    let work = works
        .get(index)
        .ok_or("Ficha não encontrada; atualize o catálogo.")?;
    if field(work, "titulo") != title
        || field(work, "autor") != author
        || field(work, "local_indicado") != destination
    {
        return Err("A ficha foi alterada; atualize o catálogo antes de associar.".into());
    }
    let mut warnings = Vec::new();
    let legacy = super::read_catalog(&root, &[], &mut warnings);
    if !warnings.is_empty() {
        return Err("Não foi possível verificar associações do catálogo legado.".into());
    }
    if legacy
        .iter()
        .any(|entry| entry.associated_files.contains(&relative_path))
        || works.iter().enumerate().any(|(other, entry)| {
            other != index
                && entry
                    .get("arquivos_encontrados")
                    .and_then(Value::as_array)
                    .is_some_and(|paths| {
                        paths
                            .iter()
                            .any(|path| path.as_str() == Some(&relative_path))
                    })
        })
    {
        return Err("Este arquivo já está associado a outra obra.".into());
    }
    let work = works
        .get_mut(index)
        .and_then(Value::as_object_mut)
        .ok_or("Ficha inválida.")?;
    let paths = work
        .entry("arquivos_encontrados")
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .ok_or("Associações da ficha inválidas.")?;
    if paths
        .iter()
        .any(|path| path.as_str() == Some(&relative_path))
    {
        return Ok(());
    }
    paths.push(Value::String(relative_path));
    save_values(&root, works)
}

/// Export only to the exact CSV path chosen in the native save dialog.
/// Exclusive creation avoids overwriting an existing user file.
#[tauri::command]
pub fn save_catalog_csv(path: String, csv_text: String) -> Result<String, String> {
    let destination = Path::new(&path);
    if !destination.is_absolute()
        || !destination
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("csv"))
    {
        return Err("Escolha um arquivo .csv em um caminho absoluto.".into());
    }
    if csv_text.is_empty() || csv_text.len() > 12_000_000 || csv_text.contains('\0') {
        return Err("Conteúdo CSV vazio, inválido ou maior que 12 MB.".into());
    }
    let parent = fs::canonicalize(
        destination
            .parent()
            .ok_or("Pasta de exportação inválida.")?,
    )
    .map_err(|error| format!("Pasta de exportação indisponível: {error}"))?;
    if !parent.is_dir() {
        return Err("Pasta de exportação inválida.".into());
    }
    let target = parent.join(destination.file_name().ok_or("Nome de arquivo inválido.")?);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
    }
    let mut file = options.open(&target).map_err(|error| {
        format!("Não foi possível salvar CSV sem substituir arquivo existente: {error}")
    })?;
    let saved = file
        .write_all("\u{feff}".as_bytes())
        .and_then(|_| file.write_all(csv_text.as_bytes()))
        .and_then(|_| file.sync_all());
    drop(file);
    if let Err(error) = saved {
        let _ = fs::remove_file(&target);
        return Err(format!("Falha ao gravar CSV: {error}"));
    }
    Ok(target.to_string_lossy().into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_csv_is_utf8_and_never_overwrites_existing_files() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir =
            std::env::temp_dir().join(format!("acervo-csv-test-{}-{unique}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let target = dir.join("lista.csv");
        let text = "titulo,autor\r\nO Banquete,Platão\r\n";
        let saved = save_catalog_csv(target.to_string_lossy().to_string(), text.into()).unwrap();
        assert_eq!(saved, target.to_string_lossy());
        assert_eq!(
            fs::read(&target).unwrap(),
            [vec![0xef, 0xbb, 0xbf], text.as_bytes().to_vec()].concat()
        );
        assert!(save_catalog_csv(target.to_string_lossy().to_string(), "outro".into()).is_err());
        assert_eq!(
            fs::read(&target).unwrap(),
            [vec![0xef, 0xbb, 0xbf], text.as_bytes().to_vec()].concat()
        );
        assert!(
            save_catalog_csv(dir.join("a.txt").to_string_lossy().to_string(), text.into()).is_err()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn validates_paths_and_json() {
        assert!(parse_list(r#"{"versao":1,"obras":[{"titulo":"Dom Casmurro","local_indicado":"Literatura/Brasileira"}]}"#).is_ok());
        assert!(
            parse_list(r#"{"obras":[{"titulo":"Perigo","local_indicado":"../../fora"}]}"#).is_err()
        );
        assert!(parse_list(r#"{"obras":[{"titulo":"Perigo","local_indicado":"/tmp"}]}"#).is_err());
        assert!(parse_list(r#"{"obras":[{"titulo":""}]}"#).is_err());
    }

    #[test]
    fn legacy_download_link_updates_only_associations_transactionally() {
        let root = std::env::temp_dir().join(format!(
            "acervo-legacy-link-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(root.join("99 - Catálogo")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let db = root.join("99 - Catálogo/Catalogo_Conhecimento.db");
        let conn = rusqlite::Connection::open(&db).unwrap();
        conn.execute_batch("CREATE TABLE livros(id INTEGER PRIMARY KEY,titulo TEXT,autor TEXT,editoras_indicadas TEXT,local_indicado TEXT,arquivos_encontrados TEXT,prioridade_alta INTEGER); INSERT INTO livros VALUES(1,'Book','Author','Publisher','Planned','[]',1),(2,'Other','','','','[]',0);").unwrap();
        fs::write(root.join("book.pdf"), b"%PDF-1.7").unwrap();
        let target = CatalogTarget {
            root: root.to_str().unwrap().into(),
            title: "Book".into(),
            author: "Author".into(),
            destination: "Planned".into(),
            legacy_id: Some(1),
        };
        target.validate().unwrap();
        target.link(&root, "book.pdf").unwrap();
        target.link(&root, "book.pdf").unwrap();
        let row: (String,String,String,i64) = conn.query_row("SELECT arquivos_encontrados,local_indicado,editoras_indicadas,prioridade_alta FROM livros WHERE id=1", [], |r| Ok((r.get(0)?,r.get(1)?,r.get(2)?,r.get(3)?))).unwrap();
        assert_eq!(
            row,
            (
                "[\"book.pdf\"]".into(),
                "Planned".into(),
                "Publisher".into(),
                1
            )
        );
        conn.execute("UPDATE livros SET titulo='Changed' WHERE id=1", [])
            .unwrap();
        assert!(target.link(&root, "book.pdf").is_err());
        drop(conn);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn download_reference_survives_reordering_but_not_identity_changes() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-target-{}-{unique}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let target = CatalogTarget {
            root: root.to_str().unwrap().into(),
            title: "Book".into(),
            author: "Author".into(),
            destination: "Planned".into(),
            legacy_id: None,
        };
        let first = json!({"titulo":"Book","autor":"Author","local_indicado":"Planned"});
        let other = json!({"titulo":"Other"});
        save_values(&root, vec![first.clone(), other.clone()]).unwrap();
        target.validate().unwrap();
        save_values(&root, vec![other.clone(), first.clone()]).unwrap();
        fs::write(root.join("Book.epub"), b"test").unwrap();
        target.link(&root, "Book.epub").unwrap();
        assert_eq!(
            load_values(&list_path(&root)).unwrap()[1]["arquivos_encontrados"],
            json!(["Book.epub"])
        );
        save_values(&root, vec![first.clone(), first]).unwrap();
        assert!(target.validate().is_err());
        save_values(&root, vec![other]).unwrap();
        assert!(target.link(&root, "Book.epub").is_err());
        assert!(root.join("Book.epub").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_file_association_validates_identity_and_preserves_other_works() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-link-{}-{unique}", std::process::id()));
        fs::create_dir_all(root.join("Books")).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let root_text = root.to_string_lossy().to_string();
        import_catalog_json(root_text.clone(), r#"{"obras":[{"titulo":"Book","autor":"Author","local_indicado":"Planned"},{"titulo":"Other","local_indicado":"Books"}]}"#.into()).unwrap();
        let file = "Books/book.epub";
        fs::write(root.join(file), b"test file").unwrap();
        let link = |id, title: &str, author: &str, dest: &str, path: &str| {
            link_catalog_file(
                root_text.clone(),
                id,
                title.into(),
                author.into(),
                dest.into(),
                path.into(),
            )
        };
        assert!(link(-1, "Wrong", "Author", "Planned", file).is_err());
        assert!(link(-1, "Book", "Author", "Planned", "../outside.epub").is_err());
        assert!(link(1, "Book", "Author", "Planned", file).is_err());
        link(-1, "Book", "Author", "Planned", file).unwrap();
        link(-1, "Book", "Author", "Planned", file).unwrap();
        assert!(link(-2, "Other", "", "Books", file).is_err());
        let values = load_values(&list_path(&root)).unwrap();
        assert_eq!(values[0]["arquivos_encontrados"], json!([file]));
        assert_eq!(values[0]["local_indicado"], "Planned");
        assert!(values[1].get("arquivos_encontrados").is_none());
        assert!(root.join(file).is_file());
        assert!(root.join("99 - Catálogo/Acervo_Listas.json.bak").is_file());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn import_is_idempotent_and_creates_sidecar() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-test-lists-{}-{unique}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let input = r#"{"versao":1,"obras":[{"titulo":"O Banquete","autor":"Platão","local_indicado":"Filosofia"}]}"#;
        let first =
            import_catalog_json(root.to_string_lossy().to_string(), input.to_string()).unwrap();
        assert_eq!(first.imported, 1);
        let again =
            import_catalog_json(root.to_string_lossy().to_string(), input.to_string()).unwrap();
        assert_eq!(again.imported, 0);
        assert_eq!(again.skipped, 1);
        let mut warnings = Vec::new();
        let result = load(&root, &[], &mut warnings);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].title, "O Banquete");
        fs::remove_dir_all(&root).unwrap();
    }
}
