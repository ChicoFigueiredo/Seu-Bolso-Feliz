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
 *
 * Sem RLS, o `where user_id = ...` da consulta à view deixou de ser cerimônia:
 * é ele que separa este arquivo dos outros que compartilham o mesmo banco.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { describe, it, expect, beforeAll, afterAll } from "vitest";
import { sql } from "drizzle-orm";
import { cards, financial_products, institutions, statement_cycles, type Database } from "@sbf/db";
import { getTestDb, limparPorUsuario, linha, linhas, semBancoDeTeste } from "../helpers/db";

const DE = "2026-03-01";
const ATE = "2026-03-31";

const USER_ID = "20000000-0000-4000-8000-000000000001";

/** Filhas antes das pais — sem cascade de `auth.users` para socorrer. */
const TABELAS = [
  "statement_items",
  "transactions",
  "statement_cycles",
  "cards",
  "financial_products",
  "institutions",
] as const;

let db: Database;
let productId: string;
let cardId: string;
let cycleId: string;

interface Despesa {
  canonical_id: string;
  source_type: string;
  amount: number | string;
  description: string | null;
}

async function despesasDoPeriodo(): Promise<Despesa[]> {
  return linhas<Despesa>(
    db,
    sql`select canonical_id, source_type, amount, description
          from public.v_expenses_deduplicated
         where user_id = ${USER_ID}::uuid
           and event_date >= ${DE}::date
           and event_date <= ${ATE}::date`,
  );
}

const somar = (linhasDespesa: { amount: number | string }[]) =>
  linhasDespesa.reduce((s, r) => s + Number(r.amount), 0);

async function inserirTransacao(valores: {
  type: string;
  amount: number;
  event_date: string;
  description: string;
}): Promise<string> {
  const row = await linha<{ id: string }>(
    db,
    sql`insert into public.transactions
          (user_id, financial_product_id, type, amount, event_date, description)
        values (${USER_ID}::uuid, ${productId}::uuid, ${valores.type}, ${valores.amount},
                ${valores.event_date}::date, ${valores.description})
        returning id`,
  );
  return row!.id;
}

async function inserirItemDeFatura(valores: {
  transactionId: string | null;
  amount: number;
  transaction_date: string;
  description: string;
}): Promise<void> {
  await db.execute(sql`
    insert into public.statement_items
      (user_id, statement_cycle_id, transaction_id, amount, transaction_date, description)
    values (${USER_ID}::uuid, ${cycleId}::uuid, ${valores.transactionId}::uuid,
            ${valores.amount}, ${valores.transaction_date}::date, ${valores.description})
  `);
}

