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
    pub blockers: Vec<ReferenceBlocker>,
    pub revision: String,
    pub edition_verification: EditionVerification,
}

const EDITION_VERIFICATION_BYTES: u64 = 1024 * 1024 * 1024;
const EDITION_VERIFICATION_FILES: usize = 512;

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditionVerification {
    checked: usize,
    deferred: usize,
    bytes_reserved: u64,
}

impl EditionVerification {
    fn reserve(&mut self, bytes: u64, limit: u64, max_files: usize) -> bool {
        // The fingerprint reader permits an extra byte to detect concurrent growth.
        let cost = bytes.saturating_add(1);
        if self.checked >= max_files || cost > limit.saturating_sub(self.bytes_reserved) {
            self.deferred += 1;
            return false;
        }
        self.checked += 1;
        self.bytes_reserved += cost;
        true
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceBlocker {
    path: String,
    reason: &'static str,
}

pub(crate) fn revision(value: &impl Serialize) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let bytes = serde_json::to_vec(value).map_err(|e| e.to_string())?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn physical_book_exists(root: &Path, path: &str) -> bool {
    if crate::manual_metadata::valid_path(path).is_err() {
        return false;
    }
    let mut current = root.to_path_buf();
    let mut parts = path.split('/').peekable();
    while let Some(part) = parts.next() {
        current.push(part);
        let Ok(meta) = fs::symlink_metadata(&current) else {
            return false;
        };
        if if parts.peek().is_some() {
            !meta.file_type().is_dir()
        } else {
            !meta.file_type().is_file()
        } {
            return false;
        }
    }
    true
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

    pub(crate) fn map(&self, path: &str) -> Result<Option<PathChange>, String> {
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
    works.sort_by_key(|work| work.id);
    let lists = crate::catalog_lists::load_values(&crate::catalog_lists::list_path(root))?;
    let legacy_revision = revision(&works)?;
    let list_revision = revision(&lists)?;
    works.extend(crate::catalog_lists::works_from_values(
        lists,
        &[],
        &mut warnings,
    ));
    if !warnings.is_empty() {
        return Err(format!(
            "Catálogo não pode ser planejado com segurança: {}",
            warnings.join("; ")
        ));
    }

    let mut result = ReferenceImpact::default();
    let mut affected_paths = std::collections::BTreeSet::new();
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
                affected_paths.insert(change.before.clone());
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
    result.revision = revision(&(1u8, legacy_revision, list_revision, &manual, &editions))?;
    affected_paths.extend(
        result
            .manual_metadata
            .iter()
            .chain(&result.editions)
            .map(|change| change.before.clone()),
    );
    for path in affected_paths {
        if !physical_book_exists(root, &path) {
            result.blockers.push(ReferenceBlocker {
                path,
                reason: "missingOrInvalidBook",
            });
            continue;
        }
        if let Some(record) = manual.entries.get(&path) {
            if crate::manual_metadata::identity(root, &path).ok().as_ref() != Some(&record.identity)
            {
                result.blockers.push(ReferenceBlocker {
                    path: path.clone(),
                    reason: "staleManualIdentity",
                });
            }
        }
        if let Some(record) = editions.get(&path) {
            if !result.edition_verification.reserve(
                record.fingerprint.bytes,
                EDITION_VERIFICATION_BYTES,
                EDITION_VERIFICATION_FILES,
            ) {
                result.blockers.push(ReferenceBlocker {
                    path,
                    reason: "editionVerificationDeferred",
                });
            } else if crate::library_editions::verify_book(root, &path, &record.fingerprint)
                .is_err()
            {
                result.blockers.push(ReferenceBlocker {
                    path,
                    reason: "staleOrUnreadableEdition",
                });
            }
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn edition_verification_budget_is_bounded_and_defers_without_claiming_success() {
        let mut v = EditionVerification::default();
        assert!(v.reserve(4, 10, 2));
        assert!(!v.reserve(6, 10, 2));
        assert!(v.reserve(4, 10, 2));
        assert!(!v.reserve(0, 10, 2));
        assert_eq!((v.checked, v.deferred, v.bytes_reserved), (2, 2, 10));
    }

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
