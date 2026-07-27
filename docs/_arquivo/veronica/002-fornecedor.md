# Prompt de revisão do refino — adicionar dimensão de fornecedor, unidades de consumo e auditoria histórica

A proposta de refino atual do projeto **Seu Bolso Feliz** está boa, mas precisa ser **revisada e expandida** para incorporar uma nova dimensão central do domínio:

# Nova exigência central

O sistema deve passar a tratar **fornecedor** como entidade própria e estratégica do domínio.

Essa revisão não deve ser cosmética.  
A equipe deve **revisar o refino atual** e incorporar fornecedor de forma consistente em:

- modelo de dados;
- regras de negócio;
- recorrências;
- documentos;
- categorização;
- conciliação;
- relatórios;
- auditoria;
- leitura futura por IA/MCP.

---

## 1. Contexto da nova exigência

O usuário quer saber com precisão:

- para quais fornecedores o dinheiro está indo;
- quanto está sendo gasto por fornecedor;
- quais despesas recorrentes pertencem a cada fornecedor;
- como um mesmo fornecedor aparece em diferentes contextos;
- como comparar históricos por fornecedor;
- como cruzar lançamentos diretos, faturas de cartão e documentos com fornecedor;
- como armazenar nomes antigos, novos e aliases do mesmo fornecedor;
- como lidar com fornecedores de utilidades e telecom que também possuem métricas operacionais, como:
  - kWh,
  - consumo,
  - unidade consumidora,
  - velocidade/plano,
  - linhas telefônicas,
  - identificadores de contrato,
  - número da instalação,
  - número do cliente,
  - conta contrato,
  - etc.

Exemplos reais:

- GitHub é fornecedor;
- AnswerThePublic é fornecedor;
- Vivo é fornecedor;
- Claro é fornecedor;
- Canaã é fornecedor;
- Neoenergia é fornecedor;
- nomes antigos e nomes novos do mesmo fornecedor precisam ser suportados;
- uma mesma tag como `internet` pode agrupar despesas de fornecedores diferentes;
- um mesmo fornecedor pode aparecer em despesas diretas, faturas, cartão de crédito, boleto e débito.

---

## 2. O que precisa mudar no refino atual

A equipe deve **revisar o refino atual** e atualizar explicitamente os seguintes blocos.

### 2.1. Mapa do domínio

Adicionar a entidade de **fornecedor** como parte estruturante do domínio.

A equipe deve propor:

- fornecedor como entidade independente;
- relacionamento entre fornecedor e lançamentos;
- relacionamento entre fornecedor e recorrências;
- relacionamento entre fornecedor e documentos;
- relacionamento entre fornecedor e faturas/itens de fatura;
- relacionamento entre fornecedor e contas/contratos;
- relacionamento entre fornecedor e métricas de consumo, quando existirem.

A equipe deve deixar claro que:

- categoria **não substitui** fornecedor;
- tag **não substitui** fornecedor;
- fornecedor é uma dimensão paralela e complementar.

---

## 3. Requisitos funcionais novos relacionados a fornecedor

A equipe deve incorporar no refinamento que o sistema precisa permitir:

1. cadastrar fornecedores;
2. associar lançamentos a fornecedor;
3. associar recorrências a fornecedor;
4. associar documentos a fornecedor;
5. associar contratos/contas/identificadores externos a fornecedor;
6. registrar aliases e nomes antigos;
7. consolidar histórico quando um fornecedor muda de nome;
8. agrupar gastos por fornecedor;
9. filtrar despesas por fornecedor;
10. cruzar fornecedor com:

- categoria,
- tags,
- prioridade,
- instituição financeira,
- produto financeiro,
- período financeiro,
- cartão,
- recorrência,
- documento,
- unidade de consumo;

11. identificar despesas pagas no cartão mas pertencentes a um fornecedor específico;
12. auditar histórico por fornecedor;
13. permitir que IA/MCP use fornecedor como dimensão de classificação e conciliação.

---

## 4. Revisão obrigatória do modelo de dados

A equipe deve revisar o modelo de dados atual para incluir explicitamente, no mínimo, algo nesta linha:

- suppliers
- supplier_aliases
- supplier_categories ou supplier_types
- supplier_accounts_or_contracts
- transaction_suppliers (se a modelagem permitir mais de um fornecedor por evento)
- recurring_template_suppliers
- document_suppliers
- supplier_metrics ou structure similar
- supplier_units ou structure similar, se fizer sentido
- supplier_audit_links ou estrutura equivalente, se necessário

A equipe pode renomear, ajustar ou simplificar, desde que justifique.

