# Plano — Integração Pluggy (Open Finance) + migração de infraestrutura

> Deriva de [`docs/prompts/2026-08-24-prompt-integracao-pluggy.md`](../prompts/2026-08-24-prompt-integracao-pluggy.md),
> reconciliado contra o código real e contra [`docs/specs/00-estado-real.md`](../specs/00-estado-real.md).
> Decisão do CEO em 2026-08-24: a arquitetura-alvo está mudando (ver §1). Este documento
> substitui, para fins de infraestrutura, as premissas de
> [`docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md`](../arquitetura/2026-07-27-arquitetura-hibrida-alvo.md)
> — aquele documento ainda descreve corretamente o pipeline canônico (SourceAdapter →
> EvidenceEnvelope → obrigação → draft → materialização), só não o back-end de infra.

## 0. Por que este plano difere do prompt original

O prompt anexado foi escrito como template genérico e presumia Neon como já-existente
("abandonar qualquer pressuposto de Neon como dependência estrutural obrigatória" — o
projeto real nunca teve Neon; usa Supabase). Duas correções feitas aqui:

1. **Infra:** o CEO confirmou a migração para Vercel + Neon + Worker VPS São Paulo
   (mensagem: "Estou mudando a arquitetura, se vira"). Tratado como decisão tomada, não
   como ambiguidade — ver §1.
2. **Motor de reconciliação:** o prompt (§8–§10) pede um "Reconciliation Engine"
   novo e independente. **Já existe um**, testado e em produção:
   `workers/ingestion/src/reconciliation/reconciliation.ts`, com 4 regras
   (`match_duplicate`/`match_exact`/`match_fuzzy`/`match_recurring`) escritas em
   `draft_records.reconciliation_status`. A integração Pluggy deve **alimentar esse motor
   como nova fonte de evidência**, não duplicá-lo — regra 5 da Verônica (menor mudança
   que satisfaz o contrato).

## 1. Decisão de arquitetura (registrar como ADR-008 ao final da Fase 1)

| Camada              | Antes (`arquitetura-hibrida-alvo.md`) | Depois (este plano)                                                                                                                                                         |
| ------------------- | ------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Banco               | Supabase Postgres                     | **Neon Postgres** (produção)                                                                                                                                                |
| Web/API             | Vercel (já era)                       | Vercel (mantém)                                                                                                                                                             |
| Worker long-running | Máquina local (agente instalável)     | **VPS São Paulo** (`ssh root@ssh.chico-figueiredo.com.br`), local como alternativo/dev                                                                                      |
| Auth/Storage        | Supabase Auth + Storage               | **A decidir na Fase 1**: migrar para alternativa própria ou manter Supabase só para Auth/Storage enquanto o Postgres migra para Neon (arquitetura híbrida — ver gap abaixo) |

**Gap explícito a fechar na Fase 1, não decidido por mim:** o app usa hoje RLS do
Supabase Auth e Storage (buckets `ingestion-originals` etc.) em ~15 Server Actions e no
scanner de Gmail. Migrar _só_ o Postgres para Neon é direto (dump/restore + connection
string); migrar Auth e Storage é um segundo projeto dentro deste. Vou propor manter
Supabase Auth+Storage e mover **apenas o Postgres** para Neon nesta primeira fase — é a
menor mudança que atende ao Goal A sem reescrever autenticação e upload. Se isso não for
o que você quer (ex.: sair do Supabase por completo), me avise; caso contrário, sigo com
essa leitura por padrão.

## 2. O que fica de fora do bloqueio "CEO" vs. o que não fica

🤚 **Requer você, não delegável a subagente:**

- criar/configurar conta e projeto Neon → `DATABASE_URL` de produção;
- acesso `ssh root@ssh.chico-figueiredo.com.br` (a sessão precisa da chave/senha ou de
  você rodando os comandos de deploy iniciais);
- criar app no **Meu Pluggy** → `PLUGGY_CLIENT_ID` / `PLUGGY_CLIENT_SECRET`;
- vincular domínio/projeto de produção na Vercel, se ainda não vinculado;
- decidir o gap de Auth/Storage do §1 se a leitura padrão acima não servir.

⏳ **Não bloqueado — Bolso-Equipe segue sozinho:**

- todo o desenho de código: `FinancialDataProvider`, `PluggyProvider`, schema/migrations,
  normalização, matching, dedup, testes, UI de conexão/revisão, worker `pluggy-sync`,
  documentação técnica, feature flag.
