# Workspace de biblioteca — especificação e entregas

Objetivo: transformar Acervo em um editor e organizador de bibliotecas físicas e catálogos, intuitivo como um explorador de arquivos, mas com operações em lote, planejamento de hierarquia, inteligência de metadados, segurança e recuperação. A pasta real continua sendo a fonte dos arquivos; o catálogo não é uma cópia desconectada.

## Princípios de produto

- Navegar, selecionar, editar uma proposta, revisar e aplicar são ações distintas. Não exigir JSON para as operações manuais comuns.
- O editor deve operar sobre arquivos, subárvores, pastas vazias e obras planejadas. Respeitar arquivos auxiliares e não bibliográficos; nunca descartá-los implicitamente.
- O plano pode ser moldado livremente antes de qualquer alteração no disco. A execução é explícita, com impacto no catálogo visível, sem sobrescrita, exclusão automática ou correção silenciosa.
- Uma mudança de caminho não deve criar uma nova identidade de obra/edição, perder vínculos, metadados, prioridades ou deixar destinos planejados desatualizados.
- UI consistente em Acervo, Liquid Glass (macOS/iOS) e Material 3 (Niri/Noctalia), em claro e escuro. Mesmas ações, hierarquia e acessibilidade; cada aparência pode ter movimento e superfícies próprios.
- Sem comprometer desempenho: virtualização, efeitos locais e movimento reduzido. O núcleo não depende de animações nem de provedor de IA.

## Etapa 1 — Navegação e seleção em lote

**Em desenvolvimento** — branch feat/library-workspace.

- Selecionar por clique, modificadores, intervalo na ordem completa do resultado, Ctrl/Cmd+A, inversão, limpar, manter somente filtro atual. Mostrar quantidade total, visível e fora do filtro; não confundir selecionado com montado na tela.
- Selecionar livros por pasta e subárvore; alternar pasta direta/recursiva; expandir/recolher árvore, busca de pastas, breadcrumbs e contagem.
- Destinos pesquisáveis em explorador hierárquico, com caminho completo, prévia origem → destino, retirada individual do lote, conflitos e confirmação. Lotes iniciais de até 100; preservar a seleção restante, sem ocultar limite.
- Drag-and-drop de livros e seleção; manter feedback de destino e impedir arraste acidental. Não interpretar arrastar como autorização para sobrescrever.
- Preparar organização por IA com escopo explícito de seleção, inventário relativo e JSON exato aprovado, sem envio automático.
- Melhorar hierarquia dos controles, seleção visual, expansão, menus e transições; testar grade/lista, agrupamento, teclado e janelas estreitas.

Aceite: 500+ itens, seleção atravessa paginação, filtros e virtualização; lote de 220 mantém os 120 restantes após primeiro lote de 100; trocar raiz limpa seleção e invalida prévias; conflitos não alteram arquivos; menus acessíveis nos três estilos. A primeira implementação já compila e possui testes unitários; validação nativa do fluxo ainda pendente.

## Etapa 2 — Núcleo de planejamento e coordenação

**Iniciada, somente leitura; liberação depende da etapa 1 estabilizada.**

Primeiro incremento: comando interno `preview_library_restructure` com operações tipadas `createFolder` e `moveFolder` (renomear é mover para um novo caminho). A prévia contabiliza livros, arquivos auxiliares e pastas, incluindo vazias; aceita destinos em pastas propostas anteriormente. Rejeita links simbólicos, caminhos protegidos, destinos existentes, ciclos e subárvores sobrepostas. Limites iniciais: 100 operações e 50.000 entradas no inventário. A revisão física determinística (filesystemRevision) vincula a raiz canônica, a ordem das operações, o inventário e os ancestrais observados, inclusive ausências de destinos. Repetir a prévia com expectedFilesystemRevision exige revisão igual; mudanças observadas invalidam a proposta. O hash representa metadados, não conteúdo; em Unix usa também dispositivo, inode e ctime. Não substitui snapshot consistente de todos os dados, revalidação imediatamente antes de escrever ou proteção contra condições de corrida. Não há executor, integração visual ou token de aprovação neste incremento; a etapa 3 já adiciona análise de referências em leitura. Movimentos encadeados e consolidação permanecem fora do contrato inicial.

