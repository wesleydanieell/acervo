//! Session-local download queue. Nothing enters the library before explicit approval.
use super::source_search::{safe_url, secure_https_client};
use serde::Serialize;
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Manager;

const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ACTIVE: usize = 3;
const MAX_HISTORY: usize = 60;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadInfo {
    pub id: String,
    pub title: String,
    pub source_name: String,
    pub format: String,
    pub filename: String,
    pub planned_destination: String,
    pub status: String,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub error: String,
    pub saved_path: String,
}

#[derive(Clone)]
struct DownloadRecord {
    info: DownloadInfo,
    url: String,
    cancel: Arc<AtomicBool>,
}

#[derive(Clone, Default)]
pub struct DownloadManager(Arc<Mutex<Vec<DownloadRecord>>>);

impl DownloadManager {
    fn snapshot(&self) -> Result<Vec<DownloadInfo>, String> {
        self.0
            .lock()
            .map(|items| items.iter().map(|item| item.info.clone()).rev().collect())
            .map_err(|_| "Fila de downloads indisponível.".into())
    }

    fn change(&self, id: &str, update: impl FnOnce(&mut DownloadRecord)) {
        if let Ok(mut items) = self.0.lock() {
            if let Some(item) = items.iter_mut().find(|item| item.info.id == id) {
                update(item);
            }
        }
    }
}

fn allowed_format(format: &str) -> Option<&'static str> {
    match format.to_ascii_lowercase().as_str() {
        "pdf" => Some("pdf"),
        "epub" => Some("epub"),
        "mobi" => Some("mobi"),
        "azw3" => Some("azw3"),
        "djvu" => Some("djvu"),
        _ => None,
    }
}

fn safe_filename(title: &str, format: &str) -> Result<String, String> {
    let extension = allowed_format(format).ok_or("Formato desconhecido para download.")?;
    let stem: String = title
        .trim()
        .chars()
        .map(|c| {
            if c == '/'
                || c == '\\'
                || c.is_control()
                || matches!(c, ':' | '*' | '?' | '"' | '<' | '>' | '|')
            {
                '_'
            } else {
                c
            }
        })
        .take(120)
        .collect();
    let stem = stem.trim().trim_matches('.').trim();
    if stem.is_empty() || stem == "." || stem == ".." {
        return Err("Título inválido para nome do arquivo.".into());
    }
    Ok(format!("{stem}.{extension}"))
}

fn staged_path(app: &tauri::AppHandle, id: &str) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_cache_dir()
        .map_err(|error| error.to_string())?
        .join("staged-downloads");
    if !id
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("Identificador de download inválido.".into());
    }
    Ok(dir.join(format!("{id}.part")))
}

fn set_progress(manager: &DownloadManager, id: &str, bytes: u64, total: Option<u64>) {
    manager.change(id, |item| {
        item.info.downloaded_bytes = bytes;
        item.info.total_bytes = total;
        item.info.status = "downloading".into();
    });
}

