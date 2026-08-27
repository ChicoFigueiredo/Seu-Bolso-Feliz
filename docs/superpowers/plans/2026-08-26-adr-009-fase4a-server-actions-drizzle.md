# ADR-009 Fase 4a — Server Actions e componentes client-side → Drizzle/Neon Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Reescrever as 17 Server Actions + 3 componentes client-side de `apps/web` para usar `@sbf/db` (Drizzle/Neon) com filtro `WHERE user_id` explícito, em vez de `supabase-js` dependendo de RLS implícito.

**Architecture:** Cada Server Action mantém `supabase.auth.getUser()` (via `createClient()` de `@/lib/supabase/server`) como única fonte de identidade — só o que vem depois muda de `supabase.from(...)` para `getDb().select()/.insert()/.update()/.delete()` do Drizzle, com `eq(<tabela>.user_id, userId)` explícito em toda operação. Os 3 componentes client-side ganham Server Actions finas novas no lugar de chamadas diretas ao Supabase pelo browser.

**Tech Stack:** Drizzle ORM (`drizzle-orm`), `@sbf/db` (`packages/db`), Neon Postgres, Vitest.

**Spec:** `docs/superpowers/specs/2026-08-26-adr-009-fase4a-server-actions-drizzle-design.md`

## Global Constraints

- `supabase.auth.getUser()` não muda nesta fase — é a única fonte de identidade (spec, seção "Fora").
- Toda leitura/atualização/exclusão ganha `eq(<tabela>.user_id, userId)` explícito no `where` — inclusive onde o código Supabase de hoje não filtra nada (RLS fazia isso escondido).
- Update/delete que afetam 0 linhas **lançam erro**, replicando o comportamento atual de `.select().single()` falhando — **exceto** onde o código Supabase de hoje não usa `.select().single()` (não checa linhas afetadas): nesse caso, preservar o comportamento atual (não lançar erro), documentado inline em cada task onde isso se aplica.
- Busca por id que não encontra devolve `null` (não lança erro) quando o código atual já faz isso (padrão `PGRST116 → null`).
- Tipos vêm de `typeof schema.<tabela>.$inferSelect`/`$inferInsert` (`packages/db/src/schema.ts`) — nunca de `@sbf/shared-types` nos arquivos reescritos.
- Nenhuma transação nova é introduzida "de brinde" — escritas multi-tabela sequenciais sem transação hoje continuam sequenciais sem transação (paridade de comportamento). Candidatos a corrigir isso ficam no Apêndice B da spec, não decididos aqui.
- Contrato de cada Server Action (assinatura, tipo de retorno, quando lança erro vs. devolve `null`) é idêntico ao de hoje — nenhuma mudança de shape para os consumidores.
- Testes de integração desta fase leem `TEST_DATABASE_URL` (nunca `DATABASE_URL`) e são pulados (`describe.skipIf`) se essa variável não existir. Proibido fallback para `DATABASE_URL`.
- Cada teste limpa o que criou (transação com rollback, ou `afterEach` explícito) — nenhuma linha órfã pode contaminar o teste seguinte.

---

### Task 1: Corrigir casing e tipo numérico de `packages/db/src/schema.ts`

**Files:**

- Modify: `packages/db/drizzle.config.ts`
- Modify: `packages/db/src/schema.ts` (regenerado + edições manuais reaplicadas)
- Modify: `packages/db/src/relations.ts` (regenerado)
- Test: `packages/db/src/schema.test.ts` (novo — teste de forma/contrato)

**Interfaces:**

- Produz: `schema.<tabela>.<coluna_snake_case>` para toda tabela/coluna (ex.:
  `schema.institutions.user_id`, `schema.institutions.display_order`) — é o que
  todas as tasks seguintes consomem. Colunas `numeric` devolvem `number` (não
  `string`). Timestamps continuam `mode: "string"` (inalterado).

- [ ] **Step 1: Editar `packages/db/drizzle.config.ts`**

Adicionar a opção `introspect.casing`:

```ts
import { defineConfig } from "drizzle-kit";

export default defineConfig({
  dialect: "postgresql",
  schema: "./src/schema.ts",
  out: "./drizzle",
  introspect: {
    casing: "preserve",
  },
  dbCredentials: {
    url: process.env.DATABASE_URL!,
  },
});
```

(mantém o resto do arquivo — comentários existentes sobre o processo de mover a
saída — intocado.)

- [ ] **Step 2: Rodar a introspecção contra o Neon real**

Run (dentro de `packages/db`, com `DATABASE_URL` do Neon no ambiente):
`bun run db:introspect`

Expected: gera/atualiza `packages/db/drizzle/schema.ts`,
`packages/db/drizzle/relations.ts`, `packages/db/drizzle/0000_*.sql`,
`packages/db/drizzle/meta/_journal.json`.

- [ ] **Step 3: Conferir os nomes exportados das tabelas**

Run: `grep -n "^export const" packages/db/drizzle/schema.ts`

Expected: uma linha por tabela (ex.: `export const institutions = pgTable(...)`,
`export const financialPeriods = pgTable(...)`). **Anotar exatamente esses nomes**
— as tasks seguintes deste plano assumem que os identificadores de tabela
continuam camelCase (`schema.financial_periods`, `schema.recurring_templates`,
`schema.statement_cycles`, `schema.provider_connections`,
`schema.external_account_mappings`, `schema.user_financial_preferences`,
`schema.recurring_instances`, `schema.recurring_template_tags`,
`schema.statement_items`, e demais tabelas por nome análogo) e que só as
**propriedades de coluna dentro de cada tabela** viram snake_case. Se a saída
real divergir disso, corrigir os nomes usados nas tasks seguintes antes de
prosseguir — isso é o único ponto de incerteza real deste plano.

- [ ] **Step 4: Mover a saída pra `src/`**

```bash
cp packages/db/drizzle/schema.ts packages/db/src/schema.ts
cp packages/db/drizzle/relations.ts packages/db/src/relations.ts
```

- [ ] **Step 5: Reaplicar as 2 edições manuais documentadas no cabeçalho antigo de `schema.ts`**

Antes de sobrescrever, o `schema.ts` antigo tinha um comentário de cabeçalho
listando 2 edições manuais (Fase 1): quebra de ciclo `AnyPgColumn` em
`transactions`/`recurringInstances`, e `table`→`_table` em `user_secrets`.
Reaplicar as duas no `schema.ts` novo, com os nomes de propriedade agora em
snake_case onde relevante. Manter o comentário de cabeçalho documentando as
edições (atualizando qualquer nome de propriedade citado nele).

- [ ] **Step 6: Expor `schema` como namespace em `packages/db/src/index.ts`**

**Achado na revisão** (verificado lendo o arquivo real): `index.ts` hoje faz
`export * from "./schema"` (achatado — cada tabela vira um export top-level de
`@sbf/db`), não um namespace `schema`. Todo o padrão desta spec/plano
(`import { getDb, schema } from "@sbf/db"`, `schema.institutions.user_id`)
depende de `schema` existir. Corrigir sem quebrar o único consumidor atual
(`__tests__/integration/db-neon-smoke.test.ts`, que importa `institutions` de
forma achatada):

```ts
// packages/db/src/index.ts
export * from "./schema";
export * as schema from "./schema";
export * from "./relations";
export * from "./client"; // criado na Task 5
```

(mantém a linha achatada existente — não remover, o teste de fumaça da Fase 1
depende dela — e adiciona a linha de namespace logo abaixo.)

- [ ] **Step 7: Adicionar `mode: "number"` em toda coluna `numeric(...)`**

Run: `grep -c "numeric(" packages/db/src/schema.ts` — anotar o número N.

Editar cada ocorrência de `numeric(...)` (com ou sem nome de coluna/precisão como
primeiro/segundo argumento) para incluir `mode: "number"` no objeto de opções.
Exemplo (`credit_limit`, hoje sem mode):

```diff
- creditLimit: numeric("credit_limit", { precision: 15, scale: 2 }),
+ credit_limit: numeric("credit_limit", { precision: 15, scale: 2, mode: "number" }),
```

(o nome da propriedade à esquerda também já deve estar em snake_case pela
introspecção do Step 2 — só confirmar que bate.)

Run: `grep -c "mode: \"number\"" packages/db/src/schema.ts` — deve ser igual a N
do passo anterior (toda ocorrência de `numeric(` ganhou `mode: "number"`, nenhuma
ficou de fora). **Não** adicionar `mode` em colunas `integer`, `bigint` ou
`timestamp` — só `numeric`.

- [ ] **Step 8: Escrever o teste de forma/contrato**

```ts
// packages/db/src/schema.test.ts
import { describe, expect, it } from "vitest";
import { schema } from "./index";

describe.skipIf(!process.env.TEST_DATABASE_URL)("schema.ts — contrato de forma", () => {
  it("institutions usa colunas snake_case", () => {
    expect(schema.institutions.user_id).toBeDefined();
    expect(schema.institutions.display_order).toBeDefined();
  });

  it("colunas numeric declaram mode: number", () => {
    const numericColumnNames = ["confidence_threshold", "credit_limit", "unit_price", "subtotal"];
    for (const name of numericColumnNames) {
      // Busca a coluna em qualquer tabela do schema que a tenha.
      const found = Object.values(schema).some(
        (table: any) => table?.[name]?.columnType === "PgNumeric" && table[name].mode === "number",
      );
      expect(found).toBe(true);
    }
  });
});
```

(Ajustar a forma exata de inspecionar `mode`/`columnType` conforme a API interna
do Drizzle exposta em tempo de execução — o objetivo do teste é falhar caso
`mode: "number"` seja removido ou uma coluna numeric nova seja adicionada sem
ele.)

- [ ] **Step 9: Rodar typecheck**

Run: `bun run typecheck`

Expected: sem erros novos. `packages/db/src/client.test.ts` e
`__tests__/integration/db-neon-smoke.test.ts` (que já consomem `@sbf/db`) não
deveriam quebrar — `db-neon-smoke.test.ts` importa `institutions` de forma
achatada, que continua existindo (Step 6 preservou o `export *` antigo). Se
algum quebrar, corrigir o nome da propriedade ali.

- [ ] **Step 10: Criar `apps/web/src/app/actions/test-db-setup.ts` (achado CRÍTICO da revisão da Verônica-Bolso)**

**Achado:** `getDb()` (`packages/db/src/client.ts:8`) lê `process.env.DATABASE_URL`
diretamente — **nunca** `TEST_DATABASE_URL`. Todos os testes de integração
deste plano chamam as Server Actions, que chamam `getDb()` internamente. Sem
uma correção, `TEST_DATABASE_URL=<branch-de-teste> bunx vitest run ...` (como
escrito nos "Run" de cada task) **não redireciona nada** — os testes escrevem
(inclusive os dois `user_id` de teste do teste de isolamento) direto no
`DATABASE_URL` do ambiente, que pode ser o Neon de **produção**. É exatamente
o acidente que a condição 1 da spec proíbe.

**Correção (2ª rodada de revisão da Verônica-Bolso — achado real):** um
`throw` direto neste arquivo aconteceria em tempo de **import** (carga do
módulo), antes de `describe.skipIf` conseguir pular a suíte — o Vitest
executa todo o código de topo de nível de um arquivo de teste durante a
coleta, independente do `skipIf`. Isso quebraria `bun run test` (e o CI, e
os 319 testes já existentes) em qualquer ambiente sem `TEST_DATABASE_URL`,
contradizendo a própria regra de "pular, não falhar" da spec. Correção que
preserva o skip E o bloqueio de produção: sem `TEST_DATABASE_URL`, **apaga**
`DATABASE_URL` do ambiente (em vez de lançar) — o `skipIf` pula o corpo dos
testes normalmente, e se algum teste _escapasse_ do skip por engano, `getDb()`
(`packages/db/src/client.ts:8`) já lança sozinho ("DATABASE_URL é obrigatório
para @sbf/db") em vez de escrever em produção.

Cada arquivo de teste desta fase precisa importar este arquivo **como a
primeira linha**, antes de qualquer outro import:

```ts
// apps/web/src/app/actions/test-db-setup.ts
if (process.env.TEST_DATABASE_URL) {
  process.env.DATABASE_URL = process.env.TEST_DATABASE_URL;
} else {
  // Sem banco de teste: describe.skipIf pula as suítes; apagar DATABASE_URL
  // garante que nenhum teste que escape do skip consiga tocar o banco real.
  delete process.env.DATABASE_URL;
}
```

Uso em cada teste (primeira linha do arquivo, antes de `@sbf/db` ou de
qualquer action):

```ts
import "./test-db-setup";
```

Todas as tasks seguintes (2 em diante) que escrevem teste de integração real
devem ter essa linha como primeiro import — os blocos de teste já escritos
nas Tasks 2-6 acima **precisam ser corrigidos para incluir essa linha** antes
da implementação (ver Step 11 abaixo).

- [ ] **Step 11: Criar `apps/web/src/app/actions/__tests__/fixtures.ts` (achado CRÍTICO — FKs)**

**Achado:** o schema Neon manteve as foreign keys do Postgres original — várias
`NOT NULL` (`cards.financial_product_id`, `transactions.financial_product_id`,
`liabilities.financial_product_id`, `statement_cycles.card_id`,
`financial_products.institution_id`, todas com `ON DELETE CASCADE`). Fixtures de
teste que inserem `cards`/`transactions`/`liabilities`/`statement_cycles`/
`statement_items` sem criar a cadeia de tabelas-pai primeiro falham por
violação de FK/NOT NULL, não testam isolamento. Helper compartilhado, cadeia
`institutions → financial_products → (cards | transactions | liabilities)`:

```ts
// apps/web/src/app/actions/__tests__/fixtures.ts
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";

export async function seedInstitution(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.institutions)
    .values({ user_id: userId, name: `Instituição de teste ${crypto.randomUUID()}`, type: "bank" })
    .returning();
  return row;
}

export async function seedFinancialProduct(
  userId: string,
  institutionId: string,
  type: string = "checking_account",
) {
  const db = getDb();
  const [row] = await db
    .insert(schema.financial_products)
    .values({ user_id: userId, institution_id: institutionId, name: "Produto de teste", type })
    .returning();
  return row;
}

export async function seedCard(userId: string, financialProductId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.cards)
    .values({ user_id: userId, financial_product_id: financialProductId })
    .returning();
  return row;
}

/**
 * Deleta a instituição de teste — o `ON DELETE CASCADE` em toda a cadeia
 * (financial_products → cards/transactions/liabilities/recurring_templates
 * → statement_cycles/statement_items/recurring_instances/
 * document_transactions/liability_installments) remove tudo que dependia
 * dela. Usar isto no `afterEach` em vez de deletar tabela por tabela.
 */
export async function cleanupInstitution(institutionId: string) {
  const db = getDb();
  await db.delete(schema.institutions).where(eq(schema.institutions.id, institutionId));
}
```

As Tasks 5 (`statement-cycles.ts`), 7 (`document-transactions.ts`), 9
(`transactions.ts`) e 10 (`liabilities.ts`) — corrigidas abaixo — usam este
helper. Tasks futuras (`cards.ts`, e qualquer outra que crie linha em tabela
com FK obrigatória) devem seguir o mesmo padrão: verificar `NOT NULL`/FK no
schema antes de escrever a fixture, nunca assumir que um UUID arbitrário
passa.

- [ ] **Step 12: Commit**

```bash
git add packages/db/drizzle.config.ts packages/db/src/schema.ts packages/db/src/relations.ts packages/db/src/index.ts packages/db/src/schema.test.ts apps/web/src/app/actions/test-db-setup.ts apps/web/src/app/actions/__tests__/fixtures.ts
git commit -m "fix(db): corrige casing (preserve), mode numeric, expoe namespace schema e cria infra de teste segura"
```

---

### Task 2: `institutions.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/institutions.ts`
- Test: `apps/web/src/app/actions/institutions.test.ts` (novo)

**Interfaces:**

- Consome: `schema.institutions` (Task 1).
- Produz: nenhuma outra task depende diretamente das funções deste arquivo.

- [ ] **Step 1: Escrever os testes (isolamento por usuário + caminho feliz + erro)**

```ts
// apps/web/src/app/actions/institutions.test.ts
import "./test-db-setup"; // redireciona getDb() para TEST_DATABASE_URL — ver Task 1, Step 10
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createInstitution,
  deleteInstitution,
  getInstitution,
  getInstitutions,
  updateInstitution,
} from "./institutions";

// Mocka a identidade — os testes chamam as funções "como se fossem" o userId A.
let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("institutions actions", () => {
  const createdIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db.delete(schema.institutions).where(eq(schema.institutions.id, id));
    }
  });

  it("isola instituições por usuário", async () => {
    currentUserId = USER_A;
    const a = await createInstitution({ name: "Banco A", type: "bank" });
    createdIds.push(a.id);
    currentUserId = USER_B;
    const b = await createInstitution({ name: "Banco B", type: "bank" });
    createdIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getInstitutions();
    expect(listA.map((i) => i.id)).toContain(a.id);
    expect(listA.map((i) => i.id)).not.toContain(b.id);

    const found = await getInstitution(b.id);
    expect(found).toBeNull();
  });

  it("cria, busca, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createInstitution({ name: "Nubank", type: "bank" });
    createdIds.push(created.id);

    const fetched = await getInstitution(created.id);
    expect(fetched?.name).toBe("Nubank");

    const updated = await updateInstitution(created.id, { name: "Nubank Atualizado" });
    expect(updated.name).toBe("Nubank Atualizado");

    await deleteInstitution(created.id);
    expect(await getInstitution(created.id)).toBeNull();
    createdIds.length = 0; // já deletado, não tentar de novo no afterEach
  });

  it("lança erro ao atualizar id inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateInstitution("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("lança erro ao excluir id de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createInstitution({ name: "Banco B2", type: "bank" });
    createdIds.push(b.id);

    currentUserId = USER_A;
    await expect(deleteInstitution(b.id)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham (arquivo de produção ainda não mudou)**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/institutions.test.ts`
Expected: FAIL (import de `./institutions` ainda usa `supabase-js`, mock não bate com o shape real).

- [ ] **Step 3: Reescrever `institutions.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

type Institution = typeof schema.institutions.$inferSelect;
type InstitutionInsert = Omit<typeof schema.institutions.$inferInsert, "user_id" | "id">;
type InstitutionUpdate = Partial<InstitutionInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getInstitutions(): Promise<Institution[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.institutions)
    .where(eq(schema.institutions.user_id, userId))
    .orderBy(asc(schema.institutions.display_order), asc(schema.institutions.name));
}

export async function getInstitution(id: string): Promise<Institution | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.institutions)
    .where(and(eq(schema.institutions.id, id), eq(schema.institutions.user_id, userId)));
  return row ?? null;
}

export async function createInstitution(input: InstitutionInsert): Promise<Institution> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.institutions)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateInstitution(
  id: string,
  input: InstitutionUpdate,
): Promise<Institution> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.institutions)
    .set(input)
    .where(and(eq(schema.institutions.id, id), eq(schema.institutions.user_id, userId)))
    .returning();
  if (!row) throw new Error("Instituição não encontrada");
  return row;
}

export async function deleteInstitution(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .delete(schema.institutions)
    .where(and(eq(schema.institutions.id, id), eq(schema.institutions.user_id, userId)))
    .returning({ id: schema.institutions.id });
  if (result.length === 0) throw new Error("Instituição não encontrada");
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/institutions.test.ts`
Expected: PASS (5 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/institutions.ts apps/web/src/app/actions/institutions.test.ts
git commit -m "feat(web): institutions.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 3: `financial-periods.ts` → Drizzle (com 2 bloqueios documentados)

**Files:**

- Modify: `apps/web/src/app/actions/financial-periods.ts`
- Test: `apps/web/src/app/actions/financial-periods.test.ts` (novo)

**Interfaces:**

- Consome: `schema.financial_periods`, `schema.user_financial_preferences` (Task 1).
- Produz: nenhuma outra task depende diretamente.

**Bloqueio conhecido (não resolvido nesta task):** `generate_financial_periods` e
`get_financial_period_for_date` foram descartadas por completo na migração
Supabase→Neon (`packages/db/schema.sql`, nota de gaps, item 4 — "SEIS funções
descartadas por inteiro", porque decidiam "quem é o usuário atual" via
`auth.uid()` internamente, sem receber `p_user_id`). Essas 2 funções **ficam como
estão hoje** (ainda `supabase.rpc()`) — não fazem parte desta task. Mesmo
bloqueio afeta `apps/mcp-server/src/tools/recompute-financial-periods.ts` (Fase
4b). Decisão de recriar no Neon com `p_user_id` explícito vs. reimplementar em
TypeScript fica para depois desta fase.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/financial-periods.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createFinancialPeriod,
  deleteFinancialPeriod,
  getCurrentFinancialPeriod,
  getFinancialPeriods,
  updateFinancialPeriod,
} from "./financial-periods";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("financial-periods actions", () => {
  const createdIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db.delete(schema.financial_periods).where(eq(schema.financial_periods.id, id));
    }
  });

  it("isola períodos por usuário", async () => {
    currentUserId = USER_A;
    const a = await createFinancialPeriod({
      start_date: "2026-01-01",
      end_date: "2026-01-31",
      is_current: false,
    });
    createdIds.push(a.id);
    currentUserId = USER_B;
    const b = await createFinancialPeriod({
      start_date: "2026-01-01",
      end_date: "2026-01-31",
      is_current: false,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getFinancialPeriods();
    expect(listA.map((p) => p.id)).toContain(a.id);
    expect(listA.map((p) => p.id)).not.toContain(b.id);
  });

  it("cria, marca como atual, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createFinancialPeriod({
      start_date: "2026-02-01",
      end_date: "2026-02-28",
      is_current: true,
    });
    createdIds.push(created.id);

    const current = await getCurrentFinancialPeriod();
    expect(current?.id).toBe(created.id);

    const updated = await updateFinancialPeriod(created.id, { is_current: false });
    expect(updated.is_current).toBe(false);

    await deleteFinancialPeriod(created.id);
    createdIds.length = 0;
  });

  it("lança erro ao atualizar id de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createFinancialPeriod({
      start_date: "2026-03-01",
      end_date: "2026-03-31",
      is_current: false,
    });
    createdIds.push(b.id);

    currentUserId = USER_A;
    await expect(updateFinancialPeriod(b.id, { is_current: true })).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/financial-periods.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `financial-periods.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq, gte, lte } from "drizzle-orm";

type FinancialPeriod = typeof schema.financial_periods.$inferSelect;
type FinancialPeriodInsert = Omit<typeof schema.financial_periods.$inferInsert, "user_id" | "id">;
type FinancialPeriodUpdate = Partial<FinancialPeriodInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

// ══════════════════════════════════════════════════════════════
// Financial Periods
// ══════════════════════════════════════════════════════════════

export async function getFinancialPeriods(filters?: {
  isCurrent?: boolean;
  startDate?: string;
  endDate?: string;
}): Promise<FinancialPeriod[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.financial_periods.user_id, userId)];
  if (filters?.isCurrent !== undefined) {
    conditions.push(eq(schema.financial_periods.is_current, filters.isCurrent));
  }
  if (filters?.startDate) {
    conditions.push(gte(schema.financial_periods.start_date, filters.startDate));
  }
  if (filters?.endDate) {
    conditions.push(lte(schema.financial_periods.end_date, filters.endDate));
  }

  return db
    .select()
    .from(schema.financial_periods)
    .where(and(...conditions))
    .orderBy(asc(schema.financial_periods.start_date));
}

export async function getCurrentFinancialPeriod(): Promise<FinancialPeriod | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.financial_periods)
    .where(
      and(
        eq(schema.financial_periods.is_current, true),
        eq(schema.financial_periods.user_id, userId),
      ),
    );
  return row ?? null;
}

export async function createFinancialPeriod(
  input: FinancialPeriodInsert,
): Promise<FinancialPeriod> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.financial_periods)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateFinancialPeriod(
  id: string,
  input: FinancialPeriodUpdate,
): Promise<FinancialPeriod> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.financial_periods)
    .set(input)
    .where(and(eq(schema.financial_periods.id, id), eq(schema.financial_periods.user_id, userId)))
    .returning();
  if (!row) throw new Error("Período financeiro não encontrado");
  return row;
}

export async function deleteFinancialPeriod(id: string): Promise<void> {
  // Nota: o código Supabase original não faz `.select().single()` aqui — só
  // verifica `error`. 0 linhas afetadas (id inexistente ou de outro usuário)
  // não lança erro hoje; esta reescrita preserva esse comportamento (exceção
  // documentada à regra geral de "lança erro em 0 linhas").
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.financial_periods)
    .where(and(eq(schema.financial_periods.id, id), eq(schema.financial_periods.user_id, userId)));
}

// ══════════════════════════════════════════════════════════════
// RPC: Gerar Períodos Financeiros — BLOQUEADA, NÃO REESCRITA NESTA TASK
// ══════════════════════════════════════════════════════════════
//
// generate_financial_periods foi descartada por completo na migração pro Neon
// (packages/db/schema.sql, nota de gaps item 4). Decisão de recriar com
// p_user_id explícito ou reimplementar em TypeScript é separada desta fase.
// Permanece EXATAMENTE como está hoje:

export async function generateFinancialPeriods(
  startDay: number,
  monthsAhead: number = 12,
  anchorDate?: string,
): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.rpc("generate_financial_periods", {
    p_start_day: startDay,
    p_months_ahead: monthsAhead,
    p_anchor_date: anchorDate ?? new Date().toISOString().split("T")[0]!,
  });

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// RPC: Encontrar Período para Data — BLOQUEADA, NÃO REESCRITA NESTA TASK
// ══════════════════════════════════════════════════════════════
//
// get_financial_period_for_date também está na lista de "SEIS funções
// descartadas por inteiro". Mesma decisão pendente. Permanece como está hoje:

export async function getFinancialPeriodForDate(date: string): Promise<FinancialPeriod | null> {
  const supabase = await createClient();
  const { data: periodId, error } = await supabase.rpc("get_financial_period_for_date", {
    p_date: date,
  });

  if (error) throw new Error(error.message);
  if (!periodId) return null;

  const { data, error: fetchError } = await supabase
    .from("financial_periods")
    .select("*")
    .eq("id", periodId)
    .single();

  if (fetchError) throw new Error(fetchError.message);
  return data;
}

// ══════════════════════════════════════════════════════════════
// User Financial Preferences
// ══════════════════════════════════════════════════════════════

export async function getFinancialPreferences() {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.user_financial_preferences)
    .where(eq(schema.user_financial_preferences.user_id, userId));
  return row ?? null;
}

export async function upsertFinancialPreferences(input: {
  financial_cycle_start_day?: number | null;
  financial_cycle_anchor_date?: string | null;
  default_currency?: string;
}) {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.user_financial_preferences)
    .values({ ...input, user_id: userId })
    .onConflictDoUpdate({
      target: schema.user_financial_preferences.user_id,
      set: input,
    })
    .returning();
  return row;
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/financial-periods.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/financial-periods.ts apps/web/src/app/actions/financial-periods.test.ts
git commit -m "feat(web): financial-periods.ts usa Drizzle/Neon (2 RPCs descartadas ficam bloqueadas)"
```

---

### Task 4: `pluggy-connections.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/pluggy-connections.ts`
- Test: `apps/web/src/app/actions/pluggy-connections.test.ts` (novo)

**Interfaces:**

- Consome: `schema.provider_connections`, `schema.external_account_mappings` (Task 1).

