import { describe, expect, it } from "vitest";
import { cpfValido } from "./cpf";

// 529.982.247-25 é um CPF de exemplo público, usado em documentação e testes.
describe("cpfValido", () => {
  it("aceita CPF com e sem pontuação", () => {
    expect(cpfValido("529.982.247-25")).toBe(true);
    expect(cpfValido("52998224725")).toBe(true);
  });

  it("recusa dígito verificador errado, tamanho errado e dígitos repetidos", () => {
    expect(cpfValido("529.982.247-24")).toBe(false);
    expect(cpfValido("5299822472")).toBe(false);
    expect(cpfValido("111.111.111-11")).toBe(false);
  });
});
