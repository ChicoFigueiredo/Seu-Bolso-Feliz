"use server";

/**
 * Os dados da tela Documentos — a caixa de entrada de boletos, faturas e
 * comprovantes.
 *
 * Esta action **não fala com o banco**. Todo acesso a dado passa pelas actions
 * de `./ingestion`, que já resolveram autenticação (`requireUser()`), filtro
 * por `ownerId` e as peculiaridades de cada tabela. O que existe aqui é a
 * outra metade do trabalho, a que nenhuma dessas actions faz: juntar documento
 * + job + rascunho + diário do pipeline numa única frase que a pessoa entende,
 * e ordenar a lista pela urgência de quem lê, não pela ordem das tabelas.
 *
 * A tradução de status para linguagem humana mora em
 * `@/components/documentos/estado-do-documento` — módulo puro, testável sem
 * banco, e compartilhado com a interface para que o rótulo do filtro e o
 * rótulo da linha nunca discordem.
 *
 * Sobre o arquivo original: ele é o único dado desta tela que não pode ser
 * reconstruído. Toda mensagem de falha diz que ele continua guardado, e o
 * detalhe sempre oferece o link para abri-lo — mesmo quando a leitura
 * automática não produziu nada.
 */
import {
  getDocumentStorageUrl,
  getDocumentWithRelations,
  getDraftRecords,
  getIngestionJobs,
  getIngestionLogs,
  getSourceDocuments,
  updateDraftData,
} from "./ingestion";
import { TransactionDraftV1Schema, parseDraftPayload } from "@sbf/contracts";
import type { DocumentState } from "@sbf/design-system";
import {
  type BaldeDeDocumento,
  ORDEM_DOS_BALDES,
  diagnosticarDocumento,
  faixaDeConfianca,
  formatarChegada,
  formatarTamanho,
  pareceProblemaDeSenha,
  rotuloDaOrigem,
} from "@/components/documentos/estado-do-documento";

/**
 * Quantos documentos a primeira leitura traz.
 *
 * Generoso de propósito. As contagens dos filtros e a frase "nada esperando por
 * você" só podem ser ditas quando a leitura cobre a caixa inteira — e a caixa
 * de uma pessoa física raramente passa disto. Quem passar vê o botão "Ver mais"
 * e as contagens somem até a leitura completar.
 */
const PAGINA = 120;

/** Teto de jobs varridos para achar o job atual de cada documento. */
const JOBS_VARRIDOS = 400;

/**
 * Teto de logs varridos para descobrir PDFs protegidos.
 *
 * O worker **não** marca o job como falho quando nenhuma senha abre o PDF:
 * ele grava um aviso no diário e segue com texto vazio. Sem ler o diário, a
 * tela mostraria "nada foi extraído" sem dizer que o que falta é a senha.
 */
const AVISOS_VARRIDOS = 200;

// ══════════════════════════════════════════════════════════════
// Caixa de entrada
// ══════════════════════════════════════════════════════════════

export interface DocumentoDaCaixa {
  id: string;
  nome: string;
  /** "Gmail · fatura@cemig.com.br", "Pasta local", "Enviado por você". */
  origem: string;
  /** "30/08 às 06:41" — já formatado, porque só serve para leitura. */
  chegada: string | null;
  tamanho: string | null;
  fornecedor: string | null;
  estado: DocumentState;
  balde: BaldeDeDocumento;
  porQue: string | null;
  oQueFazer: string | null;
  rotaDoQueFazer: string | null;
  rascunhosPendentes: number;
  /**
   * Em reais, e só quando há **um** lançamento esperando decisão.
   *
   * Com dois ou mais, qualquer número único mentiria: a soma dá a impressão de
   * um lançamento gordo, e o primeiro esconde os outros. Nesses casos a frase
   * de estado já diz quantos são, e o valor de cada um aparece ao abrir.
   */
  valor: number | null;
  /** `false` quando o arquivo foi removido do storage. */
  temOriginal: boolean;
}

export interface CaixaDeDocumentos {
  /** Já ordenados: o que depende de você primeiro. */
  documentos: DocumentoDaCaixa[];
  /** Quantos documentos existem ao todo (fora os apagados). */
  total: number;
  /**
   * `true` quando a leitura trouxe todos. Só então as contagens dos filtros
   * podem ser exibidas — um contador que conta só a página seria mentira.
   */
  completo: boolean;
  contagens: Record<BaldeDeDocumento, number>;
}

