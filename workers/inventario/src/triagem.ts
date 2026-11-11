/**
 * E3: triagem por regras. Para cada achado decide se é documento financeiro,
 * se foi visto e descartado (com motivo) ou se fica em dúvida para texto/OCR/IA.
 *
 * Módulo puro (sem bun:sqlite nem Bun.*), para o vitest importar. Todas as
 * origens do achado são avaliadas e o sinal mais forte vence: o mesmo conteúdo
 * numa pasta financeira e numa pasta qualquer é documento.
 *
 * Precisão antes de revocação para "documento": na dúvida, "duvida".
 */
import { FOTOS, TEXTOS_PESSOAIS, nomeFinanceiro } from "./areas";

export interface OrigemResumo {
  origin_type: string;
  local_filepath: string | null;
  filename: string;
  gmail_from: string | null;
  gmail_subject: string | null;
  gmail_label: string | null;
  mensagem_ref: string | null;
}

export interface AchadoParaTriagem {
  content_hash: string;
  extensao: string | null;
  file_size_bytes: number;
  camada: "A" | "B";
  origens: OrigemResumo[];
  texto?: string | null;
}

export interface Veredito {
  estado: "documento" | "descartado" | "duvida";
  document_type: string | null;
  motivo: string;
  confianca: number;
}

export const VERSAO_REGRAS = "regras@1";

// ---------------------------------------------------------------------------
// Utilidades

function semAcento(texto: string): string {
  return texto
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase();
}

function doc(tipo: string | null, motivo: string, confianca: number): Veredito {
  return { estado: "documento", document_type: tipo, motivo, confianca };
}
function duvida(motivo: string, confianca: number, tipo: string | null = null): Veredito {
  return { estado: "duvida", document_type: tipo, motivo, confianca };
}
function descarte(motivo: string, confianca: number): Veredito {
  return { estado: "descartado", document_type: null, motivo, confianca };
}

/** Tipo do documento a partir de um nome de arquivo, assunto ou pasta. */
const TIPOS_POR_NOME: Array<[RegExp, string]> = [
  [/contra[ _.-]?cheque|holerite|demonstrativo[ _.-]*de[ _.-]*pagamento/, "contracheque"],
  [/informes?[ _.-]*(de[ _.-]*)?rendimento/, "informe_rendimentos"],
  [/irpf|imposto[ _.-]*(de[ _.-]*)?renda|recibo[ _.-]*de[ _.-]*entrega/, "irpf"],
  [/\bdarf\b|\biptu\b|\bipva\b/, "outro_financeiro"],
  [/fatura|invoice/, "fatura"],
  [/boleto|segunda[ _.-]*via/, "boleto"],
  [/comprovante|\bpix\b|transferencia|\bted\b|deposito/, "comprovante"],
  [/extrato|statement/, "extrato"],
  [/nota[ _.-]?fiscal|\bnfs?-?e\b|danfe|\bnf\b/, "nota_fiscal"],
  [/recibo|receipt/, "recibo"],
  [/contrato|consignad|emprestimo|financiamento/, "contrato"],
  [/cobranca|negocia[cç]/, "cobranca"],
  [/pedido|compra[ _.-]*(aprovada|confirmada|realizada)|sua[ _.-]*compra/, "pedido"],
];

export function tipoPorNome(texto: string): string | null {
  const t = semAcento(texto);
  for (const [re, tipo] of TIPOS_POR_NOME) if (re.test(t)) return tipo;
  return null;
}

// ---------------------------------------------------------------------------
// Texto extraído (primeiras páginas de PDF, XML, corpo de e-mail)

interface SinalDeTexto {
  tipo: string | null;
  forte: boolean;
  motivo: string;
}

const MARCAS_NFE =
  /<nfe\b|<nfeproc|<infnfe|<cnfse|<compnfse|<nfse\b|<infnfse|xmlns="http:\/\/www\.portalfiscal\.inf\.br/i;

