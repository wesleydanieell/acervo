//! Bounded, inert HTML extraction. This module performs no I/O.
use crate::extension_api::{SearchPage, SearchRequest, SourceBook, SourceError, SourceErrorCode};
use crate::sources::{HtmlMapping, SourceManifest};
use scraper::{ElementRef, Html, Selector};
use url::Url;

const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_NODES: usize = 20_000;
const MAX_SCAN: usize = 400;
const MAX_RESULTS: usize = 40;
const MAX_TEXT: usize = 500;

fn invalid(message: impl Into<String>) -> SourceError {
    SourceError {
        code: SourceErrorCode::InvalidResponse,
        message: message.into(),
    }
}

fn selector(value: &str, required: bool) -> Result<Option<Selector>, String> {
    if value.is_empty() && !required {
        return Ok(None);
    }
    // A deliberately small CSS subset: no pseudo classes, escapes, nesting,
    // selector lists or sibling combinators. This also blocks escaped :has.
    if value.trim().is_empty()
        || value.len() > 200
        || value.chars().any(char::is_control)
        || value.contains([':', '\\', '(', ')', ',', '+', '~', '|', '*'])
        || value.matches('>').count() > 4
        || value.split_whitespace().count() > 8
        || value.matches('[').count() > 4
    {
        return Err("Seletor HTML vazio, excessivo ou fora do subconjunto CSS permitido.".into());
    }
    Selector::parse(value)
        .map(Some)
        .map_err(|_| "Seletor HTML inválido.".into())
}

pub(crate) fn validate_mapping(mapping: &HtmlMapping) -> Result<(), String> {
    for (value, required) in [
        (&mapping.item_selector, true),
        (&mapping.title_selector, true),
        (&mapping.author_selector, false),
        (&mapping.language_selector, false),
        (&mapping.id_selector, false),
        (&mapping.download_selector, false),
        (&mapping.next_selector, false),
    ] {
        selector(value, required)?;
    }
    Ok(())
}

fn capped(value: &str, limit: usize) -> String {
    value
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}

fn text(item: ElementRef<'_>, selector: Option<&Selector>) -> String {
    let Some(found) = selector.and_then(|s| item.select(s).next()) else {
        return String::new();
    };
    let mut output = String::new();
    // Ignore executable/style text even if a mapping selects its container.
    for node in found.descendants().take(MAX_NODES) {
        let Some(value) = node.value().as_text() else {
            continue;
        };
        if node
            .ancestors()
            .filter_map(ElementRef::wrap)
            .any(|e| matches!(e.value().name(), "script" | "style" | "template"))
        {
            continue;
        }
        for word in value.split_whitespace() {
            if !output.is_empty() && output.len() < MAX_TEXT {
                output.push(' ');
            }
            for c in word.chars().filter(|c| !c.is_control()) {
                if output.len() + c.len_utf8() > MAX_TEXT {
                    return output;
                }
                output.push(c);
            }
            if output.len() >= MAX_TEXT {
                return output;
            }
        }
    }
    output
}

fn link(base: &Url, element: ElementRef<'_>) -> Option<Url> {
    if element.value().name() != "a" {
        return None;
    }
    let href = element.value().attr("href")?.trim();
    if href.is_empty() || href.len() > 2048 || href.chars().any(char::is_control) {
        return None;
    }
    let resolved = base.join(href).ok()?;
    if resolved.scheme() != "https"
        || !resolved.username().is_empty()
        || resolved.password().is_some()
        || resolved.as_str().len() > 2048
    {
        return None;
    }
    Some(resolved)
}

fn format(url: &Url) -> Option<String> {
    let extension = url
        .path()
        .rsplit('/')
        .next()?
        .rsplit_once('.')?
        .1
        .to_ascii_lowercase();
    matches!(
        extension.as_str(),
        "pdf" | "epub" | "mobi" | "azw3" | "djvu"
    )
    .then_some(extension)
}

