-- Migration: financial_obligations + financial_obligation_evidences
-- Agrupa evidências financeiras pela mesma obrigação real (ex: vários emails da mesma fatura)
-- usando financial_identity_key para deduplicação semântica.

CREATE TABLE IF NOT EXISTS financial_obligations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,

  obligation_type text NOT NULL CHECK (obligation_type IN (
    'bill_to_pay',
    'bill_reminder',
    'invoice_statement',
    'recurring_charge',
    'liability_installment',
    'unknown'
  )),

  status text NOT NULL DEFAULT 'open' CHECK (status IN (
    'open',
    'pending_review',
    'approved',
    'paid',
    'cancelled',
    'duplicate',
    'rejected'
  )),

  supplier_id uuid REFERENCES suppliers(id) ON DELETE SET NULL,
  supplier_name_raw text,
  financial_product_id uuid REFERENCES financial_products(id) ON DELETE SET NULL,

  amount numeric(15,2),
  due_date date,
  competence_date date,
  cycle_start_date date,
  cycle_end_date date,

  document_number text,
  barcode_digitable_line text,
  financial_identity_key text,

  confidence_score numeric(3,2),
  metadata jsonb DEFAULT '{}',

  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

-- Índice único por usuário + chave de identidade (deduplicação semântica)
CREATE UNIQUE INDEX IF NOT EXISTS idx_financial_obligations_identity
  ON financial_obligations(user_id, financial_identity_key)
  WHERE financial_identity_key IS NOT NULL;

-- Índice de busca por status + usuário
CREATE INDEX IF NOT EXISTS idx_financial_obligations_user_status
  ON financial_obligations(user_id, status);

-- Tabela de vínculo: uma obrigação pode ter múltiplas evidências (documentos/emails)
CREATE TABLE IF NOT EXISTS financial_obligation_evidences (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  obligation_id uuid NOT NULL REFERENCES financial_obligations(id) ON DELETE CASCADE,
  source_document_id uuid NOT NULL REFERENCES source_documents(id) ON DELETE CASCADE,
  evidence_role text NOT NULL DEFAULT 'supporting' CHECK (evidence_role IN (
    'primary',
    'supporting',
    'duplicate',
    'conflicting'
  )),
  confidence_score numeric(3,2),
  reasons jsonb DEFAULT '[]',
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE(user_id, obligation_id, source_document_id)
);

CREATE INDEX IF NOT EXISTS idx_financial_obligation_evidences_obligation
  ON financial_obligation_evidences(obligation_id);

CREATE INDEX IF NOT EXISTS idx_financial_obligation_evidences_document
  ON financial_obligation_evidences(source_document_id);

-- Trigger para updated_at em financial_obligations
CREATE OR REPLACE FUNCTION update_financial_obligations_updated_at()
RETURNS TRIGGER AS $$
BEGIN
  NEW.updated_at = now();
  RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_financial_obligations_updated_at ON financial_obligations;
CREATE TRIGGER trg_financial_obligations_updated_at
  BEFORE UPDATE ON financial_obligations
  FOR EACH ROW
  EXECUTE FUNCTION update_financial_obligations_updated_at();

-- RLS
ALTER TABLE financial_obligations ENABLE ROW LEVEL SECURITY;
ALTER TABLE financial_obligation_evidences ENABLE ROW LEVEL SECURITY;

-- Políticas RLS: usuário só acessa seus próprios dados
CREATE POLICY "financial_obligations_user_policy"
  ON financial_obligations
  FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);

CREATE POLICY "financial_obligation_evidences_user_policy"
  ON financial_obligation_evidences
  FOR ALL
  USING (auth.uid() = user_id)
  WITH CHECK (auth.uid() = user_id);

-- Service role acessa tudo (para workers server-side)
CREATE POLICY "financial_obligations_service_role"
  ON financial_obligations
  FOR ALL
  TO service_role
  USING (true)
  WITH CHECK (true);

CREATE POLICY "financial_obligation_evidences_service_role"
  ON financial_obligation_evidences
  FOR ALL
  TO service_role
  USING (true)
  WITH CHECK (true);
