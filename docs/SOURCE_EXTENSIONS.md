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

Implementado nesta etapa:
- Pesquisa da obra faltante no Catálogo encaminhada às fontes instaladas, sem site fixo.
- Fila de até 3 downloads simultâneos a partir de URLs HTTPS informadas por fontes.
- Progresso, cancelamento, tentativa novamente, descarte e seleção de pasta existente.
- Download limitado a 512 MiB e feito em **cache temporário fora da biblioteca**.
- Salvamento na biblioteca apenas com confirmação e sem substituir arquivos existentes.
  Em bibliotecas montadas via FUSE, o arquivo final pode ficar visível durante a
  cópia já autorizada; caso ocorra erro, a cópia incompleta é removida.
- A fila é mantida durante a sessão; salve os downloads concluídos antes de fechar o app.

**Ainda pendente:** pausa com retomada por HTTP Range, fila persistente entre sessões,
validação aprofundada de conteúdo, criação automática de pastas planejadas e associação
automática à obra do Catálogo (sem movimentação não autorizada).

## Próximas interfaces do motor

1. Persistência e retomada segura da fila, com verificação de tamanho/hash e retry por parte.
2. Integração de edição e obra no Catálogo depois de salvar um download.
3. Extensões complexas, se realmente necessárias, só em sandbox isolada e com permissões explícitas.

Uma fonte pode ser opcionalmente configurada para qualquer catálogo compatível.
O aplicativo não depende de um site fixo e não executa código vindo de extensões.
