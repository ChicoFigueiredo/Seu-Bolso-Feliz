# Prompt inicial de kickoff e refino — Projeto Seu Bolso Feliz

Você é uma equipe sênior de produto, arquitetura, engenharia de software, modelagem de dados, UX e finanças pessoais.

Sua missão é conduzir uma **reunião de refino técnico-funcional** para iniciar o projeto **Seu Bolso Feliz**, um sistema web/mobile de organização financeira pessoal com foco inicial em:

- controle patrimonial e financeiro pessoal;
- despesas recorrentes;
- cartões e faturas;
- múltiplos bancos e múltiplos produtos por banco;
- empréstimos e financiamento habitacional;
- histórico financeiro;
- documentos e anexos;
- categorização e etiquetagem flexível de despesas;
- priorização de pagamentos;
- futura integração com IA;
- futura integração com MCP/agentes.

A prioridade agora **não é construir tudo**, mas **refinar corretamente o domínio, a arquitetura e o MVP**, de forma que o sistema seja sólido, extensível e útil desde a primeira versão.

---

## 1. Contexto do problema

Hoje o controle financeiro é feito em planilhas, mas isso está ineficiente, cansativo e sujeito a erros.

O objetivo do projeto é substituir a planilha por um sistema próprio que permita:

- controlar contas em múltiplos bancos;
- organizar múltiplos produtos por instituição financeira;
- controlar despesas recorrentes;
- controlar cartões de crédito, incluindo cartões adicionais;
- controlar faturas;
- controlar empréstimos e financiamento habitacional;
- separar corretamente **despesas reais** de **movimentações internas**;
- manter histórico financeiro detalhado de pelo menos 12 meses;
- permitir importar dados antigos de planilhas;
- anexar prints, notas, comprovantes e PDFs;
- permitir classificação por categorias e múltiplas tags;
- permitir priorização de despesas e contas a pagar;
- futuramente usar IA para interpretar documentos, classificar lançamentos, resumir períodos e responder perguntas em linguagem natural;
- futuramente expor ferramentas via MCP para integração com agentes.

O objetivo prático do usuário é organizar a vida financeira e criar condições para **zerar dívidas até o fim do ano**, com visão clara de fluxo, passivos, juros e amortização.

---

## 2. Diretrizes de arquitetura

A arquitetura desejada é **serverless-first**, evitando VPS no início.

### 2.1. Stack-base desejado

A equipe deve assumir como stack-base preferencial:

#### Web

- **React**
- **Next.js**
- **Tailwind CSS**

#### Mobile

- stack também baseada em **React**, priorizando:
  - **React Native**
  - e, se fizer sentido, **Expo**

#### Backend / dados / autenticação / storage

- **Supabase** como backend principal
- Postgres
- Auth
- RLS
- Storage
- Edge Functions
- rotinas agendadas server-side

### 2.2. Diretriz de coerência entre web e mobile

A equipe deve priorizar uma arquitetura que maximize:

- reaproveitamento conceitual entre web e mobile;
- compartilhamento de contratos de dados;
- compartilhamento de tipos;
- compartilhamento de regras de validação;
- compartilhamento de tokens visuais;
- consistência visual e funcional entre plataformas.

Não é obrigatório compartilhar tudo, mas é desejável que a equipe proponha uma estratégia de reutilização inteligente.

### 2.3. Design system

A equipe deve propor um **design system de mercado** compatível com:

- React
- Next.js
- Tailwind
- possível evolução para mobile em React Native/Expo

A escolha deve ser justificada.

A equipe pode usar como referência sistemas maduros do mercado, como os inspirados por:

- Google
- Meta/Facebook
- ou outros padrões sólidos e amplamente adotados

Mas deve selecionar a alternativa **mais adequada ao stack escolhido e à natureza do produto**.

A proposta deve considerar:

- consistência entre telas;
- velocidade de desenvolvimento;
- acessibilidade;
- maturidade do ecossistema;
- adequação a produto financeiro;
- facilidade de evolução;
- aderência ao stack React + Next.js + Tailwind + Supabase.

### 2.4. Restrições arquiteturais

A equipe deve assumir como desejável:

- baixo custo operacional;
- simplicidade de manutenção;
- boa segurança;
- extensibilidade;
- separação clara entre núcleo financeiro determinístico e camada de IA interpretativa.

---

## 3. Princípios obrigatórios de modelagem do domínio

A equipe deve tratar como regra absoluta:

### 3.1. Não misturar despesa com transferência interna

