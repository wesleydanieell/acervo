# Acervo reference extension repository

Add this immutable preview index in Extensões → Repositórios:

https://raw.githubusercontent.com/wesleydanieell/acervo/160c11ccb3b45a0e4cf2a2148b5e5f37e892290d/integrations/extension-repository/index.json

It contains Gutenberg (curated public-domain works) and JSON, OPDS and HTML test sources. Use a disposable library for tests. Review the declared endpoints and download domains before installation. The pinned index is a reference snapshot, not a continuously updated official repository.

## Publish a repository

Host an index.json and manifests on the same public HTTPS origin. Each entry gives id, name, version, manifestUrl and SHA-256 of the exact raw manifest bytes (for example, sha256sum manifest.json). Manifest URLs should be immutable. A stable index URL can advertise newer versions; the user confirms each update. Existing local extensions with the same ID must be removed explicitly before switching origin.

The hash verifies integrity against the retrieved index, not publisher identity. There are no signatures, trust keys or automatic updates in this preview. Never install an index from an untrusted publisher. Only declarative manifests are accepted.
