"use server";

/**
 * Cartões de crédito — CRUD (issue #4).
 *
 * Até aqui, `cards` era a única entidade central sem nenhuma porta de entrada:
 * não havia action, não havia tela, e o único caminho para cadastrar um cartão
 * era `INSERT` manual no SQL. Como `statement_cycles.card_id` é `NOT NULL`, sem
 * cartão nenhuma fatura podia existir — e a jornada inteira de cartão, que é o
 * Marco 2, ficava barrada na primeira porta.
 */

import { revalidatePath } from "next/cache";
import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, count, desc, eq, inArray, ne } from "drizzle-orm";
import { createCardSchema, updateCardSchema } from "@sbf/validation";

type Card = typeof schema.cards.$inferSelect;
type FinancialProduct = typeof schema.financial_products.$inferSelect;

export type CardInput = Omit<typeof schema.cards.$inferInsert, "user_id" | "id">;
export type CardUpdate = Partial<CardInput>;

/** Cartão com o produto financeiro e a instituição já resolvidos. */
export interface CardComProduto extends Card {
  financial_products:
    | (Pick<FinancialProduct, "id" | "name" | "type"> & {
        institutions: { name: string } | null;
      })
    | null;
}

async function requireUserId(): Promise<string> {
  // `ownerId`, não `id`: o filtro de dados é sempre pelo dono, nunca pelo ator.
  // Ver apps/web/src/lib/auth/index.ts.
  const { ownerId } = await requireUser();
  return ownerId;
}

/**
 * Valida que `financial_product_id` pertence ao usuário antes de gravá-lo em
 * `cards` — sem isso, o usuário A poderia criar (ou atualizar) um cartão em
 * seu próprio `user_id` apontando `financial_product_id` para um produto de
 * outro usuário. O RLS antigo em `financial_products` cobria esse gap de
 * graça; sem RLS, a checagem precisa ser explícita aqui. Mesmo padrão do fix
 * de `document-transactions.ts` (commit b5995eb).
 */
async function requireOwnedFinancialProduct(
  financialProductId: string,
  userId: string,
): Promise<void> {
  const db = getDb();
  const [product] = await db
    .select({ id: schema.financial_products.id })
    .from(schema.financial_products)
    .where(
      and(
        eq(schema.financial_products.id, financialProductId),
        eq(schema.financial_products.user_id, userId),
      ),
    );
  if (!product) throw new Error("Produto financeiro não encontrado");
}

function cardWithProductSelect(userId: string) {
  const db = getDb();
  return db
    .select({
      card: schema.cards,
      financialProduct: {
        id: schema.financial_products.id,
        name: schema.financial_products.name,
        type: schema.financial_products.type,
      },
      institution: { name: schema.institutions.name },
    })
    .from(schema.cards)
    .leftJoin(
      schema.financial_products,
      and(
        eq(schema.cards.financial_product_id, schema.financial_products.id),
        // Defesa em profundidade: mesmo que `financial_product_id` aponte
        // para um produto de outro usuário (dado legado ou bug de escrita
        // anterior a `requireOwnedFinancialProduct`), nunca devolver os dados
        // desse produto. O LEFT JOIN garante que o cartão continua aparecendo
        // — só a informação do produto de terceiro é suprimida.
        eq(schema.financial_products.user_id, userId),
      ),
    )
    .leftJoin(
      schema.institutions,
      and(
        eq(schema.financial_products.institution_id, schema.institutions.id),
        eq(schema.institutions.user_id, userId),
      ),
    );
}

function toCardComProduto(row: {
  card: Card;
  financialProduct: Pick<FinancialProduct, "id" | "name" | "type"> | null;
  institution: { name: string } | null;
}): CardComProduto {
  return {
    ...row.card,
    financial_products: row.financialProduct
      ? { ...row.financialProduct, institutions: row.institution }
      : null,
  };
}

export async function getCards(): Promise<CardComProduto[]> {
  const userId = await requireUserId();
  const rows = await cardWithProductSelect(userId)
    .where(eq(schema.cards.user_id, userId))
    .orderBy(desc(schema.cards.is_primary), asc(schema.cards.created_at));
  return rows.map(toCardComProduto);
}

