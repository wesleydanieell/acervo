//! Constrained JSON source search. No extension code is executed.
use super::sources::{self, SourceManifest};
use serde_json::Value;
use std::time::Duration;

const MAX_RESPONSE_BYTES: usize = 2 * 1024 * 1024;
const MAX_RESULTS_PER_SOURCE: usize = 40;

use super::extension_api::{
    API_VERSION, BookSource, SearchPage, SearchRequest, SourceBook, SourceCapabilities,
    SourceDescriptor, SourceError, SourceErrorCode,
};
use super::extension_network::{safe_url, secure_https_client};
use std::{future::Future, pin::Pin};

pub(crate) type SourceResult = SourceBook;

pub(crate) struct JsonSource {
    manifest: SourceManifest,
}

impl JsonSource {
    pub fn new(manifest: SourceManifest) -> Self {
        Self { manifest }
    }
}

impl BookSource for JsonSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            api_version: API_VERSION,
            capabilities: SourceCapabilities {
                search: true,
                download: !self.manifest.download_url_field.is_empty(),
                pagination: self.manifest.pagination.is_some(),
                details: false,
                editions: false,
            },
        }
    }

    fn search(
        &self,
        request: SearchRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SearchPage, SourceError>> + Send + '_>> {
        Box::pin(async move {
            request.validate()?;
            if request.cursor.is_some() && self.manifest.pagination.is_none() {
                return Err(SourceError {
                    code: SourceErrorCode::Unsupported,
                    message: "Esta fonte não oferece paginação.".into(),
                });
            }
            search_one(self.manifest.clone(), request).await
        })
    }
}

