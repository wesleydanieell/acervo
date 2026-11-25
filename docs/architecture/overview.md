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

## Associação de arquivos (incremento)

Fichas JSON podem associar arquivos físicos existentes por seleção e confirmação explícita. Rust valida o ID e os campos de identidade, arquivo dentro da raiz, conflitos com fichas JSON e referências legadas. A pasta planejada permanece intacta e o arquivo não é movido; gravação atômica com backup. O ID negativo atual depende da posição da ficha, portanto a comparação de identidade é obrigatória. Próximo passo: identidade estável persistente e vínculo de download à raiz/ficha de origem, com recuperação caso a gravação da associação falhe depois do download. SQLite legado não é alterado por este comando.


## Referência de aquisição persistente

A fila registra a raiz e a identidade exata da ficha JSON. A identidade é revalidada durante a associação e tolera reordenação da lista, mas não edição dos campos identificadores. As operações de escrita do catálogo JSON são serializadas no processo. O salvamento físico e a associação têm resultados distintos; falha de metadados não apaga o livro importado. UUIDs persistentes e recuperação transacional entre arquivo, ficha e histórico continuam no roadmap.

## Host de extensões

Consulte [Acervo Extension API e Host v1](extension-api.md). Os adaptadores implementam o contrato comum; catálogo, downloads e segurança de rede são serviços do aplicativo.