pub(crate) fn parse_html(
    manifest: &SourceManifest,
    body: &str,
    url: &Url,
    request: &SearchRequest,
) -> Result<SearchPage, SourceError> {
    request.validate()?;
    if body.len() > MAX_BODY {
        return Err(invalid("Resposta HTML excede 2 MB."));
    }
    let mapping = manifest
        .html
        .as_ref()
        .ok_or_else(|| invalid("Mapeamento HTML ausente."))?;
    validate_mapping(mapping).map_err(invalid)?;
    let item = selector(&mapping.item_selector, true)
        .map_err(invalid)?
        .unwrap();
    let title = selector(&mapping.title_selector, true).map_err(invalid)?;
    let author = selector(&mapping.author_selector, false).map_err(invalid)?;
    let language = selector(&mapping.language_selector, false).map_err(invalid)?;
    let id = selector(&mapping.id_selector, false).map_err(invalid)?;
    let download = selector(&mapping.download_selector, false).map_err(invalid)?;
    let next = selector(&mapping.next_selector, false).map_err(invalid)?;
    let document = Html::parse_document(body);
    if document.tree.nodes().take(MAX_NODES + 1).count() > MAX_NODES {
        return Err(invalid("Resposta HTML contém nós demais."));
    }
    let mut page = SearchPage {
        results: Vec::new(),
        next_cursor: None,
    };
    for row in document.select(&item).take(MAX_SCAN) {
        let title = text(row, title.as_ref());
        if title.is_empty() {
            continue;
        }
        let language = text(row, language.as_ref());
        if !request.language.is_empty() && !request.language.eq_ignore_ascii_case(&language) {
            continue;
        }
        let acquisition = download.as_ref().and_then(|s| {
            row.select(s)
                .take(MAX_SCAN)
                .filter_map(|e| link(url, e))
                .find_map(|u| {
                    let fmt = format(&u)?;
                    (manifest.download_domains.is_some()
                        && crate::sources::permits_download(manifest, u.as_str())
                        && (request.format.is_empty() || request.format.eq_ignore_ascii_case(&fmt)))
                    .then(|| (u.to_string(), fmt))
                })
        });
        if !request.format.is_empty() && acquisition.is_none() {
            continue;
        }
        let (download_url, format) = acquisition.unwrap_or_default();
        let remote_id = text(row, id.as_ref());
        page.results.push(SourceBook {
            source_id: capped(&manifest.id, 64),
            source_name: capped(&manifest.name, 100),
            title,
            author: text(row, author.as_ref()),
            language,
            format,
            download_url,
            publisher: String::new(),
            edition_label: String::new(),
            remote_id: (!remote_id.is_empty() && remote_id.len() < MAX_TEXT - 3)
                .then_some(remote_id),
        });
        if page.results.len() == MAX_RESULTS {
            break;
        }
    }
    if let Some(next) = next {
        let mut current = url.clone();
        current.set_fragment(None);
        page.next_cursor = document
            .select(&next)
            .take(MAX_SCAN)
            .filter_map(|e| link(url, e))
            .find(|candidate| {
                crate::extension_network::safe_url(candidate.as_str())
                    && candidate.fragment().is_none()
                    && candidate.origin() == url.origin()
                    && candidate != &current
                    && request
                        .cursor
                        .as_deref()
                        .and_then(|s| url.join(s).ok())
                        .is_none_or(|previous| {
                            let mut previous = previous;
                            previous.set_fragment(None);
                            previous != *candidate
                        })
            })
            .map(|u| u.to_string());
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn manifest() -> SourceManifest {
        serde_json::from_value(json!({
            "schemaVersion":1,"id":"html-catalog","name":"Catalog",
            "adapter":"html","baseUrl":"https://example.org",
            "searchPath":"/search?q={query}","downloadDomains":["example.org","files.example.org"],
            "html":{"itemSelector":"article","titleSelector":".title",
                "authorSelector":".author","languageSelector":".language",
                "idSelector":".id","downloadSelector":".download","nextSelector":".next"}
        }))
        .unwrap()
    }
    fn request() -> SearchRequest {
        SearchRequest {
            query: "book".into(),
            language: String::new(),
            format: String::new(),
            cursor: None,
        }
    }
    fn parse(body: &str) -> SearchPage {
        parse_html(
            &manifest(),
            body,
            &Url::parse("https://example.org/catalog/page").unwrap(),
            &request(),
        )
        .unwrap()
    }
    #[test]
    fn filters_are_case_insensitive() {
        let mut req = request();
        req.language = "PT".into();
        req.format = "EPUB".into();
        let page = parse_html(&manifest(), r#"<article><h2 class=title>Book</h2><span class=language>pt</span><a class=download href=/book.epub>EPUB</a></article>"#, &Url::parse("https://example.org/search").unwrap(), &req).unwrap();
        assert_eq!(page.results.len(), 1);
    }

    #[test]
    fn malformed_html_text_descendants_and_inert_base() {
        let page = parse(
            r#"<base href="https://evil.org/"><h1 class=title>Outside</h1><article data-id=wrong><b class=title> A &amp; <i>Book</i></b><span class=id>remote-7</span><span class=author>Writer</span><a class=download href=../book.EPUB?file=bad.pdf>download"#,
        );
        let book = &page.results[0];
        assert_eq!(book.title, "A & Book");
        assert_eq!(book.author, "Writer");
        assert_eq!(book.remote_id.as_deref(), Some("remote-7"));
        assert_eq!(
            book.download_url,
            "https://example.org/book.EPUB?file=bad.pdf"
        );
        assert_eq!(book.format, "epub");
        assert!(
            parse("<article class=title>Self only</article>")
                .results
                .is_empty()
        );
    }
    #[test]
    fn bounded_selector_subset_and_optional_fields() {
        let mut m = manifest().html.unwrap();
        for bad in [
            "",
            " ",
            "[",
            "article:has(a)",
            "a:not(.x)",
            r"a\3a has(x)",
            "a,b",
            "a + b",
        ] {
            m.item_selector = bad.into();
            assert!(validate_mapping(&m).is_err(), "{bad}");
        }
        m.item_selector = "x".repeat(201);
        assert!(validate_mapping(&m).is_err());
        m.item_selector = "section > article.book[data-kind='book']".into();
        m.author_selector.clear();
        m.download_selector.clear();
        m.next_selector.clear();
        assert!(validate_mapping(&m).is_ok());
        m.title_selector.clear();
        assert!(validate_mapping(&m).is_err());
    }
    #[test]
    fn hostile_links_and_domain_permissions() {
        for href in [
            "javascript:alert(1)",
            "data:text/plain,book.pdf",
            "file:///book.pdf",
            "http://example.org/book.pdf",
            "https://example.org@evil.org/book.pdf",
            "https://files.example.org.evil.org/book.pdf",
            "https://127.0.0.1/book.pdf",
            "https://evil.org/book.pdf",
            "/get?file=book.pdf",
            "/book.txt#book.pdf",
        ] {
            let page = parse(&format!(
                "<article><b class=title>Book</b><a class=download href='{href}'>PDF</a></article>"
            ));
            assert!(page.results[0].download_url.is_empty(), "{href}");
            assert!(page.results[0].format.is_empty());
        }
        let body = "<article><b class=title>Book</b><div class=download href='/book.pdf'></div><a class=download href='https://files.example.org/book.pdf'>PDF</a></article>";
        assert_eq!(parse(body).results[0].format, "pdf");
        let mut m = manifest();
        m.download_domains = Some(vec![]);
        let url = Url::parse("https://example.org/").unwrap();
        assert!(
            parse_html(&m, body, &url, &request()).unwrap().results[0]
                .download_url
                .is_empty()
        );
        m.download_domains = None;
        assert!(
            parse_html(&m, body, &url, &request()).unwrap().results[0]
                .download_url
                .is_empty()
        );
    }
    #[test]
    fn exact_filters_and_metadata_only_rows() {
        let body = "<article><b class=title>Known</b><span class=language>pt-BR</span><a class=download href='/a.pdf'>PDF</a><a class=download href='/a.epub'>EPUB</a></article><article><b class=title>Unknown</b></article>";
        let url = Url::parse("https://example.org/").unwrap();
        let mut req = request();
        assert_eq!(parse(body).results.len(), 2);
        req.language = "pt".into();
        assert!(
            parse_html(&manifest(), body, &url, &req)
                .unwrap()
                .results
                .is_empty()
        );
        req.language = "pt-BR".into();
        req.format = "epub".into();
        let page = parse_html(&manifest(), body, &url, &req).unwrap();
        assert_eq!(page.results.len(), 1);
        assert_eq!(page.results[0].format, "epub");
        req.format = "EPUB".into();
        assert_eq!(
            parse_html(&manifest(), body, &url, &req)
                .unwrap()
                .results
                .len(),
            1
        );
        req.language.clear();
        req.format = "mobi".into();
        assert!(
            parse_html(&manifest(), body, &url, &req)
                .unwrap()
                .results
                .is_empty()
        );
    }
    #[test]
    fn next_is_same_origin_anchor_and_does_not_repeat() {
        for href in [
            "/catalog/page",
            "#top",
            "/next#top",
            "https://evil.org/next",
            "http://example.org/next",
            "https://example.org:8443/next",
            "javascript:alert(1)",
        ] {
            assert!(
                parse(&format!("<a class=next href='{href}'>Next</a>"))
                    .next_cursor
                    .is_none(),
                "{href}"
            );
        }
        assert!(
            parse("<div class=next href='/next'>Next</div>")
                .next_cursor
                .is_none()
        );
        assert_eq!(
            parse("<a class=next href='?page=2'>Next</a>")
                .next_cursor
                .as_deref(),
            Some("https://example.org/catalog/page?page=2")
        );
        let mut req = request();
        req.cursor = Some("https://example.org/previous".into());
        assert!(
            parse_html(
                &manifest(),
                "<a class=next href='/previous'>Next</a>",
                &Url::parse("https://example.org/current").unwrap(),
                &req
            )
            .unwrap()
            .next_cursor
            .is_none()
        );
    }
    #[test]
    fn supported_formats_come_only_from_path() {
        for extension in ["pdf", "epub", "mobi", "azw3", "djvu"] {
            let page = parse(&format!(
                "<article><b class=title>Book</b><a class=download href='/book.{extension}?format=exe#other.zip'>file</a></article>"
            ));
            assert_eq!(page.results[0].format, extension);
        }
    }

    #[test]
    fn missing_mapping_and_invalid_request_fail() {
        let url = Url::parse("https://example.org/").unwrap();
        let mut m = manifest();
        m.html = None;
        assert_eq!(
            parse_html(&m, "", &url, &request()).unwrap_err().code,
            SourceErrorCode::InvalidResponse
        );
        let mut req = request();
        req.query.clear();
        assert_eq!(
            parse_html(&manifest(), "", &url, &req).unwrap_err().code,
            SourceErrorCode::InvalidRequest
        );
    }

    #[test]
    fn limits_body_tree_rows_and_metadata() {
        let url = Url::parse("https://example.org/").unwrap();
        assert!(parse_html(&manifest(), &"x".repeat(MAX_BODY + 1), &url, &request()).is_err());
        assert!(parse_html(&manifest(), &"<i></i>".repeat(MAX_NODES), &url, &request()).is_err());
        assert_eq!(
            parse(&"<article><b class=title>Book</b></article>".repeat(50))
                .results
                .len(),
            40
        );
        let body = format!(
            "{}<article><b class=title>Late</b></article>",
            "<article></article>".repeat(MAX_SCAN)
        );
        assert!(parse(&body).results.is_empty());
        let body = format!(
            "<article><b class=title>{}</b><span class=author><script>evil()</script>Writer</span></article>",
            "é".repeat(1000)
        );
        let page = parse(&body);
        assert!(page.results[0].title.len() <= MAX_TEXT);
        assert_eq!(page.results[0].author, "Writer");
    }
}
