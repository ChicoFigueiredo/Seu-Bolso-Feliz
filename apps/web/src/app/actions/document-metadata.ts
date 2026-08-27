"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, eq } from "drizzle-orm";
import type { Json } from "@sbf/shared-types";

// ─── S3-010 — Atualização de metadados de source_document ─────────────────────

type SourceDocumentUpdate = Partial<
  Pick<
    typeof schema.source_documents.$inferInsert,
    "supplier_name_raw" | "metadata" | "document_type"
  >
>;

interface UpdateDocumentMetadataInput {
  supplier_name_raw?: string;
  /** A coluna é `jsonb` tipada como `Json` no schema Drizzle (`.$type<Json>()`). */
  metadata?: Json;
  document_type?: string;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function updateDocumentMetadata(
  id: string,
  input: UpdateDocumentMetadataInput,
): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  // Monta objeto de update apenas com campos fornecidos.
  const updatePayload: SourceDocumentUpdate = {};
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

  const result = await db
    .update(schema.source_documents)
    .set(updatePayload)
    .where(and(eq(schema.source_documents.id, id), eq(schema.source_documents.user_id, userId)))
    .returning({ id: schema.source_documents.id });

  if (result.length === 0) throw new Error("Documento não encontrado");
}
