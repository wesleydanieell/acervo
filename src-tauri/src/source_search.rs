//! Constrained JSON source search. No extension code is executed.
use super::sources::{self, SourceManifest};
use serde::Serialize;
use serde_json::Value;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESULTS_PER_SOURCE: usize = 40;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceResult {
    pub source_id: String,
    pub source_name: String,
    pub title: String,
    pub author: String,
    pub format: String,
    pub language: String,
    pub download_url: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOutcome {
    pub results: Vec<SourceResult>,
    pub errors: Vec<String>,
}

fn public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(a == 0
        || a == 10
        || a == 127
        || a >= 224
        || (a == 100 && (64..=127).contains(&b))
        || (a == 169 && b == 254)
        || (a == 172 && (16..=31).contains(&b))
        || (a == 192 && (b == 168 || (b == 0 && c == 0) || (b == 0 && c == 2)))
        || (a == 198 && (b == 18 || b == 19 || (b == 51 && c == 100)))
        || (a == 203 && b == 0 && c == 113))
}

fn public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(ip) => public_ipv4(ip),
        IpAddr::V6(ip) => {
            if let Some(mapped) = ip.to_ipv4_mapped() {
                return public_ipv4(mapped);
            }
            !(ip.is_unspecified()
                || ip.is_loopback()
                || ip.is_multicast()
                || ip.is_unique_local()
                || ip.is_unicast_link_local())
        }
    }
}

pub(crate) fn safe_url(url: &str) -> bool {
    let Ok(url) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    url.scheme() == "https"
        && url.username().is_empty()
        && url.password().is_none()
        && url.port().is_none()
        && url.as_str().len() < 2048
        && host.contains('.')
        && host.parse::<IpAddr>().is_err()
        && ![".local", ".localhost", ".internal", ".test", ".invalid"]
            .iter()
            .any(|suffix| host.ends_with(suffix))
}

/// Build a request client pinned to public DNS answers, with no redirects or proxy.
pub(crate) async fn secure_https_client(
    url: &url::Url,
    timeout: Duration,
) -> Result<reqwest::Client, String> {
    if !safe_url(url.as_str()) {
        return Err("URL de origem insegura.".into());
    }
    let hostname = url.host_str().ok_or("Domínio ausente.")?.to_string();
    let resolved: Vec<SocketAddr> = tokio::net::lookup_host((hostname.as_str(), 443))
        .await
        .map_err(|error| format!("DNS: {error}"))?
        .take(24)
        .collect();
    if resolved.is_empty() || resolved.iter().any(|addr| !public_ip(addr.ip())) {
        return Err("DNS apontou para IP interno ou inválido; conexão bloqueada.".into());
    }
    reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .resolve_to_addrs(&hostname, &resolved)
        .timeout(timeout)
        .build()
        .map_err(|error| error.to_string())
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn search_url(
    source: &SourceManifest,
    query: &str,
    language: &str,
    format: &str,
) -> Result<url::Url, String> {
    let path = source
        .search_path
        .replace("{query}", &encode(query))
        .replace("{language}", &encode(language))
        .replace("{format}", &encode(format));
    let base = url::Url::parse(&source.base_url).map_err(|error| error.to_string())?;
    let url = base.join(&path).map_err(|error| error.to_string())?;
    if url.origin() != base.origin() || !safe_url(url.as_str()) {
        return Err("O endpoint tentou sair do domínio HTTPS declarado.".into());
    }
    Ok(url)
}

fn field<'a>(record: &'a Value, path: &str) -> Option<&'a Value> {
    if path.is_empty() {
        return None;
    }
    let mut value = record;
    for part in path.split('.') {
        value = value.get(part)?;
    }
    Some(value)
}

fn read_string(record: &Value, path: &str) -> String {
    let value = field(record, path);
    let text = match value {
        Some(Value::String(value)) => value.clone(),
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .take(2)
            .collect::<Vec<_>>()
            .join(", "),
        Some(Value::Number(value)) => value.to_string(),
        _ => String::new(),
    };
    text.trim().chars().take(300).collect()
}

fn supports_filter(
    declared: &[String],
    field: &str,
    search_path: &str,
    placeholder: &str,
    value: &str,
) -> bool {
    value.is_empty()
        || ((declared.is_empty() || declared.iter().any(|item| item.eq_ignore_ascii_case(value)))
            && (!field.is_empty() || search_path.contains(placeholder)))
}

