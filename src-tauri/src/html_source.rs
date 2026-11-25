//! Static HTML source. No scripts, browser session or subresource requests.
use super::extension_api::{
    API_VERSION, BookSource, SearchPage, SearchRequest, SourceCapabilities, SourceDescriptor,
    SourceError, SourceErrorCode,
};
use super::extension_network::{safe_url, secure_https_client};
use super::html_parser::parse_html;
use super::sources::SourceManifest;
use std::{future::Future, pin::Pin, time::Duration};

const MAX_BODY: usize = 2 * 1024 * 1024;

pub(crate) struct HtmlSource {
    manifest: SourceManifest,
}
impl HtmlSource {
    pub(crate) fn new(manifest: SourceManifest) -> Self {
        Self { manifest }
    }
}

fn failure(code: SourceErrorCode, message: impl Into<String>) -> SourceError {
    SourceError {
        code,
        message: message.into(),
    }
}

fn page_url(manifest: &SourceManifest, request: &SearchRequest) -> Result<url::Url, SourceError> {
    let invalid = || {
        failure(
            SourceErrorCode::InvalidRequest,
            "Endpoint HTML inválido ou fora da origem.",
        )
    };
    let base = url::Url::parse(&manifest.base_url).map_err(|_| invalid())?;
    let encode = |value: &str| -> String {
        url::form_urlencoded::byte_serialize(value.as_bytes()).collect()
    };
    let url = if let Some(cursor) = &request.cursor {
        if !manifest
            .html
            .as_ref()
            .is_some_and(|mapping| !mapping.next_selector.is_empty())
        {
            return Err(failure(
                SourceErrorCode::Unsupported,
                "Esta fonte não oferece paginação.",
            ));
        }
        url::Url::parse(cursor).map_err(|_| invalid())?
    } else {
        base.join(
            &manifest
                .search_path
                .replace("{query}", &encode(&request.query))
                .replace("{language}", &encode(&request.language))
                .replace("{format}", &encode(&request.format)),
        )
        .map_err(|_| invalid())?
    };
    if url.origin() != base.origin() || url.fragment().is_some() || !safe_url(url.as_str()) {
        return Err(invalid());
    }
    Ok(url)
}

impl BookSource for HtmlSource {
    fn descriptor(&self) -> SourceDescriptor {
        let mapping = self.manifest.html.as_ref();
        SourceDescriptor {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            api_version: API_VERSION,
            capabilities: SourceCapabilities {
                search: true,
                download: mapping.is_some_and(|m| !m.download_selector.is_empty())
                    && self
                        .manifest
                        .download_domains
                        .as_ref()
                        .is_some_and(|d| !d.is_empty()),
                pagination: mapping.is_some_and(|m| !m.next_selector.is_empty()),
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
            for (value, supported) in [
                (&request.language, &self.manifest.languages),
                (&request.format, &self.manifest.formats),
            ] {
                if !value.is_empty()
                    && !supported.is_empty()
                    && !supported.iter().any(|s| s.eq_ignore_ascii_case(value))
                {
                    return Err(failure(
                        SourceErrorCode::Unsupported,
                        "Filtro não oferecido pela fonte HTML.",
                    ));
                }
            }
            let url = page_url(&self.manifest, &request)?;
            let client = secure_https_client(&url, Duration::from_secs(15))
                .await
                .map_err(|e| failure(SourceErrorCode::Network, e))?;
            let mut res = client
                .get(url.clone())
                .header("Accept", "text/html, application/xhtml+xml;q=0.9")
                .send()
                .await
                .map_err(|_| {
                    failure(SourceErrorCode::Network, "Falha ao conectar à fonte HTML.")
                })?;
            if !res.status().is_success() {
                return Err(failure(
                    SourceErrorCode::Network,
                    format!("Resposta HTTP {}", res.status()),
                ));
            }
            if res
                .content_length()
                .is_some_and(|size| size > MAX_BODY as u64)
            {
                return Err(failure(
                    SourceErrorCode::InvalidResponse,
                    "HTML excede 2 MB.",
                ));
            }
            let mut body = Vec::new();
            while let Some(chunk) = res
                .chunk()
                .await
                .map_err(|_| failure(SourceErrorCode::Network, "Falha ao ler HTML."))?
            {
                if body.len() + chunk.len() > MAX_BODY {
                    return Err(failure(
                        SourceErrorCode::InvalidResponse,
                        "HTML excede 2 MB.",
                    ));
                }
                body.extend_from_slice(&chunk);
            }
            let manifest = self.manifest.clone();
            tokio::task::spawn_blocking(move || {
                let body = std::str::from_utf8(&body)
                    .map_err(|_| failure(SourceErrorCode::InvalidResponse, "HTML não é UTF-8."))?;
                parse_html(&manifest, body, &url, &request)
            })
            .await
            .map_err(|_| failure(SourceErrorCode::Internal, "Falha no processamento HTML."))?
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixture_uses_the_common_host_and_only_permitted_epub() {
        let manifest: SourceManifest =
            serde_json::from_str(include_str!("../../integrations/html-test/manifest.json"))
                .unwrap();
        let source = crate::extension_host::create_source(manifest.clone()).unwrap();
        let descriptor = source.descriptor();
        assert!(descriptor.capabilities.search && descriptor.capabilities.download);
        assert!(!descriptor.capabilities.pagination);
        let request = SearchRequest {
            query: "Acervo".into(),
            language: "pt".into(),
            format: "epub".into(),
            cursor: None,
        };
        let page = parse_html(
            &manifest,
            include_str!("../../integrations/html-test/search.html"),
            &page_url(&manifest, &request).unwrap(),
            &request,
        )
        .unwrap();
        assert_eq!(page.results.len(), 1);
        assert_eq!(page.results[0].title, "Acervo: Livro de teste");
        assert_eq!(
            page.results[0].remote_id.as_deref(),
            Some("acervo-fixture-1")
        );
        assert!(page.results[0].download_url.ends_with("test-book.epub"));
        assert!(page.next_cursor.is_none());
    }

    #[test]
    fn html_search_urls_encode_inputs_and_reject_cross_origin_cursors() {
        let manifest: SourceManifest=serde_json::from_str(r#"{"schemaVersion":1,"id":"example-html","name":"HTML","adapter":"html","baseUrl":"https://example.org","searchPath":"/search?q={query}","downloadDomains":["example.org"],"html":{"itemSelector":"article","titleSelector":"h2","nextSelector":"a.next"}}"#).unwrap();
        crate::sources::validate(&manifest).unwrap();
        let mut request = SearchRequest {
            query: "a&b".into(),
            language: "".into(),
            format: "".into(),
            cursor: None,
        };
        assert_eq!(
            page_url(&manifest, &request)
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
        ] {
            request.cursor = Some(cursor.into());
            assert!(page_url(&manifest, &request).is_err());
        }
        request.cursor = Some("https://example.org/next?page=2".into());
        assert!(page_url(&manifest, &request).is_ok());
    }
}
