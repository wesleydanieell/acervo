//! Explicit, preflighted JSON moves. Never apply an AI-generated plan without preview and confirmation.
use super::{is_book, move_book, resolve_inside};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OrganizationPlan {
    versao: u8,
    movimentacoes: Vec<RequestedMove>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestedMove {
    arquivo: String,
    destino: String,
    #[serde(default)]
    tamanho_bytes: Option<u64>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlannedMove {
    source: String,
    destination: String,
    target: String,
    bytes: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrganizationPreview {
    items: Vec<PlannedMove>,
    total_bytes: u64,
}

fn validate_plan(root: &Path, json_text: &str) -> Result<OrganizationPreview, String> {
    if json_text.len() > 1_000_000 {
        return Err("O plano JSON excede 1 MB.".into());
    }
    let plan: OrganizationPlan = serde_json::from_str(json_text)
        .map_err(|error| format!("JSON de organização inválido: {error}"))?;
    if plan.versao != 1 {
        return Err("Versão de plano não suportada: use versao=1.".into());
    }
    if plan.movimentacoes.is_empty() || plan.movimentacoes.len() > 100 {
        return Err("Um plano deve conter de 1 a 100 movimentações.".into());
    }
    let root = fs::canonicalize(root).map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("Biblioteca inválida.".into());
    }

    let mut sources: HashSet<PathBuf> = HashSet::new();
    let mut targets: HashSet<PathBuf> = HashSet::new();
    let mut items = Vec::with_capacity(plan.movimentacoes.len());
    let mut total_bytes = 0u64;
    for requested in plan.movimentacoes {
        let source = resolve_inside(&root, &requested.arquivo)?;
        let destination = resolve_inside(&root, &requested.destino)?;
        if !source.is_file() || !is_book(&source) || source.starts_with(root.join("99 - Catálogo"))
        {
            return Err(format!(
                "Arquivo não encontrado ou protegido: {}",
                requested.arquivo
            ));
        }
        if fs::symlink_metadata(root.join(&requested.arquivo))
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
        {
            return Err(format!(
                "Links simbólicos não podem ser movidos: {}",
                requested.arquivo
            ));
        }
        if !destination.is_dir() || destination.starts_with(root.join("99 - Catálogo")) {
            return Err(format!(
                "Destino inválido ou protegido: {}",
                requested.destino
            ));
        }
        let size = source.metadata().map_err(|error| error.to_string())?.len();
        if let Some(expected) = requested.tamanho_bytes {
            if size != expected {
                return Err(format!(
                    "Arquivo foi modificado desde a exportação: {}",
                    requested.arquivo
                ));
            }
        }
        let target = destination.join(source.file_name().ok_or("Nome de arquivo inválido.")?);
        if target == source || target.exists() {
            return Err(format!(
                "Destino já contém o livro ou o caminho é o mesmo: {}",
                target.display()
            ));
        }
        if !sources.insert(source.clone()) || !targets.insert(target.clone()) {
            return Err("O JSON contém arquivos duplicados ou destinos conflitantes.".into());
        }
        let relative = |path: &Path| {
            path.strip_prefix(&root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
        };
        total_bytes = total_bytes.saturating_add(size);
        items.push(PlannedMove {
            source: relative(&source),
            destination: relative(&destination),
            target: relative(&target),
            bytes: size,
        });
    }
    Ok(OrganizationPreview { items, total_bytes })
}

#[tauri::command]
pub fn preview_organization(
    root: String,
    json_text: String,
) -> Result<OrganizationPreview, String> {
    validate_plan(Path::new(&root), &json_text)
}

#[tauri::command]
pub fn apply_organization(root: String, json_text: String) -> Result<Vec<String>, String> {
    // Always re-validate at application time in case the filesystem changed after preview.
    let preview = validate_plan(Path::new(&root), &json_text)?;
    let canonical = fs::canonicalize(root).map_err(|error| error.to_string())?;
    let root_text = canonical.to_string_lossy().to_string();
    let mut finished: Vec<(String, String)> = Vec::new();

    for item in preview.items {
        match move_book(root_text.clone(), item.source.clone(), item.destination) {
            Ok(new_path) => finished.push((item.source, new_path)),
            Err(error) => {
                let mut rollback_errors = Vec::new();
                for (original, moved) in finished.iter().rev() {
                    let parent = Path::new(original).parent().unwrap_or(Path::new(""));
                    if let Err(rollback) = move_book(
                        root_text.clone(),
                        moved.clone(),
                        parent.to_string_lossy().to_string(),
                    ) {
                        rollback_errors.push(format!("{moved}: {rollback}"));
                    }
                }
                if rollback_errors.is_empty() {
                    return Err(format!("{error} A operação foi revertida."));
                }
                return Err(format!(
                    "{error} Reversão incompleta: {}. Revise os arquivos indicados.",
                    rollback_errors.join("; ")
                ));
            }
        }
    }
    Ok(finished.into_iter().map(|(_, target)| target).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn fixture() -> PathBuf {
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-organize-{}-{seed}", std::process::id()));
        for folder in ["Entrada", "Literatura", "Didaticos", "99 - Catálogo"] {
            fs::create_dir_all(root.join(folder)).unwrap();
        }
        fs::write(root.join("Entrada/A.epub"), b"abc").unwrap();
        fs::write(root.join("Entrada/B.pdf"), b"books").unwrap();
        root
    }

    #[test]
    fn previews_without_modifying_files_and_applies_explicit_moves() {
        let root = fixture();
        let json = r#"{"versao":1,"movimentacoes":[{"arquivo":"Entrada/A.epub","destino":"Literatura","tamanho_bytes":3},{"arquivo":"Entrada/B.pdf","destino":"Didaticos","tamanho_bytes":5}]}"#;
        let preview =
            preview_organization(root.to_string_lossy().to_string(), json.into()).unwrap();
        assert_eq!(preview.items.len(), 2);
        assert_eq!(preview.total_bytes, 8);
        assert!(root.join("Entrada/A.epub").exists());
        let done = apply_organization(root.to_string_lossy().to_string(), json.into()).unwrap();
        assert_eq!(done, vec!["Literatura/A.epub", "Didaticos/B.pdf"]);
        assert!(root.join("Literatura/A.epub").exists());
        assert!(root.join("Didaticos/B.pdf").exists());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn rejects_conflicting_and_unsafe_plans_before_touching_disk() {
        let root = fixture();
        fs::write(root.join("Literatura/A.epub"), b"existing").unwrap();
        for json in [
            r#"{"versao":1,"movimentacoes":[{"arquivo":"Entrada/A.epub","destino":"Literatura"}]}"#,
            r#"{"versao":1,"movimentacoes":[{"arquivo":"../outside.epub","destino":"Didaticos"}]}"#,
            r#"{"versao":1,"movimentacoes":[{"arquivo":"Entrada/B.pdf","destino":"Didaticos","tamanho_bytes":99}]}"#,
            r#"{"versao":1,"movimentacoes":[{"arquivo":"Entrada/A.epub","destino":"99 - Catálogo"}]}"#,
        ] {
            assert!(preview_organization(root.to_string_lossy().to_string(), json.into()).is_err());
        }
        assert!(
            crate::move_book(
                root.to_string_lossy().to_string(),
                "Entrada/A.epub".into(),
                "99 - Catálogo".into()
            )
            .is_err()
        );
        assert!(
            crate::create_folder(
                root.to_string_lossy().to_string(),
                "99 - Catálogo".into(),
                "Nova pasta".into()
            )
            .is_err()
        );
        assert!(root.join("Entrada/A.epub").exists());
        assert!(root.join("Entrada/B.pdf").exists());
        assert_eq!(
            fs::read(root.join("Literatura/A.epub")).unwrap(),
            b"existing"
        );
        fs::remove_dir_all(root).unwrap();
    }
}
