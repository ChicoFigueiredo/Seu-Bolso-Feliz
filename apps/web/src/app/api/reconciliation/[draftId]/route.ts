/**
 * API Route: GET /api/reconciliation/[draftId]
 *
 * Retorna candidatos de reconciliação para um draft específico.
 * Executa a engine determinística on-demand (útil para re-checar ao vivo).
 *
 * Também aceita PATCH para confirmar uma decisão do usuário:
 *   { action: "confirmed_new" | "confirmed_duplicate", transactionId? }
 */

import { NextRequest, NextResponse } from "next/server";
import { createClient } from "@/lib/supabase/server";
import type { Json } from "@sbf/shared-types";

// Engine de reconciliação reutilizada do worker (lógica pura, sem dependências de runtime)
// Importamos diretamente as heurísticas para evitar duplicação

type ReconciliationStatus =
  | "not_checked"
  | "no_match"
  | "match_exact"
  | "match_fuzzy"
  | "match_duplicate"
  | "match_recurrence"
  | "confirmed_new"
  | "confirmed_duplicate";

interface ReconciliationCandidate {
  transactionId: string | null;
  recurringTemplateId: string | null;
  matchType: string;
  score: number;
  description: string;
  candidateData: {
    amount: number | null;
    date: string | null;
    supplierName: string | null;
    category: string | null;
  };
}

function amountWithinPct(a: number, b: number, pct = 0.05): boolean {
  if (a === 0 && b === 0) return true;
  const base = Math.max(Math.abs(a), Math.abs(b));
  return Math.abs(a - b) / base <= pct;
}

function dateWithinDays(a: string, b: string, days = 7): boolean {
  const da = new Date(a).getTime();
  const db = new Date(b).getTime();
  if (isNaN(da) || isNaN(db)) return false;
  return Math.abs(da - db) <= days * 24 * 60 * 60 * 1000;
}

