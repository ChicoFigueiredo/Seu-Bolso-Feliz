# Guia de Implementação Passo a Passo — Dimensão Fornecedor

**Data:** 2026-03-21
**Origem:** Parecer Verônica (Seção 6 — Sequenciamento Recomendado)
**ADRs de referência:**

- `ADR-001-deduplicacao-transacao-item-fatura.md`
- `ADR-002-norma-consumption-metrics.md`
- `ADR-003-governanca-aliases-fornecedor.md`
- `ADR-004-arquitetura-operacional-repositorio-cicd.md`

---

## Pré-requisito: Etapa 0 — Setup do Monorepo (Sprint 0)

> **⚠️ OBRIGATÓRIO:** Antes de iniciar qualquer etapa abaixo, o monorepo deve estar configurado conforme a ADR-004 e o guia `002-guia-cicd-engenharia-operacional.md`. Isso inclui: Bun workspaces, estrutura `apps/` + `packages/`, ESLint, Vitest, Husky, Supabase init e `.gitlab-ci.yml`.

---

## Visão Geral

Este guia detalha as **5 etapas de implementação** da dimensão fornecedor, seguindo o sequenciamento recomendado pela Verônica. Cada etapa é sequencial e depende da anterior.

```
Etapa 0: Setup Monorepo + CI/CD  → Pré-requisito (ver 002-guia-cicd-engenharia-operacional.md)
Etapa 1: Base Estrutural         → Banco pronto, vazio, com constraints
Etapa 2: Contrato Comportamental → Testes escritos e falhando (red)
Etapa 3: Núcleo Funcional        → CRUD, aliases, autocomplete, associações
Etapa 4: Relatórios e Filtros    → Views, filtros compostos, home operacional
Etapa 5: Recursos Avançados      → Métricas, merge, retroatividade, IA
```

### Caminhos no Monorepo

| Item                   | Caminho                                                 |
| ---------------------- | ------------------------------------------------------- |
| Migrações SQL          | `supabase/migrations/`                                  |
| Edge Functions         | `supabase/functions/`                                   |
| Testes de domínio      | `__tests__/domain/` ou `packages/domain/src/__tests__/` |
| Testes de integração   | `__tests__/integration/`                                |
| Componentes web (CRUD) | `apps/web/src/components/`                              |
| Páginas web            | `apps/web/src/app/`                                     |
| Tipos compartilhados   | `packages/shared-types/src/`                            |
| Lógica de domínio      | `packages/domain/src/`                                  |
| Validação (Zod)        | `packages/validation/src/`                              |

---

## Etapa 1 — Base Estrutural

**Objetivo:** Criar toda a infraestrutura de banco de dados com migrations, RLS, índices e constraints.

### 1.1. Migrations a Criar

#### Migration 001: Tabelas de fornecedor

```sql
-- suppliers
CREATE TABLE suppliers (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id),
  name text NOT NULL,
  trade_name text,
  legal_name text,
  document_number text,
  type text NOT NULL DEFAULT 'company'
    CHECK (type IN ('company', 'individual', 'government', 'utility', 'telecom', 'saas', 'platform', 'other')),
  website text,
  contact_info jsonb,
  notes text,
  institution_id uuid REFERENCES institutions(id),
  is_active boolean DEFAULT true,
  display_order integer,
  created_at timestamptz DEFAULT now(),
  updated_at timestamptz DEFAULT now()
);

-- supplier_aliases
CREATE TABLE supplier_aliases (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id),
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  alias_name text NOT NULL,
  alias_type text NOT NULL DEFAULT 'other'
    CHECK (alias_type IN ('former_name', 'abbreviation', 'trade_name', 'billing_name', 'other')),
  is_active boolean DEFAULT true,
  valid_from date,
  valid_until date,
  notes text,
  created_at timestamptz DEFAULT now(),
  updated_at timestamptz DEFAULT now()
);

-- supplier_contracts
CREATE TABLE supplier_contracts (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id),
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  contract_type text NOT NULL
    CHECK (contract_type IN ('service', 'subscription', 'utility', 'loan', 'insurance', 'maintenance', 'other')),
  identifier text,
  label text,
  is_active boolean DEFAULT true,
  start_date date,
  end_date date,
  metadata jsonb,
  notes text,
  created_at timestamptz DEFAULT now(),
  updated_at timestamptz DEFAULT now()
);

-- consumption_metrics
CREATE TABLE consumption_metrics (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id),
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  supplier_contract_id uuid REFERENCES supplier_contracts(id),
  transaction_id uuid REFERENCES transactions(id),
  document_id uuid REFERENCES documents(id),
  reference_period_start date NOT NULL,
  reference_period_end date NOT NULL,
  metric_name text,
  metric_unit text,
  quantity numeric(15,4),
  unit_price numeric(15,6),
  subtotal numeric(15,2),
  metadata jsonb,
  created_at timestamptz DEFAULT now(),
  updated_at timestamptz DEFAULT now(),
  -- ADR-002: Constraint métrica vs atributo
  CONSTRAINT chk_metric_or_attribute CHECK (
    (quantity IS NOT NULL AND metric_name IS NOT NULL AND metric_unit IS NOT NULL)
    OR
    (quantity IS NULL AND metadata IS NOT NULL AND metadata->>'type' = 'attribute')
  )
);

-- supplier_tags (schema-only no MVP, sem UI)
CREATE TABLE supplier_tags (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id),
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  tag_id uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  created_at timestamptz DEFAULT now(),
  UNIQUE(supplier_id, tag_id)
);
```

