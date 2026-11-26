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


### Revisão física da proposta estrutural

`library_restructure/snapshot.rs` observa metadados da raiz, ancestrais de origens/destinos, entradas do inventário e caminhos ausentes, em mapa ordenado. O SHA-256 inclui versão do formato, raiz canônica, operações em ordem e observações; é uma revisão física e não um hash de conteúdo, token de aprovação ou reserva de mutação. Observações repetidas divergentes retornam erro; diretórios são observados antes e depois da leitura. A chamada de prévia aceita `expectedFilesystemRevision` opcional e rejeita revisão divergente. Em Unix a revisão inclui dev/inode/ctime; outras plataformas usam tamanho e timestamps disponíveis (menor garantia). A futura execução precisa de proteção contra TOCTOU, validação de identidades persistidas, snapshots do catálogo/fila/regras e coordenação de mutações; a revisão não autoriza escrita.


### Revisão lógica e composta

A análise de referências reutiliza a leitura das fichas JSON para projetar a UI e calcular a revisão, sem reler o arquivo para o hash. A revisão lógica inclui os campos existentes do modelo SQLite (ordenados por ID), fichas JSON interpretadas e índices tipados completos. Não equivale a um snapshot exato de armazenamento para rollback. A fila usa uma projeção ordenada por ID dos registros pertinentes à raiz ou não vinculados, exclui progresso volátil e histórico salvo, e preserva o estado/intenções de importação na revisão. A revisão composta vincula essas informações, as regras completas fornecidas, os impactos e a revisão física; `expectedPlanRevision` permite revalidação somente leitura.

Referências físicas afetadas são verificadas sem seguir links simbólicos; registros manuais têm sua identidade comparada com a observada. Referências órfãs/identidades divergentes aparecem como bloqueios diagnósticos. Fingerprints de edições, coordenação global, snapshot consistente entre módulos, comparação imediatamente antes de escrever e journal exato permanecem requisitos do futuro executor.


### Coordenação de mutações por biblioteca

`library_mutations::Reservation` registra a raiz canônica em memória; raízes iguais ou ancestral/descendente conflitam, raízes independentes podem obter reservas simultâneas. A aquisição é não bloqueante quanto a operações em andamento: retorna erro de biblioteca ocupada. RAII libera ao sair, inclusive em erro. A ordem é reserva da biblioteca → locks internos dos módulos (manual, listas, edições ou fila conforme operação). O mutex do registro de reservas não permanece adquirido durante I/O nem durante a operação. Movimentos em lote e suas reversões recebem a mesma reserva por referência, sem aquisição recursiva. A reserva cobre criação de pasta, movimento simples/lote, organização JSON/inteligente, edição manual, importação de listas, associação explícita de catálogo, importação/recovery de downloads e prévia estrutural. Os helpers internos de associação e edição chamados pela importação continuam usando seus locks locais; os chamadores externos devem adquirir a reserva antes.

Limites: a coordenação é somente no processo, não controla mudanças externas, outros processos, aliases físicos não resolvidos por canonicalize (ex.: bind mounts), cache derivado ou mudanças da fila que não importam arquivos. Locks globais existentes podem serializar partes de operações em raízes independentes. Regras ainda estão em localStorage. Não há reserva durável, transação distribuída, journal ou executor estrutural. Importação automática concorrente permanece pronta e é suspensa com mensagem para ação manual. Não alterar a ordem de aquisição nem chamar comandos que adquirem reserva de dentro de uma operação reservada.


### Integridade de edições e destinos futuros

A verificação de edições reaproveita `library_editions::verify_book`, limitando a leitura ao tamanho esperado (mais um byte para detectar crescimento) e validando metadados durante o hash. O planejador reserva no máximo 1 GiB e 512 tentativas de verificação de edições afetadas por proposta, em ordem de caminho; excedentes são bloqueios diagnósticos `editionVerificationDeferred`. O contador checked representa tentativas, não sucessos; arquivos ausentes são tratados separadamente. A verificação manual tem seu próprio custo, não incluído nesse orçamento. Não substitui revalidação antes da escrita nem resolve condições de corrida externas.

`downloadImpact.destinationConflicts` agrupa destinos prospectivos por caminho completo, considerando destino planejado e da ficha, e reporta convergências novas de origens distintas com IDs distintos da mesma raiz. Históricos, outras raízes e intenções de importação existentes são excluídos (intenções têm diagnóstico próprio); avulsos não recebem raiz presumida. Igualdade é textual por caminho canônico relativo; colisões com arquivos físicos após aplicação e equivalência de nomes case-insensitive permanecem pendentes.


### Journal estrutural: primeiro incremento

`library_journal.rs` implementa intenção e log append-only por biblioteca no diretório de serviço protegido. Cada registro imutável tem versão, número sequencial, checksum anterior, evento e checksum SHA-256; leitura limitada a 128 etapas, 516 registros publicados, 256 KiB por registro e 1032 entradas totais. A criação/publicação usa arquivo temporário exclusivo, sync_all, rename sem substituição e sincronização de diretório. Falhas de sincronização não autorizam iniciar a próxima etapa. O leitor rejeita lacunas, registros inválidos, transições ilegais e diretório vazio; arquivos temporários regulares de publicação interrompida não são considerados checkpoints.

Estados: Applying(completed,inFlight), RollingBack(remaining,inFlight), Committed, RolledBack. Prepared antecede qualquer escrita estrutural; BeginStep e BeginCompensation antecedem a respectiva ação; StepApplied/StepCompensated confirmam o resultado. StepNotApplied/CompensationNotApplied exigirão evidência física/persistida de não aplicação no executor futuro. Não há autorização para executar ou compensar um inFlight ambíguo apenas por conhecer seu índice. Compensações ocorrem em ordem inversa. Estados terminais não aceitam eventos adicionais.

O comando inspect_library_journal apenas consulta o estado, sem criar arquivos. O coordenador consulta o journal após reservar a raiz, sem manter o mutex global de reservas; pendência ou erro de leitura rejeita a operação. A inspeção independente pode observar uma criação em andamento e responder erro transitório. O bloqueio persistido é por raiz consultada; ainda não descobre journals pendentes em raízes ancestrais/descendentes após reinício. Não há bypass de recuperação disponível. O writer interno não é chamado por comandos produtivos; em plataformas sem sincronização de diretórios implementada, falha antes de criar. O encadeamento detecta corrupção e lacunas, mas não é autenticação contra quem pode reescrever o journal nem prova contra remoção de um sufixo inteiro. Ainda não há snapshots exatos, identidade forte de origem na intenção, arquivamento/rotação, execução, recuperação automática ou undo/redo.
