//! Read-only evidence for an interrupted step, under exclusive recovery access.
use super::{FolderIdentity, Image, Resource, State, Step, images, load};
use crate::library_mutations::RecoveryReservation;
use serde::Serialize;
use std::{fs, path::Path};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Evidence {
    Before,
    After,
    Conflict,
    Unsupported,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct RecoveryInspection {
    state: State,
    step_index: Option<usize>,
    evidence: Option<Evidence>,
}

#[derive(Debug, PartialEq, Eq)]
enum Location {
    Missing,
    Directory(FolderIdentity),
    Invalid,
}

fn locate(root: &Path, relative: &str) -> Result<Location, String> {
    let path = crate::library_restructure::relative(relative)?;
    let mut current = root.to_path_buf();
    let mut parts = path.components().peekable();
    while let Some(part) = parts.next() {
        current.push(part);
        match fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_dir() => {}
            Ok(_) => return Ok(Location::Invalid),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // Missing ancestors mean that the expected parent is no longer present.
                return Ok(if parts.peek().is_none() {
                    Location::Missing
                } else {
                    Location::Invalid
                });
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(Location::Directory(FolderIdentity::capture(
        root, relative,
    )?))
}

fn classify(root: &Path, step: &Step) -> Result<Evidence, String> {
    use Evidence::*;
    match step {
        Step::MoveFolder {
            source,
            target,
            identity,
        } => {
            // Without birth time, inode reuse cannot be distinguished reliably.
            if identity.created_ns.is_none() {
                return Ok(Unsupported);
            }
            match (locate(root, source)?, locate(root, target)?) {
                (Location::Directory(actual), Location::Missing) if &actual == identity => {
                    Ok(Before)
                }
                (Location::Missing, Location::Directory(actual)) if &actual == identity => {
                    Ok(After)
                }
                _ => Ok(Conflict),
            }
        }
        Step::CreateFolder { path } => match locate(root, path)? {
            Location::Missing => Ok(Before),
            // An existing directory may have been created by somebody else.
            Location::Directory(_) | Location::Invalid => Ok(Conflict),
        },
        Step::RewriteReferences {
            resource,
            before,
            after,
        } => {
            if !matches!(
                resource,
                Resource::Lists | Resource::ManualMetadata | Resource::Editions
            ) {
                return Ok(Unsupported);
            }
            let actual = images::capture_json(root, *resource)?;
            let image = Image::of(actual.as_deref())?;
            Ok(if &image == before {
                Before
            } else if &image == after {
                After
            } else {
                Conflict
            })
        }
    }
}

pub(crate) fn inspect(reservation: &RecoveryReservation) -> Result<RecoveryInspection, String> {
    let root = reservation.root();
    let journal = load(root)?.ok_or("Journal ausente.")?;
    let step_index = match &journal.state {
        State::Applying { in_flight, .. } | State::RollingBack { in_flight, .. } => *in_flight,
        _ => return Err("Journal não está pendente.".into()),
    };
    let evidence = step_index
        .map(|i| classify(root, &journal.intent.steps[i]))
        .transpose()?;
    Ok(RecoveryInspection {
        state: journal.state,
        step_index,
        evidence,
    })
}

/// Diagnostics only. No checkpoint, mutation or compensation is authorized.
#[tauri::command]
pub async fn inspect_library_recovery(root: String) -> Result<RecoveryInspection, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let reservation = RecoveryReservation::acquire(Path::new(&root))?;
        inspect(&reservation)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::super::{Event, Images, append, prepare, tests::Fixture};
    use super::*;
    use crate::library_mutations::Reservation;

    #[cfg(unix)]
    #[test]
    fn interrupted_move_evidence_is_repeatable_and_never_writes() {
        let f = Fixture::new();
        let root = &f.0;
        let guard = Reservation::acquire(root).unwrap();
        let mut intent = f.intent();
        intent.steps.remove(0);
        fs::create_dir(root.join("New")).unwrap();
        prepare(&guard, intent, &Images::default()).unwrap();
        append(&guard, Event::BeginStep { index: 0 }).unwrap();
        drop(guard);
        let recovery = RecoveryReservation::acquire(root).unwrap();
        assert!(Reservation::acquire(root).is_err());
        assert!(RecoveryReservation::acquire(root).is_err());
        assert!(Reservation::acquire(&root.join("A")).is_err());
        let first = inspect(&recovery).unwrap().evidence;
        if first == Some(Evidence::Unsupported) {
            return;
        }
        assert_eq!(first, Some(Evidence::Before));
        fs::rename(root.join("A"), root.join("New/A")).unwrap();
        for _ in 0..2 {
            assert_eq!(inspect(&recovery).unwrap().evidence, Some(Evidence::After));
        }
        fs::create_dir(root.join("A")).unwrap();
        assert_eq!(
            inspect(&recovery).unwrap().evidence,
            Some(Evidence::Conflict)
        );
        assert_eq!(
            super::super::inspect(root).unwrap(),
            Some(State::Applying {
                completed: 0,
                in_flight: Some(0)
            })
        );
        assert!(root.join("New/A").is_dir());
        drop(recovery);
        assert!(Reservation::acquire(root).is_err());
        assert!(RecoveryReservation::acquire(root).is_ok());
    }

    #[cfg(unix)]
    #[test]
    fn interrupted_compensation_reports_original_image_without_changing_checkpoints() {
        let f = Fixture::new();
        let root = &f.0;
        let guard = Reservation::acquire(root).unwrap();
        let mut intent = f.intent();
        let mut images = Images::default();
        let before = images.insert(None).unwrap();
        let after = images.insert(Some(b"new".to_vec())).unwrap();
        intent.steps = vec![Step::RewriteReferences {
            resource: Resource::Lists,
            before,
            after,
        }];
        prepare(&guard, intent, &images).unwrap();
        append(&guard, Event::BeginStep { index: 0 }).unwrap();
        let path = root.join("99 - Catálogo/Acervo_Listas.json");
        fs::write(&path, b"new").unwrap();
        append(&guard, Event::StepApplied { index: 0 }).unwrap();
        append(&guard, Event::BeginRollback).unwrap();
        append(&guard, Event::BeginCompensation { index: 0 }).unwrap();
        drop(guard);
        let recovery = RecoveryReservation::acquire(root).unwrap();
        assert_eq!(inspect(&recovery).unwrap().evidence, Some(Evidence::After));
        fs::remove_file(path).unwrap();
        let result = inspect(&recovery).unwrap();
        assert_eq!(result.evidence, Some(Evidence::Before));
        assert_eq!(
            result.state,
            State::RollingBack {
                remaining: 1,
                in_flight: Some(0)
            }
        );
    }

    #[cfg(unix)]
    #[test]
    fn json_images_and_created_folder_are_classified_conservatively() {
        let f = Fixture::new();
        let root = &f.0;
        let before = Image::Missing;
        let after = Image::of(Some(b"new")).unwrap();
        let step = Step::RewriteReferences {
            resource: Resource::Lists,
            before,
            after,
        };
        assert_eq!(classify(root, &step).unwrap(), Evidence::Before);
        fs::create_dir(root.join("99 - Catálogo")).unwrap();
        let file = root.join("99 - Catálogo/Acervo_Listas.json");
        fs::write(&file, b"new").unwrap();
        assert_eq!(classify(root, &step).unwrap(), Evidence::After);
        fs::write(&file, b"changed").unwrap();
        assert_eq!(classify(root, &step).unwrap(), Evidence::Conflict);
        let create = Step::CreateFolder { path: "New".into() };
        assert_eq!(classify(root, &create).unwrap(), Evidence::Before);
        fs::create_dir(root.join("New")).unwrap();
        assert_eq!(classify(root, &create).unwrap(), Evidence::Conflict);
        let unsupported = Step::RewriteReferences {
            resource: Resource::LegacyCatalog,
            before: Image::Missing,
            after: Image::of(Some(b"db")).unwrap(),
        };
        assert_eq!(classify(root, &unsupported).unwrap(), Evidence::Unsupported);
        assert!(RecoveryReservation::acquire(root).is_err());
    }
}
