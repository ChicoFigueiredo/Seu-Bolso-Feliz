/**
 * Orquestra a sincronização de uma `provider_connections` (um item Pluggy):
 * lista transações de cada conta mapeada, gera `draft_records` (mesmo
 * destino do pipeline de documentos, sem OCR/EvidenceEnvelope — ver
 * docs/planejamento/2026-08-24-plano-integracao-pluggy.md), roda o motor de
 * reconciliação existente e nunca duplica uma transação já sincronizada
 * (draft_records.external_ref, UNIQUE por usuário).
 *
 * Cada conta mantém seu próprio checkpoint (ingestion_checkpoints, ver
 * checkpoint.ts) — uma interrupção no meio de um backfill de 365 dias retoma
 * da última página persistida, não do início. Um erro de inserção numa
 * transação específica é registrado e pulado — não derruba o resto da página
 * nem das contas seguintes; a idempotência de external_ref torna reexecutar
 * seguro de qualquer forma.
 */
import type { SupabaseClient } from "@supabase/supabase-js";
import type { FinancialDataProvider } from "@sbf/financial-connectors";
import { findReconciliationCandidates } from "@sbf/worker-ingestion/reconciliation";
import { buildDraftRecordInsert } from "./build-draft";
import { completeCheckpoint, recordPageProgress, startOrResumeCheckpoint } from "./checkpoint";

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
  /**
   * "incremental": sem checkpoint — `last_synced_at` por conexão já basta
   * pra uma janela curta (poucas páginas). "backfill": usa
   * ingestion_checkpoints por conta, retomável entre execuções.
   *
   * Os dois modos NUNCA compartilham checkpoint — um cron incremental
   * disparando no meio de um backfill de 365 dias em andamento não deve
   * "herdar" silenciosamente a página onde o backfill parou. Ver checkpoint.ts.
   */
  mode: "incremental" | "backfill";
}

export interface SyncProviderConnectionResult {
  batchId: string | null;
  accountsSynced: number;
  created: number;
  skippedDuplicate: number;
  skippedIgnored: number;
  itemErrors: number;
}

export async function syncProviderConnection(
  input: SyncProviderConnectionInput,
): Promise<SyncProviderConnectionResult> {
  const { supabase, userId, provider, externalItemId, mappings, from, to, mode } = input;

  const activeMappings = mappings.filter((m) => m.status !== "ignored");
  const skippedIgnored = mappings.length - activeMappings.length;

  if (activeMappings.length === 0) {
    return {
      batchId: null,
      accountsSynced: 0,
      created: 0,
      skippedDuplicate: 0,
      skippedIgnored,
      itemErrors: 0,
    };
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
  let itemErrors = 0;

  for (const mapping of activeMappings) {
    const checkpoint =
      mode === "backfill"
        ? await startOrResumeCheckpoint(
            supabase,
            userId,
            `${externalItemId}:${mapping.externalAccountId}`,
            { from, to },
          )
        : null;

    const window = checkpoint ?? { from, to, startPage: 1 };
    let pageNumber = window.startPage;
    let hasMore = true;

    while (hasMore) {
      const page = await provider.listTransactions(mapping.externalAccountId, {
        from: window.from,
        to: window.to,
        cursor: String(pageNumber),
      });

      let pageCreated = 0;
      let pageDuplicates = 0;
      let pageErrors = 0;

      for (const tx of page.transactions) {
        const insertRow = buildDraftRecordInsert(tx, {
          batchId: batch.id as string,
          userId,
          financialProductId: mapping.financialProductId,
        });

        let draftId: string | undefined;
        try {
          const { data: draft, error: insertError } = await supabase
            .from("draft_records")
            .insert(insertRow)
            .select("id")
            .single();

          if (insertError) {
            if (insertError.code === UNIQUE_VIOLATION) {
              pageDuplicates++;
              continue;
            }
            throw new Error(insertError.message);
          }

          draftId = draft.id as string;
          pageCreated++;
        } catch (err) {
          pageErrors++;
          console.error(
            `[pluggy-sync] falha ao inserir draft (txn ${tx.externalTransactionId}): ${err instanceof Error ? err.message : String(err)}`,
          );
          continue;
        }

        // Reconciliação é uma anotação sobre um draft que já foi criado com
        // sucesso — falhar aqui não desfaz a criação nem conta como item
        // com erro, só fica sem candidato de reconciliação anexado.
        try {
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
            .eq("id", draftId);
        } catch (err) {
          console.error(
            `[pluggy-sync] reconciliação falhou pro draft ${draftId} (não crítico): ${err instanceof Error ? err.message : String(err)}`,
          );
        }
      }

      created += pageCreated;
      skippedDuplicate += pageDuplicates;
      itemErrors += pageErrors;

      if (checkpoint) {
        await recordPageProgress(supabase, checkpoint.id, {
          page: pageNumber,
          transactionsSeen: page.transactions.length,
          created: pageCreated,
          duplicates: pageDuplicates,
          errors: pageErrors,
        });
      }

      hasMore = Boolean(page.nextCursor);
      pageNumber++;
    }

    if (checkpoint) {
      await completeCheckpoint(supabase, checkpoint.id);
    }
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
    itemErrors,
  };
}
