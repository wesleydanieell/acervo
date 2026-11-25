//! Local, read-only palette integration. Events are invalidation hints;
//! the frontend's periodic fallback covers missing/replaced watch directories.
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use tauri::Emitter;

const NAME: &str = "noctalia-colors.json";
const MAX_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize, Serialize)]
pub struct Palette {
    mode: Option<String>,
    primary: String,
    on_primary: String,
    surface: String,
    on_surface: String,
    surface_container: String,
    outline_variant: String,
}

fn directory() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))?;
    Some(base.join("acervo"))
}
fn color(value: &str) -> bool {
    value.starts_with('#')
        && matches!(value.len(), 7 | 9)
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn read(path: &Path) -> Option<Palette> {
    // Config-managed links are legitimate (for example, a generated theme
    // symlink). Resolve the target, then open only a regular file.
    let target = path.canonicalize().ok()?;
    let file = crate::library_editions::open_regular(&target).ok()?;
    let mut content = Vec::new();
    file.take(MAX_BYTES + 1).read_to_end(&mut content).ok()?;
    if content.len() as u64 > MAX_BYTES {
        return None;
    }
    let mut palette: Palette = serde_json::from_slice(&content).ok()?;
    for field in [
        &mut palette.primary,
        &mut palette.on_primary,
        &mut palette.surface,
        &mut palette.on_surface,
        &mut palette.surface_container,
        &mut palette.outline_variant,
    ] {
        if !color(field) {
            return None;
        }
        field.make_ascii_lowercase();
    }
    if !matches!(palette.mode.as_deref(), Some("light" | "dark")) {
        palette.mode = None;
    }
    Some(palette)
}

#[tauri::command]
pub async fn read_noctalia_colors() -> Option<Palette> {
    tauri::async_runtime::spawn_blocking(|| read(&directory()?.join(NAME)))
        .await
        .ok()
        .flatten()
}
fn relevant(path: &Path, event: &Event) -> bool {
    event.need_rescan()
        || (!matches!(event.kind, EventKind::Access(_))
            && (event.paths.is_empty()
                || event
                    .paths
                    .iter()
                    .any(|item| item == path || item == path.parent().unwrap_or(path))))
}

fn watch(
    dir: &Path,
    mut signal: impl FnMut() + Send + 'static,
) -> notify::Result<RecommendedWatcher> {
    // Watching the directory (not the inode) handles atomic replacement.
    let dir = dir.canonicalize()?;
    let path = dir.join(NAME);
    let mut last: Option<Instant> = None;
    let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
        if event.as_ref().is_ok_and(|event| !relevant(&path, event)) {
            return;
        }
        let now = Instant::now();
        if last.is_some_and(|last| now.duration_since(last) < Duration::from_millis(50)) {
            return;
        }
        last = Some(now);
        signal();
    })?;
    watcher.watch(&dir, RecursiveMode::NonRecursive)?;
    Ok(watcher)
}

pub struct PaletteWatcher {
    _watcher: Mutex<RecommendedWatcher>,
}
impl PaletteWatcher {
    pub fn start(app: &tauri::AppHandle) -> Option<Self> {
        let app = app.clone();
        let watcher = watch(&directory()?, move || {
            let _ = app.emit_to("main", "acervo-noctalia-changed", ());
        })
        .ok()?;
        Some(Self {
            _watcher: Mutex::new(watcher),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, sync::mpsc, time::SystemTime};
    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let dir = std::env::temp_dir().join(format!(
                "acervo-palette-{}-{}",
                std::process::id(),
                SystemTime::now()
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_nanos()
            ));
            fs::create_dir(&dir).unwrap();
            Self(dir)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    const SAMPLE: &str = r##"{"mode":"dark","primary":"#A0EFC4","on_primary":"#103522","surface":"#111b16","on_surface":"#eaf3ed","surface_container":"#233329","outline_variant":"#4b6756","extra":"ignored"}"##;
    #[test]
    fn bounded_palette_read_normalizes_and_validates_fields() {
        let dir = Fixture::new();
        let path = dir.0.join(NAME);
        fs::write(&path, SAMPLE).unwrap();
        let data = read(&path).unwrap();
        assert_eq!(data.primary, "#a0efc4");
        assert!(
            !serde_json::to_value(data)
                .unwrap()
                .as_object()
                .unwrap()
                .contains_key("extra")
        );
        fs::write(&path, SAMPLE.replace("#A0EFC4", "bad")).unwrap();
        assert!(read(&path).is_none());
        fs::write(&path, vec![b' '; MAX_BYTES as usize + 1]).unwrap();
        assert!(read(&path).is_none());
        fs::write(&path, b"{partial").unwrap();
        assert!(read(&path).is_none());
        fs::remove_file(&path).unwrap();
        assert!(read(&path).is_none());
    }
    #[cfg(unix)]
    #[test]
    fn palette_symlink_follows_regular_target_only() {
        let dir = Fixture::new();
        let target = dir.0.join("generated.json");
        let path = dir.0.join(NAME);
        fs::write(&target, SAMPLE).unwrap();
        std::os::unix::fs::symlink(&target, &path).unwrap();
        assert!(read(&path).is_some());
        fs::remove_file(&target).unwrap();
        fs::create_dir(&target).unwrap();
        assert!(read(&path).is_none());
    }

    #[test]
    fn watcher_filters_unrelated_and_access_events() {
        let path = PathBuf::from("/test").join(NAME);
        assert!(!relevant(
            &path,
            &Event::new(EventKind::Any).add_path(PathBuf::from("/test/other"))
        ));
        assert!(!relevant(
            &path,
            &Event::new(EventKind::Access(notify::event::AccessKind::Any)).add_path(path.clone())
        ));
        assert!(relevant(
            &path,
            &Event::new(EventKind::Any).add_path(path.clone())
        ));
    }
    #[test]
    fn native_watcher_observes_atomic_palette_install() {
        let dir = Fixture::new();
        let (tx, rx) = mpsc::channel();
        let _watcher = watch(&dir.0, move || {
            let _ = tx.send(());
        })
        .unwrap();
        let temporary = dir.0.join("colors.tmp");
        fs::write(&temporary, SAMPLE).unwrap();
        fs::rename(&temporary, dir.0.join(NAME)).unwrap();
        rx.recv_timeout(Duration::from_secs(5))
            .expect("palette replacement event");
        assert!(read(&dir.0.join(NAME)).is_some());
    }
}