#### Migration 002: Alterar tabelas existentes

```sql
-- Adicionar supplier_id nas tabelas existentes
ALTER TABLE transactions ADD COLUMN supplier_id uuid REFERENCES suppliers(id);
ALTER TABLE statement_items ADD COLUMN supplier_id uuid REFERENCES suppliers(id);
ALTER TABLE recurring_templates ADD COLUMN supplier_id uuid REFERENCES suppliers(id);
ALTER TABLE documents ADD COLUMN supplier_id uuid REFERENCES suppliers(id);
ALTER TABLE liabilities ADD COLUMN supplier_id uuid REFERENCES suppliers(id);

-- ADR-001: Campo origin_type em transactions
ALTER TABLE transactions ADD COLUMN origin_type text DEFAULT 'manual'
  CHECK (origin_type IN ('manual', 'import', 'recurring', 'statement_link'));

-- ADR-001: UNIQUE index para vínculo 1:1 statement_item ↔ transaction
CREATE UNIQUE INDEX idx_statement_items_transaction_id
  ON statement_items(transaction_id) WHERE transaction_id IS NOT NULL;
```

#### Migration 003: Índices

```sql
-- Extensão trigram para busca fuzzy
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Índices de FK
CREATE INDEX idx_suppliers_user_id ON suppliers(user_id);
CREATE INDEX idx_supplier_aliases_supplier_id ON supplier_aliases(supplier_id);
CREATE INDEX idx_supplier_aliases_user_id ON supplier_aliases(user_id);
CREATE INDEX idx_supplier_contracts_supplier_id ON supplier_contracts(supplier_id);
CREATE INDEX idx_consumption_metrics_supplier_id ON consumption_metrics(supplier_id);
CREATE INDEX idx_supplier_tags_supplier_id ON supplier_tags(supplier_id);

-- ADR-003: Índices trigram para busca fuzzy
CREATE INDEX idx_suppliers_name_trgm ON suppliers USING gin (name gin_trgm_ops);
CREATE INDEX idx_supplier_aliases_trgm ON supplier_aliases USING gin (alias_name gin_trgm_ops);

-- Índice para resolução temporal de alias
CREATE INDEX idx_supplier_aliases_resolve
  ON supplier_aliases(user_id, alias_name, valid_from, valid_until)
  WHERE is_active = true;

-- Índice supplier_id nas tabelas existentes
CREATE INDEX idx_transactions_supplier_id ON transactions(supplier_id);
CREATE INDEX idx_statement_items_supplier_id ON statement_items(supplier_id);
CREATE INDEX idx_recurring_templates_supplier_id ON recurring_templates(supplier_id);
CREATE INDEX idx_documents_supplier_id ON documents(supplier_id);
CREATE INDEX idx_liabilities_supplier_id ON liabilities(supplier_id);
```

#### Migration 004: Triggers (ADR-003)