- Criar representação tipada, versionada, imutável de plano, raiz canônica, identidade de origem, caminho final, operações, dependências, impacto, avisos e conflitos. A IA e a UI produzem propostas, nunca instruções de shell.
- Suportar criar pastas, mover/renomear livros preservando formato, mover/renomear subárvores inteiras, e preparar consolidação de pastas. Operações de união não devem sobrescrever arquivos. Pastas vazias e arquivos auxiliares são parte do inventário.
- Validar componentes de caminho, limites, nomes inválidos, proteção do catálogo, links simbólicos, destino inexistente/plano de criação, ciclos, mover para descendente, conflitos de nomes (inclusive no filesystem de destino), dependências e alterações externas. Bloquear operação não suportada em vez de improvisar.
- Coordenação inicial no processo implementada: reserva exclusiva por raiz canônica e raízes sobrepostas, usada pelas principais entradas de escrita e pela prévia estrutural. Lotes retêm a reserva até concluir ou reverter. Operações conflitantes retornam erro de biblioteca ocupada. Não abrange processos externos, alterações da fila fora de importações, regras em localStorage ou recuperação durável.
- Coordenar mutações por biblioteca e com importações; preparar prévia sem escrita e revalidar identidade/estado antes da execução. A aprovação vincula o plano exato, não apenas a quantidade de itens.
- Separar planner, executor, remapeamento de referências e recuperação em módulos Rust. Reaproveitar verificações existentes, não duplicar o motor no React.

Aceite: testes de planos sem efeitos colaterais, colisões, ciclos, subárvores, pastas vazias, nomes Unicode, modificações externas, operações concorrentes e limites. Nenhuma operação estrutural habilitada ao usuário antes de cumprir etapas 3 e 4.

## Etapa 3 — Integridade do catálogo e remapeamento de caminhos

**Iniciada, somente leitura; execução ainda bloqueada.**

O módulo `library_reference_impact.rs` fornece o mapa de prefixos por componentes e a análise de impacto integrada a `preview_library_restructure`. A prévia apresenta ID da obra, título, associações físicas e destinos planejados alterados separadamente, além de chaves de metadados manuais e edições. Inclui destinos inexistentes. Não encadeia substituições, não modifica dados e rejeita colisões de índices, novas ambiguidades de identidade nas listas, associações inválidas e caminhos não canônicos. A prévia identifica referências afetadas a arquivos ausentes, simbólicos ou inválidos e compara a identidade de registros manuais com o arquivo atual; divergências são bloqueios diagnósticos. A prévia verifica SHA-256 das edições afetadas, com orçamento de 1 GiB de leitura reservada e 512 tentativas por proposta; excedentes recebem bloqueio de verificação pendente. Edições divergentes ou ilegíveis recebem bloqueio distinto. O limite é exclusivo das edições, não do inventário ou da verificação manual. A consistência entre snapshots e a escrita ainda precisam ser verificadas pelo executor futuro.

A revisão composta `planRevision` inclui a revisão física, os dados lógicos lidos do SQLite (campos do modelo de obra, ordenados por ID), as fichas JSON completas interpretadas, os índices de metadados manuais/edições, o estado pertinente da fila, as regras fornecidas e os impactos. `expectedPlanRevision` opcional rejeita divergências ao repetir a prévia. Contadores de transferência e histórico salvo não fazem parte da revisão da fila; alterações de estado, destinos, nome de arquivo e intenção de importação fazem. Não é um backup exato dos bytes, não inclui colunas SQLite fora do modelo nem campos externos às fichas JSON, e não garante um snapshot simultâneo. Ausência de regras continua significando não analisado. Nenhuma escrita estrutural foi habilitada.

A fila de downloads referencia a obra por raiz, título, autor e destino (e ID legado quando aplicável); IDs negativos das listas são posicionais, não UUIDs. Alterar destinos exige atualizar as referências de aquisição de forma coordenada ou bloquear a operação. A prévia agora inclui `downloadImpact` e `ruleImpact`: a fila é consultada em memória, sem gravar o registro, e as regras são analisadas quando o frontend fornece seu snapshot tipado (null significa não analisado, não ausência de impacto). Destinos de regras desativadas também são incluídos. Downloads salvos são históricos e não são reescritos; downloads de outra raiz são ignorados. Downloads sem raiz conhecida e destino afetado são bloqueados; importações em andamento ou pendentes de recuperação na raiz (ou sem raiz comprovada) são bloqueadas. Transferências ativas afetadas e fichas de catálogo desatualizadas são bloqueios. O cálculo não reserva a fila, nem garante que o snapshot permaneça atual. Colisões de destinos futuros na fila, persistência coordenada de regras e invalidação do cache ainda precisam de implementação.

