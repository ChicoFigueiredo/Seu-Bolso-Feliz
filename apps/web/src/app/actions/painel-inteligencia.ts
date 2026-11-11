"use server";

/**
 * Os números da tela Inteligência: onde a IA presta contas.
 *
 * Três perguntas, três blocos de dados:
 *
 *   1. **O que ela ainda quer confirmar** — `fila`: os `draft_records` em
 *      `pending_review`/`corrected` do dono, do mais antigo para o mais novo
 *      (mesma ordem que `idx_draft_records_review_queue` otimiza — a fila é
 *      FIFO de propósito, para nada envelhecer esquecido). A humanização
 *      (título, frase, rótulo de confiança) é montada AQUI, no servidor, e
 *      não na view: assim ela ganha cobertura dos testes de integração deste
 *      arquivo, e a view fica só com `formatBRL` — a única parte que, por
 *      convenção do repo (ver painel-hoje.ts), fica do lado do cliente.
 *      Valores monetários viajam em reais (não centavos): quem grava em
 *      centavos é `@sbf/contracts` (`MoneyCentsSchema`), e é o formato salvo
 *      em `draft_data`.
 *
 *   2. **O que ela aprendeu** — `padroes`: composição de `listPatterns()`
 *      (patterns.ts já filtra por dono) com a mesma fórmula de taxa de
 *      sucesso e critério de "em risco" da tela /dashboard/ingestion/patterns
 *      — dois lugares, uma fórmula, para os números nunca discordarem.
 *
 *   3. **Como ela está indo** — `desempenho`: reaproveita `getIngestionStats()`
 *      e soma um recorte próprio de `draft_records` já concluídos (aprovados,
 *      lançados ou rejeitados) para separar "acertou sozinha" de "precisou de
 *      correção". O sinal de correção é `corrections.manual_edit === true`,
 *      gravado por `updateDraftData` (ver ingestion.ts) — sobrevive à
 *      aprovação porque `approveDraftRecord` só toca `status`/`approved_*`,
 *      nunca `corrections`. Sem essa marca, "aprovado" e "aprovado depois de
 *      eu corrigir tudo" ficariam indistinguíveis, e o número deixaria de ser
 *      honesto.
 */
import { requireUser } from "@/lib/auth";
import { getDb, schema } from "@sbf/db";
import { and, asc, count, eq, inArray } from "drizzle-orm";
import { approveDraftRecord, getIngestionStats, rejectDraftRecord } from "./ingestion";
import { listPatterns } from "./patterns";

/** Status que ainda pedem uma decisão do usuário. */
const STATUS_FILA = ["pending_review", "corrected"] as const;
/** Status que já tiveram uma decisão — usados para medir acerto/correção/rejeição. */
const STATUS_CONCLUIDOS = ["approved", "posted", "rejected"] as const;
/** Teto de itens carregados por vez: uma tela de revisão, não uma exportação. */
const LIMITE_FILA = 50;

export interface ItemFila {
  id: string;
  draftType: string;
  /** Glifo Material Symbols já escolhido para o tipo de draft. */
  icone: string;
  /** Nome/descrição já resolvido entre os campos disponíveis. */
  nome: string;
  /** Valor em reais. `null` quando a IA não extraiu valor nenhum. */
  valor: number | null;
  /** Frase pronta com o resto do contexto — sem valor monetário embutido. */
  detalhe: string;
  /** Sempre um texto, nunca só uma cor — "Confiança baixa · 42%". */
  confidenceLabel: string;
  /** Para onde mandar quem quer corrigir em vez de só confirmar/rejeitar. */
  linkCorrigir: string | null;
  criadoEm: string;
}

export interface PadraoAprendido {
  id: string;
  nome: string;
  tipoDocumento: string;
  ativo: boolean;
  acertos: number;
  correcoes: number;
  taxaSucessoPct: number;
  /** Mesmo critério da tela /dashboard/ingestion/patterns: ativo, com histórico, taxa < 60%. */
  emRisco: boolean;
  atualizadoEm: string;
}

