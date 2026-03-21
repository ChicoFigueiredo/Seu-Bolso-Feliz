-- ============================================================
-- Migration 006: RLS Policies — Todas as tabelas
-- ============================================================
-- Cada usuário vê e manipula APENAS seus dados.
-- Padrão: FOR ALL USING (auth.uid() = user_id)
--         WITH CHECK (auth.uid() = user_id)
-- ============================================================

-- ── Habilitar RLS em TODAS as tabelas ──
ALTER TABLE user_financial_preferences ENABLE ROW LEVEL SECURITY;
ALTER TABLE institutions ENABLE ROW LEVEL SECURITY;
ALTER TABLE financial_products ENABLE ROW LEVEL SECURITY;
ALTER TABLE cards ENABLE ROW LEVEL SECURITY;
ALTER TABLE categories ENABLE ROW LEVEL SECURITY;
ALTER TABLE tags ENABLE ROW LEVEL SECURITY;
ALTER TABLE financial_periods ENABLE ROW LEVEL SECURITY;
ALTER TABLE statement_cycles ENABLE ROW LEVEL SECURITY;
ALTER TABLE statement_items ENABLE ROW LEVEL SECURITY;
ALTER TABLE liabilities ENABLE ROW LEVEL SECURITY;
ALTER TABLE liability_installments ENABLE ROW LEVEL SECURITY;
ALTER TABLE transactions ENABLE ROW LEVEL SECURITY;
ALTER TABLE transfers ENABLE ROW LEVEL SECURITY;
ALTER TABLE recurring_templates ENABLE ROW LEVEL SECURITY;
ALTER TABLE recurring_instances ENABLE ROW LEVEL SECURITY;
ALTER TABLE documents ENABLE ROW LEVEL SECURITY;
ALTER TABLE user_secrets ENABLE ROW LEVEL SECURITY;
ALTER TABLE import_jobs ENABLE ROW LEVEL SECURITY;
ALTER TABLE audit_logs ENABLE ROW LEVEL SECURITY;
ALTER TABLE suppliers ENABLE ROW LEVEL SECURITY;
ALTER TABLE supplier_aliases ENABLE ROW LEVEL SECURITY;
ALTER TABLE supplier_contracts ENABLE ROW LEVEL SECURITY;
ALTER TABLE consumption_metrics ENABLE ROW LEVEL SECURITY;
ALTER TABLE supplier_tags ENABLE ROW LEVEL SECURITY;

-- ── Tabelas de junção sem user_id direto — RLS via FK join ──
ALTER TABLE transaction_tags ENABLE ROW LEVEL SECURITY;
ALTER TABLE recurring_template_tags ENABLE ROW LEVEL SECURITY;
ALTER TABLE liability_tags ENABLE ROW LEVEL SECURITY;

-- ============================================================
-- Policies — tabelas com user_id direto
-- ============================================================

CREATE POLICY user_financial_preferences_policy ON user_financial_preferences
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY institutions_policy ON institutions
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY financial_products_policy ON financial_products
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY cards_policy ON cards
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY categories_policy ON categories
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY tags_policy ON tags
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY financial_periods_policy ON financial_periods
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY statement_cycles_policy ON statement_cycles
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY statement_items_policy ON statement_items
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY liabilities_policy ON liabilities
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY liability_installments_policy ON liability_installments
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY transactions_policy ON transactions
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY transfers_policy ON transfers
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY recurring_templates_policy ON recurring_templates
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY recurring_instances_policy ON recurring_instances
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY documents_policy ON documents
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY user_secrets_policy ON user_secrets
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY import_jobs_policy ON import_jobs
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY audit_logs_policy ON audit_logs
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY suppliers_policy ON suppliers
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY supplier_aliases_policy ON supplier_aliases
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY supplier_contracts_policy ON supplier_contracts
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY consumption_metrics_policy ON consumption_metrics
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY supplier_tags_policy ON supplier_tags
  FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

-- ============================================================
-- Policies — tabelas de junção (via subquery no dono da entidade)
-- ============================================================

CREATE POLICY transaction_tags_policy ON transaction_tags
  FOR ALL USING (
    EXISTS (
      SELECT 1 FROM transactions t
      WHERE t.id = transaction_tags.transaction_id
        AND t.user_id = auth.uid()
    )
  )
  WITH CHECK (
    EXISTS (
      SELECT 1 FROM transactions t
      WHERE t.id = transaction_tags.transaction_id
        AND t.user_id = auth.uid()
    )
  );

CREATE POLICY recurring_template_tags_policy ON recurring_template_tags
  FOR ALL USING (
    EXISTS (
      SELECT 1 FROM recurring_templates rt
      WHERE rt.id = recurring_template_tags.recurring_template_id
        AND rt.user_id = auth.uid()
    )
  )
  WITH CHECK (
    EXISTS (
      SELECT 1 FROM recurring_templates rt
      WHERE rt.id = recurring_template_tags.recurring_template_id
        AND rt.user_id = auth.uid()
    )
  );

CREATE POLICY liability_tags_policy ON liability_tags
  FOR ALL USING (
    EXISTS (
      SELECT 1 FROM liabilities l
      WHERE l.id = liability_tags.liability_id
        AND l.user_id = auth.uid()
    )
  )
  WITH CHECK (
    EXISTS (
      SELECT 1 FROM liabilities l
      WHERE l.id = liability_tags.liability_id
        AND l.user_id = auth.uid()
    )
  );
