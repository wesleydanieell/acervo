//! One scoped filesystem watcher, with bounded coalescing and no file writes.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Serialize;
use std::collections::BTreeSet;
use std::{
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::Emitter;

#[derive(Default)]
pub struct LibraryMonitor(Mutex<MonitorState>);
#[derive(Default)]
struct MonitorState {
    generation: u64,
    session: Option<Session>,
}
struct Session {
    _watcher: RecommendedWatcher,
    stop: Arc<AtomicBool>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
    }
}

fn relevant(root: &Path, event: &Event) -> bool {
    if event.need_rescan() {
        return true;
    }
    if matches!(event.kind, EventKind::Access(_)) {
        return false;
    }
    if event.paths.is_empty() {
        return true;
    }
    event.paths.iter().any(|path| {
        let Ok(relative) = path.strip_prefix(root) else {
            return false;
        };
        let parts: Vec<_> = relative.iter().map(|part| part.to_string_lossy()).collect();
        if parts.first().is_some_and(|part| part == "99 - Catálogo") {
            return parts.len() == 1
                || (parts.len() == 2
                    && matches!(
                        parts[1].as_ref(),
                        "Catalogo_Conhecimento.db"
                            | "Catalogo_Conhecimento.db-wal"
                            | "Acervo_Listas.json"
                            | "Acervo_Edicoes.json"
                            | "Acervo_Metadados.json"
                    ));
        }
        // Discovery excludes hidden directories, not hidden book filenames.
        // Keep their source subtree dirty even after the file no longer exists.
        !parts.iter().enumerate().any(|(i, part)| {
            let last = i + 1 == parts.len();
            part == "node_modules"
                || (part.starts_with('.') && !(last && crate::is_book(Path::new(part.as_ref()))))
        })
    })
}

const MAX_DIRTY: usize = 128;
#[derive(Default)]
struct Dirty {
    full: bool,
    catalog: bool,
    paths: BTreeSet<String>,
}
impl Dirty {
    fn full(&mut self) {
        self.full = true;
        self.paths.clear();
    }
    fn record(&mut self, root: &Path, event: notify::Result<Event>) {
        let Ok(event) = event else {
            self.full();
            return;
        };
        if !relevant(root, &event) || self.full {
            return;
        }
        if event.need_rescan() || event.paths.is_empty() {
            self.full();
            return;
        }
        for path in event.paths {
            let Ok(relative) = path.strip_prefix(root) else {
                self.full();
                return;
            };
            let Some(first) = relative.iter().next() else {
                self.full();
                return;
            };
            let Some(first) = first.to_str() else {
                self.full();
                return;
            };
            if first == "99 - Catálogo" {
                self.catalog = true;
                continue;
            }
            if !relevant(root, &Event::new(event.kind).add_path(path.clone())) {
                continue;
            }
            self.paths.insert(first.to_owned());
            if self.paths.len() > MAX_DIRTY {
                self.full();
                return;
            }
        }
    }
    fn take(&mut self) -> Option<Option<Vec<String>>> {
        if self.full {
            *self = Self::default();
            return Some(None);
        }
        if !self.catalog && self.paths.is_empty() {
            return None;
        }
        self.catalog = false;
        Some(Some(std::mem::take(&mut self.paths).into_iter().collect()))
    }
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChangeEvent {
    root: String,
    changes: Option<Vec<String>>,
}

fn create_session(root: PathBuf, app: tauri::AppHandle) -> Result<Session, String> {
    let dirty = Arc::new(Mutex::new(Dirty {
        full: true,
        ..Default::default()
    })); // close scan-before-watch gap
    let stop = Arc::new(AtomicBool::new(false));
    let callback_dirty = dirty.clone();
    let callback_root = root.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if let Ok(mut dirty) = callback_dirty.lock() {
            dirty.record(&callback_root, result);
        }
    })
    .map_err(|e| e.to_string())?;
    watcher
        .watch(&root, RecursiveMode::Recursive)
        .map_err(|e| format!("Não foi possível monitorar a biblioteca: {e}"))?;
    let stopped = stop.clone();
    std::thread::Builder::new()
        .name("acervo-library-monitor".into())
        .spawn(move || {
            while !stopped.load(Ordering::Acquire) {
                std::thread::sleep(Duration::from_millis(650));
                if stopped.load(Ordering::Acquire) {
                    break;
                }
                let batch = dirty.lock().ok().and_then(|mut value| value.take());
                if let Some(changes) = batch {
                    let _ = app.emit_to(
                        "main",
                        "acervo-library-fs-changed",
                        ChangeEvent {
                            root: root.to_string_lossy().into_owned(),
                            changes,
                        },
                    );
                }
            }
        })
        .map_err(|e| e.to_string())?;
    Ok(Session {
        _watcher: watcher,
        stop,
    })
}

