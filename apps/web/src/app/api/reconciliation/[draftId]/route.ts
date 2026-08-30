/**
 * API Route: GET /api/reconciliation/[draftId]
 *
 * Retorna candidatos de reconciliação para um draft específico.
 * Executa a engine determinística on-demand (útil para re-checar ao vivo).
 *
 * Também aceita PATCH para confirmar uma decisão do usuário:
 *   { action: "confirmed_new" | "confirmed_duplicate", transactionId? }
 *
 * ⚠️ Sem RLS: cada consulta abaixo carrega o filtro `user_id = user.ownerId`
 * explicitamente. `ownerId` (o dono dos dados), nunca `id` (quem está
 * agindo) — ver `@/lib/auth`.
 */

import { NextRequest, NextResponse } from "next/server";
import { and, eq, gte, inArray, lte, ne } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import { getUserOrNull } from "@/lib/auth";
import type { Json } from "@sbf/shared-types";

// Cópia independente das regras de workers/ingestion/src/reconciliation/reconciliation.ts
// (duplicação catalogada em docs/specs/00-estado-real.md item 6) — as duas precisam ficar
// em sincronia manualmente até serem extraídas para um módulo compartilhado.

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

/** Data ISO (YYYY-MM-DD) deslocada em N dias. */
function isoDatePlusDays(base: string, days: number): string | null {
  const ms = new Date(base).getTime();
  if (isNaN(ms)) return null;
  return new Date(ms + days * 24 * 60 * 60 * 1000).toISOString().slice(0, 10);
}

