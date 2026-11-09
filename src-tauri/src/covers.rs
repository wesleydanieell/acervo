//! Read-only thumbnails for existing PDF and EPUB files.
use base64::Engine;
use std::fs::{self, File};
use std::hash::{Hash, Hasher};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, UNIX_EPOCH};
use tauri::Manager;
use zip::ZipArchive;

const MAX_IMAGE: u64 = 4 * 1024 * 1024;
const MAX_XML: u64 = 1024 * 1024;

fn zip_bytes(zip: &mut ZipArchive<File>, entry_name: &str, limit: u64) -> Option<Vec<u8>> {
    let mut entry = zip.by_name(entry_name).ok()?;
    if entry.is_dir() || entry.size() > limit {
        return None;
    }
    let mut data = Vec::with_capacity(entry.size() as usize);
    entry.by_ref().take(limit + 1).read_to_end(&mut data).ok()?;
    if data.len() as u64 > limit {
        return None;
    }
    Some(data)
}

fn zip_text(zip: &mut ZipArchive<File>, entry: &str, limit: u64) -> Option<String> {
    String::from_utf8(zip_bytes(zip, entry, limit)?).ok()
}

fn image_kind(bytes: &[u8]) -> Option<(&'static str, &'static str)> {
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(("image/jpeg", "jpg"));
    }
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(("image/png", "png"));
    }
    if bytes.len() > 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        return Some(("image/webp", "webp"));
    }
    None
}

fn epub_cover(path: &Path) -> Option<Vec<u8>> {
    let mut zip = ZipArchive::new(File::open(path).ok()?).ok()?;
    let container = zip_text(&mut zip, "META-INF/container.xml", MAX_XML)?;
    let doc = roxmltree::Document::parse(&container).ok()?;
    let opf_path = doc
        .descendants()
        .find(|node| node.has_tag_name("rootfile"))?
        .attribute("full-path")?;
    if opf_path.starts_with('/') || opf_path.contains("..") {
        return None;
    }
    let opf = zip_text(&mut zip, opf_path, MAX_XML)?;
    let document = roxmltree::Document::parse(&opf).ok()?;
    let metadata_cover = document
        .descendants()
        .find(|node| node.has_tag_name("meta") && node.attribute("name") == Some("cover"))
        .and_then(|node| node.attribute("content"));
    let manifest = document
        .descendants()
        .filter(|node| node.has_tag_name("item"))
        .filter_map(|node| {
            Some((
                node.attribute("id")?,
                node.attribute("href")?,
                node.attribute("properties").unwrap_or(""),
            ))
        })
        .collect::<Vec<_>>();
    let preferred = manifest
        .iter()
        .find(|(_, _, properties)| {
            properties
                .split_whitespace()
                .any(|property| property == "cover-image")
        })
        .or_else(|| {
            metadata_cover.and_then(|id| manifest.iter().find(|(item_id, _, _)| *item_id == id))
        })
        .or_else(|| {
            manifest
                .iter()
                .find(|(_, href, _)| href.to_ascii_lowercase().contains("cover"))
        });
    let href = preferred.map(|(_, href, _)| *href).or_else(|| {
        document
            .descendants()
            .find(|node| node.has_tag_name("reference") && node.attribute("type") == Some("cover"))
            .and_then(|node| node.attribute("href"))
    })?;
    if href.starts_with('/') || href.split('/').any(|part| part == "..") {
        return None;
    }
    let parent = Path::new(opf_path)
        .parent()
        .unwrap_or_else(|| Path::new(""));
    let inside_zip = parent.join(href).to_string_lossy().replace('\\', "/");
    let bytes = zip_bytes(&mut zip, &inside_zip, MAX_IMAGE)?;
    image_kind(&bytes)?;
    Some(bytes)
}

fn cache_key(path: &Path, metadata: &fs::Metadata) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    path.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|time| time.as_nanos())
        .hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

