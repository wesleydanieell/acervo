# Histórico de alterações

Segue o formato [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e SemVer.

## [Unreleased]

### Adicionado
- Aba de configurações para controlar aparência e biblioteca.
- Seletor nativo de pastas, com alternativa por caminho manual.
- Dois estilos visuais: Liquid Glass e Material 3.
- Template Noctalia 5.4+ para sincronizar cores e modo claro/escuro com o wallpaper sem reiniciar.
- Breadcrumbs, ordenação por título, autor, formato e tamanho, atalho Ctrl+K.

### Planejado
- Importação/exportação persistente de JSON e CSV, com visualização antes de aplicar.
- Download planner com fila e destino baseado em lista, fontes e direitos de uso.
- Integração configurável do Noctalia e melhor gerenciamento de arquivos.

## [0.1.0-alpha.1] - 2026-11-09

### Adicionado
- Projeto independente Acervo com Tauri, Rust, React e TypeScript.
- Scanner local de EPUB, PDF, MOBI, AZW3 e DJVU.
- Leitura do catálogo SQLite legado e visualização de itens presentes e pendentes.
- Pastas, busca, filtros, visualização em grade/lista e criação de diretórios.
- Arrastar para mover livros com confirmação e atualização transacional das referências SQLite.
- Prompt para gerar listas JSON e validação preliminar de importação.
- Acesso à pesquisa externa e seleção de idioma/formato.
- Temas de cores, com leitura opcional de tokens Noctalia.
