"use server";

import { createClient } from "@/lib/supabase/server";

// ─── S3-010 — Atualização de metadados de source_document ─────────────────────

interface UpdateDocumentMetadataInput {
  supplier_name_raw?: string;
  metadata?: Record<string, unknown>;
  document_type?: string;
}

export async function updateDocumentMetadata(
  id: string,
  input: UpdateDocumentMetadataInput,
): Promise<void> {
  const supabase = await createClient();

  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  // Monta objeto de update apenas com campos fornecidos
  const updatePayload: Record<string, unknown> = {};
  if (input.supplier_name_raw !== undefined) {
    updatePayload.supplier_name_raw = input.supplier_name_raw;
  }
  if (input.metadata !== undefined) {
    updatePayload.metadata = input.metadata;
  }
  if (input.document_type !== undefined) {
    updatePayload.document_type = input.document_type;
  }

  if (Object.keys(updatePayload).length === 0) return;

  const { error } = await supabase
    .from("source_documents")
    .update(updatePayload)
    .eq("id", id)
    .eq("user_id", user.id); // RLS + double-check

  if (error) throw new Error(error.message);
}