export async function getCard(id: string): Promise<CardComProduto | null> {
  const userId = await requireUserId();
  const [row] = await cardWithProductSelect(userId).where(
    and(eq(schema.cards.id, id), eq(schema.cards.user_id, userId)),
  );
  return row ? toCardComProduto(row) : null;
}

/**
 * Produtos elegíveis a virar cartão.
 *
 * `credit_card` é o tipo esperado, mas cartões de débito também existem e
 * pendurar a lista num único tipo deixaria a tela vazia para quem cadastrou o
 * produto como conta corrente.
 */
export async function getProdutosParaCartao(): Promise<FinancialProduct[]> {
  const userId = await requireUserId();
  const db = getDb();
  return db
    .select()
    .from(schema.financial_products)
    .where(
      and(
        eq(schema.financial_products.user_id, userId),
        inArray(schema.financial_products.type, ["credit_card", "checking_account"]),
      ),
    )
    .orderBy(asc(schema.financial_products.name));
}

export async function createCard(input: CardInput): Promise<Card> {
  const parsed = createCardSchema.parse(input);
  const userId = await requireUserId();
  const db = getDb();

  await requireOwnedFinancialProduct(parsed.financial_product_id, userId);

  // Só um cartão principal por vez. Fire-and-forget, igual ao código atual
  // (o Antes também não desestrutura `error` desta chamada).
  if (parsed.is_primary) {
    await db
      .update(schema.cards)
      .set({ is_primary: false })
      .where(eq(schema.cards.user_id, userId));
  }

  const [row] = await db
    .insert(schema.cards)
    .values({ ...parsed, user_id: userId })
    .returning();

  if (!row) throw new Error("Falha ao criar cartão");

  revalidatePath("/dashboard/cards");
  return row;
}

export async function updateCard(id: string, input: CardUpdate): Promise<Card> {
  const parsed = updateCardSchema.parse(input);
  const userId = await requireUserId();
  const db = getDb();

  if (parsed.financial_product_id) {
    await requireOwnedFinancialProduct(parsed.financial_product_id, userId);
  }

  if (parsed.is_primary) {
    await db
      .update(schema.cards)
      .set({ is_primary: false })
      .where(and(eq(schema.cards.user_id, userId), ne(schema.cards.id, id)));
  }

  const [row] = await db
    .update(schema.cards)
    .set(parsed)
    .where(and(eq(schema.cards.id, id), eq(schema.cards.user_id, userId)))
    .returning();

  if (!row) throw new Error("Cartão não encontrado");

  revalidatePath("/dashboard/cards");
  return row;
}

/**
 * Remove o cartão, recusando quando há fatura ligada a ele.
 *
 * O banco já barraria por chave estrangeira, mas com uma mensagem que fala de
 * constraint e não de fatura. A checagem explícita troca isso por uma frase que
 * diz o que fazer — e a alternativa oferecida (desativar) preserva o histórico,
 * que é o que quase sempre se quer com um cartão que foi cancelado.
 */
export async function deleteCard(id: string): Promise<void> {
  const userId = await requireUserId();
  const db = getDb();

  const [row] = await db
    .select({ value: count() })
    .from(schema.statement_cycles)
    .where(
      and(eq(schema.statement_cycles.card_id, id), eq(schema.statement_cycles.user_id, userId)),
    );

  const faturas = row?.value ?? 0;

  if (faturas > 0) {
    throw new Error(
      `Este cartão tem ${faturas} fatura(s) registrada(s) e não pode ser excluído. ` +
        "Desative-o para parar de usá-lo sem perder o histórico.",
    );
  }

  const result = await db
    .delete(schema.cards)
    .where(and(eq(schema.cards.id, id), eq(schema.cards.user_id, userId)))
    .returning({ id: schema.cards.id });

  if (result.length === 0) throw new Error("Cartão não encontrado");

  revalidatePath("/dashboard/cards");
}
