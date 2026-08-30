"use server";

import { requireUser } from "@/lib/auth";
import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq } from "drizzle-orm";

export type Supplier = typeof schema.suppliers.$inferSelect;
type SupplierInsert = Omit<typeof schema.suppliers.$inferInsert, "user_id" | "id">;
type SupplierUpdate = Partial<SupplierInsert>;

export type SupplierAlias = typeof schema.supplier_aliases.$inferSelect;
type AliasInsert = Omit<typeof schema.supplier_aliases.$inferInsert, "user_id" | "id">;
type AliasUpdate = Partial<AliasInsert>;

export type SupplierContract = typeof schema.supplier_contracts.$inferSelect;
type ContractInsert = Omit<typeof schema.supplier_contracts.$inferInsert, "user_id" | "id">;
type ContractUpdate = Partial<ContractInsert>;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

/**
 * Valida posse de `institution_id` antes de gravar (IDOR — achado da
 * revisão desta task, mesma classe de `document-transactions.ts`,
 * commit b5995eb). `suppliers.institution_id` é uma FK opcional para
 * `institutions`, tabela de outro usuário; sem essa checagem, o usuário A
 * poderia vincular o próprio fornecedor a uma instituição de B.
 * `null`/`undefined` (limpar o vínculo) não precisam de checagem.
 */
async function requireOwnedInstitution(
  db: ReturnType<typeof getDb>,
  institutionId: string,
  userId: string,
): Promise<void> {
  const [row] = await db
    .select({ id: schema.institutions.id })
    .from(schema.institutions)
    .where(and(eq(schema.institutions.id, institutionId), eq(schema.institutions.user_id, userId)));
  if (!row) throw new Error("Instituição não encontrada");
}

/**
 * Valida posse de `supplier_id` antes de gravar (IDOR — achado da revisão
 * desta task). `supplier_aliases.supplier_id`/`supplier_contracts.supplier_id`
 * são FKs obrigatórias para `suppliers`; sem essa checagem, o usuário A
 * poderia criar (ou reatribuir) um alias/contrato apontando para um
 * fornecedor de B.
 */
async function requireOwnedSupplier(
  db: ReturnType<typeof getDb>,
  supplierId: string,
  userId: string,
): Promise<void> {
  const [row] = await db
    .select({ id: schema.suppliers.id })
    .from(schema.suppliers)
    .where(and(eq(schema.suppliers.id, supplierId), eq(schema.suppliers.user_id, userId)));
  if (!row) throw new Error("Fornecedor não encontrado");
}

export async function getSuppliers(activeOnly = true): Promise<Supplier[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.suppliers.user_id, userId)];
  if (activeOnly) conditions.push(eq(schema.suppliers.is_active, true));
  return db
    .select()
    .from(schema.suppliers)
    .where(and(...conditions))
    .orderBy(asc(schema.suppliers.name));
}

export async function getSupplier(id: string): Promise<Supplier | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.suppliers)
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)));
  return row ?? null;
}

/**
 * BLOQUEADA — NÃO CONVERTIDA. `search_suppliers` foi descartada por inteiro
 * na migração. Ver Apêndice "RPCs bloqueadas". Permanece com o código
 * Supabase atual.
 */
export async function searchSuppliers(query: string, limit = 10) {
  const supabase = await createClient();
  const { data, error } = await supabase.rpc("search_suppliers", {
    p_query: query,
    p_limit: limit,
  });
  if (error) throw new Error(error.message);
  return data;
}

export async function createSupplier(input: SupplierInsert): Promise<Supplier> {
  const userId = await requireUserId();
  const db = getDb();
  if (input.institution_id) {
    await requireOwnedInstitution(db, input.institution_id, userId);
  }
  const [row] = await db
    .insert(schema.suppliers)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar fornecedor");
  return row;
}

export async function updateSupplier(id: string, input: SupplierUpdate): Promise<Supplier> {
  const userId = await requireUserId();
  const db = getDb();
  if (input.institution_id) {
    await requireOwnedInstitution(db, input.institution_id, userId);
  }
  const [row] = await db
    .update(schema.suppliers)
    .set(input)
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)))
    .returning();
  if (!row) throw new Error("Fornecedor não encontrado");
  return row;
}

export async function deleteSupplier(id: string): Promise<void> {
  // Soft delete. Nota: sem `.select().single()` hoje — comportamento
  // preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.suppliers)
    .set({ is_active: false })
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)));
}

/**
 * NOVA — migrada de `app/dashboard/suppliers/[id]/page.tsx` (handleDelete).
 * Sem contrato Supabase anterior a preservar — aplica a regra padrão da fase
 * (lança erro em 0 linhas) em vez do silêncio de `deleteSupplier`.
 */
export async function updateSupplierActive(id: string, isActive: boolean): Promise<Supplier> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.suppliers)
    .set({ is_active: isActive })
    .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId)))
    .returning();
  if (!row) throw new Error("Fornecedor não encontrado");
  return row;
}

