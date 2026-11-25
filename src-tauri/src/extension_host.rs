//! Extension dispatch and compatibility boundary. UI never calls adapters directly.
use super::extension_api::{
    BookSource, SearchRequest, SourceBook, SourceDescriptor, SourceError, SourceErrorCode,
};
use super::opds_source::OpdsSource;
use super::source_search::JsonSource;
use super::sources::{self, AdapterKind, SourceManifest};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchOutcome {
    results: Vec<SourceBook>,
    errors: Vec<String>,
    next_pages: BTreeMap<String, String>,
}

pub(crate) fn create_source(manifest: SourceManifest) -> Result<Box<dyn BookSource>, SourceError> {
    sources::validate(&manifest).map_err(|message| SourceError {
        code: SourceErrorCode::InvalidRequest,
        message,
    })?;
    match manifest.adapter {
        AdapterKind::Json => Ok(Box::new(JsonSource::new(manifest))),
        AdapterKind::Opds => Ok(Box::new(OpdsSource::new(manifest))),
    }
}

#[tauri::command]
pub fn list_source_descriptors(app: tauri::AppHandle) -> Result<Vec<SourceDescriptor>, String> {
    sources::list_sources(app)?
        .into_iter()
        .map(|installed| {
            create_source(installed.manifest)
                .map(|source| source.descriptor())
                .map_err(|error| error.message)
        })
        .collect()
}

async fn search_adapters(
    adapters: Vec<Box<dyn BookSource>>,
    request: SearchRequest,
) -> SearchOutcome {
    let mut jobs = tokio::task::JoinSet::new();
    for adapter in adapters {
        let request = request.clone();
        jobs.spawn(async move {
            let descriptor = adapter.descriptor();
            (
                descriptor.id,
                descriptor.name,
                adapter.search(request).await,
            )
        });
    }
    let mut outcome = SearchOutcome {
        results: vec![],
        errors: vec![],
        next_pages: BTreeMap::new(),
    };
    while let Some(done) = jobs.join_next().await {
        match done {
            Ok((id, _, Ok(page))) => {
                outcome
                    .results
                    .extend(page.results.into_iter().filter(|book| book.source_id == id));
                if let Some(cursor) = page.next_cursor {
                    outcome.next_pages.insert(id, cursor);
                }
            }
            Ok((_, name, Err(error))) => outcome.errors.push(format!("{name}: {}", error.message)),
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
    outcome.errors.sort();
    outcome
}

/// Existing frontend command: translate the legacy flat response into API v1.
#[tauri::command]
pub async fn search_sources(
    app: tauri::AppHandle,
    query: String,
    language: String,
    format: String,
    source_ids: Vec<String>,
    cursor: Option<String>,
) -> Result<SearchOutcome, String> {
    let request = SearchRequest {
        query,
        language,
        format,
        cursor,
    };
    if request.cursor.is_some() && source_ids.len() != 1 {
        return Err("Paginação exige uma única fonte.".into());
    }
    request.validate().map_err(|error| error.message)?;
    let installed = sources::list_sources(app)?
        .into_iter()
        .filter(|source| {
            source.enabled && (source_ids.is_empty() || source_ids.contains(&source.manifest.id))
        })
        .collect::<Vec<_>>();
    if installed.len() > 12 {
        return Err("Selecione no máximo 12 fontes por pesquisa.".into());
    }
    if installed.is_empty() {
        return Ok(SearchOutcome {
            results: vec![],
            errors: vec!["Nenhuma fonte ativa selecionada.".into()],
            next_pages: BTreeMap::new(),
        });
    }
    let adapters = installed
        .into_iter()
        .map(|source| create_source(source.manifest).map_err(|error| error.message))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(search_adapters(adapters, request).await)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::extension_api::{API_VERSION, SearchPage, SourceCapabilities};
    use std::{future::Future, pin::Pin};

    struct FakeSource {
        name: &'static str,
        fail: bool,
    }

    impl BookSource for FakeSource {
        fn descriptor(&self) -> SourceDescriptor {
            SourceDescriptor {
                id: self.name.into(),
                name: self.name.into(),
                api_version: API_VERSION,
                capabilities: SourceCapabilities {
                    search: true,
                    download: false,
                    pagination: false,
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
                if self.fail {
                    return Err(SourceError {
                        code: SourceErrorCode::Network,
                        message: "offline".into(),
                    });
                }
                Ok(SearchPage {
                    results: vec![SourceBook {
                        source_id: self.name.into(),
                        source_name: self.name.into(),
                        title: request.query,
                        author: String::new(),
                        format: String::new(),
                        language: request.language,
                        download_url: String::new(),
                        remote_id: None,
                    }],
                    next_cursor: (self.name == "a").then(|| "2".into()),
                })
            })
        }
    }

    #[test]
    fn host_combines_adapters_and_isolates_provider_failures() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let sources: Vec<Box<dyn BookSource>> = vec![
            Box::new(FakeSource {
                name: "z",
                fail: false,
            }),
            Box::new(FakeSource {
                name: "broken",
                fail: true,
            }),
            Box::new(FakeSource {
                name: "a",
                fail: false,
            }),
        ];
        let response = runtime.block_on(search_adapters(
            sources,
            SearchRequest {
                query: "Books".into(),
                language: "pt".into(),
                format: "".into(),
                cursor: None,
            },
        ));
        assert_eq!(
            response
                .results
                .iter()
                .map(|b| b.source_id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "z"]
        );
        assert_eq!(response.errors, vec!["broken: offline"]);
        assert_eq!(response.next_pages.get("a").map(String::as_str), Some("2"));
        assert_eq!(response.next_pages.len(), 1);
        let data = serde_json::to_value(response).unwrap();
        assert_eq!(data["results"][0]["sourceId"], "a");
        assert!(data["results"][0].get("downloadUrl").is_some());
    }

    #[test]
    fn shipped_sources_follow_same_api_without_network() {
        for raw in [
            include_str!("../../integrations/test-source/manifest.json"),
            include_str!("../../integrations/gutenberg/manifest.json"),
            include_str!("../../sdk/extensions/manifest.example.json"),
        ] {
            let manifest: SourceManifest = serde_json::from_str(raw).unwrap();
            let source = create_source(manifest).unwrap();
            let descriptor = source.descriptor();
            assert_eq!(descriptor.api_version, API_VERSION);
            assert!(descriptor.capabilities.search && descriptor.capabilities.download);
            assert!(!descriptor.capabilities.pagination && !descriptor.capabilities.details);
            let runtime = tokio::runtime::Builder::new_current_thread()
                .build()
                .unwrap();
            let result = runtime.block_on(source.search(SearchRequest {
                query: "Books".into(),
                language: "".into(),
                format: "".into(),
                cursor: Some("page-2".into()),
            }));
            assert_eq!(result.unwrap_err().code, SourceErrorCode::Unsupported);
        }
    }
}
