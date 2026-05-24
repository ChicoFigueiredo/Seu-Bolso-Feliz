# 004 — Plano Pós-Auditoria: Supabase Security Hardening + Edge Functions

> Documento de planejamento baseado na auditoria de 2026-03-22.
> Referência: `docs/refinos/2026-03/2026-03-22-18-00-pos-auditoria-supabase-edge-functions-e-seguranca.md`

---

## Contexto

A auditoria identificou:

- 3 gaps **críticos** de segurança que bloqueiam produção
- 11 gaps **importantes** que devem ser resolvidos antes de promover para produção
- 10 gaps **opcionais** de qualidade adicional
- 1 configuração de desenvolvimento (MCP local) implementada

---

## Sprint Atual (2026-03-22): Gaps Críticos e Infraestrutura Dev

### Tarefa H1-A — Migration: RLS audit_logs imutável

**Responsável:** André Santos (DBA)  
**Prazo:** 2026-03-22  
**Arquivo criado:** `supabase/migrations/20260322180000_fix_audit_logs_immutability.sql`

**O que fazer:**

```sql
-- 1. Remover policy permissiva atual
DROP POLICY IF EXISTS "Users can manage their own audit_logs" ON audit_logs;

-- 2. Criar policies restritivas (SELECT + INSERT apenas)
CREATE POLICY "Users can read own audit_logs" ON audit_logs
  FOR SELECT USING (auth.uid() = user_id);

CREATE POLICY "System can insert audit_logs" ON audit_logs
  FOR INSERT WITH CHECK (auth.uid() = user_id);

-- 3. Trigger de imutabilidade (garante mesmo via service_role)
CREATE OR REPLACE FUNCTION prevent_audit_log_mutation()
RETURNS TRIGGER LANGUAGE plpgsql AS $$
BEGIN
  RAISE EXCEPTION 'audit_logs are immutable — % operation not allowed', TG_OP;
END;
$$;

CREATE TRIGGER trg_audit_logs_immutable
  BEFORE UPDATE OR DELETE ON audit_logs
  FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();
```

**Critério de aceite:**

- [ ] Migration aplica sem erro (`npx supabase db reset`)
- [ ] `DELETE FROM audit_logs WHERE ...` levanta exception
- [ ] `UPDATE audit_logs SET ...` levanta exception
- [ ] `SELECT` funciona normalmente
- [ ] `INSERT` funciona normalmente (via Edge Functions e código de aplicação)

---

### Tarefa H1-B — Migration: Storage buckets + policies

**Responsável:** Fernando Gomes (DevOps)  
**Prazo:** 2026-03-22  
**Arquivo criado:** `supabase/migrations/20260322180100_create_storage_buckets.sql`

**O que fazer:**

```sql
-- Criar buckets privados
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES
  ('documents', 'documents', false, 52428800,
   ARRAY['application/pdf','image/jpeg','image/png','image/webp',
         'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet',
         'text/csv','application/msword',
         'application/vnd.openxmlformats-officedocument.wordprocessingml.document']),
  ('imports', 'imports', false, 52428800,
   ARRAY['text/csv','application/vnd.ms-excel',
         'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet']);

-- Storage RLS — bucket documents
CREATE POLICY "Users upload own documents"
  ON storage.objects FOR INSERT
  WITH CHECK (bucket_id = 'documents'
    AND (storage.foldername(name))[1] = auth.uid()::text);

CREATE POLICY "Users read own documents"
  ON storage.objects FOR SELECT
  USING (bucket_id = 'documents'
    AND (storage.foldername(name))[1] = auth.uid()::text);

CREATE POLICY "Users delete own documents"
  ON storage.objects FOR DELETE
  USING (bucket_id = 'documents'
    AND (storage.foldername(name))[1] = auth.uid()::text);

-- Storage RLS — bucket imports
CREATE POLICY "Users upload own imports"
  ON storage.objects FOR INSERT
  WITH CHECK (bucket_id = 'imports'
    AND (storage.foldername(name))[1] = auth.uid()::text);

CREATE POLICY "Users read own imports"
  ON storage.objects FOR SELECT
  USING (bucket_id = 'imports'
    AND (storage.foldername(name))[1] = auth.uid()::text);

CREATE POLICY "Users delete own imports"
  ON storage.objects FOR DELETE
  USING (bucket_id = 'imports'
    AND (storage.foldername(name))[1] = auth.uid()::text);
```

