# Organização da biblioteca por JSON (v1)

O Acervo permite exportar um inventário **somente com metadados e caminhos relativos** para pedir a uma IA propostas de organização. Nenhum arquivo ou livro é enviado automaticamente. O usuário cola o plano na tela **Minha biblioteca → Organizar por JSON**.

## Formato do plano

```json
{
  "versao": 1,
  "movimentacoes": [
    {
      "arquivo": "00 - Entrada/O Banquete.epub",
      "destino": "Literatura/Filosofia",
      "tamanho_bytes": 123456
    }
  ]
}
```

- `arquivo`: caminho *atual* do livro, relativo à raiz escolhida.
- `destino`: pasta de destino *existente*, também relativa à raiz. `""` indica a raiz.
- `tamanho_bytes`: opcional no parser, mas **recomendado**; compara o tamanho atual com o inventário para impedir operações com dados desatualizados.
- O nome do arquivo **não é alterado**, só sua pasta.
- Até **100 movimentações** por operação; para grandes acervos, dividir em lotes.

## Segurança e revisão

1. Cole o JSON produzido pela IA.
2. Clique em **Conferir movimentações**. Essa etapa não escreve nada no disco.
3. Revise a lista completa **origem → destino**, o tamanho total e possíveis conflitos.
4. Marque **Conferi os destinos** e então **Aplicar**.
5. Antes da execução, o backend valida o plano novamente. Em erro durante a movimentação, tenta reverter os passos anteriores e relata claramente falhas de reversão.

Restrições: caminhos devem ficar dentro da biblioteca; arquivos precisam existir e ter extensão de livro suportada; não são permitidos destinos repetidos, substituição de arquivos existentes, links simbólicos de arquivo ou movimentação de arquivos do catálogo. As referências dos bancos SQLite e JSON são sincronizadas ao mover.

## Próximas versões

- `v2`: criar novas pastas declaradas no plano com prévia, limites e reversão.
- `v2`: classificação assistida por IA das fichas de obras não catalogadas, sem mover automaticamente.
- `v3`: histórico auditável e recuperação por lote; comparação por hash e prévia de renomeação.
- Extensões de busca/download e origem dos arquivos são **um módulo separado**; não podem executar scripts da IA nem alterar o acervo diretamente.
