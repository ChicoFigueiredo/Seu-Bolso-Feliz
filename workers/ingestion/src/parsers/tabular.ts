/**
 * Leitura de formatos tabulares: CSV, OFX e XLSX.
 *
 * POR QUE ISTO EXISTE: a interface aceitava XLSX, OFX, DOC e QIF e o pipeline
 * não lia nenhum. Pior que não ler: XLSX caía num `Buffer.toString()` e, como
 * XLSX é um ZIP, o "texto extraído" era lixo binário que atravessava o pipeline
 * inteiro sem erro e virava um `extraction_result` vazio. Uma promessa que
 * falha em silêncio é pior que uma ausência declarada.
 *
 * A saída de todos eles é TEXTO EM LINHAS, e não uma estrutura própria, porque
 * o resto do pipeline (boleto-parser, cemig-parser, templates de fornecedor,
 * enriquecimento por IA) já opera sobre texto. Entregar linhas mantém um único
 * caminho a jusante em vez de criar um segundo.
 */
import ExcelJS from "exceljs";

// ══════════════════════════════════════════════════════════════
// Codificação
// ══════════════════════════════════════════════════════════════

/**
 * Decodifica bytes para texto, escolhendo entre UTF-8 e latin-1.
 *
 * Extratos de banco brasileiros são exportados por sistemas Windows e chegam em
 * cp1252/latin-1 com frequência. Decodificar esses bytes como UTF-8 produz
 * U+FFFD no lugar de cada acento — e aí `MANUTENÇÃO` vira `MANUTEN��O`, que
 * nenhum alias de fornecedor casa.
 *
 * O critério é objetivo: decodifica como UTF-8 e, se aparecer o caractere de
 * substituição, refaz em latin-1. latin-1 nunca falha (todo byte é um caractere
 * válido), então é o fallback correto — não o palpite.
 */
export function decodificarTexto(buf: Buffer): string {
  const utf8 = buf.toString("utf-8");
  const texto = utf8.includes("�") ? buf.toString("latin1") : utf8;

  // O BOM não é conteúdo. Deixá-lo grudado transforma o primeiro cabeçalho em
  // `U+FEFF` + "Data", que não casa com nada.
  return texto.charCodeAt(0) === 0xfeff ? texto.slice(1) : texto;
}

// ══════════════════════════════════════════════════════════════
// CSV
// ══════════════════════════════════════════════════════════════

const DELIMITADORES_CANDIDATOS = [";", ",", "\t", "|"] as const;

export interface TabelaCsv {
  cabecalho: string[];
  linhas: string[][];
  /** `null` quando o conteúdo não é tabular — ver `detectarDelimitador`. */
  delimitador: string | null;
}

/**
 * Descobre o delimitador pela CONSISTÊNCIA entre linhas, não pela contagem.
 *
 * Duas armadilhas, e as duas custaram um bug real:
 *
 * 1. Contar ocorrências brutas erra no caso mais comum do Brasil: em
 *    `01/03/2026;-120,50;1.234,56` há uma vírgula decimal por valor, então a
 *    vírgula ganharia do `;` por frequência e quebraria cada campo ao meio.
 *
 * 2. Um arquivo `.csv` que NÃO é tabela — texto corrido salvo com essa
 *    extensão, coisa comum em conta de luz exportada — não tem delimitador
 *    nenhum. Eleger o "menos ruim" transformava `Valor a Pagar R$ 245,50` em
 *    dois campos e o valor extraído virava R$ 245,00. Um erro de cinquenta
 *    centavos que ninguém percebe olhando o relatório.
 *
 * Daí o retorno `null`: só há delimitador quando ele produz o MESMO número de
 * campos (≥2) em todas as linhas examinadas. Vírgula decimal não faz isso;
 * separador de verdade faz.
 */
