---
Título da Reunião: Checkpoint de Pré-Implementação — Ajustes Obrigatórios do Parecer Verônica
Data e Hora: 2026-03-21 13:30
Participantes:
  - Chico (CEO) — facilitador
  - Ana Silva (Arquiteta de Software) — coordenação técnica
  - Ricardo Monteiro (Economista / Consultor Financeiro) — regras de domínio financeiro
  - Camila Duarte (Consultora de Finanças Pessoais) — validação operacional
  - André Santos (DBA PostgreSQL) — modelagem e constraints
  - Maria Oliveira (Backend Sênior) — regras de negócio e testes
  - João Pereira (Backend Sênior) — implementação e APIs
  - Pedro Santos (Backend Python) — conciliação e importação
  - Laura Costa (Backend) — Edge Functions e automação
  - Roberto Lima (Frontend Sênior) — impacto em interfaces
  - Sofia Almeida (Frontend Sênior) — filtros e componentes
  - Helena Vargas (UX) — experiência do usuário
  - Isabella Torres (UI Designer) — interface e componentes visuais
  - Thiago Martins (Front Engineer) — componentização e tipagem
  - Lucas Ferreira (Mobile Sênior) — impacto mobile futuro
  - Beatriz Rocha (Mobile) — React Native/Expo
  - Fernando Gomes (DevOps) — migrations e CI/CD
  - Renata Silva (QA Visual/A11y) — testes e acessibilidade
  - Carlos Mendes (Designer de Software) — design system
  - Gabriela Nunes (Marketing Digital) — observadora
Pauta:
  - Item 1: Apresentação do parecer da Verônica — aprovação com ajustes obrigatórios
  - Item 2: Ajuste 4.1 — Estratégia de deduplicação entre transação e item de fatura
  - Item 3: Ajuste 4.2 — Norma de uso de `consumption_metrics`
  - Item 4: Ajuste 4.3 — Governança técnica de aliases de fornecedor
  - Item 5: Ajuste 5.1 — Papel de `supplier_tags` no MVP (não bloqueante)
  - Item 6: Decisões finais e sequenciamento de implementação
  - Item 7: Encerramento e entregáveis
---

## 1. Abertura e Contexto

**Chico (CEO):**
Time, a Verônica analisou nosso refino da dimensão fornecedor e emitiu um parecer formal. A boa notícia: **estamos aprovados para início**. A notícia que exige trabalho: **não podemos abrir IDE sem antes fechar três pontos técnicos**. Ela foi clara — e a Verônica não dá ponto sem nó.

Os três ajustes obrigatórios de pré-implementação são:

1. Estratégia de deduplicação entre transação e item de fatura
2. Norma de uso de `consumption_metrics`
3. Governança técnica de aliases de fornecedor

Mais um ponto não bloqueante: 4. Papel de `supplier_tags` no MVP

Vamos atacar cada ponto de forma objetiva e sair daqui com documentação definitiva.

**Ana Silva (Arquiteta):**
Concordo com a abordagem. Cada ponto vira um mini-ADR (Architecture Decision Record) com definição, exemplos válidos e inválidos, e regras técnicas de constraint. Esses ADRs vão para `docs/planejamento/` e servem de referência durante a implementação.

**Ricardo Monteiro (Economista):**
A Verônica identificou exatamente os pontos que, na minha experiência, geram retrabalho se não forem fechados antes. Especialmente a deduplicação — em fintech, double-counting destrói a credibilidade do sistema.

---

## 2. Ajuste Obrigatório 4.1 — Estratégia de Deduplicação entre Transação e Item de Fatura

### 2.1. Contexto do Problema

**Ana Silva (Arquiteta):**
A Verônica apontou que nossa R27 (deduplicação por heurística: mesmo supplier_id + valor + data ± 3 dias + competência) é frágil. Concordo. Heurística é para IA futura. Para o core determinístico, precisamos de **vínculo explícito entre registros**.

O problema concreto: uma compra no cartão pode existir como:

- `statement_item` na fatura do cartão
- `transaction` manual que o usuário lançou antes
- registro importado de extrato/CSV

Se não houver vínculo explícito, relatórios podem contar o mesmo gasto 2-3 vezes.

### 2.2. Debate Técnico

**André Santos (DBA):**
Já temos um mecanismo parcial: `statement_items.transaction_id` pode apontar para uma transação. E `transactions.statement_cycle_id` pode apontar para uma fatura. Mas faltam regras claras de:

- Quem é a "fonte da verdade" quando ambos existem?
- Como consolidar em relatórios?
- Como tratar itens de fatura que NUNCA viram transação?

**Maria Oliveira (Backend):**
Proponho uma abordagem baseada em **fonte primária** e **registro derivado**, com uma entidade explícita de vínculo. Vou detalhar:

#### Modelo Conceitual: Evento Financeiro → Observações

Todo gasto real é UM evento financeiro, mas pode ter MÚLTIPLAS observações:

```
Evento real: "Compra no Supermercado X — R$ 350 — 10/Mar"
├── Observação 1: statement_item na fatura Nubank (fonte: fatura do cartão)
├── Observação 2: transaction manual lançada pelo usuário (fonte: manual)
└── Observação 3: linha do CSV importado (fonte: extrato bancário)
```

**A regra é:** todas essas observações se referem ao MESMO evento. Precisamos de um mecanismo para vinculá-las e definir qual é a canônica.

**Pedro Santos (Backend):**
Concordo com a Maria. Proponho resolver isso com uma combinação de:

1. O campo `statement_items.transaction_id` que já temos — funciona como o vínculo entre item de fatura e transação
2. Uma regra clara de **entidade primária** para relatórios
3. Um campo de `origin_type` na transação para rastreabilidade

**Ricardo Monteiro (Economista):**
Na prática financeira, a regra de ouro é: **uma despesa é contabilizada no momento em que a obrigação nasce**. Para cartão de crédito, a obrigação nasce na compra (data da transação), não no pagamento da fatura. Então:

- O `statement_item` registra a compra no cartão (quando a obrigação nasceu)
- A `transaction` do tipo `statement_payment` registra o pagamento da fatura (quando o dinheiro saiu da conta)
- Esses são dois EVENTOS DIFERENTES, não duplicatas

O risco de duplicação real é outro: o usuário lança manualmente "Supermercado R$ 350" E o mesmo gasto aparece no statement_item da fatura. Aí sim são duas observações do mesmo evento.

### 2.3. Proposta: Modelo de Vínculo e Regras

**Ana Silva (Arquiteta):**
Com base no debate, proponho as seguintes regras formais:

#### 2.3.1. Tipos de Registro e Seus Papéis

| Registro                                | Papel                          | Quando nasce                       | Contabiliza como gasto?                    |
| --------------------------------------- | ------------------------------ | ---------------------------------- | ------------------------------------------ |
| `statement_item`                        | Observação de compra em cartão | Quando a fatura é gerada/importada | **SIM** — é a despesa real                 |
| `transaction` (type: expense) manual    | Lançamento manual do usuário   | Quando o usuário registra          | **SIM** — é a despesa real                 |
| `transaction` (type: statement_payment) | Pagamento da fatura            | Quando o usuário quita a fatura    | **NÃO** — é quitação de obrigação          |
| `transaction` (type: expense) importada | Importação de CSV/extrato      | Quando o extrato é importado       | **SIM, se não vinculada a statement_item** |
| `recurring_instance`                    | Expectativa de gasto futuro    | Quando o sistema gera a instância  | **NÃO** — é previsão (até ser confirmada)  |

#### 2.3.2. Regra de Vínculo (Conciliação)

```
REGRA: statement_items.transaction_id é o vínculo explícito.

CASO 1: Item de fatura SEM transação manual prévia
  → statement_item existe com transaction_id = NULL
  → O item de fatura É a fonte primária da despesa
  → Relatórios contam o statement_item
  → Quando a fatura for paga, o statement_payment NÃO gera novo expense

CASO 2: Item de fatura COM transação manual prévia
  → Usuário lançou "Supermercado R$ 350" manualmente (transaction criada)
  → Depois, fatura do cartão chega com item "SUPERM BOM PRECO R$ 350"
  → O sistema sugere vinculação: statement_item.transaction_id = id da transação
  → Se vinculado: a TRANSAÇÃO é a fonte primária (tem mais metadados: categoria, tags, fornecedor, notas)
  → Relatórios contam APENAS a transação, e o statement_item é tratado como observação vinculada

CASO 3: Importação de CSV com item já na fatura
  → CSV traz "SUPERM BOM PRECO R$ 350"
  → Sistema detecta: já existe statement_item com mesma descrição + valor + data
  → Não cria nova transação — marca como duplicata detectada
  → Se o usuário forçar importação, cria transação MAS vincula ao statement_item.transaction_id
```

#### 2.3.3. Regra de Precedência (Fonte Primária)

**Maria Oliveira:**

| Cenário                                       | Fonte primária           | Motivo                                                  |
| --------------------------------------------- | ------------------------ | ------------------------------------------------------- |
| Só statement_item (sem transação vinculada)   | statement_item           | É a única observação                                    |
| statement_item + transação vinculada          | transação                | Tem mais metadados (categoria, tags, fornecedor, notas) |
| Só transação manual (sem item de fatura)      | transação                | É a única observação                                    |
| Importação + statement_item (match detectado) | statement_item existente | Evita duplicação                                        |
| recurring_instance confirmada → transação     | transação gerada         | A instância é expectativa, a transação é fato           |

#### 2.3.4. Regra de Exibição

**Roberto Lima (Frontend):**

| Tela                         | O que mostrar                                                   |
| ---------------------------- | --------------------------------------------------------------- |
| **Relatório de despesas**    | Fonte primária de cada evento (sem duplicatas)                  |
| **Fatura do cartão**         | Todos os statement_items (é a visão da fatura)                  |
| **Extrato da conta**         | Todas as transactions da conta (incluindo statement_payment)    |
| **Relatório por fornecedor** | Fonte primária de cada evento, agrupada por supplier_id         |
| **Detalhes do lançamento**   | Fonte primária + indicador visual se tem observações vinculadas |

#### 2.3.5. Regra de Soma em Relatório

**Ricardo Monteiro (Economista):**

```
REGRA DE SOMA:

1. Para cada evento real, soma-se APENAS a fonte primária
2. statement_payment NUNCA entra na soma de despesas (é quitação)
3. transfers NUNCA entram na soma de despesas (são movimentações internas)
4. recurring_instances com status ≠ 'paid' NÃO entram na soma de gastos realizados
5. Se statement_item tem transaction_id preenchido → conta APENAS a transaction
6. Se statement_item NÃO tem transaction_id → conta o statement_item
7. Exceção: relatório "Composição da Fatura" — conta todos os statement_items (é visão da fatura)
```

### 2.4. Exemplos: Casos Típicos e Casos-Limite

**Camila Duarte (Consultora):**

#### Caso Típico 1: Compra no cartão sem lançamento manual

```
1. Usuário compra no Supermercado X com Nubank
2. Fatura fecha → statement_item: "SUPERM BOM PRECO R$ 350"
3. Não há transação manual prévia
4. Relatório de despesas: conta R$ 350 (fonte: statement_item)
5. Usuário paga fatura → transaction type=statement_payment de R$ 1.200
6. Relatório de despesas: NÃO soma R$ 1.200 (é quitação)
```

#### Caso Típico 2: Lançamento manual DEPOIS vinculado à fatura

```
1. Usuário lança manualmente: "Supermercado X — R$ 350 — Nubank — cat: Alimentação"
2. Fatura fecha → statement_item: "SUPERM BOM PRECO R$ 350"
3. Sistema sugere: "Este item parece ser o lançamento 'Supermercado X'"
4. Usuário confirma → statement_item.transaction_id = id do lançamento manual
5. Relatório de despesas: conta R$ 350 (fonte: transaction, tem categoria/tags)
6. Fatura do cartão: mostra statement_item (é visão da fatura)
```

#### Caso-Limite 1: Valores diferentes (parcela vs total)