```sql
-- ADR-003: Trigger de unicidade temporal de alias
CREATE OR REPLACE FUNCTION check_alias_uniqueness()
RETURNS TRIGGER AS $$
BEGIN
  IF EXISTS (
    SELECT 1 FROM supplier_aliases sa
    WHERE sa.user_id = NEW.user_id
      AND sa.id != COALESCE(NEW.id, '00000000-0000-0000-0000-000000000000'::uuid)
      AND LOWER(sa.alias_name) = LOWER(NEW.alias_name)
      AND sa.is_active = true
      AND (
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

-- ADR-003: Trigger de auto-alias em renomeação
CREATE OR REPLACE FUNCTION auto_alias_on_rename()
RETURNS TRIGGER AS $$
BEGIN
  IF OLD.name IS DISTINCT FROM NEW.name THEN
    INSERT INTO supplier_aliases (user_id, supplier_id, alias_name, alias_type, valid_until)
    VALUES (NEW.user_id, NEW.id, OLD.name, 'former_name', NOW());
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_auto_alias_on_rename
  BEFORE UPDATE ON suppliers
  FOR EACH ROW EXECUTE FUNCTION auto_alias_on_rename();

-- Trigger updated_at para todas as tabelas novas
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_suppliers_updated_at BEFORE UPDATE ON suppliers
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_supplier_aliases_updated_at BEFORE UPDATE ON supplier_aliases
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_supplier_contracts_updated_at BEFORE UPDATE ON supplier_contracts
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();
CREATE TRIGGER trg_consumption_metrics_updated_at BEFORE UPDATE ON consumption_metrics
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();
```

#### Migration 005: RLS Policies

```sql
-- Habilitar RLS
ALTER TABLE suppliers ENABLE ROW LEVEL SECURITY;
ALTER TABLE supplier_aliases ENABLE ROW LEVEL SECURITY;
ALTER TABLE supplier_contracts ENABLE ROW LEVEL SECURITY;
ALTER TABLE consumption_metrics ENABLE ROW LEVEL SECURITY;
ALTER TABLE supplier_tags ENABLE ROW LEVEL SECURITY;

-- Policies: cada usuário vê apenas seus dados
CREATE POLICY suppliers_user_policy ON suppliers
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY supplier_aliases_user_policy ON supplier_aliases
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY supplier_contracts_user_policy ON supplier_contracts
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY consumption_metrics_user_policy ON consumption_metrics
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY supplier_tags_user_policy ON supplier_tags
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);
```

### 1.2. Entregável

- [ ] Todas as 5 migrations aplicadas com sucesso no Supabase local
- [ ] RLS testado: usuário A não vê dados do usuário B
- [ ] Triggers testados: unicidade temporal de alias, auto-alias em renomeação
- [ ] Constraint `chk_metric_or_attribute` testado com INSERT válido e inválido
- [ ] UNIQUE INDEX `idx_statement_items_transaction_id` testado
- [ ] Extensão `pg_trgm` habilitada e índices trigram criados

### 1.3. O que Fazer Manualmente

1. **Criar projeto Supabase local** (se ainda não existe):

   ```bash
   npx supabase init
   npx supabase start
   ```

2. **Criar os arquivos de migration** no diretório `supabase/migrations/`:

   ```bash
   npx supabase migration new create_supplier_tables
   npx supabase migration new alter_existing_tables_supplier
   npx supabase migration new create_supplier_indexes
   npx supabase migration new create_supplier_triggers
   npx supabase migration new create_supplier_rls
   ```

3. **Aplicar migrations:**

   ```bash
   npx supabase db reset  # aplica todas as migrations do zero
   ```

4. **Verificar no Supabase Studio** (localhost:54323):
   - Tabelas criadas com colunas e tipos corretos
   - RLS habilitado em todas as tabelas
   - Triggers listados em cada tabela

### 1.4. Critérios de Aceitação

- Migration aplica sem erros
- `INSERT` em `consumption_metrics` sem `quantity` nem `metadata.type='attribute'` → falha
- `INSERT` de alias duplicado no mesmo período → falha (trigger)
- `UPDATE` de `suppliers.name` → cria alias automático (trigger)
- `SELECT` com `auth.uid()` diferente → retorna vazio (RLS)

---

## Etapa 2 — Contrato Comportamental

