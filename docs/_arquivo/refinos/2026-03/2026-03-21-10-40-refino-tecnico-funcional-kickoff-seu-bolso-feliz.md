---
Título da Reunião: Refino Técnico-Funcional — Kickoff do Projeto Seu Bolso Feliz
Data e Hora: 2026-03-21 10:40
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
Pauta: 1. Entendimento do problema e proposta de valor
  2. Premissas assumidas
  3. Escopo do MVP e fora de escopo
  4. Mapa do domínio (entidades e relacionamentos)
  5. Modelo de dados proposto
  6. Modelagem da dimensão do tempo
  7. Modelagem de categorias, tags e prioridades
  8. Modelagem de recorrências
  9. Modelagem de dívidas e passivos
  10. Regras de negócio essenciais
  11. Fluxos principais do MVP
  12. Arquitetura com Supabase
  13. Arquitetura web e mobile
  14. Design system — escolha e justificativa
  15. Documentos, segredos e backup
  16. Estratégia de testes e cenários de aceitação
  17. Riscos e dúvidas em aberto
  18. Roadmap priorizado
  19. Recomendações finais
---

# Reunião de Refino Técnico-Funcional — Kickoff Seu Bolso Feliz

## 1. Resumo Executivo do Entendimento do Problema

**Verônica (Consultora Externa):**
O projeto nasce de uma dor real e urgente: o controle financeiro pessoal feito em planilhas está falhando. O sistema precisa substituir esse processo com ganho de clareza, redução de atrito e capacidade de decisão. O objetivo prático do usuário é organizar a vida financeira para **zerar dívidas até o fim do ano**, com visão clara de fluxo, passivos, juros e amortização. Não estamos construindo um app bancário — estamos construindo um **sistema de decisão financeira pessoal**.

**Ricardo Monteiro (Economista):**
Do ponto de vista financeiro, o problema central é a falta de visibilidade sobre a **composição real das obrigações**. Quando alguém paga uma parcela de financiamento, não sabe quanto é amortização, quanto é juros, quanto é seguro. Quando paga a fatura do cartão, confunde com uma nova despesa. Quando tem múltiplos cartões e bancos, perde noção de para onde o dinheiro vai. A planilha não distingue nada disso — trata tudo como "saída". O sistema precisa **decompor a realidade financeira** em seus elementos reais.

**Camila Duarte (Consultora de Finanças Pessoais):**
Trabalho com pessoas que têm renda variável, múltiplos cartões e dívidas sobrepostas. O padrão que vejo é: a pessoa até tenta organizar, mas o atrito operacional vence. Ela esquece de anotar, cansa de categorizar, não entende por que o saldo não bate. O sistema precisa **funcionar mesmo com disciplina imperfeita**. A primeira tela não pode ser um dashboard bonito que ninguém lê — precisa ser uma **lista de ação**: o que pagar agora, o que pode esperar, o que está atrasado, quanto dinheiro precisa durar.

**Ana Silva (Arquiteta):**
Concordo com a Camila. O maior risco técnico aqui não é a stack — é a **modelagem errada do domínio**. Se misturarmos despesa com transferência, ou tratarmos parcela de dívida como gasto comum, o sistema vai gerar dados incorretos e o usuário vai perder confiança. A arquitetura precisa ser simples, mas o domínio precisa ser **rigorosamente correto**.

**André Santos (DBA):**
E precisa ser correto desde o banco de dados. Se a modelagem de dados não distinguir transferência de despesa, não tem frontend que conserte. O schema é o contrato real do sistema.

**Chico (CEO):**
Perfeito. Então o consenso é: domínio correto primeiro, interface útil depois. Vamos prosseguir com essa premissa.

---

## 2. Premissas Assumidas

A equipe, após discussão, assume as seguintes premissas para o projeto:

1. **Usuário único inicialmente**: O MVP é para uso pessoal do CEO. Multi-tenant pode vir depois, mas não é prioridade.
2. **Dados financeiros são sensíveis**: RLS obrigatório, segredos isolados, sem exposição acidental.
3. **Serverless-first**: Supabase como backend principal, sem VPS no início. Custo operacional mínimo.
4. **Stack definido**: React + Next.js + Tailwind (web), React Native + Expo (mobile), Supabase (backend), Bun (runtime local).
5. **Sem IA no MVP**: A IA é futuro. O MVP é determinístico e correto.
6. **Importação de histórico é essencial**: O sistema precisa aceitar dados de planilhas desde o início.
7. **O ciclo financeiro do usuário não é o mês civil**: O sistema DEVE suportar períodos personalizados.
8. **Categoria ≠ Tag**: São conceitos distintos e não devem ser fundidos.
9. **Prioridade de pagamento é funcional, não cosmética**: Afeta ordenação, alertas e decisão.
10. **Documentos e anexos fazem parte do MVP**: Prints, comprovantes e PDFs são necessários desde o início.
11. **Testes moldam o comportamento**: Testes são especificação, não validação posterior.
12. **Web primeiro, mobile depois**: O MVP será web. Mobile vem na sequência com arquitetura preparada.
13. **Verônica é consultora externa**: Ela orienta os refinos mas não implementa.
14. **O sistema não é um app bancário**: Não faz transações, não conecta a APIs de banco. É registro e controle.
15. **Disciplina imperfeita é o cenário real**: O sistema deve minimizar atrito e não depender de consistência manual absoluta.

---

## 3. Escopo do MVP

### 3.1. Dentro do Escopo (MVP — Fase 1)

**Verônica:**
Vou listar o que deve entrar no MVP, baseado nos cenários obrigatórios do documento de referência. A equipe pode debater cada item.

**Discussão da equipe — itens aprovados para o MVP:**

| #   | Funcionalidade                                                                                                                    | Responsável primário sugerido     |
| --- | --------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| 1   | Cadastro de instituições financeiras (bancos, fintechs)                                                                           | João Pereira / André Santos       |
| 2   | Cadastro de produtos por instituição (conta corrente, poupança, cartão, empréstimo, financiamento, investimento, cheque especial) | João Pereira / André Santos       |
| 3   | Cadastro de cartões de crédito (com suporte a adicionais)                                                                         | João Pereira                      |
| 4   | Cadastro de ciclos de fatura por cartão (fechamento + vencimento)                                                                 | João Pereira / Ricardo Monteiro   |
| 5   | Configuração de ciclo financeiro personalizado do usuário                                                                         | João Pereira / Ricardo Monteiro   |
| 6   | Lançamento manual de transações (receita, despesa, ajuste, estorno)                                                               | Maria Oliveira                    |
| 7   | Registro de transferências internas entre contas/produtos                                                                         | Maria Oliveira                    |
| 8   | Registro de pagamento de fatura (sem gerar nova despesa)                                                                          | Maria Oliveira / Ricardo Monteiro |
| 9   | Cadastro de despesas parceladas                                                                                                   | Maria Oliveira                    |
| 10  | Cadastro de dívidas/passivos (empréstimo, financiamento, cheque especial) com composição de parcela                               | Ricardo Monteiro / João Pereira   |
| 11  | Cadastro de templates recorrentes (com tags e prioridade herdáveis)                                                               | Maria Oliveira                    |
| 12  | Geração de instâncias recorrentes a partir de templates                                                                           | Maria Oliveira                    |
| 13  | Sistema de categorias (categoria principal por lançamento)                                                                        | Maria Oliveira                    |
| 14  | Sistema de tags múltiplas (por lançamento, recorrência, dívida)                                                                   | Maria Oliveira                    |
| 15  | Prioridade de pagamento (manual e derivável por tag/tipo)                                                                         | Ricardo Monteiro / Maria Oliveira |
| 16  | Importação de histórico a partir de planilhas (CSV/XLSX)                                                                          | Pedro Santos / Laura Costa        |
| 17  | Upload de documentos e anexos (Supabase Storage)                                                                                  | Pedro Santos                      |
| 18  | Vinculação de documentos a entidades do domínio                                                                                   | Pedro Santos                      |
| 19  | Dashboard/primeira tela orientada a ação e decisão                                                                                | Roberto Lima / Helena Vargas      |
| 20  | Visão por instituição e por produto                                                                                               | Roberto Lima                      |
| 21  | Visão de próximos vencimentos                                                                                                     | Roberto Lima                      |
| 22  | Visão de faturas em aberto                                                                                                        | Roberto Lima                      |
| 23  | Visão de dívidas por produto (saldo, juros, amortização)                                                                          | Roberto Lima / Ricardo Monteiro   |
| 24  | Relatório por mês civil                                                                                                           | Sofia Almeida                     |
| 25  | Relatório por período financeiro personalizado                                                                                    | Sofia Almeida                     |
| 26  | Filtros por categoria e por tag                                                                                                   | Sofia Almeida                     |
| 27  | Visão de despesas essenciais vs postergáveis                                                                                      | Roberto Lima / Camila Duarte      |
| 28  | Fila/priorização operacional de pagamentos                                                                                        | Roberto Lima / Camila Duarte      |
| 29  | Autenticação via Supabase Auth                                                                                                    | Fernando Gomes / João Pereira     |
| 30  | RLS em todas as tabelas de dados do usuário                                                                                       | André Santos / Fernando Gomes     |
| 31  | Auditoria básica (log de operações sensíveis)                                                                                     | Maria Oliveira                    |

### 3.2. Fora do Escopo (para fases futuras)

**Camila Duarte:**
É importante ser explícito sobre o que NÃO entra agora, para evitar scope creep.

| #   | Item                                           | Fase prevista |
| --- | ---------------------------------------------- | ------------- |
| 1   | IA para classificação automática               | Fase 4        |
| 2   | Leitura automática de PDFs e comprovantes      | Fase 3-4      |
| 3   | OCR de documentos                              | Fase 4        |
| 4   | Assistente conversacional                      | Fase 4        |
| 5   | Integração com APIs de bancos (Open Banking)   | Fase futura   |
| 6   | MCP/agentes externos                           | Fase 5        |
| 7   | Multi-tenant / compartilhamento familiar       | Fase futura   |
| 8   | App mobile nativo                              | Fase 2-3      |
| 9   | Notificações push                              | Fase 2+       |
| 10  | Metas financeiras com tracking automático      | Fase 2        |
| 11  | Conciliação automática                         | Fase 2-4      |
| 12  | Importação inteligente (com regras aprendidas) | Fase 2        |
| 13  | Sugestão automática de tags e prioridade       | Fase 4        |
| 14  | Relatórios exportáveis (PDF/XLS)               | Fase 2        |
| 15  | Backup automático programado                   | Fase 2        |

**Chico (CEO):**
Aprovado. Mobile fica para depois, mas a arquitetura já deve estar preparada.

---

## 4. Mapa do Domínio

### 4.1. Visão Geral — Hierarquia Principal

**Ana Silva (Arquiteta):**
Proponho a seguinte hierarquia de domínio, que respeita os 8 princípios obrigatórios de modelagem:

```
Usuário (User)
├── Preferências Financeiras (UserFinancialPreferences)
│   └── Ciclo financeiro personalizado
│
├── Instituições Financeiras (Institution)
│   ├── Produtos Financeiros (FinancialProduct)
│   │   ├── Contas (Account) — corrente, poupança, investimento
│   │   ├── Cartões (Card)
│   │   │   ├── Titulares/Adicionais (CardHolder)
│   │   │   └── Ciclos de Fatura (StatementCycle)
│   │   │       └── Itens da Fatura (StatementItem)
│   │   └── Passivos/Dívidas (Liability)
│   │       ├── Empréstimo
│   │       ├── Financiamento Habitacional
│   │       ├── Cheque Especial
│   │       └── Parcelas com composição (LiabilityInstallment)
│   │           ├── Amortização
│   │           ├── Juros
│   │           ├── Seguros
│   │           └── Taxas/Encargos
│   └── ...múltiplos produtos
│
├── Transações (Transaction)
│   ├── Receita
│   ├── Despesa
│   ├── Estorno
│   ├── Ajuste
│   └── Pagamento de fatura (referência, sem nova despesa)
│
├── Transferências Internas (Transfer)
│   └── Conta origem → Conta destino (movimentação, não despesa)
│
├── Templates Recorrentes (RecurringTemplate)
│   └── Instâncias Recorrentes (RecurringInstance)
│       └── Status: previsto / vencido / pago / parcial / cancelado
│
├── Categorias (Category)
│   └── Hierarquia opcional (categoria pai)
│
├── Tags (Tag)
│   └── Relações N:N com transações, recorrências, dívidas
│
├── Prioridade de Pagamento
│   └── essencial / alta / média / baixa / opcional
│
├── Períodos Financeiros (FinancialPeriod)
│   └── Ciclo do usuário materializado (ex: 20/03 a 19/04)
│
├── Documentos e Anexos (Document / Attachment)
│   └── Vinculado a transação, dívida, fatura ou entidade genérica
│
└── Importações (ImportJob)
    └── Controle de importação sem duplicação
```

