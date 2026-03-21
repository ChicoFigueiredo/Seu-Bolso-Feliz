---
Título da Reunião: Revisão do Refino — Incorporação da Dimensão Fornecedor, Unidades de Consumo e Auditoria Histórica
Data e Hora: 2026-03-21 11:57
Referência: Revisão do documento "2026-03-21-10-40-refino-tecnico-funcional-kickoff-seu-bolso-feliz.md"
Prompt de origem: docs/Veronica/002-fornecedor.md
Participantes:
  - Chico (CEO) — facilitador, decisão final
  - Verônica (Consultora Externa) — orientação funcional e visão de produto
  - Ana Silva (Arquiteta de Software) — arquitetura e design de sistemas
  - Carlos Mendes (Designer de Software) — UX/UI e prototipagem
  - João Pereira (Backend Sênior — Node/Bun) — backend e APIs
  - Maria Oliveira (Backend Sênior — Node/Bun) — backend, segurança e testes
  - Pedro Santos (Backend Sênior — Python/Django) — modelagem e integração
  - Laura Costa (Backend Sênior — Python/Django) — backend e DevOps básico
  - Roberto Lima (Frontend Sênior — React/Next.js) — frontend web
  - Sofia Almeida (Frontend Sênior — React/Next.js) — frontend web e responsividade
  - Lucas Ferreira (Mobile Sênior — React Native/Expo) — mobile
  - Beatriz Rocha (Mobile Sênior — React Native/Expo) — mobile e publicação
  - Fernando Gomes (DevOps Sênior) — infraestrutura e CI/CD
  - Ricardo Monteiro (Economista / Consultor Financeiro) — modelagem financeira e regras de negócio
  - Camila Duarte (Consultora de Finanças Pessoais) — experiência do usuário financeiro
  - Gabriela Nunes (Marketing Digital) — SEO e estratégia de lançamento
  - Helena Vargas (UX/UI Especialista) — pesquisa de usuários e prototipagem
  - Isabella Torres (UI Designer) — hierarquia visual e micro-interações
  - Thiago Martins (Front Engineer) — componentização e performance
  - Renata Silva (QA Visual/A11y) — acessibilidade e testes visuais
  - André Santos (DBA Sênior PostgreSQL) — modelagem de dados e otimização
Pauta: 1. Resumo do impacto da nova dimensão fornecedor
  2. Alterações no mapa de domínio
  3. Decisão sobre fornecedor vs instituição financeira
  4. Alterações no modelo de dados (novas tabelas e campos)
  5. Modelagem de aliases, nomes antigos e consolidação histórica
  6. Modelagem de métricas de consumo / unidades
  7. Impacto em recorrências
  8. Impacto em documentos e conciliação
  9. Revisão de categorias, tags e prioridade com fornecedor
  10. Impacto em relatórios, filtros e auditoria
  11. Impacto na primeira tela (dashboard de ação)
  12. Impacto na estratégia de testes (novos cenários obrigatórios)
  13. Impacto na futura camada de IA/MCP
  14. Regras de negócio novas e revisadas
  15. Revisão do escopo do MVP
  16. Revisão do roadmap
  17. Decisões finais e próximos passos
---

# Reunião de Revisão do Refino — Dimensão Fornecedor

## 1. Abertura e Contexto

**Chico (CEO):**
Time, a Verônica nos mandou uma nova exigência que considero essencial: precisamos tratar **fornecedor** como entidade própria e central do domínio. Hoje, quando eu olho minha planilha, sei que pago a Vivo, a Neoenergia, o GitHub — mas no modelo atual do sistema, essa informação se perde nas descrições de texto. Preciso saber: para quem vai meu dinheiro, quanto estou gastando com cada fornecedor, se o valor mudou, se posso substituir. Isso não é cosmético — é estrutural.

**Verônica (Consultora Externa):**
Exatamente. O prompt que enviei pede uma revisão profunda, não uma nota de rodapé. Fornecedor é uma dimensão paralela e complementar a categoria e tag. Categoria responde "que tipo de gasto é?". Tag responde "como classifico?". Fornecedor responde "para QUEM vai o dinheiro?". São perguntas diferentes e todas precisam de resposta.

**Ana Silva (Arquiteta):**
Li o prompt inteiro. A exigência é clara e legítima. Impacta o modelo de dados, recorrências, documentos, relatórios, testes, e até a futura IA. Vamos tratar com a profundidade que merece.

**Ricardo Monteiro (Economista):**
Do ponto de vista financeiro, fornecedor é fundamental para auditoria. "Quanto gastei com a Neoenergia nos últimos 12 meses?" — essa é uma pergunta que qualquer consultor financeiro faz. Se o sistema não responde, está incompleto.

**Camila Duarte (Consultora de Finanças Pessoais):**
E para o usuário operacional, fornecedor responde: "estou pagando caro demais nessa internet?" ou "o GitHub aumentou o preço?". São perguntas de decisão, não de curiosidade.

---

## 2. Resumo do Impacto da Nova Dimensão Fornecedor

**Ana Silva (Arquiteta):**
Vou consolidar o impacto em cada camada do sistema:

| Camada                | Impacto                                                                                                                                                         | Severidade        |
| --------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------- |
| **Modelo de dados**   | Novas tabelas: suppliers, supplier_aliases, supplier_contracts, consumption_metrics. Novos FKs em transactions, recurring_templates, statement_items, documents | 🔴 Alto           |
| **Mapa do domínio**   | Fornecedor como entidade de primeira classe, paralela a categoria/tag                                                                                           | 🔴 Alto           |
| **Regras de negócio** | Novas regras R18-R24 sobre integridade de fornecedor                                                                                                            | 🟠 Médio          |
| **Recorrências**      | FK supplier_id em recurring_templates, herança para instâncias                                                                                                  | 🟠 Médio          |
| **Documentos**        | FK supplier_id em documents, associação IA futura                                                                                                               | 🟠 Médio          |
| **Categorias/Tags**   | Sem mudança estrutural — coexistem com fornecedor                                                                                                               | 🟢 Baixo          |
| **Relatórios**        | Novas dimensões de agrupamento e filtro                                                                                                                         | 🟠 Médio          |
| **Primeira tela**     | Nova seção opcional de fornecedores no dashboard                                                                                                                | 🟡 Baixo-Médio    |
| **Testes**            | 10 novos cenários obrigatórios (T18-T27)                                                                                                                        | 🟠 Médio          |
| **IA/MCP futura**     | Fornecedor como dimensão de classificação, conciliação e detecção de anomalias                                                                                  | 🟡 Baixo (futuro) |
| **Escopo MVP**        | 5-6 novas funcionalidades adicionadas ao MVP                                                                                                                    | 🟠 Médio          |

**Pedro Santos (Backend):**
Quantificando: estamos adicionando ~5 tabelas novas e ~4 FKs em tabelas existentes. É um incremento significativo mas gerenciável — não altera a arquitetura, estende o modelo. Não quebramos nada que já foi decidido.

**Maria Oliveira (Backend):**
Concordo. O impacto é **aditivo**, não **destrutivo**. As tabelas existentes recebem novos campos nullable (supplier_id), e as novas tabelas seguem o mesmo padrão de RLS e auditoria que já definimos.

**Decisão da equipe:** O impacto é alto no modelo de dados, médio na maioria das camadas, e não invalida nenhuma decisão anterior. Prosseguimos com a incorporação.

---

## 3. Alterações no Mapa do Domínio

### 3.1. Mapa do Domínio Revisado

**Ana Silva (Arquiteta):**
Atualizando a hierarquia do domínio aprovada no refino anterior para incluir fornecedor:

```
Usuário (User)
├── Preferências Financeiras (UserFinancialPreferences)
│   └── Ciclo financeiro personalizado
│
├── Instituições Financeiras (Institution)
│   ├── Produtos Financeiros (FinancialProduct)
│   │   ├── Contas (Account)
│   │   ├── Cartões (Card)
│   │   │   ├── Titulares/Adicionais (CardHolder)
│   │   │   └── Ciclos de Fatura (StatementCycle)
│   │   │       └── Itens da Fatura (StatementItem) ←── supplier_id (FK)
│   │   └── Passivos/Dívidas (Liability)
│   │       └── Parcelas (LiabilityInstallment)
│   └── ...múltiplos produtos
│
├── ★ Fornecedores (Supplier) ← NOVA ENTIDADE CENTRAL
│   ├── Aliases (SupplierAlias)
│   │   └── nomes antigos, nomes alternativos, variações
│   ├── Contratos/Identificadores (SupplierContract)
│   │   └── conta contrato, unidade consumidora, cliente, instalação
│   ├── Tags de Fornecedor (SupplierTag) ← N:N
│   └── Métricas de Consumo (ConsumptionMetric)
│       └── kWh, velocidade, licenças, horas, etc.
│
├── Transações (Transaction) ←── supplier_id (FK, nullable)
│   ├── Receita, Despesa, Estorno, Ajuste...
│   └── (mantém todos os tipos já definidos)
│
├── Transferências Internas (Transfer)
│   └── (sem fornecedor — é movimento interno)
│
├── Templates Recorrentes (RecurringTemplate) ←── supplier_id (FK, nullable)
│   └── Instâncias Recorrentes (RecurringInstance)
│       └── (herda supplier do template)
│
├── Categorias (Category) — INALTERADA
├── Tags (Tag) — INALTERADA
├── Prioridade de Pagamento — INALTERADA
├── Períodos Financeiros (FinancialPeriod) — INALTERADO
│
├── Documentos (Document) ←── supplier_id (FK, nullable)
│   └── Vinculado a transação, dívida, fatura, fornecedor
│
└── Importações (ImportJob) — INALTERADA
```

### 3.2. Posição do Fornecedor no Domínio

**Ricardo Monteiro (Economista):**
Para ficar absolutamente claro — fornecedor é uma dimensão **ortogonal** às demais:

| Dimensão        | Pergunta que responde                     | Exemplo                        |
| --------------- | ----------------------------------------- | ------------------------------ |
| **Categoria**   | Que tipo de gasto é?                      | Moradia, Alimentação, Software |
| **Tag**         | Como classifico transversalmente?         | essencial, trabalho, casa      |
| **Fornecedor**  | Para QUEM vai o dinheiro?                 | Vivo, Neoenergia, GitHub       |
| **Prioridade**  | Quão urgente/importante é?                | essencial, alta, média         |
| **Instituição** | Onde está meu dinheiro / de qual banco é? | Caixa, Nubank, C6              |
| **Período**     | Quando aconteceu / a qual ciclo pertence? | 20/Mar-19/Abr                  |

**Camila Duarte:**
Perfeito. Um lançamento pode ter TODAS essas dimensões simultaneamente:

```
Exemplo completo de um lançamento:
- Transação: "Conta de Internet — Março/2026"
- Valor: R$ 149,90
- Fornecedor: Vivo
- Categoria: Internet/Telefonia
- Tags: [internet, casa, essencial]
- Prioridade: alta
- Instituição: Caixa (débito automático)
- Período: 20/Mar - 19/Abr
- Documento: PDF da conta vinculado
- Métrica de consumo: Plano 300Mbps, velocidade contratada
```

**André Santos (DBA):**
E do ponto de vista de banco de dados, cada dimensão é um FK ou tabela N:N — nenhuma substitui a outra, todas coexistem.

### 3.3. O que Fornecedor NÃO é

**Maria Oliveira (Backend):**
Para evitar confusão futura, vou documentar o que fornecedor NÃO é:

1. Fornecedor **NÃO substitui** categoria — "Vivo" não é categoria, "Internet/Telefonia" é
2. Fornecedor **NÃO substitui** tag — "Vivo" não é tag, "internet" é tag que pode agrupar Vivo + Canaã
3. Fornecedor **NÃO é** instituição financeira — Caixa é onde está o dinheiro, Vivo é para quem vai o dinheiro
4. Fornecedor **NÃO tem saldo** — não é conta, não é produto financeiro
5. Fornecedor **NÃO é obrigatório** em toda transação — transferências internas, ajustes e estornos podem não ter fornecedor

**Chico (CEO):**
Essa clareza é fundamental. Aprovado.

---

## 4. Decisão: Fornecedor vs Instituição Financeira

### 4.1. Contexto do Problema

**Verônica (Consultora):**
A Caixa Econômica é uma instituição financeira onde o CEO tem conta corrente e financiamento. Mas quando a Caixa cobra tarifa de manutenção, ela é também um "fornecedor" de serviço bancário. Quando o Nubank cobra anuidade do cartão, é um fornecedor. Precisamos decidir: unificamos ou separamos?

### 4.2. Análise das Opções

**Ana Silva (Arquiteta):**
Analisei as três opções do prompt da Verônica:

#### Opção A: Entidades separadas, relacionáveis quando necessário

