# 004 — Checklist Consolidado: O que Falta (pós-refino 2026-03-25)

> Visão agrupada de TODOS os itens pendentes do projeto, extraída dos checklists 002 e 003.
> Referência: [Ata de refino 2026-03-25](../refinos/2026-03/2026-03-25-21-07-refino-estado-atual-e-proximos-passos.md)

---

## Legenda

- ⬜ Não iniciado
- 🔲 Bloqueado (dependência pendente)
- 🟡 Em progresso
- ✅ Concluído

---

## Grupo A — Validação Imediata (esta semana)

> Itens que podem ser executados AGORA sem nenhum desenvolvimento novo.

| #   | Tarefa                                                                                                   | Origem | Status | Responsável |
| --- | -------------------------------------------------------------------------------------------------------- | ------ | ------ | ----------- |
| A.1 | Testar Gmail scan REAL (`--limit 10`, sem `--dry-run`)                                                   | Refino | ⬜     | CEO         |
| A.2 | Rodar worker ingestion com jobs reais: `bun run workers/ingestion/src/index.ts`                          | Refino | ⬜     | CEO         |
| A.3 | Validar fluxo completo via MCP: `list_unparsed_documents` → `list_draft_batches` → `approve_draft_batch` | Refino | ⬜     | CEO + João  |
| A.4 | Push commits pendentes (gmail-scanner + fixes) para GitLab                                               | Refino | ⬜     | CEO         |
| A.5 | Escanear pasta local com documentos reais do CEO                                                         | Refino | ⬜     | CEO         |

---

## Grupo B — Código Pendente (workers e MCP)

> Itens de implementação que ainda precisam de código novo.

### B1 — MCP Tools Pendentes

| #   | Tarefa                                                                | Origem (checklist 003) | Status | Responsável |
| --- | --------------------------------------------------------------------- | ---------------------- | ------ | ----------- |
| B.1 | Implementar tool MCP `scan_gmail_label` (integrar `scanGmailLabel()`) | 5.2                    | ⬜     | João        |
| B.2 | Implementar tool MCP `scan_gmail_period`                              | 5.3                    | 🔲     | João        |
| B.3 | Documentar uso do MCP com exemplos reais                              | 5.14                   | ⬜     | João        |
| B.4 | Teste: idempotência das tools MCP (tool 2x sem duplicar)              | 5.15                   | ⬜     | Maria       |

### B2 — Gmail Scanner Pendente

| #   | Tarefa                                                     | Origem (checklist 003) | Status | Responsável |
| --- | ---------------------------------------------------------- | ---------------------- | ------ | ----------- |
| B.5 | Implementar scan por período (query `after:` `before:`)    | 3.12                   | 🔲     | João        |
| B.6 | Implementar scan por query livre                           | 3.13                   | 🔲     | João        |
| B.7 | Sugestão de período financeiro integrada com `@sbf/domain` | 4.14                   | 🟡     | Pedro       |

### B3 — Schemas de Validação

| #   | Tarefa                                               | Origem (checklist 003) | Status | Responsável |
| --- | ---------------------------------------------------- | ---------------------- | ------ | ----------- |
| B.8 | Definir schemas Zod de ingestão em `@sbf/validation` | 2.16                   | ⬜     | Maria       |

---

## Grupo C — Testes Pendentes

> Testes que ainda não foram implementados.

| #   | Tarefa                                                           | Origem (checklist 003) | Status | Responsável   | Depende de         |
| --- | ---------------------------------------------------------------- | ---------------------- | ------ | ------------- | ------------------ |
| C.1 | Teste: dry-run não grava nada (integração com mock Supabase)     | 3.24                   | 🔲     | Maria         | —                  |
| C.2 | Teste: backfill por período funciona                             | 3.25                   | 🔲     | Maria         | B.5                |
| C.3 | Teste: PDF com senha é processado com segredo (e2e com PDF real) | 4.15                   | 🟡     | Maria         | PDF protegido real |
| C.4 | Teste: integração real MCP no Copilot                            | 5.13                   | 🟡     | Thiago + João | —                  |

---

## Grupo D — DevOps e Deploy

> Itens de CI/CD, ambientes e deploy real.

| #   | Tarefa                                                          | Origem (checklist) | Status | Responsável    |
| --- | --------------------------------------------------------------- | ------------------ | ------ | -------------- |
| D.1 | Substituir placeholder `deploy-web` por Vercel CLI no GitLab CI | 003 / 1.22         | ⬜     | Fernando       |
| D.2 | Validar pipeline completo em branch develop                     | 003 / 1.23         | 🔲     | Fernando       |
| D.3 | Validar deploy staging end-to-end                               | 003 / 1.24         | 🔲     | Fernando       |
| D.4 | Documentar política de promoção staging → production            | 003 / 1.25         | 🔲     | Fernando       |
| D.5 | Testar Google Auth staging end-to-end                           | 003 / 3.AA.10      | ⬜     | Fernando + CEO |

