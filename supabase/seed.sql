-- ============================================================
-- seed.sql — Dados de teste realistas para Seu Bolso Feliz
-- ============================================================
-- Executado via `supabase db reset` após todas as migrations.
-- Cria um usuário de teste e popula todas as entidades do domínio.
-- ============================================================

-- ── 0. Usuário de teste via auth.users (Supabase local) ──
INSERT INTO auth.users (
  id,
  instance_id,
  aud,
  role,
  email,
  encrypted_password,
  email_confirmed_at,
  raw_app_meta_data,
  raw_user_meta_data,
  created_at,
  updated_at
) VALUES (
  '00000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000000',
  'authenticated',
  'authenticated',
  'teste@seubolsofeliz.com.br',
  crypt('teste123456', gen_salt('bf')),
  now(),
  '{"provider":"email","providers":["email"]}',
  '{"name":"Usuário de Teste"}',
  now(),
  now()
) ON CONFLICT (id) DO NOTHING;

-- Identity para login funcionar
INSERT INTO auth.identities (
  id,
  user_id,
  identity_data,
  provider,
  provider_id,
  last_sign_in_at,
  created_at,
  updated_at
) VALUES (
  '00000000-0000-0000-0000-000000000001',
  '00000000-0000-0000-0000-000000000001',
  '{"sub":"00000000-0000-0000-0000-000000000001","email":"teste@seubolsofeliz.com.br"}',
  'email',
  '00000000-0000-0000-0000-000000000001',
  now(),
  now(),
  now()
) ON CONFLICT (provider, provider_id) DO NOTHING;

-- Variável para reusar o user_id
DO $$
DECLARE
  v_user_id uuid := '00000000-0000-0000-0000-000000000001';
  -- Instituições
  v_caixa uuid;
  v_nubank uuid;
  v_c6 uuid;
  -- Produtos
  v_caixa_cc uuid;
  v_caixa_poup uuid;
  v_nubank_cc uuid;
  v_nubank_cartao_prod uuid;
  v_c6_cc uuid;
  v_c6_cartao_prod uuid;
  -- Cartões
  v_nubank_cartao uuid;
  v_c6_cartao uuid;
  -- Categorias
  v_cat_moradia uuid;
  v_cat_alimentacao uuid;
  v_cat_transporte uuid;
  v_cat_saude uuid;
  v_cat_educacao uuid;
  v_cat_lazer uuid;
  v_cat_servicos uuid;
  -- Tags
  v_tag_essencial uuid;
  v_tag_casa uuid;
  v_tag_trabalho uuid;
  v_tag_pessoal uuid;
  -- Fornecedores
  v_sup_cemig uuid;
  v_sup_copasa uuid;
  v_sup_claro uuid;
  v_sup_ifood uuid;
  v_sup_uber uuid;
  v_sup_spotify uuid;
  v_sup_mercado uuid;
  -- Período financeiro
  v_periodo_mar uuid;
  v_periodo_abr uuid;
  -- Statement cycle
  v_nubank_cycle uuid;
  v_c6_cycle uuid;
  -- Liability
  v_emprestimo_caixa uuid;