Exemplo:

- pagar a fatura do Nubank com dinheiro da Caixa **não é nova despesa**;
- isso é quitação de obrigação + transferência/movimentação entre contas/produtos.

### 3.2. Não misturar parcela de dívida com gasto comum

Em empréstimos e financiamentos, cada parcela pode conter:

- amortização;
- juros;
- seguros;
- taxas;
- encargos adicionais.

O sistema deve permitir separar isso de forma estruturada.

### 3.3. Distinguir claramente os tipos de evento

O domínio precisa distinguir pelo menos:

- receita;
- despesa;
- transferência interna;
- passivo/dívida;
- juros/encargos;
- amortização;
- estorno;
- ajuste;
- pagamento de fatura;
- lançamento previsto;
- lançamento realizado.

### 3.4. Pensar em agrupamento por instituição e produto

Exemplo:

- instituição: Caixa, Nubank, C6;
- produto: conta corrente, poupança, cartão, cheque especial, empréstimo, financiamento, investimento;
- subconta ou contrato: cartão final 1234, empréstimo X, financiamento Y.

### 3.5. Reduzir atrito operacional

O sistema deve reduzir a preguiça operacional do usuário.
Ele não pode depender de disciplina perfeita.

### 3.6. O sistema deve suportar ciclos financeiros personalizados

O sistema **não pode assumir que o período financeiro do usuário acompanha o mês civil**.

Há usuários cujo ciclo financeiro:

- começa em um dia específico do mês;
- termina em outro dia do mês seguinte;
- e serve como referência para orçamento, fluxo de caixa, relatórios e controle de sobrevivência até o próximo pagamento.

Exemplo real a suportar:

- ciclo financeiro começa em **20/03** e termina em **19/04**;
- o dinheiro precisa durar esse período;
- o “mês financeiro” do usuário não é o mesmo que março civil.

Essa regra deve impactar:

- dashboards;
- orçamento;
- fluxo de caixa;
- previsão de vencimentos;
- análise de consumo;
- relatórios;
- metas;
- alerta de estouro;
- agrupamento temporal dos lançamentos.

### 3.7. O sistema deve suportar categorias e múltiplas tags

O sistema deve diferenciar:

- **categoria principal** da despesa/receita/evento;
- **tags múltiplas** para classificação complementar.

Uma mesma despesa pode receber mais de uma tag.

Exemplos:

- `trabalho_externo`
- `diversao`
- `pesquisa_desenvolvimento`
- `ensino`
- `essencial`
- `pessoal_fisica`
- `casa`
- `cartao_prioritario`

Exemplo concreto:

- uma assinatura do ChatGPT pode estar na categoria “software/serviços digitais” e ter simultaneamente as tags:
  - `trabalho_externo`
  - `pesquisa_desenvolvimento`
  - `ensino`

A equipe deve propor uma modelagem que suporte:

- múltiplas tags por lançamento;
- tags por recorrência/template;
- tags por dívida ou obrigação, quando fizer sentido;
- filtros e relatórios por tag;
- análise cruzada entre categoria e tags.

### 3.8. O sistema deve suportar prioridade de pagamento

Algumas despesas, obrigações e contas a pagar têm precedência sobre outras.

O sistema deve permitir classificar itens com prioridade de pagamento, por exemplo:

- essencial;
- alta;
- média;
- baixa;
- opcional/postergável.

Essa prioridade pode ser derivada de:

- regra manual;
- tag;
- tipo de obrigação;
- recorrência;
- combinação desses fatores.

Exemplos de alta prioridade:

- moradia;
- condomínio;
- diarista;
- pagamentos de pessoas físicas;
- certas faturas;
- despesas essenciais à operação do usuário.

Essa dimensão deve afetar:

- ordenação da primeira tela;
- alertas;
- painéis;
- simulação de sobrevivência até o próximo recebimento;
- visualização do que pode ou não ser postergado.

---

## 4. Cenários que o sistema precisa suportar

A equipe deve considerar no refinamento os seguintes cenários reais:

1. Um banco pode ter várias contas e produtos.
2. Um banco pode ter vários cartões.
3. Um cartão pode ter adicionais.
4. Uma fatura de cartão é recorrente e tem ciclo próprio.
5. Uma despesa pode ser parcelada.
6. Um empréstimo precisa exibir composição da parcela.
7. Um financiamento habitacional precisa ser modelado como passivo complexo.
8. O sistema precisa aceitar histórico importado de planilhas.
9. O sistema precisa guardar anexos e documentos.
10. O sistema precisa reconhecer que certos lançamentos são transferências internas.
11. O sistema deve permitir templates recorrentes, como:

