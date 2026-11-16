/**
 * Extração de campos do inventário (E3): funções puras sobre texto. Sem
 * `bun:sqlite` nem `Bun.*`, para o vitest (Node) testar. Quem lê arquivo e
 * grava no banco é `e3-extrair.ts`.
 *
 * Reaproveitado do app (workers/ingestion/src/parsers/boleto-parser.ts e
 * boleto-utils-extractor.ts): o desenho dos campos (vencimento, competência,
 * valor, CNPJ, cedente, linha digitável) e o formato das linhas digitáveis. A
 * lógica foi copiada, não importada: o parser do app só casa MAIÚSCULAS, pega
 * "pagador" como fornecedor e o `boleto-utils` é dependência opcional carregada
 * por import dinâmico. Aqui a data do boleto sai do fator de vencimento com a
 * virada de 22/02/2025.
 */

export type TipoDocumento =
  "boleto" | "comprovante" | "fatura" | "contracheque" | "nfe" | "extrato" | "pedido";

export type TipoDataChave =
  "pagamento" | "vencimento" | "emissao" | "compra" | "mensagem" | "arquivo";

export interface Campos {
  vencimento?: string | null;
  pagamento?: string | null;
  emissao?: string | null;
  compra?: string | null;
  /** AAAA-MM */
  competencia?: string | null;
  /** Data da mensagem de e-mail que trouxe o documento. */
  mensagem?: string | null;
  /** Data do arquivo (mtime), último recurso. */
  arquivo?: string | null;
  valorCentavos?: number | null;
  fornecedor?: string | null;
  cnpj?: string | null;
  linhaDigitavel?: string | null;
}

// ── Utilidades de data e valor ────────────────────────────────────────────

const ANO_MIN = 1995;
const ANO_MAX = 2035;

function dois(n: number): string {
  return String(n).padStart(2, "0");
}

/** Monta AAAA-MM-DD se o dia existir no calendário e o ano for plausível. */
export function montarData(ano: number, mes: number, dia: number): string | null {
  if (ano < ANO_MIN || ano > ANO_MAX || mes < 1 || mes > 12 || dia < 1) return null;
  const diasNoMes = new Date(Date.UTC(ano, mes, 0)).getUTCDate();
  if (dia > diasNoMes) return null;
  return `${ano}-${dois(mes)}-${dois(dia)}`;
}

const MESES: Record<string, number> = {
  jan: 1,
  fev: 2,
  mar: 3,
  abr: 4,
  mai: 5,
  jun: 6,
  jul: 7,
  ago: 8,
  set: 9,
  out: 10,
  nov: 11,
  dez: 12,
};

function anoCompleto(a: string): number {
  const n = Number(a);
  return a.length === 2 ? 2000 + n : n;
}

const RE_DATA =
  /(?<![\d/.-])(?:(\d{4})-(\d{2})-(\d{2})|(\d{1,2})[/.-](\d{1,2})[/.-](\d{4}|\d{2})(?![\d/])|(\d{1,2})\s*(?:de\s+)?(jan|fev|mar|abr|mai|jun|jul|ago|set|out|nov|dez)[a-zç]*\.?,?\s*(?:de\s+|\/)?(\d{4}))/gi;

/** Primeira data válida de um trecho, em AAAA-MM-DD (dia/mês/ano brasileiro). */
export function primeiraData(trecho: string): string | null {
  for (const m of trecho.matchAll(RE_DATA)) {
    let iso: string | null = null;
    if (m[1]) iso = montarData(Number(m[1]), Number(m[2]), Number(m[3]));
    else if (m[4]) iso = montarData(anoCompleto(m[6]!), Number(m[5]), Number(m[4]));
    else if (m[7]) iso = montarData(Number(m[9]), MESES[m[8]!.toLowerCase()]!, Number(m[7]));
    if (iso) return iso;
  }
  return null;
}