export async function carregarCaixaDeDocumentos(opcoes?: {
  limite?: number;
}): Promise<CaixaDeDocumentos> {
  const limite = Math.max(1, Math.min(opcoes?.limite ?? PAGINA, 500));

  const [documentos, jobs, pendentes, avisos] = await Promise.all([
    getSourceDocuments({ limit: limite }),
    // Um job por documento na esmagadora maioria dos casos; o fator 3 cobre os
    // reprocessamentos sem trazer o histórico inteiro do usuário. Documento sem
    // job correspondente na varredura cai no status da própria linha, que já é
    // suficiente para tudo menos distinguir o motivo de uma falha.
    getIngestionJobs({ limit: Math.min(limite * 3, JOBS_VARRIDOS) }),
    // A fila de revisão é pequena por definição: é o que ainda não foi
    // decidido. Buscar por status é mais barato que por documento — e
    // `getDraftRecords` não filtra por documento.
    getDraftRecords({ status: "pending_review", limit: 500 }),
    getIngestionLogs({ level: "warn", limit: AVISOS_VARRIDOS }),
  ]);

  // Os jobs vêm do mais novo para o mais antigo, então o primeiro de cada
  // documento é o atual.
  const jobPorDocumento = new Map<string, (typeof jobs)[number]>();
  for (const job of jobs) {
    if (!job.source_document_id) continue;
    if (!jobPorDocumento.has(job.source_document_id)) {
      jobPorDocumento.set(job.source_document_id, job);
    }
  }

  const pendentesPorDocumento = new Map<string, LinhaDeRascunho[]>();
  for (const rascunho of pendentes) {
    if (!rascunho.source_document_id) continue;
    const atuais = pendentesPorDocumento.get(rascunho.source_document_id);
    if (atuais) atuais.push(rascunho);
    else pendentesPorDocumento.set(rascunho.source_document_id, [rascunho]);
  }

  const jobsComAvisoDeSenha = new Set(
    avisos.data
      .filter((log) => log.job_id && pareceProblemaDeSenha(log.message))
      .map((log) => log.job_id as string),
  );

  const linhas: DocumentoDaCaixa[] = documentos.data.map((doc) => {
    const job = jobPorDocumento.get(doc.id) ?? null;
    const rascunhos = pendentesPorDocumento.get(doc.id) ?? [];

    const diagnostico = diagnosticarDocumento({
      statusDoDocumento: doc.status,
      statusDoJob: job?.status ?? null,
      mensagemDeErro: job?.error_message ?? null,
      precisaDeIaCompleta: job?.needs_full_ai_review ?? false,
      rascunhosPendentes: rascunhos.length,
      avisoDeSenha: job ? jobsComAvisoDeSenha.has(job.id) : false,
    });

    return {
      id: doc.id,
      nome: doc.filename,
      origem: rotuloDaOrigem(doc.origin_type, doc.gmail_from),
      chegada: formatarChegada(doc.created_at),
      tamanho: formatarTamanho(doc.file_size_bytes),
      fornecedor: doc.supplier_name_raw,
      rascunhosPendentes: rascunhos.length,
      valor: rascunhos.length === 1 && rascunhos[0] ? valorEmReais(rascunhos[0]) : null,
      temOriginal: !!doc.storage_path,
      ...diagnostico,
    };
  });

  const contagens: Record<BaldeDeDocumento, number> = {
    revisar: 0,
    processando: 0,
    concluidos: 0,
    erro: 0,
  };
  for (const linha of linhas) contagens[linha.balde] += 1;

  // Dentro de cada balde a ordem de chegada já veio do banco (mais novo
  // primeiro); o `sort` só reagrupa por urgência, e é estável no V8.
  linhas.sort((a, b) => ORDEM_DOS_BALDES.indexOf(a.balde) - ORDEM_DOS_BALDES.indexOf(b.balde));

  return {
    documentos: linhas,
    total: documentos.count,
    completo: linhas.length >= documentos.count,
    contagens,
  };
}

// ══════════════════════════════════════════════════════════════
// Detalhe de um documento
// ══════════════════════════════════════════════════════════════

