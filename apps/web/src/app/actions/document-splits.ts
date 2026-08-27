"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

export type DocumentSplit = typeof schema.document_splits.$inferSelect;

export interface DocumentSplitInsert {
  source_document_id: string;
  category_id?: string | null;
  tags?: string[];
  amount: number;
  description?: string | null;
}

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function listDocumentSplits(sourceDocumentId: string): Promise<DocumentSplit[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.document_splits)
    .where(
      and(
        eq(schema.document_splits.source_document_id, sourceDocumentId),
        eq(schema.document_splits.user_id, userId),
      ),
    )
    .orderBy(asc(schema.document_splits.created_at));
}

export async function createDocumentSplit(input: DocumentSplitInsert): Promise<DocumentSplit> {
  const userId = await requireUserId();
  const db = getDb();

  // Validação de soma server-side (além do trigger DB fn_validate_splits_sum).
  const existing = await db
    .select({ amount: schema.document_splits.amount })
    .from(schema.document_splits)
    .where(
      and(
        eq(schema.document_splits.source_document_id, input.source_document_id),
        eq(schema.document_splits.user_id, userId),
      ),
    );
  const currentSum = existing.reduce((acc, s) => acc + s.amount, 0);

  // Valida posse de source_document_id antes de gravar — sem isso, o
  // usuário A poderia criar um rateio (com o próprio user_id na linha de
  // document_splits) apontando para um documento que pertence a outro
  // usuário. O RLS antigo cobria esse gap de graça; sem RLS, a checagem
  // precisa ser explícita aqui (mesmo padrão do fix em
  // document-transactions.ts, commit b5995eb). Desvio deliberado do
  // comportamento original em Supabase, que pulava a validação de soma em
  // vez de lançar erro quando o documento não existia/não pertencia ao
  // usuário — aqui isso é tratado como IDOR e bloqueado.
  const [doc] = await db
    .select({ metadata: schema.source_documents.metadata })
    .from(schema.source_documents)
    .where(
      and(
        eq(schema.source_documents.id, input.source_document_id),
        eq(schema.source_documents.user_id, userId),
      ),
    );
  if (!doc) throw new Error("Documento não encontrado");

  const docAmount =
    doc.metadata && typeof doc.metadata === "object" && "amount" in doc.metadata
      ? Number((doc.metadata as Record<string, unknown>).amount)
      : null;

  if (docAmount !== null && currentSum + input.amount > docAmount) {
    throw new Error(
      `Soma dos rateios (${(currentSum + input.amount).toFixed(2)}) excede o valor do documento (${docAmount.toFixed(2)})`,
    );
  }

  const [row] = await db
    .insert(schema.document_splits)
    .values({ ...input, user_id: userId, tags: input.tags ?? [] })
    .returning();

  if (!row) throw new Error("Falha ao criar rateio de documento");
  return row;
}

export async function deleteDocumentSplit(splitId: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.document_splits)
    .where(and(eq(schema.document_splits.id, splitId), eq(schema.document_splits.user_id, userId)));
}

export async function getDocumentSplitsTotal(sourceDocumentId: string): Promise<number> {
  const userId = await requireUserId();
  const db = getDb();
  const rows = await db
    .select({ amount: schema.document_splits.amount })
    .from(schema.document_splits)
    .where(
      and(
        eq(schema.document_splits.source_document_id, sourceDocumentId),
        eq(schema.document_splits.user_id, userId),
      ),
    );
  return rows.reduce((acc, s) => acc + s.amount, 0);
}