**Nota (candidato a Apêndice B, não decidido aqui):** `completePluggyConnection`
faz upsert em `provider_connections` seguido de um loop de upserts em
`external_account_mappings`, sequencial sem transação — comportamento
pré-existente (Supabase também não usa transação aqui), preservado sem mudança.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/pluggy-connections.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  getProviderConnections,
  revokeProviderConnection,
  updateExternalAccountMapping,
} from "./pluggy-connections";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("@sbf/financial-connectors/pluggy", () => ({ PluggyProvider: { fromEnv: () => ({}) } }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("pluggy-connections actions", () => {
  const createdConnectionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdConnectionIds.splice(0)) {
      await db
        .delete(schema.external_account_mappings)
        .where(eq(schema.external_account_mappings.provider_connection_id, id));
      await db.delete(schema.provider_connections).where(eq(schema.provider_connections.id, id));
    }
  });

  it("isola conexões por usuário", async () => {
    const db = getDb();
    const [connA] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_A, provider: "pluggy", external_item_id: "item-a", status: "active" })
      .returning();
    createdConnectionIds.push(connA.id);
    const [connB] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_B, provider: "pluggy", external_item_id: "item-b", status: "active" })
      .returning();
    createdConnectionIds.push(connB.id);

    currentUserId = USER_A;
    const listA = await getProviderConnections();
    expect(listA.map((c) => c.id)).toContain(connA.id);
    expect(listA.map((c) => c.id)).not.toContain(connB.id);
  });

  it("lança erro ao revogar conexão de outro usuário e não afeta a linha", async () => {
    const db = getDb();
    const [connB] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_B, provider: "pluggy", external_item_id: "item-c", status: "active" })
      .returning();
    createdConnectionIds.push(connB.id);

    currentUserId = USER_A;
    // Nota: revokeProviderConnection não checa linhas afetadas hoje (preservado) —
    // este teste confirma que a linha do outro usuário não é revogada, não que
    // a chamada lance erro.
    await revokeProviderConnection(connB.id);
    const [stillActive] = await db
      .select()
      .from(schema.provider_connections)
      .where(eq(schema.provider_connections.id, connB.id));
    expect(stillActive.status).toBe("active");
  });

  it("lança erro ao atualizar mapeamento de outro usuário", async () => {
    const db = getDb();
    const [connB] = await db
      .insert(schema.provider_connections)
      .values({ user_id: USER_B, provider: "pluggy", external_item_id: "item-d", status: "active" })
      .returning();
    createdConnectionIds.push(connB.id);
    const [mapping] = await db
      .insert(schema.external_account_mappings)
      .values({
        user_id: USER_B,
        provider_connection_id: connB.id,
        external_account_id: "acc-1",
        external_account_name: "Conta",
        external_account_type: "checking",
      })
      .returning();

    currentUserId = USER_A;
    await expect(
      updateExternalAccountMapping(mapping.id, { external_account_name: "Hackeado" }),
    ).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/pluggy-connections.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `pluggy-connections.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { PluggyProvider } from "@sbf/financial-connectors/pluggy";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq } from "drizzle-orm";

export type ProviderConnection = typeof schema.provider_connections.$inferSelect;
export type ExternalAccountMapping = typeof schema.external_account_mappings.$inferSelect;
type ExternalAccountMappingUpdate = Partial<
  Omit<typeof schema.external_account_mappings.$inferInsert, "user_id" | "id">
>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getProviderConnections(): Promise<ProviderConnection[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.provider_connections)
    .where(eq(schema.provider_connections.user_id, userId))
    .orderBy(desc(schema.provider_connections.created_at));
}

export async function getExternalAccountMappings(
  providerConnectionId: string,
): Promise<ExternalAccountMapping[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.external_account_mappings)
    .where(
      and(
        eq(schema.external_account_mappings.provider_connection_id, providerConnectionId),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    )
    .orderBy(asc(schema.external_account_mappings.created_at));
}

/** Token de vida curta (30min) pro widget Pluggy Connect no navegador. */
export async function createPluggyConnectToken(options?: {
  itemId?: string;
}): Promise<{ connectToken: string }> {
  const userId = await requireUserId();
  const provider = PluggyProvider.fromEnv();
  const { connectToken } = await provider.createConnectToken({
    itemId: options?.itemId,
    clientUserId: userId,
  });
  return { connectToken };
}

/**
 * Chamada pelo frontend depois que o widget Pluggy Connect termina e devolve
 * um `itemId`. Persiste a conexão e cria (sem sobrescrever mapeamentos já
 * feitos pelo usuário) um `external_account_mappings` "pending_mapping" pra
 * cada conta que a Pluggy retornar.
 */
export async function completePluggyConnection(
  externalItemId: string,
): Promise<ProviderConnection> {
  const userId = await requireUserId();
  const db = getDb();

  const provider = PluggyProvider.fromEnv();
  const connection = await provider.getConnection(externalItemId);
  const accounts = await provider.listAccounts(externalItemId);

  const [row] = await db
    .insert(schema.provider_connections)
    .values({
      user_id: userId,
      provider: "pluggy",
      external_item_id: externalItemId,
      institution_name: connection.institutionName ?? null,
      status: "active",
    })
    .onConflictDoUpdate({
      target: [
        schema.provider_connections.user_id,
        schema.provider_connections.provider,
        schema.provider_connections.external_item_id,
      ],
      set: { institution_name: connection.institutionName ?? null, status: "active" },
    })
    .returning();

  for (const account of accounts) {
    await db
      .insert(schema.external_account_mappings)
      .values({
        user_id: userId,
        provider_connection_id: row.id,
        external_account_id: account.externalAccountId,
        external_account_name: account.name,
        external_account_type: account.type,
      })
      .onConflictDoNothing({
        target: [
          schema.external_account_mappings.provider_connection_id,
          schema.external_account_mappings.external_account_id,
        ],
      });
  }

  return row;
}

export async function updateExternalAccountMapping(
  id: string,
  input: ExternalAccountMappingUpdate,
): Promise<ExternalAccountMapping> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.external_account_mappings)
    .set(input)
    .where(
      and(
        eq(schema.external_account_mappings.id, id),
        eq(schema.external_account_mappings.user_id, userId),
      ),
    )
    .returning();
  if (!row) throw new Error("Vínculo de conta externa não encontrado");
  return row;
}

export async function revokeProviderConnection(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — 0 linhas afetadas não lança erro,
  // comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.provider_connections)
    .set({ status: "revoked" })
    .where(
      and(eq(schema.provider_connections.id, id), eq(schema.provider_connections.user_id, userId)),
    );
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/pluggy-connections.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/pluggy-connections.ts apps/web/src/app/actions/pluggy-connections.test.ts
git commit -m "feat(web): pluggy-connections.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 5: `statement-cycles.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/statement-cycles.ts`
- Test: `apps/web/src/app/actions/statement-cycles.test.ts` (novo)

**Interfaces:**

- Consome: `schema.statement_cycles`, `schema.statement_items` (Task 1).

- [ ] **Step 1: Escrever os testes**

**Correção (achado CRÍTICO da revisão da Verônica-Bolso):** `statement_cycles`
tem `card_id uuid NOT NULL` (FK pra `cards`, `ON DELETE CASCADE`), e as colunas
reais são `reference_month`, `cycle_start_date`, `cycle_end_date`, `due_date`
(todas `NOT NULL`) — **não existe** coluna `closing_date` (erro do rascunho
original desta task). Fixture corrigida usa `seedInstitution`/
`seedFinancialProduct`/`seedCard` de `./__tests__/fixtures` (Task 1, Step 11) e
`cleanupInstitution` no lugar de deletar `statement_cycles` direto (o CASCADE
cuida do resto).

```ts
// apps/web/src/app/actions/statement-cycles.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  cleanupInstitution,
  seedCard,
  seedFinancialProduct,
  seedInstitution,
} from "./__tests__/fixtures";
import {
  createStatementCycle,
  deleteStatementCycle,
  getStatementCycle,
  getStatementCycles,
  updateStatementCycle,
} from "./statement-cycles";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedCardFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id, "credit_card");
  const card = await seedCard(userId, product.id);
  return { institutionId: institution.id, card };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("statement-cycles actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/cards/statement_cycles
    }
  });

  it("isola ciclos de fatura por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedCardFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createStatementCycle({
      card_id: seedA.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    currentUserId = USER_B;
    const seedB = await seedCardFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createStatementCycle({
      card_id: seedB.card.id,
      reference_month: "2026-01-01",
      cycle_start_date: "2025-12-01",
      cycle_end_date: "2025-12-31",
      due_date: "2026-01-10",
    });

    currentUserId = USER_A;
    const listA = await getStatementCycles();
    expect(listA.map((c) => c.id)).toContain(a.id);
    expect(listA.map((c) => c.id)).not.toContain(b.id);
    expect(await getStatementCycle(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedCardFor(USER_A);
    institutionIds.push(seed.institutionId);

    const created = await createStatementCycle({
      card_id: seed.card.id,
      reference_month: "2026-02-01",
      cycle_start_date: "2026-01-01",
      cycle_end_date: "2026-01-31",
      due_date: "2026-02-10",
    });

    const updated = await updateStatementCycle(created.id, { due_date: "2026-02-12" });
    expect(updated.due_date).toBe("2026-02-12");

    await deleteStatementCycle(created.id);
    expect(await getStatementCycle(created.id)).toBeNull();
  });

  it("lança erro ao atualizar ciclo inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateStatementCycle("00000000-0000-0000-0000-000000000000", { due_date: "2026-01-01" }),
    ).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/statement-cycles.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `statement-cycles.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq } from "drizzle-orm";

type StatementCycle = typeof schema.statement_cycles.$inferSelect;
type StatementCycleInsert = Omit<typeof schema.statement_cycles.$inferInsert, "user_id" | "id">;
type StatementCycleUpdate = Partial<StatementCycleInsert>;
type StatementItem = typeof schema.statement_items.$inferSelect;
type StatementItemInsert = Omit<typeof schema.statement_items.$inferInsert, "user_id" | "id">;
type StatementItemUpdate = Partial<StatementItemInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getStatementCycles(cardId?: string): Promise<StatementCycle[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.statement_cycles.user_id, userId)];
  if (cardId) conditions.push(eq(schema.statement_cycles.card_id, cardId));
  return db
    .select()
    .from(schema.statement_cycles)
    .where(and(...conditions))
    .orderBy(desc(schema.statement_cycles.due_date));
}

export async function getStatementCycle(id: string): Promise<StatementCycle | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.statement_cycles)
    .where(and(eq(schema.statement_cycles.id, id), eq(schema.statement_cycles.user_id, userId)));
  return row ?? null;
}

export async function createStatementCycle(input: StatementCycleInsert): Promise<StatementCycle> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.statement_cycles)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateStatementCycle(
  id: string,
  input: StatementCycleUpdate,
): Promise<StatementCycle> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.statement_cycles)
    .set(input)
    .where(and(eq(schema.statement_cycles.id, id), eq(schema.statement_cycles.user_id, userId)))
    .returning();
  if (!row) throw new Error("Ciclo de fatura não encontrado");
  return row;
}

export async function deleteStatementCycle(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.statement_cycles)
    .where(and(eq(schema.statement_cycles.id, id), eq(schema.statement_cycles.user_id, userId)));
}

export async function getStatementItems(statementCycleId: string): Promise<StatementItem[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.statement_items)
    .where(
      and(
        eq(schema.statement_items.statement_cycle_id, statementCycleId),
        eq(schema.statement_items.user_id, userId),
      ),
    )
    .orderBy(asc(schema.statement_items.transaction_date));
}

export async function createStatementItem(input: StatementItemInsert): Promise<StatementItem> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.statement_items)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateStatementItem(
  id: string,
  input: StatementItemUpdate,
): Promise<StatementItem> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.statement_items)
    .set(input)
    .where(and(eq(schema.statement_items.id, id), eq(schema.statement_items.user_id, userId)))
    .returning();
  if (!row) throw new Error("Item de fatura não encontrado");
  return row;
}

export async function deleteStatementItem(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.statement_items)
    .where(and(eq(schema.statement_items.id, id), eq(schema.statement_items.user_id, userId)));
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/statement-cycles.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/statement-cycles.ts apps/web/src/app/actions/statement-cycles.test.ts
git commit -m "feat(web): statement-cycles.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 6: `recurring.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/recurring.ts`
- Test: `apps/web/src/app/actions/recurring.test.ts` (novo)

**Interfaces:**

- Consome: `schema.recurring_templates`, `schema.recurring_instances`, `schema.recurring_template_tags` (Task 1).

**Nota (candidato a Apêndice B, não decidido aqui):** `createRecurringTemplate`/
`updateRecurringTemplate` escrevem em `recurring_templates` e depois
`recurring_template_tags` sequencialmente sem transação — comportamento
pré-existente, preservado. `recurring_template_tags` não tem coluna `user_id`
própria — a segurança depende do `id` do template já ter sido validado no passo
anterior.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/recurring.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createRecurringTemplate,
  deleteRecurringTemplate,
  getRecurringTemplate,
  getRecurringTemplates,
  updateRecurringTemplate,
} from "./recurring";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("recurring actions", () => {
  const createdIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of createdIds.splice(0)) {
      await db
        .delete(schema.recurring_template_tags)
        .where(eq(schema.recurring_template_tags.recurring_template_id, id));
      await db.delete(schema.recurring_templates).where(eq(schema.recurring_templates.id, id));
    }
  });

  it("isola templates recorrentes por usuário", async () => {
    currentUserId = USER_A;
    const a = await createRecurringTemplate({
      name: "Aluguel",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 1500,
      is_active: true,
    } as any);
    createdIds.push(a.id);
    currentUserId = USER_B;
    const b = await createRecurringTemplate({
      name: "Aluguel B",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 1200,
      is_active: true,
    } as any);
    createdIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getRecurringTemplates();
    expect(listA.map((t) => t.id)).toContain(a.id);
    expect(listA.map((t) => t.id)).not.toContain(b.id);
    expect(await getRecurringTemplate(b.id)).toBeNull();
  });

  it("cria com tags, atualiza e desativa (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createRecurringTemplate({
      name: "Internet",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 100,
      is_active: true,
    } as any);
    createdIds.push(created.id);

    const updated = await updateRecurringTemplate(created.id, { amount: 120 } as any);
    expect(updated.amount).toBe(120);

    await deleteRecurringTemplate(created.id);
    const afterDelete = await getRecurringTemplate(created.id);
    expect(afterDelete?.is_active).toBe(false);
  });

  it("lança erro ao atualizar template de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createRecurringTemplate({
      name: "Luz",
      type: "expense", // NOT NULL, CHECK obriga income|expense|liability_payment|statement_payment
      frequency: "monthly",
      amount: 200,
      is_active: true,
    } as any);
    createdIds.push(b.id);

    currentUserId = USER_A;
    await expect(updateRecurringTemplate(b.id, { amount: 999 } as any)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/recurring.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `recurring.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq, gte, lte } from "drizzle-orm";

type RecurringTemplate = typeof schema.recurring_templates.$inferSelect;
type RecurringTemplateInsert = Omit<
  typeof schema.recurring_templates.$inferInsert,
  "user_id" | "id"
>;
type RecurringTemplateUpdate = Partial<RecurringTemplateInsert>;
type RecurringInstance = typeof schema.recurring_instances.$inferSelect;
type RecurringInstanceUpdate = Partial<
  Omit<typeof schema.recurring_instances.$inferInsert, "user_id" | "id">
>;
type RecurringInstanceInsert = Omit<
  typeof schema.recurring_instances.$inferInsert,
  "id" | "created_at" | "updated_at"
>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getRecurringTemplates(activeOnly = true): Promise<RecurringTemplate[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.recurring_templates.user_id, userId)];
  if (activeOnly) conditions.push(eq(schema.recurring_templates.is_active, true));
  return db
    .select()
    .from(schema.recurring_templates)
    .where(and(...conditions))
    .orderBy(asc(schema.recurring_templates.name));
}

export async function getRecurringTemplate(id: string): Promise<RecurringTemplate | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.recurring_templates)
    .where(
      and(eq(schema.recurring_templates.id, id), eq(schema.recurring_templates.user_id, userId)),
    );
  return row ?? null;
}

export async function createRecurringTemplate(
  input: RecurringTemplateInsert,
  tagIds?: string[],
): Promise<RecurringTemplate> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.recurring_templates)
    .values({ ...input, user_id: userId })
    .returning();

  if (tagIds && tagIds.length > 0) {
    await db
      .insert(schema.recurring_template_tags)
      .values(tagIds.map((tagId) => ({ recurring_template_id: row.id, tag_id: tagId })));
  }
  return row;
}

export async function updateRecurringTemplate(
  id: string,
  input: RecurringTemplateUpdate,
  tagIds?: string[],
): Promise<RecurringTemplate> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.recurring_templates)
    .set(input)
    .where(
      and(eq(schema.recurring_templates.id, id), eq(schema.recurring_templates.user_id, userId)),
    )
    .returning();
  if (!row) throw new Error("Template recorrente não encontrado");

  if (tagIds !== undefined) {
    await db
      .delete(schema.recurring_template_tags)
      .where(eq(schema.recurring_template_tags.recurring_template_id, id));
    if (tagIds.length > 0) {
      await db
        .insert(schema.recurring_template_tags)
        .values(tagIds.map((tagId) => ({ recurring_template_id: id, tag_id: tagId })));
    }
  }
  return row;
}

export async function deleteRecurringTemplate(id: string): Promise<void> {
  // Nota: soft delete (is_active=false), sem checagem de linhas afetadas hoje
  // — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.recurring_templates)
    .set({ is_active: false })
    .where(
      and(eq(schema.recurring_templates.id, id), eq(schema.recurring_templates.user_id, userId)),
    );
}

export async function getRecurringInstances(
  templateId: string,
  filters?: { status?: string; startDate?: string; endDate?: string },
): Promise<RecurringInstance[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [
    eq(schema.recurring_instances.recurring_template_id, templateId),
    eq(schema.recurring_instances.user_id, userId),
  ];
  if (filters?.status) conditions.push(eq(schema.recurring_instances.status, filters.status));
  if (filters?.startDate)
    conditions.push(gte(schema.recurring_instances.expected_date, filters.startDate));
  if (filters?.endDate)
    conditions.push(lte(schema.recurring_instances.expected_date, filters.endDate));
  return db
    .select()
    .from(schema.recurring_instances)
    .where(and(...conditions))
    .orderBy(asc(schema.recurring_instances.expected_date));
}

export async function updateRecurringInstance(
  id: string,
  input: RecurringInstanceUpdate,
): Promise<RecurringInstance> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.recurring_instances)
    .set(input)
    .where(
      and(eq(schema.recurring_instances.id, id), eq(schema.recurring_instances.user_id, userId)),
    )
    .returning();
  if (!row) throw new Error("Instância recorrente não encontrada");
  return row;
}

/**
 * Gera instâncias de recorrência para um template até uma data limite.
 * Recorrências geram EXPECTATIVA de pagamento, não marcam pagamento automático.
 */
export async function generateRecurringInstances(
  templateId: string,
  untilDate: string,
): Promise<number> {
  const userId = await requireUserId();
  const db = getDb();

  const template = await getRecurringTemplate(templateId);
  if (!template) throw new Error("Template não encontrado");
  if (!template.is_active) throw new Error("Template inativo");

  const [lastInstance] = await db
    .select({ expected_date: schema.recurring_instances.expected_date })
    .from(schema.recurring_instances)
    .where(
      and(
        eq(schema.recurring_instances.recurring_template_id, templateId),
        eq(schema.recurring_instances.user_id, userId),
      ),
    )
    .orderBy(desc(schema.recurring_instances.expected_date))
    .limit(1);

  const startFrom = lastInstance
    ? new Date(lastInstance.expected_date)
    : template.starts_at
      ? new Date(template.starts_at)
      : new Date();

  const endDate = new Date(untilDate);
  const instances: RecurringInstanceInsert[] = [];
  const current = new Date(startFrom);
  if (lastInstance) {
    advanceDate(current, template.frequency, template.day_of_month, template.custom_interval_days);
  }

  while (current <= endDate) {
    if (template.ends_at && current > new Date(template.ends_at)) break;
    instances.push({
      user_id: userId,
      recurring_template_id: templateId,
      expected_date: current.toISOString().split("T")[0]!,
      expected_amount: template.amount,
      status: "pending",
    });
    advanceDate(current, template.frequency, template.day_of_month, template.custom_interval_days);
  }

  if (instances.length > 0) {
    await db.insert(schema.recurring_instances).values(instances);
  }
  return instances.length;
}

function advanceDate(
  date: Date,
  frequency: string,
  dayOfMonth: number | null,
  customDays: number | null,
) {
  switch (frequency) {
    case "monthly":
      date.setMonth(date.getMonth() + 1);
      if (dayOfMonth) {
        const lastDay = new Date(date.getFullYear(), date.getMonth() + 1, 0).getDate();
        date.setDate(Math.min(dayOfMonth, lastDay));
      }
      break;
    case "weekly":
      date.setDate(date.getDate() + 7);
      break;
    case "biweekly":
      date.setDate(date.getDate() + 14);
      break;
    case "quarterly":
      date.setMonth(date.getMonth() + 3);
      break;
    case "annual":
      date.setFullYear(date.getFullYear() + 1);
      break;
    case "custom":
      date.setDate(date.getDate() + (customDays ?? 30));
      break;
  }
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/recurring.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/recurring.ts apps/web/src/app/actions/recurring.test.ts
git commit -m "feat(web): recurring.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 7: `document-transactions.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/document-transactions.ts`
- Test: `apps/web/src/app/actions/document-transactions.test.ts` (novo)

**Interfaces:**

- Consome: `schema.document_transactions`, `schema.transactions` (Task 1).

**Nota:** `listLinkedTransactions` faz join `document_transactions` × `transactions`
que hoje não filtra por usuário (RLS fazia isso). O Drizzle não tem uma relational
query API ligada por padrão (`client.ts` só passa `schema`, não `relations`) —
usar `.innerJoin()` explícito do query builder, sem mudar `client.ts` (fora de
escopo desta task).

- [ ] **Step 1: Escrever os testes**

**Correção (achado CRÍTICO da revisão da Verônica-Bolso):**
`transactions.financial_product_id` é `NOT NULL` — a fixture original criava
uma transação sem esse campo, o que falharia por violação de NOT NULL antes de
sequer chegar a testar isolamento. Corrigido usando os helpers de
`./__tests__/fixtures` (Task 1, Step 11) para montar a cadeia
`institution → financial_product → transaction`.

```ts
// apps/web/src/app/actions/document-transactions.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  linkTransactionToDocument,
  listLinkedTransactions,
  unlinkTransactionFromDocument,
} from "./document-transactions";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("document-transactions actions", () => {
  const linkIds: string[] = [];
  const docIds: string[] = [];
  const institutionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of linkIds.splice(0)) {
      await db.delete(schema.document_transactions).where(eq(schema.document_transactions.id, id));
    }
    for (const id of docIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/transactions/document_transactions
    }
  });

  async function seedDocAndTx(userId: string) {
    const db = getDb();
    const [doc] = await db
      .insert(schema.source_documents)
      .values({
        user_id: userId,
        origin_type: "manual_upload",
        origin_key: `test::${crypto.randomUUID()}`,
        filename: "teste.pdf",
        status: "new",
      })
      .returning();
    docIds.push(doc.id);

    const institution = await seedInstitution(userId);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(userId, institution.id);

    const [tx] = await db
      .insert(schema.transactions)
      .values({
        user_id: userId,
        financial_product_id: product.id,
        type: "expense",
        event_date: "2026-01-01",
        amount: 100,
      } as any)
      .returning();
    return { doc, tx };
  }

  it("isola vínculos por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedDocAndTx(USER_A);
    const linkA = await linkTransactionToDocument(seedA.doc.id, seedA.tx.id, "payment");
    linkIds.push(linkA.id);

    currentUserId = USER_B;
    const seedB = await seedDocAndTx(USER_B);
    const linkB = await linkTransactionToDocument(seedB.doc.id, seedB.tx.id, "payment");
    linkIds.push(linkB.id);

    currentUserId = USER_A;
    const listA = await listLinkedTransactions(seedA.doc.id);
    expect(listA.map((l) => l.id)).toContain(linkA.id);

    const listB = await listLinkedTransactions(seedB.doc.id);
    expect(listB.map((l) => l.id)).not.toContain(linkA.id);
  });

  it("lança erro ao desvincular id de outro usuário", async () => {
    currentUserId = USER_B;
    const seedB = await seedDocAndTx(USER_B);
    const linkB = await linkTransactionToDocument(seedB.doc.id, seedB.tx.id, "payment");
    linkIds.push(linkB.id);

    currentUserId = USER_A;
    await expect(unlinkTransactionFromDocument(linkB.id)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/document-transactions.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `document-transactions.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, count, eq } from "drizzle-orm";

export type DocumentLinkType = "payment" | "refund" | "installment" | "support";

export type DocumentTransactionLink = typeof schema.document_transactions.$inferSelect;

export interface DocumentTransactionLinkWithTransaction extends DocumentTransactionLink {
  transaction: typeof schema.transactions.$inferSelect;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function listLinkedTransactions(
  sourceDocumentId: string,
): Promise<DocumentTransactionLinkWithTransaction[]> {
  const userId = await requireUserId();
  const db = getDb();

  const rows = await db
    .select({ link: schema.document_transactions, transaction: schema.transactions })
    .from(schema.document_transactions)
    .innerJoin(
      schema.transactions,
      eq(schema.document_transactions.transaction_id, schema.transactions.id),
    )
    .where(
      and(
        eq(schema.document_transactions.source_document_id, sourceDocumentId),
        eq(schema.document_transactions.user_id, userId),
      ),
    )
    .orderBy(asc(schema.document_transactions.created_at));

  return rows.map(({ link, transaction }) => ({ ...link, transaction }));
}

export async function linkTransactionToDocument(
  sourceDocumentId: string,
  transactionId: string,
  linkType: DocumentLinkType,
  options?: { confidence?: number; notes?: string },
): Promise<DocumentTransactionLink> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .insert(schema.document_transactions)
    .values({
      source_document_id: sourceDocumentId,
      transaction_id: transactionId,
      user_id: userId,
      link_type: linkType,
      confidence: options?.confidence ?? null,
      created_by: "user",
      notes: options?.notes ?? null,
    })
    .returning();

  return row;
}

export async function unlinkTransactionFromDocument(linkId: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.document_transactions)
    .where(
      and(
        eq(schema.document_transactions.id, linkId),
        eq(schema.document_transactions.user_id, userId),
      ),
    )
    .returning({ id: schema.document_transactions.id });

  if (result.length === 0) throw new Error("Vínculo não encontrado");
}

