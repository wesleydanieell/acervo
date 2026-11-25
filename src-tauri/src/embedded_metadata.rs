//! Bounded, read-only metadata readers. No archive members are written to disk.
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

const MAX_FILE: u64 = 512 * 1024 * 1024;
const MAX_XML: u64 = 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_OUTPUT: u64 = 64 * 1024;
const CONTAINER: &str = "urn:oasis:names:tc:opendocument:xmlns:container";
const OPF: &str = "http://www.idpf.org/2007/opf";
const DC: &str = "http://purl.org/dc/elements/1.1/";

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EmbeddedMetadata {
    pub title: String,
    pub author: String,
    pub language: String,
    pub publisher: String,
    pub isbn: String,
    pub description: String,
}

pub(crate) fn extract(path: &Path, format: &str) -> Result<EmbeddedMetadata, String> {
    if !matches!(format, "EPUB" | "PDF") {
        return Err("Unsupported metadata format (expected EPUB or PDF)".into());
    }
    // Check before opening as well, so FIFOs/devices cannot block the open.
    check_file(&std::fs::symlink_metadata(path).map_err(|e| format!("Cannot inspect file: {e}"))?)?;
    let file = crate::library_editions::open_regular(path)?;
    check_file(
        &file
            .metadata()
            .map_err(|e| format!("Cannot inspect open file: {e}"))?,
    )?;
    match format {
        "EPUB" => epub(file),
        _ => pdf(path),
    }
}

fn check_file(metadata: &std::fs::Metadata) -> Result<(), String> {
    if !metadata.file_type().is_file() {
        return Err("Metadata input must be a regular file".into());
    }
    if metadata.len() > MAX_FILE {
        return Err("Metadata input exceeds 512 MiB".into());
    }
    Ok(())
}

fn normalized(text: &str, limit: usize) -> String {
    let mut out = String::new();
    let mut space = false;
    for ch in text.chars() {
        if ch.is_whitespace() || ch.is_control() {
            space = !out.is_empty();
            continue;
        }
        // Remove Unicode directional/format controls as well as C0/C1 controls.
        if matches!(ch, '\u{00ad}' | '\u{061c}' | '\u{200b}'..='\u{200f}' | '\u{202a}'..='\u{202e}' | '\u{2060}'..='\u{206f}' | '\u{feff}')
        {
            continue;
        }
        let needed = ch.len_utf8() + usize::from(space);
        if out.len() + needed > limit {
            break;
        }
        if space {
            out.push(' ');
            space = false;
        }
        out.push(ch);
    }
    out
}

// Preflight the classic central directory before ZipArchive can allocate it.
// ZIP64 and multi-volume archives are deliberately unsupported.
fn zip_preflight(file: &mut File) -> Result<(), String> {
    let len = file.metadata().map_err(|e| e.to_string())?.len();
    let tail_len = len.min(65_557) as usize;
    file.seek(SeekFrom::End(-(tail_len as i64)))
        .map_err(|e| e.to_string())?;
    let mut tail = vec![0; tail_len];
    file.read_exact(&mut tail).map_err(|e| e.to_string())?;
    let pos = (0..tail.len().saturating_sub(21))
        .rev()
        .find(|&i| {
            tail[i..].starts_with(b"PK\x05\x06")
                && i + 22 + u16::from_le_bytes([tail[i + 20], tail[i + 21]]) as usize == tail.len()
        })
        .ok_or("Corrupt EPUB ZIP directory")?;
    let u16_at = |i| u16::from_le_bytes([tail[pos + i], tail[pos + i + 1]]);
    let u32_at = |i| u32::from_le_bytes(tail[pos + i..pos + i + 4].try_into().unwrap());
    if pos >= 20 && tail[pos - 20..].starts_with(b"PK\x06\x07") {
        return Err("ZIP64 EPUB archives are unsupported".into());
    }
    let count = u16_at(10);
    let size = u32_at(12);
    let offset = u32_at(16);
    if u16_at(4) != 0
        || u16_at(6) != 0
        || u16_at(8) != count
        || count as usize > MAX_ENTRIES
        || count == u16::MAX
        || size > 8 * 1024 * 1024
        || offset == u32::MAX
        || offset as u64 + size as u64 != len - tail_len as u64 + pos as u64
    {
        return Err("Unsupported or oversized EPUB ZIP directory".into());
    }
    file.rewind().map_err(|e| e.to_string())
}

