//! Persistent staged download queue. Nothing enters the library before explicit approval.
use super::source_search::{safe_url, secure_https_client};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::Manager;

const MAX_DOWNLOAD_BYTES: u64 = 512 * 1024 * 1024;
const MAX_ACTIVE: usize = 3;
const MAX_HISTORY: usize = 60;
const RUNNING: u8 = 0;
const CANCEL_REQUESTED: u8 = 1;
const PAUSE_REQUESTED: u8 = 2;
static NEXT_ID: AtomicU64 = AtomicU64::new(0);

fn interruption_reason(signal: &AtomicU8) -> Option<&'static str> {
    match signal.load(Ordering::Relaxed) {
        CANCEL_REQUESTED => Some("Download cancelado."),
        PAUSE_REQUESTED => Some("Download pausado."),
        _ => None,
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    etag: String,
    control: Arc<AtomicU8>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StoredDownload {
    info: DownloadInfo,
    url: String,
    #[serde(default)]
    etag: String,
}

#[derive(Clone, Default)]
pub struct DownloadManager(
    Arc<Mutex<Vec<DownloadRecord>>>,
    Option<PathBuf>,
    Option<String>,
);

fn read_download_registry(path: &Path) -> Result<Vec<DownloadRecord>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    if path
        .symlink_metadata()
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("O registro de downloads não pode ser um link simbólico.".into());
    }
    let data = fs::read(path).map_err(|error| format!("Falha ao ler downloads: {error}"))?;
    if data.len() > 256 * 1024 {
        return Err("Registro de downloads excessivamente grande.".into());
    }
    let saved: Vec<StoredDownload> = serde_json::from_slice(&data)
        .map_err(|error| format!("Registro de downloads inválido: {error}"))?;
    if saved.len() > MAX_HISTORY {
        return Err("Registro de downloads excede o limite.".into());
    }
    let mut ids = std::collections::HashSet::new();
    saved
        .into_iter()
        .map(|saved| {
            if !safe_url(&saved.url)
                || saved.info.id.len() < 3
                || saved.info.id.len() > 96
                || !saved
                    .info
                    .id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                || !ids.insert(saved.info.id.clone())
                || safe_filename(&saved.info.title, &saved.info.format)
                    .ok()
                    .as_deref()
                    != Some(saved.info.filename.as_str())
            {
                return Err("Registro de downloads contém entrada inválida.".into());
            }
            if !saved.etag.is_empty() && !valid_strong_etag(&saved.etag) {
                return Err("Registro contém um validador de arquivo inválido.".into());
            }
            Ok(DownloadRecord {
                info: saved.info,
                url: saved.url,
                etag: saved.etag,
                control: Arc::new(AtomicU8::new(RUNNING)),
            })
        })
        .collect()
}

fn write_download_registry(path: &Path, records: &[DownloadRecord]) -> Result<(), String> {
    let dir = path.parent().ok_or("Pasta de downloads inválida.")?;
    fs::create_dir_all(dir).map_err(|error| error.to_string())?;
    if path.exists()
        && path
            .symlink_metadata()
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("O registro de downloads não pode ser um link simbólico.".into());
    }
    let saved: Vec<StoredDownload> = records
        .iter()
        .map(|record| StoredDownload {
            info: record.info.clone(),
            url: record.url.clone(),
            etag: record.etag.clone(),
        })
        .collect();
    let data = serde_json::to_vec_pretty(&saved).map_err(|error| error.to_string())?;
    if data.len() > 256 * 1024 {
        return Err("Registro de downloads excede 256 KB.".into());
    }
    let tick = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temp = dir.join(format!(".downloads-{}-{tick}.tmp", std::process::id()));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(&temp).map_err(|error| error.to_string())?;
    if let Err(error) = file.write_all(&data).and_then(|_| file.sync_all()) {
        let _ = fs::remove_file(&temp);
        return Err(error.to_string());
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("Falha ao salvar downloads: {error}"));
    }
    Ok(())
}

fn reconcile_restored(record: &mut DownloadRecord, staged_size: Option<u64>) {
    match record.info.status.as_str() {
        "queued" | "downloading" | "saving" => {
            record.info.status = "failed".into();
            record.info.error =
                "Transferência interrompida ao fechar o aplicativo. Tente novamente.".into();
        }
        "pausing" => {
            record.info.status = "paused".into();
            record.info.error.clear();
        }
        "ready"
            if staged_size.is_none()
                || staged_size != Some(record.info.downloaded_bytes)
                || record
                    .info
                    .total_bytes
                    .is_some_and(|total| Some(total) != staged_size) =>
        {
            record.info.status = "failed".into();
            record.info.error = "Arquivo temporário ausente ou incompleto. Tente novamente.".into();
        }
        _ => {}
    }
    if matches!(
        record.info.status.as_str(),
        "failed" | "cancelled" | "paused"
    ) {
        record.info.downloaded_bytes = staged_size.unwrap_or(0);
    }
}

