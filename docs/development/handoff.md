# Acervo — registro de continuidade

Atualizado em 2026-11-25. Este arquivo é o ponto de entrada para retomar o desenvolvimento em outra sessão. Atualizar a cada incremento, junto ao código, sem depender do histórico do chat.

## Retomar

- Repositório: wesleydanieell/acervo; checkout original: /home/wesley/Projects/acervo.
- Branch de trabalho: feat/library-workspace. Worktree desta sessão: /tmp/acervo-library-workspace.b9wT0z (temporária; pode desaparecer). Se necessário, recuperar a branch remota em uma nova worktree persistente. Não trocar de branch em checkout com alterações locais.
- Começar com git status, git worktree list, git fetch origin e git log; confirmar a branch e evitar modificar a biblioteca real.
- Ler docs/product/library-workspace.md, docs/product/roadmap.md, docs/architecture/overview.md e docs/ux/workspace.md.
- Agrupar PRs por etapa coerente; Conventional Commits pequenos. Não habilitar operações destrutivas sem testes de recuperação e revisão.
- Atualizar este registro com data, mudanças, decisões, testes realmente executados, limitações e próximo passo. Não marcar como concluído o que só foi projetado.

## 2026-11-25 — seleção e planejamento estrutural

- 7a49d55: seleção em lote e escopos; 0cee23a: explorador hierárquico de destinos e prévia; 21a83aa: roadmap detalhado.
- eef653e: prévia Rust de criação/movimentação/renomeação de pastas, inventário de arquivos auxiliares e pastas vazias, bloqueio de colisões, ciclos e links simbólicos. 156 testes Rust e 60 frontend aprovados, build frontend aprovado nessa sessão.
- Incremento atual: prévia de referências integrada, módulo library_reference_impact.rs. Mapeia prefixos simultâneos por componentes (A não afeta AB), separa destinos planejados de arquivos associados, inclui obras ausentes, detecta colisões de índices e ambiguidades novas nas listas. Testes integrados verificam SQLite, JSON, metadados manuais e edições sem alteração dos bytes persistidos.
- Testes desta sessão: cargo test --manifest-path src-tauri/Cargo.toml --lib: 160 aprovados. git diff --check aprovado. Não houve validação visual nesta sessão.
- Decisão: não há executor estrutural nem integração visual nesta etapa. A prévia não é autorização para executar e não garante snapshot consistente durante mudanças concorrentes. Sem escrita nem alteração na biblioteca real.
- Pendências: validar identidade dos arquivos antes da aplicação; mapear fila e referências de aquisição, regras persistidas e cache; estabelecer snapshots exatos e coordenação de mutações; journal durável com recuperação; integrar editor visual. Validar etapa 1 no executável e nos três temas. PR #77 (morph de retorno de capas) estava em draft na sessão anterior; conferir estado remoto antes de agir.

## 2026-11-25 — impacto de aquisição e regras

- Prévia integrada ao estado de downloads: separa destino planejado e destino da ficha; preserva histórico salvo e ignora outra raiz. Bloqueia raiz ambígua quando destino afetado, importação pendente, transferência ativa afetada, ficha inválida ou destino inválido. Não altera fila.
- Regras: armazenamento atual em localStorage por raiz; backend calcula mudanças de destinos de regras de formato e personalizadas, inclusive desativadas, a partir de snapshot opcional. null indica ausência de análise, não ausência de impacto. Não grava configurações. Frontend do editor ainda não consome o contrato.
- Chamada IPC de prévia passou a executar I/O em spawn_blocking.
- Testes: cargo test --manifest-path src-tauri/Cargo.toml --lib: 162 aprovados (inclui fixtures de fila, histórico, outra raiz, importação pendente e regras desativadas). git diff --check aprovado. Nenhuma validação visual ou execução estrutural. Nenhum dado da biblioteca real alterado.
- Limitações: mudanças concorrentes podem invalidar a prévia; colisões entre destinos futuros ainda não analisadas; raiz de downloads avulsos não é persistida. Bloqueios são diagnósticos e não um mecanismo de exclusão mútua. Regras precisam de persistência coordenada para integrar ao journal.

## 2026-11-25 — revisão física do plano