- Fatura Nubank
- Internet
- Academia
- Parcela do financiamento

12. Alguns documentos chegam por e-mail e podem estar protegidos por senha.
13. O sistema poderá no futuro ler anexos de e-mail, abrir PDFs protegidos e extrair dados.
14. Senhas de PDF e outros segredos **não devem** ficar em tabela comum de negócio.
15. O sistema deve ter estratégia de backup local além do ambiente na nuvem.
16. O sistema deve permitir configurar **ciclos financeiros personalizados por usuário**.
17. O sistema deve permitir mapear cada transação para:

- data do evento;
- data de competência;
- período financeiro do usuário;
- ciclo de fatura, quando aplicável.

18. O sistema deve permitir que relatórios e alertas usem:

- mês civil;
- período financeiro personalizado;
- intervalo livre.

19. O sistema deve permitir múltiplas tags por despesa.
20. O sistema deve permitir usar tags como base para filtros, relatórios e regras operacionais.
21. O sistema deve permitir marcar ou derivar prioridade de pagamento para despesas e obrigações.
22. O sistema deve permitir que certas tags influenciem prioridade, por exemplo:

- `essencial`
- `moradia`
- `pessoa_fisica`
- `trabalho`

23. A primeira tela do sistema deve ajudar o usuário a decidir o que pagar primeiro, o que vence logo, o que é essencial e o que pode esperar.

---

## 5. O que a equipe deve propor no refino

A equipe deve retornar com uma proposta estruturada contendo, no mínimo:

### A. Visão de produto

- qual é o escopo real do sistema;
- quais dores ele resolve no MVP;
- quais funcionalidades ficam para depois;
- qual é a proposta de valor inicial sem depender de IA.

### B. Mapa do domínio

- entidades principais;
- relacionamentos;
- hierarquia instituição → produto → conta/cartão/contrato → evento financeiro;
- separação entre ativos, passivos, despesas e transferências;
- separação entre tempo civil, competência financeira e ciclo financeiro personalizado;
- separação entre categoria principal, tags complementares e prioridade operacional.

### C. Modelo de dados inicial

Propor tabelas principais para o MVP, por exemplo:

- institutions
- financial_products
- accounts
- cards
- card_holders ou adicionais
- liabilities / loans / mortgages
- transactions
- transfers
- recurring_templates
- recurring_instances
- statement_cycles
- statement_items
- documents
- attachments_metadata
- categories
- tags
- transaction_tags
- recurring_template_tags
- liability_tags
- budgets
- payment_priorities
- import_jobs
- audit_logs
- financial_cycles
- financial_periods
- user_financial_preferences

A equipe pode renomear, fundir ou separar melhor, desde que justifique.

### D. Modelagem específica de recorrência

A equipe deve propor como modelar:

- template recorrente;
- instância recorrente;
- vencimento;
- competência;
- vínculo com conta/cartão/produto;
- status esperado / recebido / processado / pago;
- metadados variáveis;
- tags herdadas do template;
- prioridade padrão herdada do template, quando fizer sentido.

### E. Modelagem específica de dívidas

A equipe deve propor como modelar:

- empréstimos;
- cheque especial;
- parcelamentos;
- financiamento habitacional;
- composição da parcela;
- saldo devedor;
- taxa de juros;
- amortização;
- cronograma;
- pagamento antecipado;
- cenários de quitação;
- tags e prioridade, quando aplicável.

### F. Modelagem temporal / dimensão do tempo

A equipe deve tratar a dimensão temporal como parte central do domínio.

Precisa propor como o sistema representará:

#### F.1. Tempo civil

- data do lançamento;
- mês civil;
- ano civil;
- filtros tradicionais por calendário.

#### F.2. Tempo financeiro personalizado

- ciclo financeiro do usuário;
- data de início do ciclo;
- data de fim do ciclo;
- regra de fechamento;
- regra de virada de período;
- vínculo do lançamento a um período financeiro.

#### F.3. Competência

- competência financeira do lançamento;
- diferença entre data real do evento e período ao qual ele pertence;
- casos em que a despesa ocorre em um dia mas impacta o próximo ciclo.

#### F.4. Relação com outros ciclos

- ciclo da fatura do cartão;
- vencimento de dívida;
- vencimento de recorrência;
- janela de salário/recebimento;
- alertas de sobrevivência até o próximo pagamento.

