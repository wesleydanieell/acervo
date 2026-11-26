//! Read-only download impact. Never rewrite historical records or guess an unbound root.
use super::{DownloadInfo, DownloadManager, destination_parts};
use crate::library_reference_impact::{PathChange, PathRemap};
use serde::Serialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

#[derive(Debug, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub(crate) struct DownloadImpact {
    changes: Vec<DownloadChange>,
    blockers: Vec<DownloadBlocker>,
    destination_conflicts: Vec<DestinationConflict>,
    pub(crate) revision: String,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadChange {
    id: String,
    title: String,
    status: String,
    planned_destination: Option<PathChange>,
    catalog_destination: Option<PathChange>,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadBlocker {
    id: String,
    title: String,
    reason: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DestinationConflict {
    target: String,
    download_ids: Vec<String>,
}

/// Only newly converging planned paths are reported here. Existing duplicate
/// destinations, unbound jobs and historical downloads have separate semantics.
fn destination_conflicts(
    root: &str,
    remap: &PathRemap<'_>,
    records: &[DownloadInfo],
) -> Result<Vec<DestinationConflict>, String> {
    let mut groups = BTreeMap::<String, BTreeSet<(String, String)>>::new();
    for info in records {
        let Some(target) = &info.catalog_target else {
            continue;
        };
        if target.root != root || info.status == "saved" || info.import_intent.is_some() {
            continue;
        }
        // Both manually suggested and automatic import destinations are meaningful.
        for destination in [&info.planned_destination, &target.destination] {
            let destination = if destination == "." {
                ""
            } else {
                destination.as_str()
            };
            if destination_parts(destination).is_err() {
                continue;
            }
            let Ok(change) = remap.map(destination) else {
                continue;
            };
            let after = change
                .as_ref()
                .map_or(destination, |change| change.after.as_str());
            if destination_parts(after).is_err() {
                continue;
            }
            let join = |dir: &str| {
                if dir.is_empty() {
                    info.filename.clone()
                } else {
                    format!("{dir}/{}", info.filename)
                }
            };
            groups
                .entry(join(after))
                .or_default()
                .insert((info.id.clone(), join(destination)));
        }
    }
    Ok(groups
        .into_iter()
        .filter_map(|(target, entries)| {
            let ids: BTreeSet<_> = entries.iter().map(|(id, _)| id.clone()).collect();
            let origins: BTreeSet<_> = entries.iter().map(|(_, path)| path).collect();
            if ids.len() < 2 || origins.len() < 2 {
                return None;
            }
            Some(DestinationConflict {
                target,
                download_ids: ids.into_iter().collect(),
            })
        })
        .collect())
}

fn block(result: &mut DownloadImpact, info: &DownloadInfo, reason: &'static str) {
    result.blockers.push(DownloadBlocker {
        id: info.id.clone(),
        title: info.title.clone(),
        reason,
    });
}

pub(crate) fn preview(
    root: &Path,
    remap: &PathRemap<'_>,
    manager: &DownloadManager,
) -> Result<DownloadImpact, String> {
    analyze(root, remap, &manager.snapshot()?)
}

fn analyze(
    root: &Path,
    remap: &PathRemap<'_>,
    records: &[DownloadInfo],
) -> Result<DownloadImpact, String> {
    let root = root.to_str().ok_or("Raiz não UTF-8.")?;
    let mut result = DownloadImpact::default();
    // Scope to this library and unbound records; ignore saved history and progress counters.
    let mut relevant: Vec<_> = records
        .iter()
        .filter(|info| {
            info.import_intent
                .as_ref()
                .is_some_and(|intent| intent.root == root)
                || (info.status != "saved"
                    && info
                        .catalog_target
                        .as_ref()
                        .is_none_or(|target| target.root == root))
        })
        .collect();
    relevant.sort_by(|a, b| a.id.cmp(&b.id));
    let state: Vec<_> = relevant
        .iter()
        .map(|info| {
            (
                &info.id,
                &info.title,
                &info.status,
                &info.filename,
                &info.planned_destination,
                &info.catalog_target,
                info.auto_import,
                &info.import_intent,
                &info.edition,
            )
        })
        .collect();
    result.revision = crate::library_reference_impact::revision(&(1u8, state))?;
    result.destination_conflicts = destination_conflicts(root, remap, records)?;
    for info in records {
        let bound = info.catalog_target.as_ref().map(|t| t.root.as_str());
        // An approved import is immutable. Even if its destination is unrelated,
        // the future executor must wait for all imports in this library to settle.
        if let Some(intent) = &info.import_intent {
            if intent.root == root || bound == Some(root) {
                block(&mut result, info, "pendingImport");
            }
            continue;
        }
        if info.status == "saved" {
            continue;
        }
        if bound.is_some_and(|r| r != root) {
            continue;
        }
        if matches!(info.status.as_str(), "saving" | "recovery") {
            block(&mut result, info, "pendingImport");
            continue;
        }

        let planned = match remap.map(&info.planned_destination) {
            Ok(change) => change,
            Err(_) => {
                block(&mut result, info, "invalidDestination");
                continue;
            }
        };
        if bound.is_none() {
            if planned.is_some() {
                block(&mut result, info, "unknownLibrary");
            }
            continue;
        }
        let target = info.catalog_target.as_ref().expect("bound target");
        let catalog = match remap.map(&target.destination) {
            Ok(change) => change,
            Err(_) => {
                block(&mut result, info, "invalidDestination");
                continue;
            }
        };
        if planned.is_none() && catalog.is_none() {
            continue;
        }
        if planned
            .iter()
            .chain(catalog.iter())
            .any(|change| destination_parts(&change.after).is_err())
        {
            block(&mut result, info, "invalidDestination");
            continue;
        }
        if target.validate().is_err() {
            block(&mut result, info, "staleCatalogTarget");
            continue;
        }
        // Waiting and transferring jobs can start auto-import without another click.
        // Only quiescent, explicitly retryable states are candidates for future updates.
        if !matches!(
            info.status.as_str(),
            "paused" | "ready" | "failed" | "cancelled"
        ) {
            block(&mut result, info, "activeTransfer");
        }
        result.changes.push(DownloadChange {
            id: info.id.clone(),
            title: info.title.clone(),
            status: info.status.clone(),
            planned_destination: planned,
            catalog_destination: catalog,
        });
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog_lists::CatalogTarget;
    use std::fs;
    fn info(id: &str, status: &str, root: Option<&str>, destination: &str) -> DownloadInfo {
        serde_json::from_value(serde_json::json!({
            "id":id,"title":"Book","sourceName":"Fixture","format":"pdf","filename":"Book.pdf",
            "plannedDestination":destination,"status":status,"downloadedBytes":0,"totalBytes":null,
            "error":"","savedPath":"A/Book.pdf",
            "catalogTarget":root.map(|r| CatalogTarget{root:r.into(),title:"Book".into(),author:"Author".into(),destination:destination.into(),legacy_id:None})
        })).unwrap()
    }

    #[test]
    fn newly_converging_download_destinations_report_only_matching_filenames_and_root() {
        let map = PathRemap::new(vec![("A", "New")]);
        let mut records = vec![
            info("first", "ready", Some("/library"), "A/Future"),
            info("second", "paused", Some("/library"), "New/Future"),
            info("different", "ready", Some("/library"), "New/Future"),
            info("other", "ready", Some("/other"), "New/Future"),
            info("saved", "saved", Some("/library"), "New/Future"),
            info("unbound", "ready", None, "New/Future"),
        ];
        records[2].filename = "Other.pdf".into();
        let conflicts = destination_conflicts("/library", &map, &records).unwrap();
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].target, "New/Future/Book.pdf");
        assert_eq!(conflicts[0].download_ids, vec!["first", "second"]);
        records[1].planned_destination = "A/Future".into();
        records[1].catalog_target.as_mut().unwrap().destination = "A/Future".into();
        assert!(
            destination_conflicts("/library", &map, &records)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn download_revision_ignores_progress_and_history_but_tracks_pending_state() {
        let root = Path::new("/library");
        let map = PathRemap::new(vec![("A", "New")]);
        let mut records = vec![
            info("current", "ready", Some("/library"), "AB"),
            info("other", "ready", Some("/other"), "A"),
            info("saved", "saved", Some("/library"), "A"),
        ];
        let first = analyze(root, &map, &records).unwrap().revision;
        records[0].downloaded_bytes = 42;
        records[1].status = "paused".into();
        records[2].saved_path = "Changed.pdf".into();
        records.reverse();
        assert_eq!(first, analyze(root, &map, &records).unwrap().revision);
        records[2].status = "saving".into();
        assert_ne!(first, analyze(root, &map, &records).unwrap().revision);
    }

    #[test]
    fn maps_only_bound_current_destinations_and_preserves_history() {
        let root = std::env::temp_dir().join(format!(
            "acervo-download-impact-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let root = fs::canonicalize(root).unwrap();
        let r = root.to_str().unwrap();
        crate::catalog_lists::import_catalog_json(
            r.into(),
            r#"[{"titulo":"Book","autor":"Author","local_indicado":"A/Future"}]"#.into(),
        )
        .unwrap();
        let mut records = vec![
            info("ready", "ready", Some(r), "A/Future"),
            info("active", "downloading", Some(r), "A/Future"),
            info("other", "ready", Some("/other"), "A/Future"),
            info("saved", "saved", Some(r), "A/Future"),
            info("unbound", "ready", None, "A/Future"),
            info("unrelated", "ready", None, "AB/Future"),
            info("saving", "saving", None, "AB"),
            info("recovery", "recovery", Some(r), "AB"),
            info("stale", "ready", Some(r), "A/Missing"),
        ];
        let path = root.join("fixture.pdf");
        fs::write(&path, b"fixture").unwrap();
        records[7].import_intent = Some(super::super::ImportIntent {
            root: r.into(),
            destination: "AB".into(),
            link_catalog: false,
            fingerprint: crate::import_fingerprint::fingerprint(&path).unwrap(),
        });
        let before = serde_json::to_vec(&records).unwrap();
        let impact = analyze(&root, &PathRemap::new(vec![("A", "New")]), &records).unwrap();
        assert_eq!(impact.changes.len(), 2);
        assert_eq!(
            impact.changes[0]
                .planned_destination
                .as_ref()
                .unwrap()
                .after,
            "New/Future"
        );
        assert_eq!(
            impact.changes[0]
                .catalog_destination
                .as_ref()
                .unwrap()
                .after,
            "New/Future"
        );
        let blockers: Vec<_> = impact
            .blockers
            .iter()
            .map(|b| (b.id.as_str(), b.reason))
            .collect();
        assert_eq!(
            blockers,
            vec![
                ("active", "activeTransfer"),
                ("unbound", "unknownLibrary"),
                ("saving", "pendingImport"),
                ("recovery", "pendingImport"),
                ("stale", "staleCatalogTarget")
            ]
        );
        assert_eq!(serde_json::to_vec(&records).unwrap(), before);
        fs::remove_dir_all(root).unwrap();
    }
}