impl DownloadManager {
    pub fn restore(app: &tauri::AppHandle) -> Self {
        let path = app
            .path()
            .app_config_dir()
            .ok()
            .map(|dir| dir.join("downloads.json"));
        let result = match &path {
            Some(path) => read_download_registry(path),
            None => Err("Não foi possível acessar a pasta de configuração.".into()),
        };
        let (mut records, error) = match result {
            Ok(records) => (records, None),
            Err(error) => (Vec::new(), Some(error)),
        };
        for record in &mut records {
            let staged = staged_path(app, &record.info.id).ok();
            let staged_size = staged
                .and_then(|path| fs::metadata(path).ok())
                .map(|meta| meta.len());
            reconcile_restored(record, staged_size);
        }
        let manager = Self(Arc::new(Mutex::new(records)), path, error);
        if manager.2.is_none() {
            let _ = manager.persist();
        }
        manager
    }

    fn persist(&self) -> Result<(), String> {
        if let Some(error) = &self.2 {
            return Err(error.clone());
        }
        let path = self
            .1
            .as_ref()
            .ok_or("Registro de downloads indisponível.")?;
        let records = self.0.lock().map_err(|_| "Fila indisponível.")?;
        write_download_registry(path, &records)
    }

    fn snapshot(&self) -> Result<Vec<DownloadInfo>, String> {
        if let Some(error) = &self.2 {
            return Err(error.clone());
        }
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

fn valid_strong_etag(etag: &str) -> bool {
    let bytes = etag.as_bytes();
    (2..=200).contains(&bytes.len())
        && bytes.first() == Some(&b'"')
        && bytes.last() == Some(&b'"')
        && bytes[1..bytes.len() - 1]
            .iter()
            .all(|b| (0x21..=0x7e).contains(b) && *b != b'"')
}

fn parse_content_range(value: &str) -> Option<(u64, u64, u64)> {
    let payload = value.strip_prefix("bytes ")?;
    let (range, total) = payload.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (
        start.parse::<u64>().ok()?,
        end.parse::<u64>().ok()?,
        total.parse::<u64>().ok()?,
    );
    (start <= end && end < total && total <= MAX_DOWNLOAD_BYTES).then_some((start, end, total))
}

#[derive(Debug, PartialEq)]
struct TransferMode {
    append: bool,
    offset: u64,
    total: Option<u64>,
    etag: String,
}

fn choose_transfer(
    existing: u64,
    prior_etag: &str,
    status: u16,
    content_range: Option<&str>,
    response_etag: Option<&str>,
    length: Option<u64>,
) -> Result<TransferMode, String> {
    if status == 200 {
        if length.is_some_and(|n| n > MAX_DOWNLOAD_BYTES) {
            return Err("Arquivo maior que 512 MB.".into());
        }
        return Ok(TransferMode {
            append: false,
            offset: 0,
            total: length,
            etag: response_etag
                .filter(|tag| valid_strong_etag(tag))
                .unwrap_or("")
                .to_string(),
        });
    }
    if status != 206 {
        return Err(format!("Resposta HTTP {status}."));
    }
    if existing == 0 || !valid_strong_etag(prior_etag) {
        return Err("A fonte retornou um trecho inesperado, sem retomada autorizada.".into());
    }
    let (start, end, total) = parse_content_range(content_range.ok_or("Content-Range ausente.")?)
        .ok_or("Content-Range inválido ou incompatível.")?;
    if start != existing
        || length.is_some_and(|length| length != end - start + 1)
        || response_etag.is_some_and(|tag| tag != prior_etag)
    {
        return Err("O trecho retornado não corresponde ao arquivo interrompido.".into());
    }
    Ok(TransferMode {
        append: true,
        offset: existing,
        total: Some(total),
        etag: prior_etag.into(),
    })
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
        if item.info.status != "pausing" {
            item.info.status = "downloading".into();
        }
    });
}

async fn wait_for_interruption(signal: &AtomicU8) {
    while interruption_reason(signal).is_none() {
        tokio::time::sleep(Duration::from_millis(120)).await;
    }
}

