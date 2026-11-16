import { describe, expect, it } from "vitest";
import { nomeNaVista, trechoDeNome } from "./vista-nome";

describe("trechoDeNome", () => {
  it("tira acento, símbolo e barra, e corta sem deixar hífen no fim", () => {
    expect(trechoDeNome("Condomínio do Edifício / YES Ltda.")).toBe(
      "condominio-do-edificio-yes-ltda",
    );
    expect(trechoDeNome("A".repeat(60), 10)).toBe("aaaaaaaaaa");
    expect(trechoDeNome(null)).toBe("");
  });
});

describe("nomeNaVista", () => {
  it("monta data_fornecedor_valor_hash.ext", () => {
    expect(
      nomeNaVista({
        data_chave: "2026-10-25",
        supplier_name_raw: "Nu Pagamentos S.A.",
        valor_centavos: -123456,
        content_hash: "ab12cd34ef567890",
        extensao: "pdf",
        nome_original: "fatura.pdf",
      }),
    ).toBe("2026-10-25_nu-pagamentos-s-a_1234,56_ab12cd34.pdf");
  });

  it("sem fornecedor usa o nome original; sem data e valor, só o que existe", () => {
    expect(
      nomeNaVista({
        data_chave: null,
        supplier_name_raw: null,
        valor_centavos: null,
        content_hash: "ffeeddccbbaa",
        extensao: "jpg",
        nome_original: "IMG-20260501-WA0001.jpg",
      }),
    ).toBe("img-20260501-wa0001_ffeeddcc.jpg");
  });
});