**Objetivo:** Escrever TODOS os testes mandatórios como suíte de regressão. Os testes devem FALHAR (red) porque a implementação ainda não existe.

### 2.1. Testes a Escrever

#### Testes de Domínio/Fornecedor (T18-T27 do refino)

| #   | Teste                     | Cenário                                                                                           |
| --- | ------------------------- | ------------------------------------------------------------------------------------------------- |
| T18 | CRUD fornecedor           | DADO novo fornecedor → QUANDO criar → ENTÃO persiste com name, type, document_number              |
| T19 | Alias com autocomplete    | DADO fornecedor "Neoenergia" com alias "CELPE" → QUANDO buscar "CEL" → ENTÃO retorna "Neoenergia" |
| T20 | Merge de fornecedores     | DADO dois fornecedores A e B → QUANDO merge A→B → ENTÃO transações migram, A.is_active=false      |
| T21 | Fornecedor em recorrência | DADO template com supplier_id → QUANDO gerar instâncias → ENTÃO instâncias herdam fornecedor      |
| T22 | Relatório por fornecedor  | DADO 3 transações para "Neoenergia" → QUANDO relatório → ENTÃO total correto, deduplicado         |
| T23 | Fornecedor na home        | DADO fornecedor com vencimentos próximos → QUANDO home → ENTÃO aparece com nome e valor           |
| T24 | Associação a documentos   | DADO documento (PDF conta de luz) → QUANDO associar a fornecedor → ENTÃO vínculo persiste         |
| T25 | Métricas de consumo       | DADO fornecedor energia → QUANDO registrar 320 kWh → ENTÃO métrica persiste com quantity/unit     |
| T26 | Histórico de métricas     | DADO métricas em 3 meses → QUANDO consultar período → ENTÃO retorna série temporal                |
| T27 | Deduplicação (revisada)   | DADO statement_item com transaction_id → QUANDO relatório → ENTÃO conta apenas UMA vez            |

#### Testes de ADR-001 (Deduplicação)

| #          | Teste                            | Cenário                              |
| ---------- | -------------------------------- | ------------------------------------ |
| T-DEDUP-01 | statement_item com txn vinculada | Relatório conta APENAS a transaction |
| T-DEDUP-02 | statement_item sem txn vinculada | Relatório conta o statement_item     |
| T-DEDUP-03 | statement_payment excluído       | NUNCA aparece em soma de despesas    |
| T-DEDUP-04 | Mix vinculados + não vinculados  | Soma da view == soma esperada        |

#### Testes de ADR-002 (Norma de Métricas)

| #           | Teste                       | Cenário                                          |
| ----------- | --------------------------- | ------------------------------------------------ |
| T-METRIC-01 | Métrica válida              | quantity + unit → aceito                         |
| T-METRIC-02 | Atributo válido             | NULL quantity + metadata type=attribute → aceito |
| T-METRIC-03 | Sem quantity nem metadata   | → rejeitado pelo constraint                      |
| T-METRIC-04 | quantity sem unit           | → rejeitado pelo constraint                      |
| T-METRIC-05 | Série temporal de atributos | Bandeira por mês retorna corretamente            |

#### Testes de ADR-003 (Governança de Aliases)

| #          | Teste                               | Cenário                      |
| ---------- | ----------------------------------- | ---------------------------- |
| T-ALIAS-01 | Alias duplicado mesmo período       | → rejeitado                  |
| T-ALIAS-02 | Alias duplicado períodos diferentes | → aceito                     |
| T-ALIAS-03 | Alias inativo em resolução          | → não encontra               |
| T-ALIAS-04 | Auto-alias em renomeação            | → cria former_name           |
| T-ALIAS-05 | Merge migra referências             | → todas as tabelas migradas  |
| T-ALIAS-06 | Merge não deleta absorvido          | → is_active = false          |
| T-ALIAS-07 | Merge registra audit_log            | → log completo               |
| T-ALIAS-08 | Busca fuzzy trigram                 | → encontra por alias ou nome |
| T-ALIAS-09 | Resolução temporal                  | Alias expirado não resolve   |

### 2.2. Estrutura de Arquivos de Teste

