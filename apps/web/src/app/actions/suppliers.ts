"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq, sql } from "drizzle-orm";

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
 * `type` e não `interface` de propósito: `db.execute<T>` exige
 * `T extends Record<string, unknown>`, e só um alias de tipo objeto ganha a
 * assinatura de índice implícita que satisfaz essa restrição.
 */
export type SupplierSearchResult = {
  id: string;
  name: string;
  type: string | null;
  document_number: string | null;
  is_active: boolean;
  /** Alias que casou com a busca; `null` quando o casamento foi pelo nome. */
  matched_alias: string | null;
  similarity_score: number;
};

/**
 * Busca fuzzy de fornecedores por nome ou alias ativo (era a RPC
 * `search_suppliers`).
 *
 * `search_suppliers` é uma das SEIS funções descartadas por inteiro na
 * migração Supabase → Neon (packages/db/schema.sql, NOTA item 4) — filtrava
 * por `auth.uid()`. Confirmado por consulta ao `pg_proc` do Neon: NÃO existe
 * lá. O corpo dela vem para cá como `sql` parametrizado, com o filtro por
 * dono explícito nas DUAS pernas da união (nome e alias): `similarity()` e o
 * operador `%` são do pg_trgm e não têm equivalente no query builder do
 * Drizzle, e os índices GIN `idx_suppliers_name_trgm`/
 * `idx_supplier_aliases_trgm` só servem essa forma.
 */
export async function searchSuppliers(query: string, limit = 10): Promise<SupplierSearchResult[]> {
  const userId = await requireUserId();
  const db = getDb();

  const resultado = await db.execute<SupplierSearchResult>(sql`
    WITH direct_matches AS (
      SELECT s.id, s.name, s.type, s.document_number, s.is_active,
             NULL::text AS matched_alias,
             similarity(s.name, ${query}::text) AS similarity_score
        FROM public.suppliers s
       WHERE s.user_id = ${userId}::uuid
         AND s.is_active = true
         AND (s.name % ${query}::text OR s.name ILIKE '%' || ${query}::text || '%')
    ),
    alias_matches AS (
      SELECT s.id, s.name, s.type, s.document_number, s.is_active,
             sa.alias_name AS matched_alias,
             similarity(sa.alias_name, ${query}::text) AS similarity_score
        FROM public.supplier_aliases sa
        JOIN public.suppliers s ON s.id = sa.supplier_id
       WHERE s.user_id = ${userId}::uuid
         -- Defesa em profundidade: sem RLS, um alias com user_id de outro
         -- dono pendurado num fornecedor deste não pode entrar no resultado.
         AND sa.user_id = ${userId}::uuid
         AND sa.is_active = true
         AND s.is_active = true
         AND (sa.alias_name % ${query}::text OR sa.alias_name ILIKE '%' || ${query}::text || '%')
         AND (sa.valid_from IS NULL OR sa.valid_from <= CURRENT_DATE)
         AND (sa.valid_until IS NULL OR sa.valid_until >= CURRENT_DATE)
    ),
    combined AS (
      SELECT * FROM direct_matches
      UNION ALL
      SELECT * FROM alias_matches
    )
    SELECT sub.id, sub.name, sub.type, sub.document_number, sub.is_active,
           sub.matched_alias, sub.similarity_score
      FROM (
        SELECT DISTINCT ON (combined.id)
               combined.id, combined.name, combined.type, combined.document_number,
               combined.is_active, combined.matched_alias, combined.similarity_score
          FROM combined
         ORDER BY combined.id, combined.similarity_score DESC
      ) sub
     ORDER BY sub.similarity_score DESC
     LIMIT ${limit}
  `);

  return resultado.rows;
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