export async function getLinkedTransactionCount(sourceDocumentId: string): Promise<number> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .select({ value: count() })
    .from(schema.document_transactions)
    .where(
      and(
        eq(schema.document_transactions.source_document_id, sourceDocumentId),
        eq(schema.document_transactions.user_id, userId),
      ),
    );

  return row?.value ?? 0;
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/document-transactions.test.ts`
Expected: PASS (2 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/document-transactions.ts apps/web/src/app/actions/document-transactions.test.ts
git commit -m "feat(web): document-transactions.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 8: `ingestion.ts` → Drizzle (arquivo maior, 22 funções)

**Files:**

- Modify: `apps/web/src/app/actions/ingestion.ts`
- Test: `apps/web/src/app/actions/ingestion.test.ts` (novo)

**Interfaces:**

- Consome: `schema.source_documents`, `schema.ingestion_jobs`, `schema.ingestion_runs`,
  `schema.ingestion_logs`, `schema.draft_batches`, `schema.draft_records`,
  `schema.parsed_document_versions`, `schema.extraction_results`,
  `schema.document_fingerprints`, `schema.pattern_feedback` (Task 1).

**Notas importantes:**

- Storage (`supabase.storage.from("ingestion-originals")`) **não muda nesta
  task** — permanece via `supabase-js` (é escopo da Fase 3). `getDocumentStorageUrl`,
  a parte de upload em `uploadDocument`, e a remoção de arquivo em
  `deleteSourceDocument` continuam intocadas.
- **Candidato a Apêndice B (não decidido aqui):** `uploadDocument` faz 3 inserts
  sequenciais sem transação (`source_documents` → `ingestion_runs` →
  `ingestion_jobs`) — é o exemplo já citado na spec, confirmando este como o
  call site real. Comportamento preservado, não corrigido nesta task.
- Updates "fire-and-forget" que hoje não checam linhas afetadas (`reprocessDocument`'s
  update de status do job, `triggerAiFullAnalysis`'s update do run pra "completed")
  **preservam esse comportamento** — não ganham throw novo (exceção documentada).
- Deletes em massa dentro de `deleteSourceDocument` (drafts, pattern_feedback,
  extraction_results, parsed_document_versions, draft_batches,
  document_fingerprints, ingestion_logs, ingestion_jobs) são bulk deletes — 0
  linhas afetadas é normal ali (nem toda dependência existe sempre), sem throw.
  Só o delete final do próprio `source_documents` lança erro em 0 linhas.
- Mensagens de erro deixam de ter o prefixo customizado (`Erro ao <ação>: ...`)
  porque o Drizzle lança o erro nativo do driver — o comportamento de "propaga
  erro" é idêntico, só o texto da mensagem muda. Isso é uma divergência
  cosmética aceita (a spec fala de "mesmo comportamento de erro", não "mesma
  string de erro").

- [ ] **Step 1: Escrever os testes (cobrindo as funções mais centrais — isolamento, upload, delete)**

```ts
// apps/web/src/app/actions/ingestion.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { deleteSourceDocument, getSourceDocument, getSourceDocuments } from "./ingestion";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
    storage: { from: () => ({ remove: async () => ({ error: null }) }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("ingestion actions — source documents", () => {
  const docIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of docIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
  });

  async function seedDoc(userId: string) {
    const db = getDb();
    const [doc] = await db
      .insert(schema.source_documents)
      .values({
        user_id: userId,
        origin_type: "manual_upload",
        origin_key: `test::${crypto.randomUUID()}`,
        filename: "boleto.pdf",
        status: "new",
      })
      .returning();
    docIds.push(doc.id);
    return doc;
  }

  it("isola documentos por usuário", async () => {
    currentUserId = USER_A;
    const docA = await seedDoc(USER_A);
    currentUserId = USER_B;
    const docB = await seedDoc(USER_B);

    currentUserId = USER_A;
    const { data: listA } = await getSourceDocuments();
    expect(listA.map((d) => d.id)).toContain(docA.id);
    expect(listA.map((d) => d.id)).not.toContain(docB.id);
    expect(await getSourceDocument(docB.id)).toBeNull();
  });

  it("lança erro ao excluir documento de outro usuário", async () => {
    currentUserId = USER_B;
    const docB = await seedDoc(USER_B);

    currentUserId = USER_A;
    await expect(deleteSourceDocument(docB.id, "document_only")).rejects.toThrow();
  });

  it("exclui documento (modo document_only) removendo o registro", async () => {
    currentUserId = USER_A;
    const doc = await seedDoc(USER_A);
    await deleteSourceDocument(doc.id, "document_only");

    const found = await getSourceDocument(doc.id);
    expect(found?.status).toBe("deleted");
    docIds.length = 0;
  });
});
```

(Este arquivo tem 22 funções — os testes acima cobrem o núcleo de leitura/escrita
com isolamento por usuário, que é a mudança de comportamento real desta task. As
demais funções seguem o mesmo padrão de filtro `eq(<tabela>.user_id, userId)` já
validado pelos testes acima; não há necessidade de replicar o mesmo teste de
isolamento 22 vezes.)

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/ingestion.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `ingestion.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, count, desc, eq, gte, ilike, inArray, isNotNull, lte, ne } from "drizzle-orm";

async function computeContentHash(data: ArrayBuffer | Uint8Array): Promise<string> {
  const buffer = data instanceof Uint8Array ? data : new Uint8Array(data);
  const hashBuffer = await crypto.subtle.digest("SHA-256", buffer as unknown as BufferSource);
  return Array.from(new Uint8Array(hashBuffer))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

export type DeleteSourceDocumentMode = "document_only" | "document_and_ingestion";

function throwIfError(error: { message: string } | null, context: string): void {
  if (error) {
    throw new Error(`Erro ao ${context}: ${error.message}`);
  }
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

type SourceDocument = typeof schema.source_documents.$inferSelect;
type IngestionJob = typeof schema.ingestion_jobs.$inferSelect;
type IngestionLog = typeof schema.ingestion_logs.$inferSelect;
type IngestionRun = typeof schema.ingestion_runs.$inferSelect;
type DraftBatch = typeof schema.draft_batches.$inferSelect;
type DraftRecord = typeof schema.draft_records.$inferSelect;

// ══════════════════════════════════════════════════════════════
// Source Documents
// ══════════════════════════════════════════════════════════════

export async function getSourceDocuments(filters?: {
  status?: string;
  originType?: string;
  mimeType?: string;
  dateFrom?: string;
  dateTo?: string;
  search?: string;
  limit?: number;
  offset?: number;
}): Promise<{ data: SourceDocument[]; count: number }> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.source_documents.user_id, userId)];

  if (!filters?.status) {
    conditions.push(ne(schema.source_documents.status, "deleted"));
  }
  if (filters?.status) {
    conditions.push(eq(schema.source_documents.status, filters.status));
  }
  if (filters?.originType) {
    conditions.push(eq(schema.source_documents.origin_type, filters.originType));
  }
  if (filters?.mimeType) {
    conditions.push(eq(schema.source_documents.mime_type, filters.mimeType));
  }
  if (filters?.dateFrom) {
    conditions.push(gte(schema.source_documents.created_at, filters.dateFrom));
  }
  if (filters?.dateTo) {
    conditions.push(lte(schema.source_documents.created_at, filters.dateTo));
  }
  if (filters?.search) {
    conditions.push(ilike(schema.source_documents.filename, `%${filters.search}%`));
  }

  const where = and(...conditions);

  const [{ value: total }] = await db
    .select({ value: count() })
    .from(schema.source_documents)
    .where(where);
  const orderBy = desc(schema.source_documents.created_at);

  let rows: SourceDocument[];
  if (filters?.offset !== undefined) {
    const pageSize = filters?.limit ?? 20;
    rows = await db
      .select()
      .from(schema.source_documents)
      .where(where)
      .orderBy(orderBy)
      .limit(pageSize)
      .offset(filters.offset);
  } else if (filters?.limit) {
    rows = await db
      .select()
      .from(schema.source_documents)
      .where(where)
      .orderBy(orderBy)
      .limit(filters.limit);
  } else {
    rows = await db.select().from(schema.source_documents).where(where).orderBy(orderBy);
  }

  return { data: rows, count: total };
}

export async function getSourceDocument(id: string): Promise<SourceDocument | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.source_documents)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)));
  return row ?? null;
}

export async function getDocumentWithRelations(id: string) {
  const userId = await requireUserId();
  const db = getDb();

  const [docRows, jobs, drafts] = await Promise.all([
    db
      .select()
      .from(schema.source_documents)
      .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId))),
    db
      .select()
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.source_document_id, id),
          eq(schema.ingestion_jobs.user_id, userId),
        ),
      )
      .orderBy(desc(schema.ingestion_jobs.created_at)),
    db
      .select()
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.source_document_id, id),
          eq(schema.draft_records.user_id, userId),
        ),
      )
      .orderBy(desc(schema.draft_records.created_at)),
  ]);

  const document = docRows[0];
  if (!document) return null;
  return { document, jobs, drafts };
}

// Storage — inalterado nesta fase (Fase 3), continua via supabase-js.
export async function getDocumentStorageUrl(storagePath: string): Promise<string | null> {
  const supabase = await createClient();
  const { data } = await supabase.storage
    .from("ingestion-originals")
    .createSignedUrl(storagePath, 3600);
  return data?.signedUrl ?? null;
}

export async function reprocessDocument(documentId: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  // Bulk: 0 linhas afetadas é normal (nenhum draft para arquivar), sem throw.
  await db
    .update(schema.draft_records)
    .set({ status: "archived" })
    .where(
      and(
        eq(schema.draft_records.source_document_id, documentId),
        eq(schema.draft_records.user_id, userId),
        inArray(schema.draft_records.status, ["pending_review", "rejected"]),
      ),
    );

  const [latestJob] = await db
    .select({ id: schema.ingestion_jobs.id })
    .from(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, documentId),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at))
    .limit(1);

  if (latestJob) {
    // Fire-and-forget igual ao código atual: não checa linhas afetadas.
    await db
      .update(schema.ingestion_jobs)
      .set({
        status: "queued",
        error_message: null,
        error_details: null,
        retry_count: 0,
        updated_at: new Date().toISOString(),
      })
      .where(
        and(eq(schema.ingestion_jobs.id, latestJob.id), eq(schema.ingestion_jobs.user_id, userId)),
      );
  }
}

// ══════════════════════════════════════════════════════════════
// Ingestion Jobs
// ══════════════════════════════════════════════════════════════

export async function getIngestionJobs(filters?: {
  sourceDocumentId?: string;
  status?: string;
  limit?: number;
}): Promise<IngestionJob[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.ingestion_jobs.user_id, userId)];
  if (filters?.sourceDocumentId)
    conditions.push(eq(schema.ingestion_jobs.source_document_id, filters.sourceDocumentId));
  if (filters?.status) conditions.push(eq(schema.ingestion_jobs.status, filters.status));

  const base = db
    .select()
    .from(schema.ingestion_jobs)
    .where(and(...conditions))
    .orderBy(desc(schema.ingestion_jobs.created_at));
  return filters?.limit ? base.limit(filters.limit) : base;
}

export async function getIngestionJob(id: string): Promise<IngestionJob | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.ingestion_jobs)
    .where(and(eq(schema.ingestion_jobs.id, id), eq(schema.ingestion_jobs.user_id, userId)));
  return row ?? null;
}

export async function getLatestIngestionJobByDocumentId(
  sourceDocumentId: string,
): Promise<IngestionJob | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, sourceDocumentId),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at))
    .limit(1);
  return row ?? null;
}

// ══════════════════════════════════════════════════════════════
// Ingestion Runs
// ══════════════════════════════════════════════════════════════

export async function getIngestionRuns(limit = 20): Promise<IngestionRun[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.ingestion_runs)
    .where(eq(schema.ingestion_runs.user_id, userId))
    .orderBy(desc(schema.ingestion_runs.started_at))
    .limit(limit);
}

// ══════════════════════════════════════════════════════════════
// Draft Batches
// ══════════════════════════════════════════════════════════════

export async function getDraftBatches(filters?: {
  status?: string;
  limit?: number;
  source_document_id?: string;
}): Promise<DraftBatch[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.draft_batches.user_id, userId)];
  if (filters?.status) conditions.push(eq(schema.draft_batches.status, filters.status));
  if (filters?.source_document_id)
    conditions.push(eq(schema.draft_batches.source_document_id, filters.source_document_id));

  const base = db
    .select()
    .from(schema.draft_batches)
    .where(and(...conditions))
    .orderBy(desc(schema.draft_batches.created_at));
  return filters?.limit ? base.limit(filters.limit) : base;
}

export async function getDraftBatchByDocumentId(
  sourceDocumentId: string,
): Promise<DraftBatch | null> {
  const batches = await getDraftBatches({ source_document_id: sourceDocumentId, limit: 1 });
  if (batches[0]) return batches[0];

  // Fallback para batches antigos que não tinham source_document_id preenchido.
  const userId = await requireUserId();
  const db = getDb();
  const [latestDraft] = await db
    .select({ batch_id: schema.draft_records.batch_id })
    .from(schema.draft_records)
    .where(
      and(
        eq(schema.draft_records.source_document_id, sourceDocumentId),
        eq(schema.draft_records.user_id, userId),
        isNotNull(schema.draft_records.batch_id),
      ),
    )
    .orderBy(desc(schema.draft_records.created_at))
    .limit(1);

  if (!latestDraft?.batch_id) return null;
  return getDraftBatch(latestDraft.batch_id);
}

export async function getDraftBatch(id: string): Promise<DraftBatch | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.draft_batches)
    .where(and(eq(schema.draft_batches.id, id), eq(schema.draft_batches.user_id, userId)));
  return row ?? null;
}

// ══════════════════════════════════════════════════════════════
// Draft Records
// ══════════════════════════════════════════════════════════════

export async function getDraftRecords(filters?: {
  batchId?: string;
  status?: string;
  limit?: number;
}): Promise<DraftRecord[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.draft_records.user_id, userId)];
  if (filters?.batchId) conditions.push(eq(schema.draft_records.batch_id, filters.batchId));
  if (filters?.status) conditions.push(eq(schema.draft_records.status, filters.status));

  const base = db
    .select()
    .from(schema.draft_records)
    .where(and(...conditions))
    .orderBy(desc(schema.draft_records.created_at));
  return filters?.limit ? base.limit(filters.limit) : base;
}

/** Aprova um draft. NÃO cria registro financeiro. */
export async function approveDraftRecord(id: string): Promise<DraftRecord> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.draft_records)
    .set({ status: "approved", approved_at: new Date().toISOString(), approved_by: userId })
    .where(
      and(
        eq(schema.draft_records.id, id),
        eq(schema.draft_records.user_id, userId),
        inArray(schema.draft_records.status, ["pending_review", "corrected"]),
      ),
    )
    .returning();

  if (!row) throw new Error("Draft não encontrado");
  return row;
}

export async function updateDraftData(
  id: string,
  draftData: Record<string, unknown>,
): Promise<DraftRecord> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.draft_records)
    .set({
      draft_data: draftData,
      status: "corrected",
      corrections: { manual_edit: true, edited_at: new Date().toISOString() },
      updated_at: new Date().toISOString(),
    })
    .where(and(eq(schema.draft_records.id, id), eq(schema.draft_records.user_id, userId)))
    .returning();

  if (!row) throw new Error("Draft não encontrado");
  return row;
}

export async function rejectDraftRecord(id: string, reason?: string): Promise<DraftRecord> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.draft_records)
    .set({ status: "rejected", corrections: reason ? { rejection_reason: reason } : null })
    .where(and(eq(schema.draft_records.id, id), eq(schema.draft_records.user_id, userId)))
    .returning();

  if (!row) throw new Error("Draft não encontrado");
  return row;
}

/** Aprova todos os drafts pendentes de um batch. NÃO cria registros financeiros. */
export async function approveDraftBatch(batchId: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  // Bulk: 0 linhas é normal (nenhum draft pendente), sem throw.
  await db
    .update(schema.draft_records)
    .set({ status: "approved", approved_at: new Date().toISOString(), approved_by: userId })
    .where(
      and(
        eq(schema.draft_records.batch_id, batchId),
        eq(schema.draft_records.user_id, userId),
        eq(schema.draft_records.status, "pending_review"),
      ),
    );

  const result = await db
    .update(schema.draft_batches)
    .set({ status: "approved" })
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, userId)))
    .returning({ id: schema.draft_batches.id });

  if (result.length === 0) throw new Error("Batch não encontrado");
}

// ══════════════════════════════════════════════════════════════
// Ingestion Logs
// ══════════════════════════════════════════════════════════════

export async function getIngestionLogs(filters?: {
  level?: string;
  jobId?: string;
  runId?: string;
  limit?: number;
  offset?: number;
}): Promise<{ data: IngestionLog[]; count: number }> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.ingestion_logs.user_id, userId)];
  if (filters?.level) conditions.push(eq(schema.ingestion_logs.level, filters.level));
  if (filters?.jobId) conditions.push(eq(schema.ingestion_logs.job_id, filters.jobId));
  if (filters?.runId) conditions.push(eq(schema.ingestion_logs.run_id, filters.runId));

  const where = and(...conditions);
  const [{ value: total }] = await db
    .select({ value: count() })
    .from(schema.ingestion_logs)
    .where(where);
  const orderBy = desc(schema.ingestion_logs.created_at);

  let rows: IngestionLog[];
  if (filters?.offset !== undefined) {
    const pageSize = filters?.limit ?? 50;
    rows = await db
      .select()
      .from(schema.ingestion_logs)
      .where(where)
      .orderBy(orderBy)
      .limit(pageSize)
      .offset(filters.offset);
  } else if (filters?.limit) {
    rows = await db
      .select()
      .from(schema.ingestion_logs)
      .where(where)
      .orderBy(orderBy)
      .limit(filters.limit);
  } else {
    rows = await db.select().from(schema.ingestion_logs).where(where).orderBy(orderBy);
  }

  return { data: rows, count: total };
}

// ══════════════════════════════════════════════════════════════
// Ingestion Stats (para dashboard)
// ══════════════════════════════════════════════════════════════

export async function getIngestionStats(): Promise<{
  totalDocuments: number;
  pendingReview: number;
  totalDrafts: number;
  pendingDrafts: number;
  recentErrors: number;
}> {
  const userId = await requireUserId();
  const db = getDb();

  const since = new Date(Date.now() - 24 * 60 * 60 * 1000).toISOString();

  const [[docs], [pendingJobs], [drafts], [pendingDrafts], [errors]] = await Promise.all([
    db
      .select({ value: count() })
      .from(schema.source_documents)
      .where(eq(schema.source_documents.user_id, userId)),
    db
      .select({ value: count() })
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.user_id, userId),
          eq(schema.ingestion_jobs.status, "pending_review"),
        ),
      ),
    db
      .select({ value: count() })
      .from(schema.draft_records)
      .where(eq(schema.draft_records.user_id, userId)),
    db
      .select({ value: count() })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.user_id, userId),
          eq(schema.draft_records.status, "pending_review"),
        ),
      ),
    db
      .select({ value: count() })
      .from(schema.ingestion_logs)
      .where(
        and(
          eq(schema.ingestion_logs.user_id, userId),
          eq(schema.ingestion_logs.level, "error"),
          gte(schema.ingestion_logs.created_at, since),
        ),
      ),
  ]);

  return {
    totalDocuments: docs?.value ?? 0,
    pendingReview: pendingJobs?.value ?? 0,
    totalDrafts: drafts?.value ?? 0,
    pendingDrafts: pendingDrafts?.value ?? 0,
    recentErrors: errors?.value ?? 0,
  };
}

// ══════════════════════════════════════════════════════════════
// Upload Manual de Documento (S2-004)
// ══════════════════════════════════════════════════════════════

export async function uploadDocument(formData: FormData): Promise<SourceDocument> {
  const supabase = await createClient();
  const userId = await requireUserId();
  const db = getDb();

  const file = formData.get("file") as File | null;
  if (!file) throw new Error("Arquivo não informado");

  const fileBytes = new Uint8Array(await file.arrayBuffer());
  const contentHash = await computeContentHash(fileBytes);

  const [duplicateFingerprint] = await db
    .select({ source_document_id: schema.document_fingerprints.source_document_id })
    .from(schema.document_fingerprints)
    .where(
      and(
        eq(schema.document_fingerprints.user_id, userId),
        eq(schema.document_fingerprints.content_hash, contentHash),
      ),
    )
    .limit(1);

  if (duplicateFingerprint?.source_document_id) {
    throw new Error(
      `Documento duplicado detectado (mesmo hash). ID original: ${duplicateFingerprint.source_document_id}`,
    );
  }

  // Storage — inalterado nesta fase.
  const ext = file.name.split(".").pop() ?? "bin";
  const storagePath = `${userId}/manual/${crypto.randomUUID()}.${ext}`;
  const { error: uploadError } = await supabase.storage
    .from("ingestion-originals")
    .upload(storagePath, file, { contentType: file.type, upsert: false });
  if (uploadError) throw new Error(`Erro no upload: ${uploadError.message}`);

  const originKey = `manual::${storagePath}`;
  const [srcDoc] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: originKey,
      filename: file.name,
      mime_type: file.type || ext,
      file_size_bytes: file.size,
      storage_path: storagePath,
      content_hash: contentHash,
      status: "queued",
    })
    .returning();

  if (!srcDoc) throw new Error("Erro ao registrar documento");

  // Apêndice B: 3 inserts sequenciais sem transação — paridade com hoje.
  const [run] = await db
    .insert(schema.ingestion_runs)
    .values({
      user_id: userId,
      source_type: "manual_upload",
      status: "running",
      metadata: { source: "manual_upload", filename: file.name },
    })
    .returning();

  if (!run) throw new Error("Erro ao criar run");

  await db.insert(schema.ingestion_jobs).values({
    run_id: run.id,
    user_id: userId,
    source_document_id: srcDoc.id,
    status: "discovered",
    metadata: { source: "manual_upload" },
  });

  return srcDoc;
}

// ══════════════════════════════════════════════════════════════
// Deletar Documento
// ══════════════════════════════════════════════════════════════

export async function deleteSourceDocument(
  id: string,
  mode: DeleteSourceDocumentMode = "document_and_ingestion",
): Promise<void> {
  const supabase = await createClient();
  const userId = await requireUserId();
  const db = getDb();

  const [doc] = await db
    .select({
      id: schema.source_documents.id,
      storage_path: schema.source_documents.storage_path,
      metadata: schema.source_documents.metadata,
      status: schema.source_documents.status,
    })
    .from(schema.source_documents)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)));

  if (!doc) throw new Error("Documento não encontrado ou sem permissão");

  const now = new Date().toISOString();

  if (doc.storage_path) {
    const { error: storageError } = await supabase.storage
      .from("ingestion-originals")
      .remove([doc.storage_path]);
    throwIfError(storageError, "remover arquivo do storage");
  }

  if (mode === "document_only") {
    const metadata = {
      ...((doc.metadata as Record<string, unknown> | null) ?? {}),
      deletion: {
        mode,
        deleted_at: now,
        deleted_by: userId,
        original_status: doc.status,
        original_storage_path: doc.storage_path,
      },
    };

    const result = await db
      .update(schema.source_documents)
      .set({ status: "deleted", storage_path: null, updated_at: now, metadata })
      .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)))
      .returning({ id: schema.source_documents.id });

    if (result.length === 0) throw new Error("Documento não encontrado ou sem permissão");
    return;
  }

  const [jobs, parsedVersions] = await Promise.all([
    db
      .select({ id: schema.ingestion_jobs.id })
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.source_document_id, id),
          eq(schema.ingestion_jobs.user_id, userId),
        ),
      ),
    db
      .select({ id: schema.parsed_document_versions.id })
      .from(schema.parsed_document_versions)
      .where(
        and(
          eq(schema.parsed_document_versions.source_document_id, id),
          eq(schema.parsed_document_versions.user_id, userId),
        ),
      ),
  ]);

  const jobIds = jobs.map((job) => job.id);
  const parsedVersionIds = parsedVersions.map((version) => version.id);

  await db
    .delete(schema.draft_records)
    .where(
      and(
        eq(schema.draft_records.source_document_id, id),
        eq(schema.draft_records.user_id, userId),
      ),
    );
  await db
    .delete(schema.pattern_feedback)
    .where(
      and(
        eq(schema.pattern_feedback.source_document_id, id),
        eq(schema.pattern_feedback.user_id, userId),
      ),
    );

  if (parsedVersionIds.length > 0) {
    await db
      .delete(schema.extraction_results)
      .where(
        and(
          inArray(schema.extraction_results.parsed_version_id, parsedVersionIds),
          eq(schema.extraction_results.user_id, userId),
        ),
      );
  }

  await db
    .delete(schema.parsed_document_versions)
    .where(
      and(
        eq(schema.parsed_document_versions.source_document_id, id),
        eq(schema.parsed_document_versions.user_id, userId),
      ),
    );
  await db
    .delete(schema.draft_batches)
    .where(
      and(
        eq(schema.draft_batches.source_document_id, id),
        eq(schema.draft_batches.user_id, userId),
      ),
    );
  await db
    .delete(schema.document_fingerprints)
    .where(
      and(
        eq(schema.document_fingerprints.source_document_id, id),
        eq(schema.document_fingerprints.user_id, userId),
      ),
    );

  if (jobIds.length > 0) {
    await db
      .delete(schema.ingestion_logs)
      .where(
        and(
          inArray(schema.ingestion_logs.job_id, jobIds),
          eq(schema.ingestion_logs.user_id, userId),
        ),
      );
  }

  await db
    .delete(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, id),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    );

  // Não removemos ingestion_runs aqui (mesma nota do código atual).

  const result = await db
    .delete(schema.source_documents)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)))
    .returning({ id: schema.source_documents.id });

  if (result.length === 0) throw new Error("Documento não encontrado ou sem permissão");
}

// ══════════════════════════════════════════════════════════════
// Autopilot / IA
// ══════════════════════════════════════════════════════════════

export interface AutopilotResult {
  documentId: string;
  status: "complete" | "needs_review" | "failed";
  extraction: {
    supplier_name: string | null;
    total_amount: number | null;
    due_date: string | null;
    competence_date: string | null;
    financial_intent: string | null;
    document_type: string | null;
    ai_enrichment_type: string | null;
    confidence_per_field: Record<string, number> | null;
    reasoning: string | null;
  } | null;
  pipeline: {
    parser_used: string;
    ai_enriched: boolean;
    needs_full_ai_review: boolean;
    job_status: string | null;
  };
  drafts: { count: number; batch_id: string | null };
  issues: string[];
  decision_suggestion: "approve" | "review_required" | "reject";
}

export async function getAutopilotStatus(documentId: string): Promise<AutopilotResult> {
  const userId = await requireUserId();
  const db = getDb();

  const [doc] = await db
    .select({ id: schema.source_documents.id })
    .from(schema.source_documents)
    .where(
      and(eq(schema.source_documents.id, documentId), eq(schema.source_documents.user_id, userId)),
    );
  if (!doc) throw new Error("Documento não encontrado");

  const [job] = await db
    .select({
      id: schema.ingestion_jobs.id,
      status: schema.ingestion_jobs.status,
      metadata: schema.ingestion_jobs.metadata,
      needs_full_ai_review: schema.ingestion_jobs.needs_full_ai_review,
    })
    .from(schema.ingestion_jobs)
    .where(
      and(
        eq(schema.ingestion_jobs.source_document_id, documentId),
        eq(schema.ingestion_jobs.user_id, userId),
      ),
    )
    .orderBy(desc(schema.ingestion_jobs.created_at))
    .limit(1);

  const meta = (job?.metadata ?? {}) as Record<string, unknown>;
  const extractionResultId = meta.extraction_result_id as string | null;

  let extraction: AutopilotResult["extraction"] = null;
  if (extractionResultId) {
    const [er] = await db
      .select({
        supplier_name_raw: schema.extraction_results.supplier_name_raw,
        total_amount: schema.extraction_results.total_amount,
        due_date: schema.extraction_results.due_date,
        competence_date: schema.extraction_results.competence_date,
        financial_intent: schema.extraction_results.financial_intent,
        ai_enrichment_type: schema.extraction_results.ai_enrichment_type,
        confidence_per_field: schema.extraction_results.confidence_per_field,
        reasoning: schema.extraction_results.reasoning,
      })
      .from(schema.extraction_results)
      .where(
        and(
          eq(schema.extraction_results.id, extractionResultId),
          eq(schema.extraction_results.user_id, userId),
        ),
      );

    if (er) {
      extraction = {
        supplier_name: er.supplier_name_raw,
        total_amount: er.total_amount,
        due_date: er.due_date,
        competence_date: er.competence_date,
        financial_intent: er.financial_intent ?? null,
        document_type: (meta.document_type as string) ?? null,
        ai_enrichment_type: er.ai_enrichment_type ?? null,
        confidence_per_field: (er.confidence_per_field as Record<string, number>) ?? null,
        reasoning: er.reasoning ?? null,
      };
    }
  }

  const [draftBatch] = await db
    .select({ id: schema.draft_batches.id })
    .from(schema.draft_batches)
    .where(
      and(
        eq(schema.draft_batches.source_document_id, documentId),
        eq(schema.draft_batches.user_id, userId),
      ),
    )
    .limit(1);

  const batchId = draftBatch?.id ?? null;

  let draftCount = 0;
  if (batchId) {
    const [{ value }] = await db
      .select({ value: count() })
      .from(schema.draft_records)
      .where(
        and(eq(schema.draft_records.batch_id, batchId), eq(schema.draft_records.user_id, userId)),
      );
    draftCount = value;
  }

  const issues: string[] = [];
  if (!extraction?.total_amount) issues.push("Valor total não identificado");
  if (!extraction?.due_date) issues.push("Data de vencimento não identificada");
  if (!extraction?.supplier_name) issues.push("Fornecedor não identificado");
  if (job?.needs_full_ai_review) issues.push("Requer análise de IA completa (visão)");
  if (draftCount === 0) issues.push("Nenhum draft gerado para revisão");

  const needsFullReview = job?.needs_full_ai_review ?? false;
  const jobStatus = job?.status ?? null;
  const aiEnriched = (meta.ai_enriched as boolean) ?? false;

  const decisionSuggestion: AutopilotResult["decision_suggestion"] =
    issues.length === 0
      ? "approve"
      : issues.some((i) => i.includes("Valor") || i.includes("Nenhum draft"))
        ? "reject"
        : "review_required";

  return {
    documentId,
    status: issues.length === 0 ? "complete" : jobStatus === "failed" ? "failed" : "needs_review",
    extraction,
    pipeline: {
      parser_used: (meta.parser_type as string) ?? "unknown",
      ai_enriched: aiEnriched,
      needs_full_ai_review: needsFullReview,
      job_status: jobStatus,
    },
    drafts: { count: draftCount, batch_id: batchId },
    issues,
    decision_suggestion: decisionSuggestion,
  };
}

