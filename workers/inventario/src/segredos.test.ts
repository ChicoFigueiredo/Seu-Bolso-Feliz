import { describe, expect, it } from "vitest";
import { cifrar, decifrar } from "./segredos";

const SEGREDOS = { cpf: "52998224725", senhas: { "condominio-qnl": "abc123" } };
// Custo baixo só no teste: o de produção leva segundos de propósito.
const KDF_DE_TESTE = { N: 2 ** 10, r: 8, p: 1 };

describe("cofre de segredos", () => {
  it("decifra com a senha-mestra certa", () => {
    const arquivo = cifrar(SEGREDOS, "senha-mestra-de-teste", KDF_DE_TESTE);
    expect(decifrar(arquivo, "senha-mestra-de-teste")).toEqual(SEGREDOS);
  });

  it("não deixa nenhum segredo legível no arquivo", () => {
    const texto = JSON.stringify(cifrar(SEGREDOS, "senha-mestra-de-teste", KDF_DE_TESTE));
    expect(texto).not.toContain("52998224725");
    expect(texto).not.toContain("abc123");
  });

  it("recusa senha errada e arquivo adulterado com a mesma mensagem", () => {
    const arquivo = cifrar(SEGREDOS, "senha-mestra-de-teste", KDF_DE_TESTE);
    expect(() => decifrar(arquivo, "outra-senha-qualquer")).toThrow("senha-mestra errada");
    const adulterado = { ...arquivo, dados: Buffer.from("xx").toString("base64") };
    expect(() => decifrar(adulterado, "senha-mestra-de-teste")).toThrow("senha-mestra errada");
  });
});
