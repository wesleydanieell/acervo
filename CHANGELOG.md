# Histórico de alterações

Segue o formato [Keep a Changelog](https://keepachangelog.com/pt-BR/1.1.0/) e SemVer.

## [Unreleased]

### Alterado
- Adaptador local de pequenos recortes bibliográficos importáveis, sem rede nem downloads.
- Registro permanente de edições adquiridas por biblioteca, detalhes e pesquisa de metadados, sincronização ao mover livros.
- Registro prévio de importação com SHA-256 e recuperação explícita após interrupção, preservando arquivos divergentes.
- Metadados de edição persistidos na fila e histórico, complemento opcional de nome e renomeação de downloads prontos sem nova transferência.
- Agrupamento seguro de obras na descoberta e seleção de formatos/edições, com mapeamento JSON aninhado e metadados opcionais de editora e edição.
- Repositórios HTTPS de extensões, instalação revisada, SHA-256, procedência e atualização manual por versão.
- Adaptador HTML declarativo com seletores CSS, links diretos, paginação na mesma origem e permissões de domínio.
- Adaptador OPDS para feeds de aquisição Atom/XML e JSON, com paginação de mesma origem e permissões explícitas.
- Paginação independente por fonte, IDs remotos opcionais, deduplicação e domínios explícitos de download com compatibilidade legada.
- API v1 de extensões, host comum, adaptador JSON, serviço de rede compartilhado, versionamento compatível e SDK preview.
- Fontes opcionais de teste e clássicos brasileiros do Gutenberg; busca local para catálogos estáticos e correções de cancelamento, retomada e atualização do catálogo.
- Seletor de obras ao baixar, com metadados e destino do catálogo, importação automática opcional e associação a fichas JSON ou SQLite legado.
- Downloads iniciados no catálogo JSON preservam a obra de origem e podem associar a edição após a confirmação de importação.
- Fichas JSON podem vincular edições existentes por seleção e confirmação, sem mover os arquivos ou alterar o SQLite legado.
- Downloads podem criar destinos aninhados personalizados após confirmação, sem sobrescrever arquivos.
- Extensões JSON aceitam JSON Pointer, links MIME aninhados, idiomas em arrays e URLs completas de download.
- Descobrir e Extensões em áreas distintas, com destino do catálogo visível, busca invalidada ao alterar filtros e proteção contra cliques repetidos ao baixar.
- Centro de downloads com filtros de estado, contadores e pesquisa; destinos planejados inexistentes exigem escolha explícita.
- Validação e importação de downloads fora da thread da interface.
- Novo workspace Acervo, inspirado no BookLore, com capas verticais, navegação agrupada e detalhes laterais.
- Indexação em worker Rust e cache de termos de pesquisa para reduzir bloqueios e trabalho repetido.

### Adicionado
- Validação do formato real dos downloads (PDF, EPUB, MOBI, AZW3 e DjVu) antes de liberar ou salvar um arquivo, rejeitando páginas HTML e arquivos incompatíveis com a extensão.
- Seleção de intervalos de livros com Shift+clique e extensão da seleção com Ctrl+Shift+clique, respeitando a ordem e os filtros visíveis.
- Gerador de listas com escolha de destinos por pasta, filtro de diretórios e prévia editável do prompt para IA.
- Seletor hierárquico de pastas para IA, com navegação por níveis, breadcrumbs, seleção recursiva, indicador parcial e opção de incluir toda a biblioteca sem limitar a seleção ao número de itens exibidos.

### Planejado
- Melhor comparação de diferenças e edição das fichas durante a importação CSV/JSON.
- Validação de integridade de downloads e associação de edições ao Catálogo.
- Criação de pastas planejadas e mais operações em lote com recuperação segura.

## [0.1.0-alpha.2] - 2026-11-10

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
- Importação/exportação de listas JSON e CSV (com prévia, tratamento de planilhas e exportação sem sobrescrita), prévia de organização por JSON e central de Catálogo separada dos arquivos físicos.
- Fontes de pesquisa JSON instaláveis, busca agregada e filtros de idioma/formato.
- Gerenciador de downloads em cache, fila persistente, pausa/retomada manual por HTTP Range/ETag forte e confirmação antes de salvar na biblioteca.

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
