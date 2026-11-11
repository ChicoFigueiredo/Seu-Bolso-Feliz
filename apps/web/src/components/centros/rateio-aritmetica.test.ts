import { describe, expect, it } from "vitest";
import {
  formatarCentavos,
  paraAlocacoes,
  paraCentavos,
  reaisParaCentavos,
  resumirRateio,
  type LinhaRateio,
} from "./rateio-aritmetica";

/**
 * O formulário de rateio é onde um centavo perdido vira dinheiro errado no
 * relatório de um projeto. Estes testes existem para travar exatamente isso:
 * a leitura do que o usuário digita, a soma das linhas e a decisão de deixar
 * (ou não) salvar.
 */

const linha = (centroId: string, valorTexto: string): LinhaRateio => ({ centroId, valorTexto });

const CENTRO_A = "11111111-1111-4111-8111-111111111111";
const CENTRO_B = "22222222-2222-4222-8222-222222222222";
const CENTRO_C = "33333333-3333-4333-8333-333333333333";

describe("paraCentavos", () => {
  it("lê o formato que o brasileiro digita", () => {
    expect(paraCentavos("1.234,56")).toBe(123456);
    expect(paraCentavos("R$ 1.234,56")).toBe(123456);
    expect(paraCentavos("1234,56")).toBe(123456);
    expect(paraCentavos("  89,90 ")).toBe(8990);
  });

  it("trata ponto como milhar só quando os grupos têm três dígitos", () => {
    expect(paraCentavos("1.234")).toBe(123400);
    expect(paraCentavos("1.234.567")).toBe(123456700);
    // Um grupo que não tem três dígitos não é milhar: é o teclado numérico
    // do celular, que só oferece ponto.
    expect(paraCentavos("12.5")).toBe(1250);
    expect(paraCentavos("12.34")).toBe(1234);
  });

  it("arredonda meio centavo para longe do zero, sem erro de ponto flutuante", () => {
    // 0.29 * 100 vale 28.999999999999996 em IEEE-754. Um Math.round ingênuo
    // sobre o produto cru devolveria 28 em alguns valores desta família.
    expect(paraCentavos("0,29")).toBe(29);
    expect(paraCentavos("8,165")).toBe(817);
    expect(paraCentavos("2,675")).toBe(268);
    expect(paraCentavos("1,005")).toBe(101);
  });

  it("devolve nulo para o que não é número", () => {
    expect(paraCentavos("")).toBeNull();
    expect(paraCentavos("   ")).toBeNull();
    expect(paraCentavos("abc")).toBeNull();
    expect(paraCentavos("R$")).toBeNull();
    expect(paraCentavos("1,2,3")).toBeNull();
  });

  it("não acumula erro ao somar cem vezes o mesmo valor quebrado", () => {
    let total = 0;
    for (let i = 0; i < 100; i += 1) total += paraCentavos("0,29") ?? 0;
    expect(total).toBe(2900);
  });
});

describe("reaisParaCentavos", () => {
  it("converte o valor que veio do banco com o mesmo arredondamento do texto", () => {
    expect(reaisParaCentavos(1234.56)).toBe(123456);
    expect(reaisParaCentavos(0.29)).toBe(29);
    expect(reaisParaCentavos(8.165)).toBe(817);
    expect(reaisParaCentavos(0)).toBe(0);
    expect(reaisParaCentavos(-100)).toBe(-10000);
  });

  it("concorda com `paraCentavos`: o valor gravado volta para o campo e é lido igual", () => {
    // É o caminho real: banco → campo → banco. Se as duas conversões
    // divergissem, abrir e salvar de novo um rateio já correto mudaria o
    // valor sem ninguém ter digitado nada.
    for (const valor of [0.29, 1.005, 8.165, 1234.56, 99.99, 0.01]) {
      const centavos = reaisParaCentavos(valor);
      expect(paraCentavos(formatarCentavos(centavos))).toBe(centavos);
    }
  });
});

