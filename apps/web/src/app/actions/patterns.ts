"use server";

import { createClient } from "@/lib/supabase/server";
import { revalidatePath } from "next/cache";
import { getDb, schema } from "@sbf/db";
import { and, desc, eq, ilike, sql } from "drizzle-orm";
import type { Json } from "@sbf/shared-types";

type DocumentPatternRow = typeof schema.document_patterns.$inferSelect;
export type DocumentPattern = Omit<DocumentPatternRow, "extraction_rules" | "field_mappings"> & {
  extraction_rules: Record<string, unknown>;
  field_mappings: Record<string, unknown>;
};

type PatternFeedbackRow = typeof schema.pattern_feedback.$inferSelect;
export type PatternFeedback = Omit<PatternFeedbackRow, "corrections"> & {
  corrections: Record<string, unknown>;
};

export type CreatePatternInput = {
  name: string;
  document_type: string;
  supplier_id?: string | null;
  institution_id?: string | null;
  extraction_rules?: Record<string, unknown>;
  field_mappings?: Record<string, unknown>;
  sample_fingerprints?: string[];
  confidence_threshold?: number;
};

export type UpdatePatternInput = Partial<
  Pick<
    DocumentPattern,
    | "name"
    | "extraction_rules"
    | "field_mappings"
    | "sample_fingerprints"
    | "confidence_threshold"
    | "supplier_id"
    | "institution_id"
  >
>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Unauthorized");
  return user.id;
}

/**
 * Valida posse das FKs opcionais (supplier_id/institution_id) antes de
 * gravar — sem isso, o usuário A poderia apontar o próprio padrão para um
 * supplier ou institution que pertence a outro usuário. O RLS antigo cobria
 * esse gap de graça em `suppliers`/`institutions`; sem RLS, a checagem
 * precisa ser explícita aqui (mesmo padrão de document-transactions.ts,
 * liabilities.ts, etc.).
 */
async function assertOwnsOptionalFks(
  userId: string,
  fks: { supplier_id?: string | null; institution_id?: string | null },
): Promise<void> {
  const db = getDb();

  if (fks.supplier_id) {
    const [supplier] = await db
      .select({ id: schema.suppliers.id })
      .from(schema.suppliers)
      .where(and(eq(schema.suppliers.id, fks.supplier_id), eq(schema.suppliers.user_id, userId)));
    if (!supplier) throw new Error("Fornecedor não encontrado");
  }

  if (fks.institution_id) {
    const [institution] = await db
      .select({ id: schema.institutions.id })
      .from(schema.institutions)
      .where(
        and(
          eq(schema.institutions.id, fks.institution_id),
          eq(schema.institutions.user_id, userId),
        ),
      );
    if (!institution) throw new Error("Instituição não encontrada");
  }
}

export async function listPatterns(filters?: {
  document_type?: string;
  supplier_id?: string;
  institution_id?: string;
  is_active?: boolean;
  search?: string;
  limit?: number;
  offset?: number;
}): Promise<{ data: DocumentPattern[]; count: number }> {
  const userId = await requireUserId();
  const db = getDb();

  const conditions = [eq(schema.document_patterns.user_id, userId)];
  if (filters?.document_type)
    conditions.push(eq(schema.document_patterns.document_type, filters.document_type));
  if (filters?.supplier_id)
    conditions.push(eq(schema.document_patterns.supplier_id, filters.supplier_id));
  if (filters?.institution_id)
    conditions.push(eq(schema.document_patterns.institution_id, filters.institution_id));
  if (filters?.is_active !== undefined)
    conditions.push(eq(schema.document_patterns.is_active, filters.is_active));
  if (filters?.search) conditions.push(ilike(schema.document_patterns.name, `%${filters.search}%`));

  const [countRow] = await db
    .select({ count: sql<number>`count(*)::int` })
    .from(schema.document_patterns)
    .where(and(...conditions));

  const base = db
    .select()
    .from(schema.document_patterns)
    .where(and(...conditions))
    .orderBy(desc(schema.document_patterns.updated_at));

  const data =
    filters?.offset !== undefined
      ? await base.limit(filters.limit ?? 20).offset(filters.offset)
      : filters?.limit
        ? await base.limit(filters.limit)
        : await base;

  return { data: data as DocumentPattern[], count: countRow?.count ?? 0 };
}

export async function getPattern(id: string): Promise<{
  pattern: DocumentPattern | null;
  feedback: PatternFeedback[];
}> {
  const userId = await requireUserId();
  const db = getDb();

  const [pattern, feedback] = await Promise.all([
    db
      .select()
      .from(schema.document_patterns)
      .where(
        and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)),
      ),
    db
      .select()
      .from(schema.pattern_feedback)
      .where(
        and(
          eq(schema.pattern_feedback.pattern_id, id),
          eq(schema.pattern_feedback.user_id, userId),
        ),
      )
      .orderBy(desc(schema.pattern_feedback.created_at))
      .limit(50),
  ]);

  return {
    pattern: (pattern[0] as DocumentPattern) ?? null,
    feedback: feedback as PatternFeedback[],
  };
}

