/**
 * Manutenção — aprova e lança TODOS os batches abertos da sincronização
 * Pluggy, em lote.
 *
 * Executar este script É o ato de aprovação humana (§9 do plano mestre:
 * nenhuma IA grava registro financeiro sozinha — quem roda isto é você).
 * Transações vindas do banco são fatos que já aconteceram; a revisão fina
 * (categoria, fornecedor, rateio) continua possível depois, em Movimentações.
 *
 * Usa exatamente as mesmas funções das tools MCP `approve_draft_batch` e
 * `post_draft_batch` — inclusive `fn_materialize_draft_record`, que valida os
 * invariantes no banco. Nada aqui inventa caminho novo.
 */
import { and, eq, like, inArray, sql } from "drizzle-orm";
import { getDb, resolveOwnerId, schema } from "@sbf/db";
import { approveDraftBatch } from "./tools/approve-draft-batch.js";
import { postDraftBatch } from "./tools/post-draft-batch.js";

const db = getDb();
const ownerId = await resolveOwnerId();

const batches = await db
  .select({ id: schema.draft_batches.id, name: schema.draft_batches.name })
  .from(schema.draft_batches)
  .where(
    and(
      eq(schema.draft_batches.user_id, ownerId),
      inArray(schema.draft_batches.status, ["open", "reviewing"]),
      like(schema.draft_batches.name, "Sincronização Pluggy%"),
      sql`exists (select 1 from ${schema.draft_records}
           where ${schema.draft_records.batch_id} = ${schema.draft_batches.id}
             and ${schema.draft_records.status} = 'pending_review')`,
    ),
  );

if (batches.length === 0) {
  console.log("Nenhum batch do Pluggy com drafts pendentes — nada a fazer.");
  process.exit(0);
}

let aprovados = 0;
let lancados = 0;
let falhas = 0;

for (const batch of batches) {
  const aprovacao = await approveDraftBatch(db, ownerId, batch.id);
  aprovados += aprovacao.approvedCount;

  const lancamento = await postDraftBatch(db, ownerId, batch.id);
  lancados += lancamento.succeeded;
  falhas += lancamento.failed;
  console.log(
    `${batch.name} (${batch.id.slice(0, 8)}…): ` +
      `${aprovacao.approvedCount} aprovados, ${lancamento.succeeded} lançados, ${lancamento.failed} falhas`,
  );

  // Falha aqui é exceção, não rotina — mostrar as primeiras para diagnóstico.
  for (const r of lancamento.results.filter((x) => !x.success).slice(0, 5)) {
    console.log(`  ✗ ${r.draftId.slice(0, 8)}…: ${r.message} ${r.validationErrors.join("; ")}`);
  }
}

const [contagem] = await db
  .select({ total: sql<number>`count(*)::int` })
  .from(schema.transactions)
  .where(eq(schema.transactions.user_id, ownerId));

console.log(`\nTotal: ${aprovados} aprovados, ${lancados} lançados, ${falhas} falhas.`);
console.log(`Transações na sua conta agora: ${contagem?.total ?? 0}.`);
console.log("Abra /dashboard/movimentacoes — elas estão lá.");
if (falhas > 0) process.exit(1);
