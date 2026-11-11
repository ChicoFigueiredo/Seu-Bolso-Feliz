/**
 * Normalização dos registros vindos da Pluggy (E3). Módulo puro, sem bun:sqlite
 * nem Bun.*, para o vitest testar.
 *
 * Problema: o extrato mistura gasto real com movimento do dinheiro entre contas
 * do próprio titular. Contar os dois duplica: a compra no cartão já é gasto, e o
 * "Pagamento de fatura" na conta corrente é só a mesma compra saindo do caixa.
 *
 * Escolhas:
 * - Aplicação/resgate de POUPANÇA INTEGRADA é `transferencia_interna`, não
 *   `investimento`: o app tem as contas-poupança como contas próprias e os dois
 *   lados (conta corrente e poupança) aparecem como registros que se anulam.
 *   `investimento` fica para produto fora das contas do app (CDB, fundo, ações).
 * - Fatura de cartão paga: o lado da conta e o lado do cartão ("Pagamento
 *   recebido") viram `pagamento_fatura`; o gasto é contado nas compras.
 * - Transferência para o próprio titular é `transferencia_interna` nos dois
 *   lados. O titular é reconhecido só pelo prenome "FRANCISC…" + sobrenome da
 *   família ou pelo apelido, depois de isolar o nome da contraparte.
 * - IOF, juros, multa e tarifa seguem como estão (são custo real).
 */

export type TipoMovimento =
  "transferencia_interna" | "pagamento_fatura" | "investimento" | "estorno";

export interface EntradaMovimento {
  descricao_original: string | null;
  supplier_name_raw: string | null;
  tipo: string;
  valor_centavos: number;
  conta_provavel: string | null;
  categoria_provavel: string | null;
}

export interface SaidaMovimento {
  tipo: TipoMovimento | null;
  fornecedor_normalizado: string | null;
  motivo: string;
}

/** Maiúsculas, sem acento, espaços colapsados. */
export function limpar(texto: string): string {
  return texto
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toUpperCase()
    .replace(/\s+/g, " ")
    .trim();
}

/** Nome do titular (prenome FRANCISC… e sobrenome, ou apelido). Recebe texto já limpo. */
const TITULAR = /^(FRANCISC\w*\s+(?:\w+\s+)*FIGUEIREDO|CHICO\s+FIGUEIREDO)$/;

export function ehTitular(nomeLimpo: string): boolean {
  return TITULAR.test(nomeLimpo);
}

/** Tira o verbo do extrato e devolve só a contraparte: "Pix recebido de X" → "X". */
function contraparte(descricao: string): string {
  return descricao
    .replace(
      /^(?:PIX (?:RECEBIDO|ENVIADO)(?: C6)? (?:DE|PARA)|TRANSFERENCIA (?:ENVIADA|RECEBIDA)(?: PELO PIX)?)[\s|]*/,
      "",
    )
    .trim();
}

const GENERICAS = [
  /^PIX (ENVIADO|RECEBIDO)$/,
  /^(DEB|CRED) PIX /,
  /^COMPRA CARTAO DEBITO$/,
  /^COMPRA NO DEBITO$/,
  /^DEBITO DE CARTAO$/,
  /^DEB AUT$/,
  /^PAG BOLETO IBC$/,
  /^PAGAMENTO DE BOLETO$/,
  /^PAGAMENTO BOLETO\b/,
  /^PAGAMENTO EFETUADO$/,
  /^TRANSFERENCIA (ENVIADA|RECEBIDA)(?: PELO PIX)?$/,
  /^CHEQUE /,
  /^SAQUE /,
  /^DEPOSITO /,
];

/** Instituição que aparece como "fornecedor" quando a contraparte real não veio. */
const SO_BANCO =
  /^(CAIXA ECONOMICA FEDERAL|BANCO .*|NU PAGAMENTOS.*|PAYPAL DO BRASIL.*|CAIXA CARTOES.*)$/;