- `library_restructure/snapshot.rs`: mapa ordenado de observações físicas, inclui raiz canônica, operação em ordem, origens, descendentes, ancestrais e destinos ausentes. SHA-256 determinístico de metadados (Unix: dispositivo, inode, ctime, tamanho e timestamps; fallback de outras plataformas mais fraco). Não é hash de conteúdo.
- `preview_library_restructure`: retorna filesystemRevision e aceita expectedFilesystemRevision opcional para rejeitar prévias antigas. Reusa validação de destinos existente; rejeita nomes não UTF-8 no inventário em vez de convertê-los com perda. Observações repetidas divergentes invalidam a prévia.
- Validação: 165 testes Rust aprovados (3 novos: determinismo e ordem, edição/criação/remoção externa, substituição de arquivo de mesmo tamanho e troca de pasta pai). git diff --check aprovado.
- Limitações: continua somente leitura; não protege toda a janela entre validação e futura escrita, nem garante snapshot simultâneo do catálogo, regras e fila. A validação foi feita em Linux, não em Windows/macOS. Sem alteração de arquivos reais ou validação visual.

## 2026-11-25 — revisão lógica e composta do plano

- A prévia retorna `planRevision` e aceita `expectedPlanRevision`, vinculando a revisão física ao estado lógico do catálogo, índices, fila, regras fornecidas e impactos. Divergências exigem nova prévia; não é autorização de execução.
- Catálogo: leitura de fichas JSON compartilhada entre projeção e hash (inclui campos extras das fichas). SQLite: revisão dos campos do modelo atual, ordenada por ID. Índices: revisão do conteúdo tipado de metadados manuais e edições.
- Fila: revisão ordenada dos estados relevantes à raiz e downloads avulsos, excluindo progresso e histórico salvo. Regras completas entram no hash, inclusive critérios; ausência do snapshot continua explícita.
- Referências afetadas a arquivos ausentes ou inválidos e identidade manual divergente geram bloqueios diagnósticos. A verificação de fingerprint de edições permanece pendente.
- Validação Linux: 168 testes Rust aprovados (3 novos: mudança lógica sem mudança física e órfãos, critérios de regras, estado da fila versus progresso/histórico). Teste integrado de catálogo também verifica identidade manual real e alteração externa. git diff --check aprovado.
- Limitações: revisão lógica não inclui campos SQLite fora do modelo, envelope JSON externo às fichas nem bytes exatos; não há snapshot atômico, reserva de mutação, integração visual ou executor. Nenhuma alteração na biblioteca real.

## 2026-11-25 — coordenação de operações

- Novo módulo `library_mutations.rs`: reserva RAII por raiz canônica, rejeita sobreposições ancestral/descendente e libera ao fim. Registro não mantém mutex durante I/O. Aquisição antes de locks de módulos; sem aquisição recursiva.
- Integração nas entradas de criação de pasta, movimento simples/lote, organização JSON e inteligente, edição manual, importação e associação de catálogo, importação/recuperação de downloads e prévia estrutural. Lotes e reversões compartilham a reserva via `move_book_reserved`.
- Importação automática que falhar por concorrência fica pronta para ação manual, com mensagem de suspensão. Helpers internos de persistência continuam com locks existentes.
- Validação: 170 testes Rust aprovados (2 novos: exclusão de raízes sobrepostas e integração com comandos existentes sob reserva em outra thread). git diff --check aprovado.
- Limitações: reserva em memória e no processo; não impede mudanças externas, outros processos, aliases físicos não canônicos ou mudanças na fila fora da importação. Regras em localStorage e cache derivado não são coordenados por essa reserva. Locks globais existentes ainda podem serializar operações independentes. Não houve validação visual, instalação, execução estrutural nem alterações na biblioteca real.

## Próximo incremento recomendado

1. Verificar fingerprints de edições com orçamento de I/O definido; definir verificação de identidades e snapshots exatos necessários para recuperação. Revisar aquisição de reservas para futuros pontos de escrita.
2. Definir coordenação de mutações e reservas por raiz, política de colisões de destinos futuros e snapshots exatos da fila e regras. Considerar persistência nativa de regras com migração testada, preservando dados do usuário.
3. Desenhar journal e executor com recuperação idempotente e testes de falhas; manter aplicação estrutural indisponível até os critérios de segurança.
4. Integrar prévia ao editor visual, com estado explícito de dados não analisados, bloqueios e comparação de estrutura. Validar etapa 1 nativamente e nos três temas.
