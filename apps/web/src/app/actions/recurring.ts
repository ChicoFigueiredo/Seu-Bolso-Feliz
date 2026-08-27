"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq, gte, inArray, lte } from "drizzle-orm";

type RecurringTemplate = typeof schema.recurring_templates.$inferSelect;
type RecurringTemplateInsert = Omit<
  typeof schema.recurring_templates.$inferInsert,
  "user_id" | "id"
>;
type RecurringTemplateUpdate = Partial<RecurringTemplateInsert>;
type RecurringInstance = typeof schema.recurring_instances.$inferSelect;
type RecurringInstanceUpdate = Partial<
  Omit<typeof schema.recurring_instances.$inferInsert, "user_id" | "id">
>;
type RecurringInstanceInsert = Omit<
  typeof schema.recurring_instances.$inferInsert,
  "id" | "created_at" | "updated_at"
>;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

// Fecha IDOR (achado da varredura retroativa, mesma classe do fix em
// document-transactions.ts, commit b5995eb): create/update aceitavam
// financial_product_id/category_id/supplier_id vindos do cliente e
// gravavam sem checar que pertencem ao usuário chamador. Um usuário A
// conseguia criar/atualizar um template apontando para um produto
// financeiro, categoria ou fornecedor de outro usuário B (e a partir daí
// referenciar/vazar dados de B via joins em outras telas). Valida posse de
// cada FK presente antes do insert/update — mesmo padrão SELECT id WHERE
// id = fk AND user_id = userId, lançando erro se vazio.
async function assertOwnsRecurringTemplateForeignKeys(
  db: ReturnType<typeof getDb>,
  userId: string,
  input: Pick<RecurringTemplateInsert, "financial_product_id" | "category_id" | "supplier_id">,
): Promise<void> {
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

  if (input.category_id) {
    const [category] = await db
      .select({ id: schema.categories.id })
      .from(schema.categories)
      .where(
        and(eq(schema.categories.id, input.category_id), eq(schema.categories.user_id, userId)),
      );
    if (!category) throw new Error("Categoria não encontrada");
  }

  if (input.supplier_id) {
    const [supplier] = await db
      .select({ id: schema.suppliers.id })
      .from(schema.suppliers)
      .where(and(eq(schema.suppliers.id, input.supplier_id), eq(schema.suppliers.user_id, userId)));
    if (!supplier) throw new Error("Fornecedor não encontrado");
  }
}

// Mesmo anti-padrão, achado secundário da mesma varredura: tag_id (dentro de
// tagIds) nunca era validado contra tags.user_id antes de gravar em
// recurring_template_tags. A ordem de escrita (template já
// validado/committado antes de tocar nas tags) já impede vazamento de leitura
// hoje, mas sem esta checagem um usuário A conseguiria associar ao próprio
// template uma tag_id de outro usuário B.
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

export async function getRecurringTemplates(activeOnly = true): Promise<RecurringTemplate[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [eq(schema.recurring_templates.user_id, userId)];
  if (activeOnly) conditions.push(eq(schema.recurring_templates.is_active, true));
  return db
    .select()
    .from(schema.recurring_templates)
    .where(and(...conditions))
    .orderBy(asc(schema.recurring_templates.name));
}

export async function getRecurringTemplate(id: string): Promise<RecurringTemplate | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.recurring_templates)
    .where(
      and(eq(schema.recurring_templates.id, id), eq(schema.recurring_templates.user_id, userId)),
    );
  return row ?? null;
}

export async function createRecurringTemplate(
  input: RecurringTemplateInsert,
  tagIds?: string[],
): Promise<RecurringTemplate> {
  const userId = await requireUserId();
  const db = getDb();
  await assertOwnsRecurringTemplateForeignKeys(db, userId, input);
  if (tagIds && tagIds.length > 0) await assertOwnsTags(db, userId, tagIds);

  const [row] = await db
    .insert(schema.recurring_templates)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar template recorrente");

  if (tagIds && tagIds.length > 0) {
    await db
      .insert(schema.recurring_template_tags)
      .values(tagIds.map((tagId) => ({ recurring_template_id: row.id, tag_id: tagId })));
  }
  return row;
}