export async function triggerAiFullAnalysis(documentId: string): Promise<{ jobId: string }> {
  const userId = await requireUserId();
  const db = getDb();

  const [doc] = await db
    .select({ id: schema.source_documents.id })
    .from(schema.source_documents)
    .where(
      and(eq(schema.source_documents.id, documentId), eq(schema.source_documents.user_id, userId)),
    );
  if (!doc) throw new Error("Documento não encontrado");

  const [run] = await db
    .insert(schema.ingestion_runs)
    .values({
      user_id: userId,
      source_type: "manual_upload",
      status: "running",
      metadata: { triggered_by: "ai_full_request", document_id: documentId },
    })
    .returning({ id: schema.ingestion_runs.id });

  if (!run) throw new Error("Falha ao criar run para análise full");

  const [job] = await db
    .insert(schema.ingestion_jobs)
    .values({
      run_id: run.id,
      user_id: userId,
      source_document_id: documentId,
      status: "queued",
      metadata: { ai_mode: "full", force_reprocess: true, triggered_by: "user_request" },
    })
    .returning({ id: schema.ingestion_jobs.id });

  if (!job) throw new Error("Falha ao criar job de análise full");

  // Fire-and-forget igual ao código atual: não checa linhas afetadas.
  await db
    .update(schema.ingestion_runs)
    .set({ status: "completed" })
    .where(and(eq(schema.ingestion_runs.id, run.id), eq(schema.ingestion_runs.user_id, userId)));

  return { jobId: job.id };
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/ingestion.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/ingestion.ts apps/web/src/app/actions/ingestion.test.ts
git commit -m "feat(web): ingestion.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 9: `transactions.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/transactions.ts`
- Test: `apps/web/src/app/actions/transactions.test.ts` (novo)

**Interfaces:**

- Consome: `schema.transactions`, `schema.transaction_tags`, `schema.v_expenses_deduplicated` (Task 1).

**Nota:** `transaction_tags` não tem `user_id` próprio — a posse é validada pelo
update do registro pai (`transactions`) antes de mexer nas tags.

- [ ] **Step 1: Escrever os testes**

**Correção (achado CRÍTICO da revisão da Verônica-Bolso):**
`transactions.financial_product_id` é `NOT NULL` — a fixture original criava
transações sem esse campo. Corrigido com os helpers de `./__tests__/fixtures`.

```ts
// apps/web/src/app/actions/transactions.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  createTransaction,
  deleteTransaction,
  getTransaction,
  getTransactions,
  updateTransaction,
} from "./transactions";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedProductFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id);
  return { institutionId: institution.id, productId: product.id };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("transactions actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/transactions
    }
  });

  it("isola transações por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createTransaction({
      financial_product_id: seedA.productId,
      type: "expense",
      event_date: "2026-01-01",
      amount: 100,
    } as any);

    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createTransaction({
      financial_product_id: seedB.productId,
      type: "expense",
      event_date: "2026-01-01",
      amount: 200,
    } as any);

    currentUserId = USER_A;
    const listA = await getTransactions();
    expect(listA.map((t) => t.id)).toContain(a.id);
    expect(listA.map((t) => t.id)).not.toContain(b.id);
    expect(await getTransaction(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedProductFor(USER_A);
    institutionIds.push(seed.institutionId);
    const created = await createTransaction({
      financial_product_id: seed.productId,
      type: "expense",
      event_date: "2026-01-05",
      amount: 50,
    } as any);

    const updated = await updateTransaction(created.id, { amount: 75 } as any);
    expect(updated.amount).toBe(75);

    await deleteTransaction(created.id);
    expect(await getTransaction(created.id)).toBeNull();
  });

  it("lança erro ao excluir transação de outro usuário", async () => {
    currentUserId = USER_B;
    const seed = await seedProductFor(USER_B);
    institutionIds.push(seed.institutionId);
    const b = await createTransaction({
      financial_product_id: seed.productId,
      type: "expense",
      event_date: "2026-01-01",
      amount: 300,
    } as any);

    currentUserId = USER_A;
    await expect(deleteTransaction(b.id)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/transactions.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `transactions.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, gte, lte } from "drizzle-orm";

type Transaction = typeof schema.transactions.$inferSelect;
type TransactionInsert = Omit<typeof schema.transactions.$inferInsert, "user_id" | "id">;
type TransactionUpdate = Partial<TransactionInsert>;
type DeduplicatedExpense = typeof schema.v_expenses_deduplicated.$inferSelect;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getTransactions(filters?: {
  financialProductId?: string;
  financialPeriodId?: string;
  statementCycleId?: string;
  type?: string;
  startDate?: string;
  endDate?: string;
  limit?: number;
}): Promise<Transaction[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.transactions.user_id, userId)];
  if (filters?.financialProductId)
    conditions.push(eq(schema.transactions.financial_product_id, filters.financialProductId));
  if (filters?.financialPeriodId)
    conditions.push(eq(schema.transactions.financial_period_id, filters.financialPeriodId));
  if (filters?.statementCycleId)
    conditions.push(eq(schema.transactions.statement_cycle_id, filters.statementCycleId));
  if (filters?.type) conditions.push(eq(schema.transactions.type, filters.type));
  if (filters?.startDate) conditions.push(gte(schema.transactions.event_date, filters.startDate));
  if (filters?.endDate) conditions.push(lte(schema.transactions.event_date, filters.endDate));

  const base = db
    .select()
    .from(schema.transactions)
    .where(and(...conditions))
    .orderBy(desc(schema.transactions.event_date));
  return filters?.limit ? base.limit(filters.limit) : base;
}

export async function getTransaction(id: string): Promise<Transaction | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.transactions)
    .where(and(eq(schema.transactions.id, id), eq(schema.transactions.user_id, userId)));
  return row ?? null;
}

export async function createTransaction(
  input: TransactionInsert,
  tagIds?: string[],
): Promise<Transaction> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .insert(schema.transactions)
    .values({ ...input, user_id: userId })
    .returning();

  if (tagIds && tagIds.length > 0) {
    await db
      .insert(schema.transaction_tags)
      .values(tagIds.map((tag_id) => ({ transaction_id: row.id, tag_id })));
  }

  return row;
}

export async function updateTransaction(
  id: string,
  input: TransactionUpdate,
  tagIds?: string[],
): Promise<Transaction> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.transactions)
    .set(input)
    .where(and(eq(schema.transactions.id, id), eq(schema.transactions.user_id, userId)))
    .returning();

  if (!row) throw new Error("Transação não encontrada");

  if (tagIds !== undefined) {
    await db.delete(schema.transaction_tags).where(eq(schema.transaction_tags.transaction_id, id));
    if (tagIds.length > 0) {
      await db
        .insert(schema.transaction_tags)
        .values(tagIds.map((tag_id) => ({ transaction_id: id, tag_id })));
    }
  }

  return row;
}

export async function deleteTransaction(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.transactions)
    .where(and(eq(schema.transactions.id, id), eq(schema.transactions.user_id, userId)))
    .returning({ id: schema.transactions.id });

  if (result.length === 0) throw new Error("Transação não encontrada");
}

export async function getDeduplicatedExpenses(filters?: {
  financialPeriodId?: string;
  statementCycleId?: string;
  supplierId?: string;
  categoryId?: string;
}): Promise<DeduplicatedExpense[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.v_expenses_deduplicated.user_id, userId)];
  if (filters?.financialPeriodId)
    conditions.push(
      eq(schema.v_expenses_deduplicated.financial_period_id, filters.financialPeriodId),
    );
  if (filters?.statementCycleId)
    conditions.push(
      eq(schema.v_expenses_deduplicated.statement_cycle_id, filters.statementCycleId),
    );
  if (filters?.supplierId)
    conditions.push(eq(schema.v_expenses_deduplicated.supplier_id, filters.supplierId));
  if (filters?.categoryId)
    conditions.push(eq(schema.v_expenses_deduplicated.category_id, filters.categoryId));

  return db
    .select()
    .from(schema.v_expenses_deduplicated)
    .where(and(...conditions))
    .orderBy(desc(schema.v_expenses_deduplicated.event_date));
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/transactions.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/transactions.ts apps/web/src/app/actions/transactions.test.ts
git commit -m "feat(web): transactions.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 10: `liabilities.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/liabilities.ts`
- Test: `apps/web/src/app/actions/liabilities.test.ts` (novo)

**Interfaces:**

- Consome: `schema.liabilities`, `schema.liability_installments`, `schema.liability_tags` (Task 1).

- [ ] **Step 1: Escrever os testes**

**Correção (achado CRÍTICO da revisão da Verônica-Bolso, mais 3 colunas
`NOT NULL` que o rascunho original também deixou de fora):**
`liabilities.financial_product_id`, `type`, `original_amount` e
`outstanding_balance` são todos `NOT NULL` — a fixture original só informava
`name`/`status`. Corrigido com os helpers de `./__tests__/fixtures`.

```ts
// apps/web/src/app/actions/liabilities.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  createLiability,
  deleteLiability,
  getLiability,
  getLiabilities,
  updateLiability,
} from "./liabilities";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedProductFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id);
  return { institutionId: institution.id, productId: product.id };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("liabilities actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/liabilities/liability_installments
    }
  });

  it("isola dívidas por usuário", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createLiability({
      financial_product_id: seedA.productId,
      name: "Financiamento carro",
      type: "personal_loan",
      original_amount: 30000,
      outstanding_balance: 30000,
      status: "active",
    } as any);

    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createLiability({
      financial_product_id: seedB.productId,
      name: "Empréstimo",
      type: "personal_loan",
      original_amount: 5000,
      outstanding_balance: 5000,
      status: "active",
    } as any);

    currentUserId = USER_A;
    const listA = await getLiabilities();
    expect(listA.map((l) => l.id)).toContain(a.id);
    expect(listA.map((l) => l.id)).not.toContain(b.id);
    expect(await getLiability(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedProductFor(USER_A);
    institutionIds.push(seed.institutionId);
    const created = await createLiability({
      financial_product_id: seed.productId,
      name: "Cartão parcelado",
      type: "personal_loan",
      original_amount: 1000,
      outstanding_balance: 1000,
      status: "active",
    } as any);

    const updated = await updateLiability(created.id, { status: "paid_off" } as any);
    expect(updated.status).toBe("paid_off");

    await deleteLiability(created.id);
    expect(await getLiability(created.id)).toBeNull();
  });

  it("lança erro ao atualizar dívida de outro usuário", async () => {
    currentUserId = USER_B;
    const seed = await seedProductFor(USER_B);
    institutionIds.push(seed.institutionId);
    const b = await createLiability({
      financial_product_id: seed.productId,
      name: "Dívida B",
      type: "personal_loan",
      original_amount: 2000,
      outstanding_balance: 2000,
      status: "active",
    } as any);

    currentUserId = USER_A;
    await expect(updateLiability(b.id, { status: "paid_off" } as any)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/liabilities.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `liabilities.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

type Liability = typeof schema.liabilities.$inferSelect;
type LiabilityInsert = Omit<typeof schema.liabilities.$inferInsert, "user_id" | "id">;
type LiabilityUpdate = Partial<LiabilityInsert>;
type LiabilityInstallment = typeof schema.liability_installments.$inferSelect;
type InstallmentInsert = Omit<typeof schema.liability_installments.$inferInsert, "user_id" | "id">;
type InstallmentUpdate = Partial<InstallmentInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getLiabilities(filters?: {
  status?: string;
  financialProductId?: string;
}): Promise<Liability[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.liabilities.user_id, userId)];
  if (filters?.status) conditions.push(eq(schema.liabilities.status, filters.status));
  if (filters?.financialProductId)
    conditions.push(eq(schema.liabilities.financial_product_id, filters.financialProductId));

  return db
    .select()
    .from(schema.liabilities)
    .where(and(...conditions))
    .orderBy(asc(schema.liabilities.name));
}

export async function getLiability(id: string): Promise<Liability | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.liabilities)
    .where(and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, userId)));
  return row ?? null;
}

export async function createLiability(
  input: LiabilityInsert,
  tagIds?: string[],
): Promise<Liability> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .insert(schema.liabilities)
    .values({ ...input, user_id: userId })
    .returning();

  if (tagIds && tagIds.length > 0) {
    await db
      .insert(schema.liability_tags)
      .values(tagIds.map((tag_id) => ({ liability_id: row.id, tag_id })));
  }

  return row;
}

export async function updateLiability(
  id: string,
  input: LiabilityUpdate,
  tagIds?: string[],
): Promise<Liability> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.liabilities)
    .set(input)
    .where(and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, userId)))
    .returning();

  if (!row) throw new Error("Dívida não encontrada");

  if (tagIds !== undefined) {
    await db.delete(schema.liability_tags).where(eq(schema.liability_tags.liability_id, id));
    if (tagIds.length > 0) {
      await db
        .insert(schema.liability_tags)
        .values(tagIds.map((tag_id) => ({ liability_id: id, tag_id })));
    }
  }

  return row;
}

export async function deleteLiability(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.liabilities)
    .where(and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, userId)))
    .returning({ id: schema.liabilities.id });

  if (result.length === 0) throw new Error("Dívida não encontrada");
}

export async function getLiabilityInstallments(
  liabilityId: string,
  filters?: { status?: string },
): Promise<LiabilityInstallment[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [
    eq(schema.liability_installments.liability_id, liabilityId),
    eq(schema.liability_installments.user_id, userId),
  ];
  if (filters?.status) conditions.push(eq(schema.liability_installments.status, filters.status));

  return db
    .select()
    .from(schema.liability_installments)
    .where(and(...conditions))
    .orderBy(asc(schema.liability_installments.installment_number));
}

export async function createLiabilityInstallment(
  input: InstallmentInsert,
): Promise<LiabilityInstallment> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.liability_installments)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateLiabilityInstallment(
  id: string,
  input: InstallmentUpdate,
): Promise<LiabilityInstallment> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.liability_installments)
    .set(input)
    .where(
      and(
        eq(schema.liability_installments.id, id),
        eq(schema.liability_installments.user_id, userId),
      ),
    )
    .returning();
  if (!row) throw new Error("Parcela não encontrada");
  return row;
}