**Critério de aceite:**

- [ ] Migration aplica sem erro
- [ ] Buckets `documents` e `imports` aparecem na aba Storage do Studio
- [ ] Upload de arquivo para `/documents/<user_id>/arquivo.pdf` funciona autenticado
- [ ] Upload para path de outro usuário é rejeitado (403)

---

### Tarefa H1-C — Migration: pgcrypto para user_secrets

**Responsável:** André Santos (DBA)  
**Prazo:** 2026-03-22  
**Arquivo criado:** `supabase/migrations/20260322180200_encrypt_user_secrets.sql`

**O que fazer:**

```sql
-- Garantir extensão disponível (já deve estar)
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- Alterar coluna para bytea (armazena dados encriptados)
-- NOTA: dados existentes ficam como estão — migration apenas muda o tipo para novas inserções
-- Em produção, seria necessário migrar dados existentes com backup primeiro

-- Adicionar campo de versão para facilitar rotação de chave
ALTER TABLE user_secrets ADD COLUMN IF NOT EXISTS encryption_version integer NOT NULL DEFAULT 1;

-- Criar função helper para encriptação (usa chave da env var)
CREATE OR REPLACE FUNCTION encrypt_secret(plaintext text)
RETURNS text LANGUAGE plpgsql SECURITY DEFINER AS $$
BEGIN
  RETURN encode(
    pgp_sym_encrypt(plaintext, current_setting('app.encryption_key', true)),
    'base64'
  );
END;
$$;

-- Criar função helper para decriptação
CREATE OR REPLACE FUNCTION decrypt_secret(ciphertext text)
RETURNS text LANGUAGE plpgsql SECURITY DEFINER AS $$
BEGIN
  RETURN pgp_sym_decrypt(
    decode(ciphertext, 'base64'),
    current_setting('app.encryption_key', true)
  );
END;
$$;

-- Comentar a tabela para documentar o contrato
COMMENT ON COLUMN user_secrets.encrypted_value IS
  'Valor encriptado com pgp_sym_encrypt usando app.encryption_key. Use encrypt_secret() e decrypt_secret() helpers.';
```

**Configuração da chave de encriptação:**

```bash
# Em desenvolvimento local (definir no .env.local e configurar via supabase)
# Exemplo: gerar chave segura
openssl rand -base64 32

# Adicionar ao .env.local (não commitar):
SUPABASE_ENCRYPTION_KEY=<chave-gerada>

# Para uso em Edge Functions (secrets):
npx supabase secrets set APP_ENCRYPTION_KEY=<chave-gerada>

# Para o banco ter acesso via current_setting():
# Edge Function deve chamar: SET LOCAL app.encryption_key = '<chave>'
# antes de usar as funções helpers
```

**Critério de aceite:**

- [ ] Migration aplica sem erro
- [ ] `encrypt_secret('senha123')` retorna string base64 diferente da entrada
- [ ] `decrypt_secret(encrypt_secret('senha123'))` retorna `'senha123'`
- [ ] Coluna `encryption_version` existe na tabela

---

### Tarefa H2-A — Adicionar CORS headers nas Edge Functions

**Responsável:** João Pereira (Backend)  
**Prazo:** 2026-03-22

**Criticidade:** Sem CORS headers, chamadas do browser (frontend Next.js → Edge Function) vão falhar em produção com erro de preflight/CORS.

**Padrão a aplicar em ambas as funções:**

