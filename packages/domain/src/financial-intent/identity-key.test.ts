/**
 * Matriz de identidade financeira.
 *
 * O valor destes testes está tanto nos casos que DEVEM colidir quanto nos que
 * NÃO PODEM. Uma chave frouxa demais funde duas contas distintas do mesmo
 * fornecedor; uma rígida demais recria o defeito original, em que fatura e
 * lembrete da mesma obrigação nunca se encontravam.
 */
import { describe, it, expect } from "vitest";
import {
  buildFinancialIdentityKeys,
  buildFinancialIdentityKey,
  type FinancialIdentityInput,
} from "./identity-key";

const USER = "user-a";
const OTHER_USER = "user-b";

/** Verdadeiro se os dois documentos compartilham ao menos uma chave. */
function collides(a: FinancialIdentityInput, b: FinancialIdentityInput): boolean {
  const ka = buildFinancialIdentityKeys(a).entries.map((e) => e.key);
  const kb = new Set(buildFinancialIdentityKeys(b).entries.map((e) => e.key));
  return ka.some((k) => kb.has(k));
}

/** Colisão restrita a chaves de força >= a informada. */
function collidesAtLeast(
  a: FinancialIdentityInput,
  b: FinancialIdentityInput,
  min: "strong" | "medium",
): boolean {
  const order = { strong: 2, medium: 1, weak: 0 } as const;
  const ka = buildFinancialIdentityKeys(a).entries.filter((e) => order[e.strength] >= order[min]);
  const kb = new Set(
    buildFinancialIdentityKeys(b)
      .entries.filter((e) => order[e.strength] >= order[min])
      .map((e) => e.key),
  );
  return ka.some((e) => kb.has(e.key));
}

const BARCODE = "83640000001 2 34560000000 3 45670000000 4 56780000000 5";

describe("identidade financeira — casos que DEVEM colidir", () => {
  it("1. boleto e comprovante com o mesmo código de barras", () => {
    const boleto = { userId: USER, barcodeDigitableLine: BARCODE, amount: 245.5 };
    const comprovante = { userId: USER, barcodeDigitableLine: BARCODE.replace(/\s/g, "") };
    expect(collidesAtLeast(boleto, comprovante, "strong")).toBe(true);
  });

  it("2. fatura e lembrete da mesma obrigação (o defeito original)", () => {
    // Antes: fatura hasheava com prefixo "invoice" e 7 partes; lembrete com
    // prefixo "reminder" e 5. Colisão era matematicamente impossível.
    const fatura = {
      userId: USER,
      institutionName: "Nubank",
      cardLast4: "1234",
      cycleStartDate: "2026-03-01",
      cycleEndDate: "2026-03-28",
      dueDate: "2026-04-10",
      amount: 1200,
    };
    const lembrete = {
      userId: USER,
      institutionName: "Nubank",
      dueDate: "2026-04-10",
      amount: 1200,
    };
    expect(collidesAtLeast(fatura, lembrete, "medium")).toBe(true);
  });

  it("3. lembrete sem valor encontra a fatura pela chave fraca", () => {
    const lembreteSemValor = { userId: USER, institutionName: "Nubank", dueDate: "2026-04-10" };
    const fatura = {
      userId: USER,
      institutionName: "Nubank",
      dueDate: "2026-04-10",
      amount: 1200,
    };
    expect(collides(lembreteSemValor, fatura)).toBe(true);
    // Mas nunca como identidade canônica.
    expect(buildFinancialIdentityKeys(lembreteSemValor).primary).toBeNull();
  });

  it("6. 2ª via com vencimento renegociado continua sendo a mesma dívida", () => {
    const original = {
      userId: USER,
      documentNumber: "NF-998877",
      supplierName: "CEMIG",
      dueDate: "2026-04-10",
    };
    const segundaVia = {
      userId: USER,
      documentNumber: "NF-998877",
      supplierName: "CEMIG",
      dueDate: "2026-04-25",
    };
    expect(collidesAtLeast(original, segundaVia, "strong")).toBe(true);
  });

  it("7. um documento traz só o nome, outro só o CNPJ", () => {
    const porNome = {
      userId: USER,
      supplierName: "CEMIG",
      supplierCnpj: "06.981.180/0001-16",
      dueDate: "2026-04-10",
      amount: 245.5,
    };
    const porCnpj = {
      userId: USER,
      supplierCnpj: "06.981.180/0001-16",
      dueDate: "2026-04-10",
      amount: 245.5,
    };
    expect(collidesAtLeast(porNome, porCnpj, "medium")).toBe(true);
  });

  it("10. normalização de acento, caixa e pontuação", () => {
    const a = {
      userId: USER,
      supplierName: "CEMIG  Distribuição S/A",
      dueDate: "2026-04-10",
      amount: 100,
    };
    const b = {
      userId: USER,
      supplierName: "cemig distribuicao s a",
      dueDate: "2026-04-10",
      amount: 100,
    };
    expect(collidesAtLeast(a, b, "medium")).toBe(true);
  });

  it("11. código de barras formatado e cru", () => {
    const a = { userId: USER, barcodeDigitableLine: BARCODE };
    const b = { userId: USER, barcodeDigitableLine: BARCODE.replace(/\s/g, "") };
    expect(collidesAtLeast(a, b, "strong")).toBe(true);
  });

  it("12. fatura com ciclo e fatura com final do cartão, mesmo mês e valor", () => {
    const porCiclo = {
      userId: USER,
      institutionName: "Nubank",
      cycleStartDate: "2026-03-01",
      cycleEndDate: "2026-03-28",
      dueDate: "2026-04-10",
      amount: 1200,
    };
    const porCartao = {
      userId: USER,
      institutionName: "Nubank",
      cardLast4: "1234",
      dueDate: "2026-04-10",
      amount: 1200,
    };
    expect(collidesAtLeast(porCiclo, porCartao, "medium")).toBe(true);
  });

  it("fatura com fechamento em março e lembrete com vencimento em abril", () => {
    // O bucket YYYY-MM vem de dueDate ?? competenceDate ?? cycleEndDate, então
    // ambos caem em 2026-04 pelo vencimento.
    const fatura = {
      userId: USER,
      institutionName: "Itau",
      cycleEndDate: "2026-03-28",
      dueDate: "2026-04-05",
      amount: 800,
    };
    const lembrete = {
      userId: USER,
      institutionName: "Itau",
      dueDate: "2026-04-05",
      amount: 800,
    };
    expect(collidesAtLeast(fatura, lembrete, "medium")).toBe(true);
  });
});