async fn search_one(
    source: SourceManifest,
    query: String,
    language: String,
    format: String,
) -> Result<Vec<SourceResult>, String> {
    if !supports_filter(
        &source.formats,
        &source.format_field,
        &source.search_path,
        "{format}",
        &format,
    ) {
        return Err("Esta fonte não oferece o formato solicitado.".into());
    }
    if !supports_filter(
        &source.languages,
        &source.language_field,
        &source.search_path,
        "{language}",
        &language,
    ) {
        return Err("Esta fonte não oferece o idioma solicitado.".into());
    }
    let url = search_url(&source, &query, &language, &format)?;
    let client = secure_https_client(&url, Duration::from_secs(15)).await?;
    let mut response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| format!("Conexão: {error}"))?;
    if !response.status().is_success() {
        return Err(format!("Resposta HTTP {}", response.status()));
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err("Resposta JSON excede 2 MB.".into());
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|error| format!("Leitura: {error}"))?
    {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err("Resposta JSON excede 2 MB.".into());
        }
        body.extend_from_slice(&chunk);
    }
    let data: Value =
        serde_json::from_slice(&body).map_err(|error| format!("JSON inválido: {error}"))?;
    let records = field(&data, &source.items_path)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Resposta não contém array em '{}'.", source.items_path))?;

    let mut result = Vec::new();
    for record in records.iter().take(MAX_RESULTS_PER_SOURCE * 3) {
        let title = read_string(record, &source.title_field);
        if title.is_empty() {
            continue;
        }
        let item_format = read_string(record, &source.format_field).to_ascii_lowercase();
        let item_language = read_string(record, &source.language_field).to_ascii_lowercase();
        if !format.is_empty()
            && !item_format.eq_ignore_ascii_case(&format)
            && (!item_format.is_empty() || !source.search_path.contains("{format}"))
        {
            continue;
        }
        if !language.is_empty()
            && !item_language.eq_ignore_ascii_case(&language)
            && (!item_language.is_empty() || !source.search_path.contains("{language}"))
        {
            continue;
        }
        let candidate_url = read_string(record, &source.download_url_field);
        let download_url = if safe_url(&candidate_url) {
            candidate_url
        } else {
            String::new()
        };
        result.push(SourceResult {
            source_id: source.id.clone(),
            source_name: source.name.clone(),
            title,
            author: read_string(record, &source.author_field),
            format: item_format,
            language: item_language,
            download_url,
        });
        if result.len() == MAX_RESULTS_PER_SOURCE {
            break;
        }
    }
    Ok(result)
}

#[tauri::command]
pub async fn search_sources(
    app: tauri::AppHandle,
    query: String,
    language: String,
    format: String,
    source_ids: Vec<String>,
) -> Result<SearchOutcome, String> {
    if query.trim().chars().count() < 2
        || query.chars().count() > 200
        || query.chars().any(char::is_control)
    {
        return Err("Digite uma pesquisa de 2–200 caracteres.".into());
    }
    if language.len() > 25
        || format.len() > 25
        || ![&language, &format].iter().all(|value| {
            value
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        })
    {
        return Err("Filtros inválidos.".into());
    }
    let sources = sources::list_sources(app)?
        .into_iter()
        .filter(|source| {
            source.enabled && (source_ids.is_empty() || source_ids.contains(&source.manifest.id))
        })
        .collect::<Vec<_>>();
    if sources.len() > 12 {
        return Err("Selecione no máximo 12 fontes por pesquisa.".into());
    }
    if sources.is_empty() {
        return Ok(SearchOutcome {
            results: vec![],
            errors: vec!["Nenhuma fonte ativa selecionada.".into()],
        });
    }

    let mut jobs = tokio::task::JoinSet::new();
    for source in sources {
        let query = query.clone();
        let language = language.clone();
        let format = format.clone();
        jobs.spawn(async move {
            let name = source.manifest.name.clone();
            (
                name,
                search_one(source.manifest, query, language, format).await,
            )
        });
    }
    let mut outcome = SearchOutcome {
        results: Vec::new(),
        errors: Vec::new(),
    };
    while let Some(done) = jobs.join_next().await {
        match done {
            Ok((_, Ok(results))) => outcome.results.extend(results),
            Ok((name, Err(error))) => outcome.errors.push(format!("{name}: {error}")),
            Err(error) => outcome
                .errors
                .push(format!("Erro interno de pesquisa: {error}")),
        }
    }
    outcome.results.sort_by(|a, b| {
        a.source_name
            .cmp(&b.source_name)
            .then(a.title.cmp(&b.title))
    });
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn prevents_private_networks_and_unsafe_urls() {
        for ip in [
            "127.0.0.1",
            "10.0.0.3",
            "192.168.1.1",
            "169.254.169.254",
            "172.19.0.1",
            "100.64.0.1",
            "::1",
            "fc00::1",
        ] {
            assert!(
                !public_ip(ip.parse().unwrap()),
                "IP should be blocked: {ip}"
            );
        }
        assert!(public_ip("1.1.1.1".parse().unwrap()));
        assert!(!safe_url("http://example.org/"));
        assert!(!safe_url("https://127.0.0.1/"));
        assert!(!safe_url("https://example.org:8443/"));
    }
    #[test]
    fn rejects_unsupported_filters_without_metadata_or_endpoint_support() {
        assert!(!supports_filter(
            &[],
            "",
            "/search?q={query}",
            "{format}",
            "pdf"
        ));
        assert!(supports_filter(
            &[],
            "",
            "/search?q={query}&format={format}",
            "{format}",
            "pdf"
        ));
        assert!(supports_filter(
            &["pdf".into()],
            "format",
            "/search?q={query}",
            "{format}",
            "pdf"
        ));
        assert!(!supports_filter(
            &["epub".into()],
            "format",
            "/search?q={query}",
            "{format}",
            "pdf"
        ));
        assert!(supports_filter(
            &[],
            "",
            "/search?q={query}",
            "{format}",
            ""
        ));
    }
    #[test]
    fn encodes_search_values_and_extracts_arrays() {
        let source: SourceManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion":1,"id":"example-source","name":"Example","baseUrl":"https://example.org",
            "searchPath":"/api/search?q={query}&lang={language}","itemsPath":"docs","titleField":"title"
        })).unwrap();
        let url = search_url(&source, "Ética e Política", "pt", "").unwrap();
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "q").unwrap().1,
            "Ética e Política"
        );
        let data = serde_json::json!({"docs":[{"author":["Machado", "Assis"]}]});
        let first = &data["docs"][0];
        assert_eq!(read_string(first, "author"), "Machado, Assis");
    }
}
