# 00 — Estado Real do Projeto (mapa único e verificado)

> **Esta é a única fonte de verdade sobre o que está pronto.** Foi verificada contra o
> código em 2026-06-20, não contra documentos. Sempre que reabrir, **reconfirme rodando**
> `bun run typecheck` e `bun run test` antes de confiar em qualquer linha.
>
> Legenda: ✅ Pronto e testado · 🟡 Parcial (existe, falta X) · ⬜ Faltando · 🔒 Bloqueado por você (CEO)

## Saúde do código (medida, não afirmada)

| Sinal                | Resultado                                      | Como reproduzir            |
| -------------------- | ---------------------------------------------- | -------------------------- |
| TypeScript           | ✅ passa limpo                                 | `bun run typecheck`        |
| Testes unitários     | ✅ **319/319** (20 arquivos)                   | `bun run test:unit`        |
| Testes de integração | 🟡 existem (3 arquivos), exigem Supabase local | `bun run test:integration` |
| Build web            | a confirmar                                    | `bun run build`            |

> ⚠️ A grande mentira dos docs antigos: o `checklists/004-consolidado-gaps-pendentes.md`
> dizia UI de ingestão **0/15** e IA OpenAI **0/11 bloqueado**. Ambas estão construídas
> e cobertas por testes. Por isso o projeto "não andava" — você nunca teve um mapa real.

---

## Mapa por capacidade

### 1. Ingestão (pipeline de documentos) — ✅ em grande parte

Spec detalhado: [`01-ingestao.md`](01-ingestao.md)

| Item                                                             | Estado                       | Evidência                                                          |
| ---------------------------------------------------------------- | ---------------------------- | ------------------------------------------------------------------ |
| Máquina de estados (13 status)                                   | ✅                           | `workers/ingestion/src/state-machine.ts` + `state-machine.test.ts` |
| Processor (download→hash→parse→enrich→draft)                     | ✅                           | `workers/ingestion/src/processor.ts`                               |
| Extração de texto (PDF nativo + OCR ocrmypdf + senha)            | ✅                           | `parsers/text-extractor.ts`                                        |
| Parsers determinísticos (boleto, CEMIG, boleto-utils, templates) | ✅ (boleto-parser 🟡 básico) | `parsers/*.ts`                                                     |
| Consenso de campos multi-fonte                                   | ✅                           | `parsers/field-consensus.ts`                                       |
| Enriquecimento IA lite (gpt-4o-mini) + full (gpt-4o Vision)      | ✅ chamadas reais à OpenAI   | `parsers/ai-lite-enricher.ts`, `ai-full-enricher.ts`               |
| Geração de drafts                                                | ✅                           | `draft-generation` + `draft-generation.test.ts`                    |
| Scanner Gmail (OAuth2, label/query, dedup 2 fases)               | ✅                           | `workers/gmail-scanner/`                                           |
| Scanner pasta local (scan-once + watch)                          | ✅                           | `workers/local-scanner/`                                           |
| Orquestrador de evidências (Gmail+local CLI unificada)           | ✅                           | `workers/financial-evidence-worker/` (spec 004)                    |

### 2. UI de ingestão e revisão — 🟡 funciona, falta split-view real

Spec detalhado: [`04-ia-e-chat.md`](04-ia-e-chat.md) (parte de UI) + este mapa