### 4.1. A entidade fornecedor deve suportar, quando aplicável:

- nome principal;
- nome fantasia;
- razão social, se útil;
- aliases;
- nomes antigos;
- tipo de fornecedor;
- categoria do fornecedor;
- status ativo/inativo;
- observações;
- identificadores externos;
- website, contato, referência, se útil;
- se é utilidade, telecom, SaaS, instituição, pessoa física, etc.

### 4.2. O fornecedor pode ser:

- empresa;
- pessoa física;
- concessionária;
- instituição financeira;
- prestador autônomo;
- operadora;
- software/SaaS;
- plataforma digital.

A equipe deve decidir se isso entra como:

- tipo do fornecedor;
- tags de fornecedor;
- categoria do fornecedor;
- ou combinação dessas abordagens.

---

## 5. Fornecedor e instituições financeiras

A equipe deve revisar a modelagem considerando que **instituições financeiras podem também ser tratadas como fornecedores em certos contextos**, mas isso não significa obrigatoriamente unificar as entidades sem critério.

A equipe deve avaliar e justificar uma destas abordagens:

### Opção A

Instituição financeira e fornecedor são entidades separadas, mas relacionadas quando necessário.

### Opção B

Fornecedor é uma superentidade e instituição financeira é um subtipo.

### Opção C

Outra modelagem equivalente, desde que bem justificada.

A resposta deve explicar:

- prós e contras;
- impacto em consultas;
- impacto em relatórios;
- impacto em manutenção;
- impacto em conciliação;
- impacto em IA futura.

---

## 6. Fornecedor em recorrências

A equipe deve revisar a modelagem de recorrências para permitir:

- uma recorrência ter fornecedor associado;
- uma recorrência herdar fornecedor para as instâncias;
- uma fatura/documento recebido ser conciliado com fornecedor esperado;
- o histórico por fornecedor incluir despesas recorrentes corretamente;
- prioridade, categoria e tags poderem coexistir com fornecedor.

Exemplos:

- internet Vivo
- internet Canaã
- energia Neoenergia
- assinatura GitHub
- assinatura ChatGPT
- AnswerThePublic

---

## 7. Fornecedor e documentos

A equipe deve revisar o refino para suportar associação entre documentos e fornecedor.

Exemplos:

- PDF de conta de energia vinculado à Neoenergia;
- fatura vinculada ao banco/instituição e também ao fornecedor emissor;
- nota fiscal vinculada ao fornecedor;
- comprovante de pagamento vinculado à obrigação e ao fornecedor.

A equipe deve propor como a leitura futura por IA pode usar:

- nome do fornecedor;
- aliases;
- CNPJ, se encontrado;
- conta contrato;
- unidade consumidora;
- telefone;
- linha;
- número do cliente;
- instalação;
- plano;
- item de cobrança.

---

## 8. Fornecedor e métricas operacionais / consumo

A equipe deve revisar o refinamento para suportar fornecedores que geram despesas com **métricas mensuráveis**.

Exemplos:

- energia:
  - kWh,
  - tarifa,
  - bandeira,
  - unidade consumidora,
  - valor por mês;
- telecom:
  - linha,
  - plano,
  - velocidade,
  - franquia,
  - unidade/contrato;
- SaaS:
  - número de licenças,
  - plano mensal,
  - usuários;
- serviços:
  - horas,
  - sessões,
  - diárias;
- utilidades em geral:
  - unidade de medição,
  - quantidade,
  - preço unitário,
  - valor total.

A equipe deve propor uma modelagem que permita registrar, quando aplicável:

- nome da métrica;
- unidade;
- quantidade;
- período de medição;
- preço unitário;
- subtotal;
- observações;
- vínculo ao lançamento e/ou documento e/ou fornecedor.

A equipe deve evitar uma modelagem rígida demais, mas também evitar abstração genérica inútil.

---

## 9. Auditoria histórica e conciliação

A equipe deve incorporar no refinamento que o sistema precisa permitir, futuramente, auditoria histórica com base em fornecedor.

Exemplos de perguntas que o sistema deve conseguir responder:

- quanto gastei com a Neoenergia nos últimos 12 meses?
- qual foi o consumo de kWh mês a mês?
- quanto paguei de internet somando Vivo e Canaã?
- quais cobranças do GitHub foram pagas no cartão?
- quais cobranças recorrentes do mesmo fornecedor mudaram de valor?
- quais despesas de um fornecedor específico ficaram sem documento?
- quais lançamentos antigos parecem pertencer ao mesmo fornecedor mesmo com nomes diferentes?