fn run_pdf_renderer(path: &Path, prefix: &Path) -> bool {
    let mut child = match Command::new("pdftoppm")
        .args([
            "-f",
            "1",
            "-l",
            "1",
            "-singlefile",
            "-scale-to",
            "240",
            "-png",
        ])
        .arg(path)
        .arg(prefix)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => return false,
    };
    let start = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return status.success(),
            Ok(None) if start.elapsed() < Duration::from_secs(15) => {
                std::thread::sleep(Duration::from_millis(80))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return false;
            }
        }
    }
}

fn get_cached_cover(
    root: String,
    relative_path: String,
    cache: PathBuf,
) -> Result<Option<String>, String> {
    let root = fs::canonicalize(root).map_err(|e| e.to_string())?;
    let path = super::resolve_inside(&root, &relative_path)?;
    if !path.is_file() {
        return Ok(None);
    }
    let format = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if format != "pdf" && format != "epub" {
        return Ok(None);
    }
    let metadata = path.metadata().map_err(|e| e.to_string())?;
    fs::create_dir_all(&cache).map_err(|e| e.to_string())?;
    let key = cache_key(&path, &metadata);
    let from_cache = ["png", "jpg", "webp"].iter().find_map(|extension| {
        let file = cache.join(format!("{key}.{extension}"));
        if file.exists() { Some(file) } else { None }
    });
    let cover_file = if let Some(file) = from_cache {
        file
    } else if format == "epub" {
        let Some(bytes) = epub_cover(&path) else {
            return Ok(None);
        };
        let Some((_, extension)) = image_kind(&bytes) else {
            return Ok(None);
        };
        let file = cache.join(format!("{key}.{extension}"));
        let mut writer = File::create(&file).map_err(|e| e.to_string())?;
        writer.write_all(&bytes).map_err(|e| e.to_string())?;
        file
    } else {
        let file = cache.join(format!("{key}.png"));
        let prefix = cache.join(&key);
        if !run_pdf_renderer(&path, &prefix) {
            return Ok(None);
        }
        file
    };
    let metadata = cover_file.metadata().map_err(|e| e.to_string())?;
    if metadata.len() > MAX_IMAGE || metadata.len() == 0 {
        return Ok(None);
    }
    let bytes = fs::read(&cover_file).map_err(|e| e.to_string())?;
    let Some((mime, _)) = image_kind(&bytes) else {
        return Ok(None);
    };
    Ok(Some(format!(
        "data:{mime};base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )))
}

#[tauri::command]
pub async fn get_book_cover(
    app: tauri::AppHandle,
    root: String,
    relative_path: String,
) -> Result<Option<String>, String> {
    let cache = app
        .path()
        .app_cache_dir()
        .map_err(|e| e.to_string())?
        .join("book-covers");
    tauri::async_runtime::spawn_blocking(move || get_cached_cover(root, relative_path, cache))
        .await
        .map_err(|e| e.to_string())?
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn accepts_only_raster_cover_bytes() {
        assert_eq!(
            image_kind(&[0xff, 0xd8, 0xff, 0x00]).unwrap().0,
            "image/jpeg"
        );
        assert!(image_kind(b"<svg><script/></svg>").is_none());
    }

    #[test]
    fn reads_manifest_declared_epub_cover() {
        let unique = std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let file_path = std::env::temp_dir().join(format!(
            "acervo-cover-test-{}-{unique}.epub",
            std::process::id()
        ));
        let file = File::create(&file_path).unwrap();
        let mut archive = zip::ZipWriter::new(file);
        archive
            .start_file(
                "META-INF/container.xml",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(br#"<?xml version="1.0"?><container><rootfiles><rootfile full-path="OEBPS/content.opf"/></rootfiles></container>"#).unwrap();
        archive
            .start_file(
                "OEBPS/content.opf",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        archive.write_all(br#"<package><manifest><item id="front" properties="cover-image" href="images/front.png"/></manifest></package>"#).unwrap();
        archive
            .start_file(
                "OEBPS/images/front.png",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
        let png = b"\x89PNG\r\n\x1a\nminimal-test-bytes";
        archive.write_all(png).unwrap();
        archive.finish().unwrap();
        let result = epub_cover(&file_path).unwrap();
        assert_eq!(result, png);
        fs::remove_file(file_path).unwrap();
    }
}
