//! Explicitly configured HTTPS indexes; only verified declarative manifests are installed.
use crate::{
    extension_network::{safe_url, secure_https_client},
    repository_index::{RepositoryEntry, RepositoryIndex, parse_index, verify_manifest},
    sources::{self, InstalledSource, SourceManifest},
};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tauri::Manager;
use url::Url;
const MAX_REPOS: usize = 20;
const MAX_INDEX: usize = 256 * 1024;
const MAX_MANIFEST: usize = 64 * 1024;
static LOCK: Mutex<()> = Mutex::new(());

fn checked_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw.trim()).map_err(|_| "URL de repositório inválida.")?;
    if !safe_url(url.as_str()) || url.fragment().is_some() || url.query().is_some() {
        return Err("Use URL HTTPS pública sem fragmento ou parâmetros.".into());
    }
    Ok(url)
}
fn path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?
        .join("extension-repositories.json"))
}
fn read(path: &Path) -> Result<Vec<String>, String> {
    match fs::symlink_metadata(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
        Ok(m) if m.is_file() && !m.file_type().is_symlink() && m.len() <= 64 * 1024 => {}
        _ => return Err("Registro de repositórios inválido.".into()),
    }
    let urls: Vec<String> = serde_json::from_slice(&fs::read(path).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    if urls.len() > MAX_REPOS
        || urls.iter().any(|s| checked_url(s).is_err())
        || urls.iter().collect::<std::collections::HashSet<_>>().len() != urls.len()
    {
        return Err("Registro de repositórios inválido.".into());
    }
    Ok(urls)
}
fn write(path: &Path, urls: &[String]) -> Result<(), String> {
    let parent = path.parent().ok_or("Pasta inválida.")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_nanos();
    let temp = parent.join(format!(".repos-{}-{stamp}.tmp", std::process::id()));
    let data = serde_json::to_vec_pretty(urls).map_err(|e| e.to_string())?;
    let result = (|| -> Result<(), String> {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        file.write_all(&data)
            .and_then(|_| file.sync_all())
            .map_err(|e| e.to_string())?;
        fs::rename(&temp, path).map_err(|e| e.to_string())
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}
async fn fetch(url: &Url, limit: usize) -> Result<Vec<u8>, String> {
    let client = secure_https_client(url, Duration::from_secs(15)).await?;
    let mut response = client
        .get(url.clone())
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| "Falha de rede ao consultar repositório.")?;
    if !response.status().is_success() {
        return Err(format!("Resposta HTTP {}", response.status()));
    }
    if response.content_length().is_some_and(|n| n > limit as u64) {
        return Err("Resposta excede limite.".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| "Falha ao ler resposta.")?
    {
        if bytes.len() + chunk.len() > limit {
            return Err("Resposta excede limite.".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
async fn index(url: &Url) -> Result<RepositoryIndex, String> {
    parse_index(&fetch(url, MAX_INDEX).await?, url)
}
fn registered(app: &tauri::AppHandle, raw: &str) -> Result<Url, String> {
    let url = checked_url(raw)?;
    if !read(&path(app)?)?.iter().any(|s| s == url.as_str()) {
        return Err("Adicione o repositório antes de consultar.".into());
    }
    Ok(url)
}
async fn verified(
    app: &tauri::AppHandle,
    raw: &str,
    id: &str,
    sha256: &str,
) -> Result<(Url, RepositoryEntry, SourceManifest), String> {
    let url = registered(app, raw)?;
    let entry = index(&url)
        .await?
        .extensions
        .into_iter()
        .find(|e| e.id == id && e.sha256 == sha256)
        .ok_or("Índice mudou; atualize a lista e revise novamente.")?;
    let manifest_url = checked_url(&entry.manifest_url)?;
    let manifest = verify_manifest(&entry, &fetch(&manifest_url, MAX_MANIFEST).await?)?;
    Ok((url, entry, manifest))
}
#[tauri::command]
pub fn list_repositories(app: tauri::AppHandle) -> Result<Vec<String>, String> {
    read(&path(&app)?)
}
#[tauri::command]
pub async fn add_repository(app: tauri::AppHandle, url: String) -> Result<Vec<String>, String> {
    let url = checked_url(&url)?;
    index(&url).await?;
    let _guard = LOCK.lock().map_err(|_| "Registro indisponível.")?;
    let path = path(&app)?;
    let mut urls = read(&path)?;
    if urls.iter().any(|s| s == url.as_str()) {
        return Ok(urls);
    }
    if urls.len() >= MAX_REPOS {
        return Err("Limite de repositórios atingido.".into());
    }
    urls.push(url.into());
    write(&path, &urls)?;
    Ok(urls)
}
#[tauri::command]
pub fn remove_repository(app: tauri::AppHandle, url: String) -> Result<Vec<String>, String> {
    let _guard = LOCK.lock().map_err(|_| "Registro indisponível.")?;
    let path = path(&app)?;
    let mut urls = read(&path)?;
    urls.retain(|s| s != &url);
    write(&path, &urls)?;
    Ok(urls)
}
#[tauri::command]
pub async fn browse_repository(
    app: tauri::AppHandle,
    url: String,
) -> Result<RepositoryIndex, String> {
    index(&registered(&app, &url)?).await
}
#[tauri::command]
pub async fn preview_repository_extension(
    app: tauri::AppHandle,
    url: String,
    id: String,
    sha256: String,
) -> Result<SourceManifest, String> {
    let (_, _, manifest) = verified(&app, &url, &id, &sha256).await?;
    Ok(manifest)
}
#[tauri::command]
pub async fn install_repository_extension(
    app: tauri::AppHandle,
    url: String,
    id: String,
    sha256: String,
) -> Result<Vec<InstalledSource>, String> {
    let (url, entry, manifest) = verified(&app, &url, &id, &sha256).await?;
    let _guard = LOCK.lock().map_err(|_| "Registro indisponível.")?;
    registered(&app, url.as_str())?;
    sources::apply_repository_manifest(&app, manifest, url.into(), entry.sha256)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_repository_manifests_match_published_digests() {
        let url=Url::parse("https://raw.githubusercontent.com/wesleydanieell/acervo/160c11ccb3b45a0e4cf2a2148b5e5f37e892290d/integrations/extension-repository/index.json").unwrap();
        let repo = parse_index(
            include_bytes!("../../integrations/extension-repository/index.json"),
            &url,
        )
        .unwrap();
        assert_eq!(repo.extensions.len(), 4);
        for entry in repo.extensions {
            let data = match entry.manifest_url.rsplit('/').next().unwrap() {
                "gutenberg.json" => {
                    include_bytes!("../../integrations/extension-repository/gutenberg.json")
                        .as_slice()
                }
                "test-json.json" => {
                    include_bytes!("../../integrations/extension-repository/test-json.json")
                        .as_slice()
                }
                "test-opds.json" => {
                    include_bytes!("../../integrations/extension-repository/test-opds.json")
                        .as_slice()
                }
                "test-html.json" => {
                    include_bytes!("../../integrations/extension-repository/test-html.json")
                        .as_slice()
                }
                _ => panic!("unexpected manifest"),
            };
            let manifest = verify_manifest(&entry, data).unwrap();
            assert_eq!(manifest.id, entry.id);
        }
    }

    #[test]
    fn registry_roundtrip_and_unsafe_urls() {
        let p = std::env::temp_dir().join(format!(
            "acervo-repos-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let urls = vec!["https://example.org/index.json".to_owned()];
        assert!(read(&p).unwrap().is_empty());
        write(&p, &urls).unwrap();
        assert_eq!(read(&p).unwrap(), urls);
        for url in [
            "http://example.org/index",
            "https://127.0.0.1/index",
            "https://example.org/index#frag",
            "https://user@example.org/index",
            "https://example.org/index?token=x",
        ] {
            assert!(checked_url(url).is_err());
        }
        fs::write(&p, b"invalid").unwrap();
        assert!(read(&p).is_err());
        fs::remove_file(p).unwrap();
    }
}