#[tauri::command]
pub async fn set_library_monitor(
    app: tauri::AppHandle,
    state: tauri::State<'_, LibraryMonitor>,
    root: Option<String>,
    generation: u64,
) -> Result<(), String> {
    {
        let mut inner = state.0.lock().map_err(|_| "Monitor indisponível.")?;
        if generation <= inner.generation {
            return Ok(());
        }
        inner.generation = generation;
        inner.session.take();
    }
    let Some(root) = root else {
        return Ok(());
    };
    let session = tauri::async_runtime::spawn_blocking(move || {
        let path = PathBuf::from(root);
        crate::library_editions::canonical_root(&path)?;
        create_session(path, app)
    })
    .await
    .map_err(|e| e.to_string())??;
    let mut inner = state.0.lock().map_err(|_| "Monitor indisponível.")?;
    if inner.generation == generation {
        inner.session = Some(session);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ignores_access_hidden_and_service_temporary_files() {
        let root = PathBuf::from("/library");
        let event = |path: &str, kind| Event::new(kind).add_path(root.join(path));
        assert!(!relevant(
            &root,
            &event(
                "Book.epub",
                EventKind::Access(notify::event::AccessKind::Any)
            )
        ));
        assert!(!relevant(&root, &event(".hidden/a.pdf", EventKind::Any)));
        assert!(!relevant(
            &root,
            &event("99 - Catálogo/.temporary", EventKind::Any)
        ));
        assert!(!relevant(
            &root,
            &event("99 - Catálogo/Catalogo_Conhecimento.db-shm", EventKind::Any)
        ));
        assert!(relevant(
            &root,
            &event("99 - Catálogo/Acervo_Metadados.json", EventKind::Any)
        ));
        assert!(relevant(&root, &event("New/Book.epub", EventKind::Any)));
    }
    #[test]
    fn coalesces_dirty_subtrees_catalog_and_overflow() {
        let root = PathBuf::from("/library");
        let mut dirty = Dirty::default();
        let event = |a: &str, b: &str| {
            Ok(Event::new(EventKind::Any)
                .add_path(root.join(a))
                .add_path(root.join(b)))
        };
        dirty.record(&root, event("A/old.pdf", "B/new.pdf"));
        dirty.record(
            &root,
            event("A/another.pdf", "99 - Catálogo/Acervo_Listas.json"),
        );
        assert_eq!(dirty.take(), Some(Some(vec!["A".into(), "B".into()])));
        assert_eq!(dirty.take(), None);
        dirty.record(&root, event("A/.book.pdf", "B/book.pdf"));
        assert_eq!(dirty.take(), Some(Some(vec!["A".into(), "B".into()])));
        dirty.record(
            &root,
            Ok(Event::new(EventKind::Any).add_path(root.join(".book.pdf"))),
        );
        assert_eq!(dirty.take(), Some(Some(vec![".book.pdf".into()])));

        dirty.record(
            &root,
            event("99 - Catálogo/Acervo_Metadados.json", "99 - Catálogo/.tmp"),
        );
        assert_eq!(dirty.take(), Some(Some(vec![])));
        for i in 0..=MAX_DIRTY {
            dirty.record(
                &root,
                Ok(Event::new(EventKind::Any).add_path(root.join(format!("folder{i}")))),
            );
        }
        assert_eq!(dirty.take(), Some(None));
    }

    #[test]
    fn native_monitor_observes_external_create_and_rename() {
        let dir = std::env::temp_dir().join(format!(
            "acervo-watch-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&dir).unwrap();
        let dir = std::fs::canonicalize(dir).unwrap();
        let (tx, rx) = std::sync::mpsc::channel();
        let root = dir.clone();
        let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
            if let Ok(event) = result {
                if relevant(&root, &event) {
                    let _ = tx.send(event);
                }
            }
        })
        .unwrap();
        watcher.watch(&dir, RecursiveMode::Recursive).unwrap();
        std::fs::write(dir.join("Book.pdf"), b"test").unwrap();
        let until = std::time::Instant::now() + Duration::from_secs(5);
        let wait_for = |path: &Path| {
            while let Ok(event) =
                rx.recv_timeout(until.saturating_duration_since(std::time::Instant::now()))
            {
                if event.paths.iter().any(|p| p == path) {
                    return true;
                }
            }
            false
        };
        assert!(wait_for(&dir.join("Book.pdf")));
        std::fs::rename(dir.join("Book.pdf"), dir.join("Moved.pdf")).unwrap();
        assert!(wait_for(&dir.join("Moved.pdf")));
        drop(watcher);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