export function detectarDelimitador(texto: string): string | null {
  const linhas = texto
    .split(/\r?\n/)
    .filter((l) => l.trim().length > 0)
    .slice(0, 10);

  if (linhas.length === 0) return null;

  let melhor: string | null = null;
  let melhorCampos = 0;

  for (const delim of DELIMITADORES_CANDIDATOS) {
    const contagens = linhas.map((l) => dividirLinhaCsv(l, delim).length);
    const campos = contagens[0]!;
    if (campos < 2) continue;
    if (!contagens.every((c) => c === campos)) continue;

    // Entre delimitadores igualmente consistentes, mais campos significa
    // separação mais fina — e portanto correta.
    if (campos > melhorCampos) {
      melhorCampos = campos;
      melhor = delim;
    }
  }

  return melhor;
}

/** Divide uma linha respeitando aspas e a convenção `""` de aspa literal. */
function dividirLinhaCsv(linha: string, delimitador: string): string[] {
  const campos: string[] = [];
  let atual = "";
  let dentroDeAspas = false;

  for (let i = 0; i < linha.length; i++) {
    const c = linha[i]!;

    if (c === '"') {
      if (dentroDeAspas && linha[i + 1] === '"') {
        atual += '"';
        i++;
      } else {
        dentroDeAspas = !dentroDeAspas;
      }
      continue;
    }

    if (c === delimitador && !dentroDeAspas) {
      campos.push(atual.trim());
      atual = "";
      continue;
    }

    atual += c;
  }

  campos.push(atual.trim());
  return campos;
}

export function parseCsv(texto: string, delimitador?: string): TabelaCsv {
  const delim = delimitador ?? detectarDelimitador(texto);
  const linhas = texto.split(/\r?\n/).filter((l) => l.trim().length > 0);

  if (linhas.length === 0 || delim === null) {
    return { cabecalho: [], linhas: [], delimitador: null };
  }

  return {
    cabecalho: dividirLinhaCsv(linhas[0]!, delim),
    linhas: linhas.slice(1).map((l) => dividirLinhaCsv(l, delim)),
    delimitador: delim,
  };
}

/**
 * Converte a tabela em linhas `Campo: valor`.
 *
 * O formato não é decorativo: os parsers determinísticos e os templates de
 * fornecedor procuram exatamente esse padrão (`Vencimento: 15/04/2026`) no
 * texto de PDFs. Emitir a mesma forma faz um CSV atravessar o pipeline pelo
 * mesmo caminho, sem código novo a jusante.
 */
export function csvParaTexto(tabela: TabelaCsv): string {
  const { cabecalho, linhas } = tabela;

  return linhas
    .map((linha) =>
      linha
        .map((valor, i) => {
          const nome = cabecalho[i]?.trim();
          return nome ? `${nome}: ${valor}` : valor;
        })
        .join("\n"),
    )
    .join("\n\n");
}

// ══════════════════════════════════════════════════════════════
// OFX
// ══════════════════════════════════════════════════════════════

export interface TransacaoOfx {
  tipo: string;
  data: string;
  valor: number;
  identificador: string;
  descricao: string;
}

export interface ExtratoOfx {
  moeda: string;
  conta: string;
  transacoes: TransacaoOfx[];
}

/**
 * Lê OFX 1.x (SGML) e 2.x (XML) com o mesmo código.
 *
 * As duas versões usam as MESMAS tags; a diferença é que o 1.x dispensa o
 * fechamento (`<TRNAMT>-245.50` termina na próxima tag). Uma expressão que
 * aceita "até a próxima `<` ou até a tag de fechamento" cobre os dois casos, e
 * evita arrastar um parser de XML para dentro do worker só por causa do 2.x.
 */
function valorDaTag(bloco: string, tag: string): string {
  const re = new RegExp(`<${tag}>\\s*([^<\\n\\r]*)`, "i");
  return re.exec(bloco)?.[1]?.trim() ?? "";
}

/** `20260301` e `20260301120000[-3:BRT]` viram `2026-03-01`. */
function dataOfx(bruto: string): string {
  const m = /^(\d{4})(\d{2})(\d{2})/.exec(bruto);
  return m ? `${m[1]}-${m[2]}-${m[3]}` : bruto;
}

/**
 * `-1234,56` e `-1234.56` viram -1234.56.
 *
 * O padrão OFX manda ponto decimal, mas bancos brasileiros emitem vírgula. Ler
 * `-1234,56` com `parseFloat` devolve -1234 — um erro de 56 centavos que passa
 * despercebido justamente por ser plausível.
 */
