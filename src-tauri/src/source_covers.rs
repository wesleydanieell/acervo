//! Bounded same-origin cover retrieval. Never load arbitrary remote images in the UI.
use crate::{
    extension_network::{safe_url, secure_https_client},
    sources::{self, SourceManifest},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::time::Duration;
use url::Url;
const MAX_IMAGE: usize = 512 * 1024;
static SLOTS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(4);

pub(crate) fn resolve_cover(manifest: &SourceManifest, base: &Url, href: &str) -> Option<String> {
    if href.trim().is_empty() || href.len() > 2047 || href.chars().any(char::is_control) {
        return None;
    }
    let origin = Url::parse(&manifest.base_url).ok()?;
    let url = base.join(href).ok()?;
    (url.origin() == origin.origin() && url.fragment().is_none() && safe_url(url.as_str()))
        .then(|| url.into())
}
fn mime(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 && &bytes[12..16] == b"IHDR" {
        let w = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let h = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        if w > 0 && h > 0 && w <= 4096 && h <= 4096 && w * h <= 8_000_000 {
            return Some("image/png");
        }
    }
    if bytes.starts_with(b"\xff\xd8\xff") {
        // Find the JPEG frame header to bound decoded dimensions.
        let mut i = 2;
        while i + 4 <= bytes.len() {
            if bytes[i] != 0xff {
                break;
            }
            while i < bytes.len() && bytes[i] == 0xff {
                i += 1;
            }
            let marker = *bytes.get(i)?;
            i += 1;
            if marker == 0xda || marker == 0xd9 {
                break;
            }
            let len = u16::from_be_bytes(bytes.get(i..i + 2)?.try_into().ok()?) as usize;
            if len < 2 || i + len > bytes.len() {
                break;
            }
            if matches!(marker, 0xc0 | 0xc1 | 0xc2) && len >= 8 {
                let h = u16::from_be_bytes(bytes[i + 3..i + 5].try_into().ok()?) as u32;
                let w = u16::from_be_bytes(bytes[i + 5..i + 7].try_into().ok()?) as u32;
                if w > 0 && h > 0 && w <= 4096 && h <= 4096 && w * h <= 8_000_000 {
                    return Some("image/jpeg");
                }
                break;
            }
            i += len;
        }
    }
    None
}
#[tauri::command]
pub async fn get_source_cover(
    app: tauri::AppHandle,
    id: String,
    url: String,
) -> Result<Option<String>, String> {
    let source = sources::list_sources(app)?
        .into_iter()
        .find(|s| s.enabled && s.manifest.id == id)
        .ok_or("Fonte indisponível.")?;
    let base = Url::parse(&source.manifest.base_url).map_err(|_| "Origem inválida.")?;
    let Some(url) = resolve_cover(&source.manifest, &base, &url) else {
        return Ok(None);
    };
    let url = Url::parse(&url).map_err(|_| "Capa inválida.")?;
    let _permit = SLOTS
        .try_acquire()
        .map_err(|_| "Limite de capas simultâneas.")?;
    tokio::time::timeout(Duration::from_secs(12), async {
        let client = secure_https_client(&url, Duration::from_secs(10)).await?;
        let mut res = client
            .get(url)
            .header("Accept", "image/jpeg, image/png")
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !res.status().is_success() || res.content_length().is_some_and(|n| n > MAX_IMAGE as u64)
        {
            return Ok(None);
        }
        let mut data = Vec::new();
        while let Some(chunk) = res.chunk().await.map_err(|e| e.to_string())? {
            if data.len() + chunk.len() > MAX_IMAGE {
                return Ok(None);
            }
            data.extend_from_slice(&chunk);
        }
        Ok(mime(&data).map(|kind| format!("data:{kind};base64,{}", STANDARD.encode(data))))
    })
    .await
    .map_err(|_| "Tempo limite da capa.".to_owned())?
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_bounded_raster_headers_are_accepted() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(200u32.to_be_bytes());
        png.extend(300u32.to_be_bytes());
        assert_eq!(mime(&png), Some("image/png"));
        png[16..20].copy_from_slice(&5000u32.to_be_bytes());
        assert_eq!(mime(&png), None);
        assert_eq!(mime(b"<svg>"), None);
        assert_eq!(mime(b"\xff\xd8\xff"), None);
    }
}
