# Acervo Extension SDK — preview v1

Use manifest.example.json as a starting point. The JSON Schema is for editor assistance, not a replacement for Rust validation. An extension is currently a declarative JSON manifest, not an executable or ZIP package.

- schemaVersion: manifest structure.
- apiVersion: host interface compatibility.
- version: extension release version, numeric MAJOR.MINOR.PATCH.
- adapter: json (only available adapter).
- Existing manifests without these fields continue to work.
- Source capabilities are computed by the host; declaring a download field does not guarantee that every result has a file.
- All endpoints must use public HTTPS. The host blocks credentials, redirects, proxies and private network answers. The host currently does not enforce per-extension domain allowlists.
- Downloads, paths, catalog updates and persistence remain under Acervo control.

See ../../docs/SOURCE_EXTENSIONS.md and ../../docs/architecture/extension-api.md. Replace the example URL before using it; the sample is a template, not an actual provider.

Conformance tests are in src-tauri/src/extension_host.rs and source_search.rs. Run cargo test --manifest-path src-tauri/Cargo.toml --lib from the repository root.