function valorOfx(bruto: string): number {
  const n = Number.parseFloat(bruto.replace(",", "."));
  return Number.isFinite(n) ? n : 0;
}

export function parseOfx(conteudo: string): ExtratoOfx {
  const blocos = conteudo.match(/<STMTTRN>[\s\S]*?(?:<\/STMTTRN>|(?=<STMTTRN>)|$)/gi) ?? [];

  const transacoes = blocos.map((bloco) => ({
    tipo: valorDaTag(bloco, "TRNTYPE"),
    data: dataOfx(valorDaTag(bloco, "DTPOSTED")),
    valor: valorOfx(valorDaTag(bloco, "TRNAMT")),
    identificador: valorDaTag(bloco, "FITID"),
    descricao: valorDaTag(bloco, "MEMO") || valorDaTag(bloco, "NAME"),
  }));

  return {
    moeda: valorDaTag(conteudo, "CURDEF"),
    conta: valorDaTag(conteudo, "ACCTID"),
    transacoes,
  };
}

export function ofxParaTexto(extrato: ExtratoOfx): string {
  const cabecalho = [
    extrato.conta ? `Conta: ${extrato.conta}` : "",
    extrato.moeda ? `Moeda: ${extrato.moeda}` : "",
    `Transacoes: ${extrato.transacoes.length}`,
  ]
    .filter(Boolean)
    .join("\n");

  const linhas = extrato.transacoes.map((t) =>
    [
      `Data: ${t.data}`,
      `Descricao: ${t.descricao}`,
      `Valor: ${t.valor.toFixed(2)}`,
      `Tipo: ${t.tipo}`,
      t.identificador ? `Identificador: ${t.identificador}` : "",
    ]
      .filter(Boolean)
      .join("\n"),
  );

  return [cabecalho, ...linhas].join("\n\n");
}

// ══════════════════════════════════════════════════════════════
// XLSX
// ══════════════════════════════════════════════════════════════

/**
 * Lê a planilha e devolve o texto de todas as abas.
 *
 * Datas recebem tratamento explícito porque o Excel as guarda como número de
 * série; deixar o `toString()` padrão agir produziria `Mon Mar 01 2026 …`, que
 * nenhum parser de data brasileiro casa.
 */
export async function xlsxParaTexto(buf: Buffer): Promise<string> {
  const workbook = new ExcelJS.Workbook();
  await workbook.xlsx.load(buf as unknown as ArrayBuffer);

  const abas: string[] = [];

  workbook.eachSheet((planilha) => {
    const linhas: string[] = [];
    let cabecalho: string[] = [];

    planilha.eachRow((row, numero) => {
      const valores: string[] = [];
      row.eachCell({ includeEmpty: true }, (cell) => {
        valores.push(celulaParaTexto(cell.value));
      });

      if (numero === 1) {
        cabecalho = valores;
        return;
      }

      if (valores.every((v) => v === "")) return;

      linhas.push(
        valores
          .map((v, i) => {
            const nome = cabecalho[i]?.trim();
            return nome ? `${nome}: ${v}` : v;
          })
          .filter((l) => !l.endsWith(": "))
          .join("\n"),
      );
    });

    if (linhas.length > 0) {
      abas.push(`Planilha: ${planilha.name}\n\n${linhas.join("\n\n")}`);
    }
  });

  return abas.join("\n\n");
}

function celulaParaTexto(valor: ExcelJS.CellValue): string {
  if (valor === null || valor === undefined) return "";
  if (valor instanceof Date) {
    return `${String(valor.getUTCDate()).padStart(2, "0")}/${String(valor.getUTCMonth() + 1).padStart(2, "0")}/${valor.getUTCFullYear()}`;
  }
  if (typeof valor === "object") {
    // Fórmula, hyperlink ou rich text: o que interessa é o valor apresentado.
    const obj = valor as { result?: unknown; text?: unknown; richText?: { text: string }[] };
    if (obj.richText) return obj.richText.map((r) => r.text).join("");
    if (obj.text !== undefined) return String(obj.text);
    if (obj.result !== undefined) return String(obj.result);
    return "";
  }
  return String(valor);
}
