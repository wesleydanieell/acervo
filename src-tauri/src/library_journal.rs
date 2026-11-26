//! Durable, append-only structural journal. No filesystem executor is exposed.
use crate::library_reference_impact::revision;
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

const DIRECTORY: &str = ".acervo-structural-journal";
const MAX_STEPS: usize = 128;
const MAX_RECORDS: usize = MAX_STEPS * 4 + 4;
const MAX_RECORD_BYTES: u64 = 256 * 1024;
static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Step {
    CreateFolder {
        path: String,
    },
    MoveFolder {
        source: String,
        target: String,
    },
    RewriteReferences {
        resource: Resource,
        before: String,
        after: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) enum Resource {
    LegacyCatalog,
    Lists,
    ManualMetadata,
    Editions,
    Downloads,
    Rules,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct Intent {
    pub(crate) root: String,
    pub(crate) plan_revision: String,
    pub(crate) steps: Vec<Step>,
}
fn digest_valid(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
impl Intent {
    fn validate(&self, root: &Path) -> Result<(), String> {
        if root.to_str() != Some(&self.root)
            || !digest_valid(&self.plan_revision)
            || self.steps.is_empty()
            || self.steps.len() > MAX_STEPS
        {
            return Err("Intenção estrutural inválida.".into());
        }
        for step in &self.steps {
            match step {
                Step::CreateFolder { path } => {
                    crate::library_restructure::relative(path)?;
                }
                Step::MoveFolder { source, target } => {
                    let source = crate::library_restructure::relative(source)?;
                    let target = crate::library_restructure::relative(target)?;
                    if source.starts_with(&target) || target.starts_with(&source) {
                        return Err("Movimento estrutural sobreposto.".into());
                    }
                }
                Step::RewriteReferences { before, after, .. } => {
                    if !digest_valid(before) || !digest_valid(after) || before == after {
                        return Err("Revisões de referência inválidas.".into());
                    }
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", deny_unknown_fields)]
pub(crate) enum Event {
    Prepared { intent: Intent },
    BeginStep { index: usize },
    StepApplied { index: usize },
    StepNotApplied { index: usize },
    BeginRollback,
    BeginCompensation { index: usize },
    StepCompensated { index: usize },
    CompensationNotApplied { index: usize },
    Committed,
    RolledBack,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(crate) enum State {
    Applying {
        completed: usize,
        in_flight: Option<usize>,
    },
    RollingBack {
        remaining: usize,
        in_flight: Option<usize>,
    },
    Committed,
    RolledBack,
}
impl State {
    fn advance(&self, event: &Event, count: usize) -> Result<Self, String> {
        use Event::*;
        use State::*;
        let next = match (self, event) {
            (
                Applying {
                    completed,
                    in_flight: None,
                },
                BeginStep { index },
            ) if index == completed && *index < count => Applying {
                completed: *completed,
                in_flight: Some(*index),
            },
            (
                Applying {
                    completed,
                    in_flight: Some(active),
                },
                StepApplied { index },
            ) if active == index => Applying {
                completed: completed + 1,
                in_flight: None,
            },
            // Only a future recovery executor with verified evidence may emit this.
            (
                Applying {
                    completed,
                    in_flight: Some(active),
                },
                StepNotApplied { index },
            ) if active == index => Applying {
                completed: *completed,
                in_flight: None,
            },
            (
                Applying {
                    completed,
                    in_flight: None,
                },
                BeginRollback,
            ) => RollingBack {
                remaining: *completed,
                in_flight: None,
            },
            (
                RollingBack {
                    remaining,
                    in_flight: None,
                },
                BeginCompensation { index },
            ) if remaining.checked_sub(1) == Some(*index) => RollingBack {
                remaining: *remaining,
                in_flight: Some(*index),
            },
            (
                RollingBack {
                    remaining,
                    in_flight: Some(active),
                },
                StepCompensated { index },
            ) if active == index => RollingBack {
                remaining: remaining - 1,
                in_flight: None,
            },
            (
                RollingBack {
                    remaining,
                    in_flight: Some(active),
                },
                CompensationNotApplied { index },
            ) if active == index => RollingBack {
                remaining: *remaining,
                in_flight: None,
            },
            (
                Applying {
                    completed,
                    in_flight: None,
                },
                Event::Committed,
            ) if *completed == count => State::Committed,
            (
                RollingBack {
                    remaining: 0,
                    in_flight: None,
                },
                Event::RolledBack,
            ) => State::RolledBack,
            _ => return Err("Transição de journal inválida ou ambígua.".into()),
        };
        Ok(next)
    }
    pub(crate) fn pending(&self) -> bool {
        !matches!(self, Self::Committed | Self::RolledBack)
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Record {
    version: u8,
    sequence: usize,
    previous: Option<String>,
    event: Event,
    checksum: String,
}
impl Record {
    fn new(sequence: usize, previous: Option<String>, event: Event) -> Result<Self, String> {
        let checksum = revision(&(1u8, sequence, &previous, &event))?;
        Ok(Self {
            version: 1,
            sequence,
            previous,
            event,
            checksum,
        })
    }
    fn validate(&self, sequence: usize, previous: Option<&str>) -> Result<(), String> {
        if self.version != 1
            || self.sequence != sequence
            || self.previous.as_deref() != previous
            || self.checksum
                != revision(&(self.version, self.sequence, &self.previous, &self.event))?
        {
            return Err("Journal corrompido: sequência ou checksum divergente.".into());
        }
        Ok(())
    }
}

#[derive(Debug)]
struct Loaded {
    intent: Intent,
    state: State,
    next: usize,
    checksum: String,
}
fn directory(root: &Path) -> Result<Option<PathBuf>, String> {
    let Some(service) = crate::library_editions::service(root)? else {
        return Ok(None);
    };
    let path = service.join(DIRECTORY);
    match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(Some(path)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        _ => Err("Diretório de journal inválido ou simbólico.".into()),
    }
}
fn filename(sequence: usize) -> String {
    format!("{sequence:06}.json")
}

fn load(root: &Path) -> Result<Option<Loaded>, String> {
    let Some(dir) = directory(root)? else {
        return Ok(None);
    };
    let mut paths = Vec::new();
    for (count, entry) in fs::read_dir(&dir).map_err(|e| e.to_string())?.enumerate() {
        if count >= MAX_RECORDS * 2 {
            return Err("Journal excede limite de entradas.".into());
        }
        let entry = entry.map_err(|e| e.to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "Nome de journal inválido.")?;
        if name.starts_with(".pending-") && name.ends_with(".tmp") {
            if !entry.file_type().map_err(|e| e.to_string())?.is_file() {
                return Err("Temporário de journal inválido.".into());
            }
            continue;
        }
        if !entry.file_type().map_err(|e| e.to_string())?.is_file() || paths.len() >= MAX_RECORDS {
            return Err("Journal inválido ou excede limite.".into());
        }
        paths.push(name);
    }
    paths.sort();
    let mut loaded: Option<Loaded> = None;
    for (index, name) in paths.iter().enumerate() {
        if *name != filename(index) {
            return Err("Journal com lacuna ou arquivo desconhecido.".into());
        }
        let file = crate::library_editions::open_regular(&dir.join(name))?;
        let mut bytes = Vec::new();
        file.take(MAX_RECORD_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 > MAX_RECORD_BYTES {
            return Err("Registro de journal excede limite.".into());
        }
        let record: Record =
            serde_json::from_slice(&bytes).map_err(|e| format!("Journal inválido: {e}"))?;
        record.validate(index, loaded.as_ref().map(|l| l.checksum.as_str()))?;
        if let Some(current) = &mut loaded {
            current.state = current
                .state
                .advance(&record.event, current.intent.steps.len())?;
            current.next = index + 1;
            current.checksum = record.checksum;
        } else {
            let Event::Prepared { intent } = record.event else {
                return Err("Journal sem intenção.".into());
            };
            intent.validate(root)?;
            loaded = Some(Loaded {
                intent,
                state: State::Applying {
                    completed: 0,
                    in_flight: None,
                },
                next: 1,
                checksum: record.checksum,
            });
        }
    }
    loaded
        .ok_or_else(|| "Journal incompleto: intenção não persistida.".into())
        .map(Some)
}

pub(crate) fn inspect(root: &Path) -> Result<Option<State>, String> {
    Ok(load(root)?.map(|value| value.state))
}

#[tauri::command]
pub async fn inspect_library_journal(root: String) -> Result<Option<State>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
        inspect(&root)
    })
    .await
    .map_err(|e| e.to_string())?
}

// Writer is deliberately internal and not connected to IPC or physical operations.
#[allow(dead_code)]
fn sync_directory(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        File::open(path)
            .and_then(|f| f.sync_all())
            .map_err(|e| e.to_string())
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Err("Persistência durável de diretório não validada nesta plataforma.".into())
    }
}

#[allow(dead_code)]
fn write_record(dir: &Path, record: &Record) -> Result<(), String> {
    let bytes = serde_json::to_vec(record).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_RECORD_BYTES {
        return Err("Registro de journal excede limite.".into());
    }
    let target = dir.join(filename(record.sequence));
    let (temp, mut file) = loop {
        let temp = dir.join(format!(
            ".pending-{}-{}.tmp",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        match options.open(&temp) {
            Ok(file) => break (temp, file),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(e.to_string()),
        }
    };
    let result = (|| {
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        drop(file);
        crate::rename_without_replace(&temp, &target).map_err(|e| e.to_string())?;
        sync_directory(dir)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temp);
    }
    result
}

#[allow(dead_code)]
pub(crate) fn prepare(
    reservation: &crate::library_mutations::Reservation,
    intent: Intent,
) -> Result<(), String> {
    let root = reservation.root();
    intent.validate(root)?;
    // Fail before creating a directory on platforms without the durability contract.
    sync_directory(root)?;
    let record = Record::new(0, None, Event::Prepared { intent })?;
    let service = match crate::library_editions::service(root)? {
        Some(path) => path,
        None => {
            let path = root.join("99 - Catálogo");
            fs::create_dir(&path).map_err(|e| e.to_string())?;
            sync_directory(root)?;
            path
        }
    };
    let dir = service.join(DIRECTORY);
    fs::create_dir(&dir).map_err(|e| e.to_string())?;
    sync_directory(&service)?;
    write_record(&dir, &record)
}

#[allow(dead_code)]
pub(crate) fn append(
    reservation: &crate::library_mutations::Reservation,
    event: Event,
) -> Result<State, String> {
    let root = reservation.root();
    let current = load(root)?.ok_or("Journal ausente.")?;
    let next = current.state.advance(&event, current.intent.steps.len())?;
    if current.next >= MAX_RECORDS {
        return Err("Journal excede limite.".into());
    }
    let record = Record::new(current.next, Some(current.checksum), event)?;
    write_record(&directory(root)?.ok_or("Journal ausente.")?, &record)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "acervo-journal-{}-{}",
                std::process::id(),
                NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
            ));
            fs::create_dir(&path).unwrap();
            Self(fs::canonicalize(path).unwrap())
        }
        fn intent(&self) -> Intent {
            Intent {
                root: self.0.to_str().unwrap().into(),
                plan_revision: "a".repeat(64),
                steps: vec![
                    Step::CreateFolder { path: "New".into() },
                    Step::MoveFolder {
                        source: "A".into(),
                        target: "New/A".into(),
                    },
                ],
            }
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[cfg(unix)]
    #[test]
    fn durable_checkpoints_reopen_and_reject_ambiguous_transitions() {
        let f = Fixture::new();
        let guard = crate::library_mutations::Reservation::acquire(&f.0).unwrap();
        assert!(inspect(&f.0).unwrap().is_none());
        prepare(&guard, f.intent()).unwrap();
        assert!(prepare(&guard, f.intent()).is_err());
        assert!(append(&guard, Event::Committed).is_err());
        append(&guard, Event::BeginStep { index: 0 }).unwrap();
        assert_eq!(
            inspect(&f.0).unwrap(),
            Some(State::Applying {
                completed: 0,
                in_flight: Some(0)
            })
        );
        assert!(append(&guard, Event::BeginRollback).is_err());
        assert!(append(&guard, Event::StepApplied { index: 1 }).is_err());
        append(&guard, Event::StepApplied { index: 0 }).unwrap();
        append(&guard, Event::BeginRollback).unwrap();
        append(&guard, Event::BeginCompensation { index: 0 }).unwrap();
        assert!(append(&guard, Event::RolledBack).is_err());
        append(&guard, Event::StepCompensated { index: 0 }).unwrap();
        assert_eq!(
            append(&guard, Event::RolledBack).unwrap(),
            State::RolledBack
        );
        assert!(!inspect(&f.0).unwrap().unwrap().pending());
        assert!(append(&guard, Event::BeginStep { index: 0 }).is_err());
        assert!(!f.0.join("New").exists());
    }

    #[cfg(unix)]
    #[test]
    fn pending_journal_blocks_mutation_after_reopen_until_reconciled() {
        let f = Fixture::new();
        let guard = crate::library_mutations::Reservation::acquire(&f.0).unwrap();
        prepare(&guard, f.intent()).unwrap();
        append(&guard, Event::BeginStep { index: 0 }).unwrap();
        assert!(append(&guard, Event::BeginRollback).is_err());
        // Simulated evidence: no physical operations were performed in this fixture.
        append(&guard, Event::StepNotApplied { index: 0 }).unwrap();
        append(&guard, Event::BeginRollback).unwrap();
        drop(guard);
        assert!(crate::library_mutations::Reservation::acquire(&f.0).is_err());
        assert!(inspect(&f.0).unwrap().unwrap().pending());
        assert!(!f.0.join("New").exists());
    }

    #[cfg(unix)]
    #[test]
    fn corruption_gaps_and_incomplete_intent_fail_closed() {
        let f = Fixture::new();
        let guard = crate::library_mutations::Reservation::acquire(&f.0).unwrap();
        prepare(&guard, f.intent()).unwrap();
        append(&guard, Event::BeginStep { index: 0 }).unwrap();
        let dir = directory(&f.0).unwrap().unwrap();
        fs::write(dir.join(".pending-crashed.tmp"), b"partial").unwrap();
        assert!(inspect(&f.0).unwrap().unwrap().pending());
        let path = dir.join(filename(1));
        let original = fs::read(&path).unwrap();
        let mut record: Record = serde_json::from_slice(&original).unwrap();
        record.checksum = "0".repeat(64);
        fs::write(&path, serde_json::to_vec(&record).unwrap()).unwrap();
        assert!(inspect(&f.0).is_err());
        fs::write(&path, original).unwrap();
        fs::remove_file(dir.join(filename(0))).unwrap();
        assert!(inspect(&f.0).is_err());
        fs::remove_file(path).unwrap();
        assert!(inspect(&f.0).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn completed_journal_is_readable_and_does_not_block_existing_commands() {
        let f = Fixture::new();
        let guard = crate::library_mutations::Reservation::acquire(&f.0).unwrap();
        prepare(&guard, f.intent()).unwrap();
        for index in 0..2 {
            append(&guard, Event::BeginStep { index }).unwrap();
            append(&guard, Event::StepApplied { index }).unwrap();
        }
        assert_eq!(append(&guard, Event::Committed).unwrap(), State::Committed);
        drop(guard);
        assert!(crate::library_mutations::Reservation::acquire(&f.0).is_ok());
    }

    #[test]
    fn rejects_protected_paths_and_wrong_root() {
        let f = Fixture::new();
        let mut intent = f.intent();
        intent.steps.push(Step::CreateFolder {
            path: "99 - Catálogo/Hidden".into(),
        });
        assert!(intent.validate(&f.0).is_err());
        let mut intent = f.intent();
        intent.root = "/other".into();
        assert!(intent.validate(&f.0).is_err());
    }
}
