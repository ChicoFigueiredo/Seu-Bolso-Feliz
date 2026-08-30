"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

type FinancialProduct = typeof schema.financial_products.$inferSelect;
type FinancialProductInsert = Omit<typeof schema.financial_products.$inferInsert, "user_id" | "id">;
type FinancialProductUpdate = Partial<FinancialProductInsert>;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

/**
 * Valida que `institution_id` pertence ao usuário antes de gravá-lo em
 * `financial_products` — sem isso, o usuário A poderia criar (ou atualizar)
 * um produto financeiro em seu próprio `user_id` apontando `institution_id`
 * para uma instituição de outro usuário. O RLS antigo em `institutions`
 * cobria esse gap de graça; sem RLS, a checagem precisa ser explícita aqui.
 * Mesmo padrão do fix de `document-transactions.ts` (commit b5995eb) e de
 * `requireOwnedFinancialProduct` em `cards.ts`.
 */
async function requireOwnedInstitution(institutionId: string, userId: string): Promise<void> {
  const db = getDb();
  const [institution] = await db
    .select({ id: schema.institutions.id })
    .from(schema.institutions)
    .where(and(eq(schema.institutions.id, institutionId), eq(schema.institutions.user_id, userId)));
  if (!institution) throw new Error("Instituição não encontrada");
}

export async function getFinancialProducts(institutionId?: string): Promise<FinancialProduct[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.financial_products.user_id, userId)];
  if (institutionId) conditions.push(eq(schema.financial_products.institution_id, institutionId));
  return db
    .select()
    .from(schema.financial_products)
    .where(and(...conditions))
    .orderBy(asc(schema.financial_products.display_order), asc(schema.financial_products.name));
}

export async function getFinancialProduct(id: string): Promise<FinancialProduct | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.financial_products)
    .where(
      and(eq(schema.financial_products.id, id), eq(schema.financial_products.user_id, userId)),
    );
  return row ?? null;
}

export async function createFinancialProduct(
  input: FinancialProductInsert,
): Promise<FinancialProduct> {
  const userId = await requireUserId();
  const db = getDb();

  await requireOwnedInstitution(input.institution_id, userId);

  const [row] = await db
    .insert(schema.financial_products)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar produto financeiro");
  return row;
}

export async function updateFinancialProduct(
  id: string,
  input: FinancialProductUpdate,
): Promise<FinancialProduct> {
  const userId = await requireUserId();
  const db = getDb();

  if (input.institution_id) {
    await requireOwnedInstitution(input.institution_id, userId);
  }

  const [row] = await db
    .update(schema.financial_products)
    .set(input)
    .where(and(eq(schema.financial_products.id, id), eq(schema.financial_products.user_id, userId)))
    .returning();
  if (!row) throw new Error("Produto financeiro não encontrado");
  return row;
}

export async function deleteFinancialProduct(id: string): Promise<void> {
  // Nota: sem checagem de "0 linhas afetadas" — comportamento preservado
  // (o código anterior via Supabase também não lançava erro ao tentar
  // excluir um produto de outro usuário; o RLS simplesmente não encontrava
  // a linha e o delete não afetava nada. O filtro explícito por user_id
  // abaixo reproduz o mesmo efeito sem depender de RLS).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.financial_products)
    .where(
      and(eq(schema.financial_products.id, id), eq(schema.financial_products.user_id, userId)),
    );
}
