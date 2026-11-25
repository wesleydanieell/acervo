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

## Próximo incremento recomendado

1. Definir snapshot de identidade de origem e inventário determinístico, revalidável antes da execução. Validar paths com política consistente entre módulos e testar alterações externas.
2. Definir coordenação de mutações e reservas por raiz, política de colisões de destinos futuros e snapshots exatos da fila e regras. Considerar persistência nativa de regras com migração testada, preservando dados do usuário.
3. Desenhar journal e executor com recuperação idempotente e testes de falhas; manter aplicação estrutural indisponível até os critérios de segurança.
4. Integrar prévia ao editor visual, com estado explícito de dados não analisados, bloqueios e comparação de estrutura. Validar etapa 1 nativamente e nos três temas.
