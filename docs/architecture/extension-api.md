# Acervo Extension API e Host v1

## Implementação atual

- extension_api.rs: modelos serializáveis, validação comum, erros tipados e contrato BookSource (objeto Send + Sync).
- extension_host.rs: valida e instancia adaptadores, seleciona fontes ativas, agrega resultados e isola falhas; oferece descritores.
- source_search.rs: adaptador JSON, mapeamento de campos, filtros e busca local.
- extension_network.rs: fronteira de rede compartilhada com Downloads; HTTPS público, DNS fixado, sem redirecionamentos nem proxy.
- sources.rs: registro e validação de manifestos com schemaVersion, apiVersion, version e adapter. Campos novos opcionais para manifestos antigos.

A interface Rust é interna, não ABI estável para binários. O frontend ainda usa o comando search_sources com o formato plano compatível. O host disponibiliza list_source_descriptors, incluindo as capacidades calculadas. Nenhuma extensão executa código externo.

## Contratos

BookSource: descriptor() e search(SearchRequest) -> SearchPage ou SourceError. SearchRequest inclui query (2–200 caracteres, sem controles), filtros de idioma/formato (códigos de até 25 bytes) e cursor opaco (opcional, até 2048 bytes). SearchPage devolve SourceBook e next_cursor; erros classificam solicitação inválida, operação não suportada, rede, resposta inválida e erro interno. Os campos de SourceBook preservam o contrato de UI. O adaptador JSON expõe busca e links diretos. Paginação é opcional por manifesto, com cursor opaco de até 2048 bytes e uma origem HTTPS fixa. Detalhes e edições ainda não estão implementados. Links são candidatos, não evidência de arquivo válido. O fluxo de aquisição, cache, validação, destinos, catálogo e persistência permanece exclusivamente no Acervo.

## Versões e SDK

schemaVersion versiona a estrutura do manifesto; apiVersion versiona o contrato do host; version identifica a release da extensão. A versão inicial aceita API 1 e os adaptadores json, opds e html. Manifestos antigos recebem defaults sem migração manual; a leitura de manifestos novos por binários antigos não é garantida. O SDK preview está em sdk/extensions com template e JSON Schema de apoio ao editor; Rust é autoridade para validação de segurança.

## Fronteiras e evolução

1. Paginação por fonte e seletor opcional de ID remoto implementados; IDs estáveis dependem de cada provedor. Múltiplas edições são o próximo contrato.
2. OPDS 1.2/2.0 (feeds de aquisição) integrado ao host. HTML declarativo integrado como preview para páginas estáticas.
3. Allowlist exata opcional de domínios de download, validada na extração e no enfileiramento; o endpoint de busca permanece restrito à origem declarada. Manifestos legados sem allowlist ainda usam HTTPS público. Consentimento granular e revogação de autorizações de transferências já enfileiradas permanecem pendentes.
4. Repositório remoto com índice versionado, hash de conteúdo, confiança e atualização atômica; nenhum formato de pacote executável existe nesta etapa.
5. Avaliar WASM isolado, com limites de recursos e chamadas explícitas ao host; nenhuma biblioteca nativa arbitrária.

Inspirado na separação de interfaces e extensões do Mihon/Komikku, sem copiar sua ABI Android. Inicialmente monorepo; separar SDK e repositórios quando o contrato se estabilizar. Os testes do host usam adaptadores simulados e manifestos reais sem rede.


## Adaptador OPDS

opds_source.rs usa a mesma fronteira HTTPS e limita respostas a 2 MB. opds_parser.rs normaliza Atom/XML e OPDS JSON para SourceBook, sem introduzir dependências. Manifestos OPDS não usam seletores JSON e exigem lista explícita de domínios. O parser limita cada resposta a 40 edições elegíveis; fontes devem oferecer páginas pequenas. Entradas sem aquisição direta elegível são omitidas. A paginação segue links next da mesma origem, com validação também no momento da requisição. Não interpreta árvores de navegação ou descoberta de busca, não fornece empréstimos, compras, autenticação ou DRM. O SDK contém exemplo e feeds equivalentes para testes.


## Adaptador HTML

html_source.rs realiza a requisição pela fronteira HTTPS comum e html_parser.rs converte resultados por seletores CSS declarativos. O parser scraper/html5ever tolera HTML imperfeito sem executar código ou carregar recursos externos. Não há navegador, automação de login, navegação por detalhes ou resolução dinâmica. Links diretos com extensão conhecida são normalizados para SourceBook; o serviço de download permanece responsável por validar os bytes. Exige lista explícita de hosts; paginação é restrita à mesma origem e não depende de seletores JSON. O SDK inclui template e fixture original.

## Extension repository preview

