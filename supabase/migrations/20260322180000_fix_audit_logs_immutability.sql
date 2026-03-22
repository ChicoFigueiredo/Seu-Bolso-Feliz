-- Migration: RLS audit_logs imutável
-- Gap C1 identificado em auditoria 2026-03-22
-- A policy FOR ALL permite DELETE pelo usuário — destrói imutabilidade de auditoria.
-- Solução: FOR SELECT + FOR INSERT apenas + trigger BEFORE UPDATE OR DELETE.

-- 1. Remover policy permissiva existente
DROP POLICY IF EXISTS "Users can manage their own audit_logs" ON audit_logs;
DROP POLICY IF EXISTS "Users can manage own audit_logs" ON audit_logs;

-- 2. Policies restritivas
CREATE POLICY "Users can read own audit_logs"
  ON audit_logs FOR SELECT
  USING (auth.uid() = user_id);

CREATE POLICY "Service can insert audit_logs"
  ON audit_logs FOR INSERT
  WITH CHECK (auth.uid() = user_id);

-- 3. Função e trigger de imutabilidade
CREATE OR REPLACE FUNCTION prevent_audit_log_mutation()
RETURNS TRIGGER
LANGUAGE plpgsql
SECURITY DEFINER
AS $$
BEGIN
  RAISE EXCEPTION 'audit_logs are immutable — % operation not permitted on this table', TG_OP
    USING ERRCODE = 'restrict_violation';
END;
$$;

DROP TRIGGER IF EXISTS trg_audit_logs_immutable ON audit_logs;
CREATE TRIGGER trg_audit_logs_immutable
  BEFORE UPDATE OR DELETE ON audit_logs
  FOR EACH ROW
  EXECUTE FUNCTION prevent_audit_log_mutation();

COMMENT ON TABLE audit_logs IS
  'Log de auditoria imutável. INSERT permitido via RLS. UPDATE e DELETE bloqueados por trigger.';