```
1. Usuário compra TV de R$ 3.000 em 10x no Nubank
2. Cada mês → statement_item: "LOJA TV R$ 300 (3/10)"
3. Se o usuário também lançou a compra como R$ 3.000 parcelada
4. A transação-mãe é R$ 3.000 — os statement_items são R$ 300 cada
5. REGRA: NÃO vincular automaticamente — valores diferentes, são visões diferentes
6. A transação parcelada gera instâncias/parcelas que SIM se vinculam 1:1 com statement_items
```

#### Caso-Limite 2: Importação de CSV duplicando statement_item

```
1. Fatura Nubank já importada → 15 statement_items criados
2. Usuário importa CSV do extrato bancário contendo pagamento da fatura
3. CSV traz: "PIX NUBANK R$ 1.200"
4. Sistema: isso é tipo statement_payment (pagamento de fatura), NÃO é nova despesa
5. Cria transaction type=statement_payment, vincula ao statement_cycle
6. Nenhuma duplicação de despesa
```

#### Caso-Limite 3: Débito automático (conta de luz via conta corrente)

```
1. Conta de energia debitada automaticamente: transaction type=expense R$ 280
2. PDF da conta de luz é uploadado como document
3. NÃO existe statement_item (não é cartão de crédito)
4. Fonte primária: transaction
5. Relatório de despesas: conta R$ 280 (fonte: transaction)
6. Relatório por fornecedor (Neoenergia): R$ 280
```

#### Caso-Limite 4: Estorno

```
1. statement_item: "LOJA X R$ 200"
2. Depois: statement_item: "ESTORNO LOJA X -R$ 200"
3. Ambos ficam na fatura, ambos contam
4. Soma líquida: R$ 0 para LOJA X
5. Relatório de despesas: mostra ambos, totalizando R$ 0
```

### 2.5. Impacto em Schema — Adições Necessárias

**André Santos (DBA):**
Para materializar essas regras, proponho:

1. **Adicionar `origin_type` em `transactions`:**

```sql
ALTER TABLE transactions ADD COLUMN origin_type enum(
  'manual',          -- usuário digitou
  'import',          -- importação de CSV/XLSX
  'recurring',       -- gerado por recorrência
  'statement_link'   -- gerado a partir de statement_item
) DEFAULT 'manual';
```

2. **Constraint de unicidade em `statement_items.transaction_id`:**

```sql
-- Cada statement_item pode ter no máximo UMA transação vinculada
-- E cada transação pode estar vinculada a no máximo UM statement_item
CREATE UNIQUE INDEX idx_statement_items_transaction_id
  ON statement_items(transaction_id) WHERE transaction_id IS NOT NULL;
```

3. **View para relatórios sem duplicação:**

```sql
CREATE VIEW v_expenses_deduplicated AS
SELECT
  -- Se statement_item tem transaction vinculada, usa dados da transaction
  COALESCE(t.id, si.id) AS canonical_id,
  CASE
    WHEN si.id IS NOT NULL AND si.transaction_id IS NOT NULL THEN 'transaction'
    WHEN si.id IS NOT NULL AND si.transaction_id IS NULL THEN 'statement_item'
    ELSE 'transaction'
  END AS source_type,
  COALESCE(t.amount, si.amount) AS amount,
  COALESCE(t.description, si.description) AS description,
  COALESCE(t.supplier_id, si.supplier_id) AS supplier_id,
  t.category_id,
  t.priority,
  COALESCE(t.event_date, si.transaction_date) AS event_date,
  t.competence_date,
  t.financial_period_id
FROM statement_items si
FULL OUTER JOIN transactions t ON si.transaction_id = t.id
WHERE (t.type IS NULL OR t.type NOT IN ('statement_payment', 'refund'))
  OR (si.id IS NOT NULL AND si.transaction_id IS NULL);
```

**Maria Oliveira:**
Na prática do MVP, a view simplificada seria:

```sql
-- Relatório de despesas =
--   transactions do tipo expense/fee/interest_charge (exceto as que vieram de statement_item duplicado)
-- + statement_items SEM transaction vinculada
-- Isso garante zero duplicação.
```

### 2.6. Decisão Final sobre Deduplicação

**Chico (CEO):**
Aceito a proposta. É sólida, determinística e não depende de heurística.

**Aprovado:**

- Vínculo explícito via `statement_items.transaction_id` (1:1)
- `origin_type` em transactions para rastreabilidade
- Regra de precedência: se vinculado, transaction é fonte primária
- Regra de soma: nunca contar statement_item E transaction vinculada na mesma soma
- Regra de exibição: relatórios usam fonte primária, fatura mostra statement_items
- UNIQUE index em `statement_items.transaction_id`
- View `v_expenses_deduplicated` para facilitar relatórios
- R27 revisada: substituir heurística por vínculo explícito + view

**R27 revisada:**

> Relatórios por fornecedor (e por qualquer dimensão) utilizam a fonte primária de cada evento financeiro. Quando um `statement_item` possui `transaction_id` vinculado, a `transaction` é a fonte primária. Quando não possui, o `statement_item` é a fonte primária. Pagamentos de fatura (`statement_payment`) NUNCA são contabilizados como despesa. A view `v_expenses_deduplicated` materializa essa regra.

---

## 3. Ajuste Obrigatório 4.2 — Norma de Uso de `consumption_metrics`

### 3.1. Contexto do Problema

**Ana Silva (Arquiteta):**
A Verônica apontou que nossa tabela `consumption_metrics` pode virar "tabela-lixeira" se não distinguirmos claramente:

- **Métrica quantitativa** (kWh, m³, GB, unidades) — tem quantity, unit_price, subtotal
- **Atributo qualitativo** (bandeira tarifária, plano, velocidade contratada) — não tem quantity/subtotal
- **Metadado complementar** (observação, referência) — vai em metadata/notes

### 3.2. Debate Técnico

**Ricardo Monteiro (Economista):**
Na minha experiência com contas de energia e telecom, os dados extraíveis de uma conta se dividem claramente:

**Conta de energia (Neoenergia):**

- Consumo em kWh → **métrica** (quantity=320, unit=kWh, unit_price=0.87, subtotal=278.40)
- Bandeira tarifária → **atributo** (não tem quantity, não tem subtotal)
- Tarifa de iluminação pública → **métrica** (quantity=1, unit=taxa, unit_price=42.50, subtotal=42.50)
- Número do medidor → **metadado** (referência do equipamento)

**Conta de telefone (Vivo):**

- Dados consumidos → **métrica** (quantity=45, unit=GB)
- Velocidade contratada → **atributo** (300Mbps — não é consumo medido)
- Plano → **atributo** (Fibra 300 — é classificação, não quantidade)
- Chamadas realizadas → **métrica** (quantity=120, unit=minutos)

**Assinatura SaaS (GitHub):**

- Licenças ativas → **métrica** (quantity=1, unit=licenças, unit_price=4.00, subtotal=4.00)
- Plano → **atributo** (Pro — não é quantidade)
- Storage utilizado → **métrica** (quantity=2.5, unit=GB)

**Maria Oliveira (Backend):**
Vejo 3 categorias claras:

| Tipo         | Definição                                  | Campos usados                                                      | Exemplos                                          |
| ------------ | ------------------------------------------ | ------------------------------------------------------------------ | ------------------------------------------------- |
| **Métrica**  | Valor quantitativo mensurável com unidade  | `quantity`, `metric_unit`, opcionalmente `unit_price` e `subtotal` | kWh, GB, minutos, m³, litros, unidades            |
| **Atributo** | Qualificador categórico ou classificatório | `metadata` (campo JSONB)                                           | bandeira, plano, faixa de consumo, tipo de tarifa |
| **Metadado** | Informação de referência imutável          | `notes` ou `metadata`                                              | número do medidor, ID de instalação, observação   |

### 3.3. Proposta: Norma Formal

**André Santos (DBA):**

#### 3.3.1. Definição: O que É uma métrica

> **Métrica** é um registro quantitativo mensurável associado a um fornecedor em um período, que possui unidade de medida e pode ou não ter valor monetário associado.

Critérios para ser métrica:

- Tem **quantidade numérica** (quantity > 0 ou = 0)
- Tem **unidade de medida** (metric_unit é obrigatório)
- É **associada a um período** (reference_period_start/end)
- Pode ter **preço unitário e subtotal** (quando monetizável)

#### 3.3.2. Definição: O que NÃO é métrica (é atributo)

> **Atributo** é uma característica qualitativa ou classificatória que não possui quantidade numérica mensurável.

Critérios para NÃO ser métrica:

- Não tem quantidade numérica (ex: "Plano Fibra 300")
- É classificação/enumeração (ex: "Bandeira Amarela", "Faixa B2")
- É referência estática (ex: "Medidor #12345")

#### 3.3.3. Regra de Registro

| Se o dado...                        | Então...              | Onde registrar                                                                   |
| ----------------------------------- | --------------------- | -------------------------------------------------------------------------------- |
| Tem quantity + unit                 | É métrica             | `consumption_metrics` com `quantity` + `metric_unit`                             |
| Tem quantity + unit + preço         | É métrica monetizável | `consumption_metrics` com `quantity` + `metric_unit` + `unit_price` + `subtotal` |
| Não tem quantity, é categórico      | É atributo            | `consumption_metrics.metadata` (campo JSONB)                                     |
| É referência/equipamento/observação | É metadado            | `supplier_contracts.metadata` ou `notes`                                         |

**IMPORTANTE:** Atributos qualitativos que se repetem mensalmente (ex: bandeira tarifária que muda todo mês) podem ser registrados em `consumption_metrics` usando `quantity = NULL` e o valor no `metadata`. Isso mantém a série temporal.

#### 3.3.4. Convenção para o campo `metadata`

**João Pereira (Backend):**

```jsonc
// metadata para ATRIBUTO qualitativo periódico
{
  "type": "attribute",           // obrigatório quando não é métrica quantitativa
  "attribute_name": "bandeira",  // nome do atributo
  "attribute_value": "amarela"   // valor do atributo
}

// metadata para MÉTRICA com contexto extra
{
  "type": "metric",              // pode ser implícito (quando tem quantity)
  "tariff": "convencional",      // contexto adicional da métrica
  "reading_date": "2026-03-15"   // informação complementar
}
```

### 3.4. Exemplos Válidos

**Ricardo Monteiro:**

#### Conta de Energia — Neoenergia (3 registros)

| metric_name | metric_unit | quantity | unit_price | subtotal | metadata                                                                            |
| ----------- | ----------- | -------- | ---------- | -------- | ----------------------------------------------------------------------------------- |
| Consumo     | kWh         | 320.0000 | 0.875000   | 278.40   | `{"tariff": "convencional"}`                                                        |
| COSIP       | taxa        | 1.0000   | 42.50      | 42.50    | `{"description": "Contribuição iluminação pública"}`                                |
| —           | —           | NULL     | NULL       | NULL     | `{"type": "attribute", "attribute_name": "bandeira", "attribute_value": "amarela"}` |

**Nota:** A bandeira tarifária é registrada como atributo (sem quantity) para preservar a série temporal: "em março era amarela, em abril era verde".

#### Conta de Internet — Vivo (2 registros)

| metric_name      | metric_unit | quantity | unit_price | subtotal | metadata                                                                               |
| ---------------- | ----------- | -------- | ---------- | -------- | -------------------------------------------------------------------------------------- |
| Dados utilizados | GB          | 45.0000  | NULL       | NULL     | NULL                                                                                   |
| —                | —           | NULL     | NULL       | NULL     | `{"type": "attribute", "attribute_name": "plano", "attribute_value": "Fibra 300Mbps"}` |

#### Assinatura — GitHub (1 registro)

| metric_name | metric_unit | quantity | unit_price | subtotal | metadata          |
| ----------- | ----------- | -------- | ---------- | -------- | ----------------- |
| Licenças    | unidades    | 1.0000   | 4.000000   | 4.00     | `{"plan": "Pro"}` |

