"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, count, eq } from "drizzle-orm";

export type DocumentLinkType = "payment" | "refund" | "installment" | "support";

export type DocumentTransactionLink = typeof schema.document_transactions.$inferSelect;

export interface DocumentTransactionLinkWithTransaction extends DocumentTransactionLink {
  transaction: typeof schema.transactions.$inferSelect;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function listLinkedTransactions(
  sourceDocumentId: string,
): Promise<DocumentTransactionLinkWithTransaction[]> {
  const userId = await requireUserId();
  const db = getDb();

  const rows = await db
    .select({ link: schema.document_transactions, transaction: schema.transactions })
    .from(schema.document_transactions)
    .innerJoin(
      schema.transactions,
      eq(schema.document_transactions.transaction_id, schema.transactions.id),
    )
    .where(
      and(
        eq(schema.document_transactions.source_document_id, sourceDocumentId),
        eq(schema.document_transactions.user_id, userId),
      ),
    )
    .orderBy(asc(schema.document_transactions.created_at));

  return rows.map(({ link, transaction }) => ({ ...link, transaction }));
}

export async function linkTransactionToDocument(
  sourceDocumentId: string,
  transactionId: string,
  linkType: DocumentLinkType,
  options?: { confidence?: number; notes?: string },
): Promise<DocumentTransactionLink> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .insert(schema.document_transactions)
    .values({
      source_document_id: sourceDocumentId,
      transaction_id: transactionId,
      user_id: userId,
      link_type: linkType,
      confidence: options?.confidence ?? null,
      created_by: "user",
      notes: options?.notes ?? null,
    })
    .returning();

  if (!row) throw new Error("Falha ao vincular transação ao documento");
  return row;
}

export async function unlinkTransactionFromDocument(linkId: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.document_transactions)
    .where(
      and(
        eq(schema.document_transactions.id, linkId),
        eq(schema.document_transactions.user_id, userId),
      ),
    )
    .returning({ id: schema.document_transactions.id });

  if (result.length === 0) throw new Error("Vínculo não encontrado");
}

export async function getLinkedTransactionCount(sourceDocumentId: string): Promise<number> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .select({ value: count() })
    .from(schema.document_transactions)
    .where(
      and(
        eq(schema.document_transactions.source_document_id, sourceDocumentId),
        eq(schema.document_transactions.user_id, userId),
      ),
    );

  return row?.value ?? 0;
}
