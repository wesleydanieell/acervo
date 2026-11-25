# Anna's Archive — avaliação de integração

## Estado

Não há uma extensão funcional validada. Em 24/11/2026, a consulta HTTPS à política de acesso do domínio encontrado expirou no ambiente de desenvolvimento. A documentação indexada descreve datasets de metadados e uma API de download para membros, mas não uma API pública estável de pesquisa.

Referências: FAQ e documentação de datasets do projeto. O acesso direto e as permissões de automação não foram confirmados nesta avaliação.

## Compatibilidade com o Acervo

- HTML declarativo pode extrair metadados de uma página estática autorizada, mas precisa de seletores validados contra a resposta real.
- JSON pode consumir um serviço autorizado de metadados com downloadDomains vazio e sem seletores de aquisição. Não exige mudanças no host.
- A API atual não oferece credenciais por extensão nem resolução de páginas intermediárias. Não armazenar tokens em manifestos, índices públicos, logs ou URLs de histórico.
- Uma licença da obra não necessariamente cobre uma tradução ou edição específica. Manter aquisição limitada a arquivos cuja distribuição seja autorizada.

## Critérios de aceitação

Validar uma origem autorizada e alcançável, testar pesquisa e normalização de metadados com fixtures próprias e declarar capacidades reais. Para aquisição, validar licença da edição e usar um endpoint autorizado. Não contornar controles de acesso, automatizar desafios ou substituir a política HTTPS. Não publicar manifestos fictícios como fontes funcionais.

Por enquanto, manter esta avaliação fora do índice instalável; os adaptadores e as fontes de domínio público existentes continuam disponíveis.