/**
 * Valor de um campo extraído, ainda sem formatação.
 *
 * A action devolve número e ISO; quem formata é a interface, com `formatBRL`.
 * Assim o mesmo dado serve para exibir e para preencher o campo de correção,
 * sem passar por texto e voltar.
 */
export type ValorDeCampo =
  | { tipo: "texto"; texto: string }
  | { tipo: "dinheiro"; valor: number }
  | { tipo: "data"; iso: string };

export interface CampoExtraido {
  rotulo: string;
  valor: ValorDeCampo;
  confianca: "alta" | "provavel" | "baixa" | null;
}

export interface EdicaoDeRascunho {
  descricao: string;
  fornecedor: string;
  /** Em reais. */
  valor: number | null;
  /** ISO `YYYY-MM-DD`. */
  vencimento: string | null;
}

export interface RascunhoDoDocumento {
  id: string;
  rotuloDoTipo: string;
  rotuloDoStatus: string;
  /** Ainda espera decisão. */
  pendente: boolean;
  titulo: string;
  /** Em reais. */
  valor: number | null;
  /** ISO `YYYY-MM-DD`. */
  data: string | null;
  confianca: "alta" | "provavel" | "baixa" | null;
  /**
   * Campos editáveis. `null` quando este rascunho não pode ser corrigido por
   * aqui — ver `corrigirRascunhoDeTransacao`.
   */
  edicao: EdicaoDeRascunho | null;
  /** Por que não dá para corrigir aqui, quando `edicao` é `null`. */
  motivoSemEdicao: string | null;
}

export interface MensagemDoPipeline {
  nivel: string;
  texto: string;
  quando: string | null;
}

export interface DetalheDocumento {
  id: string;
  nome: string;
  origem: string;
  chegada: string | null;
  tamanho: string | null;
  formato: string | null;
  estado: DocumentState;
  balde: BaldeDeDocumento;
  porQue: string | null;
  oQueFazer: string | null;
  rotaDoQueFazer: string | null;
  /**
   * Link temporário (1 h) para o arquivo exatamente como chegou. `null` só
   * quando o arquivo não está mais no storage — e nesse caso a interface
   * precisa dizer isso, não sumir com o botão em silêncio.
   */
  urlDoOriginal: string | null;
  /** `false` quando o documento não tem mais arquivo guardado. */
  temOriginal: boolean;
  campos: CampoExtraido[];
  /** O que o próprio sistema diz que ficou faltando. */
  pendencias: string[];
  rascunhos: RascunhoDoDocumento[];
  /** Últimas linhas do diário do pipeline — só avisos e erros. */
  mensagens: MensagemDoPipeline[];
}

const ROTULO_DO_TIPO_DE_RASCUNHO: Record<string, string> = {
  transaction: "Lançamento",
  recurring_template: "Recorrência",
  liability: "Dívida",
  consumption_metric: "Consumo",
};

const ROTULO_DO_STATUS_DE_RASCUNHO: Record<string, string> = {
  pending_review: "Aguardando você",
  approved: "Aprovado",
  posted: "Lançado",
  rejected: "Rejeitado",
  corrected: "Corrigido por você",
  archived: "Arquivado",
};

const FORMATO_LEGIVEL: Record<string, string> = {
  "application/pdf": "PDF",
  "text/csv": "CSV",
  "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet": "Excel",
  "application/x-ofx": "OFX",
  "application/xml": "XML",
  "text/xml": "XML",
  "image/png": "Imagem PNG",
  "image/jpeg": "Imagem JPEG",
  "image/webp": "Imagem WebP",
};

