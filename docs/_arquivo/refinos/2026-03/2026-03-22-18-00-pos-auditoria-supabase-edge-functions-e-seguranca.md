---
Título da Reunião: Pós-Auditoria — Edge Functions, Supabase Local e Segurança
Data e Hora: 2026-03-22 18:00
Participantes:
  - Francisco "Chico" Figueiredo (CEO) — facilitador
  - Ana Silva (Arquiteta de Software) — anotadora
  - João Pereira (Backend Sênior NodeJS/Bun) — relator de Edge Functions
  - Maria Oliveira (Backend Sênior NodeJS/Bun) — relator de Edge Functions
  - Fernando Gomes (DevOps Sênior) — relator de config.toml e MCP
  - André Santos (DBA Sênior PostgreSQL) — relator de RLS e Storage
  - Pedro Santos (Backend Sênior Python/Django) — relator de Auth
  - Carlos Mendes (Designer de Software) — observador
Pauta:
  - Análise dos resultados da auditoria de Edge Functions e Configurações do Supabase
  - Definir criticalidade e prioridade de cada gap encontrado
  - Responder perguntas do CEO sobre Edge Functions no dashboard e testes locais
  - Planejar correção dos gaps críticos (C1, C2, C3)
  - Configurar MCP do Supabase local no VS Code
  - Atualizar checklist e criar planejamento de continuidade
---

## Contexto

Esta reunião é convocada pelo CEO imediatamente após a auditoria técnica completa do projeto, que revelou o status das Edge Functions e das configurações do Supabase local. A auditoria cruzou o codebase com os documentos de planejamento (`docs/planejamento/001-guia-implementacao-passo-a-passo.md`) e os ADRs publicados.

---

## Discussão

### 1. Edge Functions — Status e Testes Locais

**João Pereira (Backend Sênior):** A auditoria confirmou que das 3 Edge Functions planejadas, 2 estão implementadas e 1 ausente:

| #   | Função                             | Status                                 |
| --- | ---------------------------------- | -------------------------------------- |
| 1   | `merge-suppliers`                  | ✅ Implementada (~90% completa)        |
| 2   | `retroactive-supplier-association` | ✅ Implementada (~75% completa)        |
| 3   | `refresh-mv-supplier-spending`     | ❌ Não criada (planejada na Etapa 5.4) |

**Por que não aparecem no Studio (`http://127.0.0.1:54323`)?**

**Fernando Gomes (DevOps):** A resposta é direta: o serviço `supabase_edge_runtime_seu.bolso.feliz` está **parado**. O output do `supabase status` mostra:

```
Stopped services: [supabase_imgproxy_seu.bolso.feliz supabase_edge_runtime_seu.bolso.feliz supabase_pooler_seu.bolso.feliz]
```

O Studio lista Edge Functions apenas quando o Edge Runtime está ativo. **As funções existem como código** em `supabase/functions/`, mas para aparecerem no Studio e serem testadas localmente, o runtime precisa estar rodando.

**Como testar Edge Functions localmente:**

Opção A — runtime completo (estável, igual à produção):

```bash
npx supabase functions serve
```

Isso inicia o Deno Edge Runtime e todas as funções ficam acessíveis em `http://127.0.0.1:54321/functions/v1/<nome-da-funcao>`.

Opção B — função específica:

```bash
npx supabase functions serve merge-suppliers
npx supabase functions serve retroactive-supplier-association
```

**Após iniciar**, para testar com cURL:

```bash
# Teste merge-suppliers (requer JWT de usuário autenticado)
curl -X POST http://127.0.0.1:54321/functions/v1/merge-suppliers \
  -H "Authorization: Bearer <JWT_TOKEN>" \
  -H "Content-Type: application/json" \
  -d '{"source_supplier_id": "<uuid>", "target_supplier_id": "<uuid>"}'

# Teste retroactive-supplier-association (GET — busca candidatos)
curl http://127.0.0.1:54321/functions/v1/retroactive-supplier-association \
  -H "Authorization: Bearer <JWT_TOKEN>"
```

