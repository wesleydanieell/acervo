//! App-owned wishlists live beside the legacy SQLite catalog, never inside it.
use super::{BookFile, CatalogWork};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::HashSet;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Component, Path};
use std::time::{SystemTime, UNIX_EPOCH};

const LIST_FILE: &str = "Acervo_Listas.json";

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

fn list_path(root: &Path) -> std::path::PathBuf {
    root.join("99 - Catálogo").join(LIST_FILE)
}

fn load_values(path: &Path) -> Result<Vec<Value>, String> {
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

pub fn load(root: &Path, files: &[BookFile], warnings: &mut Vec<String>) -> Vec<CatalogWork> {
    let path = list_path(root);
    let items = match load_values(&path) {
        Ok(items) => items,
        Err(error) => {
            warnings.push(format!("Listas do Acervo: {error}"));
            return Vec::new();
        }
    };
    let known: HashSet<&str> = files.iter().map(|file| file.path.as_str()).collect();
    items
        .into_iter()
        .enumerate()
        .map(|(index, book)| {
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
    let root = fs::canonicalize(root).map_err(|e| format!("Biblioteca inválida: {e}"))?;
    if !root.is_dir() {
        return Err("A raiz deve ser uma pasta.".into());
    }
    let incoming = parse_list(&json_text)?;
    let catalog_dir = root.join("99 - Catálogo");
    fs::create_dir_all(&catalog_dir)
        .map_err(|e| format!("Falha ao criar pasta do catálogo: {e}"))?;
    let resolved_catalog = fs::canonicalize(&catalog_dir).map_err(|e| e.to_string())?;
    if !resolved_catalog.starts_with(&root) {
        return Err("Catálogo fora da biblioteca.".into());
    }
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

    let new_data = serde_json::to_vec_pretty(&json!({ "versao": 1, "obras": existing }))
        .map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let staging = catalog_dir.join(format!(".acervo-import-{}-{stamp}.tmp", std::process::id()));
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&staging)
            .map_err(|e| format!("Falha ao preparar gravação: {e}"))?;
        if let Err(error) = file.write_all(&new_data).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&staging);
            return Err(format!("Falha ao gravar lista: {error}"));
        }
    }
    if destination.exists() {
        if let Err(error) = fs::copy(&destination, catalog_dir.join("Acervo_Listas.json.bak")) {
            let _ = fs::remove_file(&staging);
            return Err(format!(
                "Backup não foi criado; importação cancelada: {error}"
            ));
        }
    }
    if let Err(error) = fs::rename(&staging, &destination) {
        let _ = fs::remove_file(&staging);
        return Err(format!("Falha ao concluir importação: {error}"));
    }
    Ok(ImportReport {
        imported,
        skipped,
        total: existing.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