export async function carregarDetalheDocumento(id: string): Promise<DetalheDocumento | null> {
  const relacoes = await getDocumentWithRelations(id);
  if (!relacoes) return null;

  const { document, jobs, drafts } = relacoes;
  const job = jobs[0] ?? null;

  const [urlDoOriginal, diario] = await Promise.all([
    document.storage_path ? getDocumentStorageUrl(document.storage_path) : Promise.resolve(null),
    job ? getIngestionLogs({ jobId: job.id, limit: 40 }) : Promise.resolve(null),
  ]);

  const mensagens: MensagemDoPipeline[] = (diario?.data ?? [])
    .filter((log) => log.level === "warn" || log.level === "error")
    .slice(0, 6)
    .map((log) => ({
      nivel: log.level,
      texto: log.message,
      quando: formatarChegada(log.created_at),
    }));

  const avisoDeSenha = mensagens.some((m) => pareceProblemaDeSenha(m.texto));
  const rascunhosPendentes = drafts.filter((d) => d.status === "pending_review").length;

  const diagnostico = diagnosticarDocumento({
    statusDoDocumento: document.status,
    statusDoJob: job?.status ?? null,
    mensagemDeErro: job?.error_message ?? null,
    precisaDeIaCompleta: job?.needs_full_ai_review ?? false,
    rascunhosPendentes,
    avisoDeSenha,
  });

  // Confiança por campo mora no `extraction_result` que o job apontou. Nem
  // todo documento tem um; quando não tem, os campos aparecem sem pílula em
  // vez de com uma confiança inventada.
  const confiancaPorCampo = confiancaDoJob(job?.metadata ?? null);

  const rascunhos = drafts.map((linha) => descreverRascunho(linha, confiancaPorCampo));
  const campos = camposDoPrimeiroRascunho(rascunhos, confiancaPorCampo, document.supplier_name_raw);

  return {
    id: document.id,
    nome: document.filename,
    origem: rotuloDaOrigem(document.origin_type, document.gmail_from),
    chegada: formatarChegada(document.created_at),
    tamanho: formatarTamanho(document.file_size_bytes),
    formato: document.mime_type ? (FORMATO_LEGIVEL[document.mime_type] ?? null) : null,
    urlDoOriginal,
    temOriginal: !!document.storage_path,
    campos,
    pendencias: pendenciasDoDocumento(rascunhos, diagnostico.estado),
    rascunhos,
    mensagens,
    ...diagnostico,
  };
}

/**
 * `ingestion_jobs.metadata.confidence_per_field`, quando o consenso por campo
 * chegou a gravá-lo. Formato tolerante de propósito: é um `jsonb` livre.
 */
function confiancaDoJob(metadata: unknown): Record<string, number> {
  if (!metadata || typeof metadata !== "object") return {};
  const bruto = (metadata as Record<string, unknown>).confidence_per_field;
  if (!bruto || typeof bruto !== "object") return {};
  const saida: Record<string, number> = {};
  for (const [chave, valor] of Object.entries(bruto as Record<string, unknown>)) {
    if (typeof valor === "number" && Number.isFinite(valor)) saida[chave] = valor;
  }
  return saida;
}

type LinhaDeRascunho = NonNullable<
  Awaited<ReturnType<typeof getDocumentWithRelations>>
>["drafts"][number];

/**
 * O valor de um rascunho, em reais — ou `null` quando ele não tem valor.
 *
 * Os contratos guardam dinheiro em centavos inteiros de propósito (ver
 * `MoneyCentsSchema`); a divisão por 100 acontece aqui e em `descreverRascunho`,
 * nunca no componente, para que a interface nunca receba centavos achando que
 * são reais.
 */
function valorEmReais(linha: LinhaDeRascunho): number | null {
  const lido = parseDraftPayload({
    draft_type: linha.draft_type,
    draft_data: linha.draft_data,
    draft_schema_version: linha.draft_schema_version,
  });
  if (!lido.ok) return null;

  const p = lido.payload;
  const centavos =
    p.draft_type === "transaction" || p.draft_type === "recurring_template"
      ? p.amount_cents
      : p.draft_type === "liability"
        ? (p.outstanding_balance_cents ?? p.original_amount_cents)
        : p.subtotal_cents;

  return centavos == null ? null : centavos / 100;
}

/**
 * Traduz uma linha de `draft_records` no que a folha de detalhe mostra.
 *
 * A leitura passa por `parseDraftPayload`, que sabe migrar rascunhos v0 em
 * memória. Um rascunho migrado é exibido normalmente, mas **não** pode ser
 * corrigido daqui: `updateDraftData` não consegue subir
 * `draft_schema_version`, então gravar um payload v1 numa linha marcada v0
 * deixaria a próxima leitura tentando migrar o que já está migrado.
 */