- `institutions` = onde está o dinheiro (bancos, fintechs)
- `suppliers` = para quem vai o dinheiro (Vivo, GitHub, Neoenergia)
- Quando necessário, campo `institution_id` nullable em `suppliers` para ligar os dois

#### Opção B: Fornecedor como superentidade

- `suppliers` é a tabela central, `institutions` é um tipo de fornecedor
- Toda instituição é também um supplier

#### Opção C: Separação com link bidirecional

- Entidades separadas com tabela de link `supplier_institution_links`
- Apenas quando a instituição atua como fornecedor (tarifa, anuidade)

### 4.3. Debate da Equipe

**André Santos (DBA):**
Sou fortemente a favor da **Opção A** com um campo de link opcional. Razões técnicas:

1. `institutions` já tem semântica própria: é o container de produtos financeiros, contas, cartões, saldos. Misturar com "fornecedores genéricos" poluiria o modelo.
2. Um JOIN `suppliers JOIN institutions` quando a Caixa cobra tarifa é trivial e raro.
3. A maioria dos fornecedores (Vivo, GitHub, Neoenergia) NÃO é instituição financeira. Forçar todos por uma entidade única seria artificially.
4. RLS e queries ficam mais simples com entidades distintas.

**João Pereira (Backend):**
Concordo com André. Na Opção B, teríamos que colocar campos como `current_balance`, `credit_limit` em suppliers — que só fazem sentido para instituições. Ficaria sujo.

**Ricardo Monteiro (Economista):**
Do ponto de vista financeiro, a separação é natural. Instituição financeira é onde guardo e administro dinheiro. Fornecedor é para quem pago. São papéis diferentes. A Caixa pode ser ambos, mas o papel primário é "instituição". O papel secundário de "fornecedor" (quando cobra tarifa) é ocasional e pode ser resolvido com um link simples.

**Pedro Santos (Backend):**
Na Opção B, os relatórios de fornecedor misturariam "gastos com a Caixa" (tarifas) com "movimentações na Caixa" (saldos de conta). Confuso para o usuário.

**Sofia Almeida (Frontend):**
No frontend, faz mais sentido: na tela de "Minhas Instituições" eu vejo bancos e contas. Na tela de "Meus Fornecedores" eu vejo para quem pago. São espaços mentais diferentes do usuário.

**Laura Costa (Backend):**
E para IA futura, ter entidades distintas facilita: o modelo pode aprender que "Caixa" aparece em contextos de transferência/saldo (instituição) vs "Caixa — tarifa mensal" é fornecedor. Sem ambiguidade.

### 4.4. Análise de Prós e Contras

| Critério                      | Opção A (Separadas + link)                              | Opção B (Superentidade)         | Opção C (Link bidirecional)       |
| ----------------------------- | ------------------------------------------------------- | ------------------------------- | --------------------------------- |
| **Simplicidade de modelo**    | ✅ Cada entidade com seus campos próprios               | ❌ Muitos campos nullable       | ⚠️ Tabela extra de link           |
| **Clareza semântica**         | ✅ Instituição = dinheiro, Fornecedor = pagamento       | ❌ Mistura papéis               | ✅ Clara mas complexa             |
| **Impacto em queries**        | ✅ Queries diretas, JOIN simples quando necessário      | ⚠️ WHERE type = para filtrar    | ⚠️ JOIN extra pela tabela de link |
| **Impacto em relatórios**     | ✅ "Gastos por fornecedor" não mistura saldos           | ❌ Confusão entre saldo e gasto | ✅ OK com cuidado                 |
| **Impacto em manutenção**     | ✅ Evolução independente de cada entidade               | ⚠️ Mudança em uma afeta outra   | ⚠️ Mais uma tabela para manter    |
| **Impacto em conciliação**    | ✅ Fornecedor para lançamentos, Instituição para contas | ⚠️ Ambíguo                      | ✅ OK                             |
| **Impacto em IA futura**      | ✅ Classificação sem ambiguidade                        | ⚠️ IA precisa distinguir papéis | ✅ OK                             |
| **Caso "Caixa cobra tarifa"** | ⚠️ Precisa criar um Supplier "Caixa" + link             | ✅ Já é supplier                | ✅ Link resolve                   |

### 4.5. Decisão Final

**Chico (CEO):**
Vou com a recomendação da equipe: **Opção A — Entidades separadas com link opcional**.

**Decisão:**

- `institutions` permanece como está — container de produtos financeiros
- `suppliers` é entidade nova e independente
- Campo `institution_id` (FK, nullable) em `suppliers` para os raros casos em que a instituição também atua como fornecedor (tarifa, anuidade, juros cobrados)
- Quando a Caixa cobra tarifa de manutenção, o lançamento tem: `institution_id` = Caixa (onde sai o dinheiro), `supplier_id` = Caixa-Fornecedor (para quem vai o dinheiro). São FKs para tabelas diferentes.
- A equipe cria um Supplier "Caixa Econômica Federal" linkado à Institution "Caixa" apenas quando necessário — não é automático.

**Ricardo Monteiro:**
Isso é correto. Na prática, poucos lançamentos terão a instituição como fornecedor. A maioria dos fornecedores (Vivo, Neoenergia, GitHub) jamais é instituição financeira.

---

## 5. Alterações no Modelo de Dados

### 5.1. Novas Tabelas

**André Santos (DBA):**
Proponho as seguintes novas tabelas, seguindo o padrão já definido (UUID v7, user_id para RLS, timestamps UTC):

#### 5.1.1. suppliers (Fornecedores)

```
suppliers
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── name: text — nome principal de exibição (ex: "Vivo", "Neoenergia")
├── trade_name: text (nullable) — nome fantasia, se diferente
├── legal_name: text (nullable) — razão social, se útil
├── document_number: text (nullable) — CNPJ ou CPF, se conhecido
├── type: enum (company, individual, utility, telecom, saas, financial_institution,
│         autonomous, platform, government, other)
├── website: text (nullable)
├── contact_info: text (nullable) — telefone, email, SAC
├── notes: text (nullable) — observações livres
├── institution_id: uuid (FK → institutions, nullable) — link quando a IF é também fornecedor
├── is_active: boolean (default true)
├── display_order: int (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz
```

**Maria Oliveira (Backend):**
O campo `type` como enum cobre a diversidade exigida pela Verônica: empresa, pessoa física, concessionária, SaaS, etc. É extensível sem ser genérico demais.

**Camila Duarte:**
O campo `notes` é importante. Muitas vezes o usuário quer anotar algo como "plano antigo, migrar em setembro" ou "contrato termina em dezembro". Informação operacional que não cabe em campo estruturado.

#### 5.1.2. supplier_aliases (Aliases e Nomes Antigos)

```
supplier_aliases
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── supplier_id: uuid (FK → suppliers)
├── alias_name: text — nome alternativo (ex: "CELPE" para "Neoenergia Pernambuco")
├── alias_type: enum (former_name, abbreviation, trade_name, billing_name, other)
├── valid_from: date (nullable) — desde quando esse nome era usado
├── valid_until: date (nullable) — até quando esse nome era usado (null = ainda válido)
├── notes: text (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz
```

**Pedro Santos (Backend):**
O `alias_type` é estratégico para IA futura. Quando um lançamento importado tem "CELPE" na descrição, o sistema (ou IA) pode buscar em `supplier_aliases` e resolver que é Neoenergia. O `valid_from`/`valid_until` ajuda a desambiguar: se o nome mudou em 2020, lançamentos anteriores a 2020 com "CELPE" apontam para Neoenergia mas com contexto temporal.

**Ricardo Monteiro:**
Exemplos reais que isso resolve:

- CELPE → Neoenergia Pernambuco (mudança societária)
- GVT → Vivo (aquisição)
- Oi → Oi/V.tal (reorganização)
- "GITHUB.COM" → GitHub (nome como aparece na fatura do cartão)
- "OPENAI \*CHATGPT" → ChatGPT/OpenAI (descrição no extrato bancário)

#### 5.1.3. supplier_contracts (Contratos e Identificadores Externos)

```
supplier_contracts
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── supplier_id: uuid (FK → suppliers)
├── contract_type: enum (utility_account, phone_line, subscription, service_contract,
│                        customer_id, installation, other)
├── identifier: text — número do contrato, conta, instalação, linha, cliente
├── label: text (nullable) — descrição amigável (ex: "Linha principal", "Casa", "Escritório")
├── is_active: boolean (default true)
├── start_date: date (nullable)
├── end_date: date (nullable)
├── metadata: jsonb (nullable) — dados extras (ex: plano contratado, velocidade, endereço)
├── notes: text (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz
```

**João Pereira (Backend):**
Essa tabela resolve todos os cenários de identificadores que a Verônica menciona: unidade consumidora, conta contrato, número da instalação, linha telefônica, número do cliente. Em vez de criar uma coluna para cada tipo, temos `contract_type` + `identifier` + `label` — flexível e claro.

**Camila Duarte:**
Exemplo real: Neoenergia pode ter duas instalações — casa e escritório. Cada uma com sua unidade consumidora, sua conta contrato. O mesmo fornecedor, contratos diferentes, faturas diferentes. Essa tabela captura isso perfeitamente.

**André Santos (DBA):**
E o `metadata` em jsonb permite armazenar dados específicos sem poluir o schema:

```json
{
  "plan": "Residencial B1",
  "voltage": "220V",
  "address": "Rua X, 123"
}
```

#### 5.1.4. supplier_tags (Tags N:N para Fornecedores)

```
supplier_tags
├── supplier_id: uuid (FK → suppliers)
├── tag_id: uuid (FK → tags)
└── PK: (supplier_id, tag_id)
```

**Maria Oliveira:**
Reutilizamos a tabela `tags` existente. Assim como transações e recorrências podem ter tags, fornecedores também. Exemplo: Vivo poderia ter tags `internet`, `telefonia`, `essencial`. Isso permite filtrar fornecedores por tag — "mostre todos os fornecedores essenciais".

#### 5.1.5. consumption_metrics (Métricas de Consumo)

```
consumption_metrics
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── supplier_id: uuid (FK → suppliers)
├── supplier_contract_id: uuid (FK → supplier_contracts, nullable) — vínculo opcional com contrato específico
├── transaction_id: uuid (FK → transactions, nullable) — vínculo com lançamento, se aplicável
├── document_id: uuid (FK → documents, nullable) — vínculo com documento, se extraído de fatura
├── reference_period_start: date — início do período de medição
├── reference_period_end: date — fim do período de medição
├── metric_name: text — nome da métrica (ex: "Consumo kWh", "Velocidade contratada", "Licenças")
├── metric_unit: text — unidade (ex: "kWh", "Mbps", "unidades", "horas", "GB")
├── quantity: numeric(15,4) — quantidade medida
├── unit_price: numeric(15,6) (nullable) — preço unitário (ex: R$/kWh)
├── subtotal: numeric(15,2) (nullable) — valor parcial desta métrica
├── metadata: jsonb (nullable) — dados extras (ex: bandeira tarifária, faixa de consumo)
├── notes: text (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz
```

**Ricardo Monteiro (Economista):**
Essa tabela é excelente para auditoria histórica. Imagine poder ver: "meu consumo de kWh foi de 280 em janeiro, 320 em fevereiro, 450 em março — o que aconteceu?" Ou: "a tarifa da Neoenergia subiu de R$ 0,72 para R$ 0,85 — impacto de R$ 40/mês no custo".

**Pedro Santos:**
Os campos `metric_name` e `metric_unit` como texto livre (não enum) são intencionais — cada fornecedor pode ter métricas diferentes. Energia tem kWh, telecom tem Mbps, SaaS tem licenças. Não faz sentido engessar em enum.

**Camila Duarte:**
Para o CEO, isso resolve perguntas como:

- "Quanto aumentou meu consumo de energia desde que comprei o ar-condicionado?"
- "Compensa trocar de plano de internet de 300Mbps para 500Mbps?"
- "O GitHub está mais caro que o ano passado?"

**João Pereira:**
O `metadata` em jsonb permite capturar informações específicas do tipo de fornecedor:

```json
// Energia
{"tariff_flag": "yellow", "consumption_tier": "B1", "icms_rate": "25%"}

// Telecom
{"download_speed": "300Mbps", "upload_speed": "150Mbps", "franchise": "unlimited"}

// SaaS
{"plan": "Team", "seats_used": 3, "seats_total": 5}
```

### 5.2. Alterações em Tabelas Existentes

**André Santos (DBA):**
As seguintes tabelas existentes recebem novos campos:

#### transactions — adicionar supplier_id

```
transactions
├── ... (campos existentes mantidos)
├── supplier_id: uuid (FK → suppliers, nullable) ← NOVO
└── ...
```

#### recurring_templates — adicionar supplier_id