export async function deleteLiabilityInstallment(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .delete(schema.liability_installments)
    .where(
      and(
        eq(schema.liability_installments.id, id),
        eq(schema.liability_installments.user_id, userId),
      ),
    )
    .returning({ id: schema.liability_installments.id });
  if (result.length === 0) throw new Error("Parcela não encontrada");
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/liabilities.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/liabilities.ts apps/web/src/app/actions/liabilities.test.ts
git commit -m "feat(web): liabilities.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 11: `cards.ts` → Drizzle (com join relacional)

**Files:**

- Modify: `apps/web/src/app/actions/cards.ts`
- Test: `apps/web/src/app/actions/cards.test.ts` (novo)

**Interfaces:**

- Consome: `schema.cards`, `schema.financial_products`, `schema.institutions`, `schema.statement_cycles` (Task 1).

**Nota:** `getCards`/`getCard` fazem join (`cards` → `financial_products` →
`institutions`) que hoje o Supabase resolve via embed do PostgREST. Considerada
e rejeitada a API relacional do Drizzle (`db.query.cards.findMany({ with: ... })`)
porque a chave da relação (`financialProduct`, singular) não bate com a chave
que a UI consome hoje (`financial_products`, plural — convenção do embed
PostgREST); usar a API relacional trocaria silenciosamente o nome da chave.
Optamos por `leftJoin` manual + reshape em JS pra preservar o shape exato.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/cards.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import { createCard, deleteCard, getCard, getCards, updateCard } from "./cards";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("next/cache", () => ({ revalidatePath: () => {} }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedProductFor(userId: string) {
  const institution = await seedInstitution(userId);
  const product = await seedFinancialProduct(userId, institution.id, "credit_card");
  return { institutionId: institution.id, productId: product.id };
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("cards actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products/cards/statement_cycles
    }
  });

  it("isola cartões por usuário e resolve o produto/instituição", async () => {
    currentUserId = USER_A;
    const seedA = await seedProductFor(USER_A);
    institutionIds.push(seedA.institutionId);
    const a = await createCard({ financial_product_id: seedA.productId } as any);

    currentUserId = USER_B;
    const seedB = await seedProductFor(USER_B);
    institutionIds.push(seedB.institutionId);
    const b = await createCard({ financial_product_id: seedB.productId } as any);

    currentUserId = USER_A;
    const listA = await getCards();
    expect(listA.map((c) => c.id)).toContain(a.id);
    expect(listA.map((c) => c.id)).not.toContain(b.id);

    const found = await getCard(a.id);
    expect(found?.financial_products?.id).toBe(seedA.productId);
    expect(await getCard(b.id)).toBeNull();
  });

  it("cria, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const seed = await seedProductFor(USER_A);
    institutionIds.push(seed.institutionId);
    const created = await createCard({ financial_product_id: seed.productId } as any);

    const updated = await updateCard(created.id, { holder_name: "Chico" } as any);
    expect(updated.holder_name).toBe("Chico");

    await deleteCard(created.id);
    expect(await getCard(created.id)).toBeNull();
  });

  it("lança erro ao atualizar cartão de outro usuário", async () => {
    currentUserId = USER_B;
    const seed = await seedProductFor(USER_B);
    institutionIds.push(seed.institutionId);
    const b = await createCard({ financial_product_id: seed.productId } as any);

    currentUserId = USER_A;
    await expect(updateCard(b.id, { holder_name: "Hackeado" } as any)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/cards.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `cards.ts`**

```ts
"use server";

/**
 * Cartões de crédito — CRUD (issue #4).
 *
 * Até aqui, `cards` era a única entidade central sem nenhuma porta de entrada:
 * não havia action, não havia tela, e o único caminho para cadastrar um cartão
 * era `INSERT` manual no SQL. Como `statement_cycles.card_id` é `NOT NULL`, sem
 * cartão nenhuma fatura podia existir — e a jornada inteira de cartão, que é o
 * Marco 2, ficava barrada na primeira porta.
 */

import { revalidatePath } from "next/cache";
import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, count, desc, eq, inArray, ne } from "drizzle-orm";
import { createCardSchema, updateCardSchema } from "@sbf/validation";

type Card = typeof schema.cards.$inferSelect;
type FinancialProduct = typeof schema.financial_products.$inferSelect;

export type CardInput = Omit<typeof schema.cards.$inferInsert, "user_id" | "id">;
export type CardUpdate = Partial<CardInput>;

/** Cartão com o produto financeiro e a instituição já resolvidos. */
export interface CardComProduto extends Card {
  financial_products:
    | (Pick<FinancialProduct, "id" | "name" | "type"> & {
        institutions: { name: string } | null;
      })
    | null;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

function cardWithProductSelect() {
  const db = getDb();
  return db
    .select({
      card: schema.cards,
      financialProduct: {
        id: schema.financial_products.id,
        name: schema.financial_products.name,
        type: schema.financial_products.type,
      },
      institution: { name: schema.institutions.name },
    })
    .from(schema.cards)
    .leftJoin(
      schema.financial_products,
      eq(schema.cards.financial_product_id, schema.financial_products.id),
    )
    .leftJoin(
      schema.institutions,
      eq(schema.financial_products.institution_id, schema.institutions.id),
    );
}

function toCardComProduto(row: {
  card: Card;
  financialProduct: Pick<FinancialProduct, "id" | "name" | "type"> | null;
  institution: { name: string } | null;
}): CardComProduto {
  return {
    ...row.card,
    financial_products: row.financialProduct
      ? { ...row.financialProduct, institutions: row.institution }
      : null,
  };
}

export async function getCards(): Promise<CardComProduto[]> {
  const userId = await requireUserId();
  const rows = await cardWithProductSelect()
    .where(eq(schema.cards.user_id, userId))
    .orderBy(desc(schema.cards.is_primary), asc(schema.cards.created_at));
  return rows.map(toCardComProduto);
}

export async function getCard(id: string): Promise<CardComProduto | null> {
  const userId = await requireUserId();
  const [row] = await cardWithProductSelect().where(
    and(eq(schema.cards.id, id), eq(schema.cards.user_id, userId)),
  );
  return row ? toCardComProduto(row) : null;
}

/**
 * Produtos elegíveis a virar cartão.
 *
 * `credit_card` é o tipo esperado, mas cartões de débito também existem e
 * pendurar a lista num único tipo deixaria a tela vazia para quem cadastrou o
 * produto como conta corrente.
 */
export async function getProdutosParaCartao(): Promise<FinancialProduct[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.financial_products)
    .where(
      and(
        eq(schema.financial_products.user_id, userId),
        inArray(schema.financial_products.type, ["credit_card", "checking_account"]),
      ),
    )
    .orderBy(asc(schema.financial_products.name));
}

export async function createCard(input: CardInput): Promise<Card> {
  const parsed = createCardSchema.parse(input);
  const userId = await requireUserId();
  const db = getDb();

  // Só um cartão principal por vez. Fire-and-forget, igual ao código atual
  // (o Antes também não desestrutura `error` desta chamada).
  if (parsed.is_primary) {
    await db
      .update(schema.cards)
      .set({ is_primary: false })
      .where(eq(schema.cards.user_id, userId));
  }

  const [row] = await db
    .insert(schema.cards)
    .values({ ...parsed, user_id: userId })
    .returning();

  revalidatePath("/dashboard/cards");
  return row;
}

export async function updateCard(id: string, input: CardUpdate): Promise<Card> {
  const parsed = updateCardSchema.parse(input);
  const userId = await requireUserId();
  const db = getDb();

  if (parsed.is_primary) {
    await db
      .update(schema.cards)
      .set({ is_primary: false })
      .where(and(eq(schema.cards.user_id, userId), ne(schema.cards.id, id)));
  }

  const [row] = await db
    .update(schema.cards)
    .set(parsed)
    .where(and(eq(schema.cards.id, id), eq(schema.cards.user_id, userId)))
    .returning();

  if (!row) throw new Error("Cartão não encontrado");

  revalidatePath("/dashboard/cards");
  return row;
}

/**
 * Remove o cartão, recusando quando há fatura ligada a ele.
 */
export async function deleteCard(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const [{ value: faturas }] = await db
    .select({ value: count() })
    .from(schema.statement_cycles)
    .where(
      and(eq(schema.statement_cycles.card_id, id), eq(schema.statement_cycles.user_id, userId)),
    );

  if (faturas > 0) {
    throw new Error(
      `Este cartão tem ${faturas} fatura(s) registrada(s) e não pode ser excluído. ` +
        "Desative-o para parar de usá-lo sem perder o histórico.",
    );
  }

  const result = await db
    .delete(schema.cards)
    .where(and(eq(schema.cards.id, id), eq(schema.cards.user_id, userId)))
    .returning({ id: schema.cards.id });

  if (result.length === 0) throw new Error("Cartão não encontrado");

  revalidatePath("/dashboard/cards");
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/cards.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/cards.ts apps/web/src/app/actions/cards.test.ts
git commit -m "feat(web): cards.ts usa Drizzle/Neon com join manual e filtro user_id explicito"
```

---

### Task 12: `document-metadata.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/document-metadata.ts`
- Test: `apps/web/src/app/actions/document-metadata.test.ts` (novo)

**Interfaces:**

- Consome: `schema.source_documents` (Task 1).

**Nota (mudança de contrato a confirmar — sinalizada pela revisão):** o update
de hoje não checa linhas afetadas (sem `.single()`); por regra geral 3
(exceção documentada quando o Antes não checa nada — aqui o Antes SEMPRE
checava `error`, então a regra normal se aplica), o Depois passa a lançar erro
se `id` não existir/for de outro usuário. Hoje isso retorna sucesso
silenciosamente sem mudar nada. Este é o único arquivo onde a regra 3 muda um
comportamento de sucesso silencioso para erro — vale o time revisor prestar
atenção nesta task especificamente.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/document-metadata.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { updateDocumentMetadata } from "./document-metadata";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("document-metadata actions", () => {
  const docIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of docIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
  });

  async function seedDoc(userId: string) {
    const db = getDb();
    const [doc] = await db
      .insert(schema.source_documents)
      .values({
        user_id: userId,
        origin_type: "manual_upload",
        origin_key: `test::${crypto.randomUUID()}`,
        filename: "doc.pdf",
        status: "new",
      })
      .returning();
    docIds.push(doc.id);
    return doc;
  }

  it("atualiza metadados só do próprio documento", async () => {
    currentUserId = USER_A;
    const doc = await seedDoc(USER_A);

    await updateDocumentMetadata(doc.id, { supplier_name_raw: "Fornecedor X" });

    const db = getDb();
    const [updated] = await db
      .select()
      .from(schema.source_documents)
      .where(eq(schema.source_documents.id, doc.id));
    expect(updated.supplier_name_raw).toBe("Fornecedor X");
  });

  it("lança erro ao atualizar documento de outro usuário", async () => {
    currentUserId = USER_B;
    const docB = await seedDoc(USER_B);

    currentUserId = USER_A;
    await expect(
      updateDocumentMetadata(docB.id, { supplier_name_raw: "Hackeado" }),
    ).rejects.toThrow();
  });

  it("não faz nada (sem erro) se nenhum campo for informado", async () => {
    currentUserId = USER_A;
    const doc = await seedDoc(USER_A);
    await expect(updateDocumentMetadata(doc.id, {})).resolves.toBeUndefined();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/document-metadata.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `document-metadata.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, eq } from "drizzle-orm";
import type { Json } from "@sbf/shared-types";

/**
 * `jsonb` sem `.$type<T>()` no schema Drizzle infere `unknown`, não `Json` —
 * `Json` (de @sbf/shared-types, que continua existindo sem alteração) é
 * atribuível a `unknown`, então mantê-lo aqui na fronteira de entrada segue
 * impedindo valor não serializável, exatamente como antes.
 */
type SourceDocumentUpdate = Partial<
  Pick<
    typeof schema.source_documents.$inferInsert,
    "supplier_name_raw" | "metadata" | "document_type"
  >
>;

interface UpdateDocumentMetadataInput {
  supplier_name_raw?: string;
  metadata?: Json;
  document_type?: string;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function updateDocumentMetadata(
  id: string,
  input: UpdateDocumentMetadataInput,
): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const updatePayload: SourceDocumentUpdate = {};
  if (input.supplier_name_raw !== undefined)
    updatePayload.supplier_name_raw = input.supplier_name_raw;
  if (input.metadata !== undefined) updatePayload.metadata = input.metadata;
  if (input.document_type !== undefined) updatePayload.document_type = input.document_type;

  if (Object.keys(updatePayload).length === 0) return;

  const result = await db
    .update(schema.source_documents)
    .set(updatePayload)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)))
    .returning({ id: schema.source_documents.id });

  if (result.length === 0) throw new Error("Documento não encontrado");
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/document-metadata.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/document-metadata.ts apps/web/src/app/actions/document-metadata.test.ts
git commit -m "feat(web): document-metadata.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 13: `materialization.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/materialization.ts`
- Test: `apps/web/src/app/actions/materialization.test.ts` (novo)

**Interfaces:**

- Consome: `schema.draft_records`, `schema.draft_batches` (Task 1).
- Chama `fn_materialize_draft_record` via SQL cru (assinatura confirmada em
  `packages/db/schema.sql:405`: `p_user_id, p_draft_id, p_target_table,
p_insert_payload, p_actor`).

**Nota:** as duas funções públicas tratam "sem usuário" como objeto de falha
retornado (`MaterializationResult`/`MaterializationBatchResult`), não exceção —
por isso **não** usam o padrão `requireUserId()` que lança; extrair um helper
que lança quebraria o contrato que a UI espera.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/materialization.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { postApprovedDraftRecord } from "./materialization";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("materialization actions", () => {
  const draftIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of draftIds.splice(0)) {
      await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
    }
  });

  async function seedDraft(userId: string, status: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.draft_records)
      .values({
        user_id: userId,
        draft_type: "transaction",
        status,
        draft_data: { amount: 100 },
        materialization_key: `test::${crypto.randomUUID()}`,
      } as any)
      .returning();
    draftIds.push(row.id);
    return row;
  }

  it("recusa lançar draft de outro usuário (draft não encontrado)", async () => {
    currentUserId = USER_B;
    const draftB = await seedDraft(USER_B, "approved");

    currentUserId = USER_A;
    const result = await postApprovedDraftRecord(draftB.id);
    expect(result.success).toBe(false);
    expect(result.message).toContain("não encontrado");
  });

  it("recusa lançar draft ainda pending_review", async () => {
    currentUserId = USER_A;
    const draft = await seedDraft(USER_A, "pending_review");

    const result = await postApprovedDraftRecord(draft.id);
    expect(result.success).toBe(false);
    expect(result.message).toContain("Aprove primeiro");
  });

  it("recusa quando não autenticado", async () => {
    currentUserId = "";
    const result = await postApprovedDraftRecord("00000000-0000-0000-0000-000000000000");
    expect(result.success).toBe(false);
    expect(result.message).toBe("Não autenticado");
  });
});
```

(O caminho de sucesso real de `postApprovedDraftRecord` — que chama
`fn_materialize_draft_record` de verdade — depende de `TARGET_TABLES`/
`POSTABLE_SCHEMAS`/`TO_INSERT` de `@sbf/contracts` e de uma tabela de destino
real populada; é um teste de integração mais pesado, fora do escopo mínimo
desta task. Os 3 testes acima cobrem o que muda de fato nesta reescrita:
isolamento por usuário e os dois caminhos de recusa que não tocam a função
SQL.)

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/materialization.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `materialization.ts`**

```ts
"use server";

/**
 * Materialização — transforma drafts APROVADOS em registros financeiros reais.
 *
 * Regra fundamental: nenhum registro financeiro é criado antes da aprovação
 * humana. Aprovar e lançar são atos distintos (§9 do plano mestre), e é a
 * função `fn_materialize_draft_record` que faz valer a regra — não este
 * arquivo.
 */

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, eq, inArray, sql } from "drizzle-orm";
import {
  POSTABLE_SCHEMAS,
  TARGET_TABLES,
  TO_INSERT,
  formatIssues,
  parseDraftPayload,
  type DraftType,
} from "@sbf/contracts";
import type { Json } from "@sbf/shared-types";

export interface MaterializationResult {
  success: boolean;
  draftRecordId: string;
  postedRecordId: string | null;
  postedRecordType: string | null;
  validationErrors: string[];
  message: string;
}

export interface MaterializationBatchResult {
  batchId: string;
  totalProcessed: number;
  succeeded: number;
  failed: number;
  results: MaterializationResult[];
}

const POSTABLE_STATUSES = ["approved", "corrected"] as const;

function failure(
  draftRecordId: string,
  message: string,
  validationErrors: string[] = [],
): MaterializationResult {
  return {
    success: false,
    draftRecordId,
    postedRecordId: null,
    postedRecordType: null,
    validationErrors,
    message,
  };
}

export async function postApprovedDraftRecord(
  draftRecordId: string,
): Promise<MaterializationResult> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) return failure(draftRecordId, "Não autenticado");

  const db = getDb();

  const [draft] = await db
    .select()
    .from(schema.draft_records)
    .where(
      and(eq(schema.draft_records.id, draftRecordId), eq(schema.draft_records.user_id, user.id)),
    );

  if (!draft) {
    return failure(draftRecordId, "Draft não encontrado: inexistente");
  }

  if (draft.posted_record_id) {
    return {
      success: true,
      draftRecordId,
      postedRecordId: draft.posted_record_id,
      postedRecordType: draft.posted_record_type,
      validationErrors: [],
      message: "Draft já foi materializado anteriormente",
    };
  }

  if (!POSTABLE_STATUSES.includes(draft.status as (typeof POSTABLE_STATUSES)[number])) {
    return failure(
      draftRecordId,
      `Draft em status "${draft.status}" não pode ser lançado. Aprove primeiro.`,
    );
  }

  const draftType = draft.draft_type as DraftType;
  const postableSchema = POSTABLE_SCHEMAS[draftType];
  if (!postableSchema) {
    return failure(draftRecordId, `Tipo de draft desconhecido: ${draft.draft_type}`);
  }

  const parsed = parseDraftPayload({
    draft_type: draft.draft_type,
    draft_data: draft.draft_data,
    draft_schema_version: draft.draft_schema_version,
  });
  if (!parsed.ok) {
    return failure(draftRecordId, "Draft com dados inconsistentes", parsed.errors);
  }

  const postable = postableSchema.safeParse(parsed.payload);
  if (!postable.success) {
    return failure(
      draftRecordId,
      "Faltam dados obrigatórios para lançar",
      formatIssues(postable.error),
    );
  }

  const targetTable = TARGET_TABLES[draftType];
  const buildInsert = TO_INSERT[draftType] as unknown as (p: unknown) => Json;
  const insertPayload = buildInsert(postable.data);

  let rpcResult:
    { status: string; posted_record_id: string; posted_record_type: string } | undefined;
  try {
    const result = await db.execute<{
      status: string;
      posted_record_id: string;
      posted_record_type: string;
    }>(
      sql`SELECT * FROM fn_materialize_draft_record(
        ${user.id}::uuid,
        ${draftRecordId}::uuid,
        ${targetTable}::text,
        ${JSON.stringify(insertPayload)}::jsonb,
        ${"web"}::text
      )`,
    );
    rpcResult = result.rows[0];
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);

    await db
      .update(schema.draft_records)
      .set({ materialization_error: { message, at: new Date().toISOString() } })
      .where(
        and(eq(schema.draft_records.id, draftRecordId), eq(schema.draft_records.user_id, user.id)),
      );

    return failure(draftRecordId, `Falha ao lançar: ${message}`);
  }

  if (!rpcResult) {
    return failure(draftRecordId, "Falha ao lançar: função não retornou resultado");
  }

  return {
    success: true,
    draftRecordId,
    postedRecordId: rpcResult.posted_record_id,
    postedRecordType: rpcResult.posted_record_type,
    validationErrors: [],
    message:
      rpcResult.status === "already_posted"
        ? "Draft já foi materializado anteriormente"
        : "Lançamento criado",
  };
}

export async function postApprovedDraftBatch(batchId: string): Promise<MaterializationBatchResult> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();

  if (!user) {
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 1,
      results: [failure("", "Não autenticado")],
    };
  }

  const db = getDb();

  let drafts: { id: string }[];
  try {
    drafts = await db
      .select({ id: schema.draft_records.id })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.batch_id, batchId),
          eq(schema.draft_records.user_id, user.id),
          inArray(schema.draft_records.status, POSTABLE_STATUSES),
        ),
      );
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      batchId,
      totalProcessed: 0,
      succeeded: 0,
      failed: 1,
      results: [failure("", `Erro ao buscar drafts: ${message}`)],
    };
  }

  if (drafts.length === 0) {
    return { batchId, totalProcessed: 0, succeeded: 0, failed: 0, results: [] };
  }

  const results: MaterializationResult[] = [];
  for (const draft of drafts) {
    results.push(await postApprovedDraftRecord(draft.id));
  }

  const succeeded = results.filter((r) => r.success).length;
  const failed = results.length - succeeded;
  const batchStatus = failed === 0 ? "approved" : succeeded === 0 ? "rejected" : "partial";

  await db
    .update(schema.draft_batches)
    .set({
      status: batchStatus,
      approved_count: succeeded,
      rejected_count: failed,
      updated_at: new Date().toISOString(),
    })
    .where(and(eq(schema.draft_batches.id, batchId), eq(schema.draft_batches.user_id, user.id)));

  return { batchId, totalProcessed: drafts.length, succeeded, failed, results };
}

export interface ApproveAndPostResult {
  approved: boolean;
  approvalError: string | null;
  materialization: MaterializationResult | null;
}

export async function approveAndPostDraftRecord(
  draftRecordId: string,
): Promise<ApproveAndPostResult> {
  const { approveDraftRecord } = await import("./ingestion");

  try {
    await approveDraftRecord(draftRecordId);
  } catch (err) {
    return {
      approved: false,
      approvalError: err instanceof Error ? err.message : String(err),
      materialization: null,
    };
  }

  return {
    approved: true,
    approvalError: null,
    materialization: await postApprovedDraftRecord(draftRecordId),
  };
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/materialization.test.ts`
Expected: PASS (3 testes). **Confirmar num teste manual/integração separado**
(fora do escopo TDD mínimo desta task, mas antes de fechar a fase): que o
`sql\`\`\``de`fn_materialize_draft_record`bind a`jsonb`corretamente via`@neondatabase/serverless` — não foi validado contra um Neon real durante o
design, só contra a assinatura SQL.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/materialization.ts apps/web/src/app/actions/materialization.test.ts
git commit -m "feat(web): materialization.ts chama fn_materialize_draft_record via SQL cru"
```

---

### Task 14: `secrets.ts` → Drizzle (com `setSecret` bloqueada)

**Files:**

- Modify: `apps/web/src/app/actions/secrets.ts`
- Test: `apps/web/src/app/actions/secrets.test.ts` (novo)

**Interfaces:**

- Consome: `schema.user_secrets` (Task 1).

**Achado (discrepância com o Apêndice A da spec):** este arquivo NÃO chama
`fn_get_secrets`/`fn_mark_secret_used` (essas só são chamadas em
`workers/ingestion/src/parsers/secret-lookup.ts`, Fase 4b). Chama
`fn_set_secret`, que **não existe** em `packages/db/schema.sql` — uma das 6
funções descartadas por completo na migração (NOTA do schema.sql, linhas
117-138), por depender de `auth.uid()` sem receber `user_id`. Mesmo recriada,
depende de `private.get_crypto_key()`, também não portado. Ver Apêndice —
RPCs bloqueadas ao final deste plano. `setSecret` **permanece com o código
atual** (`supabase.rpc("fn_set_secret", ...)`), não convertida — mesmo
tratamento já aplicado às RPCs bloqueadas de `financial-periods.ts` (Task 3).
`listSecrets`/`deleteSecret` convertem normalmente.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/secrets.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { deleteSecret, listSecrets } from "./secrets";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("next/cache", () => ({ revalidatePath: () => {} }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("secrets actions", () => {
  const secretIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of secretIds.splice(0)) {
      await db.delete(schema.user_secrets).where(eq(schema.user_secrets.id, id));
    }
  });

  async function seedSecret(userId: string) {
    const db = getDb();
    const [row] = await db
      .insert(schema.user_secrets)
      .values({
        user_id: userId,
        secret_type: "pdf_password",
        encrypted_value: "fake-encrypted-value",
      } as any)
      .returning();
    secretIds.push(row.id);
    return row;
  }

  it("isola segredos por usuário (sem devolver o valor em claro)", async () => {
    currentUserId = USER_A;
    const a = await seedSecret(USER_A);
    currentUserId = USER_B;
    const b = await seedSecret(USER_B);

    currentUserId = USER_A;
    const listA = await listSecrets();
    expect(listA.map((s) => s.id)).toContain(a.id);
    expect(listA.map((s) => s.id)).not.toContain(b.id);
    expect(listA[0]).not.toHaveProperty("encryptedValue");
    expect(listA[0]).not.toHaveProperty("encrypted_value");
  });

  it("lança erro ao excluir segredo de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await seedSecret(USER_B);

    currentUserId = USER_A;
    await expect(deleteSecret(b.id)).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/secrets.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `secrets.ts`**

```ts
"use server";

/**
 * Gestão de senhas de documentos protegidos.
 *
 * O valor em claro nunca volta para o cliente. A listagem devolve apenas
 * escopo e rótulo — quem precisa da senha é o worker, via `fn_get_secrets`
 * (Fase 4b, fora deste arquivo), concedida somente a service_role.
 *
 * ATENÇÃO: `setSecret` abaixo chama `fn_set_secret`, que NÃO existe hoje em
 * packages/db/schema.sql — uma das seis funções descartadas por inteiro na
 * migração Supabase → Neon (NOTA no topo daquele arquivo), por depender de
 * auth.uid() sem receber user_id como parâmetro. Mesmo recriada com
 * p_user_id explícito, a criptografia (encrypt_secret/decrypt_secret) depende
 * de private.get_crypto_key(), também não portado. Ver Apêndice "RPCs
 * bloqueadas" do plano de implementação desta fase — setSecret permanece
 * intocada (ainda supabase.rpc()) até essa migração separada acontecer.
 */

import { createClient } from "@/lib/supabase/server";
import { revalidatePath } from "next/cache";
import { getDb, schema } from "@sbf/db";
import { and, eq, sql } from "drizzle-orm";

export interface SecretSummary {
  id: string;
  secretType: string;
  entityType: string | null;
  entityId: string | null;
  contractIdentifier: string | null;
  label: string | null;
  lastUsedAt: string | null;
  successCount: number;
  createdAt: string | null;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

/** Lista os segredos do usuário SEM os valores. */
export async function listSecrets(): Promise<SecretSummary[]> {
  const userId = await requireUserId();
  const db = getDb();

  // asc()/desc() do drizzle-orm não encadeiam .nullsLast() em orderBy() nesta
  // versão — fragmento sql`` cru replica "NULLS LAST" de hoje.
  const rows = await db
    .select({
      id: schema.user_secrets.id,
      secretType: schema.user_secrets.secret_type,
      entityType: schema.user_secrets.entity_type,
      entityId: schema.user_secrets.entity_id,
      contractIdentifier: schema.user_secrets.contract_identifier,
      label: schema.user_secrets.label,
      lastUsedAt: schema.user_secrets.last_used_at,
      successCount: schema.user_secrets.success_count,
      createdAt: schema.user_secrets.created_at,
    })
    .from(schema.user_secrets)
    .where(eq(schema.user_secrets.user_id, userId))
    .orderBy(sql`${schema.user_secrets.last_used_at} desc nulls last`);

  return rows.map((s) => ({ ...s, successCount: s.successCount ?? 0 }));
}

export interface SetSecretInput {
  plaintext: string;
  label?: string | null;
  entityType?:
    "supplier" | "financial_product" | "card" | "supplier_contract" | "institution" | null;
  entityId?: string | null;
  contractIdentifier?: string | null;
  secretType?: string;
}

/**
 * BLOQUEADA — NÃO CONVERTIDA. Ver comentário de cabeçalho do arquivo e o
 * Apêndice "RPCs bloqueadas" do plano. Permanece com o código Supabase atual.
 */
export async function setSecret(input: SetSecretInput): Promise<{ id: string }> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  if (!input.plaintext || input.plaintext.length === 0) {
    throw new Error("Informe a senha");
  }

  const { data, error } = await supabase.rpc("fn_set_secret", {
    p_secret_type: input.secretType ?? "pdf_password",
    p_plaintext: input.plaintext,
    p_entity_type: input.entityType ?? undefined,
    p_entity_id: input.entityId ?? undefined,
    p_contract_identifier: input.contractIdentifier ?? undefined,
    p_label: input.label ?? undefined,
  });

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/settings/passwords");
  return { id: data as unknown as string };
}

export async function deleteSecret(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.user_secrets)
    .where(and(eq(schema.user_secrets.id, id), eq(schema.user_secrets.user_id, userId)))
    .returning({ id: schema.user_secrets.id });

  if (result.length === 0) throw new Error("Senha não encontrada");

  revalidatePath("/dashboard/settings/passwords");
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/secrets.test.ts`
Expected: PASS (2 testes). `setSecret` não tem teste novo — continua exercida
(ou não) pelos testes/comportamento que já existiam antes desta fase, já que o
código dela não mudou.

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/secrets.ts apps/web/src/app/actions/secrets.test.ts
git commit -m "feat(web): secrets.ts usa Drizzle/Neon (setSecret permanece bloqueada, ver apendice)"
```

---

### Task 15: `patterns.ts` → Drizzle (com `registerPatternFeedback` bloqueada)

**Files:**

- Modify: `apps/web/src/app/actions/patterns.ts`
- Test: `apps/web/src/app/actions/patterns.test.ts` (novo)

**Interfaces:**

- Consome: `schema.document_patterns`, `schema.pattern_feedback` (Task 1).

**Achado:** `registerPatternFeedback` chama `register_pattern_feedback`, uma
das 6 funções descartadas por completo na migração (mesma nota de
`packages/db/schema.sql` das RPCs já bloqueadas nas Tasks 3/14) — decidia o
usuário via `auth.uid()`/`auth.role()` sem receber `p_user_id`. Permanece
intocada (ainda `supabase.rpc()`). Ver Apêndice — RPCs bloqueadas.

**Nota de tipos:** colunas `jsonb` sem `.$type<>()` no schema (`extraction_rules`,
`field_mappings`, `corrections`) inferem `unknown` no Drizzle — sobreposições
de tipo locais (`Omit<Row, "campo"> & { campo: Record<string, unknown> }`)
preservam o contrato estreito que o código já usa hoje.

**Candidato a Apêndice B (não decidido aqui):** `createPatternVersion` faz
`update` (desativa versão atual) seguido de `insert` (nova versão) sem
transação — pré-existente, não corrigido nesta task.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/patterns.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createPattern,
  createPatternVersion,
  deactivatePattern,
  getPattern,
  listPatterns,
  reactivatePattern,
  updatePattern,
} from "./patterns";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));
vi.mock("next/cache", () => ({ revalidatePath: () => {} }));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("patterns actions", () => {
  const patternIds: string[] = [];
  const feedbackIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of feedbackIds.splice(0)) {
      await db.delete(schema.pattern_feedback).where(eq(schema.pattern_feedback.id, id));
    }
    for (const id of patternIds.splice(0)) {
      await db.delete(schema.document_patterns).where(eq(schema.document_patterns.id, id));
    }
  });

  it("isola padrões por usuário", async () => {
    currentUserId = USER_A;
    const a = await createPattern({ name: "Padrão A", document_type: "boleto" });
    patternIds.push(a.id);
    currentUserId = USER_B;
    const b = await createPattern({ name: "Padrão B", document_type: "boleto" });
    patternIds.push(b.id);

    currentUserId = USER_A;
    const { data } = await listPatterns();
    expect(data.map((p) => p.id)).toContain(a.id);
    expect(data.map((p) => p.id)).not.toContain(b.id);

    const found = await getPattern(b.id);
    expect(found.pattern).toBeNull();
    expect(found.feedback).toEqual([]);
  });

  it("cria, busca (com feedback), atualiza, cria nova versão, desativa e reativa (caminho feliz)", async () => {
    currentUserId = USER_A;
    const created = await createPattern({
      name: "Cemig",
      document_type: "utility_bill",
      confidence_threshold: 0.9,
    });
    patternIds.push(created.id);

    const db = getDb();
    const [feedback] = await db
      .insert(schema.pattern_feedback)
      .values({
        user_id: USER_A,
        pattern_id: created.id,
        feedback_type: "correct",
        corrections: {},
      } as any)
      .returning();
    feedbackIds.push(feedback.id);

    const fetched = await getPattern(created.id);
    expect(fetched.pattern?.name).toBe("Cemig");
    expect(fetched.feedback.map((f) => f.id)).toContain(feedback.id);

    const updated = await updatePattern(created.id, { name: "Cemig v2" });
    expect(updated.name).toBe("Cemig v2");

    const versioned = await createPatternVersion(created.id, {
      field_mappings: { amount: "valor" },
    });
    patternIds.push(versioned.id);
    expect(versioned.version).toBe(2);
    expect(versioned.is_active).toBe(true);
    expect((await getPattern(created.id)).pattern?.is_active).toBe(false);

    await deactivatePattern(versioned.id);
    expect((await getPattern(versioned.id)).pattern?.is_active).toBe(false);

    await reactivatePattern(versioned.id);
    expect((await getPattern(versioned.id)).pattern?.is_active).toBe(true);
  });

  it("lança erro ao atualizar padrão inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updatePattern("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("lança 'Access denied' ao criar nova versão de padrão de outro usuário", async () => {
    currentUserId = USER_B;
    const b = await createPattern({ name: "Padrão B2", document_type: "boleto" });
    patternIds.push(b.id);

    currentUserId = USER_A;
    await expect(createPatternVersion(b.id, {})).rejects.toThrow("Access denied");
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/patterns.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `patterns.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { revalidatePath } from "next/cache";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, ilike, sql } from "drizzle-orm";

type DocumentPatternRow = typeof schema.document_patterns.$inferSelect;
export type DocumentPattern = Omit<DocumentPatternRow, "extraction_rules" | "field_mappings"> & {
  extraction_rules: Record<string, unknown>;
  field_mappings: Record<string, unknown>;
};

type PatternFeedbackRow = typeof schema.pattern_feedback.$inferSelect;
export type PatternFeedback = Omit<PatternFeedbackRow, "corrections"> & {
  corrections: Record<string, unknown>;
};

export type CreatePatternInput = {
  name: string;
  document_type: string;
  supplier_id?: string | null;
  institution_id?: string | null;
  extraction_rules?: Record<string, unknown>;
  field_mappings?: Record<string, unknown>;
  sample_fingerprints?: string[];
  confidence_threshold?: number;
};

export type UpdatePatternInput = Partial<
  Pick<
    DocumentPattern,
    | "name"
    | "extraction_rules"
    | "field_mappings"
    | "sample_fingerprints"
    | "confidence_threshold"
    | "supplier_id"
    | "institution_id"
  >
>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Unauthorized");
  return user.id;
}

export async function listPatterns(filters?: {
  document_type?: string;
  supplier_id?: string;
  institution_id?: string;
  is_active?: boolean;
  search?: string;
  limit?: number;
  offset?: number;
}): Promise<{ data: DocumentPattern[]; count: number }> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.document_patterns.user_id, userId)];
  if (filters?.document_type)
    conditions.push(eq(schema.document_patterns.document_type, filters.document_type));
  if (filters?.supplier_id)
    conditions.push(eq(schema.document_patterns.supplier_id, filters.supplier_id));
  if (filters?.institution_id)
    conditions.push(eq(schema.document_patterns.institution_id, filters.institution_id));
  if (filters?.is_active !== undefined)
    conditions.push(eq(schema.document_patterns.is_active, filters.is_active));
  if (filters?.search) conditions.push(ilike(schema.document_patterns.name, `%${filters.search}%`));

  const [{ count }] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(schema.document_patterns)
    .where(and(...conditions));

  const base = db
    .select()
    .from(schema.document_patterns)
    .where(and(...conditions))
    .orderBy(desc(schema.document_patterns.updated_at));

  const data =
    filters?.offset !== undefined
      ? await base.limit(filters.limit ?? 20).offset(filters.offset)
      : filters?.limit
        ? await base.limit(filters.limit)
        : await base;

  return { data: data as DocumentPattern[], count };
}

export async function getPattern(id: string): Promise<{
  pattern: DocumentPattern | null;
  feedback: PatternFeedback[];
}> {
  const userId = await requireUserId();
  const db = getDb();

  const [pattern, feedback] = await Promise.all([
    db
      .select()
      .from(schema.document_patterns)
      .where(
        and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)),
      ),
    db
      .select()
      .from(schema.pattern_feedback)
      .where(
        and(
          eq(schema.pattern_feedback.pattern_id, id),
          eq(schema.pattern_feedback.user_id, userId),
        ),
      )
      .orderBy(desc(schema.pattern_feedback.created_at))
      .limit(50),
  ]);

  return {
    pattern: (pattern[0] as DocumentPattern) ?? null,
    feedback: feedback as PatternFeedback[],
  };
}

export async function createPattern(input: CreatePatternInput): Promise<DocumentPattern> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.document_patterns)
    .values({
      user_id: userId,
      name: input.name,
      document_type: input.document_type,
      supplier_id: input.supplier_id ?? null,
      institution_id: input.institution_id ?? null,
      extraction_rules: input.extraction_rules ?? {},
      field_mappings: input.field_mappings ?? {},
      sample_fingerprints: input.sample_fingerprints ?? [],
      confidence_threshold: input.confidence_threshold ?? 0.8,
    })
    .returning();

  revalidatePath("/dashboard/ingestion/patterns");
  return row as DocumentPattern;
}

export async function updatePattern(
  id: string,
  input: UpdatePatternInput,
): Promise<DocumentPattern> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.document_patterns)
    .set({ ...input, updated_at: new Date().toISOString() })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)))
    .returning();

  if (!row) throw new Error("Padrão não encontrado");

  revalidatePath("/dashboard/ingestion/patterns");
  revalidatePath(`/dashboard/ingestion/patterns/${id}`);
  return row as DocumentPattern;
}

/**
 * Candidato a Apêndice B (não decidido aqui): update seguido de insert sem
 * transação — comportamento pré-existente no Supabase original.
 */
export async function createPatternVersion(
  id: string,
  updates: Pick<UpdatePatternInput, "extraction_rules" | "field_mappings">,
): Promise<DocumentPattern> {
  const userId = await requireUserId();
  const db = getDb();

  // Nota: a busca original não filtra por user_id na query — a checagem de
  // posse é feita depois, em código de aplicação ("Access denied"). Mantido
  // assim pra não perder a distinção entre "não encontrado" e "de outro usuário".
  const [current] = await db
    .select()
    .from(schema.document_patterns)
    .where(eq(schema.document_patterns.id, id));
  if (!current) throw new Error("Padrão não encontrado");
  if (current.user_id !== userId) throw new Error("Access denied");

  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  await db
    .update(schema.document_patterns)
    .set({ is_active: false })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)));

  const [newVersion] = await db
    .insert(schema.document_patterns)
    .values({
      user_id: userId,
      name: current.name,
      supplier_id: current.supplier_id,
      document_type: current.document_type,
      institution_id: current.institution_id,
      extraction_rules: (updates.extraction_rules ?? current.extraction_rules) as Record<
        string,
        unknown
      >,
      field_mappings: (updates.field_mappings ?? current.field_mappings) as Record<string, unknown>,
      sample_fingerprints: current.sample_fingerprints,
      confidence_threshold: current.confidence_threshold,
      version: current.version + 1,
      is_active: true,
    })
    .returning();

  revalidatePath("/dashboard/ingestion/patterns");
  revalidatePath(`/dashboard/ingestion/patterns/${id}`);
  return newVersion as DocumentPattern;
}

export async function deactivatePattern(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.document_patterns)
    .set({ is_active: false, updated_at: new Date().toISOString() })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)));
  revalidatePath("/dashboard/ingestion/patterns");
}

export async function reactivatePattern(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.document_patterns)
    .set({ is_active: true, updated_at: new Date().toISOString() })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)));
  revalidatePath("/dashboard/ingestion/patterns");
}

/**
 * BLOQUEADA — NÃO CONVERTIDA. `register_pattern_feedback` foi descartada por
 * inteiro na migração (packages/db/schema.sql, NOTA de gaps). Ver Apêndice
 * "RPCs bloqueadas". Permanece com o código Supabase atual.
 */
export async function registerPatternFeedback(input: {
  pattern_id: string;
  source_document_id?: string | null;
  feedback_type: "correct" | "incorrect" | "partial" | "improved";
  corrections?: Record<string, unknown>;
}): Promise<PatternFeedback> {
  const supabase = await createClient();
  const { data, error } = await supabase.rpc("register_pattern_feedback", {
    p_pattern_id: input.pattern_id,
    p_source_document_id: input.source_document_id ?? null,
    p_feedback_type: input.feedback_type,
    p_corrections: input.corrections ?? {},
  });

  if (error) throw new Error(error.message);
  revalidatePath(`/dashboard/ingestion/patterns/${input.pattern_id}`);
  return data;
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/patterns.test.ts`
Expected: PASS (4 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/patterns.ts apps/web/src/app/actions/patterns.test.ts
git commit -m "feat(web): patterns.ts usa Drizzle/Neon (registerPatternFeedback bloqueada)"
```

---

### Task 16: `document-splits.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/document-splits.ts`
- Test: `apps/web/src/app/actions/document-splits.test.ts` (novo)

**Interfaces:**

- Consome: `schema.document_splits`, `schema.source_documents` (Task 1).

**Nota:** `document_splits_source_document_id_fkey` é `ON DELETE CASCADE` —
apagar o `source_document` de teste já limpa os splits associados.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/document-splits.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createDocumentSplit,
  deleteDocumentSplit,
  getDocumentSplitsTotal,
  listDocumentSplits,
} from "./document-splits";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedSourceDocument(userId: string, amount?: number) {
  const db = getDb();
  const [row] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: `test-${crypto.randomUUID()}`,
      filename: "documento-teste.pdf",
      metadata: amount !== undefined ? { amount } : null,
    } as any)
    .returning();
  return row;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("document-splits actions", () => {
  const sourceDocumentIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of sourceDocumentIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
  });

  it("isola rateios por usuário mesmo quando o id do documento é conhecido", async () => {
    currentUserId = USER_A;
    const docA = await seedSourceDocument(USER_A, 100);
    sourceDocumentIds.push(docA.id);
    await createDocumentSplit({ source_document_id: docA.id, amount: 40 });

    currentUserId = USER_B;
    const docB = await seedSourceDocument(USER_B, 100);
    sourceDocumentIds.push(docB.id);
    await createDocumentSplit({ source_document_id: docB.id, amount: 40 });

    currentUserId = USER_A;
    expect(await listDocumentSplits(docB.id)).toEqual([]);
  });

  it("cria, lista, soma e exclui rateios (caminho feliz)", async () => {
    currentUserId = USER_A;
    const doc = await seedSourceDocument(USER_A, 100);
    sourceDocumentIds.push(doc.id);

    const split1 = await createDocumentSplit({
      source_document_id: doc.id,
      amount: 40,
      description: "Parte 1",
    });
    await createDocumentSplit({ source_document_id: doc.id, amount: 30, description: "Parte 2" });

    expect((await listDocumentSplits(doc.id)).length).toBe(2);
    expect(await getDocumentSplitsTotal(doc.id)).toBe(70);

    await deleteDocumentSplit(split1.id);
    expect(await getDocumentSplitsTotal(doc.id)).toBe(30);
  });

  it("lança erro quando a soma dos rateios excede o valor do documento", async () => {
    currentUserId = USER_A;
    const doc = await seedSourceDocument(USER_A, 50);
    sourceDocumentIds.push(doc.id);

    await expect(createDocumentSplit({ source_document_id: doc.id, amount: 60 })).rejects.toThrow(
      /excede o valor do documento/,
    );
  });

  it("não exclui (nem lança erro) rateio de outro usuário — comportamento preservado", async () => {
    currentUserId = USER_B;
    const docB = await seedSourceDocument(USER_B, 100);
    sourceDocumentIds.push(docB.id);
    const splitB = await createDocumentSplit({ source_document_id: docB.id, amount: 20 });

    currentUserId = USER_A;
    await deleteDocumentSplit(splitB.id); // não deve lançar

    const db = getDb();
    const [stillThere] = await db
      .select()
      .from(schema.document_splits)
      .where(eq(schema.document_splits.id, splitB.id));
    expect(stillThere).toBeDefined();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/document-splits.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `document-splits.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

export type DocumentSplit = typeof schema.document_splits.$inferSelect;

export interface DocumentSplitInsert {
  source_document_id: string;
  category_id?: string | null;
  tags?: string[];
  amount: number;
  description?: string | null;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function listDocumentSplits(sourceDocumentId: string): Promise<DocumentSplit[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.document_splits)
    .where(
      and(
        eq(schema.document_splits.source_document_id, sourceDocumentId),
        eq(schema.document_splits.user_id, userId),
      ),
    )
    .orderBy(asc(schema.document_splits.created_at));
}

export async function createDocumentSplit(input: DocumentSplitInsert): Promise<DocumentSplit> {
  const userId = await requireUserId();
  const db = getDb();

  // Validação de soma server-side (além do trigger DB fn_validate_splits_sum).
  const existing = await db
    .select({ amount: schema.document_splits.amount })
    .from(schema.document_splits)
    .where(
      and(
        eq(schema.document_splits.source_document_id, input.source_document_id),
        eq(schema.document_splits.user_id, userId),
      ),
    );
  const currentSum = existing.reduce((acc, s) => acc + s.amount, 0);

  // Nota: a busca original não checa `error` — se o documento não existir/não
  // pertencer ao usuário, a validação de soma é pulada em vez de lançar erro.
  // Comportamento preservado (exceção documentada).
  const [doc] = await db
    .select({ metadata: schema.source_documents.metadata })
    .from(schema.source_documents)
    .where(
      and(
        eq(schema.source_documents.id, input.source_document_id),
        eq(schema.source_documents.user_id, userId),
      ),
    );

  const docAmount =
    doc?.metadata && typeof doc.metadata === "object" && "amount" in doc.metadata
      ? Number((doc.metadata as Record<string, unknown>).amount)
      : null;

  if (docAmount !== null && currentSum + input.amount > docAmount) {
    throw new Error(
      `Soma dos rateios (${(currentSum + input.amount).toFixed(2)}) excede o valor do documento (${docAmount.toFixed(2)})`,
    );
  }

  const [row] = await db
    .insert(schema.document_splits)
    .values({ ...input, user_id: userId, tags: input.tags ?? [] })
    .returning();

  return row;
}

export async function deleteDocumentSplit(splitId: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.document_splits)
    .where(and(eq(schema.document_splits.id, splitId), eq(schema.document_splits.user_id, userId)));
}

export async function getDocumentSplitsTotal(sourceDocumentId: string): Promise<number> {
  const userId = await requireUserId();
  const db = getDb();
  const rows = await db
    .select({ amount: schema.document_splits.amount })
    .from(schema.document_splits)
    .where(
      and(
        eq(schema.document_splits.source_document_id, sourceDocumentId),
        eq(schema.document_splits.user_id, userId),
      ),
    );
  return rows.reduce((acc, s) => acc + s.amount, 0);
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/document-splits.test.ts`
Expected: PASS (4 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/document-splits.ts apps/web/src/app/actions/document-splits.test.ts
git commit -m "feat(web): document-splits.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 17: `financial-products.ts` → Drizzle

**Files:**

- Modify: `apps/web/src/app/actions/financial-products.ts`
- Test: `apps/web/src/app/actions/financial-products.test.ts` (novo)

**Interfaces:**

- Consome: `schema.financial_products` (Task 1).

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/financial-products.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedInstitution } from "./__tests__/fixtures";
import {
  createFinancialProduct,
  deleteFinancialProduct,
  getFinancialProduct,
  getFinancialProducts,
  updateFinancialProduct,
} from "./financial-products";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("financial-products actions", () => {
  const institutionIds: string[] = [];

  afterEach(async () => {
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id); // CASCADE remove financial_products
    }
  });

  it("isola produtos financeiros por usuário", async () => {
    currentUserId = USER_A;
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const a = await createFinancialProduct({
      institution_id: instA.id,
      name: "Conta A",
      type: "checking_account",
    });

    currentUserId = USER_B;
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);
    const b = await createFinancialProduct({
      institution_id: instB.id,
      name: "Conta B",
      type: "checking_account",
    });

    currentUserId = USER_A;
    const listA = await getFinancialProducts();
    expect(listA.map((p) => p.id)).toContain(a.id);
    expect(listA.map((p) => p.id)).not.toContain(b.id);
    expect(await getFinancialProduct(b.id)).toBeNull();
  });

  it("cria, busca, atualiza e exclui (caminho feliz)", async () => {
    currentUserId = USER_A;
    const inst = await seedInstitution(USER_A);
    institutionIds.push(inst.id);

    const created = await createFinancialProduct({
      institution_id: inst.id,
      name: "Cartão Nubank",
      type: "credit_card",
      credit_limit: 5000,
    });

    const fetched = await getFinancialProduct(created.id);
    expect(fetched?.name).toBe("Cartão Nubank");
    expect(fetched?.credit_limit).toBe(5000); // number, não string — Task 1 (mode: "number")

    const updated = await updateFinancialProduct(created.id, { name: "Cartão Nubank Platinum" });
    expect(updated.name).toBe("Cartão Nubank Platinum");

    await deleteFinancialProduct(created.id);
    expect(await getFinancialProduct(created.id)).toBeNull();
  });

  it("lança erro ao atualizar produto inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateFinancialProduct("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("não exclui (nem lança erro) produto de outro usuário — comportamento preservado", async () => {
    currentUserId = USER_B;
    const inst = await seedInstitution(USER_B);
    institutionIds.push(inst.id);
    const product = await createFinancialProduct({
      institution_id: inst.id,
      name: "Conta B2",
      type: "checking_account",
    });

    currentUserId = USER_A;
    await deleteFinancialProduct(product.id); // não deve lançar, e não deve remover a linha de B

    const db = getDb();
    const [row] = await db
      .select()
      .from(schema.financial_products)
      .where(eq(schema.financial_products.id, product.id));
    expect(row).toBeDefined();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/financial-products.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `financial-products.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

type FinancialProduct = typeof schema.financial_products.$inferSelect;
type FinancialProductInsert = Omit<typeof schema.financial_products.$inferInsert, "user_id" | "id">;
type FinancialProductUpdate = Partial<FinancialProductInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getFinancialProducts(institutionId?: string): Promise<FinancialProduct[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.financial_products.user_id, userId)];
  if (institutionId) conditions.push(eq(schema.financial_products.institution_id, institutionId));
  return db
    .select()
    .from(schema.financial_products)
    .where(and(...conditions))
    .orderBy(asc(schema.financial_products.display_order), asc(schema.financial_products.name));
}

export async function getFinancialProduct(id: string): Promise<FinancialProduct | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.financial_products)
    .where(
      and(eq(schema.financial_products.id, id), eq(schema.financial_products.user_id, userId)),
    );
  return row ?? null;
}

export async function createFinancialProduct(
  input: FinancialProductInsert,
): Promise<FinancialProduct> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.financial_products)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateFinancialProduct(
  id: string,
  input: FinancialProductUpdate,
): Promise<FinancialProduct> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.financial_products)
    .set(input)
    .where(and(eq(schema.financial_products.id, id), eq(schema.financial_products.user_id, userId)))
    .returning();
  if (!row) throw new Error("Produto financeiro não encontrado");
  return row;
}

