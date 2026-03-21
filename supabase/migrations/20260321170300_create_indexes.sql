-- ============================================================
-- Migration 004: Índices — FK, trigram, temporal
-- ============================================================

-- ── Índices de FK nas tabelas core ──
CREATE INDEX idx_user_financial_preferences_user_id ON user_financial_preferences(user_id);
CREATE INDEX idx_institutions_user_id ON institutions(user_id);
CREATE INDEX idx_financial_products_user_id ON financial_products(user_id);
CREATE INDEX idx_financial_products_institution_id ON financial_products(institution_id);
CREATE INDEX idx_cards_user_id ON cards(user_id);
CREATE INDEX idx_cards_financial_product_id ON cards(financial_product_id);
CREATE INDEX idx_categories_user_id ON categories(user_id);
CREATE INDEX idx_categories_parent_id ON categories(parent_id);
CREATE INDEX idx_tags_user_id ON tags(user_id);
CREATE INDEX idx_financial_periods_user_id ON financial_periods(user_id);
CREATE INDEX idx_statement_cycles_user_id ON statement_cycles(user_id);
CREATE INDEX idx_statement_cycles_card_id ON statement_cycles(card_id);
CREATE INDEX idx_statement_items_user_id ON statement_items(user_id);
CREATE INDEX idx_statement_items_statement_cycle_id ON statement_items(statement_cycle_id);
CREATE INDEX idx_liabilities_user_id ON liabilities(user_id);
CREATE INDEX idx_liabilities_financial_product_id ON liabilities(financial_product_id);
CREATE INDEX idx_liability_installments_user_id ON liability_installments(user_id);
CREATE INDEX idx_liability_installments_liability_id ON liability_installments(liability_id);
CREATE INDEX idx_transactions_user_id ON transactions(user_id);
CREATE INDEX idx_transactions_financial_product_id ON transactions(financial_product_id);
CREATE INDEX idx_transactions_event_date ON transactions(user_id, event_date);
CREATE INDEX idx_transactions_type ON transactions(user_id, type);
CREATE INDEX idx_transactions_category_id ON transactions(category_id);
CREATE INDEX idx_transactions_financial_period_id ON transactions(financial_period_id);
CREATE INDEX idx_transactions_statement_cycle_id ON transactions(statement_cycle_id);
CREATE INDEX idx_transactions_recurring_instance_id ON transactions(recurring_instance_id);
CREATE INDEX idx_transfers_user_id ON transfers(user_id);
CREATE INDEX idx_transfers_source_product_id ON transfers(source_product_id);
CREATE INDEX idx_transfers_target_product_id ON transfers(target_product_id);
CREATE INDEX idx_recurring_templates_user_id ON recurring_templates(user_id);
CREATE INDEX idx_recurring_instances_user_id ON recurring_instances(user_id);
CREATE INDEX idx_recurring_instances_template_id ON recurring_instances(recurring_template_id);
CREATE INDEX idx_documents_user_id ON documents(user_id);
CREATE INDEX idx_documents_entity ON documents(entity_type, entity_id);
CREATE INDEX idx_import_jobs_user_id ON import_jobs(user_id);
CREATE INDEX idx_audit_logs_user_id ON audit_logs(user_id);
CREATE INDEX idx_audit_logs_entity ON audit_logs(entity_type, entity_id);

-- ── Índices de FK nas tabelas de fornecedor ──
CREATE INDEX idx_suppliers_user_id ON suppliers(user_id);
CREATE INDEX idx_supplier_aliases_supplier_id ON supplier_aliases(supplier_id);
CREATE INDEX idx_supplier_aliases_user_id ON supplier_aliases(user_id);
CREATE INDEX idx_supplier_contracts_supplier_id ON supplier_contracts(supplier_id);
CREATE INDEX idx_supplier_contracts_user_id ON supplier_contracts(user_id);
CREATE INDEX idx_consumption_metrics_supplier_id ON consumption_metrics(supplier_id);
CREATE INDEX idx_consumption_metrics_user_id ON consumption_metrics(user_id);
CREATE INDEX idx_supplier_tags_supplier_id ON supplier_tags(supplier_id);

-- ── Índices de supplier_id nas tabelas alteradas ──
CREATE INDEX idx_transactions_supplier_id ON transactions(supplier_id);
CREATE INDEX idx_statement_items_supplier_id ON statement_items(supplier_id);
CREATE INDEX idx_recurring_templates_supplier_id ON recurring_templates(supplier_id);
CREATE INDEX idx_documents_supplier_id ON documents(supplier_id);
CREATE INDEX idx_liabilities_supplier_id ON liabilities(supplier_id);

-- ── ADR-003: Índices trigram para busca fuzzy ──
CREATE INDEX idx_suppliers_name_trgm ON suppliers USING gin (name gin_trgm_ops);
CREATE INDEX idx_supplier_aliases_trgm ON supplier_aliases USING gin (alias_name gin_trgm_ops);

-- ── ADR-003: Índice para resolução temporal de alias ──
CREATE INDEX idx_supplier_aliases_resolve
  ON supplier_aliases(user_id, alias_name, valid_from, valid_until)
  WHERE is_active = true;
