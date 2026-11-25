//! Resolve a user-selected public acquisition page into direct file links.
//! No scripts, cookies, authentication, redirects, or additional page traversal.
use crate::{
    extension_network::{safe_url, secure_https_client},
    html_parser::{format, link, selector},
    sources::{self, SourceManifest},
};
use scraper::Html;
use serde::Serialize;
use std::{collections::HashSet, time::Duration};
use url::Url;

const MAX_BODY: usize = 2 * 1024 * 1024;
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(3);
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AcquisitionLink {
    pub url: String,
    pub format: String,
}

pub(crate) fn page_url(manifest: &SourceManifest, base: &Url, href: &str) -> Option<Url> {
    if href.trim().is_empty() || href.len() > 2047 || href.chars().any(char::is_control) {
        return None;
    }
    let origin = Url::parse(&manifest.base_url).ok()?;
    let url = base.join(href).ok()?;
    (url.origin() == origin.origin() && url.fragment().is_none() && safe_url(url.as_str()))
        .then_some(url)
}

fn extract(
    manifest: &SourceManifest,
    body: &str,
    page: &Url,
) -> Result<Vec<AcquisitionLink>, String> {
    let mapping = manifest
        .download_resolver
        .as_ref()
        .ok_or("Fonte não declara resolução de downloads.")?;
    if body.len() > MAX_BODY {
        return Err("Página de aquisição excede 2 MB.".into());
    }
    let mapping = selector(&mapping.link_selector, true)?.ok_or("Seletor de aquisição ausente.")?;
    let doc = Html::parse_document(body);
    if doc.tree.nodes().take(20_001).count() > 20_000 {
        return Err("Página de aquisição contém nós demais.".into());
    }
    let mut seen = HashSet::new();
    let mut links = Vec::new();
    for candidate in doc.select(&mapping).take(400).filter_map(|e| link(page, e)) {
        let Some(fmt) = format(&candidate) else {
            continue;
        };
        if candidate.fragment().is_some()
            || !sources::permits_download(manifest, candidate.as_str())
            || !manifest.formats.is_empty()
                && !manifest
                    .formats
                    .iter()
                    .any(|f| f.eq_ignore_ascii_case(&fmt))
            || !seen.insert(candidate.to_string())
        {
            continue;
        }
        links.push(AcquisitionLink {
            url: candidate.to_string(),
            format: fmt,
        });
        if links.len() == 20 {
            break;
        }
    }
    Ok(links)
}

#[tauri::command]
pub async fn resolve_source_downloads(
    app: tauri::AppHandle,
    id: String,
    page: String,
) -> Result<Vec<AcquisitionLink>, String> {
    let source = sources::list_sources(app)?
        .into_iter()
        .find(|s| s.enabled && s.manifest.id == id)
        .ok_or("Fonte indisponível.")?;
    let manifest = source.manifest;
    if manifest.download_resolver.is_none() {
        return Err("Fonte não declara resolução de downloads.".into());
    }
    let base = Url::parse(&manifest.base_url).map_err(|_| "Origem inválida.")?;
    let page = page_url(&manifest, &base, &page).ok_or("Página fora da origem autorizada.")?;
    let _permit = SLOTS
        .try_acquire()
        .map_err(|_| "Limite de consultas simultâneas.")?;
    tokio::time::timeout(Duration::from_secs(20), async {
        let client = secure_https_client(&page, Duration::from_secs(15)).await?;
        let mut res = client
            .get(page.clone())
            .header("Accept", "text/html, application/xhtml+xml;q=0.9")
            .send()
            .await
            .map_err(|_| "Falha ao consultar página.")?;
        if !res.status().is_success() {
            return Err(format!("Resposta HTTP {}", res.status()));
        }
        if !res
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                matches!(
                    v.split(';').next().unwrap_or("").trim(),
                    "text/html" | "application/xhtml+xml"
                )
            })
        {
            return Err("A página de aquisição não retornou HTML.".into());
        }
        if res.content_length().is_some_and(|n| n > MAX_BODY as u64) {
            return Err("Página excede 2 MB.".into());
        }
        let mut body = Vec::new();
        while let Some(chunk) = res.chunk().await.map_err(|_| "Falha ao ler página.")? {
            if body.len() + chunk.len() > MAX_BODY {
                return Err("Página excede 2 MB.".into());
            }
            body.extend_from_slice(&chunk);
        }
        tokio::task::spawn_blocking(move || {
            extract(
                &manifest,
                std::str::from_utf8(&body).map_err(|_| "HTML não é UTF-8.")?,
                &page,
            )
        })
        .await
        .map_err(|_| "Falha no processamento HTML.".to_owned())?
    })
    .await
    .map_err(|_| "Tempo limite da página de aquisição.".to_owned())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_only_declared_direct_files_and_deduplicates() {
        let m:SourceManifest=serde_json::from_value(serde_json::json!({"schemaVersion":1,"id":"generic-resolver","name":"Resolver","baseUrl":"https://example.org","searchPath":"/search?q={query}","itemsPath":"items","titleField":"title","downloadDomains":["example.org"],"downloadResolver":{"pageField":"page","linkSelector":"a.file"}})).unwrap();
        sources::validate(&m).unwrap();
        let base = Url::parse("https://example.org/catalog/item").unwrap();
        let html = r#"<a class=file href="/book.epub">book</a><a class=file href="/book.epub">duplicate</a><a class=file href="https://other.org/book.pdf">other</a><a class=file href="/another-page">page</a><a class=file href="javascript:alert(1)">script</a><a href="/not-selected.pdf">other</a>"#;
        let links = extract(&m, html, &base).unwrap();
        assert_eq!(
            links,
            vec![AcquisitionLink {
                url: "https://example.org/book.epub".into(),
                format: "epub".into()
            }]
        );
        assert!(page_url(&m, &base, "https://other.org/page").is_none());
        assert!(page_url(&m, &base, "/next").is_some());
    }
}