function sinalDoTexto(bruto: string): SinalDeTexto | null {
  const cru = bruto.slice(0, 30000);
  if (MARCAS_NFE.test(cru)) {
    return { tipo: "nota_fiscal", forte: true, motivo: "texto com marcas de NF-e/NFS-e" };
  }
  const t = semAcento(cru);
  const tem = (re: RegExp): boolean => re.test(t);
  if (tem(/\bdanfe\b|nota fiscal eletronica|chave de acesso/)) {
    return { tipo: "nota_fiscal", forte: true, motivo: "texto de nota fiscal" };
  }
  if (tem(/contra-?cheque|holerite|demonstrativo de pagamento|recibo de pagamento de salario/)) {
    return { tipo: "contracheque", forte: true, motivo: "texto de contracheque" };
  }
  if (tem(/informe de rendimentos/)) {
    return { tipo: "informe_rendimentos", forte: true, motivo: "texto de informe de rendimentos" };
  }
  if (
    tem(/declaracao de ajuste anual|recibo de entrega da declaracao|imposto de renda pessoa fisica/)
  ) {
    return { tipo: "irpf", forte: true, motivo: "texto de declaração de imposto de renda" };
  }
  if (
    tem(/linha digitavel|ficha de compensacao|codigo de barras/) &&
    tem(/vencimento|valor do documento/)
  ) {
    return {
      tipo: "boleto",
      forte: true,
      motivo: "texto de boleto (linha digitável e vencimento)",
    };
  }
  if (tem(/fatura/) && tem(/total da fatura|pagamento minimo|vencimento/)) {
    return { tipo: "fatura", forte: true, motivo: "texto de fatura com vencimento" };
  }
  if (
    tem(
      /comprovante de (pagamento|transferencia|pix|transacao|deposito)|transferencia realizada|pix enviado/,
    )
  ) {
    return { tipo: "comprovante", forte: true, motivo: "texto de comprovante" };
  }
  if (tem(/extrato/) && tem(/saldo|lancamentos|movimentacao/)) {
    return { tipo: "extrato", forte: true, motivo: "texto de extrato com saldo" };
  }
  if (tem(/\brecibo\b/) && tem(/recebi|r\$/)) {
    return { tipo: "recibo", forte: true, motivo: "texto de recibo com valor" };
  }
  if (tem(/\bdarf\b/)) return { tipo: "outro_financeiro", forte: true, motivo: "texto de DARF" };
  if (tem(/clausula primeira|contratante|contratado/) && tem(/r\$|valor/)) {
    return { tipo: "contrato", forte: false, motivo: "texto parece contrato com valor" };
  }
  if (tem(/resumo do pedido|numero do pedido|pedido n/) && tem(/r\$/)) {
    return { tipo: "pedido", forte: false, motivo: "texto de pedido com valor" };
  }
  if ((t.match(/r\$/g) ?? []).length >= 3 && tem(/vencimento|total|valor/)) {
    return { tipo: null, forte: false, motivo: "texto com vários valores em reais" };
  }
  return null;
}

// ---------------------------------------------------------------------------
// Caminhos locais

/** Pastas de desenvolvimento, instalação e dados de aplicativo: nunca financeiras. */
const SEGMENTOS_DE_DEV = new Set(
  [
    "node_modules",
    ".git",
    ".svn",
    "vendor",
    "site-packages",
    "wp-content",
    "wp-includes",
    "wp-admin",
    "themes",
    "theme",
    "plugins",
    "moodle",
    "bower_components",
    "assets",
    "bin",
    "obj",
    "lib",
    "libs",
    "src",
    "res",
    "drawable",
    "platforms",
    "build",
    "dist",
    "target",
    "jboss-eap-6.4",
    "magento",
    "magento-loja",
    ".thumbnails",
    ".cache",
    "cache",
    "__pycache__",
    ".nuget",
    "packages",
    "sdk",
    "actions-runner",
    "intermediates",
    "layers",
  ].map((s) => s.toLowerCase()),
);

