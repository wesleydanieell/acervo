# Acervo

Gerenciador local de bibliotecas digitais para Linux, desenvolvido com **Tauri 2, Rust, React e TypeScript**.

O Acervo organiza **arquivos físicos e listas de obras**. Não pretende substituir leitores de EPUB/PDF: a leitura pode continuar nos aplicativos de sua preferência.

## Recursos atuais

- Seleção da raiz da biblioteca pelo explorador nativo, com indexação de PDF, EPUB, MOBI, AZW3 e DJVU.
- **Biblioteca:** árvore de pastas, redimensionamento da lateral, arrastar/soltar arquivos com prévia de conflitos, filtros, sugestões de busca e agrupamentos. Apenas arquivos reais aparecem na raiz.
- Miniaturas de capas EPUB extraídas do arquivo e primeiras páginas de PDF (usa Poppler/`pdftoppm` no Linux), com cache e carregamento gradual.
- **Catálogo:** central separada para obras cadastradas, disponíveis, faltantes e arquivos sem ficha.
- Leitura do banco legado `99 - Catálogo/Catalogo_Conhecimento.db`, mantendo o SQLite original em modo somente leitura para consultas.
- Importação persistente de listas JSON em `99 - Catálogo/Acervo_Listas.json`, com validação, escrita atômica, backup anterior e proteção contra duplicatas exatas. Exportação das listas pelo botão **Copiar JSON**.
- Geração de prompts de IA com a estrutura de pastas existente para produzir listas JSON compatíveis.
- Aparência Liquid Glass e Material 3, paletas manuais e sincronização das cores com o Noctalia (mediante a configuração do template em `integrations/noctalia`).

> **Versão alpha:** a organização de arquivos pode mover documentos e atualizar referências legadas quando você confirmar uma movimentação. Mantenha backup. O gestor de downloads, a pesquisa em múltiplas fontes e a importação CSV ainda estão em desenvolvimento.

## Instalação no CachyOS / Linux

Requisitos: Node.js/npm, Rust/Cargo, dependências do [Tauri 2](https://v2.tauri.app/start/prerequisites/) e Poppler para prévias PDF.

```bash
npm ci
./scripts/install-desktop.sh
```

O instalador compila em modo de produção, instala o executável em `~/.local/opt/acervo` e registra um atalho em `~/.local/share/applications/acervo.desktop`, sem precisar de `sudo`. A instalação anterior fica em `acervo.previous`.

Se você já compilou a versão release, execute `./scripts/install-desktop.sh --skip-build`. O app aparece no menu de aplicativos do Niri/Noctalia como **Acervo**.

## AppImage para Linux

O workflow [Build AppImage](.github/workflows/appimage.yml) compila o Acervo em um ambiente Linux compatível (Ubuntu 22.04). Ele é acionado a cada atualização da `main`, por tags `v*` e manualmente pela aba **Actions** do GitHub.

Depois da execução, abra **Actions → Build AppImage → execução → Artifacts** e baixe `Acervo-Linux-x86_64-AppImage`. Extraia o ZIP do artefato e execute o `.AppImage` após torná-lo executável (`chmod +x Acervo*.AppImage`). O arquivo tem validade de 30 dias nos artefatos da ação.

O ícone original, editável, fica em [`branding/acervo-icon.svg`](branding/acervo-icon.svg); o comando `npm run tauri -- icon branding/acervo-icon.png` reproduz os formatos de empacotamento a partir do PNG exportado do SVG.

## Desenvolvimento e testes

```bash
npm ci
npm run build
cargo test --manifest-path src-tauri/Cargo.toml --lib
npm run tauri dev
```

Os caminhos do acervo são escolhidos pelo usuário; o projeto não copia livros para dentro do repositório. Dados pessoais e arquivos de catálogo não são versionados no Git.

## Contribuições e versionamento

- `main`: versão integrada e verificada por CI.
- `feat/*`, `fix/*`, `chore/*`: entregas isoladas em branches.
- Pull requests revisados e integrados na ordem das dependências, preservando o histórico de commits.
- Pré-lançamentos `0.1.0-alpha.N` não garantem estabilidade completa.

Consulte [VERSIONING.md](VERSIONING.md) e [CHANGELOG.md](CHANGELOG.md).

## Próximos passos

1. Provedores de pesquisa independentes, no estilo Mihon/Komikku, com busca agregada e filtros.
2. Gestor de downloads com fila, verificação de arquivos e destino relativo à biblioteca.
3. Importação/exportação de CSV com prévia de diferenças e edição das fichas do catálogo.
4. Mais operações seguras para arquivos em massa, incluindo desfazer quando possível.
