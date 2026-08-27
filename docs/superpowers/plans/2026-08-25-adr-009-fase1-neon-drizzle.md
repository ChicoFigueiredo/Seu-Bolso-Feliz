# ADR-009 Fase 1 — Neon + Drizzle Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Provisionar Neon Postgres via Vercel Marketplace, portar o schema existente (sem RLS, sem dependências do Supabase Auth/Storage) e entregar um cliente Drizzle tipado e testável em `packages/db` — a fundação sobre a qual as Fases 2-6 da ADR-009 são construídas.

**Architecture:** Novo pacote `@sbf/db` no monorepo (mesmo padrão de `packages/domain`, `packages/contracts`). Schema Drizzle gerado por introspecção contra um Neon já populado com o DDL limpo (tabelas/colunas/índices/FKs/enums), não por transcrição manual de 42 migrations. Cliente lazy-init com `@neondatabase/serverless`, sem wrapper `Proxy` (quebra libs que inspecionam o client).

**Tech Stack:** `drizzle-orm`, `drizzle-kit`, `@neondatabase/serverless`, Bun workspaces, Vitest.

**Spec:** `docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md` (seção ADR-009).

## Global Constraints

- Nenhum secret de produção (`DATABASE_URL`) entra no repo — só em `.env`/`.env.local` (já gitignorados) e nas env vars da Vercel.
- RLS não é portado — ADR-009 decidiu autorização por `WHERE user_id = $1` explícito na aplicação. O schema Neon não deve ter nenhuma `CREATE POLICY` nem depender de `auth.uid()`.
- Nenhuma referência a schemas `auth.*` ou `storage.*` (exclusivos da plataforma Supabase) no DDL portado — essas tabelas/funções não existem no Neon puro.
- Todo pacote novo segue a convenção do monorepo: `package.json` com `name`, `private: true`, `main`/`types` apontando pra `src/index.ts`; `tsconfig.json` estendendo `../../tsconfig.base.json`.

---

### Task 1: Provisionar Neon via Vercel Marketplace

**Files:** nenhum arquivo de código — ação de infraestrutura via CLI.

**Interfaces:**

- Produz: variável de ambiente `DATABASE_URL` no ambiente `production` do projeto Vercel `pessoal-seu-bolso-feliz`, e localmente em `apps/web/.env.local`.

- [ ] **Passo 1: Provisionar a integração**

```bash
cd /mnt/d/Chico/seu.bolso.feliz
set -a; source .env; set +a
vercel integration add neon --token "$VERCEL_TOKEN" --yes
```

Se o comando pedir claim interativo (conta Neon nova, aceite de billing), **pare e peça ao usuário** pra completar no navegador — não existe forma de automatizar essa etapa via CLI.

- [ ] **Passo 2: Verificar que a integração foi criada**

```bash
vercel integration ls --token "$VERCEL_TOKEN"
```

Esperado: uma entrada Neon aparece (antes retornava "No resources found").

- [ ] **Passo 3: Confirmar a env var em produção**

```bash
vercel env ls production --token "$VERCEL_TOKEN"
```

Esperado: `DATABASE_URL` (ou nome equivalente que a integração Neon usa — confirmar o nome exato aqui, documentar se for diferente) aparece na lista, criada agora.

- [ ] **Passo 4: Puxar localmente**

```bash
vercel env pull apps/web/.env.local --environment=production --token "$VERCEL_TOKEN" --yes
grep -c "DATABASE_URL" apps/web/.env.local
```

Esperado: `1` (a variável está presente). **Não** imprimir o valor no terminal além de checagem de presença/tamanho.

---

### Task 2: Criar o pacote `@sbf/db`

**Files:**

- Create: `packages/db/package.json`
- Create: `packages/db/tsconfig.json`
- Create: `packages/db/src/index.ts`
- Modify: `vitest.config.ts` (adicionar alias `@sbf/db`, seguindo o padrão dos aliases existentes)

**Interfaces:**

- Produz: pacote `@sbf/db` resolvível via `workspace:*` pelos outros workspaces.

- [ ] **Passo 1: Criar `package.json`**

