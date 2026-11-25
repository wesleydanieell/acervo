# Acervo Extension SDK — preview v1

Use manifest.example.json as a starting point. The JSON Schema is for editor assistance, not a replacement for Rust validation. An extension is currently a declarative JSON manifest, not an executable or ZIP package.

- schemaVersion: manifest structure.
- apiVersion: host interface compatibility.
- version: extension release version, numeric MAJOR.MINOR.PATCH.
- adapter: json (only available adapter).
- Existing manifests without these fields continue to work.
- Source capabilities are computed by the host; declaring a download field does not guarantee that every result has a file.
- All endpoints must use public HTTPS. The host blocks credentials, redirects, proxies and private network answers. Use downloadDomains to limit downloads to exact HTTPS hosts. Omission or null preserves legacy public HTTPS policy; [] denies all downloads. This is not a granular consent dialog. Previously queued transfers keep their authorized URL.
- Downloads, paths, catalog updates and persistence remain under Acervo control.

See ../../docs/SOURCE_EXTENSIONS.md and ../../docs/architecture/extension-api.md. Replace the example URL before using it; the sample is a template, not an actual provider.

Conformance tests are in src-tauri/src/extension_host.rs and source_search.rs. Run cargo test --manifest-path src-tauri/Cargo.toml --lib from the repository root.


## Pagination and remote identifiers

Optional remoteIdField maps a stable provider record identifier (string or unsigned integer). IDs are scoped by source; format and download URL still distinguish editions. Do not use an unstable array index.

For paginated APIs, add pagination.initialCursor and pagination.nextCursorField, and include {cursor} in searchPath. The adapter encodes the cursor, never follows a returned URL directly, and stops when the cursor is missing, invalid, or unchanged. Use only APIs providing a token or page number, not an absolute next-page URL. Local static search is not paginated.

```json
{
  "searchPath": "/books?q={query}&page={cursor}",
  "pagination": {
    "initialCursor": "1",
    "nextCursorField": "/nextPage"
  },
  "remoteIdField": "/id",
  "downloadDomains": ["files.example.org"]
}
```

Search can aggregate multiple sources; subsequent pages are requested independently. The host rejects a cursor for more than one source. Changing query or filters invalidates the current search.