Para obter um JWT de teste, autentique via Supabase Auth local e capture o `access_token`.

**Maria Oliveira (Backend Sênior):** Vale reforçar que as funções usam `SUPABASE_URL`, `SUPABASE_SERVICE_ROLE_KEY` e `SUPABASE_ANON_KEY` do ambiente — todos são auto-injetados pelo runtime local sem configuração adicional.

---

### 2. Checklist de Tudo que Deveria Aparecer no Supabase Studio

**André Santos (DBA):** Ao abrir o Studio (`http://127.0.0.1:54323`), o time deve verificar:

| Item                  | Como verificar no Studio                 | Status esperado                                         |
| --------------------- | ---------------------------------------- | ------------------------------------------------------- |
| Tabelas (27)          | Table Editor → todas as tabelas listadas | ✅ Deve aparecer após migrations                        |
| RLS ativo nas tabelas | Table Editor → cada tabela → RLS tab     | ✅ Configurado na migration 170500                      |
| Edge Functions        | Edge Functions tab                       | ⚠️ Só aparece com Edge Runtime rodando                  |
| Storage buckets       | Storage tab                              | ❌ VAZIO — nenhum bucket foi criado                     |
| Auth providers        | Auth → Providers                         | ⚠️ Apenas email (sem Google OAuth)                      |
| Auth users            | Auth → Users                             | Deve mostrar usuários de teste                          |
| API docs              | API tab → schema auto-gerado             | Deve aparecer para todas as tabelas                     |
| Logs                  | Logs tab                                 | Disponível quando serviços ativos                       |
| pg_net / extensões    | Database → Extensions                    | Deve mostrar extensões ativas (pg_trgm, pgcrypto, etc.) |

---

### 3. Gaps Críticos de Segurança

**Ana Silva (Arquiteta):** A auditoria identificou 3 gaps críticos que precisam de correção imediata antes de qualquer promoção para produção:

#### C1 — audit_logs deletáveis pelo usuário (RLS FOR ALL)

**Problema:** A policy atual em `audit_logs` usa `FOR ALL USING (auth.uid() = user_id)`, o que permite ao usuário executar `DELETE` nos próprios logs de auditoria. Isso destrói a imutabilidade, que é o propósito central da tabela.

**Solução:** Substituir `FOR ALL` por `FOR SELECT` + `FOR INSERT` na policy, e adicionar um trigger `BEFORE DELETE OR UPDATE` que levanta exception.

**André Santos (DBA):** Concordo. O trigger é a camada mais robusta porque garante imutabilidade mesmo se alguém usar `service_role` por engano em código futuro. Proposta:

```sql
-- Substituir policy atual
DROP POLICY IF EXISTS "Users can manage their own audit_logs" ON audit_logs;
CREATE POLICY "Users can read own audit_logs" ON audit_logs
  FOR SELECT USING (auth.uid() = user_id);
CREATE POLICY "System can insert audit_logs" ON audit_logs
  FOR INSERT WITH CHECK (auth.uid() = user_id);

-- Trigger de imutabilidade
CREATE OR REPLACE FUNCTION prevent_audit_log_mutation()
RETURNS TRIGGER AS $$
BEGIN
  RAISE EXCEPTION 'audit_logs are immutable -- % not allowed', TG_OP;
END;
$$ LANGUAGE plpgsql;

CREATE TRIGGER trg_audit_logs_immutable
  BEFORE UPDATE OR DELETE ON audit_logs
  FOR EACH ROW EXECUTE FUNCTION prevent_audit_log_mutation();
```

#### C2 — Nenhum bucket de Storage criado

**Problema:** `config.toml` tem Storage habilitado (50MiB, S3 protocol), mas nenhum bucket foi criado. A tabela `documents` referencia `file_path text NOT NULL` que pressupõe bucket existente. Qualquer tentativa de upload vai falhar.

