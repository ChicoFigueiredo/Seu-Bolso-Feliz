"use server";

import { createClient } from "@/lib/supabase/server";

// ── Tipos locais (aguardando regeneração de database.types após migration) ────

export interface DocumentSplit {
  id: string;
  source_document_id: string;
  user_id: string;
  category_id: string | null;
  tags: string[];
  amount: number;
  description: string | null;
  created_at: string;
  updated_at: string;
}

export interface DocumentSplitInsert {
  source_document_id: string;
  category_id?: string | null;
  tags?: string[];
  amount: number;
  description?: string | null;
}

// ══════════════════════════════════════════════════════════════
// S3-003 — Server actions para document_splits
// ══════════════════════════════════════════════════════════════

export async function listDocumentSplits(sourceDocumentId: string): Promise<DocumentSplit[]> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("document_splits")
    .select("*")
    .eq("source_document_id", sourceDocumentId)
    .order("created_at", { ascending: true });

  if (error) throw new Error(error.message);
  return (data ?? []) as DocumentSplit[];
}

export async function createDocumentSplit(input: DocumentSplitInsert): Promise<DocumentSplit> {
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  // Validação de soma server-side (além do trigger DB)
  const { data: existing } = await supabase
    .from("document_splits")
    .select("amount")
    .eq("source_document_id", input.source_document_id);

  const currentSum = (existing ?? []).reduce(
    (acc: number, s: { amount: number }) => acc + s.amount,
    0,
  );

  // Busca amount do documento via metadata
  const { data: doc } = await supabase
    .from("source_documents")
    .select("metadata")
    .eq("id", input.source_document_id)
    .single();

  const docAmount =
    doc?.metadata && typeof doc.metadata === "object" && "amount" in doc.metadata
      ? Number((doc.metadata as Record<string, unknown>).amount)
      : null;

  if (docAmount !== null && currentSum + input.amount > docAmount) {
    throw new Error(
      `Soma dos rateios (${(currentSum + input.amount).toFixed(2)}) excede o valor do documento (${docAmount.toFixed(2)})`,
    );
  }

  const { data, error } = await supabase
    .from("document_splits")
    .insert({
      ...input,
      user_id: user.id,
      tags: input.tags ?? [],
    })
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data as DocumentSplit;
}

export async function deleteDocumentSplit(splitId: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase.from("document_splits").delete().eq("id", splitId);

  if (error) throw new Error(error.message);
}

export async function getDocumentSplitsTotal(sourceDocumentId: string): Promise<number> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("document_splits")
    .select("amount")
    .eq("source_document_id", sourceDocumentId);

  if (error) throw new Error(error.message);
  return (data ?? []).reduce((acc: number, s: { amount: number }) => acc + s.amount, 0);
}