BEGIN
  -- ── 1. Preferências financeiras ──
  INSERT INTO user_financial_preferences (user_id, financial_cycle_start_day, financial_cycle_anchor_date)
  VALUES (v_user_id, 20, '2026-01-20')
  ON CONFLICT (user_id) DO NOTHING;

  -- ── 2. Instituições ──
  INSERT INTO institutions (id, user_id, name, type, display_order) VALUES
    (gen_random_uuid(), v_user_id, 'Caixa Econômica Federal', 'bank', 1),
    (gen_random_uuid(), v_user_id, 'Nubank', 'fintech', 2),
    (gen_random_uuid(), v_user_id, 'C6 Bank', 'fintech', 3);

  SELECT id INTO v_caixa FROM institutions WHERE user_id = v_user_id AND name = 'Caixa Econômica Federal';
  SELECT id INTO v_nubank FROM institutions WHERE user_id = v_user_id AND name = 'Nubank';
  SELECT id INTO v_c6 FROM institutions WHERE user_id = v_user_id AND name = 'C6 Bank';

  -- ── 3. Produtos financeiros ──
  INSERT INTO financial_products (id, user_id, institution_id, name, type, current_balance, credit_limit, display_order) VALUES
    (gen_random_uuid(), v_user_id, v_caixa, 'Conta Corrente Caixa', 'checking_account', 2350.00, NULL, 1),
    (gen_random_uuid(), v_user_id, v_caixa, 'Poupança Caixa', 'savings_account', 5000.00, NULL, 2),
    (gen_random_uuid(), v_user_id, v_nubank, 'Conta Nubank', 'checking_account', 1200.00, NULL, 3),
    (gen_random_uuid(), v_user_id, v_nubank, 'Cartão Nubank', 'credit_card', NULL, 8000.00, 4),
    (gen_random_uuid(), v_user_id, v_c6, 'Conta C6', 'checking_account', 800.00, NULL, 5),
    (gen_random_uuid(), v_user_id, v_c6, 'Cartão C6', 'credit_card', NULL, 5000.00, 6);

  SELECT id INTO v_caixa_cc FROM financial_products WHERE user_id = v_user_id AND name = 'Conta Corrente Caixa';
  SELECT id INTO v_caixa_poup FROM financial_products WHERE user_id = v_user_id AND name = 'Poupança Caixa';
  SELECT id INTO v_nubank_cc FROM financial_products WHERE user_id = v_user_id AND name = 'Conta Nubank';
  SELECT id INTO v_nubank_cartao_prod FROM financial_products WHERE user_id = v_user_id AND name = 'Cartão Nubank';
  SELECT id INTO v_c6_cc FROM financial_products WHERE user_id = v_user_id AND name = 'Conta C6';
  SELECT id INTO v_c6_cartao_prod FROM financial_products WHERE user_id = v_user_id AND name = 'Cartão C6';

  -- ── 4. Cartões ──
  INSERT INTO cards (id, user_id, financial_product_id, last_four_digits, card_brand, closing_day, due_day, credit_limit) VALUES
    (gen_random_uuid(), v_user_id, v_nubank_cartao_prod, '4567', 'Mastercard', 15, 23, 8000.00),
    (gen_random_uuid(), v_user_id, v_c6_cartao_prod, '8901', 'Mastercard', 10, 17, 5000.00);

  SELECT id INTO v_nubank_cartao FROM cards WHERE user_id = v_user_id AND last_four_digits = '4567';
  SELECT id INTO v_c6_cartao FROM cards WHERE user_id = v_user_id AND last_four_digits = '8901';

  -- ── 5. Categorias ──
  INSERT INTO categories (id, user_id, name, icon, display_order) VALUES
    (gen_random_uuid(), v_user_id, 'Moradia', '🏠', 1),
    (gen_random_uuid(), v_user_id, 'Alimentação', '🍔', 2),
    (gen_random_uuid(), v_user_id, 'Transporte', '🚗', 3),
    (gen_random_uuid(), v_user_id, 'Saúde', '🏥', 4),
    (gen_random_uuid(), v_user_id, 'Educação', '📚', 5),
    (gen_random_uuid(), v_user_id, 'Lazer', '🎮', 6),
    (gen_random_uuid(), v_user_id, 'Serviços', '⚙️', 7);

  SELECT id INTO v_cat_moradia FROM categories WHERE user_id = v_user_id AND name = 'Moradia';
  SELECT id INTO v_cat_alimentacao FROM categories WHERE user_id = v_user_id AND name = 'Alimentação';
  SELECT id INTO v_cat_transporte FROM categories WHERE user_id = v_user_id AND name = 'Transporte';
  SELECT id INTO v_cat_saude FROM categories WHERE user_id = v_user_id AND name = 'Saúde';
  SELECT id INTO v_cat_educacao FROM categories WHERE user_id = v_user_id AND name = 'Educação';
  SELECT id INTO v_cat_lazer FROM categories WHERE user_id = v_user_id AND name = 'Lazer';
  SELECT id INTO v_cat_servicos FROM categories WHERE user_id = v_user_id AND name = 'Serviços';

  -- ── 6. Tags ──
  INSERT INTO tags (id, user_id, name, color, influences_priority, suggested_priority) VALUES
    (gen_random_uuid(), v_user_id, 'essencial', '#dc2626', true, 'essential'),
    (gen_random_uuid(), v_user_id, 'casa', '#2563eb', true, 'high'),
    (gen_random_uuid(), v_user_id, 'trabalho', '#7c3aed', false, NULL),
    (gen_random_uuid(), v_user_id, 'pessoal', '#059669', false, NULL);

  SELECT id INTO v_tag_essencial FROM tags WHERE user_id = v_user_id AND name = 'essencial';
  SELECT id INTO v_tag_casa FROM tags WHERE user_id = v_user_id AND name = 'casa';
  SELECT id INTO v_tag_trabalho FROM tags WHERE user_id = v_user_id AND name = 'trabalho';
  SELECT id INTO v_tag_pessoal FROM tags WHERE user_id = v_user_id AND name = 'pessoal';

  -- ── 7. Fornecedores ──
  INSERT INTO suppliers (id, user_id, name, type, display_order) VALUES
    (gen_random_uuid(), v_user_id, 'CEMIG', 'utility', 1),
    (gen_random_uuid(), v_user_id, 'COPASA', 'utility', 2),
    (gen_random_uuid(), v_user_id, 'Claro', 'telecom', 3),
    (gen_random_uuid(), v_user_id, 'iFood', 'platform', 4),
    (gen_random_uuid(), v_user_id, 'Uber', 'platform', 5),
    (gen_random_uuid(), v_user_id, 'Spotify', 'saas', 6),
    (gen_random_uuid(), v_user_id, 'Supermercado BH', 'company', 7);

  SELECT id INTO v_sup_cemig FROM suppliers WHERE user_id = v_user_id AND name = 'CEMIG';
  SELECT id INTO v_sup_copasa FROM suppliers WHERE user_id = v_user_id AND name = 'COPASA';
  SELECT id INTO v_sup_claro FROM suppliers WHERE user_id = v_user_id AND name = 'Claro';
  SELECT id INTO v_sup_ifood FROM suppliers WHERE user_id = v_user_id AND name = 'iFood';
  SELECT id INTO v_sup_uber FROM suppliers WHERE user_id = v_user_id AND name = 'Uber';
  SELECT id INTO v_sup_spotify FROM suppliers WHERE user_id = v_user_id AND name = 'Spotify';
  SELECT id INTO v_sup_mercado FROM suppliers WHERE user_id = v_user_id AND name = 'Supermercado BH';

  -- ── 7b. Aliases de fornecedor ──
  INSERT INTO supplier_aliases (user_id, supplier_id, alias_name, alias_type) VALUES
    (v_user_id, v_sup_cemig, 'CEMIG DISTRIBUICAO SA', 'billing_name'),
    (v_user_id, v_sup_copasa, 'COPASA MG', 'abbreviation'),
    (v_user_id, v_sup_claro, 'CLARO S/A', 'billing_name'),
    (v_user_id, v_sup_ifood, 'IFOOD.COM', 'billing_name'),
    (v_user_id, v_sup_uber, 'UBER *TRIP', 'billing_name'),
    (v_user_id, v_sup_spotify, 'SPOTIFY BRAZIL', 'billing_name');

  -- ── 7c. Contratos com fornecedor ──
  INSERT INTO supplier_contracts (user_id, supplier_id, contract_type, identifier, label) VALUES
    (v_user_id, v_sup_cemig, 'utility', 'UC-123456', 'Casa - Conta de Luz'),
    (v_user_id, v_sup_copasa, 'utility', 'MAT-654321', 'Casa - Conta de Água'),
    (v_user_id, v_sup_claro, 'subscription', 'CTR-789012', 'Internet Residencial');

  -- ── 8. Períodos financeiros (ciclo 20/mês a 19/mês) ──
  INSERT INTO financial_periods (id, user_id, start_date, end_date, label, is_current) VALUES
    (gen_random_uuid(), v_user_id, '2026-02-20', '2026-03-19', 'Fev-Mar 2026', false),
    (gen_random_uuid(), v_user_id, '2026-03-20', '2026-04-19', 'Mar-Abr 2026', true);

  SELECT id INTO v_periodo_mar FROM financial_periods WHERE user_id = v_user_id AND label = 'Fev-Mar 2026';
  SELECT id INTO v_periodo_abr FROM financial_periods WHERE user_id = v_user_id AND label = 'Mar-Abr 2026';

  -- ── 9. Ciclos de fatura ──
  INSERT INTO statement_cycles (id, user_id, card_id, reference_month, cycle_start_date, cycle_end_date, due_date, total_amount, paid_amount, status) VALUES
    (gen_random_uuid(), v_user_id, v_nubank_cartao, '2026-03-01', '2026-02-16', '2026-03-15', '2026-03-23', 2150.00, 0, 'closed'),
    (gen_random_uuid(), v_user_id, v_c6_cartao, '2026-03-01', '2026-02-11', '2026-03-10', '2026-03-17', 890.00, 890.00, 'paid');

  SELECT id INTO v_nubank_cycle FROM statement_cycles WHERE user_id = v_user_id AND cycle_end_date = '2026-03-15';
  SELECT id INTO v_c6_cycle FROM statement_cycles WHERE user_id = v_user_id AND cycle_end_date = '2026-03-10';

  -- ── 10. Statement Items (itens da fatura) ──
  INSERT INTO statement_items (user_id, statement_cycle_id, description, amount, transaction_date, supplier_id) VALUES
    (v_user_id, v_nubank_cycle, 'IFOOD *PEDIDO 12345', 45.90, '2026-02-28', v_sup_ifood),
    (v_user_id, v_nubank_cycle, 'UBER *TRIP', 32.50, '2026-03-01', v_sup_uber),
    (v_user_id, v_nubank_cycle, 'SPOTIFY BRAZIL', 34.90, '2026-03-05', v_sup_spotify),
    (v_user_id, v_nubank_cycle, 'SUPERMERCADO BH', 287.45, '2026-03-08', v_sup_mercado),
    (v_user_id, v_nubank_cycle, 'IFOOD *PEDIDO 12890', 62.00, '2026-03-10', v_sup_ifood),
    (v_user_id, v_c6_cycle, 'UBER *TRIP', 28.00, '2026-02-25', v_sup_uber),
    (v_user_id, v_c6_cycle, 'SUPERMERCADO BH', 195.30, '2026-03-02', v_sup_mercado);

  -- ── 11. Transações (despesas reais, receitas, transferências internas) ──
  -- Receita: Salário
  INSERT INTO transactions (user_id, financial_product_id, type, amount, description, event_date, competence_date, financial_period_id, category_id, priority) VALUES
    (v_user_id, v_caixa_cc, 'income', 7500.00, 'Salário - Empresa XYZ', '2026-03-05', '2026-03-01', v_periodo_abr, NULL, NULL);

  -- Despesas recorrentes reais
  INSERT INTO transactions (user_id, financial_product_id, type, amount, description, event_date, competence_date, financial_period_id, category_id, supplier_id, priority) VALUES
    (v_user_id, v_caixa_cc, 'expense', 385.00, 'Conta de Luz - CEMIG', '2026-03-10', '2026-03-01', v_periodo_abr, v_cat_moradia, v_sup_cemig, 'essential'),
    (v_user_id, v_caixa_cc, 'expense', 120.00, 'Conta de Água - COPASA', '2026-03-12', '2026-03-01', v_periodo_abr, v_cat_moradia, v_sup_copasa, 'essential'),
    (v_user_id, v_caixa_cc, 'expense', 149.90, 'Internet - Claro', '2026-03-15', '2026-03-01', v_periodo_abr, v_cat_servicos, v_sup_claro, 'high'),
    (v_user_id, v_caixa_cc, 'expense', 350.00, 'Supermercado BH', '2026-03-08', '2026-03-08', v_periodo_abr, v_cat_alimentacao, v_sup_mercado, 'essential');

  -- Pagamento de fatura (NÃO é despesa nova)
  INSERT INTO transactions (user_id, financial_product_id, type, amount, description, event_date, competence_date, financial_period_id, statement_cycle_id) VALUES
    (v_user_id, v_caixa_cc, 'statement_payment', 890.00, 'Pagamento fatura C6 - Mar/2026', '2026-03-17', '2026-03-17', v_periodo_abr, v_c6_cycle);

  -- ── 12. Transferência interna (Caixa CC → Nubank) ──
  INSERT INTO transfers (user_id, source_product_id, target_product_id, amount, description, event_date) VALUES
    (v_user_id, v_caixa_cc, v_nubank_cc, 500.00, 'Transferência para Nubank', '2026-03-06');

  -- ── 13. Tags em transações ──
  INSERT INTO transaction_tags (transaction_id, tag_id)
  SELECT t.id, v_tag_essencial
  FROM transactions t
  WHERE t.user_id = v_user_id AND t.priority = 'essential';

  INSERT INTO transaction_tags (transaction_id, tag_id)
  SELECT t.id, v_tag_casa
  FROM transactions t
  WHERE t.user_id = v_user_id AND t.supplier_id IN (v_sup_cemig, v_sup_copasa, v_sup_claro);

  -- ── 14. Supplier Tags ──
  INSERT INTO supplier_tags (user_id, supplier_id, tag_id)
  VALUES
    (v_user_id, v_sup_cemig, v_tag_essencial),
    (v_user_id, v_sup_cemig, v_tag_casa),
    (v_user_id, v_sup_copasa, v_tag_essencial),
    (v_user_id, v_sup_copasa, v_tag_casa),
    (v_user_id, v_sup_claro, v_tag_casa),
    (v_user_id, v_sup_spotify, v_tag_pessoal);

  -- ── 15. Empréstimo (Caixa) ──
  INSERT INTO liabilities (id, user_id, financial_product_id, name, type, original_amount, outstanding_balance, interest_rate, rate_type, amortization_system, total_installments, paid_installments, start_date, supplier_id) VALUES
    (gen_random_uuid(), v_user_id, v_caixa_cc, 'Empréstimo pessoal Caixa', 'personal_loan', 10000.00, 7850.00, 2.5, 'monthly', 'price', 24, 6, '2025-09-15', NULL);

  SELECT id INTO v_emprestimo_caixa FROM liabilities WHERE user_id = v_user_id AND name = 'Empréstimo pessoal Caixa';

  -- Parcelas do empréstimo (próximas 3)
  INSERT INTO liability_installments (user_id, liability_id, installment_number, due_date, total_amount, principal_amount, interest_amount, fee_amount, status, paid_amount, paid_date) VALUES
    (v_user_id, v_emprestimo_caixa, 7, '2026-03-15', 560.00, 365.00, 185.00, 10.00, 'paid', 560.00, '2026-03-15'),
    (v_user_id, v_emprestimo_caixa, 8, '2026-04-15', 560.00, 374.00, 176.00, 10.00, 'pending', 0, NULL),
    (v_user_id, v_emprestimo_caixa, 9, '2026-05-15', 560.00, 383.00, 167.00, 10.00, 'pending', 0, NULL);

  -- ── 16. Templates recorrentes ──
  INSERT INTO recurring_templates (user_id, financial_product_id, supplier_id, category_id, name, type, amount, frequency, day_of_month, priority, is_active) VALUES
    (v_user_id, v_caixa_cc, v_sup_cemig, v_cat_moradia, 'Conta de Luz - CEMIG', 'expense', 385.00, 'monthly', 10, 'essential', true),
    (v_user_id, v_caixa_cc, v_sup_copasa, v_cat_moradia, 'Conta de Água - COPASA', 'expense', 120.00, 'monthly', 12, 'essential', true),
    (v_user_id, v_caixa_cc, v_sup_claro, v_cat_servicos, 'Internet - Claro', 'expense', 149.90, 'monthly', 15, 'high', true),
    (v_user_id, v_nubank_cartao_prod, v_sup_spotify, v_cat_lazer, 'Spotify', 'expense', 34.90, 'monthly', 5, 'low', true);

  -- ── 17. Consumption Metrics (CEMIG) ──
  INSERT INTO consumption_metrics (user_id, supplier_id, supplier_contract_id, reference_period_start, reference_period_end, metric_name, quantity, metric_unit)
  SELECT
    v_user_id,
    v_sup_cemig,
    sc.id,
    '2026-02-01',
    '2026-02-28',
    'electricity_consumption',
    285.0,
    'kWh'
  FROM supplier_contracts sc
  WHERE sc.supplier_id = v_sup_cemig AND sc.user_id = v_user_id
  LIMIT 1;

  -- ── 18. Materialized view refresh ──
  REFRESH MATERIALIZED VIEW mv_supplier_spending;

END $$;
