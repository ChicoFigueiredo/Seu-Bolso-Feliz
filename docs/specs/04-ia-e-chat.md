# 04 — Spec: IA e Chat (interface inteligente do pipeline)

> **Estado: ✅ integrado de verdade** (chat + sugestões inline + enrichers no worker).
> Verificado contra código em 2026-06-20. Ver [`00-estado-real.md`](00-estado-real.md).

## Princípio de arquitetura (inegociável)

A IA **não é um fluxo paralelo** — é a interface inteligente do pipeline já existente.
Toda ação da IA passa pelos mesmos workers/Edge Functions/tabelas. **IA nunca grava no
ledger sem revisão humana** nas fases atuais. Determinístico primeiro, IA depois, humano decide.

## Componentes (contrato) — verificado

| Componente             | Onde                                                | Modelo        | Função                                                            |
| ---------------------- | --------------------------------------------------- | ------------- | ----------------------------------------------------------------- |
| Chat drawer            | `apps/web/src/components/ai-chat-drawer.tsx`        | —             | UI lateral, histórico, streaming, upload                          |
| Chat API               | `apps/web/src/app/api/chat/route.ts`                | gpt-4o        | assistente com tool use, rate limit 10/min·100/dia, log de sessão |
| Sugestões inline       | `apps/web/src/app/api/ai-suggest/route.ts`          | gpt-4o-mini   | single-shot, whitelist de tools, rate limit 20/min                |
| Enrich lite (pipeline) | `workers/ingestion/src/parsers/ai-lite-enricher.ts` | gpt-4o-mini   | preenche campos críticos faltantes                                |
| Enrich full (pipeline) | `workers/ingestion/src/parsers/ai-full-enricher.ts` | gpt-4o Vision | imagens/escaneados                                                |
| Auditoria              | tabelas `ai_chat_sessions`, `ai_chat_messages`      | —             | trilha de conversa (migração `20260401100000`)                    |

## Upload-pelo-chat (fluxo verificado)

`ai-chat-drawer` aceita `.pdf,.png,.jpg,.jpeg,.xlsx,.csv,.doc,.docx,.ofx,.qif` (≤10MB) →
grava em bucket `ingestion-originals` → chama Edge `trigger-ingestion` → manda mensagem ao chat.
**Mesmo pipeline do upload manual.** ✅

## Casos de uso do chat (do prompt original) — estado

| Caso                                                               | Estado                                                                                 |
| ------------------------------------------------------------------ | -------------------------------------------------------------------------------------- |
| Receber upload no chat e mandar pro pipeline                       | ✅                                                                                     |
| Consultar documentos ingeridos / pendências / com erro / sem senha | 🟡 via tools MCP no backend; expor todas no chat a confirmar                           |
| Sugerir fornecedor / tipo / campos / conciliação                   | ✅ (`/api/ai-suggest`)                                                                 |
| Explicar por que classificou de tal forma                          | 🟡 tool `explain_extraction`/`explain_classification` existe; calibrar com dados reais |
| Aprovar em lote com segurança                                      | 🟡 aprovação em lote existe na UI; via chat a confirmar                                |

## Sugestões inline (tools whitelisted em `/api/ai-suggest`)

`suggest_reconciliation`, `suggest_splits`, `suggest_supplier_name`, `explain_classification`,
`explain_extraction` (+ outras). Disparadas por `AIFieldBadge` / hook `useAISuggest` nas telas
de documento/transação. ✅

## Gaps e critérios de aceite

| Gap                                           | Critério de aceite                                                                        | Prioridade |
| --------------------------------------------- | ----------------------------------------------------------------------------------------- | ---------- |
| 🟡 Cobertura completa de casos de uso no chat | Lista acima toda ✅: cada caso responde no chat com dados reais do usuário                | Média      |
| 🟡 Explicabilidade calibrada                  | Para um documento real, a explicação cita campos extraídos, fonte (parser/IA) e confiança | Média      |
| ⬜ Teste "IA não grava sem revisão"           | Teste garante que nenhuma sugestão de IA muda o ledger sem ação humana explícita          | Alta       |
| ⬜ Teste "chamadas IA auditáveis"             | Toda chamada registra prompt/response/tokens/custo                                        | Média      |

## Restrições (do CEO)

1. Não depender exclusivamente de OpenAI — sempre parser determinístico + IA + revisão humana. ✅ (arquitetura atual respeita)
2. Sem autopost por IA nas fases iniciais. ✅
3. Sem esconder ingestão em ferramenta técnica sem UI. ✅ (tudo tem tela)

## Referências de código

- `apps/web/src/app/api/{chat,ai-suggest}/route.ts`
- `apps/web/src/components/ai-chat-drawer.tsx`
- `workers/ingestion/src/parsers/ai-{lite,full}-enricher.ts`
- `supabase/migrations/20260401100000_*.sql`