async fn send_interruptible(
    request: reqwest::RequestBuilder,
    signal: &AtomicU8,
) -> Result<reqwest::Response, String> {
    tokio::select! {
        result = request.send() => result.map_err(|error| format!("Falha de conexão: {error}")),
        _ = wait_for_interruption(signal) =>
            Err(interruption_reason(signal).unwrap_or("Transferência interrompida.").into()),
    }
}

async fn fetch_to_stage(
    app: &tauri::AppHandle,
    manager: &DownloadManager,
    id: &str,
    url_text: &str,
    control: &AtomicU8,
) -> Result<(), String> {
    if let Some(reason) = interruption_reason(control) {
        return Err(reason.into());
    }
    let url = url::Url::parse(url_text).map_err(|_| "URL inválida.")?;
    let path = staged_path(app, id)?;
    let existing = match fs::symlink_metadata(&path) {
        Ok(meta) if meta.file_type().is_symlink() => {
            return Err("Arquivo temporário não pode ser link simbólico.".into());
        }
        Ok(meta) if meta.is_file() => meta.len(),
        Ok(_) => return Err("Arquivo temporário inválido.".into()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
        Err(error) => return Err(format!("Falha ao consultar arquivo temporário: {error}")),
    };
    if existing > MAX_DOWNLOAD_BYTES {
        return Err("Arquivo parcial excede 512 MB.".into());
    }
    let prior_etag = manager
        .0
        .lock()
        .map_err(|_| "Fila indisponível.")?
        .iter()
        .find(|record| record.info.id == id)
        .map(|record| record.etag.clone())
        .unwrap_or_default();
    let resume = existing > 0 && valid_strong_etag(&prior_etag);
    let client = tokio::select! {
        result = secure_https_client(&url, Duration::from_secs(3600)) => result?,
        _ = wait_for_interruption(control) =>
            return Err(interruption_reason(control).unwrap_or("Transferência interrompida.").into()),
    };
    let mut request = client
        .get(url.clone())
        .header(reqwest::header::ACCEPT_ENCODING, "identity");
    if resume {
        request = request
            .header(reqwest::header::RANGE, format!("bytes={existing}-"))
            .header(reqwest::header::IF_RANGE, prior_etag.as_str());
    }
    let mut response = send_interruptible(request, control).await?;
    if resume
        && (response.status().as_u16() == 416
            || (response.status().as_u16() == 206
                && choose_transfer(
                    existing,
                    &prior_etag,
                    206,
                    response
                        .headers()
                        .get(reqwest::header::CONTENT_RANGE)
                        .and_then(|v| v.to_str().ok()),
                    response
                        .headers()
                        .get(reqwest::header::ETAG)
                        .and_then(|v| v.to_str().ok()),
                    response.content_length(),
                )
                .is_err()))
    {
        // Stale or malformed ranges are never appended. Try a clean request once.
        response = send_interruptible(
            client
                .get(url)
                .header(reqwest::header::ACCEPT_ENCODING, "identity"),
            control,
        )
        .await?;
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|kind| kind.to_ascii_lowercase().starts_with("text/html"))
    {
        return Err("A fonte retornou uma página HTML, não um livro.".into());
    }
    if response
        .headers()
        .get(reqwest::header::CONTENT_ENCODING)
        .and_then(|value| value.to_str().ok())
        .is_some_and(|encoding| !encoding.eq_ignore_ascii_case("identity"))
    {
        return Err("A fonte compactou a resposta; não é seguro continuar por bytes.".into());
    }
    let header = |name| {
        response
            .headers()
            .get(name)
            .and_then(|value| value.to_str().ok())
    };
    let mode = choose_transfer(
        if resume { existing } else { 0 },
        &prior_etag,
        response.status().as_u16(),
        header(reqwest::header::CONTENT_RANGE),
        header(reqwest::header::ETAG),
        response.content_length(),
    )?;
    fs::create_dir_all(path.parent().ok_or("Cache inválido.")?)
        .map_err(|error| error.to_string())?;
    let mut options = OpenOptions::new();
    options.write(true).create(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    if mode.append {
        options.append(true);
    } else {
        options.truncate(true);
    }
    let mut output = options
        .open(&path)
        .map_err(|error| format!("Falha ao abrir arquivo temporário: {error}"))?;
    if mode.append && output.metadata().map_err(|error| error.to_string())?.len() != mode.offset {
        return Err("O tamanho do arquivo parcial mudou durante a retomada.".into());
    }
    manager.change(id, |record| {
        record.etag = mode.etag.clone();
    });
    let result: Result<(), String> = async {
        let mut downloaded = mode.offset;
        let mut persisted_bytes = mode.offset;
        set_progress(manager, id, downloaded, mode.total);
        loop {
            let chunk = tokio::select! {
                result = response.chunk() =>
                    result.map_err(|error| format!("Conexão interrompida: {error}"))?,
                _ = wait_for_interruption(control) =>
                    return Err(interruption_reason(control).unwrap_or("Transferência interrompida.").into()),
            };
            let Some(chunk) = chunk else { break };
            if let Some(reason) = interruption_reason(control) {
                return Err(reason.into());
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
            set_progress(manager, id, downloaded, mode.total);
            if downloaded.saturating_sub(persisted_bytes) >= 2 * 1024 * 1024 {
                let _ = manager.persist();
                persisted_bytes = downloaded;
            }
        }
        if let Some(reason) = interruption_reason(control) {
            return Err(reason.into());
        }
        if downloaded == 0 {
            return Err("A fonte retornou um arquivo vazio.".into());
        }
        if mode.total.is_some_and(|total| downloaded != total) {
            return Err(
                "A transferência terminou antes do tamanho anunciado pelo servidor.".into(),
            );
        }
        output.sync_all().map_err(|error| error.to_string())?;
        Ok(())
    }
    .await;
    drop(output);
    // A paused or failed transfer keeps its partial data for a validated retry.
    // Cancellation cleanup happens in spawn_download, after the file is closed.
    result?;
    let format = manager
        .0
        .lock()
        .map_err(|_| "Fila indisponível.")?
        .iter()
        .find(|record| record.info.id == id)
        .map(|record| record.info.format.clone())
        .ok_or("Download desconhecido.")?;
    super::download_validation::validate_staged_book(&path, &format)
}

fn spawn_download(
    app: tauri::AppHandle,
    manager: DownloadManager,
    id: String,
    url: String,
    control: Arc<AtomicU8>,
) {
    tauri::async_runtime::spawn(async move {
        let outcome = fetch_to_stage(&app, &manager, &id, &url, &control).await;
        let cancelled = control.load(Ordering::Relaxed) == CANCEL_REQUESTED;
        if cancelled {
            // Covers cancel arriving after fetch returned but before status was set.
            if let Ok(path) = staged_path(&app, &id) {
                let _ = fs::remove_file(path);
            }
        }
        manager.change(&id, |item| {
            if cancelled {
                item.info.status = "cancelled".into();
                item.info.error.clear();
                item.info.downloaded_bytes = 0;
                item.info.total_bytes = None;
                item.etag.clear();
            } else {
                match outcome {
                    Ok(()) => {
                        item.info.status = "ready".into();
                        item.info.error.clear();
                    }
                    Err(error) => {
                        item.info.status = if control.load(Ordering::Relaxed) == PAUSE_REQUESTED {
                            "paused"
                        } else {
                            "failed"
                        }
                        .into();
                        item.info.error = if item.info.status == "paused" {
                            String::new()
                        } else {
                            error
                        };
                    }
                }
            }
        });
        let _ = manager.persist();
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
    let control = Arc::new(AtomicU8::new(RUNNING));
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
        etag: String::new(),
        control: control.clone(),
    };
    let manager = manager.inner().clone();
    {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        if items.len() >= MAX_HISTORY {
            return Err("A fila está cheia. Limpe downloads antigos.".into());
        }
        if items
            .iter()
            .filter(|item| {
                matches!(
                    item.info.status.as_str(),
                    "queued" | "downloading" | "pausing"
                )
            })
            .count()
            >= MAX_ACTIVE
        {
            return Err("Há 3 downloads ativos. Aguarde ou cancele um deles.".into());
        }
        items.push(download);
    }
    if let Err(error) = manager.persist() {
        if let Ok(mut items) = manager.0.lock() {
            items.retain(|item| item.info.id != id);
        }
        return Err(error);
    }
    spawn_download(app, manager.clone(), id, url, control);
    manager.snapshot()
}

#[tauri::command]
pub fn pause_download(
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
        if !matches!(item.info.status.as_str(), "queued" | "downloading") {
            return Err("Apenas downloads ativos podem ser pausados.".into());
        }
        item.control.store(PAUSE_REQUESTED, Ordering::Relaxed);
        item.info.status = "pausing".into();
        item.info.error.clear();
    }
    manager.persist()?;
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
            "queued" | "downloading" | "pausing" => {
                item.control.store(CANCEL_REQUESTED, Ordering::Relaxed);
            }
            "ready" | "failed" | "cancelled" | "paused" => {
                let stage = staged_path(&app, &id)?;
                match fs::remove_file(stage) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                    Err(error) => {
                        return Err(format!("Falha ao limpar arquivo temporário: {error}"));
                    }
                }
                item.info.status = "cancelled".into();
                item.info.error.clear();
                item.info.downloaded_bytes = 0;
                item.info.total_bytes = None;
                item.etag.clear();
            }
            _ => return Err("Esse download não pode ser cancelado.".into()),
        }
    }
    manager.persist()?;
    manager.snapshot()
}

