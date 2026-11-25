# Fonte de teste Acervo

Arquivos sintéticos próprios, sem conteúdo comercial. O endpoint é estático: qualquer termo retorna os dois registros; os filtros de idioma e formato continuam funcionando. As URLs referenciam commits fixos do repositório público.

## Teste manual em uma biblioteca descartável

1. Crie uma pasta vazia fora da biblioteca real e selecione-a nas configurações.
2. No Catálogo, importe o conteúdo de `test-wishlist.json`.
3. Em Extensões, use Instalar JSON e selecione `manifest.json`.
4. Em Descobrir, escolha Acervo · Fonte de teste, pesquise Acervo e baixe Livro de teste.
5. Selecione a ficha correspondente e mantenha a importação automática marcada.
6. Confira Downloads e a presença da ficha no catálogo; o EPUB deve estar em Testes/Aquisição.
7. Baixe Arquivo inválido e selecione a segunda ficha. A validação deve rejeitar a resposta HTML; a ficha continua faltante e não deve existir um EPUB em Testes/Rejeitados.
8. Repita o download válido: o arquivo existente não deve ser sobrescrito. Verifique a fila e escolha um destino diferente se necessário.

Não é uma fonte de livros real nem é instalada automaticamente. Os testes automatizados utilizam os mesmos arquivos locais e não dependem de internet.
