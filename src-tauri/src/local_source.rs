//! Small, user-imported bibliographic snapshots. No networking or acquisition links.
use crate::extension_api::{
    API_VERSION, BookSource, SearchPage, SearchRequest, SourceBook, SourceCapabilities,
    SourceDescriptor, SourceError, SourceErrorCode,
};
use crate::sources::SourceManifest;
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, future::Future, pin::Pin};

const MAX_RECORDS: usize = 200;
const MAX_BYTES: usize = 60 * 1024;
const PAGE: usize = 40;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LocalRecord {
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub format: String,
    #[serde(default)]
    pub edition_label: String,
}

pub(crate) fn validate_manifest(m: &SourceManifest) -> Result<(), String> {
    if [
        &m.base_url,
        &m.search_path,
        &m.items_path,
        &m.title_field,
        &m.author_field,
        &m.format_field,
        &m.language_field,
        &m.download_url_field,
        &m.remote_id_field,
    ]
    .iter()
    .any(|s| !s.is_empty())
        || m.download_domains.as_ref().is_none_or(|d| !d.is_empty())
        || m.html.is_some()
        || m.editions.is_some()
        || m.pagination.is_some()
        || m.local_search
        || !m.formats.is_empty()
        || !m.languages.is_empty()
    {
        return Err(
            "Catálogo local não aceita rede, seletores, downloads ou paginação configurável."
                .into(),
        );
    }
    let records = m
        .local_records
        .as_ref()
        .ok_or("localRecords obrigatório.")?;
    if records.is_empty()
        || records.len() > MAX_RECORDS
        || serde_json::to_vec(m).map_err(|e| e.to_string())?.len() > MAX_BYTES
    {
        return Err("Catálogo local deve ter 1–200 registros e até 60 KiB.".into());
    }
    let mut ids = HashSet::new();
    for record in records {
        if record.id.trim().is_empty()
            || record.id.len() > 200
            || !ids.insert(&record.id)
            || record.title.trim().is_empty()
            || [
                &record.title,
                &record.author,
                &record.publisher,
                &record.edition_label,
            ]
            .iter()
            .any(|v| v.len() > 500 || v.chars().any(char::is_control))
            || record.id.chars().any(char::is_control)
            || record.language.len() > 25
            || !record
                .language
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-')
            || !matches!(
                record.format.as_str(),
                "" | "epub" | "pdf" | "mobi" | "azw3" | "djvu"
            )
        {
            return Err("Registro bibliográfico inválido ou ID duplicado.".into());
        }
    }
    Ok(())
}

pub(crate) struct LocalSource {
    manifest: SourceManifest,
}
impl LocalSource {
    pub(crate) fn new(manifest: SourceManifest) -> Self {
        Self { manifest }
    }
}
fn invalid(message: &str) -> SourceError {
    SourceError {
        code: SourceErrorCode::InvalidRequest,
        message: message.into(),
    }
}
impl BookSource for LocalSource {
    fn descriptor(&self) -> SourceDescriptor {
        SourceDescriptor {
            id: self.manifest.id.clone(),
            name: self.manifest.name.clone(),
            api_version: API_VERSION,
            capabilities: SourceCapabilities {
                search: true,
                download: false,
                pagination: true,
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
            if !request.query.chars().any(char::is_alphanumeric) {
                return Err(invalid("Informe um título ou autor."));
            }
            let offset = match request.cursor.as_deref() {
                None => 0,
                Some(value) => value
                    .parse::<usize>()
                    .ok()
                    .filter(|n| *n > 0 && *n < MAX_RECORDS && *n % PAGE == 0)
                    .ok_or_else(|| invalid("Cursor local inválido."))?,
            };
            let records = self.manifest.local_records.as_deref().unwrap_or_default();
            let mut matches = records
                .iter()
                .filter(|r| {
                    (request.language.is_empty()
                        || r.language.eq_ignore_ascii_case(&request.language))
                        && (request.format.is_empty()
                            || r.format.eq_ignore_ascii_case(&request.format))
                        && crate::source_search::matches_local_query(
                            &r.title,
                            &r.author,
                            &request.query,
                        )
                })
                .skip(offset);
            let results = matches
                .by_ref()
                .take(PAGE)
                .map(|r| SourceBook {
                    source_id: self.manifest.id.clone(),
                    source_name: self.manifest.name.clone(),
                    title: r.title.clone(),
                    author: r.author.clone(),
                    format: r.format.clone(),
                    language: r.language.clone(),
                    download_url: String::new(),
                    remote_id: Some(r.id.clone()),
                    publisher: r.publisher.clone(),
                    edition_label: r.edition_label.clone(),
                })
                .collect();
            Ok(SearchPage {
                results,
                next_cursor: matches.next().map(|_| (offset + PAGE).to_string()),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn manifest() -> SourceManifest {
        serde_json::from_value(json!({"schemaVersion":1,"adapter":"local","id":"offline-books","name":"Local fixture",
            "downloadDomains":[],"localRecords":[{"id":"a","title":"Memórias Póstumas","author":"Machado de Assis","language":"pt","format":"epub","publisher":"Fixture"}]})).unwrap()
    }
    #[test]
    fn offline_source_filters_and_never_exposes_downloads() {
        let m = manifest();
        let s = crate::extension_host::create_source(m.clone()).unwrap();
        assert!(!s.descriptor().capabilities.download);
        assert!(!crate::sources::permits_download(
            &m,
            "https://example.org/book.epub"
        ));
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut req = SearchRequest {
            query: "memorias machado".into(),
            language: "PT".into(),
            format: "EPUB".into(),
            cursor: None,
        };
        let page = rt.block_on(s.search(req.clone())).unwrap();
        assert_eq!(page.results.len(), 1);
        assert!(page.results[0].download_url.is_empty());
        req.language = "en".into();
        assert!(
            rt.block_on(s.search(req.clone()))
                .unwrap()
                .results
                .is_empty()
        );
        req.query = "??".into();
        assert!(rt.block_on(s.search(req)).is_err());
    }
    #[test]
    fn validates_exclusive_offline_data_and_paginates() {
        let mut m = manifest();
        m.base_url = "https://example.org".into();
        assert!(crate::sources::validate(&m).is_err());
        m.base_url.clear();
        m.download_domains = Some(vec!["example.org".into()]);
        assert!(crate::sources::validate(&m).is_err());
        m.download_domains = Some(vec![]);
        let original = m.local_records.as_ref().unwrap()[0].clone();
        let records = m.local_records.as_mut().unwrap();
        records.push(original.clone());
        assert!(crate::sources::validate(&m).is_err());
        m.local_records = Some(
            (0..81)
                .map(|i| LocalRecord {
                    id: i.to_string(),
                    ..original.clone()
                })
                .collect(),
        );
        let s = crate::extension_host::create_source(m).unwrap();
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let mut req = SearchRequest {
            query: "machado".into(),
            language: "".into(),
            format: "".into(),
            cursor: None,
        };
        for (size, next) in [(40, Some("40")), (40, Some("80")), (1, None)] {
            let page = rt.block_on(s.search(req.clone())).unwrap();
            assert_eq!(page.results.len(), size);
            assert_eq!(page.next_cursor.as_deref(), next);
            req.cursor = page.next_cursor;
        }
        req.cursor = Some("https://example.org".into());
        assert!(rt.block_on(s.search(req)).is_err());
    }
}