#[tauri::command]
pub fn retry_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    id: String,
) -> Result<Vec<DownloadInfo>, String> {
    let manager = manager.inner().clone();
    let (url, control) = {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        if items
            .iter()
            .filter(|item| {
                matches!(
                    item.info.status.as_str(),
                    "queued" | "downloading" | "pausing"
                )
            })
            .count()
            >= MAX_ACTIVE
        {
            return Err("Há 3 downloads ativos.".into());
        }
        let item = items
            .iter_mut()
            .find(|item| item.info.id == id)
            .ok_or("Download desconhecido.")?;
        if !matches!(item.info.status.as_str(), "failed" | "cancelled" | "paused") {
            return Err(
                "Apenas downloads pausados, cancelados ou com falha podem ser retomados.".into(),
            );
        }
        let stage = staged_path(&app, &id)?;
        let partial_size = match fs::symlink_metadata(&stage) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err("Arquivo temporário não pode ser link simbólico.".into());
            }
            Ok(meta) if meta.is_file() && meta.len() <= MAX_DOWNLOAD_BYTES => meta.len(),
            Ok(_) => return Err("Arquivo temporário inválido ou muito grande.".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => 0,
            Err(error) => return Err(format!("Falha ao verificar arquivo temporário: {error}")),
        };
        // Never append a partial without a strong validator. Retry starts clean.
        let can_resume = partial_size > 0 && valid_strong_etag(&item.etag);
        if !can_resume && partial_size > 0 {
            fs::remove_file(&stage).map_err(|error| format!("Falha ao limpar parcial: {error}"))?;
        }
        if !can_resume {
            item.etag.clear();
        }
        item.info.status = "queued".into();
        item.info.error.clear();
        item.info.downloaded_bytes = if can_resume { partial_size } else { 0 };
        if !can_resume {
            item.info.total_bytes = None;
        }
        item.control = Arc::new(AtomicU8::new(RUNNING));
        (item.url.clone(), item.control.clone())
    };
    manager.persist()?;
    spawn_download(app, manager.clone(), id, url, control);
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
        // A failed transfer may retain a large resumable .part file. Removing its
        // history entry must remove that cached data too, not orphan it on disk.
        let stage = staged_path(&app, &id)?;
        match fs::remove_file(stage) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(format!("Falha ao descartar arquivo temporário: {error}")),
        }
        items.remove(index);
    }
    manager.persist()?;
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
    let (filename, format) = {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        let item = items
            .iter_mut()
            .find(|item| item.info.id == id)
            .ok_or("Download desconhecido.")?;
        if item.info.status != "ready" {
            return Err("O download ainda não está pronto.".into());
        }
        item.info.status = "saving".into();
        (item.info.filename.clone(), item.info.format.clone())
    };
    let operation = (|| -> Result<String, String> {
        let directory = target_directory(&root, &destination)?;
        let stage = staged_path(&app, &id)?;
        // Revalidate an already-ready cache entry, including one restored from disk.
        super::download_validation::validate_staged_book(&stage, &format)?;
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
    if let Err(error) = manager.persist() {
        if let Ok(path) = &operation {
            return Err(format!(
                "Livro salvo em {path}, mas o histórico não pôde ser atualizado: {error}"
            ));
        }
    }
    operation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transfer_signal_distinguishes_pause_from_cancel() {
        let signal = AtomicU8::new(RUNNING);
        assert_eq!(interruption_reason(&signal), None);
        signal.store(PAUSE_REQUESTED, Ordering::Relaxed);
        assert_eq!(interruption_reason(&signal), Some("Download pausado."));
        signal.store(CANCEL_REQUESTED, Ordering::Relaxed);
        assert_eq!(interruption_reason(&signal), Some("Download cancelado."));
    }

    #[test]
    fn resume_requires_a_strong_etag_and_exact_byte_range() {
        assert!(valid_strong_etag("\"edition-42\""));
        assert!(!valid_strong_etag("W/\"edition-42\""));
        assert!(!valid_strong_etag("\"malformed\nvalue\""));
        assert_eq!(
            parse_content_range("bytes 128-255/256"),
            Some((128, 255, 256))
        );
        assert_eq!(parse_content_range("bytes */256"), None);
        assert_eq!(parse_content_range("bytes 128-256/256"), None);
        assert_eq!(parse_content_range("bytes 0-2/536870913"), None);

        let resumed = choose_transfer(
            128,
            "\"edition-42\"",
            206,
            Some("bytes 128-255/256"),
            Some("\"edition-42\""),
            Some(128),
        )
        .unwrap();
        assert_eq!(
            resumed,
            TransferMode {
                append: true,
                offset: 128,
                total: Some(256),
                etag: "\"edition-42\"".into(),
            }
        );
        for (etag, range, length) in [
            ("\"edition-43\"", "bytes 128-255/256", 128),
            ("\"edition-42\"", "bytes 127-255/256", 129),
            ("\"edition-42\"", "bytes 128-255/256", 127),
        ] {
            assert!(
                choose_transfer(
                    128,
                    "\"edition-42\"",
                    206,
                    Some(range),
                    Some(etag),
                    Some(length)
                )
                .is_err()
            );
        }
        assert!(choose_transfer(128, "", 206, Some("bytes 128-255/256"), None, Some(128)).is_err());
        assert!(choose_transfer(128, "\"edition-42\"", 416, None, None, None).is_err());
    }

    #[test]
    fn full_response_safely_replaces_partial_and_discards_weak_validator() {
        let fresh = choose_transfer(
            128,
            "\"edition-42\"",
            200,
            None,
            Some("W/\"edition-43\""),
            Some(256),
        )
        .unwrap();
        assert_eq!(
            fresh,
            TransferMode {
                append: false,
                offset: 0,
                total: Some(256),
                etag: String::new(),
            }
        );
        assert!(choose_transfer(128, "", 200, None, None, Some(MAX_DOWNLOAD_BYTES + 1)).is_err());
    }

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

    #[test]
    fn registry_round_trip_recovers_interrupted_jobs_and_preserves_corruption() {
        let unique = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("acervo-queue-test-{}-{unique}", std::process::id()));
        fs::create_dir(&dir).unwrap();
        let path = dir.join("downloads.json");
        let original = DownloadRecord {
            info: DownloadInfo {
                id: "job-1".into(),
                title: "Teste".into(),
                source_name: "Biblioteca livre".into(),
                format: "epub".into(),
                filename: "Teste.epub".into(),
                planned_destination: "Literatura".into(),
                status: "downloading".into(),
                downloaded_bytes: 75,
                total_bytes: Some(100),
                error: String::new(),
                saved_path: String::new(),
            },
            url: "https://example.org/public-domain.epub".into(),
            etag: "\"edition-42\"".into(),
            control: Arc::new(AtomicU8::new(RUNNING)),
        };
        write_download_registry(&path, &[original]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        let mut loaded = read_download_registry(&path).unwrap();
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].url, "https://example.org/public-domain.epub");
        assert_eq!(loaded[0].etag, "\"edition-42\"");
        reconcile_restored(&mut loaded[0], Some(80));
        assert_eq!(loaded[0].info.status, "failed");
        assert_eq!(loaded[0].info.downloaded_bytes, 80);
        loaded[0].info.status = "pausing".into();
        reconcile_restored(&mut loaded[0], Some(81));
        assert_eq!(loaded[0].info.status, "paused");
        assert_eq!(loaded[0].info.downloaded_bytes, 81);
        loaded[0].info.status = "ready".into();
        loaded[0].info.downloaded_bytes = 100;
        reconcile_restored(&mut loaded[0], Some(100));
        assert_eq!(loaded[0].info.status, "ready");
        reconcile_restored(&mut loaded[0], Some(99));
        assert_eq!(loaded[0].info.status, "failed");
        fs::write(&path, b"not valid json").unwrap();
        assert!(read_download_registry(&path).is_err());
        assert_eq!(fs::read(&path).unwrap(), b"not valid json");
        fs::remove_dir_all(&dir).unwrap();
    }
}
