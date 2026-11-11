"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

export type Tag = typeof schema.tags.$inferSelect;
type TagInsert = Omit<typeof schema.tags.$inferInsert, "user_id" | "id">;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

export async function getTags(): Promise<Tag[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.tags)
    .where(eq(schema.tags.user_id, userId))
    .orderBy(asc(schema.tags.name));
}

export async function createTag(input: TagInsert): Promise<Tag> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.tags)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar tag");
  return row;
}

export async function deleteTag(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .delete(schema.tags)
    .where(and(eq(schema.tags.id, id), eq(schema.tags.user_id, userId)))
    .returning({ id: schema.tags.id });
  if (result.length === 0) throw new Error("Tag não encontrada");
}