export interface DesempenhoIA {
  totalDocumentos: number;
  /** aprovados + lançados + rejeitados: documentos com decisão tomada. */
  processados: number;
  semCorrecao: number;
  comCorrecao: number;
  rejeitados: number;
  /** Igual a `filaTotalPendente` — repetido aqui para o bloco de números ficar autocontido. */
  pendentes: number;
  /** Falhas de pipeline nas últimas 24h (getIngestionStats). Sem essas 24h, o número seria enganoso. */
  erros24h: number;
  /** `null` quando não há nenhum documento processado ainda — não existe taxa de 0 itens. */
  taxaSemCorrecaoPct: number | null;
}

export interface PainelInteligencia {
  fila: ItemFila[];
  /** Pode ser maior que `fila.length` quando o total excede `LIMITE_FILA`. */
  filaTotalPendente: number;
  padroes: PadraoAprendido[];
  desempenho: DesempenhoIA;
}

export interface ResultadoLote {
  ok: string[];
  falhas: { id: string; erro: string }[];
}

const num = (v: unknown): number => (v == null ? 0 : Number(v));

/** Mesma fórmula de /dashboard/ingestion/patterns/page.tsx: sem histórico, 100%. */
function calcularTaxaSucesso(acertos: number, correcoes: number): number {
  const total = acertos + correcoes;
  if (total === 0) return 100;
  return Math.round((acertos / total) * 100);
}

/**
 * Data curta pt-BR a partir de um `YYYY-MM-DD`.
 *
 * Não usa `formatDate` de `@sbf/design-system` de propósito: aquele helper
 * faz `new Date(iso)`, que o motor JS lê como meia-noite UTC, e um fuso atrás
 * de UTC (o do Brasil) devolve o dia anterior. Meio-dia local não tem esse
 * problema — mesmo truque de `dataCurta` em painel-hoje-view.tsx, só que
 * aplicado aqui porque esta função roda no servidor, não na view.
 */
function dataCurtaLocal(iso: string): string {
  const d = new Date(`${iso}T12:00:00`);
  return d.toLocaleDateString("pt-BR", { day: "2-digit", month: "2-digit" });
}

const ICONE_POR_TIPO: Record<string, string> = {
  transaction: "receipt_long",
  liability: "account_balance",
  recurring_template: "autorenew",
  consumption_metric: "bolt",
};

/**
 * Escolhe os campos que importam de `draft_data` (formato varia por
 * `draft_type` — contratos em `packages/contracts/src/draft/*.ts`) e monta
 * uma frase honesta. `valor` sai em reais para a view formatar com
 * `formatBRL`; o resto do texto nunca precisa de formatação monetária, então
 * pode ser montado aqui mesmo.
 */
