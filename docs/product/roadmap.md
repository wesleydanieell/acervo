# Roadmap

1. Documentar baseline e melhorar workspace sem alterar dados.
2. Separar componentes e medir desempenho; ampliar scanner incremental e modelo de edições com migração testada.
3. Evoluir fontes/OPDS e busca agregada.
4. Downloads persistentes e importação por regras; testes de interrupção, conflito, conteúdo inválido e destino.
5. Coleções inteligentes, metadados e organização com histórico.
6. Leitura e sincronização opcionais após núcleo estável.

Cada entrega pequena, com testes, Conventional Commits e PR revisável. Não prometer paridade sem validar. Critérios: abrir biblioteca não muda arquivos, download inválido não entra no catálogo, mover nunca sobrescreve arquivo, pesquisa e navegação continuam responsivas.

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