/** "1.234,56" / "R$ 12,5" / "1234.56" → centavos. Null se não for dinheiro. */
export function valorParaCentavos(bruto: string): number | null {
  let s = bruto.replace(/R\$|\s/g, "");
  if (!/^-?[\d.,]+$/.test(s)) return null;
  const negativo = s.startsWith("-");
  s = s.replace(/^-/, "");
  const ultimaVirgula = s.lastIndexOf(",");
  const ultimoPonto = s.lastIndexOf(".");
  let inteiro: string;
  let frac: string;
  if (ultimaVirgula >= 0 && ultimaVirgula > ultimoPonto) {
    inteiro = s.slice(0, ultimaVirgula).replace(/\./g, "");
    frac = s.slice(ultimaVirgula + 1);
  } else if (ultimoPonto >= 0 && s.length - ultimoPonto - 1 <= 2 && !s.includes(",")) {
    // "1234.56" (decimal com ponto, como em XML)
    inteiro = s.slice(0, ultimoPonto).replace(/\./g, "");
    frac = s.slice(ultimoPonto + 1);
  } else {
    inteiro = s.replace(/[.,]/g, "");
    frac = "";
  }
  if (!/^\d*$/.test(inteiro) || !/^\d{0,2}$/.test(frac) || (inteiro === "" && frac === "")) {
    return null;
  }
  const centavos = Number(inteiro || "0") * 100 + Number(frac.padEnd(2, "0") || "0");
  if (!Number.isSafeInteger(centavos)) return null;
  return negativo ? -centavos : centavos;
}

const RE_DINHEIRO = /(?:R\$\s*)?(\d{1,3}(?:\.\d{3})+,\d{2}|\d+,\d{2})(?!\d)/;

// ── Rótulos ───────────────────────────────────────────────────────────────

/** Trecho após cada ocorrência do rótulo (até `alcance` caracteres, atravessando linhas). */
function* trechosApos(texto: string, rotulo: RegExp, alcance: number): Generator<string> {
  const re = new RegExp(
    rotulo.source,
    rotulo.flags.includes("g") ? rotulo.flags : `${rotulo.flags}g`,
  );
  for (const m of texto.matchAll(re)) {
    const ini = m.index + m[0].length;
    yield texto.slice(ini, ini + alcance);
  }
}

function dataAposRotulo(texto: string, rotulo: RegExp): string | null {
  for (const trecho of trechosApos(texto, rotulo, 80)) {
    const d = primeiraData(trecho);
    if (d) return d;
  }
  return null;
}

const ROT_PAGAMENTO =
  /\b(?:data\s+(?:do|de)\s+(?:pagamento|d[eé]bito|transa[cç][aã]o|opera[cç][aã]o|efetiva[cç][aã]o)|pago\s+em|pagamento\s+(?:realizado|efetuado)\s+em|data\s+da\s+transfer[eê]ncia|realizad[oa]\s+em|efetivad[oa]\s+em|d[eé]bito\s+em|data\s*\/\s*hora|data\s+e\s+hora|cr[eé]dito\s+em|data\s+(?:do\s+)?cr[eé]dito|data\s+(?:de\s+)?pagto)/gi;
const ROT_VENCIMENTO =
  /\b(?:data\s+(?:de\s+)?vencimento|vencimento|vence\s+em|pagamento\s+at[eé]|pagar\s+at[eé])/gi;
const ROT_EMISSAO =
  /\b(?:data\s+(?:de\s+)?emiss[aã]o|emitid[oa]\s+em|emiss[aã]o|data\s+do\s+documento|data\s+de\s+processamento|data\s+de\s+fechamento)/gi;
const ROT_COMPRA =
  /\b(?:data\s+d[ao]\s+(?:compra|pedido)|compra\s+(?:realizada\s+)?em|pedido\s+(?:realizado|feito)\s+em|pedido\s+de)/gi;

const MESES_EXTENSO: Record<string, number> = {
  janeiro: 1,
  fevereiro: 2,
  marco: 3,
  março: 3,
  abril: 4,
  maio: 5,
  junho: 6,
  julho: 7,
  agosto: 8,
  setembro: 9,
  outubro: 10,
  novembro: 11,
  dezembro: 12,
};