fn member(zip: &mut zip::ZipArchive<File>, name: &str) -> Result<String, String> {
    let mut entry = zip
        .by_name(name)
        .map_err(|e| format!("Missing/corrupt EPUB XML member: {e}"))?;
    if entry.is_dir() || entry.size() > MAX_XML {
        return Err("EPUB XML member exceeds limit or is not a file".into());
    }
    let mut bytes = Vec::new();
    entry
        .by_ref()
        .take(MAX_XML + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("Cannot read EPUB XML: {e}"))?;
    if bytes.len() as u64 > MAX_XML {
        return Err("EPUB XML exceeds limit".into());
    }
    String::from_utf8(bytes).map_err(|_| "EPUB XML must be UTF-8".into())
}

fn xml(text: &str) -> Result<roxmltree::Document<'_>, String> {
    if text.len() as u64 > MAX_XML {
        return Err("EPUB XML exceeds limit".into());
    }
    roxmltree::Document::parse_with_options(
        text,
        roxmltree::ParsingOptions {
            allow_dtd: false,
            nodes_limit: 32_768,
            ..Default::default()
        },
    )
    .map_err(|e| format!("Invalid EPUB XML (DTDs/entities are forbidden): {e}"))
}

fn rootfile(container: &str) -> Result<String, String> {
    let doc = xml(container)?;
    let root = doc.root_element();
    if !root.has_tag_name((CONTAINER, "container")) {
        return Err("Invalid EPUB container namespace".into());
    }
    let path = root
        .children()
        .filter(|n| n.has_tag_name((CONTAINER, "rootfiles")))
        .flat_map(|n| n.children())
        .find(|n| {
            n.has_tag_name((CONTAINER, "rootfile"))
                && n.attribute("media-type") == Some("application/oebps-package+xml")
        })
        .and_then(|n| n.attribute("full-path"))
        .ok_or("EPUB has no OPF rootfile")?;
    if path.is_empty()
        || path.len() > 1024
        || path.contains(['\\', ':', '\0'])
        || path.split('/').any(|part| matches!(part, "" | "." | ".."))
        || path.chars().any(char::is_control)
    {
        return Err("Invalid EPUB OPF member path".into());
    }
    Ok(path.to_owned())
}

fn epub(mut file: File) -> Result<EmbeddedMetadata, String> {
    zip_preflight(&mut file)?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| format!("Corrupt EPUB ZIP: {e}"))?;
    if zip.len() > MAX_ENTRIES {
        return Err("Too many EPUB ZIP entries".into());
    }
    let path = rootfile(&member(&mut zip, "META-INF/container.xml")?)?;
    parse_opf(&member(&mut zip, &path)?)
}

fn parse_opf(text: &str) -> Result<EmbeddedMetadata, String> {
    let doc = xml(text)?;
    let root = doc.root_element();
    if !root.has_tag_name((OPF, "package")) {
        return Err("Invalid EPUB OPF package namespace".into());
    }
    let metadata = root
        .children()
        .find(|n| n.has_tag_name((OPF, "metadata")))
        .ok_or("EPUB OPF metadata missing")?;
    let mut result = EmbeddedMetadata::default();
    for node in metadata
        .children()
        .filter(|n| n.is_element() && n.tag_name().namespace() == Some(DC))
    {
        let (field, limit) = match node.tag_name().name() {
            "title" => (&mut result.title, 500),
            "creator" => (&mut result.author, 500),
            "language" => (&mut result.language, 25),
            "publisher" => (&mut result.publisher, 500),
            "description" => (&mut result.description, 2000),
            "identifier" => {
                let value: String = node
                    .descendants()
                    .filter(|n| n.is_text())
                    .filter_map(|n| n.text())
                    .collect();
                let value = normalized(&value, 256);
                let urn = value
                    .get(..9)
                    .is_some_and(|s| s.eq_ignore_ascii_case("urn:isbn:"));
                let scheme = node
                    .attribute((OPF, "scheme"))
                    .is_some_and(|s| s.eq_ignore_ascii_case("ISBN"));
                if result.isbn.is_empty() && (urn || scheme) {
                    result.isbn = normalized(if urn { &value[9..] } else { &value }, 64);
                }
                continue;
            }
            _ => continue,
        };
        if field.is_empty() {
            let value: String = node
                .descendants()
                .filter(|n| n.is_text())
                .filter_map(|n| n.text())
                .collect();
            *field = normalized(&value, limit);
        }
    }
    Ok(result)
}

