//! Internal source API v1. This module performs no I/O or permission enforcement.
use serde::{Deserialize, Serialize};
use std::{future::Future, pin::Pin};

pub const API_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRequest {
    pub query: String,
    pub language: String,
    pub format: String,
    pub cursor: Option<String>,
}

impl SearchRequest {
    /// Validate shared request syntax without modifying the caller's values.
    /// Empty language/format mean no filter; their vocabulary is source-specific.
    pub fn validate(&self) -> Result<(), SourceError> {
        self.validate_query(false)
    }
    pub(crate) fn validate_feed(&self) -> Result<(), SourceError> {
        self.validate_query(true)
    }
    fn validate_query(&self, feed: bool) -> Result<(), SourceError> {
        if (if feed {
            !self.query.is_empty() || !self.language.is_empty() || !self.format.is_empty()
        } else {
            self.query.trim().chars().count() < 2
        }) || self.query.chars().count() > 200
            || self.query.chars().any(char::is_control)
        {
            return Err(SourceError {
                code: SourceErrorCode::InvalidRequest,
                message: "Digite uma pesquisa de 2–200 caracteres.".into(),
            });
        }
        if [&self.language, &self.format].iter().any(|value| {
            value.len() > 25
                || !value
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || c == b'-')
        }) {
            return Err(SourceError {
                code: SourceErrorCode::InvalidRequest,
                message: "Filtros inválidos.".into(),
            });
        }
        if self.cursor.as_ref().is_some_and(|cursor| {
            cursor.trim().is_empty() || cursor.len() > 2048 || cursor.chars().any(char::is_control)
        }) {
            return Err(SourceError {
                code: SourceErrorCode::InvalidRequest,
                message: "Cursor de pesquisa inválido.".into(),
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCapabilities {
    pub search: bool,
    pub download: bool,
    pub pagination: bool,
    pub details: bool,
    pub editions: bool,
    #[serde(default)]
    pub feed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceDescriptor {
    pub id: String,
    pub name: String,
    pub api_version: u32,
    pub capabilities: SourceCapabilities,
}

/// Same fields, types, and serialized names as the existing SourceResult.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceBook {
    pub source_id: String,
    pub source_name: String,
    pub title: String,
    pub author: String,
    pub format: String,
    pub language: String,
    pub download_url: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub remote_id: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub publisher: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub acquisition_page: Option<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub edition_label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchPage {
    pub results: Vec<SourceBook>,
    pub next_cursor: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SourceErrorCode {
    InvalidRequest,
    Unsupported,
    Network,
    InvalidResponse,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceError {
    pub code: SourceErrorCode,
    pub message: String,
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for SourceError {}

/// Object-safe, in-process contract; implementations must validate requests
/// before performing source work. Capability flags are not permissions.
pub trait BookSource: Send + Sync {
    fn descriptor(&self) -> SourceDescriptor;

    fn search(
        &self,
        request: SearchRequest,
    ) -> Pin<Box<dyn Future<Output = Result<SearchPage, SourceError>> + Send + '_>>;
    fn browse(
        &self,
        _category: Option<String>,
        _cursor: Option<String>,
    ) -> Pin<Box<dyn Future<Output = Result<SearchPage, SourceError>> + Send + '_>> {
        Box::pin(async {
            Err(SourceError {
                code: SourceErrorCode::Unsupported,
                message: "Esta fonte não oferece feed.".into(),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request() -> SearchRequest {
        SearchRequest {
            query: "Machado de Assis".into(),
            language: String::new(),
            format: String::new(),
            cursor: None,
        }
    }

    #[test]
    fn feed_validation_does_not_relax_search() {
        let mut r = request();
        r.query.clear();
        assert!(r.validate().is_err());
        assert!(r.validate_feed().is_ok());
        r.cursor = Some("".into());
        assert!(r.validate_feed().is_err());
        r.cursor = None;
        r.language = "pt".into();
        assert!(r.validate_feed().is_err());
    }

    #[test]
    fn accepts_unfiltered_search_and_preserves_values() {
        let mut request = request();
        request.query = "  Machado  ".into();
        let original = request.clone();
        assert_eq!(request.validate(), Ok(()));
        assert_eq!(request, original);
    }

    #[test]
    fn rejects_blank_queries() {
        for query in ["", " \t\n", "\u{2003}"] {
            let mut request = request();
            request.query = query.into();
            assert_eq!(
                request.validate().unwrap_err().code,
                SourceErrorCode::InvalidRequest
            );
        }
    }

    #[test]
    fn rejects_blank_cursor_but_preserves_opaque_cursor() {
        let mut request = request();
        for cursor in ["", " \n"] {
            request.cursor = Some(cursor.into());
            assert_eq!(
                request.validate().unwrap_err().code,
                SourceErrorCode::InvalidRequest
            );
        }
        request.cursor = Some(" opaque+/= token ".into());
        assert_eq!(request.validate(), Ok(()));
        assert_eq!(request.cursor.as_deref(), Some(" opaque+/= token "));
    }

    #[test]
    fn bounds_queries_filters_and_cursors() {
        let mut req = request();
        for query in ["a".to_string(), "x".repeat(201), "foo\nbar".into()] {
            req.query = query;
            assert!(req.validate().is_err());
        }
        req = request();
        req.language = "pt&evil=1".into();
        assert!(req.validate().is_err());
        req.language = "pt-br".into();
        req.cursor = Some("x".repeat(2049));
        assert!(req.validate().is_err());
    }

    #[test]
    fn contract_is_object_safe_and_thread_safe() {
        fn assert_send_sync<T: Send + Sync + ?Sized>() {}
        assert_send_sync::<dyn BookSource>();
        let _: Option<Box<dyn BookSource>> = None;
        assert_eq!(API_VERSION, 1);
    }
}
