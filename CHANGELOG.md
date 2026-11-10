# Histórico de alterações

Segue o formato [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e SemVer.

## [Unreleased]

### Adicionado
- Aba de configurações para controlar aparência e biblioteca.
- Seletor nativo de pastas, com alternativa por caminho manual.
- Dois estilos visuais: Liquid Glass e Material 3.
- Template Noctalia 5.4+ para sincronizar cores e modo claro/escuro com o wallpaper sem reiniciar.
- Breadcrumbs, ordenação por título, autor, formato e tamanho, atalho Ctrl+K.
- Árvore de pastas recolhível, cartões de subpastas e seleção múltipla de arquivos.
- Criação de pastas com escolha do diretório pai e acesso direto à nova pasta.
- Carregamento progressivo de livros (80 cartões por vez) para evitar travar com acervos grandes.
- Prévia de movimentação em lote, detecção de conflitos antes de mover e tentativa de rollback em caso de falha.
- Testes com bibliotecas descartáveis cobrindo movimentação em lote e sincronização SQLite.
- Importação/exportação de listas JSON, prévia de organização por JSON e central de Catálogo separada dos arquivos físicos.
- Fontes de pesquisa JSON instaláveis, busca agregada e filtros de idioma/formato.
- Gerenciador de downloads em cache, fila persistente, pausa/retomada manual por HTTP Range/ETag forte e confirmação antes de salvar na biblioteca.

### Planejado
- Importação/exportação CSV com visualização de diferenças antes de aplicar.
- Validação de integridade de downloads e associação de edições ao Catálogo.
- Criação de pastas planejadas e mais operações em lote com recuperação segura.

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