---

## Grupo E — UI de Ingestão (Fase 6)

> Interface web de revisão de documentos e drafts. Bloco completo não iniciado.

| #    | Tarefa                                                                 | Origem (checklist 003) | Status | Responsável      |
| ---- | ---------------------------------------------------------------------- | ---------------------- | ------ | ---------------- |
| E.1  | Rota `/dashboard/ingestion` (visão geral)                              | 6.1                    | ⬜     | Roberto          |
| E.2  | Rota `/dashboard/ingestion/runs` (histórico de scans)                  | 6.2                    | ⬜     | Roberto          |
| E.3  | Rota `/dashboard/ingestion/documents` (documentos processados)         | 6.3                    | ⬜     | Roberto          |
| E.4  | Rota `/dashboard/ingestion/drafts` (rascunhos em revisão)              | 6.4                    | ⬜     | Roberto          |
| E.5  | Rota `/dashboard/ingestion/drafts/[id]` (split-view: original × draft) | 6.5                    | ⬜     | Roberto + Helena |
| E.6  | Rota `/dashboard/ingestion/conflicts`                                  | 6.6                    | ⬜     | Roberto          |
| E.7  | Rota `/dashboard/ingestion/batches`                                    | 6.7                    | ⬜     | Roberto          |
| E.8  | Server actions para ingestion (approve, reject, reprocess)             | 6.8                    | ⬜     | Roberto          |
| E.9  | PDF viewer inline                                                      | 6.9                    | ⬜     | Sofia            |
| E.10 | Formulário de revisão de draft                                         | 6.10                   | ⬜     | Sofia            |
| E.11 | Aprovação em lote                                                      | 6.11                   | ⬜     | Sofia            |
| E.12 | Atalhos de teclado                                                     | 6.12                   | ⬜     | Thiago           |
| E.13 | Filtros (fornecedor, categoria, tag, período, status)                  | 6.13                   | ⬜     | Roberto          |
| E.14 | Validar acessibilidade (teclado, screen reader, contraste)             | 6.14                   | ⬜     | Renata           |
| E.15 | Conectar dados aprovados às telas financeiras existentes               | 6.15                   | ⬜     | Roberto          |

---

## Grupo F — Agentes OpenAI (Fase 7)

> Integração com IA para classificação, parsing e reconciliação. Bloqueado até API key.

| #    | Tarefa                                                               | Origem (checklist 003) | Status | Responsável     | Pré-requisito          |
| ---- | -------------------------------------------------------------------- | ---------------------- | ------ | --------------- | ---------------------- |
| F.1  | Configurar client OpenAI em `packages/operations/src/ai/`            | 7.1                    | 🔲     | Pedro           | API key                |
| F.2  | Implementar agente: Document Parser (LLM para PDFs genéricos)        | 7.2                    | 🔲     | Pedro           | F.1                    |
| F.3  | Implementar agente: Supplier Resolver (NLU para fornecedores)        | 7.3                    | 🔲     | Pedro           | F.1                    |
| F.4  | Implementar agente: Financial Classifier (categoria/tags/prioridade) | 7.4                    | 🔲     | Pedro           | F.1                    |
| F.5  | Implementar agente: Reconciler (cruzar lançamentos × extratos)       | 7.5                    | 🔲     | Pedro           | F.1 + dados reais      |
| F.6  | Logging de chamadas IA (auditoria: prompt, response, tokens, custo)  | 7.6                    | 🔲     | Pedro           | F.1                    |
| F.7  | Integrar agentes ao pipeline de parsing (upgrade do orchestrator)    | 7.7                    | 🔲     | Pedro + João    | F.2-F.4                |
| F.8  | Criar system prompts calibrados com dados reais                      | 7.8                    | 🔲     | Pedro + Ricardo | Dados reais no sistema |
| F.9  | Interface de linguagem natural no web (chat financeiro)              | 7.9                    | 🔲     | Roberto + Pedro | F.2-F.4                |
| F.10 | Teste: IA não grava no ledger sem revisão                            | 7.10                   | 🔲     | Maria           | F.7                    |
| F.11 | Teste: chamadas IA auditáveis                                        | 7.11                   | 🔲     | Maria           | F.6                    |

---

## Grupo G — Preparação para ChatGPT (Fase 8)

> Avaliação e documentação para exposição remota do MCP.

| #   | Tarefa                                                  | Origem (checklist 003) | Status | Responsável |
| --- | ------------------------------------------------------- | ---------------------- | ------ | ----------- |
| G.1 | Documentar avaliação de exposição remota do MCP         | 8.1                    | ⬜     | Ana         |
| G.2 | Classificação de tools (read/write-safe/risky)          | 8.2                    | ⬜     | Ana + Maria |
| G.3 | Documentar requisitos de autenticação remota            | 8.3                    | ⬜     | Maria       |
| G.4 | Avaliar Gmail push notifications (documento de decisão) | 8.4                    | ⬜     | João        |
| G.5 | Avaliar hospedagem de worker fora do local              | 8.5                    | ⬜     | Fernando    |

