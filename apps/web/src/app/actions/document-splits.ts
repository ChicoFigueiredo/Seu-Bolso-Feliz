"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq, inArray } from "drizzle-orm";

export type DocumentSplit = typeof schema.document_splits.$inferSelect;

export interface DocumentSplitInsert {
  source_document_id: string;
  category_id?: string | null;
  tags?: string[];
  amount: number;
  description?: string | null;
}

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

// IDOR fechado (achado da revisão da Task 16, mesma classe do fix de
// source_document_id abaixo e do padrão em transactions.ts/recurring.ts):
// category_id vem do cliente e era gravado sem checar posse contra
// categories.user_id — usuário A conseguia apontar o próprio rateio pra
// categoria de outro usuário B.
async function requireOwnedCategory(
  db: ReturnType<typeof getDb>,
  userId: string,
  categoryId: string,
): Promise<void> {
  const [category] = await db
    .select({ id: schema.categories.id })
    .from(schema.categories)
    .where(and(eq(schema.categories.id, categoryId), eq(schema.categories.user_id, userId)));
  if (!category) throw new Error("Categoria não encontrada");
}

// Mesma classe de IDOR: `tags` é um array de uuids vindo do cliente e era
// gravado sem checar posse contra tags.user_id (mesmo padrão de
// requireTagsOwned em transactions.ts / assertOwnsTags em recurring.ts).
async function requireOwnedTags(
  db: ReturnType<typeof getDb>,
  userId: string,
  tagIds: string[],
): Promise<void> {
  const uniqueIds = [...new Set(tagIds)];
  if (uniqueIds.length === 0) return;
  const owned = await db
    .select({ id: schema.tags.id })
    .from(schema.tags)
    .where(and(inArray(schema.tags.id, uniqueIds), eq(schema.tags.user_id, userId)));
  if (owned.length !== uniqueIds.length) {
    throw new Error("Uma ou mais tags não pertencem ao usuário");
  }
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

  if (input.category_id) await requireOwnedCategory(db, userId, input.category_id);
  if (input.tags && input.tags.length > 0) await requireOwnedTags(db, userId, input.tags);

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
  // comportamento original, que pulava a validação de soma em
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