function resumirDraft(
  draftType: string,
  d: Record<string, unknown>,
): { nome: string; valor: number | null; detalhe: string } {
  const texto = (v: unknown): string | null =>
    typeof v === "string" && v.trim().length > 0 ? v.trim() : null;
  const centavosParaReais = (v: unknown): number | null =>
    typeof v === "number" && Number.isFinite(v) ? v / 100 : null;
  const dataValida = (v: unknown): string | null =>
    typeof v === "string" && /^\d{4}-\d{2}-\d{2}/.test(v) ? v.slice(0, 10) : null;

  switch (draftType) {
    case "transaction": {
      // `direction` ausente não é "saída" por padrão — é desconhecido. Fingir
      // uma direção que a extração não afirmou é o tipo de falsa certeza que
      // este produto proíbe.
      const direcao =
        d.direction === "inflow"
          ? "uma entrada"
          : d.direction === "outflow"
            ? "uma saída"
            : "uma movimentação";
      const categoria = texto(d.category_suggestion);
      const data = dataValida(d.event_date) ?? dataValida(d.due_date);
      return {
        nome: texto(d.description) ?? texto(d.supplier_name_raw) ?? "Lançamento sem descrição",
        valor: centavosParaReais(d.amount_cents),
        detalhe: `A IA identificou ${direcao}${data ? ` em ${dataCurtaLocal(data)}` : ""}${
          categoria ? `, categoria sugerida "${categoria}"` : ""
        }.`,
      };
    }
    case "liability": {
      const parcelas = typeof d.total_installments === "number" ? d.total_installments : null;
      return {
        nome: texto(d.name) ?? texto(d.supplier_name_raw) ?? "Possível dívida nova",
        valor: centavosParaReais(d.original_amount_cents),
        detalhe: parcelas
          ? `Dívida em ${parcelas} parcela${parcelas === 1 ? "" : "s"}, extraída do documento.`
          : "Dívida extraída do documento — confirme os dados antes de registrar.",
      };
    }
    case "recurring_template": {
      const evidencias = typeof d.evidence_count === "number" ? d.evidence_count : null;
      // `explanation` é a própria IA documentando por que acredita na
      // recorrência (§8 do plano mestre da ingestão) — a fonte mais direta de
      // "prestação de contas" que existe no payload.
      const explicacao = Array.isArray(d.explanation)
        ? d.explanation.filter((x): x is string => typeof x === "string")
        : [];
      return {
        nome: texto(d.name) ?? texto(d.supplier_name_raw) ?? "Possível recorrência",
        valor: centavosParaReais(d.amount_cents),
        detalhe:
          explicacao.length > 0
            ? explicacao.join(" ")
            : evidencias != null
              ? `Repetição observada em ${evidencias} lançamento${evidencias === 1 ? "" : "s"}.`
              : "Padrão de repetição observado nos seus lançamentos.",
      };
    }
    case "consumption_metric": {
      const qtd = typeof d.quantity === "number" ? d.quantity : null;
      const unidade = texto(d.metric_unit);
      return {
        nome: texto(d.supplier_name_raw) ?? "Consumo medido",
        valor: centavosParaReais(d.subtotal_cents),
        detalhe:
          qtd != null && unidade
            ? `Leitura de ${qtd} ${unidade} (${texto(d.metric_name) ?? "medição"}) extraída do documento.`
            : "Leitura de consumo extraída do documento.",
      };
    }
    default:
      // O CHECK do banco só aceita os quatro tipos acima; este ramo é rede de
      // segurança para um `draft_type` novo que a UI ainda não conhece.
      return {
        nome: "Item extraído pela IA",
        valor: null,
        detalhe: "Confira os dados antes de aprovar.",
      };
  }
}

/**
 * A confiança em palavras, nunca só um número. `0.8` é o mesmo corte de
 * `CONFIDENCE_THRESHOLD` em `components/ai-field-badge.tsx` — o "bom o
 * suficiente" já usado no resto do produto.
 */
function rotuloConfianca(score: number | null): string {
  if (score == null) return "Confiança não informada";
  const pct = Math.round(score * 100);
  if (score >= 0.8) return `Confiança alta · ${pct}%`;
  if (score >= 0.5) return `Confiança média · ${pct}%`;
  return `Confiança baixa · ${pct}%`;
}