```
recurring_templates
├── ... (campos existentes mantidos)
├── supplier_id: uuid (FK → suppliers, nullable) ← NOVO
└── ...
```

#### statement_items — adicionar supplier_id

```
statement_items
├── ... (campos existentes mantidos)
├── supplier_id: uuid (FK → suppliers, nullable) ← NOVO
└── ...
```

#### documents — adicionar supplier_id

```
documents
├── ... (campos existentes mantidos)
├── supplier_id: uuid (FK → suppliers, nullable) ← NOVO
└── ...
```

#### liabilities — adicionar supplier_id (para financiamentos/empréstimos com fornecedor identificável)

```
liabilities
├── ... (campos existentes mantidos)
├── supplier_id: uuid (FK → suppliers, nullable) ← NOVO
└── ...
```

**Maria Oliveira (Backend):**
Todos os novos campos são **nullable** propositalmente. Fornecedor é desejável mas não obrigatório. Um ajuste manual ou estorno pode não ter fornecedor. Uma transferência interna nunca tem fornecedor. Forçar preenchimento seria aumentar atrito operacional — exatamente o que queremos evitar.

**Thiago Martins (Front Engineer):**
No frontend, o campo de fornecedor no formulário de transação será um autocomplete/combobox com busca — o usuário digite "Vi..." e aparece "Vivo". Baixo atrito. Opcional. Sugestão automática baseada na recorrência quando aplicável.

### 5.3. Justificativa de Simplificações

**André Santos (DBA):**
Comparando com as sugestões do prompt da Verônica:

| Sugestão da Verônica                   | Nossa implementação                                       | Justificativa                                                                               |
| -------------------------------------- | --------------------------------------------------------- | ------------------------------------------------------------------------------------------- |
| `suppliers`                            | ✅ `suppliers`                                            | Direto                                                                                      |
| `supplier_aliases`                     | ✅ `supplier_aliases`                                     | Direto                                                                                      |
| `supplier_categories / supplier_types` | ✅ Campo `type` enum em `suppliers` + `supplier_tags` N:N | Enum para tipo primário, tags para classificação flexível. Evita tabela extra só para tipos |
| `supplier_accounts_or_contracts`       | ✅ `supplier_contracts`                                   | Renomeado para clareza                                                                      |
| `transaction_suppliers`                | ❌ → FK `supplier_id` direto em `transactions`            | Um lançamento tem no máximo 1 fornecedor. N:N seria over-engineering                        |
| `recurring_template_suppliers`         | ❌ → FK `supplier_id` direto em `recurring_templates`     | Mesma razão                                                                                 |
| `document_suppliers`                   | ❌ → FK `supplier_id` direto em `documents`               | Mesma razão                                                                                 |
| `supplier_metrics`                     | ✅ `consumption_metrics`                                  | Renomeado para clareza do propósito                                                         |
| `supplier_units`                       | ❌ → Absorvido em `consumption_metrics.metric_unit`       | Não justifica tabela separada                                                               |
| `supplier_audit_links`                 | ❌ → Coberto por `audit_logs` existente                   | A tabela de auditoria já é genérica (entity_type + entity_id)                               |

**Maria Oliveira:**
A decisão de usar FK direto (1:1) em vez de tabela N:N para `transaction_suppliers` merece justificativa detalhada:

- Na realidade financeira pessoal, uma despesa tem UM fornecedor: "paguei a conta de energia da Neoenergia"
- Não existe cenário prático onde uma única transação tem múltiplos fornecedores
- Se o usuário paga uma fatura que tem itens de fornecedores diferentes, cada item já é um `statement_item` com seu próprio `supplier_id`
- FK direto é mais simples, mais rápido em queries, e mais claro semanticamente

**João Pereira:**
Concordo. Se no futuro precisarmos de N:N, a migração é simples (criar tabela pivot, migrar FKs). Mas não vamos pagar essa complexidade agora.

### 5.4. Resumo das Novas Tabelas e Campos

**André Santos:**
Consolidando:

| Nova Tabela / Campo               | Tipo               | RLS                      | Índices recomendados                               |
| --------------------------------- | ------------------ | ------------------------ | -------------------------------------------------- |
| `suppliers`                       | Tabela nova        | ✅ user_id               | name, type, institution_id, is_active              |
| `supplier_aliases`                | Tabela nova        | ✅ user_id               | supplier_id, alias_name (trigram para busca fuzzy) |
| `supplier_contracts`              | Tabela nova        | ✅ user_id               | supplier_id, contract_type, identifier             |
| `supplier_tags`                   | Tabela N:N nova    | ✅ via suppliers.user_id | (supplier_id, tag_id) PK                           |
| `consumption_metrics`             | Tabela nova        | ✅ user_id               | supplier_id, reference_period_start, metric_name   |
| `transactions.supplier_id`        | FK novo (nullable) | ✅ já tem RLS            | supplier_id                                        |
| `recurring_templates.supplier_id` | FK novo (nullable) | ✅ já tem RLS            | supplier_id                                        |
| `statement_items.supplier_id`     | FK novo (nullable) | ✅ já tem RLS            | supplier_id                                        |
| `documents.supplier_id`           | FK novo (nullable) | ✅ já tem RLS            | supplier_id                                        |
| `liabilities.supplier_id`         | FK novo (nullable) | ✅ já tem RLS            | supplier_id                                        |

**Fernando Gomes (DevOps):**
São 5 tabelas novas e 5 novos FKs. A migração será uma única migration file no Supabase. RLS precisa ser configurado para todas as novas tabelas com o padrão:

```sql
CREATE POLICY "Users can only access their own suppliers"
  ON suppliers FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);
```

### 5.5. Decisão Final sobre Modelo de Dados

A equipe aprova unanimemente:

- 5 novas tabelas: `suppliers`, `supplier_aliases`, `supplier_contracts`, `supplier_tags`, `consumption_metrics`
- 5 novos FKs nullable em tabelas existentes
- FK direto (1:1) ao invés de N:N para fornecedor em transações/recorrências/documentos
- Tipo de fornecedor como enum + tags para classificação flexível
- Métricas de consumo como tabela dedicada com campos texto para nome/unidade (não enum)
- RLS mandatório em todas as novas tabelas

---

## 6. Modelagem de Aliases, Nomes Antigos e Consolidação Histórica

### 6.1. Estratégia de Aliases

**Pedro Santos (Backend):**
A tabela `supplier_aliases` resolve três cenários distintos:

#### Cenário 1: Mudança de nome societário

```
Fornecedor: "Neoenergia Pernambuco"
Alias: "CELPE" (type: former_name, valid_until: 2020-12-31)
```

- Lançamentos antigos com "CELPE" podem ser retroativamente vinculados ao fornecedor Neoenergia
- O histórico permanece íntegro: o alias registra o nome da época

#### Cenário 2: Nome como aparece na fatura do cartão

```
Fornecedor: "GitHub"
Alias: "GITHUB.COM" (type: billing_name)
Alias: "GITHUB INC" (type: legal_name)
```

- Importações de extrato de cartão podem usar aliases para matching automático

#### Cenário 3: Abreviações e variações

```
Fornecedor: "OpenAI"
Alias: "OPENAI *CHATGPT" (type: billing_name)
Alias: "ChatGPT" (type: trade_name)
Alias: "ChatGPT Plus" (type: trade_name)
```

### 6.2. Consolidação Histórica

**Maria Oliveira (Backend):**
Quando um fornecedor muda de nome, o processo é:

1. O fornecedor permanece como entidade única — `suppliers.id` não muda
2. O nome principal (`suppliers.name`) é atualizado para o novo nome
3. O nome antigo é registrado como alias com `alias_type = 'former_name'` e `valid_until` preenchido
4. **Nenhum lançamento precisa ser alterado** — todos já apontam para o mesmo `supplier_id`
5. O histórico é automaticamente consolidado porque o FK é estável

**André Santos (DBA):**
Isso é crucial do ponto de vista de banco de dados. Se mudássemos o ID ou criássemos novo fornecedor a cada mudança de nome, perderíamos o vínculo histórico. Com aliases, o ID é imutável e o nome é metadado — exatamente como deveria ser.

**Ricardo Monteiro (Economista):**
Na prática, isso significa que a pergunta "quanto gastei com a Neoenergia nos últimos 5 anos?" retorna resultados corretos mesmo que nos primeiros 2 anos a empresa se chamasse CELPE. Nenhum esforço manual para o usuário.

### 6.3. Busca e Matching por Alias

**Pedro Santos:**
Para busca, proponho um índice trigram no campo `alias_name`:

```sql
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE INDEX idx_supplier_aliases_name_trgm
  ON supplier_aliases USING gin (alias_name gin_trgm_ops);
```

Isso permite:

- Busca parcial: "NEOENER" encontra "Neoenergia Pernambuco"
- Busca fuzzy: "CELP" encontra "CELPE"
- Útil para importação e futura IA

**Laura Costa (Backend):**
Na importação de planilha (já prevista no MVP), antes de criar um lançamento, podemos buscar em `supplier_aliases` para sugerir o fornecedor provável. Se o extrato do cartão diz "GITHUB.COM R$ 21,00", o sistema sugere: "Fornecedor: GitHub?" — com um clique o usuário confirma.

### 6.4. Fluxo de Merge de Fornecedores

**Maria Oliveira:**
Cenário: o usuário criou dois fornecedores ("Vivo" e "Vivo Telefônica") que são o mesmo. Precisamos de um fluxo de merge:

1. Usuário seleciona fornecedor A (ex: "Vivo Telefônica") como duplicata
2. Seleciona fornecedor B (ex: "Vivo") como principal
3. Sistema:
   - Move todos os lançamentos de A para B (`UPDATE transactions SET supplier_id = B WHERE supplier_id = A`)
   - Move todos os contratos de A para B
   - Move todas as métricas de A para B
   - Cria alias em B com o nome de A (`alias_type = 'former_name'`)
   - Desativa A (`is_active = false`) ou deleta, conforme preferência
4. Histórico totalmente preservado sob o fornecedor B

**André Santos:**
Esse merge deve ser uma Edge Function server-side com transação atômica. Não pode falhar no meio — ou move tudo ou nada.

### 6.5. Decisão Final sobre Aliases

Aprovado:

- `supplier_aliases` com tipos: former_name, abbreviation, trade_name, billing_name, other
- Datas opcionais de validade (valid_from, valid_until)
- Índice trigram para busca fuzzy
- Matching de alias na importação (sugestão automática)
- Fluxo de merge de fornecedores duplicados (Edge Function transacional)
- ID do fornecedor é imutável — nome e aliases são metadados

---

## 7. Impacto em Recorrências

### 7.1. Revisão do Template Recorrente

**Maria Oliveira (Backend):**
Com a adição de `supplier_id` em `recurring_templates`, cada recorrência pode (e deve) informar seu fornecedor. Revisando o fluxo:

- O template recorrente agora tem: nome, tipo, valor, frequência, conta, **fornecedor**, categoria, tags, prioridade
- Exemplo: "Internet Vivo — R$ 149,90 — mensal — dia 15 — Débito automático na Caixa — **Fornecedor: Vivo** — Categoria: Internet/Telefonia — Tags: internet, casa, essencial — Prioridade: alta"

**Regras de herança revisadas:**

1. Quando uma instância recorrente é gerada, herda: categoria, tags, prioridade E **fornecedor** do template
2. Quando a instância é confirmada (paga) e gera uma `transaction`, o `supplier_id` é passado para a transação
3. O usuário pode sobrescrever o fornecedor na instância ou na transação (ex.: trocou de operadora)

### 7.2. Cenários Práticos

**Camila Duarte (Consultora):**
Vou dar exemplos reais para validar a modelagem:

| Template recorrente     | Fornecedor            | Categoria          | Tags                           | Contrato associado           |
| ----------------------- | --------------------- | ------------------ | ------------------------------ | ---------------------------- |
| Internet Vivo (casa)    | Vivo                  | Internet/Telefonia | internet, casa, essencial      | Linha 81999XX, Plano 300Mbps |
| Internet Canaã (backup) | Canaã Telecom         | Internet/Telefonia | internet, casa, contingencia   | Contrato #4521               |
| Energia (casa)          | Neoenergia            | Utilidades         | energia, casa, essencial       | UC 0123456, Instalação #789  |
| GitHub Pro              | GitHub                | Software/Serviços  | trabalho, desenvolvimento      | Account: chicofigueiredo     |
| ChatGPT Plus            | OpenAI                | Software/Serviços  | trabalho, pesquisa, ensino     | Subscription ID              |
| Diarista                | Maria da Silva        | Serviços Pessoais  | casa, pessoa_fisica, essencial | —                            |
| Condomínio              | Adm. Cond. Edifício X | Moradia            | moradia, essencial             | Apt 302, Bloco B             |