| Item                                                            | Estado                                                               | Evidência                                             |
| --------------------------------------------------------------- | -------------------------------------------------------------------- | ----------------------------------------------------- |
| `/dashboard/ingestion` (visão geral, stats, upload)             | ✅                                                                   | `apps/web/src/app/dashboard/ingestion/page.tsx`       |
| `/ingestion/review` + `[batchId]` (aprovar/rejeitar drafts)     | ✅                                                                   | `review/[batchId]/page.tsx` + `draft-review-form.tsx` |
| `/ingestion/documents` + `[id]` (lista, filtros, detalhe, PDF)  | ✅                                                                   | `ingestion/documents/`                                |
| `/ingestion/patterns` + `[id]` (padrões documentais)            | 🟡 UI existe; ciclo de feedback a validar                            | `ingestion/patterns/`                                 |
| `/ingestion/logs`                                               | ✅                                                                   | `ingestion/logs/page.tsx`                             |
| Split-view documento × draft com edição inline antes de aprovar | 🟡 aprovar/rejeitar OK; **sem painel lado-a-lado com editor inline** | `draft-review-form.tsx`                               |
| Upload manual drag-and-drop + dedup por hash                    | ✅                                                                   | commits `19b7a4d`, `9026742`                          |

### 3. IA e Chat — ✅ integrado de verdade

Spec detalhado: [`04-ia-e-chat.md`](04-ia-e-chat.md)

| Item                                                         | Estado                                              | Evidência                                                         |
| ------------------------------------------------------------ | --------------------------------------------------- | ----------------------------------------------------------------- |
| Chat drawer com histórico, streaming                         | ✅                                                  | `apps/web/src/components/ai-chat-drawer.tsx`, `api/chat/route.ts` |
| Upload-pelo-chat → Storage → `trigger-ingestion`             | ✅                                                  | `ai-chat-drawer.tsx`                                              |
| Sugestões inline (fornecedor, splits, conciliação, explicar) | ✅                                                  | `api/ai-suggest/route.ts`, hook `useAISuggest`                    |
| Rate limiting (chat 10/min·100/dia; suggest 20/min)          | ✅                                                  | `api/chat/route.ts`                                               |
| Auditoria de sessões/mensagens de chat                       | ✅                                                  | tabelas `ai_chat_sessions`, `ai_chat_messages`                    |
| Explicabilidade ("por que classifiquei assim")               | 🟡 tool existe; calibração com dados reais pendente | `explain_extraction`/`explain_classification`                     |

### 4. Conciliação — ✅ motor; 🟡 UX

Spec detalhado: [`02-conciliacao.md`](02-conciliacao.md)

| Item                                                     | Estado                                            | Evidência                                                |
| -------------------------------------------------------- | ------------------------------------------------- | -------------------------------------------------------- |
| Motor determinístico (duplicata/exato/fuzzy/recorrência) | ✅                                                | `workers/ingestion/src/reconciliation/reconciliation.ts` |
| API GET candidatos + PATCH decisão + progress            | ✅                                                | `api/reconciliation/[draftId]/route.ts`                  |
| Colunas `reconciliation_*` em draft_records              | ✅                                                | migração `20260403110000`                                |
| UI dedicada de revisão de conciliação                    | 🟡 sugestão inline existe; tela própria a definir | —                                                        |

### 5. Padrões documentais (memória operacional) — 🟡

Spec detalhado: [`03-padroes-documentais.md`](03-padroes-documentais.md)

| Item                                                      | Estado                                                         | Evidência                       |
| --------------------------------------------------------- | -------------------------------------------------------------- | ------------------------------- |
| Tabelas `document_patterns`, `pattern_feedback`           | ✅                                                             | migração `20260403100000`       |
| UI `/ingestion/patterns`                                  | 🟡 existe; CRUD/feedback completo a validar                    | `ingestion/patterns/`           |
| Aplicação de padrão no parsing (supplier-templates)       | 🟡 templates existem em código; ligação com tabela a confirmar | `parsers/supplier-templates.ts` |
| Feedback humano que corrige padrão sem contaminar futuros | ⬜ a especificar/validar                                       | —                               |

### 6. Domínio financeiro — ✅

Spec detalhado: [`05-dominio-financeiro.md`](05-dominio-financeiro.md)

