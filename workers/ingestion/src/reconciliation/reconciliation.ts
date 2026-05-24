/**
 * Módulo de reconciliação determinística de drafts.
 *
 * Fase E do Marco 5. Objetivo: antes de criar um draft, verificar se já existe
 * uma transação ou template recorrente que corresponde ao documento ingerido.
 *
 * Regras de match:
 * 1. Mesmo supplier_id + amount ±5% + due_date ±7 dias → match_exact
 * 2. Mesmo supplier_id + amount ±5% (sem data exata) → match_fuzzy
 * 3. Mesmo content_hash (via source_documents) → match_duplicate
 * 4. Mesmo supplier_id + recurrência mensal esperada → match_recurrence
 *
 * Nenhuma regra é aplicada automaticamente: o resultado orienta o usuário.
 */

import type { SupabaseClient } from "@supabase/supabase-js";

// ── Tipos ──────────────────────────────────────────────────────────────────

export type ReconciliationStatus =
  | "not_checked"
  | "no_match"
  | "match_exact"
  | "match_fuzzy"
  | "match_duplicate"
  | "match_recurrence"
  | "confirmed_new"
  | "confirmed_duplicate";

export interface ReconciliationCandidate {
  transactionId: string | null;
  recurringTemplateId: string | null;
  matchType: Exclude<ReconciliationStatus, "not_checked" | "no_match" | "confirmed_new" | "confirmed_duplicate">;
  score: number; // 0.0-1.0 — quanto maior, mais confiante o match
  description: string;
  /** Dados resumidos do candidato para exibição */
  candidateData: {
    amount: number | null;
    date: string | null;
    supplierName: string | null;
    category: string | null;
  };
}

export interface ReconciliationResult {
  status: ReconciliationStatus;
  candidates: ReconciliationCandidate[];
  checkedAt: string;
}

// ── Helpers ────────────────────────────────────────────────────────────────

/** Retorna true se dois valores numéricos diferem em menos de `pct` (ex: 0.05 = 5%) */
function amountWithinPct(a: number, b: number, pct = 0.05): boolean {
  if (a === 0 && b === 0) return true;
  const base = Math.max(Math.abs(a), Math.abs(b));
  return Math.abs(a - b) / base <= pct;
}

/** Retorna true se duas datas (ISO strings) diferem em menos de `days` dias */
function dateWithinDays(a: string, b: string, days = 7): boolean {
  const da = new Date(a).getTime();
  const db = new Date(b).getTime();
  if (isNaN(da) || isNaN(db)) return false;
  return Math.abs(da - db) <= days * 24 * 60 * 60 * 1000;
}

