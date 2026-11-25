//! OPDS acquisition feeds via the same public HTTPS transport as JSON sources.
use super::extension_api::{
    API_VERSION, BookSource, SearchPage, SearchRequest, SourceCapabilities, SourceDescriptor,
    SourceError, SourceErrorCode,
};
use super::extension_network::{safe_url, secure_https_client};
use super::opds_parser::parse_feed;
use super::sources::SourceManifest;
use std::{future::Future, pin::Pin, time::Duration};

const MAX_FEED_BYTES: usize = 2 * 1024 * 1024;
pub(crate) struct OpdsSource {
    manifest: SourceManifest,
}
impl OpdsSource {
    pub(crate) fn new(manifest: SourceManifest) -> Self {
        Self { manifest }
    }
}
fn invalid(message: impl Into<String>) -> SourceError {
    SourceError {
        code: SourceErrorCode::InvalidRequest,
        message: message.into(),
    }
}
fn network(message: impl Into<String>) -> SourceError {
    SourceError {
        code: SourceErrorCode::Network,
        message: message.into(),
    }
}
fn response(message: impl Into<String>) -> SourceError {
    SourceError {
        code: SourceErrorCode::InvalidResponse,
        message: message.into(),
    }
}

fn feed_url(manifest: &SourceManifest, request: &SearchRequest) -> Result<url::Url, SourceError> {
    let base = url::Url::parse(&manifest.base_url).map_err(|_| invalid("Origem OPDS inválida."))?;
    let encode = |value: &str| -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    };
    let url = if let Some(cursor) = &request.cursor {
        url::Url::parse(cursor).map_err(|_| invalid("Cursor OPDS inválido."))?
    } else {
        let path = manifest
            .search_path
            .replace("{query}", &encode(&request.query))
            .replace("{language}", &encode(&request.language))
            .replace("{format}", &encode(&request.format));
        base.join(&path)
            .map_err(|_| invalid("Endpoint OPDS inválido."))?
    };
    if url.origin() != base.origin() || url.fragment().is_some() || !safe_url(url.as_str()) {
        return Err(invalid(
            "O endpoint OPDS precisa permanecer na origem HTTPS declarada.",
        ));
    }
    Ok(url)
}

impl BookSource for OpdsSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            api_version: API_VERSION,
            capabilities: SourceCapabilities {
                search: true,
                download: self
                    .manifest
                    .download_domains
                    .as_ref()
                    .is_some_and(|hosts| !hosts.is_empty()),
                pagination: true,
                details: false,
                editions: false,
                feed: self.manifest.supports_feed(),
            },
        }
    }
    fn search(
        &self,
        request: SearchRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SearchPage, SourceError>> + Send + '_>> {
        Box::pin(async move {
            request.validate()?;
            fetch_page(&self.manifest, request).await
        })
    }
    fn browse(
        &self,
        cursor: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<SearchPage, SourceError>> + Send + '_>> {
        Box::pin(async move {
            let manifest = self.manifest.for_feed()?;
            let request = SearchRequest {
                query: String::new(),
                language: String::new(),
                format: String::new(),
                cursor,
            };
            request.validate_feed()?;
            fetch_page(&manifest, request).await
        })
    }
}

async fn fetch_page(
    manifest: &SourceManifest,
    request: SearchRequest,
) -> Result<SearchPage, SourceError> {
    for (filter, supported) in [
        (&request.language, &manifest.languages),
        (&request.format, &manifest.formats),
    ] {
        if !filter.is_empty()
            && !supported.is_empty()
            && !supported
                .iter()
                .any(|value| value.eq_ignore_ascii_case(filter))
        {
            return Err(SourceError {
                code: SourceErrorCode::Unsupported,
                message: "Filtro não oferecido por esta fonte.".into(),
            });
        }
    }
    let url = feed_url(&manifest, &request)?;
    let client = secure_https_client(&url, Duration::from_secs(15))
        .await
        .map_err(network)?;
    let mut res = client
        .get(url.clone())
        .header(
            "Accept",
            "application/opds+json, application/atom+xml;q=0.9, application/json;q=0.8",
        )
        .send()
        .await
        .map_err(|_| network("Falha ao conectar à fonte OPDS."))?;
    if !res.status().is_success() {
        return Err(network(format!("Resposta HTTP {}", res.status())));
    }
    if res
        .content_length()
        .is_some_and(|size| size > MAX_FEED_BYTES as u64)
    {
        return Err(response("Feed OPDS excede 2 MB."));
    }
    let mut body = Vec::new();
    while let Some(chunk) = res
        .chunk()
        .await
        .map_err(|_| network("Falha na leitura do feed OPDS."))?
    {
        if body.len() + chunk.len() > MAX_FEED_BYTES {
            return Err(response("Feed OPDS excede 2 MB."));
        }
        body.extend_from_slice(&chunk);
    }
    let body = std::str::from_utf8(&body).map_err(|_| response("Feed OPDS não é UTF-8."))?;
    parse_feed(&manifest, body, &url, &request)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reference_feeds_share_the_same_book_contract() {
        let request = SearchRequest {
            query: "Acervo".into(),
            language: "pt".into(),
            format: "epub".into(),
            cursor: None,
        };
        let mut pages = Vec::new();
        for (raw, body) in [
            (
                include_str!("../../integrations/opds-test/manifest-atom.json"),
                include_str!("../../integrations/opds-test/feed.atom"),
            ),
            (
                include_str!("../../integrations/opds-test/manifest-json.json"),
                include_str!("../../integrations/opds-test/feed.json"),
            ),
        ] {
            let manifest: SourceManifest = serde_json::from_str(raw).unwrap();
            let source = crate::extension_host::create_source(manifest.clone()).unwrap();
            assert!(source.descriptor().capabilities.pagination);
            let page = parse_feed(
                &manifest,
                body,
                &feed_url(&manifest, &request).unwrap(),
                &request,
            )
            .unwrap();
            assert_eq!(page.results.len(), 1);
            assert_eq!(
                page.results[0].remote_id.as_deref(),
                Some("urn:acervo:test:1")
            );
            assert!(page.next_cursor.is_none());
            pages.push(page);
        }
        let a = &pages[0].results[0];
        let b = &pages[1].results[0];
        assert_eq!(
            (&a.title, &a.author, &a.format, &a.download_url),
            (&b.title, &b.author, &b.format, &b.download_url)
        );
    }

    #[test]
    fn opds_cursors_never_leave_the_declared_origin() {
        let manifest: SourceManifest=serde_json::from_str(r#"{"schemaVersion":1,"id":"opds-test","name":"Test","adapter":"opds","baseUrl":"https://example.org","searchPath":"/search?q={query}","downloadDomains":["example.org"]}"#).unwrap();
        super::super::sources::validate(&manifest).unwrap();
        let mut request = SearchRequest {
            query: "a&b".into(),
            language: "".into(),
            format: "".into(),
            cursor: None,
        };
        assert_eq!(
            feed_url(&manifest, &request)
                .unwrap()
                .query_pairs()
                .next()
                .unwrap()
                .1,
            "a&b"
        );
        for cursor in [
            "https://evil.org/next",
            "http://example.org/next",
            "https://example.org/next#frag",
            "https://user@example.org/next",
            "https://example.org:8443/next",
            "//example.org/next",
        ] {
            request.cursor = Some(cursor.into());
            assert!(feed_url(&manifest, &request).is_err(), "{cursor}");
        }
        request.cursor = Some("https://example.org/next?page=2".into());
        assert!(feed_url(&manifest, &request).is_ok());
    }
}