export async function deleteFinancialProduct(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.financial_products)
    .where(
      and(eq(schema.financial_products.id, id), eq(schema.financial_products.user_id, userId)),
    );
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/financial-products.test.ts`
Expected: PASS (4 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/app/actions/financial-products.ts apps/web/src/app/actions/financial-products.test.ts
git commit -m "feat(web): financial-products.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

### Task 18: `suppliers.ts` → Drizzle (+ migração de `suppliers/[id]/page.tsx`, `searchSuppliers` bloqueada)

**Files:**

- Modify: `apps/web/src/app/actions/suppliers.ts`
- Modify: `apps/web/src/app/dashboard/suppliers/[id]/page.tsx` (troca chamadas Supabase por Server Actions)
- Test: `apps/web/src/app/actions/suppliers.test.ts` (novo)

**Interfaces:**

- Consome: `schema.suppliers`, `schema.supplier_aliases`, `schema.supplier_contracts`, `schema.source_documents` (Task 1).
- Produz: `getSupplierDetail(id)`, `updateSupplierActive(id, isActive)` — novas, consumidas pelo componente client-side migrado.

**Achado:** `searchSuppliers` chama `search_suppliers`, descartada por inteiro
na migração (mesma nota das outras RPCs bloqueadas). Permanece intocada. Ver
Apêndice — RPCs bloqueadas.

**Achado de schema:** `source_documents.supplier_id` é `ON DELETE SET NULL`
(diferente de `supplier_aliases`/`supplier_contracts`, que são `CASCADE` em
`supplier_id`) — o teste precisa limpar `source_documents` de teste
explicitamente, apagar o `supplier` não basta.

**Decisão de julgamento sinalizada para revisão:** `updateSupplierActive` é
função nova (sem Server Action Supabase anterior — só existia como código
client-side inline que não checava linhas afetadas). Aplica a regra padrão da
fase (lança erro em 0 linhas) em vez de replicar o silêncio do código antigo,
já que não há contrato de Server Action prévio a preservar.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/suppliers.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  createSupplier,
  createSupplierAlias,
  createSupplierContract,
  deleteSupplierAlias,
  deleteSupplierContract,
  getSupplier,
  getSupplierDetail,
  getSuppliers,
  updateSupplier,
  updateSupplierActive,
} from "./suppliers";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("suppliers actions", () => {
  const supplierIds: string[] = [];
  const sourceDocumentIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of sourceDocumentIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
    for (const id of supplierIds.splice(0)) {
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    }
  });

  it("isola fornecedores por usuário", async () => {
    currentUserId = USER_A;
    const a = await createSupplier({ name: "Fornecedor A", type: "company" });
    supplierIds.push(a.id);
    currentUserId = USER_B;
    const b = await createSupplier({ name: "Fornecedor B", type: "company" });
    supplierIds.push(b.id);

    currentUserId = USER_A;
    const listA = await getSuppliers(false);
    expect(listA.map((s) => s.id)).toContain(a.id);
    expect(listA.map((s) => s.id)).not.toContain(b.id);
    expect(await getSupplier(b.id)).toBeNull();
  });

  it("getSupplierDetail devolve fornecedor + aliases + contratos + documentos do próprio usuário", async () => {
    currentUserId = USER_A;
    const supplier = await createSupplier({ name: "Cemig", type: "utility" });
    supplierIds.push(supplier.id);

    const alias = await createSupplierAlias({
      supplier_id: supplier.id,
      alias_name: "Companhia Energética de Minas Gerais",
      alias_type: "former_name",
    });
    const contract = await createSupplierContract({
      supplier_id: supplier.id,
      contract_type: "utility",
      identifier: "UC-123456",
    });

    const db = getDb();
    const [doc] = await db
      .insert(schema.source_documents)
      .values({
        user_id: USER_A,
        origin_type: "manual_upload",
        origin_key: `test-${crypto.randomUUID()}`,
        filename: "fatura-cemig.pdf",
        supplier_id: supplier.id,
        metadata: { amount: 150.5 },
      } as any)
      .returning();
    sourceDocumentIds.push(doc.id);

    const detail = await getSupplierDetail(supplier.id);
    expect(detail.supplier?.id).toBe(supplier.id);
    expect(detail.aliases.map((a) => a.id)).toContain(alias.id);
    expect(detail.contracts.map((c) => c.id)).toContain(contract.id);
    expect(detail.documents.map((d) => d.id)).toContain(doc.id);

    // Teste de contrato de forma (source_documents é tabela crítica, ver spec
    // "Testes") — chaves snake_case, tipos batendo com o contrato de hoje.
    const docRef = detail.documents.find((d) => d.id === doc.id)!;
    expect(typeof docRef.id).toBe("string");
    expect(typeof docRef.filename).toBe("string");
    expect(typeof docRef.origin_type).toBe("string");
    expect(typeof docRef.status).toBe("string");
    expect(docRef).toHaveProperty("created_at");
    expect(docRef).toHaveProperty("metadata");

    currentUserId = USER_B;
    const emptyDetail = await getSupplierDetail(supplier.id);
    expect(emptyDetail.supplier).toBeNull();
    expect(emptyDetail.aliases).toEqual([]);
    expect(emptyDetail.contracts).toEqual([]);
    expect(emptyDetail.documents).toEqual([]);
  });

  it("updateSupplierActive ativa/desativa e lança erro para id inexistente", async () => {
    currentUserId = USER_A;
    const supplier = await createSupplier({ name: "Fornecedor Ativo", type: "company" });
    supplierIds.push(supplier.id);

    expect((await updateSupplierActive(supplier.id, false)).is_active).toBe(false);
    expect((await updateSupplierActive(supplier.id, true)).is_active).toBe(true);

    await expect(
      updateSupplierActive("00000000-0000-0000-0000-000000000000", false),
    ).rejects.toThrow();
  });

  it("lança erro ao atualizar fornecedor inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      updateSupplier("00000000-0000-0000-0000-000000000000", { name: "X" }),
    ).rejects.toThrow();
  });

  it("deleteSupplierAlias remove o alias, mas não lança erro (nem remove) para alias de outro usuário", async () => {
    currentUserId = USER_A;
    const supplierA = await createSupplier({ name: "Fornecedor Alias A", type: "company" });
    supplierIds.push(supplierA.id);
    const aliasA = await createSupplierAlias({
      supplier_id: supplierA.id,
      alias_name: "Alias A",
      alias_type: "other",
    });

    await deleteSupplierAlias(aliasA.id);
    const db = getDb();
    const [goneA] = await db
      .select()
      .from(schema.supplier_aliases)
      .where(eq(schema.supplier_aliases.id, aliasA.id));
    expect(goneA).toBeUndefined();

    currentUserId = USER_B;
    const supplierB = await createSupplier({ name: "Fornecedor Alias B", type: "company" });
    supplierIds.push(supplierB.id);
    const aliasB = await createSupplierAlias({
      supplier_id: supplierB.id,
      alias_name: "Alias B",
      alias_type: "other",
    });

    currentUserId = USER_A;
    await deleteSupplierAlias(aliasB.id); // não deve lançar
    const [stillThereB] = await db
      .select()
      .from(schema.supplier_aliases)
      .where(eq(schema.supplier_aliases.id, aliasB.id));
    expect(stillThereB).toBeDefined();
  });

  it("deleteSupplierContract remove o contrato, mas não lança erro (nem remove) para contrato de outro usuário", async () => {
    currentUserId = USER_A;
    const supplierA = await createSupplier({ name: "Fornecedor Contrato A", type: "company" });
    supplierIds.push(supplierA.id);
    const contractA = await createSupplierContract({
      supplier_id: supplierA.id,
      contract_type: "service",
      identifier: "CTR-A",
    });

    await deleteSupplierContract(contractA.id);
    const db = getDb();
    const [goneA] = await db
      .select()
      .from(schema.supplier_contracts)
      .where(eq(schema.supplier_contracts.id, contractA.id));
    expect(goneA).toBeUndefined();

    currentUserId = USER_B;
    const supplierB = await createSupplier({ name: "Fornecedor Contrato B", type: "company" });
    supplierIds.push(supplierB.id);
    const contractB = await createSupplierContract({
      supplier_id: supplierB.id,
      contract_type: "service",
      identifier: "CTR-B",
    });

    currentUserId = USER_A;
    await deleteSupplierContract(contractB.id); // não deve lançar
    const [stillThereB] = await db
      .select()
      .from(schema.supplier_contracts)
      .where(eq(schema.supplier_contracts.id, contractB.id));
    expect(stillThereB).toBeDefined();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/suppliers.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `suppliers.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq } from "drizzle-orm";

export type Supplier = typeof schema.suppliers.$inferSelect;
type SupplierInsert = Omit<typeof schema.suppliers.$inferInsert, "user_id" | "id">;
type SupplierUpdate = Partial<SupplierInsert>;

export type SupplierAlias = typeof schema.supplier_aliases.$inferSelect;
type AliasInsert = Omit<typeof schema.supplier_aliases.$inferInsert, "user_id" | "id">;
type AliasUpdate = Partial<AliasInsert>;

export type SupplierContract = typeof schema.supplier_contracts.$inferSelect;
type ContractInsert = Omit<typeof schema.supplier_contracts.$inferInsert, "user_id" | "id">;
type ContractUpdate = Partial<ContractInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getSuppliers(activeOnly = true): Promise<Supplier[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.suppliers.user_id, userId)];
  if (activeOnly) conditions.push(eq(schema.suppliers.is_active, true));
  return db
    .select()
    .from(schema.suppliers)
    .where(and(...conditions))
    .orderBy(asc(schema.suppliers.name));
}

export async function getSupplier(id: string): Promise<Supplier | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.suppliers)
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)));
  return row ?? null;
}

/**
 * BLOQUEADA — NÃO CONVERTIDA. `search_suppliers` foi descartada por inteiro
 * na migração. Ver Apêndice "RPCs bloqueadas". Permanece com o código
 * Supabase atual.
 */
export async function searchSuppliers(query: string, limit = 10) {
  const supabase = await createClient();
  const { data, error } = await supabase.rpc("search_suppliers", {
    p_query: query,
    p_limit: limit,
  });
  if (error) throw new Error(error.message);
  return data;
}

export async function createSupplier(input: SupplierInsert): Promise<Supplier> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.suppliers)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateSupplier(id: string, input: SupplierUpdate): Promise<Supplier> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.suppliers)
    .set(input)
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)))
    .returning();
  if (!row) throw new Error("Fornecedor não encontrado");
  return row;
}

export async function deleteSupplier(id: string): Promise<void> {
  // Soft delete. Nota: sem `.select().single()` hoje — comportamento
  // preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.suppliers)
    .set({ is_active: false })
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)));
}

/**
 * NOVA — migrada de `app/dashboard/suppliers/[id]/page.tsx` (handleDelete).
 * Sem contrato Supabase anterior a preservar — aplica a regra padrão da fase
 * (lança erro em 0 linhas) em vez do silêncio de `deleteSupplier`.
 */
export async function updateSupplierActive(id: string, isActive: boolean): Promise<Supplier> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.suppliers)
    .set({ is_active: isActive })
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)))
    .returning();
  if (!row) throw new Error("Fornecedor não encontrado");
  return row;
}

export interface SourceDocumentRef {
  id: string;
  filename: string;
  origin_type: string;
  status: string;
  created_at: string | null;
  metadata: unknown;
}

export interface SupplierDetail {
  supplier: Supplier | null;
  aliases: SupplierAlias[];
  contracts: SupplierContract[];
  documents: SourceDocumentRef[];
}

/** NOVA — migrada de `suppliers/[id]/page.tsx` (4 selects em paralelo no `load()`). */
export async function getSupplierDetail(id: string): Promise<SupplierDetail> {
  const userId = await requireUserId();
  const db = getDb();

  const [supplierRows, aliases, contracts, documents] = await Promise.all([
    db
      .select()
      .from(schema.suppliers)
      .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId))),
    db
      .select()
      .from(schema.supplier_aliases)
      .where(
        and(
          eq(schema.supplier_aliases.supplier_id, id),
          eq(schema.supplier_aliases.user_id, userId),
        ),
      )
      .orderBy(desc(schema.supplier_aliases.created_at)),
    db
      .select()
      .from(schema.supplier_contracts)
      .where(
        and(
          eq(schema.supplier_contracts.supplier_id, id),
          eq(schema.supplier_contracts.user_id, userId),
        ),
      )
      .orderBy(desc(schema.supplier_contracts.start_date)),
    db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        origin_type: schema.source_documents.origin_type,
        status: schema.source_documents.status,
        created_at: schema.source_documents.created_at,
        metadata: schema.source_documents.metadata,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.supplier_id, id),
          eq(schema.source_documents.user_id, userId),
        ),
      )
      .orderBy(desc(schema.source_documents.created_at))
      .limit(50),
  ]);

  return { supplier: supplierRows[0] ?? null, aliases, contracts, documents };
}

export async function getSupplierAliases(supplierId: string): Promise<SupplierAlias[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.supplier_aliases)
    .where(
      and(
        eq(schema.supplier_aliases.supplier_id, supplierId),
        eq(schema.supplier_aliases.user_id, userId),
      ),
    )
    .orderBy(desc(schema.supplier_aliases.created_at));
}

export async function createSupplierAlias(input: AliasInsert): Promise<SupplierAlias> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.supplier_aliases)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateSupplierAlias(id: string, input: AliasUpdate): Promise<SupplierAlias> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.supplier_aliases)
    .set(input)
    .where(and(eq(schema.supplier_aliases.id, id), eq(schema.supplier_aliases.user_id, userId)))
    .returning();
  if (!row) throw new Error("Alias não encontrado");
  return row;
}

export async function deleteSupplierAlias(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.supplier_aliases)
    .where(and(eq(schema.supplier_aliases.id, id), eq(schema.supplier_aliases.user_id, userId)));
}

export async function getSupplierContracts(supplierId: string): Promise<SupplierContract[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.supplier_contracts)
    .where(
      and(
        eq(schema.supplier_contracts.supplier_id, supplierId),
        eq(schema.supplier_contracts.user_id, userId),
      ),
    )
    .orderBy(desc(schema.supplier_contracts.start_date));
}

export async function createSupplierContract(input: ContractInsert): Promise<SupplierContract> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.supplier_contracts)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateSupplierContract(
  id: string,
  input: ContractUpdate,
): Promise<SupplierContract> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.supplier_contracts)
    .set(input)
    .where(and(eq(schema.supplier_contracts.id, id), eq(schema.supplier_contracts.user_id, userId)))
    .returning();
  if (!row) throw new Error("Contrato não encontrado");
  return row;
}

export async function deleteSupplierContract(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.supplier_contracts)
    .where(
      and(eq(schema.supplier_contracts.id, id), eq(schema.supplier_contracts.user_id, userId)),
    );
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/suppliers.test.ts`
Expected: PASS (6 testes).

- [ ] **Step 5: Migrar `suppliers/[id]/page.tsx` para consumir as Server Actions**

Trocar as 6 chamadas `supabase.from()` diretas do browser (e o import de
`createClient` de `@/lib/supabase/client`) pelas Server Actions equivalentes:

| Local no page.tsx      | Hoje                                                                | Depois                                                                                                                                                   |
| ---------------------- | ------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `load()`               | 4 `supabase.from()` em paralelo                                     | `const detail = await getSupplierDetail(id)`; popular estado a partir de `detail.supplier`/`detail.aliases`/`detail.contracts`/`detail.documents`        |
| `handleSubmit`         | `supabase.from("suppliers").update(...).eq("id", id)`               | `await updateSupplier(id, { name, type, trade_name: tradeName \|\| null, document_number: documentNumber \|\| null, notes: notes \|\| null })`           |
| `handleDelete`         | `supabase.from("suppliers").update({is_active:false}).eq("id", id)` | `await updateSupplierActive(id, false)`                                                                                                                  |
| `handleAddAlias`       | `supabase.auth.getUser()` + insert                                  | `await createSupplierAlias({ supplier_id: id, alias_name: newAliasName.trim(), alias_type: newAliasType })` — `auth.getUser()` no cliente desaparece     |
| `handleDeleteAlias`    | `supabase.from("supplier_aliases").delete()...`                     | `await deleteSupplierAlias(aliasId)`                                                                                                                     |
| `handleAddContract`    | `supabase.auth.getUser()` + insert                                  | `await createSupplierContract({ supplier_id: id, identifier: newContractId.trim(), contract_type: newContractType, label: newContractLabel \|\| null })` |
| `handleDeleteContract` | `supabase.from("supplier_contracts").delete()...`                   | `await deleteSupplierContract(contractId)`                                                                                                               |

Os tipos locais `Alias`/`Contract`/`SourceDocRef` do componente são
substituídos pelos tipos exportados de `./actions/suppliers`
(`SupplierAlias`, `SupplierContract`, `SourceDocumentRef`).

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/app/actions/suppliers.ts apps/web/src/app/actions/suppliers.test.ts apps/web/src/app/dashboard/suppliers/\[id\]/page.tsx
git commit -m "feat(web): suppliers.ts usa Drizzle/Neon; migra page.tsx pras novas actions (searchSuppliers bloqueada)"
```

---

### Task 19: `import.ts` → Drizzle (arquivo NOVO, migrado de `dashboard/import/page.tsx`)

**Files:**

- Create: `apps/web/src/app/actions/import.ts`
- Modify: `apps/web/src/app/dashboard/import/page.tsx` (troca chamadas Supabase por Server Actions)
- Test: `apps/web/src/app/actions/import.test.ts` (novo)

**Interfaces:**

- Consome: `schema.import_jobs`, `schema.financial_products`, `schema.transactions` (Task 1).

**Nota:** o upload físico do CSV (`supabase.storage.from("user-files").upload(...)`)
**não muda** nesta fase — Storage é Fase 3. Só as 4 operações de dados
(`import_jobs`, `financial_products`, `transactions`) migram.

