/**
 * Registro único dos formatos que o pipeline sabe ler.
 *
 * POR QUE ISTO EXISTE: havia CINCO listas independentes — três `accept` na
 * interface, a allowlist do scanner local, a do scanner de Gmail — mais os
 * `allowed_mime_types` do bucket de Storage. Elas divergiam entre si e do
 * código: a interface anunciava `.doc`, `.docx` e `.qif`, que nenhum scanner
 * aceitava e nenhum parser lia; os scanners aceitavam `.xls`, que a interface
 * não oferecia e o pipeline não abria.
 *
 * O efeito prático era o pior possível para quem usa: o arquivo era aceito no
 * upload, subia, virava documento e parava em revisão sem campo nenhum — sem
 * erro, sem aviso, sem explicação. Uma recusa clara na hora do upload é
 * infinitamente melhor que um documento que morre em silêncio três etapas
 * adiante.
 *
 * A regra desta lista: **um formato só entra aqui quando existe código que o
 * lê.** Não há categoria "planejado".
 */

/**
 * O quanto o pipeline extrai de cada formato.
 *
 * - `completo`  — parser dedicado que produz campos (PDF, CSV, XLSX, OFX)
 * - `texto`     — o conteúdo é lido como texto; o que se extrai depende do
 *                 documento (XML)
 * - `ocr`       — só rende texto com OCR ligado (`INGESTION_ENABLE_IMAGE_OCR`)
 */
export type NivelDeSuporte = "completo" | "texto" | "ocr";

export interface FormatoDocumento {
  /** Extensão em minúsculas, com ponto. */
  readonly extensao: string;
  /** MIME canônico — o que o scanner grava e o bucket precisa permitir. */
  readonly mimeType: string;
  /**
   * MIMEs alternativos vistos na prática. Bancos servem OFX como
   * `application/octet-stream` e navegadores discordam sobre XML.
   */
  readonly mimeTypesAlternativos?: readonly string[];
  readonly rotulo: string;
  readonly suporte: NivelDeSuporte;
}

export const FORMATOS: readonly FormatoDocumento[] = [
  {
    extensao: ".pdf",
    mimeType: "application/pdf",
    rotulo: "PDF",
    suporte: "completo",
  },
  {
    extensao: ".csv",
    mimeType: "text/csv",
    mimeTypesAlternativos: ["text/plain", "application/csv"],
    rotulo: "CSV",
    suporte: "completo",
  },
  {
    extensao: ".xlsx",
    mimeType: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
    rotulo: "Excel (XLSX)",
    suporte: "completo",
  },
  {
    extensao: ".ofx",
    mimeType: "application/x-ofx",
    // Este é o caso real: o extrato baixado do banco quase nunca chega com o
    // MIME correto. Sem o alternativo, o anexo seria recusado no scanner de
    // Gmail antes de qualquer tentativa de leitura.
    mimeTypesAlternativos: ["application/octet-stream", "text/plain"],
    rotulo: "OFX (extrato bancário)",
    suporte: "completo",
  },
  {
    extensao: ".xml",
    mimeType: "application/xml",
    mimeTypesAlternativos: ["text/xml"],
    rotulo: "XML",
    suporte: "texto",
  },
  { extensao: ".png", mimeType: "image/png", rotulo: "Imagem PNG", suporte: "ocr" },
  { extensao: ".jpg", mimeType: "image/jpeg", rotulo: "Imagem JPEG", suporte: "ocr" },
  { extensao: ".jpeg", mimeType: "image/jpeg", rotulo: "Imagem JPEG", suporte: "ocr" },
  { extensao: ".webp", mimeType: "image/webp", rotulo: "Imagem WebP", suporte: "ocr" },
] as const;

/**
 * Formatos que saíram, e por quê. Existe para que ninguém os "reponha por
 * engano" achando que a omissão foi descuido.
 *
 * - `.xls`  — BIFF, o Excel antigo. O `exceljs` lê XLSX e **não** lê XLS. Os
 *             scanners aceitavam; o pipeline não abria.
 * - `.doc`  — anunciado na interface, sem parser e sem scanner que aceitasse.
 * - `.docx` — idem.
 * - `.qif`  — anunciado na interface e nem sequer estava na lista de nenhum
 *             scanner. Era promessa pura.
 */
export const FORMATOS_REMOVIDOS = [".xls", ".doc", ".docx", ".qif"] as const;

/** Extensões aceitas, para os scanners. */
export const EXTENSOES_ACEITAS: ReadonlySet<string> = new Set(FORMATOS.map((f) => f.extensao));

/** Todo MIME aceitável, canônico e alternativo — para o bucket e os scanners. */
export const MIME_TYPES_ACEITOS: ReadonlySet<string> = new Set(
  FORMATOS.flatMap((f) => [f.mimeType, ...(f.mimeTypesAlternativos ?? [])]),
);

/** Só os MIMEs canônicos: é o que o scanner grava em `source_documents`. */
export const MIME_TYPES_CANONICOS: readonly string[] = [
  ...new Set(FORMATOS.map((f) => f.mimeType)),
];

/** Valor do atributo `accept` de um `<input type="file">`. */
export const ACCEPT_UPLOAD: string = FORMATOS.map((f) => f.extensao).join(",");

/** Extensão → MIME canônico, para o scanner local rotular o que encontra. */
export const MIME_POR_EXTENSAO: Readonly<Record<string, string>> = Object.fromEntries(
  FORMATOS.map((f) => [f.extensao, f.mimeType]),
);

/** `documento.PDF` → `.pdf`. Devolve string vazia quando não há extensão. */
export function extensaoDe(nomeDoArquivo: string): string {
  const ponto = nomeDoArquivo.lastIndexOf(".");
  return ponto === -1 ? "" : nomeDoArquivo.slice(ponto).toLowerCase();
}

export function formatoDe(nomeDoArquivo: string): FormatoDocumento | undefined {
  const ext = extensaoDe(nomeDoArquivo);
  return FORMATOS.find((f) => f.extensao === ext);
}

export function extensaoAceita(nomeDoArquivo: string): boolean {
  return EXTENSOES_ACEITAS.has(extensaoDe(nomeDoArquivo));
}

/**
 * Texto para a interface dizer o que aceita, sem inventar.
 *
 * Ex.: "PDF, CSV, Excel (XLSX), OFX (extrato bancário), XML e imagens
 * (PNG, JPEG, WebP — requer OCR)".
 */
export function descricaoDosFormatos(): string {
  const comParser = FORMATOS.filter((f) => f.suporte !== "ocr").map((f) => f.rotulo);
  const imagens = FORMATOS.filter((f) => f.suporte === "ocr");

  const rotulosDeImagem = [...new Set(imagens.map((f) => f.rotulo.replace("Imagem ", "")))];
  const partes = [...new Set(comParser)];

  if (rotulosDeImagem.length > 0) {
    partes.push(`imagens (${rotulosDeImagem.join(", ")} — requer OCR)`);
  }

  return partes.length <= 1
    ? (partes[0] ?? "")
    : `${partes.slice(0, -1).join(", ")} e ${partes[partes.length - 1]}`;
}