/** Competência em AAAA-MM: "03/2026", "março/2026", "março de 2026". */
export function extrairCompetencia(texto: string): string | null {
  const rot =
    /(?:compet[eê]ncia|m[eê]s\s+de\s+refer[eê]ncia|m[eê]s\s*\/\s*ano\s+de\s+pagamento|refer[eê]ncia|per[ií]odo)/gi;
  for (const trecho of trechosApos(texto, rot, 120)) {
    const num = /(?<!\d)(\d{1,2})\s*[/.-]\s*(\d{4})(?!\d)/.exec(trecho);
    if (num) {
      const mes = Number(num[1]);
      const ano = Number(num[2]);
      if (mes >= 1 && mes <= 12 && ano >= ANO_MIN && ano <= ANO_MAX) return `${ano}-${dois(mes)}`;
    }
    const ext = /([a-zçã]{4,9})\s*(?:de|\/|-)?\s*(\d{4})/i.exec(trecho);
    const mes = ext ? MESES_EXTENSO[ext[1]!.toLowerCase()] : undefined;
    if (ext && mes) {
      const ano = Number(ext[2]);
      if (ano >= ANO_MIN && ano <= ANO_MAX) return `${ano}-${dois(mes)}`;
    }
  }
  return null;
}

// ── Valor ─────────────────────────────────────────────────────────────────

const ROT_VALOR_PAGO =
  /(?:valor\s+pago|valor\s+da\s+transa[cç][aã]o|valor\s+(?:da\s+)?transferid[oa]|valor\s+debitado|valor\s+do\s+pagamento|valor\s+total\s+pago)/gi;
const ROT_VALOR_DOCUMENTO =
  /(?:valor\s+do\s+documento|valor\s+cobrado|valor\s+da\s+cobran[cç]a|valor\s+do\s+boleto|valor\s+nominal)/gi;
const ROT_VALOR_TOTAL =
  /(?:total\s+a\s+pagar|valor\s+a\s+pagar|total\s+da\s+fatura|valor\s+total\s+da\s+fatura|valor\s+total|total\s+do\s+pedido|total\s+geral|total\s+da\s+compra|valor\s+da\s+compra)/gi;
const ROT_VALOR_LIQUIDO =
  /(?:l[ií]quido\s+a\s+receber|valor\s+l[ií]quido|total\s+l[ií]quido|l[ií]quido)/gi;
const ROT_VALOR_GENERICO = /(?:^|\n|\s)(?:valor|total)(?=\s*[:\sR$\d])/gi;

function valorAposRotulo(texto: string, rotulo: RegExp): number | null {
  for (const trecho of trechosApos(texto, rotulo, 70)) {
    const m = RE_DINHEIRO.exec(trecho);
    if (m) {
      const v = valorParaCentavos(m[1]!);
      if (v !== null && v > 0) return v;
    }
  }
  return null;
}

/** Valor em centavos, escolhendo o rótulo mais confiável para o tipo. */
export function extrairValor(texto: string, tipo?: TipoDocumento | null): number | null {
  const ordem: Record<string, RegExp[]> = {
    comprovante: [ROT_VALOR_PAGO, ROT_VALOR_DOCUMENTO, ROT_VALOR_TOTAL, ROT_VALOR_GENERICO],
    contracheque: [ROT_VALOR_LIQUIDO, ROT_VALOR_TOTAL],
    fatura: [ROT_VALOR_TOTAL, ROT_VALOR_DOCUMENTO, ROT_VALOR_PAGO],
    boleto: [ROT_VALOR_DOCUMENTO, ROT_VALOR_TOTAL, ROT_VALOR_PAGO],
    pedido: [ROT_VALOR_TOTAL, ROT_VALOR_PAGO, ROT_VALOR_DOCUMENTO],
  };
  const padrao = [
    ROT_VALOR_PAGO,
    ROT_VALOR_DOCUMENTO,
    ROT_VALOR_TOTAL,
    ROT_VALOR_LIQUIDO,
    ROT_VALOR_GENERICO,
  ];
  for (const rot of (tipo && ordem[tipo]) || padrao) {
    const v = valorAposRotulo(texto, rot);
    if (v !== null) return v;
  }
  const solto = /R\$\s*(\d{1,3}(?:\.\d{3})*,\d{2}|\d+,\d{2})(?!\d)/.exec(texto);
  const v = solto ? valorParaCentavos(solto[1]!) : null;
  return v !== null && v > 0 ? v : null;
}

// ── Fornecedor, CNPJ ──────────────────────────────────────────────────────

const RE_CNPJ = /(?<!\d)\d{2}\.\d{3}\.\d{3}\/\d{4}-\d{2}(?!\d)/g;

