---
Título da Reunião: Segunda Auditoria — Edge Functions, Configurações Supabase e Estado Pós-Implementação
Data e Hora: 2026-03-22 21:27
Participantes:
  - Chico (CEO) — solicitante, decisão final
  - Ana Silva (Arquiteta de Software) — facilitadora
  - João Pereira (Backend Sênior Node/Bun) — anotador
  - Maria Oliveira (Backend Sênior Node/Bun) — revisora técnica
  - André Santos (DBA Sênior PostgreSQL) — revisão de schema e RLS
  - Fernando Gomes (DevOps Sênior) — revisão de CI/CD e deploy
  - Ricardo Monteiro (Economista/Consultor Financeiro) — validação de domínio
Pauta:
  - 1. Auditoria de Edge Functions: o que estava planejado? qual é o status real?
  - 2. Auditoria de configurações Supabase: RLS, Storage, Auth — o que faltava e o que foi feito?
  - 3. Reconciliação do checklist 002-pedidos-veronica.md (itens implementados ainda marcados como pendentes)
  - 4. O que era esperado vs. o que está entregue?
---

## Contexto

Esta é a **segunda auditoria** do projeto, realizada em 2026-03-22 após a implementação das correções críticas de segurança identificadas na primeira auditoria (commit `34a0d0c`). O objetivo é responder com precisão às três perguntas do CEO:

1. **Havia Edge Functions no planejamento inicial? Qual é o status?**
2. **Havia configurações Supabase específicas que deveriam ter sido feitas e não foram?**
3. **O que era esperado?**

---

## Discussão

### Ana Silva (Arquiteta) — abertura

O planejamento inicial do projeto (Verônica, `docs/Veronica/001-prompt.inicial.md`, seção 2.1) listou como stack-base do backend:

> Supabase como backend principal — Postgres, Auth, RLS, Storage, **Edge Functions**, rotinas agendadas server-side

Edge Functions estão explicitamente no escopo desde o kickoff. Não são uma adição posterior — fazem parte da arquitetura definida. Três funções foram planejadas, duas foram implementadas, uma ainda não foi criada.

### João Pereira (Backend) — status das Edge Functions

Cruzando o codebase atual com o planejamento:

| Função                             | Planejada em          | Status                               | Commit de implementação |
| ---------------------------------- | --------------------- | ------------------------------------ | ----------------------- |
| `merge-suppliers`                  | Etapa 5.4 / ADR-003   | ✅ Implementada + CORS               | b1394ce / 34a0d0c       |
| `retroactive-supplier-association` | Etapa 5.4 / ADR-003   | ⚠️ Implementada, confirm não-atômico | b1394ce / 34a0d0c       |
| `refresh-mv-supplier-spending`     | Etapa 5.4 (implícita) | ❌ Não criada                        | —                       |

**Detalhe crítico sobre `retroactive-supplier-association`:** A função existe e funciona. O endpoint `GET /suggest` é stateless e seguro. O endpoint `POST /confirm` faz atualizações em loop — se falhar no meio, parte do lote é aplicada e outra não. Precisa ser refatorada para usar uma RPC PL/pgSQL atômica que envolva tudo em uma transação.

**Detalhe crítico sobre `refresh-mv-supplier-spending`:** Esta função depende da view materializada `mv_supplier_spending`, que **também não existe ainda**. Portanto, há duas entregas pendentes: (1) criar a MV e (2) criar a Edge Function que faz o refresh.

### Maria Oliveira (Backend) — configurações Supabase auditadas

Comparando `supabase/config.toml` atual (commit `34a0d0c`) com o estado pré-auditoria:

#### Auth — o que foi corrigido (commit 34a0d0c):

| Parâmetro                 | Era             | Ficou                          | Status |
| ------------------------- | --------------- | ------------------------------ | ------ |
| `minimum_password_length` | 6               | 8                              | ✅     |
| `password_requirements`   | `""` (vazio)    | `"lower_upper_letters_digits"` | ✅     |
| `enable_confirmations`    | `false`         | `true`                         | ✅     |
| `secure_password_change`  | `false`         | `true`                         | ✅     |
| `[auth.email.smtp]`       | não configurado | Mailpit local ativo            | ✅     |

#### Auth — o que AINDA falta:

| Item                                               | Status             | Risco                             |
| -------------------------------------------------- | ------------------ | --------------------------------- |
| Google OAuth (`[auth.external.google]`)            | ❌ Não configurado | Médio — CEO criou Gmail para isso |
| MFA TOTP (`[auth.mfa.totp] enroll_enabled = true`) | ❌ Não habilitado  | Baixo — opcional para MVP         |
| Session timebox (`[auth.sessions]`)                | ❌ Comentado       | Baixo — melhoria de segurança     |

