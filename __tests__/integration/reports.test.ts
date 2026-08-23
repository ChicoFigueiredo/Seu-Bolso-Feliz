/**
 * Relatórios: deduplicação ADR-001 (issues #17 e #18).
 *
 * O DEFEITO QUE ESTE ARQUIVO TRANCA: a página de relatórios somava
 * `transactions` cru. Numa vida financeira normal, quase toda despesa passa
 * pelo cartão, e cada uma aparecia DUAS vezes — como item da fatura e como
 * transação — enquanto o pagamento da fatura entrava como uma terceira. O
 * relatório inflava na proporção exata do que fosse pago com cartão, que é
 * justamente a maior parte. Um erro assim não parece bug: parece que se gastou
 * demais.
 *
 * A view `v_expenses_deduplicated` já existia e implementava a regra certa.
 * Ninguém a chamava.
 */
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";

const SUPABASE_URL = process.env.SUPABASE_URL ?? "http://127.0.0.1:54321";
const SUPABASE_SERVICE_KEY = exigirSupabaseSecretKey();

/**
 * A chave secreta local é fixa (mesmo valor em qualquer máquina, sai de
 * `supabase status`), mas não pode ficar hardcoded aqui: o padrão
 * `sb_secret_...` aciona o secret scanning do GitHub mesmo sendo local.
 * O CI exporta SUPABASE_SECRET_KEY logo após subir o Supabase local.
 */
function exigirSupabaseSecretKey(): string {
  const key = process.env.SUPABASE_SECRET_KEY;
  if (!key) {
    throw new Error(
      "SUPABASE_SECRET_KEY não definida. Rode `supabase status` e copie SECRET_KEY para .env.",
    );
  }
  return key;
}

const DE = "2026-03-01";
const ATE = "2026-03-31";

let supabase: SupabaseClient;
let userId: string;
let productId: string;
let cardId: string;
let cycleId: string;

async function limpar() {
  for (const tabela of [
    "statement_items",
    "transactions",
    "statement_cycles",
    "cards",
    "financial_products",
    "institutions",
  ]) {
    const { error } = await supabase.from(tabela).delete().eq("user_id", userId);
    if (error) throw new Error(`limpar ${tabela}: ${error.message}`);
  }
}

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  const email = "test-reports@sbf.local";
  const senha = "TestPass123!";
  const criado = await supabase.auth.admin.createUser({
    email,
    password: senha,
    email_confirm: true,
  });

  if (criado.data?.user?.id) {
    userId = criado.data.user.id;
  } else {
    // O usuário já existe de uma execução anterior. `listUsers()` pagina em 50
    // e os arquivos de integração criam usuários próprios, então procurar na
    // primeira página é uma aposta que falha assim que a suíte cresce — o
    // login devolve o id sem depender de paginação.
    const entrou = await supabase.auth.signInWithPassword({ email, password: senha });
    if (!entrou.data?.user?.id) {
      throw new Error(
        `não foi possível obter o usuário de teste: ${criado.error?.message ?? entrou.error?.message}`,
      );
    }
    userId = entrou.data.user.id;
  }

  await limpar();

  const { data: inst } = await supabase
    .from("institutions")
    .insert({ user_id: userId, name: "Banco Relatórios" })
    .select("id")
    .single();

  const { data: prod } = await supabase
    .from("financial_products")
    .insert({
      user_id: userId,
      institution_id: inst!.id,
      name: "Cartão Relatórios",
      type: "credit_card",
    })
    .select("id")
    .single();
  productId = prod!.id as string;

  const { data: card } = await supabase
    .from("cards")
    .insert({ user_id: userId, financial_product_id: productId, closing_day: 20, due_day: 28 })
    .select("id")
    .single();
  cardId = card!.id as string;

  const { data: cycle, error: cycleErr } = await supabase
    .from("statement_cycles")
    .insert({
      user_id: userId,
      card_id: cardId,
      reference_month: "2026-03-01",
      cycle_start_date: "2026-02-21",
      cycle_end_date: "2026-03-20",
      due_date: "2026-03-28",
      status: "open",
    })
    .select("id")
    .single();
  if (cycleErr) throw new Error(`statement_cycles: ${cycleErr.message}`);
  cycleId = cycle!.id as string;
});

afterAll(async () => {
  await limpar();
});