export function cnpjValido(cnpj: string): boolean {
  const d = cnpj.replace(/\D/g, "");
  if (d.length !== 14 || /^(\d)\1+$/.test(d)) return false;
  const dv = (base: string): number => {
    const pesos =
      base.length === 12
        ? [5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2]
        : [6, 5, 4, 3, 2, 9, 8, 7, 6, 5, 4, 3, 2];
    const soma = [...base].reduce((acc, c, i) => acc + Number(c) * pesos[i]!, 0);
    const r = soma % 11;
    return r < 2 ? 0 : 11 - r;
  };
  return dv(d.slice(0, 12)) === Number(d[12]) && dv(d.slice(0, 13)) === Number(d[13]);
}

/** CNPJ (só dígitos): o primeiro válido; com `perto`, o mais próximo dessa posição. */
export function extrairCnpj(texto: string, perto?: number): string | null {
  const candidatos = [...texto.matchAll(RE_CNPJ)].filter((m) => cnpjValido(m[0]));
  if (candidatos.length === 0) return null;
  if (perto !== undefined) {
    candidatos.sort((a, b) => Math.abs(a.index - perto) - Math.abs(b.index - perto));
  }
  return candidatos[0]![0].replace(/\D/g, "");
}

const ROTULOS_FORNECEDOR: RegExp[] = [
  /\b(?:benefici[aá]rio(?:\s+final)?|cedente)\b/gi,
  /\b(?:favorecido|recebedor|nome\s+do\s+recebedor|pago\s+a|destino\s*\n?\s*nome|para\s*\n\s*nome)\b/gi,
  /\b(?:emitente|raz[aã]o\s+social|empregador|estabelecimento|vendido\s+por|vendedor|loja)\b/gi,
];

const NAO_NOME =
  /^(?:cpf|cnpj|ag[eê]ncia|conta|banco|institui[cç][aã]o|chave|valor|data|vencimento|n[uú]mero|nome|endere[cç]o|-|\d)/i;

function limparNome(bruto: string): string | null {
  let s = bruto.split(/\s{2,}|\t/)[0] ?? "";
  s = s
    .replace(/^[\s:.\-–]+/, "")
    .replace(/\s*[-–]?\s*(?:CNPJ|CPF)\b.*$/i, "")
    .replace(/\s*\d{2,3}\.\d{3}\.\d{3}.*$/, "")
    .replace(/[\s:.\-–]+$/, "")
    .trim();
  if (s.length < 3 || s.length > 80) return null;
  if ((s.match(/\p{L}/gu) ?? []).length < 3) return null;
  if (NAO_NOME.test(s)) return null;
  return s;
}

function nomeAposRotulo(texto: string, rotulo: RegExp): { nome: string; indice: number } | null {
  const re = new RegExp(rotulo.source, rotulo.flags);
  for (const m of texto.matchAll(re)) {
    const ini = m.index + m[0].length;
    const resto = texto.slice(ini, ini + 200);
    const linhas = resto.split("\n");
    // mesma linha, depois as duas linhas seguintes não vazias
    const candidatas = [
      linhas[0] ?? "",
      ...linhas
        .slice(1)
        .filter((l) => l.trim() !== "")
        .slice(0, 2),
    ];
    for (const c of candidatas) {
      const nome = limparNome(c);
      if (nome) return { nome, indice: m.index };
    }
  }
  return null;
}

export function extrairFornecedor(texto: string): { nome: string | null; cnpj: string | null } {
  for (const rot of ROTULOS_FORNECEDOR) {
    const r = nomeAposRotulo(texto, rot);
    if (r) return { nome: r.nome, cnpj: extrairCnpj(texto, r.indice) };
  }
  return { nome: null, cnpj: extrairCnpj(texto) };
}

// ── Linha digitável ───────────────────────────────────────────────────────

const RE_LINHA_BANCARIA =
  /(?<!\d)(\d{5}[.\s]?\d{5})\s+(\d{5}[.\s]?\d{6})\s+(\d{5}[.\s]?\d{6})\s+(\d)\s+(\d{14})(?!\d)/;
const RE_LINHA_CONCESSIONARIA = /(?<!\d)((?:\d{11}\s?-?\s?\d\s+){3}\d{11}\s?-?\s?\d)(?!\d)/;

export interface LinhaDigitavel {
  linha: string;
  tipo: "banco" | "concessionaria";
  valorCentavos: number | null;
  vencimento: string | null;
}

