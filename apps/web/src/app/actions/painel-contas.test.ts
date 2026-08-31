import { describe, expect, it } from "vitest";
import {
  montarPainelContas,
  type CartaoParaPainel,
  type CicloParaPainel,
  type InstituicaoParaPainel,
  type ProdutoParaPainel,
} from "@/components/contas/montar-painel";

// Testes de unidade puros: `montarPainelContas` não toca banco, então não
// precisam de `TEST_DATABASE_URL` nem do project "integration" do vitest
// (ver comentário no topo de painel-contas.ts sobre por que a função é pura).

function produto(overrides: Partial<ProdutoParaPainel> = {}): ProdutoParaPainel {
  return {
    id: "produto-1",
    institution_id: "inst-1",
    name: "Produto",
    type: "checking_account",
    current_balance: 0,
    credit_limit: null,
    is_active: true,
    ...overrides,
  };
}

function instituicao(overrides: Partial<InstituicaoParaPainel> = {}): InstituicaoParaPainel {
  return {
    id: "inst-1",
    name: "Banco de Teste",
    type: "bank",
    is_active: true,
    ...overrides,
  };
}

function card(overrides: Partial<CartaoParaPainel> = {}): CartaoParaPainel {
  return {
    id: "card-1",
    financial_product_id: "produto-1",
    last_four_digits: "1234",
    card_brand: "Mastercard",
    holder_name: "Chico",
    is_primary: true,
    is_active: true,
    closing_day: 20,
    due_day: 28,
    financial_products: {
      id: "produto-1",
      name: "Produto",
      type: "credit_card",
      institutions: null,
    },
    ...overrides,
  };
}

function ciclo(overrides: Partial<CicloParaPainel> = {}): CicloParaPainel {
  return {
    id: "ciclo-1",
    card_id: "card-1",
    reference_month: "2026-08-01",
    cycle_start_date: "2026-07-21",
    cycle_end_date: "2026-08-20",
    due_date: "2026-08-28",
    total_amount: 0,
    paid_amount: 0,
    status: "open",
    ...overrides,
  };
}

