import { describe, expect, it } from "vitest";
import {
  FILTROS_PADRAO,
  TAMANHO_PAGINA,
  agruparPorDia,
  contarFiltrosDetalhados,
  filtrosParaBusca,
  lerFiltrosDaBusca,
  metaDaMovimentacao,
  rotuloDoDia,
  temFiltroAtivo,
  valorComSinal,
} from "./filtros";

describe("lerFiltrosDaBusca", () => {
  it("sem parâmetros, devolve o padrão", () => {
    expect(lerFiltrosDaBusca({})).toEqual(FILTROS_PADRAO);
  });

  it("aceita os parâmetros próprios da tela", () => {
    const filtros = lerFiltrosDaBusca({
      periodo: "este-ano",
      tipo: "saidas",
      situacao: "vencidas",
      busca: "  energia  ",
      categoria: "cat-1",
      tag: "tag-1",
      centro: "centro-1",
      conta: "conta-1",
    });

    expect(filtros).toEqual({
      periodo: "este-ano",
      tipo: "saidas",
      situacao: "vencidas",
      busca: "energia",
      categoriaId: "cat-1",
      tagId: "tag-1",
      centroId: "centro-1",
      produtoId: "conta-1",
      limite: TAMANHO_PAGINA,
    });
  });

  it("valor desconhecido cai no padrão em vez de quebrar a tela", () => {
    const filtros = lerFiltrosDaBusca({ periodo: "ontem-a-noite", tipo: "🙂", situacao: "" });
    expect(filtros.periodo).toBe(FILTROS_PADRAO.periodo);
    expect(filtros.tipo).toBe(FILTROS_PADRAO.tipo);
    expect(filtros.situacao).toBe(FILTROS_PADRAO.situacao);
  });

  it("usa o primeiro valor quando o parâmetro vem repetido", () => {
    expect(lerFiltrosDaBusca({ tipo: ["entradas", "saidas"] }).tipo).toBe("entradas");
  });

  // A tela Hoje publica esses links nas pendências; quebrá-los transforma a
  // pendência num beco sem saída.
  it.each([
    ["vencidas", "vencidas"],
    ["sem-categoria", "sem-categoria"],
    ["sem-centro", "nao-alocadas"],
    ["nao-confirmadas", "nao-confirmadas"],
  ])("honra o link ?filtro=%s vindo da tela Hoje", (legado, esperado) => {
    const filtros = lerFiltrosDaBusca({ filtro: legado });
    expect(filtros.situacao).toBe(esperado);
    // A contagem da tela Hoje não tem recorte de período: limitar ao período
    // atual mostraria menos linhas do que o número prometido lá.
    expect(filtros.periodo).toBe("tudo");
  });

  it("`situacao` explícita ganha do parâmetro legado", () => {
    expect(lerFiltrosDaBusca({ filtro: "vencidas", situacao: "sem-categoria" }).situacao).toBe(
      "sem-categoria",
    );
  });

  it("período explícito ganha do padrão 'tudo' do link legado", () => {
    expect(lerFiltrosDaBusca({ filtro: "vencidas", periodo: "90-dias" }).periodo).toBe("90-dias");
  });
});

describe("filtrosParaBusca", () => {
  it("não escreve na URL o que é padrão", () => {
    expect(filtrosParaBusca(FILTROS_PADRAO)).toBe("");
  });

  it("faz ida e volta com lerFiltrosDaBusca", () => {
    const filtros = {
      ...FILTROS_PADRAO,
      periodo: "90-dias" as const,
      tipo: "cartao" as const,
      situacao: "nao-alocadas" as const,
      busca: "mercado",
      categoriaId: "cat-1",
      tagId: "tag-1",
      centroId: "centro-1",
      produtoId: "conta-1",
      limite: 90,
    };

    const params = Object.fromEntries(new URLSearchParams(filtrosParaBusca(filtros)));
    // `limite` não descreve o recorte, então não volta da URL.
    expect(lerFiltrosDaBusca(params)).toEqual({ ...filtros, limite: TAMANHO_PAGINA });
  });

  it("normaliza o parâmetro legado da tela Hoje para `situacao`", () => {
    const params = filtrosParaBusca(lerFiltrosDaBusca({ filtro: "sem-centro" }));
    expect(params).toContain("situacao=nao-alocadas");
    expect(params).toContain("periodo=tudo");
    expect(params).not.toContain("filtro=");
  });
});

