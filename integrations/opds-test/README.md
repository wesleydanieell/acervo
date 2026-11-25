# OPDS acquisition fixtures

Two equivalent static acquisition feeds reference the original synthetic EPUB from integrations/test-source. The manifests are optional test sources. Install either manifest through Extensões > Instalar JSON, search Acervo, and link the result to the synthetic catalog entry in a disposable library. They intentionally return the same fixture for every query. They do not represent a live external searchable catalog.

Both feeds and manifests are exercised offline by Rust tests. Use the regular acquisition workflow for actual UI testing.