```
src/
  __tests__/
    domain/
      supplier.test.ts       # T18-T24
      supplier-metrics.test.ts # T25-T26, T-METRIC-01 a 05
      deduplication.test.ts    # T27, T-DEDUP-01 a 04
      alias-governance.test.ts # T-ALIAS-01 a 09
```

### 2.3. Entregável

- [ ] 27+ testes escritos e rodando (todos FALHANDO — red)
- [ ] Suíte organizada por domínio (supplier, metrics, dedup, alias)
- [ ] Cada teste tem DADO/QUANDO/ENTÃO documentado no `describe`/`it`
- [ ] Nenhum teste depende de outro (isolamento)

### 2.4. O que Fazer Manualmente

1. **Criar estrutura de diretórios de teste:**

   ```bash
   mkdir -p __tests__/domain
   mkdir -p __tests__/integration
   ```

   > **Nota:** No monorepo, os testes de domínio ficam em `__tests__/domain/` (root) ou `packages/domain/src/__tests__/`. Os testes de integração (Supabase) ficam em `__tests__/integration/`.

2. **Configurar Vitest** (se não configurado):

   ```bash
   bun add -d vitest @vitest/coverage-v8
   ```

   Adicionar em `package.json`:

   ```json
   {
     "scripts": {
       "test": "vitest run",
       "test:watch": "vitest"
     }
   }
   ```

3. **Escrever os testes** seguindo a tabela acima. Cada arquivo de teste importa do módulo que será criado na Etapa 3.

4. **Rodar e confirmar que TODOS falham:**
   ```bash
   bun run test
   ```

### 2.5. Critérios de Aceitação

- Todos os testes compilam sem erro de sintaxe
- Todos os testes FALHAM (red) — nenhum passa
- A suíte cobre todos os cenários listados nas tabelas acima
- Os testes são reproduzíveis e isolados

---

## Etapa 3 — Núcleo Funcional de Fornecedor

**Objetivo:** Implementar CRUD, aliases, autocomplete e associações até que os testes T18-T24 passem.

### 3.1. Funcionalidades a Implementar

| #     | Feature                | Descrição                                                       |
| ----- | ---------------------- | --------------------------------------------------------------- |
| 3.1.1 | CRUD Supplier          | Criar, ler, atualizar, desativar fornecedor                     |
| 3.1.2 | CRUD Alias             | Criar, ler, atualizar, desativar alias (com validação temporal) |
| 3.1.3 | Autocomplete           | Busca por nome + alias com trigram                              |
| 3.1.4 | Associar a transação   | Campo supplier_id em form de transação                          |
| 3.1.5 | Associar a recorrência | Campo supplier_id em template recorrente                        |
| 3.1.6 | Associar a documento   | Campo supplier_id em upload de documento                        |
| 3.1.7 | CRUD Contrato          | Criar, ler, atualizar contratos de fornecedor                   |

### 3.2. API Endpoints (Supabase Client)

```typescript
// Exemplos de chamadas via supabase-js

// 3.1.1 CRUD Supplier
const { data } = await supabase.from('suppliers').insert({ name, type, ... })
const { data } = await supabase.from('suppliers').select('*').eq('is_active', true)
const { data } = await supabase.from('suppliers').update({ name }).eq('id', id)
const { data } = await supabase.from('suppliers').update({ is_active: false }).eq('id', id)

// 3.1.2 CRUD Alias
const { data } = await supabase.from('supplier_aliases').insert({ supplier_id, alias_name, ... })

// 3.1.3 Autocomplete (via RPC ou query combinada)
const { data } = await supabase.rpc('search_suppliers', { query: 'CEL', limit: 10 })
```

### 3.3. Componentes de UI

| Componente             | Descrição                             |
| ---------------------- | ------------------------------------- |
| `SupplierForm`         | Form de criação/edição de fornecedor  |
| `SupplierList`         | Lista de fornecedores com filtros     |
| `SupplierAutocomplete` | Input com autocomplete por nome/alias |
| `AliasList`            | Lista de aliases de um fornecedor     |
| `AliasForm`            | Form de criação de alias              |
| `ContractList`         | Lista de contratos de um fornecedor   |
| `ContractForm`         | Form de criação/edição de contrato    |

### 3.4. Entregável

