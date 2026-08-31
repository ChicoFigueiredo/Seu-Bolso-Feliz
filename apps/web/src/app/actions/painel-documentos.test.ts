/**
 * As regras de tradução da caixa de entrada de documentos.
 *
 * O que está sob teste não é formatação: é a ordem das perguntas. Um documento
 * que falhou **e** tem rascunho pendente é, antes de tudo, um documento que
 * falhou — se a ordem inverter, a pessoa vai aprovar um lançamento tirado de um
 * arquivo que o sistema nunca conseguiu ler.
 *
 * Nenhum destes casos toca o banco: `diagnosticarDocumento` é puro de
 * propósito, e é isso que permite fixar as regras aqui em vez de descobri-las
 * clicando na tela.
 */
import { describe, expect, it } from "vitest";
import {
  diagnosticarDocumento,
  faixaDeConfianca,
  formatarTamanho,
  pareceProblemaDeSenha,
  rotuloDaOrigem,
  type SinaisDoDocumento,
} from "@/components/documentos/estado-do-documento";

const BASE: SinaisDoDocumento = {
  statusDoDocumento: "queued",
  statusDoJob: "queued",
  mensagemDeErro: null,
  precisaDeIaCompleta: false,
  rascunhosPendentes: 0,
  avisoDeSenha: false,
};

describe("diagnosticarDocumento", () => {
  it("PDF protegido vence qualquer outro sinal e manda cadastrar a senha", () => {
    const d = diagnosticarDocumento({
      ...BASE,
      statusDoDocumento: "pending_review",
      rascunhosPendentes: 3,
      avisoDeSenha: true,
    });

    expect(d.estado).toBe("protegido-por-senha");
    expect(d.balde).toBe("erro");
    expect(d.rotaDoQueFazer).toBe("/dashboard/settings/passwords");
    // A promessa que o CEO exige em toda falha: o original não se perde.
    expect(d.oQueFazer).toMatch(/original continua guardado/i);
  });

  it("reconhece a senha pela mensagem de erro quando não houve aviso no diário", () => {
    const d = diagnosticarDocumento({
      ...BASE,
      statusDoDocumento: "failed",
      statusDoJob: "failed",
      mensagemDeErro: "PDF protegido por senha. Senha necessária para extração.",
    });

    expect(d.estado).toBe("protegido-por-senha");
  });

  it("falha vence rascunho pendente — nunca se aprova o que não foi lido", () => {
    const d = diagnosticarDocumento({
      ...BASE,
      statusDoDocumento: "failed",
      statusDoJob: "failed",
      mensagemDeErro: "Failed to download for parsing: connection reset",
      rascunhosPendentes: 2,
    });

    expect(d.estado).toBe("nao-reconhecido");
    expect(d.balde).toBe("erro");
    expect(d.porQue).toContain("connection reset");
    expect(d.oQueFazer).toMatch(/continua guardado/i);
  });

  it("corta mensagem de erro na primeira linha, sem despejar stack na tela", () => {
    const d = diagnosticarDocumento({
      ...BASE,
      statusDoDocumento: "failed",
      statusDoJob: "failed",
      mensagemDeErro: "Erro de verdade\n    at parseDocument (parse-orchestrator.ts:250)",
    });

    expect(d.porQue).toBe("A leitura parou: Erro de verdade");
  });

  it("conta os rascunhos pendentes em português, no singular e no plural", () => {
    expect(diagnosticarDocumento({ ...BASE, rascunhosPendentes: 1 }).porQue).toBe(
      "1 lançamento esperando sua decisão.",
    );
    expect(diagnosticarDocumento({ ...BASE, rascunhosPendentes: 4 }).porQue).toBe(
      "4 lançamentos esperando sua decisão.",
    );
    expect(diagnosticarDocumento({ ...BASE, rascunhosPendentes: 1 }).balde).toBe("revisar");
  });

  it("documento em revisão sem nada proposto explica o vazio em vez de sumir com ele", () => {
    const d = diagnosticarDocumento({ ...BASE, statusDoDocumento: "pending_review" });

    expect(d.estado).toBe("precisa-revisar");
    expect(d.porQue).toMatch(/sem nenhum lançamento proposto/i);
    expect(d.oQueFazer).toMatch(/IA|à mão/i);
  });

  it("quando o próprio pipeline pediu IA completa, a frase diz que a confiança foi baixa", () => {
    const d = diagnosticarDocumento({
      ...BASE,
      statusDoDocumento: "pending_review",
      precisaDeIaCompleta: true,
    });

    expect(d.porQue).toMatch(/confiança/i);
  });

  it("processado e aprovado são concluídos, e não têm nada a explicar", () => {
    for (const status of ["processed", "approved"]) {
      const d = diagnosticarDocumento({ ...BASE, statusDoDocumento: status });
      expect(d.estado).toBe("pronto");
      expect(d.balde).toBe("concluidos");
      expect(d.porQue).toBeNull();
      expect(d.oQueFazer).toBeNull();
    }
  });

  it("qualquer status do meio do caminho vira 'processando', sem vazar jargão", () => {
    for (const status of ["new", "queued", "processing", "hashed", "ai_lite_enriching"]) {
      const d = diagnosticarDocumento({ ...BASE, statusDoDocumento: status, statusDoJob: status });
      expect(d.estado).toBe("processando");
      expect(d.balde).toBe("processando");
      expect(d.porQue).not.toContain(status);
    }
  });
});

