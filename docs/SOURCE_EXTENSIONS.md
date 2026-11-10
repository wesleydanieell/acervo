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
- Campos `*Field` são seletores de propriedades JSON (ex.: `metadata.title`), não código.
- `downloadUrlField` é opcional: fontes de metadados não precisam fornecer downloads.
- `formats` e `languages` descrevem filtros suportados e podem ser listas vazias.

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

**Ainda não implementado:** gerenciador de downloads, pausa/recomeço e associação automática do download à obra.

## Próximas interfaces do motor

1. Fila de downloads de arquivos autorizados, com destino planejado e **confirmação
   do usuário antes de salvar na biblioteca**; arquivos parciais fora da raiz,
   prevenção de sobrescrita e vinculação posterior ao catálogo.
2. Pesquisa do Catálogo integrada aos resultados das fontes instaladas.
3. Extensões complexas, se realmente necessárias, só em sandbox isolada e com permissões explícitas.

Uma fonte pode ser opcionalmente configurada para qualquer catálogo compatível.
O aplicativo não depende de um site fixo e não executa código vindo de extensões.
