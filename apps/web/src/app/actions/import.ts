"use server";

import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { createStorageClientFromEnv } from "@sbf/storage";
import { and, eq } from "drizzle-orm";
import { requireTransactionForeignKeysOwned } from "./_shared/transaction-fk-ownership";
import { hojeIso } from "@/lib/datas";

type ImportJob = typeof schema.import_jobs.$inferSelect;
type ImportJobInsert = Omit<typeof schema.import_jobs.$inferInsert, "user_id" | "id">;
type ImportJobUpdate = Partial<Omit<typeof schema.import_jobs.$inferInsert, "user_id" | "id">>;
type Transaction = typeof schema.transactions.$inferSelect;
type TransactionInsert = Omit<typeof schema.transactions.$inferInsert, "user_id" | "id">;

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

/**
 * Guarda o CSV como ele chegou, antes de qualquer parsing.
 *
 * Nenhuma tela lê este arquivo de volta hoje — e foi por isso que ele quase
 * deixou de ser guardado na migração de backend. Mas o original é a única prova
 * do que foi importado quando um número não bate meses depois: sem ele, a
 * conferência vira palavra contra palavra. Guardar é barato; reconstituir é
 * impossível.
 */
export async function arquivarCsvImportado(file: File): Promise<string> {
  const userId = await requireUserId();
  const caminho = `${userId}/importacoes/${hojeIso()}/${crypto.randomUUID()}.csv`;
  await createStorageClientFromEnv()
    .from("user-files")
    .upload(caminho, file, { contentType: file.type || "text/csv", upsert: false });
  return caminho;
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