describe("montarPainelContas", () => {
  it("separa contas (checking/savings) de outros tipos de produto e soma só as ativas no total", () => {
    const painel = montarPainelContas({
      produtos: [
        produto({
          id: "p-corrente",
          type: "checking_account",
          current_balance: 1000,
          is_active: true,
        }),
        produto({
          id: "p-poupanca",
          type: "savings_account",
          current_balance: 500,
          is_active: true,
        }),
        produto({
          id: "p-inativa",
          type: "checking_account",
          current_balance: 999,
          is_active: false,
        }),
        produto({
          id: "p-investimento",
          type: "investment",
          current_balance: 5000,
          is_active: true,
        }),
        produto({ id: "p-cartao", type: "credit_card", current_balance: 0, is_active: true }),
      ],
      cards: [],
      instituicoes: [instituicao()],
      ciclos: [],
    });

    expect(painel.contas.map((c) => c.id).sort()).toEqual([
      "p-corrente",
      "p-inativa",
      "p-poupanca",
    ]);
    // A conta inativa aparece na lista (para poder ser reativada/editada) mas
    // não entra no total — mesmo recorte de `disponivelAgora` em painel-hoje.ts.
    expect(painel.totalContas).toBe(1500);
  });

  it("resolve o nome da instituição de cada conta", () => {
    const painel = montarPainelContas({
      produtos: [produto({ institution_id: "inst-nubank" })],
      cards: [],
      instituicoes: [instituicao({ id: "inst-nubank", name: "Nubank" })],
      ciclos: [],
    });
    expect(painel.contas[0]?.instituicao).toBe("Nubank");
  });

  it("cruza a fatura pelo id do cartão (cards.id), não pelo id do produto financeiro", () => {
    // Este é o teste de regressão do bug encontrado em painel-hoje.ts: lá, o
    // cruzamento usa `financial_products.id` como chave para um mapa indexado
    // por `statement_cycles.card_id` — como as duas colunas nunca coincidem
    // (tabelas diferentes, UUIDs independentes), a fatura de todo cartão some.
    // Aqui, card.id ("card-xyz") e o id do produto financeiro ("produto-abc")
    // são deliberadamente diferentes, como sempre são na base real.
    const painel = montarPainelContas({
      produtos: [produto({ id: "produto-abc", type: "credit_card", credit_limit: 3000 })],
      cards: [card({ id: "card-xyz", financial_product_id: "produto-abc" })],
      instituicoes: [instituicao()],
      ciclos: [
        ciclo({
          id: "ciclo-1",
          card_id: "card-xyz",
          total_amount: 850,
          paid_amount: 0,
          status: "open",
        }),
      ],
    });

    expect(painel.cartoes).toHaveLength(1);
    expect(painel.cartoes[0]?.cicloAtual?.id).toBe("ciclo-1");
    expect(painel.cartoes[0]?.faturaAtual).toBe(850);
    expect(painel.cartoes[0]?.limiteLivre).toBe(2150);
  });

  it("fatura atual é a não paga mais antiga, não a de vencimento mais distante", () => {
    const painel = montarPainelContas({
      produtos: [produto({ id: "p1", type: "credit_card", credit_limit: 1000 })],
      cards: [card({ id: "c1", financial_product_id: "p1" })],
      instituicoes: [instituicao()],
      ciclos: [
        ciclo({
          id: "antiga",
          card_id: "c1",
          due_date: "2026-07-28",
          status: "overdue",
          total_amount: 200,
          paid_amount: 0,
        }),
        ciclo({
          id: "recente",
          card_id: "c1",
          due_date: "2026-08-28",
          status: "open",
          total_amount: 300,
          paid_amount: 0,
        }),
      ],
    });

    const cartao = painel.cartoes[0]!;
    expect(cartao.cicloAtual?.id).toBe("antiga");
    expect(cartao.faturaAtual).toBe(200);
    // As duas continuam disponíveis para a seção de Faturas, mais recente primeiro.
    expect(cartao.ciclos.map((f) => f.id)).toEqual(["recente", "antiga"]);
  });

  it("todas as faturas pagas: fatura atual é null e o valor é zero, não soma dívida quitada", () => {
    const painel = montarPainelContas({
      produtos: [produto({ id: "p1", type: "credit_card", credit_limit: 1000 })],
      cards: [card({ id: "c1", financial_product_id: "p1" })],
      instituicoes: [instituicao()],
      ciclos: [
        ciclo({
          id: "quitada",
          card_id: "c1",
          status: "paid",
          total_amount: 400,
          paid_amount: 400,
        }),
      ],
    });

    const cartao = painel.cartoes[0]!;
    expect(cartao.cicloAtual).toBeNull();
    expect(cartao.faturaAtual).toBe(0);
    expect(cartao.limiteLivre).toBe(1000);
  });

  it("cartão sem nenhum ciclo: fatura zero e limite livre igual ao limite total", () => {
    const painel = montarPainelContas({
      produtos: [produto({ id: "p1", type: "credit_card", credit_limit: 800 })],
      cards: [card({ id: "c1", financial_product_id: "p1" })],
      instituicoes: [instituicao()],
      ciclos: [],
    });

    const cartao = painel.cartoes[0]!;
    expect(cartao.ciclos).toEqual([]);
    expect(cartao.cicloAtual).toBeNull();
    expect(cartao.faturaAtual).toBe(0);
    expect(cartao.limiteLivre).toBe(800);
  });

  it("sem limite cadastrado, limite total e limite livre ficam null (não zero: são coisas diferentes)", () => {
    const painel = montarPainelContas({
      produtos: [produto({ id: "p1", type: "credit_card", credit_limit: null })],
      cards: [card({ id: "c1", financial_product_id: "p1" })],
      instituicoes: [instituicao()],
      ciclos: [],
    });

    const cartao = painel.cartoes[0]!;
    expect(cartao.limiteTotal).toBeNull();
    expect(cartao.limiteLivre).toBeNull();
  });

  it("exclui da lista de cartões um `cards` ligado a um produto que não é cartão de crédito (débito)", () => {
    const painel = montarPainelContas({
      produtos: [produto({ id: "p-debito", type: "checking_account" })],
      cards: [card({ id: "c-debito", financial_product_id: "p-debito" })],
      instituicoes: [instituicao()],
      ciclos: [],
    });
    expect(painel.cartoes).toEqual([]);
  });

  it("usa o nome/instituição do join de getCards quando o produto não é encontrado no mapa (defesa em profundidade)", () => {
    const painel = montarPainelContas({
      produtos: [],
      cards: [
        card({
          id: "c1",
          financial_product_id: "produto-sumido",
          financial_products: {
            id: "produto-sumido",
            name: "Cartão Recuperado",
            type: "credit_card",
            institutions: { name: "Banco X" },
          },
        }),
      ],
      instituicoes: [],
      ciclos: [],
    });

    expect(painel.cartoes).toHaveLength(1);
    expect(painel.cartoes[0]?.nome).toBe("Cartão Recuperado");
    expect(painel.cartoes[0]?.instituicao).toBe("Banco X");
    // Sem o produto no mapa, o limite não pode ser resolvido com segurança.
    expect(painel.cartoes[0]?.limiteTotal).toBeNull();
  });

  it("converte valores monetários vindos como string (driver) com segurança", () => {
    const painel = montarPainelContas({
      produtos: [
        produto({
          id: "p1",
          type: "checking_account",
          current_balance: "1234.56" as unknown as number,
        }),
      ],
      cards: [],
      instituicoes: [instituicao()],
      ciclos: [],
    });
    expect(painel.contas[0]?.saldo).toBe(1234.56);
  });
});
