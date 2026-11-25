# Fontes extensíveis do Acervo — contrato v1

O Acervo usa **fontes declarativas**. Uma extensão é um arquivo JSON que informa
como buscar um catálogo remoto e interpretar a resposta. Nenhum script de uma
extensão é executado dentro do aplicativo.

## Manifesto mínimo

```json
{
  "schemaVersion": 1,
  "id": "fonte-exemplo",
  "name": "Fonte de exemplo",
  "description": "Apenas um exemplo de formato, não um serviço real",
  "baseUrl": "https://example.org",
  "searchPath": "/api/books?q={query}&lang={language}&format={format}",
  "itemsPath": "results",
  "titleField": "title",
  "authorField": "author",
  "formatField": "format",
  "languageField": "language",
  "downloadUrlField": "download_url",
  "formats": ["epub", "pdf"],
  "languages": ["pt", "en"]
}
```

- `baseUrl` deve ser a origem HTTPS pública. Sem autenticação, IP, porta ou caminho.
- `searchPath` é um caminho começando com `/`, contém `{query}` e pode aceitar
  `{language}` e `{format}`. Nenhuma URL ou rota com `//` no começo.
- `itemsPath` localiza o array de resultados na resposta JSON (ex.: `data.books`).
- Campos `*Field` e `itemsPath` aceitam seletores simples por pontos (ex.: `metadata.title`) ou JSON Pointer (RFC 6901), ex.: `/authors/0/name`, `/formats/application~1epub+zip`. Não são código; os pointers têm limites de tamanho e profundidade.
- Para fonte de formato único com `formatField` vazio, `formats: ["epub"]` e `downloadUrlField` definido, o formato é atribuído somente se houver URL HTTPS válida. A validação do arquivo continua obrigatória.
- Filtros de idioma aceitam campos de texto e arrays de códigos; um idioma presente em qualquer posição é reconhecido.
- URLs de download não são truncadas como metadados; continuam sujeitas aos limites e à política de rede existentes.
- `downloadUrlField` é opcional: fontes de metadados não precisam fornecer downloads.
- `formats` e `languages` descrevem filtros suportados e podem ser listas vazias.

## Navegação

- Descobrir: pesquisa nas fontes ativas e envio de resultados à fila.
- Extensões: instalação, ativação e remoção de manifestos, além da documentação do contrato.
- Quando a pesquisa vem do Catálogo, o destino planejado fica visível; alterar o texto da busca desvincula esse destino.

## Estado atual

Implementado:
- Seletor nativo para instalar manifesto JSON local (até 64 KiB).
- Validação de esquema, identificação, origem HTTPS e seletores.
- Registro de até 40 fontes no diretório de configuração do aplicativo.
- Ativar, desativar e remover fontes; gravação atômica com backup.
- Listagem visual separada da biblioteca e das listas de obras.

Implementado na etapa seguinte:
- Pesquisa agregada nas fontes ativas por título/autor, com filtros de idioma, formato e fonte.
- Respostas JSON limitadas a 2 MiB e 40 resultados por fonte; timeout de 15 segundos.
- Sem proxies automáticos ou redirecionamentos; DNS resolvido e fixado para IPs públicos.
- Fonte **Open Library (metadados)** opcional, instalável na interface, sem downloads diretos.

Implementado nesta etapa:
- Pesquisa da obra faltante no Catálogo encaminhada às fontes instaladas, sem site fixo.
- Fila de até 3 downloads simultâneos a partir de URLs HTTPS informadas por fontes.
- Progresso, cancelamento, tentativa novamente, descarte e seleção de pasta existente.
- Download limitado a 512 MiB e feito em **cache temporário fora da biblioteca**.
- Salvamento na biblioteca apenas com confirmação e sem substituir arquivos existentes.
- O usuário pode escolher uma pasta existente ou informar caminho relativo personalizado. A criação de subpastas inexistentes ocorre somente após confirmação e validação do conteúdo. Caminhos absolutos, segmentos protegidos e links simbólicos são rejeitados; em caso de falha, tenta-se remover apenas as novas pastas vazias.
  Em bibliotecas montadas via FUSE, o arquivo final pode ficar visível durante a
  cópia já autorizada; caso ocorra erro, a cópia incompleta é removida.