async fn fetch_to_stage(
    app: &tauri::AppHandle,
    manager: &DownloadManager,
    id: &str,
    url_text: &str,
    cancel: &AtomicBool,
) -> Result<(), String> {
    let url = url::Url::parse(url_text).map_err(|_| "URL inválida.")?;
    let client = secure_https_client(&url, Duration::from_secs(3600)).await?;
    let mut response = client
        .get(url)
        .send()
        .await
        .map_err(|error| format!("Falha de conexão: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Servidor retornou HTTP {}.", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_DOWNLOAD_BYTES)
    {
        return Err("O arquivo excede 512 MB.".into());
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.to_ascii_lowercase().starts_with("text/html"))
    {
        return Err("A fonte retornou uma página HTML, não um livro.".into());
    }

    let path = staged_path(app, id)?;
    fs::create_dir_all(path.parent().ok_or("Cache inválido.")?)
        .map_err(|error| error.to_string())?;
    let mut output = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|error| format!("Falha ao criar arquivo temporário: {error}"))?;
    let result = async {
        let total = response.content_length();
        let mut downloaded = 0u64;
        set_progress(manager, id, 0, total);
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|error| format!("Conexão interrompida: {error}"))?
        {
            if cancel.load(Ordering::Relaxed) {
                return Err("Download cancelado.".to_string());
            }
            downloaded = downloaded
                .checked_add(chunk.len() as u64)
                .ok_or("Arquivo muito grande.")?;
            if downloaded > MAX_DOWNLOAD_BYTES {
                return Err("O download excedeu 512 MB.".into());
            }
            output
                .write_all(&chunk)
                .map_err(|error| format!("Erro de gravação: {error}"))?;
            set_progress(manager, id, downloaded, total);
        }
        if cancel.load(Ordering::Relaxed) {
            return Err("Download cancelado.".into());
        }
        if downloaded == 0 {
            return Err("A fonte retornou um arquivo vazio.".into());
        }
        output.sync_all().map_err(|error| error.to_string())?;
        Ok(())
    }
    .await;
    drop(output);
    if result.is_err() {
        let _ = fs::remove_file(&path);
    }
    result
}

fn spawn_download(
    app: tauri::AppHandle,
    manager: DownloadManager,
    id: String,
    url: String,
    cancel: Arc<AtomicBool>,
) {
    tauri::async_runtime::spawn(async move {
        let outcome = fetch_to_stage(&app, &manager, &id, &url, &cancel).await;
        manager.change(&id, |item| match outcome {
            Ok(()) => {
                item.info.status = "ready".into();
                item.info.error.clear();
            }
            Err(error) => {
                item.info.status = if cancel.load(Ordering::Relaxed) {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                item.info.error = error;
            }
        });
    });
}

#[tauri::command]
pub fn list_downloads(
    manager: tauri::State<'_, DownloadManager>,
) -> Result<Vec<DownloadInfo>, String> {
    manager.snapshot()
}

#[tauri::command]
pub fn queue_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    url: String,
    title: String,
    source_name: String,
    format: String,
    planned_destination: String,
) -> Result<Vec<DownloadInfo>, String> {
    if !safe_url(&url) {
        return Err("O link precisa ser HTTPS público e seguro.".into());
    }
    if title.len() > 200 || source_name.len() > 120 || planned_destination.len() > 500 {
        return Err("Metadados de download fora do limite.".into());
    }
    let filename = safe_filename(&title, &format)?;
    let id = format!(
        "{}-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_millis(),
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    );
    let cancel = Arc::new(AtomicBool::new(false));
    let download = DownloadRecord {
        info: DownloadInfo {
            id: id.clone(),
            title,
            source_name,
            format: format.to_ascii_lowercase(),
            filename,
            planned_destination,
            status: "queued".into(),
            downloaded_bytes: 0,
            total_bytes: None,
            error: String::new(),
            saved_path: String::new(),
        },
        url: url.clone(),
        cancel: cancel.clone(),
    };
    let manager = manager.inner().clone();
    {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        if items.len() >= MAX_HISTORY {
            return Err("A fila está cheia. Limpe downloads antigos.".into());
        }
        if items
            .iter()
            .filter(|item| matches!(item.info.status.as_str(), "queued" | "downloading"))
            .count()
            >= MAX_ACTIVE
        {
            return Err("Há 3 downloads ativos. Aguarde ou cancele um deles.".into());
        }
        items.push(download);
    }
    spawn_download(app, manager.clone(), id, url, cancel);
    manager.snapshot()
}