A equipe deve propor se isso será modelado com:

- tabelas explícitas de períodos;
- função de derivação;
- ou combinação de ambas.

A recomendação é que exista uma entidade ou estrutura clara para representar **períodos financeiros do usuário**, e não apenas cálculos soltos.

### G. Categorias, tags e priorização

A equipe deve propor explicitamente:

#### G.1. Categoria principal

- como será definida a categoria principal de um lançamento;
- se será obrigatória ou opcional;
- como será usada em relatórios.

#### G.2. Tags múltiplas

- como modelar múltiplas tags por entidade;
- como aplicar tags em:
  - lançamentos,
  - recorrências,
  - dívidas,
  - documentos, se fizer sentido;
- como permitir filtros compostos por tags;
- como diferenciar tags de domínio, tags operacionais e tags livres do usuário, se necessário.

#### G.3. Prioridade de pagamento

- como modelar prioridade:
  - campo direto;
  - tabela de referência;
  - regra derivada;
  - combinação de fatores.
- como a prioridade interage com:
  - tags;
  - recorrências;
  - vencimentos;
  - dívidas;
  - faturas;
  - alertas.

#### G.4. Regras sugeridas

A equipe deve avaliar se a prioridade pode ser:

- manual;
- herdada de template;
- sugerida automaticamente por tipo;
- reforçada por tags como `essencial`, `casa`, `pessoa_fisica`, `trabalho`.

### H. Documentos e anexos

A equipe deve propor:

- como armazenar arquivos no Supabase Storage;
- como vincular documentos a entidades do domínio;
- como versionar e organizar arquivos;
- como classificar documentos por tipo;
- como tratar PDFs protegidos por senha.

### I. Segredos e segurança

A equipe deve propor:

- como tratar segredos como senha de PDF;
- como evitar exposição em tabelas comuns;
- uso de cofre/segredos;
- RLS;
- segregação de permissões;
- operações server-side sensíveis.

### J. Backup e portabilidade

A equipe deve propor:

- estratégia de backup do banco;
- estratégia de backup dos arquivos;
- estratégia para manter cópia local organizada;
- estratégia de exportação do sistema.

### K. Fluxos principais do MVP

A equipe deve descrever fluxos de uso como:

- cadastrar banco e produtos;
- cadastrar ciclo financeiro do usuário;
- lançar transação manual;
- registrar transferência interna;
- registrar pagamento de fatura;
- cadastrar dívida;
- importar planilha;
- anexar documento;
- aplicar categoria e tags;
- visualizar despesas por tag;
- marcar ou revisar prioridade;
- visualizar próximos vencimentos;
- visualizar painel de quitação de dívidas;
- visualizar relatório por mês civil;
- visualizar relatório por período financeiro personalizado.

### L. Painéis e relatórios do MVP

A equipe deve propor telas e relatórios mínimos:

- visão por instituição;
- visão por produto;
- fluxo mensal;
- próximos vencimentos;
- faturas em aberto;
- dívidas por produto;
- juros pagos;
- amortização acumulada;
- plano de quitação;
- visão por ciclo financeiro personalizado;
- visão “quanto dinheiro precisa durar até o próximo recebimento”;
- análise por categoria;
- análise por tag;
- visão de despesas essenciais vs postergáveis;
- fila/priorização operacional de pagamentos.

### M. Primeira tela do sistema

A equipe deve propor a **primeira tela do produto** como uma tela orientada a ação e decisão, não apenas um dashboard passivo.

Essa primeira tela deve priorizar a resposta a perguntas como:

- o que vence primeiro?
- o que é essencial?
- o que está atrasado?
- o que ameaça meu período financeiro atual?
- o que eu ainda consigo postergar?
- quanto dinheiro precisa durar até o próximo recebimento?
- quais gastos estão fora do padrão?
- quais itens com alta prioridade ainda não foram pagos?

A equipe deve propor como essa primeira tela combina:

- vencimento;
- prioridade;
- essencialidade;
- saldo disponível;
- período financeiro atual;
- dívidas;
- faturas;
- alertas.

### N. Arquitetura de frontend e design system

A equipe deve propor explicitamente:

#### N.1. Estratégia web

- arquitetura web com **React + Next.js + Tailwind**
- organização de pastas
- estratégia de componentes
- estratégia de roteamento
- estratégia de formulários
- estratégia de estado
- estratégia de autenticação com Supabase
- estratégia de consumo de dados

#### N.2. Estratégia mobile

