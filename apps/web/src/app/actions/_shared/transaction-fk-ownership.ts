// Módulo compartilhado (SEM "use server" — funções auxiliares internas, não
// Server Actions). `transactions.ts` e `import.ts` gravam na mesma tabela
// `transactions` com o mesmo `TransactionInsert`/`TransactionUpdate`; esta
// validação de posse de FK precisa ser idêntica nos dois lugares.
//
// Histórico: extraído de transactions.ts (commit 6622967, "fix(web): fecha
// IDOR em transactions.ts") depois que a revisão final de branch encontrou
// que `import.ts::createTransactionFromImport` (Task 19) só validava
// `financial_product_id`, deixando as outras 7 FKs — incluindo
// `source_document_id`, que contorna inteiramente a validação de posse de
// `linkTransactionToDocument` (document-transactions.ts) — sem checagem.
import { getDb, schema } from "@sbf/db";
import { and, eq } from "drizzle-orm";
import type { AnyPgColumn, PgTable } from "drizzle-orm/pg-core";

type TransactionInsert = Omit<typeof schema.transactions.$inferInsert, "user_id" | "id">;

async function requireOwned(
  table: PgTable,
  idColumn: AnyPgColumn,
  userIdColumn: AnyPgColumn,
  id: string,
  userId: string,
  label: string,
): Promise<void> {
  const db = getDb();
  const [row] = await db
    .select({ id: idColumn })
    .from(table)
    .where(and(eq(idColumn, id), eq(userIdColumn, userId)));
  if (!row) throw new Error(`${label} não encontrado(a)`);
}

// IDOR fechado (achado CRITICAL da varredura de segurança): `transactions`
// tem 8 colunas de FK que vêm direto do `input` do cliente
// (financial_product_id, financial_period_id, statement_cycle_id,
// category_id, supplier_id, liability_installment_id,
// recurring_instance_id, source_document_id). Sem RLS, gravar/atualizar sem
// checar posse permite que o usuário A aponte qualquer uma delas pra um
// registro de outro usuário — `source_document_id` é o caso mais grave
// porque contorna inteiramente a validação de posse já aplicada em
// `linkTransactionToDocument` (document-transactions.ts). Esta função faz o
// mesmo `SELECT id FROM <tabela> WHERE id = <fk> AND user_id = userId` pra
// cada FK presente, antes de qualquer insert/update.
export async function requireTransactionForeignKeysOwned(
  input: Partial<TransactionInsert>,
  userId: string,
): Promise<void> {
  if (input.financial_product_id) {
    await requireOwned(
      schema.financial_products,
      schema.financial_products.id,
      schema.financial_products.user_id,
      input.financial_product_id,
      userId,
      "Produto financeiro",
    );
  }
  if (input.financial_period_id) {
    await requireOwned(
      schema.financial_periods,
      schema.financial_periods.id,
      schema.financial_periods.user_id,
      input.financial_period_id,
      userId,
      "Período financeiro",
    );
  }
  if (input.statement_cycle_id) {
    await requireOwned(
      schema.statement_cycles,
      schema.statement_cycles.id,
      schema.statement_cycles.user_id,
      input.statement_cycle_id,
      userId,
      "Ciclo de fatura",
    );
  }
  if (input.category_id) {
    await requireOwned(
      schema.categories,
      schema.categories.id,
      schema.categories.user_id,
      input.category_id,
      userId,
      "Categoria",
    );
  }
  if (input.supplier_id) {
    await requireOwned(
      schema.suppliers,
      schema.suppliers.id,
      schema.suppliers.user_id,
      input.supplier_id,
      userId,
      "Fornecedor",
    );
  }
  if (input.liability_installment_id) {
    await requireOwned(
      schema.liability_installments,
      schema.liability_installments.id,
      schema.liability_installments.user_id,
      input.liability_installment_id,
      userId,
      "Parcela de dívida",
    );
  }
  if (input.recurring_instance_id) {
    await requireOwned(
      schema.recurring_instances,
      schema.recurring_instances.id,
      schema.recurring_instances.user_id,
      input.recurring_instance_id,
      userId,
      "Instância recorrente",
    );
  }
  // source_document_id é o achado mais grave: sem esta checagem, um usuário
  // contorna inteiramente a validação de posse de linkTransactionToDocument
  // (document-transactions.ts) só criando/editando a transação direto com o
  // documento de outro usuário.
  if (input.source_document_id) {
    await requireOwned(
      schema.source_documents,
      schema.source_documents.id,
      schema.source_documents.user_id,
      input.source_document_id,
      userId,
      "Documento de origem",
    );
  }
}
