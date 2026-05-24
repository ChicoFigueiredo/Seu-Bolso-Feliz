# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Development (web + ingestion worker together)
bun run dev

# Run individually
bun run dev:web
bun run dev:worker

# Build
bun run build

# Tests
bun run test                  # all tests
bun run test:unit             # unit only (__tests__/domain/, packages/*/src/, workers/*/src/)
bun run test:integration      # integration only (__tests__/integration/, 30s timeout)

# Run a single test file
bunx vitest run __tests__/domain/boleto-utils-extractor.test.ts

# Type checking, lint, format
bun run typecheck
bun run lint
bun run lint:fix
bun run format

# Database
bun run db:migrate            # push migrations to Supabase
bun run db:reset

# Regenerate TypeScript types from Supabase schema
bun run generate-types
```

## Architecture

This is a **Bun monorepo** (workspaces) for a personal finance app backed by **Supabase** (Postgres + Storage + Edge Functions).

### Workspace layout

```
apps/
  web/           Next.js frontend (App Router, Server Actions, Tailwind + shadcn/ui)
  mobile/        React Native / Expo (in development)
  mcp-server/    MCP server exposing 9 tools for AI agents (stdio transport)

packages/
  shared-types/  TypeScript types auto-generated from the Supabase schema — do not edit by hand
  ingestion-types/ Enums for the ingestion state machine (IngestionJobStatus, ParserType, etc.)
  domain/        Pure business logic: financial-cycle, amortization, priority, deduplication
  validation/    Zod schemas for all entities
  operations/    Shared utilities: content hash, canonical fingerprint, idempotency check
  config/        Shared ESLint / TS / Vitest configs
  ui-tokens/     Visual design tokens

workers/
  ingestion/     Main document processing pipeline
  gmail-scanner/ Scans Gmail by label and creates ingestion jobs
  local-scanner/ Scans a local folder and creates ingestion jobs
```

### Package aliases (vitest + tsconfig)

| Alias                  | Resolves to                             |
| ---------------------- | --------------------------------------- |
| `@sbf/domain`          | `packages/domain/src/index.ts`          |
| `@sbf/validation`      | `packages/validation/src/index.ts`      |
| `@sbf/shared-types`    | `packages/shared-types/src/index.ts`    |
| `@sbf/operations`      | `packages/operations/src/index.ts`      |
| `@sbf/ingestion-types` | `packages/ingestion-types/src/index.ts` |

### Ingestion pipeline

The core of the system is a **state machine** in `workers/ingestion/src/state-machine.ts`. Each document travels through these statuses in order:

```
DISCOVERED → DOWNLOADED → HASHED → QUEUED → PARSING → PARSED
  → AI_LITE_ENRICHING (conditional, when critical fields are missing)
  → CLASSIFIED → RECONCILED → DRAFTED → PENDING_REVIEW → APPROVED → POSTED
```

`processor.ts` drives each job through the machine. `parse-orchestrator.ts` coordinates the parsing sub-pipeline:

1. **text-extractor** — PDF text extraction (with optional OCR via `ocrmypdf`)
2. **boleto-parser / cemig-parser** — document-type-specific rule-based parsers
3. **boleto-utils-extractor** — secondary boleto field extraction
4. **supplier-templates** — per-supplier extraction overrides
5. **field-consensus** — resolves conflicts when multiple sources produce different values
6. **ai-lite-enricher** — fast AI pass to fill missing critical fields (activated by `shouldActivateAiLite`)
7. **ai-full-enricher** — full AI extraction (OPENAI_VISION / OPENAI_TEXT)

Parser types are defined in `@sbf/ingestion-types`: `LOCAL_TEXT`, `LOCAL_REGEX`, `OPENAI_VISION`, `OPENAI_TEXT`.

### Web frontend

Next.js App Router. Database access goes through **Server Actions** in `apps/web/src/app/actions/` — there is no separate REST API layer for CRUD. The three API routes under `apps/web/src/app/api/` handle AI chat, AI suggestions, and reconciliation webhooks.

Dashboard sections: documents, ingestion, suppliers, transactions, statements, liabilities, recurring, products, institutions, reports, settings.

### MCP server

Runs as a stdio MCP server (`apps/mcp-server/`). Exposes 9 tools for AI agents: `scan_local_folder`, `list_unparsed_documents`, `reprocess_document`, `resolve_supplier_candidates`, `list_draft_batches`, `approve_draft_batch`, `find_documents_without_password`, `recompute_financial_periods`, `ingest_document`.

## Commit convention

Conventional commits are enforced via commitlint.

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `ci`, `perf`, `migration`

**Scopes:** `web`, `mobile`, `domain`, `types`, `validation`, `supabase`, `ci`, `docs`, `config`, `tokens`, `gmail-scanner`, `ingestion`, `mcp-server`, `local-scanner`

Example: `feat(ingestion): add field-consensus resolver`

## Environment

Copy `.env.example` to `.env` and fill in Supabase credentials. The ingestion worker additionally reads:

- `INGESTION_ENABLE_OCRMYPDF` — enable OCR (default `true` in dev)
- `OCRMYPDF_BIN` — path to ocrmypdf binary (default `ocrmypdf`)
- `OPENAI_API_KEY` — required for AI-based parsers