**Nota de comportamento (decisão aceita, não é a mesma classe da regra 3):**
`createImportJob` hoje faz `.insert(...).select().single()` mas só
desestrutura `data`, nunca `error` — uma falha de insert é silenciosamente
ignorada. O Drizzle não tem esse modo silencioso: uma falha de insert lança
exceção. Isso é consequência inevitável de trocar de driver (não há "0
linhas" possível para um insert, diferente de update/delete), documentado
como decisão aceita, não como bug a replicar.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/import.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { cleanupInstitution, seedFinancialProduct, seedInstitution } from "./__tests__/fixtures";
import {
  createImportJob,
  createTransactionFromImport,
  getDefaultFinancialProductId,
  getImportJobStatus,
  updateImportJob,
} from "./import";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: { getUser: async () => ({ data: { user: { id: currentUserId } } }) },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("import actions", () => {
  const institutionIds: string[] = [];
  const importJobIds: string[] = [];
  const transactionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of transactionIds.splice(0)) {
      await db.delete(schema.transactions).where(eq(schema.transactions.id, id));
    }
    for (const id of importJobIds.splice(0)) {
      await db.delete(schema.import_jobs).where(eq(schema.import_jobs.id, id));
    }
    for (const id of institutionIds.splice(0)) {
      await cleanupInstitution(id);
    }
  });

  it("isola jobs de importação e produto padrão por usuário", async () => {
    currentUserId = USER_A;
    const jobA = await createImportJob({ source_type: "csv", status: "processing", total_rows: 3 });
    importJobIds.push(jobA.id);
    const instA = await seedInstitution(USER_A);
    institutionIds.push(instA.id);
    const productA = await seedFinancialProduct(USER_A, instA.id, "checking_account");

    currentUserId = USER_B;
    const jobB = await createImportJob({ source_type: "csv", status: "processing", total_rows: 1 });
    importJobIds.push(jobB.id);
    const instB = await seedInstitution(USER_B);
    institutionIds.push(instB.id);
    await seedFinancialProduct(USER_B, instB.id, "checking_account");

    currentUserId = USER_A;
    expect(await getImportJobStatus(jobB.id)).toBeNull();
    expect(await getDefaultFinancialProductId()).toBe(productA.id);
  });

  it("cria job, produto padrão, transação e atualiza o job (caminho feliz)", async () => {
    currentUserId = USER_A;
    const institution = await seedInstitution(USER_A);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(USER_A, institution.id, "checking_account");

    const job = await createImportJob({ source_type: "csv", status: "processing", total_rows: 1 });
    importJobIds.push(job.id);

    const defaultProductId = await getDefaultFinancialProductId();
    expect(defaultProductId).toBe(product.id);

    const tx = await createTransactionFromImport({
      description: "Linha 1",
      amount: 42.5,
      type: "expense",
      event_date: "2026-01-15",
      financial_product_id: defaultProductId!,
      origin_type: "import",
      is_confirmed: true,
    });
    transactionIds.push(tx.id);
    expect(tx.amount).toBe(42.5);

    await updateImportJob(job.id, {
      status: "completed",
      imported_rows: 1,
      skipped_rows: 0,
      error_rows: 0,
      completed_at: new Date().toISOString(),
    });

    const updated = await getImportJobStatus(job.id);
    expect(updated?.status).toBe("completed");
    expect(updated?.imported_rows).toBe(1);
  });

  it("createTransactionFromImport lança ao violar FK de financial_product_id inexistente", async () => {
    currentUserId = USER_A;
    await expect(
      createTransactionFromImport({
        description: "Linha inválida",
        amount: 10,
        type: "expense",
        event_date: "2026-01-15",
        financial_product_id: "00000000-0000-0000-0000-000000000000",
        origin_type: "import",
        is_confirmed: true,
      }),
    ).rejects.toThrow();
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/import.test.ts`
Expected: FAIL.

- [ ] **Step 3: Criar `import.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, eq } from "drizzle-orm";

type ImportJob = typeof schema.import_jobs.$inferSelect;
type ImportJobInsert = Omit<typeof schema.import_jobs.$inferInsert, "user_id" | "id">;
type ImportJobUpdate = Partial<Omit<typeof schema.import_jobs.$inferInsert, "user_id" | "id">>;
type Transaction = typeof schema.transactions.$inferSelect;
type TransactionInsert = Omit<typeof schema.transactions.$inferInsert, "user_id" | "id">;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function createImportJob(input: ImportJobInsert): Promise<ImportJob> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.import_jobs)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function getImportJobStatus(id: string): Promise<ImportJob | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.import_jobs)
    .where(and(eq(schema.import_jobs.id, id), eq(schema.import_jobs.user_id, userId)));
  return row ?? null;
}

/**
 * Devolve o id do primeiro `financial_products` do usuário — produto
 * "padrão" para lançar as transações importadas. Sem ORDER BY (paridade com
 * o código atual — "qual produto é o primeiro" já era indefinido antes).
 */
export async function getDefaultFinancialProductId(): Promise<string | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select({ id: schema.financial_products.id })
    .from(schema.financial_products)
    .where(eq(schema.financial_products.user_id, userId))
    .limit(1);
  return row?.id ?? null;
}

export async function createTransactionFromImport(input: TransactionInsert): Promise<Transaction> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.transactions)
    .values({ ...input, user_id: userId })
    .returning();
  return row;
}

export async function updateImportJob(id: string, input: ImportJobUpdate): Promise<void> {
  // Nota: sem checagem de linhas afetadas hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.import_jobs)
    .set(input)
    .where(and(eq(schema.import_jobs.id, id), eq(schema.import_jobs.user_id, userId)));
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/import.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Migrar `dashboard/import/page.tsx`**

O parse de CSV e o upload pro Storage (`user-files`) continuam idênticos no
client. `handleImport()` passa a:

```ts
const job = await createImportJob({
  source_type: "csv",
  file_path: filePath,
  status: "processing",
  total_rows: parsed.length,
});

const defaultProductId = await getDefaultFinancialProductId();
if (!defaultProductId) {
  toast.error("Cadastre ao menos um produto financeiro antes de importar.");
  setImporting(false);
  return;
}

let imported = 0;
let skipped = 0;
const errors: string[] = [];
for (let i = 0; i < parsed.length; i++) {
  const row = parsed[i]!;
  try {
    await createTransactionFromImport({
      description: row.description,
      amount: row.amount,
      type: row.type,
      event_date: eventDate,
      financial_product_id: defaultProductId,
      origin_type: "import",
      is_confirmed: true,
    });
    imported++;
  } catch (err) {
    errors.push(`Linha ${i + 1}: ${(err as Error).message}`);
    skipped++;
  }
}

await updateImportJob(job.id, {
  status: errors.length === 0 ? "completed" : "partial",
  imported_rows: imported,
  skipped_rows: skipped,
  error_rows: errors.length,
  error_details: errors.length > 0 ? { errors: errors.slice(0, 50) } : null,
  completed_at: new Date().toISOString(),
});
```

**Ponto de atenção:** a lógica de tally (`imported++`/`skipped++`/`errors.push`)
que hoje mora dentro do bloco `if (error)` do supabase-js precisa migrar para
um `try/catch` por linha em torno de `createTransactionFromImport`, porque o
Drizzle sinaliza falha por exceção, não por `{error}` retornado.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/app/actions/import.ts apps/web/src/app/actions/import.test.ts apps/web/src/app/dashboard/import/page.tsx
git commit -m "feat(web): cria actions/import.ts e migra import/page.tsx para Drizzle"
```

---

### Task 20: `chat-sessions.ts` → Drizzle (arquivo NOVO, migrado de `ai-chat-drawer.tsx`)

**Files:**

- Create: `apps/web/src/app/actions/chat-sessions.ts`
- Modify: `apps/web/src/components/ai-chat-drawer.tsx` (troca chamada Supabase por Server Action)
- Test: `apps/web/src/app/actions/chat-sessions.test.ts` (novo)

**Interfaces:**

- Consome: `schema.ai_chat_sessions` (Task 1).

**Nota:** Storage (`ingestion-originals`) e `functions.invoke("trigger-ingestion")`
no mesmo componente **não mudam** nesta fase (Fase 2/Fase 3).

**Nota de comportamento:** `getOrCreateChatSession` sempre CRIA uma sessão
nova, nunca busca uma existente para reaproveitar — idêntico ao
comportamento de hoje (`createSession()` em `ai-chat-drawer.tsx`); o
componente já só chama isso quando `sessionId` ainda é `null`. O nome da
função vem da spec de design, não implica busca real. O código atual faz
`if (!user) return;` (silencioso); esta reescrita usa `requireUserId()`
(lança) — mesma convenção adotada em toda Server Action desta fase. O
componente precisa envolver a chamada em `try/catch` pra preservar a UX de
"não fazer nada" quando deslogado.

- [ ] **Step 1: Escrever os testes**

```ts
// apps/web/src/app/actions/chat-sessions.test.ts
import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { getOrCreateChatSession } from "./chat-sessions";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: {
      getUser: async () => ({ data: { user: currentUserId ? { id: currentUserId } : null } }),
    },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

describe.skipIf(!process.env.TEST_DATABASE_URL)("chat-sessions actions", () => {
  const sessionIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of sessionIds.splice(0)) {
      await db.delete(schema.ai_chat_sessions).where(eq(schema.ai_chat_sessions.id, id));
    }
  });

  it("isola sessões de chat por usuário", async () => {
    currentUserId = USER_A;
    const sessionA = await getOrCreateChatSession();
    sessionIds.push(sessionA.id);

    currentUserId = USER_B;
    const sessionB = await getOrCreateChatSession();
    sessionIds.push(sessionB.id);

    const db = getDb();
    const rowsForA = await db
      .select()
      .from(schema.ai_chat_sessions)
      .where(eq(schema.ai_chat_sessions.user_id, USER_A));
    expect(rowsForA.map((r) => r.id)).toContain(sessionA.id);
    expect(rowsForA.map((r) => r.id)).not.toContain(sessionB.id);
  });

  it("cria sessão com título e contexto padrão (caminho feliz)", async () => {
    currentUserId = USER_A;
    const session = await getOrCreateChatSession();
    sessionIds.push(session.id);

    expect(session.title).toBe("Nova conversa");
    expect(session.context_type).toBe("general");
    expect(session.user_id).toBe(USER_A);
  });

  it("lança erro quando não autenticado", async () => {
    currentUserId = "";
    await expect(getOrCreateChatSession()).rejects.toThrow("Não autenticado");
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/chat-sessions.test.ts`
Expected: FAIL.

- [ ] **Step 3: Criar `chat-sessions.ts`**

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";

type ChatSession = typeof schema.ai_chat_sessions.$inferSelect;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function getOrCreateChatSession(): Promise<ChatSession> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.ai_chat_sessions)
    .values({ user_id: userId, title: "Nova conversa", context_type: "general" })
    .returning();
  return row;
}
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/app/actions/chat-sessions.test.ts`
Expected: PASS (3 testes).

- [ ] **Step 5: Migrar `ai-chat-drawer.tsx`**

```ts
async function createSession() {
  try {
    const session = await getOrCreateChatSession();
    setSessionId(session.id);
  } catch (err) {
    console.error("Falha ao criar sessão de chat:", err);
  }
}
```

O import de `createClient` de `@/lib/supabase/client` permanece no
componente — ainda usado para `supabase.storage.from("ingestion-originals")`.

- [ ] **Step 6: Commit**

```bash
git add apps/web/src/app/actions/chat-sessions.ts apps/web/src/app/actions/chat-sessions.test.ts apps/web/src/components/ai-chat-drawer.tsx
git commit -m "feat(web): cria actions/chat-sessions.ts e migra ai-chat-drawer.tsx para Drizzle"
```

---

### Task 21: `lib/ai/tools.ts` → Drizzle (19 call sites)

**Files:**

- Modify: `apps/web/src/lib/ai/tools.ts`
- Test: `apps/web/src/lib/ai/tools.test.ts` (novo)

**Interfaces:**

- Consome: `schema.source_documents`, `schema.ingestion_jobs`, `schema.draft_records`, `schema.transactions`, `schema.categories`, `schema.suppliers`, `schema.document_patterns` (Task 1).

**Nota — convenção diferente do resto da fase:** aqui `getUser()` ausente
devolve `{ error: "Não autenticado" }` (não lança) — é o contrato que o SDK
de IA (function calling) espera de uma tool. Preservado; **não** se usa
`requireUserId()` que lança neste arquivo.

**Achado — bug adicional da mesma classe já documentada no arquivo:**
`suggestReconciliation` pedia colunas inexistentes (`transaction_date`,
`supplier_name` diretos em `transactions`) — mesma classe de bug que
`listRecentTransactions` já corrigia no código atual (colunas reais:
`event_date`, `supplier_id` com nome via join em `suppliers`). Corrigido
nesta reescrita com o mesmo padrão de join.

**Achado — comportamento "sempre sucesso" preservado:** `approveDraft`,
`rejectDraft` e o loop de `batchApproveDrafts` fazem update sem checar
linhas afetadas — um `draftId` de outro usuário ou inexistente ainda reporta
`success: true` hoje. Isolamento de **dados** continua garantido (a linha do
outro usuário nunca é alterada); só a mensagem de retorno é otimista, igual
ao código atual. Preservado (exceção documentada), não corrigido — mudar
isso seria alterar o contrato que o agente de IA já espera.

`postDraft`/`postDraftBatch` delegam pra `materialization.ts` (Task 13, já
convertida) — sem chamada direta a Supabase/Drizzle aqui, sem mudança.
`suggestSupplier`/`suggestCategoryTags`/`suggestDocumentType` não tocam
banco — inalteradas.

- [ ] **Step 1: Escrever os testes (3 tools representativas)**

```ts
// apps/web/src/lib/ai/tools.test.ts
import "../../app/actions/test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  cleanupInstitution,
  seedFinancialProduct,
  seedInstitution,
} from "../../app/actions/__tests__/fixtures";
import { approveDraft, listPendingDocuments, listRecentTransactions } from "./tools";

let currentUserId = "";
vi.mock("@/lib/supabase/server", () => ({
  createClient: async () => ({
    auth: {
      getUser: async () => ({ data: { user: currentUserId ? { id: currentUserId } : null } }),
    },
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";
const toolOpts = { toolCallId: "test", messages: [] } as const;

async function seedSourceDocument(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: `test::${crypto.randomUUID()}`,
      filename: "doc.pdf",
      status: "new",
    })
    .returning();
  return row;
}

async function seedDraft(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.draft_records)
    .values({
      user_id: userId,
      draft_type: "transaction",
      status: "pending_review",
      draft_data: { valor: 100 },
      materialization_key: `test::${crypto.randomUUID()}`,
    } as any)
    .returning();
  return row;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("lib/ai/tools.ts", () => {
  const docIds: string[] = [];
  const draftIds: string[] = [];
  const institutionIds: string[] = [];
  const categoryIds: string[] = [];
  const supplierIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of draftIds.splice(0))
      await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
    for (const id of docIds.splice(0))
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    for (const id of categoryIds.splice(0))
      await db.delete(schema.categories).where(eq(schema.categories.id, id));
    for (const id of supplierIds.splice(0))
      await db.delete(schema.suppliers).where(eq(schema.suppliers.id, id));
    for (const id of institutionIds.splice(0)) await cleanupInstitution(id);
  });

  it("list_pending_documents isola documentos por usuário", async () => {
    const docA = await seedSourceDocument(USER_A);
    docIds.push(docA.id);
    const docB = await seedSourceDocument(USER_B);
    docIds.push(docB.id);

    currentUserId = USER_A;
    const result = await listPendingDocuments.execute!({ limit: 10 }, toolOpts);
    expect("documents" in result).toBe(true);
    if ("documents" in result) {
      const ids = result.documents.map((d) => d.id);
      expect(ids).toContain(docA.id);
      expect(ids).not.toContain(docB.id);
    }
  });

  it("approve_draft aprova um draft do próprio usuário (caminho feliz)", async () => {
    currentUserId = USER_A;
    const draft = await seedDraft(USER_A);
    draftIds.push(draft.id);

    const result = await approveDraft.execute!({ draftId: draft.id }, toolOpts);
    expect(result).toMatchObject({ success: true });

    const db = getDb();
    const [updated] = await db
      .select()
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, draft.id));
    expect(updated?.status).toBe("approved");
  });

  it("approve_draft NÃO altera draft de outro usuário, mesmo reportando success (quirk preservado)", async () => {
    const draftB = await seedDraft(USER_B);
    draftIds.push(draftB.id);

    currentUserId = USER_A;
    const result = await approveDraft.execute!({ draftId: draftB.id }, toolOpts);
    expect(result).toMatchObject({ success: true }); // comportamento pré-existente, documentado no código

    const db = getDb();
    const [stillPending] = await db
      .select()
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, draftB.id));
    expect(stillPending?.status).toBe("pending_review"); // linha de outro usuário não foi tocada
  });

  it("list_recent_transactions traz nome de categoria e fornecedor via join", async () => {
    currentUserId = USER_A;
    const institution = await seedInstitution(USER_A);
    institutionIds.push(institution.id);
    const product = await seedFinancialProduct(USER_A, institution.id, "checking_account");

    const db = getDb();
    const [category] = await db
      .insert(schema.categories)
      .values({ user_id: USER_A, name: "Alimentação" } as any)
      .returning();
    categoryIds.push(category.id);
    const [supplier] = await db
      .insert(schema.suppliers)
      .values({ user_id: USER_A, name: "Mercado X" } as any)
      .returning();
    supplierIds.push(supplier.id);

    const [tx] = await db
      .insert(schema.transactions)
      .values({
        user_id: USER_A,
        financial_product_id: product.id,
        type: "expense",
        amount: 55.9,
        event_date: "2026-02-10",
        category_id: category.id,
        supplier_id: supplier.id,
      } as any)
      .returning();

    const result = await listRecentTransactions.execute!({ limit: 10 }, toolOpts);
    expect("transactions" in result).toBe(true);
    if ("transactions" in result) {
      const found = result.transactions.find((t) => t.id === tx.id);
      expect(found?.category_name).toBe("Alimentação");
      expect(found?.supplier_name).toBe("Mercado X");
    }
  });
});
```

- [ ] **Step 2: Rodar os testes e confirmar que falham**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/lib/ai/tools.test.ts`
Expected: FAIL.

- [ ] **Step 3: Reescrever `lib/ai/tools.ts` (arquivo completo)**

```ts
/**
 * AI function calling tool definitions for the SBF chat assistant.
 * Each tool maps to a real operation in the ingestion/financial domain.
 */

import { tool } from "ai";
import { z } from "zod";
import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, gte, ilike, inArray, isNotNull, lte, or } from "drizzle-orm";

// ══════════════════════════════════════════════════════════════
// Group 1: Read Tools
// ══════════════════════════════════════════════════════════════

export const listPendingDocuments = tool({
  description:
    "Lista documentos ingeridos com drafts pendentes de revisão. Retorna nome, tipo, data e quantidade de drafts.",
  parameters: z.object({
    limit: z.number().optional().default(10).describe("Máximo de documentos"),
    status: z
      .string()
      .optional()
      .describe("Filtrar por status do documento (ex: processed, failed)"),
  }),
  execute: async ({ limit, status }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const conditions = [eq(schema.source_documents.user_id, user.id)];
    if (status) conditions.push(eq(schema.source_documents.status, status));

    const documents = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        mime_type: schema.source_documents.mime_type,
        origin_type: schema.source_documents.origin_type,
        status: schema.source_documents.status,
        created_at: schema.source_documents.created_at,
      })
      .from(schema.source_documents)
      .where(and(...conditions))
      .orderBy(desc(schema.source_documents.created_at))
      .limit(limit);

    return { documents, count: documents.length };
  },
});

export const getDocumentDetails = tool({
  description:
    "Obtém detalhes completos de um documento incluindo metadados, jobs de processamento e drafts gerados.",
  parameters: z.object({
    documentId: z.string().uuid().describe("ID do documento"),
  }),
  execute: async ({ documentId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();

    const [docRows, jobs, drafts] = await Promise.all([
      db
        .select()
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.id, documentId),
            eq(schema.source_documents.user_id, user.id),
          ),
        ),
      db
        .select({
          id: schema.ingestion_jobs.id,
          status: schema.ingestion_jobs.status,
          error_message: schema.ingestion_jobs.error_message,
          created_at: schema.ingestion_jobs.created_at,
          updated_at: schema.ingestion_jobs.updated_at,
        })
        .from(schema.ingestion_jobs)
        .where(
          and(
            eq(schema.ingestion_jobs.source_document_id, documentId),
            eq(schema.ingestion_jobs.user_id, user.id),
          ),
        )
        .orderBy(desc(schema.ingestion_jobs.created_at)),
      db
        .select({
          id: schema.draft_records.id,
          status: schema.draft_records.status,
          draft_type: schema.draft_records.draft_type,
          draft_data: schema.draft_records.draft_data,
          confidence_score: schema.draft_records.confidence_score,
          created_at: schema.draft_records.created_at,
        })
        .from(schema.draft_records)
        .where(
          and(
            eq(schema.draft_records.source_document_id, documentId),
            eq(schema.draft_records.user_id, user.id),
          ),
        )
        .orderBy(desc(schema.draft_records.created_at)),
    ]);

    const document = docRows[0];
    if (!document) return { error: "Documento não encontrado" };

    return { document, jobs, drafts };
  },
});

export const listDrafts = tool({
  description: "Lista drafts (rascunhos) extraídos de documentos, com status e dados extraídos.",
  parameters: z.object({
    status: z
      .string()
      .optional()
      .default("pending_review")
      .describe("Status do draft: pending_review, approved, rejected"),
    limit: z.number().optional().default(10).describe("Máximo de drafts"),
  }),
  execute: async ({ status, limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const drafts = await db
      .select({
        id: schema.draft_records.id,
        status: schema.draft_records.status,
        draft_type: schema.draft_records.draft_type,
        draft_data: schema.draft_records.draft_data,
        confidence_score: schema.draft_records.confidence_score,
        source_document_id: schema.draft_records.source_document_id,
        created_at: schema.draft_records.created_at,
      })
      .from(schema.draft_records)
      .where(
        and(eq(schema.draft_records.user_id, user.id), eq(schema.draft_records.status, status)),
      )
      .orderBy(desc(schema.draft_records.created_at))
      .limit(limit);

    return { drafts, count: drafts.length };
  },
});

export const getDraftDetails = tool({
  description:
    "Obtém detalhes completos de um draft específico, incluindo dados extraídos e documento de origem.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft"),
  }),
  execute: async ({ draftId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [draft] = await db
      .select({
        id: schema.draft_records.id,
        status: schema.draft_records.status,
        draft_type: schema.draft_records.draft_type,
        draft_data: schema.draft_records.draft_data,
        confidence_score: schema.draft_records.confidence_score,
        corrections: schema.draft_records.corrections,
        source_document_id: schema.draft_records.source_document_id,
        created_at: schema.draft_records.created_at,
        updated_at: schema.draft_records.updated_at,
      })
      .from(schema.draft_records)
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    if (!draft) return { error: "Draft não encontrado" };

    let sourceDoc = null;
    if (draft.source_document_id) {
      const [doc] = await db
        .select({
          filename: schema.source_documents.filename,
          mime_type: schema.source_documents.mime_type,
          origin_type: schema.source_documents.origin_type,
        })
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.id, draft.source_document_id),
            eq(schema.source_documents.user_id, user.id),
          ),
        );
      sourceDoc = doc ?? null;
    }

    return { draft: { ...draft, source_document: sourceDoc } };
  },
});

export const listRecentTransactions = tool({
  description: "Lista transações recentes do usuário para contexto e reconciliação.",
  parameters: z.object({
    limit: z.number().optional().default(20).describe("Máximo de transações"),
    category: z.string().optional().describe("Filtrar por categoria"),
  }),
  execute: async ({ limit, category }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const conditions = [eq(schema.transactions.user_id, user.id)];
    if (category) conditions.push(eq(schema.categories.name, category));

    const transactions = await db
      .select({
        id: schema.transactions.id,
        description: schema.transactions.description,
        amount: schema.transactions.amount,
        event_date: schema.transactions.event_date,
        created_at: schema.transactions.created_at,
        category_name: schema.categories.name,
        supplier_name: schema.suppliers.name,
      })
      .from(schema.transactions)
      .leftJoin(schema.categories, eq(schema.transactions.category_id, schema.categories.id))
      .leftJoin(schema.suppliers, eq(schema.transactions.supplier_id, schema.suppliers.id))
      .where(and(...conditions))
      .orderBy(desc(schema.transactions.event_date))
      .limit(limit);

    return { transactions, count: transactions.length };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 2: Analysis Tools (sem acesso a dados — inalteradas)
// ══════════════════════════════════════════════════════════════

export const suggestSupplier = tool({
  description: "Sugere o fornecedor/empresa de um documento baseado no texto extraído.",
  parameters: z.object({
    extractedText: z.string().describe("Texto extraído do documento para análise"),
    documentType: z.string().optional().describe("Tipo do documento (boleto, fatura, recibo)"),
  }),
  execute: async ({ extractedText, documentType }) => {
    return {
      instruction:
        "Analise o texto e sugira o fornecedor mais provável. Considere: razão social, CNPJ, nome fantasia, padrões conhecidos de faturas.",
      extractedText: extractedText.slice(0, 2000),
      documentType,
    };
  },
});

export const suggestCategoryTags = tool({
  description:
    "Sugere categoria principal e tags para uma transação baseado na descrição e fornecedor.",
  parameters: z.object({
    description: z.string().describe("Descrição da transação"),
    supplierName: z.string().optional().describe("Nome do fornecedor"),
    amount: z.number().optional().describe("Valor da transação"),
  }),
  execute: async ({ description, supplierName, amount }) => {
    return {
      instruction:
        "Sugira categoria principal e até 5 tags relevantes. Categorias possíveis: moradia, alimentação, transporte, saúde, educação, lazer, trabalho, financeiro, outros. Tags devem ser específicas.",
      description,
      supplierName,
      amount,
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 3: Action Tools (require user confirmation)
// ══════════════════════════════════════════════════════════════

export const approveDraft = tool({
  description:
    "Aprova um draft, marcando como aceito. IMPORTANTE: sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft a aprovar"),
    corrections: z
      .record(z.string())
      .optional()
      .describe("Correções a aplicar antes de aprovar (campo: valor)"),
  }),
  execute: async ({ draftId, corrections }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();

    if (corrections && Object.keys(corrections).length > 0) {
      const [draft] = await db
        .select({ draft_data: schema.draft_records.draft_data })
        .from(schema.draft_records)
        .where(
          and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)),
        );

      if (draft) {
        const updatedData = {
          ...(draft.draft_data as Record<string, unknown>),
          ...corrections,
        };
        await db
          .update(schema.draft_records)
          .set({ draft_data: updatedData, corrections, status: "corrected" })
          .where(
            and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)),
          );
      }
    }

    // Nota: o código atual não usa `.select().single()` neste update — só
    // checa `error`, nunca linhas afetadas. 0 linhas (draft inexistente ou
    // de outro usuário) hoje AINDA retorna `{success: true, message: "...
    // aprovado"}`. Comportamento pré-existente preservado por paridade
    // (exceção documentada à regra geral desta fase) — isolamento de DADOS
    // continua garantido, só a mensagem de retorno é otimista.
    await db
      .update(schema.draft_records)
      .set({ status: "approved" })
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    return {
      success: true,
      message: `Draft ${draftId} aprovado. Ainda NÃO foi lançado — use postDraft para criar o registro financeiro.`,
    };
  },
});

/**
 * Sem alteração — delega para materialization.ts (já reescrito na Task 13
 * do plano), sem chamada direta a supabase/Drizzle aqui.
 */
export const postDraft = tool({
  description:
    "Lança um draft JÁ APROVADO, criando o registro financeiro definitivo (transação, dívida, recorrência ou métrica). IMPORTANTE: só funciona em drafts aprovados; sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft aprovado a lançar"),
  }),
  execute: async ({ draftId }) => {
    const { postApprovedDraftRecord } = await import("@/app/actions/materialization");
    const result = await postApprovedDraftRecord(draftId);

    if (!result.success) {
      return {
        error: result.message,
        validationErrors: result.validationErrors,
      };
    }
    return {
      success: true,
      message: result.message,
      postedRecordId: result.postedRecordId,
      postedRecordType: result.postedRecordType,
    };
  },
});

export const rejectDraft = tool({
  description:
    "Rejeita um draft com motivo. IMPORTANTE: sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft a rejeitar"),
    reason: z.string().describe("Motivo da rejeição"),
  }),
  execute: async ({ draftId, reason }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    // Mesma exceção documentada de approveDraft: sem check de linhas
    // afetadas, paridade com o código atual.
    await db
      .update(schema.draft_records)
      .set({ status: "rejected", corrections: { rejection_reason: reason } })
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    return { success: true, message: `Draft ${draftId} rejeitado: ${reason}` };
  },
});