```json
{
  "name": "@sbf/db",
  "version": "0.0.1",
  "private": true,
  "main": "src/index.ts",
  "types": "src/index.ts",
  "scripts": {
    "db:introspect": "drizzle-kit introspect",
    "db:studio": "drizzle-kit studio"
  },
  "dependencies": {
    "@neondatabase/serverless": "^1.0.2",
    "drizzle-orm": "^0.44.6"
  },
  "devDependencies": {
    "drizzle-kit": "^0.31.6"
  }
}
```

(Confirmar as versões exatas mais recentes com `bun info drizzle-orm version` / `bun info drizzle-kit version` / `bun info @neondatabase/serverless version` antes de fixar — não assumir que os números acima ainda são os mais novos.)

- [ ] **Passo 2: Criar `tsconfig.json`**

```json
{
  "extends": "../../tsconfig.base.json",
  "compilerOptions": {
    "outDir": "dist",
    "rootDir": "src",
    "noEmit": false,
    "emitDeclarationOnly": true
  },
  "include": ["src/**/*.ts"],
  "exclude": ["node_modules", "dist", "src/**/*.test.ts"]
}
```

- [ ] **Passo 3: Criar `src/index.ts` (placeholder até a Task 4 gerar o schema real)**

```ts
export {};
```

- [ ] **Passo 4: Adicionar alias no `vitest.config.ts` raiz**

Adicionar à const `aliases` (mesmo padrão das linhas existentes):

```ts
"@sbf/db": resolve(__dirname, "packages/db/src/index.ts"),
```

- [ ] **Passo 5: Instalar e verificar**

```bash
export PATH="$HOME/.bun/bin:$PATH"
bun install
bun run typecheck
```

Esperado: ambos sem erro — o pacote novo resolve e não quebra o build dos outros workspaces.

- [ ] **Passo 6: Commit**

```bash
git add packages/db vitest.config.ts bun.lock
git commit -m "feat(db): cria esqueleto do pacote @sbf/db"
```

---

### Task 3: Portar o schema (sem RLS, sem Auth/Storage) pro Neon

**Files:**

- Create: `packages/db/scripts/export-clean-schema.sh` (gera o DDL limpo a partir do Supabase local)
- Create: `packages/db/schema.sql` (dump limpo, versionado — snapshot do DDL aplicado, não gerado em cada rodada)

**Interfaces:**

- Consome: `DATABASE_URL` (Task 1) e uma instância local do Supabase já com as 42 migrations aplicadas (`supabase start` + `supabase db reset` se necessário).
- Produz: schema DDL (tabelas, colunas, índices, FKs, enums, sequences) aplicado no banco Neon real, sem `CREATE POLICY`, sem `ENABLE ROW LEVEL SECURITY`, sem objetos nos schemas `auth`/`storage`/`extensions` exclusivos do Supabase.

- [ ] **Passo 1: Garantir que o Supabase local está com o schema atual**

```bash
npx supabase start
npx supabase db reset
```

Esperado: as 42 migrations em `supabase/migrations/` aplicam sem erro (é a mesma verificação que `bun run db:reset` já faz hoje).

- [ ] **Passo 2: Extrair um dump schema-only do Postgres local**

```bash
mkdir -p packages/db/scripts
pg_dump "postgresql://postgres:postgres@127.0.0.1:54322/postgres" \
  --schema-only --no-owner --no-privileges \
  --schema=public \
  > /tmp/claude-1000/-mnt-d-Chico-seu-bolso-feliz/0cc89f1f-997b-453d-9b3c-0d91210859d8/scratchpad/schema-raw.sql
```

(Porta/credenciais do Postgres local do Supabase — confirmar com `npx supabase status` se `54322` não bater.) Restringir a `--schema=public` já exclui `auth`/`storage`/`extensions` por completo — eles vivem em schemas separados.

- [ ] **Passo 2: Remover objetos que dependem de `auth.*`/RLS**

Abrir `/tmp/.../schema-raw.sql` e remover manualmente (ou com um script `awk`/`sed` que o executor deve escrever _inspecionando o arquivo real_ — o conteúdo exato varia e não deve ser adivinhado aqui):

- todo bloco `CREATE POLICY ...`
- todo `ALTER TABLE ... ENABLE ROW LEVEL SECURITY` / `ALTER TABLE ... FORCE ROW LEVEL SECURITY`
- qualquer `DEFAULT` ou `CHECK` que chame `auth.uid()` ou `auth.role()`
- `GRANT`/`REVOKE` pros roles `anon`/`authenticated`/`service_role` (não existem no Neon)

