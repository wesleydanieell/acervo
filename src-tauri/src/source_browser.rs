//! User-initiated browsing in a separate, unprivileged webview.
use crate::{
    extension_network::{safe_url, secure_https_client},
    sources,
};
use std::time::Duration;
use tauri::{Manager, WebviewUrl, WebviewWindowBuilder};
use url::Url;

fn allowed_navigation(origin: &Url, next: &Url) -> bool {
    next.origin() == origin.origin() && safe_url(next.as_str())
}

#[tauri::command]
pub async fn open_source_webview(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let source = sources::list_sources(app.clone())?
        .into_iter()
        .find(|s| s.manifest.id == id)
        .ok_or("Extensão não encontrada.")?;
    let manifest = source.manifest;
    let raw = manifest
        .website_url
        .as_deref()
        .unwrap_or(&manifest.base_url);
    let url = Url::parse(raw).map_err(|_| "Endereço da extensão inválido.")?;
    // Validate the starting DNS address using the common network policy.
    // The browser itself has its own network stack; this is not DNS pinning.
    tokio::time::timeout(
        Duration::from_secs(5),
        secure_https_client(&url, Duration::from_secs(5)),
    )
    .await
    .map_err(|_| "Tempo limite de DNS.")??;
    let label = format!("source-web-{}", manifest.id);
    if let Some(window) = app.get_webview_window(&label) {
        window.set_focus().map_err(|e| e.to_string())?;
        return Ok(());
    }
    let origin = url.clone();
    WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url))
        .title(format!("{} · Acervo", manifest.name))
        .inner_size(1100.0, 780.0)
        .min_inner_size(550.0, 400.0)
        .incognito(true)
        .devtools(false)
        .on_navigation(move |next| allowed_navigation(&origin, next))
        .on_new_window(|_, _| tauri::webview::NewWindowResponse::Deny)
        .on_download(|_, _| false)
        .build()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn browser_navigation_stays_on_public_https_origin() {
        let origin = Url::parse("https://example.org").unwrap();
        assert!(allowed_navigation(
            &origin,
            &Url::parse("https://example.org/catalog?page=2").unwrap()
        ));
        for value in [
            "http://example.org",
            "https://other.org",
            "https://example.org:8080",
            "file:///tmp/book",
            "tauri://localhost",
            "https://user@example.org",
        ] {
            assert!(
                !allowed_navigation(&origin, &Url::parse(value).unwrap()),
                "{value}"
            );
        }
    }
}
