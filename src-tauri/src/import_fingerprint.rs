//! Bounded content fingerprints for files selected for import.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::path::Path;

const MAX_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FileFingerprint {
    pub bytes: u64,
    pub sha256: String,
}

impl FileFingerprint {
    pub(crate) fn validate(&self) -> Result<(), String> {
        validate_length(self.bytes)?;
        if self.sha256.len() != 64
            || !self
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(
                "SHA-256 deve conter exatamente 64 caracteres hexadecimais minúsculos.".into(),
            );
        }
        Ok(())
    }
}

fn validate_length(bytes: u64) -> Result<(), String> {
    if bytes == 0 || bytes > MAX_BYTES {
        return Err("O arquivo deve ter entre 1 byte e 512 MiB.".into());
    }
    Ok(())
}

pub(crate) fn fingerprint(path: &Path) -> Result<FileFingerprint, String> {
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if !metadata.file_type().is_file() {
        return Err("A importação exige arquivo regular, não link simbólico.".into());
    }
    validate_length(metadata.len())?;

    let mut options = OpenOptions::new();
    options.read(true);
    // Prevent a replaced final path component from following a symlink or
    // blocking on a FIFO between the metadata check and open.
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK);
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        options.custom_flags(0x0020_0000); // FILE_FLAG_OPEN_REPARSE_POINT
    }
    let mut file = options.open(path).map_err(|error| error.to_string())?;
    let before = file.metadata().map_err(|error| error.to_string())?;
    if !before.file_type().is_file() {
        return Err("A importação exige arquivo regular.".into());
    }
    validate_length(before.len())?;
    if before.len() != metadata.len() {
        return Err("O tamanho do arquivo mudou ao abrir.".into());
    }

    // Read at most the initial length plus one byte to detect growth, even
    // when another process continually appends to the file.
    let mut reader = (&mut file).take(before.len() + 1);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    let mut bytes = 0_u64;
    loop {
        let count = match reader.read(&mut buffer) {
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            result => result.map_err(|error| error.to_string())?,
        };
        if count == 0 {
            break;
        }
        bytes += count as u64;
        hasher.update(&buffer[..count]);
    }
    let after = file.metadata().map_err(|error| error.to_string())?;
    if bytes != before.len() || after.len() != before.len() {
        return Err("O tamanho do arquivo mudou durante a leitura.".into());
    }
    let result = FileFingerprint {
        bytes,
        sha256: format!("{:x}", hasher.finalize()),
    };
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(0);

    struct Fixture(PathBuf);

    impl Fixture {
        fn new() -> Self {
            loop {
                let path = std::env::temp_dir().join(format!(
                    "acervo-import-fingerprint-{}-{}",
                    std::process::id(),
                    NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed)
                ));
                match fs::create_dir(&path) {
                    Ok(()) => return Self(path),
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(error) => panic!("Cannot create fixture: {error}"),
                }
            }
        }

        fn file(&self, name: &str, contents: &[u8]) -> PathBuf {
            let path = self.0.join(name);
            fs::write(&path, contents).unwrap();
            path
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn hashes_contents_and_detects_same_length_mismatch() {
        let fixture = Fixture::new();
        let path = fixture.file("book", b"abc");
        let first = fingerprint(&path).unwrap();
        assert_eq!(first.bytes, 3);
        assert_eq!(
            first.sha256,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        assert_eq!(first, fingerprint(&path).unwrap());
        fs::write(&path, b"abd").unwrap();
        let second = fingerprint(&path).unwrap();
        assert_eq!(first.bytes, second.bytes);
        assert_ne!(first, second);
    }

    #[test]
    fn rejects_empty_nonregular_and_oversized_files() {
        let fixture = Fixture::new();
        assert!(fingerprint(&fixture.file("empty", b"")).is_err());
        assert!(fingerprint(&fixture.0).is_err());
        let path = fixture.file("oversized", b"x");
        OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_len(MAX_BYTES + 1)
            .unwrap();
        assert!(fingerprint(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlinks() {
        let fixture = Fixture::new();
        let target = fixture.file("target", b"abc");
        let link = fixture.0.join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(fingerprint(&link).is_err());
        fs::remove_file(target).unwrap();
        assert!(fingerprint(&link).is_err());
    }

    #[test]
    fn validates_digest_and_length() {
        let valid = FileFingerprint {
            bytes: 1,
            sha256: "a".repeat(64),
        };
        assert!(valid.validate().is_ok());
        for digest in [
            String::new(),
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
            "é".repeat(32),
        ] {
            assert!(
                FileFingerprint {
                    sha256: digest,
                    ..valid.clone()
                }
                .validate()
                .is_err()
            );
        }
        for bytes in [0, MAX_BYTES + 1] {
            assert!(
                FileFingerprint {
                    bytes,
                    ..valid.clone()
                }
                .validate()
                .is_err()
            );
        }
        assert!(
            FileFingerprint {
                bytes: MAX_BYTES,
                ..valid
            }
            .validate()
            .is_ok()
        );
    }

    #[test]
    fn serde_round_trip_rejects_unknown_fields() {
        let value = FileFingerprint {
            bytes: 3,
            sha256: "a".repeat(64),
        };
        let json = serde_json::to_string(&value).unwrap();
        assert_eq!(
            serde_json::from_str::<FileFingerprint>(&json).unwrap(),
            value
        );
        let unknown = format!(r#"{{"bytes":3,"sha256":"{}","extra":true}}"#, value.sha256);
        assert!(serde_json::from_str::<FileFingerprint>(&unknown).is_err());
    }
}
