//! Persistent staged download queue. Nothing enters the library before explicit approval.
pub(crate) mod restructure;
use super::catalog_lists::CatalogTarget;
use super::extension_network::{safe_url, secure_https_client};
use super::import_fingerprint::{FileFingerprint, fingerprint};
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{self, Write};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tauri::{Emitter, Manager};

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

/// Descriptive provider metadata, not authority to modify a catalog work.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DownloadEdition {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub edition_label: String,
    #[serde(default)]
    pub remote_id: Option<String>,
    #[serde(default)]
    pub source_id: String,
}

impl DownloadEdition {
    pub(crate) fn validate(&self) -> Result<(), String> {
        if [
            &self.title,
            &self.author,
            &self.language,
            &self.publisher,
            &self.edition_label,
            &self.source_id,
        ]
        .iter()
        .any(|value| value.len() > 1200 || value.chars().any(char::is_control))
            || self
                .remote_id
                .as_ref()
                .is_some_and(|id| id.len() > 2048 || id.chars().any(char::is_control))
        {
            return Err("Metadados de edição inválidos ou muito longos.".into());
        }
        Ok(())
    }
}

/// Durable record of the exact user-approved import. Never resume filesystem writes
/// from this record without explicit user confirmation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ImportIntent {
    root: String,
    destination: String,
    link_catalog: bool,
    fingerprint: FileFingerprint,
}

