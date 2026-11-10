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

**Ainda não implementado:** requisição de rede, agregação de resultados nem downloads.
Um manifesto instalado não faz chamadas externas nesta etapa.

## Próximas interfaces do motor

1. Pesquisa agregada usando as fontes ativas, com filtros por idioma e formato.
2. Resposta limitada em quantidade e tamanho; timeouts; nenhuma execução de JavaScript.
3. Verificação de URLs e IPs de destino, redirecionamentos e tipos de conteúdo.
4. Fila de downloads de arquivos autorizados, com destino planejado e **confirmação
   do usuário antes de salvar na biblioteca**; arquivos parciais fora da raiz,
   prevenção de sobrescrita e vinculação posterior ao catálogo.
5. Extensões complexas, se realmente necessárias, só em sandbox isolada e com permissões explícitas.

Uma fonte pode ser opcionalmente configurada para qualquer catálogo compatível.
O aplicativo não depende de um site fixo e não executa código vindo de extensões.