**Fernando Gomes:** Precisamos criar migration para o bucket `documents` (privado) e possivelmente `imports` para arquivos CSV/XLSX. A policy de Storage segue o padrão: usuário acessa apenas seus próprios arquivos via `auth.uid()` no path do objeto.

**Proposta de migration:**

```sql
-- Criar buckets
INSERT INTO storage.buckets (id, name, public, file_size_limit, allowed_mime_types)
VALUES
  ('documents', 'documents', false, 52428800,
   ARRAY['application/pdf','image/jpeg','image/png','image/webp','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet','text/csv']),
  ('imports', 'imports', false, 52428800,
   ARRAY['text/csv','application/vnd.ms-excel','application/vnd.openxmlformats-officedocument.spreadsheetml.sheet']);

-- Policies de acesso (Storage RLS)
CREATE POLICY "Users can upload own documents"
  ON storage.objects FOR INSERT
  WITH CHECK (bucket_id = 'documents' AND (storage.foldername(name))[1] = auth.uid()::text);

CREATE POLICY "Users can read own documents"
  ON storage.objects FOR SELECT
  USING (bucket_id = 'documents' AND (storage.foldername(name))[1] = auth.uid()::text);

CREATE POLICY "Users can delete own documents"
  ON storage.objects FOR DELETE
  USING (bucket_id = 'documents' AND (storage.foldername(name))[1] = auth.uid()::text);

-- Mesmas policies para 'imports'
...
```

#### C3 — user_secrets.encrypted_value em texto puro

**Problema:** A coluna `encrypted_value text NOT NULL` armazena segredos (ex: senhas de PDF) em texto puro. O nome da coluna sugere encriptação mas ela não existe. `pg_vault` está comentado no `config.toml`.

**André Santos:** Como solução pragmática para o MVP, podemos usar `pgp_sym_encrypt` do pgcrypto (já instalado como extensão). A chave de encriptação pode vir de uma env var segura. Isso não é ideal (vault seria melhor), mas é incomparavelmente melhor que texto puro.

```sql
-- Alternativa simples com pgcrypto já instalado:
-- CREATE EXTENSION IF NOT EXISTS pgcrypto; -- já existe

-- Na aplicação, ao salvar:
-- encrypted_value = pgp_sym_encrypt(valor_real, current_setting('app.encryption_key'))

-- Ao ler:
-- pgp_sym_decrypt(encrypted_value::bytea, current_setting('app.encryption_key'))
```

**Pedro Santos:** Vale notar que a `app.encryption_key` deve vir de variável de ambiente e ser configurada via `supabase secrets set` para as Edge Functions, e via `ALTER DATABASE SET` para uso no banco.

---

### 4. Gaps Importantes (Auth + Views + Edge Functions)

**Pedro Santos (Backend Python/Django):** Os gaps de autenticação são todos de configuração no `config.toml` e/ou Edge Functions secrets. Resumindo os mais urgentes:

| Gap                            | Impacto                        | Solução                                  |
| ------------------------------ | ------------------------------ | ---------------------------------------- |
| `minimum_password_length = 6`  | Baixo para produto financeiro  | Alterar para `8` em dev                  |
| `enable_confirmations = false` | Usuários sem e-mail confirmado | Ativar `true`                            |
| Sem Google OAuth               | Feature planejada bloqueada    | Adicionar bloco `[auth.external.google]` |
| SMTP não configurado           | Emails de auth não funcionam   | Mailpit já disponível localmente         |

**Maria Oliveira:** Sobre SMTP localmente — o Mailpit já roda em `http://127.0.0.1:54324`. Basta descomentar a seção de SMTP no config.toml apontando para `localhost:54325` (inbound_port).

