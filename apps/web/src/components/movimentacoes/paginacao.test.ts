import { describe, it, expect } from "vitest";
import { LIMITE_MAXIMO, decidirPaginacao, limiteDaConsulta } from "./paginacao";
import { TAMANHO_PAGINA } from "./filtros";

describe("limiteDaConsulta", () => {
  it("nunca traz menos que uma página, mesmo pedindo zero", () => {
    expect(limiteDaConsulta(0, TAMANHO_PAGINA)).toBe(TAMANHO_PAGINA);
  });

  it("respeita o pedido enquanto ele couber no teto", () => {
    expect(limiteDaConsulta(2900, TAMANHO_PAGINA)).toBe(2900);
  });

  it("corta no teto quando o pedido passa", () => {
    expect(limiteDaConsulta(999_999, TAMANHO_PAGINA)).toBe(LIMITE_MAXIMO);
  });
});

describe("decidirPaginacao", () => {
  it("oferece carregar mais enquanto falta e cabe", () => {
    expect(decidirPaginacao(2858, 100)).toEqual({ temMais: true, atingiuTeto: false });
  });

  it("para de oferecer quando tudo já está na tela", () => {
    expect(decidirPaginacao(2858, 2858)).toEqual({ temMais: false, atingiuTeto: false });
  });

  it("no teto, troca o botão pela explicação — nunca deixa clique morto", () => {
    // A regressão que isto trava: com o teto antigo de 600 e 2858 no total,
    // `temMais` continuava true para sempre e o botão não fazia nada. Aqui o
    // total PASSA do teto, que é a única situação em que ele aperta.
    expect(decidirPaginacao(LIMITE_MAXIMO + 1200, LIMITE_MAXIMO)).toEqual({
      temMais: false,
      atingiuTeto: true,
    });
  });

  it("teto atingido mas sem nada faltando não vira aviso", () => {
    expect(decidirPaginacao(LIMITE_MAXIMO, LIMITE_MAXIMO)).toEqual({
      temMais: false,
      atingiuTeto: false,
    });
  });

  it("as 2858 da conta real são alcançáveis clicando", () => {
    // Simula o "Carregar mais" até o fim: precisa terminar, e sem passar do
    // teto. É a prova de que "ver todos os registros" é possível de verdade.
    let carregadas = TAMANHO_PAGINA;
    let cliques = 0;
    while (decidirPaginacao(2858, carregadas).temMais && cliques < 1000) {
      carregadas = Math.min(limiteDaConsulta(carregadas + TAMANHO_PAGINA, TAMANHO_PAGINA), 2858);
      cliques++;
    }
    expect(carregadas).toBe(2858);
    expect(cliques).toBeLessThan(30);
  });
});
