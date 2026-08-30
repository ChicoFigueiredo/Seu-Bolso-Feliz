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
import { and, eq } from "drizzle-orm";
import { schema, type Database } from "@sbf/db";
import type { FinancialDataProvider } from "@sbf/financial-connectors";
import { findReconciliationCandidates } from "@sbf/worker-ingestion/reconciliation";
import { resolveSupplierForTransaction, type SupplierAliasCandidate } from "@sbf/domain";
import { buildDraftRecordInsert } from "./build-draft";
import { completeCheckpoint, recordPageProgress, startOrResumeCheckpoint } from "./checkpoint";

/** SQLSTATE de unique_violation — ver isUniqueViolation. */
const UNIQUE_VIOLATION = "23505";

/** Tipos das colunas de draft_records, pra não repetir `typeof ... .$inferInsert`. */
type DraftRecordColumns = typeof schema.draft_records.$inferInsert;

function pgErrorCode(err: unknown): string | undefined {
  if (typeof err !== "object" || err === null) return undefined;
  const { code } = err as { code?: unknown };
  return typeof code === "string" ? code : undefined;
}

/**
 * Duplicata = transação já sincronizada (unique de draft_records.external_ref).
 *
 * O PostgREST devolvia o SQLSTATE em `{ error: { code } }`; o driver `pg`/Neon
 * LANÇA a exceção com o código em `err.code`. O Drizzle, por sua vez, embrulha
 * o erro do driver num `DrizzleQueryError`, preservando o original em
 * `err.cause` — por isso os dois níveis são checados.
 *
 * Nunca casar por texto da mensagem: o SQLSTATE é o único contrato estável
 * (a mensagem muda com a versão do Postgres e com o locale).
 */
export function isUniqueViolation(err: unknown): boolean {
  if (pgErrorCode(err) === UNIQUE_VIOLATION) return true;
  const cause =
    typeof err === "object" && err !== null ? (err as { cause?: unknown }).cause : undefined;
  return pgErrorCode(cause) === UNIQUE_VIOLATION;
}

export interface AccountMapping {
  externalAccountId: string;
  financialProductId: string | null;
  /** Contas "ignored" nunca sincronizam — usuário escolheu excluí-las. */
  status: "pending_mapping" | "mapped" | "ignored";
}

export interface SyncProviderConnectionInput {
  db: Database;
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
  const { db, userId, provider, externalItemId, mappings, from, to, mode } = input;

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

  let batch: { id: string } | undefined;
  try {
    [batch] = await db
      .insert(schema.draft_batches)
      .values({
        user_id: userId,
        name: `Sincronização Pluggy ${new Date().toISOString().split("T")[0]}`,
        status: "open",
      })
      .returning({ id: schema.draft_batches.id });
  } catch (err) {
    throw new Error(
      `Falha ao criar draft_batch: ${err instanceof Error ? err.message : String(err)}`,
      { cause: err },
    );
  }

  if (!batch) {
    throw new Error("Falha ao criar draft_batch: insert não retornou linha");
  }

  const batchId = batch.id;

  // Buscado uma vez por sync (não por transação) — supplier_aliases não tem
  // coluna normalizada, então o match exato compara candidatos já filtrados
  // por user_id em memória (ver resolveSupplierForTransaction, @sbf/domain).
  let aliasRows: { supplier_id: string; alias_name: string }[] = [];
  try {
    aliasRows = await db
      .select({
        supplier_id: schema.supplier_aliases.supplier_id,
        alias_name: schema.supplier_aliases.alias_name,
      })
      .from(schema.supplier_aliases)
      .where(
        and(
          eq(schema.supplier_aliases.user_id, userId),
          eq(schema.supplier_aliases.is_active, true),
        ),
      );
  } catch (err) {
    console.error(
      `[pluggy-sync] falha ao buscar supplier_aliases (não crítico): ${err instanceof Error ? err.message : String(err)}`,
    );
  }

  const supplierAliasCandidates: SupplierAliasCandidate[] = aliasRows.map((row) => ({
    supplierId: row.supplier_id,
    aliasName: row.alias_name,
    isActive: true,
  }));

  let created = 0;
  let skippedDuplicate = 0;
  let itemErrors = 0;

  for (const mapping of activeMappings) {
    const checkpoint =
      mode === "backfill"
        ? await startOrResumeCheckpoint(
            db,
            userId,
            `${externalItemId}:${mapping.externalAccountId}`,
            {
              from,
              to,
            },
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
        const supplierResolution = resolveSupplierForTransaction(
          tx.merchantName ?? "",
          supplierAliasCandidates,
        );
        const supplierId =
          supplierResolution.status === "matched" ? supplierResolution.supplierId : null;

        const insertRow = buildDraftRecordInsert(tx, {
          batchId,
          userId,
          financialProductId: mapping.financialProductId,
          supplierId,
        });

        let draftId: string | undefined;
        try {
          // `materialization_key` é NOT NULL no schema mas preenchida pelo
          // trigger BEFORE INSERT `fn_draft_records_set_materialization_key`
          // (ver packages/db/schema.sql). O Drizzle não modela triggers, então
          // o tipo de insert a exige mesmo sendo correto omiti-la em runtime —
          // daí o cast, restrito a esta chamada.
          const [draft] = await db
            .insert(schema.draft_records)
            .values(insertRow as unknown as DraftRecordColumns)
            .returning({ id: schema.draft_records.id });

          if (!draft) {
            throw new Error("insert de draft_records não retornou linha");
          }

          draftId = draft.id;
          pageCreated++;
        } catch (err) {
          if (isUniqueViolation(err)) {
            pageDuplicates++;
            continue;
          }
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
            db,
            userId,
            {
              amount: (insertRow.draft_data as { amount_cents: number }).amount_cents / 100,
              due_date: tx.date,
              competence_date: tx.date,
              supplier_id: supplierId,
              supplier_name: tx.merchantName ?? null,
            },
            null,
          );

          await db
            .update(schema.draft_records)
            .set({
              reconciliation_status: reconciliation.status,
              // `candidates` é uma interface nomeada; a coluna é `jsonb` tipada
              // como `Json` (união estrutural, sem index signature) — o cast é
              // só de forma, o valor já é JSON puro.
              reconciliation_candidates:
                reconciliation.candidates as unknown as DraftRecordColumns["reconciliation_candidates"],
              reconciled_at: new Date().toISOString(),
            })
            .where(eq(schema.draft_records.id, draftId));
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
        await recordPageProgress(db, checkpoint.id, {
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
      await completeCheckpoint(db, checkpoint.id);
    }
  }

  await db
    .update(schema.draft_batches)
    .set({ total_drafts: created })
    .where(eq(schema.draft_batches.id, batchId));

  return {
    batchId,
    accountsSynced: activeMappings.length,
    created,
    skippedDuplicate,
    skippedIgnored,
    itemErrors,
  };
}
