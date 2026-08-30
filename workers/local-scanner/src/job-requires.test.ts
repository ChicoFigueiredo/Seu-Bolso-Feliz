import { describe, it, expect } from "vitest";
import {
  CAPACIDADE_OCR,
  CAPACIDADE_PASTA_LOCAL,
  CAPACIDADE_SENHA_PDF,
  capacidadesNaoAtendidas,
  pdfProtegidoPorSenha,
  resolveFileRequires,
} from "./job-requires";

const encoder = new TextEncoder();

/** PDF de mentira com trailer no fim, como o de verdade. */
function pdfFalso({ cifrado, recheioBytes = 0 }: { cifrado: boolean; recheioBytes?: number }) {
  const cabecalho = "%PDF-1.7\n";
  const recheio = "a".repeat(recheioBytes);
  const trailer = cifrado
    ? "trailer\n<< /Size 10 /Root 1 0 R /Encrypt 9 0 R >>\nstartxref\n123\n%%EOF"
    : "trailer\n<< /Size 10 /Root 1 0 R >>\nstartxref\n123\n%%EOF";
  return encoder.encode(cabecalho + recheio + trailer);
}

describe("pdfProtegidoPorSenha", () => {
  it("acha o /Encrypt no trailer", () => {
    expect(pdfProtegidoPorSenha(pdfFalso({ cifrado: true }))).toBe(true);
  });

  it("não acusa PDF sem /Encrypt", () => {
    expect(pdfProtegidoPorSenha(pdfFalso({ cifrado: false }))).toBe(false);
  });

  it("acha o /Encrypt mesmo com muito conteúdo antes do trailer", () => {
    expect(pdfProtegidoPorSenha(pdfFalso({ cifrado: true, recheioBytes: 200_000 }))).toBe(true);
  });

  it("ignora /Encrypt que esteja longe do fim do arquivo", () => {
    // Só a cauda é inspecionada de propósito: a sequência dentro de um stream
    // comprimido no meio do documento não diz nada sobre cifragem, e aceitá-la
    // tornaria o falso positivo comum em vez de raro.
    const conteudo = encoder.encode(
      "%PDF-1.7\n/Encrypt disfarçado dentro de um stream\n" +
        "b".repeat(50_000) +
        "trailer\n<< /Size 10 >>\nstartxref\n1\n%%EOF",
    );
    expect(pdfProtegidoPorSenha(conteudo)).toBe(false);
  });

  it("não quebra com arquivo vazio ou menor que a agulha", () => {
    expect(pdfProtegidoPorSenha(new Uint8Array(0))).toBe(false);
    expect(pdfProtegidoPorSenha(encoder.encode("/Enc"))).toBe(false);
  });
});

describe("resolveFileRequires", () => {
  it("todo arquivo da pasta local exige pasta-local", () => {
    expect(resolveFileRequires("extrato.csv", encoder.encode("a;b;c"))).toEqual([
      CAPACIDADE_PASTA_LOCAL,
    ]);
  });

  it("PDF protegido exige ocr e senha-pdf", () => {
    expect(resolveFileRequires("fatura.pdf", pdfFalso({ cifrado: true }))).toEqual([
      CAPACIDADE_PASTA_LOCAL,
      CAPACIDADE_OCR,
      CAPACIDADE_SENHA_PDF,
    ]);
  });

  it("PDF aberto exige ocr mas não senha-pdf", () => {
    expect(resolveFileRequires("fatura.pdf", pdfFalso({ cifrado: false }))).toEqual([
      CAPACIDADE_PASTA_LOCAL,
      CAPACIDADE_OCR,
    ]);
  });

  it("imagem exige ocr", () => {
    expect(resolveFileRequires("comprovante.png", encoder.encode("\x89PNG"))).toEqual([
      CAPACIDADE_PASTA_LOCAL,
      CAPACIDADE_OCR,
    ]);
  });

  it("não olha o conteúdo de quem não é PDF", () => {
    // Um CSV que por acaso contenha "/Encrypt" continua sendo um CSV.
    expect(resolveFileRequires("lista.csv", encoder.encode("nome;/Encrypt"))).toEqual([
      CAPACIDADE_PASTA_LOCAL,
    ]);
  });

  it("reconhece a extensão em caixa alta", () => {
    expect(resolveFileRequires("FATURA.PDF", pdfFalso({ cifrado: true }))).toEqual([
      CAPACIDADE_PASTA_LOCAL,
      CAPACIDADE_OCR,
      CAPACIDADE_SENHA_PDF,
    ]);
  });

  it("formato com parser próprio não pede ocr", () => {
    // CSV/XLSX/OFX têm parser dedicado; exigir OCR só encolheria à toa o
    // conjunto de workers elegíveis.
    expect(resolveFileRequires("extrato.csv", encoder.encode("a;b;c"))).not.toContain(
      CAPACIDADE_OCR,
    );
  });
});

describe("capacidadesNaoAtendidas", () => {
  it("devolve vazio quando o host cobre tudo que o job exige", () => {
    expect(
      capacidadesNaoAtendidas(
        [CAPACIDADE_PASTA_LOCAL, CAPACIDADE_SENHA_PDF],
        ["ocr", "pasta-local", "senha-pdf"],
      ),
    ).toEqual([]);
  });

  it("aponta o que falta — é o que vira aviso no scan", () => {
    expect(
      capacidadesNaoAtendidas([CAPACIDADE_PASTA_LOCAL, CAPACIDADE_SENHA_PDF], ["pasta-local"]),
    ).toEqual([CAPACIDADE_SENHA_PDF]);
  });

  it("job sem exigência é atendido por qualquer host, inclusive um sem capacidade", () => {
    expect(capacidadesNaoAtendidas([], [])).toEqual([]);
  });
});
