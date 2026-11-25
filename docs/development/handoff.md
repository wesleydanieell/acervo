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

## Próximo incremento recomendado

1. Identificar persistência de destinos/regras e semântica da raiz em cada download. Não tratar saved_path histórico como destino atual.
2. Definir política de bloqueio para importações em andamento ou recuperação pendente e prévia de referências de aquisição.
3. Adicionar fixtures de downloads de outra raiz, obras futuras, múltiplas edições, estados pendentes e histórico.
4. Só depois desenhar o journal e o executor com revalidação de identidades e snapshots, sem liberar a mutação estrutural prematuramente.