### 3.5. Exemplos Inválidos

**Maria Oliveira:**

| O que alguém poderia fazer                                                                      | Por que é inválido                                                                           | Correção                                                                         |
| ----------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------- |
| `metric_name: "Bandeira"`, `quantity: 1`, `metric_unit: "flag"`                                 | Bandeira não é quantidade mensurável — forçar quantity=1 é semântica errada                  | Usar metadata com `type: "attribute"`                                            |
| `metric_name: "Plano"`, `quantity: 300`, `metric_unit: "Mbps"`                                  | "Plano Fibra 300" é atributo, não consumo medido. 300 é a velocidade contratada, não consumo | Usar metadata: `{"attribute_name": "plano", "attribute_value": "Fibra 300Mbps"}` |
| `metric_name: "Observação"`, `quantity: NULL`, `metadata: {"text": "Medidor trocado em março"}` | Não é métrica NEM atributo periódico — é metadado do contrato                                | Registrar em `supplier_contracts.notes` ou `supplier_contracts.metadata`         |
| `subtotal: 280.00` sem `quantity` nem `unit_price`                                              | Subtotal sem quantity/unit_price perde rastreabilidade. O subtotal precisa ser derivável     | Se só tem valor total, usar `quantity: 1`, `unit_price: 280.00`                  |

### 3.6. Convenções por Tipo de Fornecedor

**Camila Duarte:**

| Tipo de fornecedor         | Métricas típicas                | Atributos típicos                | Onde fica referência                 |
| -------------------------- | ------------------------------- | -------------------------------- | ------------------------------------ |
| **utility** (energia)      | kWh consumo, taxa COSIP         | bandeira, faixa, grupo tarifário | Medidor, UC → supplier_contracts     |
| **utility** (água)         | m³ consumo, taxa esgoto         | faixa de consumo                 | Hidrômetro → supplier_contracts      |
| **utility** (gás)          | m³ consumo                      | tarifa residencial/comercial     | Medidor → supplier_contracts         |
| **telecom**                | GB dados, minutos chamadas      | plano, velocidade contratada     | Linha, contrato → supplier_contracts |
| **saas**                   | licenças, GB storage, API calls | plano (free/pro/team)            | Subscription ID → supplier_contracts |
| **company** (supermercado) | _geralmente sem métricas_       | _nenhum_                         | _sem contrato_                       |
| **individual** (diarista)  | horas trabalhadas               | _nenhum_                         | _sem contrato formal_                |
| **platform** (iFood)       | pedidos realizados              | _nenhum_                         | _sem contrato_                       |

### 3.7. Coerência Métrica vs Valor da Transação

**Ricardo Monteiro:**

> **REGRA:** O somatório dos subtotais das métricas NÃO precisa bater exatamente com o valor da transação.

Motivo: a transação pode incluir impostos, taxas, arredondamentos e encargos que não são decompostos em métricas.

Exemplo:

```
Transação: Conta de energia — R$ 295,00
Métricas:
  - Consumo: 320 kWh × R$ 0.875 = R$ 280,00
  - COSIP: 1 × R$ 42,50 = R$ 42,50
  Total métricas: R$ 322,50 (DIVERGE da transação por impostos/descontos)
```

Isso é ACEITÁVEL. As métricas são decomposição PARCIAL e informativa, não contábil.

### 3.8. Constraint Técnica Sugerida

**André Santos (DBA):**

```sql
-- Constraint: se é métrica quantitativa, deve ter metric_name e metric_unit
ALTER TABLE consumption_metrics ADD CONSTRAINT chk_metric_or_attribute
  CHECK (
    -- Caso 1: métrica quantitativa (tem quantity → precisa de name e unit)
    (quantity IS NOT NULL AND metric_name IS NOT NULL AND metric_unit IS NOT NULL)
    OR
    -- Caso 2: atributo qualitativo (sem quantity → precisa de metadata com type=attribute)
    (quantity IS NULL AND metadata IS NOT NULL AND metadata->>'type' = 'attribute')
  );
```

### 3.9. Decisão Final sobre consumption_metrics

**Chico (CEO):**
Aceito a norma proposta. Clara, com exemplos válidos e inválidos, e com constraint no banco.

**Aprovado:**

- Distinção formal: métrica (quantity + unit) vs atributo (metadata com type=attribute) vs metadado (supplier_contracts)
- Campos estruturados para métricas: quantity, metric_unit, unit_price, subtotal
- Campo metadata JSONB para atributos qualitativos periódicos
- Convenções por tipo de fornecedor documentadas
- Coerência parcial: subtotal das métricas NÃO precisa bater com valor da transação
- CHECK constraint no banco para garantir consistência
- Exemplos válidos e inválidos documentados como referência

---

## 4. Ajuste Obrigatório 4.3 — Governança Técnica de Aliases de Fornecedor

### 4.1. Contexto do Problema

**Ana Silva (Arquiteta):**
A Verônica apontou que temos boa modelagem de aliases mas faltam REGRAS TÉCNICAS rígidas para evitar:

- Aliases conflitantes entre fornecedores
- Fornecedores duplicados crescendo sem controle
- Merges inseguros que quebram histórico
- Associações retroativas incorretas

Precisamos transformar o conceitual em constraints e fluxos formais.

### 4.2. Debate Técnico

**André Santos (DBA):**
O ponto central é: para um dado `user_id`, em um dado período, um alias deve resolver para EXATAMENTE UM fornecedor. Caso contrário, a resolução é ambígua e o sistema não pode sugerir automaticamente.

**Maria Oliveira (Backend):**
E temos que cobrir os seguintes cenários:

1. **Criar alias:** Usuário ou sistema cria alias "CELPE" → Neoenergia
2. **Conflito:** Outro fornecedor tenta usar alias "CELPE" → proibido (no mesmo período)
3. **Expirar alias:** "CELPE" válido até 2023-12-31, depois "Neoenergia"
4. **Merge:** Dois fornecedores (ex: "CELPE" e "Neoenergia") são o mesmo → merge
5. **Reversão:** Merge foi erro → desfazer (com cautela)
6. **Renomear:** Fornecedor principal muda de nome → antigo vira alias

