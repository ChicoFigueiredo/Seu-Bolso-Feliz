import { NextResponse } from "next/server";
import { createClient } from "@/lib/supabase/server";

interface Params {
  params: Promise<{ draftId: string }>;
}

/**
 * GET /api/reconciliation/[draftId]/progress
 *
 * Retorna o progresso de reconciliação de um draft_batch via fn_reconciliation_progress.
 * Resposta: { total_count, reconciled_count, progress_pct }
 */
export async function GET(_request: Request, { params }: Params) {
  const { draftId } = await params;
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) {
    return NextResponse.json({ error: "Não autenticado" }, { status: 401 });
  }

  const { data, error } = await supabase.rpc("fn_reconciliation_progress", {
    p_batch_id: draftId,
  });

  if (error) {
    return NextResponse.json({ error: error.message }, { status: 500 });
  }

  // rpc retorna array de rows (RETURNS TABLE)
  const row = Array.isArray(data) ? data[0] : data;

  return NextResponse.json(row ?? { total_count: 0, reconciled_count: 0, progress_pct: 0 });
}