fn parse_pdfinfo(text: &str) -> EmbeddedMetadata {
    let mut result = EmbeddedMetadata::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let (field, limit) = match key {
            "Title" => (&mut result.title, 500),
            "Author" => (&mut result.author, 500),
            "Subject" => (&mut result.description, 2000),
            _ => continue,
        };
        if field.is_empty() {
            *field = normalized(value, limit);
        }
    }
    result
}

fn pdf_failure(code: Option<i32>, stderr: &str) -> String {
    let diagnostic = stderr.to_ascii_lowercase();
    if diagnostic.contains("password") || diagnostic.contains("encrypted") {
        "Password-protected PDF".into()
    } else if diagnostic.contains("unsupported") || diagnostic.contains("not supported") {
        "Unsupported PDF feature".into()
    } else {
        format!("Corrupt or unreadable PDF (pdfinfo exit {code:?})")
    }
}

fn pdf(path: &Path) -> Result<EmbeddedMetadata, String> {
    // An absolute local filename cannot be interpreted as an option or a URL.
    let path = std::fs::canonicalize(path).map_err(|e| format!("Cannot resolve PDF: {e}"))?;
    let mut child = Command::new("pdfinfo")
        .args(["-enc", "UTF-8"])
        .arg(path)
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                "PDF metadata unavailable: pdfinfo executable missing".into()
            } else {
                format!("Cannot start pdfinfo: {e}")
            }
        })?;
    let start = Instant::now();
    let (tx, rx) = mpsc::channel();
    // Each reader allocates at most MAX_OUTPUT + 1, and never waits for the child.
    fn reader(
        stream: impl Read + Send + 'static,
        tx: mpsc::Sender<(bool, std::io::Result<Vec<u8>>)>,
        stdout: bool,
    ) {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let result = stream
                .take(MAX_OUTPUT + 1)
                .read_to_end(&mut bytes)
                .map(|_| bytes);
            let _ = tx.send((stdout, result));
        });
    }
    reader(child.stdout.take().unwrap(), tx.clone(), true);
    reader(child.stderr.take().unwrap(), tx, false);
    let mut output = None;
    let mut errors = None;
    let result = 'read: loop {
        loop {
            let (stdout, bytes) = match rx.try_recv() {
                Ok(message) => message,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if output.is_none() || errors.is_none() {
                        break 'read Err("pdfinfo output could not be read".into());
                    }
                    break;
                }
            };
            match bytes {
                Ok(bytes) if bytes.len() as u64 <= MAX_OUTPUT => {
                    if stdout {
                        output = Some(bytes);
                    } else {
                        errors = Some(bytes);
                    }
                }
                _ => break 'read Err("pdfinfo output exceeded limit or could not be read".into()),
            }
        }
        if start.elapsed() >= Duration::from_secs(4) {
            break Err("pdfinfo timed out after 4 seconds".into());
        }
        match child.try_wait() {
            Ok(Some(status)) if output.is_some() && errors.is_some() => {
                let diagnostic = String::from_utf8_lossy(errors.as_ref().unwrap());
                if !status.success() || diagnostic.to_ascii_lowercase().contains("syntax error") {
                    break Err(pdf_failure(status.code(), &diagnostic));
                }
                break String::from_utf8(output.take().unwrap())
                    .map(|text| parse_pdfinfo(&text))
                    .map_err(|_| "Invalid UTF-8 from pdfinfo".into());
            }
            Err(e) => break Err(format!("Cannot wait for pdfinfo: {e}")),
            _ => std::thread::sleep(Duration::from_millis(10)),
        }
    };
    if result.is_err() {
        let _ = child.kill();
    }
    let _ = child.wait();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_from_complete_epub_archive() {
        use std::io::Write;
        let path = std::env::temp_dir().join(format!(
            "acervo-epub-meta-{}-{}.epub",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let file = File::create(&path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("mimetype", options).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        zip.start_file("META-INF/container.xml", options).unwrap();
        write!(zip,r#"<container xmlns="{CONTAINER}"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#).unwrap();
        zip.start_file("OPS/book.opf", options).unwrap();
        write!(zip,r#"<package xmlns="{OPF}" xmlns:dc="{DC}"><metadata><dc:title>Book</dc:title><dc:creator>Author</dc:creator><dc:language>pt</dc:language></metadata></package>"#).unwrap();
        zip.finish().unwrap();
        let result = extract(&path, "EPUB").unwrap();
        assert_eq!(result.title, "Book");
        assert_eq!(result.author, "Author");
        assert_eq!(result.language, "pt");
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn normalization_is_bounded_and_control_free() {
        assert_eq!(normalized(" \nOlá\t mundo\0\u{202e}! ", 500), "Olá mundo !");
        assert_eq!(normalized("ééé", 5), "éé");
        assert_eq!(normalized("abc def", 4), "abc");
    }

    #[test]
    fn opf_uses_dc_and_explicit_isbn() {
        let text = r#"<package xmlns="http://www.idpf.org/2007/opf" xmlns:dc="http://purl.org/dc/elements/1.1/" xmlns:opf="http://www.idpf.org/2007/opf"><metadata>
          <title>Wrong namespace</title><dc:title> A &amp; B </dc:title><dc:creator> Writer </dc:creator>
          <dc:language>pt-BR</dc:language><dc:publisher>Publisher</dc:publisher>
          <dc:identifier>arbitrary-id</dc:identifier><dc:identifier opf:scheme="ISBN">9781234567890</dc:identifier>
          <dc:description>Some <b>nested</b> text.</dc:description>
        </metadata></package>"#;
        let data = parse_opf(text).unwrap();
        assert_eq!(data.title, "A & B");
        assert_eq!(data.author, "Writer");
        assert_eq!(data.language, "pt-BR");
        assert_eq!(data.publisher, "Publisher");
        assert_eq!(data.isbn, "9781234567890");
        assert_eq!(data.description, "Some nested text.");
        assert!(parse_opf("<package><metadata/></package>").is_err());
    }

    #[test]
    fn rejects_dtd_and_unsafe_rootfiles() {
        assert!(
            xml(r#"<!DOCTYPE x [<!ENTITY e SYSTEM "file:///etc/passwd">]><x>&e;</x>"#).is_err()
        );
        for path in [
            "../book.opf",
            "/book.opf",
            "a/../book.opf",
            "https://host/book.opf",
            "a\\b.opf",
        ] {
            let text = format!(
                r#"<container xmlns="{CONTAINER}"><rootfiles><rootfile full-path="{path}" media-type="application/oebps-package+xml"/></rootfiles></container>"#
            );
            assert!(rootfile(&text).is_err());
        }
        let text = format!(
            r#"<container xmlns="{CONTAINER}"><rootfiles><rootfile full-path="OPS/book.opf" media-type="application/oebps-package+xml"/></rootfiles></container>"#
        );
        assert_eq!(rootfile(&text).unwrap(), "OPS/book.opf");
    }

    #[test]
    fn pdf_only_reads_supported_fields() {
        let data = parse_pdfinfo(
            "Title:  A: book\nAuthor: Alice\nSubject: A description\nCreator: Tool\nLanguage: en\nPublisher: Unknown\nISBN: 123\n",
        );
        assert_eq!(data.title, "A: book");
        assert_eq!(data.author, "Alice");
        assert_eq!(data.description, "A description");
        assert!(data.language.is_empty() && data.publisher.is_empty() && data.isbn.is_empty());
        assert!(pdf_failure(Some(1), "Incorrect password").contains("Password"));
        assert!(pdf_failure(Some(1), "Unsupported feature").contains("Unsupported"));
        assert!(pdf_failure(Some(1), "Syntax Error").contains("Corrupt"));
    }

    #[test]
    fn fields_have_utf8_byte_limits() {
        let text = format!(
            r#"<package xmlns="{OPF}" xmlns:dc="{DC}"><metadata><dc:title>{}</dc:title><dc:language>{}</dc:language><dc:description>{}</dc:description></metadata></package>"#,
            "é".repeat(400),
            "é".repeat(40),
            "é".repeat(1500)
        );
        let data = parse_opf(&text).unwrap();
        assert_eq!(data.title.len(), 500);
        assert_eq!(data.language.len(), 24);
        assert_eq!(data.description.len(), 2000);
    }
}
