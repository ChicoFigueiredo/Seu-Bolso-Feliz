import { describe, it, expect } from "vitest";
import { nomeDeCategoria } from "./categorias";

describe("nomeDeCategoria", () => {
  it("traduz as sugestões mais frequentes da conta real", () => {
    expect(nomeDeCategoria("Transfer - PIX")).toBe("PIX");
    expect(nomeDeCategoria("Groceries")).toBe("Mercado");
    expect(nomeDeCategoria("Eating out")).toBe("Restaurantes");
    expect(nomeDeCategoria("Taxi and ride-hailing")).toBe("Transporte por app");
  });

  it("faz sugestões diferentes convergirem numa categoria só", () => {
    // O usuário quer UMA linha no relatório, não três.
    for (const s of ["Transfers", "Transfer - TED", "Transfer - DOC", "Transfer - Check"]) {
      expect(nomeDeCategoria(s)).toBe("Transferências");
    }
    expect(nomeDeCategoria("Gyms")).toBe(nomeDeCategoria("Gyms and fitness centers"));
    expect(nomeDeCategoria("Gambling")).toBe("Loteria");
    expect(nomeDeCategoria("Lottery")).toBe("Loteria");
    expect(nomeDeCategoria("Health")).toBe(nomeDeCategoria("Healthcare"));
  });

  it("devolve o nome cru quando não conhece a sugestão", () => {
    // Rastreável até a documentação da Pluggy; um chute não seria.
    expect(nomeDeCategoria("Categoria Que Nao Existe")).toBe("Categoria Que Nao Existe");
  });

  it("sem sugestão, sem categoria — não inventa", () => {
    expect(nomeDeCategoria(null)).toBeNull();
    expect(nomeDeCategoria(undefined)).toBeNull();
    expect(nomeDeCategoria("")).toBeNull();
    expect(nomeDeCategoria("   ")).toBeNull();
  });

  it("é estável: traduzir o que já está traduzido não muda", () => {
    // Importa porque o reparo dos dados existentes reaplica a função sobre
    // nomes que podem já ter passado por ela.
    expect(nomeDeCategoria(nomeDeCategoria("Groceries"))).toBe("Mercado");
  });
});