const BASE_FATOR = Date.UTC(1997, 9, 7);
const DIA_MS = 86_400_000;

/**
 * Vencimento a partir do fator (dias desde 07/10/1997). O fator voltou a 1000
 * em 22/02/2025; entre as duas leituras possíveis fica a que não passa de
 * `hoje` + 400 dias (boleto de antes de 2002 não existe neste acervo).
 */
export function vencimentoDoFator(fator: number, hoje = new Date()): string | null {
  if (fator < 1000 || fator > 9999) return null;
  const limite = hoje.getTime() + 400 * DIA_MS;
  const novo = BASE_FATOR + (fator + 9000) * DIA_MS;
  const ms = novo <= limite ? novo : BASE_FATOR + fator * DIA_MS;
  return new Date(ms).toISOString().slice(0, 10);
}

export function extrairLinhaDigitavel(texto: string, hoje = new Date()): LinhaDigitavel | null {
  const b = RE_LINHA_BANCARIA.exec(texto);
  if (b) {
    const digitos = b.slice(1, 6).join("").replace(/\D/g, "");
    if (digitos.length === 47) {
      const fator = Number(digitos.slice(33, 37));
      const valor = Number(digitos.slice(37, 47));
      return {
        linha: digitos,
        tipo: "banco",
        valorCentavos: valor > 0 ? valor : null,
        vencimento: fator > 0 ? vencimentoDoFator(fator, hoje) : null,
      };
    }
  }
  const c = RE_LINHA_CONCESSIONARIA.exec(texto);
  if (c) {
    const digitos = c[1]!.replace(/\D/g, "");
    if (digitos.length === 48 && digitos[0] === "8") {
      // Quatro blocos de 11 dígitos + DV; sem os DVs sobram 44 posições:
      // produto, segmento, identificador de valor, DV geral, valor (11).
      const semDv = [0, 12, 24, 36].map((i) => digitos.slice(i, i + 11)).join("");
      // Identificador 6/8 = valor efetivo em reais; 7/9 = valor de referência.
      const efetivo = semDv[2] === "6" || semDv[2] === "8";
      const valor = efetivo ? Number(semDv.slice(4, 15)) : 0;
      return {
        linha: digitos,
        tipo: "concessionaria",
        valorCentavos: valor > 0 ? valor : null,
        vencimento: null,
      };
    }
  }
  return null;
}

// ── Tipo do documento e campos ────────────────────────────────────────────

export function inferirTipoDocumento(texto: string): TipoDocumento | null {
  const t = texto.toLowerCase();
  if (/<nfe|<infnfe|nota\s+fiscal\s+eletr[oô]nica|danfe/.test(t)) return "nfe";
  if (
    /demonstrativo\s+de\s+pagamento|contra-?cheque|holerite|recibo\s+de\s+pagamento\s+de\s+sal/.test(
      t,
    )
  ) {
    return "contracheque";
  }
  if (
    /fatura\s+(?:do\s+)?cart[aã]o|fatura\s+(?:fechada|atual)|limite\s+(?:total|dispon[ií]vel)|pagamento\s+m[ií]nimo/.test(
      t,
    ) &&
    /fatura/.test(t)
  ) {
    return "fatura";
  }
  if (
    /comprovante\s+de\s+(?:transfer|pagamento|pix|dep[oó]sito|opera|saque)|comprovante\s+pix|pix\s+enviado|transfer[eê]ncia\s+(?:realizada|enviada|efetuada)|pagamento\s+(?:realizado|efetuado)/.test(
      t,
    )
  ) {
    return "comprovante";
  }
  if (RE_LINHA_BANCARIA.test(texto)) return "boleto";
  if (/linha\s+digit[aá]vel|benefici[aá]rio|cedente/.test(t) && /vencimento/.test(t)) {
    return "boleto";
  }
  if (/extrato\s+(?:de\s+)?(?:conta|banc|cart|investiment)|<ofx|ofxheader/.test(t)) {
    return "extrato";
  }
  if (
    /pedido\s+(?:n[º°o.]|realizado|confirmado|recebido)|n[uú]mero\s+do\s+pedido|resumo\s+do\s+pedido|seu\s+pedido/.test(
      t,
    )
  ) {
    return "pedido";
  }
  return null;
}