**João Pereira:** Sobre a Edge Function `retroactive-supplier-association` — o `handleConfirm` atual executa updates em loop sequencial sem transação atômica. Se falhar no meio, o estado fica inconsistente. A correção ideal é extrair o batch update para uma RPC PL/pgSQL (como fizemos para o merge).

---

### 5. MCP do Supabase Local no VS Code

**Fernando Gomes:** O Supabase CLI v2.83.0 já expõe um servidor MCP nativo em `http://127.0.0.1:54321/mcp`. Isso aparece no `supabase status`:

```
│ MCP     │ http://127.0.0.1:54321/mcp │
```

Para conectar o VS Code, criar `.vscode/mcp.json` com:

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

Isso expõe no Copilot Chat acesso às tabelas, dados, edge functions, storage e auth do Supabase local — facilitando desenvolvimento e depuração diretamente no editor.

**Ana Silva:** Importante documentar que o MCP local só funciona quando o Supabase está rodando (`npx supabase start`). Para projeto que vai para produção, futuramente será criado um MCP apontando para o projeto Supabase em nuvem usando o `SUPABASE_ACCESS_TOKEN`.

---

## Prós e Contras

### Opção A — Corrigir gaps críticos via novas migrations

- **Prós:**
  - Histórico de banco versionado e reproduzível
  - CI/CD já aplica migrations automaticamente
  - Reversível via down migration
- **Contras:**
  - Número de migrations crescendo (já são 8)
  - Precisa de reset local para testar (ou `supabase db reset`)

### Opção B — Corrigir C2 (Storage) via config.toml `[storage.buckets]`

- **Prós:**
  - Mais simples para desenvolvimento local
  - Não precisa de migration
- **Contras:**
  - Não funciona para produção (buckets em cloud precisam de migration ou API de management)
  - Inconsistência entre ambientes

**Decisão para C1 e C2 e C3:** Opção A (nova migration `20260322*`) — garante consistência entre dev, staging e produção.

---

## Decisão Final

O time decidiu por unanimidade:

1. **C1, C2, C3 (gaps críticos):** Corrigir via nova migration antes do próximo push.
2. **I1–I6 (Auth):** Corrigir `config.toml` localmente + documentar o que vai para produção.
3. **I7 (mv_supplier_spending):** Criar na próxima sprint como migration `20260323*`.
4. **I8 (retroactive sem transação):** Refatorar para usar RPC atômica — próxima sprint.
5. **I9 (CORS nas Edge Functions):** Adicionar headers de CORS em ambas as funções agora.
6. **MCP local:** Criar `.vscode/mcp.json` apontando para `http://127.0.0.1:54321/mcp`.
7. **Edge Runtime:** Documentar no README como inicializar (`npx supabase functions serve`).

---

## Ações / Responsáveis / Prazo

| Ação                                                         | Responsável    | Prazo      |
| ------------------------------------------------------------ | -------------- | ---------- |
| Migration: RLS imutabilidade audit_logs + trigger            | André Santos   | 2026-03-22 |
| Migration: Storage buckets + policies                        | Fernando Gomes | 2026-03-22 |
| Migration: pgcrypto encrypt para user_secrets                | André Santos   | 2026-03-22 |
| Corrigir config.toml: senha mínima + confirmations + mailpit | Pedro Santos   | 2026-03-22 |
| Adicionar CORS headers em merge-suppliers e retroactive      | João Pereira   | 2026-03-22 |
| Criar `.vscode/mcp.json` com Supabase MCP local              | Fernando Gomes | 2026-03-22 |
| Atualizar checklist 002-pedidos-veronica.md                  | Ana Silva      | 2026-03-22 |
| Criar planejamento 004-pos-auditoria-supabase.md             | Ana Silva      | 2026-03-22 |
| Documentar Edge Functions serve no README/passo-a-passo      | Maria Oliveira | 2026-03-22 |
| Sprint planejada: mv_supplier_spending + retroactive RPC     | João + André   | 2026-03-25 |