describe("contarFiltrosDetalhados e temFiltroAtivo", () => {
  it("conta só os filtros que ficam escondidos na folha", () => {
    expect(contarFiltrosDetalhados(FILTROS_PADRAO)).toBe(0);
    // Período e tipo estão à vista em pílulas: não precisam de contador.
    expect(contarFiltrosDetalhados({ ...FILTROS_PADRAO, tipo: "saidas" })).toBe(0);
    expect(
      contarFiltrosDetalhados({
        ...FILTROS_PADRAO,
        situacao: "vencidas",
        categoriaId: "cat-1",
        tagId: "tag-1",
      }),
    ).toBe(3);
  });

  it("reconhece qualquer desvio do padrão, inclusive busca só com espaços", () => {
    expect(temFiltroAtivo(FILTROS_PADRAO)).toBe(false);
    expect(temFiltroAtivo({ ...FILTROS_PADRAO, busca: "   " })).toBe(false);
    expect(temFiltroAtivo({ ...FILTROS_PADRAO, busca: "luz" })).toBe(true);
    expect(temFiltroAtivo({ ...FILTROS_PADRAO, tipo: "cartao" })).toBe(true);
    expect(temFiltroAtivo({ ...FILTROS_PADRAO, periodo: "tudo" })).toBe(true);
  });

  it("ignora o limite: carregar mais linhas não é filtrar", () => {
    expect(temFiltroAtivo({ ...FILTROS_PADRAO, limite: 300 })).toBe(false);
  });
});

describe("rotuloDoDia", () => {
  const hoje = "2026-08-30";

  it("nomeia os dias próximos em vez de mostrar a data", () => {
    expect(rotuloDoDia("2026-08-30", hoje)).toBe("Hoje");
    expect(rotuloDoDia("2026-08-29", hoje)).toBe("Ontem");
    expect(rotuloDoDia("2026-08-31", hoje)).toBe("Amanhã");
  });

  it("usa dia da semana e data curta no resto do ano corrente", () => {
    expect(rotuloDoDia("2026-08-28", hoje)).toBe("Sex, 28/08");
  });

  it("só mostra o ano quando é outro", () => {
    expect(rotuloDoDia("2025-12-31", hoje)).toBe("Qua, 31/12/2025");
  });

  it("não desloca o dia por causa de fuso", () => {
    // A data chega como `date` puro do Postgres; ler em UTC empurraria o dia
    // para trás em fuso positivo.
    expect(rotuloDoDia("2026-01-01", "2026-01-02")).toBe("Ontem");
  });
});

describe("agruparPorDia", () => {
  const item = (id: string, data: string) => ({ id, data });

  it("agrupa dias consecutivos preservando a ordem recebida", () => {
    const grupos = agruparPorDia(
      [
        item("a", "2026-08-30"),
        item("b", "2026-08-30"),
        item("c", "2026-08-28"),
        item("d", "2026-08-27"),
      ],
      "2026-08-30",
    );

    expect(grupos.map((grupo) => grupo.data)).toEqual(["2026-08-30", "2026-08-28", "2026-08-27"]);
    expect(grupos[0]?.rotulo).toBe("Hoje");
    expect(grupos[0]?.itens.map((i) => i.id)).toEqual(["a", "b"]);
    expect(grupos[1]?.itens).toHaveLength(1);
  });

  it("não reordena: uma data fora de ordem vira grupo separado, à vista", () => {
    const grupos = agruparPorDia(
      [item("a", "2026-08-30"), item("b", "2026-08-28"), item("c", "2026-08-30")],
      "2026-08-30",
    );
    expect(grupos).toHaveLength(3);
  });

  it("lista vazia não vira grupo vazio", () => {
    expect(agruparPorDia([], "2026-08-30")).toEqual([]);
  });
});

describe("valorComSinal", () => {
  it("marca entrada e saída, e deixa ajuste sem sinal", () => {
    expect(valorComSinal("entrada", "R$ 100,00")).toBe("+ R$ 100,00");
    expect(valorComSinal("saida", "R$ 100,00")).toBe("− R$ 100,00");
    expect(valorComSinal("neutra", "R$ 100,00")).toBe("R$ 100,00");
  });
});

describe("metaDaMovimentacao", () => {
  it("diz por escrito quando a compra foi no cartão", () => {
    expect(
      metaDaMovimentacao({ produto: "Nubank", noCartao: true, categoria: "Alimentação" }),
    ).toBe("No cartão Nubank · Alimentação");
  });

  it("saída de conta não ganha o rótulo de cartão", () => {
    expect(
      metaDaMovimentacao({ produto: "Conta Itaú", noCartao: false, categoria: "Energia" }),
    ).toBe("Conta Itaú · Energia");
  });

  it("categoria ausente aparece como pendência, não como vazio", () => {
    expect(metaDaMovimentacao({ produto: "Conta Itaú", noCartao: false, categoria: null })).toBe(
      "Conta Itaú · Sem categoria",
    );
  });

  it("sem produto, ainda diz se foi no cartão", () => {
    expect(metaDaMovimentacao({ produto: null, noCartao: true, categoria: null })).toBe(
      "No cartão · Sem categoria",
    );
    expect(metaDaMovimentacao({ produto: null, noCartao: false, categoria: "Lazer" })).toBe(
      "Lazer",
    );
  });
});