- Gerar mapa de caminho anterior → novo, com remapeamento de prefixos por componentes (A não afeta AB), incluindo descendentes e destinos planejados ainda inexistentes.
- Atualizar referências do catálogo legado SQLite (arquivos encontrados, destino/local indicado), listas JSON (vínculos, destinos), identidade de obras e edições, metadados manuais, cache, regras e destinos de organização.
- Diferenciar localização física, localização planejada e histórico de eventos. Se um único arquivo é movido, não mudar arbitrariamente o destino de uma obra com múltiplas edições; se uma árvore é explicitamente reestruturada, remapear seus destinos planejados, inclusive obras ainda não baixadas. Expor política e impacto na prévia.
- Tratar fila/downloads: remapear destinos futuros apenas quando seguro e com mesma raiz; bloquear operações incompatíveis com importação ativa ou recuperação pendente. Preservar histórico de importação e não confundir caminho histórico com atual.
- Atualizar seleção, breadcrumbs, árvore, filtros, detalhes e indexação após confirmação do backend. Reconciliar alterações externas do filesystem com identidade comprovada; se ambíguo, sinalizar referência órfã para revisão, não inventar associação.

Aceite: fixtures contendo uma obra com várias edições, obras futuras, metadados, listas e download pendente; mover/renomear árvore mantém todas as referências pertinentes e preserva IDs, arquivos auxiliares e caminhos não afetados. Catálogo e filesystem sem divergência silenciosa.

## Etapa 4 — Execução durável, histórico e desfazer

**Em implementação; requisito para liberar edição estrutural.**

Primeiro incremento: journal append-only em `99 - Catálogo/.acervo-structural-journal` com intenção versionada, etapas tipadas e checkpoints de aplicação/compensação encadeados por SHA-256. Registros publicados sem sobrescrita após sincronização do arquivo e diretório. A consulta `inspect_library_journal` é somente leitura. A reserva da mesma raiz rejeita journal pendente, incompleto ou corrompido. Já existe armazenamento de imagens exatas antes/depois (ausência ou bytes com hash), com captura e validação anterior dos três recursos JSON locais e evidência de identidade de origem em Unix. SQLite já possui adaptador interno de snapshots lógicos transacionais dos campos de referência, restauração condicional e inspeção de recuperação. Fila e regras ainda precisam de adaptadores específicos. Não há executor, recuperação automática ou arquivamento de histórico. Escrita experimental do journal permanece interna e somente testada em Linux.

- Persistir intenção validada e estado anterior exato antes de tocar dados; checkpoints duráveis, identificação do que foi efetivamente aplicado, erros localizados e recuperação após reinício.
- Orquestrar filesystem, SQLite e JSON com compensações explícitas. Não prometer uma transação atômica única entre sistemas diferentes; impedir nova mutação conflitante enquanto houver recuperação pendente.
- Histórico de operações com data, origem, destino, impacto e resultado. Desfazer/refazer somente após verificar identidades e referências e apresentar conflito se o estado mudou; nenhuma sobrescrita na reversão.
- Cancelamento entre passos seguros, pausa/progresso em lotes, recuperação idempotente. Expor resultado parcial de forma legível; nunca alegar sucesso total se apenas parte foi aplicada.
- Preservar backup e estados anteriores relevantes. Exclusão de arquivos permanece fora do escopo desta etapa.

Aceite: injeção de falha em cada fronteira de escrita, interrupção/reinício, reversão exata de destinos planejados, mudança externa durante operação, falha de rollback e retomada repetida. Validar em biblioteca fictícia e nenhuma modificação na biblioteca real sem ação explícita do usuário.

## Etapa 5 — Editor visual de estrutura

**Planejada; prévias podem evoluir antes, aplicar exige etapas 2–4.**

- Workspace dedicado com árvore atual e proposta editável, explorador de arquivos e painel de propriedades/impacto. Alternar visualização única e comparação lado a lado conforme espaço disponível.
- Criar e renomear pastas na proposta, mover/reparentar por arraste, selecionar ramos, mover lotes, editar nomes, aplicar padrões de nomes e montar agrupamentos por assunto, autor, série, idioma, formato ou critérios combinados.
- Operações de consolidar/dividir pastas, mover entre níveis, criar estrutura a partir de modelo, regras reutilizáveis e exceções. Indicar destinos inexistentes, colisões, mudanças de caminho e obras afetadas.
- Prévia antes/depois, busca e filtros por tipo de mudança/erro, editar cada ação e aprovar subconjuntos respeitando dependências. Botões claros de cancelar proposta, salvar rascunho e aplicar.
- Animações de expansão/recolhimento, drag preview, indicação de destino, rearranjo de linhas/cards, morph entre regiões do editor, progresso e feedback de confirmação. Limitar ao viewport e manter ações imediatas.