Verificar que não sobrou nenhuma referência: `grep -in "auth\.\|create policy\|row level security" /tmp/.../schema-raw.sql` deve retornar vazio depois da limpeza.

- [ ] **Passo 3: Salvar o resultado limpo versionado**

```bash
cp /tmp/.../schema-raw.sql packages/db/schema.sql
```

- [ ] **Passo 4: Aplicar no Neon**

```bash
set -a; source apps/web/.env.local; set +a
psql "$DATABASE_URL" -f packages/db/schema.sql
```

- [ ] **Passo 5: Verificar contagem de tabelas**

```bash
psql "$DATABASE_URL" -c "select count(*) from information_schema.tables where table_schema = 'public';"
```

Esperado: mesmo número de tabelas que `select count(*) from information_schema.tables where table_schema = 'public';` retorna no Postgres local (rodar a mesma query lá pra comparar — não assumir um número fixo, o levantamento da ADR contou ~41 mas confirmar contra a fonte real).

- [ ] **Passo 6: Commit**

```bash
git add packages/db/schema.sql
git commit -m "feat(db): porta schema limpo (sem RLS/Auth/Storage) pro Neon"
```

---

### Task 4: Gerar o schema Drizzle por introspecção

**Files:**

- Create: `packages/db/drizzle.config.ts`
- Create: `packages/db/src/schema.ts` (gerado por `drizzle-kit introspect`, depois revisado)
- Modify: `packages/db/src/index.ts`

**Interfaces:**

- Consome: Neon com schema aplicado (Task 3).
- Produz: `export * from "./schema"` — tipos Drizzle de cada tabela, consumidos pelas Fases 2+.

- [ ] **Passo 1: Criar `drizzle.config.ts`**

```ts
import { defineConfig } from "drizzle-kit";

export default defineConfig({
  dialect: "postgresql",
  schema: "./src/schema.ts",
  out: "./drizzle",
  dbCredentials: {
    url: process.env.DATABASE_URL!,
  },
});
```

- [ ] **Passo 2: Rodar introspecção**

```bash
cd packages/db
set -a; source ../../apps/web/.env.local; set +a
bunx drizzle-kit introspect
```

Esperado: gera `src/schema.ts` (ou caminho equivalente que o comando escolher — mover pro lugar certo se necessário) com uma tabela Drizzle por tabela do Postgres.

- [ ] **Passo 3: Revisar o schema gerado**

Abrir `src/schema.ts` e conferir: nomes de coluna batem com os usados no código atual (grep rápido por `.from("nome_da_tabela")` em `apps/web`/`workers` pra cruzar), tipos de enum reconhecidos corretamente (Postgres enums viram `pgEnum` no Drizzle).

- [ ] **Passo 4: Exportar do índice do pacote**

```ts
// packages/db/src/index.ts
export * from "./schema";
export * from "./client"; // criado na Task 5
```

- [ ] **Passo 5: Typecheck**

```bash
cd /mnt/d/Chico/seu.bolso.feliz
export PATH="$HOME/.bun/bin:$PATH"
bun run typecheck
```

Esperado: sem erro.

- [ ] **Passo 6: Commit**

```bash
git add packages/db/drizzle.config.ts packages/db/src/schema.ts packages/db/src/index.ts
git commit -m "feat(db): gera schema Drizzle por introspecção do Neon"
```

---

### Task 5: Cliente Neon com inicialização lazy

**Files:**

- Create: `packages/db/src/client.ts`
- Test: `packages/db/src/client.test.ts`

**Interfaces:**

- Consome: `DATABASE_URL` do ambiente, `schema` da Task 4.
- Produz: `getDb(): NeonHttpDatabase<typeof schema>` — função usada por toda Server Action/worker das Fases 4-5.

- [ ] **Passo 1: Escrever o teste (mock de env, sem bater no Neon de verdade)**

```ts
// packages/db/src/client.test.ts
import { describe, it, expect, beforeEach, afterEach } from "vitest";

describe("getDb", () => {
  const originalUrl = process.env.DATABASE_URL;

  beforeEach(() => {
    process.env.DATABASE_URL = "postgresql://user:pass@example.com/db";
  });

  afterEach(() => {
    process.env.DATABASE_URL = originalUrl;
  });

  it("lança erro claro se DATABASE_URL não está definida", async () => {
    delete process.env.DATABASE_URL;
    const { getDb } = await import("./client");
    expect(() => getDb()).toThrow(/DATABASE_URL/);
  });

  it("retorna a mesma instância em chamadas repetidas (lazy singleton)", async () => {
    const { getDb } = await import("./client");
    const first = getDb();
    const second = getDb();
    expect(first).toBe(second);
  });
});
```