| Item                                                                       | Estado                                                     | Evidência                                       |
| -------------------------------------------------------------------------- | ---------------------------------------------------------- | ----------------------------------------------- |
| Ciclo financeiro personalizado                                             | ✅                                                         | `packages/domain/src/financial-cycle/` + testes |
| Amortização SAC/Price/Misto + quitação antecipada                          | ✅                                                         | `packages/domain/src/amortization/` + testes    |
| Prioridade de pagamento (5 níveis)                                         | ✅                                                         | `packages/domain/src/priority/` + testes        |
| Deduplicação transação × item de fatura (ADR-001)                          | ✅                                                         | `packages/domain/src/deduplication/` + testes   |
| Telas: transactions, statements, liabilities, recurring, reports, settings | ✅                                                         | `apps/web/src/app/dashboard/*`                  |
| Import CSV                                                                 | ✅                                                         | `dashboard/import/page.tsx`                     |
| Matriz "tipo de documento → tipo de registro" explícita e testada          | 🟡 lógica existe em draft-generation; documentar como spec | `draft-generation`                              |

### 7. MCP server — ✅ 9 tools

| Item                                                                                                                                             | Estado                                     | Evidência                                 |
| ------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------ | ----------------------------------------- |
| 9 tools (scan_local, list_unparsed, reprocess, resolve_supplier, list/approve draft_batch, find_no_password, recompute_periods, ingest_document) | ✅ todas implementadas                     | `apps/mcp-server/src/index.ts` + `tools/` |
| Tools de Gmail via MCP (`scan_gmail_label/query/period`)                                                                                         | ⬜ scanner existe, **não exposto via MCP** | —                                         |
| Tools de conciliação / padrões via MCP                                                                                                           | ⬜                                         | —                                         |

### 8. Infra, deploy e ambientes — 🟡 / 🔒 o maior buraco real

| Item                                                                    | Estado                                                                      | Evidência                |
| ----------------------------------------------------------------------- | --------------------------------------------------------------------------- | ------------------------ |
| Migrations + Edge Functions deploy (staging/prod) no CI                 | ✅ stages existem                                                           | `.gitlab-ci.yml`         |
| **Deploy web (Vercel) no CI**                                           | ⬜ **ainda é placeholder** (`echo "configurar provedor"`)                   | `.gitlab-ci.yml:188,231` |
| Google OAuth em staging/produção                                        | 🔒 config sua                                                               | —                        |
| Promoção controlada local→staging→prod (dry-run, escopos, idempotência) | ⬜ **não existe nenhuma ferramenta**                                        | `scripts/`               |
| Secrets remotos organizados / rotação                                   | 🟡 guia existe (`_arquivo/passo-a-passo/008`), chaves expostas a rotacionar | ver memória              |
| Observabilidade mínima (sucesso/falha/pendente por doc)                 | 🟡 status visível na UI; logs estruturados a definir                        | —                        |

---

## Os 7 gaps reais que importam (não os 74 fantasmas dos docs antigos)

1. **Deploy web real + staging end-to-end** — hoje é placeholder no CI. Sem isso você não testa fora do localhost. (🔒 depende de credenciais Vercel suas)
2. **Validação com documentos reais seus** — o pipeline nunca foi exercitado em escala com seus boletos/faturas/extratos. (🔒 depende de você rodar)
3. **Split-view de revisão com edição inline** — a peça de UX que falta para revisão confortável.
4. **Ciclo de feedback de padrões documentais** — fechar para "parar de reensinar o sistema".
5. **Tools de Gmail/conciliação/padrões no MCP** — para uso real via Copilot/ChatGPT.
6. **Ferramenta de promoção entre ambientes** — não existe; necessária para mover dados local→staging→prod com segurança.
7. **Rotação das chaves Supabase expostas** — pendência de segurança aberta. (🔒 você)

## Como manter este mapa honesto

- Toda mudança de estado aqui precisa de **evidência** (caminho de arquivo + teste).
- Nada vira ✅ sem teste passando ou verificação manual registrada.
- Quando começarmos a executar, cada item fechado atualiza esta tabela **no mesmo commit**.