A equipe deve propor como isso afeta:

- conciliação;
- importação;
- relatórios;
- enriquecimento por IA;
- governança do histórico.

---

## 10. Revisão da camada de categorias, tags e prioridade

A equipe deve atualizar o refinamento para deixar claro:

- categoria != fornecedor
- tag != fornecedor
- prioridade != fornecedor

Mas todos esses elementos podem coexistir no mesmo lançamento.

Exemplo:

- fornecedor: `Vivo`
- categoria: `internet/telefonia`
- tags:
  - `internet`
  - `apoio_operacional`
  - `contingencia`
- prioridade:
  - `alta`

Outro exemplo:

- fornecedor: `ChatGPT`
- categoria: `software/serviços digitais`
- tags:
  - `trabalho_externo`
  - `pesquisa_desenvolvimento`
  - `ensino`
- prioridade:
  - `média` ou conforme regra adotada

A equipe deve revisar as telas e filtros considerando essa combinação.

---

## 11. Revisão da primeira tela do sistema

A primeira tela também deve passar a refletir a dimensão fornecedor.

A equipe deve revisar a proposta da home para permitir, de forma útil:

- ver principais fornecedores do período;
- ver fornecedores com maior crescimento;
- ver fornecedores críticos/essenciais;
- ver despesas recorrentes por fornecedor;
- identificar fornecedores com pagamentos pendentes;
- identificar variações suspeitas por fornecedor;
- identificar duplicidades ou cobranças incomuns por fornecedor.

Não precisa virar um dashboard excessivamente carregado, mas a equipe deve explicar como fornecedor entra na visão operacional principal.

---

## 12. Revisão da estratégia de testes

A equipe deve revisar a estratégia de testes e acrescentar cenários obrigatórios envolvendo fornecedor.

Devem existir testes especificados para casos como:

1. um lançamento pode ser corretamente associado a fornecedor;
2. aliases do mesmo fornecedor devem consolidar histórico;
3. mudança de nome do fornecedor não pode quebrar histórico;
4. uma recorrência com fornecedor deve gerar instâncias com vínculo coerente;
5. relatórios por fornecedor devem somar corretamente despesas pagas por diferentes meios;
6. lançamentos no cartão devem continuar sendo atribuídos ao fornecedor correto;
7. categoria, tags, fornecedor e prioridade devem coexistir sem conflito de integridade;
8. métricas de consumo por fornecedor devem poder ser armazenadas e consultadas corretamente;
9. conciliação histórica deve suportar nomes alternativos do mesmo fornecedor;
10. filtros por fornecedor devem funcionar em conjunto com período financeiro, categoria, tags e prioridade.

---

## 13. Revisão da IA futura / MCP

A equipe deve revisar a seção de IA futura para considerar fornecedor como dimensão relevante de inteligência.

A futura camada de IA/MCP deve poder usar fornecedor para:

- classificação automática de lançamentos;
- conciliação de documentos;
- resolução de aliases;
- sugestão de fornecedor provável;
- agrupamento de histórico;
- detecção de anomalias;
- extração de dados estruturados de contas e faturas;
- auditoria histórica;
- enriquecimento de relatórios.

A equipe deve explicitar como fornecedor entrará:

- na taxonomia do domínio;
- nas tool calls futuras;
- nas rotinas de classificação;
- e na reconciliação de dados importados.

---

## 14. Entregável esperado desta revisão

A equipe deve responder revisando o refino atual e entregando, no mínimo:

1. **Resumo do impacto da nova dimensão fornecedor**
2. **Alterações necessárias no mapa de domínio**
3. **Alterações necessárias no modelo de dados**
4. **Decisão sobre a relação entre fornecedor e instituição financeira**
5. **Modelagem de aliases, nomes antigos e consolidação histórica**
6. **Modelagem de métricas de consumo/unidades**
7. **Impacto em recorrências, documentos e conciliação**
8. **Impacto em relatórios, filtros e auditoria**
9. **Impacto na home/tela inicial**
10. **Impacto na estratégia de testes**
11. **Impacto na futura camada de IA/MCP**
12. **Versão revisada das entidades e regras afetadas**

---

## 15. Orientação final

Não respondam apenas com uma nota de rodapé dizendo “adicionamos fornecedor”.

Façam uma revisão estrutural do refino atual incorporando fornecedor como dimensão central do domínio, com profundidade suficiente para sustentar:

- relatórios por fornecedor;
- auditoria histórica;
- conciliação;
- métricas mensuráveis;
- aliases e nomes antigos;
- leitura futura por IA;
- e uso futuro pelo MCP.
