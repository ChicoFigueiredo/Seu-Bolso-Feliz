import { NextResponse } from "next/server";
import { and, eq, sql } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { getUserOrNull } from "@/lib/auth";

interface Params {
  params: Promise<{ draftId: string }>;
}

// `type`, não `interface`: `db.execute<T>` exige `T extends
// Record<string, unknown>`, e só um type alias de objeto ganha o index
// signature implícito que satisfaz essa restrição.
type ProgressRow = {
  total_count: number | string | null;
  reconciled_count: number | string | null;
  progress_pct: number | string | null;
};

/**
 * GET /api/reconciliation/[draftId]/progress
 *
 * Retorna o progresso de reconciliação de um draft_batch via
 * `fn_reconciliation_progress`. O parâmetro de rota se chama `draftId` por
 * histórico, mas o valor é o id de um `draft_batches` (é assim que
 * `components/document-detail-new.tsx` chama).
 *
 * Resposta: { total_count, reconciled_count, progress_pct }
 */
export async function GET(_request: Request, { params }: Params) {
  const { draftId } = await params;
  const user = await getUserOrNull();
  if (!user) {
    return NextResponse.json({ error: "Não autenticado" }, { status: 401 });
  }

  const db = getDb();

  // ⚠️ `fn_reconciliation_progress` é SECURITY DEFINER e recebe SÓ o
  // `p_batch_id`: ela conta os drafts do batch sem saber de quem eles são
  // (dívida registrada na NOTA nº 3 de `packages/db/schema.sql`). Quem
  // autoriza é este call site — sem a checagem abaixo, qualquer sessão
  // válida enumeraria o progresso dos batches de outra pessoa.
  const [batch] = await db
    .select({ id: schema.draft_batches.id })
    .from(schema.draft_batches)
    .where(
      and(eq(schema.draft_batches.id, draftId), eq(schema.draft_batches.user_id, user.ownerId)),
    );

  if (!batch) {
    return NextResponse.json({ error: "Batch não encontrado" }, { status: 404 });
  }

  const result = await db.execute<ProgressRow>(
    sql`SELECT * FROM public.fn_reconciliation_progress(${draftId}::uuid)`,
  );

  const row = result.rows[0];
  if (!row) {
    return NextResponse.json({ total_count: 0, reconciled_count: 0, progress_pct: 0 });
  }

  // `bigint`/`numeric` chegam como string nos drivers do Postgres (tanto no
  // `pg` quanto no HTTP da Neon); o PostgREST entregava número. A UI monta
  // uma barra de progresso com esses valores, então a conversão é feita aqui
  // para o contrato da API continuar numérico.
  return NextResponse.json({
    total_count: Number(row.total_count ?? 0),
    reconciled_count: Number(row.reconciled_count ?? 0),
    progress_pct: Number(row.progress_pct ?? 0),
  });
}
