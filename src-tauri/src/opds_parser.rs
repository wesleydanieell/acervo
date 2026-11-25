//! Offline OPDS parsing. Entries without an eligible direct download are skipped,
//! including unfiltered searches; metadata alone never implies download permission.
use crate::extension_api::{SearchPage, SearchRequest, SourceBook, SourceError, SourceErrorCode};
use crate::sources::SourceManifest;
use serde_json::Value;
use url::Url;

const ATOM: &str = "http://www.w3.org/2005/Atom";
const ACQUISITION: &str = "http://opds-spec.org/acquisition";
const MAX_BODY: usize = 2 * 1024 * 1024;
const MAX_NODES: u32 = 20_000;
const MAX_TEXT: usize = 1024;

fn invalid() -> SourceError {
    SourceError {
        code: SourceErrorCode::InvalidResponse,
        message: "Catálogo OPDS inválido ou excessivo.".into(),
    }
}
fn bounded(s: &str) -> String {
    s.trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(MAX_TEXT)
        .collect()
}
fn format(mime: &str) -> Option<&'static str> {
    match mime.split(';').next()?.trim().to_ascii_lowercase().as_str() {
        "application/epub+zip" => Some("epub"),
        "application/pdf" => Some("pdf"),
        "application/x-mobipocket-ebook" => Some("mobi"),
        "application/vnd.amazon.mobi8-ebook" | "application/x-mobi8-ebook" => Some("azw3"),
        "image/vnd.djvu" | "image/x-djvu" => Some("djvu"),
        _ => None,
    }
}
fn acquisition(rel: &str) -> bool {
    rel == ACQUISITION || rel == "http://opds-spec.org/acquisition/open-access"
}
fn json_acquisition(rel: &str) -> bool {
    acquisition(rel) || matches!(rel, "download" | "acquisition")
}
fn forbidden_json_relation(rel: &str) -> bool {
    (rel.starts_with(ACQUISITION) && !acquisition(rel))
        || matches!(rel, "borrow" | "buy" | "preview" | "sample" | "subscribe")
}
fn resolved(base: &Url, href: &str) -> Option<Url> {
    if href.is_empty() || href.chars().any(char::is_control) {
        return None;
    }
    let url = base.join(href).ok()?;
    crate::extension_network::safe_url(url.as_str()).then_some(url)
}
fn next(base: &Url, href: &str) -> Option<String> {
    let url = resolved(base, href)?;
    (url.origin() == base.origin() && url.fragment().is_none()).then(|| url.into())
}
struct Edition<'a> {
    title: &'a str,
    author: String,
    languages: Vec<&'a str>,
    id: Option<&'a str>,
    cover_url: Option<String>,
}
fn emit(
    page: &mut SearchPage,
    manifest: &SourceManifest,
    request: &SearchRequest,
    base: &Url,
    entry: &Edition<'_>,
    href: &str,
    mime: &str,
) {
    if page.results.len() >= 40 || entry.title.trim().is_empty() {
        return;
    }
    let Some(fmt) = format(mime) else {
        return;
    };
    if !request.format.is_empty() && !request.format.eq_ignore_ascii_case(fmt) {
        return;
    }
    let language = if request.language.is_empty() {
        entry.languages.first().copied().unwrap_or("")
    } else {
        let Some(lang) = entry
            .languages
            .iter()
            .copied()
            .find(|l| l.eq_ignore_ascii_case(&request.language))
        else {
            return;
        };
        lang
    };
    let Some(url) = resolved(base, href) else {
        return;
    };
    if !crate::sources::permits_download(manifest, url.as_str()) {
        return;
    }
    page.results.push(SourceBook {
        source_id: manifest.id.clone(),
        source_name: manifest.name.clone(),
        title: bounded(entry.title),
        author: bounded(&entry.author),
        format: fmt.into(),
        language: bounded(language),
        download_url: url.into(),
        cover_url: entry.cover_url.clone(),
        acquisition_page: None,
        publisher: String::new(),
        edition_label: String::new(),
        // IDs are opaque: reject oversized values rather than truncate their identity.
        remote_id: entry
            .id
            .filter(|id| id.len() <= MAX_TEXT && !id.chars().any(char::is_control))
            .map(str::to_owned),
    });
}
fn atom(node: roxmltree::Node<'_, '_>, name: &str) -> bool {
    node.has_tag_name((ATOM, name))
}
fn text<'a>(node: roxmltree::Node<'a, 'a>, name: &str) -> &'a str {
    node.children()
        .find(|n| atom(*n, name))
        .and_then(|n| n.text())
        .unwrap_or("")
}
fn string<'a>(v: &'a Value, key: &str) -> &'a str {
    v.get(key).and_then(Value::as_str).unwrap_or("")
}
fn rels(v: &Value) -> Vec<&str> {
    match v.get("rel") {
        Some(Value::String(s)) => vec![s],
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
        _ => vec![],
    }
}

