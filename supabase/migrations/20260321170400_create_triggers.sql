-- ============================================================
-- Migration 005: Triggers — updated_at + governança de aliases
-- ============================================================

-- ── Função genérica de updated_at ──
CREATE OR REPLACE FUNCTION update_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = NOW();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- ── Triggers updated_at — tabelas core ──
CREATE TRIGGER trg_user_financial_preferences_updated_at
  BEFORE UPDATE ON user_financial_preferences
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_institutions_updated_at
  BEFORE UPDATE ON institutions
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_financial_products_updated_at
  BEFORE UPDATE ON financial_products
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_cards_updated_at
  BEFORE UPDATE ON cards
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_categories_updated_at
  BEFORE UPDATE ON categories
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_tags_updated_at
  BEFORE UPDATE ON tags
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_financial_periods_updated_at
  BEFORE UPDATE ON financial_periods
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_statement_cycles_updated_at
  BEFORE UPDATE ON statement_cycles
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_statement_items_updated_at
  BEFORE UPDATE ON statement_items
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_liabilities_updated_at
  BEFORE UPDATE ON liabilities
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_liability_installments_updated_at
  BEFORE UPDATE ON liability_installments
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_transactions_updated_at
  BEFORE UPDATE ON transactions
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_transfers_updated_at
  BEFORE UPDATE ON transfers
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_recurring_templates_updated_at
  BEFORE UPDATE ON recurring_templates
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_recurring_instances_updated_at
  BEFORE UPDATE ON recurring_instances
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_documents_updated_at
  BEFORE UPDATE ON documents
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_user_secrets_updated_at
  BEFORE UPDATE ON user_secrets
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_import_jobs_updated_at
  BEFORE UPDATE ON import_jobs
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ── Triggers updated_at — tabelas de fornecedor ──
CREATE TRIGGER trg_suppliers_updated_at
  BEFORE UPDATE ON suppliers
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_supplier_aliases_updated_at
  BEFORE UPDATE ON supplier_aliases
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_supplier_contracts_updated_at
  BEFORE UPDATE ON supplier_contracts
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

CREATE TRIGGER trg_consumption_metrics_updated_at
  BEFORE UPDATE ON consumption_metrics
  FOR EACH ROW EXECUTE FUNCTION update_updated_at();

-- ============================================================
-- ADR-003: Trigger de unicidade temporal de alias
-- ============================================================
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

-- ============================================================
-- ADR-003: Trigger de auto-alias em renomeação de fornecedor
-- ============================================================
CREATE OR REPLACE FUNCTION auto_alias_on_rename()
RETURNS TRIGGER AS $$
BEGIN
  IF OLD.name IS DISTINCT FROM NEW.name THEN
    INSERT INTO supplier_aliases (user_id, supplier_id, alias_name, alias_type, valid_until)
    VALUES (NEW.user_id, NEW.id, OLD.name, 'former_name', NOW()::date);
  END IF;
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_auto_alias_on_rename
  BEFORE UPDATE ON suppliers
  FOR EACH ROW EXECUTE FUNCTION auto_alias_on_rename();