**Ricardo Monteiro (Economista):**
Essa hierarquia está correta. Destaco a separação entre Transaction e Transfer como crítica — é o erro mais comum em sistemas financeiros pessoais. Uma transferência entre contas próprias não é gasto, não é receita, é movimentação. O sistema tem que tratar como tal.

**Camila Duarte:**
Gosto de ver a Prioridade de Pagamento como cidadã de primeira classe no domínio. Na prática, é o que decide "pago a fatura do Nubank ou a mensalidade da academia?" — e a resposta precisa vir do sistema, não da memória do usuário.

### 4.2. Tipos de Evento Financeiro

**André Santos (DBA):**
Proponho a seguinte tipificação clara, como enum no banco:

| Tipo                | Descrição                                   | Impacto no saldo                                      |
| ------------------- | ------------------------------------------- | ----------------------------------------------------- |
| `income`            | Receita (salário, freelance, rendimento)    | + saldo                                               |
| `expense`           | Despesa real (compra, serviço, consumo)     | - saldo                                               |
| `internal_transfer` | Movimentação entre contas próprias          | neutro (origem -, destino +)                          |
| `statement_payment` | Pagamento de fatura de cartão               | - saldo na conta, quita fatura                        |
| `liability_payment` | Pagamento de parcela de dívida              | - saldo, decomposto em amortização + juros + encargos |
| `refund`            | Estorno / devolução                         | + saldo (reverte despesa)                             |
| `adjustment`        | Ajuste manual (correção, arredondamento)    | ± saldo                                               |
| `interest_charge`   | Cobrança de juros (cheque especial, atraso) | - saldo                                               |
| `fee`               | Tarifa bancária                             | - saldo                                               |

**Maria Oliveira (Backend):**
Importante: `statement_payment` e `liability_payment` são **quitações de obrigação**, não despesas novas. O frontend não deve exibi-los como "gasto do mês" — o gasto já foi registrado quando a compra foi feita no cartão ou quando a dívida foi contratada.

**Thiago Martins (Front Engineer):**
Concordo. No frontend, vamos precisar de filtros inteligentes que excluam automaticamente pagamentos de fatura e transferências internas da visão de "gastos reais". Se não fizermos isso, o usuário vai ver valores duplicados.

### 4.3. Separação Ativos vs Passivos vs Eventos

**Ricardo Monteiro:**
Proponho uma separação conceitual clara no domínio:

**Ativos** (o que o usuário TEM):

- Saldos em contas correntes
- Saldos em poupança
- Investimentos
- Dinheiro a receber

**Passivos** (o que o usuário DEVE):

- Faturas de cartão em aberto
- Parcelas de empréstimo a vencer
- Financiamento habitacional (saldo devedor)
- Cheque especial utilizado

**Eventos** (o que ACONTECE):

- Receitas, despesas, transferências, pagamentos, ajustes

**Patrimônio líquido** = Ativos - Passivos

O sistema deve poder calcular isso a qualquer momento, para qualquer período.

### 4.4. Discussão — Prós e Contras da Hierarquia Proposta

**Prós:**

- Separação clara entre tipos de evento evita os erros mais comuns (fatura como despesa, transferência como gasto)
- Hierarquia Instituição → Produto → Conta/Cartão/Dívida é intuitiva e extensível
- Materialização de FinancialPeriod permite queries simples por ciclo
- Tags N:N são flexíveis o suficiente para qualquer classificação futura
- Prioridade como conceito de primeira classe permite decisão operacional real

**Contras / Riscos identificados:**

- Complexidade inicial: São muitas entidades para um MVP pessoal → **Mitigação**: implementar incrementalmente, começando pelas entidades core
- O patrimônio líquido pode ficar impreciso se investimentos não forem atualizados → **Mitigação**: deixar investimento como saldo estático no MVP, atualização manual
- A decomposição de parcela de dívida exige dados que nem sempre estão disponíveis → **Mitigação**: permitir parcela simplificada (valor total) com campo opcional para decomposição

**Decisão Final:**
A equipe aprova a hierarquia proposta por Ana Silva com os refinamentos de Ricardo e André. Implementação será incremental.

---

## 5. Modelo de Dados Proposto

### 5.1. Tabelas Principais

**André Santos (DBA):**
Proponho o seguinte modelo relacional para o MVP. Cada tabela será protegida por RLS no Supabase.

> **Nota**: Todos os campos `id` são UUID v7 (ordenável por tempo). Todos têm `user_id` para RLS. Timestamps em UTC.

#### Core — Usuário e Preferências

```
users (gerenciado pelo Supabase Auth)
├── id: uuid (PK)
├── email: text
├── created_at: timestamptz
└── ...campos do Supabase Auth

user_financial_preferences
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users, UNIQUE)
├── financial_cycle_start_day: int (1-31) — dia de início do ciclo financeiro
├── financial_cycle_anchor_date: date — data âncora para cálculo de períodos
├── default_currency: text (default 'BRL')
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Instituições e Produtos

```
institutions
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── name: text (ex: "Caixa Econômica", "Nubank", "C6 Bank")
├── type: text (bank, fintech, broker, other)
├── icon_url: text (nullable)
├── color: text (nullable — para identificação visual)
├── is_active: boolean (default true)
├── display_order: int
├── created_at: timestamptz
└── updated_at: timestamptz

financial_products
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── institution_id: uuid (FK → institutions)
├── name: text (ex: "Conta Corrente", "Cartão Platinum", "Empréstimo Pessoal")
├── type: enum (checking_account, savings_account, credit_card, overdraft,
│          personal_loan, mortgage, investment, other)
├── current_balance: numeric(15,2) (para contas) — saldo atual
├── credit_limit: numeric(15,2) (nullable — para cartão e cheque especial)
├── is_active: boolean (default true)
├── display_order: int
├── metadata: jsonb (dados extras flexíveis)
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Cartões e Faturas

```
cards
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── financial_product_id: uuid (FK → financial_products)
├── last_four_digits: text (ex: "1234")
├── card_brand: text (Visa, Master, Elo, etc.)
├── is_primary: boolean — titular ou adicional
├── holder_name: text
├── credit_limit: numeric(15,2)
├── closing_day: int (1-31) — dia de fechamento da fatura
├── due_day: int (1-31) — dia de vencimento da fatura
├── is_active: boolean (default true)
├── created_at: timestamptz
└── updated_at: timestamptz

statement_cycles
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── card_id: uuid (FK → cards)
├── reference_month: date (YYYY-MM-01 — mês de referência)
├── cycle_start_date: date — início do período de compras
├── cycle_end_date: date — fechamento
├── due_date: date — vencimento
├── total_amount: numeric(15,2) — valor total da fatura
├── paid_amount: numeric(15,2) (default 0) — quanto já foi pago
├── status: enum (open, closed, paid, partial, overdue)
├── created_at: timestamptz
└── updated_at: timestamptz

statement_items
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── statement_cycle_id: uuid (FK → statement_cycles)
├── transaction_id: uuid (FK → transactions, nullable)
├── description: text
├── amount: numeric(15,2)
├── transaction_date: date — data da compra
├── installment_number: int (nullable — ex: 3/12)
├── total_installments: int (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Passivos e Dívidas

```
liabilities
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── financial_product_id: uuid (FK → financial_products)
├── name: text (ex: "Empréstimo Pessoal Caixa", "Financiamento Apt 302")
├── type: enum (personal_loan, mortgage, overdraft, installment_plan, other)
├── original_amount: numeric(15,2) — valor original contratado
├── outstanding_balance: numeric(15,2) — saldo devedor atual
├── interest_rate: numeric(8,6) — taxa de juros (mensal ou anual, conforme rate_type)
├── rate_type: enum (monthly, annual)
├── amortization_system: enum (sac, price, mixed, other, none)
├── total_installments: int (nullable)
├── paid_installments: int (default 0)
├── start_date: date — data de contratação
├── end_date: date (nullable) — previsão de quitação
├── status: enum (active, paid_off, renegotiated, defaulted)
├── metadata: jsonb — dados extras (ex: número contrato, banco, garantias)
├── created_at: timestamptz
└── updated_at: timestamptz

liability_installments
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── liability_id: uuid (FK → liabilities)
├── installment_number: int
├── due_date: date
├── total_amount: numeric(15,2) — valor total da parcela
├── principal_amount: numeric(15,2) (nullable) — amortização
├── interest_amount: numeric(15,2) (nullable) — juros
├── insurance_amount: numeric(15,2) (nullable) — seguros
├── fee_amount: numeric(15,2) (nullable) — taxas e encargos
├── paid_amount: numeric(15,2) (default 0)
├── paid_date: date (nullable)
├── status: enum (pending, paid, partial, overdue, waived)
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Transações e Transferências

