# ADR-009 Fase 4a — Server Actions e componentes client-side → Drizzle/Neon — Spec de Design

**Status:** Aprovada pela supervisora Veronica-Bolso em 2026-08-26 (design conceitual +
4 condições vinculantes, incorporadas abaixo).

**ADR de origem:** `docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md` (ADR-009),
adendo de 2026-08-26 (reordenação de fases). Esta é a primeira metade da antiga "Fase
4", rodando **antes** da Fase 2 (auth) — ver o adendo para o porquê (RLS implícito +
`.from()` client-side quebrariam ao trocar a sessão).

**Pré-requisito:** ADR-009 Fase 1 (Neon + Drizzle, PR #22) — `@sbf/db` já existe
(`packages/db/src/schema.ts`, `getDb()`).

**Documentos irmãos (fora desta spec):**

- **Fase 4b** (workers + MCP server → Drizzle/Neon) — spec separada, mesma técnica,
  superfície de arquivo disjunta (`workers/*`, `apps/mcp-server/*`), pode ser
  desenvolvida em paralelo a esta.
- **Cutover** — plano operacional separado (migração de dados real Supabase→Neon +
  flip coordenado de env vars + verificação de paridade). **Nada desta fase vai pra
  produção sozinho** — o código pode ser desenvolvido, testado e mergeado numa branch
  normalmente, mas o app continua servindo do Supabase até o cutover acontecer com 4a
  e 4b prontas.

## Contexto

Hoje as 17 Server Actions (`apps/web/src/app/actions/`) e 3 componentes client-side
(`components/ai-chat-drawer.tsx`, `app/dashboard/import/page.tsx`,
`app/dashboard/suppliers/[id]/page.tsx`) acessam dados via `supabase-js`, dependendo
de RLS (`auth.uid() = user_id`) para autorização por linha — medido: das ~172
chamadas `.from()` nessas Server Actions, só ~63 filtram `user_id` explicitamente
(majoritariamente em `insert`); o resto (a maioria dos `select`/`update`/`delete`)
não tem filtro nenhum no código, confiando inteiramente na RLS.

## Escopo

**Dentro:**

- As 17 Server Actions passam a usar `getDb()` (`@sbf/db`) em vez de
  `supabase.from(...)` para toda leitura/escrita de dados.
- Toda query de leitura/atualização/exclusão ganha filtro explícito
  `WHERE user_id = $1` (o `$1` vindo da sessão, nunca de input do cliente) — mesmo
  onde o código Supabase de hoje não mostra filtro nenhum (RLS fazia isso
  escondido).
- Os 3 componentes client-side passam a chamar uma Server Action nova (fina) em vez
  de `supabase.from()` direto do browser.
- As 4 funções `SECURITY DEFINER` (`fn_get_secrets`, `fn_mark_secret_used`,
  `fn_materialize_draft_record`, `fn_upsert_financial_obligation`) passam a ser
  chamadas via SQL cru do Drizzle, com `p_user_id` vindo da sessão.
- Testes de integração novos (cobertura era zero antes) para cada arquivo
  reescrito.

**Fora:**

- `supabase.auth.getUser()` **não muda** — continua sendo a fonte de identidade
  nesta fase (condição da Verônica). Só o que vem _depois_ do `getUser()` muda.
- Workers, MCP server (Fase 4b, spec separada).
- Deploy/cutover para produção (plano separado; nada aqui é implantado sozinho).
- Transações multi-tabela "de brinde": onde uma escrita hoje é multi-`.insert()`
  sequencial sem transação (ex.: `ingestion_runs` → `source_documents` →
  `ingestion_jobs`, ver Fase 2), a reescrita **não introduz transação nova** — é
  paridade de comportamento, não correção de bug. Pontos onde isso corrigiria um bug
  real de escrita parcial ficam anotados no Apêndice B como decisão separada, não
  resolvidos aqui.
- `@sbf/shared-types`/`database.types.ts` não mudam — continuam servindo
  workers/MCP até a Fase 4b.

## Segurança do banco de testes (condição 1 — a mais importante)

Os testes de integração desta fase **escrevem dados de dois `user_id` diferentes**
(para provar isolamento). Isso NUNCA pode rodar contra produção.

- Toda nova suíte de teste desta fase lê `process.env.TEST_DATABASE_URL` — uma
  variável **separada** de `DATABASE_URL`. **Proibido** cair de volta pra
  `DATABASE_URL` se `TEST_DATABASE_URL` não existir.
- `describe.skipIf(!process.env.TEST_DATABASE_URL)` em cada arquivo (mesmo padrão
  da Fase 1, mas sobre a variável nova).
- `.env.example` ganha `TEST_DATABASE_URL=` documentado: "aponte para um branch Neon
  dedicado a teste (branching é barato) ou um Postgres local — nunca o banco de
  produção."
- `packages/db/drizzle.config.ts` não muda (continua apontando pra `DATABASE_URL`
  em uso normal); os testes desta fase instanciam seu próprio client Drizzle com
  `TEST_DATABASE_URL` diretamente, não via `getDb()` (que é hardcoded em
  `DATABASE_URL`).

## Higiene de teste (condição 2)

Cada teste limpa o que criou — duas opções válidas, escolhidas por arquivo conforme
o que for mais simples:

- **Preferencial:** envolver o teste numa transação (`db.transaction(async (tx) => {
...; throw new RollbackSignal(); })` ou equivalente) que sempre reverte no fim,
  nunca commitando as linhas de teste.
- **Alternativa** (quando o código sob teste já gerencia sua própria transação ou
  chama uma função `SECURITY DEFINER` que não pode rodar dentro de uma transação
  aninhada de teste): `afterEach` que deleta explicitamente, por `id`, tudo que o
  teste criou.

Nenhum teste desta fase pode deixar linha órfã que contamine o teste seguinte —
isso é crítico porque o teste de isolamento (ver próxima seção) verifica
precisamente que nenhuma linha "vaza" entre usuários.

## Pré-requisito: corrigir casing e tipo numérico de `packages/db/src/schema.ts`

**Achado na revisão da Verônica-Bolso, verificado contra o código** (2 FAILs na
regra "contrato idêntico" — corrigidos aqui antes de qualquer Server Action ser
reescrita):

**FAIL 1 — casing.** `schema.ts` foi introspectado sem opção de casing, então as
propriedades TypeScript viraram camelCase (`userId`, `iconUrl`, `isActive`,
`displayOrder`, `createdAt` — ver `schema.ts:834-843`) enquanto a coluna real no
Postgres é `snake_case` e o contrato de hoje (via PostgREST/`@sbf/shared-types`) é
`snake_case`. `$inferSelect` devolveria objetos com chaves camelCase — quebraria
todo componente/JSON que espera `display_order`, `user_id` etc.

**Correção escolhida:** regenerar `schema.ts`/`relations.ts` com
`introspect: { casing: "preserve" }` em `packages/db/drizzle.config.ts` (opção real
do `drizzle-kit@0.31.10`, confirmada em
`node_modules/drizzle-kit/index.d.mts`) — as propriedades TS passam a ser
idênticas ao nome da coluna (`user_id`, `display_order`, sem precisar do segundo
argumento string de override que existe hoje). Escolhida em vez de uma camada de
mapeamento por action porque (a) é uma correção só, não 17+ repetições, e (b) a
Fase 4b (workers/MCP, hoje 100% `snake_case`) herda o mesmo contrato de graça.

**FAIL 2 — numeric vira string.** Colunas `numeric` (`confidence_threshold`,
`credit_limit`, `quantity`, `unit_price`, `subtotal` e outras — ver
`schema.ts:119,253,325-327,381,473,692` e qualquer outra encontrada por
`grep -n "numeric(" packages/db/src/schema.ts`) estão sem `mode`, e o padrão do
Drizzle pra `numeric` sem `mode` é devolver `string`. O contrato de hoje (via
PostgREST/`database.types.ts`) é `number`. `amount` virando `"123.45"` quebra
aritmética silenciosamente.

**Correção escolhida:** adicionar `mode: "number"` em toda coluna `numeric(...)`
do schema (precisão máxima usada no schema é 15 dígitos com escala até 6 — cabe em
`double precision`/JS `number` sem perda para os valores deste app).
**Timestamps não mudam** — já estão com `mode: "string"`, que já bate com o
contrato atual; não mexer neles.

**Passos (primeira tarefa do plano de implementação, antes de qualquer Server
Action):**

1. Editar `packages/db/drizzle.config.ts`: adicionar `introspect: { casing:
"preserve" }`.
2. Rodar `bun run db:introspect` (dentro de `packages/db`) contra o Neon real.
3. Mover a saída (`drizzle/schema.ts` → `src/schema.ts`,
   `drizzle/relations.ts` → `src/relations.ts` — mesmo processo documentado no
   cabeçalho de `schema.ts` hoje).
4. Reaplicar as 2 edições manuais da Fase 1 (documentadas no cabeçalho de
   `schema.ts`: quebra de ciclo `AnyPgColumn` em `transactions`/
   `recurringInstances`, `table`→`_table` em `user_secrets`) — os nomes de
   propriedade dessas edições também mudam de camelCase pra snake_case.
5. Adicionar `mode: "number"` em toda coluna `numeric(...)` (usar
   `grep -c "numeric(" src/schema.ts` antes e depois de editar pra confirmar que
   nenhuma ficou de fora).
6. `bun run typecheck` — os dois arquivos que já consomem `@sbf/db` hoje
   (`packages/db/src/client.test.ts`, `__tests__/integration/db-neon-smoke.test.ts`)
   não deveriam quebrar (não acessam propriedades específicas de tabela que mudam
   de nome); se algum acessar, corrigir o nome da propriedade ali também.
7. Commit isolado (não misturar com as Server Actions).

## Padrão de query — exemplo completo (`institutions.ts`)

Arquivo mais simples do conjunto (5 funções, sem lógica extra) — serve de
referência literal pro resto do Apêndice A.

**Antes** (`apps/web/src/app/actions/institutions.ts`, hoje):

```ts
"use server";

import { createClient } from "@/lib/supabase/server";
import type { Institution, Insert, Update } from "@sbf/shared-types";

type InstitutionInsert = Omit<Insert<"institutions">, "user_id">;
type InstitutionUpdate = Update<"institutions">;

export async function getInstitutions(): Promise<Institution[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("institutions")
    .select("*")
    .order("display_order", { ascending: true, nullsFirst: false })
    .order("name");
  if (error) throw new Error(error.message);
  return data;
}

export async function getInstitution(id: string): Promise<Institution | null> {
  const supabase = await createClient();
  const { data, error } = await supabase.from("institutions").select("*").eq("id", id).single();
  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createInstitution(input: InstitutionInsert): Promise<Institution> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("institutions")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();
  if (error) throw new Error(error.message);
  return data;
}

export async function updateInstitution(
  id: string,
  input: InstitutionUpdate,
): Promise<Institution> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("institutions")
    .update(input)
    .eq("id", id)
    .select()
    .single();
  if (error) throw new Error(error.message);
  return data;
}

export async function deleteInstitution(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("institutions").delete().eq("id", id);
  if (error) throw new Error(error.message);
}
```

**Depois:**

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

**Regras gerais que este exemplo fixa para todo o Apêndice A:**

1. `supabase.auth.getUser()` continua a única fonte de identidade — extraído numa
   função `requireUserId()` local reaproveitada dentro do arquivo (não vira um
   helper compartilhado nesta fase; isso é trabalho da Fase 2, que substitui
   `requireUserId`/`getUser()` de vez).
2. Todo `select`/`update`/`delete` ganha `eq(<tabela>.user_id, userId)` no `where` —
   inclusive os que hoje não filtram nada.
3. Update/delete que afetam 0 linhas (id inexistente OU pertence a outro usuário)
   lançam erro — replica o comportamento atual (`.single()` do supabase-js falha
   nesse caso hoje).
4. `getInstitution` (busca por id) devolve `null` em vez de lançar erro quando não
   encontra — replica o `PGRST116 → null` de hoje.
5. Contrato de cada função (assinatura, tipo de retorno, quando lança erro vs.
   devolve `null`) é **idêntico** ao de hoje (condição 3 da Verônica) — qualquer
   Server Action cujo contrato atual não se encaixe nesse padrão (ver Apêndice A)
   é uma exceção documentada, não uma correção silenciosa.

## Tipos

Os 17 arquivos reescritos usam tipos inferidos do Drizzle
(`typeof schema.<tabela>.$inferSelect` / `$inferInsert`, de
`packages/db/src/schema.ts`, regenerado com `introspect.casing: "preserve"` — ver
seção de pré-requisito acima) em vez de `Insert<T>`/`Update<T>`/tipos de tabela de
`@sbf/shared-types`. Com o casing corrigido, as chaves desses tipos batem
byte-a-byte com as de `@sbf/shared-types` hoje (`snake_case`) — só a origem do tipo
muda, não o formato. `@sbf/shared-types` e `database.types.ts` continuam existindo
sem alteração — servem workers/MCP até a Fase 4b.

## Componentes client-side → Server Action

Os 3 componentes com `.from()` direto no browser ganham Server Actions novas e
finas — a lógica de dados migra pro servidor, o componente só chama a action.

- **`app/dashboard/suppliers/[id]/page.tsx`** (linhas 133, 135, 140, 145, 173, 196,
  221, 243, 265, 289): vira uma `actions/suppliers.ts` (arquivo já existe —
  adicionar) com `getSupplierDetail(id)` (substitui os 4 selects em paralelo de
  suppliers/aliases/contracts/source_documents), `updateSupplierActive(id, isActive)`,
  `deleteSupplierAlias(aliasId)`, `deleteSupplierContract(contractId)`, e as
  mutações de alias/contract já presentes na página. Cada função aplica o mesmo
  padrão de filtro explícito por `userId`.
- **`app/dashboard/import/page.tsx`** (linhas 111, 128, 144, 164, 186): vira
  Server Actions em `actions/import.ts` (novo arquivo) — `createImportJob`,
  `getImportJobStatus`, `createTransactionFromImport`, etc., espelhando as 5
  operações atuais. O upload em si (`supabase.storage.from("user-files").upload`)
  **não muda nesta fase** — é Storage, migra na Fase 3; só a parte de dados
  (`.from("import_jobs")`, `.from("financial_products")`, `.from("transactions")`)
  vira Server Action.
- **`components/ai-chat-drawer.tsx`** (linha 79, `ai_chat_sessions`): vira
  `getOrCreateChatSession()` em `actions/chat-sessions.ts` (novo arquivo). A
  chamada de Storage (`ingestion-originals`, linha 121) e o `functions.invoke`
  (`trigger-ingestion`, linha 138) **não mudam nesta fase** — são Storage/Fase 2,
  não dados de aplicação.

## Funções `SECURITY DEFINER` via SQL cru

Drizzle não tem equivalente a `.rpc()`. Chamada vira `db.execute(sql\`...\`)`:

```ts
import { sql } from "drizzle-orm";

const rows = await db.execute(
  sql`SELECT * FROM fn_get_secrets(${userId}, ${secretType}, ${limit})`,
);
```

`p_user_id` (aqui, o primeiro parâmetro posicional) é sempre `userId` obtido de
`requireUserId()` — nunca um valor recebido como argumento de fora pelo chamador
externo da Server Action. Isso satisfaz a amarração de sessão real exigida pela
Fase 1 (nota em `packages/db/schema.sql`) por construção, do mesmo jeito que o
patch do `trigger-ingestion` na Fase 2.

Call sites a converter (do levantamento da Fase 1): `fn_get_secrets` em
`actions/secrets.ts`; `fn_mark_secret_used` em `actions/secrets.ts`;
`fn_materialize_draft_record` em `actions/materialization.ts`;
`fn_upsert_financial_obligation` — call site a confirmar durante a implementação
(não apareceu nos grep anteriores; se não houver call site ativo em `apps/web`,
documentar como "sem uso hoje, função existe mas não é chamada" em vez de inventar
uma chamada nova).

## Testes

Por arquivo reescrito, no mínimo:

- Um teste de **isolamento por usuário**: cria uma linha para `userId` A e outra
  para `userId` B (dois UUIDs de teste distintos, nunca reais), chama a função sob
  teste como se fosse A, confirma que só a linha de A aparece/é afetada — é o teste
  que prova que o filtro explícito substitui a RLS sem regressão.
- Os testes de caminho feliz normais (criar, buscar, atualizar, excluir).
- Um teste do caso de erro (update/delete de id inexistente ou de outro usuário
  lança o erro esperado).

Além disso, para as tabelas críticas (`transactions`, `financial_obligations`,
`ingestion_runs`, `ingestion_jobs`, `source_documents`, e a própria
`institutions`): um **teste de contrato de forma**, comparando as chaves e os
tipos (`typeof valor`) do objeto retornado pela função reescrita com o shape
esperado hoje (`snake_case` + `number` em campo numérico, não `string`) — é o
teste que teria pego os 2 FAILs desta revisão (casing e numeric-vira-string) antes
de chegar em produção.

Ver "Segurança do banco de testes" acima para a variável de ambiente e limpeza
obrigatórias.

## Apêndice A — Arquivos e funções a reescrever

(Lista vem do levantamento da Fase 1/2; conferir número exato de call sites durante
a implementação.)

`institutions.ts` (ver exemplo completo acima), `cards.ts`, `document-metadata.ts`,
`materialization.ts` (inclui `fn_materialize_draft_record`), `suppliers.ts`
(ganha as funções migradas de `suppliers/[id]/page.tsx`), `financial-periods.ts`,
`pluggy-connections.ts`, `statement-cycles.ts`, `recurring.ts`, `secrets.ts`
(inclui `fn_get_secrets`/`fn_mark_secret_used`), `document-transactions.ts`,
`ingestion.ts`, `transactions.ts`, `liabilities.ts`, `patterns.ts`,
`document-splits.ts`, `financial-products.ts`. Mais os 2 arquivos novos:
`import.ts`, `chat-sessions.ts`.

`lib/ai/tools.ts` (19 call sites, mesmas ferramentas do agente de IA): mesma
técnica de filtro explícito, tratado como parte desta fase já que consome as
mesmas tabelas.

## Apêndice B — Escritas multi-tabela sem transação (decisão adiada, condição 4)

Pontos onde uma escrita hoje é sequência de `.insert()` sem transação, e onde uma
falha no meio deixa estado parcial (já seria um bug hoje, não introduzido por
esta fase) — anotar aqui durante a implementação à medida que forem encontrados,
para decisão separada sobre se vale a pena envolver em transação Drizzle depois.
Exemplo já conhecido: `ingestion_runs` → `source_documents` → `ingestion_jobs` (ver
padrão em `workers/local-scanner/src/scanner.ts:180-196` e na spec da Fase 2).

## Não-objetivos

- Mudar `supabase.auth.getUser()` ou qualquer coisa de sessão/identidade (Fase 2).
- Migrar Storage (Fase 3).
- Migrar workers/MCP (Fase 4b).
- Corrigir escritas multi-tabela sem transação (Apêndice B — decisão separada).
- Deploy/cutover para produção (plano de cutover separado).