### 4.3. Proposta: Regras Técnicas de Governança

#### 4.3.1. Regra de Unicidade

**André Santos (DBA):**

> **REGRA:** Para um dado `user_id`, não podem existir dois aliases com o mesmo `alias_name` (case-insensitive) com períodos de vigência sobrepostos.

```sql
-- Constraint parcial + trigger para unicidade temporal
-- Dois aliases iguais são permitidos se NÃO se sobrepõem no tempo

CREATE OR REPLACE FUNCTION check_alias_uniqueness()
RETURNS TRIGGER AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM supplier_aliases sa
    WHERE sa.user_id = NEW.user_id
      AND sa.id != COALESCE(NEW.id, '00000000-0000-0000-0000-000000000000'::uuid)
      AND LOWER(sa.alias_name) = LOWER(NEW.alias_name)
      AND (
        -- Períodos se sobrepõem
        (sa.valid_from IS NULL OR NEW.valid_until IS NULL OR sa.valid_from <= NEW.valid_until)
        AND
        (sa.valid_until IS NULL OR NEW.valid_from IS NULL OR sa.valid_until >= NEW.valid_from)
      )
  ) THEN
    RAISE EXCEPTION 'Alias "%" já existe para outro fornecedor no período informado', NEW.alias_name;
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_check_alias_uniqueness
  BEFORE INSERT OR UPDATE ON supplier_aliases
  FOR EACH ROW EXECUTE FUNCTION check_alias_uniqueness();
```

**Implicação:** "CELPE" só pode apontar para UM fornecedor de cada vez. Se mudar de empresa (ex: fusão), o antigo alias expira e um novo pode ser criado.

#### 4.3.2. Regra de Vigência

**Maria Oliveira:**

| Campo         | Significado                   | Valor NULL                                               |
| ------------- | ----------------------------- | -------------------------------------------------------- |
| `valid_from`  | Desde quando o alias é válido | Sem data de início conhecida (presume-se "desde sempre") |
| `valid_until` | Até quando o alias é válido   | Ainda ativo (sem data de expiração)                      |

**Regras:**

- `valid_from = NULL, valid_until = NULL` → alias ativo em qualquer data
- `valid_from = 2020-01-01, valid_until = 2023-12-31` → alias válido de 2020 a 2023
- `valid_from = 2024-01-01, valid_until = NULL` → alias ativo desde 2024

**Resolução temporal:**
Quando o sistema busca o fornecedor pelo alias "CELPE" em uma transação de 2022:

```sql
SELECT supplier_id FROM supplier_aliases
WHERE user_id = :user_id
  AND LOWER(alias_name) = LOWER('CELPE')
  AND (valid_from IS NULL OR valid_from <= '2022-01-01')
  AND (valid_until IS NULL OR valid_until >= '2022-01-01')
LIMIT 1;
```

#### 4.3.3. Status do Alias: Ativo vs Inativo

**João Pereira (Backend):**
Proponho adicionar `is_active` na tabela `supplier_aliases`:

```sql
ALTER TABLE supplier_aliases ADD COLUMN is_active boolean DEFAULT true;
```

Diferença entre `is_active = false` e `valid_until` preenchido:

- `valid_until` = expiração natural (ex: empresa mudou de nome)
- `is_active = false` = desativação manual (ex: alias errado, conflito detectado)

Um alias inativo NUNCA é usado para resolução, mesmo se estiver no período válido.

#### 4.3.4. Fluxo de Merge de Fornecedores

**Pedro Santos (Backend):**

```
FLUXO: Merge de fornecedores (A absorvido por B)

PRÉ-CONDIÇÕES:
- Ambos fornecedores pertencem ao mesmo user_id
- O usuário confirma explicitamente a operação
- O sistema exibe preview: "X transações, Y recorrências, Z documentos serão migrados"

ETAPAS (em transação atômica):
1. Migrar referências de A → B:
   - UPDATE transactions SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
   - UPDATE recurring_templates SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
   - UPDATE recurring_instances SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
   - UPDATE statement_items SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
   - UPDATE documents SET supplier_id = B WHERE supplier_id = A AND user_id = :uid
   - UPDATE liabilities SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

2. Migrar aliases de A → B:
   - UPDATE supplier_aliases SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

3. Criar alias com nome do fornecedor absorvido:
   - INSERT INTO supplier_aliases (supplier_id, alias_name, alias_type, valid_until)
     VALUES (B, A.name, 'former_name', NOW())

4. Migrar contratos de A → B:
   - UPDATE supplier_contracts SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

5. Migrar métricas de A → B:
   - UPDATE consumption_metrics SET supplier_id = B WHERE supplier_id = A AND user_id = :uid

6. Migrar tags de A → B (evitando duplicatas):
   - INSERT INTO supplier_tags (supplier_id, tag_id)
     SELECT B, tag_id FROM supplier_tags WHERE supplier_id = A AND user_id = :uid
     ON CONFLICT DO NOTHING

7. Desativar fornecedor absorvido:
   - UPDATE suppliers SET is_active = false,
       notes = CONCAT(notes, ' [Merged into ', B.name, ' at ', NOW(), ']')
     WHERE id = A

8. Registrar no audit_log:
   - INSERT INTO audit_log (action, entity_type, entity_id, details)
     VALUES ('supplier_merge', 'supplier', B,
       '{"absorbed": "A.id", "absorbed_name": "A.name", "migrated": {...counts}}'
     )

PÓS-CONDIÇÕES:
- Todas as referências agora apontam para B
- A está inativo mas NÃO deletado (preserva rastro)
- Nome de A é alias de B com type='former_name'
- Audit log registra a operação completa
```

**Fernando Gomes (DevOps):**
Esse merge DEVE ser implementado como Edge Function com `service_role_key` para garantir atomicidade e bypass de RLS durante a migração. O RLS é reativado após o merge.

#### 4.3.5. Fluxo de Reversão de Merge

**Ana Silva (Arquiteta):**

