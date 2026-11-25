# Interface: direção BookLore

Referência visual: https://github.com/booklore-app/booklore. Não copiar assets nem código. Interface local desktop, superfícies sólidas, hierarquia discreta, sidebar por tarefa, capas verticais com títulos legíveis, lista compacta, filtros juntos, detalhes contextuais sem bloquear a biblioteca quando houver espaço. Preservar temas anteriores e sincronização Noctalia.

## Fluxos
Biblioteca -> pasta -> livro -> detalhes/abrir/mover. Catálogo -> obra faltante -> fontes -> download -> validação -> destino -> importação. Operações em massa mantêm seleção Ctrl/Shift e confirmação. Não mostrar botões de recursos não implementados.

## Fluidez
Evitar blur e sombras pesadas por cartão; reduzir animações de layout. Manter cache de capas, limite de concorrência e renderização incremental. Não bloquear UI durante I/O. Medir renderização com biblioteca sintética de 1k/10k arquivos, pesquisa, rolagem e resize; buscar 60 fps em hardware alvo, sem alegar resultado sem medir. Respeitar prefers-reduced-motion. Revisar foco, teclado, contraste e janela pequena.
