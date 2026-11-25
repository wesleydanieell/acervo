//! Offline validation of repository indexes and checksum-pinned manifests.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;

const MAX_INDEX_BYTES: usize = 256 * 1024;
const MAX_MANIFEST_BYTES: usize = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryIndex {
    pub schema_version: u32,
    pub name: String,
    pub extensions: Vec<RepositoryEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RepositoryEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub manifest_url: String,
    pub sha256: String,
}

/// Numeric ordering with the same version syntax accepted by sources::validate.
pub fn version_key(value: &str) -> Option<[u32; 3]> {
    let mut parts = value.split('.');
    let mut key = [0; 3];
    for component in &mut key {
        let part = parts.next()?;
        if part.is_empty()
            || part.len() > 9
            || (part.len() > 1 && part.starts_with('0'))
            || !part.bytes().all(|byte| byte.is_ascii_digit())
        {
            return None;
        }
        *component = part.parse().ok()?;
    }
    parts.next().is_none().then_some(key)
}

fn valid_name(name: &str) -> bool {
    !name.trim().is_empty() && name.len() <= 100 && !name.chars().any(char::is_control)
}

fn valid_checksum(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

pub fn parse_index(data: &[u8], index_url: &url::Url) -> Result<RepositoryIndex, String> {
    if data.len() > MAX_INDEX_BYTES {
        return Err("Repository index exceeds 256 KiB.".into());
    }
    if !crate::extension_network::safe_url(index_url.as_str())
        || index_url.fragment().is_some()
        || index_url.query().is_some()
    {
        return Err("Unsafe repository index URL.".into());
    }
    let index: RepositoryIndex = serde_json::from_slice(data)
        .map_err(|error| format!("Invalid repository index: {error}"))?;
    if index.schema_version != 1 {
        return Err("Repository index requires schemaVersion 1.".into());
    }
    if !valid_name(&index.name) || !(1..=100).contains(&index.extensions.len()) {
        return Err("Invalid repository name or extension count.".into());
    }
    let mut ids = HashSet::new();
    for entry in &index.extensions {
        if !(3..=64).contains(&entry.id.len())
            || !entry.id.starts_with(|c: char| c.is_ascii_lowercase())
            || !entry
                .id
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        {
            return Err("Invalid extension id.".into());
        }
        if !ids.insert(&entry.id) {
            return Err("Duplicate extension id.".into());
        }
        if !valid_name(&entry.name) || version_key(&entry.version).is_none() {
            return Err("Invalid extension name or version.".into());
        }
        if !valid_checksum(&entry.sha256) {
            return Err("Invalid SHA256 checksum.".into());
        }
        let manifest_url = url::Url::parse(&entry.manifest_url)
            .map_err(|_| "Manifest URL must be absolute HTTPS.".to_string())?;
        if !crate::extension_network::safe_url(&entry.manifest_url)
            || manifest_url.fragment().is_some()
            || manifest_url.query().is_some()
            || manifest_url.origin() != index_url.origin()
        {
            return Err("Manifest URL must be safe HTTPS on the repository origin.".into());
        }
    }
    Ok(index)
}

pub fn verify_manifest(
    entry: &RepositoryEntry,
    data: &[u8],
) -> Result<crate::sources::SourceManifest, String> {
    if data.len() > MAX_MANIFEST_BYTES {
        return Err("Manifest exceeds 64 KiB.".into());
    }
    let checksum = format!("{:x}", Sha256::digest(data));
    if !valid_checksum(&entry.sha256) || checksum != entry.sha256 {
        return Err("Manifest SHA256 checksum mismatch.".into());
    }
    let manifest: crate::sources::SourceManifest =
        serde_json::from_slice(data).map_err(|error| format!("Invalid manifest: {error}"))?;
    crate::sources::validate(&manifest)?;
    if manifest.adapter == crate::sources::AdapterKind::Local {
        return Err("Catálogos locais devem ser importados manualmente.".into());
    }
    if manifest.download_domains.is_none() {
        return Err("Repository manifests require explicit downloadDomains.".into());
    }
    if manifest.id != entry.id || manifest.name != entry.name || manifest.version != entry.version {
        return Err("Manifest identity does not match the repository entry.".into());
    }
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn manifest() -> Value {
        json!({
            "schemaVersion": 1, "id": "sample-source", "name": "Sample source",
            "version": "1.2.3", "baseUrl": "https://example.org",
            "searchPath": "/search?q={query}", "itemsPath": "items",
            "titleField": "title", "downloadDomains": []
        })
    }

    fn entry(data: &[u8]) -> RepositoryEntry {
        RepositoryEntry {
            id: "sample-source".into(),
            name: "Sample source".into(),
            version: "1.2.3".into(),
            manifest_url: "https://example.org/manifests/sample.json".into(),
            sha256: format!("{:x}", Sha256::digest(data)),
        }
    }

    fn index() -> RepositoryIndex {
        RepositoryIndex {
            schema_version: 1,
            name: "Example repository".into(),
            extensions: vec![entry(&serde_json::to_vec(&manifest()).unwrap())],
        }
    }

    fn parse(value: &RepositoryIndex) -> Result<RepositoryIndex, String> {
        parse_index(
            &serde_json::to_vec(value).unwrap(),
            &url::Url::parse("https://example.org/index.json").unwrap(),
        )
    }

    #[test]
    fn valid_index_round_trip_and_metadata_manifest() {
        let sample = index();
        assert_eq!(parse(&sample).unwrap(), sample);
        let data = serde_json::to_vec(&manifest()).unwrap();
        let verified = verify_manifest(&entry(&data), &data).unwrap();
        assert_eq!(verified.download_domains, Some(vec![]));
    }

    #[test]
    fn rejects_bad_checksum_and_mismatched_identity() {
        let data = serde_json::to_vec(&manifest()).unwrap();
        let mut sample = entry(&data);
        sample.sha256 = "0".repeat(64);
        assert!(
            verify_manifest(&sample, &data)
                .unwrap_err()
                .contains("checksum")
        );
        for field in ["id", "name", "version"] {
            let mut changed = manifest();
            changed[field] = json!(match field {
                "id" => "other-source",
                "name" => "Other",
                _ => "2.0.0",
            });
            let changed = serde_json::to_vec(&changed).unwrap();
            sample = entry(&changed);
            assert!(
                verify_manifest(&sample, &changed)
                    .unwrap_err()
                    .contains("identity")
            );
        }
    }

    #[test]
    fn rejects_legacy_omission_and_null_permissions() {
        for null in [false, true] {
            let mut value = manifest();
            if null {
                value["downloadDomains"] = Value::Null;
            } else {
                value.as_object_mut().unwrap().remove("downloadDomains");
            }
            let data = serde_json::to_vec(&value).unwrap();
            assert!(
                verify_manifest(&entry(&data), &data)
                    .unwrap_err()
                    .contains("downloadDomains")
            );
        }
    }

    #[test]
    fn rejects_unsafe_and_cross_origin_urls() {
        for address in [
            "/manifest.json",
            "http://example.org/m.json",
            "https://other.org/m.json",
            "https://example.org/m.json#fragment",
            "https://user:pass@example.org/m.json",
            "https://127.0.0.1/m.json",
            "https://localhost/m.json",
            "https://host.local/m.json",
            "https://example.org:8443/m.json",
            "https://example.org.evil.org/m.json",
        ] {
            let mut sample = index();
            sample.extensions[0].manifest_url = address.into();
            assert!(parse(&sample).is_err(), "{address}");
        }
        for address in [
            "http://example.org/index.json",
            "https://127.0.0.1/index.json",
            "https://example.org/index.json#fragment",
        ] {
            assert!(
                parse_index(
                    &serde_json::to_vec(&index()).unwrap(),
                    &url::Url::parse(address).unwrap()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn strict_versions_and_numeric_ordering() {
        assert_eq!(version_key("0.1.999999999"), Some([0, 1, 999999999]));
        assert!(version_key("1.10.0") > version_key("1.9.99"));
        for version in [
            "",
            "1.2",
            "1.2.3.4",
            "01.2.3",
            "1.02.3",
            "1.2.03",
            "1.2.-3",
            "1.2.+3",
            "1.2.3-beta",
            "1.2.3+build",
            "1.2.3 ",
            "1.2.１００",
            "1.2.1000000000",
        ] {
            assert_eq!(version_key(version), None, "{version}");
            let mut sample = index();
            sample.extensions[0].version = version.into();
            assert!(parse(&sample).is_err(), "{version}");
        }
    }

    #[test]
    fn rejects_duplicates_invalid_fields_and_counts() {
        let mut sample = index();
        sample.extensions.push(sample.extensions[0].clone());
        assert!(parse(&sample).is_err());
        for count in [0, 101] {
            sample = index();
            sample.extensions = vec![sample.extensions[0].clone(); count];
            assert!(parse(&sample).is_err());
        }
        sample = index();
        sample.schema_version = 2;
        assert!(parse(&sample).is_err());
        for name in ["".into(), " ".into(), "é".repeat(51)] {
            sample = index();
            sample.name = name.clone();
            assert!(parse(&sample).is_err());
            sample = index();
            sample.extensions[0].name = name;
            assert!(parse(&sample).is_err());
        }
        for id in [
            "ab".into(),
            "A-source".into(),
            "a_b".into(),
            "aé-source".into(),
            "a".repeat(65),
        ] {
            sample = index();
            sample.extensions[0].id = id;
            assert!(parse(&sample).is_err());
        }
        for checksum in [
            "a".repeat(63),
            "a".repeat(65),
            "A".repeat(64),
            "g".repeat(64),
        ] {
            sample = index();
            sample.extensions[0].sha256 = checksum;
            assert!(parse(&sample).is_err());
        }
    }

    #[test]
    fn rejects_unknown_fields_and_invalid_manifests() {
        for nested in [false, true] {
            let mut value = serde_json::to_value(index()).unwrap();
            if nested {
                value["extensions"][0]["extra"] = json!(true);
            } else {
                value["extra"] = json!(true);
            }
            assert!(
                parse_index(
                    &serde_json::to_vec(&value).unwrap(),
                    &url::Url::parse("https://example.org/index.json").unwrap()
                )
                .is_err()
            );
        }
        let mut value = manifest();
        value["searchPath"] = json!("unsafe");
        let data = serde_json::to_vec(&value).unwrap();
        assert!(verify_manifest(&entry(&data), &data).is_err());
        let data = b"not json";
        assert!(verify_manifest(&entry(data), data).is_err());
    }

    #[test]
    fn byte_limits_are_inclusive() {
        let mut data = serde_json::to_vec(&index()).unwrap();
        data.resize(MAX_INDEX_BYTES, b' ');
        let url = url::Url::parse("https://example.org/index.json").unwrap();
        assert!(parse_index(&data, &url).is_ok());
        data.push(b' ');
        assert!(parse_index(&data, &url).is_err());
        let mut data = serde_json::to_vec(&manifest()).unwrap();
        data.resize(MAX_MANIFEST_BYTES, b' ');
        assert!(verify_manifest(&entry(&data), &data).is_ok());
        data.push(b' ');
        assert!(verify_manifest(&entry(&data), &data).is_err());
    }
}
