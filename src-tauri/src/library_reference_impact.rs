//! Read-only reference impact for a validated, non-overlapping folder plan.
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PathChange {
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkImpact {
    pub work_id: i64,
    pub title: String,
    pub associated_files: Vec<PathChange>,
    pub planned_destination: Option<PathChange>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceImpact {
    pub works: Vec<WorkImpact>,
    pub manual_metadata: Vec<PathChange>,
    pub editions: Vec<PathChange>,
}

/// A single simultaneous substitution, never cascading across operations.
/// Only canonical slash-separated relative references are accepted.
pub(crate) struct PathRemap<'a> {
    moves: Vec<(&'a str, &'a str)>,
}

impl<'a> PathRemap<'a> {
    pub(crate) fn new(moves: Vec<(&'a str, &'a str)>) -> Self {
        Self { moves }
    }

    fn map(&self, path: &str) -> Result<Option<PathChange>, String> {
        if path.is_empty() || path == "." {
            return Ok(None);
        }
        if path.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || p.contains(['\\', ':'])
                || p.chars().any(char::is_control)
        }) {
            return Err(format!("Referência de caminho não canônica: {path}"));
        }
        for &(from, to) in &self.moves {
            let after = if path == from {
                to.to_owned()
            } else if let Some(suffix) = path
                .strip_prefix(from)
                .and_then(|rest| rest.strip_prefix('/'))
            {
                format!("{to}/{suffix}")
            } else {
                continue;
            };
            return Ok(Some(PathChange {
                before: path.into(),
                after,
            }));
        }
        Ok(None)
    }
}

fn index_impact<'a>(
    remap: &PathRemap<'_>,
    paths: impl Iterator<Item = &'a String>,
    validate: impl Fn(&str) -> Result<(), String>,
) -> Result<Vec<PathChange>, String> {
    let mut targets = HashSet::new();
    let mut changes = Vec::new();
    for path in paths {
        let change = remap.map(path)?;
        let target = change.as_ref().map_or(path.as_str(), |c| c.after.as_str());
        validate(target)?;
        if !targets.insert(target.to_owned()) {
            return Err(format!("Conflito de registro no destino: {target}"));
        }
        if let Some(change) = change {
            changes.push(change);
        }
    }
    Ok(changes)
}

pub(crate) fn preview(root: &Path, remap: &PathRemap<'_>) -> Result<ReferenceImpact, String> {
    if remap.moves.is_empty() {
        return Ok(ReferenceImpact::default());
    }
    // Do not follow a redirected service directory or catalog database.
    if let Some(service) = crate::library_editions::service(root)? {
        for name in ["Catalogo_Conhecimento.db", "Acervo_Listas.json"] {
            match fs::symlink_metadata(service.join(name)) {
                Ok(meta) if !meta.file_type().is_file() => {
                    return Err(format!("Arquivo de catálogo inválido: {name}"));
                }
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.to_string()),
                _ => {}
            }
        }
    }
    let mut warnings = Vec::new();
    let mut works = crate::read_catalog(root, &[], &mut warnings);
    works.extend(crate::catalog_lists::load(root, &[], &mut warnings));
    if !warnings.is_empty() {
        return Err(format!(
            "Catálogo não pode ser planejado com segurança: {}",
            warnings.join("; ")
        ));
    }

    let mut result = ReferenceImpact::default();
    let mut list_identities = HashMap::<(String, String, String), bool>::new();
    for work in works {
        let destination = remap.map(&work.destination)?;
        // JSON work identity includes its destination. Do not introduce ambiguity.
        if work.id < 0 {
            let identity = (
                work.title.trim().to_lowercase(),
                work.author.trim().to_lowercase(),
                destination
                    .as_ref()
                    .map_or(work.destination.as_str(), |c| c.after.as_str())
                    .trim()
                    .to_lowercase(),
            );
            let changed = destination.is_some();
            if let Some(previous_changed) = list_identities.insert(identity, changed) {
                if previous_changed || changed {
                    return Err(
                        "O plano criaria uma identidade ambígua nas listas do catálogo.".into(),
                    );
                }
            }
        }
        let mut changes = Vec::new();
        let mut targets = HashMap::<String, String>::new();
        for path in &work.associated_files {
            let change = remap.map(path)?;
            let target = change.as_ref().map_or(path.as_str(), |c| c.after.as_str());
            if let Some(previous) = targets.insert(target.to_owned(), path.clone()) {
                if previous != *path {
                    return Err(format!(
                        "Associações conflitantes na obra {}: {target}",
                        work.id
                    ));
                }
            }
            if let Some(change) = change {
                changes.push(change);
            }
        }
        if !changes.is_empty() || destination.is_some() {
            result.works.push(WorkImpact {
                work_id: work.id,
                title: work.title,
                associated_files: changes,
                planned_destination: destination,
            });
        }
    }

    let manual = crate::manual_metadata::load(root)?;
    result.manual_metadata = index_impact(
        remap,
        manual.entries.keys(),
        crate::manual_metadata::valid_path,
    )?;
    let editions = crate::library_editions::load(root)?;
    result.editions = index_impact(remap, editions.keys(), crate::library_editions::valid_path)?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn component_mapping_is_not_a_text_replacement_or_cascade() {
        let map = PathRemap::new(vec![("A", "New"), ("New", "Other")]);
        assert_eq!(map.map("A").unwrap().unwrap().after, "New");
        assert_eq!(
            map.map("A/Vazia/Á.epub").unwrap().unwrap().after,
            "New/Vazia/Á.epub"
        );
        for untouched in ["AB/Book.pdf", "X/A", "", "."] {
            assert!(map.map(untouched).unwrap().is_none());
        }
        for invalid in ["A/../B", "/A", "A//B", "A\\B", "A/./B"] {
            assert!(map.map(invalid).is_err());
        }
    }

    #[test]
    fn stale_target_index_conflicts_are_not_overwritten() {
        let map = PathRemap::new(vec![("A", "B")]);
        let entries = ["A/book.epub".to_owned(), "B/book.epub".to_owned()];
        assert!(index_impact(&map, entries.iter(), |_| Ok(())).is_err());
    }
}