- migração do Postgres para Neon em si (schema/migrations são portáveis; só a
  `DATABASE_URL` final depende do item 🤚 acima — até lá, roda contra Neon de dev/branch
  se disponível, ou localmente).

## 3. Fases e gates (adaptado das seções 35.1 e 36 do prompt)

### Fase 1 — Arquitetura preparada (Goal A, 1)

- Registrar ADR-008 (decisão §1).
- Provisionar Neon (branch de dev primeiro), rodar migrations existentes nele.
- Isolar o worker de longa duração da Vercel (se ainda não estiver — conferir `workers/ingestion`).
- **Gate:** `bun run build` + `bun run typecheck` + migrations aplicadas em Neon dev.

### Fase 2 — Provider Pluggy (Goal B, 2)

- `packages/financial-connectors/` com contrato `FinancialDataProvider` e `PluggyProvider`.
- Tabelas `provider_connections`, `external_account_mappings` (reaproveitar `source_type`/proveniência já existente em `source_documents` como modelo).
- Autenticação server-side only; nenhum secret no bundle web (checagem automatizada no CI).
- **Gate:** conexão sandbox Pluggy autenticada, contas retornadas, nenhum secret no client bundle (`grep` no build output).

### Fase 3 — Backfill 365 dias (Goal C, 6)

- Job de backfill com checkpoint, paginação, retomada, idempotência — reaproveitar o padrão de `ingestion_jobs`/`transitionJob` (locking otimista já existe, ver arquitetura-hibrida-alvo §"Nota de P0").
- **Gate:** reexecução do backfill não duplica; interrupção no meio retoma sem perda.

### Fase 4 — Reconciliação (Goal D, 8-12)

- `PluggyProvider` normaliza para `NormalizedTransaction` e entra no pipeline **existente** (`SourceAdapter` → evidência → `reconciliation.ts`), não um motor paralelo.
- Extensão do matching (valor/data/merchant/documento) só onde o motor atual ainda não cobre para transações bancárias puras.
- **Gate:** casos de teste do prompt §37 (100 transações sintéticas, 40 e-mails, 20 PDFs, 15 recorrências, 10 parcelamentos, 10 conflitos, 10 ambíguos) rodando como suíte, sem payload real de usuário no repo.

### Fase 5 — Worker de produção na VPS (Goal 5) — 🤚 parcialmente

- Dockerfile/compose, healthcheck, restart policy — código pode ser feito sem acesso à VPS.
- Deploy real e verificação de restart **precisa** do acesso SSH (🤚).

### Fase 6 — Produção Vercel + Neon (Goal 6) — 🤚

- Depende das credenciais do item 🤚.

### Fase 7 — Ponta a ponta (Goal 7)

- Só roda depois das Fases 5 e 6 estarem realmente em produção.

## 4. Alocação de subagentes (mais barato adequado à tarefa)

| Tarefa                                                         | Perfil                                                                      |
| -------------------------------------------------------------- | --------------------------------------------------------------------------- |
| Leitura/mapeamento do código existente, checagem de convenções | `Explore` (barato, leitura)                                                 |
| Migrations SQL, tipos, DTOs mecânicos                          | subagente `general-purpose`, esforço baixo                                  |
| `PluggyProvider`, normalização, matching/scoring               | subagente com esforço médio/alto (lógica de domínio)                        |
| Testes (unit + fixtures sintéticas)                            | subagente dedicado, esforço médio, roda em paralelo à implementação via TDD |
| Infra (Neon/VPS/Docker)                                        | subagente único, esforço alto — não paralelizar infra com código de domínio |
| Documentação (`docs/integrations/pluggy.md`)                   | subagente barato, roda por último                                           |

Máximo 3 workers simultâneos por fase, um dono por arquivo/módulo mutável (regra padrão
da Verônica), fan-in obrigatório antes de cada gate.

## 5. Critérios de não-regressão (herdados do repo)

- `bun run typecheck`, `bun run lint`, `bun run test` verdes a cada gate.
- Nenhum segredo (`CLIENT_SECRET`, `DATABASE_URL` de produção) commitado — checagem
  automatizada antes de cada fechamento de fase (o repo já tem histórico de vazamento de
  credencial de teste; ver `docs/_arquivo` e memória do projeto — trate como não-negociável).
- Overrides manuais do usuário (categoria corrigida etc.) nunca são sobrescritos por sync — já é princípio do domínio existente (`05-dominio-financeiro.md`).