/** Todos os campos que o texto permite, sem decidir a data-chave. */
export function extrairCampos(
  texto: string,
  hoje = new Date(),
): Campos & { tipoDoc: TipoDocumento | null } {
  const tipoDoc = inferirTipoDocumento(texto);
  const linha = extrairLinhaDigitavel(texto, hoje);
  const forn = extrairFornecedor(texto);

  const vencimento = dataAposRotulo(texto, ROT_VENCIMENTO) ?? linha?.vencimento ?? null;
  const valorRotulo = extrairValor(texto, tipoDoc);
  // Em boleto, o valor do código de barras é a fonte mais confiável.
  const valor =
    tipoDoc === "boleto" && linha?.valorCentavos
      ? linha.valorCentavos
      : (valorRotulo ?? linha?.valorCentavos ?? null);

  return {
    tipoDoc,
    vencimento,
    pagamento: dataAposRotulo(texto, ROT_PAGAMENTO),
    emissao: dataAposRotulo(texto, ROT_EMISSAO),
    compra: dataAposRotulo(texto, ROT_COMPRA),
    competencia: extrairCompetencia(texto),
    valorCentavos: valor,
    fornecedor: forn.nome,
    cnpj: forn.cnpj,
    linhaDigitavel: linha?.linha ?? null,
  };
}

// ── Data-chave (CONTEXT.md › Ciclo) ───────────────────────────────────────

function ultimoDiaDoMes(competencia: string): string | null {
  const m = /^(\d{4})-(\d{2})$/.exec(competencia);
  if (!m) return null;
  const ano = Number(m[1]);
  const mes = Number(m[2]);
  return montarData(ano, mes, new Date(Date.UTC(ano, mes, 0)).getUTCDate());
}

/**
 * A data em que o dinheiro se moveu, se conhecida; senão o vencimento, a
 * emissão, a data da mensagem e, por último, a do arquivo. Compra no cartão ou
 * pedido usa a data da compra (o pagamento da fatura é outro registro). Fatura
 * de cartão vai direto ao vencimento: as datas de pagamento dentro dela são de
 * faturas anteriores.
 */
export function escolherDataChave(
  campos: Campos,
  tipoDoc?: TipoDocumento | string | null,
): { data: string; tipo: TipoDataChave } | null {
  const ordem: [string | null | undefined, TipoDataChave][] =
    tipoDoc === "fatura"
      ? [[campos.vencimento, "vencimento"]]
      : tipoDoc === "pedido"
        ? [
            [campos.compra, "compra"],
            [campos.pagamento, "pagamento"],
          ]
        : [
            [campos.pagamento, "pagamento"],
            [campos.compra, "compra"],
            [campos.vencimento, "vencimento"],
          ];
  ordem.push(
    [campos.emissao, "emissao"],
    [campos.competencia ? ultimoDiaDoMes(campos.competencia) : null, "emissao"],
    [campos.mensagem, "mensagem"],
    [campos.arquivo, "arquivo"],
  );
  for (const [data, tipo] of ordem) {
    if (data && /^\d{4}-\d{2}-\d{2}$/.test(data)) return { data, tipo };
  }
  return null;
}

// ── NF-e (XML) ────────────────────────────────────────────────────────────

export interface Nfe {
  emitente: string | null;
  cnpj: string | null;
  emissao: string | null;
  valorCentavos: number | null;
  numero: string | null;
}

function tag(xml: string, nome: string): string | null {
  const m = new RegExp(`<${nome}(?:\\s[^>]*)?>([^<]*)</${nome}>`, "i").exec(xml);
  return m ? m[1]!.trim() : null;
}

export function parsearNfe(xml: string): Nfe | null {
  if (!/<(?:infNFe|NFe|nfeProc)\b/i.test(xml)) return null;
  const emit = /<emit\b[^>]*>([\s\S]*?)<\/emit>/i.exec(xml)?.[1] ?? "";
  const ide = /<ide\b[^>]*>([\s\S]*?)<\/ide>/i.exec(xml)?.[1] ?? "";
  const dh = tag(ide, "dhEmi") ?? tag(ide, "dEmi");
  const vNF = tag(xml, "vNF");
  return {
    emitente: tag(emit, "xNome"),
    cnpj: tag(emit, "CNPJ"),
    emissao: dh ? primeiraData(dh) : null,
    valorCentavos: vNF ? valorParaCentavos(vNF) : null,
    numero: tag(ide, "nNF"),
  };
}