describe.skipIf(semBancoDeTeste)(
  "v_expenses_deduplicated — a fatura não é contada em dobro",
  () => {
    beforeAll(async () => {
      db = getTestDb();
      await limparPorUsuario(db, TABELAS, USER_ID);

      const [inst] = await db
        .insert(institutions)
        .values({ user_id: USER_ID, name: "Banco Relatórios" })
        .returning({ id: institutions.id });

      const [prod] = await db
        .insert(financial_products)
        .values({
          user_id: USER_ID,
          institution_id: inst!.id,
          name: "Cartão Relatórios",
          type: "credit_card",
        })
        .returning({ id: financial_products.id });
      productId = prod!.id;

      const [card] = await db
        .insert(cards)
        .values({
          user_id: USER_ID,
          financial_product_id: productId,
          closing_day: 20,
          due_day: 28,
        })
        .returning({ id: cards.id });
      cardId = card!.id;

      const [cycle] = await db
        .insert(statement_cycles)
        .values({
          user_id: USER_ID,
          card_id: cardId,
          reference_month: "2026-03-01",
          cycle_start_date: "2026-02-21",
          cycle_end_date: "2026-03-20",
          due_date: "2026-03-28",
          status: "open",
        })
        .returning({ id: statement_cycles.id });
      cycleId = cycle!.id;
    });

    afterAll(async () => {
      await limparPorUsuario(db, TABELAS, USER_ID);
    });

    it("item de fatura JÁ lançado como transação conta uma vez só", async () => {
      const txId = await inserirTransacao({
        type: "expense",
        amount: 250,
        event_date: "2026-03-05",
        description: "Mercado",
      });

      await inserirItemDeFatura({
        transactionId: txId,
        amount: 250,
        transaction_date: "2026-03-05",
        description: "Mercado",
      });

      const despesas = await despesasDoPeriodo();

      expect(despesas).toHaveLength(1);
      expect(despesas[0]!.source_type).toBe("transaction");
      expect(somar(despesas)).toBe(250);
    });

    it("item de fatura AINDA NÃO lançado conta como despesa", async () => {
      // Sem isto, tudo que estivesse na fatura e ainda não tivesse virado
      // transação simplesmente sumiria do relatório.
      await inserirItemDeFatura({
        transactionId: null,
        amount: 80,
        transaction_date: "2026-03-07",
        description: "Farmácia",
      });

      const despesas = await despesasDoPeriodo();

      expect(despesas).toHaveLength(2);
      expect(somar(despesas)).toBe(330);
      expect(despesas.find((d) => d.description === "Farmácia")!.source_type).toBe(
        "statement_item",
      );
    });

    it("o pagamento da fatura NÃO entra como despesa", async () => {
      // A terceira contagem: pagar a fatura não é gastar de novo o que já foi
      // gasto. É este lançamento que dobrava o mês inteiro do cartão.
      await inserirTransacao({
        type: "statement_payment",
        amount: 330,
        event_date: "2026-03-28",
        description: "Pagamento da fatura",
      });

      const despesas = await despesasDoPeriodo();

      expect(somar(despesas)).toBe(330);
      expect(despesas.some((d) => d.description === "Pagamento da fatura")).toBe(false);
    });

    it("estorno, entrada, ajuste e pagamento de dívida também ficam de fora", async () => {
      // Esta asserção era vazia até agora. A versão anterior inseria um
      // `refund` e um `transfer` de uma vez só, sem checar o erro devolvido —
      // e `transfer` NÃO é um tipo válido de `transactions` (transferência tem
      // tabela própria, `public.transfers`). O check constraint recusava a
      // instrução inteira, nenhuma das duas linhas entrava, e o teste
      // "confirmava" que a view ignora movimentações que nunca existiram.
      //
      // Agora entram os quatro tipos que o schema realmente admite e que a view
      // precisa deixar de fora, um insert por vez, com o erro propagando.
      for (const [type, description] of [
        ["refund", "Estorno"],
        ["income", "Salário"],
        ["adjustment", "Ajuste de saldo"],
        ["liability_payment", "Parcela do financiamento"],
      ] as const) {
        await inserirTransacao({ type, amount: 1000, event_date: "2026-03-11", description });
      }

      expect(somar(await despesasDoPeriodo())).toBe(330);
    });

    it("taxa e juros contam — são despesa de verdade", async () => {
      await inserirTransacao({
        type: "fee",
        amount: 12,
        event_date: "2026-03-12",
        description: "Anuidade",
      });
      await inserirTransacao({
        type: "interest_charge",
        amount: 8,
        event_date: "2026-03-13",
        description: "Juros do rotativo",
      });

      expect(somar(await despesasDoPeriodo())).toBe(350);
    });

    it("a soma crua de transactions diverge da view — é a medida do defeito", async () => {
      const cru = await linhas<{ amount: string }>(
        db,
        sql`select amount from public.transactions
           where user_id = ${USER_ID}::uuid
             and type in ('expense', 'fee', 'interest_charge')
             and event_date >= ${DE}::date
             and event_date <= ${ATE}::date`,
      );

      // 250 + 12 + 8 = 270 no cru, contra 350 na view: o cru perde a compra de
      // fatura ainda não lançada (80) — e, num mês com mais itens de fatura já
      // conciliados, passaria a contar cada um deles duas vezes.
      expect(somar(cru)).toBe(270);
      expect(somar(await despesasDoPeriodo())).toBe(350);
    });
  },
);
