const MAX_FILENAME_BYTES: usize = 240;
const MAX_SUFFIX_BYTES: usize = 72;
const MAX_SUFFIX_INPUT_BYTES: usize = 300;

/// Builds a portable filename without changing the legacy download naming rules.
/// Empty sanitized titles are rejected. Suffixes are optional and
/// receive their own byte budget before the title is truncated.
pub(crate) fn edition_filename(title: &str, format: &str, suffix: &str) -> Result<String, String> {
    let extension = format.to_ascii_lowercase();
    if !matches!(
        extension.as_str(),
        "pdf" | "epub" | "mobi" | "azw3" | "djvu"
    ) {
        return Err("Formato desconhecido para download.".to_owned());
    }
    if suffix.len() > MAX_SUFFIX_INPUT_BYTES {
        return Err("Complemento do nome excede 300 bytes.".to_owned());
    }

    let suffix = if suffix.trim().is_empty() {
        String::new()
    } else {
        let mut value = sanitize(suffix);
        if value.is_empty() {
            return Err("Complemento do nome inválido.".to_owned());
        }
        truncate_utf8(&mut value, MAX_SUFFIX_BYTES);
        value.truncate(value.trim_end_matches(['.', ' ']).len());
        format!(" - {value}")
    };

    let mut title = sanitize(title);
    if title.is_empty() {
        return Err("Título inválido para nome do arquivo.".into());
    }
    if is_windows_device_name(&title) {
        title.insert(0, '_');
    }
    let title_budget = MAX_FILENAME_BYTES - suffix.len() - 1 - extension.len();
    truncate_utf8(&mut title, title_budget);
    title.truncate(title.trim_end_matches(['.', ' ']).len());

    Ok(format!("{title}{suffix}.{extension}"))
}

fn sanitize(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_control() || matches!(ch, '/' | '\\' | '<' | '>' | ':' | '"' | '|' | '?' | '*')
            {
                '_'
            } else {
                ch
            }
        })
        .collect::<String>()
        .trim_matches(['.', ' '])
        .to_owned()
}

fn truncate_utf8(value: &mut String, budget: usize) {
    let mut end = value.len().min(budget);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
}

fn is_windows_device_name(value: &str) -> bool {
    let stem = value
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end_matches(' ');
    let upper = stem.to_ascii_uppercase();
    matches!(
        upper.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || upper
        .strip_prefix("COM")
        .or_else(|| upper.strip_prefix("LPT"))
        .is_some_and(|number| {
            matches!(
                number,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_only_supported_formats_and_normalizes_extension() {
        for format in ["PDF", "ePuB", "MOBI", "AzW3", "DJVU"] {
            assert_eq!(
                edition_filename("Book", format, "").unwrap(),
                format!("Book.{}", format.to_ascii_lowercase())
            );
        }
        for format in ["", "exe", ".pdf", " pdf", "pdf/../../exe", "pdf\0"] {
            assert!(edition_filename("Book", format, "").is_err());
        }
    }

    #[test]
    fn sanitizes_paths_controls_and_windows_punctuation() {
        let filename = edition_filename(" ../a\\b/<c>:d\"e|f?g*\n\0 . ", "pdf", "../v\\2").unwrap();
        assert_eq!(filename, "_a_b__c__d_e_f_g___ - _v_2.pdf");
        assert!(
            !filename
                .chars()
                .any(|ch| ch.is_control() || "/\\<>:\"|?*".contains(ch))
        );
    }

    #[test]
    fn avoids_hidden_empty_and_reserved_titles() {
        for title in ["", " . .. "] {
            assert!(edition_filename(title, "pdf", "").is_err());
        }
        assert_eq!(
            edition_filename("...secret. ", "pdf", "").unwrap(),
            "secret.pdf"
        );
        for title in [
            "con", "NUL", "AUX.txt", "PRN", "COM1", "lpt9", "COM¹", "CON .txt", "CONIN$", "CONOUT$",
        ] {
            assert!(edition_filename(title, "pdf", "").unwrap().starts_with('_'));
        }
        for title in ["console", "COM10", "LPT0", "book.CON"] {
            assert_eq!(
                edition_filename(title, "pdf", "").unwrap(),
                format!("{title}.pdf")
            );
        }
    }

    #[test]
    fn truncates_multibyte_title_by_bytes_and_keeps_extension() {
        for format in ["pdf", "epub", "mobi", "azw3", "djvu"] {
            let filename = edition_filename(&"📚漢é".repeat(100), format, "").unwrap();
            assert!(filename.len() <= MAX_FILENAME_BYTES);
            assert!(filename.len() >= MAX_FILENAME_BYTES - 3);
            assert!(filename.ends_with(&format!(".{format}")));
        }
        assert_eq!(
            edition_filename("book.exe", "PDF", "v1.exe").unwrap(),
            "book.exe - v1.exe.pdf"
        );
    }

    #[test]
    fn reserves_suffix_budget_before_truncating_title() {
        let suffix = "é📚".repeat(40);
        let filename = edition_filename(&"漢".repeat(200), "azw3", &suffix).unwrap();
        let (title, ending) = filename.split_once(" - ").unwrap();
        let suffix = ending.strip_suffix(".azw3").unwrap();
        assert_eq!(suffix.len(), MAX_SUFFIX_BYTES);
        assert_eq!(title.len(), 159);
        assert_eq!(filename.len(), MAX_FILENAME_BYTES - 1);
    }

    #[test]
    fn handles_optional_invalid_and_oversized_suffixes() {
        assert_eq!(
            edition_filename("Book", "pdf", " \t\n").unwrap(),
            "Book.pdf"
        );
        assert_eq!(
            edition_filename("Book", "pdf", " .edition. ").unwrap(),
            "Book - edition.pdf"
        );
        assert!(edition_filename("Book", "pdf", " ... ").is_err());
        assert!(edition_filename("Book", "pdf", &"a".repeat(300)).is_ok());
        assert!(edition_filename("Book", "pdf", &"a".repeat(301)).is_err());
        assert!(edition_filename("Book", "pdf", &"é".repeat(151)).is_err());
        assert!(edition_filename("Book", "pdf", &" ".repeat(301)).is_err());
    }

    #[test]
    fn distinct_suffixes_distinguish_long_identical_titles() {
        let title = "A".repeat(500);
        let first = edition_filename(&title, "pdf", "edition-1").unwrap();
        let second = edition_filename(&title, "pdf", "edition-2").unwrap();
        assert_ne!(first, second);
        assert!(first.ends_with(" - edition-1.pdf"));
        assert!(second.ends_with(" - edition-2.pdf"));
        assert_eq!(first.len(), MAX_FILENAME_BYTES);
        assert_eq!(second.len(), MAX_FILENAME_BYTES);
    }
}