describe("formatarCentavos", () => {
  it("volta para o texto do campo, sempre com dois decimais", () => {
    expect(formatarCentavos(123456)).toBe("1234,56");
    expect(formatarCentavos(29)).toBe("0,29");
    expect(formatarCentavos(100)).toBe("1,00");
    expect(formatarCentavos(0)).toBe("0,00");
  });

  it("faz ida e volta sem perder centavo", () => {
    for (const centavos of [1, 29, 100, 817, 123456, 999999999]) {
      expect(paraCentavos(formatarCentavos(centavos))).toBe(centavos);
    }
  });
});

describe("resumirRateio — soma exata", () => {
  it("fecha em zero não alocado e libera o salvamento", () => {
    const resumo = resumirRateio(100, [linha(CENTRO_A, "60,00"), linha(CENTRO_B, "40,00")]);

    expect(resumo.totalCentavos).toBe(10000);
    expect(resumo.alocadoCentavos).toBe(10000);
    expect(resumo.naoAlocadoCentavos).toBe(0);
    expect(resumo.excedeu).toBe(false);
    expect(resumo.podeSalvar).toBe(true);
  });

  it("usa o módulo do valor: saída gravada com sinal negativo rateia igual", () => {
    const resumo = resumirRateio(-100, [linha(CENTRO_A, "100,00")]);
    expect(resumo.totalCentavos).toBe(10000);
    expect(resumo.naoAlocadoCentavos).toBe(0);
    expect(resumo.podeSalvar).toBe(true);
  });
});

describe("resumirRateio — soma menor que a movimentação", () => {
  it("deixa o resto como não alocado, que é estado legítimo", () => {
    const resumo = resumirRateio(100, [linha(CENTRO_A, "30,00")]);

    expect(resumo.alocadoCentavos).toBe(3000);
    expect(resumo.naoAlocadoCentavos).toBe(7000);
    expect(resumo.excedeu).toBe(false);
    expect(resumo.podeSalvar).toBe(true);
  });

  it("lista vazia é válida: significa apagar todos os rateios", () => {
    const resumo = resumirRateio(100, []);
    expect(resumo.alocadoCentavos).toBe(0);
    expect(resumo.naoAlocadoCentavos).toBe(10000);
    expect(resumo.podeSalvar).toBe(true);
  });
});

describe("resumirRateio — soma excedendo", () => {
  it("bloqueia o salvamento e mostra quanto passou", () => {
    const resumo = resumirRateio(100, [linha(CENTRO_A, "60,00"), linha(CENTRO_B, "50,00")]);

    expect(resumo.alocadoCentavos).toBe(11000);
    expect(resumo.naoAlocadoCentavos).toBe(-1000);
    expect(resumo.excedeu).toBe(true);
    expect(resumo.podeSalvar).toBe(false);
  });

  it("um único centavo a mais já é excesso", () => {
    const resumo = resumirRateio(10, [linha(CENTRO_A, "10,01")]);
    expect(resumo.excedeu).toBe(true);
    expect(resumo.podeSalvar).toBe(false);
  });
});

describe("resumirRateio — divisão igualitária com resto", () => {
  it("aceita a divisão de R$ 1,00 entre três centros sem sobrar centavo", () => {
    // É exatamente o que `splitEvenly` (@sbf/domain) grava: o resto do
    // centavo vai inteiro para o primeiro centro. Se o formulário
    // discordasse dessa regra, ele acusaria excesso — ou mostraria
    // R$ 0,01 não alocado — num rateio que o servidor considera perfeito.
    const resumo = resumirRateio(1, [
      linha(CENTRO_A, "0,34"),
      linha(CENTRO_B, "0,33"),
      linha(CENTRO_C, "0,33"),
    ]);

    expect(resumo.alocadoCentavos).toBe(100);
    expect(resumo.naoAlocadoCentavos).toBe(0);
    expect(resumo.excedeu).toBe(false);
    expect(resumo.podeSalvar).toBe(true);
  });

  it("aceita R$ 100,00 entre três centros — 33,34 / 33,33 / 33,33", () => {
    const resumo = resumirRateio(100, [
      linha(CENTRO_A, "33,34"),
      linha(CENTRO_B, "33,33"),
      linha(CENTRO_C, "33,33"),
    ]);

    expect(resumo.alocadoCentavos).toBe(10000);
    expect(resumo.naoAlocadoCentavos).toBe(0);
    expect(resumo.podeSalvar).toBe(true);
  });
});

