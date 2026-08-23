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
import { createClient } from "@/lib/supabase/server";
import { createCardSchema, updateCardSchema } from "@sbf/validation";
import type { Card, FinancialProduct, Insert, Update } from "@sbf/shared-types";

export type CardInput = Omit<Insert<"cards">, "user_id" | "id">;
export type CardUpdate = Update<"cards">;

/** Cartão com o produto financeiro e a instituição já resolvidos. */
export interface CardComProduto extends Card {
  financial_products:
    | (Pick<FinancialProduct, "id" | "name" | "type"> & {
        institutions: { name: string } | null;
      })
    | null;
}

export async function getCards(): Promise<CardComProduto[]> {
  const supabase = await createClient();

  const { data, error } = await supabase
    .from("cards")
    .select("*, financial_products(id, name, type, institutions(name))")
    .order("is_primary", { ascending: false })
    .order("created_at");

  if (error) throw new Error(error.message);
  return (data ?? []) as unknown as CardComProduto[];
}

export async function getCard(id: string): Promise<CardComProduto | null> {
  const supabase = await createClient();

  const { data, error } = await supabase
    .from("cards")
    .select("*, financial_products(id, name, type, institutions(name))")
    .eq("id", id)
    .single();

  if (error) {
    if (error.code === "PGRST116") return null;
    throw new Error(error.message);
  }
  return data as unknown as CardComProduto;
}

/**
 * Produtos elegíveis a virar cartão.
 *
 * `credit_card` é o tipo esperado, mas cartões de débito também existem e
 * pendurar a lista num único tipo deixaria a tela vazia para quem cadastrou o
 * produto como conta corrente.
 */
export async function getProdutosParaCartao(): Promise<FinancialProduct[]> {
  const supabase = await createClient();

  const { data, error } = await supabase
    .from("financial_products")
    .select("*")
    .in("type", ["credit_card", "checking_account"])
    .order("name");

  if (error) throw new Error(error.message);
  return data ?? [];
}

export async function createCard(input: CardInput): Promise<Card> {
  const parsed = createCardSchema.parse(input);

  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  // Só um cartão principal por vez: marcar um novo como principal rebaixa o
  // anterior. Sem isto, dois cartões "principais" fariam a home escolher um
  // deles por ordem de criação — silenciosamente, e mudando com o tempo.
  if (parsed.is_primary) {
    await supabase.from("cards").update({ is_primary: false }).eq("user_id", user.id);
  }

  const { data, error } = await supabase
    .from("cards")
    .insert({ ...parsed, user_id: user.id })
    .select()
    .single();

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/cards");
  return data;
}

export async function updateCard(id: string, input: CardUpdate): Promise<Card> {
  const parsed = updateCardSchema.parse(input);

  const supabase = await createClient();
  const {
    data: { user },
  } = await supabase.auth.getUser();
  if (!user) throw new Error("Não autenticado");

  if (parsed.is_primary) {
    await supabase.from("cards").update({ is_primary: false }).eq("user_id", user.id).neq("id", id);
  }

  const { data, error } = await supabase
    .from("cards")
    .update(parsed)
    .eq("id", id)
    .select()
    .single();

  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/cards");
  return data;
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
  const supabase = await createClient();

  const { count, error: countError } = await supabase
    .from("statement_cycles")
    .select("id", { count: "exact", head: true })
    .eq("card_id", id);

  if (countError) throw new Error(countError.message);

  if (count && count > 0) {
    throw new Error(
      `Este cartão tem ${count} fatura(s) registrada(s) e não pode ser excluído. ` +
        "Desative-o para parar de usá-lo sem perder o histórico.",
    );
  }

  const { error } = await supabase.from("cards").delete().eq("id", id);
  if (error) throw new Error(error.message);

  revalidatePath("/dashboard/cards");
}
