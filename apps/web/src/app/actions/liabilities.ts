"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, eq, inArray } from "drizzle-orm";

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

// Mesmo padrão de recurring.ts::assertOwnsTags / transactions.ts::requireTagsOwned
// / document-splits.ts::requireOwnedTags — sem esta checagem, o usuário A
// conseguiria associar à própria dívida uma tag_id que pertence a outro
// usuário B (o RLS antigo cobria isso de graça em `liability_tags`).
async function assertOwnsTags(
  db: ReturnType<typeof getDb>,
  userId: string,
  tagIds: string[],
): Promise<void> {
  const uniqueIds = [...new Set(tagIds)];
  const owned = await db
    .select({ id: schema.tags.id })
    .from(schema.tags)
    .where(and(inArray(schema.tags.id, uniqueIds), eq(schema.tags.user_id, userId)));
  if (owned.length !== uniqueIds.length) {
    throw new Error("Uma ou mais tags não pertencem ao usuário");
  }
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

  // Valida posse das FKs antes de gravar — sem isso, o usuário A poderia
  // apontar a própria dívida para um financial_product ou supplier que
  // pertence a outro usuário. O RLS antigo cobria esse gap de graça; sem
  // RLS, a checagem precisa ser explícita aqui.
  const [product] = await db
    .select({ id: schema.financial_products.id })
    .from(schema.financial_products)
    .where(
      and(
        eq(schema.financial_products.id, input.financial_product_id),
        eq(schema.financial_products.user_id, userId),
      ),
    );
  if (!product) throw new Error("Produto financeiro não encontrado");

  if (input.supplier_id) {
    const [supplier] = await db
      .select({ id: schema.suppliers.id })
      .from(schema.suppliers)
      .where(and(eq(schema.suppliers.id, input.supplier_id), eq(schema.suppliers.user_id, userId)));
    if (!supplier) throw new Error("Fornecedor não encontrado");
  }

  if (tagIds && tagIds.length > 0) await assertOwnsTags(db, userId, tagIds);

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

  // Mesma validação de posse de createLiability, só que aqui as FKs são
  // opcionais (Partial) — só checa quando o caller está de fato trocando o
  // valor.
  if (input.financial_product_id) {
    const [product] = await db
      .select({ id: schema.financial_products.id })
      .from(schema.financial_products)
      .where(
        and(
          eq(schema.financial_products.id, input.financial_product_id),
          eq(schema.financial_products.user_id, userId),
        ),
      );
    if (!product) throw new Error("Produto financeiro não encontrado");
  }

  if (input.supplier_id) {
    const [supplier] = await db
      .select({ id: schema.suppliers.id })
      .from(schema.suppliers)
      .where(and(eq(schema.suppliers.id, input.supplier_id), eq(schema.suppliers.user_id, userId)));
    if (!supplier) throw new Error("Fornecedor não encontrado");
  }

  if (tagIds && tagIds.length > 0) await assertOwnsTags(db, userId, tagIds);

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

  // Valida posse de liability_id antes de gravar — sem isso, o usuário B
  // conseguia criar uma parcela apontando para a dívida de outro usuário
  // (achado CRITICAL confirmado empiricamente contra Postgres real pela
  // varredura de segurança). O RLS antigo cobria esse gap de graça; sem
  // RLS, a checagem precisa ser explícita aqui.
  const [liability] = await db
    .select({ id: schema.liabilities.id })
    .from(schema.liabilities)
    .where(
      and(eq(schema.liabilities.id, input.liability_id), eq(schema.liabilities.user_id, userId)),
    );
  if (!liability) throw new Error("Dívida não encontrada");

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

  // Achado CRITICAL da revisão final de branch: createLiabilityInstallment já
  // validava liability_id (fix da varredura retroativa), mas este update não
  // validava o mesmo campo — um usuário A conseguia repontar a própria
  // parcela para a dívida de outro usuário B via
  // updateLiabilityInstallment({ liability_id: <de B> }). Mesma checagem,
  // só que aqui é opcional (Partial) — só valida quando o caller está de
  // fato trocando o valor.
  if (input.liability_id) {
    const [liability] = await db
      .select({ id: schema.liabilities.id })
      .from(schema.liabilities)
      .where(
        and(eq(schema.liabilities.id, input.liability_id), eq(schema.liabilities.user_id, userId)),
      );
    if (!liability) throw new Error("Dívida não encontrada");
  }

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