describe("resumirRateio — valor com centavos quebrados", () => {
  it("não perde centavo somando três valores quebrados", () => {
    const resumo = resumirRateio(0.87, [
      linha(CENTRO_A, "0,29"),
      linha(CENTRO_B, "0,29"),
      linha(CENTRO_C, "0,29"),
    ]);

    expect(resumo.totalCentavos).toBe(87);
    expect(resumo.alocadoCentavos).toBe(87);
    expect(resumo.naoAlocadoCentavos).toBe(0);
    expect(resumo.podeSalvar).toBe(true);
  });

  it("uma movimentação de R$ 8,165 vale 817 centavos para o rateio", () => {
    const resumo = resumirRateio(8.165, [linha(CENTRO_A, "8,17")]);
    expect(resumo.totalCentavos).toBe(817);
    expect(resumo.excedeu).toBe(false);
    expect(resumo.podeSalvar).toBe(true);
  });
});

describe("resumirRateio — problemas por linha", () => {
  it("acusa centro repetido", () => {
    const resumo = resumirRateio(100, [linha(CENTRO_A, "30,00"), linha(CENTRO_A, "20,00")]);

    expect(resumo.problemas[1]).toBe("centro_repetido");
    expect(resumo.podeSalvar).toBe(false);
    // A linha repetida não é somada duas vezes.
    expect(resumo.alocadoCentavos).toBe(3000);
  });

  it("acusa valor sem centro escolhido", () => {
    const resumo = resumirRateio(100, [linha("", "30,00")]);
    expect(resumo.problemas[0]).toBe("sem_centro");
    expect(resumo.podeSalvar).toBe(false);
  });

  it("acusa valor não positivo em linha que já tem centro", () => {
    expect(resumirRateio(100, [linha(CENTRO_A, "0,00")]).problemas[0]).toBe("valor_nao_positivo");
    expect(resumirRateio(100, [linha(CENTRO_A, "-5,00")]).problemas[0]).toBe("valor_nao_positivo");
    expect(resumirRateio(100, [linha(CENTRO_A, "")]).problemas[0]).toBe("valor_nao_positivo");
  });

  it("acusa texto que não é número", () => {
    expect(resumirRateio(100, [linha(CENTRO_A, "trinta")]).problemas[0]).toBe("valor_invalido");
  });

  it("ignora linha totalmente em branco", () => {
    const resumo = resumirRateio(100, [linha(CENTRO_A, "30,00"), linha("", "")]);
    expect(resumo.problemas[1]).toBeNull();
    expect(resumo.podeSalvar).toBe(true);
    expect(resumo.linhasConsideradas).toBe(1);
  });
});

describe("paraAlocacoes", () => {
  it("entrega reais exatos derivados dos centavos, prontos para a Server Action", () => {
    expect(paraAlocacoes([linha(CENTRO_A, "1.234,56"), linha(CENTRO_B, "0,29")])).toEqual([
      { financialCenterId: CENTRO_A, amount: 1234.56 },
      { financialCenterId: CENTRO_B, amount: 0.29 },
    ]);
  });

  it("descarta linha em branco e linha sem centro", () => {
    expect(paraAlocacoes([linha("", ""), linha("", "10,00"), linha(CENTRO_A, "5,00")])).toEqual([
      { financialCenterId: CENTRO_A, amount: 5 },
    ]);
  });
});