export interface SourceDocumentRef {
  id: string;
  filename: string;
  origin_type: string;
  status: string;
  created_at: string | null;
  metadata: unknown;
}

export interface SupplierDetail {
  supplier: Supplier | null;
  aliases: SupplierAlias[];
  contracts: SupplierContract[];
  documents: SourceDocumentRef[];
}

/** NOVA — migrada de `suppliers/[id]/page.tsx` (4 selects em paralelo no `load()`). */
export async function getSupplierDetail(id: string): Promise<SupplierDetail> {
  const userId = await requireUserId();
  const db = getDb();

  const [supplierRows, aliases, contracts, documents] = await Promise.all([
    db
      .select()
      .from(schema.suppliers)
      .where(and(eq(schema.suppliers.id, id), eq(schema.suppliers.user_id, userId))),
    db
      .select()
      .from(schema.supplier_aliases)
      .where(
        and(
          eq(schema.supplier_aliases.supplier_id, id),
          eq(schema.supplier_aliases.user_id, userId),
        ),
      )
      .orderBy(desc(schema.supplier_aliases.created_at)),
    db
      .select()
      .from(schema.supplier_contracts)
      .where(
        and(
          eq(schema.supplier_contracts.supplier_id, id),
          eq(schema.supplier_contracts.user_id, userId),
        ),
      )
      .orderBy(desc(schema.supplier_contracts.start_date)),
    db
      .select({
        id: schema.source_documents.id,
        filename: schema.source_documents.filename,
        origin_type: schema.source_documents.origin_type,
        status: schema.source_documents.status,
        created_at: schema.source_documents.created_at,
        metadata: schema.source_documents.metadata,
      })
      .from(schema.source_documents)
      .where(
        and(
          eq(schema.source_documents.supplier_id, id),
          eq(schema.source_documents.user_id, userId),
        ),
      )
      .orderBy(desc(schema.source_documents.created_at))
      .limit(50),
  ]);

  return { supplier: supplierRows[0] ?? null, aliases, contracts, documents };
}

export async function getSupplierAliases(supplierId: string): Promise<SupplierAlias[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.supplier_aliases)
    .where(
      and(
        eq(schema.supplier_aliases.supplier_id, supplierId),
        eq(schema.supplier_aliases.user_id, userId),
      ),
    )
    .orderBy(desc(schema.supplier_aliases.created_at));
}

export async function createSupplierAlias(input: AliasInsert): Promise<SupplierAlias> {
  const userId = await requireUserId();
  const db = getDb();
  await requireOwnedSupplier(db, input.supplier_id, userId);
  const [row] = await db
    .insert(schema.supplier_aliases)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar alias");
  return row;
}

export async function updateSupplierAlias(id: string, input: AliasUpdate): Promise<SupplierAlias> {
  const userId = await requireUserId();
  const db = getDb();
  if (input.supplier_id) {
    await requireOwnedSupplier(db, input.supplier_id, userId);
  }
  const [row] = await db
    .update(schema.supplier_aliases)
    .set(input)
    .where(and(eq(schema.supplier_aliases.id, id), eq(schema.supplier_aliases.user_id, userId)))
    .returning();
  if (!row) throw new Error("Alias não encontrado");
  return row;
}

export async function deleteSupplierAlias(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.supplier_aliases)
    .where(and(eq(schema.supplier_aliases.id, id), eq(schema.supplier_aliases.user_id, userId)));
}

export async function getSupplierContracts(supplierId: string): Promise<SupplierContract[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.supplier_contracts)
    .where(
      and(
        eq(schema.supplier_contracts.supplier_id, supplierId),
        eq(schema.supplier_contracts.user_id, userId),
      ),
    )
    .orderBy(desc(schema.supplier_contracts.start_date));
}

export async function createSupplierContract(input: ContractInsert): Promise<SupplierContract> {
  const userId = await requireUserId();
  const db = getDb();
  await requireOwnedSupplier(db, input.supplier_id, userId);
  const [row] = await db
    .insert(schema.supplier_contracts)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar contrato");
  return row;
}

export async function updateSupplierContract(
  id: string,
  input: ContractUpdate,
): Promise<SupplierContract> {
  const userId = await requireUserId();
  const db = getDb();
  if (input.supplier_id) {
    await requireOwnedSupplier(db, input.supplier_id, userId);
  }
  const [row] = await db
    .update(schema.supplier_contracts)
    .set(input)
    .where(and(eq(schema.supplier_contracts.id, id), eq(schema.supplier_contracts.user_id, userId)))
    .returning();
  if (!row) throw new Error("Contrato não encontrado");
  return row;
}

export async function deleteSupplierContract(id: string): Promise<void> {
  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .delete(schema.supplier_contracts)
    .where(
      and(eq(schema.supplier_contracts.id, id), eq(schema.supplier_contracts.user_id, userId)),
    );
}
