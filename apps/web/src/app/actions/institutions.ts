"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

type Institution = typeof schema.institutions.$inferSelect;
type InstitutionInsert = Omit<typeof schema.institutions.$inferInsert, "user_id" | "id">;
type InstitutionUpdate = Partial<InstitutionInsert>;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

export async function getInstitutions(): Promise<Institution[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.institutions)
    .where(eq(schema.institutions.user_id, userId))
    .orderBy(asc(schema.institutions.display_order), asc(schema.institutions.name));
}

export async function getInstitution(id: string): Promise<Institution | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.institutions)
    .where(and(eq(schema.institutions.id, id), eq(schema.institutions.user_id, userId)));
  return row ?? null;
}

export async function createInstitution(input: InstitutionInsert): Promise<Institution> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.institutions)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar instituição");
  return row;
}

export async function updateInstitution(
  id: string,
  input: InstitutionUpdate,
): Promise<Institution> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.institutions)
    .set(input)
    .where(and(eq(schema.institutions.id, id), eq(schema.institutions.user_id, userId)))
    .returning();
  if (!row) throw new Error("Instituição não encontrada");
  return row;
}

export async function deleteInstitution(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .delete(schema.institutions)
    .where(and(eq(schema.institutions.id, id), eq(schema.institutions.user_id, userId)))
    .returning({ id: schema.institutions.id });
  // Nota: por contrato herdado, não checávamos linhas afetadas (sem
  // .select().single()) — pela regra geral do plano isso seria "exceção
  // preservada" (não lançar). Decisão do controlador (ledger): lançar mesmo
  // assim, porque é mais seguro que apagar silenciosamente algo que não
  // pertence ao usuário, e não há call site hoje que dependa do silêncio
  // antigo. Divergência deliberada, não descuido.
  if (result.length === 0) throw new Error("Instituição não encontrada");
}