#[tauri::command]
pub fn cancel_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    id: String,
) -> Result<Vec<DownloadInfo>, String> {
    let manager = manager.inner().clone();
    {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        let item = items
            .iter_mut()
            .find(|item| item.info.id == id)
            .ok_or("Download desconhecido.")?;
        match item.info.status.as_str() {
            "queued" | "downloading" => {
                item.cancel.store(true, Ordering::Relaxed);
            }
            "ready" | "failed" | "cancelled" => {
                item.info.status = "cancelled".into();
                item.info.error.clear();
                let _ = fs::remove_file(staged_path(&app, &id)?);
            }
            _ => return Err("Esse download não pode ser cancelado.".into()),
        }
    }
    manager.snapshot()
}

#[tauri::command]
pub fn retry_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    id: String,
) -> Result<Vec<DownloadInfo>, String> {
    let manager = manager.inner().clone();
    let (url, cancel) = {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        if items
            .iter()
            .filter(|item| matches!(item.info.status.as_str(), "queued" | "downloading"))
            .count()
            >= MAX_ACTIVE
        {
            return Err("Há 3 downloads ativos.".into());
        }
        let item = items
            .iter_mut()
            .find(|item| item.info.id == id)
            .ok_or("Download desconhecido.")?;
        if !matches!(item.info.status.as_str(), "failed" | "cancelled") {
            return Err("Apenas downloads cancelados ou com falha podem ser reiniciados.".into());
        }
        let _ = fs::remove_file(staged_path(&app, &id)?);
        item.info.status = "queued".into();
        item.info.error.clear();
        item.info.downloaded_bytes = 0;
        item.info.total_bytes = None;
        item.cancel = Arc::new(AtomicBool::new(false));
        (item.url.clone(), item.cancel.clone())
    };
    spawn_download(app, manager.clone(), id, url, cancel);
    manager.snapshot()
}

#[tauri::command]
pub fn dismiss_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    id: String,
) -> Result<Vec<DownloadInfo>, String> {
    let manager = manager.inner().clone();
    {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        let index = items
            .iter()
            .position(|item| item.info.id == id)
            .ok_or("Download desconhecido.")?;
        if matches!(
            items[index].info.status.as_str(),
            "queued" | "downloading" | "saving"
        ) {
            return Err("Cancele ou conclua o download antes de removê-lo.".into());
        }
        if items[index].info.status == "ready" {
            let stage = staged_path(&app, &id)?;
            if stage.exists() {
                fs::remove_file(stage)
                    .map_err(|error| format!("Falha ao descartar arquivo temporário: {error}"))?;
            }
        }
        items.remove(index);
    }
    manager.snapshot()
}

fn target_directory(root_text: &str, relative: &str) -> Result<PathBuf, String> {
    let root =
        fs::canonicalize(root_text).map_err(|error| format!("Biblioteca inválida: {error}"))?;
    if !root.is_dir() {
        return Err("A raiz da biblioteca não é uma pasta.".into());
    }
    let directory = super::resolve_inside(&root, relative)?;
    if !directory.is_dir() || directory.starts_with(root.join("99 - Catálogo")) {
        return Err("Escolha uma pasta existente fora do catálogo de serviço.".into());
    }
    Ok(directory)
}

fn commit_staged(
    stage: &std::path::Path,
    directory: &std::path::Path,
    filename: &str,
    id: &str,
) -> Result<PathBuf, String> {
    commit_staged_with_mode(stage, directory, filename, id, false)
}

