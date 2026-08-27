"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq } from "drizzle-orm";

type Liability = typeof schema.liabilities.$inferSelect;
type LiabilityInsert = Omit<typeof schema.liabilities.$inferInsert, "user_id" | "id">;
type LiabilityUpdate = Partial<LiabilityInsert>;
type LiabilityInstallment = typeof schema.liability_installments.$inferSelect;
type InstallmentInsert = Omit<typeof schema.liability_installments.$inferInsert, "user_id" | "id">;
type InstallmentUpdate = Partial<InstallmentInsert>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

// ══════════════════════════════════════════════════════════════
// Liabilities (Dívidas / Passivos)
// ══════════════════════════════════════════════════════════════

export async function getLiabilities(filters?: {
  status?: string;
  financialProductId?: string;
}): Promise<Liability[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.liabilities.user_id, userId)];
  if (filters?.status) conditions.push(eq(schema.liabilities.status, filters.status));
  if (filters?.financialProductId)
    conditions.push(eq(schema.liabilities.financial_product_id, filters.financialProductId));

  return db
    .select()
    .from(schema.liabilities)
    .where(and(...conditions))
    .orderBy(asc(schema.liabilities.name));
}

export async function getLiability(id: string): Promise<Liability | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.liabilities)
    .where(and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, userId)));
  return row ?? null;
}

export async function createLiability(
  input: LiabilityInsert,
  tagIds?: string[],
): Promise<Liability> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .insert(schema.liabilities)
    .values({ ...input, user_id: userId })
    .returning();

  if (!row) throw new Error("Falha ao criar dívida");

  if (tagIds && tagIds.length > 0) {
    await db
      .insert(schema.liability_tags)
      .values(tagIds.map((tag_id) => ({ liability_id: row.id, tag_id })));
  }

  return row;
}

export async function updateLiability(
  id: string,
  input: LiabilityUpdate,
  tagIds?: string[],
): Promise<Liability> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .update(schema.liabilities)
    .set(input)
    .where(and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, userId)))
    .returning();

  if (!row) throw new Error("Dívida não encontrada");

  if (tagIds !== undefined) {
    await db.delete(schema.liability_tags).where(eq(schema.liability_tags.liability_id, id));
    if (tagIds.length > 0) {
      await db
        .insert(schema.liability_tags)
        .values(tagIds.map((tag_id) => ({ liability_id: id, tag_id })));
    }
  }

  return row;
}

export async function deleteLiability(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const result = await db
    .delete(schema.liabilities)
    .where(and(eq(schema.liabilities.id, id), eq(schema.liabilities.user_id, userId)))
    .returning({ id: schema.liabilities.id });

  if (result.length === 0) throw new Error("Dívida não encontrada");
}

// ══════════════════════════════════════════════════════════════
// Liability Installments (Parcelas de Dívida)
// ══════════════════════════════════════════════════════════════

export async function getLiabilityInstallments(
  liabilityId: string,
  filters?: { status?: string },
): Promise<LiabilityInstallment[]> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [
    eq(schema.liability_installments.liability_id, liabilityId),
    eq(schema.liability_installments.user_id, userId),
  ];
  if (filters?.status) conditions.push(eq(schema.liability_installments.status, filters.status));

  return db
    .select()
    .from(schema.liability_installments)
    .where(and(...conditions))
    .orderBy(asc(schema.liability_installments.installment_number));
}

export async function createLiabilityInstallment(
  input: InstallmentInsert,
): Promise<LiabilityInstallment> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.liability_installments)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar parcela");
  return row;
}

export async function updateLiabilityInstallment(
  id: string,
  input: InstallmentUpdate,
): Promise<LiabilityInstallment> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.liability_installments)
    .set(input)
    .where(
      and(
        eq(schema.liability_installments.id, id),
        eq(schema.liability_installments.user_id, userId),
      ),
    )
    .returning();
  if (!row) throw new Error("Parcela não encontrada");
  return row;
}

export async function deleteLiabilityInstallment(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  const result = await db
    .delete(schema.liability_installments)
    .where(
      and(
        eq(schema.liability_installments.id, id),
        eq(schema.liability_installments.user_id, userId),
      ),
    )
    .returning({ id: schema.liability_installments.id });
  if (result.length === 0) throw new Error("Parcela não encontrada");
}