- A fila é persistida em `downloads.json` na configuração local do Acervo (modo 0600
  no Linux); os arquivos baixados continuam no cache temporário após reiniciar.
- Jobs interrompidos são marcados como falhos e exigem nova tentativa explícita;
  a inicialização nunca dispara downloads automaticamente.
- Uma tentativa posterior pode retomar um arquivo parcial usando HTTP Range e
  If-Range, desde que a fonte forneça **ETag forte** e responda com bytes e
  Content-Range consistentes. Respostas completas (200), intervalos expirados
  (416) e intervalos inválidos levam a um novo download inteiro; nunca
  misturamos conteúdos de versões diferentes. Sem ETag forte a tentativa
  recomeça do zero.
- Botão Pausar interrompe a conexão, guarda o parcial no cache e libera o slot
  de transferência. Continuar é manual; Cancelar descarta os bytes armazenados.
  Downloads pausados e solicitações de pausa são restaurados sem iniciar rede.
- Registros inválidos não são sobrescritos; o aplicativo informa o erro de leitura.
- O formato declarado é validado após a transferência e antes do salvamento:
  PDF (%PDF), EPUB (ZIP com mimetype e container), MOBI/AZW3 (BOOKMOBI)
  e DjVu (FORM). Respostas HTML disfarçadas ou arquivos incompatíveis
  ficam no cache como falha e não entram automaticamente na biblioteca.

**Ainda pendente:** checagem de hash confiável fornecido pela origem, associação automática à obra do Catálogo
(sem movimentação não autorizada).

## Próximas interfaces do motor

1. Validação mais profunda de conteúdo/tamanho/hash.
2. Integração de edição e obra no Catálogo depois de salvar um download.
3. Extensões complexas, se realmente necessárias, só em sandbox isolada e com permissões explícitas.

Uma fonte pode ser opcionalmente configurada para qualquer catálogo compatível.
O aplicativo não depende de um site fixo e não executa código vindo de extensões.

## Pesquisa de provedores