> **REGRA:** Reversão de merge é operação de ALTO RISCO e NÃO será implementada como funcionalidade automática no MVP.

Motivo: após o merge, o usuário pode ter editado transações, adicionado novas vinculações e alterado metadados. Reverter automaticamente é inseguro.

**Alternativa para o MVP:**

1. Criar novo fornecedor com os dados do antigo (manualmente)
2. Reatribuir transações desejadas ao novo fornecedor (manualmente ou em lote)
3. O audit_log preserva os dados do merge original para referência

**Para Fase 2+:** Avaliar "undo merge" com janela de tempo (ex: até 24h após merge, se nenhuma edição manual foi feita).

#### 4.3.6. Fluxo de Renomeação de Fornecedor Principal

**Maria Oliveira:**

```
FLUXO: Renomear fornecedor (ex: "CELPE" → "Neoenergia")

1. Usuário edita suppliers.name de "CELPE" para "Neoenergia"
2. ANTES de salvar, sistema:
   a. Cria alias automático: alias_name = "CELPE", type = 'former_name', valid_until = NOW()
   b. Atualiza suppliers.name = "Neoenergia"
   c. Registra no audit_log
3. Todas as transações continuam vinculadas pelo supplier_id (inalterado)
4. Busca por "CELPE" continua encontrando o fornecedor via alias
```

**André Santos (DBA):**
Isso deve ser feito via trigger no banco:

```sql
CREATE OR REPLACE FUNCTION auto_alias_on_rename()
RETURNS TRIGGER AS $$
BEGIN
  IF OLD.name != NEW.name THEN
    INSERT INTO supplier_aliases (user_id, supplier_id, alias_name, alias_type, valid_until)
    VALUES (NEW.user_id, NEW.id, OLD.name, 'former_name', NOW());
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_auto_alias_on_rename
  BEFORE UPDATE ON suppliers
  FOR EACH ROW EXECUTE FUNCTION auto_alias_on_rename();
```

#### 4.3.7. Revisão Humana Obrigatória

**Camila Duarte (Consultora):**
Em quais cenários o sistema DEVE exigir confirmação humana?

| Cenário                                      | Ação do sistema                    | Confirmação humana?                                    |
| -------------------------------------------- | ---------------------------------- | ------------------------------------------------------ |
| Resolução de alias com match exato           | Sugere fornecedor                  | **Não no MVP** (aceita automaticamente se match exato) |
| Resolução de alias com match fuzzy           | Sugere fornecedor com % confiança  | **Sim** — sempre                                       |
| Merge de fornecedores                        | Exibe preview de impacto           | **Sim** — sempre                                       |
| Associação retroativa em lote                | Exibe lista de candidatos          | **Sim** — sempre (individual ou lote)                  |
| Criação automática de alias (via renomeação) | Auto-cria alias type='former_name' | **Não** — é automático e seguro                        |
| Detecção de possível duplicata de fornecedor | Sugere merge                       | **Sim** — sempre                                       |
| Importação com alias sem match               | Mostra "fornecedor desconhecido"   | **Sim** — usuário decide                               |

### 4.4. Resumo das Constraints Técnicas

**André Santos (DBA):**

| Constraint                                     | Tipo                | Descrição                                                                                           |
| ---------------------------------------------- | ------------------- | --------------------------------------------------------------------------------------------------- |
| Unicidade temporal de alias                    | Trigger             | Mesmo alias_name (case-insensitive) não pode existir para dois fornecedores com vigência sobreposta |
| is_active = false exclui de resolução          | Lógica de aplicação | Alias inativo ignorado em queries de resolve                                                        |
| Auto-alias em renomeação                       | Trigger             | Nome antigo vira alias type='former_name' automaticamente                                           |
| Merge é atômico                                | Edge Function       | Todas as migrações em transação única                                                               |
| Fornecedor absorvido não é deletado            | Regra de negócio    | is_active = false, preserva rastro                                                                  |
| Index trigram para busca fuzzy                 | Índice              | Enable `pg_trgm`, CREATE INDEX em alias_name                                                        |
| UNIQUE INDEX em statement_items.transaction_id | Índice              | Garante 1:1 entre item e transação                                                                  |

### 4.5. Decisão Final sobre Governança de Aliases

**Chico (CEO):**
Aceito todas as regras propostas.

**Aprovado:**

- Unicidade temporal de alias por user_id (trigger + constraint)
- Campo `is_active` adicionado em supplier_aliases
- Vigência via `valid_from`/`valid_until` com regras de NULL documentadas
- Fluxo de merge atômico via Edge Function com 8 etapas
- Reversão de merge: manual no MVP, auto avaliado na Fase 2
- Auto-alias em renomeação via trigger
- Tabela de revisão humana obrigatória por cenário
- Fornecedor absorvido nunca é deletado (is_active = false)

---

## 5. Ajuste Recomendado 5.1 — Papel de `supplier_tags` no MVP

### 5.1. Debate

**Ana Silva (Arquiteta):**
A Verônica recomenda decidir explicitamente: `supplier_tags` entra no schema sem UI, ou fica fora do MVP?

**Maria Oliveira (Backend):**
Minha recomendação: **entra no schema, sem UI no MVP**. Motivos:

1. A tabela é simples (supplier_id + tag_id, N:N)
2. Incluir no schema agora evita migration posterior
3. Sem UI, não adiciona complexidade de frontend
4. Permite preenchimento via importação/automação antes da UI existir

**Roberto Lima (Frontend):**
Concordo. Zero impacto no frontend do MVP. A tabela existe, a UI não.

**Camila Duarte:**
Uma ressalva: se entrar no schema, documentar que NÃO é utilizada na interface do MVP. Evita confusão durante implementação onde alguém acha que precisa criar form/filtro para tags de fornecedor.

### 5.2. Decisão Final

**Chico (CEO):**
Aceito. supplier_tags entra no schema, sem UI no MVP.

**Aprovado:**

- Tabela `supplier_tags` criada nas migrations da Etapa 1
- NENHUM componente de UI, form ou filtro no MVP
- Documentado como "schema-only no MVP, UI na Fase 2"
- Seed de tags genéricas opcional (telecom, utility, essential, etc.)