/** Marca conhecida → nome curto. Testada contra descrição e fornecedor juntos. */
const ALIAS: [RegExp, string][] = [
  [/\bUBER\b|UBERRIDES/, "UBER"],
  [/DIGITALOCEA/, "DIGITALOCEAN"],
  [/ANTHROPIC|CLAUDE\.?AI/, "ANTHROPIC"],
  [/OPENAI|CHATGPT/, "OPENAI"],
  [/\bGITHUB\b/, "GITHUB"],
  [/APPLE\.COM|APPLE COMPUTER/, "APPLE"],
  [/GOOGLE (YOUTUBE|ONE|PLAY|CLOUD)|GOOGLE BRASIL/, "GOOGLE"],
  [/BEHONEST/, "BEHONEST"],
  [/FOLHA ?DE ?S ?PAULO|FOLHA DA MANHA/, "FOLHA DE S.PAULO"],
  [/NEOENERGIA/, "NEOENERGIA"],
  [/FUNCEF/, "FUNCEF"],
  [/TELEFONICA BRASIL/, "VIVO"],
];

const CIDADES = [
  "SAN FRANCISCO",
  "SAO PAULO",
  "BRASILIA",
  "AGUAS CLARAS",
  "TAGUATINGA",
  "OSASCO",
  "GOIANIA",
  "RIO DE JANEIRO",
  "BELO HORIZONTE",
  "AMSTERDAM",
  "DUBLIN",
  "LONDON",
  "CAMPINAS",
  "CURITIBA",
  "SALVADOR",
  "SAN JOSE",
];

const PAISES = "BRA|USA|NLD|IRL|GBR|CAN|DEU|FRA|PRT|ESP|SGP|LUX|ARG|URY|CHL|CYM";

