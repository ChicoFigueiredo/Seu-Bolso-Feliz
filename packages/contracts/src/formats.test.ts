/**
 * Equivalência entre o registro de formatos e todos os seus consumidores.
 *
 * Este arquivo é o teste que a issue #14 pede, e a parte que importa não é a
 * checagem das funções puras — é a comparação do registro contra os ARQUIVOS
 * REAIS: os componentes de upload, os dois scanners e a migration do bucket.
 * Uma constante compartilhada não impede ninguém de escrever uma lista literal
 * ao lado dela; só um teste que lê os arquivos impede.
 */
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import {
  ACCEPT_UPLOAD,
  EXTENSOES_ACEITAS,
  FORMATOS,
  FORMATOS_REMOVIDOS,
  MIME_POR_EXTENSAO,
  MIME_TYPES_ACEITOS,
  MIME_TYPES_CANONICOS,
  descricaoDosFormatos,
  extensaoAceita,
  extensaoDe,
  formatoDe,
  uploadAceito,
} from "./formats";

const RAIZ = join(import.meta.dirname, "../../..");
const ler = (caminho: string) => readFileSync(join(RAIZ, caminho), "utf-8");

describe("registro de formatos", () => {
  it("não tem extensão duplicada", () => {
    const extensoes = FORMATOS.map((f) => f.extensao);
    expect(new Set(extensoes).size).toBe(extensoes.length);
  });

  it("toda extensão é minúscula e começa com ponto", () => {
    for (const f of FORMATOS) {
      expect(f.extensao).toBe(f.extensao.toLowerCase());
      expect(f.extensao.startsWith(".")).toBe(true);
    }
  });

  it("nenhum formato removido reaparece no registro", () => {
    for (const removido of FORMATOS_REMOVIDOS) {
      expect(EXTENSOES_ACEITAS.has(removido)).toBe(false);
    }
  });
});

describe("consultas por nome de arquivo", () => {
  it("reconhece a extensão sem se importar com maiúsculas", () => {
    expect(extensaoDe("FATURA.PDF")).toBe(".pdf");
    expect(extensaoAceita("FATURA.PDF")).toBe(true);
    expect(formatoDe("FATURA.PDF")?.mimeType).toBe("application/pdf");
  });

  it("usa a última extensão em nomes com vários pontos", () => {
    expect(extensaoDe("extrato.2026.03.csv")).toBe(".csv");
  });

  it("recusa arquivo sem extensão", () => {
    expect(extensaoDe("arquivo")).toBe("");
    expect(extensaoAceita("arquivo")).toBe(false);
  });

  it("recusa os formatos que foram removidos de propósito", () => {
    expect(extensaoAceita("planilha.xls")).toBe(false);
    expect(extensaoAceita("contrato.docx")).toBe(false);
    expect(extensaoAceita("extrato.qif")).toBe(false);
  });
});

describe("derivados", () => {
  it("ACCEPT_UPLOAD lista exatamente as extensões do registro", () => {
    expect(ACCEPT_UPLOAD.split(",").sort()).toEqual([...EXTENSOES_ACEITAS].sort());
  });

  it("MIME_POR_EXTENSAO cobre toda extensão aceita", () => {
    for (const ext of EXTENSOES_ACEITAS) {
      expect(MIME_POR_EXTENSAO[ext]).toBeTruthy();
    }
  });

  it("MIME_TYPES_ACEITOS contém os canônicos e os alternativos", () => {
    expect(MIME_TYPES_ACEITOS.has("application/pdf")).toBe(true);
    expect(MIME_TYPES_ACEITOS.has("application/octet-stream")).toBe(true);
  });

  it("descreve os formatos sem prometer o que não lê", () => {
    const descricao = descricaoDosFormatos();

    expect(descricao).toContain("PDF");
    expect(descricao).toContain("OFX");
    expect(descricao).toContain("OCR");
    expect(descricao).not.toContain("QIF");
    expect(descricao).not.toContain("DOC");
  });
});

// ══════════════════════════════════════════════════════════════
// Equivalência contra os consumidores reais
// ══════════════════════════════════════════════════════════════

const COMPONENTES_DE_UPLOAD = [
  "apps/web/src/components/upload-documents.tsx",
  "apps/web/src/components/ai-chat-drawer.tsx",
  "apps/web/src/components/document-upload-dnd.tsx",
];

describe("equivalência com a interface", () => {
  it.each(COMPONENTES_DE_UPLOAD)("%s importa o accept em vez de escrever a lista", (caminho) => {
    const fonte = ler(caminho);

    expect(fonte).toContain("ACCEPT_UPLOAD");
    // A regressão trancada aqui: uma lista literal reintroduzida ao lado do
    // import, que é exatamente como as cinco listas divergiram da primeira vez.
    expect(fonte).not.toMatch(/"\.pdf,[^"]*"/);
  });
});

describe("equivalência com os scanners", () => {
  it.each([
    "workers/local-scanner/src/scanner.ts",
    "workers/gmail-scanner/src/message-processor.ts",
  ])("%s usa o registro em vez de uma allowlist própria", (caminho) => {
    const fonte = ler(caminho);

    expect(fonte).toMatch(/EXTENSOES_ACEITAS|MIME_TYPES_ACEITOS/);
    expect(fonte).not.toContain('".xlsx",');
  });
});

describe("a porta de entrada do upload", () => {
  /**
   * Este bloco existia como "equivalência com o bucket de Storage": o bucket
   * anterior tinha uma allowlist de MIME e recusava o que estivesse fora —
   * mas recusava DEPOIS de o documento já estar registrado, deixando linha
   * órfã em `source_documents`.
   *
   * Com o S3 não existe allowlist no bucket: ele aceita qualquer coisa. O
   * invariante não sumiu, mudou de lugar — quem recusa agora é
   * `uploadAceito`, chamado ANTES de qualquer escrita. Estes testes seguem o
   * invariante até o novo dono dele.
   */
  it("aceita todo MIME que o pipeline sabe processar", () => {
    const recusados = [...MIME_TYPES_ACEITOS].filter(
      (m) => !uploadAceito("arquivo.desconhecido", m),
    );
    expect(recusados).toEqual([]);
  });

  it("aceita toda extensão do registro, mesmo sem o navegador informar o MIME", () => {
    const recusadas = [...EXTENSOES_ACEITAS].filter((ext) => !uploadAceito(`documento${ext}`, ""));
    expect(recusadas).toEqual([]);
  });

  it("recusa formato sem parser", () => {
    expect(
      uploadAceito(
        "contrato.docx",
        "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
      ),
    ).toBe(false);
    expect(uploadAceito("contrato.doc", "application/msword")).toBe(false);
    expect(uploadAceito("arquivo.exe", "application/x-msdownload")).toBe(false);
  });

  it("todo MIME canônico está entre os aceitos", () => {
    for (const mime of MIME_TYPES_CANONICOS) {
      expect(MIME_TYPES_ACEITOS.has(mime)).toBe(true);
    }
  });
});
