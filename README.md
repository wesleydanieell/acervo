# Acervo

Gerenciador local de bibliotecas digitais para Linux, desenvolvido com **Tauri 2, Rust, React e TypeScript**.

O Acervo substitui a parte de **organização de arquivos e catálogo** do antigo Minha Biblioteca. Ele não pretende ser outro leitor de EPUB/PDF: o usuário pode continuar lendo no aplicativo que preferir.

## O que esta versão faz

- Abre uma pasta existente e indexa arquivos PDF, EPUB, MOBI, AZW3 e DJVU sem renomeá-los.
- Lê o banco legado `99 - Catálogo/Catalogo_Conhecimento.db` em modo somente leitura.
- Exibe arquivos, pastas e obras pendentes, com pesquisa e filtros.
- Cria pastas, move livros por arrastar/soltar mediante confirmação e sincroniza referências no catálogo SQLite.
- Mostra o destino planejado para obras ainda não encontradas.
- Gera um prompt para solicitar a uma IA listas em JSON; valida preliminarmente o texto importado.
- Abre uma busca externa para encontrar uma edição, com filtros de formato e idioma.
- Oferece paletas de cores e lê um arquivo JSON de cores do Noctalia, caso configurado.

> **Versão experimental:** antes de reorganizar um acervo real, mantenha backup do banco de dados e dos livros. A importação JSON ainda não é persistida; o gerenciador de downloads e a sincronização automática com Noctalia são entregas futuras.

## Desenvolvimento

Requisitos: Node.js, npm, Rust/Cargo e dependências Linux do [Tauri 2](https://v2.tauri.app/start/prerequisites/).

```sh
npm ci
npm run build
cd src-tauri && cargo test && cd ..
npm run tauri dev
```

Os caminhos de acervo são informados pelo usuário; o projeto não copia automaticamente os livros da pasta antiga. Os dados pessoais e arquivos de catálogo não são versionados no Git.

## Repositório e contribuições

- `main`: versões integradas e testadas.
- `feat/*`, `fix/*`, `chore/*`: alterações isoladas por objetivo.
- PR antes de integrar na `main`, preferencialmente squash merge.
- Pre-releases `0.1.0-alpha.N` não prometem compatibilidade de formato ou funcionamento completo.

Consulte [VERSIONING.md](VERSIONING.md) e [CHANGELOG.md](CHANGELOG.md).

## Próximas prioridades

1. Importação e exportação reais de JSON/CSV com validação, preview e backup.
2. Gerenciamento de arquivos com operações seguras (prévia, desfazer e conflitos).
3. Integração nativa com Noctalia e acessibilidade/UX refinadas.
4. Fila de downloads com escolha de origem legal e pasta de destino planejada.