Aceite: remodelar árvore com vários níveis e centenas de livros sem sair do editor, sem editar JSON manualmente, sem perder seleção ao navegar e sem alterar disco até aplicar. Operações bloqueadas exibem motivo e forma de corrigir.

## Etapa 6 — Organização inteligente e assistida por IA

**Planejada; proposta declarativa usa o mesmo planner e editor.**

- Pedidos em linguagem natural: organizar toda biblioteca, pasta, lote selecionado, obras ainda ausentes ou uma parte da hierarquia; reorganizar por assunto/autor/série, criar estrutura, preservar exceções e limites.
- Combinar regras determinísticas, metadados disponíveis e sugestões de IA. Mostrar justificativas, incertezas e itens sem classificação. Não fabricar metadados nem inventar arquivos.
- Plano editável com criação de pastas, remapeamento de referências, revisão de conflitos e aprovações parciais; executar em lotes dependentes e rastreáveis, sem necessidade de gerar múltiplos JSON manualmente.
- Modo manual local (copiar inventário/prompt e importar plano) e, posteriormente, integração opcional com provedor escolhido pelo usuário, consentimento explícito e indicação dos dados enviados. Não transmitir arquivos ou dados pessoais automaticamente.
- Guardar modelos de organização e regras automáticas reutilizáveis. Automação futura limitada por política explícita; sem mover/excluir silenciosamente.

Aceite: a mesma operação proposta manualmente ou por IA tem as mesmas validações, prévia, journal e recuperação. Entradas inválidas/maliciosas, caminhos fora do escopo e propostas desatualizadas não são executáveis.

## Etapa 7 — Revisão de duplicatas e polimento contínuo

- Integrar o fluxo de duplicatas ao editor: escolha de cópia, vínculos e edições protegidas, prévia de impactos e resolução reversível; nenhuma exclusão definitiva automática.
- Unificar seletores, menus, rótulos, nomes longos, estados vazios, diálogos e feedback em todas as telas. Liquid Glass sem cores decorativas indevidas; Material alinhado ao Noctalia; Acervo com identidade própria. Transições entre os três estilos e sincronização de paleta animada.
- Expandir virtualização para grade e grupos, morph de capas, grade/lista, ordenação/filtros, árvore e seleção; animar download/progresso/feed/estados. Preferir componentes compartilhados e limitar efeitos caros.
- Validar teclado, redução de movimento, resize, mudança de tema durante operação, clareza de nomes e menus, acervos grandes, mudanças externas e desempenho com métricas. Intercalar com etapas anteriores; não esperar uma grande reformulação final.

## Entregas e integração

- Uma branch por etapa coerente, vários Conventional Commits pequenos e auditáveis, PR agrupado quando o conjunto estiver pronto. Correções urgentes podem ter PR próprio se necessário; não abrir um PR por microajuste.
- Revisar e testar antes de mesclar. Mudanças de armazenamento precisam de regressão de dados e recuperação; alterações de UI precisam de smoke test nativo em perfis isolados. CI aprovado não substitui validação visual ou teste de integridade.
- Não instalar binários de branches experimentais no perfil normal. Instalar somente a versão integrada e validada. Registrar o que está pronto, o que depende de outra etapa e limitações conhecidas.

A prévia de aquisição também identifica novos destinos de arquivo coincidentes entre downloads vinculados à mesma raiz, considerando nome e destinos planejados/da ficha. Não confunde formatos/nomes distintos, registros históricos ou outras bibliotecas. Destinos anteriormente iguais não são classificados como nova colisão. Ainda não verifica colisões com arquivos físicos projetados nem equivalência de nomes em filesystems case-insensitive.

A inspeção de recuperação já possui reserva exclusiva interna e diagnóstico Before/After/Conflict/Unsupported da etapa interrompida, para imagens JSON e identidade de movimentação de pasta. Não aplica compensações nem altera checkpoints; criação existente sem prova de identidade continua conflito.

Os três recursos JSON locais agora possuem adaptador interno de planejamento e substituição condicional de arquivos existentes, preservando dados de referência e permitindo roundtrip dos bytes originais. O executor ainda precisa integrar os adaptadores ao journal; não há reorganização estrutural habilitada.
