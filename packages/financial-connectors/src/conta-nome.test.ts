import { describe, it, expect } from "vitest";
import { nomeDeExibicaoDaConta } from "./conta-nome";

describe("nomeDeExibicaoDaConta", () => {
  it("diferencia as 31 subcontas da Caixa pelo número", () => {
    expect(
      nomeDeExibicaoDaConta({ name: "CAIXA", subtype: "CHECKING_ACCOUNT", number: "400000181-6" }),
    ).toBe("CAIXA · 400000181-6");
  });

  it("poupança vinculada com o MESMO número da corrente ganha nomes distintos", () => {
    // Caso real: 593889552-5 existe como corrente E como poupança.
    const corrente = nomeDeExibicaoDaConta({
      name: "CAIXA",
      subtype: "CHECKING_ACCOUNT",
      number: "593889552-5",
    });
    const poupanca = nomeDeExibicaoDaConta({
      name: "CAIXA",
      subtype: "SAVINGS_ACCOUNT",
      number: "593889552-5",
    });
    expect(corrente).toBe("CAIXA · 593889552-5");
    expect(poupanca).toBe("CAIXA Poupança · 593889552-5");
    expect(corrente).not.toBe(poupanca);
  });

  it("remove o padding que a Pluggy manda no nome do cartão", () => {
    expect(
      nomeDeExibicaoDaConta({
        name: "BANDEIRADO                                        ",
        subtype: "CREDIT_CARD",
        number: "6294",
      }),
    ).toBe("BANDEIRADO · 6294");
  });

  it("sem número, fica só o nome", () => {
    expect(nomeDeExibicaoDaConta({ name: "gold", subtype: "CREDIT_CARD" })).toBe("gold");
  });

  it("nome vazio não produz cartão sem título", () => {
    expect(nomeDeExibicaoDaConta({ name: "   ", subtype: undefined, number: "12-3" })).toBe(
      "Conta · 12-3",
    );
  });
});
