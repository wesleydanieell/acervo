//! Conservative format validation for completed staged downloads.
//! A valid extension or HTTP Content-Type is not evidence of a book.
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek};
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use zip::ZipArchive;

fn open_regular_file(path: &Path) -> Result<File, String> {
    let meta = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !meta.is_file() || meta.file_type().is_symlink() || meta.len() == 0 {
        return Err("Arquivo temporário ausente ou vazio.".into());
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW);
    options.open(path).map_err(|error| error.to_string())
}

/// Check the declared book format before marking a transfer ready and again
/// before committing it. This is a format check, not cryptographic verification.
pub fn validate_staged_book(path: &Path, format: &str) -> Result<(), String> {
    let mut file = open_regular_file(path)?;
    let mut prefix = Vec::new();
    file.by_ref()
        .take(1024)
        .read_to_end(&mut prefix)
        .map_err(|error| error.to_string())?;
    let kind = format.to_ascii_lowercase();
    let valid = match kind.as_str() {
        "pdf" => prefix.windows(5).any(|bytes| bytes == b"%PDF-"),
        "djvu" => {
            prefix.starts_with(b"AT&TFORM")
                && prefix
                    .get(12..16)
                    .is_some_and(|chunk| chunk == b"DJVU" || chunk == b"DJVM")
        }
        "mobi" | "azw3" => prefix.get(60..68) == Some(b"BOOKMOBI".as_slice()),
        "epub" => {
            file.rewind().map_err(|error| error.to_string())?;
            let mut archive = ZipArchive::new(file)
                .map_err(|_| "O EPUB não é um arquivo ZIP íntegro.".to_string())?;
            let mimetype_ok = match archive.by_name("mimetype") {
                Ok(mut entry) if entry.size() == 20 => {
                    let mut value = String::new();
                    entry.read_to_string(&mut value).is_ok() && value == "application/epub+zip"
                }
                _ => false,
            };
            let has_container = archive.by_name("META-INF/container.xml").is_ok();
            mimetype_ok && has_container
        }
        _ => return Err("Formato desconhecido para validação.".into()),
    };
    if valid {
        Ok(())
    } else {
        Err(format!(
            "O download não parece ser um arquivo {} válido. Confira a fonte antes de tentar novamente.",
            kind.to_ascii_uppercase()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    fn fixture(bytes: &[u8]) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "acervo-validate-{}-{}.part",
            std::process::id(),
            NEXT_TEST.fetch_add(1, Ordering::Relaxed)
        ));
        fs::write(&path, bytes).unwrap();
        path
    }

    #[test]
    fn rejects_html_and_wrongly_named_payloads() {
        let html = fixture(b"<html>Try another mirror</html>");
        for format in ["pdf", "epub", "djvu", "mobi", "azw3"] {
            assert!(validate_staged_book(&html, format).is_err());
        }
        fs::remove_file(html).unwrap();

        let pdf = fixture(b"%PDF-1.7\n1 0 obj\nendobj\n%%EOF");
        assert!(validate_staged_book(&pdf, "PDF").is_ok());
        assert!(validate_staged_book(&pdf, "epub").is_err());
        fs::remove_file(pdf).unwrap();
    }

    #[test]
    fn checks_djvu_and_palm_database_signatures() {
        let djvu = fixture(b"AT&TFORM\0\0\0\0DJVU");
        assert!(validate_staged_book(&djvu, "djvu").is_ok());
        fs::remove_file(djvu).unwrap();

        let mut palm = vec![0u8; 80];
        palm[60..68].copy_from_slice(b"BOOKMOBI");
        let mobi = fixture(&palm);
        assert!(validate_staged_book(&mobi, "mobi").is_ok());
        assert!(validate_staged_book(&mobi, "azw3").is_ok());
        fs::remove_file(mobi).unwrap();
    }

    #[test]
    fn epub_needs_zip_and_package_container() {
        let bad_zip = fixture(b"PK\x03\x04invalid EPUB");
        assert!(validate_staged_book(&bad_zip, "epub").is_err());
        fs::remove_file(bad_zip).unwrap();

        let path = fixture(b"temporary");
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        zip.start_file("mimetype", zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        zip.start_file(
            "META-INF/container.xml",
            zip::write::SimpleFileOptions::default(),
        )
        .unwrap();
        zip.write_all(b"<container/>").unwrap();
        zip.finish().unwrap();
        assert!(validate_staged_book(&path, "epub").is_ok());
        fs::remove_file(path).unwrap();
    }
}
