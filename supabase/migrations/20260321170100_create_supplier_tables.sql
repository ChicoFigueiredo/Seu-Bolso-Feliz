-- ============================================================
-- Migration 002: Tabelas de Fornecedor — Seu Bolso Feliz
-- ============================================================
-- ADR-001, ADR-002, ADR-003
-- suppliers, supplier_aliases, supplier_contracts,
-- consumption_metrics, supplier_tags
-- ============================================================

-- ============================================================
-- 1. Fornecedores
-- ============================================================
CREATE TABLE suppliers (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  name text NOT NULL,
  trade_name text,
  legal_name text,
  document_number text,
  type text NOT NULL DEFAULT 'company'
    CHECK (type IN ('company', 'individual', 'government', 'utility', 'telecom', 'saas', 'platform', 'other')),
  website text,
  contact_info jsonb,
  notes text,
  institution_id uuid REFERENCES institutions(id) ON DELETE SET NULL,
  is_active boolean NOT NULL DEFAULT true,
  display_order int,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 2. Aliases de fornecedor (ADR-003)
-- ============================================================
CREATE TABLE supplier_aliases (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  alias_name text NOT NULL,
  alias_type text NOT NULL DEFAULT 'other'
    CHECK (alias_type IN ('former_name', 'abbreviation', 'trade_name', 'billing_name', 'other')),
  is_active boolean NOT NULL DEFAULT true,
  valid_from date,
  valid_until date,
  notes text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 3. Contratos com fornecedor
-- ============================================================
CREATE TABLE supplier_contracts (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  contract_type text NOT NULL
    CHECK (contract_type IN ('service', 'subscription', 'utility', 'loan', 'insurance', 'maintenance', 'other')),
  identifier text,
  label text,
  is_active boolean NOT NULL DEFAULT true,
  start_date date,
  end_date date,
  metadata jsonb,
  notes text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 4. Métricas de consumo (ADR-002)
-- ============================================================
CREATE TABLE consumption_metrics (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  supplier_contract_id uuid REFERENCES supplier_contracts(id) ON DELETE SET NULL,
  transaction_id uuid REFERENCES transactions(id) ON DELETE SET NULL,
  document_id uuid REFERENCES documents(id) ON DELETE SET NULL,
  reference_period_start date NOT NULL,
  reference_period_end date NOT NULL,
  metric_name text,
  metric_unit text,
  quantity numeric(15,4),
  unit_price numeric(15,6),
  subtotal numeric(15,2),
  metadata jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  -- ADR-002: Constraint métrica vs atributo
  CONSTRAINT chk_metric_or_attribute CHECK (
    (quantity IS NOT NULL AND metric_name IS NOT NULL AND metric_unit IS NOT NULL)
    OR
    (quantity IS NULL AND metadata IS NOT NULL AND metadata->>'type' = 'attribute')
  ),
  CONSTRAINT chk_reference_period CHECK (reference_period_end >= reference_period_start)
);

-- ============================================================
-- 5. Tags de fornecedor (schema-only no MVP)
-- ============================================================
CREATE TABLE supplier_tags (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  supplier_id uuid NOT NULL REFERENCES suppliers(id) ON DELETE CASCADE,
  tag_id uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE(supplier_id, tag_id)
);