export const reprocessDocumentTool = tool({
  description:
    "Reprocessa um documento, criando novo job de ingestão. Útil quando o processamento original falhou ou dados mudaram.",
  parameters: z.object({
    documentId: z.string().uuid().describe("ID do documento a reprocessar"),
  }),
  execute: async ({ documentId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();

    // Bulk: 0 linhas é normal (nenhum draft pendente/rejeitado) — sem throw,
    // mesmo comportamento de hoje.
    await db
      .update(schema.draft_records)
      .set({ status: "archived" })
      .where(
        and(
          eq(schema.draft_records.source_document_id, documentId),
          eq(schema.draft_records.user_id, user.id),
          inArray(schema.draft_records.status, ["pending_review", "rejected"]),
        ),
      );

    const [latestJob] = await db
      .select({ id: schema.ingestion_jobs.id })
      .from(schema.ingestion_jobs)
      .where(
        and(
          eq(schema.ingestion_jobs.source_document_id, documentId),
          eq(schema.ingestion_jobs.user_id, user.id),
        ),
      )
      .orderBy(desc(schema.ingestion_jobs.created_at))
      .limit(1);

    if (latestJob) {
      await db
        .update(schema.ingestion_jobs)
        .set({ status: "queued" })
        .where(
          and(
            eq(schema.ingestion_jobs.id, latestJob.id),
            eq(schema.ingestion_jobs.user_id, user.id),
          ),
        );
    }

    return {
      success: true,
      message: `Documento ${documentId} enviado para reprocessamento.`,
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 4: Pattern Tools (M5-006 — ADR-006)
// ══════════════════════════════════════════════════════════════

export const listDocumentPatterns = tool({
  description:
    "Lista padrões documentais cadastrados pelo usuário que orientam a extração e classificação automática. Mostra taxa de acerto e status.",
  parameters: z.object({
    documentType: z
      .string()
      .optional()
      .describe("Filtrar por tipo de documento (ex: conta_energia, boleto_generico)"),
    activeOnly: z.boolean().optional().default(true).describe("Retornar apenas padrões ativos"),
    limit: z.number().optional().default(20).describe("Máximo de padrões"),
  }),
  execute: async ({ documentType, activeOnly, limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const conditions = [eq(schema.document_patterns.user_id, user.id)];
    if (activeOnly) conditions.push(eq(schema.document_patterns.is_active, true));
    if (documentType) conditions.push(eq(schema.document_patterns.document_type, documentType));

    const rows = await db
      .select({
        id: schema.document_patterns.id,
        name: schema.document_patterns.name,
        document_type: schema.document_patterns.document_type,
        version: schema.document_patterns.version,
        is_active: schema.document_patterns.is_active,
        feedback_count: schema.document_patterns.feedback_count,
        success_count: schema.document_patterns.success_count,
        confidence_threshold: schema.document_patterns.confidence_threshold,
        updated_at: schema.document_patterns.updated_at,
      })
      .from(schema.document_patterns)
      .where(and(...conditions))
      .orderBy(desc(schema.document_patterns.version))
      .limit(limit);

    const patterns = rows.map((p) => ({
      ...p,
      successRate:
        p.feedback_count > 0 ? Math.round((p.success_count / p.feedback_count) * 100) : null,
    }));

    return { patterns, count: patterns.length };
  },
});

export const registerDocumentPattern = tool({
  description:
    "Cria um novo padrão documental que ensina o sistema a extrair e classificar um tipo específico de documento automaticamente.",
  parameters: z.object({
    name: z
      .string()
      .describe("Nome descritivo do padrão (ex: 'Conta CEMIG BH', 'Boleto Celular Vivo')"),
    documentType: z
      .string()
      .describe(
        "Tipo do documento (ex: conta_energia, boleto_generico, fatura_cartao, conta_agua, conta_telefone, outros)",
      ),
    extractionRules: z
      .record(z.union([z.string(), z.number(), z.boolean()]))
      .optional()
      .describe("Regras de extração (campo -> regex ou valor esperado)"),
    fieldMappings: z
      .record(
        z.union([
          z.string(),
          z.object({
            target: z.string(),
            transform: z.enum(["uppercase", "lowercase", "trim", "parse_number"]).optional(),
          }),
        ]),
      )
      .optional()
      .describe("Mapeamentos de campo (fonte -> destino ou { target, transform })"),
    confidenceThreshold: z
      .number()
      .min(0)
      .max(1)
      .optional()
      .default(0.7)
      .describe("Confiança mínima para aplicação automática (0.0-1.0)"),
    supplierId: z.string().uuid().optional().describe("ID do fornecedor associado (opcional)"),
    institutionId: z.string().uuid().optional().describe("ID da instituição associada (opcional)"),
  }),
  execute: async ({
    name,
    documentType,
    extractionRules,
    fieldMappings,
    confidenceThreshold,
    supplierId,
    institutionId,
  }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [pattern] = await db
      .insert(schema.document_patterns)
      .values({
        user_id: user.id,
        name,
        document_type: documentType,
        extraction_rules: extractionRules ?? {},
        field_mappings: fieldMappings ?? {},
        confidence_threshold: confidenceThreshold,
        supplier_id: supplierId ?? null,
        institution_id: institutionId ?? null,
        version: 1,
        is_active: true,
        feedback_count: 0,
        success_count: 0,
      })
      .returning({
        id: schema.document_patterns.id,
        name: schema.document_patterns.name,
        document_type: schema.document_patterns.document_type,
        version: schema.document_patterns.version,
      });

    return {
      success: true,
      pattern,
      message: `Padrão "${name}" criado com sucesso. ID: ${pattern?.id}`,
    };
  },
});

export const updateDocumentPattern = tool({
  description:
    "Atualiza regras de extração ou mapeamentos de um padrão documental existente. Cria nova versão automaticamente para preservar histórico.",
  parameters: z.object({
    patternId: z.string().uuid().describe("ID do padrão a atualizar"),
    extractionRules: z
      .record(z.union([z.string(), z.number(), z.boolean()]))
      .optional()
      .describe("Novas regras de extração"),
    fieldMappings: z
      .record(
        z.union([
          z.string(),
          z.object({
            target: z.string(),
            transform: z.enum(["uppercase", "lowercase", "trim", "parse_number"]).optional(),
          }),
        ]),
      )
      .optional()
      .describe("Novos mapeamentos de campo"),
    confidenceThreshold: z
      .number()
      .min(0)
      .max(1)
      .optional()
      .describe("Novo threshold de confiança"),
    isActive: z.boolean().optional().describe("Ativar ou desativar o padrão"),
  }),
  execute: async ({ patternId, extractionRules, fieldMappings, confidenceThreshold, isActive }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [existing] = await db
      .select({ id: schema.document_patterns.id, name: schema.document_patterns.name })
      .from(schema.document_patterns)
      .where(
        and(
          eq(schema.document_patterns.id, patternId),
          eq(schema.document_patterns.user_id, user.id),
        ),
      );

    if (!existing) return { error: "Padrão não encontrado" };

    const updates: Partial<typeof schema.document_patterns.$inferInsert> = {};
    if (extractionRules !== undefined) updates.extraction_rules = extractionRules;
    if (fieldMappings !== undefined) updates.field_mappings = fieldMappings;
    if (confidenceThreshold !== undefined) updates.confidence_threshold = confidenceThreshold;
    if (isActive !== undefined) updates.is_active = isActive;

    if (Object.keys(updates).length === 0) {
      return { error: "Nenhum campo para atualizar fornecido" };
    }

    // Este update DO CÓDIGO ATUAL usa `.select().single()` (diferente de
    // approveDraft/rejectDraft) — 0 linhas afetadas já produz erro hoje.
    // Preservado com `.returning()` + checagem de nulo.
    const [updated] = await db
      .update(schema.document_patterns)
      .set(updates)
      .where(
        and(
          eq(schema.document_patterns.id, patternId),
          eq(schema.document_patterns.user_id, user.id),
        ),
      )
      .returning({
        id: schema.document_patterns.id,
        name: schema.document_patterns.name,
        document_type: schema.document_patterns.document_type,
        version: schema.document_patterns.version,
        is_active: schema.document_patterns.is_active,
      });

    if (!updated) return { error: "Padrão não encontrado" };
    return {
      success: true,
      pattern: updated,
      message: `Padrão "${existing.name}" atualizado.`,
    };
  },
});

export const suggestReconciliation = tool({
  description:
    "Sugere reconciliação entre um draft e transações existentes baseado em valor, data e fornecedor.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft para buscar candidatos de reconciliação"),
  }),
  execute: async ({ draftId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [draft] = await db
      .select({ draft_data: schema.draft_records.draft_data })
      .from(schema.draft_records)
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    if (!draft) return { error: "Draft não encontrado" };

    const draftData = draft.draft_data as Record<string, unknown> | null;
    const amount = draftData?.amount ?? draftData?.valor;
    const date = draftData?.date ?? draftData?.data;

    if (!amount) {
      return { candidates: [], message: "Valor não encontrado no draft para reconciliação." };
    }

    // BUG CORRIGIDO nesta reescrita: o código atual pedia as colunas
    // `transaction_date` e `supplier_name`, que NÃO EXISTEM em `transactions`
    // (as reais são `event_date` e `supplier_id`, com nome vindo por join em
    // `suppliers`) — mesma classe de bug já corrigida em `listRecentTransactions`
    // dentro deste mesmo arquivo.
    const numAmount = Number(amount);
    const margin = numAmount * 0.05;

    const conditions = [
      eq(schema.transactions.user_id, user.id),
      gte(schema.transactions.amount, numAmount - margin),
      lte(schema.transactions.amount, numAmount + margin),
    ];

    if (date) {
      const dateMs = new Date(String(date)).getTime();
      if (!isNaN(dateMs)) {
        const sevenDaysBefore = new Date(dateMs - 7 * 24 * 60 * 60 * 1000)
          .toISOString()
          .split("T")[0]!;
        conditions.push(gte(schema.transactions.event_date, sevenDaysBefore));
      }
    }

    const candidates = await db
      .select({
        id: schema.transactions.id,
        description: schema.transactions.description,
        amount: schema.transactions.amount,
        event_date: schema.transactions.event_date,
        supplier_name: schema.suppliers.name,
      })
      .from(schema.transactions)
      .leftJoin(schema.suppliers, eq(schema.transactions.supplier_id, schema.suppliers.id))
      .where(and(...conditions))
      .orderBy(desc(schema.transactions.event_date))
      .limit(5);

    return { candidates, draftAmount: numAmount };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 5: Classification & Analysis Tools
// ══════════════════════════════════════════════════════════════

export const suggestDocumentType = tool({
  description:
    "Sugere o tipo de documento (boleto, fatura, recibo, comprovante, extrato, contrato) baseado no texto extraído.",
  parameters: z.object({
    extractedText: z.string().describe("Texto extraído do documento"),
    filename: z.string().optional().describe("Nome do arquivo original"),
  }),
  execute: async ({ extractedText, filename }) => {
    return {
      instruction:
        "Analise o texto e o nome do arquivo para classificar o tipo de documento. Tipos possíveis: boleto, fatura_cartao, fatura_servico, recibo, comprovante_pagamento, extrato_bancario, contrato, nota_fiscal, outro. Explique sua classificação.",
      extractedText: extractedText.slice(0, 2000),
      filename,
    };
  },
});

export const explainClassification = tool({
  description:
    "Explica por que um documento ou transação foi classificado de determinada forma. Útil para transparência e aprendizado.",
  parameters: z.object({
    draftId: z.string().uuid().describe("ID do draft para explicar a classificação"),
  }),
  execute: async ({ draftId }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [draft] = await db
      .select({
        draft_type: schema.draft_records.draft_type,
        draft_data: schema.draft_records.draft_data,
        confidence_score: schema.draft_records.confidence_score,
        corrections: schema.draft_records.corrections,
        source_document_id: schema.draft_records.source_document_id,
      })
      .from(schema.draft_records)
      .where(and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.id)));

    if (!draft) return { error: "Draft não encontrado" };

    let sourceInfo = null;
    if (draft.source_document_id) {
      const [doc] = await db
        .select({
          filename: schema.source_documents.filename,
          mime_type: schema.source_documents.mime_type,
          origin_type: schema.source_documents.origin_type,
          metadata: schema.source_documents.metadata,
        })
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.id, draft.source_document_id),
            eq(schema.source_documents.user_id, user.id),
          ),
        );
      sourceInfo = doc ?? null;
    }

    return {
      instruction:
        "Explique em linguagem acessível como este draft foi classificado, quais dados foram extraídos e qual a confiança. Se houver correções, mencione o que foi ajustado.",
      draft: {
        type: draft.draft_type,
        data: draft.draft_data,
        confidence: draft.confidence_score,
        corrections: draft.corrections,
      },
      source: sourceInfo,
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 6: Specialized Query Tools
// ══════════════════════════════════════════════════════════════

export const listErrorDocuments = tool({
  description: "Lista documentos com erro de processamento, para diagnóstico e reprocessamento.",
  parameters: z.object({
    limit: z.number().optional().default(10).describe("Máximo de documentos"),
  }),
  execute: async ({ limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const documents = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        status: schema.source_documents.status,
        created_at: schema.source_documents.created_at,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.user_id, user.id),
          eq(schema.source_documents.status, "error"),
        ),
      )
      .orderBy(desc(schema.source_documents.created_at))
      .limit(limit);

    return { documents, count: documents.length };
  },
});

export const listMissingPasswordDocuments = tool({
  description:
    "Lista documentos que falharam por falta de senha (PDF protegido), para que o usuário possa fornecer.",
  parameters: z.object({
    limit: z.number().optional().default(10).describe("Máximo de documentos"),
  }),
  execute: async ({ limit }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const rows = await db
      .select({
        id: schema.ingestion_jobs.id,
        source_document_id: schema.ingestion_jobs.source_document_id,
        status: schema.ingestion_jobs.status,
        error_message: schema.ingestion_jobs.error_message,
        filename: schema.source_documents.filename,
      })
      .from(schema.ingestion_jobs)
      .innerJoin(
        schema.source_documents,
        eq(schema.ingestion_jobs.source_document_id, schema.source_documents.id),
      )
      .where(
        and(
          eq(schema.ingestion_jobs.user_id, user.id),
          eq(schema.source_documents.user_id, user.id),
          eq(schema.ingestion_jobs.status, "failed"),
          or(
            ilike(schema.ingestion_jobs.error_message, "%password%"),
            ilike(schema.ingestion_jobs.error_message, "%senha%"),
          ),
        ),
      )
      .orderBy(desc(schema.ingestion_jobs.created_at))
      .limit(limit);

    return { documents: rows, count: rows.length };
  },
});

// ══════════════════════════════════════════════════════════════
// Group 7: Batch Action Tools
// ══════════════════════════════════════════════════════════════

export const batchApproveDrafts = tool({
  description:
    "Aprova múltiplos drafts de uma vez. IMPORTANTE: SEMPRE liste os drafts primeiro e peça confirmação explícita do usuário antes de executar.",
  parameters: z.object({
    draftIds: z.array(z.string().uuid()).describe("Lista de IDs dos drafts a aprovar"),
  }),
  execute: async ({ draftIds }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    if (draftIds.length > 50) {
      return { error: "Máximo de 50 drafts por batch." };
    }

    const db = getDb();
    const results: { id: string; success: boolean; error?: string }[] = [];

    // Loop sequencial de updates independentes, sem transação — paridade
    // com o código atual (candidato a Apêndice B, não decidido aqui).
    for (const draftId of draftIds) {
      try {
        // Mesma exceção documentada de approveDraft/rejectDraft: sem check
        // de linhas afetadas.
        await db
          .update(schema.draft_records)
          .set({ status: "approved" })
          .where(
            and(
              eq(schema.draft_records.id, draftId),
              eq(schema.draft_records.user_id, user.id),
              eq(schema.draft_records.status, "pending_review"),
            ),
          );
        results.push({ id: draftId, success: true });
      } catch (err) {
        results.push({ id: draftId, success: false, error: (err as Error).message });
      }
    }

    const approved = results.filter((r) => r.success).length;
    const failed = results.filter((r) => !r.success).length;

    return {
      success: true,
      message: `Batch concluído: ${approved} aprovados, ${failed} falharam. Nenhum foi lançado — use postDraftBatch para criar os registros financeiros.`,
      details: results,
    };
  },
});

/**
 * Sem alteração — delega para materialization.ts (Task 13), sem chamada
 * direta a supabase/Drizzle aqui.
 */
export const postDraftBatch = tool({
  description:
    "Lança todos os drafts JÁ APROVADOS de um batch, criando os registros financeiros definitivos. IMPORTANTE: sempre confirme com o usuário antes de executar.",
  parameters: z.object({
    batchId: z.string().uuid().describe("ID do batch a lançar"),
  }),
  execute: async ({ batchId }) => {
    const { postApprovedDraftBatch } = await import("@/app/actions/materialization");
    const result = await postApprovedDraftBatch(batchId);

    return {
      success: result.failed === 0,
      message: `Lançamento concluído: ${result.succeeded} lançados, ${result.failed} com problema.`,
      details: result.results.map((r) => ({
        id: r.draftRecordId,
        success: r.success,
        message: r.message,
        validationErrors: r.validationErrors,
      })),
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Sprint 4 Tools: suggest_splits, suggest_supplier_name, explain_extraction
// ══════════════════════════════════════════════════════════════

export const suggestSplits = tool({
  description:
    "Sugere como dividir (ratear) um documento de custo em múltiplas linhas de despesa. " +
    "Analisa o tipo do documento, valor total e histórico de padrões para propor splits.",
  parameters: z.object({
    document_id: z.string().describe("UUID do documento fonte"),
    total_amount: z
      .number()
      .optional()
      .describe("Valor total do documento (usado quando não disponível no banco)"),
    hint: z
      .string()
      .optional()
      .describe("Dica adicional do usuário sobre como dividir (ex: 50% pessoal, 50% empresa)"),
  }),
  execute: async ({ document_id, total_amount, hint }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [doc] = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        document_type: schema.source_documents.document_type,
        supplier_name_raw: schema.source_documents.supplier_name_raw,
        metadata: schema.source_documents.metadata,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.id, document_id),
          eq(schema.source_documents.user_id, user.id),
        ),
      );

    if (!doc) return { error: "Documento não encontrado" };

    const meta = doc.metadata as Record<string, unknown> | null;
    const amount = total_amount ?? (meta?.total_amount as number | null) ?? null;

    return {
      document_id,
      filename: doc.filename,
      document_type: doc.document_type,
      supplier_name_raw: doc.supplier_name_raw,
      total_amount: amount,
      hint: hint ?? null,
      suggestion:
        "Com base no tipo de documento e fornecedor, analise os splits mais prováveis. " +
        "Se for nota fiscal de serviço, considere rateio por centro de custo. " +
        "Se for fatura de utilidade, considere pessoal vs. profissional. " +
        "Retorne uma lista de splits sugeridos com percentual e descrição.",
      message: "Revise e ajuste os percentuais conforme necessário antes de confirmar o rateio.",
    };
  },
});

export const suggestSupplierName = tool({
  description:
    "Dado um CNPJ, sugere a razão social e possíveis aliases/nomes alternativos do fornecedor " +
    "com base no histórico de documentos ingeridos e padrões conhecidos.",
  parameters: z.object({
    document_number: z.string().describe("CNPJ ou CPF do fornecedor (apenas dígitos ou formatado)"),
  }),
  execute: async ({ document_number }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const digits = document_number.replace(/\D/g, "");

    const existing = await db
      .select({
        id: schema.suppliers.id,
        name: schema.suppliers.name,
        trade_name: schema.suppliers.trade_name,
        document_number: schema.suppliers.document_number,
      })
      .from(schema.suppliers)
      .where(
        and(
          eq(schema.suppliers.user_id, user.id),
          eq(schema.suppliers.document_number, document_number),
        ),
      )
      .limit(1);

    if (existing.length > 0) {
      const s = existing[0]!;
      return {
        found: true,
        supplier_id: s.id,
        name: s.name,
        trade_name: s.trade_name,
        document_number: s.document_number,
        aliases: [s.name, s.trade_name].filter(Boolean),
        message: "Fornecedor já cadastrado — dados recuperados do histórico.",
      };
    }

    const docs = await db
      .select({
        supplier_name_raw: schema.source_documents.supplier_name_raw,
        metadata: schema.source_documents.metadata,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.user_id, user.id),
          isNotNull(schema.source_documents.supplier_name_raw),
        ),
      )
      .limit(20);

    const found = docs.filter((d) => {
      const meta = d.metadata as Record<string, unknown> | null;
      const cnpj = (meta?.cnpj as string | null)?.replace(/\D/g, "");
      return cnpj === digits;
    });

    const namesFound = [
      ...new Set(found.map((d) => d.supplier_name_raw).filter(Boolean)),
    ] as string[];

    return {
      found: false,
      document_number,
      digits,
      suggested_names: namesFound,
      aliases: namesFound,
      message:
        namesFound.length > 0
          ? `Encontrados ${namesFound.length} nomes nos documentos ingeridos.`
          : "Nenhum histórico encontrado. Consulte a Receita Federal ou insira manualmente.",
    };
  },
});

export const explainExtraction = tool({
  description:
    "Explica por que um campo específico foi extraído com determinada confiança por OCR ou IA. " +
    "Analisa o documento e fornece contexto sobre a qualidade da extração.",
  parameters: z.object({
    document_id: z.string().describe("UUID do documento fonte"),
    field_name: z.string().describe("Nome do campo (ex: total_amount, supplier_name, due_date)"),
    extracted_value: z.string().optional().describe("Valor extraído (para verificação)"),
    confidence: z.number().optional().describe("Confiança reportada (0-1)"),
    source: z.string().optional().describe("Método de extração (ex: ocr, regex, gpt-4o)"),
  }),
  execute: async ({ document_id, field_name, extracted_value, confidence, source }) => {
    const supabase = await createClient();
    const {
      data: { user },
    } = await supabase.auth.getUser();
    if (!user) return { error: "Não autenticado" };

    const db = getDb();
    const [doc] = await db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        document_type: schema.source_documents.document_type,
        metadata: schema.source_documents.metadata,
        status: schema.source_documents.status,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.id, document_id),
          eq(schema.source_documents.user_id, user.id),
        ),
      );

    if (!doc) return { error: "Documento não encontrado" };

    const meta = doc.metadata as Record<string, unknown> | null;
    const docConfidence = (meta?.confidence as number | null) ?? null;

    return {
      document_id,
      filename: doc.filename,
      document_type: doc.document_type,
      field_name,
      extracted_value: extracted_value ?? null,
      confidence: confidence ?? docConfidence,
      source: source ?? "desconhecido",
      context: {
        document_status: doc.status,
        overall_confidence: docConfidence,
      },
      explanation:
        `O campo "${field_name}" foi extraído via ${source ?? "método não informado"} ` +
        `com confiança ${confidence !== undefined ? Math.round(confidence * 100) + "%" : "não informada"}. ` +
        "Fatores que podem reduzir a confiança: qualidade do scan, layout não padronizado, " +
        "caracteres ambíguos (O/0, l/1), carimbo sobreposto, ou modelo de documento desconhecido. " +
        "Recomenda-se verificar visualmente o PDF e corrigir o valor se necessário.",
    };
  },
});

// ══════════════════════════════════════════════════════════════
// Tool registry for the chat route
// ══════════════════════════════════════════════════════════════

export const sbfTools = {
  list_pending_documents: listPendingDocuments,
  get_document_details: getDocumentDetails,
  list_drafts: listDrafts,
  get_draft_details: getDraftDetails,
  list_recent_transactions: listRecentTransactions,
  suggest_supplier: suggestSupplier,
  suggest_category_tags: suggestCategoryTags,
  suggest_document_type: suggestDocumentType,
  explain_classification: explainClassification,
  approve_draft: approveDraft,
  post_draft: postDraft,
  reject_draft: rejectDraft,
  reprocess_document: reprocessDocumentTool,
  list_document_patterns: listDocumentPatterns,
  register_document_pattern: registerDocumentPattern,
  update_document_pattern: updateDocumentPattern,
  suggest_reconciliation: suggestReconciliation,
  list_error_documents: listErrorDocuments,
  list_missing_password_documents: listMissingPasswordDocuments,
  batch_approve_drafts: batchApproveDrafts,
  post_draft_batch: postDraftBatch,
  suggest_splits: suggestSplits,
  suggest_supplier_name: suggestSupplierName,
  explain_extraction: explainExtraction,
};
```

- [ ] **Step 4: Rodar os testes e confirmar que passam**

Run: `TEST_DATABASE_URL=<branch-neon-de-teste> bunx vitest run apps/web/src/lib/ai/tools.test.ts`
Expected: PASS (4 testes).

- [ ] **Step 5: Commit**

```bash
git add apps/web/src/lib/ai/tools.ts apps/web/src/lib/ai/tools.test.ts
git commit -m "feat(web): lib/ai/tools.ts usa Drizzle/Neon com filtro user_id explicito"
```

---

## Apêndice — RPCs bloqueadas (resolver antes do cutover)

**Regra vinculante da Verônica-Bolso:** deixar uma função como `supabase.rpc()`
sem converter é aceitável **dentro desta fase** (4a), mas não é uma decisão
adiável para sempre — toda chamada `supabase.rpc()` remanescente **quebra no
cutover** (quando o Supabase for desligado). Esta lista é o registro corrente,
atualizado à medida que novas RPCs bloqueadas forem encontradas nas tasks
seguintes. O documento de cutover (fora desta spec/plano) precisa de uma seção
"RPCs bloqueadas — resolver antes do cutover" listando cada uma com as 3
saídas possíveis: (a) recriar no Neon com `p_user_id` explícito; (b)
reimplementar em TypeScript; (c) aceitar perda da feature, com aval do CEO. O
critério de aceite do cutover inclui "zero chamadas `supabase.rpc()`/
`supabase.from()`/`supabase.auth` em código ativo", verificado por grep.

| Função                          | Onde é chamada                                                                                       | Motivo do bloqueio                                                                                                                                                                                                                                                                                                                                                                                                         | Task que encontrou |
| ------------------------------- | ---------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------ |
| `generate_financial_periods`    | `actions/financial-periods.ts`, `apps/mcp-server/src/tools/recompute-financial-periods.ts` (Fase 4b) | Descartada por completo na migração — decidia o usuário via `auth.uid()` interno, sem `p_user_id`                                                                                                                                                                                                                                                                                                                          | Task 3             |
| `get_financial_period_for_date` | `actions/financial-periods.ts`                                                                       | Idem acima                                                                                                                                                                                                                                                                                                                                                                                                                 | Task 3             |
| `fn_set_secret`                 | `actions/secrets.ts` (`setSecret`)                                                                   | Idem acima — **e mesmo recriada com `p_user_id`, depende de `private.get_crypto_key()`, também não portado ao Neon** (segundo bloqueador independente). **Achado adicional (Task 14):** a função grava `entity_id` recebido do cliente sem validar posse — mesma família dos 6 IDORs já fechados nas Server Actions, mas do lado SQL; quem recriar esta RPC precisa incluir essa validação, não só resolver o `p_user_id`. | Task 14            |
| `register_pattern_feedback`     | `actions/patterns.ts` (`registerPatternFeedback`)                                                    | Idem acima                                                                                                                                                                                                                                                                                                                                                                                                                 | Task 15            |
| `search_suppliers`              | `actions/suppliers.ts` (`searchSuppliers`)                                                           | Idem acima                                                                                                                                                                                                                                                                                                                                                                                                                 | Task 18            |

(Tasks 15-20, ainda não escritas neste plano, podem adicionar linhas aqui.)

### Backlog de produto (não são RPCs bloqueadas — não bloqueiam a Fase 4a)

| Item                                                                                               | Onde                            | Nota técnica                                                                                                                                                                                                                                                                                                                                                                                                                                                           | Ruling                                                                                                                      |
| -------------------------------------------------------------------------------------------------- | ------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `completePluggyConnection(externalItemId)` usa um id de sistema externo (Pluggy) sem validar posse | `actions/pluggy-connections.ts` | Gap pré-existente de produto, não regressão da migração — no código Supabase antigo a RLS só escopava o `INSERT` ao próprio `user_id`; vincular um `itemId` alheio já era igualmente possível antes. Quando a premissa de usuário único (ADR-007) cair, a validação correta é conferir via API da Pluggy que o item foi criado por um connect token emitido para o `clientUserId` da sessão (`createConnectToken` já passa `clientUserId` — é fechar o loop na volta). | Verônica-Bolso (varredura retroativa de IDOR, 2026-08-27): não bloqueia a 4a, não exige fix agora (regra da menor mudança). |