- uso de **React Native** e, se fizer sentido, **Expo**
- alinhamento de design e experiência com o web
- estratégia de compartilhamento com o monorepo ou estrutura similar
- o que pode e o que não pode ser compartilhado entre web e mobile

#### N.3. Design system e biblioteca-base

A equipe deve escolher e justificar:

- um design system de referência
- uma biblioteca-base ou abordagem compatível com React/Next.js/Tailwind
- diretrizes de acessibilidade
- diretrizes visuais apropriadas para produto financeiro
- tokens visuais
- padrão de componentes reutilizáveis
- estratégia para manter consistência entre web e mobile

#### N.4. Critérios de escolha

A decisão deve levar em conta:

- produtividade
- consistência
- manutenção
- maturidade do ecossistema
- aderência ao React
- aderência ao Next.js
- aderência ao Tailwind
- viabilidade de extensão para mobile
- clareza visual para dados financeiros
- qualidade de UX

### O. Estratégia de testes e especificação orientada por comportamento

A equipe deve assumir como diretriz fundamental que **os testes moldam o comportamento do sistema**.

Os testes não são apenas validação posterior.  
Eles devem funcionar como **especificação viva**, contrato funcional e instrumento de clareza do domínio.

#### O.1. Princípio geral

Antes ou junto da implementação, a equipe deve explicitar os testes e cenários que definem o comportamento esperado do sistema.

A implementação deve seguir os testes especificados para o domínio e para os fluxos críticos.

#### O.2. Testes são mandatórios

A equipe deve tratar como obrigatória a especificação de testes para:

- regras de negócio centrais;
- fluxos críticos do usuário;
- cálculos financeiros;
- classificação de eventos financeiros;
- recorrências;
- períodos financeiros personalizados;
- faturas;
- dívidas;
- amortização;
- juros;
- transferências internas;
- importação de histórico;
- conciliação básica;
- categorização;
- tags;
- prioridade de pagamento;
- ordenação operacional da primeira tela.

#### O.3. Os testes não devem ser alterados por conveniência de implementação

Os testes devem ser tratados como contrato do sistema.

Eles **não devem ser alterados apenas para acomodar uma implementação ruim, incompleta ou oportunista**.

Os testes só podem ser revistos quando houver:

- mudança legítima de regra de negócio;
- mudança explícita de escopo;
- correção de entendimento do domínio;
- refinamento funcional aprovado;
- simplificação intencional do MVP com justificativa clara.

#### O.4. A equipe deve propor estratégia de testes por camadas

A proposta da equipe deve incluir recomendação para:

- testes de domínio / unidade para regras financeiras;
- testes de integração entre serviços e persistência;
- testes de API;
- testes de fluxos críticos;
- testes de interface apenas onde agregarem valor real;
- testes de regressão para regras sensíveis.

#### O.5. Casos de teste devem nascer no refinamento

A equipe não deve retornar apenas com arquitetura e tabelas.

Ela deve também propor:

- cenários de aceitação;
- exemplos concretos;
- casos-limite;
- comportamentos inválidos;
- comportamentos esperados;
- invariantes do sistema.

#### O.6. Regras críticas que obrigatoriamente devem ter testes especificados

A equipe deve trazer testes especificados para pelo menos os seguintes casos:

1. pagamento de fatura não pode gerar nova despesa;
2. transferência entre contas próprias não pode ser contabilizada como gasto;
3. lançamentos devem ser corretamente atribuídos ao período financeiro personalizado;
4. ciclo financeiro do usuário deve respeitar data de início e fim configuradas;
5. compras em cartão devem respeitar ciclo de fechamento e vencimento;
6. empréstimos e financiamentos devem separar amortização, juros e encargos;
7. recorrências devem gerar expectativa de ocorrência sem necessariamente marcar pagamento automático;
8. estornos e ajustes não podem distorcer receita ou despesa;
9. importações não podem duplicar registros sem controle;
10. documentos anexados não devem alterar saldo automaticamente sem regra explícita;
11. quitação antecipada de dívida deve recalcular corretamente saldo e impacto;
12. relatórios por mês civil e por período financeiro personalizado devem poder divergir corretamente;
13. uma despesa pode conter múltiplas tags sem perda de integridade;
14. filtros por tag devem funcionar corretamente;
15. prioridades de pagamento devem influenciar ordenação e alertas;
16. itens essenciais não devem ser tratados como postergáveis quando a regra determinar o contrário;
17. a primeira tela deve refletir corretamente vencimento, prioridade e período financeiro.