---

## 6. Decisões Finais e Sequenciamento

### 6.1. Checklist dos 3 Ajustes Obrigatórios

| #   | Ajuste                             | Entregável                                                              | Status     |
| --- | ---------------------------------- | ----------------------------------------------------------------------- | ---------- |
| 4.1 | Deduplicação txn vs statement_item | Modelo de vínculo, regras de precedência/soma/exibição, view SQL        | ✅ Fechado |
| 4.2 | Norma de consumption_metrics       | Definição métrica vs atributo, exemplos válidos/inválidos, constraint   | ✅ Fechado |
| 4.3 | Governança de aliases              | Unicidade temporal, vigência, merge atômico, renomeação, revisão humana | ✅ Fechado |

### 6.2. Ajuste Não Bloqueante

| #   | Ajuste               | Decisão             | Status      |
| --- | -------------------- | ------------------- | ----------- |
| 5.1 | supplier_tags no MVP | Schema-only, sem UI | ✅ Decidido |

### 6.3. Sequenciamento de Implementação Aprovado

Seguindo a recomendação da Verônica (Parecer Seção 6), com detalhamento:

#### Etapa 1 — Base Estrutural

- Migrations de TODAS as tabelas (originais + fornecedor)
- RLS policies por user_id
- Índices (trigram, unique, FK)
- Constraints (CHECK, triggers de unicidade temporal e auto-alias)
- Enums e seeds (categorias, tipos)
- **Entregável:** Banco 100% estruturado, vazio, com RLS e constraints testados

#### Etapa 2 — Contrato Comportamental

- Escrever os 27 testes mandatórios (T1-T27) como suíte de regressão
- Escrever testes de constraint (unicidade de alias, deduplicação, etc.)
- Fechar cenários de aceitação com DADO/QUANDO/ENTÃO
- **Entregável:** Suíte de testes que FALHA (red) porque a implementação ainda não existe

#### Etapa 3 — Núcleo Funcional de Fornecedor

- CRUD de fornecedor (name, type, document_number, tags)
- CRUD de aliases (com validação de unicidade temporal)
- Autocomplete de fornecedor (busca por nome + alias + trigram)
- Associação de fornecedor a transações
- Associação de fornecedor a recorrências
- Associação de fornecedor a documentos
- **Entregável:** Fornecedor funcional end-to-end, testes T18-T24 passando

#### Etapa 4 — Relatórios e Filtros

- View `v_expenses_deduplicated`
- Relatório de gastos por fornecedor
- Filtros compostos (fornecedor × categoria × tag × período × prioridade)
- Fornecedor na home operacional (nome nos itens, top fornecedores, alertas)
- **Entregável:** Relatórios e filtros por fornecedor funcionais, testes T22, T27 passando

#### Etapa 5 — Recursos Avançados

- Registro de métricas de consumo (com norma de uso)
- Associação retroativa por alias (sugestão + confirmação humana)
- Merge de fornecedores (Edge Function atômica)
- View materializada `mv_supplier_spending`
- **Entregável:** Métricas, merge e retroatividade funcionais, testes T25-T26 passando

---

## 7. Encerramento

### 7.1. Recomendações

**Ana Silva (Arquiteta):**

> Os três ajustes obrigatórios estão fechados com nível de detalhe suficiente para implementação. Transformem cada um em ADR formal na pasta docs/planejamento/ e sigam.

**Ricardo Monteiro (Economista):**

> A regra de deduplicação é a mais crítica. Se errarem aqui, o relatório de gastos não serve para nada. A view `v_expenses_deduplicated` é sua rede de segurança — implemente no dia 1.

**André Santos (DBA):**

> Os triggers de unicidade de alias e auto-alias em renomeação são essenciais. Sem eles, a governança depende de código de aplicação, e isso é frágil. Banco impõe regra, aplicação consome.

**Maria Oliveira (Backend):**

> Escrevam os 27 testes ANTES. Sério. Se os testes estão escritos e falhando (red), cada feature implementada faz testes passarem (green). É o jeito mais seguro de saber que estamos no caminho certo.

**Fernando Gomes (DevOps):**

> Leiam o guia de implementação antes de começar. As etapas são sequenciais por um motivo — cada uma depende da anterior.

### 7.2. Fechamento

**Chico (CEO):**
Perfeito, time. A Verônica pediu que fechássemos 3 pontos antes de codar. Fechamos os 3 com:

- Modelo de vínculo explícito para deduplicação (substituindo heurística)
- Norma formal de consumption_metrics (com constraint no banco)
- Governança completa de aliases (unicidade, merge, renomeação)

Agora temos: luz verde para implementar.

Os documentos de planejamento serão gerados na pasta `docs/planejamento/` com:

1. ADR-001: Estratégia de deduplicação
2. ADR-002: Norma de consumption_metrics
3. ADR-003: Governança de aliases
4. Guia de implementação passo a passo
5. Índice geral do planejamento

**Verônica (Consultora):**
Os ajustes atendem ao parecer. A equipe pode seguir para implementação.

---

**Ações / Responsáveis / Prazo:**

| #   | Ação                                               | Responsável                   | Prazo    |
| --- | -------------------------------------------------- | ----------------------------- | -------- |
| 1   | Gerar ADRs formais em docs/planejamento/           | Ana Silva / Maria Oliveira    | Imediato |
| 2   | Gerar guia de implementação passo a passo          | Ana Silva / Fernando Gomes    | Imediato |
| 3   | Revisar migrations com novas constraints           | André Santos                  | Sprint 1 |
| 4   | Escrever suíte de testes T1-T27                    | Maria Oliveira / Pedro Santos | Sprint 1 |
| 5   | Implementar triggers (unicidade alias, auto-alias) | André Santos / João Pereira   | Sprint 1 |
| 6   | Implementar Edge Function de merge                 | Laura Costa / Fernando Gomes  | Sprint 2 |
| 7   | Criar view v_expenses_deduplicated                 | André Santos                  | Sprint 1 |
