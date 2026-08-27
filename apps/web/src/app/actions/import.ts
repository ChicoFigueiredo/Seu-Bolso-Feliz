"use server";

import { createClient } from "@/lib/supabase/server";
import { getDb, schema } from "@sbf/db";
import { and, eq } from "drizzle-orm";
import { requireTransactionForeignKeysOwned } from "./_shared/transaction-fk-ownership";

type ImportJob = typeof schema.import_jobs.$inferSelect;
type ImportJobInsert = Omit<typeof schema.import_jobs.$inferInsert, "user_id" | "id">;
type ImportJobUpdate = Partial<Omit<typeof schema.import_jobs.$inferInsert, "user_id" | "id">>;
type Transaction = typeof schema.transactions.$inferSelect;
type TransactionInsert = Omit<typeof schema.transactions.$inferInsert, "user_id" | "id">;

async function requireUserId(): Promise<string> {
  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");
  return user.id;
}

export async function createImportJob(input: ImportJobInsert): Promise<ImportJob> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .insert(schema.import_jobs)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar job de importação");
  return row;
}

export async function getImportJobStatus(id: string): Promise<ImportJob | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select()
    .from(schema.import_jobs)
    .where(and(eq(schema.import_jobs.id, id), eq(schema.import_jobs.user_id, userId)));
  return row ?? null;
}

/**
 * Devolve o id do primeiro `financial_products` do usuário — produto
 * "padrão" para lançar as transações importadas. Sem ORDER BY (paridade com
 * o código atual — "qual produto é o primeiro" já era indefinido antes).
 */
export async function getDefaultFinancialProductId(): Promise<string | null> {
  const userId = await requireUserId();
  const db = getDb();
  const [row] = await db
    .select({ id: schema.financial_products.id })
    .from(schema.financial_products)
    .where(eq(schema.financial_products.user_id, userId))
    .limit(1);
  return row?.id ?? null;
}

export async function createTransactionFromImport(input: TransactionInsert): Promise<Transaction> {
  const userId = await requireUserId();
  const db = getDb();

  // Valida posse de TODAS as FKs recebidas do cliente antes de gravar — não
  // só financial_product_id. `input` é o mesmo TransactionInsert completo de
  // transactions.ts::createTransaction (mesma tabela), então está sujeito ao
  // mesmo achado CRITICAL da varredura de segurança: sem checar posse de
  // financial_period_id/statement_cycle_id/category_id/supplier_id/
  // liability_installment_id/recurring_instance_id/source_document_id, o
  // usuário A poderia gravar uma transação (com o próprio user_id) apontando
  // para qualquer uma dessas FKs pertencentes a outro usuário —
  // source_document_id é o caso mais grave, porque contorna inteiramente a
  // validação de posse de linkTransactionToDocument
  // (document-transactions.ts). Helper compartilhado com transactions.ts via
  // ./_shared/transaction-fk-ownership — mesma validação, uma só definição.
  await requireTransactionForeignKeysOwned(input, userId);

  const [row] = await db
    .insert(schema.transactions)
    .values({ ...input, user_id: userId })
    .returning();
  if (!row) throw new Error("Falha ao criar transação de importação");
  return row;
}

export async function updateImportJob(id: string, input: ImportJobUpdate): Promise<void> {
  // Nota: sem checagem de linhas afetadas hoje — comportamento preservado (exceção documentada).
  const userId = await requireUserId();
  const db = getDb();
  await db
    .update(schema.import_jobs)
    .set(input)
    .where(and(eq(schema.import_jobs.id, id), eq(schema.import_jobs.user_id, userId)));
}