// ── OFX ───────────────────────────────────────────────────────────────────

export interface Ofx {
  banco: string | null;
  inicio: string | null;
  fim: string | null;
  transacoes: number;
}

function dataOfx(v: string | null): string | null {
  const m = v ? /^(\d{4})(\d{2})(\d{2})/.exec(v) : null;
  return m ? montarData(Number(m[1]), Number(m[2]), Number(m[3])) : null;
}

/** OFX 1.x (SGML, sem fechar tags) e 2.x (XML): lê o primeiro valor de cada tag. */
export function parsearOfx(texto: string): Ofx | null {
  if (!/<OFX>|OFXHEADER/i.test(texto)) return null;
  const valor = (nome: string): string | null => {
    const m = new RegExp(`<${nome}>\\s*([^<\\r\\n]+)`, "i").exec(texto);
    return m ? m[1]!.trim() : null;
  };
  return {
    banco: valor("ORG") ?? valor("BANKID") ?? valor("FID"),
    inicio: dataOfx(valor("DTSTART")),
    fim: dataOfx(valor("DTEND")),
    transacoes: (texto.match(/<STMTTRN>/gi) ?? []).length,
  };
}

// ── E-mail (.eml) ─────────────────────────────────────────────────────────

export interface Eml {
  de: string | null;
  deNome: string | null;
  assunto: string | null;
  data: string | null;
  corpo: string;
}