- [ ] **Passo 2: Rodar o teste, confirmar que falha**

```bash
cd /mnt/d/Chico/seu.bolso.feliz
export PATH="$HOME/.bun/bin:$PATH"
bunx vitest run packages/db/src/client.test.ts
```

Esperado: FALHA — `./client` ainda não existe.

- [ ] **Passo 3: Implementar (lazy `let`, sem `Proxy` — ver aviso da skill vercel-storage)**

```ts
// packages/db/src/client.ts
import { neon } from "@neondatabase/serverless";
import { drizzle } from "drizzle-orm/neon-http";
import * as schema from "./schema";

let _db: ReturnType<typeof buildDb> | null = null;

function buildDb() {
  const url = process.env.DATABASE_URL;
  if (!url) {
    throw new Error("DATABASE_URL é obrigatório para @sbf/db");
  }
  const sql = neon(url);
  return drizzle(sql, { schema });
}

export function getDb() {
  if (!_db) _db = buildDb();
  return _db;
}
```

- [ ] **Passo 4: Rodar o teste, confirmar que passa**

```bash
bunx vitest run packages/db/src/client.test.ts
```

Esperado: PASS, 2 testes.

- [ ] **Passo 5: Commit**

```bash
git add packages/db/src/client.ts packages/db/src/client.test.ts
git commit -m "feat(db): cliente Neon lazy-init (@sbf/db)"
```

---

### Task 6: Teste de fumaça contra o Neon real

**Files:**

- Test: `__tests__/integration/db-neon-smoke.test.ts`

**Interfaces:**

- Consome: `getDb()` (Task 5), uma tabela real do schema portado (Task 3/4) — usar `institutions` ou outra tabela pequena/estável em vez de uma tabela com dado sensível.

- [ ] **Passo 1: Escrever o teste de integração**

```ts
// __tests__/integration/db-neon-smoke.test.ts
import { describe, it, expect } from "vitest";
import { getDb, institutions } from "@sbf/db";
import { sql } from "drizzle-orm";

describe("Neon smoke test", () => {
  it("conecta no Neon real e executa uma query simples", async () => {
    const db = getDb();
    const result = await db.execute(sql`select 1 as ok`);
    expect(result.rows[0]).toEqual({ ok: 1 });
  });

  it("a tabela institutions existe e é consultável", async () => {
    const db = getDb();
    const rows = await db.select().from(institutions).limit(1);
    expect(Array.isArray(rows)).toBe(true);
  });
});
```

(Se `institutions` não for o nome real da tabela no schema gerado pela Task 4, ajustar pro nome que a introspecção produziu.)

- [ ] **Passo 2: Rodar contra o Neon real**

```bash
cd /mnt/d/Chico/seu.bolso.feliz
export PATH="$HOME/.bun/bin:$PATH"
set -a; source apps/web/.env.local; set +a
bunx vitest run __tests__/integration/db-neon-smoke.test.ts
```

Esperado: PASS, 2 testes — prova real de que o Neon provisionado (Task 1) + schema portado (Task 3) + introspecção (Task 4) + cliente (Task 5) funcionam de ponta a ponta contra o banco de verdade, não um mock.

- [ ] **Passo 3: Commit**

```bash
git add __tests__/integration/db-neon-smoke.test.ts
git commit -m "test(db): fumaça de integração contra Neon real"
```

---

## Definição de pronto da Fase 1

- `vercel integration ls` mostra Neon provisionado.
- `packages/db` existe, resolve via workspace, typecheck limpo.
- Schema aplicado no Neon sem RLS/Auth/Storage, contagem de tabelas bate com o Postgres local.
- `getDb()` funciona, testado (unit + integração real).
- Todos os commits desta fase existem no branch `feat/pluggy-integration-plan`.

Isso desbloqueia a Fase 2 (Auth mínimo) e a Fase 3 (Storage), que podem rodar em paralelo entre si assim que a Fase 1 fechar — nenhuma das duas depende da outra, só dependem de `@sbf/db` existir.
