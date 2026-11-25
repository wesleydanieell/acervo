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
