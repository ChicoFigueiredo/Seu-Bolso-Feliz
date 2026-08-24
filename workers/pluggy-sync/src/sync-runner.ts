/**
 * Orquestra a sincronização de uma `provider_connections` (um item Pluggy):
 * lista transações de cada conta mapeada, gera `draft_records` (mesmo
 * destino do pipeline de documentos, sem OCR/EvidenceEnvelope — ver
 * docs/planejamento/2026-08-24-plano-integracao-pluggy.md), roda o motor de
 * reconciliação existente e nunca duplica uma transação já sincronizada
 * (draft_records.external_ref, UNIQUE por usuário).
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import type { FinancialDataProvider } from "@sbf/financial-connectors";
import { findReconciliationCandidates } from "@sbf/worker-ingestion/reconciliation";
import { buildDraftRecordInsert } from "./build-draft";

const UNIQUE_VIOLATION = "23505";

export interface AccountMapping {
  externalAccountId: string;
  financialProductId: string | null;
  /** Contas "ignored" nunca sincronizam — usuário escolheu excluí-las. */
  status: "pending_mapping" | "mapped" | "ignored";
}

export interface SyncProviderConnectionInput {
  supabase: SupabaseClient;
  userId: string;
  provider: FinancialDataProvider;
  externalItemId: string;
  mappings: AccountMapping[];
  from: string;
  to: string;
}

export interface SyncProviderConnectionResult {
  batchId: string | null;
  accountsSynced: number;
  created: number;
  skippedDuplicate: number;
  skippedIgnored: number;
}

export async function syncProviderConnection(
  input: SyncProviderConnectionInput,
): Promise<SyncProviderConnectionResult> {
  const { supabase, userId, provider, mappings, from, to } = input;

  const activeMappings = mappings.filter((m) => m.status !== "ignored");
  const skippedIgnored = mappings.length - activeMappings.length;

  if (activeMappings.length === 0) {
    return { batchId: null, accountsSynced: 0, created: 0, skippedDuplicate: 0, skippedIgnored };
  }

  const { data: batch, error: batchError } = await supabase
    .from("draft_batches")
    .insert({
      user_id: userId,
      name: `Sincronização Pluggy ${new Date().toISOString().split("T")[0]}`,
      status: "open",
    })
    .select("id")
    .single();

  if (batchError || !batch) {
    throw new Error(`Falha ao criar draft_batch: ${batchError?.message}`);
  }

  let created = 0;
  let skippedDuplicate = 0;

  for (const mapping of activeMappings) {
    let cursor: string | undefined;
    do {
      const page = await provider.listTransactions(mapping.externalAccountId, {
        from,
        to,
        cursor,
      });

      for (const tx of page.transactions) {
        const insertRow = buildDraftRecordInsert(tx, {
          batchId: batch.id as string,
          userId,
          financialProductId: mapping.financialProductId,
        });

        const { data: draft, error: insertError } = await supabase
          .from("draft_records")
          .insert(insertRow)
          .select("id")
          .single();

        if (insertError) {
          if (insertError.code === UNIQUE_VIOLATION) {
            skippedDuplicate++;
            continue;
          }
          throw new Error(
            `Falha ao inserir draft_record (${tx.externalTransactionId}): ${insertError.message}`,
          );
        }

        created++;

        const reconciliation = await findReconciliationCandidates(
          supabase,
          userId,
          {
            amount: (insertRow.draft_data as { amount_cents: number }).amount_cents / 100,
            due_date: tx.date,
            competence_date: tx.date,
            supplier_id: null,
            supplier_name: tx.merchantName ?? null,
          },
          null,
        );

        await supabase
          .from("draft_records")
          .update({
            reconciliation_status: reconciliation.status,
            reconciliation_candidates: reconciliation.candidates,
            reconciled_at: new Date().toISOString(),
          })
          .eq("id", draft!.id);
      }

      cursor = page.nextCursor;
    } while (cursor);
  }

  await supabase
    .from("draft_batches")
    .update({ total_drafts: created })
    .eq("id", batch.id as string);

  return {
    batchId: batch.id as string,
    accountsSynced: activeMappings.length,
    created,
    skippedDuplicate,
    skippedIgnored,
  };
}
