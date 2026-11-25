# Arquitetura evolutiva

## Base atual
Tauri 2 / Rust, React 19 / TypeScript. App.tsx coordena navegação; CatalogCenter, SourcesCenter, DownloadsCenter separam áreas. Rust já possui catalog_lists, organization, sources, source_search, downloads, download_validation e covers. SQLite legado para consultas, listas JSON/CSV, integração Noctalia. Manter contratos existentes.

## Direção
- Modelo conceitual: obra -> edição -> arquivo físico; coleção virtual e raiz física independentes. Não é uma migração aprovada.
- Rust é autoridade para caminhos, validação, persistência e operações físicas. UI não decide segurança de caminhos.
- Evoluir fontes declarativas por manifesto versionado e capacidades; sem execução arbitrária no host.
- Transferir para staging, validar, resolver destino, confirmar conforme política, efetivar sem sobrescrita e atualizar associação. Separar estado de transferência de importação.
- Regras de organização com prévia, colisões, links simbólicos, falhas parciais e recuperação.
- Separar componentes UI e testar lógica pura; preservar tema e acessibilidade.
