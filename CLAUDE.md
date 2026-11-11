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
bun run db:migrate            # push migrations to Neon

# Regenerate TypeScript types from the database schema
bun run generate-types
```

## Architecture

This is a **Bun monorepo** (workspaces) for a personal finance app backed by **Neon** (Postgres via Drizzle ORM), with its own custom authentication (`apps/web/src/lib/auth/`) and object storage on **Vercel Blob** (via `@sbf/storage`).

### Workspace layout

```
apps/
  web/           Next.js frontend (App Router, Server Actions). Two visual systems coexist — see "Web frontend".
  mobile/        React Native / Expo (in development)
  mcp-server/    MCP server exposing 10 tools for AI agents (stdio transport)

packages/
  db/            Neon + Drizzle: schema, relations, job queue, secrets, owner resolution. Read packages/db/README.md — schema.sql is a BASELINE, migrations/ change it.
  storage/       Object storage over Vercel Blob (private blobs, short-lived signed URLs)
  design-system/ @sbf/design-system: 36 components + 7 themes. Used by the (nova-ia) routes.
  contracts/     Draft payload schemas/migrations and the accepted-upload rules (uploadAceito)
  financial-connectors/ Open Finance providers (Pluggy)
  shared-types/  Legacy generated types — STALE, see its header. Drizzle's $inferSelect is the real source.
  ingestion-types/ Enums for the ingestion state machine (IngestionJobStatus, ParserType, etc.)
  domain/        Pure business logic: financial-cycle, amortization, priority, deduplication, financial-intent (reconciliation logic lives in workers/ingestion/src/reconciliation/)
  validation/    Zod schemas for all entities
  operations/    Shared utilities: content hash, canonical fingerprint, idempotency check
  config/        Shared ESLint / TS / Vitest configs
  ui-tokens/     Visual design tokens

workers/
  ingestion/     Main document processing pipeline (state machine + parsers + reconciliation)
  gmail-scanner/ Scans Gmail by label/query and creates ingestion jobs (OAuth2 refresh-token)
  local-scanner/ Scans a local folder (scan-once or watch mode) and creates ingestion jobs
  pluggy-sync/   Pulls Pluggy transactions. `--backfill` does a resumable 365-day window.
  cli/           Combined entry point for the scanners plus processing
  financial-evidence-worker/ Autonomous worker that promotes documents into financial_obligations + evidences (spec 004)
