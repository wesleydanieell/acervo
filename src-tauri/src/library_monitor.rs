//! One scoped filesystem watcher, with bounded coalescing and no file writes.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
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
        !parts
            .iter()
            .any(|part| part.starts_with('.') || part == "node_modules")
    })
}

fn create_session(root: PathBuf, app: tauri::AppHandle) -> Result<Session, String> {
    let dirty = Arc::new(AtomicBool::new(true)); // close the scan-before-watch gap
    let stop = Arc::new(AtomicBool::new(false));
    let callback_dirty = dirty.clone();
    let callback_root = root.clone();
    let mut watcher = notify::recommended_watcher(move |result: notify::Result<Event>| {
        if result
            .as_ref()
            .map_or(true, |event| relevant(&callback_root, event))
        {
            callback_dirty.store(true, Ordering::Release);
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
                if dirty.swap(false, Ordering::AcqRel) {
                    let _ = app.emit_to(
                        "main",
                        "acervo-library-fs-changed",
                        root.to_string_lossy().as_ref(),
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