/** Segmento começa com palavra financeira (prefixos numéricos e marcas ignorados). */
const PASTAS_FINANCEIRAS: Array<[RegExp, string | null]> = [
  [/^(imposto[ _.-]*(de[ _.-]*)?renda|impostorenda|irpf\d*|ir)(?![a-z])/, "irpf"],
  [/^(contra[ _.-]?cheques?|holerites?)(?![a-z])/, "contracheque"],
  [/^comprovantes?(?![a-z])/, "comprovante"],
  [/^faturas?(?![a-z])/, "fatura"],
  [/^boletos?(?![a-z])/, "boleto"],
  [/^recibos?(?![a-z])/, "recibo"],
  [/^extratos?(?![a-z])/, "extrato"],
  [/^notas?[ _.-]*fiscais(?![a-z])/, "nota_fiscal"],
  [/^(emprestimos?|consig[a-z]*|financiamentos?)(?![a-z])/, "contrato"],
  [/^financeiro(?![a-z])/, null],
  [/^financas(?![a-z])/, null],
  [/^contas?(?![a-z])/, null],
  [/^conta[ _.-]corrente/, null],
  [/^condominio(?![a-z])/, null],
  [/^banco(?![a-z])(?!.*(questoes|imagens))/, null],
  [/^cartao(?![a-z])/, null],
  [/^despesas?(?![a-z])/, null],
  [/^pagamentos?(?![a-z])/, null],
  [/^(ipva|iptu|cheques?|pensao|seguros?(?![a-z])|contabilidade)/, null],
  [/^cobranca$/, "cobranca"],
];

/** Pastas que não provam nada, mas tornam um documento digno de dúvida. */
const PASTAS_SUSPEITAS = /^(mayara|saude|pessoal|documentos|downloads?|planos?)(?![a-z])/;

const PASTAS_DE_CAPTURA =
  /^(whatsapp( images| documents| business)?|screenshots?|capturas?( de tela)?|downloads?|download|comprovantes?|telegram( images| documents)?)$/;

