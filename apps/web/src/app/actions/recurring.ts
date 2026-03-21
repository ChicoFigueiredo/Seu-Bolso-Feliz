"use server";

import { createClient } from "@/lib/supabase/server";
import type { RecurringTemplate, RecurringInstance, Insert, Update } from "@sbf/shared-types";

type TemplateInsert = Omit<Insert<"recurring_templates">, "user_id">;
type TemplateUpdate = Update<"recurring_templates">;

// ══════════════════════════════════════════════════════════════
// Recurring Templates
// ══════════════════════════════════════════════════════════════

export async function getRecurringTemplates(activeOnly = true): Promise<RecurringTemplate[]> {
  const supabase = await createClient();
  let query = supabase.from("recurring_templates").select("*").order("name");

  if (activeOnly) {
    query = query.eq("is_active", true);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function getRecurringTemplate(id: string): Promise<RecurringTemplate | null> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("recurring_templates")
    .select("*")
    .eq("id", id)
    .single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data;
}

export async function createRecurringTemplate(
  input: TemplateInsert,
  tagIds?: string[],
): Promise<RecurringTemplate> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  const { data, error } = await supabase
    .from("recurring_templates")
    .insert({ ...input, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);

  if (tagIds && tagIds.length > 0) {
    const tagInserts = tagIds.map((tag_id) => ({
      recurring_template_id: data.id,
      tag_id,
    }));
    const { error: tagError } = await supabase.from("recurring_template_tags").insert(tagInserts);
    if (tagError) throw new Error(tagError.message);
  }

  return data;
}

export async function updateRecurringTemplate(
  id: string,
  input: TemplateUpdate,
  tagIds?: string[],
): Promise<RecurringTemplate> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("recurring_templates")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);

  if (tagIds !== undefined) {
    await supabase.from("recurring_template_tags").delete().eq("recurring_template_id", id);
    if (tagIds.length > 0) {
      const tagInserts = tagIds.map((tag_id) => ({
        recurring_template_id: id,
        tag_id,
      }));
      const { error: tagError } = await supabase.from("recurring_template_tags").insert(tagInserts);
      if (tagError) throw new Error(tagError.message);
    }
  }

  return data;
}

export async function deleteRecurringTemplate(id: string): Promise<void> {
  const supabase = await createClient();
  const { error } = await supabase
    .from("recurring_templates")
    .update({ is_active: false })
    .eq("id", id);

  if (error) throw new Error(error.message);
}

// ══════════════════════════════════════════════════════════════
// Recurring Instances
// ══════════════════════════════════════════════════════════════

export async function getRecurringInstances(
  templateId: string,
  filters?: { status?: string; startDate?: string; endDate?: string },
): Promise<RecurringInstance[]> {
  const supabase = await createClient();
  let query = supabase
    .from("recurring_instances")
    .select("*")
    .eq("recurring_template_id", templateId)
    .order("expected_date", { ascending: true });

  if (filters?.status) {
    query = query.eq("status", filters.status);
  }
  if (filters?.startDate) {
    query = query.gte("expected_date", filters.startDate);
  }
  if (filters?.endDate) {
    query = query.lte("expected_date", filters.endDate);
  }

  const { data, error } = await query;
  if (error) throw new Error(error.message);
  return data;
}

export async function updateRecurringInstance(
  id: string,
  input: Update<"recurring_instances">,
): Promise<RecurringInstance> {
  const supabase = await createClient();
  const { data, error } = await supabase
    .from("recurring_instances")
    .update(input)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);
  return data;
}

/**
 * Gera instâncias de recorrência para um template até uma data limite.
 * Recorrências geram EXPECTATIVA de pagamento, não marcam pagamento automático.
 */
export async function generateRecurringInstances(
  templateId: string,
  untilDate: string,
): Promise<number> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  // Buscar template
  const template = await getRecurringTemplate(templateId);
  if (!template) throw new Error("Template não encontrado");
  if (!template.is_active) throw new Error("Template inativo");

  // Buscar última instância existente
  const { data: lastInstance } = await supabase
    .from("recurring_instances")
    .select("expected_date")
    .eq("recurring_template_id", templateId)
    .order("expected_date", { ascending: false })
    .limit(1)
    .single();

  const startFrom = lastInstance
    ? new Date(lastInstance.expected_date)
    : template.starts_at
      ? new Date(template.starts_at)
      : new Date();

  const endDate = new Date(untilDate);
  const instances: Array<{
    user_id: string;
    recurring_template_id: string;
    expected_date: string;
    expected_amount: number | null;
    status: string;
  }> = [];

  const current = new Date(startFrom);
  if (lastInstance) {
    // Avançar para a próxima data após a última instância
    advanceDate(current, template.frequency, template.day_of_month, template.custom_interval_days);
  }

  while (current <= endDate) {
    if (template.ends_at && current > new Date(template.ends_at)) break;

    instances.push({
      user_id: user.id,
      recurring_template_id: templateId,
      expected_date: current.toISOString().split("T")[0]!,
      expected_amount: template.amount,
      status: "pending",
    });

    advanceDate(current, template.frequency, template.day_of_month, template.custom_interval_days);
  }

  if (instances.length > 0) {
    const { error } = await supabase.from("recurring_instances").insert(instances);
    if (error) throw new Error(error.message);
  }

  return instances.length;
}

function advanceDate(
  date: Date,
  frequency: string,
  dayOfMonth: number | null,
  customDays: number | null,
) {
  switch (frequency) {
    case "monthly":
      date.setMonth(date.getMonth() + 1);
      if (dayOfMonth) {
        const lastDay = new Date(date.getFullYear(), date.getMonth() + 1, 0).getDate();
        date.setDate(Math.min(dayOfMonth, lastDay));
      }
      break;
    case "weekly":
      date.setDate(date.getDate() + 7);
      break;
    case "biweekly":
      date.setDate(date.getDate() + 14);
      break;
    case "quarterly":
      date.setMonth(date.getMonth() + 3);
      break;
    case "annual":
      date.setFullYear(date.getFullYear() + 1);
      break;
    case "custom":
      date.setDate(date.getDate() + (customDays ?? 30));
      break;
  }
}