function decodificarBytes(bytes: Uint8Array, charset: string | undefined): string {
  try {
    return new TextDecoder(charset?.trim().replace(/^["']|["']$/g, "") || "utf-8").decode(bytes);
  } catch {
    return new TextDecoder("utf-8").decode(bytes);
  }
}

function quotedPrintable(s: string): Uint8Array {
  const limpo = s.replace(/=\r?\n/g, "");
  const out: number[] = [];
  const enc = new TextEncoder();
  for (let i = 0; i < limpo.length; i++) {
    const c = limpo[i]!;
    if (c === "=" && /^[0-9A-Fa-f]{2}$/.test(limpo.slice(i + 1, i + 3))) {
      out.push(parseInt(limpo.slice(i + 1, i + 3), 16));
      i += 2;
    } else {
      out.push(...enc.encode(c));
    }
  }
  return Uint8Array.from(out);
}

function base64Bytes(s: string): Uint8Array {
  const bin = atob(s.replace(/[^A-Za-z0-9+/=]/g, ""));
  return Uint8Array.from(bin, (c) => c.charCodeAt(0));
}

/** =?charset?B|Q?texto?= nos cabeçalhos. */
export function decodificarCabecalho(valor: string): string {
  return valor
    .replace(/(\?=)\s+(=\?)/g, "$1$2")
    .replace(/=\?([^?]+)\?([bBqQ])\?([^?]*)\?=/g, (_t, cs: string, enc: string, dados: string) => {
      try {
        const bytes =
          enc.toLowerCase() === "b"
            ? base64Bytes(dados)
            : quotedPrintable(dados.replace(/_/g, " "));
        return decodificarBytes(bytes, cs);
      } catch {
        return dados;
      }
    });
}

interface ParteMime {
  cabecalhos: Map<string, string>;
  corpo: string;
}

function separarCabecalhos(bruto: string): ParteMime {
  const norm = bruto.replace(/\r\n/g, "\n");
  const corte = norm.indexOf("\n\n");
  const cab = corte < 0 ? norm : norm.slice(0, corte);
  const corpo = corte < 0 ? "" : norm.slice(corte + 2);
  const cabecalhos = new Map<string, string>();
  for (const linha of cab.replace(/\n[ \t]+/g, " ").split("\n")) {
    const i = linha.indexOf(":");
    if (i > 0) {
      const k = linha.slice(0, i).trim().toLowerCase();
      if (!cabecalhos.has(k)) cabecalhos.set(k, linha.slice(i + 1).trim());
    }
  }
  return { cabecalhos, corpo };
}

const ENTIDADES: Record<string, string> = {
  nbsp: " ",
  amp: "&",
  lt: "<",
  gt: ">",
  quot: '"',
  apos: "'",
  aacute: "á",
  agrave: "à",
  acirc: "â",
  atilde: "ã",
  eacute: "é",
  ecirc: "ê",
  iacute: "í",
  oacute: "ó",
  ocirc: "ô",
  otilde: "õ",
  uacute: "ú",
  ccedil: "ç",
};

export function htmlParaTexto(html: string): string {
  return html
    .replace(/<(?:style|script|head)\b[\s\S]*?<\/(?:style|script|head)>/gi, " ")
    .replace(/<br\s*\/?>|<\/(?:p|div|tr|li|h\d|table)>/gi, "\n")
    .replace(/<\/t[dh]>/gi, "  ")
    .replace(/<[^>]+>/g, " ")
    .replace(/&#(\d+);/g, (_t, n: string) => String.fromCodePoint(Number(n)))
    .replace(/&([a-z]+);/gi, (t, nome: string) => ENTIDADES[nome.toLowerCase()] ?? t)
    .replace(/[ \t\u00a0]+/g, " ")
    .replace(/ *\n */g, "\n")
    .replace(/\n{3,}/g, "\n\n")
    .trim();
}

function corpoDaParte(parte: ParteMime, profundidade = 0): { plain: string[]; html: string[] } {
  const tipo = parte.cabecalhos.get("content-type") ?? "text/plain";
  const vazio = { plain: [] as string[], html: [] as string[] };
  if (profundidade > 6) return vazio;
  const boundary = /boundary="?([^";\s]+)"?/i.exec(tipo)?.[1];
  if (/^multipart\//i.test(tipo) && boundary) {
    const out = { plain: [] as string[], html: [] as string[] };
    for (const pedaco of parte.corpo.split(`--${boundary}`).slice(1)) {
      if (pedaco.startsWith("--")) break;
      const r = corpoDaParte(separarCabecalhos(pedaco.replace(/^\r?\n/, "")), profundidade + 1);
      out.plain.push(...r.plain);
      out.html.push(...r.html);
    }
    return out;
  }
  const disp = parte.cabecalhos.get("content-disposition") ?? "";
  if (/^attachment/i.test(disp)) return vazio;
  const ehHtml = /^text\/html/i.test(tipo);
  if (!ehHtml && !/^text\/plain/i.test(tipo)) return vazio;
  const cte = (parte.cabecalhos.get("content-transfer-encoding") ?? "7bit").toLowerCase();
  const charset = /charset="?([^";\s]+)"?/i.exec(tipo)?.[1];
  let bytes: Uint8Array;
  try {
    bytes =
      cte === "base64"
        ? base64Bytes(parte.corpo)
        : cte === "quoted-printable"
          ? quotedPrintable(parte.corpo)
          : new TextEncoder().encode(parte.corpo);
  } catch {
    return vazio;
  }
  const texto = decodificarBytes(bytes, charset);
  return ehHtml ? { plain: [], html: [texto] } : { plain: [texto], html: [] };
}

/** Data de cabeçalho RFC 2822 → dia no fuso do usuário. */
function dataDoCabecalho(valor: string | undefined, fuso = "America/Sao_Paulo"): string | null {
  if (!valor) return null;
  const d = new Date(valor);
  if (Number.isNaN(d.getTime())) return null;
  return d.toLocaleDateString("sv-SE", { timeZone: fuso });
}

export function parsearEml(bruto: string): Eml {
  const raiz = separarCabecalhos(bruto);
  const de = decodificarCabecalho(raiz.cabecalhos.get("from") ?? "") || null;
  const email = de ? (/<([^>]+)>/.exec(de)?.[1] ?? de) : null;
  const nome = de ? de.replace(/<[^>]*>/, "").replace(/^["'\s]+|["'\s]+$/g, "") : "";
  const partes = corpoDaParte(raiz);
  const corpo = htmlParaTexto(
    partes.plain.join("\n").trim() || partes.html.map(htmlParaTexto).join("\n").trim(),
  );
  return {
    de: email,
    deNome: nome && nome !== email ? nome : null,
    assunto: decodificarCabecalho(raiz.cabecalhos.get("subject") ?? "") || null,
    data: dataDoCabecalho(raiz.cabecalhos.get("date")),
    corpo,
  };
}