/** Extrai o mês/ano de uma data ISO como chave de competência */
function competenceKey(date: string): string {
  const d = new Date(date);
  if (isNaN(d.getTime())) return "";
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}`;
}

// ── Engine principal ───────────────────────────────────────────────────────

/**
 * Busca candidatos de reconciliação para os dados extraídos de um documento.
 *
 * @param supabase  Cliente Supabase autenticado
 * @param userId    ID do usuário dono do draft
 * @param draftData Dados do draft (draft_data JSONB) a ser conciliado
 * @param sourceDocumentId  ID do source_document (para checar duplicate hash)
 */
export async function findReconciliationCandidates(
  supabase: SupabaseClient,
  userId: string,
  draftData: Record<string, unknown>,
  sourceDocumentId: string | null,
): Promise<ReconciliationResult> {
  const candidates: ReconciliationCandidate[] = [];

  const amount =
    typeof draftData.amount === "number"
      ? draftData.amount
      : typeof draftData.total_amount === "number"
        ? draftData.total_amount
        : null;

  const dueDate =
    typeof draftData.due_date === "string"
      ? draftData.due_date
      : typeof draftData.dueDate === "string"
        ? draftData.dueDate
        : null;

  const supplierId =
    typeof draftData.supplier_id === "string" ? draftData.supplier_id : null;

  const supplierName =
    typeof draftData.supplier_name === "string"
      ? draftData.supplier_name
      : typeof draftData.supplierNameRaw === "string"
        ? draftData.supplierNameRaw
        : null;

  const competenceDate =
    typeof draftData.competence_date === "string"
      ? draftData.competence_date
      : typeof draftData.competenceDate === "string"
        ? draftData.competenceDate
        : null;

  // ── Regra 1: Duplicate hash ────────────────────────────────────────────
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
        // Buscar drafts aprovados originados desses documentos
        const dupeDocIds = dupes.map((d) => d.id);
        const { data: dupeDrafts } = await supabase
          .from("draft_records")
          .select("id, draft_data, created_at")
          .eq("user_id", userId)
          .in("source_document_id", dupeDocIds)
          .in("status", ["approved", "posted"])
          .limit(1);

        if (dupeDrafts && dupeDrafts.length > 0) {
          const dd = dupeDrafts[0]!;
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

  // ── Regra 2: Match exato por supplier + amount + data ─────────────────
  if (supplierId && amount !== null && dueDate) {
    // Janela de 60 dias ao redor da due_date para busca eficiente
    const fromDate = new Date(new Date(dueDate).getTime() - 60 * 24 * 60 * 60 * 1000)
      .toISOString()
      .split("T")[0];
    const toDate = new Date(new Date(dueDate).getTime() + 60 * 24 * 60 * 60 * 1000)
      .toISOString()
      .split("T")[0];

    const { data: txns } = await (supabase as unknown as Record<string, unknown>["from"] extends never ? never : typeof supabase)
      .from("transactions" as never)
      .select("id, amount, transaction_date, supplier_name, category")
      .eq("user_id", userId)
      .eq("supplier_id", supplierId)
      .gte("transaction_date", fromDate!)
      .lte("transaction_date", toDate!)
      .limit(10) as unknown as { data: Array<{ id: string; amount: number; transaction_date: string; supplier_name: string | null; category: string | null }> | null };

    for (const txn of txns ?? []) {
      if (!amountWithinPct(amount, txn.amount)) continue;

      const isExact = dateWithinDays(dueDate, txn.transaction_date, 7);
      candidates.push({
        transactionId: txn.id,
        recurringTemplateId: null,
        matchType: isExact ? "match_exact" : "match_fuzzy",
        score: isExact ? 0.95 : 0.7,
        description: isExact
          ? `Transação com mesmo fornecedor, valor e data encontrada.`
          : `Transação com mesmo fornecedor e valor semelhante (data difere).`,
        candidateData: {
          amount: txn.amount,
          date: txn.transaction_date,
          supplierName: txn.supplier_name,
          category: txn.category,
        },
      });
    }
  }

  // ── Regra 3: Match por recorrência (template mensal mesmo competence) ──
  if (supplierId && competenceDate) {
    const { data: templates } = await (supabase as unknown as typeof supabase)
      .from("recurring_templates" as never)
      .select("id, name, amount, frequency, supplier_id")
      .eq("user_id", userId)
      .eq("supplier_id", supplierId)
      .eq("is_active", true)
      .eq("frequency", "monthly")
      .limit(5) as unknown as { data: Array<{ id: string; name: string; amount: number; frequency: string; supplier_id: string }> | null };

    for (const tpl of templates ?? []) {
      if (amount !== null && !amountWithinPct(amount, tpl.amount, 0.15)) continue;

      // Verificar se já existe instância no mesmo mês/ano
      const compKey = competenceKey(competenceDate);
      const { data: instances } = await (supabase as unknown as typeof supabase)
        .from("recurring_instances" as never)
        .select("id, due_date, status")
        .eq("user_id", userId)
        .eq("template_id", tpl.id)
        .gte("due_date", `${compKey}-01`)
        .lte("due_date", `${compKey}-31`)
        .limit(1) as unknown as { data: Array<{ id: string; due_date: string; status: string }> | null };

      const hasInstance = instances && instances.length > 0;
      candidates.push({
        transactionId: null,
        recurringTemplateId: tpl.id,
        matchType: "match_recurrence",
        score: hasInstance ? 0.85 : 0.6,
        description: hasInstance
          ? `Template recorrente "${tpl.name}" tem instância prevista neste período.`
          : `Template recorrente "${tpl.name}" corresponde ao fornecedor (sem instância confirmada).`,
        candidateData: {
          amount: tpl.amount,
          date: hasInstance ? instances![0]!.due_date : null,
          supplierName: supplierName,
          category: null,
        },
      });
    }
  }

  // ── Resultado final ────────────────────────────────────────────────────
  // Ordenar por score decrescente
  candidates.sort((a, b) => b.score - a.score);

  let status: ReconciliationStatus = "no_match";
  if (candidates.length > 0) {
    status = candidates[0]!.matchType;
  }

  return {
    status,
    candidates,
    checkedAt: new Date().toISOString(),
  };
}

/**
 * Determina se o draft deve ser marcado como potencial duplicata
 * (para exibir alerta na UI, não para bloquear automaticamente).
 */
export function isDuplicateRisk(result: ReconciliationResult): boolean {
  return result.candidates.some(
    (c) => c.matchType === "match_duplicate" || (c.matchType === "match_exact" && c.score >= 0.9),
  );
}
