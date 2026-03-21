-- ============================================================
-- Migration 001: Tabelas Core Fundacionais — Seu Bolso Feliz
-- ============================================================
-- Cria todas as tabelas base do domínio financeiro.
-- Ordem de criação respeita dependências de FK.
-- ============================================================

-- ── Extensões necessárias ──
CREATE EXTENSION IF NOT EXISTS pg_trgm;
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ============================================================
-- 1. Preferências financeiras do usuário
-- ============================================================
CREATE TABLE user_financial_preferences (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL UNIQUE REFERENCES auth.users(id) ON DELETE CASCADE,
  financial_cycle_start_day int CHECK (financial_cycle_start_day BETWEEN 1 AND 31),
  financial_cycle_anchor_date date,
  default_currency text NOT NULL DEFAULT 'BRL',
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 2. Instituições financeiras
-- ============================================================
CREATE TABLE institutions (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  name text NOT NULL,
  type text NOT NULL DEFAULT 'bank'
    CHECK (type IN ('bank', 'fintech', 'broker', 'other')),
  icon_url text,
  color text,
  is_active boolean NOT NULL DEFAULT true,
  display_order int,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 3. Produtos financeiros (contas, cartões, empréstimos, etc.)
-- ============================================================
CREATE TABLE financial_products (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  institution_id uuid NOT NULL REFERENCES institutions(id) ON DELETE CASCADE,
  name text NOT NULL,
  type text NOT NULL
    CHECK (type IN ('checking_account', 'savings_account', 'credit_card', 'overdraft', 'personal_loan', 'mortgage', 'investment', 'other')),
  current_balance numeric(15,2) DEFAULT 0,
  credit_limit numeric(15,2),
  is_active boolean NOT NULL DEFAULT true,
  display_order int,
  metadata jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 4. Cartões
-- ============================================================
CREATE TABLE cards (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  financial_product_id uuid NOT NULL REFERENCES financial_products(id) ON DELETE CASCADE,
  last_four_digits text,
  card_brand text,
  is_primary boolean NOT NULL DEFAULT true,
  holder_name text,
  credit_limit numeric(15,2),
  closing_day int CHECK (closing_day BETWEEN 1 AND 31),
  due_day int CHECK (due_day BETWEEN 1 AND 31),
  is_active boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 5. Categorias (hierárquicas)
-- ============================================================
CREATE TABLE categories (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  name text NOT NULL,
  parent_id uuid REFERENCES categories(id) ON DELETE SET NULL,
  icon text,
  color text,
  display_order int,
  is_active boolean NOT NULL DEFAULT true,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 6. Tags
-- ============================================================
CREATE TABLE tags (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  name text NOT NULL,
  color text,
  influences_priority boolean NOT NULL DEFAULT false,
  suggested_priority text
    CHECK (suggested_priority IS NULL OR suggested_priority IN ('essential', 'high', 'medium', 'low', 'optional')),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 7. Períodos financeiros personalizados
-- ============================================================
CREATE TABLE financial_periods (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  start_date date NOT NULL,
  end_date date NOT NULL,
  label text,
  is_current boolean NOT NULL DEFAULT false,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now(),
  CONSTRAINT chk_period_dates CHECK (end_date >= start_date)
);

-- ============================================================
-- 8. Ciclos de fatura (statement cycles)
-- ============================================================
CREATE TABLE statement_cycles (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  card_id uuid NOT NULL REFERENCES cards(id) ON DELETE CASCADE,
  reference_month date NOT NULL,
  cycle_start_date date NOT NULL,
  cycle_end_date date NOT NULL,
  due_date date NOT NULL,
  total_amount numeric(15,2) DEFAULT 0,
  paid_amount numeric(15,2) DEFAULT 0,
  status text NOT NULL DEFAULT 'open'
    CHECK (status IN ('open', 'closed', 'paid', 'partial', 'overdue')),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 9. Passivos / Dívidas
-- ============================================================
CREATE TABLE liabilities (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  financial_product_id uuid NOT NULL REFERENCES financial_products(id) ON DELETE CASCADE,
  name text NOT NULL,
  type text NOT NULL
    CHECK (type IN ('personal_loan', 'mortgage', 'overdraft', 'installment_plan', 'other')),
  original_amount numeric(15,2) NOT NULL,
  outstanding_balance numeric(15,2) NOT NULL,
  interest_rate numeric(8,6),
  rate_type text CHECK (rate_type IS NULL OR rate_type IN ('monthly', 'annual')),
  amortization_system text CHECK (amortization_system IS NULL OR amortization_system IN ('sac', 'price', 'mixed', 'other', 'none')),
  total_installments int,
  paid_installments int NOT NULL DEFAULT 0,
  start_date date,
  end_date date,
  status text NOT NULL DEFAULT 'active'
    CHECK (status IN ('active', 'paid_off', 'renegotiated', 'defaulted')),
  metadata jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 10. Parcelas de dívida
-- ============================================================
CREATE TABLE liability_installments (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  liability_id uuid NOT NULL REFERENCES liabilities(id) ON DELETE CASCADE,
  installment_number int NOT NULL,
  due_date date NOT NULL,
  total_amount numeric(15,2) NOT NULL,
  principal_amount numeric(15,2),
  interest_amount numeric(15,2),
  insurance_amount numeric(15,2),
  fee_amount numeric(15,2),
  paid_amount numeric(15,2) NOT NULL DEFAULT 0,
  paid_date date,
  status text NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'paid', 'partial', 'overdue', 'waived')),
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 11. Transações (sem recurring_instance_id FK por enquanto — circular)
-- ============================================================
CREATE TABLE transactions (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  financial_product_id uuid NOT NULL REFERENCES financial_products(id) ON DELETE CASCADE,
  type text NOT NULL
    CHECK (type IN ('income', 'expense', 'refund', 'adjustment', 'interest_charge', 'fee', 'statement_payment', 'liability_payment')),
  amount numeric(15,2) NOT NULL,
  description text,
  event_date date NOT NULL,
  competence_date date,
  financial_period_id uuid REFERENCES financial_periods(id) ON DELETE SET NULL,
  statement_cycle_id uuid REFERENCES statement_cycles(id) ON DELETE SET NULL,
  liability_installment_id uuid REFERENCES liability_installments(id) ON DELETE SET NULL,
  category_id uuid REFERENCES categories(id) ON DELETE SET NULL,
  priority text CHECK (priority IS NULL OR priority IN ('essential', 'high', 'medium', 'low', 'optional')),
  is_confirmed boolean NOT NULL DEFAULT false,
  -- recurring_instance_id será adicionado após criar recurring_instances
  notes text,
  metadata jsonb,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ADR-001: Campo origin_type em transactions
ALTER TABLE transactions ADD COLUMN origin_type text NOT NULL DEFAULT 'manual'
  CHECK (origin_type IN ('manual', 'import', 'recurring', 'statement_link'));

-- ============================================================
-- 12. Transferências internas
-- ============================================================
CREATE TABLE transfers (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  source_product_id uuid NOT NULL REFERENCES financial_products(id) ON DELETE CASCADE,
  target_product_id uuid NOT NULL REFERENCES financial_products(id) ON DELETE CASCADE,
  amount numeric(15,2) NOT NULL,
  description text,
  event_date date NOT NULL,
  competence_date date,
  financial_period_id uuid REFERENCES financial_periods(id) ON DELETE SET NULL,
  is_confirmed boolean NOT NULL DEFAULT false,
  notes text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 13. Templates recorrentes
-- ============================================================
CREATE TABLE recurring_templates (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  financial_product_id uuid REFERENCES financial_products(id) ON DELETE SET NULL,
  name text NOT NULL,
  type text NOT NULL
    CHECK (type IN ('income', 'expense', 'liability_payment', 'statement_payment')),
  amount numeric(15,2),
  is_variable_amount boolean NOT NULL DEFAULT false,
  frequency text NOT NULL
    CHECK (frequency IN ('monthly', 'weekly', 'biweekly', 'quarterly', 'annual', 'custom')),
  day_of_month int CHECK (day_of_month IS NULL OR day_of_month BETWEEN 1 AND 31),
  custom_interval_days int,
  category_id uuid REFERENCES categories(id) ON DELETE SET NULL,
  priority text CHECK (priority IS NULL OR priority IN ('essential', 'high', 'medium', 'low', 'optional')),
  starts_at date,
  ends_at date,
  is_active boolean NOT NULL DEFAULT true,
  notes text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 14. Instâncias de recorrência
-- ============================================================
CREATE TABLE recurring_instances (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  recurring_template_id uuid NOT NULL REFERENCES recurring_templates(id) ON DELETE CASCADE,
  expected_date date NOT NULL,
  expected_amount numeric(15,2),
  actual_amount numeric(15,2),
  status text NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'paid', 'partial', 'skipped', 'overdue', 'cancelled')),
  paid_date date,
  transaction_id uuid REFERENCES transactions(id) ON DELETE SET NULL,
  notes text,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- Resolver dependência circular: transactions.recurring_instance_id
ALTER TABLE transactions ADD COLUMN recurring_instance_id uuid
  REFERENCES recurring_instances(id) ON DELETE SET NULL;

-- ============================================================
-- 15. Itens de fatura (statement items)
-- ============================================================
CREATE TABLE statement_items (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  statement_cycle_id uuid NOT NULL REFERENCES statement_cycles(id) ON DELETE CASCADE,
  transaction_id uuid REFERENCES transactions(id) ON DELETE SET NULL,
  description text,
  amount numeric(15,2) NOT NULL,
  transaction_date date,
  installment_number int,
  total_installments int,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ADR-001: UNIQUE index para vínculo 1:1 statement_item ↔ transaction
CREATE UNIQUE INDEX idx_statement_items_transaction_id
  ON statement_items(transaction_id) WHERE transaction_id IS NOT NULL;

-- ============================================================
-- 16. Documentos e anexos
-- ============================================================
CREATE TABLE documents (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  name text NOT NULL,
  description text,
  file_path text NOT NULL,
  file_type text,
  file_size bigint,
  document_type text
    CHECK (document_type IS NULL OR document_type IN ('receipt', 'invoice', 'statement', 'contract', 'proof', 'other')),
  entity_type text,
  entity_id uuid,
  version int NOT NULL DEFAULT 1,
  is_password_protected boolean NOT NULL DEFAULT false,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 17. Segredos do usuário (senhas de PDF, etc.)
-- ============================================================
CREATE TABLE user_secrets (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  secret_type text NOT NULL,
  entity_type text,
  entity_id uuid,
  encrypted_value text NOT NULL,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 18. Jobs de importação
-- ============================================================
CREATE TABLE import_jobs (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  source_type text NOT NULL
    CHECK (source_type IN ('csv', 'xlsx', 'manual', 'api')),
  file_path text,
  status text NOT NULL DEFAULT 'pending'
    CHECK (status IN ('pending', 'processing', 'completed', 'failed', 'partial')),
  total_rows int,
  imported_rows int NOT NULL DEFAULT 0,
  skipped_rows int NOT NULL DEFAULT 0,
  error_rows int NOT NULL DEFAULT 0,
  error_details jsonb,
  started_at timestamptz,
  completed_at timestamptz,
  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 19. Audit log (imutável — sem updated_at)
-- ============================================================
CREATE TABLE audit_logs (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  action text NOT NULL,
  entity_type text,
  entity_id uuid,
  old_values jsonb,
  new_values jsonb,
  ip_address inet,
  created_at timestamptz NOT NULL DEFAULT now()
);

-- ============================================================
-- 20. Tabelas de junção N:N (tags)
-- ============================================================
CREATE TABLE transaction_tags (
  transaction_id uuid NOT NULL REFERENCES transactions(id) ON DELETE CASCADE,
  tag_id uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (transaction_id, tag_id)
);

CREATE TABLE recurring_template_tags (
  recurring_template_id uuid NOT NULL REFERENCES recurring_templates(id) ON DELETE CASCADE,
  tag_id uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (recurring_template_id, tag_id)
);

CREATE TABLE liability_tags (
  liability_id uuid NOT NULL REFERENCES liabilities(id) ON DELETE CASCADE,
  tag_id uuid NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
  PRIMARY KEY (liability_id, tag_id)
);