async function despesasDoPeriodo() {
  const { data, error } = await supabase
    .from("v_expenses_deduplicated")
    .select("canonical_id, source_type, amount, description")
    .eq("user_id", userId)
    .gte("event_date", DE)
    .lte("event_date", ATE);

  if (error) throw new Error(`view: ${error.message}`);
  return data ?? [];
}

const somar = (linhas: { amount: number | string }[]) =>
  linhas.reduce((s, r) => s + Number(r.amount), 0);

describe("v_expenses_deduplicated — a fatura não é contada em dobro", () => {
  it("item de fatura JÁ lançado como transação conta uma vez só", async () => {
    const { data: tx } = await supabase
      .from("transactions")
      .insert({
        user_id: userId,
        financial_product_id: productId,
        type: "expense",
        amount: 250,
        event_date: "2026-03-05",
        description: "Mercado",
      })
      .select("id")
      .single();

    const { error } = await supabase.from("statement_items").insert({
      user_id: userId,
      statement_cycle_id: cycleId,
      transaction_id: tx!.id,
      amount: 250,
      transaction_date: "2026-03-05",
      description: "Mercado",
    });
    if (error) throw new Error(`statement_items: ${error.message}`);

    const despesas = await despesasDoPeriodo();

    expect(despesas).toHaveLength(1);
    expect(despesas[0]!.source_type).toBe("transaction");
    expect(somar(despesas)).toBe(250);
  });

  it("item de fatura AINDA NÃO lançado conta como despesa", async () => {
    // Sem isto, tudo que estivesse na fatura e ainda não tivesse virado
    // transação simplesmente sumiria do relatório.
    const { error } = await supabase.from("statement_items").insert({
      user_id: userId,
      statement_cycle_id: cycleId,
      transaction_id: null,
      amount: 80,
      transaction_date: "2026-03-07",
      description: "Farmácia",
    });
    if (error) throw new Error(`statement_items: ${error.message}`);

    const despesas = await despesasDoPeriodo();

    expect(despesas).toHaveLength(2);
    expect(somar(despesas)).toBe(330);
    expect(despesas.find((d) => d.description === "Farmácia")!.source_type).toBe("statement_item");
  });

  it("o pagamento da fatura NÃO entra como despesa", async () => {
    // A terceira contagem: pagar a fatura não é gastar de novo o que já foi
    // gasto. É este lançamento que dobrava o mês inteiro do cartão.
    await supabase.from("transactions").insert({
      user_id: userId,
      financial_product_id: productId,
      type: "statement_payment",
      amount: 330,
      event_date: "2026-03-28",
      description: "Pagamento da fatura",
    });

    const despesas = await despesasDoPeriodo();

    expect(somar(despesas)).toBe(330);
    expect(despesas.some((d) => d.description === "Pagamento da fatura")).toBe(false);
  });

  it("estorno e transferência também ficam de fora", async () => {
    await supabase.from("transactions").insert([
      {
        user_id: userId,
        financial_product_id: productId,
        type: "refund",
        amount: 50,
        event_date: "2026-03-10",
        description: "Estorno",
      },
      {
        user_id: userId,
        financial_product_id: productId,
        type: "transfer",
        amount: 1000,
        event_date: "2026-03-11",
        description: "Transferência entre contas",
      },
    ]);

    expect(somar(await despesasDoPeriodo())).toBe(330);
  });

  it("taxa e juros contam — são despesa de verdade", async () => {
    await supabase.from("transactions").insert([
      {
        user_id: userId,
        financial_product_id: productId,
        type: "fee",
        amount: 12,
        event_date: "2026-03-12",
        description: "Anuidade",
      },
      {
        user_id: userId,
        financial_product_id: productId,
        type: "interest_charge",
        amount: 8,
        event_date: "2026-03-13",
        description: "Juros do rotativo",
      },
    ]);

    expect(somar(await despesasDoPeriodo())).toBe(350);
  });

  it("a soma crua de transactions diverge da view — é a medida do defeito", async () => {
    const { data: cru } = await supabase
      .from("transactions")
      .select("amount")
      .eq("user_id", userId)
      .in("type", ["expense", "fee", "interest_charge"])
      .gte("event_date", DE)
      .lte("event_date", ATE);

    // 250 + 12 + 8 = 270 no cru, contra 350 na view: o cru perde a compra de
    // fatura ainda não lançada (80) — e, num mês com mais itens de fatura já
    // conciliados, passaria a contar cada um deles duas vezes.
    expect(somar(cru ?? [])).toBe(270);
    expect(somar(await despesasDoPeriodo())).toBe(350);
  });
});