describe("pareceProblemaDeSenha", () => {
  it("casa as frases que o pipeline realmente escreve", () => {
    expect(pareceProblemaDeSenha("Sem senha encontrada para PDF protegido")).toBe(true);
    expect(pareceProblemaDeSenha("Nenhuma das 3 senha(s) cadastrada(s) abriu o documento")).toBe(
      true,
    );
    expect(pareceProblemaDeSenha("PasswordException: Incorrect Password")).toBe(true);
  });

  it("não confunde falha comum com senha", () => {
    expect(pareceProblemaDeSenha("Texto vazio após extração")).toBe(false);
    expect(pareceProblemaDeSenha(null)).toBe(false);
  });
});

describe("rotuloDaOrigem", () => {
  it("mostra o endereço do Gmail sem o nome de exibição", () => {
    expect(rotuloDaOrigem("gmail", "Cemig <fatura@cemig.com.br>")).toBe(
      "Gmail · fatura@cemig.com.br",
    );
  });

  it("traduz as demais origens sem exigir remetente", () => {
    expect(rotuloDaOrigem("manual_upload")).toBe("Enviado por você");
    expect(rotuloDaOrigem("local_file")).toBe("Pasta local");
  });
});

describe("faixaDeConfianca", () => {
  it("usa os mesmos cortes do pipeline e não inventa confiança quando não há número", () => {
    expect(faixaDeConfianca(0.95)).toBe("alta");
    expect(faixaDeConfianca(0.8)).toBe("alta");
    expect(faixaDeConfianca(0.5)).toBe("provavel");
    expect(faixaDeConfianca(0.49)).toBe("baixa");
    expect(faixaDeConfianca(null)).toBeNull();
    expect(faixaDeConfianca(Number.NaN)).toBeNull();
  });
});

describe("formatarTamanho", () => {
  it("omite o tamanho em vez de exibir '0 B'", () => {
    expect(formatarTamanho(0)).toBeNull();
    expect(formatarTamanho(null)).toBeNull();
  });

  it("sobe de unidade nos limites certos", () => {
    expect(formatarTamanho(900)).toBe("900 B");
    expect(formatarTamanho(2048)).toBe("2 KB");
    expect(formatarTamanho(3 * 1024 * 1024)).toBe("3.0 MB");
  });
});
