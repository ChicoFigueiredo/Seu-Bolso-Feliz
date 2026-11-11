# 03 — Spec: Padrões Documentais (memória operacional)

> **Estado: ✅ modelagem + versionamento + feedback prontos · 🟡 ligação com o parsing e validação.**
> Verificado contra código em 2026-06-20. Ver [`00-estado-real.md`](00-estado-real.md).

## Objetivo (comportamento desejado)

"Parar de reensinar o sistema." Quando o usuário corrige a extração de um tipo de documento
(fatura de cartão, conta de energia, boleto de um fornecedor), o sistema **registra um padrão
reutilizável**, versionado, ligado a fornecedor/instituição/tipo — e **um padrão ruim não pode
contaminar extrações futuras**.

## Modelagem (contrato) — hoje em `packages/db/schema.sql` (era a migração `20260403100000` do backend anterior)

**`document_patterns`** — verificado:

- `name`, `document_type`, `supplier_id?`, `institution_id?`
- `extraction_rules JSONB`, `field_mappings JSONB`, `sample_fingerprints TEXT[]`
- contadores de feedback: `feedback_count`, (+ positivos/negativos e taxa de sucesso)
- Isolamento por `user_id` — hoje não há mais RLS: cada query filtra `WHERE user_id = ownerId` explicitamente (não existe `auth.uid()` fora do Supabase)

**`pattern_feedback`** — verificado:

- `pattern_id`, `source_document_id?`, sinal de feedback humano (positivo/negativo)

**Proteção contra padrão ruim — JÁ EXISTE:** trigger de **auto-desativação quando feedback
negativo > 3 e taxa de sucesso < 50%**. Este é exatamente o requisito "padrão ruim não
contamina extrações futuras". ✅

## Operações disponíveis (server actions) — verificado

`apps/web/src/app/actions/patterns.ts`:
`listPatterns`, `getPattern`, `createPattern`, `updatePattern`,
**`createPatternVersion`** (versionamento ✅), `deactivatePattern`, `reactivatePattern`,
**`registerPatternFeedback`** (feedback humano ✅).

UI: `/dashboard/ingestion/patterns` (lista) + `[id]` (detalhe).

## Gaps e critérios de aceite

| Gap                                     | Critério de aceite                                                                                                                                                                                               | Prioridade |
| --------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------- |
| 🟡 Ligação padrão → parsing             | Confirmar/garantir que `parsers/supplier-templates.ts` consome `document_patterns` da tabela (não só templates hardcoded). Critério: criar padrão na UI muda a extração de um novo documento do mesmo fornecedor | **Alta**   |
| 🟡 Loop fechado de feedback na UI       | A partir de uma revisão de draft, "salvar como padrão" e "corrigir padrão" em ≤2 cliques; feedback negativo reflete nos contadores                                                                               | Média      |
| ⬜ Teste do trigger de auto-desativação | Teste de integração: 4 feedbacks negativos + sucesso <50% desativa o padrão automaticamente                                                                                                                      | Média      |
| ⬜ Tools MCP de padrões                 | `suggest_document_pattern`, `register_document_pattern`, `list_documents_by_pattern` para uso por agente                                                                                                         | Baixa      |

## Alinhamento histórico

A modelagem aqui é a **canônica**. ADR-006 (`docs/_arquivo/adrs/ADR-006-padroes-documentais-aprendizado.md`)
e versões antigas no histórico usam nomenclatura divergente (`supplier_match`, `content_match`,
`job_id`) — **ignorar**. Vale o que está na migração `20260403100000` e em `patterns.ts`.

## Referências de código

- `packages/db/schema.sql` (tabelas `document_patterns`, `pattern_feedback` — `supabase/migrations/20260403100000_*.sql` era do backend anterior e não existe mais)
- `apps/web/src/app/actions/patterns.ts`
- `apps/web/src/app/dashboard/ingestion/patterns/`
- `workers/ingestion/src/parsers/supplier-templates.ts`