repository_index.rs validates versioned indexes and manifest identity/checksum; extension_repositories.rs manages explicit HTTPS repository URLs, bounded fetches and verified installation. sources.rs stores optional repository provenance and digest, serializes registry writes, rejects ID collisions across origins and downgrades, and preserves enabled state on updates. Manifests are declarative only. The review step shows actual manifest origin and download domains; installation re-fetches against the reviewed digest. No signature infrastructure, package execution, automated update or official update channel is implemented. Removing a repository preserves installed manifests. Local installation remains supported.


## Obras e edições na descoberta

O contrato SourceBook continua plano e compatível com a fila, com publisher e editionLabel opcionais. O adaptador JSON aceita mapeamento de array de edições aninhado por obra com ID remoto obrigatório; cada aquisição preserva formato, idioma, editora, descrição e URL. Na interface discovery-works.ts agrupa somente identidades exatas da mesma fonte, nunca por similaridade. OPDS reaproveita o agrupamento de seus links; HTML continua com o primeiro link por container. A API não oferece ainda get_details/get_editions separados nem identidade bibliográfica global. As informações da edição são mostradas antes de associar ao catálogo, mas não sobrescrevem seus metadados.


## Metadados de aquisição e nomes

A fila persiste um snapshot dos metadados descritivos da edição (título original, autor, idioma, editora, descrição, ID remoto e ID da fonte conferido pelo host), separado do título e destino da ficha escolhida. Não são metadados bibliográficos verificados. O histórico os mantém até sua remoção; o catálogo não recebe esses campos automaticamente. O usuário escolhe um complemento opcional antes de baixar, com sugestão baseada nos metadados. O Rust sanitiza nomes e limita os bytes UTF-8 sem cortar caracteres; a extensão vem do formato permitido. Entradas antigas mantêm o algoritmo original, identificado pela ausência de filenameSuffix. Downloads prontos podem ter o complemento alterado sem retransferência; a mudança é persistida antes de confirmar, com rollback em caso de falha. Arquivos físicos existentes nunca são substituídos. Não há regras globais de nomenclatura nem recuperação transacional de importações nesta etapa.


## Recuperação de importações (preview)

Antes de qualquer gravação na biblioteca, downloads.rs persiste um plano de importação com raiz canônica, pasta, escolha de associação e impressão SHA-256/tamanho do arquivo temporário validado. O nome permanece no registro do download. Se a aplicação for interrompida, o plano é restaurado em estado recovery, sem gravar na biblioteca automaticamente. Na biblioteca original, o usuário confirma a recuperação. Uma cópia de destino com conteúdo idêntico permite concluir a associação de forma idempotente; uma cópia de cache íntegra permite retornar a ready. Uma cópia divergente é preservada; sem cópia íntegra, a transferência pode ser refeita. A finalização do histórico usa escrita serializada com rollback em caso de falha. O hash é comparado ao próprio download, não comprova autenticidade da fonte.

Limites: não é uma transação distribuída entre cache, catálogo e filesystem nem garantia contra perda de energia; associação após recuperação depende da identidade da ficha ainda válida. Arquivos temporários órfãos na biblioteca podem restar após interrupção durante a cópia e não são apagados automaticamente; cada tentativa usa um nome temporário distinto. A biblioteca original deve estar acessível para recuperar. Não protege contra modificações concorrentes maliciosas do filesystem. Testes usam bibliotecas descartáveis e estados simulados, não interrupção real do processo nativo.


## Registro persistente de edições (preview)

A biblioteca possui um índice separado, 99 - Catálogo/Acervo_Edicoes.json, versionado e escrito atomicamente. Cada registro vincula um caminho relativo a metadados declarados pela fonte e ao tamanho/SHA-256 do arquivo adquirido. O índice é independente do histórico de downloads e das fichas JSON/SQLite; não altera o esquema legado nem substitui título/autor cadastrado. Somente novas importações com metadados de edição são registradas nesta fase. Não há migração retroativa do histórico.

A finalização da importação mantém o plano de recuperação se a persistência de edição falhar. Uma nova recuperação verifica os bytes e repete a gravação idempotente; um registro conflitante não é sobrescrito. O arquivo já importado é preservado. Movimentos realizados pelo Acervo atualizam o caminho após verificar os bytes no destino; falhas provocam tentativa de reversão do arquivo e das referências do catálogo. Não se trata de uma transação atômica entre os três registros.

A leitura do índice não cria arquivos e a varredura usa caminho e tamanho para anexar os metadados, evitando recalcular hashes de todos os livros. Mudanças externas de mesmo tamanho podem deixar metadados desatualizados; a associação por conteúdo é conferida na gravação e movimentação. Movimentações externas não são reconciliadas automaticamente. Registros ausentes ou divergentes não são apagados automaticamente. Edição manual, reparo de registros conflitantes, migração de histórico e reconciliação por hash permanecem pendentes.