```

Workers and the MCP server run without a session. They resolve whose data they
are writing through `resolveOwnerId()` in `@sbf/db` — the DB is the source of
truth, and `LOCAL_USER_ID` is only a validated override.

> **Runtime note:** `bun` lives at `~/.bun/bin/bun` and is not always on a non-interactive shell's PATH. Prefix commands with `export PATH="$HOME/.bun/bin:$PATH"` when a script can't find `bun`.

### Package aliases (vitest + tsconfig)

| Alias                  | Resolves to                             |
| ---------------------- | --------------------------------------- |
| `@sbf/domain`          | `packages/domain/src/index.ts`          |
| `@sbf/validation`      | `packages/validation/src/index.ts`      |
| `@sbf/shared-types`    | `packages/shared-types/src/index.ts`    |
| `@sbf/operations`      | `packages/operations/src/index.ts`      |
| `@sbf/ingestion-types` | `packages/ingestion-types/src/index.ts` |
| `@sbf/db`              | `packages/db/src/index.ts`              |
| `@sbf/storage`         | `packages/storage/src/index.ts`         |
| `@sbf/contracts`       | `packages/contracts/src/index.ts`       |
| `@sbf/design-system`   | `packages/design-system/src/index.ts`   |

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

Next.js App Router. Database access goes through **Server Actions** in `apps/web/src/app/actions/` — there is no separate REST API layer for CRUD. The API routes under `apps/web/src/app/api/` are AI/reconciliation only: `chat` (streaming GPT-4o assistant with tool use), `ai-suggest` (single-shot gpt-4o-mini suggestions), and `reconciliation/[draftId]` (+`/progress`).

**Two visual systems coexist, on purpose.** The current information
architecture lives in the route group `apps/web/src/app/dashboard/(nova-ia)/`
and is built on `@sbf/design-system`: `hoje`, `movimentacoes`, `contas`,
`dividas`, `recorrentes`, `centros` (+ `[id]`), `documentos`, `conexoes`,
`inteligencia`. The route group only exists to scope `<SbfTheme>` — it does not
appear in the URL. **New screens go here.**

The older screens under `dashboard/` (transactions, liabilities, recurring,
products, institutions, cards, statements, suppliers, reports, settings,
import, documents, ingestion with its `review`/`documents`/`patterns`/`logs`
sub-routes) are still shadcn and still reachable; several are the only place
some CRUD exists. `next.config.ts` redirects the ones that were replaced.

Each new screen follows the same shape: a `painel-*.ts` server action that
returns plain data, and a client component that renders it. The action composes
the existing CRUD actions rather than querying again. Pages carry
`export const dynamic = "force-dynamic"` — a stale balance shown as current is
worse than half a second of latency.

Dates: use `hojeIso`/`paraIso`/`somaDias` from `@/lib/datas`, never
`toISOString().slice(0,10)`. The server runs in UTC and the user is in UTC−3;
the naive version reports tomorrow from 9pm onward.

**AI is integrated, not future work.** The chat drawer (`components/ai-chat-drawer.tsx`) supports upload-via-chat, calling the same `uploadDocument()` server action as the manual upload — there is no separate path and no edge function. Inline AI suggestions (supplier/splits/reconciliation/explain) flow through `/api/ai-suggest`. Pipeline AI enrichment runs in the ingestion worker (`ai-lite-enricher` = gpt-4o-mini, `ai-full-enricher` = gpt-4o Vision).

### MCP server

Runs as a stdio MCP server (`apps/mcp-server/`). Exposes 10 tools for AI agents: `scan_local_folder`, `list_unparsed_documents`, `reprocess_document`, `resolve_supplier_candidates`, `list_draft_batches`, `approve_draft_batch`, `post_draft_batch`, `find_documents_without_password`, `recompute_financial_periods`, `ingest_document`.

## Documentation status (read before trusting docs/)

The `docs/` tree (~54 files: `refinos/`, `checklists/`, `planejamento/`, `passo-a-passo/`, `adrs/`, `veronica/`) was produced by an AI "team roleplay" process and **has drifted badly from the code**. Several checklists mark as "not started" features that are fully implemented (e.g. `004-consolidado-gaps-pendentes.md` claims the ingestion UI and OpenAI integration are 0% done — both exist and work). **Treat docs as historical intent, not ground truth. Always verify a claim against the code before acting on it.** ADRs (`docs/adrs/`) are the most reliable; the newest file in any series is canonical (older versions in history use divergent modeling).

## Commit convention

Conventional commits are enforced via commitlint (husky `commit-msg` hook). Subject must be lower-case, no sentence-case; type must be one of the allowed list. Do not bypass hooks.

**Types:** `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `ci`, `perf`, `migration`

**Scopes:** `web`, `mobile`, `domain`, `types`, `validation`, `ci`, `docs`, `config`, `tokens`, `gmail-scanner`, `ingestion`, `mcp-server`, `local-scanner`, `db`

Example: `feat(ingestion): add field-consensus resolver`

## Branches (non-negotiable)

**Never commit to or force-changes onto `main`, `hmp`, or `develop`.** Work happens on feature branches (`feat/*`). Push is the user's responsibility unless they explicitly ask. The repo has multiple remotes (`origin`, `github`).

## Environment

Copy `.env.example` to `.env` and fill in the credentials (Neon, storage, auth, AI provider). The ingestion worker additionally reads:

- `INGESTION_ENABLE_OCRMYPDF` — enable OCR (default `true` in dev)
- `OCRMYPDF_BIN` — path to ocrmypdf binary (default `ocrmypdf`)
- `OPENAI_API_KEY` — required for AI-based parsers