- [Gutendex](https://github.com/garethbjohnson/gutendex): catálogo JSON com autores aninhados e formatos indexados por MIME. Referência para seletores, não uma extensão incorporada.
- [Política do Project Gutenberg](https://www.gutenberg.org/policy/terms_of_use.html): usar meios autorizados para automação e espelhos para downloads em massa; não integrar downloads automáticos pelo site principal. Verificar permissões e disponibilidade de cada origem antes de publicar um provedor.


## Importação vinculada à ficha

Pesquisas iniciadas em uma ficha JSON guardam uma referência persistente com raiz canônica, título, autor e destino planejado. Na importação, o usuário pode manter ou desmarcar a associação, e precisa selecionar a biblioteca de origem. A referência é resolvida por identidade exata e única, não pelo índice da lista nem por similaridade de título. Se a ficha mudou, foi removida, está duplicada ou a gravação falhou, o livro permanece salvo e a fila apresenta o erro de associação; o vínculo pode ser feito manualmente no catálogo. Registros antigos continuam legíveis. Não há UUID estável, associação ao SQLite legado, nem importação sem confirmação neste incremento. Um desligamento durante a gravação ainda pode exigir reconciliação manual do histórico.


## Escolha da obra antes da transferência

O botão Baixar abre um seletor pesquisável de fichas JSON e SQLite legado. A seleção define título de arquivo, referência à obra e pasta indicada; autor e editoras são mostrados a partir da ficha, sem sobrescrever metadados. O usuário pode escolher baixar sem vínculo ou desmarcar a importação automática. Com a opção automática confirmada, após validar a transferência o worker revalida a obra, cria pastas necessárias, salva sem sobrescrever e atualiza a associação. O catálogo aberto é recarregado por evento. Se a ficha mudou antes da importação, o arquivo continua na fila para revisão. Se o vínculo falhar depois de gravar, o arquivo permanece salvo. O SQLite é atualizado em transação apenas no campo de referências. A fila antiga permanece compatível; downloads restaurados não são automaticamente iniciados.

## Fontes de teste e domínio público

Instale as fontes opcionais pela área Extensões ou importe os manifestos em integrations/test-source e integrations/gutenberg. O campo opcional localSearch aplica filtro por título e autor aos resultados JSON recebidos, útil para catálogos estáticos pequenos. A seleção Gutenberg usa o espelho PGLAF; não inclui o catálogo integral do Project Gutenberg. Testes de regressão são independentes da rede. Recuperação transacional após interrupção abrupta durante importação continua pendente.

## Extension API v1

O manifesto aceita apiVersion, version e adapter. Manifestos antigos recebem os valores 1, 1.0.0 e json; a API 1 e os adaptadores JSON, OPDS e HTML estão disponíveis. Consulte [arquitetura](architecture/extension-api.md) e [SDK preview](../sdk/extensions/README.md). O JSON Schema auxilia a edição; a validação Rust permanece obrigatória.

## Paginação e domínios

O contrato v1 admite remoteIdField, pagination (initialCursor e nextCursorField) e downloadDomains. O cursor é um token ou inteiro não negativo; {cursor} é substituído de forma codificada e o host não navega para uma URL retornada pelo provedor. A UI carrega mais resultados por fonte e elimina duplicatas. A lista de domínios é exata, sem curingas: [] bloqueia downloads, ausência ou null mantém compatibilidade com HTTPS público. O Rust confere a extensão ativa e o domínio antes de enfileirar. Transferências existentes não são revogadas ao desativar fontes. Para detalhes, veja o SDK.


## Adaptador OPDS (preview)

Use adapter: "opds", searchPath com {query}, e downloadDomains explícito. Não use seletores JSON ou pagination customizado. A fonte deve expor um endpoint de busca conhecido com resultados OPDS 1.2 ou 2.0. Links de aquisição diretos seguem a política de rede existente; próximas páginas ficam restritas à mesma origem HTTPS. O preview só lista publicações com aquisição direta elegível; uma lista vazia de domínios não retorna livros. Feeds de navegação, descoberta OpenSearch e fontes privadas/autenticadas ainda não são suportados. Veja sdk/extensions/manifest.opds.example.json e integrations/opds-test.


## Adaptador HTML (preview)

Use adapter: "html", searchPath com {query}, html com itemSelector e titleSelector, e downloadDomains explícito. Os seletores opcionais são authorSelector, languageSelector, idSelector, downloadSelector e nextSelector. Seletores de links apontam para âncoras a[href]; o formato é identificado pela extensão do caminho da URL. O host usa a URL real da página como base, nunca executa JavaScript, não carrega subrecursos nem contorna restrições de acesso. A paginação segue somente links na origem declarada. Consulte sdk/extensions/manifest.html.example.json e integrations/html-test. O preview retorna o primeiro link elegível de cada container; para múltiplas edições, o endpoint deve fornecer containers separados. Seletores CSS são limitados a tags, classes, IDs, atributos simples e combinadores de descendência/filho; pseudoclasses e listas não são aceitas. A integração inicial não inclui navegação em páginas de detalhes nem arquivos com endpoints sem extensão.

## Repositórios remotos (preview)

Em Extensões → Repositórios, adicione a URL de um índice JSON v1, consulte fontes e revise as permissões antes de confirmar a instalação. O SHA-256 confere o manifesto com o índice; não autentica o publicador. Atualizações exigem a mesma URL de repositório e uma versão maior. Veja integrations/extension-repository/README.md para o índice de referência. A remoção do repositório não apaga extensões nem livros. Sem atualizações automáticas ou assinaturas nesta etapa.


## Múltiplas edições JSON

Use editions com itemsField, formatField, downloadUrlField e seletores opcionais de idioma, editora e descrição. O registro pai precisa de remoteIdField e a fonte precisa declarar downloadDomains. Não combine com formatField/downloadUrlField no nível da obra. Consulte sdk/extensions/manifest.editions.example.json. As opções agrupadas aparecem na descoberta e a edição escolhida segue o fluxo de importação existente.


## Dados de edição na fila

O seletor de download exibe a edição e permite um complemento de nome opcional, inclusive uma sugestão derivada de descrição, editora e idioma. O histórico guarda os metadados recebidos da extensão e a referência à ficha selecionada separadamente. Na fila A importar, Alterar nome permite corrigir o complemento, inclusive após conflito de nome, sem baixar novamente. O título da ficha permanece como base do nome e a extensão não é editável. Os metadados remotos não substituem a ficha local.