pub(crate) fn parse_feed(
    manifest: &SourceManifest,
    body: &str,
    feed_url: &Url,
    request: &SearchRequest,
) -> Result<SearchPage, SourceError> {
    if body.len() > MAX_BODY || !crate::extension_network::safe_url(feed_url.as_str()) {
        return Err(invalid());
    }
    let mut page = SearchPage {
        results: vec![],
        next_cursor: None,
    };
    if body.trim_start().starts_with('<') {
        let doc = roxmltree::Document::parse_with_options(
            body,
            roxmltree::ParsingOptions {
                allow_dtd: false,
                nodes_limit: MAX_NODES,
                ..Default::default()
            },
        )
        .map_err(|_| invalid())?;
        if !atom(doc.root_element(), "feed")
            || doc.descendants().any(|n| {
                n.attribute(("http://www.w3.org/XML/1998/namespace", "base"))
                    .is_some()
            })
        {
            return Err(invalid());
        }
        for node in doc.root_element().children() {
            if atom(node, "link")
                && node.attribute("rel") == Some("next")
                && page.next_cursor.is_none()
            {
                page.next_cursor = node.attribute("href").and_then(|s| next(feed_url, s));
            }
            if !atom(node, "entry") {
                continue;
            }
            let entry = Edition {
                cover_url: node
                    .children()
                    .filter(|n| {
                        atom(*n, "link")
                            && matches!(
                                n.attribute("rel"),
                                Some(
                                    "http://opds-spec.org/image/thumbnail"
                                        | "http://opds-spec.org/image"
                                )
                            )
                    })
                    .find_map(|n| {
                        crate::source_covers::resolve_cover(
                            manifest,
                            feed_url,
                            n.attribute("href")?,
                        )
                    }),
                title: text(node, "title"),
                author: node
                    .children()
                    .filter(|n| atom(*n, "author"))
                    .map(|n| bounded(text(n, "name")))
                    .collect::<Vec<_>>()
                    .join(", "),
                languages: node
                    .children()
                    .filter(|n| {
                        n.has_tag_name(("http://purl.org/dc/terms/", "language"))
                            || n.has_tag_name(("http://purl.org/dc/elements/1.1/", "language"))
                    })
                    .filter_map(|n| n.text())
                    .collect(),
                id: node
                    .children()
                    .find(|n| atom(*n, "id"))
                    .and_then(|n| n.text()),
            };
            for link in node.children().filter(|n| atom(*n, "link")) {
                // Nested acquisition descriptors signify indirect acquisition.
                if !acquisition(link.attribute("rel").unwrap_or(""))
                    || link.children().any(|n| n.is_element())
                {
                    continue;
                }
                emit(
                    &mut page,
                    manifest,
                    request,
                    feed_url,
                    &entry,
                    link.attribute("href").unwrap_or(""),
                    link.attribute("type").unwrap_or(""),
                );
            }
        }
    } else {
        let doc: Value = serde_json::from_str(body).map_err(|_| invalid())?;
        let publications = doc
            .get("publications")
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if let Some(links) = doc.get("links").and_then(Value::as_array) {
            page.next_cursor = links
                .iter()
                .filter(|l| rels(l).contains(&"next"))
                .find_map(|l| next(feed_url, string(l, "href")));
        }
        for publication in publications {
            let Some(meta) = publication.get("metadata") else {
                continue;
            };
            let languages = match meta.get("language") {
                Some(Value::String(s)) => vec![s.as_str()],
                Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).collect(),
                _ => vec![],
            };
            let authors: Vec<&Value> = match meta.get("author") {
                Some(Value::Array(a)) => a.iter().collect(),
                Some(a) => vec![a],
                None => vec![],
            };
            let entry = Edition {
                cover_url: publication
                    .get("images")
                    .and_then(Value::as_array)
                    .and_then(|images| {
                        images.iter().find_map(|image| {
                            crate::source_covers::resolve_cover(
                                manifest,
                                feed_url,
                                string(image, "href"),
                            )
                        })
                    }),
                title: string(meta, "title"),
                languages,
                author: authors
                    .iter()
                    .map(|a| bounded(a.as_str().unwrap_or_else(|| string(a, "name"))))
                    .collect::<Vec<_>>()
                    .join(", "),
                id: meta.get("identifier").and_then(Value::as_str),
            };
            if let Some(links) = publication.get("links").and_then(Value::as_array) {
                for link in links {
                    let relations = rels(link);
                    if !relations.iter().any(|r| json_acquisition(r))
                        || relations.iter().any(|r| forbidden_json_relation(r))
                    {
                        continue;
                    }
                    // Fail closed on indirect or encryption declarations, even empty ones.
                    if link.get("properties").is_some_and(|p| {
                        p.get("indirectAcquisition").is_some()
                            || p.get("encrypted").is_some()
                            || p.get("encryption").is_some()
                            || p.get("price").is_some()
                    }) || link.get("indirectAcquisition").is_some()
                    {
                        continue;
                    }
                    emit(
                        &mut page,
                        manifest,
                        request,
                        feed_url,
                        &entry,
                        string(link, "href"),
                        string(link, "type"),
                    );
                }
            }
        }
    }
    Ok(page)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn manifest() -> SourceManifest {
        serde_json::from_value(json!({"schemaVersion":1,"id":"test-opds","name":"Catalog",
            "adapter":"opds","baseUrl":"https://example.org","searchPath":"/search?q={query}",
            "downloadDomains":["example.org"]}))
        .unwrap()
    }
    fn request() -> SearchRequest {
        SearchRequest {
            query: "books".into(),
            language: String::new(),
            format: String::new(),
            cursor: None,
        }
    }
    fn parse(body: &str) -> Result<SearchPage, SourceError> {
        parse_feed(
            &manifest(),
            body,
            &Url::parse("https://example.org/catalog/feed").unwrap(),
            &request(),
        )
    }
    fn publication() -> Value {
        json!({"metadata":{"title":"Book","identifier":"urn:book:1","author":[{"name":"Writer"}],"language":["en","pt-BR"]},"links":[{"rel":ACQUISITION,"href":"../book.epub","type":"application/epub+zip"},{"rel":format!("{ACQUISITION}/open-access"),"href":"/book.pdf","type":"application/pdf"}]})
    }
    fn xml(links: &str) -> String {
        format!(
            r#"<feed xmlns="{ATOM}" xmlns:dc="http://purl.org/dc/terms/"><link rel="next" href="?page=2"/><entry><id>urn:book:1</id><title>Book</title><author><name>Writer</name></author><dc:language>en</dc:language>{links}</entry></feed>"#
        )
    }
    #[test]
    fn json_editions_relative_links_metadata_and_next() {
        let page = parse(
            &json!({"publications":[publication()],"links":[{"rel":["next"],"href":"?page=2"}]})
                .to_string(),
        )
        .unwrap();
        assert_eq!(page.results.len(), 2);
        assert_eq!(
            page.results[0].download_url,
            "https://example.org/book.epub"
        );
        assert_eq!(page.results[0].remote_id.as_deref(), Some("urn:book:1"));
        assert_eq!(page.results[0].author, "Writer");
        assert_eq!(
            page.next_cursor.as_deref(),
            Some("https://example.org/catalog/feed?page=2")
        );
    }
    #[test]
    fn opds2_standard_aliases_and_conflicting_transaction_relations() {
        for rel in ["download", "acquisition"] {
            let mut p = publication();
            p["links"].as_array_mut().unwrap().truncate(1);
            p["links"][0]["rel"] = json!(rel);
            assert_eq!(
                parse(&json!({"publications":[p.clone()]}).to_string())
                    .unwrap()
                    .results
                    .len(),
                1
            );
            p["links"][0]["rel"] = json!([rel, "buy"]);
            assert!(
                parse(&json!({"publications":[p.clone()]}).to_string())
                    .unwrap()
                    .results
                    .is_empty()
            );
            p["links"][0]["rel"] = json!(rel);
            p["links"][0]["properties"] = json!({"price":{"value":1,"currency":"USD"}});
            assert!(
                parse(&json!({"publications":[p]}).to_string())
                    .unwrap()
                    .results
                    .is_empty()
            );
        }
    }
    #[test]
    fn atom_namespace_editions_and_relative_links() {
        let page = parse(&xml(&format!(r#"<link rel="{ACQUISITION}" href="../book.epub" type="application/epub+zip"/><link rel="{ACQUISITION}/open-access" href="/book.pdf" type="application/pdf"/>"#))).unwrap();
        assert_eq!(page.results.len(), 2);
        assert_eq!(page.results[0].language, "en");
        assert_eq!(page.results[0].remote_id.as_deref(), Some("urn:book:1"));
        assert!(parse("<feed><entry/></feed>").is_err());
    }
    #[test]
    fn unsupported_relations_mime_and_indirect_are_skipped() {
        for suffix in [
            "/borrow",
            "/buy",
            "/sample",
            "/subscribe",
            "/open-access/extra",
        ] {
            let link =
                format!(r#"<link rel="{ACQUISITION}{suffix}" href="/a" type="application/pdf"/>"#);
            assert!(parse(&xml(&link)).unwrap().results.is_empty());
            let mut p = publication();
            p["links"][0]["rel"] = json!(format!("{ACQUISITION}{suffix}"));
            p["links"].as_array_mut().unwrap().truncate(1);
            assert!(
                parse(&json!({"publications":[p]}).to_string())
                    .unwrap()
                    .results
                    .is_empty()
            );
        }
        for extra in [
            json!({"indirectAcquisition":[]}),
            json!({"encrypted":{}}),
            json!({"encryption":{}}),
        ] {
            let mut p = publication();
            p["links"].as_array_mut().unwrap().truncate(1);
            p["links"][0]["properties"] = extra;
            assert!(
                parse(&json!({"publications":[p]}).to_string())
                    .unwrap()
                    .results
                    .is_empty()
            );
        }
        assert!(parse(&xml(&format!(r#"<link rel="{ACQUISITION}" href="/a" type="application/pdf"><indirectAcquisition xmlns="http://opds-spec.org/2010/catalog"/></link>"#))).unwrap().results.is_empty());
        for mime in [
            "application/zip",
            "application/vnd.adobe.adept+xml",
            "text/html",
        ] {
            assert!(format(mime).is_none());
        }
        for mime in [
            "application/epub+zip",
            "application/pdf",
            "application/x-mobipocket-ebook",
            "application/vnd.amazon.mobi8-ebook",
            "image/vnd.djvu",
        ] {
            assert!(format(mime).is_some());
        }
        assert!(parse(&xml("")).unwrap().results.is_empty());
    }
    #[test]
    fn download_permissions_and_safe_urls() {
        for href in [
            "https://evil.org/a",
            "http://example.org/a",
            "https://127.0.0.1/a",
            "https://user:pass@example.org/a",
            "https://example.org:8443/a",
            "https://foo.local/a",
        ] {
            let mut p = publication();
            p["links"].as_array_mut().unwrap().truncate(1);
            p["links"][0]["href"] = json!(href);
            assert!(
                parse(&json!({"publications":[p]}).to_string())
                    .unwrap()
                    .results
                    .is_empty(),
                "{href}"
            );
        }
        let mut m = manifest();
        m.download_domains = Some(vec![]);
        assert!(
            parse_feed(
                &m,
                &json!({"publications":[publication()]}).to_string(),
                &Url::parse("https://example.org/").unwrap(),
                &request()
            )
            .unwrap()
            .results
            .is_empty()
        );
    }
    #[test]
    fn filters_both_variants() {
        for body in [
            json!({"publications":[publication()]}).to_string(),
            xml(&format!(
                r#"<link rel="{ACQUISITION}" href="/a" type="application/pdf"/>"#
            )),
        ] {
            let mut req = request();
            req.format = "PDF".into();
            req.language = "EN".into();
            let run = |r: &SearchRequest| {
                parse_feed(
                    &manifest(),
                    &body,
                    &Url::parse("https://example.org/").unwrap(),
                    r,
                )
                .unwrap()
            };
            assert_eq!(run(&req).results.len(), 1);
            req.language = "fr".into();
            assert!(run(&req).results.is_empty());
            req.language.clear();
            req.format = "djvu".into();
            assert!(run(&req).results.is_empty());
        }
    }
    #[test]
    fn hostile_xml_and_malformed_feeds() {
        for body in [
            "<!DOCTYPE feed [<!ENTITY x 'bad'>]><feed/>",
            "<feed",
            "{}",
            "[]",
            "{bad",
            "<html/>",
        ] {
            assert!(parse(body).is_err());
        }
        assert!(
            parse(&format!(
                r#"<feed xmlns="{ATOM}" xml:base="https://evil.org/"/>"#
            ))
            .is_err()
        );
        assert!(
            parse(&format!(
                "<feed xmlns='{ATOM}'>{}</feed>",
                "<x/>".repeat(MAX_NODES as usize)
            ))
            .is_err()
        );
        assert!(parse(&" ".repeat(MAX_BODY + 1)).is_err());
    }
    #[test]
    fn hostile_next_links_both_variants() {
        for href in [
            "https://evil.org/next",
            "//evil.org/next",
            "http://example.org/next",
            "https://example.org:8443/next",
            "/next#fragment",
            "https://127.0.0.1/",
            "https://user@example.org/next",
        ] {
            assert!(
                parse(&json!({"publications":[],"links":[{"rel":"next","href":href}]}).to_string())
                    .unwrap()
                    .next_cursor
                    .is_none()
            );
            assert!(
                parse(&format!(
                    r#"<feed xmlns="{ATOM}"><link rel="next" href="{href}"/></feed>"#
                ))
                .unwrap()
                .next_cursor
                .is_none()
            );
        }
    }
    #[test]
    fn bounds_results_metadata_and_preserves_opaque_ids() {
        let mut p = publication();
        p["metadata"]["title"] = json!("é".repeat(3000));
        let page = parse(&json!({"publications":vec![p; 50]}).to_string()).unwrap();
        assert_eq!(page.results.len(), 40);
        assert_eq!(page.results[0].title.chars().count(), MAX_TEXT);
        assert_eq!(page.results[0].remote_id.as_deref(), Some("urn:book:1"));
    }
}