export async function createPattern(input: CreatePatternInput): Promise<DocumentPattern> {
  const userId = await requireUserId();
  const db = getDb();

  await assertOwnsOptionalFks(userId, {
    supplier_id: input.supplier_id,
    institution_id: input.institution_id,
  });

  const [row] = await db
    .insert(schema.document_patterns)
    .values({
      user_id: userId,
      name: input.name,
      document_type: input.document_type,
      supplier_id: input.supplier_id ?? null,
      institution_id: input.institution_id ?? null,
      extraction_rules: (input.extraction_rules ?? {}) as Json,
      field_mappings: (input.field_mappings ?? {}) as Json,
      sample_fingerprints: input.sample_fingerprints ?? [],
      confidence_threshold: input.confidence_threshold ?? 0.8,
    })
    .returning();

  if (!row) throw new Error("Falha ao criar padrão");

  revalidatePath("/dashboard/ingestion/patterns");
  return row as DocumentPattern;
}

export async function updatePattern(
  id: string,
  input: UpdatePatternInput,
): Promise<DocumentPattern> {
  const userId = await requireUserId();
  const db = getDb();

  await assertOwnsOptionalFks(userId, {
    supplier_id: input.supplier_id,
    institution_id: input.institution_id,
  });

  const { extraction_rules, field_mappings, ...rest } = input;
  const [row] = await db
    .update(schema.document_patterns)
    .set({
      ...rest,
      ...(extraction_rules !== undefined ? { extraction_rules: extraction_rules as Json } : {}),
      ...(field_mappings !== undefined ? { field_mappings: field_mappings as Json } : {}),
      updated_at: new Date().toISOString(),
    })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)))
    .returning();

  if (!row) throw new Error("Padrão não encontrado");

  revalidatePath("/dashboard/ingestion/patterns");
  revalidatePath(`/dashboard/ingestion/patterns/${id}`);
  return row as DocumentPattern;
}

/**
 * Candidato a Apêndice B (não decidido aqui): update seguido de insert sem
 * transação — comportamento pré-existente no Supabase original.
 */
export async function createPatternVersion(
  id: string,
  updates: Pick<UpdatePatternInput, "extraction_rules" | "field_mappings">,
): Promise<DocumentPattern> {
  const userId = await requireUserId();
  const db = getDb();

  // Nota: a busca original não filtra por user_id na query — a checagem de
  // posse é feita depois, em código de aplicação ("Access denied"). Mantido
  // assim pra não perder a distinção entre "não encontrado" e "de outro usuário".
  const [current] = await db
    .select()
    .from(schema.document_patterns)
    .where(eq(schema.document_patterns.id, id));
  if (!current) throw new Error("Padrão não encontrado");
  if (current.user_id !== userId) throw new Error("Access denied");

  // Nota: sem `.select().single()` hoje — comportamento preservado (exceção documentada).
  await db
    .update(schema.document_patterns)
    .set({ is_active: false })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)));

  const [newVersion] = await db
    .insert(schema.document_patterns)
    .values({
      user_id: userId,
      name: current.name,
      supplier_id: current.supplier_id,
      document_type: current.document_type,
      institution_id: current.institution_id,
      extraction_rules: (updates.extraction_rules ?? current.extraction_rules) as Json,
      field_mappings: (updates.field_mappings ?? current.field_mappings) as Json,
      sample_fingerprints: current.sample_fingerprints,
      confidence_threshold: current.confidence_threshold,
      version: current.version + 1,
      is_active: true,
    })
    .returning();

  if (!newVersion) throw new Error("Falha ao criar nova versão do padrão");

  revalidatePath("/dashboard/ingestion/patterns");
  revalidatePath(`/dashboard/ingestion/patterns/${id}`);
  return newVersion as DocumentPattern;
}

export async function deactivatePattern(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.document_patterns)
    .set({ is_active: false, updated_at: new Date().toISOString() })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)));
  revalidatePath("/dashboard/ingestion/patterns");
}

export async function reactivatePattern(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.document_patterns)
    .set({ is_active: true, updated_at: new Date().toISOString() })
    .where(and(eq(schema.document_patterns.id, id), eq(schema.document_patterns.user_id, userId)));
  revalidatePath("/dashboard/ingestion/patterns");
}

/**
 * BLOQUEADA — NÃO CONVERTIDA. `register_pattern_feedback` foi descartada por
 * inteiro na migração (packages/db/schema.sql, NOTA de gaps). Ver Apêndice
 * "RPCs bloqueadas". Permanece com o código Supabase atual.
 */
export async function registerPatternFeedback(input: {
  pattern_id: string;
  source_document_id?: string | null;
  feedback_type: "correct" | "incorrect" | "partial" | "improved";
  corrections?: Record<string, unknown>;
}): Promise<PatternFeedback> {
  const supabase = await createClient();
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const { data, error } = await (supabase as any).rpc("register_pattern_feedback", {
    p_pattern_id: input.pattern_id,
    p_source_document_id: input.source_document_id ?? null,
    p_feedback_type: input.feedback_type,
    p_corrections: input.corrections ?? {},
  });

  if (error) throw new Error(error.message);
  revalidatePath(`/dashboard/ingestion/patterns/${input.pattern_id}`);
  return data;
}
