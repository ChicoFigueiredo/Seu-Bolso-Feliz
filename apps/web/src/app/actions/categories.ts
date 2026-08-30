"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

export type Category = typeof schema.categories.$inferSelect;
type CategoryInsert = Omit<typeof schema.categories.$inferInsert, "user_id" | "id">;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

export async function getCategories(): Promise<Category[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.categories)
    .where(eq(schema.categories.user_id, userId))
    .orderBy(asc(schema.categories.name));
}

export async function createCategory(input: CategoryInsert): Promise<Category> {
  const userId = await requireUserId();
  const db = getDb();

  // Valida posse de parent_id antes de gravar — mesmo padrão de
  // requireOwnedInstitution em financial-products.ts: sem isso, o usuário A
  // poderia pendurar a própria categoria embaixo de uma categoria de outro
  // usuário B.
  if (input.parent_id) {
    const [parent] = await db
      .select({ id: schema.categories.id })
      .from(schema.categories)
      .where(and(eq(schema.categories.id, input.parent_id), eq(schema.categories.user_id, userId)));
    if (!parent) throw new Error("Categoria pai não encontrada");
  }

  const [row] = await db
    .insert(schema.categories)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar categoria");
  return row;
}

export async function deleteCategory(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .delete(schema.categories)
    .where(and(eq(schema.categories.id, id), eq(schema.categories.user_id, userId)))
    .returning({ id: schema.categories.id });
  if (result.length === 0) throw new Error("Categoria não encontrada");
}
