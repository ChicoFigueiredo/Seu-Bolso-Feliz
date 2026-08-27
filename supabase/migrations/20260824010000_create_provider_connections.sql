-- Fase 2 da integração Pluggy (Open Finance): conexões com provedores externos
-- e mapeamento de contas retornadas para produtos financeiros já cadastrados.
-- Ver docs/planejamento/2026-08-24-plano-integracao-pluggy.md e ADR-008
-- (docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md).

-- ══════════════════════════════════════════════════════════════
-- Provider Connections
-- ══════════════════════════════════════════════════════════════
-- Um "item" Pluggy (uma sessão de login autorizada num banco) = uma linha aqui.
-- `provider` é extensível para futuras fontes (open finance ou não).

CREATE TABLE IF NOT EXISTS provider_connections (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth.users(id),
    provider TEXT NOT NULL CHECK (provider IN ('pluggy')),
    external_item_id TEXT NOT NULL,
    institution_name TEXT,
    status TEXT NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'error', 'revoked', 'expired')),
    last_synced_at TIMESTAMPTZ,
    metadata JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE (user_id, provider, external_item_id)
);

-- ══════════════════════════════════════════════════════════════
-- External Account Mappings
-- ══════════════════════════════════════════════════════════════
-- Cada conta/cartão que o provider devolve para um item vira uma linha aqui.
-- Fica `pending_mapping` até o usuário ligá-la a um `financial_products`
-- existente (ou criar um novo) — nunca materializa transações sem esse vínculo.

CREATE TABLE IF NOT EXISTS external_account_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    user_id UUID NOT NULL REFERENCES auth.users(id),
    provider_connection_id UUID NOT NULL REFERENCES provider_connections(id) ON DELETE CASCADE,
    external_account_id TEXT NOT NULL,
    external_account_name TEXT,
    external_account_type TEXT,
    financial_product_id UUID REFERENCES financial_products(id),
    status TEXT NOT NULL DEFAULT 'pending_mapping' CHECK (status IN ('pending_mapping', 'mapped', 'ignored')),
    created_at TIMESTAMPTZ DEFAULT now(),
    updated_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE (provider_connection_id, external_account_id)
);

-- ══════════════════════════════════════════════════════════════
-- RLS Policies
-- ══════════════════════════════════════════════════════════════

ALTER TABLE provider_connections ENABLE ROW LEVEL SECURITY;
ALTER TABLE external_account_mappings ENABLE ROW LEVEL SECURITY;

CREATE POLICY provider_connections_policy ON provider_connections
    FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

CREATE POLICY external_account_mappings_policy ON external_account_mappings
    FOR ALL USING (auth.uid() = user_id) WITH CHECK (auth.uid() = user_id);

-- ══════════════════════════════════════════════════════════════
-- Indexes
-- ══════════════════════════════════════════════════════════════

CREATE INDEX idx_provider_connections_user ON provider_connections(user_id);
CREATE INDEX idx_external_account_mappings_connection ON external_account_mappings(provider_connection_id);
CREATE INDEX idx_external_account_mappings_product ON external_account_mappings(financial_product_id);
CREATE INDEX idx_external_account_mappings_user ON external_account_mappings(user_id);

-- rollback:
--   DROP TABLE IF EXISTS external_account_mappings;
--   DROP TABLE IF EXISTS provider_connections;