**Ricardo Monteiro:**
Note que "Internet Vivo" e "Internet Canaã" têm a MESMA categoria ("Internet/Telefonia") e a MESMA tag ("internet") — mas fornecedores DIFERENTES. Isso demonstra que fornecedor é dimensão ortogonal. Sem ele, o relatório diria "gastei R$ 270/mês em internet" mas não diria "R$ 150 com Vivo e R$ 120 com Canaã".

### 7.3. Conciliação de Recorrência com Documento

**Pedro Santos (Backend):**
Quando um documento (fatura, conta) chega, a conciliação pode usar o fornecedor como critério de matching:

1. Documento chega com `supplier_id = Neoenergia`
2. Sistema busca recorrências pendentes com `supplier_id = Neoenergia` e data próxima
3. Sugere: "Este documento parece ser a conta de energia de Março/2026 (recorrência: Energia Casa)"
4. Usuário confirma → instância marcada como paga, documento vinculado

**Laura Costa:**
No MVP isso pode ser matching manual (o usuário liga o documento à instância). Na Fase 4 (IA), o matching pode ser automático usando fornecedor + nome + valor + período.

### 7.4. Decisão Final sobre Recorrências

Aprovado:

- `supplier_id` FK nullable em `recurring_templates`
- Herança de fornecedor para instâncias e transações derivadas
- Fornecedor como critério de matching na conciliação com documentos
- Override de fornecedor permitido em instância/transação individual

---

## 8. Impacto em Documentos e Conciliação

### 8.1. Documento com Fornecedor

**Pedro Santos (Backend):**
Com o novo FK `supplier_id` em `documents`, o fluxo de upload revisado é:

1. Usuário faz upload de documento (PDF, imagem)
2. Define: tipo de documento, entidade vinculada (transação, dívida, fatura)
3. **Novo:** Opcionalmente associa a um fornecedor
4. Exemplo: PDF da conta de energia → `document_type: invoice`, `entity_type: transaction`, `supplier_id: Neoenergia`

### 8.2. Cenários de Vinculação Documento-Fornecedor

**João Pereira (Backend):**

| Tipo de documento        | Fornecedor associado           | Benefício                               |
| ------------------------ | ------------------------------ | --------------------------------------- |
| Conta de energia (PDF)   | Neoenergia                     | Agrupa todas as contas desse fornecedor |
| Fatura do cartão (PDF)   | Instituição do cartão (Nubank) | Vincula à fatura                        |
| Nota fiscal de compra    | Loja X                         | Comprova gasto                          |
| Comprovante de pagamento | Fornecedor do serviço pago     | Evidência de quitação                   |
| Contrato de internet     | Vivo                           | Referência do plano contratado          |
| Boleto pago              | Fornecedor do boleto           | Histórico de pagamentos                 |

### 8.3. Preparação para Leitura por IA Futura

**Maria Oliveira (Backend):**
O prompt da Verônica pede que preparemos a modelagem para que a IA futura possa usar dados do fornecedor na leitura de documentos. Com a modelagem proposta, a IA terá acesso a:

| Dado disponível       | Fonte                                                   | Uso pela IA                         |
| --------------------- | ------------------------------------------------------- | ----------------------------------- |
| Nome do fornecedor    | `suppliers.name`                                        | Matching com texto do documento     |
| Aliases               | `supplier_aliases.alias_name`                           | Resolução de nomes variantes no PDF |
| CNPJ                  | `suppliers.document_number`                             | Validação de emissor                |
| Conta contrato        | `supplier_contracts.identifier`                         | Matching de unidade consumidora     |
| Unidade consumidora   | `supplier_contracts.identifier` (type: utility_account) | Identificação automática            |
| Número do cliente     | `supplier_contracts.identifier` (type: customer_id)     | Cross-reference                     |
| Instalação            | `supplier_contracts.identifier` (type: installation)    | Validação de endereço               |
| Plano/Linha           | `supplier_contracts.metadata`                           | Contexto do serviço                 |
| Histórico de métricas | `consumption_metrics`                                   | Detecção de anomalias               |

**Pedro Santos:**
Na prática, a IA futura fará:

1. Receber PDF → extrair texto
2. Buscar CNPJ/nome no texto → matching com `suppliers` + `supplier_aliases`
3. Buscar identificadores (UC, instalação) → matching com `supplier_contracts`
4. Extrair valores, datas, métricas → criar/atualizar `consumption_metrics`
5. Vincular documento à transação/recorrência correta

Tudo isso depende de a modelagem dos dados do fornecedor estar correta AGORA — é por isso que fazemos na Fase 1, mesmo que a IA só venha na Fase 4.

### 8.4. Impacto na Conciliação

**Ricardo Monteiro (Economista):**
Conciliação é o processo de verificar se os lançamentos do sistema batem com os extratos reais. Fornecedor melhora a conciliação em vários aspectos:

1. **Import matching**: Ao importar extrato, a descrição "CELPE" pode ser automaticamente resolvida para "Neoenergia" via alias
2. **Deduplicação**: Lançamento manual "Conta de energia R$ 250 — Neoenergia" + importação do extrato "NEOENERGIA PE R$ 250" → sistema sugere que são o mesmo lançamento
3. **Consistência temporal**: Recorrência "Energia Neoenergia R$ ~250/mês" + documento PDF mostrando R$ 280 → sistema alerta "valor acima do padrão"
4. **Verificação cruzada**: Fatura do cartão com item "GITHUB.COM R$ 21" → alias resolve para GitHub → vincula ao template recorrente "GitHub Pro"

**Laura Costa:**
No MVP, a conciliação manual já se beneficia do fornecedor: o usuário pode filtrar "mostre todos os lançamentos da Neoenergia" e cruzar com os PDFs de conta de energia. Mais tarde, a conciliação automática usa as mesmas entidades.

### 8.5. Decisão Final sobre Documentos e Conciliação

Aprovado:

- `supplier_id` FK nullable em `documents`
- Documentos podem ser vinculados simultaneamente a entidade (transação, dívida) E fornecedor
- Modelagem prepara todos os dados necessários para leitura por IA futura
- Conciliação manual beneficiada imediatamente; automática na Fase 4
- Aliases como base para matching de importação

---

## 9. Revisão de Categorias, Tags e Prioridade com Fornecedor

### 9.1. Coexistência de Dimensões

**Maria Oliveira (Backend):**
Vou tornar explícita a coexistência de TODAS as dimensões em um lançamento. Revisando a anatomia completa de uma transação no modelo atualizado:

```
Transação: "Conta de Energia — Março/2026"
├── Valor: R$ 280,00
├── Tipo: expense
├── Fornecedor: Neoenergia (FK → suppliers)
├── Categoria: Utilidades / Energia (FK → categories)
├── Tags: [energia, casa, essencial] (N:N → transaction_tags)
├── Prioridade: essential (enum, derivado ou manual)
├── Conta: Caixa - Conta Corrente (FK → financial_products)
├── Período: 20/Mar - 19/Abr (FK → financial_periods)
├── Contrato: UC 0123456 (via supplier_contracts)
├── Métricas: 320 kWh, R$0.87/kWh, Bandeira Amarela (via consumption_metrics)
├── Documento: conta_energia_mar2026.pdf (FK → documents)
└── Recorrência: Energia Neoenergia (FK → recurring_instances → recurring_templates)
```

**Camila Duarte:**
Isso é poderoso. Com uma única transação, o usuário pode responder:

- "Que tipo de gasto?" → Categoria: Utilidades
- "É essencial?" → Prioridade: essential
- "Para quem?" → Fornecedor: Neoenergia
- "De qual conta saiu?" → Conta: Caixa CC
- "Em qual ciclo?" → Período: 20/Mar - 19/Abr
- "Quanto consumiu?" → Métrica: 320 kWh
- "Tem comprovante?" → Documento: PDF vinculado

### 9.2. Hierarquia — O que NÃO sobreescreve o quê

**Ana Silva (Arquiteta):**
Regra fundamental: nenhuma dimensão invalida ou substitui outra.

| Regra                             | Descrição                                                                                       |
| --------------------------------- | ----------------------------------------------------------------------------------------------- |
| Fornecedor NÃO define categoria   | Vivo pode ser "Internet/Telefonia" ou "Comunicação" — depende da categoria, não do fornecedor   |
| Fornecedor NÃO define prioridade  | Vivo pode ser essencial (internet principal) ou baixa (linha de backup) — depende do lançamento |
| Categoria NÃO define fornecedor   | "Internet/Telefonia" pode ser Vivo, Canaã, Claro — depende do fornecedor associado              |
| Tag NÃO é alias de fornecedor     | Tag "internet" agrupa transações de qualquer fornecedor de internet                             |
| Fornecedor pode ter tags próprias | Vivo pode ter tag "essencial" — mas é a tag DO FORNECEDOR, não da transação                     |

### 9.3. Tags de Fornecedor vs Tags de Transação

**João Pereira (Backend):**
Temos agora dois níveis de tag:

1. **Tags da transação** (`transaction_tags`): Classificam o lançamento individual
2. **Tags do fornecedor** (`supplier_tags`): Classificam o fornecedor como entidade

**Exemplo:**

```
Fornecedor: Vivo
  Tags do fornecedor: [telecom, essencial, conta_mensal]

Transação: Vivo — Fatura Março
  Tags da transação: [internet, casa, essencial]
```

**Maria Oliveira:**
A distinção é clara:

- Tag do fornecedor = "que tipo de fornecedor é?" → usado para filtrar/agrupar fornecedores
- Tag da transação = "que tipo de gasto é?" → usado para relatórios e priorização

No MVP, deixamos a tag da transação como principal (já existe). Tags de fornecedor ficam como funcionalidade complementar, preenchida sob demanda.

### 9.4. Revisão dos Filtros

**Roberto Lima (Frontend):**
Com a adição de fornecedor, os filtros na interface precisam ser atualizados:

| Filtro                                | Antes          | Agora       |
| ------------------------------------- | -------------- | ----------- |
| Por categoria                         | ✅ Existia     | ✅ Mantido  |
| Por tag                               | ✅ Existia     | ✅ Mantido  |
| Por prioridade                        | ✅ Existia     | ✅ Mantido  |
| Por período                           | ✅ Existia     | ✅ Mantido  |
| Por conta/produto                     | ✅ Existia     | ✅ Mantido  |
| **Por fornecedor**                    | ❌ Não existia | ✅ **NOVO** |
| **Cruzamento categoria × fornecedor** | ❌ Não existia | ✅ **NOVO** |
| **Cruzamento tag × fornecedor**       | ❌ Não existia | ✅ **NOVO** |
| **Cruzamento fornecedor × período**   | ❌ Não existia | ✅ **NOVO** |

**Sofia Almeida (Frontend):**
No componente de filtros, proponho um chip/badge para cada dimensão ativa. Exemplo: `[Fornecedor: Vivo] [Categoria: Internet] [Tag: essencial] [Período: Mar/2026]`. O usuário pode combinar livremente.

**Helena Vargas (UX):**
Concordo com chips. E no mobile futuro, filtros aparecem como bottom sheet com seções expansíveis. A combinação livre é essencial — "me mostre tudo que paguei para a Vivo nos últimos 6 meses com tag essencial".

### 9.5. Decisão Final sobre Categorias, Tags e Prioridade

Aprovado:

- Fornecedor coexiste com categoria, tags e prioridade sem sobreposição
- Tags de fornecedor (`supplier_tags`) como funcionalidade complementar
- Novos filtros por fornecedor e cruzamentos no frontend
- Interface com chips/badges para filtros combinados
- Nenhuma dimensão invalida ou substitui outra

---

## 10. Impacto em Relatórios, Filtros e Auditoria

### 10.1. Novos Relatórios

**Ricardo Monteiro (Economista):**
Fornecedor habilita relatórios que antes eram impossíveis:

#### Relatório: Gastos por Fornecedor

```
Período: 20/Mar - 19/Abr/2026

| Fornecedor       | Total      | % do Total | Variação vs período anterior |
|-------------------|-----------|------------|------------------------------|
| Neoenergia        | R$ 280,00 | 9,8%       | ▲ +R$ 30 (+12%)             |
| Vivo              | R$ 149,90 | 5,3%       | → igual                     |
| Canaã Telecom     | R$ 119,90 | 4,2%       | → igual                     |
| GitHub            | R$ 21,00  | 0,7%       | → igual                     |
| Supermercado X    | R$ 850,00 | 29,8%      | ▼ -R$ 100 (-10,5%)          |
| ...               | ...       | ...        | ...                         |
| TOTAL             | R$2.850,00| 100%       |                              |
```

#### Relatório: Evolução por Fornecedor (série temporal)