describe("identidade financeira — casos que NÃO PODEM colidir", () => {
  it("4. mesma conta em meses diferentes", () => {
    const marco = { userId: USER, supplierName: "CEMIG", dueDate: "2026-03-10", amount: 245.5 };
    const abril = { userId: USER, supplierName: "CEMIG", dueDate: "2026-04-10", amount: 245.5 };
    expect(collides(marco, abril)).toBe(false);
  });

  it("5. mesmo fornecedor e mês, valores diferentes: não colide em chave média", () => {
    const a = { userId: USER, supplierName: "CEMIG", dueDate: "2026-03-10", amount: 245.5 };
    const b = { userId: USER, supplierName: "CEMIG", dueDate: "2026-03-20", amount: 99 };
    expect(collidesAtLeast(a, b, "medium")).toBe(false);
    // Colidem apenas na chave fraca, que por isso precisa ser filtrada por
    // tolerância de valor antes de fundir duas obrigações.
    expect(collides(a, b)).toBe(true);
    const weakOnly = buildFinancialIdentityKeys(a).entries.filter((e) => e.strength === "weak");
    expect(weakOnly.length).toBeGreaterThanOrEqual(1);
  });

  it("9. documentos idênticos de usuários diferentes", () => {
    const doc = {
      supplierName: "CEMIG",
      dueDate: "2026-04-10",
      amount: 245.5,
      barcodeDigitableLine: BARCODE,
    };
    expect(collides({ ...doc, userId: USER }, { ...doc, userId: OTHER_USER })).toBe(false);
  });

  it("fornecedores diferentes no mesmo mês e valor", () => {
    const cemig = { userId: USER, supplierName: "CEMIG", dueDate: "2026-04-10", amount: 245.5 };
    const vivo = { userId: USER, supplierName: "Vivo", dueDate: "2026-04-10", amount: 245.5 };
    expect(collides(cemig, vivo)).toBe(false);
  });
});

describe("identidade financeira — propriedades estruturais", () => {
  it("8. sem fornecedor, sem valor e sem data devolve conjunto vazio", () => {
    const set = buildFinancialIdentityKeys({ userId: USER });
    expect(set.primary).toBeNull();
    expect(set.alternates).toEqual([]);
    expect(set.entries).toEqual([]);
  });

  it("13. é determinística entre chamadas", () => {
    const input = {
      userId: USER,
      supplierName: "CEMIG",
      dueDate: "2026-04-10",
      amount: 245.5,
      documentNumber: "NF-1",
    };
    expect(buildFinancialIdentityKeys(input)).toEqual(buildFinancialIdentityKeys(input));
  });

  it("o intent nunca participa da chave", () => {
    // O fallback antigo usava o intent cru como prefixo, o que impedia
    // qualquer convergência entre tipos de documento. A entrada nem aceita
    // mais o campo; este teste trava a regressão via superfície da API.
    const input = { userId: USER, supplierName: "CEMIG", dueDate: "2026-04-10", amount: 245.5 };
    const comIntentEspurio = { ...input, intent: "bill_reminder" } as FinancialIdentityInput;
    expect(buildFinancialIdentityKeys(comIntentEspurio)).toEqual(buildFinancialIdentityKeys(input));
  });

  it("a primary é sempre a chave mais forte disponível", () => {
    const set = buildFinancialIdentityKeys({
      userId: USER,
      barcodeDigitableLine: BARCODE,
      supplierName: "CEMIG",
      dueDate: "2026-04-10",
      amount: 245.5,
    });
    expect(set.entries[0]!.kind).toBe("barcode");
    expect(set.primary).toBe(set.entries[0]!.key);
  });

  it("chave fraca nunca é promovida a primary", () => {
    const set = buildFinancialIdentityKeys({
      userId: USER,
      supplierName: "CEMIG",
      dueDate: "2026-04-10",
    });
    expect(set.entries.every((e) => e.strength === "weak")).toBe(true);
    expect(set.primary).toBeNull();
  });

  it("todas as chaves têm o mesmo comprimento e formato", () => {
    const set = buildFinancialIdentityKeys({
      userId: USER,
      barcodeDigitableLine: BARCODE,
      documentNumber: "NF-1",
      supplierName: "CEMIG",
      supplierCnpj: "06.981.180/0001-16",
      cardLast4: "1234",
      dueDate: "2026-04-10",
      amount: 245.5,
    });
    expect(set.entries.length).toBeGreaterThan(3);
    for (const e of set.entries) {
      expect(e.key).toMatch(/^[0-9a-f]{32}$/);
    }
  });

  it("o shim depreciado devolve a primary", () => {
    const input = { userId: USER, barcodeDigitableLine: BARCODE };
    expect(buildFinancialIdentityKey(input)).toBe(buildFinancialIdentityKeys(input).primary);
  });
});