function descreverRascunho(
  linha: LinhaDeRascunho,
  confiancaPorCampo: Record<string, number>,
): RascunhoDoDocumento {
  const base = {
    id: linha.id,
    rotuloDoTipo: ROTULO_DO_TIPO_DE_RASCUNHO[linha.draft_type] ?? linha.draft_type,
    rotuloDoStatus: ROTULO_DO_STATUS_DE_RASCUNHO[linha.status] ?? linha.status,
    pendente: linha.status === "pending_review" || linha.status === "corrected",
    confianca: faixaDeConfianca(linha.confidence_score),
  };

  const lido = parseDraftPayload({
    draft_type: linha.draft_type,
    draft_data: linha.draft_data,
    draft_schema_version: linha.draft_schema_version,
  });

  if (!lido.ok) {
    return {
      ...base,
      titulo: "Não foi possível ler este rascunho",
      valor: null,
      data: null,
      edicao: null,
      motivoSemEdicao: `O rascunho está fora do formato esperado (${lido.errors.slice(0, 2).join("; ")}).`,
    };
  }

  const payload = lido.payload;

  if (payload.draft_type === "transaction") {
    const valor = payload.amount_cents == null ? null : payload.amount_cents / 100;
    const data = payload.due_date ?? payload.event_date ?? payload.competence_date ?? null;
    return {
      ...base,
      titulo: payload.description ?? payload.supplier_name_raw ?? "Lançamento sem descrição",
      valor,
      data,
      confianca: base.confianca ?? faixaDeConfianca(confiancaPorCampo.total_amount),
      // Migrado (v0) é legível mas não regravável — ver o comentário acima.
      edicao: lido.migrated
        ? null
        : {
            descricao: payload.description ?? "",
            fornecedor: payload.supplier_name_raw ?? "",
            valor,
            vencimento: payload.due_date,
          },
      motivoSemEdicao: lido.migrated
        ? "Este rascunho está no formato antigo. Aprove, rejeite ou mande reprocessar — corrigir por aqui gravaria num formato que a próxima leitura não entenderia."
        : null,
    };
  }

  if (payload.draft_type === "recurring_template") {
    return {
      ...base,
      titulo: payload.name ?? payload.supplier_name_raw ?? "Recorrência sem nome",
      valor: payload.amount_cents == null ? null : payload.amount_cents / 100,
      data: payload.starts_at,
      edicao: null,
      motivoSemEdicao: "Recorrências se corrigem na tela de Recorrentes, depois de aprovadas.",
    };
  }

  if (payload.draft_type === "liability") {
    return {
      ...base,
      titulo: payload.name ?? payload.supplier_name_raw ?? "Dívida sem nome",
      valor:
        payload.outstanding_balance_cents == null
          ? payload.original_amount_cents == null
            ? null
            : payload.original_amount_cents / 100
          : payload.outstanding_balance_cents / 100,
      data: payload.start_date,
      edicao: null,
      motivoSemEdicao: "Dívidas se corrigem na tela de Dívidas, depois de aprovadas.",
    };
  }

  return {
    ...base,
    titulo: [payload.metric_name, payload.quantity, payload.metric_unit]
      .filter((p) => p != null && p !== "")
      .join(" ")
      .trim(),
    valor: payload.subtotal_cents == null ? null : payload.subtotal_cents / 100,
    data: payload.reference_period_end,
    edicao: null,
    motivoSemEdicao: "Medições de consumo não são editadas aqui.",
  };
}

/**
 * Os campos que a folha mostra sob "o que o sistema leu".
 *
 * Vêm do primeiro rascunho e não do `extraction_result` porque o rascunho é o
 * que a pessoa vai aprovar — mostrar um número diferente do que será gravado
 * seria pior do que não mostrar nada.
 */