```typescript
const corsHeaders = {
  "Access-Control-Allow-Origin": "*", // restringir para domínio do app em produção
  "Access-Control-Allow-Headers": "authorization, x-client-info, apikey, content-type",
  "Access-Control-Allow-Methods": "POST, GET, OPTIONS",
};

// No início do handler:
if (req.method === "OPTIONS") {
  return new Response("ok", { headers: corsHeaders });
}

// Em todas as responses, incluir corsHeaders:
return new Response(JSON.stringify(data), {
  status: 200,
  headers: { ...corsHeaders, "Content-Type": "application/json" },
});
```

**Critério de aceite:**

- [ ] Ambas as funções respondem `200 OK` para `OPTIONS` preflight
- [ ] Respostas incluem `Access-Control-Allow-Origin`
- [ ] Chamadas do frontend não são bloqueadas por CORS

---

### Tarefa H2-B — config.toml: hardening de autenticação básico

**Responsável:** Pedro Santos (Backend)  
**Prazo:** 2026-03-22

**Alterações no `supabase/config.toml`:**

```toml
# Auth básico
[auth]
minimum_password_length = 8  # era 6
password_requirements = "lower_upper_letters_digits"  # era ""
enable_confirmations = true  # era false
secure_password_change = true  # era false

# SMTP local (Mailpit)
[auth.email.smtp]
host = "inbucket"
port = 54325
user = "fake"
pass = "fake"
admin_email = "admin@seu.bolso.feliz.local"
sender_name = "Seu Bolso Feliz (Local)"
```

**Critério de aceite:**

- [ ] Senha `abc123` é rejeitada (menos que 8 chars)
- [ ] Senha `password` é rejeitada (sem uppercase/digit)
- [ ] Senha `Senha123!` é aceita
- [ ] E-mail de confirmação é enviado e aparece no Mailpit (`http://127.0.0.1:54324`)

---

### Tarefa H2-C — .vscode/mcp.json: MCP Supabase local

**Responsável:** Fernando Gomes (DevOps)  
**Prazo:** 2026-03-22  
**Arquivo criado:** `.vscode/mcp.json`

```json
{
  "servers": {
    "supabase-local": {
      "type": "http",
      "url": "http://127.0.0.1:54321/mcp"
    }
  }
}
```

**Critério de aceite:**

- [ ] Arquivo `.vscode/mcp.json` criado
- [ ] VS Code/Copilot reconhece o servidor MCP (ícone de conectado no painel)
- [ ] Consultas sobre tabelas/schema respondem via Copilot usando dados do Supabase local

---

## Sprint Seguinte (2026-03-25): Completude Técnica

### Tarefa H3-A — Migration: mv_supplier_spending (view materializada)

**Responsável:** João Pereira + André Santos  
**Prazo:** 2026-03-25  
**Arquivo criado:** `supabase/migrations/20260325000000_create_mv_supplier_spending.sql`

```sql
CREATE MATERIALIZED VIEW mv_supplier_spending AS
SELECT
  t.user_id,
  t.supplier_id,
  s.name AS supplier_name,
  date_trunc('month', t.event_date) AS month,
  COUNT(*) AS transaction_count,
  SUM(t.amount) AS total_amount,
  AVG(t.amount) AS avg_amount,
  MAX(t.event_date) AS last_transaction_date
FROM transactions t
JOIN suppliers s ON s.id = t.supplier_id
WHERE t.type IN ('expense', 'fee', 'interest_charge')
  AND t.supplier_id IS NOT NULL
GROUP BY t.user_id, t.supplier_id, s.name, date_trunc('month', t.event_date);

CREATE UNIQUE INDEX ON mv_supplier_spending (user_id, supplier_id, month);
CREATE INDEX ON mv_supplier_spending (user_id, month);
```

**Edge Function scheduled (`refresh-mv-supplier-spending`):**

```typescript
// supabase/functions/refresh-mv-supplier-spending/index.ts
// Chamada a cada hora via pg_cron ou manualmente
// REFRESH MATERIALIZED VIEW CONCURRENTLY mv_supplier_spending
```

**Critério de aceite:**

- [ ] View materializada criada e populável com dados de teste
- [ ] `REFRESH MATERIALIZED VIEW CONCURRENTLY` funciona
- [ ] Edge Function de refresh criada (3ª função planejada)
- [ ] Relatório de gastos por fornecedor usa a MV (mais rápido)

