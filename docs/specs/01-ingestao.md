# 01 — Spec: Ingestão de Documentos

> Capacidade central do produto. **Estado geral: ✅ construída, ✅ testada.**
> Verificado contra código em 2026-06-20. Ver mapa: [`00-estado-real.md`](00-estado-real.md).

## Objetivo (comportamento desejado)

Um documento financeiro (boleto, fatura, extrato, comprovante, conta de consumo) entra
por um de três canais — **upload manual/chat**, **Gmail** ou **pasta local** — e é levado
automaticamente, com idempotência e trilha de auditoria, de "arquivo bruto" até "rascunho
de registro financeiro pronto para revisão humana", usando parsers determinísticos primeiro
e IA só quando necessário.

## Máquina de estados (contrato)

```
DISCOVERED → DOWNLOADED → HASHED → QUEUED → PARSING → PARSED
  → AI_LITE_ENRICHING (condicional: shouldActivateAiLite — confiança<0.95 ou campo crítico faltando)
  → CLASSIFIED → RECONCILED → DRAFTED → PENDING_REVIEW → APPROVED → POSTED
```

- Implementação: `workers/ingestion/src/state-machine.ts` (transições válidas) + `processor.ts` (driver).
- Tipos: `@sbf/ingestion-types` (`IngestionJobStatus`, `ParserType` = `LOCAL_TEXT | LOCAL_REGEX | OPENAI_VISION | OPENAI_TEXT`).
- Persistência: `ingestion_runs`, `ingestion_jobs`, `source_documents`, `draft_batches`, `draft_records` (migração `20260323120200`).

## Sub-pipeline de parsing (ordem real)

1. `text-extractor` — PDF nativo → OCR (`ocrmypdf`) → fallback imagem; trata PDF com senha. ✅
2. `boleto-parser` / `cemig-parser` — parsers por tipo (regex). ✅ (boleto-parser 🟡 básico, apoia-se em boleto-utils)
3. `boleto-utils-extractor` — valida linha digitável via lib `boleto-utils`. ✅
4. `supplier-templates` — overrides de extração por fornecedor. ✅
5. `field-consensus` — resolve conflito entre fontes (prioridade boleto_utils > template > determinístico). ✅
6. `ai-lite-enricher` — gpt-4o-mini, preenche campos críticos faltantes. ✅
7. `ai-full-enricher` — gpt-4o Vision, para imagens/escaneados. ✅

Orquestração: `parsers/parse-orchestrator.ts` → grava `parsed_document_versions` + `extraction_results`.

## Canais de entrada

| Canal | Estado | Entrada |
| --- | --- | --- |
| Upload manual / chat | ✅ | UI `/dashboard/ingestion` + `ai-chat-drawer` → bucket `ingestion-originals` → Edge `trigger-ingestion` |
| Gmail | ✅ | `workers/gmail-scanner` — OAuth2 refresh-token, scan por label/query, dedup por (msgId+filename) e por hash |
| Pasta local | ✅ | `workers/local-scanner` — scan-once ou watch, filtro por extensão, dedup por (path+mtime) |
| Orquestrador | ✅ | `workers/financial-evidence-worker` — CLI unificada Gmail+local com `--dry-run` |

## Idempotência (invariante crítica)

- Dedup por **content hash** (`@sbf/operations`) e por **origin key**. Cobertura: `hash.test.ts`, `origin-key.test.ts`, `idempotency.test.ts`.
- Critério: rodar o mesmo scan/upload 2× **não pode** criar documento/job duplicado.

## Gaps e critérios de aceite

| Gap | Critério de aceite | Prioridade |
| --- | --- | --- |
| 🟡 Split-view de revisão com edição inline | Tela mostra arquivo original lado a lado com draft, permite editar cada campo antes de aprovar, com indicação de confiança/origem do valor | Alta |
| 🟡 `boleto-parser` genérico raso | Boleto não-CEMIG sem template extrai valor, vencimento, beneficiário, linha digitável com confiança ≥0.8 em N amostras reais | Média |
| ⬜ Validação com documentos reais do CEO | Rodar lote real (Gmail label + pasta) ponta a ponta; registrar taxa de sucesso/falha/pendência | **Alta (🔒 CEO)** |
| ⬜ Observabilidade estruturada | Para cada documento dá para responder em ≤1 clique: entrou? falhou (por quê)? pendente? aprovado? | Média |

## Referências de código

- `workers/ingestion/src/{state-machine,processor}.ts`, `workers/ingestion/src/parsers/*`
- `workers/{gmail-scanner,local-scanner,financial-evidence-worker}/src/*`
- `supabase/functions/trigger-ingestion/index.ts`
- Testes: `__tests__/domain/{state-machine,parsers,text-extractor-ocr,field-consensus,draft-generation}.test.ts`, `__tests__/integration/{ingestion-pipeline,parse-orchestrator.integration}.test.ts`
