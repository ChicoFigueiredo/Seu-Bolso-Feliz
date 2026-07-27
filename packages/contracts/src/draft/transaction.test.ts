/**
 * Contrato de draft de transação.
 *
 * A asserção central deste arquivo é o par honesto/lançável: um draft recém-
 * gerado DEVE passar em `TransactionDraftV1Schema` e DEVE reprovar em
 * `TransactionPostableSchema` por falta de `financial_product_id`. Isso é a
 * codificação de "o worker não tem como saber a conta" — o fato que o schema
 * antigo escondia ao exigir tudo de uma vez e reprovar 100% dos drafts.
 */
import { describe, it, expect } from "vitest";
import {
  TransactionDraftV1Schema,
  TransactionPostableSchema,
  toTransactionInsert,
  type TransactionDraftV1,
} from "./transaction";
import { toTransactionDraft } from "../mappers/extraction-to-draft";
import { centsToNumeric, toCents } from "./common";

const CEMIG = {
  supplier_name_raw: "CEMIG",
  total_amount: 245.5,
  due_date: "2026-04-15",
  competence_date: "2026-03-01",
  category_suggestion: "energia_eletrica",
  tags_suggestion: ["essencial", "moradia"],
  document_number: "123456",
  contract_identifier: "12345678",
};

describe("TransactionDraftV1Schema — camada honesta", () => {
  it("aceita um draft recém-extraído, sem conta e sem data de evento", () => {
    const draft = toTransactionDraft(CEMIG);
    expect(TransactionDraftV1Schema.safeParse(draft).success).toBe(true);
  });

  it("guarda valor em centavos inteiros, não em ponto flutuante", () => {
    const draft = toTransactionDraft(CEMIG) as TransactionDraftV1;
    expect(draft.amount_cents).toBe(24550);
    expect(centsToNumeric(draft.amount_cents!)).toBe(245.5);
  });

  it("converte numeric devolvido como string pelo driver do Supabase", () => {
    // Origem exata do `amount` inválido que reprovava todo draft: o gerador
    // antigo repassava total_amount cru, e numeric(12,2) chega como string.
    const draft = toTransactionDraft({ ...CEMIG, total_amount: "245.50" }) as TransactionDraftV1;
    expect(draft.amount_cents).toBe(24550);
  });

  it("separa categoria sugerida de categoria canônica", () => {
    const draft = toTransactionDraft(CEMIG) as TransactionDraftV1;
    expect(draft.category_suggestion).toBe("energia_eletrica");
    expect(draft.category_id).toBeNull();
  });

  it("aceita entrada em camelCase e em snake_case", () => {
    const camel = toTransactionDraft({
      supplierNameRaw: "Vivo",
      totalAmount: 99.9,
      dueDate: "2026-04-20",
    }) as TransactionDraftV1;
    const snake = toTransactionDraft({
      supplier_name_raw: "Vivo",
      total_amount: 99.9,
      due_date: "2026-04-20",
    }) as TransactionDraftV1;
    expect(camel).toEqual(snake);
    expect(camel.supplier_name_raw).toBe("Vivo");
    expect(camel.amount_cents).toBe(9990);
  });

  it("não perde o vencimento ao deixar event_date nulo", () => {
    const draft = toTransactionDraft(CEMIG) as TransactionDraftV1;
    expect(draft.due_date).toBe("2026-04-15");
    expect(draft.event_date).toBeNull();
  });
});

describe("trava de regressão — o contrato antigo não pode voltar", () => {
  const draft = toTransactionDraft(CEMIG) as unknown as Record<string, unknown>;

  it('não emite o campo `type` com o literal português "despesa"', () => {
    expect(draft).not.toHaveProperty("type");
    expect(JSON.stringify(draft)).not.toContain("despesa");
  });

  it("usa `direction`, um termo que nenhum dos dois lados possuía", () => {
    expect(draft.direction).toBe("outflow");
  });

  it("não emite `amount` cru nem `category` polimórfico", () => {
    expect(draft).not.toHaveProperty("amount");
    expect(draft).not.toHaveProperty("category");
  });
});

describe("TransactionPostableSchema — camada de lançamento", () => {
  it("reprova um draft recém-gerado, apontando a conta que falta", () => {
    const draft = toTransactionDraft(CEMIG);
    const res = TransactionPostableSchema.safeParse(draft);
    expect(res.success).toBe(false);
    if (!res.success) {
      const paths = res.error.errors.map((e) => e.path.join("."));
      expect(paths).toContain("financial_product_id");
      expect(paths).toContain("event_date");
      expect(paths).toContain("transaction_type");
    }
  });

  it("aprova depois que o humano completa conta, tipo e data", () => {
    const draft = {
      ...(toTransactionDraft(CEMIG) as TransactionDraftV1),
      financial_product_id: "11111111-1111-4111-8111-111111111111",
      transaction_type: "expense" as const,
      event_date: "2026-04-15",
    };
    expect(TransactionPostableSchema.safeParse(draft).success).toBe(true);
  });

  it("rejeita valor zero ou negativo", () => {
    const base = {
      ...(toTransactionDraft(CEMIG) as TransactionDraftV1),
      financial_product_id: "11111111-1111-4111-8111-111111111111",
      transaction_type: "expense" as const,
      event_date: "2026-04-15",
    };
    for (const cents of [0, -1]) {
      const res = TransactionPostableSchema.safeParse({ ...base, amount_cents: cents });
      expect(res.success).toBe(false);
    }
  });
});

describe("toTransactionInsert", () => {
  const postable: TransactionDraftV1 = {
    ...(toTransactionDraft(CEMIG) as TransactionDraftV1),
    financial_product_id: "11111111-1111-4111-8111-111111111111",
    transaction_type: "expense",
    event_date: "2026-04-15",
  };

  it("produz exatamente as colunas de `transactions`", () => {
    const insert = toTransactionInsert(postable);
    expect(Object.keys(insert).sort()).toEqual(
      [
        "amount",
        "category_id",
        "competence_date",
        "description",
        "event_date",
        "financial_product_id",
        "is_confirmed",
        "metadata",
        "notes",
        "origin_type",
        "priority",
        "type",
      ].sort(),
    );
  });

  it("converte centavos de volta para numeric(15,2)", () => {
    expect(toTransactionInsert(postable).amount).toBe(245.5);
  });

  it("marca origem como import, nunca manual", () => {
    expect(toTransactionInsert(postable).origin_type).toBe("import");
  });

  it("preserva vencimento e proveniência em metadata, sem perdê-los", () => {
    const meta = toTransactionInsert(postable).metadata;
    expect(meta.due_date).toBe("2026-04-15");
    expect(meta.provenance).toBeDefined();
  });
});

describe("toCents", () => {
  it.each([
    [null, null],
    [undefined, null],
    ["", null],
    ["abc", null],
    [0, 0],
    [245.5, 24550],
    ["245.50", 24550],
    [0.1, 10],
    [1234.567, 123457],
  ])("toCents(%p) === %p", (input, expected) => {
    expect(toCents(input)).toBe(expected);
  });
});