---

### Tarefa H3-B — retroactive-supplier-association: confirm atômico

**Responsável:** Maria Oliveira  
**Prazo:** 2026-03-25

**Refatoring do `handleConfirm`:**

- Criar RPC `confirm_supplier_associations(confirmations jsonb, user_id uuid) RETURNS jsonb`
- RPC executa updates em transação PL/pgSQL
- Edge Function chama a RPC em vez de loop sequencial

**Critério de aceite:**

- [ ] RPC criada em migration
- [ ] Edge Function refatorada para usar RPC
- [ ] Se erro na metade do lote, nenhum registro é atualizado (rollback)
- [ ] Testes de integração cobrindo o fluxo de confirmação em lote

---

### Tarefa H3-C — Google OAuth (planejamento técnico)

**Responsável:** Pedro Santos + Fernando Gomes  
**Prazo:** 2026-03-28

**Pré-requisitos:**

1. CEO cria projeto no Google Cloud Console com o Gmail criado
2. Habilitar Google+ API / People API
3. Criar OAuth 2.0 Client ID (Web application)
4. Adicionar `http://127.0.0.1:54321/auth/v1/callback` como redirect URI local
5. Adicionar redirect URI de produção quando disponível

**Configuração no config.toml:**

```toml
[auth.external.google]
enabled = true
client_id = "env(GOOGLE_CLIENT_ID)"
secret = "env(GOOGLE_SECRET)"
skip_nonce_check = false
```

**Variáveis no .env.local:**

```
GOOGLE_CLIENT_ID=<id-do-google-cloud>
GOOGLE_SECRET=<secret-do-google-cloud>
```

**Critério de aceite:**

- [ ] Login via Google funciona localmente (`npx supabase start`)
- [ ] Redirect para `http://127.0.0.1:3105` após OAuth funciona
- [ ] Usuário criado no Supabase Auth após primeiro login Google
- [ ] Funciona em staging antes de promover para produção

---

## Checklist de Verificação do Ambiente Local

Antes de cada sessão de desenvolvimento, verificar:

```bash
# 1. Supabase local rodando
npx supabase status

# 2. Edge Runtime (para testar Edge Functions)
npx supabase functions serve

# 3. App web
cd apps/web && bun dev

# 4. Studio disponível
# http://127.0.0.1:54323

# 5. Mailpit (emails de auth)
# http://127.0.0.1:54324

# 6. MCP disponível para VS Code
# http://127.0.0.1:54321/mcp
```

---

## Como as Edge Functions Aparecem no Studio

**Pergunta:** Por que não aparecem as edge functions no dashboard do Supabase?

**Resposta:** O serviço `supabase_edge_runtime_seu.bolso.feliz` está **parado**. O output do `supabase status` confirma:

```
Stopped services: [supabase_imgproxy_seu.bolso.feliz supabase_edge_runtime_seu.bolso.feliz supabase_pooler_seu.bolso.feliz]
```

Para as funções aparecerem na aba "Edge Functions" do Studio:

1. Iniciar o Edge Runtime: `npx supabase functions serve`
2. Abrir o Studio: `http://127.0.0.1:54323`
3. Navegar até "Edge Functions" — as funções devem listar automaticamente

**As funções EXISTEM como código** em `supabase/functions/` e serão detectadas pelo runtime quando iniciado.

---

## Roadmap Atualizado

```
Fase H (atual):
  H1 — Gaps Críticos (migrations + CORS + config) ← ESTA SPRINT
  H2 — Auth hardening + MCP local ← ESTA SPRINT
  H3 — mv_supplier_spending + retroactive RPC ← PRÓXIMA SPRINT
  H4 — Google OAuth ← SPRINT +2

Fase G (fechamento):
  Deploy web real automatizado
  Promoção controlada develop → staging → main
  Checklist operacional assinado

Fase E E2 (pendente):
  Auditoria histórica visual por fornecedor na interface
```