```
Fornecedor: Neoenergia — últimos 6 meses

| Período  | Valor    | kWh  | R$/kWh | Bandeira |
|----------|---------|------|--------|----------|
| Out/2025 | R$210   | 240  | 0.87   | Verde    |
| Nov/2025 | R$230   | 265  | 0.87   | Verde    |
| Dez/2025 | R$290   | 310  | 0.93   | Amarela  |
| Jan/2026 | R$340   | 380  | 0.89   | Amarela  |
| Fev/2026 | R$250   | 280  | 0.89   | Verde    |
| Mar/2026 | R$280   | 320  | 0.87   | Amarela  |
```

**Camila Duarte:**
Esse segundo relatório é ouro puro. O usuário olha e pensa: "em janeiro meu consumo foi 380kWh — provavelmente o ar-condicionado ligado todo dia". Decisão informada em vez de achismo.

#### Relatório: Fornecedores com variação suspeita

```
⚠️ Alertas de variação:
- Neoenergia: R$ 280 este mês vs média R$ 250 (6 meses) → +12%
- Supermercado X: R$ 850 este mês vs média R$ 720 → +18%
- ChatGPT: R$ 120 este mês vs R$ 21 mês passado → +471% (mudou de plano?)
```

**Ricardo Monteiro:**
Esse tipo de alerta é extremamente valioso para consultoria financeira. No MVP, pode ser calculado server-side (Edge Function) ou com query SQL e exibido como seção do dashboard.

### 10.2. Perguntas de Auditoria Agora Respondíveis

**André Santos (DBA):**
Com a modelagem de fornecedor, aliases e métricas, o sistema agora responde:

| Pergunta                                                                    | Como responde                                                                                          | Tabelas envolvidas                                  |
| --------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------ | --------------------------------------------------- | ---------- | --- | ----------------------------------------------------- | ------------------------------ |
| Quanto gastei com a Neoenergia nos últimos 12 meses?                        | `SUM(amount) WHERE supplier_id = X AND event_date > now()-12 months`                                   | transactions, suppliers                             |
| Qual foi o consumo de kWh mês a mês?                                        | `SELECT metric_name, quantity, reference_period_start WHERE supplier_id = X AND metric_name = 'kWh'`   | consumption_metrics                                 |
| Quanto paguei de internet somando Vivo e Canaã?                             | `SUM(amount) WHERE supplier_id IN (vivo_id, canaa_id)`                                                 | transactions, suppliers                             |
| Quais cobranças do GitHub foram pagas no cartão?                            | `SELECT * FROM transactions t JOIN statement_items si ON ... WHERE t.supplier_id = github_id`          | transactions, statement_items, suppliers            |
| Quais cobranças recorrentes do mesmo fornecedor mudaram de valor?           | `SELECT * FROM recurring_instances WHERE supplier_id = X AND actual_amount != expected_amount`         | recurring_instances, recurring_templates, suppliers |
| Quais despesas de um fornecedor ficaram sem documento?                      | `SELECT t.* FROM transactions t LEFT JOIN documents d ON ... WHERE t.supplier_id = X AND d.id IS NULL` | transactions, documents, suppliers                  |
| Quais lançamentos antigos parecem do mesmo fornecedor com nomes diferentes? | `SELECT \* FROM transactions t WHERE t.description ILIKE ANY(SELECT '%'                                |                                                     | alias_name |     | '%' FROM supplier_aliases) AND t.supplier_id IS NULL` | transactions, supplier_aliases |

**Maria Oliveira:**
A última query é particularmente interessante — pode ser executada como rotina periódica para sugerir associações: "Encontramos 15 lançamentos com 'CELPE' na descrição que parecem ser da Neoenergia. Deseja associar?"

### 10.3. Impacto em Views e Agregações SQL

**André Santos:**
Proponho adicionar views materializadas para os relatórios mais comuns:

```sql
-- View: Gastos por fornecedor por período
CREATE MATERIALIZED VIEW mv_supplier_spending AS
SELECT
  t.user_id,
  t.supplier_id,
  s.name AS supplier_name,
  s.type AS supplier_type,
  fp.id AS financial_period_id,
  fp.label AS period_label,
  fp.start_date,
  fp.end_date,
  SUM(t.amount) AS total_amount,
  COUNT(*) AS transaction_count,
  AVG(t.amount) AS avg_amount
FROM transactions t
JOIN suppliers s ON t.supplier_id = s.id
LEFT JOIN financial_periods fp ON t.financial_period_id = fp.id
WHERE t.type IN ('expense', 'fee', 'interest_charge')
  AND t.supplier_id IS NOT NULL
GROUP BY t.user_id, t.supplier_id, s.name, s.type,
         fp.id, fp.label, fp.start_date, fp.end_date;

-- Refresh periódico via cron do Supabase
```

**Fernando Gomes (DevOps):**
O refresh da view materializada pode rodar via cron job do Supabase — uma vez por hora ou sob demanda quando o usuário acessa relatórios.

### 10.4. Decisão Final sobre Relatórios e Auditoria

Aprovado:

- 3 novos tipos de relatório: gastos por fornecedor, evolução temporal, variação suspeita
- Todas as 7 perguntas de auditoria da Verônica são respondíveis com o modelo proposto
- View materializada `mv_supplier_spending` para performance
- Sugestão de associação retroativa de lançamentos sem fornecedor via alias matching
- Alerta de variação de valor como funcionalidade do dashboard

---

## 11. Revisão da Primeira Tela com Fornecedor

### 11.1. Impacto na Tela de Ação

**Helena Vargas (UX):**
No refino anterior, definimos que a primeira tela é uma **tela de comando operacional**, não um dashboard passivo. Fornecedor agora é uma dimensão que entra nessa visão — mas sem sobrecarregar.

**Isabella Torres (UI Designer):**
Proponho adicionar o fornecedor de forma contextual, não como seção nova. O fornecedor aparece DENTRO dos itens que já existem:

```
┌─────────────────────────────────────────────────────────────┐
│ 🏠 Seu Bolso Feliz          Ciclo: 20/Mar - 19/Abr   ⚙️    │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐      │
│  │ Saldo Total  │  │ A Pagar      │  │ Dias Restam  │      │
│  │ R$ 4.230,00  │  │ R$ 2.850,00  │  │     18       │      │
│  │  ▲ +R$ 200   │  │ 7 itens      │  │ até 19/Abr   │      │
│  └──────────────┘  └──────────────┘  └──────────────┘      │
│                                                              │
│  ⚠️ ATENÇÃO ───────────────────────────────────────────      │
│  │ 🔴 Condomínio (Adm. Cond. Ed. X) — ATRASADO — R$ 850   │
│  │ 🟡 Fatura Nubank — vence em 3 dias — R$ 1.200           │
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  📋 PRÓXIMOS PAGAMENTOS (por prioridade) ──────────────     │
│  │ 🔴 [essencial] Internet — Vivo — 28/Mar — R$120 [Pagar] │
│  │ 🔴 [essencial] Energia — Neoenergia — 25/Mar — R$280    │
│  │ 🟠 [alta]      Diarista — Maria S. — 30/Mar — R$200     │
│  │ 🟡 [média]     Academia — SmartFit — 01/Abr — R$90      │
│  │ 🔵 [baixa]     Spotify — Spotify — 05/Abr — R$34        │
│  │ ⚪ [opcional]   Curso XY — Udemy — 10/Abr — R$200 [Adiar]│
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  📊 TOP FORNECEDORES DO PERÍODO ────────────────────────  🔽│
│  │ Neoenergia     R$ 280  ████████████░░░ 9,8%  ▲+12%      │
│  │ Vivo           R$ 270  ██████████░░░░░ 9,5%  →           │
│  │ SmartFit       R$  90  ███░░░░░░░░░░░░ 3,2%  →           │
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  💳 FATURAS ─────────────────────────────────────────────    │
│  │ Nubank (****1234) — Fatura Mar — R$ 1.200 — vence 23/Mar│
│  │   Top: Supermercado X (R$450), iFood (R$180), Amazon...  │
│  │ C6 (****5678)     — Fatura Mar — R$ 450   — vence 28/Mar│
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  📊 DÍVIDAS ──────────────────────────────────────────────   │
│  │ Financiamento Apt (Caixa) — Parcela 24/120 — R$1.800    │
│  │ Empréstimo Pessoal (Nubank) — Parcela 8/24 — R$ 650     │
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  🧮 SOBREVIVÊNCIA ─────────────────────────────────────      │
│  │ Saldo disponível: R$ 4.230                               │
│  │ Obrigações restantes no período: R$ 2.850                │
│  │ Margem livre: R$ 1.380                                   │
│  │ ✅ Você cobre todas as obrigações essenciais deste ciclo │
│  └──────────────────────────────────────────────────────     │
└─────────────────────────────────────────────────────────────┘
```

### 11.2. Onde Entra o Fornecedor na Home

**Camila Duarte (Consultora):**
O fornecedor aparecer de 3 formas na home:

| Local                   | Como aparece                         | Exemplo                                         |
| ----------------------- | ------------------------------------ | ----------------------------------------------- |
| **Próximos pagamentos** | Nome do fornecedor ao lado do item   | "Internet — **Vivo** — 28/Mar — R$ 120"         |
| **TOP fornecedores**    | Mini-ranking com valor, % e variação | "Neoenergia R$ 280 (9,8%) ▲+12%"                |
| **Faturas expandidas**  | Fornecedores dentro da fatura        | "Top: Supermercado X (R$450), iFood (R$180)..." |

**Roberto Lima (Frontend):**
A seção "TOP Fornecedores" é colapsável (começa fechada em mobile). Não sobrecarrega, mas está disponível para quem quer análise rápida.

### 11.3. Alertas Relacionados a Fornecedor

**Ricardo Monteiro (Economista):**
Proponho 3 tipos de alerta de fornecedor na home:

1. **Variação de valor**: "Neoenergia subiu 12% este mês (R$ 280 vs média R$ 250)"
2. **Cobrança inesperada**: "Nova cobrança de ChatGPT: R$ 120 (era R$ 21). Mudou de plano?"
3. **Fornecedor pendente**: "3 cobranças da Vivo sem fornecedor associado — associar?"

**Helena Vargas:**
Esses alertas aparecem na seção "Atenção" junto com vencimentos atrasados. É a mesma lógica: "precisa da sua ação agora". O sistema ordena por criticidade.

### 11.4. Decisão Final sobre Primeira Tela

Aprovado:

- Fornecedor aparece no nome dos itens de pagamento (contextual)
- Nova seção "TOP Fornecedores do Período" colapsável
- Fornecedores dentro das faturas expandidas
- 3 tipos de alerta relacionado a fornecedor na seção "Atenção"
- A tela continua orientada à ação, não vira dashboard analítico

---

## 12. Revisão da Estratégia de Testes — Cenários de Fornecedor

### 12.1. Novos Testes Mandatórios

**Maria Oliveira (Backend):**
Seguindo o padrão do refino anterior (T1-T17), acrescentamos 10 novos cenários mandatórios (T18-T27) que cobrem toda a dimensão fornecedor. Cada teste é um contrato. Mesma regra: não podem ser alterados por conveniência.

---

#### T18. Lançamento pode ser corretamente associado a fornecedor

**Cenário de aceitação:**

```
DADO que existe o fornecedor "Neoenergia" (type: utility, document_number: 10.338.320/0001-00)
E existe uma transação "Conta de Energia — Mar/2026" com valor R$ 280
QUANDO o usuário associa a transação ao fornecedor "Neoenergia"
ENTÃO a transação tem supplier_id = id da Neoenergia
E o relatório "Gastos por Fornecedor" inclui R$ 280 para Neoenergia
E a transação aparece ao filtrar por fornecedor "Neoenergia"
```

**Caso-limite:**

- Transação sem fornecedor (supplier_id = NULL) → aceita, não obrigatório
- Transação com fornecedor + categoria + tags + prioridade → todos coexistem

---

#### T19. Aliases do mesmo fornecedor consolidam histórico

**Cenário de aceitação:**

```
DADO que existe o fornecedor "Neoenergia" com alias "CELPE" (type: legacy_name, valid_until: 2023-12-31)
E existem 5 transações antigas com supplier_id = NULL e description contendo "CELPE"
E existem 3 transações novas com supplier_id = Neoenergia
QUANDO o sistema executa sugestão de associação retroativa via alias
ENTÃO as 5 transações antigas são sugeridas para associação com Neoenergia
E se o usuário confirma, as 5 transações passam a ter supplier_id = Neoenergia
E o relatório de Neoenergia mostra 8 transações totais
E a busca por "CELPE" continua encontrando as transações via alias
```

**Caso-limite:**

- Alias com período de validade → transações fora do período não são sugeridas
- Dois fornecedores com aliases semelhantes → sistema exibe ambas as opções

---

#### T20. Mudança de nome do fornecedor NÃO quebra histórico

