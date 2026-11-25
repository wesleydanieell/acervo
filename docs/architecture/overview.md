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


## Planejamento estrutural da biblioteca

O contrato de prévia em `library_restructure.rs` recebe operações tipadas e valida o inventário de subárvores. `library_reference_impact.rs` calcula o impacto de prefixos relativos em catálogo SQLite, listas JSON, índices manuais e edições. Não há comando de aplicação estrutural. O mapa é simultâneo, sem cascata e sem substituição textual parcial. A UI e a IA devem consumir o mesmo contrato, sem replicar validação no frontend.

O caminho planejado de uma obra é independente de suas associações físicas. Identidades de aquisição incluem o destino planejado, por isso o executor não pode modificar apenas as fichas. A política inicial de prévia está em `downloads/restructure.rs`; diferencia registros históricos de destinos atuais, bloqueia importações pendentes e evita adivinhar raízes de downloads sem ficha. Regras permanecem em localStorage por raiz e sua prévia tipada depende de snapshot fornecido pelo frontend. A chamada estrutural usa spawn_blocking para não bloquear a thread da UI. O próximo incremento deve definir snapshots exatos, coordenação da fila e regras, identidade física e recuperação durável; cache é derivado e deve ser invalidado/reconciliado, não tratado como identidade primária. Consulte [especificação](../product/library-workspace.md) e [registro de continuidade](../development/handoff.md).
