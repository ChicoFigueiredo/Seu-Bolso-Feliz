"use server";

import { createClient } from "@/lib/supabase/server";
import type { Transaction } from "@sbf/shared-types";

// ── Tipos locais (aguardando regeneração de database.types após migration) ────

export interface DocumentTransactionLink {
  id: string;
  source_document_id: string;
  transaction_id: string;
  user_id: string;
  link_type: "payment" | "refund" | "installment" | "support";
  confidence: number | null;
  created_by: "user" | "ai" | "pattern";
  notes: string | null;
  created_at: string;
}

export interface DocumentTransactionLinkWithTransaction extends DocumentTransactionLink {
  transaction: Transaction;
}

// ══════════════════════════════════════════════════════════════
// S3-004 — Server actions para document_transactions
// ══════════════════════════════════════════════════════════════

export async function listLinkedTransactions(
  sourceDocumentId: string,
): Promise<DocumentTransactionLinkWithTransaction[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("document_transactions")
    .select(
      `
      *,
      transaction:transactions (*)
    `,
    )
    .eq("source_document_id", sourceDocumentId)
    .order("created_at", { ascending: true });

  if (error) throw new Error(error.message);
  return (data ?? []) as DocumentTransactionLinkWithTransaction[];
}

export async function linkTransactionToDocument(
  sourceDocumentId: string,
  transactionId: string,
  linkType: DocumentTransactionLink["link_type"],
  options?: { confidence?: number; notes?: string },
): Promise<DocumentTransactionLink> {
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("document_transactions")
    .insert({
      source_document_id: sourceDocumentId,
      transaction_id: transactionId,
      user_id: user.id,
      link_type: linkType,
      confidence: options?.confidence ?? null,
      created_by: "user",
      notes: options?.notes ?? null,
    })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data as DocumentTransactionLink;
}

export async function unlinkTransactionFromDocument(linkId: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("document_transactions").delete().eq("id", linkId);

  if (error) throw new Error(error.message);
}

export async function getLinkedTransactionCount(sourceDocumentId: string): Promise<number> {
  const supabase = await createClient();
  const { count, error } = await supabase
    .from("document_transactions")
    .select("id", { count: "exact", head: true })
    .eq("source_document_id", sourceDocumentId);

  if (error) throw new Error(error.message);
  return count ?? 0;
}