export async function updateRecurringTemplate(
  id: string,
  input: RecurringTemplateUpdate,
  tagIds?: string[],
): Promise<RecurringTemplate> {
  const userId = await requireUserId();
  const db = getDb();
  await assertOwnsRecurringTemplateForeignKeys(db, userId, input);
  if (tagIds && tagIds.length > 0) await assertOwnsTags(db, userId, tagIds);

  const [row] = await db
    .update(schema.recurring_templates)
    .set(input)
    .where(
      and(eq(schema.recurring_templates.id, id), eq(schema.recurring_templates.user_id, userId)),
    )
    .returning();
  if (!row) throw new Error("Template recorrente não encontrado");

  if (tagIds !== undefined) {
    await db
      .delete(schema.recurring_template_tags)
      .where(eq(schema.recurring_template_tags.recurring_template_id, id));
    if (tagIds.length > 0) {
      await db
        .insert(schema.recurring_template_tags)
        .values(tagIds.map((tagId) => ({ recurring_template_id: id, tag_id: tagId })));
    }
  }
  return row;
}

export async function deleteRecurringTemplate(id: string): Promise<void> {
  // Nota: soft delete (is_active=false), sem checagem de linhas afetadas hoje
  // — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.recurring_templates)
    .set({ is_active: false })
    .where(
      and(eq(schema.recurring_templates.id, id), eq(schema.recurring_templates.user_id, userId)),
    );
}

export async function getRecurringInstances(
  templateId: string,
  filters?: { status?: string; startDate?: string; endDate?: string },
): Promise<RecurringInstance[]> {
  const userId = await requireUserId();
  const db = getDb();
  const conditions = [
    eq(schema.recurring_instances.recurring_template_id, templateId),
    eq(schema.recurring_instances.user_id, userId),
  ];
  if (filters?.status) conditions.push(eq(schema.recurring_instances.status, filters.status));
  if (filters?.startDate)
    conditions.push(gte(schema.recurring_instances.expected_date, filters.startDate));
  if (filters?.endDate)
    conditions.push(lte(schema.recurring_instances.expected_date, filters.endDate));
  return db
    .select()
    .from(schema.recurring_instances)
    .where(and(...conditions))
    .orderBy(asc(schema.recurring_instances.expected_date));
}

export async function updateRecurringInstance(
  id: string,
  input: RecurringInstanceUpdate,
): Promise<RecurringInstance> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .update(schema.recurring_instances)
    .set(input)
    .where(
      and(eq(schema.recurring_instances.id, id), eq(schema.recurring_instances.user_id, userId)),
    )
    .returning();
  if (!row) throw new Error("Instância recorrente não encontrada");
  return row;
}

/**
 * Gera instâncias de recorrência para um template até uma data limite.
 * Recorrências geram EXPECTATIVA de pagamento, não marcam pagamento automático.
 */
export async function generateRecurringInstances(
  templateId: string,
  untilDate: string,
): Promise<number> {
  const userId = await requireUserId();
  const db = getDb();

  const template = await getRecurringTemplate(templateId);
  if (!template) throw new Error("Template não encontrado");
  if (!template.is_active) throw new Error("Template inativo");

  const [lastInstance] = await db
    .select({ expected_date: schema.recurring_instances.expected_date })
    .from(schema.recurring_instances)
    .where(
      and(
        eq(schema.recurring_instances.recurring_template_id, templateId),
        eq(schema.recurring_instances.user_id, userId),
      ),
    )
    .orderBy(desc(schema.recurring_instances.expected_date))
    .limit(1);

  const startFrom = lastInstance
    ? new Date(lastInstance.expected_date)
    : template.starts_at
      ? new Date(template.starts_at)
      : new Date();

  const endDate = new Date(untilDate);
  const instances: RecurringInstanceInsert[] = [];
  const current = new Date(startFrom);
  if (lastInstance) {
    advanceDate(current, template.frequency, template.day_of_month, template.custom_interval_days);
  }

  while (current <= endDate) {
    if (template.ends_at && current > new Date(template.ends_at)) break;
    instances.push({
      user_id: userId,
      recurring_template_id: templateId,
      expected_date: current.toISOString().split("T")[0]!,
      expected_amount: template.amount,
      status: "pending",
    });
    advanceDate(current, template.frequency, template.day_of_month, template.custom_interval_days);
  }

  if (instances.length > 0) {
    await db.insert(schema.recurring_instances).values(instances);
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