impl ImportIntent {
    fn validate(&self) -> Result<(), String> {
        if self.root.len() > 4096
            || !Path::new(&self.root).is_absolute()
            || self.root.chars().any(char::is_control)
        {
            return Err("Raiz de recuperação inválida.".into());
        }
        destination_parts(&self.destination)?;
        self.fingerprint.validate()
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
    #[serde(default)]
    pub catalog_target: Option<CatalogTarget>,
    #[serde(default)]
    pub catalog_linked: bool,
    #[serde(default)]
    pub auto_import: bool,
    #[serde(default)]
    pub edition: Option<DownloadEdition>,
    // None denotes the historical filename algorithm; Some("") opts into the new one.
    #[serde(default)]
    pub filename_suffix: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub naming_preset: Option<super::download_naming::NamingPreset>,
    #[serde(default)]
    import_intent: Option<ImportIntent>,
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
    if data.len() > 1024 * 1024 {
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
                || record_filename(&saved.info, saved.info.filename_suffix.as_deref())
                    .ok()
                    .as_deref()
                    != Some(saved.info.filename.as_str())
            {
                return Err("Registro de downloads contém entrada inválida.".into());
            }
            if let Some(intent) = &saved.info.import_intent {
                intent.validate()?;
                if intent.link_catalog && saved.info.catalog_target.is_none()
                    || saved
                        .info
                        .catalog_target
                        .as_ref()
                        .is_some_and(|target| target.root != intent.root)
                {
                    return Err("Plano de importação incompatível com a ficha.".into());
                }
            }
            if let Some(edition) = &saved.info.edition {
                edition.validate()?;
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
    if data.len() > 1024 * 1024 {
        return Err("Registro de downloads excede 1 MB.".into());
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
    if record.info.import_intent.is_some() {
        record.info.status = "recovery".into();
        record.info.error =
            "Importação interrompida. Verifique o destino e recupere sem sobrescrever arquivos."
                .into();
        return;
    }
    match record.info.status.as_str() {
        "saving"
            if staged_size.is_some_and(|size| size > 0 && size == record.info.downloaded_bytes) =>
        {
            record.info.status = "ready".into();
            record.info.error =
                "Importação anterior interrompida; confira o destino antes de salvar.".into();
        }
        "waiting" => {
            record.info.status = "paused".into();
            record.info.error =
                "Fila interrompida ao fechar o aplicativo. Retome quando desejar.".into();
        }
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
                .and_then(|path| fs::symlink_metadata(path).ok())
                .filter(|meta| meta.is_file() && !meta.file_type().is_symlink())
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

fn planned_filename(title: &str, format: &str, suffix: Option<&str>) -> Result<String, String> {
    match suffix {
        Some(suffix) if suffix.len() > 300 => Err("Complemento do nome excede 300 bytes.".into()),
        Some(suffix) => super::download_naming::edition_filename(title, format, suffix),
        None => safe_filename(title, format),
    }
}

fn record_filename(info: &DownloadInfo, suffix: Option<&str>) -> Result<String, String> {
    if let Some(preset) = info.naming_preset {
        let suffix = suffix.ok_or("Registro de nome sem complemento.")?;
        let author = super::download_naming::author_for_name(
            info.catalog_target
                .as_ref()
                .map(|target| target.author.as_str()),
            info.edition.as_ref().map(|edition| edition.author.as_str()),
        );
        super::download_naming::named_filename(&info.title, &author, &info.format, suffix, preset)
    } else {
        planned_filename(&info.title, &info.format, suffix)
    }
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

type ReservedJob = (String, String, Arc<AtomicU8>);

fn reserve_waiting(items: &mut [DownloadRecord]) -> Vec<ReservedJob> {
    let available = MAX_ACTIVE.saturating_sub(
        items
            .iter()
            .filter(|item| {
                matches!(
                    item.info.status.as_str(),
                    "queued" | "downloading" | "pausing"
                )
            })
            .count(),
    );
    items
        .iter_mut()
        .filter(|item| item.info.status == "waiting")
        .take(available)
        .map(|item| {
            item.info.status = "queued".into();
            item.info.error.clear();
            (item.info.id.clone(), item.url.clone(), item.control.clone())
        })
        .collect()
}

fn schedule_downloads(app: &tauri::AppHandle, manager: &DownloadManager) -> Result<(), String> {
    if let Some(error) = &manager.2 {
        return Err(error.clone());
    }
    let path = manager.1.as_ref().ok_or("Registro indisponível.")?;
    let jobs = {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        let previous = items.clone();
        let jobs = reserve_waiting(&mut items);
        if jobs.is_empty() {
            return Ok(());
        }
        if let Err(error) = write_download_registry(path, &items) {
            *items = previous;
            return Err(error);
        }
        jobs
    };
    for (id, url, control) in jobs {
        spawn_download(app.clone(), manager.clone(), id, url, control);
    }
    Ok(())
}

fn request_schedule(app: &tauri::AppHandle, manager: &DownloadManager) {
    if let Err(error) = schedule_downloads(app, manager) {
        if let Ok(mut items) = manager.0.lock() {
            for item in items
                .iter_mut()
                .filter(|item| item.info.status == "waiting")
            {
                item.info.error = format!("Fila aguardando: {error}. Tente iniciar novamente.");
            }
        }
    }
}

#[tauri::command]
pub fn start_waiting_downloads(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
) -> Result<Vec<DownloadInfo>, String> {
    schedule_downloads(&app, &manager)?;
    manager.snapshot()
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
        // Finalize under the same queue lock used by cancel_download. A retry cannot
        // observe "cancelled" until the previous worker has removed its staged file.
        manager.change(&id, |item| {
            if control.load(Ordering::Relaxed) == CANCEL_REQUESTED {
                let cleanup = staged_path(&app, &id).and_then(|path| match fs::remove_file(path) {
                    Ok(()) => Ok(()),
                    Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
                    Err(error) => Err(error.to_string()),
                });
                item.info.status = if cleanup.is_ok() {
                    "cancelled"
                } else {
                    "failed"
                }
                .into();
                item.info.error = cleanup.err().unwrap_or_default();
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
        if manager.persist().is_err() {
            return;
        }
        request_schedule(&app, &manager);
        let auto_target = manager.0.lock().ok().and_then(|items| {
            items
                .iter()
                .find(|item| {
                    item.info.id == id
                        && item.info.status == "ready"
                        && item.info.auto_import
                        && item.control.load(Ordering::Relaxed) == RUNNING
                })
                .and_then(|item| item.info.catalog_target.clone())
        });
        if let Some(target) = auto_target {
            let root = target.root.clone();
            let destination = if target.destination == "." {
                String::new()
            } else {
                target.destination.clone()
            };
            let _ = tauri::async_runtime::spawn_blocking(move || {
                if let Err(error) = target.validate() {
                    manager.change(&id, |item| {
                        if item.info.status == "ready" {
                            item.info.auto_import = false;
                            item.info.error = format!("Importação automática suspensa: {error}");
                        }
                    });
                    let _ = manager.persist();
                    return;
                }
                let result = save_download_to_library(
                    app.clone(),
                    manager.clone(),
                    id.clone(),
                    root.clone(),
                    destination,
                    true,
                    true,
                );
                if result.is_err() {
                    manager.change(&id, |item| {
                        if item.info.status == "ready" {
                            item.info.auto_import = false;
                        }
                    });
                    let _ = manager.persist();
                }
            })
            .await;
        }
    });
}

#[tauri::command]
pub fn list_downloads(
    manager: tauri::State<'_, DownloadManager>,
) -> Result<Vec<DownloadInfo>, String> {
    manager.snapshot()
}

fn same_unfinished_download(existing: &DownloadRecord, candidate: &DownloadRecord) -> bool {
    matches!(
        existing.info.status.as_str(),
        "waiting"
            | "queued"
            | "downloading"
            | "pausing"
            | "paused"
            | "ready"
            | "saving"
            | "recovery"
    ) && existing.url == candidate.url
        && existing.info.format == candidate.info.format
        && existing.info.planned_destination == candidate.info.planned_destination
        && existing.info.catalog_target == candidate.info.catalog_target
}

#[tauri::command]
pub fn queue_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    url: String,
    title: String,
    source_id: String,
    format: String,
    planned_destination: String,
    catalog_target: Option<CatalogTarget>,
    auto_import: Option<bool>,
    edition: Option<DownloadEdition>,
    filename_suffix: Option<String>,
    naming_preset: Option<super::download_naming::NamingPreset>,
    expected_filename: Option<String>,
) -> Result<Vec<DownloadInfo>, String> {
    let source = super::sources::list_sources(app.clone())?
        .into_iter()
        .find(|source| source.enabled && source.manifest.id == source_id)
        .ok_or("Fonte ausente ou desativada.")?;
    if !super::sources::permits_download(&source.manifest, &url) {
        return Err("O domínio do download não é permitido por esta extensão.".into());
    }
    let source_name = source.manifest.name;
    let edition = edition
        .map(|mut metadata| {
            metadata.source_id = source_id.clone();
            for value in [
                &mut metadata.title,
                &mut metadata.author,
                &mut metadata.language,
                &mut metadata.publisher,
                &mut metadata.edition_label,
            ] {
                *value = value
                    .chars()
                    .map(|c| if c.is_control() { ' ' } else { c })
                    .collect::<String>()
                    .trim()
                    .into();
            }
            metadata.validate()?;
            Ok::<_, String>(metadata)
        })
        .transpose()?;
    let filename_suffix = Some(filename_suffix.unwrap_or_default());
    let auto_import = auto_import.unwrap_or(false);
    let (title, planned_destination) = if let Some(target) = &catalog_target {
        target.validate()?;
        let destination = if target.destination == "." {
            String::new()
        } else {
            target.destination.clone()
        };
        if auto_import {
            destination_parts(&destination)?;
        }
        (target.title.clone(), destination)
    } else {
        if auto_import {
            return Err("Importação automática exige uma obra do catálogo.".into());
        }
        (title, planned_destination)
    };
    if !safe_url(&url) {
        return Err("O link precisa ser HTTPS público e seguro.".into());
    }
    if title.len() > 500 || source_name.len() > 120 || planned_destination.len() > 600 {
        return Err("Metadados de download fora do limite.".into());
    }
    let naming_preset = Some(naming_preset.unwrap_or_default());
    let author = super::download_naming::author_for_name(
        catalog_target.as_ref().map(|target| target.author.as_str()),
        edition.as_ref().map(|edition| edition.author.as_str()),
    );
    let filename = super::download_naming::named_filename(
        &title,
        &author,
        &format,
        filename_suffix.as_deref().unwrap_or_default(),
        naming_preset.unwrap_or_default(),
    )?;
    if expected_filename
        .as_ref()
        .is_some_and(|expected| expected != &filename)
    {
        return Err("O nome confirmado mudou. Confira a prévia novamente.".into());
    }
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
            status: "waiting".into(),
            downloaded_bytes: 0,
            total_bytes: None,
            error: String::new(),
            saved_path: String::new(),
            catalog_target,
            catalog_linked: false,
            auto_import,
            edition,
            filename_suffix,
            naming_preset,
            import_intent: None,
        },
        url: url.clone(),
        etag: String::new(),
        control: control.clone(),
    };
    let manager = manager.inner().clone();
    {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        if items
            .iter()
            .any(|item| same_unfinished_download(item, &download))
        {
            return Err("Este arquivo já está na fila para o mesmo destino.".into());
        }
        if items.len() >= MAX_HISTORY {
            return Err("A fila está cheia. Limpe downloads antigos.".into());
        }
        if let Some(error) = &manager.2 {
            return Err(error.clone());
        }
        let path = manager.1.as_ref().ok_or("Registro indisponível.")?;
        items.push(download);
        if let Err(error) = write_download_registry(path, &items) {
            items.pop();
            return Err(error);
        }
    }
    request_schedule(&app, &manager);
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
        if item.info.status == "waiting" {
            let previous = item.info.clone();
            item.info.status = "paused".into();
            item.info.error.clear();
            if let Err(error) =
                write_download_registry(manager.1.as_ref().ok_or("Registro indisponível.")?, &items)
            {
                items
                    .iter_mut()
                    .find(|item| item.info.id == id)
                    .unwrap()
                    .info = previous;
                return Err(error);
            }
            drop(items);
            return manager.snapshot();
        }
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
            "waiting" | "ready" | "failed" | "cancelled" | "paused" => {
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
    {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
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
        item.info.status = "waiting".into();
        item.info.error.clear();
        item.info.downloaded_bytes = if can_resume { partial_size } else { 0 };
        if !can_resume {
            item.info.total_bytes = None;
        }
        item.control = Arc::new(AtomicU8::new(RUNNING));
        // Retried work goes to the back of the FIFO.
        let index = items.iter().position(|item| item.info.id == id).unwrap();
        let item = items.remove(index);
        items.push(item);
        if let Err(error) =
            write_download_registry(manager.1.as_ref().ok_or("Registro indisponível.")?, &items)
        {
            if let Some(item) = items.last_mut() {
                item.info.status = "failed".into();
                item.info.error = error.clone();
            }
            return Err(error);
        }
    }
    request_schedule(&app, &manager);
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
            "waiting" | "queued" | "downloading" | "pausing" | "saving" | "recovery"
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

fn protected_folder(name: &str) -> bool {
    let normalized = name
        .trim()
        .to_lowercase()
        .replace(['–', '—'], "-")
        .replace('á', "a");
    normalized
        .split_once('-')
        .is_some_and(|(left, right)| left.trim() == "99" && right.trim() == "catalogo")
        || name.starts_with('.')
        || name == "node_modules"
}

/// Validate the entire path before creating anything; reject symlink traversal.
pub(crate) fn destination_parts(relative: &str) -> Result<Vec<&str>, String> {
    if relative.is_empty() {
        return Ok(vec![]);
    }
    let parts: Vec<_> = relative.split('/').collect();
    if relative.len() > 600
        || parts.len() > 80
        || parts.iter().any(|part| {
            part.is_empty()
                || part.len() > 255
                || part.trim() != *part
                || part.contains('\\')
                || part.chars().any(char::is_control)
                || protected_folder(part)
        })
    {
        return Err("Use um caminho relativo válido, fora das pastas protegidas.".into());
    }
    Ok(parts)
}

fn target_directory(
    root: &Path,
    relative: &str,
    create_missing: bool,
    created: &mut Vec<PathBuf>,
) -> Result<PathBuf, String> {
    if !root.is_dir() {
        return Err("A raiz da biblioteca não é uma pasta.".into());
    }
    let parts = destination_parts(relative)?;
    let mut directory = root.to_path_buf();
    for part in parts {
        directory.push(part);
        match fs::symlink_metadata(&directory) {
            Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {}
            Ok(_) => return Err("O destino contém arquivo ou link simbólico.".into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound && create_missing => {
                fs::create_dir(&directory)
                    .map_err(|error| format!("Falha ao criar destino: {error}"))?;
                created.push(directory.clone());
            }
            Err(error) => return Err(format!("Destino indisponível: {error}")),
        }
        if fs::canonicalize(&directory).map_err(|error| error.to_string())? != directory {
            return Err("O destino mudou ou aponta para fora da biblioteca.".into());
        }
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
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = directory.join(format!(".acervo-{id}-{nonce}.tmp"));
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

/// Rename only a staged, ready download; never touch the physical library here.
#[tauri::command]
pub fn rename_ready_download(
    manager: tauri::State<'_, DownloadManager>,
    id: String,
    suffix: String,
) -> Result<Vec<DownloadInfo>, String> {
    rename_ready(&manager, &id, &suffix)?;
    manager.snapshot()
}

fn rename_ready(manager: &DownloadManager, id: &str, suffix: &str) -> Result<(), String> {
    if let Some(error) = &manager.2 {
        return Err(error.clone());
    }
    let path = manager
        .1
        .as_ref()
        .ok_or("Registro de downloads indisponível.")?;
    let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
    let index = items
        .iter()
        .position(|item| item.info.id == id)
        .ok_or("Download desconhecido.")?;
    if items[index].info.status != "ready" {
        return Err("Somente downloads prontos podem ser renomeados.".into());
    }
    let next = record_filename(&items[index].info, Some(suffix))?;
    let old = items[index].info.clone();
    items[index].info.filename = next;
    items[index].info.filename_suffix = Some(suffix.into());
    items[index].info.error.clear();
    if let Err(error) = write_download_registry(path, &items) {
        items[index].info = old;
        return Err(error);
    }
    Ok(())
}

/// User-approved commit into the library. Prefer atomic hard links; fallback to
/// exclusive creation on FUSE without ever replacing an existing book.
#[tauri::command]
pub async fn save_download(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    id: String,
    root: String,
    destination: String,
    create_missing: Option<bool>,
    link_catalog: Option<bool>,
) -> Result<String, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        save_download_to_library(
            app,
            manager,
            id,
            root,
            destination,
            create_missing.unwrap_or(false),
            link_catalog.unwrap_or(false),
        )
    })
    .await
    .map_err(|error| format!("Falha ao executar a importação: {error}"))?
}

fn approved_root(
    root: &str,
    catalog_target: Option<&CatalogTarget>,
    link_catalog: bool,
) -> Result<PathBuf, String> {
    let root = fs::canonicalize(root).map_err(|error| format!("Biblioteca inválida: {error}"))?;
    if !root.is_dir() {
        return Err("Biblioteca inválida.".into());
    }
    if let Some(target) = catalog_target {
        if root.to_str() != Some(target.root.as_str()) {
            return Err("Selecione a biblioteca de origem deste download.".into());
        }
    } else if link_catalog {
        return Err("Download sem ficha de origem.".into());
    }
    Ok(root)
}

/// Check an existing path without following any symlink or creating directories.
/// A missing ancestor means the file is missing.
fn inspect_target(
    root: &Path,
    destination: &str,
    filename: &str,
) -> Result<Option<PathBuf>, String> {
    let mut directory = root.to_path_buf();
    for part in destination_parts(destination)? {
        directory.push(part);
        match fs::symlink_metadata(&directory) {
            Ok(meta)
                if meta.is_dir()
                    && !meta.file_type().is_symlink()
                    && fs::canonicalize(&directory).ok().as_ref() == Some(&directory) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            _ => return Err("Destino indisponível ou contém link simbólico.".into()),
        }
    }
    let target = directory.join(filename);
    match fs::symlink_metadata(&target) {
        Ok(meta) if meta.is_file() && !meta.file_type().is_symlink() => Ok(Some(target)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        _ => Err("Arquivo de destino inválido ou indisponível.".into()),
    }
}

fn update_import_record(
    manager: &DownloadManager,
    id: &str,
    update: impl FnOnce(&mut DownloadRecord),
) -> Result<(), String> {
    if let Some(error) = &manager.2 {
        return Err(error.clone());
    }
    let path = manager
        .1
        .as_ref()
        .ok_or("Registro de downloads indisponível.")?;
    let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
    let index = items
        .iter()
        .position(|record| record.info.id == id)
        .ok_or("Download desconhecido.")?;
    let previous = items[index].clone();
    update(&mut items[index]);
    if let Err(error) = write_download_registry(path, &items) {
        items[index] = previous;
        return Err(error);
    }
    Ok(())
}

fn persist_edition(
    info: &DownloadInfo,
    intent: &ImportIntent,
    relative: &str,
) -> Result<(), String> {
    if let Some(metadata) = &info.edition {
        super::library_editions::record(
            Path::new(&intent.root),
            super::library_editions::EditionRecord {
                path: relative.into(),
                source_name: info.source_name.clone(),
                metadata: metadata.clone(),
                fingerprint: intent.fingerprint.clone(),
            },
        )?;
    }
    Ok(())
}

fn mark_saved(item: &mut DownloadRecord, path: &str, association: Option<Result<(), String>>) {
    item.info.status = "saved".into();
    item.info.saved_path = path.into();
    item.info.catalog_linked = matches!(&association, Some(Ok(())));
    item.info.error = match association {
        Some(Err(error)) => format!("Arquivo salvo; associação não concluída: {error}"),
        _ => String::new(),
    };
    item.info.import_intent = None;
}

fn save_download_to_library(
    app: tauri::AppHandle,
    manager: DownloadManager,
    id: String,
    root: String,
    destination: String,
    create_missing: bool,
    link_catalog: bool,
) -> Result<String, String> {
    let info = {
        let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
        let item = items
            .iter_mut()
            .find(|item| item.info.id == id)
            .ok_or("Download desconhecido.")?;
        if item.info.status != "ready" {
            return Err("O download ainda não está pronto.".into());
        }
        item.info.status = "saving".into();
        item.info.clone()
    };
    let prepare = (|| -> Result<(PathBuf, ImportIntent), String> {
        let stage = staged_path(&app, &id)?;
        let root = approved_root(&root, info.catalog_target.as_ref(), link_catalog)?;
        super::download_validation::validate_staged_book(&stage, &info.format)?;
        if inspect_target(&root, &destination, &info.filename)?.is_some() {
            return Err(format!("Já existe {} no destino.", info.filename));
        }
        let intent = ImportIntent {
            root: root.to_str().ok_or("Raiz não UTF-8.")?.into(),
            destination: destination.clone(),
            link_catalog,
            fingerprint: fingerprint(&stage)?,
        };
        intent.validate()?;
        if intent.fingerprint.bytes != info.downloaded_bytes {
            return Err("Arquivo temporário mudou de tamanho.".into());
        }
        Ok((stage, intent))
    })();
    let (stage, intent) = match prepare {
        Ok(value) => value,
        Err(error) => {
            manager.change(&id, |item| {
                item.info.status = "ready".into();
                item.info.error = error.clone();
            });
            return Err(error);
        }
    };
    if let Err(error) = update_import_record(&manager, &id, |item| {
        item.info.import_intent = Some(intent.clone())
    }) {
        manager.change(&id, |item| {
            item.info.status = "ready".into();
            item.info.error = error.clone();
        });
        return Err(error);
    }
    // The exact authorized target and verified bytes are durable before any library write.
    let (operation, association) = import_staged_book(
        &stage,
        &intent.root,
        &destination,
        &info.filename,
        &info.format,
        &id,
        create_missing,
        info.catalog_target.as_ref(),
        link_catalog,
    );
    if let Ok(path) = &operation {
        if let Err(error) = persist_edition(&info, &intent, path) {
            let message = format!("Livro salvo; registro da edição pendente: {error}");
            manager.change(&id, |item| {
                item.info.status = "recovery".into();
                item.info.error = message.clone();
            });
            let _ = manager.persist();
            let _ = app.emit("acervo-library-changed", intent.root);
            return Err(message);
        }
    }
    if let Err(error) = update_import_record(&manager, &id, |item| match &operation {
        Ok(path) => mark_saved(item, path, association),
        Err(error) => {
            item.info.status = "ready".into();
            item.info.error = error.clone();
            item.info.import_intent = None;
        }
    }) {
        manager.change(&id, |item| {
            item.info.status = "recovery".into();
            item.info.error = format!("Importação requer recuperação: {error}");
        });
        if operation.is_ok() {
            let _ = app.emit("acervo-library-changed", intent.root);
        }
        return Err(error);
    }
    if operation.is_ok() {
        let _ = app.emit("acervo-library-changed", intent.root);
    }
    operation
}

enum RecoveryOutcome {
    Saved(String, Option<Result<(), String>>),
    Ready(String),
    Failed(String),
}

fn inspect_interrupted_import(
    stage: &Path,
    info: &DownloadInfo,
    intent: &ImportIntent,
    root: &Path,
) -> Result<RecoveryOutcome, String> {
    let target = inspect_target(root, &intent.destination, &info.filename)?;
    if let Some(target) = &target {
        if fs::symlink_metadata(target)
            .map_err(|e| e.to_string())?
            .len()
            == intent.fingerprint.bytes
            && fingerprint(target)? == intent.fingerprint
        {
            super::download_validation::validate_staged_book(target, &info.format)?;
            let relative = target
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_str()
                .ok_or("Destino não UTF-8.")?
                .replace('\\', "/");
            let association = if intent.link_catalog {
                Some(
                    info.catalog_target
                        .as_ref()
                        .ok_or("Ficha de origem ausente.")?
                        .link(root, &relative),
                )
            } else {
                None
            };
            return Ok(RecoveryOutcome::Saved(relative, association));
        }
    }
    let stage_matches = fingerprint(stage).is_ok_and(|value| value == intent.fingerprint)
        && super::download_validation::validate_staged_book(stage, &info.format).is_ok();
    if stage_matches {
        let message = if target.is_some() {
            "O destino contém outro arquivo, preservado. Altere o nome ou a pasta e importe novamente."
        } else {
            "Arquivo temporário recuperado. Confirme a importação novamente."
        };
        Ok(RecoveryOutcome::Ready(message.into()))
    } else {
        Ok(RecoveryOutcome::Failed(
            "Nenhuma cópia íntegra localizada. O destino foi preservado; tente baixar novamente."
                .into(),
        ))
    }
}

#[tauri::command]
pub async fn recover_download_import(
    app: tauri::AppHandle,
    manager: tauri::State<'_, DownloadManager>,
    id: String,
    root: String,
) -> Result<Vec<DownloadInfo>, String> {
    let manager = manager.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let (info, intent, stage, root) = {
            let mut items = manager.0.lock().map_err(|_| "Fila indisponível.")?;
            let item = items
                .iter_mut()
                .find(|item| item.info.id == id)
                .ok_or("Download desconhecido.")?;
            if item.info.status != "recovery" {
                return Err("Download não está aguardando recuperação.".into());
            }
            let intent = item
                .info
                .import_intent
                .clone()
                .ok_or("Plano de recuperação ausente.")?;
            let root = approved_root(
                &root,
                item.info.catalog_target.as_ref(),
                intent.link_catalog,
            )?;
            if root.to_str() != Some(intent.root.as_str()) {
                return Err("Selecione a biblioteca da importação original.".into());
            }
            let stage = staged_path(&app, &id)?;
            item.info.status = "saving".into();
            (item.info.clone(), intent, stage, root)
        };
        let outcome = match inspect_interrupted_import(&stage, &info, &intent, &root) {
            Ok(value) => value,
            Err(error) => {
                manager.change(&id, |item| {
                    item.info.status = "recovery".into();
                    item.info.error = error.clone();
                });
                return Err(error);
            }
        };
        if let RecoveryOutcome::Saved(relative, _) = &outcome {
            if let Err(error) = persist_edition(&info, &intent, relative) {
                let message = format!("Livro preservado; registro da edição pendente: {error}");
                manager.change(&id, |item| {
                    item.info.status = "recovery".into();
                    item.info.error = message.clone();
                });
                let _ = manager.persist();
                return Err(message);
            }
        }
        let saved = matches!(&outcome, RecoveryOutcome::Saved(..));
        if let Err(error) = update_import_record(&manager, &id, |item| match outcome {
            RecoveryOutcome::Saved(path, association) => mark_saved(item, &path, association),
            RecoveryOutcome::Ready(message) => {
                item.info.status = "ready".into();
                item.info.error = message;
                item.info.auto_import = false;
                item.info.import_intent = None;
            }
            RecoveryOutcome::Failed(message) => {
                item.info.status = "failed".into();
                item.info.error = message;
                item.info.import_intent = None;
                item.info.auto_import = false;
                item.etag.clear();
                item.info.downloaded_bytes = 0;
                item.info.total_bytes = None;
            }
        }) {
            manager.change(&id, |item| {
                item.info.status = "recovery".into();
                item.info.error = error.clone();
            });
            return Err(error);
        }
        if saved {
            // Only clean up an identical cache copy, after the recovered history is durable.
            if fingerprint(&stage).is_ok_and(|value| value == intent.fingerprint) {
                let _ = fs::remove_file(stage);
            }
            let _ = app.emit("acervo-library-changed", intent.root);
        }
        manager.snapshot()
    })
    .await
    .map_err(|error| format!("Falha na recuperação: {error}"))?
}

/// Keep physical import and optional association independently observable:
/// a failed metadata write never invalidates a successfully imported book.
#[allow(clippy::too_many_arguments)]
fn import_staged_book(
    stage: &Path,
    root: &str,
    destination: &str,
    filename: &str,
    format: &str,
    id: &str,
    create_missing: bool,
    catalog_target: Option<&CatalogTarget>,
    link_catalog: bool,
) -> (Result<String, String>, Option<Result<(), String>>) {
    let mut created = Vec::new();
    let mut association_result = None;
    let operation = (|| -> Result<String, String> {
        let root =
            fs::canonicalize(&root).map_err(|error| format!("Biblioteca inválida: {error}"))?;
        if let Some(target) = catalog_target {
            if root.to_str() != Some(target.root.as_str()) {
                return Err("Selecione a biblioteca de origem deste download.".into());
            }
        } else if link_catalog {
            return Err("Download sem ficha de origem.".into());
        }
        // Validate before creating any directories.
        super::download_validation::validate_staged_book(stage, format)?;
        let directory = target_directory(&root, destination, create_missing, &mut created)?;
        let target = commit_staged(stage, &directory, filename, id)?;
        let relative = target
            .strip_prefix(&root)
            .map_err(|error| error.to_string())?
            .to_string_lossy()
            .replace('\\', "/");
        if link_catalog {
            if let Some(target) = catalog_target {
                association_result = Some(target.link(&root, &relative));
            }
        }
        Ok(relative)
    })();
    if operation.is_err() {
        for path in created.iter().rev() {
            // Only remove directories created by this operation, and only if empty.
            if fs::symlink_metadata(path)
                .is_ok_and(|meta| meta.is_dir() && !meta.file_type().is_symlink())
            {
                let _ = fs::remove_dir(path);
            }
        }
    }
    (operation, association_result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interrupted_import_recovers_verified_destination_or_cache_without_overwriting() {
        let dir = std::env::temp_dir().join(format!(
            "acervo-recovery-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let dir = fs::canonicalize(dir).unwrap();
        let root = dir.join("library");
        fs::create_dir(&root).unwrap();
        let root_text = root.to_str().unwrap();
        let stage = dir.join("download.part");
        let bytes = b"%PDF-1.7\noriginal";
        fs::write(&stage, bytes).unwrap();
        super::super::catalog_lists::import_catalog_json(
            root_text.into(),
            r#"{"versao":1,"obras":[{"titulo":"Book","autor":"Author","local_indicado":"Books"}]}"#
                .into(),
        )
        .unwrap();
        let target = CatalogTarget {
            root: root_text.into(),
            title: "Book".into(),
            author: "Author".into(),
            destination: "Books".into(),
            legacy_id: None,
        };
        let intent = ImportIntent {
            root: root_text.into(),
            destination: "Books".into(),
            link_catalog: true,
            fingerprint: fingerprint(&stage).unwrap(),
        };
        let info = DownloadInfo {
            id: "job-1".into(),
            title: "Book".into(),
            source_name: "Test".into(),
            format: "pdf".into(),
            filename: "Book.pdf".into(),
            planned_destination: "Books".into(),
            status: "saving".into(),
            downloaded_bytes: bytes.len() as u64,
            total_bytes: Some(bytes.len() as u64),
            error: String::new(),
            saved_path: String::new(),
            catalog_target: Some(target.clone()),
            catalog_linked: false,
            auto_import: true,
            edition: None,
            filename_suffix: None,
            naming_preset: None,
            import_intent: Some(intent.clone()),
        };
        let record = DownloadRecord {
            info: info.clone(),
            url: "https://example.org/book.pdf".into(),
            etag: String::new(),
            control: Arc::new(AtomicU8::new(RUNNING)),
        };
        let registry = dir.join("downloads.json");
        write_download_registry(&registry, &[record]).unwrap();
        let mut restored = read_download_registry(&registry).unwrap().remove(0);
        reconcile_restored(&mut restored, Some(bytes.len() as u64));
        assert_eq!(restored.info.status, "recovery");
        let bad_parent = dir.join("not-a-directory");
        fs::write(&bad_parent, b"x").unwrap();
        let manager = DownloadManager(
            Arc::new(Mutex::new(vec![restored.clone()])),
            Some(bad_parent.join("downloads.json")),
            None,
        );
        assert!(
            update_import_record(&manager, "job-1", |item| {
                item.info.import_intent = None;
                item.info.status = "saved".into();
            })
            .is_err()
        );
        assert_eq!(manager.snapshot().unwrap()[0].status, "recovery");
        assert!(manager.snapshot().unwrap()[0].import_intent.is_some());

        assert!(matches!(
            inspect_interrupted_import(&stage, &info, &intent, &root).unwrap(),
            RecoveryOutcome::Ready(_)
        ));
        fs::create_dir(root.join("Books")).unwrap();
        let destination = root.join("Books/Book.pdf");
        let different = b"%PDF-1.7\nmodified";
        assert_eq!(bytes.len(), different.len());
        fs::write(&destination, b"").unwrap();
        assert!(matches!(
            inspect_interrupted_import(&stage, &info, &intent, &root).unwrap(),
            RecoveryOutcome::Ready(_)
        ));
        assert_eq!(fs::metadata(&destination).unwrap().len(), 0);
        fs::write(&destination, different).unwrap();
        assert!(matches!(
            inspect_interrupted_import(&stage, &info, &intent, &root).unwrap(),
            RecoveryOutcome::Ready(_)
        ));
        assert_eq!(fs::read(&destination).unwrap(), different);
        fs::remove_file(&stage).unwrap();
        assert!(matches!(
            inspect_interrupted_import(&stage, &info, &intent, &root).unwrap(),
            RecoveryOutcome::Failed(_)
        ));
        assert_eq!(fs::read(&destination).unwrap(), different);
        fs::write(&destination, bytes).unwrap();
        for _ in 0..2 {
            assert!(
                matches!(inspect_interrupted_import(&stage,&info,&intent,&root).unwrap(),RecoveryOutcome::Saved(path,Some(Ok(()))) if path=="Books/Book.pdf")
            );
        }
        let mut with_edition = info.clone();
        with_edition.edition = Some(DownloadEdition {
            title: "Original title".into(),
            author: "Author".into(),
            publisher: "Fixture Publisher".into(),
            language: "pt".into(),
            source_id: "fixture".into(),
            ..Default::default()
        });
        for _ in 0..2 {
            persist_edition(&with_edition, &intent, "Books/Book.pdf").unwrap();
        }
        let saved_editions = crate::library_editions::load(&root).unwrap();
        assert_eq!(saved_editions.len(), 1);
        assert_eq!(
            saved_editions["Books/Book.pdf"].metadata.publisher,
            "Fixture Publisher"
        );
        let mut conflicting = with_edition.clone();
        conflicting.edition.as_mut().unwrap().publisher = "Other publisher".into();
        assert!(persist_edition(&conflicting, &intent, "Books/Book.pdf").is_err());
        let scanned = super::super::scan_collection(root_text.into()).unwrap();
        let work = scanned.works.iter().find(|w| w.title == "Book").unwrap();
        assert!(work.present);
        assert!(approved_root(dir.to_str().unwrap(), Some(&target), true).is_err());
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            fs::remove_file(&destination).unwrap();
            symlink(&registry, &destination).unwrap();
            assert!(inspect_interrupted_import(&stage, &info, &intent, &root).is_err());
        }
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn extension_fixture_search_to_import_rejects_html_and_preserves_catalog() {
        use super::super::{catalog_lists, source_search, sources};
        let manifest: sources::SourceManifest =
            serde_json::from_str(include_str!("../../integrations/test-source/manifest.json"))
                .unwrap();
        sources::validate(&manifest).unwrap();
        let data =
            serde_json::from_str(include_str!("../../integrations/test-source/catalog.json"))
                .unwrap();
        let results = source_search::extract_results(&manifest, &data, "pt", "epub").unwrap();
        assert_eq!(results.len(), 2);
        assert!(
            source_search::extract_results(&manifest, &data, "en", "epub")
                .unwrap()
                .is_empty()
        );
        let dir = std::env::temp_dir().join(format!(
            "acervo-source-fixture-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let dir = fs::canonicalize(dir).unwrap();
        let root = dir.join("library");
        fs::create_dir(&root).unwrap();
        let root_text = root.to_str().unwrap();
        catalog_lists::import_catalog_json(
            root_text.into(),
            include_str!("../../integrations/test-source/test-wishlist.json").into(),
        )
        .unwrap();
        let before = super::super::scan_collection(root_text.into()).unwrap();
        let stage = dir.join("download.part");
        for (item, payload) in [
            (
                &results[0],
                include_bytes!("../../integrations/test-source/test-book.epub").as_slice(),
            ),
            (
                &results[1],
                include_bytes!("../../integrations/test-source/invalid-book.html").as_slice(),
            ),
        ] {
            let work = before.works.iter().find(|w| w.title == item.title).unwrap();
            let target = CatalogTarget {
                root: root_text.into(),
                title: work.title.clone(),
                author: work.author.clone(),
                destination: work.destination.clone(),
                legacy_id: None,
            };
            target.validate().unwrap();
            fs::write(&stage, payload).unwrap();
            let filename = safe_filename(&target.title, &item.format).unwrap();
            let (result, link) = import_staged_book(
                &stage,
                root_text,
                &target.destination,
                &filename,
                &item.format,
                "fixture-job",
                true,
                Some(&target),
                true,
            );
            if item.download_url.ends_with(".epub") {
                assert!(result.is_ok() && matches!(link, Some(Ok(()))));
                assert!(!stage.exists());
                fs::write(&stage, payload).unwrap();
                let (collision, _) = import_staged_book(
                    &stage,
                    root_text,
                    &target.destination,
                    &filename,
                    &item.format,
                    "fixture-repeat",
                    true,
                    Some(&target),
                    true,
                );
                assert!(collision.is_err() && stage.exists());
                assert_eq!(
                    fs::read(root.join(&target.destination).join(&filename)).unwrap(),
                    payload
                );
            } else {
                assert!(result.is_err() && link.is_none());
                assert!(stage.exists());
                assert!(!root.join(&target.destination).exists());
            }
        }
        let after = super::super::scan_collection(root_text.into()).unwrap();
        assert_eq!(after.files.len(), 1);
        assert_eq!(after.works.iter().filter(|work| work.present).count(), 1);
        assert!(
            after
                .works
                .iter()
                .find(|work| work.title == results[0].title)
                .unwrap()
                .present
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn staged_import_links_only_the_requested_work_and_preserves_files_on_link_failure() {
        let dir = std::env::temp_dir().join(format!(
            "acervo-linked-import-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let dir = fs::canonicalize(dir).unwrap();
        let root = dir.join("library");
        let other = dir.join("other");
        fs::create_dir(&root).unwrap();
        fs::create_dir(&other).unwrap();
        let root_text = root.to_str().unwrap();
        super::super::catalog_lists::import_catalog_json(
            root_text.into(),
            r#"{"obras":[{"titulo":"Book","autor":"Author","local_indicado":"Planned"}]}"#.into(),
        )
        .unwrap();
        let target = CatalogTarget {
            root: root_text.into(),
            title: "Book".into(),
            author: "Author".into(),
            destination: "Planned".into(),
            legacy_id: None,
        };
        let stage = dir.join("download.part");
        fs::write(&stage, b"%PDF-1.7\n%%EOF").unwrap();
        let (result, link) = import_staged_book(
            &stage,
            other.to_str().unwrap(),
            "Wrong",
            "book.pdf",
            "pdf",
            "job-1",
            true,
            Some(&target),
            true,
        );
        assert!(result.is_err() && link.is_none());
        assert!(stage.exists() && !other.join("Wrong").exists());
        let (result, link) = import_staged_book(
            &stage,
            root_text,
            "Planned",
            "book.pdf",
            "pdf",
            "job-1",
            true,
            Some(&target),
            true,
        );
        assert_eq!(result.unwrap(), "Planned/book.pdf");
        assert_eq!(link, Some(Ok(())));
        assert!(!stage.exists());
        let scanned = super::super::scan_collection(root_text.into()).unwrap();
        assert!(scanned.works[0].present);
        let mut stale = target.clone();
        stale.title = "Missing".into();
        fs::write(&stage, b"%PDF-1.7\n%%EOF").unwrap();
        let (result, link) = import_staged_book(
            &stage,
            root_text,
            "Planned",
            "other.pdf",
            "pdf",
            "job-2",
            false,
            Some(&stale),
            true,
        );
        assert_eq!(result.unwrap(), "Planned/other.pdf");
        assert!(matches!(link, Some(Err(_))));
        assert!(root.join("Planned/other.pdf").exists());
        let scanned = super::super::scan_collection(root_text.into()).unwrap();
        assert_eq!(scanned.works[0].associated_files, vec!["Planned/book.pdf"]);
        fs::remove_dir_all(dir).unwrap();
    }

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
    fn planned_destinations_require_permission_and_reject_unsafe_components() {
        let dir = std::env::temp_dir().join(format!(
            "acervo-dest-{}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&dir).unwrap();
        let dir = fs::canonicalize(dir).unwrap();
        let mut created = Vec::new();
        assert!(target_directory(&dir, "Books/History", false, &mut created).is_err());
        assert!(!dir.join("Books").exists());
        for path in [
            "/outside",
            "Books/../escape",
            "Books//History",
            "Books/.private",
            "99 — Catalogo/Hidden",
            "Books/node_modules",
        ] {
            assert!(
                target_directory(&dir, path, true, &mut created).is_err(),
                "{path}"
            );
            assert!(created.is_empty());
        }
        let target = target_directory(&dir, "Books/History", true, &mut created).unwrap();
        assert_eq!(target, dir.join("Books/History"));
        assert_eq!(created.len(), 2);
        let mut existing = Vec::new();
        assert_eq!(
            target_directory(&dir, "Books/History", false, &mut existing).unwrap(),
            target
        );
        assert!(existing.is_empty());
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&target, dir.join("Alias")).unwrap();
            assert!(target_directory(&dir, "Alias/New", true, &mut existing).is_err());
            assert!(!target.join("New").exists());
        }
        fs::remove_dir_all(&dir).unwrap();
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
                catalog_target: None,
                catalog_linked: false,
                auto_import: false,
                edition: None,
                filename_suffix: None,
                naming_preset: None,
                import_intent: None,
            },
            url: "https://example.org/public-domain.epub".into(),
            etag: "\"edition-42\"".into(),
            control: Arc::new(AtomicU8::new(RUNNING)),
        };
        let mut jobs: Vec<_> = (0..6)
            .map(|n| {
                let mut job = original.clone();
                job.info.id = format!("job-{n}");
                job.info.status = "waiting".into();
                job.control = Arc::new(AtomicU8::new(RUNNING));
                job
            })
            .collect();
        jobs[0].info.status = "downloading".into();
        jobs[2].info.status = "paused".into();
        assert_eq!(
            reserve_waiting(&mut jobs)
                .iter()
                .map(|job| job.0.as_str())
                .collect::<Vec<_>>(),
            vec!["job-1", "job-3"]
        );
        assert!(reserve_waiting(&mut jobs).is_empty());
        jobs[0].info.status = "ready".into();
        assert_eq!(reserve_waiting(&mut jobs)[0].0, "job-4");
        jobs[1].info.status = "pausing".into();
        assert!(reserve_waiting(&mut jobs).is_empty());
        jobs[1].info.status = "cancelled".into();
        assert_eq!(reserve_waiting(&mut jobs)[0].0, "job-5");
        let mut waiting = original.clone();
        waiting.info.status = "waiting".into();
        assert!(same_unfinished_download(&waiting, &original));
        reconcile_restored(&mut waiting, Some(45));
        assert_eq!(waiting.info.status, "paused");
        assert_eq!(waiting.info.downloaded_bytes, 45);
        let mut candidate = original.clone();
        candidate.info.id = "job-2".into();
        assert!(same_unfinished_download(&original, &candidate));
        candidate.info.planned_destination = "Outro".into();
        assert!(!same_unfinished_download(&original, &candidate));
        candidate.info.planned_destination = original.info.planned_destination.clone();
        let mut finished = original.clone();
        finished.info.status = "saved".into();
        assert!(!same_unfinished_download(&finished, &candidate));
        write_download_registry(&path, &[original]).unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&path).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        // Old registry entries omit the new optional fields.
        let mut json: serde_json::Value =
            serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        let info = json[0]["info"].as_object_mut().unwrap();
        info.remove("catalogTarget");
        info.remove("catalogLinked");
        info.remove("autoImport");
        info.remove("edition");
        info.remove("filenameSuffix");
        fs::write(&path, serde_json::to_vec(&json).unwrap()).unwrap();
        let mut loaded = read_download_registry(&path).unwrap();
        assert!(loaded[0].info.catalog_target.is_none());
        assert!(!loaded[0].info.catalog_linked);
        assert!(!loaded[0].info.auto_import);
        assert!(loaded[0].info.edition.is_none() && loaded[0].info.filename_suffix.is_none());
        loaded[0].info.edition = Some(DownloadEdition {
            title: "Original".into(),
            publisher: "Editora".into(),
            language: "pt".into(),
            ..Default::default()
        });
        loaded[0].info.filename_suffix = Some("pt - Editora".into());
        loaded[0].info.filename = planned_filename("Teste", "epub", Some("pt - Editora")).unwrap();

        loaded[0].info.auto_import = true;
        loaded[0].info.catalog_target = Some(CatalogTarget {
            root: "/books".into(),
            title: "Teste".into(),
            author: "Author".into(),
            destination: "Literatura".into(),
            legacy_id: None,
        });
        loaded[0].info.naming_preset =
            Some(super::super::download_naming::NamingPreset::AuthorTitle);
        loaded[0].info.filename =
            record_filename(&loaded[0].info, loaded[0].info.filename_suffix.as_deref()).unwrap();
        assert_eq!(
            loaded[0].info.filename,
            "Author - Teste - pt - Editora.epub"
        );
        write_download_registry(&path, &loaded).unwrap();
        assert_eq!(
            read_download_registry(&path).unwrap()[0]
                .info
                .catalog_target,
            loaded[0].info.catalog_target
        );
        assert!(read_download_registry(&path).unwrap()[0].info.auto_import);
        let restored = read_download_registry(&path).unwrap();
        assert_eq!(restored[0].info.edition, loaded[0].info.edition);
        assert_eq!(restored[0].info.filename, loaded[0].info.filename);
        let manager = DownloadManager(Arc::new(Mutex::new(restored)), Some(path.clone()), None);
        assert!(rename_ready(&manager, "job-1", "Outra").is_err());
        manager.change("job-1", |item| item.info.status = "ready".into());
        rename_ready(&manager, "job-1", "Outra").unwrap();
        assert_eq!(
            read_download_registry(&path).unwrap()[0].info.filename,
            "Author - Teste - Outra.epub"
        );
        assert!(rename_ready(&manager, "job-1", &"x".repeat(301)).is_err());
        assert_eq!(
            manager.snapshot().unwrap()[0].filename,
            "Author - Teste - Outra.epub"
        );
        let invalid_registry = dir.join("missing-parent");
        fs::write(&invalid_registry, b"not a directory").unwrap();
        let broken = DownloadManager(
            manager.0.clone(),
            Some(invalid_registry.join("downloads.json")),
            None,
        );
        assert!(rename_ready(&broken, "job-1", "Not persisted").is_err());
        assert_eq!(
            manager.snapshot().unwrap()[0].filename,
            "Author - Teste - Outra.epub"
        );

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