#### Migrations de segurança — o que foi feito (commit 34a0d0c):

| Gap                         | Migration                                        | Status |
| --------------------------- | ------------------------------------------------ | ------ |
| C1: audit_logs deletável    | `20260322180000_fix_audit_logs_immutability.sql` | ✅     |
| C2: storage sem buckets     | `20260322180100_create_storage_buckets.sql`      | ✅     |
| C3: user_secrets texto puro | `20260322180200_encrypt_user_secrets.sql`        | ✅     |

#### Storage — estado atual:

- Buckets `documents` e `imports` criados via migration ✅
- RLS policies por `user_id` via path pattern (`<user_id>/<filename>`) ✅
- `[storage.buckets.*]` **não está no config.toml** — mas isso é intencional: a migration SQL é o caminho correto para Supabase

#### RLS — cobertura:

Todas as 27 tabelas têm RLS habilitado (migration `20260321170500`). A política `FOR ALL` da `audit_logs` foi substituída por políticas granulares + trigger de imutabilidade.

### André Santos (DBA) — view materializada mv_supplier_spending

A ausência da view materializada `mv_supplier_spending` é um gap de performance que começa a importar quando o volume de transações/fornecedores cresce. O Verônica prompt 002 pede explicitamente relatórios por fornecedor com histórico — a MV é a infraestrutura de suporte para essa query ser eficiente.

Proposta de estrutura mínima:

```sql
CREATE MATERIALIZED VIEW mv_supplier_spending AS
SELECT
  s.user_id,
  s.id AS supplier_id,
  s.name AS supplier_name,
  DATE_TRUNC('month', t.event_date) AS month,
  COUNT(t.id) AS transaction_count,
  SUM(t.amount) AS total_amount,
  AVG(t.amount) AS avg_amount
FROM suppliers s
JOIN transactions t ON t.supplier_id = s.id
WHERE t.type IN ('expense', 'fee', 'interest_charge')
GROUP BY s.user_id, s.id, s.name, DATE_TRUNC('month', t.event_date);

CREATE UNIQUE INDEX ON mv_supplier_spending (user_id, supplier_id, month);
```

O `REFRESH MATERIALIZED VIEW CONCURRENTLY` exige um índice único — incluí-lo na migration é obrigatório para que o refresh funcione sem lock.

### Fernando Gomes (DevOps) — CORS e deploy de funções

O deploy das Edge Functions no CI/CD já está configurado no `.gitlab-ci.yml` (job `deploy-functions`). Com CORS implementado nas duas funções existentes, o deploy por CI/CD está correto.

A terceira função (`refresh-mv-supplier-spending`) precisa ser criada antes de qualquer tentativa de deploy automatizado — caso contrário, o CI vai tentar fazer deploy de um diretório inexistente e falhar.

### Ricardo Monteiro (Economista) — seed.sql vazio

Do ponto de vista funcional, `seed.sql` vazio significa que ao rodar `supabase db reset`, o banco fica sem dados básicos de domínio. Categorias (alimentação, moradia, transporte, saúde, lazer, educação, etc.) e tags padrão (essencial, fixo, variável, etc.) deveriam estar no seed para que o sistema seja utilizável logo após o reset local.

---

## Respostas Diretas às Perguntas do CEO

### 1. Havia Edge Functions no planejamento inicial? Qual é o status?

**Sim.** Edge Functions estão explícitas no planejamento desde o kickoff (`docs/Veronica/001-prompt.inicial.md`, seção 2.1). Três funções foram planejadas, com foco na dimensão de fornecedor (ADR-003 e ADR-004).

**Status atual:**

| Função                             | Status                                                                  |
| ---------------------------------- | ----------------------------------------------------------------------- |
| `merge-suppliers`                  | ✅ Implementada, CORS, deploy-ready                                     |
| `retroactive-supplier-association` | ⚠️ Implementada, CORS presente, mas confirm não-atômico                 |
| `refresh-mv-supplier-spending`     | ❌ Não criada (depende de `mv_supplier_spending` que também não existe) |

### 2. Havia configurações Supabase específicas que deveriam ter sido feitas e não foram?

**Sim, várias.** Na auditoria pós-commit `34a0d0c`, o estado é:

**Já corrigidos (commit 34a0d0c):**