export async function carregarPainelInteligencia(): Promise<PainelInteligencia> {
  const { ownerId } = await requireUser();
  const db = getDb();

  const [filaRows, [totalFilaRow], concluidos, stats, { data: padroesRaw }] = await Promise.all([
    db
      .select({
        id: schema.draft_records.id,
        draft_type: schema.draft_records.draft_type,
        draft_data: schema.draft_records.draft_data,
        confidence_score: schema.draft_records.confidence_score,
        corrections: schema.draft_records.corrections,
        batch_id: schema.draft_records.batch_id,
        source_document_id: schema.draft_records.source_document_id,
        created_at: schema.draft_records.created_at,
        filename: schema.source_documents.filename,
      })
      .from(schema.draft_records)
      .leftJoin(
        schema.source_documents,
        and(
          eq(schema.source_documents.id, schema.draft_records.source_document_id),
          eq(schema.source_documents.user_id, ownerId),
        ),
      )
      .where(
        and(
          eq(schema.draft_records.user_id, ownerId),
          inArray(schema.draft_records.status, [...STATUS_FILA]),
        ),
      )
      .orderBy(asc(schema.draft_records.created_at))
      .limit(LIMITE_FILA),

    db
      .select({ value: count() })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.user_id, ownerId),
          inArray(schema.draft_records.status, [...STATUS_FILA]),
        ),
      ),

    db
      .select({
        status: schema.draft_records.status,
        corrections: schema.draft_records.corrections,
      })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.user_id, ownerId),
          inArray(schema.draft_records.status, [...STATUS_CONCLUIDOS]),
        ),
      ),

    getIngestionStats(),
    listPatterns({ limit: 100 }),
  ]);

  // ── Desempenho: sem correção vs. com correção vs. rejeitado ─────────────
  let semCorrecao = 0;
  let comCorrecao = 0;
  let rejeitados = 0;
  for (const row of concluidos) {
    if (row.status === "rejected") {
      rejeitados++;
      continue;
    }
    const corr = row.corrections as Record<string, unknown> | null;
    if (corr && corr["manual_edit"] === true) comCorrecao++;
    else semCorrecao++;
  }
  const processados = semCorrecao + comCorrecao;
  const totalPendente = num(totalFilaRow?.value);

  // ── Padrões aprendidos ───────────────────────────────────────────────────
  const padroes: PadraoAprendido[] = padroesRaw.map((p) => {
    const taxa = calcularTaxaSucesso(p.success_count, p.feedback_count);
    return {
      id: p.id,
      nome: p.name,
      tipoDocumento: p.document_type,
      ativo: p.is_active,
      acertos: p.success_count,
      correcoes: p.feedback_count,
      taxaSucessoPct: taxa,
      emRisco: p.is_active && p.feedback_count > 1 && taxa < 60,
      atualizadoEm: p.updated_at,
    };
  });

  // ── Fila de confirmação ──────────────────────────────────────────────────
  const fila: ItemFila[] = filaRows.map((r) => {
    const dados = (r.draft_data ?? {}) as Record<string, unknown>;
    const { nome, valor, detalhe: detalheBase } = resumirDraft(r.draft_type, dados);
    const corr = r.corrections as Record<string, unknown> | null;
    const foiCorrigido = Boolean(corr && corr["manual_edit"] === true);

    const partes = [detalheBase];
    if (r.filename) partes.push(`Extraído de "${r.filename}".`);
    if (foiCorrigido) partes.push("Você já corrigiu este item; falta só confirmar.");
    if (r.confidence_score != null && r.confidence_score < 0.5) {
      partes.push("Confira os dados originais antes de confirmar.");
    }

    return {
      id: r.id,
      draftType: r.draft_type,
      icone: ICONE_POR_TIPO[r.draft_type] ?? "auto_awesome",
      nome,
      valor,
      detalhe: partes.join(" "),
      confidenceLabel: rotuloConfianca(r.confidence_score),
      linkCorrigir: r.batch_id
        ? `/dashboard/ingestion/review/${r.batch_id}`
        : r.source_document_id
          ? `/dashboard/documents/${r.source_document_id}`
          : null,
      criadoEm: r.created_at ?? "",
    };
  });

  return {
    fila,
    filaTotalPendente: totalPendente,
    padroes,
    desempenho: {
      totalDocumentos: stats.totalDocuments,
      processados,
      semCorrecao,
      comCorrecao,
      rejeitados,
      pendentes: totalPendente,
      erros24h: stats.recentErrors,
      taxaSemCorrecaoPct: processados > 0 ? Math.round((semCorrecao / processados) * 100) : null,
    },
  };
}

/**
 * Aprova/rejeita vários drafts de uma vez, reaproveitando
 * `approveDraftRecord`/`rejectDraftRecord` (ingestion.ts) — que já filtram
 * por `ownerId` — em vez de duplicar a lógica de posse aqui. Sequencial, não
 * `Promise.all`: um id de outro usuário deve virar UMA falha isolada, nunca
 * abortar os demais nem exigir `Promise.allSettled` para descobrir qual foi.
 */
async function processarLote(
  ids: string[],
  acao: (id: string) => Promise<unknown>,
): Promise<ResultadoLote> {
  const ok: string[] = [];
  const falhas: { id: string; erro: string }[] = [];
  for (const id of ids) {
    try {
      await acao(id);
      ok.push(id);
    } catch (err) {
      falhas.push({ id, erro: err instanceof Error ? err.message : "Erro desconhecido" });
    }
  }
  return { ok, falhas };
}

export async function aprovarItensDaFila(ids: string[]): Promise<ResultadoLote> {
  return processarLote(ids, approveDraftRecord);
}

export async function rejeitarItensDaFila(ids: string[]): Promise<ResultadoLote> {
  return processarLote(ids, rejectDraftRecord);
}
