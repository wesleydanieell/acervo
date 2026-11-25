//! Declarative source registry. Manifests contain data mappings, never executable code.
use crate::extension_api::API_VERSION;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Manager;

const MAX_MANIFEST_BYTES: u64 = 64 * 1024;
const MAX_SOURCES: usize = 40;

fn default_api_version() -> u32 {
    API_VERSION
}
fn default_version() -> String {
    "1.0.0".into()
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AdapterKind {
    #[default]
    Json,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceManifest {
    pub schema_version: u32,
    #[serde(default = "default_api_version")]
    pub api_version: u32,
    #[serde(default = "default_version")]
    pub version: String,
    #[serde(default)]
    pub adapter: AdapterKind,
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    pub base_url: String,
    pub search_path: String,
    pub items_path: String,
    pub title_field: String,
    #[serde(default)]
    pub author_field: String,
    #[serde(default)]
    pub format_field: String,
    #[serde(default)]
    pub language_field: String,
    #[serde(default)]
    pub download_url_field: String,
    #[serde(default)]
    pub formats: Vec<String>,
    #[serde(default)]
    pub languages: Vec<String>,
    #[serde(default)]
    pub local_search: bool,
    #[serde(default)]
    pub remote_id_field: String,
    #[serde(default)]
    pub pagination: Option<JsonPagination>,
    #[serde(default)]
    pub download_domains: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct JsonPagination {
    pub initial_cursor: String,
    pub next_cursor_field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InstalledSource {
    pub manifest: SourceManifest,
    pub enabled: bool,
}

fn validate_field(field: &str, required: bool) -> bool {
    // RFC 6901 JSON pointers support array indices and MIME keys without executing code.
    if field.starts_with('/') {
        return field.len() <= 200
            && field.split('/').skip(1).count() <= 16
            && field.split('/').skip(1).all(|part| {
                !part.is_empty()
                    && !part.chars().any(char::is_control)
                    && part
                        .split('~')
                        .skip(1)
                        .all(|suffix| suffix.starts_with('0') || suffix.starts_with('1'))
            });
    }
    (!required && field.is_empty())
        || (!field.is_empty()
            && field.len() <= 100
            && field.split('.').all(|part| {
                !part.is_empty()
                    && part.len() <= 50
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
            }))
}

pub fn validate(manifest: &SourceManifest) -> Result<(), String> {
    if manifest.api_version != API_VERSION {
        return Err(format!(
            "API de extensão incompatível: {} (esperada {}).",
            manifest.api_version, API_VERSION
        ));
    }
    let version_parts: Vec<_> = manifest.version.split('.').collect();
    if version_parts.len() != 3
        || version_parts.iter().any(|part| {
            part.is_empty()
                || part.len() > 9
                || (part.len() > 1 && part.starts_with('0'))
                || !part.bytes().all(|b| b.is_ascii_digit())
        })
    {
        return Err("version deve usar três componentes numéricos MAJOR.MINOR.PATCH.".into());
    }
    if manifest.schema_version != 1 {
        return Err("Versão de manifesto incompatível; use schemaVersion 1.".into());
    }
    if manifest.id.len() < 3
        || manifest.id.len() > 64
        || !manifest.id.starts_with(|c: char| c.is_ascii_lowercase())
        || !manifest
            .id
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
    {
        return Err("O id deve ter 3–64 caracteres minúsculos, números ou hífens.".into());
    }
    if manifest.name.trim().is_empty()
        || manifest.name.len() > 100
        || manifest.description.len() > 500
    {
        return Err("Nome ou descrição da fonte fora do limite.".into());
    }
    let url = url::Url::parse(&manifest.base_url).map_err(|_| "baseUrl inválida.")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
        || url.path() != "/"
    {
        return Err(
            "baseUrl deve ser apenas um domínio HTTPS, sem credenciais, porta ou caminho.".into(),
        );
    }
    let host = url
        .host_str()
        .ok_or("Domínio ausente.")?
        .to_ascii_lowercase();
    if !host.contains('.')
        || host.parse::<IpAddr>().is_ok()
        || host == "localhost"
        || [".localhost", ".local", ".internal", ".test", ".invalid"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
    {
        return Err("Use um domínio público HTTPS, não endereços internos ou IPs.".into());
    }
    if manifest.search_path.len() > 1000
        || !manifest.search_path.starts_with('/')
        || manifest.search_path.starts_with("//")
        || !manifest.search_path.contains("{query}")
        || manifest.search_path.contains('#')
        || manifest.search_path.contains("..")
        || manifest.search_path.chars().any(char::is_control)
    {
        return Err(
            "searchPath deve ser um caminho relativo à origem, começando em / e contendo {query}."
                .into(),
        );
    }
    if !validate_field(&manifest.items_path, true)
        || !validate_field(&manifest.title_field, true)
        || !validate_field(&manifest.author_field, false)
        || !validate_field(&manifest.format_field, false)
        || !validate_field(&manifest.language_field, false)
        || !validate_field(&manifest.download_url_field, false)
        || !validate_field(&manifest.remote_id_field, false)
    {
        return Err(
            "Seletores JSON inválidos; use campos separados por pontos ou JSON Pointer.".into(),
        );
    }
    if let Some(pagination) = &manifest.pagination {
        if !manifest.search_path.contains("{cursor}")
            || manifest.local_search
            || !validate_field(&pagination.next_cursor_field, true)
            || !valid_cursor(&pagination.initial_cursor)
        {
            return Err("Paginação JSON inválida: use cursor inicial, seletor e {cursor}; busca local não é paginada.".into());
        }
    } else if manifest.search_path.contains("{cursor}") {
        return Err("O placeholder {cursor} exige configuração de paginação.".into());
    }
    if let Some(domains) = &manifest.download_domains {
        if domains.len() > 20
            || domains.iter().any(|domain| {
                let candidate = format!("https://{domain}");
                if domain.split('.').any(|label| {
                    label.is_empty()
                        || label.starts_with('-')
                        || label.ends_with('-')
                        || !label
                            .bytes()
                            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
                }) {
                    return true;
                }
                !super::extension_network::safe_url(&candidate)
                    || url::Url::parse(&candidate)
                        .ok()
                        .and_then(|u| u.host_str().map(str::to_owned))
                        .as_deref()
                        != Some(domain.as_str())
            })
        {
            return Err("Domínios de download devem ser hosts HTTPS públicos exatos, sem curingas ou caminhos.".into());
        }
    }
    for items in [&manifest.formats, &manifest.languages] {
        if items.len() > 25
            || items.iter().any(|value| {
                value.len() > 25
                    || value.is_empty()
                    || !value
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            })
        {
            return Err("Lista de idiomas ou formatos inválida.".into());
        }
    }
    Ok(())
}

pub(crate) fn valid_cursor(cursor: &str) -> bool {
    !cursor.trim().is_empty() && cursor.len() <= 2048 && !cursor.chars().any(char::is_control)
}

pub(crate) fn permits_download(manifest: &SourceManifest, link: &str) -> bool {
    if manifest.download_url_field.is_empty() || !super::extension_network::safe_url(link) {
        return false;
    }
    match &manifest.download_domains {
        None => true, // compatibility for pre-permission manifests
        Some(domains) => url::Url::parse(link)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .is_some_and(|host| domains.contains(&host)),
    }
}

fn registry_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_config_dir()
        .map_err(|error| error.to_string())?;
    Ok(dir.join("sources.json"))
}

fn read_registry(path: &Path) -> Result<Vec<InstalledSource>, String> {
    if !path.exists() {
        return Ok(Vec::new());
    }
    if path
        .symlink_metadata()
        .map_err(|error| error.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("O arquivo de fontes não pode ser um link simbólico.".into());
    }
    let content = fs::read(path).map_err(|error| format!("Falha ao ler fontes: {error}"))?;
    if content.len() > 256 * 1024 {
        return Err("Arquivo de fontes excessivamente grande.".into());
    }
    let sources: Vec<InstalledSource> = serde_json::from_slice(&content)
        .map_err(|error| format!("Registro de fontes inválido: {error}"))?;
    if sources.len() > MAX_SOURCES {
        return Err("Limite de fontes excedido.".into());
    }
    let mut ids = std::collections::HashSet::new();
    for source in &sources {
        validate(&source.manifest)?;
        if !ids.insert(&source.manifest.id) {
            return Err("IDs duplicados no registro.".into());
        }
    }
    Ok(sources)
}

fn write_registry(path: &Path, sources: &[InstalledSource]) -> Result<(), String> {
    let dir = path.parent().ok_or("Pasta de configuração inválida.")?;
    fs::create_dir_all(dir).map_err(|error| format!("Falha ao criar configuração: {error}"))?;
    if path.exists()
        && path
            .symlink_metadata()
            .map_err(|error| error.to_string())?
            .file_type()
            .is_symlink()
    {
        return Err("O registro não pode ser um link simbólico.".into());
    }
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| error.to_string())?
        .as_nanos();
    let temp = dir.join(format!(
        ".acervo-sources-{}-{timestamp}.tmp",
        std::process::id()
    ));
    let payload = serde_json::to_vec_pretty(sources).map_err(|error| error.to_string())?;
    {
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)
            .map_err(|error| error.to_string())?;
        if let Err(error) = file.write_all(&payload).and_then(|_| file.sync_all()) {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
    }
    if path.exists() {
        if let Err(error) = fs::copy(path, path.with_extension("json.bak")) {
            let _ = fs::remove_file(&temp);
            return Err(format!(
                "Falha ao criar backup; nada foi substituído: {error}"
            ));
        }
    }
    if let Err(error) = fs::rename(&temp, path) {
        let _ = fs::remove_file(&temp);
        return Err(format!("Falha ao salvar fontes: {error}"));
    }
    Ok(())
}

#[tauri::command]
pub fn list_sources(app: tauri::AppHandle) -> Result<Vec<InstalledSource>, String> {
    read_registry(&registry_path(&app)?)
}

#[tauri::command]
pub fn install_source(app: tauri::AppHandle, path: String) -> Result<Vec<InstalledSource>, String> {
    let path = Path::new(&path);
    let metadata = fs::metadata(path).map_err(|error| format!("Arquivo inválido: {error}"))?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Err("Selecione um manifesto JSON de até 64 KB.".into());
    }
    let data = fs::read(path).map_err(|error| format!("Falha ao ler manifesto: {error}"))?;
    let manifest: SourceManifest = serde_json::from_slice(&data)
        .map_err(|error| format!("Manifesto incompatível: {error}"))?;
    install_manifest(&app, manifest)
}

fn install_manifest(
    app: &tauri::AppHandle,
    manifest: SourceManifest,
) -> Result<Vec<InstalledSource>, String> {
    validate(&manifest)?;
    let registry = registry_path(app)?;
    let mut sources = read_registry(&registry)?;
    if sources
        .iter()
        .any(|source| source.manifest.id == manifest.id)
    {
        return Err(format!("A fonte '{}' já está instalada.", manifest.id));
    }
    if sources.len() >= MAX_SOURCES {
        return Err("Limite de 40 fontes atingido.".into());
    }
    sources.push(InstalledSource {
        manifest,
        enabled: true,
    });
    write_registry(&registry, &sources)?;
    Ok(sources)
}

#[tauri::command]
pub fn install_demo_source(app: tauri::AppHandle) -> Result<Vec<InstalledSource>, String> {
    // Metadata-only optional example; all search behavior is still manifest-driven.
    install_manifest(
        &app,
        SourceManifest {
            schema_version: 1,
            api_version: API_VERSION,
            version: default_version(),
            adapter: AdapterKind::Json,
            id: "open-library".into(),
            name: "Open Library · metadados".into(),
            description: "Catálogo público de livros; não fornece downloads diretos.".into(),
            base_url: "https://openlibrary.org".into(),
            search_path: "/search.json?q={query}&limit=20".into(),
            items_path: "docs".into(),
            title_field: "title".into(),
            author_field: "author_name".into(),
            format_field: "".into(),
            language_field: "".into(),
            download_url_field: "".into(),
            formats: vec![],
            languages: vec![],
            local_search: false,
            remote_id_field: String::new(),
            pagination: None,
            download_domains: None,
        },
    )
}

#[tauri::command]
pub fn install_example_source(
    app: tauri::AppHandle,
    id: String,
) -> Result<Vec<InstalledSource>, String> {
    let raw = match id.as_str() {
        "acervo-test-source" => include_str!("../../integrations/test-source/manifest.json"),
        "gutenberg-curated" => include_str!("../../integrations/gutenberg/manifest.json"),
        _ => return Err("Fonte de exemplo desconhecida.".into()),
    };
    let manifest = serde_json::from_str(raw).map_err(|error| error.to_string())?;
    install_manifest(&app, manifest)
}

#[tauri::command]
pub fn set_source_enabled(
    app: tauri::AppHandle,
    id: String,
    enabled: bool,
) -> Result<Vec<InstalledSource>, String> {
    let registry = registry_path(&app)?;
    let mut sources = read_registry(&registry)?;
    let source = sources
        .iter_mut()
        .find(|source| source.manifest.id == id)
        .ok_or("Fonte não encontrada.")?;
    source.enabled = enabled;
    write_registry(&registry, &sources)?;
    Ok(sources)
}

#[tauri::command]
pub fn remove_source(app: tauri::AppHandle, id: String) -> Result<Vec<InstalledSource>, String> {
    let registry = registry_path(&app)?;
    let mut sources = read_registry(&registry)?;
    let before = sources.len();
    sources.retain(|source| source.manifest.id != id);
    if sources.len() == before {
        return Err("Fonte não encontrada.".into());
    }
    write_registry(&registry, &sources)?;
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn example() -> SourceManifest {
        SourceManifest {
            schema_version: 1,
            api_version: API_VERSION,
            version: default_version(),
            adapter: AdapterKind::Json,
            id: "catalogo-publico".into(),
            name: "Catálogo público".into(),
            description: "".into(),
            base_url: "https://example.org".into(),
            search_path: "/api/search?q={query}&lang={language}".into(),
            items_path: "results".into(),
            title_field: "title".into(),
            author_field: "author".into(),
            format_field: "format".into(),
            language_field: "language".into(),
            download_url_field: "download_url".into(),
            formats: vec!["epub".into(), "pdf".into()],
            languages: vec!["pt".into(), "en".into()],
            local_search: false,
            remote_id_field: String::new(),
            pagination: None,
            download_domains: None,
        }
    }

    #[test]
    fn manifest_defaults_preserve_v1_and_reject_incompatible_contracts() {
        let old = include_str!("../../integrations/test-source/manifest.json");
        let mut manifest: SourceManifest = serde_json::from_str(old).unwrap();
        assert_eq!(manifest.api_version, API_VERSION);
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.adapter, AdapterKind::Json);
        validate(&manifest).unwrap();
        manifest.api_version = API_VERSION + 1;
        assert!(validate(&manifest).is_err());
        manifest.api_version = API_VERSION;
        manifest.version = "1.02.3".into();
        assert!(validate(&manifest).is_err());
        let mut data: serde_json::Value = serde_json::from_str(old).unwrap();
        data["adapter"] = serde_json::json!("wasm");
        assert!(serde_json::from_value::<SourceManifest>(data).is_err());
    }

    #[test]
    fn explicit_download_domains_are_exact_and_empty_denies_all() {
        let mut manifest = example();
        manifest.download_domains = Some(vec!["files.example.org".into()]);
        validate(&manifest).unwrap();
        assert!(permits_download(
            &manifest,
            "https://files.example.org/book.epub"
        ));
        for link in [
            "https://evil.example.org/book.epub",
            "https://files.example.org.evil.com/book.epub",
            "http://files.example.org/book.epub",
            "https://files.example.org@evil.com/book.epub",
        ] {
            assert!(!permits_download(&manifest, link));
        }
        manifest.download_domains = Some(vec![]);
        assert!(!permits_download(
            &manifest,
            "https://files.example.org/book.epub"
        ));
        manifest.download_domains = None;
        assert!(permits_download(
            &manifest,
            "https://files.example.org/book.epub"
        ));
        for domain in [
            "*.example.org",
            "example.org/path",
            "127.0.0.1",
            "example.org:8443",
            "example.org@other.org",
        ] {
            manifest.download_domains = Some(vec![domain.into()]);
            assert!(validate(&manifest).is_err(), "{domain}");
        }
    }

    #[test]
    fn accepts_basic_manifest() {
        validate(&example()).unwrap();
    }

    #[test]
    fn accepts_bounded_json_pointers_without_breaking_legacy_selectors() {
        let mut sample = example();
        sample.author_field = "/authors/0/name".into();
        sample.download_url_field = "/formats/application~1epub+zip".into();
        validate(&sample).unwrap();
        for invalid in [
            "/formats/~2",
            "/authors//name",
            "/authors/~",
            "/authors/\u{0001}",
        ] {
            sample.author_field = invalid.into();
            assert!(validate(&sample).is_err());
        }
    }

    #[test]
    fn rejects_untrusted_endpoints_and_script_like_selectors() {
        let mut sample = example();
        for domain in [
            "http://example.org",
            "https://127.0.0.1",
            "https://localhost",
            "https://myhost.local",
            "https://example.org@127.0.0.1",
        ] {
            sample.base_url = domain.into();
            assert!(validate(&sample).is_err(), "must reject {domain}");
        }
        sample = example();
        sample.title_field = "title;alert(1)".into();
        assert!(validate(&sample).is_err());
    }

    #[test]
    fn registry_round_trip_and_duplicate_detection() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("acervo-source-test-{}-{nonce}", std::process::id()));
        fs::create_dir(&root).unwrap();
        let path = root.join("sources.json");
        let sources = vec![InstalledSource {
            manifest: example(),
            enabled: true,
        }];
        write_registry(&path, &sources).unwrap();
        assert_eq!(read_registry(&path).unwrap().len(), 1);
        let mut duplicated = sources.clone();
        duplicated.extend(sources);
        write_registry(&path, &duplicated).unwrap();
        assert!(read_registry(&path).is_err());
        let _ = fs::remove_dir_all(&root);
    }
}