/** Tira cidade/país do fim, parcela, ids e sufixo societário. Texto já limpo. */
export function limparNomeComercial(bruto: string): string {
  let t = bruto;
  // Colunas do extrato de cartão: "NOME      CIDADE    PAIS"
  const partes = t.split(/\s{2,}/);
  t = partes.length >= 2 && new RegExp(`(^|\\s)(${PAISES})$`).test(t) ? (partes[0] ?? t) : t;
  t = t.replace(new RegExp(`\\s+(${PAISES})$`), "");
  t = t.replace(new RegExp(`\\s+(${CIDADES.join("|")})$`), "");
  t = t
    .replace(/^(PAYPAL|DL|MP|PG|IZ|SUMUP|PAGSEGURO)\s*\*\s*/, "")
    .replace(/^\d{2}\.\d{3}\.\d{3}\s+/, "") // base do CNPJ de MEI
    .replace(/\*?\s*PENDING\b/, "")
    .replace(/\bPARC(?:ELA)?\s*\d{1,2}\s*(?:\/|DE)\s*\d{1,2}\b/, "")
    .replace(/\s\(?\d{1,2}\/\d{1,2}\)?$/, "")
    .replace(/[\s*#-]+\d{6,}$/, "")
    .replace(/^\d{6,}[A-Z]*\s*$/, "")
    .replace(/\s+(LTDA|EIRELI|S\.?\/?A\.?|ME|EPP|MEI)\.?$/, "")
    .replace(/\s*\*\s*$/, "")
    .replace(/\s+/g, " ")
    .trim();
  return t;
}

function nomeComercial(desc: string, forn: string): string | null {
  const junto = `${desc} ${forn}`;
  for (const [re, nome] of ALIAS) if (re.test(junto)) return nome;

  const descGenerica = GENERICAS.some((re) => re.test(desc));
  const prefixoBancario =
    /^(DEBITO|DEB|PAGAMENTO BOLETO|PAG BOLETO IBC|TRANSFERENCIA ENVIADA)\b/.test(desc);
  const fornUtil = forn !== "" && !SO_BANCO.test(forn);

  if (fornUtil && (descGenerica || prefixoBancario)) return limparNomeComercial(forn) || null;
  if (descGenerica) return null;

  let d = contraparte(desc);
  d = d.replace(/^(DEBITO|DEB) (?!PIX|AUT|DE CARTAO)/, "");
  d = limparNomeComercial(d);
  if (d === "" || /^\d+$/.test(d)) return fornUtil ? limparNomeComercial(forn) || null : null;
  // Descrição ainda é rótulo genérico depois da limpeza → tenta o fornecedor.
  if (fornUtil && /^(PAYPAL|COMPRA|PAGAMENTO)$/.test(d)) return limparNomeComercial(forn) || null;
  return d;
}

export function classificarMovimento(r: EntradaMovimento): SaidaMovimento {
  const desc = limpar(r.descricao_original ?? "");
  const forn = limpar(r.supplier_name_raw ?? "");
  if (desc === "" && forn === "") {
    return { tipo: null, fornecedor_normalizado: null, motivo: "sem descrição nem fornecedor" };
  }
  const nome = nomeComercial(desc, forn);
  const saiu = r.valor_centavos < 0;
  const entrou = r.valor_centavos > 0;
  const interno = (tipo: TipoMovimento, motivo: string): SaidaMovimento => ({
    tipo,
    fornecedor_normalizado: null,
    motivo,
  });

  // Fatura do cartão: lado do cartão e lado da conta.
  if (entrou && (/^PAGAMENTO RECEBIDO$/.test(desc) || /^INCLUSAO DE PAGAMENTO/.test(desc))) {
    return interno("pagamento_fatura", "pagamento recebido no cartão (entrada da fatura)");
  }
  if (saiu && /^PAGAMENTO DE FATURA$/.test(desc)) {
    return interno("pagamento_fatura", "pagamento de fatura debitado da conta");
  }
  if (saiu && /^(PGTO|PAGTO|PAG) FAT\b/.test(desc)) {
    return interno("pagamento_fatura", "débito de fatura de cartão na conta");
  }
  if (saiu && /BANCO C6/.test(forn) && /^(C6 BANK|PIX ENVIADO)$/.test(desc)) {
    return interno(
      "pagamento_fatura",
      "débito para o C6 coincide com pagamento recebido no cartão",
    );
  }

  // Poupança integrada: movimento entre contas do titular.
  if (/^(APLICACAO|RESGATE) POUPANCA/.test(desc)) {
    return interno(
      "transferencia_interna",
      "aplicação/resgate de poupança integrada (conta própria)",
    );
  }
  // Nubank rola o saldo não pago para "atraso"/"rotativo" e devolve o crédito: o gasto já
  // está nas compras originais, então o par se anula.
  if (/^(SALDO EM|CREDITO DE) (ATRASO|ROTATIVO)$/.test(desc)) {
    return interno("transferencia_interna", "rolagem de saldo do cartão (atraso/rotativo)");
  }
  // Produto de investimento fora das contas do app.
  if (
    /^(APLICACAO|RESGATE|APLIC|RESG)\b.*\b(CDB|LCI|LCA|RDB|FUNDO|FI|TESOURO|ACOES|PREVIDENCIA)\b/.test(
      desc,
    )
  ) {
    return interno("investimento", "aplicação/resgate em produto de investimento");
  }

  // Transferência para o próprio titular.
  for (const candidato of [contraparte(desc), forn]) {
    if (candidato !== "" && ehTitular(candidato)) {
      return interno("transferencia_interna", "contraparte é o próprio titular");
    }
  }

  // Devolução de valor na conta.
  if (entrou && /^(DEVOLUCAO|DEV VAL|ESTORNO)\b/.test(desc)) {
    return { tipo: "estorno", fornecedor_normalizado: nome, motivo: "devolução/estorno na conta" };
  }

  if (/^IOF\b/.test(desc)) return { tipo: null, fornecedor_normalizado: "IOF", motivo: "IOF" };
  if (/\bJUROS\b|^MULTA\b/.test(desc)) {
    return { tipo: null, fornecedor_normalizado: "JUROS E MULTAS", motivo: "juros/multa" };
  }
  return { tipo: null, fornecedor_normalizado: nome, motivo: "sem reclassificação" };
}