fn commit_staged_with_mode(
    stage: &std::path::Path,
    directory: &std::path::Path,
    filename: &str,
    id: &str,
    force_fuse_fallback: bool,
) -> Result<PathBuf, String> {
    let target = directory.join(filename);
    if target.exists() {
        return Err(format!("Já existe {filename} no destino."));
    }
    if !stage.is_file() {
        return Err("Arquivo temporário ausente. Reinicie o download.".into());
    }
    let temp = directory.join(format!(".acervo-{id}.tmp"));
    let mut input = fs::File::open(stage).map_err(|error| error.to_string())?;
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|error| error.to_string())?;
    let synced = io::copy(&mut input, &mut output).and_then(|_| output.sync_all());
    drop(output);
    if let Err(error) = synced {
        let _ = fs::remove_file(&temp);
        return Err(format!("Falha ao gravar livro: {error}"));
    }
    if force_fuse_fallback || fs::hard_link(&temp, &target).is_err() {
        // FUSE mounts (including some cloud-backed libraries) can reject hard links.
        // create_new prevents replacement even if another process creates the target
        // between validation and copying. On FUSE, the final name is visible while
        // the confirmed transfer finishes; failed transfers are removed.
        let fallback = (|| -> io::Result<()> {
            let mut source = fs::File::open(&temp)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&target)?;
            let saved = io::copy(&mut source, &mut output).and_then(|_| output.sync_all());
            drop(output);
            if saved.is_err() {
                let _ = fs::remove_file(&target);
            }
            saved
        })();
        if let Err(error) = fallback {
            let _ = fs::remove_file(&temp);
            return Err(format!("Falha ao salvar sem sobrescrever: {error}"));
        }
    }
    let _ = fs::remove_file(&temp);
    let _ = fs::remove_file(stage);
    Ok(target)
}

/// User-approved commit into the library. Prefer atomic hard links; fallback to
/// exclusive creation on FUSE without ever replacing an existing book.
#[tauri::command]
pub fn save_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    id: String,
    root: String,
    destination: String,
) -> Result<String, String> {
    let manager = manager.inner().clone();
    let filename = {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        let item = items
            .iter_mut()
            .find(|item| item.info.id == id)
            .ok_or("Download desconhecido.")?;
        if item.info.status != "ready" {
            return Err("O download ainda não está pronto.".into());
        }
        item.info.status = "saving".into();
        item.info.filename.clone()
    };
    let operation = (|| -> Result<String, String> {
        let directory = target_directory(&root, &destination)?;
        let stage = staged_path(&app, &id)?;
        let target = commit_staged(&stage, &directory, &filename, &id)?;
        let relative = target
            .strip_prefix(fs::canonicalize(root).map_err(|error| error.to_string())?)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        Ok(relative)
    })();
    manager.change(&id, |item| match &operation {
        Ok(path) => {
            item.info.status = "saved".into();
            item.info.saved_path = path.clone();
            item.info.error.clear();
        }
        Err(error) => {
            item.info.status = "ready".into();
            item.info.error = error.clone();
        }
    });
    operation
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_are_safe_and_extensions_are_restricted() {
        assert_eq!(
            safe_filename("Machado/Assis?", "EPUB").unwrap(),
            "Machado_Assis_.epub"
        );
        assert!(!safe_filename("../", "pdf").unwrap().contains("/"));
        assert!(safe_filename("Livro", "exe").is_err());
        assert!(safe_filename("", "pdf").is_err());
    }

    #[test]
    fn staging_commit_never_overwrites_an_existing_book() {
        let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "acervo-download-test-{}-{unique}",
            std::process::id()
        ));
        fs::create_dir(&dir).unwrap();
        let stage = dir.join("staged.part");
        fs::write(&stage, b"new book").unwrap();
        let target = commit_staged(&stage, &dir, "book.pdf", "one").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new book");
        assert!(!stage.exists());
        fs::write(&stage, b"must not replace").unwrap();
        assert!(commit_staged(&stage, &dir, "book.pdf", "two").is_err());
        assert_eq!(fs::read(&target).unwrap(), b"new book");
        assert_eq!(fs::read(&stage).unwrap(), b"must not replace");
        let second = commit_staged_with_mode(&stage, &dir, "fallback.epub", "three", true).unwrap();
        assert_eq!(fs::read(&second).unwrap(), b"must not replace");
        fs::write(&stage, b"collision").unwrap();
        assert!(commit_staged_with_mode(&stage, &dir, "fallback.epub", "four", true).is_err());
        assert_eq!(fs::read(&second).unwrap(), b"must not replace");
        fs::remove_dir_all(&dir).unwrap();
    }
}
