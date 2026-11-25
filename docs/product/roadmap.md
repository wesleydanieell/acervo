# Roadmap

1. Documentar baseline e melhorar workspace sem alterar dados.
2. Separar componentes e medir desempenho; ampliar scanner incremental e modelo de edições com migração testada.
3. Evoluir fontes/OPDS e busca agregada.
4. Downloads persistentes e importação por regras; testes de interrupção, conflito, conteúdo inválido e destino.
5. Coleções inteligentes, metadados e organização com histórico.
6. Leitura e sincronização opcionais após núcleo estável.

Cada entrega coerente, com Conventional Commits pequenos, testes e PR agrupado por etapa. Não prometer paridade sem validar. Critérios: abrir biblioteca não muda arquivos, download inválido não entra no catálogo, mover nunca sobrescreve arquivo, pesquisa e navegação continuam responsivas.

## Evolução contínua de UI e UX

Tratar design e usabilidade como parte da qualidade do produto, sem concentrar toda a reformulação em uma única entrega. Priorizar as áreas que desbloqueiam fluxos reais e validar cada etapa antes de avançar.

- [ ] Fundamentos compartilhados: hierarquia tipográfica, espaçamento, campos, botões, menus, estados de interação e diálogos.
- [ ] Biblioteca e navegação: hierarquia de pastas, busca, filtros, seleção, detalhes e ações contextuais; virtualização e desempenho em acervos grandes.
- [ ] Duplicatas: comparar cópias com clareza, proteger vínculos e metadados, revisar decisões e implementar resolução reversível separadamente.
- [ ] Catálogo e metadados: organização dos campos, edição, associação de obras e descoberta de formatos.
- [ ] Descoberta, extensões e downloads: simplificar aquisição, feedback de progresso, estados vazios e recuperação de falhas.
- [ ] Aparência: consolidar Acervo, Material 3 e Liquid Glass como estilos coerentes, com superfícies, contraste, hierarquia e componentes adequados a cada proposta, em claro e escuro.
- [ ] Movimento: transições de navegação, abertura de painéis, feedback de interação e continuidade visual; respeitar redução de movimento e evitar efeitos caros em listas.
- [ ] Validação de usabilidade: verificar fluxos representativos no desktop, janelas estreitas e navegação por teclado, corrigindo regressões antes de mesclar.

Não tratar alterações cosméticas isoladas como conclusão de uma etapa. Preservar funcionalidades existentes, evitar ações destrutivas implícitas e não trocar correção, desempenho ou manutenibilidade por velocidade de entrega.

### Direções de aparência

- Liquid Glass: inspiração na linguagem visual do macOS/iOS, com navegação e barras flutuantes, reflexos de borda, transparência localizada, superfícies de conteúdo legíveis e movimento suave. Não simular refração cara em cada cartão; manter distinção entre vidro decorativo e conteúdo.
- Material 3: integração visual com Niri + Noctalia (paleta dinâmica, superfícies tonais, indicadores de seleção e contêineres arredondados). Preservar o tema ativo e evitar cores fixas decorativas.
- Sincronização: animar alterações de paleta provenientes do Noctalia entre esquemas do mesmo modo, respeitando redução de movimento; alternar claro/escuro atomicamente para preservar contraste. Validar com mudanças repetidas, diálogos abertos e controles de diferentes telas.

### Sistema de movimento (incremental)

- [ ] Trocas de aparência e paleta: continuidade de geometria, superfícies e seleções; suportar mudanças consecutivas.
- [ ] Navegação: transições direcionadas, destaque móvel e continuidade da seleção, sem bloquear atalhos.
- [ ] Painéis, menus e diálogos: abertura e fechamento, estados de foco corretos e expansão/recolhimento.
- [ ] Biblioteca: transições entre grade/lista, filtros, capa/detalhes e arraste; limitar efeitos aos elementos visíveis.
- [ ] Aquisição: animações de estados de download, progresso, feed, erros, confirmações e recuperação.
- [ ] Interações: microfeedback de botões, switches, rádio, checkboxes, seletores e estados vazios.
- [ ] Ajustes de qualidade: consistência nos três estilos, validação em WebKitGTK, janelas pequenas, mudanças rápidas e preferência de movimento reduzido. Priorizar composição GPU; evitar animações ilimitadas em listas grandes e elementos que não estejam visíveis.

## Workspace de organização da biblioteca (entregas por fluxo)

Especificação detalhada, dependências e critérios de aceitação: [Workspace de biblioteca](library-workspace.md).

Fluxo de contribuição: commits semânticos pequenos dentro de uma branch de trabalho por etapa coerente. PRs agrupam mudanças relacionadas; não abrir um PR por detalhe visual. Mudanças que alteram arquivos ou índices requerem revisão, testes de regressão e validação nativa antes da integração. Não usar a main como área de experimentação de operações de arquivos.

1. Seleção e navegação: seleção persistente entre filtros, intervalos no resultado inteiro, seleção por subárvore, lotes explícitos, busca e expansão de hierarquia, controles compartilhados e movimento de interação. Organizar com IA usando um escopo explícito, somente caminhos relativos e proposta revisada. Lotes atuais limitados a 100 movimentações.
2. Núcleo de operações: contrato tipado de plano imutável com raiz, identidades de arquivos, operações e impactos. Validar limites, permissões, nomes, ancestrais, links, conflitos, referências de catálogo e estados de download. Uma coordenação de mutações por biblioteca; não executar o texto produzido por IA. Criar, mover, renomear e eventualmente consolidar pastas sem perder arquivos não bibliográficos ou subpastas vazias.
3. Remapeamento de caminhos: atualizar vínculos físicos e destinos planejados (inclusive obras ainda ausentes), listas JSON, catálogo SQLite legado, metadados manuais, edições, cache, regras de organização e referências de download conforme sua semântica. Remapear prefixos por componentes, não por substituição textual. Bloquear operações sobre importações ativas ou pendentes de recuperação até existir uma política segura; não reescrever histórico como se fosse destino atual. Atualizar árvore, breadcrumbs e seleção no frontend. Expor impacto por referência na prévia.
4. Recuperação: registrar intenção persistente antes de modificar dados, checkpoints duráveis, revisão de estado após reinício, compensação exata de valores anteriores e conflitos na reversão. Não prometer atomicidade de múltiplos arquivos/SQLite; testar falhas em cada fronteira, mudanças externas e recuperação idempotente. Histórico e desfazer somente quando a identidade dos arquivos e referências ainda corresponde ao registro.
5. Organizador assistido: árvore editável de estrutura proposta (pastas novas, renomeadas e reparentadas), comparação antes/depois, regras reutilizáveis, exceções, aprovação parcial, lotes sequenciais e revisão de referências. IA gera exclusivamente plano declarativo validado pelo núcleo; integração direta opcional mediante autorização explícita do provedor/dados enviados. Não excluir nem sobrescrever automaticamente.
6. Usabilidade: drag-and-drop de pastas com prévia, seletor hierárquico de destino, navegação direta/recursiva, seleção por teclado, barra de ações persistente e feedback de progresso, animações de expansão e rearranjo limitadas aos elementos visíveis, suporte a movimento reduzido.

Limitação atual: mover livros já atualiza catálogo legado, listas, registros de metadados e edições; reversão de lotes tenta mover de volta, mas não é um journal transacional exato de toda a estrutura e ainda não cobre todos os destinos planejados/regras/downloads. Não reutilizar esse mecanismo sem ampliar os contratos para renomear árvores inteiras.