- ✅ RLS audit_logs: imutável (trigger + policies restritas)
- ✅ Storage: buckets `documents` e `imports` com RLS por usuário
- ✅ Criptografia de `user_secrets.encrypted_value` (pgcrypto)
- ✅ CORS nas Edge Functions
- ✅ auth: senha mínima 8 + complexidade + confirmações + troca segura
- ✅ auth SMTP: Mailpit funcional em dev
- ✅ MCP local: `.vscode/mcp.json` configurado

**Ainda pendentes:**

- ❌ Google OAuth (`[auth.external.google]`)
- ❌ `mv_supplier_spending` (view materializada)
- ❌ `refresh-mv-supplier-spending` (3ª Edge Function)
- ❌ confirm atômico em `retroactive-supplier-association`
- ❌ `seed.sql` com dados base
- ❌ MFA TOTP, session timebox, ssl_enforcement (opcionais)

### 3. O que era esperado?

O `copilot-instructions.md` e os prompts da Verônica estabelecem como esperado para o MVP:

- **Supabase completo:** Auth configurado, RLS em todas as tabelas, Storage com buckets, Edge Functions para operações complexas, rotinas agendadas.
- **Segurança desde o início:** A instrução "Segurança: Dados financeiros exigem tratamento cuidadoso. Segredos, permissões e RLS são obrigatórios desde o início." estava nas diretrizes.
- **Edge Functions para operações que exigem atomicidade:** merge de fornecedores, associação retroativa, refresh de dados agregados.
- **Configuração de autenticação robusta:** senha forte, confirmações, OAuth social.

O que foi entregue nas implementações até agora cobre a maior parte. Os gaps restantes são claros, são poucos, e têm prioridade definida no planejamento `docs/planejamento/004-pos-auditoria-supabase-seguranca.md`.

---

## Prós e Contras das Abordagens para o que Resta

### Opção A: Criar mv_supplier_spending + refresh Edge Function primeiro

- **Prós:**
  - Completa a infraestrutura de relatórios de fornecedor (prometida pela Verônica)
  - Habilita a tela de auditoria histórica (E2.3 pendente)
  - Desbloqueia a 3ª Edge Function planejada
- **Contras:**
  - Requer atenção ao índice único para `REFRESH CONCURRENTLY`
  - Adiciona complexidade operacional (quando disparar o refresh?)

### Opção B: Refatorar confirm atômico primeiro

- **Prós:**
  - Elimina risco de estado inconsistente em operação que já está em produção
  - Segurança de dados — evitar associação parcial
- **Contras:**
  - Mudança de comportamento existente — precisa testar regressão
  - Menos visível para o usuário final

### Opção C: Google OAuth primeiro

- **Prós:**
  - CEO já tem Gmail criado para isso — desbloqueia fluxo de autenticação real
  - Alta visibilidade / experiência de usuário
- **Contras:**
  - Requer credenciais OAuth no Google Cloud Console (ação manual do CEO)
  - Bloqueante por dependência externa

---

## Decisão Final

**Ordem de execução recomendada pela equipe:**

1. **Agora:** Atualizar checklist para refletir estado real (itens já implementados marcados como ✅)
2. **Sprint 1 (2026-03-23):** `mv_supplier_spending` + `refresh-mv-supplier-spending` Edge Function + confirm atômico
3. **Sprint 2 (2026-03-24):** `seed.sql` com dados base + tela de auditoria histórica de fornecedor (E2.3)
4. **Sprint 3 (quando CEO tiver credenciais Google):** Google OAuth

---

## Ações / Responsáveis / Prazo

| Ação                                                | Responsável    | Prazo      |
| --------------------------------------------------- | -------------- | ---------- |
| Corrigir checklist (marcar H1 + H2 parcial como ✅) | João/Maria     | 2026-03-22 |
| Migration: `mv_supplier_spending` com índice único  | André          | 2026-03-23 |
| Edge Function: `refresh-mv-supplier-spending`       | João           | 2026-03-23 |
| Refatorar confirm atômico (`handleConfirm` → RPC)   | Maria          | 2026-03-23 |
| Testes para confirm atômico                         | Maria          | 2026-03-23 |
| `seed.sql` com categorias/tags base                 | João           | 2026-03-24 |
| Tela de auditoria histórica por fornecedor (E2.3)   | Roberto/Sofia  | 2026-03-24 |
| Google OAuth (bloqueante: CEO cria credenciais)     | Fernando + CEO | A definir  |
| Itens opcionais (MFA, timebox, ssl_enforcement)     | Fernando       | Pós-MVP    |