- [ ] CRUD de fornecedor funcional (api + UI)
- [ ] CRUD de alias funcional (api + UI)
- [ ] Autocomplete de fornecedor por nome + alias (trigram)
- [ ] Associação de supplier_id em transações (form atualizado)
- [ ] Associação de supplier_id em templates recorrentes (form atualizado)
- [ ] Associação de supplier_id em documentos (form atualizado)
- [ ] Testes T18, T19, T21, T23, T24 passando (green)

### 3.5. O que Fazer Manualmente

1. **Criar RPC para autocomplete** no Supabase:

   ```sql
   -- Em supabase/migrations/
   CREATE OR REPLACE FUNCTION search_suppliers(search_query text, result_limit int DEFAULT 10)
   RETURNS TABLE(id uuid, name text, match_type text, match_score real) AS $$
   BEGIN
     RETURN QUERY
     SELECT DISTINCT s.id, s.name,
       CASE
         WHEN LOWER(s.name) = LOWER(search_query) THEN 'exact_name'
         WHEN s.name ILIKE search_query || '%' THEN 'prefix_name'
         WHEN s.name % search_query THEN 'fuzzy_name'
         ELSE 'fuzzy_alias'
       END AS match_type,
       GREATEST(
         similarity(s.name, search_query),
         COALESCE((SELECT MAX(similarity(sa.alias_name, search_query))
           FROM supplier_aliases sa
           WHERE sa.supplier_id = s.id AND sa.is_active = true), 0)
       ) AS match_score
     FROM suppliers s
     WHERE s.user_id = auth.uid() AND s.is_active = true
       AND (
         s.name % search_query
         OR s.name ILIKE search_query || '%'
         OR EXISTS (
           SELECT 1 FROM supplier_aliases sa
           WHERE sa.supplier_id = s.id AND sa.is_active = true
             AND (sa.alias_name % search_query OR sa.alias_name ILIKE search_query || '%')
         )
       )
     ORDER BY match_score DESC
     LIMIT result_limit;
   END;
   $$ LANGUAGE plpgsql SECURITY DEFINER;
   ```

2. **Criar componentes de UI** na pasta `apps/web/src/components/supplier/`

3. **Atualizar forms existentes** (transaction, recurring_template, document) para incluir `SupplierAutocomplete`

4. **Rodar testes:**
   ```bash
   bun run test -- --grep "T18|T19|T21|T23|T24"
   ```

### 3.6. Critérios de Aceitação

- Testes T18, T19, T21, T23, T24 passando
- Testes T-ALIAS-01 a T-ALIAS-04, T-ALIAS-08, T-ALIAS-09 passando
- Form de transação tem campo de fornecedor com autocomplete
- Form de template recorrente tem campo de fornecedor
- Upload de documento permite associar fornecedor

---

## Etapa 4 — Relatórios e Filtros

**Objetivo:** Implementar view de deduplicação, relatório por fornecedor, filtros compostos e presença do fornecedor na home.

### 4.1. Funcionalidades a Implementar

| #     | Feature                  | Descrição                                           |
| ----- | ------------------------ | --------------------------------------------------- |
| 4.1.1 | View deduplicação        | `v_expenses_deduplicated` no banco                  |
| 4.1.2 | Relatório por fornecedor | Gastos agrupados por supplier_id                    |
| 4.1.3 | Filtros compostos        | Fornecedor × categoria × tag × período × prioridade |
| 4.1.4 | Fornecedor na home       | Nome nos itens da fila, top fornecedores            |

### 4.2. View SQL

```sql
-- Migration: create_dedup_view
CREATE VIEW v_expenses_deduplicated AS
-- statement_items SEM transação vinculada
SELECT
  si.id AS canonical_id,
  'statement_item' AS source_type,
  si.user_id,
  si.amount,
  si.description,
  si.supplier_id,
  NULL::uuid AS category_id,
  NULL::text AS priority,
  si.transaction_date AS event_date,
  NULL::date AS competence_date,
  NULL::uuid AS financial_period_id,
  si.statement_cycle_id
FROM statement_items si
WHERE si.transaction_id IS NULL

UNION ALL

-- transactions de despesa (incluindo vinculadas a statement_items)
SELECT
  t.id AS canonical_id,
  'transaction' AS source_type,
  t.user_id,
  t.amount,
  t.description,
  t.supplier_id,
  t.category_id,
  t.priority,
  t.event_date,
  t.competence_date,
  t.financial_period_id,
  t.statement_cycle_id
FROM transactions t
WHERE t.type IN ('expense', 'fee', 'interest_charge');
```