fn encode(value: &str) -> String {
    url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

fn search_url(
    source: &SourceManifest,
    query: &str,
    language: &str,
    format: &str,
    cursor: Option<&str>,
) -> Result<url::Url, String> {
    let cursor = cursor
        .or_else(|| {
            source
                .pagination
                .as_ref()
                .map(|p| p.initial_cursor.as_str())
        })
        .unwrap_or("");
    let path = source
        .search_path
        .replace("{cursor}", &encode(cursor))
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
    if path.starts_with('/') {
        return record.pointer(path);
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

fn field_matches(record: &Value, path: &str, value: &str) -> bool {
    match field(record, path) {
        Some(Value::Array(values)) => values
            .iter()
            .filter_map(Value::as_str)
            .any(|item| item.eq_ignore_ascii_case(value)),
        Some(Value::String(item)) => item.eq_ignore_ascii_case(value),
        _ => false,
    }
}

fn fixed_format(source: &SourceManifest) -> Option<&str> {
    if source.format_field.is_empty()
        && !source.download_url_field.is_empty()
        && source.formats.len() == 1
    {
        source.formats.first().map(String::as_str)
    } else {
        None
    }
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
    request: SearchRequest,
) -> Result<SearchPage, SourceError> {
    let SearchRequest {
        query,
        language,
        format,
        cursor,
    } = request;
    if !supports_filter(
        &source.formats,
        if fixed_format(&source).is_some() {
            &source.download_url_field
        } else {
            &source.format_field
        },
        &source.search_path,
        "{format}",
        &format,
    ) {
        return Err(SourceError {
            code: SourceErrorCode::Unsupported,
            message: "Esta fonte não oferece o formato solicitado.".into(),
        });
    }
    if !supports_filter(
        &source.languages,
        &source.language_field,
        &source.search_path,
        "{language}",
        &language,
    ) {
        return Err(SourceError {
            code: SourceErrorCode::Unsupported,
            message: "Esta fonte não oferece o idioma solicitado.".into(),
        });
    }
    let url =
        search_url(&source, &query, &language, &format, cursor.as_deref()).map_err(|message| {
            SourceError {
                code: SourceErrorCode::InvalidRequest,
                message,
            }
        })?;
    let client = secure_https_client(&url, Duration::from_secs(15))
        .await
        .map_err(|message| SourceError {
            code: SourceErrorCode::Network,
            message,
        })?;
    let mut response = client
        .get(url)
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|error| SourceError {
            code: SourceErrorCode::Network,
            message: format!("Conexão: {error}"),
        })?;
    if !response.status().is_success() {
        return Err(SourceError {
            code: SourceErrorCode::Network,
            message: format!("Resposta HTTP {}", response.status()),
        });
    }
    if response
        .content_length()
        .is_some_and(|size| size > MAX_RESPONSE_BYTES as u64)
    {
        return Err(SourceError {
            code: SourceErrorCode::InvalidResponse,
            message: "Resposta JSON excede 2 MB.".into(),
        });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|error| SourceError {
        code: SourceErrorCode::Network,
        message: format!("Leitura: {error}"),
    })? {
        if body.len() + chunk.len() > MAX_RESPONSE_BYTES {
            return Err(SourceError {
                code: SourceErrorCode::InvalidResponse,
                message: "Resposta JSON excede 2 MB.".into(),
            });
        }
        body.extend_from_slice(&chunk);
    }
    let data: Value = serde_json::from_slice(&body).map_err(|error| SourceError {
        code: SourceErrorCode::InvalidResponse,
        message: format!("JSON inválido: {error}"),
    })?;
    let mut results =
        extract_results(&source, &data, &language, &format).map_err(|message| SourceError {
            code: SourceErrorCode::InvalidResponse,
            message,
        })?;
    if source.local_search {
        results.retain(|item| matches_local_query(&item.title, &item.author, &query));
    }
    let next_cursor = source.pagination.as_ref().and_then(|pagination| {
        let token = cursor_value(&data, &pagination.next_cursor_field)?;
        let previous = cursor.as_deref().unwrap_or(&pagination.initial_cursor);
        (token != previous).then_some(token)
    });
    Ok(SearchPage {
        results,
        next_cursor,
    })
}

fn cursor_value(data: &Value, selector: &str) -> Option<String> {
    let token = match field(data, selector)? {
        Value::String(value) => value.clone(),
        Value::Number(value) if value.is_u64() => value.to_string(),
        _ => return None,
    };
    sources::valid_cursor(&token).then_some(token)
}

fn matches_local_query(title: &str, author: &str, query: &str) -> bool {
    fn fold(text: &str) -> String {
        text.to_lowercase()
            .chars()
            .map(|c| match c {
                'á' | 'à' | 'â' | 'ã' | 'ä' => 'a',
                'é' | 'è' | 'ê' | 'ë' => 'e',
                'í' | 'ì' | 'î' | 'ï' => 'i',
                'ó' | 'ò' | 'ô' | 'õ' | 'ö' => 'o',
                'ú' | 'ù' | 'û' | 'ü' => 'u',
                'ç' => 'c',
                _ if c.is_alphanumeric() => c,
                _ => ' ',
            })
            .collect()
    }
    let haystack = fold(&format!("{title} {author}"));
    fold(query)
        .split_whitespace()
        .all(|term| haystack.contains(term))
}

pub(crate) fn extract_results(
    source: &SourceManifest,
    data: &Value,
    language: &str,
    format: &str,
) -> Result<Vec<SourceResult>, String> {
    let records = field(data, &source.items_path)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("Resposta não contém array em '{}'.", source.items_path))?;

    let mut result = Vec::new();
    for record in records.iter().take(MAX_RESULTS_PER_SOURCE * 3) {
        let title = read_string(record, &source.title_field);
        if title.is_empty() {
            continue;
        }
        let candidate_url = field(record, &source.download_url_field)
            .and_then(Value::as_str)
            .unwrap_or("")
            .trim();
        let download_url = if sources::permits_download(source, candidate_url) {
            candidate_url.to_owned()
        } else {
            String::new()
        };
        let item_format = if !download_url.is_empty() {
            fixed_format(source)
                .map(str::to_owned)
                .unwrap_or_else(|| read_string(record, &source.format_field))
        } else {
            read_string(record, &source.format_field)
        }
        .to_ascii_lowercase();
        let language_matches =
            !language.is_empty() && field_matches(record, &source.language_field, language);
        let item_language = if language_matches {
            language.to_ascii_lowercase()
        } else {
            read_string(record, &source.language_field).to_ascii_lowercase()
        };
        if !format.is_empty()
            && !item_format.eq_ignore_ascii_case(format)
            && (fixed_format(source).is_some()
                || !item_format.is_empty()
                || !source.search_path.contains("{format}"))
        {
            continue;
        }
        if !language.is_empty()
            && !language_matches
            && (!item_language.is_empty() || !source.search_path.contains("{language}"))
        {
            continue;
        }
        result.push(SourceResult {
            source_id: source.id.clone(),
            source_name: source.name.clone(),
            title,
            author: read_string(record, &source.author_field),
            format: item_format,
            language: item_language,
            download_url,
            remote_id: cursor_value(record, &source.remote_id_field),
        });
        if result.len() == MAX_RESULTS_PER_SOURCE {
            break;
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
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
    fn pagination_encodes_tokens_and_preserves_remote_ids() {
        let mut source: SourceManifest =
            serde_json::from_str(include_str!("../../sdk/extensions/manifest.example.json"))
                .unwrap();
        source.search_path = "/books?q={query}&page={cursor}".into();
        source.remote_id_field = "id".into();
        source.pagination = Some(sources::JsonPagination {
            initial_cursor: "1".into(),
            next_cursor_field: "/next".into(),
        });
        source.download_domains = Some(vec!["files.example.org".into()]);
        sources::validate(&source).unwrap();
        let url = search_url(&source, "Machado", "", "", Some("2&extra=evil")).unwrap();
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "page").unwrap().1,
            "2&extra=evil"
        );
        assert_eq!(url.query_pairs().count(), 2);
        assert!(
            search_url(&source, "Books", "", "", None)
                .unwrap()
                .as_str()
                .ends_with("page=1")
        );
        let data = serde_json::json!({"next":2,"results":[{"id":123,"title":"Book","formats":{"application/epub+zip":"https://files.example.org/b.epub"}},{"id":"other","title":"Other","formats":{"application/epub+zip":"https://evil.example.org/b.epub"}}]});
        assert_eq!(cursor_value(&data, "/next").as_deref(), Some("2"));
        let results = extract_results(&source, &data, "", "").unwrap();
        assert_eq!(results[0].remote_id.as_deref(), Some("123"));
        assert!(results[1].download_url.is_empty());
        assert!(cursor_value(&serde_json::json!({"next":null}), "/next").is_none());
        source.pagination = None;
        assert!(sources::validate(&source).is_err());
    }

    #[test]
    fn curated_gutenberg_source_uses_mirror_and_matches_portuguese_queries() {
        let manifest: SourceManifest =
            serde_json::from_str(include_str!("../../integrations/gutenberg/manifest.json"))
                .unwrap();
        sources::validate(&manifest).unwrap();
        assert!(manifest.local_search);
        let data = serde_json::from_str(include_str!("../../integrations/gutenberg/catalog.json"))
            .unwrap();
        let results = extract_results(&manifest, &data, "pt", "epub").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|item| {
            item.download_url
                .starts_with("https://gutenberg.pglaf.org/")
        }));
        assert_eq!(
            results
                .iter()
                .filter(|item| matches_local_query(
                    &item.title,
                    &item.author,
                    "memorias bras cubas"
                ))
                .count(),
            1
        );
        assert_eq!(
            results
                .iter()
                .filter(|item| matches_local_query(&item.title, &item.author, "machado assis"))
                .count(),
            2
        );
        assert!(!matches_local_query(
            "Dom Casmurro",
            "Machado de Assis",
            "Shakespeare"
        ));
    }

    #[test]
    fn structured_catalogs_extract_nested_mime_links_and_match_any_language() {
        let source: SourceManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion":1,"id":"example-source","name":"Example","baseUrl":"https://example.org",
            "searchPath":"/api/search?q={query}","itemsPath":"/results","titleField":"title",
            "authorField":"/authors/0/name","languageField":"languages",
            "formats":["epub"],"downloadUrlField":"/formats/application~1epub+zip"
        })).unwrap();
        let long_url = format!("https://example.org/file.epub?token={}", "a".repeat(400));
        let data = serde_json::json!({"results":[
            {"title":"Test","authors":[{"name":"Author"}],"languages":["en","pt","es"],"formats":{"application/epub+zip":long_url}},
            {"title":"No EPUB","languages":["pt"],"formats":{"text/html":"https://example.org/page"}},
            {"title":"Other language","languages":["fr"],"formats":{"application/epub+zip":"https://example.org/other.epub"}}
        ]});
        let results = extract_results(&source, &data, "es", "epub").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].author, "Author");
        assert_eq!(results[0].format, "epub");
        assert_eq!(results[0].language, "es");
        assert_eq!(results[0].download_url, long_url);
        assert!(
            extract_results(&source, &data, "es", "pdf")
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn encodes_search_values_and_extracts_arrays() {
        let source: SourceManifest = serde_json::from_value(serde_json::json!({
            "schemaVersion":1,"id":"example-source","name":"Example","baseUrl":"https://example.org",
            "searchPath":"/api/search?q={query}&lang={language}","itemsPath":"docs","titleField":"title"
        })).unwrap();
        let url = search_url(&source, "Ética e Política", "pt", "", None).unwrap();
        assert_eq!(
            url.query_pairs().find(|(key, _)| key == "q").unwrap().1,
            "Ética e Política"
        );
        let data = serde_json::json!({"docs":[{"author":["Machado", "Assis"]}]});
        let first = &data["docs"][0];
        assert_eq!(read_string(first, "author"), "Machado, Assis");
    }
}
