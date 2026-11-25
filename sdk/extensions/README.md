# Acervo Extension SDK — preview v1

Use manifest.example.json as a starting point. The JSON Schema is for editor assistance, not a replacement for Rust validation. An extension is currently a declarative JSON manifest, not an executable or ZIP package.

- schemaVersion: manifest structure.
- apiVersion: host interface compatibility.
- version: extension release version, numeric MAJOR.MINOR.PATCH.
- adapter: json, opds or html.
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


## OPDS 1.2 and 2.0 (acquisition feeds)

Use manifest.opds.example.json as a starting point. The endpoint must return an acquisition feed for the encoded search query; no JSON selectors or custom pagination configuration are necessary. The host negotiates OPDS JSON and Atom XML, extracts supported directly downloadable editions and follows a next link only within the original HTTPS origin. Explicit downloadDomains is required, with an empty list disabling all acquisition results in this preview. OPDS navigation trees, OpenSearch discovery, authentication, loans and DRM are not supported by this initial adapter. A source should provide a known search endpoint instead of a generic catalog root.

Reference test manifests live in integrations/opds-test. They use static synthetic feeds and do not implement server-side keyword search.

Specifications: https://specs.opds.io/opds-1.2.html and https://specs.opds.io/opds-2.0.html.

## HTML declarative sources (preview)

Use manifest.html.example.json with an authorized static search endpoint. The html mapping defines CSS selectors for result containers, titles, authors, languages, remote IDs, download anchors and optional next-page anchors. itemSelector and titleSelector are required. All other selectors are optional. Links must use direct a[href] elements; file format comes from the URL path suffix and the downloaded content is independently validated. Search results without direct acquisitions are metadata only; format filters exclude them. Download hosts must be explicitly declared. Next pages must stay on the original HTTPS origin.

The host parses UTF-8 HTML without executing JavaScript, opening browser sessions, loading assets, logging in, submitting forms, or traversing book detail pages. Site-specific automation must respect the site's rules. A provider's search endpoint is responsible for applying the query. Each selector uses a restricted CSS subset: tags, IDs, classes, simple attributes, descendant and child combinators. Pseudo-classes, escapes, selector lists, wildcards and sibling combinators are excluded. The first eligible download per result container is returned. Selector syntax and complexity are validated by Rust; the schema is only editor assistance. The response is capped at 2 MB and 40 results.

The static fixture in integrations/html-test can be installed manually; the query is intentionally ignored by that test endpoint.

## Repository index v1 (preview)

Publish a JSON index on a public HTTPS origin with schemaVersion: 1, name and extensions. Each entry contains id, name, version, manifestUrl (absolute HTTPS on the index origin), sha256 (64 lowercase hexadecimal digits). The manifest must declare downloadDomains, even when empty, and match identity/version/name exactly. Use SHA-256 over the exact raw bytes, including whitespace. Installation re-fetches the index and manifest and verifies the reviewed hash. Updates must come from the same configured index URL, have a higher numeric version, and be confirmed by the user; enabled state is preserved. Removing an index does not uninstall its sources.

See ../../integrations/extension-repository for a reproducible public example. The index hash is not a signature; publisher trust remains an explicit user decision.

The repository.schema.json file provides editor validation; the Rust validator additionally checks unique IDs, exact origin and byte limits.


## Multiple editions (JSON preview)

See manifest.editions.example.json. For one work record containing an array of downloadable editions, configure editions.itemsField, formatField and downloadUrlField; optional edition-relative selectors include languageField, publisherField and labelField. The parent record must have a stable remoteIdField. Explicit downloadDomains are required; the old top-level format/download selectors are mutually exclusive with this mapping. Each edition is validated separately and normalized to SourceBook with optional publisher/editionLabel. When no edition matches, metadata-only results are possible without a format filter. The search page is capped at 40 normalized results, not 40 works.

Discovery groups only the same source, exact remote ID, title and author. Without an ID, only exact duplicate results collapse. OPDS direct links with the same identifier also group; HTML still returns the first eligible link per container. No cross-source fuzzy matching or separate detail/edition endpoints exist yet. Labels and publishers are display metadata, not automatic changes to the local catalog. A second download with the same title and format may require a different destination; existing files are never overwritten.