function camposDoPrimeiroRascunho(
  rascunhos: RascunhoDoDocumento[],
  confiancaPorCampo: Record<string, number>,
  fornecedorDoDocumento: string | null,
): CampoExtraido[] {
  const principal = rascunhos.find((r) => r.pendente) ?? rascunhos[0];
  const campos: CampoExtraido[] = [];

  const fornecedor = principal?.edicao?.fornecedor || fornecedorDoDocumento;
  if (fornecedor) {
    campos.push({
      rotulo: "Fornecedor",
      valor: { tipo: "texto", texto: fornecedor },
      confianca: faixaDeConfianca(confiancaPorCampo.supplier_name_raw),
    });
  }

  if (principal?.titulo && principal.titulo !== fornecedor) {
    campos.push({
      rotulo: "Descrição",
      valor: { tipo: "texto", texto: principal.titulo },
      confianca: null,
    });
  }

  if (principal?.valor != null) {
    campos.push({
      rotulo: "Valor",
      valor: { tipo: "dinheiro", valor: principal.valor },
      confianca: principal.confianca ?? faixaDeConfianca(confiancaPorCampo.total_amount),
    });
  }

  if (principal?.data) {
    campos.push({
      rotulo: "Vencimento",
      valor: { tipo: "data", iso: principal.data },
      confianca: faixaDeConfianca(confiancaPorCampo.due_date),
    });
  }

  return campos;
}

/** O que ficou faltando, na voz do produto — não na do pipeline. */
function pendenciasDoDocumento(rascunhos: RascunhoDoDocumento[], estado: DocumentState): string[] {
  const pendencias: string[] = [];
  const principal = rascunhos.find((r) => r.pendente) ?? rascunhos[0];

  if (estado === "protegido-por-senha") return pendencias;

  if (!principal) {
    pendencias.push("Nenhum lançamento foi proposto a partir deste arquivo.");
    return pendencias;
  }
  if (principal.valor == null) pendencias.push("O valor não foi identificado.");
  if (!principal.data) pendencias.push("A data de vencimento não foi identificada.");
  if (!principal.edicao?.fornecedor) pendencias.push("O fornecedor não foi identificado.");
  return pendencias;
}

// ══════════════════════════════════════════════════════════════
// Correção
// ══════════════════════════════════════════════════════════════

export interface CorrecaoDeRascunho {
  documentoId: string;
  rascunhoId: string;
  descricao: string;
  fornecedor: string;
  /** Em reais. `null` apaga o valor. */
  valor: number | null;
  /** ISO `YYYY-MM-DD`. `null` apaga o vencimento. */
  vencimento: string | null;
}

/**
 * Grava a correção humana de um rascunho de lançamento.
 *
 * O rascunho é relido pelo caminho autorizado (`getDocumentWithRelations`
 * filtra por dono) e o payload corrigido passa pelo schema antes de ir para o
 * banco: o que se grava aqui é o que a materialização vai ler, e um payload
 * inválido só apareceria como erro três telas adiante.
 *
 * `updateDraftData` marca a linha como `corrected` e registra que houve edição
 * manual — a origem do dado continua rastreável depois da correção.
 */
export async function corrigirRascunhoDeTransacao(entrada: CorrecaoDeRascunho): Promise<void> {
  const relacoes = await getDocumentWithRelations(entrada.documentoId);
  if (!relacoes) throw new Error("Documento não encontrado");

  const linha = relacoes.drafts.find((d) => d.id === entrada.rascunhoId);
  if (!linha) throw new Error("Rascunho não encontrado neste documento");
  if (linha.draft_type !== "transaction") {
    throw new Error("Só rascunhos de lançamento podem ser corrigidos por aqui");
  }

  const lido = parseDraftPayload({
    draft_type: linha.draft_type,
    draft_data: linha.draft_data,
    draft_schema_version: linha.draft_schema_version,
  });
  if (!lido.ok) throw new Error(`Rascunho ilegível: ${lido.errors.join("; ")}`);
  if (lido.migrated) {
    throw new Error(
      "Este rascunho está no formato antigo e não pode ser corrigido por aqui sem perder a marcação de versão.",
    );
  }
  if (lido.payload.draft_type !== "transaction") {
    throw new Error("Só rascunhos de lançamento podem ser corrigidos por aqui");
  }

  const descricao = entrada.descricao.trim();
  const fornecedor = entrada.fornecedor.trim();

  const corrigido = TransactionDraftV1Schema.parse({
    ...lido.payload,
    description: descricao === "" ? null : descricao,
    supplier_name_raw: fornecedor === "" ? null : fornecedor,
    amount_cents: entrada.valor == null ? null : Math.round(entrada.valor * 100),
    due_date: entrada.vencimento === "" ? null : entrada.vencimento,
  });

  await updateDraftData(entrada.rascunhoId, corrigido);
}