```
transactions
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── financial_product_id: uuid (FK → financial_products)
├── type: enum (income, expense, refund, adjustment, interest_charge,
│         fee, statement_payment, liability_payment)
├── amount: numeric(15,2)
├── description: text
├── event_date: date — quando aconteceu
├── competence_date: date — a qual período pertence
├── financial_period_id: uuid (FK → financial_periods, nullable)
├── statement_cycle_id: uuid (FK → statement_cycles, nullable)
├── liability_installment_id: uuid (FK → liability_installments, nullable)
├── category_id: uuid (FK → categories, nullable)
├── priority: enum (essential, high, medium, low, optional) (nullable)
├── is_confirmed: boolean (default false) — previsto vs realizado
├── recurring_instance_id: uuid (FK → recurring_instances, nullable)
├── notes: text (nullable)
├── metadata: jsonb
├── created_at: timestamptz
└── updated_at: timestamptz

transfers
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── source_product_id: uuid (FK → financial_products) — origem
├── target_product_id: uuid (FK → financial_products) — destino
├── amount: numeric(15,2)
├── description: text
├── event_date: date
├── competence_date: date
├── financial_period_id: uuid (FK → financial_periods, nullable)
├── is_confirmed: boolean (default false)
├── notes: text (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Recorrências

```
recurring_templates
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── financial_product_id: uuid (FK → financial_products, nullable)
├── name: text (ex: "Fatura Nubank", "Internet Vivo", "Academia")
├── type: enum (income, expense, liability_payment, statement_payment)
├── amount: numeric(15,2) (nullable — pode ser variável)
├── is_variable_amount: boolean (default false)
├── frequency: enum (monthly, weekly, biweekly, quarterly, annual, custom)
├── day_of_month: int (nullable — para monthly)
├── custom_interval_days: int (nullable — para custom)
├── category_id: uuid (FK → categories, nullable)
├── priority: enum (essential, high, medium, low, optional) (nullable)
├── starts_at: date
├── ends_at: date (nullable)
├── is_active: boolean (default true)
├── notes: text (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz

recurring_instances
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── recurring_template_id: uuid (FK → recurring_templates)
├── expected_date: date — quando deveria ocorrer
├── expected_amount: numeric(15,2) (nullable)
├── actual_amount: numeric(15,2) (nullable)
├── status: enum (pending, paid, partial, skipped, overdue, cancelled)
├── paid_date: date (nullable)
├── transaction_id: uuid (FK → transactions, nullable) — vínculo com transação real
├── notes: text (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Categorias e Tags

```
categories
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── name: text (ex: "Moradia", "Alimentação", "Transporte", "Software/Serviços")
├── parent_id: uuid (FK → categories, nullable) — hierarquia opcional
├── icon: text (nullable)
├── color: text (nullable)
├── display_order: int
├── is_active: boolean (default true)
├── created_at: timestamptz
└── updated_at: timestamptz

tags
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── name: text (ex: "essencial", "trabalho_externo", "casa", "pessoa_fisica")
├── color: text (nullable)
├── influences_priority: boolean (default false)
├── suggested_priority: enum (essential, high, medium, low, optional) (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz

transaction_tags
├── transaction_id: uuid (FK → transactions)
├── tag_id: uuid (FK → tags)
└── PK: (transaction_id, tag_id)

recurring_template_tags
├── recurring_template_id: uuid (FK → recurring_templates)
├── tag_id: uuid (FK → tags)
└── PK: (recurring_template_id, tag_id)

liability_tags
├── liability_id: uuid (FK → liabilities)
├── tag_id: uuid (FK → tags)
└── PK: (liability_id, tag_id)
```

#### Períodos Financeiros

```
financial_periods
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── start_date: date — início do período (ex: 20/03)
├── end_date: date — fim do período (ex: 19/04)
├── label: text (ex: "Mar/2026", "Ciclo 2026-03")
├── is_current: boolean (default false)
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Documentos e Anexos

```
documents
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── name: text
├── description: text (nullable)
├── file_path: text — caminho no Supabase Storage
├── file_type: text (pdf, png, jpg, xlsx, csv, etc.)
├── file_size: bigint — em bytes
├── document_type: enum (receipt, invoice, statement, contract, proof, other)
├── entity_type: text (nullable) — polimórfico: 'transaction', 'liability', 'statement_cycle'
├── entity_id: uuid (nullable) — ID da entidade vinculada
├── version: int (default 1)
├── is_password_protected: boolean (default false)
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Segredos (isolado)

```
user_secrets (tabela separada, acesso apenas por Edge Functions server-side)
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── secret_type: text (ex: 'pdf_password', 'api_key')
├── entity_type: text (nullable)
├── entity_id: uuid (nullable)
├── encrypted_value: text — valor criptografado
├── created_at: timestamptz
└── updated_at: timestamptz
```

#### Importação e Auditoria

```
import_jobs
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── source_type: enum (csv, xlsx, manual, api)
├── file_path: text (nullable) — arquivo de origem no Storage
├── status: enum (pending, processing, completed, failed, partial)
├── total_rows: int (nullable)
├── imported_rows: int (default 0)
├── skipped_rows: int (default 0)
├── error_rows: int (default 0)
├── error_details: jsonb (nullable)
├── started_at: timestamptz (nullable)
├── completed_at: timestamptz (nullable)
├── created_at: timestamptz
└── updated_at: timestamptz

audit_logs
├── id: uuid (PK)
├── user_id: uuid (FK → auth.users)
├── action: text (ex: 'create', 'update', 'delete', 'import', 'login')
├── entity_type: text
├── entity_id: uuid (nullable)
├── old_values: jsonb (nullable)
├── new_values: jsonb (nullable)
├── ip_address: inet (nullable)
├── created_at: timestamptz
└── (sem updated_at — logs são imutáveis)
```

### 5.2. Discussão do Modelo de Dados

**João Pereira (Backend):**
O modelo está abrangente. Gosto do uso de `jsonb` para metadata — dá flexibilidade sem poluir o schema com colunas opcionais. Uma dúvida: por que `financial_period_id` nas transações é nullable?

**André Santos (DBA):**
Porque o período financeiro pode ser calculado retroativamente. Quando o usuário importa histórico, pode não ter períodos criados. O campo existe para otimizar queries, mas pode ser preenchido por uma função de "refresh" que associa transações a períodos com base nas datas.

**Ricardo Monteiro (Economista):**
Na tabela `liability_installments`, a separação `principal_amount`, `interest_amount`, `insurance_amount`, `fee_amount` é essencial. Destaco que a soma desses campos pode não bater exatamente com `total_amount` por arredondamentos bancários — o campo `total_amount` é o que vale. Os subcampos são **informativos e analíticos**.

**Maria Oliveira (Backend):**
Preciso reforçar: a tabela `transfers` é separada de `transactions` intencionalmente. Uma transferência gera dois movimentos (saída da origem, entrada no destino), mas NÃO é uma despesa. Não deve aparecer em relatórios de gastos.

**Roberto Lima (Frontend):**
Pergunta: o `is_confirmed` em transações — previsto vs realizado — como afeta a interface?

**Camila Duarte:**
É fundamental. Previsto significa "sei que vou gastar R$ 200 de internet dia 15". Realizado significa "já paguei, aqui está o comprovante". A primeira tela precisa mostrar ambos, mas diferenciados visualmente. Previstos são **alertas de ação**, realizados são **fatos consumados**.

**Helena Vargas (UX):**
Concordo. Visualmente, lançamentos previstos devem ter um estilo diferente — talvez opacidade reduzida ou ícone de relógio. Realizados são sólidos. Isso reduz confusão cognitiva do usuário.

### 5.3. Prós e Contras do Modelo Proposto

**Prós:**

- Modelo normalizado e robusto, com separação clara de responsabilidades
- Polimorfismo em `documents` (entity_type + entity_id) dá flexibilidade sem joins excessivos
- Tags N:N com tabelas de junção são padrão e eficientes
- `financial_periods` como entidade explícita elimina cálculos ambíguos
- `user_secrets` isolada protege dados sensíveis
- `import_jobs` com controle de linhas evita duplicação na importação
- `audit_logs` imutáveis garantem rastreabilidade

**Contras / Pontos de atenção:**

- Número de tabelas é alto (~20+) para MVP → **Mitigação**: implementar por módulo, começando por institutions + financial_products + transactions + categories + tags
- O polimorfismo em `documents` pode dificultar joins e constraints → **Mitigação**: aceito para MVP, pode ser refatorado para tabelas de junção específicas se necessário
- `current_balance` em `financial_products` é um campo desnormalizado → **Mitigação**: mantido por performance, atualizado via triggers ou Edge Functions

**Decisão Final:**
Modelo aprovado pela equipe como base do MVP. Implementação será incremental por módulo.

---

## 6. Modelagem da Dimensão do Tempo

### 6.1. As Três Visões Temporais

**Ricardo Monteiro (Economista):**
O tempo é a dimensão mais crítica e mais confusa em finanças pessoais. Proponho três visões claras e coexistentes:

#### Visão 1: Mês Civil

- Período: 01/MM a último dia do mês
- Uso: Filtros tradicionais, comparação com meses anteriores, relatórios padrão
- Exemplo: 01/03/2026 a 31/03/2026

#### Visão 2: Período Financeiro do Usuário (Ciclo Personalizado)

- Período: Configurável (ex: dia 20 ao dia 19 do mês seguinte)
- Uso: Orçamento real, análise de sobrevivência, meta de gastos, fluxo de caixa pessoal
- Exemplo: 20/03/2026 a 19/04/2026
- Regra: O dinheiro do usuário precisa durar ESSE período, não o mês civil

#### Visão 3: Ciclo da Fatura/Cartão

- Período: Fechamento ao próximo fechamento (ex: dia 15 ao dia 15)
- Vencimento: Fixo (ex: dia 23)
- Uso: Atribuir compras à fatura correta, saber quando pagar
- Regra: Compra feita APÓS o fechamento cai na fatura SEGUINTE

### 6.2. Como as Visões Coexistem

**Ana Silva (Arquiteta):**
Proponho a seguinte regra de atribuição de datas para cada transação:

| Campo                 | Significado                              | Exemplo                                                                     |
| --------------------- | ---------------------------------------- | --------------------------------------------------------------------------- |
| `event_date`          | Quando realmente aconteceu               | 18/03/2026 (dia da compra)                                                  |
| `competence_date`     | A qual período lógico pertence           | 20/03/2026 (início do ciclo) - ou o próprio event_date se não houver ajuste |
| `financial_period_id` | FK para o período financeiro do usuário  | Período "20/03 a 19/04"                                                     |
| `statement_cycle_id`  | FK para o ciclo de fatura (se aplicável) | Fatura Nubank Mar/2026                                                      |

**Regras de associação:**

1. `event_date` é SEMPRE preenchido — é fato.
2. `competence_date` pode diferir do event_date quando o usuário faz ajuste manual ou quando uma despesa feita num período impacta outro (ex.: compra dia 14/03 com cartão que fecha dia 15 → cai na fatura de março, não de abril).
3. `financial_period_id` é derivado do `competence_date` e da configuração do ciclo do usuário.
4. `statement_cycle_id` é derivado da data da compra e do ciclo de fechamento do cartão.

### 6.3. Materialização dos Períodos

**André Santos (DBA):**
Proponho que os períodos financeiros sejam **materializados** (tabela `financial_periods`), não apenas calculados. Razões:

1. **Performance**: Queries por período são extremamente comuns — um FK é mais rápido que calcular range de datas.
2. **Clareza**: O usuário pode ver seus períodos listados, com labels amigáveis.
3. **Flexibilidade**: Se o usuário mudar o dia de início do ciclo, os períodos antigos permanecem corretos.
4. **Relatórios**: Agrupar transações por período é trivial com FK.

**Geração de períodos:**

- Uma Edge Function pode gerar períodos automaticamente para os próximos 12 meses.
- Ao configurar ciclo, sistema gera períodos históricos para cobrir dados importados.
- Se o usuário mudar o dia de início, novos períodos futuros são gerados com a nova regra; os antigos permanecem intactos.

### 6.4. Exemplo Concreto

**Camila Duarte:**
Para ficar tangível, vamos com um exemplo real:

> Usuário recebe salário dia 20. Ciclo financeiro: 20/03 a 19/04.
> Dia 18/03, compra no cartão Nubank (fecha dia 15, vence dia 23).
> Dia 22/03, paga conta de internet.

**Mapeamento:**

| Evento         | event_date | competence_date | financial_period | statement_cycle          |
| -------------- | ---------- | --------------- | ---------------- | ------------------------ |
| Compra Nubank  | 18/03      | 18/03           | 20/02 a 19/03\*  | Fatura Abr (fecha 15/04) |
| Conta Internet | 22/03      | 22/03           | 20/03 a 19/04    | N/A                      |

\*A compra do dia 18/03 ocorre dentro do período financeiro anterior (20/02 a 19/03), porque o novo ciclo só começa dia 20/03.

**Ricardo Monteiro:**
E a compra do Nubank no dia 18/03, como o cartão fecha dia 15, vai cair na fatura que fecha em 15/04, com vencimento em 23/04. Isso é o ciclo da fatura, independente do ciclo financeiro do usuário.

### 6.5. Qual Data para Cada Finalidade

| Finalidade                          | Data utilizada                                      |
| ----------------------------------- | --------------------------------------------------- |
| Saldo da conta                      | event_date (quando movimentou)                      |
| Orçamento do período                | competence_date → financial_period_id               |
| Análise de sobrevivência            | financial_period_id + saldo atual                   |
| Previsão de estouro                 | competence_date + recorrências previstas no período |
| Relatório de gastos mensais (civil) | event_date filtrado por mês civil                   |
| Relatório de gastos por período     | transactions.financial_period_id                    |
| Atribuição à fatura                 | event_date + closing_day do cartão                  |
| Parcelamentos                       | cada parcela tem seu own due_date                   |
| Recorrências                        | recurring_instance.expected_date                    |
| Vencimentos                         | due_date da parcela ou da fatura                    |

### 6.6. Decisão Final sobre Tempo

A equipe aprova unanimemente:

- **Materialização** de períodos financeiros (tabela `financial_periods`)
- **Três datas** em transações: event_date, competence_date, financial_period_id
- **Ciclo de fatura** separado e vinculado via statement_cycle_id
- **Edge Function** para gerar períodos automaticamente
- **Regra**: Mudança de configuração de ciclo NÃO altera períodos passados

---

## 7. Modelagem de Categorias, Tags e Prioridades

### 7.1. Categoria Principal

**Maria Oliveira (Backend):**
Cada transação tem UMA categoria principal (FK nullable). A categoria é obrigatória para despesas, opcional para ajustes e transferências.

Categorias sugeridas para seed inicial:

| Categoria                  | Subcategoria (opcional)                               |
| -------------------------- | ----------------------------------------------------- |
| Moradia                    | Aluguel, Condomínio, IPTU, Manutenção                 |
| Alimentação                | Supermercado, Restaurante, Delivery                   |
| Transporte                 | Combustível, Estacionamento, Uber, Transporte público |
| Saúde                      | Plano, Farmácia, Consulta, Exame                      |
| Educação                   | Curso, Livro, Assinatura educacional                  |
| Trabalho                   | Home office, Coworking, Equipamento, Software         |
| Software/Serviços Digitais | Assinatura, SaaS, Cloud                               |
| Lazer                      | Streaming, Jogos, Viagem, Hobby                       |
| Pessoal                    | Vestuário, Cuidados pessoais, Presentes               |
| Financeiro                 | Juros, Tarifas, IOF, Seguros                          |
| Renda                      | Salário, Freelance, Rendimento, Reembolso             |
| Outros                     | —                                                     |

**Camila Duarte:**
Gosto da simplicidade. Um erro comum é criar muitas categorias desde o início — o usuário desiste de categorizar. Menos é mais. Tags fazem o trabalho fino.

### 7.2. Tags Múltiplas

**Maria Oliveira:**
Tags são N:N com qualquer entidade relevante. Definições:

- Tags são texto livre do usuário (com sugestões pré-definidas)
- Cada tag pode ter: `influences_priority` (boolean) e `suggested_priority` (enum)
- Exemplo: tag "essencial" → influences_priority = true, suggested_priority = "essential"

**Tags pré-definidas sugeridas:**

| Tag                      | influences_priority | suggested_priority |
| ------------------------ | ------------------- | ------------------ |
| essencial                | true                | essential          |
| moradia                  | true                | essential          |
| pessoa_fisica            | true                | high               |
| trabalho                 | true                | high               |
| trabalho_externo         | false               | —                  |
| casa                     | false               | —                  |
| diversao                 | false               | —                  |
| pesquisa_desenvolvimento | false               | —                  |
| ensino                   | false               | —                  |
| cartao_prioritario       | false               | —                  |

### 7.3. Prioridade de Pagamento

**Ricardo Monteiro:**
A prioridade pode ser definida de três formas, com precedência:

1. **Manual**: O usuário define explicitamente (campo `priority` na transação ou recorrência)
2. **Herdada do template**: Se veio de uma recorrência, herda a prioridade do template
3. **Derivada de tag**: Se nenhuma prioridade manual, verifica se alguma tag `influences_priority` sugere uma

**Regra de resolução:**

```
prioridade_efetiva =
  priority_manual OU
  priority_do_template OU
  MAX(suggested_priority das tags com influences_priority = true) OU
  'medium' (default)
```

**Camila Duarte:**
Na prática, isso significa que se o usuário criar uma recorrência "Condomínio" com prioridade "essential" e tag "moradia", todas as instâncias futuras já vêm com prioridade essencial. Sem esforço manual. É assim que se reduz atrito.

### 7.4. Decisão Final

A equipe aprova:

- Categoria principal obrigatória para despesas, opcional para outros tipos
- Tags N:N em transações, recorrências e dívidas
- Tags com capacidade de influenciar prioridade
- Prioridade resolúvel por cascata: manual → template → tag → default
- Seed de categorias e tags pré-definidas no setup inicial

---

## 8. Modelagem de Recorrências

### 8.1. Template vs Instância

**Maria Oliveira (Backend):**
Proponho separação clara:

**RecurringTemplate** = a definição da recorrência (ex: "Internet Vivo, R$ 120, todo dia 15, categoria Moradia, tags: casa, essencial")

**RecurringInstance** = cada ocorrência esperada (ex: "Internet Vivo — Março/2026, vencimento 15/03, status: pending")

**Regras:**

1. O template define: o quê, quanto (fixo ou variável), quando, qual conta, qual categoria, quais tags, qual prioridade
2. Instâncias são geradas automaticamente para os próximos N meses (sugestão: 3 meses à frente)
3. Uma instância pode estar: pending → paid / skipped / overdue / cancelled
4. Quando paga, a instância se vincula a uma transaction real (FK transaction_id)
5. Instâncias NÃO geram pagamento automático — geram EXPECTATIVA
6. Tags e prioridade do template são herdados pelas instâncias, mas podem ser sobrescritos

**João Pereira (Backend):**
A geração de instâncias pode ser feita por uma Edge Function agendada (cron no Supabase) que roda todo dia e cria instâncias faltantes para os próximos 3 meses. Simples e confiável.

**Camila Duarte:**
Fundamental: a instância nasce como "pending", não como "paid". O sistema mostra ao usuário "você tem X contas para pagar neste período" — e o usuário confirma quando pagou. Isso é o "reduzir atrito sem depender de disciplina perfeita".

### 8.2. Decisão Final

Aprovado: Template + Instância. Geração automática para 3 meses à frente. Status managed pelo usuário. Herança de tags e prioridade.

---

## 9. Modelagem de Dívidas e Passivos

### 9.1. Tipos de Passivo

**Ricardo Monteiro (Economista):**
O sistema deve modelar pelo menos quatro tipos de passivo com comportamentos distintos:

#### Empréstimo Pessoal (personal_loan)

- Valor fixo contratado, parcelas fixas ou decrescentes
- Composição da parcela: amortização + juros + seguros + taxas
- Sistema de amortização: SAC, Price ou outro
- Cronograma previsível

#### Financiamento Habitacional (mortgage)

- Passivo de longo prazo (10-35 anos)
- Composição complexa: amortização + juros + seguros (MIP, DFI) + taxas (TR, IPCA)
- Sistema SAC é mais comum
- Saldo devedor atualizado mensalmente
- Possibilidade de amortização extraordinária (pagamento antecipado de parcelas ou saldo)

#### Cheque Especial (overdraft)

- Sem parcelas fixas — juros sobre saldo devedor diário
- Ciclo: usa → paga → usa de novo
- Necessidade de registro do saldo devedor atual e taxa de juros
- Modelagem simplificada: saldo devedor + taxa de juros + registro de quitações

#### Parcelamento de Compra (installment_plan)

- Parcelas fixas, sem juros (ou com juros embutidos)
- Vinculado a um cartão ou a uma conta
- Cada parcela aparece numa fatura diferente
- Precisa rastrear: parcela atual / total (ex: 3/12)

### 9.2. Composição da Parcela

**Ricardo Monteiro:**
Para empréstimos e financiamentos, a tabela `liability_installments` permite registrar:

| Campo            | Significado                               |
| ---------------- | ----------------------------------------- |
| total_amount     | Valor total que sai do bolso naquele mês  |
| principal_amount | Quanto foi para amortizar o saldo devedor |
| interest_amount  | Quanto foi de juros                       |
| insurance_amount | Seguros (MIP, DFI para financiamento)     |
| fee_amount       | Taxas, encargos, chamadas extras          |

Isso permite relatórios como:

- "Em 2026, paguei R$ 28.000 no financiamento, dos quais R$ 12.000 foram juros"
- "Minha amortização acumulada é R$ 45.000 em 3 anos"
- "Se eu pagar R$ 10.000 extra agora, quito X parcelas e economizo R$ Y em juros"

### 9.3. Cenário de Quitação Antecipada

**Ricardo Monteiro:**
O MVP deve suportar pelo menos visualização simplificada de impacto de quitação:

1. Saldo devedor atual
2. Se pagar R$ X extra, quantas parcelas elimina
3. Quanto economiza em juros

A fórmula exata depende do sistema de amortização (SAC vs Price), mas para o MVP podemos usar cálculo simplificado com base no saldo devedor e taxa. Na Fase 4 (IA), podemos refinar.

### 9.4. Decisão Final

Aprovado: 4 tipos de passivo modelados. Tabela `liability_installments` com decomposição de parcela. Cenário de quitação como funcionalidade de leitura (sem automação de pagamento). Cheque especial com modelagem simplificada de saldo.

---

## 10. Regras de Negócio Essenciais

**Verônica (Consultora):**
Vou consolidar as regras que emergiram de toda a discussão. São invioláveis:

### Regras Fundamentais

| #   | Regra                                                        | Justificativa                                                                           |
| --- | ------------------------------------------------------------ | --------------------------------------------------------------------------------------- |
| R1  | Pagamento de fatura NÃO gera nova despesa                    | A despesa já foi registrada quando a compra foi feita no cartão                         |
| R2  | Transferência entre contas próprias NÃO é gasto              | É movimentação interna, neutra no total                                                 |
| R3  | Cada transação deve ser atribuída a um período financeiro    | Sem isso, não há como responder "quanto gastei neste ciclo"                             |
| R4  | O ciclo financeiro respeita configuração do usuário          | Se começa dia 20, começa dia 20 — o mês civil é secundário                              |
| R5  | Compras em cartão seguem ciclo de fechamento do cartão       | Compra após fechamento → fatura seguinte                                                |
| R6  | Parcela de dívida decomposta: amortização + juros + encargos | Sem isso, o usuário não sabe quanto é custo do dinheiro vs quanto está reduzindo dívida |
| R7  | Recorrências geram EXPECTATIVA, não pagamento automático     | O sistema sugere, o usuário confirma                                                    |
| R8  | Estornos revertem o valor sem criar receita artificial       | Estorno de R$ 50 = reduz despesa, não gera renda                                        |
| R9  | Importações controlam duplicação via hash/controle           | Importar duas vezes o mesmo CSV não pode duplicar registros                             |
| R10 | Documentos anexados NÃO alteram saldo                        | São evidência, não evento financeiro                                                    |
| R11 | Quitação antecipada recalcula saldo e impacto                | Se paga mais, deve reduzir saldo devedor e projeção de juros                            |
| R12 | Relatórios por mês civil e por período podem divergir        | Março civil ≠ ciclo 20/03-19/04 — e ambos devem estar corretos                          |
| R13 | Múltiplas tags sem perda de integridade                      | Uma transação com 5 tags não pode ter comportamento inconsistente                       |
| R14 | Filtros por tag devem funcionar corretamente                 | "Mostre tudo com tag essencial" deve retornar tudo, inclusive recorrências e dívidas    |
| R15 | Prioridade influencia ordenação e alertas                    | Essencial aparece primeiro, itens vencidos com prioridade alta geram destaque           |
| R16 | Essenciais não podem ser tratados como postergáveis          | Se a regra diz "essencial", não pode ser rebaixado automaticamente                      |
| R17 | Primeira tela reflete vencimento + prioridade + período      | É a tela de DECISÃO do usuário — deve ser confiável                                     |

---

## 11. Fluxos Principais do MVP

**Helena Vargas (UX):**
Vou descrever os fluxos de uso centrais. Cada fluxo será referência para desenvolvimento e testes.

### F1. Onboarding / Setup Inicial

1. Usuário cria conta (Supabase Auth — email/senha)
2. Configura ciclo financeiro (dia de início do período)
3. Cadastra pelo menos uma instituição
4. Cadastra pelo menos um produto financeiro
5. (Opcional) Importa planilha com histórico

### F2. Cadastro de Instituição e Produtos

1. Usuário adiciona instituição (nome, tipo, cor)
2. Dentro da instituição, adiciona produtos (conta corrente, cartão, empréstimo, etc.)
3. Para cartão: define últimos 4 dígitos, bandeira, limite, dia de fechamento e vencimento
4. Para empréstimo/financiamento: define valor original, saldo devedor, taxa, amortização, parcelas

### F3. Lançamento de Transação

1. Usuário seleciona produto financeiro (ou "geral")
2. Define: tipo (receita/despesa/etc.), valor, data, descrição
3. (Opcional) Seleciona categoria
4. (Opcional) Adiciona tags
5. (Opcional) Define prioridade
6. (Opcional) Marca como confirmado ou previsto
7. Sistema associa transação ao período financeiro correto
8. Se for compra em cartão, sistema associa ao ciclo de fatura correto

### F4. Registro de Transferência Interna

1. Usuário seleciona "Transferência entre contas"
2. Define: conta origem, conta destino, valor, data
3. Sistema registra como Transfer (neutra), não como despesa

### F5. Pagamento de Fatura de Cartão

1. Usuário seleciona fatura (statement_cycle)
2. Define: valor pago, conta de origem do pagamento, data
3. Sistema: atualiza status da fatura, registra transferência da conta para quitação da fatura
4. NÃO gera nova despesa

### F6. Cadastro e Gerenciamento de Dívida

1. Usuário cadastra passivo (tipo, valor, taxa, parcelas, sistema de amortização)
2. Sistema gera cronograma de parcelas (liability_installments)
3. Cada mês, o usuário confirma pagamento da parcela
4. Opcionalmente, informa decomposição (amortização, juros, seguros)
5. Sistema atualiza saldo devedor

### F7. Criação de Template Recorrente

1. Usuário define: nome, tipo, valor (fixo ou variável), frequência, dia, conta, categoria, tags, prioridade
2. Sistema gera instâncias futuras automaticamente
3. Cada instância aparece como "pendente" até confirmação de pagamento

### F8. Importação de Planilha

1. Usuário faz upload de arquivo (CSV/XLSX)
2. Sistema faz parse e apresenta preview dos dados
3. Usuário mapeia colunas (data, valor, descrição, categoria)
4. Sistema importa com controle de duplicação
5. Registro em import_jobs com contagem de linhas

### F9. Anexação de Documento

1. Usuário faz upload de arquivo (PDF, imagem, etc.)
2. Define: tipo de documento, entidade vinculada (transação, dívida, fatura)
3. Arquivo armazenado no Supabase Storage com RLS
4. Se protegido por senha, senha registrada em user_secrets (criptografada)

### F10. Visualização da Primeira Tela (Dashboard de Ação)

1. Ao abrir o sistema, o usuário vê:
   - Período financeiro atual e quantos dias restam
   - Saldo disponível total (soma de contas)
   - Próximos vencimentos (ordenados por prioridade → data)
   - Itens essenciais não pagos
   - Itens atrasados
   - Faturas em aberto
   - Parcelas de dívida do período
   - Quanto dinheiro precisa durar até o próximo recebimento
   - Gastos fora do padrão (comparado com meses anteriores)
2. Cada item tem ação rápida: "Marcar como pago", "Adiar", "Ver detalhes"

---

## 12. Arquitetura Sugerida com Supabase

### 12.1. Visão Geral

**Ana Silva (Arquiteta):**
Proponho a seguinte arquitetura serverless-first com Supabase:

```
┌─────────────────────────────────────────────────────┐
│                     FRONTEND                         │
│  Next.js (SSR/SSG) + React + Tailwind               │
│  Hospedagem: Vercel (free tier)                      │
└──────────────────────┬──────────────────────────────┘
                       │ HTTPS
                       ▼
┌─────────────────────────────────────────────────────┐
│                    SUPABASE                          │
│                                                      │
│  ┌─────────────┐  ┌─────────────┐  ┌────────────┐  │
│  │   Auth       │  │  Realtime   │  │  Storage   │  │
│  │  (email/pw)  │  │ (futuro)    │  │ (docs/PDF) │  │
│  └─────────────┘  └─────────────┘  └────────────┘  │
│                                                      │
│  ┌─────────────────────────────────────────────┐    │
│  │         PostgreSQL + RLS                     │    │
│  │   ~20 tabelas com Row Level Security         │    │
│  │   Triggers para atualizar saldos             │    │
│  │   Views para relatórios otimizados           │    │
│  └─────────────────────────────────────────────┘    │
│                                                      │
│  ┌─────────────────────────────────────────────┐    │
│  │         Edge Functions (Deno)                │    │
│  │   - Geração de períodos financeiros          │    │
│  │   - Geração de instâncias recorrentes        │    │
│  │   - Parse de importação (CSV/XLSX)           │    │
│  │   - Criptografia de segredos                 │    │
│  │   - Cálculo de quitação de dívidas           │    │
│  │   - Cron jobs agendados                      │    │
│  └─────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────┘
                       │
                       ▼
┌─────────────────────────────────────────────────────┐
│                LOCAL (Dev)                            │
│  Bun — scripts, seeds, migrações, ferramentas dev    │
└─────────────────────────────────────────────────────┘
```

### 12.2. Componentes e Responsabilidades

**Fernando Gomes (DevOps):**
Detalhando:

| Componente     | Tecnologia                                    | Responsabilidade                                                              |
| -------------- | --------------------------------------------- | ----------------------------------------------------------------------------- |
| Frontend Web   | Next.js + Vercel                              | UI, SSR para SEO futuro, autenticação client-side, consumo direto do Supabase |
| Auth           | Supabase Auth                                 | Email/senha, JWT, gestão de sessão                                            |
| DB             | Supabase Postgres                             | Fonte da verdade, RLS em todas as tabelas                                     |
| API            | Supabase auto-generated REST + Edge Functions | CRUD via PostgREST, lógica complexa via Edge Functions                        |
| Storage        | Supabase Storage                              | Documentos, comprovantes, PDFs — protegido por RLS                            |
| Edge Functions | Deno (Supabase)                               | Lógica server-side: cron para recorrências, importação, segredos, cálculos    |
| Dev Tools      | Bun                                           | Scripts de seed, migração local, testes de domínio, tooling                   |
| CI/CD          | GitHub Actions                                | Lint, testes, deploy automatizado                                             |

### 12.3. Segurança

**André Santos (DBA):**
RLS é obrigatório em TODAS as tabelas de dados do usuário. Modelo:

```sql
-- Exemplo de política RLS
CREATE POLICY "Users can only access their own data"
  ON transactions
  FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);
```

**Regras de segurança:**

1. **Toda tabela** com `user_id` tem RLS ativo
2. **user_secrets** tem RLS + acesso APENAS via Edge Functions (não exposta no client)
3. **audit_logs** são insert-only para o usuário, sem delete/update
4. **Supabase Storage** com políticas de bucket por user_id
5. JWT validado em toda requisição
6. Sem segredos no client-side — nunca

**Maria Oliveira (Backend):**
Para a tabela `user_secrets`, proponho acesso EXCLUSIVO via Edge Function com service_role_key. O client chama a Edge Function, que valida o JWT, executa a operação e retorna sem expor o valor do segredo em nenhum momento no client.

### 12.4. Estratégia de Consumo de Dados no Frontend

**João Pereira (Backend):**
Proponho padrão de consumo:

1. **CRUD simples** (transações, categorias, tags): Supabase JS client direto (PostgREST auto-generated). Exemplo: `supabase.from('transactions').select('*').eq('financial_period_id', periodId)`
2. **Operações complexas** (importação, cálculos, segredos): Edge Functions via `supabase.functions.invoke('nome-da-function', { body: {...} })`
3. **Relatórios e agregações**: Views materializadas no Postgres + queries via client
4. **Realtime** (futuro): Subscriptions do Supabase para atualização live

**Roberto Lima (Frontend):**
No Next.js, vamos usar Server Components quando possível para buscar dados no server (melhor performance, menos JS no client). Client Components apenas para interatividade (formulários, modais, ações).

### 12.5. Custo Operacional

**Fernando Gomes (DevOps):**
Estimativa de custo no início:

| Serviço          | Tier                                                              | Custo      |
| ---------------- | ----------------------------------------------------------------- | ---------- |
| Supabase         | Free tier (500MB DB, 1GB Storage, 500K Edge Function invocations) | $0         |
| Vercel           | Free tier (Hobby)                                                 | $0         |
| GitHub           | Free tier                                                         | $0         |
| Domínio (futuro) | .com.br / .app                                                    | ~R$ 40/ano |

**Custo total inicial: $0**. Escalável para Pro ($25/mês Supabase) quando necessário.

### 12.6. Decisão Final

Aprovada arquitetura serverless-first com Supabase + Vercel. Custo zero no início. RLS mandatório. Edge Functions para lógica server-side. Bun para ferramentas de desenvolvimento.

---

## 13. Arquitetura Web e Mobile

### 13.1. Arquitetura Web (Next.js)

**Roberto Lima (Frontend):**
Proponho a seguinte organização do projeto Next.js:

```
apps/
  web/                          # Next.js app
    src/
      app/                      # App Router (Next.js 14+)
        (auth)/                 # Grupo de rotas autenticadas
          dashboard/            # Primeira tela
          institutions/         # Cadastro de bancos
          products/             # Produtos financeiros
          transactions/         # Lançamentos
          transfers/            # Transferências internas
          recurring/            # Recorrências
          liabilities/          # Dívidas e passivos
          statements/           # Faturas
          reports/              # Relatórios
          settings/             # Configurações (ciclo financeiro, etc.)
          import/               # Importação de planilhas
          documents/            # Documentos e anexos
        (public)/               # Rotas públicas
          login/
          register/
        layout.tsx
        page.tsx
      components/
        ui/                     # Componentes base do design system
        domain/                 # Componentes específicos do domínio
          transaction-form/
          priority-badge/
          period-selector/
          liability-card/
          statement-summary/
        layout/                 # Header, Sidebar, Footer
      lib/
        supabase/               # Client e server Supabase helpers
        hooks/                  # Custom hooks
        utils/                  # Utilitários
        types/                  # Tipos TypeScript
        constants/              # Constantes e enums
        validators/             # Validação com Zod
      styles/
        globals.css             # Tailwind imports + tokens customizados

packages/
  shared/                       # Pacote compartilhado (web + mobile futuro)
    types/                      # Tipos do domínio financeiro
    validators/                 # Schemas Zod
    constants/                  # Enums, constantes de domínio
    utils/                      # Cálculos financeiros puros
```

**Thiago Martins (Front Engineer):**
Destaco pontos técnicos:

1. **App Router** (Next.js 14+): Server Components por padrão, Client Components explícitos
2. **Formulários**: React Hook Form + Zod para validação tipada
3. **Estado**: Evitar Redux. Usar: Server Components para dados, React Context para estado local, Tanstack Query para cache de dados client-side quando necessário
4. **Roteamento**: File-based routing do Next.js com route groups `(auth)` e `(public)`
5. **Autenticação**: Supabase Auth com middleware Next.js para proteger rotas
6. **Tipagem**: TypeScript strict mode, tipos gerados do Supabase com `supabase gen types`

### 13.2. Estratégia Mobile (Futuro)

**Lucas Ferreira (Mobile):**
Para o futuro (Fase 2-3), proponho:

1. **React Native + Expo** (managed workflow)
2. **Monorepo** com Turborepo: apps/web + apps/mobile + packages/shared
3. **Compartilhamento via pacote shared**: tipos, validadores Zod, constantes de domínio, funções de cálculo financeiro
4. **Não compartilhar**: componentes UI (web usa Tailwind, mobile usa StyleSheet/NativeWind), navegação, storage local
5. **NativeWind** (Tailwind para React Native) para manter tokens visuais consistentes
6. **Expo Router** para navegação file-based similar ao Next.js

**Beatriz Rocha (Mobile):**
Complementando: no mobile, a primeira tela será essencialmente a mesma "tela de ação" da web, mas otimizada para touch. Swipe para marcar como pago, notificações push para vencimentos. Mas isso é Fase 2+.

**Sofia Almeida (Frontend):**
O monorepo com Turborepo é a melhor aposta. O pacote `shared` pode ter testes próprios que validam as regras de domínio — essas mesmas regras valem para web e mobile.

### 13.3. Decisão Final

Aprovada estrutura monorepo com Turborepo (apps/web + packages/shared). Mobile será apps/mobile quando chegar a hora. Pacote shared para tipos, validação e cálculos. NativeWind como ponte de tokens visuais entre plataformas.

---

## 14. Design System — Escolha e Justificativa

### 14.1. Análise de Opções

**Carlos Mendes (Designer de Software):**
Analisei as principais opções de design system compatíveis com React + Next.js + Tailwind:

| Design System     | Base                     | Compatível com Tailwind?                   | Mobile path?            | Adequação financeira   | Maturidade    |
| ----------------- | ------------------------ | ------------------------------------------ | ----------------------- | ---------------------- | ------------- |
| **shadcn/ui**     | Radix UI + Tailwind      | ✅ Nativo                                  | Via NativeWind adaptado | ✅ Alta (clean, dados) | ✅ Alta       |
| Material UI (MUI) | Material Design (Google) | ⚠️ Pode conflitar com Tailwind (CSS-in-JS) | React Native Paper      | ✅ Alta                | ✅ Muito alta |
| Ant Design        | Ant Design (Alibaba)     | ⚠️ CSS próprio, conflito com Tailwind      | Ant Design Mobile       | ✅ Alta (enterprise)   | ✅ Muito alta |
| Chakra UI         | Estilo próprio           | ⚠️ Theme system próprio                    | Não oficial             | ⚠️ Média               | ✅ Alta       |
| Mantine           | Estilo próprio           | ⚠️ CSS modules, parcialmente               | Não oficial             | ✅ Alta                | ✅ Alta       |

### 14.2. Recomendação da Equipe: shadcn/ui

**Carlos Mendes:**
Recomendo **shadcn/ui** como base do design system. Justificativas:

**Isabella Torres (UI Designer):**
Concordo fortemente. shadcn/ui não é uma biblioteca — é uma **coleção de componentes copiáveis** que usam Radix UI (acessibilidade) + Tailwind (estilização). Isso significa:

1. **Controle total**: Os componentes ficam no nosso código, não numa dependência externa. Podemos customizar livremente.
2. **Tailwind nativo**: Zero conflito com Tailwind. OS componentes SÃO Tailwind.
3. **Acessibilidade incluída**: Radix UI é a melhor base de acessibilidade do ecossistema React.
4. **Estilo visual limpo**: Perfeito para dados financeiros — sem ruído visual, hierarquia clara.
5. **Composabilidade**: Componentes primitivos que combinamos conforme necessário.

**Helena Vargas (UX):**
Do ponto de vista de UX para produto financeiro:

- Dados financeiros precisam de **clareza hierárquica** — shadcn/ui é minimalista e denso, perfeito
- Tabelas, cards, badges, inputs — tudo que precisamos está lá
- O sistema de **dark mode** é gratuito com CSS variables
- A estética é profissional e sóbria, adequada a dinheiro

**Roberto Lima (Frontend):**
Tecnicamente, shadcn/ui se encaixa perfeitamente:

- Next.js App Router: ✅ (Server Components compatíveis)
- Tailwind: ✅ (é a base)
- TypeScript: ✅ (tipagem completa)
- Formulários: ✅ (integra com React Hook Form)
- Data tables: ✅ (componente de tabela com Tanstack Table)
- CLI de scaffold: `npx shadcn-ui@latest add button` — adiciona componentes sob demanda

**Renata Silva (QA Visual/A11y):**
A base Radix UI garante:

- Navegação por teclado
- ARIA labels
- Foco corretamente gerenciado
- Contraste adequado (configurável via tokens)
- Compatibilidade com screen readers

Isso nos poupa meses de trabalho em acessibilidade.

### 14.3. Por que NÃO Material UI (MUI)

**Carlos Mendes:**
O CEO mencionou Material Design. Reconhecemos que é um sistema maduro e excelente, mas para ESTE stack:

1. **Conflito com Tailwind**: MUI usa Emotion (CSS-in-JS). Ter dois sistemas de estilização (Emotion + Tailwind) gera conflito, complexidade e bundle maior.
2. **Overhead**: MUI é grande e opinado. Para um MVP, é excesso.
3. **Lock-in**: Com MUI, ficamos presos ao tema Material. Com shadcn/ui, podemos criar um visual inspirado em Material SEM a dependência.
4. **Mobile**: React Native Paper (Material para RN) existe, mas não compartilha código com MUI web.

**Thiago Martins (Front Engineer):**
Na prática, podemos pegar princípios do Material Design (elevação, hierarquia, density, motion) e aplicar via Tailwind nos componentes shadcn/ui. O resultado visual será similar, mas sem o custo técnico.

### 14.4. Tokens Visuais e Design Decisions

**Isabella Torres (UI Designer):**
Proponho os seguintes tokens visuais base:

#### Paleta de Cores (CSS Variables — Tailwind)

```
--primary: Azul sóbrio (#2563eb → blue-600) — confiança, finanças
--primary-foreground: Branco
--destructive: Vermelho (#dc2626 → red-600) — alertas, atrasos
--success: Verde (#16a34a → green-600) — pagamentos confirmados, saldo positivo
--warning: Âmbar (#d97706 → amber-600) — vencendo em breve
--muted: Cinza (#6b7280 → gray-500) — previstos, inativos
--background: Branco (#ffffff)
--card: Off-white (#f9fafb → gray-50)
--border: Cinza claro (#e5e7eb → gray-200)
```

#### Tipografia

- **Sans-serif**: Inter (padrão do shadcn/ui) — boa legibilidade em números
- **Monospace**: JetBrains Mono — para valores financeiros e tabelas numéricas
- **Tamanhos**: sm (12px), base (14px), lg (16px), xl (18px), 2xl (24px), 3xl (30px)

#### Espaçamento (Tailwind scale)

- Density alta para tabelas e listas financeiras (p-2, gap-2)
- Density normal para formulários e cards (p-4, gap-4)
- Density baixa para telas de destaque (p-6, gap-6)

#### Componentes-chave do domínio financeiro

- **TransactionRow**: Linha de transação com ícone de tipo, valor, categoria, tags, status
- **PriorityBadge**: Badge colorido com nível de prioridade
- **PeriodSelector**: Seletor de período financeiro (civil / personalizado / livre)
- **LiabilityCard**: Card de dívida com barra de progresso de amortização
- **StatementSummary**: Resumo da fatura com breakdown de valores
- **BalanceDisplay**: Saldo com indicador positivo/negativo
- **DueDateIndicator**: Indicador visual de proximidade de vencimento

### 14.5. Prós e Contras da Escolha

**Prós:**

- Aderência total ao stack (Tailwind + Next.js + React)
- Componentes customizáveis e sob controle do projeto
- Acessibilidade de base sólida (Radix UI)
- Comunidade ativa e crescente
- Sem lock-in em design system externo
- Bundle pequeno (adiciona apenas o que usa)
- Dark mode gratuito via CSS variables
- Path para mobile via NativeWind + componentes análogos

**Contras:**

- Não é um design system "completo" — precisamos compor nossos componentes de domínio → **Mitigação**: isso é feature, não bug — adaptamos ao domínio financeiro
- Menos maduro que MUI ou Ant Design → **Mitigação**: Radix UI (a base) é extremamente estável
- Requer mais decisões de design upfront → **Mitigação**: Isabella e Carlos definem tokens e guidelines no início

### 14.6. Decisão Final

**Chico (CEO):**
Aceito a recomendação de shadcn/ui. A justificativa técnica é sólida — manter dois sistemas de estilização (MUI + Tailwind) seria contraproducente. A estética Material pode ser inspiração para nossos tokens sem o overhead da biblioteca. Aprovado.

**Decisão**: shadcn/ui (Radix UI + Tailwind) como base. Paleta azul financeira. Inter + JetBrains Mono. Componentes de domínio financeiro customizados sobre a base. Dark mode planejado. Guidelines de acessibilidade obrigatórios.

---

## 15. Documentos, Segredos e Backup

### 15.1. Estratégia de Documentos e Anexos

**Pedro Santos (Backend Python):**
Proponho:

1. **Armazenamento**: Supabase Storage, bucket `user-documents` com política RLS por user_id
2. **Organização**: path no storage: `{user_id}/{entity_type}/{entity_id}/{filename}`
3. **Tipos aceitos**: PDF, PNG, JPG, JPEG, XLSX, CSV, DOC, DOCX
4. **Limite de tamanho**: 10MB por arquivo (configurável)
5. **Versionamento**: campo `version` na tabela `documents` — nova versão = novo upload com version incrementado
6. **Vinculação**: polimórfica via entity_type + entity_id na tabela documents

### 15.2. Tratamento de PDFs Protegidos

**Maria Oliveira (Backend):**

1. Se o documento é marcado como `is_password_protected = true`, a senha é armazenada em `user_secrets`
2. Acesso à senha: APENAS via Edge Function server-side
3. No MVP, a senha serve apenas para referência do usuário ("qual é a senha desse PDF?")
4. Na Fase 3-4, a Edge Function usará a senha para abrir e extrair dados do PDF

### 15.3. Estratégia de Segredos

**Fernando Gomes (DevOps):**

1. Tabela `user_secrets` com RLS restritivo: o próprio client NÃO pode ler diretamente
2. Acesso via Edge Function com service_role_key
3. Valores encriptados com AES-256-GCM antes de armazenar
4. Chave de encriptação: variável de ambiente no Supabase (não no código)
5. Edge Function valida JWT → decripta → retorna valor → loga acesso no audit_log

### 15.4. Estratégia de Backup

**Fernando Gomes:**

1. **Backup de banco**: Supabase faz backups automáticos diários (Pro plan). No free tier, backup manual via `pg_dump` periódico
2. **Backup de Storage**: Download periódico dos arquivos via Supabase CLI
3. **Backup local**: Script Bun que exporta banco + arquivos para pasta local compactada, versionada por data
4. **Frequência sugerida**: Semanal no início, diário quando em uso intenso
5. **Exportação do sistema**: Funcionalidade futura de "Exportar todos os meus dados" (LGPD-ready)

### 15.5. Decisão Final

Aprovado: Supabase Storage com RLS. Segredos via Edge Function + AES-256-GCM. Backup manual via pg_dump + script Bun. Exportação planejada para Fase 2.

---

## 16. Primeira Tela — Dashboard de Ação e Decisão

### 16.1. Conceito

**Helena Vargas (UX):**
A primeira tela NÃO é um dashboard passivo. É uma **tela de comando operacional**. O usuário abre o sistema e em 5 segundos sabe: o que pagar, o que está atrasado, quanto tem, quanto precisa durar.

**Camila Duarte:**
Na minha experiência, a maioria dos apps financeiros falha aqui. Mostram gráficos bonitos mas o usuário não sabe "e agora, pago o quê?". Nossa tela responde essa pergunta.

### 16.2. Layout Proposto

**Isabella Torres (UI Designer):**

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
│  │ 🔴 Condomínio — ATRASADO (venceu 15/Mar) — R$ 850      │
│  │ 🟡 Fatura Nubank — vence em 3 dias — R$ 1.200           │
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  📋 PRÓXIMOS PAGAMENTOS (por prioridade) ──────────────     │
│  │ 🔴 [essencial] Diarista — 25/Mar — R$ 200    [Pagar]   │
│  │ 🟠 [alta]      Internet — 28/Mar — R$ 120    [Pagar]   │
│  │ 🟡 [média]     Academia — 01/Abr — R$ 90     [Pagar]   │
│  │ 🔵 [baixa]     Spotify  — 05/Abr — R$ 34     [Pagar]   │
│  │ ⚪ [opcional]   Curso XY — 10/Abr — R$ 200    [Adiar]  │
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  💳 FATURAS ─────────────────────────────────────────────    │
│  │ Nubank (****1234) — Fatura Mar — R$ 1.200 — vence 23/Mar│
│  │ C6 (****5678)     — Fatura Mar — R$ 450   — vence 28/Mar│
│  └──────────────────────────────────────────────────────     │
│                                                              │
│  📊 DÍVIDAS ──────────────────────────────────────────────   │
│  │ Financiamento Apt — Parcela 24/120 — R$ 1.800 — 10/Abr │
│  │   Saldo devedor: R$ 180.000 | Amort. acum: R$ 42.000   │
│  │ Empréstimo Pessoal — Parcela 8/24 — R$ 650 — 15/Abr    │
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

### 16.3. Regras da Primeira Tela

**Roberto Lima (Frontend):**

1. Ordenação: Atrasados primeiro, depois por prioridade (essencial → alta → média → baixa → opcional), dentro da mesma prioridade por data de vencimento
2. Cores: 🔴 Essencial/Atrasado, 🟠 Alta, 🟡 Média, 🔵 Baixa, ⚪ Opcional
3. Dados são do período financeiro ATUAL (não do mês civil)
4. Seção "Atenção" aparece apenas se há itens atrasados ou vencendo em 3 dias
5. Seção "Sobrevivência" calcula: saldo - obrigações pendentes do período = margem livre
6. Ações rápidas: "Pagar" (marca como confirmado), "Adiar" (move para próximo período), "Detalhes"

**Camila Duarte:**
Essa tela resolve o problema real: "Abri o app — e agora?". A resposta é imediata. Perfeito.

### 16.4. Decisão Final

Aprovado o conceito de tela de ação. Layout será refinado por Isabella e Helena na prototipagem. Regras de ordenação e cálculo de sobrevivência como especificados.

---

## 17. Estratégia de Testes e Cenários de Aceitação

### 17.1. Princípio Geral

**Maria Oliveira (Backend):**
Testes são **especificação viva**, não validação posterior. Eles definem o contrato do sistema. A implementação segue os testes. Se um teste quebra por mudança de implementação (e não de regra), a implementação está errada, não o teste.

**Ana Silva (Arquiteta):**
Testes só podem ser alterados quando:

- Regra de negócio muda legitimamente
- Escopo muda explicitamente
- Entendimento do domínio é corrigido
- Refinamento funcional é aprovado
- Simplificação intencional do MVP com justificativa

### 17.2. Estratégia por Camada

| Camada                    | O que testar                                                                                  | Ferramenta sugerida              | Prioridade      |
| ------------------------- | --------------------------------------------------------------------------------------------- | -------------------------------- | --------------- |
| **Domínio / Unidade**     | Cálculos financeiros, resolução de prioridade, atribuição de período, regras de negócio puras | Vitest (+ Bun para execução)     | 🔴 Crítica      |
| **Integração**            | Interação entre serviços e o banco (Supabase), triggers, RLS                                  | Vitest + Supabase local (Docker) | 🔴 Crítica      |
| **API / Edge Functions**  | Endpoints, importação, segredos, geração de períodos                                          | Vitest + supertest ou fetch      | 🟠 Alta         |
| **Fluxos críticos (E2E)** | Onboarding, lançamento, pagamento de fatura, importação                                       | Playwright                       | 🟡 Média (MVP+) |
| **Interface**             | Componentes de domínio financeiro, responsividade                                             | Vitest + Testing Library         | 🟡 Média        |
| **Regressão**             | Regras críticas que não podem quebrar nunca                                                   | Vitest (suíte dedicada)          | 🔴 Crítica      |

### 17.3. Organização da Suíte de Testes

```
packages/
  shared/
    __tests__/
      domain/
        financial-period.test.ts      # Cálculos de período
        priority-resolution.test.ts   # Resolução de prioridade
        liability-calculation.test.ts # Amortização, juros, quitação
        transaction-rules.test.ts     # Regras de classificação
        recurring-generation.test.ts  # Geração de instâncias

apps/
  web/
    __tests__/
      integration/
        transactions.test.ts          # CRUD + RLS
        transfers.test.ts             # Transferências internas
        statements.test.ts            # Faturas e pagamentos
        imports.test.ts               # Importação sem duplicação
      api/
        edge-functions.test.ts        # Edge Functions
      components/
        transaction-form.test.tsx     # Formulário de lançamento
        priority-badge.test.tsx       # Badge de prioridade
        dashboard.test.tsx            # Primeira tela
      e2e/
        onboarding.spec.ts            # Fluxo completo de setup
        transaction-flow.spec.ts      # Fluxo de lançamento
        import-flow.spec.ts           # Fluxo de importação
```

### 17.4. Testes Mandatórios do MVP — 17 Regras Críticas

**Verônica (Consultora):**
Cada regra do documento de referência DEVE ter pelo menos um teste. Vou especificar cenários de aceitação para cada uma:

---

#### T1. Pagamento de fatura NÃO gera nova despesa

**Cenário de aceitação:**

```
DADO que existe uma fatura do Nubank com total R$ 1.200
E o saldo da conta corrente Caixa é R$ 5.000
QUANDO o usuário registra pagamento da fatura via conta Caixa
ENTÃO o saldo da Caixa diminui para R$ 3.800
E o status da fatura muda para "paid"
E NÃO é criada nenhuma transação do tipo "expense"
E o relatório de despesas do período NÃO inclui R$ 1.200 como nova despesa
E as despesas reais são apenas os itens da fatura (compras anteriores)
```

**Caso-limite:**

- Pagamento parcial da fatura (R$ 800 de R$ 1.200) → status "partial", saldo diminui R$ 800
- Pagamento de fatura com cartão adicional → mesma regra

---

#### T2. Transferência entre contas próprias NÃO é contabilizada como gasto

**Cenário de aceitação:**

```
DADO que a conta Caixa tem saldo R$ 3.000
E a conta Nubank tem saldo R$ 500
QUANDO o usuário transfere R$ 1.000 da Caixa para Nubank
ENTÃO o saldo da Caixa é R$ 2.000
E o saldo do Nubank é R$ 1.500
E o total patrimonial permanece R$ 3.500
E a transferência NÃO aparece no relatório de despesas
E a transferência NÃO aparece no relatório de receitas
E a transferência aparece apenas no extrato de movimentações
```

**Caso-limite:**

- Transferência entre contas da mesma instituição → mesma regra
- Transferência entre contas de instituições diferentes → mesma regra

---

#### T3. Lançamentos atribuídos corretamente ao período financeiro personalizado

**Cenário de aceitação:**

```
DADO que o ciclo financeiro do usuário começa dia 20
E existe o período "20/Mar - 19/Abr"
QUANDO o usuário lança uma despesa com event_date = 22/Mar
ENTÃO a transação é atribuída ao período "20/Mar - 19/Abr"
E aparece no relatório do período "20/Mar - 19/Abr"
E NÃO aparece no relatório do período "20/Fev - 19/Mar"
```

**Caso-limite:**

- Despesa no dia 19 (último dia do período anterior) → atribuída ao período anterior
- Despesa no dia 20 (primeiro dia do novo período) → atribuída ao novo período
- Despesa importada sem competence_date → usa event_date para derivar período

---

#### T4. Ciclo financeiro respeita data de início e fim configuradas

**Cenário de aceitação:**

```
DADO que o usuário configura ciclo financeiro com início dia 15
QUANDO o sistema gera períodos financeiros
ENTÃO o período de Março é "15/Mar - 14/Abr"
E o período de Abril é "15/Abr - 14/Mai"
E cada período tem exatamente os dias entre início e fim
E nenhum dia fica fora de algum período
```

**Caso-limite:**

- Mês com 28 dias (fevereiro) → período ajustado corretamente
- Início dia 31 → para meses com menos de 31 dias, usa último dia do mês
- Mudança de configuração de ciclo → períodos futuros mudam, anteriores permanecem

---

#### T5. Compras em cartão respeitam ciclo de fechamento e vencimento

**Cenário de aceitação:**

```
DADO que o cartão Nubank fecha dia 15 e vence dia 23
E a fatura atual cobre compras de 16/Fev a 15/Mar
QUANDO o usuário registra compra de R$ 100 no dia 14/Mar (antes do fechamento)
ENTÃO a compra entra na fatura de Mar (fencha 15/Mar, vence 23/Mar)
QUANDO o usuário registra compra de R$ 200 no dia 16/Mar (após fechamento)
ENTÃO a compra entra na fatura de Abr (fecha 15/Abr, vence 23/Abr)
```

**Caso-limite:**

- Compra exatamente no dia do fechamento → entra na fatura atual
- Compra no dia seguinte ao fechamento → entra na próxima fatura

---

#### T6. Empréstimos e financiamentos separam amortização, juros e encargos

**Cenário de aceitação:**

```
DADO que existe um empréstimo com parcela de R$ 1.000
E a composição é: amortização R$ 650, juros R$ 280, seguro R$ 50, taxa R$ 20
QUANDO o usuário registra pagamento da parcela
ENTÃO o relatório mostra a decomposição completa
E o saldo devedor reduz em R$ 650 (apenas amortização)
E o relatório de "juros pagos" soma R$ 280
E o relatório de "amortização acumulada" soma R$ 650
```

**Caso-limite:**

- Parcela sem decomposição (apenas total_amount) → aceita, decomposição como null
- Soma dos componentes ≠ total_amount (arredondamento bancário) → total_amount prevalece

---

#### T7. Recorrências geram EXPECTATIVA, não pagamento automático

**Cenário de aceitação:**

```
DADO que existe um template recorrente "Internet Vivo — R$ 120 — dia 15 — mensal"
QUANDO o sistema gera instâncias para os próximos 3 meses
ENTÃO são criadas 3 instâncias com status "pending"
E nenhuma transação de tipo "expense" é criada automaticamente
E as instâncias aparecem na primeira tela como "a pagar"
E somente quando o usuário clica "Pagar" é que uma transaction é criada
E a instância muda status para "paid" e vincula à transaction
```

**Caso-limite:**

- Template com valor variável → instância criada com expected_amount = null, usuário informa na confirmação
- Template desativado → não gera novas instâncias, existentes permanecem

---

#### T8. Estornos e ajustes NÃO distorcem receita ou despesa

**Cenário de aceitação:**

```
DADO que existe uma despesa de R$ 500 (compra de produto)
E o total de despesas do período é R$ 3.000
QUANDO o usuário registra estorno de R$ 500 (devolução do produto)
ENTÃO o total de despesas do período é R$ 2.500 (reduzido pelo estorno)
E o estorno NÃO aparece como receita
E o relatório de receitas permanece inalterado
```

**Caso-limite:**

- Estorno parcial (R$ 200 de R$ 500) → despesa líquida = R$ 300
- Ajuste positivo → não é receita, é correção — aparece separadamente

---

#### T9. Importações NÃO duplicam registros sem controle

**Cenário de aceitação:**

```
DADO que o usuário importa um arquivo CSV com 100 linhas
E a importação é bem-sucedida (100 registros criados)
QUANDO o usuário importa o mesmo arquivo novamente
ENTÃO o sistema detecta linhas duplicadas
E importa apenas linhas novas (0 neste caso)
E o relatório mostra: 0 importados, 100 ignorados (duplicados)
E o import_job registra o resultado
```

**Caso-limite:**

- Arquivo com 50 linhas novas e 50 repetidas → importa 50, ignora 50
- Detecção de duplicação: hash de (data + valor + descrição) ou ID externo quando disponível

---

#### T10. Documentos anexados NÃO alteram saldo automaticamente

**Cenário de aceitação:**

```
DADO que uma transação de R$ 500 existe
E o saldo é R$ 3.000
QUANDO o usuário anexa um comprovante PDF à transação
ENTÃO o saldo permanece R$ 3.000
E a transação permanece com valor R$ 500
E o documento é vinculado mas não altera dados financeiros
```

---

#### T11. Quitação antecipada de dívida recalcula saldo e impacto

**Cenário de aceitação:**

```
DADO que existe um empréstimo com saldo devedor R$ 20.000
E taxa de juros 1,5% a.m.
E faltam 24 parcelas
QUANDO o sistema calcula impacto de pagamento extra de R$ 5.000
ENTÃO mostra: novo saldo devedor R$ 15.000
E mostra: parcelas eliminadas (aproximadamente X)
E mostra: economia em juros (aproximadamente R$ Y)
E os valores são financeiramente coerentes com a taxa informada
```

---

#### T12. Relatórios por mês civil e por período financeiro podem divergir

**Cenário de aceitação:**

```
DADO que o ciclo começa dia 20
E existe despesa de R$ 100 no dia 18/Mar (período 20/Fev-19/Mar)
E existe despesa de R$ 200 no dia 22/Mar (período 20/Mar-19/Abr)
E existe despesa de R$ 300 no dia 25/Mar (período 20/Mar-19/Abr)
QUANDO o relatório por mês civil (Março) é gerado
ENTÃO mostra total: R$ 600 (todas as despesas de março civil)
QUANDO o relatório por período "20/Mar-19/Abr" é gerado
ENTÃO mostra total: R$ 500 (apenas as de 22/Mar e 25/Mar)
E os dois relatórios divergem corretamente
```

---

#### T13. Múltiplas tags sem perda de integridade

**Cenário de aceitação:**

```
DADO que existem tags: "essencial", "casa", "trabalho_externo"
QUANDO o usuário cria transação com as 3 tags
ENTÃO a transação tem 3 registros na tabela transaction_tags
E remover a tag "trabalho_externo" mantém as outras 2
E a transação aparece em filtros de cada uma das 3 tags
E a transação NÃO aparece duplicada em nenhum relatório
```

---

#### T14. Filtros por tag funcionam corretamente

**Cenário de aceitação:**

```
DADO que existem 3 transações:
  - T1: tags ["essencial", "casa"] — R$ 500
  - T2: tags ["essencial", "trabalho"] — R$ 300
  - T3: tags ["diversão"] — R$ 200
QUANDO filtro por tag "essencial"
ENTÃO retorna T1 e T2 (total R$ 800)
QUANDO filtro por tag "casa"
ENTÃO retorna apenas T1 (total R$ 500)
QUANDO filtro por tags "essencial" E "casa" (interseção)
ENTÃO retorna apenas T1
```

---

#### T15. Prioridades influenciam ordenação e alertas

**Cenário de aceitação:**

```
DADO que existem 3 itens pendentes no período atual:
  - Item A: prioridade "optional", vence 25/Mar
  - Item B: prioridade "essential", vence 28/Mar
  - Item C: prioridade "high", vence 22/Mar
QUANDO a primeira tela é renderizada
ENTÃO a ordenação é: Item C (high, vence primeiro), Item B (essential), Item A (optional)
E Item B tem destaque visual de "essencial"
E se Item C está a 3 dias do vencimento, aparece alerta
```

Nota: A ordenação combina prioridade + data. Itens atrasados sempre primeiro, independente de prioridade.

---

#### T16. Itens essenciais NÃO são tratados como postergáveis

**Cenário de aceitação:**

```
DADO que uma despesa tem prioridade "essential"
E tem tag "essencial" com influences_priority = true
QUANDO o sistema calcula a margem livre do período
ENTÃO a despesa essencial é incluída como obrigação firme
E NÃO aparece na lista de "pode ser postergado"
E se o saldo não cobre as obrigações essenciais, gera alerta
```

---

#### T17. Primeira tela reflete vencimento + prioridade + período financeiro

**Cenário de aceitação:**

```
DADO que o período financeiro atual é 20/Mar - 19/Abr
E existem obrigações em diferentes datas e prioridades
QUANDO o usuário abre a primeira tela
ENTÃO vê apenas obrigações do período 20/Mar - 19/Abr
E a seção "Atenção" mostra itens atrasados e próximos do vencimento
E a seção "Próximos pagamentos" está ordenada por prioridade + data
E a seção "Sobrevivência" mostra cálculo correto de margem
E itens de outros períodos NÃO aparecem na listagem principal
```

---

### 17.5. Decisão Final sobre Testes

A equipe aprova:

- **Vitest** como runner principal (compatível com Bun e TypeScript)
- **Playwright** para E2E (Fase 2+, não bloqueia MVP)
- **Suíte de regressão** separada com os 17 testes mandatórios
- **Testes de domínio no pacote shared** (reutilizáveis web + mobile)
- **Cobertura mínima**: 100% dos 17 cenários obrigatórios antes do deploy
- Testes como **contrato** — não alteráveis por conveniência

---

## 18. Riscos e Dúvidas em Aberto

### 18.1. Riscos Identificados

| #   | Risco                                                      | Probabilidade | Impacto | Mitigação                                                                                               |
| --- | ---------------------------------------------------------- | ------------- | ------- | ------------------------------------------------------------------------------------------------------- |
| 1   | Complexidade do domínio atrasa MVP                         | Alta          | Alto    | Implementação incremental por módulo. Começar por: instituições + transações + categorias/tags          |
| 2   | Free tier do Supabase insuficiente para dados históricos   | Média         | Médio   | Monitorar uso. 500MB é suficiente para ~2 anos de uso pessoal. Upgrade para Pro ($25/mês) se necessário |
| 3   | Modelagem de dívida muito complexa para uso diário         | Média         | Médio   | Permitir parcela simplificada (apenas total). Decomposição é opcional                                   |
| 4   | Importação de planilha com formatos inconsistentes         | Alta          | Médio   | Parser robusto com preview + mapeamento manual de colunas. Aceitar imperfeição                          |
| 5   | Ciclo financeiro personalizado causa confusão na interface | Média         | Alto    | Sempre mostrar ambas as visões (civil + personalizado). Seletor claro de período                        |
| 6   | Scope creep antes do MVP                                   | Alta          | Alto    | Escopo documentado nesta ata. CEO tem veto. Cada feature nova exige justificativa                       |
| 7   | Performance de queries com muitas tabelas e joins          | Baixa         | Médio   | Views materializadas para relatórios. Índices planejados. Supabase é Postgres — escala bem              |
| 8   | Edge Functions do Supabase com limitações de runtime       | Baixa         | Médio   | Lógica crítica está no Postgres (triggers, functions). Edge Functions para orquestração                 |

### 18.2. Dúvidas em Aberto

| #   | Dúvida                                                                | Decisão pendente                                                       | Responsável      |
| --- | --------------------------------------------------------------------- | ---------------------------------------------------------------------- | ---------------- |
| 1   | Detecção de duplicação na importação: hash simples ou matching fuzzy? | Iniciar com hash (data + valor + descrição). Fuzzy na Fase 2           | Pedro Santos     |
| 2   | Como lidar com investimentos (CDB, Tesouro, fundos)?                  | MVP: saldo estático manual. Modelagem completa na Fase 2               | Ricardo Monteiro |
| 3   | Geração de períodos financeiros: quantos meses para frente?           | 6 meses (3 meses muito pouco para planejamento)                        | Maria Oliveira   |
| 4   | Dark mode desde o MVP?                                                | Sim — shadcn/ui suporta nativamente com custo zero                     | Isabella Torres  |
| 5   | Internacionalização (i18n)?                                           | Apenas pt-BR no MVP. Estrutura preparada mas sem implementação         | Roberto Lima     |
| 6   | Como tratar cartão adicional compartilhado com familiar?              | MVP: registro como card_holder adicional. Fatura única. Sem multi-user | João Pereira     |
| 7   | Estratégia de seed de categorias: pré-definidas ou personalizáveis?   | Ambas: seed com categorias padrão + possibilidade de criar novas       | Maria Oliveira   |

---

## 19. Roadmap Priorizado

### Fase 1: MVP sem IA

**Objetivo**: Sistema funcional de controle financeiro pessoal com domínio correto.

| Módulo         | Entregáveis                                                 | Prioridade interna |
| -------------- | ----------------------------------------------------------- | ------------------ |
| Core           | Auth, setup de ciclo financeiro, instituições, produtos     | 🔴 P0              |
| Transações     | Lançamento manual, categorias, tags, prioridade             | 🔴 P0              |
| Transferências | Registro de transferências internas (sem nova despesa)      | 🔴 P0              |
| Cartões        | Cadastro de cartões, ciclos de fatura, itens de fatura      | 🔴 P0              |
| Faturas        | Pagamento de fatura (sem gerar despesa)                     | 🔴 P0              |
| Dívidas        | Cadastro de passivos, parcelas, decomposição                | 🟠 P1              |
| Recorrências   | Templates + instâncias + geração automática                 | 🟠 P1              |
| Dashboard      | Primeira tela de ação e decisão                             | 🔴 P0              |
| Relatórios     | Por mês civil, por período personalizado, por categoria/tag | 🟠 P1              |
| Importação     | Upload de CSV/XLSX com preview e controle de duplicação     | 🟡 P2              |
| Documentos     | Upload e vinculação de anexos                               | 🟡 P2              |
| Testes         | 17 cenários mandatórios + domínio + integração              | 🔴 P0              |

### Fase 2: Importação e Automação Básica

- Templates avançados de recorrência
- Regras automáticas de categorização (baseadas em padrões, sem IA)
- Importação inteligente com matching de colunas
- Notificações por email (vencimentos próximos)
- Relatórios exportáveis (PDF/XLS)
- Backup automatizado local
- Início do desenvolvimento mobile (React Native + Expo)

### Fase 3: Documentos e Leitura Assistida

- Organização avançada de documentos
- Classificação de tipo de documento
- Versionamento completo
- Reader de PDF server-side (Edge Function)
- Extração básica de dados de comprovantes (sem IA, regex patterns)
- Mobile funcional com paridade de features essenciais

### Fase 4: IA

- Classificação automática de transações (NLP/ML)
- Leitura inteligente de PDFs e comprovantes (OCR + LLM)
- Conciliação assistida
- Resumo mensal em linguagem natural
- Assistente conversacional
- Sugestão automática de tags e prioridade
- Detecção de anomalias financeiras

### Fase 5: MCP/Agentes

- Exposição de ferramentas via Model Context Protocol
- Integração com agentes autônomos
- Consultas em linguagem natural ao banco de dados
- Automações orientadas por agente
- Planejamento financeiro assistido

---

## 20. Recomendações Finais da Equipe

### Ana Silva (Arquiteta):

> Mantenham o domínio simples e correto. A tentação de adicionar features será constante. Resistam. O MVP é sobre ter dados confiáveis, não sobre ter tudo bonito. Uma base sólida permite qualquer evolução futura sem refatorar.

### Ricardo Monteiro (Economista):

> Nunca economizem na modelagem de dívida. A diferença entre "paguei R$ 1.000" e "paguei R$ 650 de amortização + R$ 280 de juros + R$ 50 de seguro + R$ 20 de taxa" é a diferença entre ignorância e controle. Mesmo que o usuário não preencha tudo no início, a estrutura precisa estar lá.

### Camila Duarte (Consultora):

> A primeira tela é o produto. Se ela não responder "o que eu pago agora?", o sistema vai ser abandonado como mais uma planilha que ninguém abre. Prioridade não é cosmética — é o core da experiência.

### André Santos (DBA):

> Gerem as migrações com cuidado. Cada mudança no schema é uma migração versionada. Usem Supabase CLI para isso. E por favor — não façam ALTER TABLE em produção sem testar localmente primeiro.

### Fernando Gomes (DevOps):

> CI/CD desde o dia 1. Lint + testes + deploy automático. Se não automatizar agora, nunca vai automatizar. GitHub Actions é gratuito para repos privados até 2.000 minutos/mês.

### Helena Vargas (UX):

> Testem com dados reais o mais cedo possível. Mock data não revela os problemas reais de UX. Importem a planilha do CEO e vejam se a interface faz sentido com dados de verdade.

### Thiago Martins (Front Engineer):

> Componentizem desde o início. TransactionRow, PriorityBadge, BalanceDisplay — cada um é um componente isolado, testável, reutilizável. Isso economiza semanas quando o mobile chegar.

### Renata Silva (QA A11y):

> Acessibilidade não é fase 2. Cada componente nasce acessível. Radix UI nos dá a base — usem. Testem com teclado. Testem com screen reader. Se um usuário com deficiência visual pode usar o produto, todos podem.

### Maria Oliveira (Backend):

> Escrevam os 17 testes mandatórios ANTES de qualquer feature. Eles são o contrato. Se a implementação não passa nos testes, a implementação está errada. Simples assim.

### Roberto Lima (Frontend):

> Server Components do Next.js são poderosos — usem. Menos JavaScript no client = mais rápido. Client Components apenas para formulários e interações. Isso muda a forma de pensar o frontend.

---

## Encerramento

**Chico (CEO):**
Excelente trabalho, equipe. Temos agora uma base sólida para começar. O resumo é:

1. **Domínio correto e rigoroso** — transferências ≠ despesas, faturas ≠ novas despesas, parcelas decompostas
2. **Modelo de dados robusto** com ~20 tabelas planejadas, RLS mandatório, auditoria
3. **Três dimensões temporais** convivendo sem ambiguidade: mês civil, ciclo financeiro, ciclo de fatura
4. **Categorias + tags + prioridade** como cidadãos de primeira classe do domínio
5. **Arquitetura serverless** com Supabase + Vercel a custo zero
6. **shadcn/ui** como base do design system — Tailwind nativo, Radix para a11y, sem conflitos
7. **Primeira tela de ação**, não dashboard passivo
8. **17 testes mandatórios** como contrato do sistema
9. **Roadmap de 5 fases** — do MVP determinístico até MCP/agentes com IA

Próximo passo: iniciar implementação da Fase 1 (MVP) seguindo este documento como referência.

**Verônica (Consultora):**
O documento está completo e reflete o que foi discutido. Recomendo que este refino seja revisitado a cada sprint para ajustes baseados em aprendizados da implementação. Bom trabalho.

---

**Ações / Responsáveis / Prazo:**

- Criar projeto Next.js com estrutura proposta — Roberto Lima / Thiago Martins — Sprint 1
- Criar schema de banco completo com migrações — André Santos / João Pereira — Sprint 1
- Configurar Supabase (Auth + RLS + Storage) — Fernando Gomes / João Pereira — Sprint 1
- Implementar tokens visuais e componentes base (shadcn/ui) — Isabella Torres / Carlos Mendes — Sprint 1
- Escrever os 17 testes mandatórios como suíte de regressão — Maria Oliveira / Pedro Santos — Sprint 1
- Implementar CRUD de instituições e produtos — João Pereira — Sprint 1
- Implementar transações com categorias e tags — Maria Oliveira — Sprint 1-2
- Implementar transferências internas — Maria Oliveira — Sprint 2
- Implementar cartões e faturas — João Pereira — Sprint 2
- Implementar recorrências (templates + instâncias) — Maria Oliveira — Sprint 2-3
- Implementar dívidas e decomposição de parcela — Ricardo Monteiro (regras) / João Pereira (backend) — Sprint 3
- Implementar primeira tela (dashboard de ação) — Roberto Lima / Helena Vargas — Sprint 2-3
- Implementar importação de planilha — Pedro Santos / Laura Costa — Sprint 3-4
- Configurar CI/CD (GitHub Actions) — Fernando Gomes — Sprint 1
- Prototipar wireframes da primeira tela — Helena Vargas / Isabella Torres — Sprint 1
