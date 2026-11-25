//! User-initiated browsing in a separate, unprivileged webview.
use crate::{
    extension_network::{safe_url, secure_https_client},
    sources,
};
use std::time::Duration;
use tauri::{
    Manager, WebviewUrl, WebviewWindowBuilder,
    menu::{Menu, MenuItem, Submenu},
};
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
    // Menu IDs are unique per source window because Tauri dispatches menu events globally.
    let action_id = |action: &str| format!("{label}:{action}");
    let back = MenuItem::with_id(&app, action_id("back"), "Voltar", true, Some("Alt+Left"))
        .map_err(|e| e.to_string())?;
    let forward = MenuItem::with_id(
        &app,
        action_id("forward"),
        "Avançar",
        true,
        Some("Alt+Right"),
    )
    .map_err(|e| e.to_string())?;
    let reload = MenuItem::with_id(
        &app,
        action_id("reload"),
        "Recarregar",
        true,
        Some("Ctrl+R"),
    )
    .map_err(|e| e.to_string())?;
    let home = MenuItem::with_id(
        &app,
        action_id("home"),
        "Página inicial",
        true,
        Some("Alt+Home"),
    )
    .map_err(|e| e.to_string())?;
    let close = MenuItem::with_id(
        &app,
        action_id("close"),
        "Encerrar sessão",
        true,
        Some("Ctrl+W"),
    )
    .map_err(|e| e.to_string())?;
    let navigation = Submenu::with_items(
        &app,
        "Navegação",
        true,
        &[&back, &forward, &reload, &home, &close],
    )
    .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(&app, &[&navigation]).map_err(|e| e.to_string())?;
    let origin = url.clone();
    let home_url = url.clone();
    let menu_label = label.clone();
    let source_name = manifest.name.clone();
    WebviewWindowBuilder::new(&app, label, WebviewUrl::External(url))
        .title(format!("{} · Acervo", manifest.name))
        .inner_size(1100.0, 780.0)
        .min_inner_size(550.0, 400.0)
        .menu(menu)
        .on_menu_event(move |window, event| {
            let Some(action) = event.id().as_ref().strip_prefix(&format!("{menu_label}:")) else {
                return;
            };
            let Some(browser) = window.app_handle().get_webview_window(&menu_label) else {
                return;
            };
            if !browser.is_focused().unwrap_or(false) {
                return;
            }
            match action {
                // Constant scripts affect only browser history, never application IPC.
                "back" => {
                    let _ = browser.eval("window.history.back();");
                }
                "forward" => {
                    let _ = browser.eval("window.history.forward();");
                }
                "reload" => {
                    let _ = browser.reload();
                }
                "home" => {
                    let _ = browser.navigate(home_url.clone());
                }
                "close" => {
                    let _ = browser.close();
                }
                _ => {}
            }
        })
        .on_page_load(move |window, payload| {
            // Display the actual origin in native chrome, not untrusted page titles.
            if let Some(host) = payload.url().host_str() {
                let _ = window.set_title(&format!("{source_name} · {host} · Acervo"));
            }
        })
        .incognito(true)
        .on_permission_request(|_, _| tauri::webview::PermissionResponse::Deny)
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