**Cenário de aceitação:**

```
DADO que o fornecedor "CELPE" foi renomeado para "Neoenergia" (alias criado)
E existem transações vinculadas a este fornecedor de 2020 a 2026
QUANDO o nome do fornecedor muda de "CELPE" para "Neoenergia"
ENTÃO todas as transações históricas continuam vinculadas ao MESMO supplier_id
E o relatório mostra "Neoenergia" como nome atual em todas as transações
E a busca por "CELPE" encontra o fornecedor via alias
E nenhuma transação perde o vínculo
```

---

#### T21. Recorrência com fornecedor gera instâncias com vínculo coerente

**Cenário de aceitação:**

```
DADO que existe um template recorrente "Internet Vivo" com supplier_id = Vivo
E frequência = mensal, dia = 15, valor = R$ 149,90
QUANDO o sistema gera a instância de Abril/2026
ENTÃO a instância herda supplier_id = Vivo
E quando confirmada (paga), a transação gerada tem supplier_id = Vivo
E o relatório por fornecedor inclui R$ 149,90 para Vivo no período de Abril
```

**Caso-limite:**

- Usuário troca de fornecedor no meio (Vivo → Claro) → atualiza template, instâncias futuras mudam, históricas permanecem
- Instância gerada mas não paga → não aparece em relatório de fornecedor (é previsão, não gasto)

---

#### T22. Relatórios por fornecedor somam corretamente despesas de diferentes meios

**Cenário de aceitação:**

```
DADO que o fornecedor "Vivo" está associado a:
  - R$ 149,90 débito automático na Caixa (transação direta)
  - R$ 89,90 compra no cartão Nubank (item de fatura)
  - R$ 50,00 boleto avulso (transação manual)
QUANDO o relatório "Gastos com Vivo" é gerado
ENTÃO o total exibido é R$ 289,80
E a quebra por meio de pagamento mostra: Caixa (R$149,90), Nubank (R$89,90), Boleto (R$50,00)
E nenhuma duplicação ocorre (mesmo que a transação do cartão apareça na fatura E como transação)
```

---

#### T23. Lançamentos em cartão atribuídos ao fornecedor correto

**Cenário de aceitação:**

```
DADO que na fatura do Nubank de Março existem os itens:
  - "GITHUB.COM" — R$ 21,00
  - "OPENAI *CHATGPT" — R$ 120,00
  - "SUPERM BOM PRECO" — R$ 350,00
E existem aliases: "GITHUB.COM" → GitHub, "OPENAI *CHATGPT" → OpenAI
QUANDO os itens de fatura são processados
ENTÃO "GITHUB.COM" é sugerido como fornecedor GitHub
E "OPENAI *CHATGPT" é sugerido como fornecedor OpenAI
E "SUPERM BOM PRECO" fica sem fornecedor (pode ser associado manualmente)
E cada item mantém a descrição original da fatura + supplier_id resolvido
```

---

#### T24. Categoria, tags, fornecedor e prioridade coexistem sem conflito

**Cenário de aceitação:**

```
DADO uma transação com:
  - supplier_id = Vivo
  - category_id = Internet/Telefonia
  - tags = [internet, casa, essencial]
  - priority = essential
QUANDO a transação é salva e consultada
ENTÃO todos os 4 campos estão presentes e corretos
E filtrar por fornecedor "Vivo" inclui esta transação
E filtrar por categoria "Internet/Telefonia" inclui esta transação
E filtrar por tag "internet" inclui esta transação
E filtrar por prioridade "essential" inclui esta transação
E filtrar por fornecedor "Vivo" + tag "essencial" inclui esta transação
E nenhum campo sobrescreve ou invalida outro
```

---

#### T25. Métricas de consumo armazenadas e consultadas corretamente

**Cenário de aceitação:**

```
DADO que existe o fornecedor "Neoenergia" e uma transação "Energia Mar/2026"
E o usuário registra métricas: kWh = 320, R$/kWh = 0.87, bandeira = "Amarela"
QUANDO as métricas são salvas em consumption_metrics
ENTÃO a consulta de métricas por fornecedor retorna:
  - metric_name: "kWh", quantity: 320, unit_price: 0.87
  - metric_name: "bandeira", metadata: {"valor": "Amarela"}
E o relatório de evolução mostra a série temporal de kWh mês a mês
E o subtotal calculado (320 × 0.87 = 278,40) é consistente com o valor da transação
```

**Caso-limite:**

- Transação sem métricas → aceita normalmente, métricas são opcionais
- Métricas com unidades diferentes no mesmo fornecedor → aceitas (ex: kWh para energia, m³ para água em fornecedores de utilidade)

---

#### T26. Conciliação histórica suporta nomes alternativos do mesmo fornecedor

**Cenário de aceitação:**

```
DADO que:
  - O fornecedor "Neoenergia" tem alias "CELPE" e "CELPE DISTRIBUIÇÃO"
  - Há 10 transações importadas com descriptions: "CELPE", "Celpe Distrib", "CELPE DISTRIBUIÇÃO", "Neoenergia"
  - Nenhuma dessas transações tem supplier_id definido
QUANDO o sistema executa a rotina de sugestão de associação
ENTÃO sugere associar todas as 10 transações ao fornecedor "Neoenergia"
E apresenta o grau de confiança do matching (exato vs fuzzy)
E o usuário pode confirmar em lote (batch) ou individualmente
E transações confirmadas passam a ter supplier_id = Neoenergia
E o audit_log registra a ação de associação retroativa
```

---

#### T27. Filtros por fornecedor funcionam com período, categoria, tags e prioridade

**Cenário de aceitação:**

```
DADO que existem transações de 3 fornecedores (Vivo, Neoenergia, GitHub)
  em 2 períodos financeiros (Feb/Mar e Mar/Abr)
  com categorias e tags variadas
  com prioridades diferentes
QUANDO o usuário aplica filtro: fornecedor = Vivo, período = Mar/Abr, tag = essencial
ENTÃO apenas transações que satisfazem TODOS os critérios são retornadas
E a contagem e soma refletem apenas o subset filtrado
E a remoção de um filtro expande o resultado corretamente
E a combinação de TODOS os filtros simultâneos funciona sem erro de integridade
```

---

### 12.2. Resumo dos Testes Mandatórios Atualizados

**Maria Oliveira:**
O sistema agora tem **27 testes mandatórios**:

| Faixa       | Área                          | Descrição                                          |
| ----------- | ----------------------------- | -------------------------------------------------- |
| T1-T2       | Transferências                | Fatura e transferência interna não geram despesa   |
| T3-T4       | Período financeiro            | Atribuição e geração de ciclos                     |
| T5-T6       | Cartões e faturas             | Ciclo de fechamento/vencimento, decomposição       |
| T7-T8       | Dívidas                       | Amortização, juros, quitação antecipada            |
| T9-T10      | Recorrências e importação     | Geração de instâncias, deduplicação                |
| T11-T12     | Classificação                 | Estornos, documentos vs saldo                      |
| T13-T14     | Categorias e tags             | Multi-tag sem perda de integridade, filtro correto |
| T15-T16     | Prioridade                    | Ordenação e alertas, essenciais ≠ postergáveis     |
| T17         | Primeira tela                 | Período, prioridade, sobrevivência correta         |
| **T18-T19** | **Fornecedor — Associação**   | **Vínculo a transação, consolidação por alias**    |
| **T20**     | **Fornecedor — Histórico**    | **Renomeação sem perda de vínculo**                |
| **T21**     | **Fornecedor — Recorrência**  | **Herança em instância e transação**               |
| **T22-T23** | **Fornecedor — Relatórios**   | **Soma multi-meio, resolução em fatura**           |
| **T24**     | **Fornecedor — Coexistência** | **Sem conflito com categoria/tag/prioridade**      |
| **T25**     | **Fornecedor — Métricas**     | **Armazenamento e consulta de consumo**            |
| **T26**     | **Fornecedor — Conciliação**  | **Associação retroativa com aliases**              |
| **T27**     | **Fornecedor — Filtros**      | **Combinação com todas as dimensões**              |

### 12.3. Decisão Final sobre Testes

Aprovado:

- 10 novos cenários mandatórios (T18-T27) com padrão DADO/QUANDO/ENTÃO
- Total de 27 testes obrigatórios antes de deploy
- Mesma regra do refino anterior: testes são contrato, não alteráveis por conveniência
- Cobertura mínima: 100% dos 27 cenários antes de cualquer deploy

---

## 13. Revisão da IA Futura / MCP com Fornecedor

### 13.1. Fornecedor como Dimensão de Inteligência

**Pedro Santos (Backend):**
A camada de IA/MCP planejada para as Fases 4 e 5 ganha significativamente com fornecedor. Vou mapear exatamente como:

### 13.2. Classificação Automática

**Maria Oliveira (Backend):**

| Cenário                                    | Sem fornecedor                | Com fornecedor                                                                                                                |
| ------------------------------------------ | ----------------------------- | ----------------------------------------------------------------------------------------------------------------------------- |
| Importação de extrato com "PIX NEOENERGIA" | IA tenta adivinhar categoria  | IA resolve: alias "NEOENERGIA" → fornecedor → tipo utility → sugestão: categoria "Utilidades/Energia", prioridade "essencial" |
| Fatura do cartão com "GITHUB.COM"          | IA tenta adivinhar            | IA resolve: alias "GITHUB.COM" → fornecedor → tipo saas → sugestão: categoria "Software/Serviços", tags [trabalho, dev]       |
| Novo lançamento manual "Internet"          | Ambíguo — Vivo? Canaã? Claro? | Se houver recorrência com fornecedor → sugere automaticamente                                                                 |

**Ricardo Monteiro:**
A taxonomia de fornecedor (tipo + tags + aliases) funciona como um **vocabulário de domínio** para a IA. Quanto mais classificado o fornecedor, mais precisa a IA.

### 13.3. Conciliação de Documentos

**Laura Costa (Backend):**
No fluxo de leitura de PDF:

1. IA extrai texto do PDF (OCR se necessário)
2. Busca CNPJ no texto → match com `suppliers.document_number`
3. Busca identificadores → match com `supplier_contracts.identifier`
4. Busca nome/alias → match com `suppliers.name` + `supplier_aliases.alias_name`
5. Se match encontrado: sugere fornecedor, recorrência e período
6. Se não encontrado: sugere criar novo fornecedor com dados extraídos

### 13.4. Tool Calls MCP com Fornecedor

**João Pereira (Backend):**
Na Fase 5 (MCP/Agentes), fornecedor expande significativamente o vocabulário de ferramentas:

```typescript
// Novas tool calls habilitadas pelo domínio de fornecedor

// Consulta por fornecedor
get_supplier_spending({ supplier_name: "Neoenergia", period: "last_6_months" });
// → { total: 1580, avg_monthly: 263.33, trend: "up_5%", transactions: [...] }

// Comparação entre fornecedores
compare_suppliers({ category: "internet", period: "last_12_months" });
// → { Vivo: { total: 1798.80, avg: 149.90 }, Canaã: { total: 1438.80, avg: 119.90 } }

// Evolução de métricas
get_consumption_metrics({ supplier_name: "Neoenergia", metric: "kWh", period: "last_12_months" });
// → [{ month: "Out/2025", quantity: 240 }, { month: "Nov/2025", quantity: 265 }, ...]

// Detecção de anomalias
detect_supplier_anomalies({ period: "current" });
// → [{ supplier: "ChatGPT", expected: 21, actual: 120, change: "+471%" }]

// Sugestão de associação
suggest_supplier_associations({ unlinked_transactions: true });
// → [{ transaction_id: "xxx", suggested_supplier: "Neoenergia", confidence: 0.95, via: "alias:CELPE" }]

// Resolução de alias
resolve_supplier_alias({ text: "CELPE DISTRIBUIÇÃO S/A" });
// → { supplier_id: "xxx", name: "Neoenergia", match_type: "alias", confidence: 0.92 }
```

**Pedro Santos:**
Cada uma dessas chamadas é possível porque o domínio de fornecedor está modelado agora. Sem as tabelas `suppliers`, `supplier_aliases`, `supplier_contracts` e `consumption_metrics`, essas tool calls não teriam de onde buscar dados.

### 13.5. Rotinas de Classificação e Enriquecimento

**Ana Silva (Arquiteta):**
Proponho um pipeline de enriquecimento automático para a Fase 4:

```
Transação nova → Pipeline de enriquecimento:
1. Resolução de fornecedor (alias matching)
2. Sugestão de categoria (baseado em fornecedor.type + histórico)
3. Sugestão de tags (baseado em fornecedor.tags + padrão do usuário)
4. Sugestão de prioridade (baseado em fornecedor.tags + regras)
5. Extração de métricas (se documento anexado)
6. Vinculação com recorrência (se fornecedor + período match)
```

