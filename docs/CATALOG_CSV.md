# CSV do Catálogo — contrato v1

O CSV é uma alternativa simples ao JSON para listas de obras, planilhas e sugestões de IA.
Ele **não move arquivos** e **não modifica o SQLite legado**.

## Importação

1. Abra **Catálogo → Importar CSV** e selecione um arquivo UTF-8 de até 12 MB.
2. Confira o JSON convertido na tela de prévia (nenhum dado foi escrito ainda).
3. Clique **Importar obras**. A importação usa as mesmas validações e deduplicação do JSON.
4. O resultado é gravado em `99 - Catálogo/Acervo_Listas.json`, com backup da versão anterior.

O arquivo pode usar vírgula ou ponto e vírgula. Aspas duplas, quebras de linha dentro
de uma célula e BOM UTF-8 são suportados. Colunas desconhecidas ou duplicadas
são rejeitadas para evitar perda silenciosa de informação.

## Colunas aceitas

| Coluna | Uso |
| --- | --- |
| `titulo` | Obrigatória. |
| `autor` | Texto opcional. |
| `editoras_indicadas` | Lista JSON de strings ou editoras separadas por `|`. |
| `local_indicado` | Pasta relativa à biblioteca, sem caminho absoluto ou `..`. |
| `prioridade_alta` | `1`, `0`, `true`, `false` ou vazio. |
| `arquivos_encontrados` | Lista JSON de caminhos relativos ou caminhos separados por `|`. |

Exemplo:

```csv
titulo,autor,editoras_indicadas,local_indicado,prioridade_alta,arquivos_encontrados
"Dom Casmurro","Machado de Assis","[""Editora A"",""Editora B""]","Literatura/Brasileira",1,"[]"
```

## Exportação

Em **Catálogo → Exportar CSV**, escolha um novo caminho no diálogo nativo.
O aplicativo não substitui um CSV que já existe. O arquivo é UTF-8 com BOM para
facilitar a abertura em aplicativos de planilha. Conteúdos que parecem fórmulas são
escapados para reduzir riscos ao abrir em Excel/LibreOffice.

A exportação contempla as colunas visíveis no Catálogo (inclusive as fichas lidas
do SQLite). Campos avançados que só existem no JSON original não são exportados
como CSV; para preservá-los, mantenha a cópia JSON original.