#### O.7. Entregável adicional obrigatório

Além dos entregáveis já pedidos, a equipe deve retornar também com:

- estratégia de testes do projeto;
- lista dos testes mandatórios do MVP;
- cenários de aceitação por fluxo crítico;
- regras que exigem testes de regressão;
- proposta de organização da suíte de testes.

### P. IA futura

Sem implementar agora, a equipe deve mapear onde a IA entrará depois:

- classificação automática;
- leitura de comprovantes;
- leitura de PDFs e notas;
- importação inteligente;
- conciliação;
- resumo do mês;
- assistente conversacional;
- sugestão de tags;
- sugestão de prioridade;
- MCP.

### Q. Roadmap

A equipe deve propor fases claras:

- Fase 1: MVP sem IA
- Fase 2: importação e automação básica
- Fase 3: documentos e leitura assistida
- Fase 4: IA
- Fase 5: MCP/agentes

---

## 6. Orientações específicas sobre a dimensão do tempo

A equipe deve considerar que “mês” não é um conceito único no sistema.

O sistema precisa operar com pelo menos três visões temporais:

### 6.1. Mês civil

Exemplo:

- 01/03 a 31/03

### 6.2. Período financeiro do usuário

Exemplo:

- 20/03 a 19/04

### 6.3. Ciclo da fatura/cartão

Exemplo:

- fechamento dia 15;
- vencimento dia 23;
- compras após o fechamento caem na fatura seguinte.

A equipe deve explicar como essas três janelas convivem sem gerar ambiguidades.

Também deve explicar:

- qual data será usada para saldo;
- qual data será usada para orçamento;
- qual data será usada para análise de sobrevivência até o próximo pagamento;
- qual data será usada para previsão de estouro;
- como isso afetará parcelamentos, recorrências e vencimentos.

---

## 7. Restrições e cuidados

A equipe deve evitar:

- escopo excessivo no MVP;
- tentar resolver OCR perfeito desde o início;
- depender de disciplina manual intensa;
- misturar evento previsto com evento realizado;
- misturar dívida com despesa comum;
- misturar pagamento de fatura com nova despesa;
- jogar segredos em tabela comum;
- construir uma arquitetura complexa demais para um projeto pessoal em fase inicial;
- ignorar a modelagem de ciclos financeiros personalizados;
- escolher stack visual/frontend sem justificar aderência a React + Next.js + Tailwind + Supabase;
- propor design system incompatível com evolução coerente entre web e mobile;
- confundir categoria com tag;
- tratar prioridade de pagamento apenas como detalhe visual.

---

## 8. Entregáveis esperados da reunião de refino

A resposta da equipe deve vir em formato estruturado, contendo:

1. **Resumo executivo do entendimento do problema**
2. **Premissas assumidas**
3. **Escopo do MVP**
4. **Fora do escopo por enquanto**
5. **Mapa de domínio**
6. **Modelo de dados proposto**
7. **Modelagem da dimensão do tempo**
8. **Modelagem de categorias, tags e prioridades**
9. **Regras de negócio essenciais**
10. **Fluxos principais**
11. **Arquitetura sugerida com Supabase**
12. **Arquitetura sugerida para web e mobile**
13. **Escolha e justificativa do design system**
14. **Estratégia de documentos, segredos e backup**
15. **Estratégia de testes e cenários de aceitação**
16. **Lista de testes mandatórios do MVP**
17. **Riscos e dúvidas em aberto**
18. **Roadmap priorizado**
19. **Recomendações finais da equipe**

---

## 9. Orientação de qualidade

Não tragam resposta genérica.

A resposta deve ser:

- técnica;
- criteriosa;
- orientada a produto real;
- financeiramente coerente;
- extensível;
- prática para implementação;
- adequada para revisão posterior.

Quando houver ambiguidade, façam suposições explícitas.

Quando propuserem simplificações no MVP, justifiquem.

Priorizem uma base sólida de domínio e dados, mesmo que a interface inicial seja simples.

A dimensão do tempo e dos ciclos financeiros personalizados deve ser tratada como parte essencial da modelagem e não como detalhe cosmético.

A arquitetura de frontend e o design system devem ser tratados como elementos estratégicos do produto e não como detalhe visual posterior.

Categorias, tags e prioridade de pagamento devem ser tratadas como parte do comportamento operacional do sistema.

---

## 10. Instrução final para a equipe

Façam agora a reunião de refino completa e retornem com uma proposta inicial madura para revisão crítica.
