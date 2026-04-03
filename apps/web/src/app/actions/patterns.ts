"use server";

import { createClient } from "@/lib/supabase/server";
import { revalidatePath } from "next/cache";

// ── Tipos locais (tabela ainda não está nos generated types) ──

export interface DocumentPattern {
  id: string;
  user_id: string;
  name: string;
  supplier_id: string | null;
  document_type: string;
  institution_id: string | null;
  extraction_rules: Record<string, unknown>;
  field_mappings: Record<string, unknown>;
  sample_fingerprints: string[];
  confidence_threshold: number;
  version: number;
  is_active: boolean;
  feedback_count: number;
  success_count: number;
  created_at: string;
  updated_at: string;
}

export interface PatternFeedback {
  id: string;
  user_id: string;
  pattern_id: string;
  source_document_id: string | null;
  feedback_type: "correct" | "incorrect" | "partial" | "improved";
  corrections: Record<string, unknown>;
  notes: string | null;
  created_at: string;
}

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

// ══════════════════════════════════════════════════════════════
// List
// ══════════════════════════════════════════════════════════════

export async function listPatterns(filters?: {
  document_type?: string;
  supplier_id?: string;
  institution_id?: string;
  is_active?: boolean;
  search?: string;
  limit?: number;
  offset?: number;
}): Promise<{ data: DocumentPattern[]; count: number }> {
  const supabase = await createClient();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  let query = (supabase as any)
    .from("document_patterns")
    .select("*", { count: "exact" })
    .order("updated_at", { ascending: false });

  if (filters?.document_type) {
    query = query.eq("document_type", filters.document_type);
  }
  if (filters?.supplier_id) {
    query = query.eq("supplier_id", filters.supplier_id);
  }
  if (filters?.institution_id) {
    query = query.eq("institution_id", filters.institution_id);
  }
  if (filters?.is_active !== undefined) {
    query = query.eq("is_active", filters.is_active);
  }
  if (filters?.search) {
    query = query.ilike("name", `%${filters.search}%`);
  }
  if (filters?.limit) {
    query = query.limit(filters.limit);
  }
  if (filters?.offset) {
    const limit = filters.limit ?? 20;
    query = query.range(filters.offset, filters.offset + limit - 1);
  }

  const { data, error, count } = await query;
  if (error) throw new Error(error.message);
  return { data: data ?? [], count: count ?? 0 };
}

// ══════════════════════════════════════════════════════════════
// Get (detalhe + histórico de feedback)
// ══════════════════════════════════════════════════════════════

export async function getPattern(id: string): Promise<{
  pattern: DocumentPattern | null;
  feedback: PatternFeedback[];
}> {
  const supabase = await createClient();

  const [patternResult, feedbackResult] = await Promise.all([
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (supabase as any).from("document_patterns").select("*").eq("id", id).single(),
    // eslint-disable-next-line @typescript-eslint/no-explicit-any
    (supabase as any)
      .from("pattern_feedback")
      .select("*")
      .eq("pattern_id", id)
      .order("created_at", { ascending: false })
      .limit(50),
  ]);

  if (patternResult.error) {
    if (patternResult.error.code === "PGRST116") return { pattern: null, feedback: [] };
    throw new Error(patternResult.error.message);
  }

  return {
    pattern: patternResult.data,
    feedback: feedbackResult.data ?? [],
  };
}

// ══════════════════════════════════════════════════════════════
// Create
// ══════════════════════════════════════════════════════════════

export async function createPattern(input: CreatePatternInput): Promise<DocumentPattern> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Unauthorized");

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const { data, error } = await (supabase as any)
    .from("document_patterns")
    .insert({
      user_id: user.id,
      name: input.name,
      document_type: input.document_type,
      supplier_id: input.supplier_id ?? null,
      institution_id: input.institution_id ?? null,
      extraction_rules: input.extraction_rules ?? {},
      field_mappings: input.field_mappings ?? {},
      sample_fingerprints: input.sample_fingerprints ?? [],
      confidence_threshold: input.confidence_threshold ?? 0.8,
    })
    .select()
    .single();

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/ingestion/patterns");
  return data;
}

// ══════════════════════════════════════════════════════════════
// Update
// Cria nova versão se extraction_rules ou field_mappings mudar significativamente
// ══════════════════════════════════════════════════════════════

export async function updatePattern(
  id: string,
  input: UpdatePatternInput,
): Promise<DocumentPattern> {
  const supabase = await createClient();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const { data, error } = await (supabase as any)
    .from("document_patterns")
    .update({
      ...input,
      updated_at: new Date().toISOString(),
    })
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/ingestion/patterns");
  revalidatePath(`/dashboard/ingestion/patterns/${id}`);
  return data;
}

// ══════════════════════════════════════════════════════════════
// Create new version (copia o padrão com version+1, desativa o anterior)
// ══════════════════════════════════════════════════════════════

export async function createPatternVersion(
  id: string,
  updates: Pick<UpdatePatternInput, "extraction_rules" | "field_mappings">,
): Promise<DocumentPattern> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Unauthorized");

  // Busca o padrão atual
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const { data: current, error: fetchError } = await (supabase as any)
    .from("document_patterns")
    .select("*")
    .eq("id", id)
    .single();

  if (fetchError) throw new Error(fetchError.message);
  if (current.user_id !== user.id) throw new Error("Access denied");

  // Desativa versão atual
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  await (supabase as any).from("document_patterns").update({ is_active: false }).eq("id", id);

  // Cria nova versão
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const { data: newVersion, error: insertError } = await (supabase as any)
    .from("document_patterns")
    .insert({
      user_id: user.id,
      name: current.name,
      supplier_id: current.supplier_id,
      document_type: current.document_type,
      institution_id: current.institution_id,
      extraction_rules: updates.extraction_rules ?? current.extraction_rules,
      field_mappings: updates.field_mappings ?? current.field_mappings,
      sample_fingerprints: current.sample_fingerprints,
      confidence_threshold: current.confidence_threshold,
      version: current.version + 1,
      is_active: true,
    })
    .select()
    .single();

  if (insertError) throw new Error(insertError.message);

  revalidatePath("/dashboard/ingestion/patterns");
  revalidatePath(`/dashboard/ingestion/patterns/${id}`);
  return newVersion;
}

// ══════════════════════════════════════════════════════════════
// Deactivate
// ══════════════════════════════════════════════════════════════

export async function deactivatePattern(id: string): Promise<void> {
  const supabase = await createClient();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const { error } = await (supabase as any)
    .from("document_patterns")
    .update({ is_active: false, updated_at: new Date().toISOString() })
    .eq("id", id);

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/ingestion/patterns");
}

// ══════════════════════════════════════════════════════════════
// Reactivate
// ══════════════════════════════════════════════════════════════

export async function reactivatePattern(id: string): Promise<void> {
  const supabase = await createClient();

  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  const { error } = await (supabase as any)
    .from("document_patterns")
    .update({ is_active: true, updated_at: new Date().toISOString() })
    .eq("id", id);

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/ingestion/patterns");
}

// ══════════════════════════════════════════════════════════════
// Register feedback (via RPC para atomicidade)
// ══════════════════════════════════════════════════════════════

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
