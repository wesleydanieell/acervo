//! Read-only suggestions and explicitly selected independent moves.
use super::{
    BookFile, Collection, downloads::destination_parts, move_book_with_policy, scan_collection,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

const LIMIT: usize = 100;
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Rules {
    catalog: bool,
    format: Option<String>,
    destination: Option<String>,
    #[serde(default)]
    custom: Vec<CustomRule>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CustomRule {
    name: String,
    enabled: bool,
    format: String,
    author: String,
    language: String,
    publisher: String,
    destination: String,
}

fn normalized(value: &str) -> String {
    value.trim().to_lowercase()
}

impl CustomRule {
    fn matches(&self, file: &BookFile) -> bool {
        if !self.enabled {
            return false;
        }
        let manual = file.manual.as_ref().map(|r| &r.metadata);
        let edition = file.edition.as_ref().map(|entry| &entry.metadata);
        let embedded = file
            .embedded
            .as_ref()
            .and_then(|entry| entry.metadata.as_ref());
        fn prefer<'a>(first: Option<&'a str>, second: Option<&'a str>) -> Option<&'a str> {
            first.filter(|value| !value.trim().is_empty()).or(second)
        }
        let matches_value = |wanted: &str, actual: Option<&str>| {
            wanted.is_empty()
                || actual.is_some_and(|value| {
                    !value.trim().is_empty() && normalized(value) == normalized(wanted)
                })
        };
        matches_value(&self.format, Some(&file.format))
            && matches_value(
                &self.author,
                prefer(
                    manual.and_then(|m| m.author.as_deref()),
                    prefer(
                        edition.map(|entry| entry.author.as_str()),
                        embedded.map(|entry| entry.author.as_str()),
                    ),
                ),
            )
            && matches_value(
                &self.language,
                prefer(
                    manual.and_then(|m| m.language.as_deref()),
                    prefer(
                        edition.map(|entry| entry.language.as_str()),
                        embedded.map(|entry| entry.language.as_str()),
                    ),
                ),
            )
            && matches_value(
                &self.publisher,
                prefer(
                    manual.and_then(|m| m.publisher.as_deref()),
                    prefer(
                        edition.map(|entry| entry.publisher.as_str()),
                        embedded.map(|entry| entry.publisher.as_str()),
                    ),
                ),
            )
    }
}

fn validate_rules(rules: &Rules) -> Result<(), String> {
    if rules.custom.len() > 20 {
        return Err("Limite de 20 regras personalizadas.".into());
    }
    let mut names = HashSet::new();
    for rule in &rules.custom {
        let valid_text = |value: &str, limit: usize| {
            value.len() <= limit && value.trim() == value && !value.chars().any(char::is_control)
        };
        if rule.name.is_empty()
            || !valid_text(&rule.name, 100)
            || !names.insert(normalized(&rule.name))
        {
            return Err("Nomes de regras devem ser únicos e não vazios (até 100 bytes).".into());
        }
        if !rule.format.is_empty()
            && !matches!(
                rule.format.as_str(),
                "PDF" | "EPUB" | "MOBI" | "AZW3" | "DJVU"
            )
        {
            return Err("Formato de regra inválido.".into());
        }
        if !valid_text(&rule.author, 500)
            || !valid_text(&rule.publisher, 500)
            || !valid_text(&rule.language, 25)
            || !rule
                .language
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err("Critério de metadados inválido.".into());
        }
        if [&rule.format, &rule.author, &rule.language, &rule.publisher]
            .iter()
            .all(|value| value.is_empty())
        {
            return Err(format!(
                "Adicione pelo menos um critério à regra {}.",
                rule.name
            ));
        }
        destination_parts(&rule.destination)?;
        if Path::new(&rule.destination).is_absolute()
            || Path::new(&rule.destination).components().any(|part| {
                matches!(
                    part,
                    std::path::Component::Prefix(_) | std::path::Component::ParentDir
                )
            })
        {
            return Err("Destino de regra fora da biblioteca.".into());
        }
    }
    match (&rules.format, &rules.destination) {
        (Some(format), Some(dest)) => {
            if !matches!(format.as_str(), "PDF" | "EPUB" | "MOBI" | "AZW3" | "DJVU") {
                return Err("Formato inválido.".into());
            }
            destination_parts(dest)?;
        }
        (None, None) => {}
        _ => return Err("Regra de formato incompleta.".into()),
    }
    Ok(())
}

#[tauri::command]
pub fn validate_organization_rules(rules: Rules) -> Result<(), String> {
    validate_rules(&rules)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Suggestion {
    source: String,
    destination: String,
    target: String,
    bytes: u64,
    modified_ms: Option<u64>,
    reason: String,
    work_id: Option<i64>,
    blocked: Option<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    items: Vec<Suggestion>,
    total: usize,
    warnings: Vec<String>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    moved: Vec<String>,
    errors: Vec<String>,
    not_attempted: usize,
}

fn existing_path(root: &Path, relative: &str, directory: bool) -> Result<PathBuf, String> {
    destination_parts(relative)?;
    if Path::new(relative).is_absolute()
        || Path::new(relative).components().any(|part| {
            matches!(
                part,
                std::path::Component::Prefix(_) | std::path::Component::ParentDir
            )
        })
    {
        return Err("Caminho fora da biblioteca.".into());
    }
    let path = root.join(relative);
    let resolved = fs::canonicalize(&path).map_err(|_| "Caminho ausente ou inacessível.")?;
    if !resolved.starts_with(root)
        || resolved != path
        || (if directory {
            !resolved.is_dir()
        } else {
            !resolved.is_file()
        })
    {
        return Err("Caminho inválido ou simbólico.".into());
    }
    Ok(resolved)
}
fn modified_ms(path: &Path) -> Option<u64> {
    path.metadata()
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()?
        .as_millis()
        .try_into()
        .ok()
}

fn preview(collection: &Collection, rules: &Rules) -> Result<Preview, String> {
    validate_rules(rules)?;
    let root = Path::new(&collection.root);
    let service = root.join("99 - Catálogo");
    for path in [
        &service,
        &service.join("Catalogo_Conhecimento.db"),
        &service.join("Acervo_Listas.json"),
    ] {
        match fs::symlink_metadata(path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Catálogo simbólico; organização bloqueada.".into());
            }
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                return Err(format!("Não foi possível verificar catálogo: {e}"));
            }
            _ => {}
        }
    }
    let format_rule = match (&rules.format, &rules.destination) {
        (Some(format), Some(dest)) => {
            if !matches!(format.as_str(), "PDF" | "EPUB" | "MOBI" | "AZW3" | "DJVU") {
                return Err("Formato inválido.".into());
            }
            existing_path(root, dest, true)?;
            Some((format, dest))
        }
        (None, None) => None,
        _ => return Err("Regra de formato incompleta.".into()),
    };
    let known_paths: HashSet<&str> = collection
        .files
        .iter()
        .map(|file| file.path.as_str())
        .collect();
    let mut associations: HashMap<&str, Vec<_>> = HashMap::new();
    for work in &collection.works {
        for path in &work.associated_files {
            if let Ok(resolved) = fs::canonicalize(root.join(path)) {
                if let Ok(relative) = resolved.strip_prefix(root) {
                    let canonical = relative.to_string_lossy().replace('\\', "/");
                    if canonical != *path && known_paths.contains(canonical.as_str()) {
                        return Err("Associação de catálogo usa caminho não canônico. Revise as referências antes de organizar.".into());
                    }
                }
            }

            let entries = associations.entry(path).or_default();
            if !entries
                .iter()
                .any(|existing: &&super::CatalogWork| existing.id == work.id)
            {
                entries.push(work);
            }
        }
    }
    let mut items = Vec::new();
    let mut warnings = collection.warnings.clone();
    // If a catalog could not be read, never classify its files as unassociated.
    if warnings
        .iter()
        .any(|warning| !warning.starts_with("Cache de metadados:"))
    {
        return Err(format!(
            "Revise os avisos da biblioteca antes de organizar: {}",
            warnings
                .iter()
                .take(3)
                .cloned()
                .collect::<Vec<_>>()
                .join("; ")
        ));
    }
    for file in &collection.files {
        let (dest, reason, mut blocked) = match associations.get(file.path.as_str()) {
            Some(works) if rules.catalog => {
                let work = works[0];
                let dest = if work.destination == "." {
                    String::new()
                } else {
                    work.destination.clone()
                };
                let blocked = if works.len() > 1 {
                    Some("Mais de uma obra associada.".to_owned())
                } else {
                    None
                };
                (dest, format!("Catálogo: {}", work.title), blocked)
            }
            // Custom rules never override an explicit catalog association, even
            // when catalog-directed moves are disabled.
            Some(_) => continue,
            None => {
                let mut matches: Vec<(&str, String)> = rules
                    .custom
                    .iter()
                    .filter(|rule| rule.matches(file))
                    .map(|rule| (rule.destination.as_str(), rule.name.clone()))
                    .collect();
                if let Some((format, dest)) = format_rule {
                    if &file.format == format {
                        matches.push((dest.as_str(), format!("Formato {format}")));
                    }
                }
                let Some((dest, _)) = matches.first() else {
                    continue;
                };
                let blocked = if matches.iter().any(|(other, _)| other != dest) {
                    Some("Regras compatíveis indicam destinos diferentes.".into())
                } else {
                    None
                };
                (
                    dest.to_string(),
                    format!(
                        "Regras: {}",
                        matches
                            .iter()
                            .map(|(_, name)| name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                    blocked,
                )
            }
        };
        if file.folder == dest && blocked.is_none() {
            continue;
        }
        let Some(filename) = Path::new(&file.path)
            .file_name()
            .and_then(|name| name.to_str())
        else {
            continue;
        };
        let target = if dest.is_empty() {
            filename.to_owned()
        } else {
            format!("{dest}/{filename}")
        };
        if blocked.is_none() {
            blocked = existing_path(root, &file.path, false)
                .err()
                .or_else(|| existing_path(root, &dest, true).err());
        }
        if blocked.is_none() {
            match fs::symlink_metadata(root.join(&target)) {
                Ok(_) => blocked = Some("Já existe um arquivo no destino.".into()),
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                    blocked = Some("Não foi possível verificar o destino.".into())
                }
                _ => {}
            }
        }
        items.push(Suggestion {
            source: file.path.clone(),
            destination: dest,
            target,
            bytes: file.bytes,
            modified_ms: modified_ms(&root.join(&file.path)),
            reason,
            work_id: associations.get(file.path.as_str()).and_then(|works| {
                if works.len() == 1 {
                    Some(works[0].id)
                } else {
                    None
                }
            }),
            blocked,
        });
    }
    let mut target_counts = HashMap::new();
    for item in items.iter().filter(|item| item.blocked.is_none()) {
        *target_counts.entry(item.target.clone()).or_insert(0usize) += 1;
    }
    for item in &mut items {
        if item.blocked.is_none() && target_counts.get(&item.target).copied().unwrap_or(0) > 1 {
            item.blocked = Some("Duas sugestões apontam para o mesmo arquivo.".into());
        }
    }
    items.sort_by(|a, b| {
        a.blocked
            .is_some()
            .cmp(&b.blocked.is_some())
            .then_with(|| a.source.cmp(&b.source))
    });
    let total = items.len();
    if total > LIMIT {
        warnings.push("Exibindo até 100 sugestões. Gere outra prévia após revisar o lote.".into());
    }
    items.truncate(LIMIT);
    Ok(Preview {
        items,
        total,
        warnings,
    })
}

#[tauri::command]
pub async fn preview_smart_organization(root: String, rules: Rules) -> Result<Preview, String> {
    tauri::async_runtime::spawn_blocking(move || preview(&scan_collection(root)?, &rules))
        .await
        .map_err(|e| e.to_string())?
}
#[tauri::command]
pub async fn apply_smart_organization(
    root: String,
    rules: Rules,
    selected: Vec<Suggestion>,
) -> Result<Applied, String> {
    tauri::async_runtime::spawn_blocking(move || apply(&root, &rules, selected))
        .await
        .map_err(|e| e.to_string())?
}
fn apply(root: &str, rules: &Rules, selected: Vec<Suggestion>) -> Result<Applied, String> {
    if selected.is_empty() || selected.len() > LIMIT {
        return Err("Selecione de 1 a 100 sugestões.".into());
    }
    let collection = scan_collection(root.into())?;
    if collection.root != root {
        return Err("Biblioteca mudou; gere outra prévia.".into());
    }
    let current = preview(&collection, rules)?;
    let by_path: HashMap<_, _> = current
        .items
        .iter()
        .map(|item| (&item.source, item))
        .collect();
    let mut seen = HashSet::new();
    for item in &selected {
        if !seen.insert(&item.source)
            || item.blocked.is_some()
            || by_path.get(&item.source).copied() != Some(item)
        {
            return Err("Sugestões alteradas ou conflitantes. Gere outra prévia.".into());
        }
    }
    let mut result = Applied {
        moved: vec![],
        errors: vec![],
        not_attempted: 0,
    };
    let selected_count = selected.len();
    for item in selected {
        // Check source identity by path and size again immediately before the move.
        // Each move has its own catalog rollback; do not undo unrelated successes.
        let operation = (|| {
            let source = existing_path(Path::new(root), &item.source, false)?;
            existing_path(Path::new(root), &item.destination, true)?;
            if source.metadata().map_err(|e| e.to_string())?.len() != item.bytes
                || modified_ms(&source) != item.modified_ms
            {
                return Err("Arquivo mudou desde a prévia.".into());
            }
            match fs::symlink_metadata(Path::new(root).join(&item.target)) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                _ => return Err("Destino ocupado ou inacessível.".into()),
            }
            move_book_with_policy(
                root.into(),
                item.source.clone(),
                item.destination.clone(),
                item.work_id.is_some(),
            )
        })();
        match operation {
            Ok(target) => result.moved.push(target),
            Err(error) => {
                result.errors.push(format!("{}: {error}", item.source));
                result.not_attempted = selected_count - result.moved.len() - 1;
                break;
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn catalog_associations_are_not_overridden_by_custom_rules() {
        let root = std::env::temp_dir().join(format!(
            "acervo-rule-catalog-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for folder in ["Inbox", "A", "B", "99 - Catálogo"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("Inbox/Book.epub"), b"book").unwrap();
        fs::write(root.join("99 - Catálogo/Acervo_Listas.json"),r#"{"versao":1,"obras":[{"titulo":"Book","local_indicado":"A","arquivos_encontrados":["Inbox/Book.epub"]}]}"#).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let text = root.to_str().unwrap();
        let mut rules = Rules {
            catalog: true,
            format: None,
            destination: None,
            custom: vec![CustomRule {
                name: "EPUB".into(),
                enabled: true,
                format: "EPUB".into(),
                author: String::new(),
                language: String::new(),
                publisher: String::new(),
                destination: "B".into(),
            }],
        };
        let collection = scan_collection(text.into()).unwrap();
        let p = preview(&collection, &rules).unwrap();
        assert_eq!(p.items.len(), 1);
        assert_eq!(p.items[0].target, "A/Book.epub");
        assert!(p.items[0].blocked.is_none());
        rules.catalog = false;
        assert!(preview(&collection, &rules).unwrap().items.is_empty());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn embedded_metadata_fills_missing_fields_without_overriding_acquired_metadata() {
        let mut file = BookFile {
            id: "test".into(),
            title: "File".into(),
            path: "File.epub".into(),
            folder: String::new(),
            format: "EPUB".into(),
            bytes: 4,
            manual: None,
            edition: None,
            embedded: Some(crate::metadata_cache::CachedMetadata {
                metadata: Some(crate::embedded_metadata::EmbeddedMetadata {
                    author: "Writer".into(),
                    language: "pt".into(),
                    publisher: "Press".into(),
                    ..Default::default()
                }),
                error: None,
            }),
        };
        let rule = CustomRule {
            name: "Rule".into(),
            enabled: true,
            format: "EPUB".into(),
            author: "writer".into(),
            language: "PT".into(),
            publisher: "press".into(),
            destination: "Books".into(),
        };
        assert!(rule.matches(&file));
        file.edition = Some(crate::library_editions::EditionRecord {
            path: file.path.clone(),
            source_name: "Source".into(),
            metadata: crate::downloads::DownloadEdition {
                author: "Other".into(),
                ..Default::default()
            },
            fingerprint: crate::import_fingerprint::FileFingerprint {
                bytes: 4,
                sha256: "a".repeat(64),
            },
        });
        assert!(!rule.matches(&file));
        file.edition.as_mut().unwrap().metadata.author.clear();
        assert!(rule.matches(&file));
        let root = std::env::temp_dir().join(format!("acervo-manual-rule-{}", std::process::id()));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("File.epub"), b"book").unwrap();
        let root = fs::canonicalize(root).unwrap();
        file.manual = Some(crate::manual_metadata::ManualRecord {
            metadata: crate::manual_metadata::ManualMetadata {
                author: Some("Manual".into()),
                ..Default::default()
            },
            identity: crate::manual_metadata::identity(&root, "File.epub").unwrap(),
        });
        assert!(!rule.matches(&file));
        file.manual.as_mut().unwrap().metadata.author = Some("Writer".into());
        file.edition.as_mut().unwrap().metadata.author = "Source".into();
        assert!(rule.matches(&file));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn reusable_rules_require_all_criteria_and_report_conflicts() {
        let root = std::env::temp_dir().join(format!(
            "acervo-rule-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for folder in ["Inbox", "A", "B"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        let source = root.join("Inbox/Book.epub");
        fs::write(&source, b"book").unwrap();
        fs::write(root.join("Inbox/Unknown.epub"), b"other").unwrap();
        let root = fs::canonicalize(root).unwrap();
        let text = root.to_str().unwrap();
        crate::library_editions::record(
            &root,
            crate::library_editions::EditionRecord {
                path: "Inbox/Book.epub".into(),
                source_name: "Example".into(),
                metadata: crate::downloads::DownloadEdition {
                    title: "Book".into(),
                    author: "Machado".into(),
                    language: "pt".into(),
                    publisher: "Editora".into(),
                    source_id: "example".into(),
                    ..Default::default()
                },
                fingerprint: crate::import_fingerprint::fingerprint(&source).unwrap(),
            },
        )
        .unwrap();
        let rule = CustomRule {
            name: "Português".into(),
            enabled: true,
            format: "EPUB".into(),
            author: "machado".into(),
            language: "PT".into(),
            publisher: "editora".into(),
            destination: "A".into(),
        };
        let mut rules = Rules {
            catalog: true,
            format: None,
            destination: None,
            custom: vec![rule.clone()],
        };
        let collection = scan_collection(text.into()).unwrap();
        let result = preview(&collection, &rules).unwrap();
        assert_eq!(result.items.len(), 1);
        assert_eq!(result.items[0].target, "A/Book.epub");
        assert!(result.items[0].blocked.is_none());
        rules.custom.push(CustomRule {
            name: "Conflito".into(),
            destination: "B".into(),
            ..rule.clone()
        });
        let result = preview(&collection, &rules).unwrap();
        assert_eq!(result.items.len(), 1);
        assert!(result.items[0].blocked.is_some());
        rules.custom[1].enabled = false;
        let result = preview(&collection, &rules).unwrap();
        assert!(result.items[0].blocked.is_none());
        rules.custom[1].name = "PORTUGUÊS".into();
        assert!(validate_rules(&rules).is_err());
        rules.custom.pop();
        rules.custom[0].publisher = "Outra".into();
        assert!(preview(&collection, &rules).unwrap().items.is_empty());
        rules.custom[0].publisher = "editora".into();
        let result = preview(&collection, &rules).unwrap();
        let done = apply(text, &rules, result.items).unwrap();
        assert_eq!(done.moved, vec!["A/Book.epub"]);
        assert!(root.join("Inbox/Unknown.epub").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn moves_selected_editions_without_changing_planned_destinations() {
        let root = std::env::temp_dir().join(format!(
            "acervo-smart-root-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for dir in ["Inbox", "Books", "99 - Catálogo"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in ["Inbox/A.pdf", "Inbox/A.epub", "Inbox/B.pdf"] {
            fs::write(root.join(file), b"book").unwrap();
        }
        let db = rusqlite::Connection::open(root.join("99 - Catálogo/Catalogo_Conhecimento.db"))
            .unwrap();
        db.execute_batch(r#"CREATE TABLE livros (id INTEGER PRIMARY KEY,titulo TEXT,autor TEXT,editoras_indicadas TEXT,local_indicado TEXT,arquivos_encontrados TEXT,prioridade_alta INTEGER);
        INSERT INTO livros VALUES(1,'A','Author','','Books','["Inbox/A.epub","Inbox/A.pdf"]',0);"#).unwrap();
        fs::write(root.join("99 - Catálogo/Acervo_Listas.json"),r#"{"versao":1,"obras":[{"titulo":"B","local_indicado":"","arquivos_encontrados":["Inbox/B.pdf"]}]}"#).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let text = root.to_str().unwrap();
        let rules = Rules {
            catalog: true,
            format: None,
            destination: None,
            custom: vec![],
        };
        let previewed = preview(&scan_collection(text.into()).unwrap(), &rules).unwrap();
        let selected: Vec<_> = previewed
            .items
            .into_iter()
            .filter(|item| item.source.ends_with(".pdf"))
            .collect();
        assert_eq!(selected.len(), 2);
        assert!(selected.iter().all(|item| item.blocked.is_none()));
        let done = apply(text, &rules, selected).unwrap();
        assert_eq!(done.moved.len(), 2);
        assert!(done.errors.is_empty());
        let planned: String = db
            .query_row("SELECT local_indicado FROM livros WHERE id=1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(planned, "Books");
        assert!(
            root.join("Books/A.pdf").exists()
                && root.join("B.pdf").exists()
                && root.join("Inbox/A.epub").exists()
        );
        let next = preview(&scan_collection(text.into()).unwrap(), &rules).unwrap();
        assert_eq!(next.items.len(), 1);
        assert_eq!(next.items[0].target, "Books/A.epub");
        drop(db);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn catalog_takes_precedence_and_conflicts_never_move() {
        let root = std::env::temp_dir().join(format!(
            "acervo-smart-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        for dir in ["Inbox", "Other", "Books", "PDF", "99 - Catálogo"] {
            fs::create_dir_all(root.join(dir)).unwrap();
        }
        for file in ["Inbox/A.pdf", "Inbox/B.pdf", "Other/B.pdf", "Inbox/C.pdf"] {
            fs::write(root.join(file), b"book").unwrap();
        }
        fs::write(root.join("99 - Catálogo/Acervo_Listas.json"),r#"{"versao":1,"obras":[{"titulo":"A","local_indicado":"Books","arquivos_encontrados":["Inbox/A.pdf"]}]}"#).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let text = root.to_str().unwrap();
        let rules = Rules {
            catalog: true,
            format: Some("PDF".into()),
            destination: Some("PDF".into()),
            custom: vec![],
        };
        let p = preview(&scan_collection(text.into()).unwrap(), &rules).unwrap();
        assert_eq!(p.total, 4);
        assert_eq!(p.items.iter().filter(|i| i.blocked.is_some()).count(), 2);
        assert!(p.items.iter().any(|i| i.target == "Books/A.pdf"));
        assert!(root.join("Inbox/A.pdf").is_file());
        let catalog = root.join("99 - Catálogo/Acervo_Listas.json");
        let original = fs::read(&catalog).unwrap();
        fs::write(&catalog,r#"{"versao":1,"obras":[{"titulo":"A","local_indicado":"Books","arquivos_encontrados":"Inbox/A.pdf"}]}"#).unwrap();
        assert!(preview(&scan_collection(text.into()).unwrap(), &rules).is_err());
        fs::write(&catalog, original).unwrap();
        let selected: Vec<_> = p
            .items
            .into_iter()
            .filter(|i| i.blocked.is_none())
            .collect();
        fs::write(root.join("PDF/C.pdf"), b"existing").unwrap();
        assert!(apply(text, &rules, selected.clone()).is_err());
        assert!(root.join("Inbox/A.pdf").is_file());
        fs::remove_file(root.join("PDF/C.pdf")).unwrap();
        let done = apply(text, &rules, selected).unwrap();
        assert_eq!(done.moved.len(), 2);
        assert!(done.errors.is_empty());
        assert!(root.join("Books/A.pdf").is_file() && root.join("PDF/C.pdf").is_file());
        assert!(root.join("Inbox/B.pdf").is_file() && root.join("Other/B.pdf").is_file());
        fs::remove_dir_all(root).unwrap();
    }
}