### 4.3. Componentes de UI

| Componente          | Descrição                                                 |
| ------------------- | --------------------------------------------------------- |
| `SupplierReport`    | Tabela/gráfico de gastos por fornecedor                   |
| `SupplierFilter`    | Dropdown de filtro por fornecedor                         |
| `CompositeFilter`   | Barra de filtros: fornecedor + categoria + tag + período  |
| `HomeSupplierBadge` | Badge com nome do fornecedor em cada item da fila da home |

### 4.4. Entregável

- [ ] View `v_expenses_deduplicated` criada no banco
- [ ] Relatório de gastos por fornecedor funcional
- [ ] Filtro por fornecedor nos relatórios existentes
- [ ] Filtros compostos (fornecedor × categoria × tag × período)
- [ ] Nome do fornecedor visível nos itens da home operacional
- [ ] Testes T22, T27 passando (green)
- [ ] Testes T-DEDUP-01 a T-DEDUP-04 passando (green)

### 4.5. O que Fazer Manualmente

1. **Criar migration** para a view:

   ```bash
   npx supabase migration new create_expenses_dedup_view
   ```

2. **Criar componentes** em `src/components/reports/`

3. **Atualizar home operacional** para exibir `supplier.name` em cada item

4. **Testar manualmente:** Criar cenário com:
   - 2 statement_items sem transaction_id
   - 1 statement_item COM transaction_id (vinculado)
   - 1 transaction manual (sem statement_item)
   - Verificar: relatório mostra 4 itens, não 5

5. **Rodar testes:**
   ```bash
   bun run test -- --grep "T22|T27|DEDUP"
   ```

### 4.6. Critérios de Aceitação

- View retorna resultados corretos (sem duplicação)
- Relatório por fornecedor agrupa e totaliza corretamente
- Filtro por fornecedor funciona sozinho e combinado com outros filtros
- Home mostra nome do fornecedor em itens relevantes
- Todos os testes de deduplicação passando

---

## Etapa 5 — Recursos Avançados

**Objetivo:** Implementar métricas de consumo, associação retroativa, merge de fornecedores, e view materializada.

### 5.1. Funcionalidades a Implementar

| #     | Feature               | Descrição                                            |
| ----- | --------------------- | ---------------------------------------------------- |
| 5.1.1 | CRUD métricas         | Registrar, listar, editar métricas de consumo        |
| 5.1.2 | Associação retroativa | Sugerir fornecedor para transações antigas por alias |
| 5.1.3 | Merge atômico         | Edge Function para merge de fornecedores             |
| 5.1.4 | View materializada    | `mv_supplier_spending` para performance              |
| 5.1.5 | Enriquecer audit      | Logs detalhados de todas operações de fornecedor     |

### 5.2. Edge Function: Merge de Fornecedores

Criar em `supabase/functions/merge-suppliers/index.ts`:

```typescript
import { createClient } from "@supabase/supabase-js";

Deno.serve(async (req) => {
  const { source_supplier_id, target_supplier_id } = await req.json();

  // Usar service_role_key para bypass de RLS
  const supabase = createClient(
    Deno.env.get("SUPABASE_URL")!,
    Deno.env.get("SUPABASE_SERVICE_ROLE_KEY")!,
  );

  // Validar que ambos pertencem ao mesmo user_id
  // Executar as 8 etapas em transação (via SQL raw)
  // Registrar no audit_log
  // Retornar resultado com contagem de migrações
});
```

### 5.3. Edge Function: Associação Retroativa

Criar em `supabase/functions/retroactive-supplier-association/index.ts`:

```typescript
// Buscar transações sem supplier_id
// Para cada, verificar se description tem match com aliases existentes
// Retornar lista de candidatos para confirmação humana
// Usuário confirma em lote ou individualmente
// Aplicar atualizações
```

### 5.4. View Materializada