---

## Grupo H — Pendências do Checklist 002 (Verônica)

> Itens remanescentes do MVP original.

| #   | Tarefa                                                         | Origem (checklist 002) | Status | Responsável |
| --- | -------------------------------------------------------------- | ---------------------- | ------ | ----------- |
| H.1 | Auditoria histórica visível por fornecedor na interface        | E2.3                   | ⬜     | Roberto     |
| H.2 | Deploy web real automatizado no CI (sem placeholder)           | D2.3                   | ⬜     | Fernando    |
| H.3 | Comprovação em ambiente remoto da política completa (GitLab)   | D2.4                   | ⬜     | Fernando    |
| H.4 | Fluxo develop → feature → MR → pipeline → staging validado e2e | G.1                    | ⬜     | Fernando    |
| H.5 | Promoção controlada para main/produção com gates ativos        | G.2                    | ⬜     | Fernando    |
| H.6 | Checklist de operação contínua assinado pelo time              | G.3                    | ⬜     | Fernando    |

---

## Grupo I — Segurança e Hardening (pendências H2/H3 do checklist 002)

### I1 — Importantes (antes de produção)

| #   | Tarefa                                                        | Origem (checklist 002) | Status | Responsável    |
| --- | ------------------------------------------------------------- | ---------------------- | ------ | -------------- |
| I.1 | Google OAuth configurado no `config.toml` para staging/prod   | H2.5                   | 🟡     | Fernando + CEO |
| I.2 | mv_supplier_spending criada                                   | H2.7                   | ✅     | André          |
| I.3 | Confirm atômico retroactive-supplier-association              | H2.8                   | ✅     | Maria          |
| I.4 | refresh-mv-supplier-spending Edge Function                    | H2.10                  | ✅     | Maria          |
| I.5 | seed.sql populado                                             | H2.11                  | ✅     | André          |
| I.6 | Verificação manual no Studio (tabelas, RLS, buckets visíveis) | H4.3                   | ⬜     | CEO            |
| I.7 | Edge Runtime documentado (supabase functions serve)           | H4.1                   | ⬜     | Fernando       |

### I2 — Opcionais (qualidade)

| #    | Tarefa                                         | Origem (checklist 002) | Status | Responsável |
| ---- | ---------------------------------------------- | ---------------------- | ------ | ----------- |
| I.8  | MFA TOTP habilitado                            | H3.1                   | ⬜     | Fernando    |
| I.9  | Session timebox (24h + inactivity 8h)          | H3.2                   | ⬜     | Fernando    |
| I.10 | Vault habilitado para segredos                 | H3.3                   | ⬜     | Fernando    |
| I.11 | SSL enforcement em produção                    | H3.4                   | ⬜     | Fernando    |
| I.12 | PgBouncer em produção                          | H3.5                   | ⬜     | Fernando    |
| I.13 | Email templates customizados                   | H3.6                   | ⬜     | Roberto     |
| I.14 | hCaptcha/Turnstile para signup                 | H3.7                   | ⬜     | Fernando    |
| I.15 | `OPENAI_API_KEY` documentado no `.env.example` | H3.8                   | ⬜     | Pedro       |

---

## Resumo Quantitativo

| Grupo                    | Total  | Concluído | Pendente | Bloqueado |
| ------------------------ | ------ | --------- | -------- | --------- |
| A — Validação imediata   | 5      | 0         | 5        | 0         |
| B — Código (workers/MCP) | 8      | 0         | 5        | 3         |
| C — Testes pendentes     | 4      | 0         | 2        | 2         |
| D — DevOps/deploy        | 5      | 0         | 2        | 3         |
| E — UI ingestão          | 15     | 0         | 15       | 0         |
| F — Agentes OpenAI       | 11     | 0         | 0        | 11        |
| G — Prep ChatGPT         | 5      | 0         | 5        | 0         |
| H — Pendências Verônica  | 6      | 0         | 6        | 0         |
| I — Segurança            | 15     | 4         | 8        | 3         |
| **TOTAL**                | **74** | **4**     | **48**   | **22**    |

---

## Prioridade Recomendada (sequência de execução)

```
A (validação imediata)     ← AGORA
  ↓
B.1 (tool MCP scan_gmail)  ← esta semana
  ↓
D.1 (deploy real CI/CD)    ← esta semana
  ↓
D.5 (Google Auth staging)  ← esta semana
  ↓
C (testes pendentes)        ← próxima semana
  ↓
E (UI ingestão mínima)     ← abril
  ↓
H (pendências Verônica)    ← abril
  ↓
F (Agentes OpenAI)         ← maio
  ↓
G (prep ChatGPT)           ← junho
  ↓
I (hardening produção)     ← contínuo
```

---

_Gerado em: 2026-03-25 — Referência: Ata de refino 2026-03-25-21-07_
