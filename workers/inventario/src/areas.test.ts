import { describe, expect, it } from "vitest";
import { type Classe, classificarPasta, nomeFinanceiro, tratamentoDoArquivo } from "./areas";

/** Classifica um caminho pasta a pasta, como o scanner faz descendo a árvore. */
function classeDe(caminho: string): Classe {
  const partes = caminho.split("/").filter(Boolean);
  let classe: Classe | undefined;
  for (let i = 2; i <= partes.length; i++) {
    classe = classificarPasta(`/${partes.slice(0, i).join("/")}`, classe);
  }
  return classe!;
}

describe("classificarPasta", () => {
  it("áreas pessoais são A, inclusive subpastas", () => {
    expect(classeDe("/mnt/p/OneDrive/Pessoal/Contas/2026/2026.05").camada).toBe("A");
    expect(classeDe("/mnt/e2/Pessoal/Chico/Celular/Xiomi").camada).toBe("A");
  });

  it("o resto do disco é B", () => {
    expect(classeDe("/mnt/p/OneDrive/Familia").camada).toBe("B");
    expect(classeDe("/mnt/p/Edulingo/notas").camada).toBe("B");
  });

  it("C guarda o motivo e a raiz da área", () => {
    expect(classeDe("/mnt/e2/Livros_Catalogação/!!Livros/Telegram Desktop")).toMatchObject({
      camada: "C",
      motivo: "livros",
      raiz: "/mnt/e2/Livros_Catalogação",
    });
  });

  it("caminho exato vence a herança: _classifica volta a B dentro do disco Estudo", () => {
    expect(classeDe("/mnt/e/Ingles").camada).toBe("C");
    expect(classeDe("/mnt/e/_classifica/Telegram Desktop").camada).toBe("B");
  });

  it("pasta de outra pessoa é C mesmo dentro de área pessoal", () => {
    expect(classeDe("/mnt/bak/Pessoal/Daiane/Contas")).toMatchObject({
      camada: "C",
      motivo: "outra-pessoa",
    });
  });

  it("Daiane dentro do seu IR é dependente na sua declaração: continua A", () => {
    expect(classeDe("/mnt/p/OneDrive/Pessoal/IR/2025-2024/Comprovantes/Daiane").camada).toBe("A");
    expect(classeDe("/mnt/e2/Pessoal/Chico/Financeiro/ImpostoRenda/2014/Daiane").camada).toBe("A");
    expect(classeDe("/mnt/p/OneDrive/Pessoal/Daiane").camada).toBe("C");
  });

  it("Mayara é A em qualquer lugar, menos dentro de C", () => {
    expect(classeDe("/mnt/p2/Diversos/Mayara").camada).toBe("A");
    expect(classeDe("/mnt/p/CAIXA/Mayara").camada).toBe("C");
  });

  it("node_modules e afins são C em qualquer lugar", () => {
    expect(classeDe("/mnt/e2/Pessoal/Chico/Sites/x/node_modules")).toMatchObject({
      camada: "C",
      motivo: "sistema",
    });
  });

  it("o cofre é ignorado, nem contado", () => {
    expect(classeDe("/mnt/d/Chico/financas/acervo").ignorar).toBe(true);
  });
});

describe("tratamentoDoArquivo", () => {
  const A = classeDe("/mnt/p/OneDrive/Pessoal");
  const B = classeDe("/mnt/p/OneDrive/Familia");
  const C = classeDe("/mnt/p/CAIXA/GEFAT");

  it("em A, documento é copiado e foto só hasheada", () => {
    expect(tratamentoDoArquivo("fatura.pdf", A)).toBe("copiar");
    expect(tratamentoDoArquivo("export.json", A)).toBe("copiar");
    expect(tratamentoDoArquivo("IMG-20260501-WA0001.jpg", A)).toBe("hash");
    expect(tratamentoDoArquivo("setup.exe", A)).toBe("ignorar");
  });

  it("em B, documento é hasheado e foto contada, salvo nome financeiro", () => {
    expect(tratamentoDoArquivo("planilha.xlsx", B)).toBe("hash");
    expect(tratamentoDoArquivo("praia.jpg", B)).toBe("foto-contada");
    expect(tratamentoDoArquivo("comprovante pix.jpg", B)).toBe("hash");
    expect(tratamentoDoArquivo("notas.txt", B)).toBe("ignorar");
  });

  it("em C, só nome financeiro é resgatado", () => {
    expect(tratamentoDoArquivo("Contracheque 2025-10.pdf", C)).toBe("hash");
    expect(tratamentoDoArquivo("Informe de Rendimentos 2025.pdf", C)).toBe("hash");
    expect(tratamentoDoArquivo("relatorio-gefat.pdf", C)).toBe("contar");
  });
});

describe("nomeFinanceiro", () => {
  it("ignora acento e caixa", () => {
    expect(nomeFinanceiro("CONDOMÍNIO-2013.10.jpg")).toBe(true);
    expect(nomeFinanceiro("Pensão Mayara.pdf")).toBe(true);
    expect(nomeFinanceiro("DANFE_123.pdf")).toBe(true);
  });

  it("não dispara em palavras comuns", () => {
    expect(nomeFinanceiro("pixel-art.png")).toBe(false);
    expect(nomeFinanceiro("caixa-de-ferramentas.pdf")).toBe(false);
  });
});