```sql
CREATE MATERIALIZED VIEW mv_supplier_spending AS
SELECT
  v.user_id,
  v.supplier_id,
  s.name AS supplier_name,
  s.type AS supplier_type,
  date_trunc('month', v.event_date) AS month,
  COUNT(*) AS transaction_count,
  SUM(v.amount) AS total_amount,
  AVG(v.amount) AS avg_amount
FROM v_expenses_deduplicated v
JOIN suppliers s ON s.id = v.supplier_id
WHERE v.supplier_id IS NOT NULL
GROUP BY v.user_id, v.supplier_id, s.name, s.type, date_trunc('month', v.event_date);

-- Refresh policy (via cron ou Edge Function)
-- REFRESH MATERIALIZED VIEW CONCURRENTLY mv_supplier_spending;
```

### 5.5. Componentes de UI

| Componente                   | Descrição                                       |
| ---------------------------- | ----------------------------------------------- |
| `MetricsForm`                | Form para registrar métrica/atributo de consumo |
| `MetricsTimeline`            | Série temporal de métricas por fornecedor       |
| `MergeSupplierDialog`        | Diálogo de merge com preview de impacto         |
| `RetroactiveAssociationList` | Lista de sugestões para associação retroativa   |

### 5.6. Entregável

- [ ] CRUD de métricas funcional com constraint validada
- [ ] Edge Function de merge implantada e testada
- [ ] Edge Function de associação retroativa implantada
- [ ] View materializada criada com refresh configurado
- [ ] Testes T25, T26 passando (green)
- [ ] Testes T-METRIC-01 a T-METRIC-05 passando (green)
- [ ] Testes T20, T-ALIAS-05, T-ALIAS-06, T-ALIAS-07 passando (green)
- [ ] TODOS os testes passando (green) — suíte completa

### 5.7. O que Fazer Manualmente

1. **Criar Edge Functions:**

   ```bash
   npx supabase functions new merge-suppliers
   npx supabase functions new retroactive-supplier-association
   ```

2. **Deploy Edge Functions:**

   ```bash
   npx supabase functions deploy merge-suppliers
   npx supabase functions deploy retroactive-supplier-association
   ```

3. **Criar migration para view materializada:**

   ```bash
   npx supabase migration new create_supplier_spending_mv
   ```

4. **Configurar refresh da view materializada** (cron job no Supabase ou Edge Function scheduled)

5. **Rodar suíte completa:**
   ```bash
   bun run test
   ```

### 5.8. Critérios de Aceitação

- TODOS os 27+ testes passando (green)
- Edge Function de merge executa em transação atômica
- Métricas respeitam constraint (métrica vs atributo)
- View materializada atualiza corretamente
- Associação retroativa exige confirmação humana

---

## Checklist Geral

| Etapa | Entregável Principal                                    | Status |
| ----- | ------------------------------------------------------- | ------ |
| 1     | Banco 100% estruturado, vazio, com RLS e constraints    | ⬜     |
| 2     | 27+ testes escritos e falhando (red)                    | ⬜     |
| 3     | CRUD fornecedor + aliases + associações (T18-T24 green) | ⬜     |
| 4     | Relatórios + filtros + home (T22, T27, DEDUP green)     | ⬜     |
| 5     | Métricas + merge + retroativo (TODOS green)             | ⬜     |

---

## Referências

| Documento                | Caminho                                                                                      |
| ------------------------ | -------------------------------------------------------------------------------------------- |
| Ata do checkpoint        | `docs/refinos/2026-03/2026-03-21-13-30-checkpoint-pre-implementacao-ajustes-obrigatorios.md` |
| ADR-001 Deduplicação     | `docs/planejamento/ADR-001-deduplicacao-transacao-item-fatura.md`                            |
| ADR-002 Métricas         | `docs/planejamento/ADR-002-norma-consumption-metrics.md`                                     |
| ADR-003 Aliases          | `docs/planejamento/ADR-003-governanca-aliases-fornecedor.md`                                 |
| Refino kickoff           | `docs/refinos/2026-03/2026-03-21-10-40-refino-tecnico-funcional-kickoff-seu-bolso-feliz.md`  |
| Refino fornecedor        | `docs/refinos/2026-03/2026-03-21-11-57-revisao-refino-dimensao-fornecedor.md`                |
| Prompt inicial Verônica  | `docs/Veronica/001-prompt.inicial.md`                                                        |
| Especificação fornecedor | `docs/Veronica/002-fornecedor.md`                                                            |