function segmentosLimpos(caminho: string): string[] {
  return semAcento(caminho)
    .split("/")
    .slice(0, -1)
    .filter(Boolean)
    .map((s) => s.replace(/^[\d@#_!. -]+/, ""));
}

interface PastaFinanceira {
  tipo: string | null;
  pasta: string;
}

function pastaFinanceira(segs: string[]): PastaFinanceira | null {
  // O segmento mais próximo do arquivo é o mais específico.
  for (let i = segs.length - 1; i >= 0; i--) {
    for (const [re, tipo] of PASTAS_FINANCEIRAS) {
      if (re.test(segs[i]!)) return { tipo, pasta: segs[i]! };
    }
  }
  return null;
}

const ARQUIVO_DE_PROGRAMA =
  /^(leia[ _.-]?me|readme|offline|ajuda|help|index|license|licenca|changelog)/;
const EXT_DE_RECURSO = new Set([
  "js",
  "css",
  "php",
  "ico",
  "svg",
  "ini",
  "dat",
  "dll",
  "exe",
  "class",
  "jar",
  "map",
]);
const EXT_OFFICE = new Set([
  "pdf",
  "doc",
  "docx",
  "odt",
  "rtf",
  "xls",
  "xlsx",
  "xlsm",
  "ods",
  "csv",
]);
const EXT_EXTRATO = new Set(["ofx", "ofc", "qif"]);
const EXT_COMPACTADO = new Set(["zip", "rar", "7z"]);
const EXT_EMAIL = new Set(["eml", "msg", "mbox"]);
const CHAVE_NFE = /(^|\D)\d{44}(\D|$)/;

interface Contexto {
  ext: string;
  camada: "A" | "B";
  tamanho: number;
  /** Texto lido, longo, sem nenhum termo financeiro. */
  textoNegativo: boolean;
}

/** Fallback de um arquivo sem nenhum sinal: dúvida só se ainda vale ler. */
function semSinal(ext: string, ctx: Contexto): Veredito {
  if (ctx.textoNegativo) return descarte("texto lido sem termos financeiros", 0.6);
  if (ext === "pdf") {
    return ctx.camada === "A"
      ? duvida("pdf em área pessoal sem sinal no nome nem na pasta", 0.3)
      : duvida("pdf fora das áreas pessoais sem sinal; falta o texto", 0.2);
  }
  if (EXT_EMAIL.has(ext)) return duvida("mensagem de e-mail salva, sem sinal; falta o texto", 0.3);
  if (EXT_OFFICE.has(ext) || EXT_COMPACTADO.has(ext)) {
    return ctx.camada === "A"
      ? duvida(`${ext} em área pessoal sem sinal no nome nem na pasta`, 0.25)
      : descarte(`${ext} fora das áreas pessoais, sem sinal financeiro`, 0.6);
  }
  if (FOTOS.has(ext)) return descarte("foto pessoal, fora de pasta financeira ou de captura", 0.8);
  if (ext === "xml") return descarte("xml sem marca de nota fiscal", 0.6);
  if (TEXTOS_PESSOAIS.has(ext))
    return descarte(`${ext} sem sinal financeiro (página, dado ou dev)`, 0.7);
  return descarte(`extensão ${ext || "(nenhuma)"} sem sinal financeiro`, 0.7);
}

function avaliarLocal(o: OrigemResumo, ctx: Contexto): Veredito {
  const caminho = o.local_filepath ?? o.filename;
  const ext = ctx.ext;
  const nome = o.filename;
  const nomeSemAcento = semAcento(nome);
  const segs = segmentosLimpos(caminho);
  const pasta = pastaFinanceira(segs);
  const nomeFin = nomeFinanceiro(nome);
  const tipoNome = tipoPorNome(nome);
  const emDev = segs.some((s) => SEGMENTOS_DE_DEV.has(s));

  if (EXT_EXTRATO.has(ext)) return doc("extrato", `arquivo ${ext} (extrato bancário)`, 0.95);

  if (ext === "xml") {
    if (CHAVE_NFE.test(nome))
      return doc("nota_fiscal", "xml nomeado com chave de NF-e (44 dígitos)", 0.9);
  }

  // Pasta de desenvolvimento ou de instalação: só um nome financeiro salva.
  if (emDev && !pasta) {
    if (nomeFin && (EXT_OFFICE.has(ext) || FOTOS.has(ext))) {
      return duvida("nome financeiro dentro de pasta de desenvolvimento", 0.3, tipoNome);
    }
    return descarte("artefato de desenvolvimento/instalação", 0.9);
  }

  if (EXT_DE_RECURSO.has(ext)) return descarte(`recurso ${ext} (código ou aplicativo)`, 0.9);

  // Pasta financeira
  if (pasta) {
    const tipo = tipoNome ?? pasta.tipo ?? "outro_financeiro";
    const ondeEsta = `pasta financeira "${pasta.pasta}"`;
    if (EXT_OFFICE.has(ext)) {
      return doc(tipo, `${ext} em ${ondeEsta}`, ext === "pdf" ? 0.75 : 0.65);
    }
    if (ext === "gif") return descarte("gif (ícone ou animação), nunca comprovante", 0.9);
    if (FOTOS.has(ext)) return duvida(`imagem em ${ondeEsta}; precisa de OCR`, 0.5, tipo);
    if (ext === "htm" || ext === "html") {
      if (ARQUIVO_DE_PROGRAMA.test(nomeSemAcento) && !nomeFin) {
        return descarte("documentação de programa dentro de pasta financeira", 0.7);
      }
      return duvida(`página html em ${ondeEsta}`, 0.4, tipo);
    }
    if (EXT_COMPACTADO.has(ext)) return duvida(`arquivo compactado em ${ondeEsta}`, 0.5, tipo);
    if (EXT_EMAIL.has(ext)) return duvida(`e-mail salvo em ${ondeEsta}`, 0.5, tipo);
    return duvida(`${ext || "arquivo"} em ${ondeEsta}`, 0.4, tipo);
  }

  // Nome financeiro fora de pasta financeira
  if (nomeFin) {
    if (EXT_OFFICE.has(ext))
      return doc(tipoNome ?? "outro_financeiro", `${ext} com nome financeiro`, 0.8);
    if (FOTOS.has(ext)) return duvida("imagem com nome financeiro; precisa de OCR", 0.5, tipoNome);
    if (ext === "xml" || ext === "htm" || ext === "html" || ext === "txt" || EXT_EMAIL.has(ext)) {
      return duvida(`${ext} com nome financeiro`, 0.5, tipoNome);
    }
    if (EXT_COMPACTADO.has(ext)) return duvida("compactado com nome financeiro", 0.5, tipoNome);
  }

  // Fotos fora de pasta financeira
  if (FOTOS.has(ext)) {
    if (ext === "gif" || ext === "webp")
      return descarte(`${ext} (figurinha, ícone ou animação)`, 0.85);
    if (
      /(^|[ _-])stk-|sticker|thumb/.test(nomeSemAcento) ||
      segs.some((s) => /sticker|profile pictures|thumbnails/.test(s))
    ) {
      return descarte("figurinha, foto de perfil ou miniatura", 0.9);
    }
    const emCaptura = segs.some((s) => PASTAS_DE_CAPTURA.test(s));
    const nomeDeCaptura = /^(screenshot|captura|img-\d{8}-wa|photo-\d{4}|print)/.test(
      nomeSemAcento,
    );
    if (emCaptura || nomeDeCaptura) {
      return duvida("imagem de captura, WhatsApp ou download; precisa de OCR", 0.3);
    }
    return descarte("foto pessoal", 0.85);
  }

  // Documento em pasta suspeita (saúde, Mayara, Documentos...): vale ler.
  if (segs.some((s) => PASTAS_SUSPEITAS.test(s)) && (ext === "pdf" || EXT_OFFICE.has(ext))) {
    if (ctx.textoNegativo) return descarte("texto lido sem termos financeiros", 0.6);
    return duvida(
      "documento em pasta pessoal que pode ter despesa (saúde, filha, documentos)",
      0.3,
    );
  }

  return semSinal(ext, ctx);
}

// ---------------------------------------------------------------------------
// Gmail

function dominioDe(remetente: string | null): string {
  if (!remetente) return "";
  const m = remetente.match(/@([a-z0-9.-]+)/i);
  return m ? m[1]!.toLowerCase().replace(/\.+$/, "") : "";
}

function dominioEm(dominio: string, lista: readonly string[]): boolean {
  return lista.some((d) => dominio === d || dominio.endsWith(`.${d}`));
}

const BANCOS = [
  "nubank.com.br",
  "c6bank.com.br",
  "caixa.gov.br",
  "caixa.com.br",
  "itau.com.br",
  "itau-unibanco.com.br",
  "bradesco.com.br",
  "santander.com.br",
  "bb.com.br",
  "inter.co",
  "bancointer.com.br",
  "picpay.com",
  "mercadopago.com.br",
  "mercadopago.com",
  "paypal.com",
  "paypal.com.br",
  "pagseguro.com.br",
  "pagseguro.uol.com.br",
  "asaas.com",
  "serasa.com.br",
  "bancopan.com.br",
  "neon.com.br",
  "sicredi.com.br",
  "sicoob.com.br",
];

/** Concessionárias, serviços e lojas que mandam comprovante/fatura/pedido. */
const EMISSORES = [
  "cemig.com.br",
  "copasa.com.br",
  "neoenergia.com",
  "enel.com",
  "claro.com.br",
  "vivo.com.br",
  "tim.com.br",
  "canaatelecom.com.br",
  "registro.br",
  "digitalocean.com",
  "amazon.com.br",
  "amazon.com",
  "uber.com",
  "ifood.com.br",
  "99app.com",
  "apple.com",
  "google.com",
  "vercel.com",
  "github.com",
  "anthropic.com",
  "openai.com",
  "netflix.com",
  "spotify.com",
  "smiles.com.br",
  "voegol.com.br",
  "latam.com",
  "booking.com",
  "airbnb.com",
  "mercadolivre.com.br",
  "shopee.com.br",
  "aliexpress.com",
  "magazineluiza.com.br",
  "americanas.com.br",
  "kabum.com.br",
  "drogasil.com.br",
  "raiadrogasil.com.br",
  "leroymerlin.com.br",
  "nespresso.com",
  "samsclub.com.br",
  "fastshop.com.br",
  "sheinemail.com",
  "shein.com",
  "gov.br",
  "fazenda.gov.br",
  "playstation.com",
  "stripe.com",
  "hotmart.com",
  "udemy.com",
];

/** Empresas de cobrança de dívida: o e-mail É a cobrança. */
const COBRADORAS = ["acerto.com.br", "acordocerto.com.br", "meuacerto.com.br", "serasa.com.br"];

const BOLETINS = [
  "washingtonpost.com",
  "groups.io",
  "folhadespaulo.com.br",
  "folha.com.br",
  "theintercept.com",
  "beehiiv.com",
  "jusbrasil.com.br",
  "linkedin.com",
  "udemymail.com",
  "filipedeschamps.com.br",
  "globo.com",
  "minhaband.com.br",
  "medium.com",
  "substack.com",
  "facebookmail.com",
  "twitter.com",
  "x.com",
  "instagram.com",
  "youtube.com",
  "quora.com",
  "pinterest.com",
  "reddit.com",
  "tiktok.com",
  "nytimes.com",
  "uol.com.br",
  "meetup.com",
];

const PREFIXO_DE_MARKETING =
  /^(edm|mkt|news|newsletter|nl|e|emails?|exclusivo|relacionamento|curadoria|comunicacao|comunica|marketing|promo|ofertas?|mail|info|informe|cp|campanhas?)\./;
const PREFIXO_DE_COBRANCA = /^(cobranca|negoci[a-z]*|recupera|renegociacao|scpc|informa)\./;

const ASSUNTO_TRANSACIONAL =
  /(sua fatura|fatura (do|de|da|disponivel|fechada|vence|#)|a fatura|\binvoice\b|\breceipt\b|pix de r\$|payment (received|successful)|received .{0,10}payment|thanks for your payment|boleto|comprovante|recibo|nota fiscal|\bnf-?e\b|segunda via|extrato|pagamento (confirmado|realizado|recebido|aprovado|efetuado)|compra (aprovada|confirmada|realizada)|pedido (confirmado|realizado|recebido|n[o°º ]|#)|seu pedido|debito automatico|cobranca|vencimento|transferencia (realizada|recebida)|pix (enviado|recebido))/;
const ASSUNTO_FINANCEIRO_FRACO =
  /(pagamento|payment|compra|pedido|\bconta\b|cobranca|vencimento|fatura|fiscal|pagar|debito|credito|cartao|parcela|emprestimo|seguro|imposto)/;
const ASSUNTO_DE_PROMOCAO =
  /(oferta|desconto|% ?off|promocao|ultimas horas|black friday|cupom|frete gratis|newsletter|cashback|imperdivel|aproveite|liquida|so hoje|ganhe)/;

const CATEGORIAS_PROMOCIONAIS = /CATEGORY_(PROMOTIONS|SOCIAL|FORUMS)/;
const EXT_DE_IMAGEM_DE_ASSINATURA = /^(image\d*|logo|outlook-|unnamed|att\d+|signature)/;

function avaliarGmail(o: OrigemResumo, ctx: Contexto): Veredito {
  const dominio = dominioDe(o.gmail_from);
  const assunto = semAcento(o.gmail_subject ?? "");
  const nome = o.filename;
  const ext = ctx.ext;
  const promo = CATEGORIAS_PROMOCIONAIS.test(o.gmail_label ?? "");
  const boletim = dominioEm(dominio, BOLETINS);
  const banco = dominioEm(dominio, BANCOS);
  const emissor = banco || dominioEm(dominio, EMISSORES);
  const cobradora = dominioEm(dominio, COBRADORAS);
  const marketing = PREFIXO_DE_MARKETING.test(dominio) || ASSUNTO_DE_PROMOCAO.test(assunto);
  const transacional = ASSUNTO_TRANSACIONAL.test(assunto);
  const tipoAssunto = tipoPorNome(o.gmail_subject ?? "");
  const anexo = o.mensagem_ref === "anexo";

  if (EXT_EXTRATO.has(ext)) return doc("extrato", `anexo ${ext} (extrato bancário)`, 0.9);

  if (anexo) {
    const tipoNome = tipoPorNome(nome);
    const nomeFin = nomeFinanceiro(nome);
    if (FOTOS.has(ext)) {
      if (ctx.tamanho < 50_000 || EXT_DE_IMAGEM_DE_ASSINATURA.test(semAcento(nome))) {
        if (!nomeFin) return descarte("imagem pequena de assinatura ou logotipo", 0.85);
      }
      if (nomeFin || (emissor && transacional)) {
        return duvida(
          "imagem anexada de origem financeira; precisa de OCR",
          0.5,
          tipoNome ?? tipoAssunto,
        );
      }
      return descarte("imagem anexada sem sinal financeiro", 0.6);
    }
    if (boletim && !nomeFin) return descarte("anexo de boletim ou rede social", 0.8);
    if (cobradora) return doc("cobranca", "anexo de empresa de cobrança", 0.7);
    if (nomeFin || (transacional && !promo)) {
      if (EXT_OFFICE.has(ext) || ext === "xml" || EXT_COMPACTADO.has(ext)) {
        return doc(
          tipoNome ?? tipoAssunto ?? "outro_financeiro",
          "anexo com nome ou assunto financeiro",
          0.8,
        );
      }
    }
    if (emissor && !promo && (EXT_OFFICE.has(ext) || ext === "xml")) {
      return doc(
        tipoNome ?? tipoAssunto ?? "outro_financeiro",
        `anexo de emissor conhecido (${dominio})`,
        0.8,
      );
    }
    if (promo || marketing) return descarte("anexo de e-mail promocional", 0.6);
    if (ext === "pdf") return duvida("pdf anexado sem sinal no nome nem no assunto", 0.4);
    if (EXT_OFFICE.has(ext) || EXT_COMPACTADO.has(ext) || EXT_EMAIL.has(ext)) {
      return duvida(`${ext} anexado sem sinal financeiro`, 0.3);
    }
    return descarte(`anexo ${ext || "sem extensão"} sem sinal financeiro`, 0.6);
  }

  // Mensagem bruta (.eml)
  if (boletim || /^\[?grupo /.test(assunto) || /^(re: )?\[grupo /.test(assunto)) {
    return descarte("boletim, rede social ou grupo", 0.9);
  }
  if (cobradora && PREFIXO_DE_COBRANCA.test(dominio)) {
    return doc("cobranca", `cobrança de dívida (${dominio})`, 0.6);
  }
  if (transacional && !promo && !PREFIXO_DE_MARKETING.test(dominio)) {
    if (emissor)
      return doc(tipoAssunto ?? "outro_financeiro", `e-mail transacional de ${dominio}`, 0.85);
    return duvida("assunto financeiro de remetente desconhecido", 0.5, tipoAssunto);
  }
  if (transacional && emissor && !ASSUNTO_DE_PROMOCAO.test(assunto)) {
    return duvida(
      "assunto financeiro de emissor conhecido em categoria promocional",
      0.4,
      tipoAssunto,
    );
  }
  if (promo || marketing) return descarte("promoção, rede social ou fórum", 0.85);
  if (emissor && ASSUNTO_FINANCEIRO_FRACO.test(assunto)) {
    return duvida("emissor conhecido com assunto financeiro fraco", 0.5, tipoAssunto);
  }
  if (banco) return duvida("mensagem de banco sem assunto claro", 0.4);
  if (ctx.textoNegativo) return descarte("texto lido sem termos financeiros", 0.6);
  if (ASSUNTO_FINANCEIRO_FRACO.test(assunto)) {
    return duvida("remetente desconhecido com assunto financeiro fraco", 0.35, tipoAssunto);
  }
  return descarte("e-mail sem sinal financeiro", 0.6);
}

// ---------------------------------------------------------------------------
// Triagem

function ordem(v: Veredito): number {
  return v.estado === "documento" ? 2 : v.estado === "duvida" ? 1 : 0;
}

export function triar(a: AchadoParaTriagem): Veredito {
  const ext = (a.extensao ?? "").toLowerCase();
  const origens = a.origens;
  if (origens.length === 0) {
    return duvida("achado sem origem registrada", 0.1);
  }

  const texto = a.texto?.trim() ?? "";
  const sinal = texto.length >= 30 ? sinalDoTexto(texto) : null;
  const ctx: Contexto = {
    ext,
    camada: a.camada,
    tamanho: a.file_size_bytes,
    textoNegativo: texto.length >= 200 && sinal === null,
  };

  const candidatos: Veredito[] = [];

  // O texto só vale para quem não é e-mail bruto (o corpo de newsletter cita fatura).
  const soEml = origens.every((o) => o.origin_type === "gmail" && o.mensagem_ref !== "anexo");
  if (sinal && !soEml) {
    candidatos.push(
      sinal.forte ? doc(sinal.tipo, sinal.motivo, 0.9) : duvida(sinal.motivo, 0.55, sinal.tipo),
    );
  }

  for (const o of origens) {
    const v =
      o.origin_type === "gmail"
        ? avaliarGmail(o, { ...ctx, textoNegativo: ctx.textoNegativo && o.mensagem_ref !== "eml" })
        : avaliarLocal(o, ctx);
    candidatos.push(v);
  }

  let melhor = candidatos[0]!;
  for (const v of candidatos) {
    if (
      ordem(v) > ordem(melhor) ||
      (ordem(v) === ordem(melhor) && v.confianca > melhor.confianca)
    ) {
      melhor = v;
    }
  }

  // Tipo que o texto sabe e a origem não.
  if (melhor.estado === "documento" && !melhor.document_type) {
    melhor = { ...melhor, document_type: sinal?.tipo ?? "outro_financeiro" };
  }
  return melhor;
}