function competenceKey(date: string): string {
  const d = new Date(date);
  if (isNaN(d.getTime())) return "";
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

async function runReconciliation(
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  supabase: any,
  userId: string,
  draftData: Record<string, unknown>,
  sourceDocumentId: string | null,
): Promise<{ status: ReconciliationStatus; candidates: ReconciliationCandidate[] }> {
  const candidates: ReconciliationCandidate[] = [];

  const amount = typeof draftData.amount === "number" ? draftData.amount : null;
  const dueDate = typeof draftData.due_date === "string" ? draftData.due_date : null;
  const supplierId = typeof draftData.supplier_id === "string" ? draftData.supplier_id : null;
  const supplierName = typeof draftData.supplier_name === "string" ? draftData.supplier_name : null;
  const competenceDate =
    typeof draftData.competence_date === "string" ? draftData.competence_date : null;

  // Regra 1: Duplicate hash
  if (sourceDocumentId) {
    const { data: srcDoc } = await supabase
      .from("source_documents")
      .select("content_hash")
      .eq("id", sourceDocumentId)
      .single();

    if (srcDoc?.content_hash) {
      const { data: dupes } = await supabase
        .from("source_documents")
        .select("id")
        .eq("user_id", userId)
        .eq("content_hash", srcDoc.content_hash)
        .neq("id", sourceDocumentId)
        .limit(3);

      if (dupes && dupes.length > 0) {
        const { data: dupeDrafts } = await supabase
          .from("draft_records")
          .select("id, draft_data, created_at")
          .eq("user_id", userId)
          .in(
            "source_document_id",
            dupes.map((d: { id: string }) => d.id),
          )
          .in("status", ["approved", "posted"])
          .limit(1);

        if (dupeDrafts && dupeDrafts.length > 0) {
          const dd = dupeDrafts[0];
          const ddData = dd.draft_data as Record<string, unknown>;
          candidates.push({
            transactionId: dd.id,
            recurringTemplateId: null,
            matchType: "match_duplicate",
            score: 1.0,
            description: "Documento idêntico já foi processado anteriormente (mesmo hash).",
            candidateData: {
              amount: typeof ddData.amount === "number" ? ddData.amount : null,
              date: typeof ddData.due_date === "string" ? ddData.due_date : null,
              supplierName: typeof ddData.supplier_name === "string" ? ddData.supplier_name : null,
              category: typeof ddData.category === "string" ? ddData.category : null,
            },
          });
        }
      }
    }
  }

  // Regra 2: Match por supplier + amount + data
  if (supplierId && amount !== null && dueDate) {
    const fromDate = new Date(new Date(dueDate).getTime() - 60 * 24 * 60 * 60 * 1000)
      .toISOString()
      .split("T")[0];
    const toDate = new Date(new Date(dueDate).getTime() + 60 * 24 * 60 * 60 * 1000)
      .toISOString()
      .split("T")[0];

    const { data: txns } = await supabase
      .from("transactions")
      .select("id, amount, transaction_date, supplier_name, category")
      .eq("user_id", userId)
      .eq("supplier_id", supplierId)
      .gte("transaction_date", fromDate)
      .lte("transaction_date", toDate)
      .limit(10);

    for (const txn of txns ?? []) {
      if (!amountWithinPct(amount, txn.amount)) continue;
      const isExact = dateWithinDays(dueDate, txn.transaction_date, 7);
      candidates.push({
        transactionId: txn.id,
        recurringTemplateId: null,
        matchType: isExact ? "match_exact" : "match_fuzzy",
        score: isExact ? 0.95 : 0.7,
        description: isExact
          ? "Transação com mesmo fornecedor, valor e data encontrada."
          : "Transação com mesmo fornecedor e valor semelhante (data difere).",
        candidateData: {
          amount: txn.amount,
          date: txn.transaction_date,
          supplierName: txn.supplier_name,
          category: txn.category,
        },
      });
    }
  }

  // Regra 3: Recorrência mensal
  if (supplierId && competenceDate) {
    const { data: templates } = await supabase
      .from("recurring_templates")
      .select("id, name, amount, frequency, supplier_id")
      .eq("user_id", userId)
      .eq("supplier_id", supplierId)
      .eq("is_active", true)
      .eq("frequency", "monthly")
      .limit(5);

    for (const tpl of templates ?? []) {
      if (amount !== null && !amountWithinPct(amount, tpl.amount, 0.15)) continue;
      const compKey = competenceKey(competenceDate);
      const { data: instances } = await supabase
        .from("recurring_instances")
        .select("id, due_date, status")
        .eq("user_id", userId)
        .eq("template_id", tpl.id)
        .gte("due_date", `${compKey}-01`)
        .lte("due_date", `${compKey}-31`)
        .limit(1);

      const hasInstance = instances && instances.length > 0;
      candidates.push({
        transactionId: null,
        recurringTemplateId: tpl.id,
        matchType: "match_recurrence",
        score: hasInstance ? 0.85 : 0.6,
        description: hasInstance
          ? `Template recorrente "${tpl.name}" tem instância prevista neste período.`
          : `Template recorrente "${tpl.name}" corresponde ao fornecedor.`,
        candidateData: {
          amount: tpl.amount,
          date: hasInstance ? instances[0].due_date : null,
          supplierName,
          category: null,
        },
      });
    }
  }

  candidates.sort((a, b) => b.score - a.score);
  const status: ReconciliationStatus =
    candidates.length > 0 ? (candidates[0]!.matchType as ReconciliationStatus) : "no_match";

  return { status, candidates };
}

// ── GET /api/reconciliation/[draftId] ─────────────────────────────────────

export async function GET(_req: NextRequest, { params }: { params: Promise<{ draftId: string }> }) {
  const { draftId } = await params;
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) return NextResponse.json({ error: "Não autenticado" }, { status: 401 });

  // Buscar draft com dados necessários
  const { data: draft, error } = await supabase
    .from("draft_records")
    .select(
      "id, draft_data, source_document_id, reconciliation_status, reconciliation_candidates, reconciled_at",
    )
    .eq("id", draftId)
    .eq("user_id", user.id)
    .single();

  if (error || !draft) {
    return NextResponse.json({ error: "Draft não encontrado" }, { status: 404 });
  }

  // Se já foi checado e tem candidatos, retornar cache
  if (
    draft.reconciliation_status !== "not_checked" &&
    Array.isArray(draft.reconciliation_candidates) &&
    (draft.reconciliation_candidates as unknown[]).length > 0
  ) {
    return NextResponse.json({
      draftId,
      status: draft.reconciliation_status,
      candidates: draft.reconciliation_candidates,
      checkedAt: draft.reconciled_at,
      cached: true,
    });
  }

  // Executar reconciliação ao vivo
  const draftData = draft.draft_data as Record<string, unknown>;
  const { status, candidates } = await runReconciliation(
    supabase,
    user.id,
    draftData,
    draft.source_document_id as string | null,
  );

  // Persistir resultado
  await supabase
    .from("draft_records")
    .update({
      reconciliation_status: status,
      reconciliation_candidates: candidates as unknown as Json,
      reconciled_at: new Date().toISOString(),
    })
    .eq("id", draftId)
    .eq("user_id", user.id);

  return NextResponse.json({
    draftId,
    status,
    candidates,
    checkedAt: new Date().toISOString(),
    cached: false,
  });
}

// ── PATCH /api/reconciliation/[draftId] — confirmar decisão ──────────────

export async function PATCH(
  req: NextRequest,
  { params }: { params: Promise<{ draftId: string }> },
) {
  const { draftId } = await params;
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) return NextResponse.json({ error: "Não autenticado" }, { status: 401 });

  let body: { action: string; transactionId?: string };
  try {
    body = await req.json();
  } catch {
    return NextResponse.json({ error: "Corpo inválido" }, { status: 400 });
  }

  if (!["confirmed_new", "confirmed_duplicate"].includes(body.action)) {
    return NextResponse.json({ error: "Ação inválida" }, { status: 400 });
  }

  const updateData: Record<string, unknown> = {
    reconciliation_status: body.action,
  };

  if (body.action === "confirmed_duplicate" && body.transactionId) {
    updateData.reconciled_transaction_id = body.transactionId;
  }

  const { error } = await supabase
    .from("draft_records")
    .update(updateData)
    .eq("id", draftId)
    .eq("user_id", user.id);

  if (error) return NextResponse.json({ error: error.message }, { status: 500 });

  return NextResponse.json({ success: true, draftId, action: body.action });
}