async function runReconciliation(
  ownerId: string,
  draftData: Record<string, unknown>,
  sourceDocumentId: string | null,
): Promise<{ status: ReconciliationStatus; candidates: ReconciliationCandidate[] }> {
  const db = getDb();
  const candidates: ReconciliationCandidate[] = [];

  const amount = typeof draftData.amount === "number" ? draftData.amount : null;
  const dueDate = typeof draftData.due_date === "string" ? draftData.due_date : null;
  const supplierId = typeof draftData.supplier_id === "string" ? draftData.supplier_id : null;
  const supplierName = typeof draftData.supplier_name === "string" ? draftData.supplier_name : null;
  const competenceDate =
    typeof draftData.competence_date === "string" ? draftData.competence_date : null;

  // Regra 1: Duplicate hash
  if (sourceDocumentId) {
    // Filtro por dono também AQUI: a leitura do hash do documento de origem
    // não tinha `user_id` e dependia do RLS do Supabase para não vazar o
    // conteúdo de documentos alheios. No Neon isso viraria leitura
    // cross-user, então o filtro é explícito.
    const [srcDoc] = await db
      .select({ content_hash: schema.source_documents.content_hash })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.id, sourceDocumentId),
          eq(schema.source_documents.user_id, ownerId),
        ),
      );

    if (srcDoc?.content_hash) {
      const dupes = await db
        .select({ id: schema.source_documents.id })
        .from(schema.source_documents)
        .where(
          and(
            eq(schema.source_documents.user_id, ownerId),
            eq(schema.source_documents.content_hash, srcDoc.content_hash),
            ne(schema.source_documents.id, sourceDocumentId),
          ),
        )
        .limit(3);

      if (dupes.length > 0) {
        const dupeDrafts = await db
          .select({
            id: schema.draft_records.id,
            draft_data: schema.draft_records.draft_data,
            created_at: schema.draft_records.created_at,
          })
          .from(schema.draft_records)
          .where(
            and(
              eq(schema.draft_records.user_id, ownerId),
              inArray(
                schema.draft_records.source_document_id,
                dupes.map((d) => d.id),
              ),
              inArray(schema.draft_records.status, ["approved", "posted"]),
            ),
          )
          .limit(1);

        const dd = dupeDrafts[0];
        if (dd) {
          const ddData = (dd.draft_data ?? {}) as Record<string, unknown>;
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
    const fromDate = isoDatePlusDays(dueDate, -60);
    const toDate = isoDatePlusDays(dueDate, 60);

    if (fromDate && toDate) {
      const txns = await db
        .select({
          id: schema.transactions.id,
          amount: schema.transactions.amount,
          event_date: schema.transactions.event_date,
          category_id: schema.transactions.category_id,
        })
        .from(schema.transactions)
        .where(
          and(
            eq(schema.transactions.user_id, ownerId),
            eq(schema.transactions.supplier_id, supplierId),
            gte(schema.transactions.event_date, fromDate),
            lte(schema.transactions.event_date, toDate),
          ),
        )
        .limit(10);

      for (const txn of txns) {
        if (!amountWithinPct(amount, txn.amount)) continue;
        const isExact = dateWithinDays(dueDate, txn.event_date, 7);
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
            date: txn.event_date,
            supplierName: supplierName,
            category: txn.category_id,
          },
        });
      }
    }
  }

  // Regra 3: Recorrência mensal
  if (supplierId && competenceDate) {
    const templates = await db
      .select({
        id: schema.recurring_templates.id,
        name: schema.recurring_templates.name,
        amount: schema.recurring_templates.amount,
        frequency: schema.recurring_templates.frequency,
        supplier_id: schema.recurring_templates.supplier_id,
      })
      .from(schema.recurring_templates)
      .where(
        and(
          eq(schema.recurring_templates.user_id, ownerId),
          eq(schema.recurring_templates.supplier_id, supplierId),
          eq(schema.recurring_templates.is_active, true),
          eq(schema.recurring_templates.frequency, "monthly"),
        ),
      )
      .limit(5);

    for (const tpl of templates) {
      // `amount` é anulável em recurring_templates (template de valor
      // variável): sem valor não há como comparar, então a regra de 15% é
      // pulada em vez de tratar null como zero.
      if (amount !== null && tpl.amount !== null && !amountWithinPct(amount, tpl.amount, 0.15))
        continue;
      const compKey = competenceKey(competenceDate);
      const instances = await db
        .select({
          id: schema.recurring_instances.id,
          expected_date: schema.recurring_instances.expected_date,
          status: schema.recurring_instances.status,
        })
        .from(schema.recurring_instances)
        .where(
          and(
            eq(schema.recurring_instances.user_id, ownerId),
            eq(schema.recurring_instances.recurring_template_id, tpl.id),
            gte(schema.recurring_instances.expected_date, `${compKey}-01`),
            lte(schema.recurring_instances.expected_date, `${compKey}-31`),
          ),
        )
        .limit(1);

      const firstInstance = instances[0];
      candidates.push({
        transactionId: null,
        recurringTemplateId: tpl.id,
        matchType: "match_recurrence",
        score: firstInstance ? 0.85 : 0.6,
        description: firstInstance
          ? `Template recorrente "${tpl.name}" tem instância prevista neste período.`
          : `Template recorrente "${tpl.name}" corresponde ao fornecedor.`,
        candidateData: {
          amount: tpl.amount,
          date: firstInstance ? firstInstance.expected_date : null,
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
  const user = await getUserOrNull();
  if (!user) return NextResponse.json({ error: "Não autenticado" }, { status: 401 });

  const db = getDb();

  // Buscar draft com dados necessários
  const [draft] = await db
    .select({
      id: schema.draft_records.id,
      draft_data: schema.draft_records.draft_data,
      source_document_id: schema.draft_records.source_document_id,
      reconciliation_status: schema.draft_records.reconciliation_status,
      reconciliation_candidates: schema.draft_records.reconciliation_candidates,
      reconciled_at: schema.draft_records.reconciled_at,
    })
    .from(schema.draft_records)
    .where(
      and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.ownerId)),
    );

  if (!draft) {
    return NextResponse.json({ error: "Draft não encontrado" }, { status: 404 });
  }

  // Se já foi checado e tem candidatos, retornar cache
  if (
    draft.reconciliation_status !== "not_checked" &&
    Array.isArray(draft.reconciliation_candidates) &&
    draft.reconciliation_candidates.length > 0
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
  const draftData = (draft.draft_data ?? {}) as Record<string, unknown>;
  const { status, candidates } = await runReconciliation(
    user.ownerId,
    draftData,
    draft.source_document_id,
  );

  // Persistir resultado
  await db
    .update(schema.draft_records)
    .set({
      reconciliation_status: status,
      reconciliation_candidates: candidates as unknown as Json,
      reconciled_at: new Date().toISOString(),
    })
    .where(
      and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.ownerId)),
    );

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
  const user = await getUserOrNull();
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

  // Tipado pelo $inferInsert da tabela, não Record<string, unknown>: uma
  // coluna inexistente precisa continuar sendo erro de compilação.
  const updateData: Partial<typeof schema.draft_records.$inferInsert> = {
    reconciliation_status: body.action,
  };

  const db = getDb();

  if (body.action === "confirmed_duplicate" && body.transactionId) {
    // IDOR fechado aqui: `transactionId` vem do cliente e vira a FK
    // `reconciled_transaction_id`. Sem esta checagem, o usuário A poderia
    // apontar o próprio draft para uma transação do usuário B — e a tela de
    // reconciliação passaria a exibir dados alheios como "duplicata
    // confirmada". Mesmo padrão de `app/actions/transactions.ts`.
    const [txn] = await db
      .select({ id: schema.transactions.id })
      .from(schema.transactions)
      .where(
        and(
          eq(schema.transactions.id, body.transactionId),
          eq(schema.transactions.user_id, user.ownerId),
        ),
      );

    if (!txn) {
      return NextResponse.json({ error: "Transação não encontrada" }, { status: 404 });
    }

    updateData.reconciled_transaction_id = body.transactionId;
  }

  // `.returning()` para distinguir "atualizei" de "não existe / não é seu" —
  // antes o retorno era `success: true` mesmo com zero linhas afetadas.
  const updated = await db
    .update(schema.draft_records)
    .set(updateData)
    .where(
      and(eq(schema.draft_records.id, draftId), eq(schema.draft_records.user_id, user.ownerId)),
    )
    .returning({ id: schema.draft_records.id });

  if (updated.length === 0) {
    return NextResponse.json({ error: "Draft não encontrado" }, { status: 404 });
  }

  return NextResponse.json({ success: true, draftId, action: body.action });
}