Cada etapa é independente e auditável. O usuário pode aceitar/rejeitar cada sugestão.

### 13.6. Reconciliação de Dados Importados

**Laura Costa:**
Na importação de dados (CSV/XLSX), fornecedor melhora significativamente a qualidade:

| Etapa              | Sem fornecedor           | Com fornecedor                                |
| ------------------ | ------------------------ | --------------------------------------------- |
| Parse da descrição | Texto bruto              | Resolução via aliases → fornecedor conhecido  |
| Deduplicação       | Data + valor + descrição | Data + valor + **supplier_id** (mais preciso) |
| Categorização      | Manual por linha         | Automática via supplier.type + histórico      |
| Conciliação        | Comparação de texto      | Matching por supplier_id + período            |

### 13.7. Decisão Final sobre IA/MCP

Aprovado:

- Fornecedor como dimensão central da taxonomia de IA
- 6 novas tool calls MCP habilitadas pela modelagem de fornecedor
- Pipeline de enriquecimento automático em 6 etapas para Fase 4
- Aliases e contratos como vocabulário de resolução para OCR/NLP
- Métricas de consumo como base para detecção de anomalias

---

## 14. Regras de Negócio Revisadas e Novas Regras

### 14.1. Regras Existentes Impactadas

**Maria Oliveira (Backend):**
Revisando as 17 regras originais, as seguintes são impactadas pela adição de fornecedor:

| Regra | Descrição original                         | Impacto do fornecedor                                                        |
| ----- | ------------------------------------------ | ---------------------------------------------------------------------------- |
| R1    | Pagamento de fatura não gera despesa       | **Sem impacto** — fornecedor não altera esta regra                           |
| R2    | Transferência não é gasto                  | **Sem impacto** — transferência interna não tem fornecedor                   |
| R3    | Lançamento atribuído ao período            | **Sem impacto** — fornecedor não altera atribuição temporal                  |
| R5    | Compras respeitam fechamento               | Impactado: item de fatura pode ter `supplier_id` → matching por alias        |
| R7    | Recorrência gera expectativa               | Impactado: instância herda `supplier_id` do template                         |
| R9    | Importação sem duplicação                  | Impactado: `supplier_id` melhora deduplicação                                |
| R13   | Múltiplas tags por despesa                 | **Sem impacto** — tags de transação coexistem com fornecedor                 |
| R14   | Filtros por tag                            | Impactado: filtro por tag deve funcionar combinado com filtro por fornecedor |
| R17   | Primeira tela reflete prioridade e período | Impactado: primeira tela agora mostra fornecedor                             |

### 14.2. Novas Regras de Negócio (R18-R27)

**Ricardo Monteiro (Economista) e equipe:**

#### R18. Fornecedor é opcional em transações

- Todo lançamento PODE ter um fornecedor, mas não é obrigatório
- Transferências internas NÃO têm fornecedor (são movimentações entre contas próprias)
- Pagamento de fatura NÃO é transação com fornecedor (é quitação de obrigação)

#### R19. Fornecedor não substitui categoria nem tag

- Fornecedor, categoria, tags e prioridade são dimensões ortogonais
- Uma transação pode ter fornecedor + categoria + N tags + prioridade simultaneamente
- Nenhuma dimensão invalida outra

#### R20. Alias de fornecedor deve resolver para ID único

- Cada alias resolve para exatamente UM fornecedor
- Dois fornecedores NÃO podem compartilhar o mesmo alias ativo no mesmo período
- Alias expirado (valid_until < now) não é usado em resolução ativa

#### R21. Histórico é vinculado ao ID, não ao nome

- Renomear um fornecedor NÃO altera o supplier_id
- Transações históricas vinculadas pelo ID permanecem vinculadas
- O nome antigo é preservado como alias com tipo "legacy_name"

#### R22. Métricas de consumo são opcionais e livres

- Fornecedor pode ter zero ou muitas métricas por transação
- Tipo de métrica é texto livre (kWh, m³, minutos, GB, unidades)
- Métricas são informativas — não alteram valor da transação

#### R23. Merge de fornecedores é operação atômica

- Ao mesclar dois fornecedores, TODAS as referências são migradas em transação
- O fornecedor absorvido tem seu nome convertido em alias do sobrevivente
- Histórico de métricas e contratos são migrados
- Audit log registra a operação de merge completa

#### R24. Contratos de fornecedor são informativos

- Contratos (UC, instalação, conta cliente) são metadados de referência
- Não alteram contabilização, saldo ou classificação
- Servem para: conciliação, auditoria, leitura por IA, organização pessoal

#### R25. Fornecedor de tipo financial_institution pode ter link com institutions

- Se supplier.type = 'financial_institution', o campo institution_id pode ser preenchido
- O link é opcional — um fornecedor financeiro pode existir sem instituição cadastrada
- A instituição financeira continua sendo a entidade de agrupamento patrimonial (contas, cartões, produtos)
- O fornecedor financeiro é a entidade de classificação de gastos

#### R26. Sugestões de associação não são aplicadas automaticamente

- Rotinas de matching por alias geram SUGESTÕES, não associações automáticas
- O usuário deve confirmar cada associação (individual ou em lote)
- Exceção futura: na Fase 4 (IA), associações com confiança > 95% podem ser auto-aplicadas com opt-in do usuário

#### R27. Relatórios por fornecedor não duplicam entre meios de pagamento

- Se a mesma despesa aparece como item de fatura E como transação direta, o relatório por fornecedor conta apenas UMA vez
- A regra de deduplicação é: mesmo supplier_id + mesmo valor + mesma data ± 3 dias + mesma competência = potencial duplicata → exibir apenas uma vez

### 14.3. Decisão Final sobre Regras de Negócio

**Chico (CEO):**
Aceito todas as 10 novas regras (R18-R27). São consistentes com o domínio e não conflitam com as 17 regras anteriores. O conjunto completo agora é de 27 regras de negócio mandatórias.

Aprovado:

- 17 regras originais (R1-R17) mantidas, 5 com impacto documentado
- 10 novas regras (R18-R27) incorporadas
- Total: 27 regras de negócio mandatórias

---

## 15. Escopo do MVP Revisado com Fornecedor

### 15.1. Novas Funcionalidades do MVP

**Ana Silva (Arquiteta):**
Revisando o escopo do MVP incorporando fornecedor com impacto mínimo no cronograma:

| #        | Funcionalidade                                     | Prioridade | Sprint estimada | Dependência              |
| -------- | -------------------------------------------------- | ---------- | --------------- | ------------------------ |
| F-NEW-1  | CRUD de fornecedores (nome, tipo, documento, tags) | 🔴 P0      | Sprint 1        | Schema de banco          |
| F-NEW-2  | Aliases de fornecedor (cadastro e busca)           | 🟠 P1      | Sprint 2        | F-NEW-1                  |
| F-NEW-3  | Contratos de fornecedor (cadastro informativo)     | 🟡 P2      | Sprint 3        | F-NEW-1                  |
| F-NEW-4  | Associação de fornecedor a transações              | 🔴 P0      | Sprint 1-2      | F-NEW-1, CRUD transações |
| F-NEW-5  | Associação de fornecedor a recorrências            | 🔴 P0      | Sprint 2        | F-NEW-1, recorrências    |
| F-NEW-6  | Fornecedor na primeira tela (nome, top, alertas)   | 🟠 P1      | Sprint 2-3      | F-NEW-1, dashboard       |
| F-NEW-7  | Filtro por fornecedor (individual e combinado)     | 🟠 P1      | Sprint 2-3      | F-NEW-4                  |
| F-NEW-8  | Relatório de gastos por fornecedor                 | 🟠 P1      | Sprint 3        | F-NEW-4                  |
| F-NEW-9  | Métricas de consumo (registro manual)              | 🟡 P2      | Sprint 3-4      | F-NEW-1                  |
| F-NEW-10 | Sugestão de associação retroativa por alias        | 🟡 P2      | Sprint 4        | F-NEW-2                  |

### 15.2. Tabela Completa do MVP Revisado

**João Pereira (Backend):**
Integrando com o escopo anterior:

| Módulo                    | Entregáveis                                                           | P      | Sprint         |
| ------------------------- | --------------------------------------------------------------------- | ------ | -------------- |
| Core                      | Auth, setup de ciclo financeiro, instituições, produtos               | P0     | 1              |
| **Fornecedores**          | **CRUD de fornecedores, aliases, tipos**                              | **P0** | **1**          |
| Transações                | Lançamento manual, categorias, tags, prioridade, **fornecedor**       | P0     | 1-2            |
| Transferências            | Registro de transferências internas (sem despesa, **sem fornecedor**) | P0     | 2              |
| Cartões                   | Cadastro de cartões, ciclos de fatura, itens de fatura                | P0     | 2              |
| Faturas                   | Pagamento de fatura (sem gerar despesa)                               | P0     | 2              |
| Recorrências              | Templates + instâncias + geração automática + **fornecedor herdado**  | P1     | 2-3            |
| Dívidas                   | Cadastro de passivos, parcelas, decomposição                          | P1     | 3              |
| Dashboard                 | Primeira tela de ação + **top fornecedores + alertas**                | P0     | 2-3            |
| Relatórios                | Por mês/período/categoria/tag + **por fornecedor**                    | P1     | 3              |
| **Filtros**               | **Combinação fornecedor × categoria × tag × prioridade × período**    | **P1** | **2-3**        |
| **Contratos**             | **Cadastro informativo de contratos por fornecedor**                  | **P2** | **3**          |
| **Métricas**              | **Registro manual de consumo (kWh, m³, etc.)**                        | **P2** | **3-4**        |
| Importação                | Upload de CSV/XLSX com preview + controle de duplicação               | P2     | 4              |
| Documentos                | Upload e vinculação de anexos + **fornecedor**                        | P2     | 4              |
| **Associação retroativa** | **Sugestão de vínculo antigo por alias**                              | **P2** | **4**          |
| Testes                    | **27 cenários mandatórios** + domínio + integração                    | P0     | 1 (e contínuo) |

### 15.3. Impacto no Cronograma

**Fernando Gomes (DevOps):**
O CRUD de fornecedor é relativamente simples — uma tabela com form. Aliases e contratos também. O impacto real é que TODA transação/recorrência/documento agora tem um campo a mais. A boa notícia: é um FK nullable, então não bloqueia nada.

**Ana Silva:**
Estimo impacto de +15-20% no esforço total do MVP (não no prazo). A maioria das features de fornecedor se encaixa dentro das sprints já planejadas. As adições puras (métricas, associação retroativa) ficam em P2 e não bloqueiam o lançamento.

### 15.4. Fluxos do MVP Revisados

**Camila Duarte (Consultora):**
Revisando os fluxos principais com fornecedor integrado:

| #       | Fluxo                                | Atualização                                      |
| ------- | ------------------------------------ | ------------------------------------------------ |
| F1      | Cadastrar banco/produtos             | Sem mudança                                      |
| F2      | Configurar ciclo financeiro          | Sem mudança                                      |
| F3      | Lançar transação                     | **+ campo fornecedor (select/autocomplete)**     |
| F4      | Registrar transferência interna      | Sem mudança (sem fornecedor)                     |
| F5      | Registrar pagamento de fatura        | Sem mudança (sem fornecedor)                     |
| F6      | Cadastrar dívida                     | **+ campo fornecedor (para quem é a dívida)**    |
| F7      | Importar planilha                    | **+ sugestão de fornecedor por alias**           |
| F8      | Anexar documento                     | **+ campo fornecedor**                           |
| F9      | Aplicar categoria/tags               | Sem mudança (fornecedor é campo separado)        |
| F10     | Visualizar vencimentos               | **+ nome do fornecedor nos itens**               |
| **F11** | **Cadastrar fornecedor**             | **NOVO: nome, tipo, documento, tags**            |
| **F12** | **Cadastrar alias de fornecedor**    | **NOVO: alias, tipo, período**                   |
| **F13** | **Cadastrar contrato de fornecedor** | **NOVO: tipo, identificador, metadados**         |
| **F14** | **Registrar métrica de consumo**     | **NOVO: tipo, quantidade, preço, subtotal**      |
| **F15** | **Filtrar por fornecedor**           | **NOVO: select + combinação com outros filtros** |
| **F16** | **Ver relatório por fornecedor**     | **NOVO: totais, evolução, comparação**           |

### 15.5. Decisão Final sobre Escopo MVP

**Chico (CEO):**
Aceito o escopo ampliado. Fornecedor é aderente ao MVP porque resolve um problema real do dia-a-dia ("para QUEM eu pago?"). Prioridade P0 para CRUD, P1 para filtros e relatórios, P2 para métricas e retroatividade.

Aprovado:

- 10 novas funcionalidades (F-NEW-1 a F-NEW-10)
- 6 novos fluxos (F11-F16)
- Impacto de ~15-20% no esforço, sem aumento significativo de prazo
- Fornecedor CRUD entra na Sprint 1 junto com instituições

---

## 16. Roadmap Revisado

### 16.1. Fases Atualizadas

**Ana Silva (Arquiteta):**

#### Fase 1: MVP sem IA (revisada)

- Tudo do roadmap original MAIS:
- CRUD de fornecedores + aliases + tipos
- Associação de fornecedor a transações, recorrências, documentos
- Filtro por fornecedor na interface
- Fornecedor na primeira tela (nome nos itens + top fornecedores)
- Relatório de gastos por fornecedor
- Contratos de fornecedor (cadastro informativo)
- 27 testes mandatórios (17 originais + 10 de fornecedor)

#### Fase 2: Importação e Automação Básica (revisada)

- Tudo do roadmap original MAIS:
- Sugestão de associação retroativa por alias
- Matching de fornecedor em importação de extrato (via alias)
- Regras automáticas de categorização baseadas em fornecedor.type
- Registro manual de métricas de consumo
- Relatório de evolução temporal por fornecedor
- Merge de fornecedores duplicados

#### Fase 3: Documentos e Leitura Assistida (revisada)

- Tudo do roadmap original MAIS:
- Vinculação de documento com fornecedor
- Preparação de dados para OCR (CNPJ, UC, instalação em supplier_contracts)
- Extração básica de dados vinculados ao fornecedor (regex patterns)

#### Fase 4: IA (revisada)

- Tudo do roadmap original MAIS:
- Classificação automática de fornecedor por alias/CNPJ
- Extração de métricas de consumo de PDFs
- Detecção de anomalias por fornecedor
- Sugestão automática de fornecedor + categoria + tags
- Pipeline de enriquecimento em 6 etapas
- Auto-associação com confiança > 95% (opt-in)

#### Fase 5: MCP/Agentes (revisada)

- Tudo do roadmap original MAIS:
- Tool calls de fornecedor: get_supplier_spending, compare_suppliers, get_consumption_metrics, detect_supplier_anomalies, suggest_supplier_associations, resolve_supplier_alias
- Consultas em linguagem natural com dimensão fornecedor
- Enriquecimento de relatórios via agente

### 16.2. Decisão Final sobre Roadmap

Aprovado:

- Todas as 5 fases revisadas incorporam fornecedor gradualmente
- Fase 1 absorve CRUD, associação, filtro e relatório básico
- Fases 2-5 evoluem progressivamente a inteligência sobre fornecedor
- Nenhuma fase é bloqueada pela ausência de fornecedor nas fases posteriores

---

## 17. Decisões Finais e Checklist de Entregáveis

### 17.1. Checklist dos 12 Entregáveis Obrigatórios

**Verônica (Consultora):**
Conferindo cada entregável pedido no prompt #002:

| #   | Entregável                                        | Seção        | Status                                   |
| --- | ------------------------------------------------- | ------------ | ---------------------------------------- |
| 1   | Resumo do impacto da nova dimensão fornecedor     | Seção 2      | ✅ Completo                              |
| 2   | Alterações necessárias no mapa de domínio         | Seção 3      | ✅ Completo                              |
| 3   | Alterações necessárias no modelo de dados         | Seção 5      | ✅ Completo (5 tabelas novas, 5 FKs)     |
| 4   | Decisão sobre relação fornecedor vs IF            | Seção 4      | ✅ Opção A aprovada                      |
| 5   | Modelagem de aliases e consolidação histórica     | Seção 6      | ✅ Completo com 4 cenários               |
| 6   | Modelagem de métricas de consumo/unidades         | Seção 5.4    | ✅ Tabela consumption_metrics            |
| 7   | Impacto em recorrências, documentos e conciliação | Seções 7 e 8 | ✅ Completo                              |
| 8   | Impacto em relatórios, filtros e auditoria        | Seção 10     | ✅ 3 relatórios + 7 queries de auditoria |
| 9   | Impacto na home/tela inicial                      | Seção 11     | ✅ Wireframe revisado + 3 alertas        |
| 10  | Impacto na estratégia de testes                   | Seção 12     | ✅ 10 novos testes (T18-T27)             |
| 11  | Impacto na futura camada de IA/MCP                | Seção 13     | ✅ 6 tool calls + pipeline               |
| 12  | Versão revisada das entidades e regras afetadas   | Seção 14     | ✅ 10 novas regras (R18-R27)             |

### 17.2. Resumo das Decisões Chave desta Revisão

| #   | Decisão                                                     | Justificativa                                         | Aprovado por             |
| --- | ----------------------------------------------------------- | ----------------------------------------------------- | ------------------------ |
| D1  | Fornecedor e Instituição como entidades separadas (Opção A) | Clareza semântica, simplicidade de queries            | Todos                    |
| D2  | FK direto (1:1) em vez de tabela N:N para supplier_id       | Realidade de finanças pessoais (1 txn = 1 fornecedor) | Ana, André, Maria        |
| D3  | Enum de tipo de fornecedor + tags livres                    | Flexibilidade com estrutura base                      | Maria, João, Camila      |
| D4  | Métricas de consumo como tabela dedicada                    | Isolamento, auditoria, queries específicas            | Ricardo, André           |
| D5  | Aliases com tipo e período de validade                      | Suporte a renomeação, nomes de fatura, abreviações    | Pedro, Ana               |
| D6  | Merge atômico de fornecedores via Edge Function             | Garantia de integridade referencial                   | André, Fernando          |
| D7  | Fornecedor contextual na home (não seção dedicada)          | Não sobrecarregar tela de ação                        | Helena, Isabella, Camila |
| D8  | 10 novos testes mandatórios (T18-T27)                       | Cobertura do domínio de fornecedor                    | Maria, equipe            |
| D9  | 10 novas regras de negócio (R18-R27)                        | Formalização das regras do novo domínio               | Ricardo, equipe          |
| D10 | Escopo MVP expandido com impacto +15-20%                    | Fornecedor resolve problema real, vale o custo        | Chico (CEO)              |

---

## 18. Encerramento

### 18.1. Recomendações Finais da Equipe

**Ana Silva (Arquiteta):**

> A inclusão de fornecedor no domínio é uma das decisões mais acertadas deste projeto. Fornecedor transforma o sistema de "registro de gastos" para "gestão de relacionamentos financeiros". A modelagem está sólida — implementem com confiança.

**Ricardo Monteiro (Economista):**

> Métricas de consumo vão transformar a análise financeira do CEO. "Gastei R$ 280 de energia" é informação. "Gastei R$ 280 por 320 kWh com bandeira amarela, 12% acima da média semestral" é inteligência. Essa é a diferença.

**Camila Duarte (Consultora):**

> Na minha experiência com clientes, a pergunta "para quem eu pago?" vem logo depois de "quanto eu gasto?". Fornecedor responde a primeira. E combinado com métricas, responde "estou pagando caro?". Essas respostas mudam comportamento.

**André Santos (DBA):**

> As 5 novas tabelas são leves e bem indexadas. O maior cuidado é com os aliases — o índice trigram em `alias_name` é essencial para performance de busca fuzzy. Não pulem esse índice.

**Maria Oliveira (Backend):**

> Os 27 testes mandatórios agora cobrem o domínio completo. São contrato. Implementação que não passa nos testes está errada. Escrevam T18-T27 ANTES de implementar o CRUD de fornecedor.

**Roberto Lima (Frontend):**

> O componente de select/autocomplete de fornecedor será usado em muitos forms (transação, recorrência, documento, dívida). Componentize bem desde o início — SupplierSelect com busca, criação inline e display de tipo/tags.

**Pedro Santos (Backend):**

> A preparação para IA é o aspecto mais estratégico. CNPJ, aliases, contratos — tudo que modelamos agora será vocabulário para a IA na Fase 4. Quanto mais rico o dado do fornecedor, mais inteligente a classificação automática.

**Fernando Gomes (DevOps):**

> As migrações das 5 novas tabelas são independentes — podem ser aplicadas em paralelo. Separar por arquivo: `001_suppliers.sql`, `002_supplier_aliases.sql`, etc. Isso facilita rollback se necessário.

**Helena Vargas (UX):**

> O fornecedor na home deve ser sutil mas acessível. Chip no nome do pagamento, seção colapsável de top fornecedores. Se sobrecarregar, perdemos a clareza de "o que pagar agora?". Façam teste de usabilidade com dados reais.

**Thiago Martins (Front Engineer):**

> O SupplierBadge será tão frequente quanto o PriorityBadge. Pensem como componente de design system: variantes por tipo (utility=azul, saas=roxo, individual=verde), tamanho small/medium, com e sem ícone.

### 18.2. Fechamento

**Chico (CEO):**
Excelente trabalho, equipe. A Verônica tinha razão — fornecedor não é um detalhe, é uma dimensão central do domínio financeiro pessoal. Este documento incorpora fornecedor de forma profunda e estruturada em TODAS as camadas do sistema: modelo, regras, testes, telas, relatórios, IA e MCP.

Resumo do que muda:

1. **+5 tabelas** no modelo de dados (suppliers, aliases, contracts, supplier_tags, consumption_metrics)
2. **+5 FKs** em tabelas existentes (transactions, recurring_templates, statement_items, documents, liabilities)
3. **+10 regras de negócio** (R18-R27)
4. **+10 testes obrigatórios** (T18-T27)
5. **+6 novos fluxos** de usuário (F11-F16)
6. **+6 tool calls MCP** planejadas
7. **+3 relatórios** novos
8. **+3 alertas** de fornecedor na home
9. **Roadmap revisado** nas 5 fases
10. **Decisão arquitetural**: Opção A — fornecedor e instituição como entidades separadas

Este documento é a **referência oficial atualizada** para implementação. O refino anterior (#2026-03-21-10-40) continua válido para tudo que não foi revisado aqui.

**Verônica (Consultora):**
A equipe entregou exatamente o que foi pedido, com profundidade. Todos os 12 entregáveis estão presentes e documentados. Recomendo que este refino seja revisitado a cada sprint, assim como o anterior. Bom trabalho.

---

**Ações / Responsáveis / Prazo:**

| #   | Ação                                                                                                    | Responsável                      | Sprint                            |
| --- | ------------------------------------------------------------------------------------------------------- | -------------------------------- | --------------------------------- |
| 1   | Criar migrações das 5 novas tabelas (suppliers, aliases, contracts, supplier_tags, consumption_metrics) | André Santos / João Pereira      | Sprint 1                          |
| 2   | Adicionar FK supplier_id em transactions, recurring_templates, statement_items, documents, liabilities  | André Santos                     | Sprint 1                          |
| 3   | Implementar CRUD de fornecedores + aliases + tipos                                                      | Maria Oliveira / João Pereira    | Sprint 1                          |
| 4   | Criar componente SupplierSelect (autocomplete, busca, criação inline)                                   | Roberto Lima / Thiago Martins    | Sprint 1-2                        |
| 5   | Criar componente SupplierBadge (variantes por tipo)                                                     | Isabella Torres / Thiago Martins | Sprint 1                          |
| 6   | Integrar fornecedor no formulário de transação e recorrência                                            | Roberto Lima / Sofia Almeida     | Sprint 2                          |
| 7   | Implementar filtro por fornecedor (individual + combinado)                                              | Roberto Lima / Sofia Almeida     | Sprint 2-3                        |
| 8   | Adicionar fornecedor na primeira tela (nome nos itens + seção top + alertas)                            | Roberto Lima / Helena Vargas     | Sprint 2-3                        |
| 9   | Implementar relatório de gastos por fornecedor                                                          | João Pereira / Ricardo Monteiro  | Sprint 3                          |
| 10  | Implementar CRUD de contratos de fornecedor                                                             | Maria Oliveira                   | Sprint 3                          |
| 11  | Implementar registro de métricas de consumo                                                             | Maria Oliveira / Pedro Santos    | Sprint 3-4                        |
| 12  | Implementar sugestão de associação retroativa por alias                                                 | Pedro Santos / Laura Costa       | Sprint 4                          |
| 13  | Escrever testes T18-T27 como suíte de regressão                                                         | Maria Oliveira / Pedro Santos    | Sprint 1 (antes da implementação) |
| 14  | Prototipar wireframes com fornecedor (forms, filtros, home)                                             | Helena Vargas / Isabella Torres  | Sprint 1                          |
| 15  | Configurar índice trigram para busca fuzzy de aliases                                                   | André Santos                     | Sprint 1                          |
| 16  | Criar view materializada mv_supplier_spending                                                           | André Santos                     | Sprint 3                          |
| 17  | Documentar regras R18-R27 em ADR (Architecture Decision Record)                                         | Ana Silva                        | Sprint 1                          |
