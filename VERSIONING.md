# Versionamento e publicação

## Política

Usamos [SemVer](https://semver.org/lang/pt-BR/) e [Conventional Commits](https://www.conventionalcommits.org/).

| Marcador | Uso |
| --- | --- |
| `0.1.0-alpha.N` | Experimentos funcionais, ainda sujeitos a alteração |
| `0.1.0-beta.N` | Fluxos completos, foco em corrigir regressões |
| `0.1.0` | Primeira versão utilizável, com testes de integridade e backup |
| `0.2.0` | Novas funções após estabilizar o acervo |
| `1.0.0` | API e formatos de importação estabilizados |

O número fica sincronizado em `package.json`, `src-tauri/Cargo.toml` e `src-tauri/tauri.conf.json`. `Cargo.lock` e `package-lock.json` devem acompanhar mudanças de dependências/versões.

## Branches e commits

- `main` recebe apenas trabalho revisado e que passa em build e testes.
- `feat/nome` para uma funcionalidade; `fix/nome` para bugs; `chore/nome` para manutenção.
- Commit semântico: `feat(catalog): ...`, `fix(files): ...`, `test(catalog): ...`, `docs: ...`.
- PR pequeno e com descrição de impacto, evidências de teste, riscos e eventual migração de dados.
- Preferir squash merge para preservar um histórico legível; não usar force-push em `main`.
- Tags anotadas `v0.1.0-alpha.1`, `v0.1.0` somente quando o commit estiver integrado em `main`; não mover tags publicadas.

## Checklist de merge

- [ ] Build web (`npm run build`) e testes Rust (`cargo test --manifest-path src-tauri/Cargo.toml`) concluídos.
- [ ] Teste funcional em uma biblioteca descartável; não alterar o acervo do usuário em testes.
- [ ] Revisão dos caminhos de origem/destino, conflitos e rollback de operações sobre arquivos.
- [ ] Não versionar livros, banco real, arquivos de configuração pessoal nem credenciais.
- [ ] Atualizar documentação e changelog quando o comportamento mudar.
- [ ] Para alterações em formato de dados: backup, estratégia de migração e reversão documentados.

## Entregas planejadas

- **alpha.1 (bootstrap):** leitura do acervo legado, visualização, navegação, filtro, criação de pasta e movimentação básica.
- **alpha.2 (catálogo):** importação/exportação persistente JSON/CSV, validação e detecção de duplicatas.
- **alpha.3 (workflow):** download planner/fila controlada, integração Noctalia, operações seguras de arquivos.
- **beta.1 (polimento):** testes E2E Linux, navegação por teclado, acessibilidade e refinamento visual.
- **0.1.0:** instalador e documentação de uso/recuperação com testes de acervo.

Não automatizar merge antes da aprovação dos testes; merges devem ser conscientes. Pull requests de funcionalidades ainda incompletas podem ser mantidos como draft.
