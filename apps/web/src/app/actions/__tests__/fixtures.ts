import { eq } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";

// NOTA (Task 1, Step 3 do brief): a introspecção real com `introspect.casing:
// "preserve"` gerou identificadores de tabela em snake_case (ex.:
// `schema.financial_products`, não `schema.financialProducts`) — não só as
// colunas. Este arquivo usa os nomes reais de packages/db/src/schema.ts.

export async function seedInstitution(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.institutions)
    .values({ user_id: userId, name: `Instituição de teste ${crypto.randomUUID()}`, type: "bank" })
    .returning();
  return row;
}

export async function seedFinancialProduct(
  userId: string,
  institutionId: string,
  type: string = "checking_account",
) {
  const db = getDb();
  const [row] = await db
    .insert(schema.financial_products)
    .values({ user_id: userId, institution_id: institutionId, name: "Produto de teste", type })
    .returning();
  return row;
}

export async function seedCard(userId: string, financialProductId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.cards)
    .values({ user_id: userId, financial_product_id: financialProductId })
    .returning();
  return row;
}

/**
 * Deleta a instituição de teste — o `ON DELETE CASCADE` em toda a cadeia
 * (financial_products → cards/transactions/liabilities/recurring_templates
 * → statement_cycles/statement_items/recurring_instances/
 * document_transactions/liability_installments) remove tudo que dependia
 * dela. Usar isto no `afterEach` em vez de deletar tabela por tabela.
 */
export async function cleanupInstitution(institutionId: string) {
  const db = getDb();
  await db.delete(schema.institutions).where(eq(schema.institutions.id, institutionId));
}
