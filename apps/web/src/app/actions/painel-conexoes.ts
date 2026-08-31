"use server";

/**
 * Os dados da tela Conexões — o caminho de entrada do produto.
 *
 * Esta tela responde três perguntas, nesta ordem: (1) por onde o dinheiro
 * entra no app, (2) o que está quebrado agora, (3) o que ainda depende de uma
 * decisão minha. Tudo aqui é leitura; as escritas continuam nas actions que
 * já existem (`pluggy-connections.ts`), que esta action deliberadamente NÃO
 * reimplementa.
 *
 * Três decisões que valem o comentário:
 *
 *   1. **`pluggyConfigurado` é um booleano, nunca o segredo.** `PLUGGY_CLIENT_ID`
 *      e `PLUGGY_CLIENT_SECRET` são lidos aqui, no servidor, só para dizer à
 *      tela se dá para clicar em "Conectar banco". O valor nunca atravessa —
 *      quem fala com a Pluggy é `createPluggyConnectToken`, e o único token
 *      que chega ao navegador é o de Connect, de 30 minutos.
 *
 *   2. **"Sincronizando" não é um status do banco.** O CHECK de
 *      `provider_connections.status` só admite active/error/revoked/expired.
 *      O estado "importando histórico" é derivado de um checkpoint Pluggy vivo
 *      (`ingestion_checkpoints`, `scope_key = "<itemId>:<contaId>"`, gravado
 *      pelo backfill em workers/pluggy-sync/src/checkpoint.ts). É dado real,
 *      não um estado inventado na UI.
 *
 *   3. **Contagem de documentos por origem vem de `ingestion_jobs`, não do
 *      `stats` jsonb.** `stats` tem forma diferente em cada worker (o Gmail
 *      grava `jobsCreated`, a pasta local grava `discovered`); contar as
 *      linhas da própria run é o mesmo número sem depender do formato.
 */

import { getDb, schema } from "@sbf/db";
import { and, asc, desc, eq, inArray, like, sql } from "drizzle-orm";
import { requireUser } from "@/lib/auth";

/** Mesmos valores do CHECK `provider_connections_status_check`. */
export type StatusConexao = "active" | "error" | "revoked" | "expired";

/** Mesmos valores do CHECK `external_account_mappings_status_check`. */
export type StatusContaExterna = "pending_mapping" | "mapped" | "ignored";

/** Mesmos valores do CHECK `ingestion_runs_status_check`. */
export type StatusVarredura = "running" | "completed" | "failed" | "cancelled";

export interface ContaExterna {
  id: string;
  /** Nome como a instituição chama a conta — "Conta Corrente", "Cartão Platinum". */
  nome: string | null;
  tipo: string | null;
  status: StatusContaExterna;
  /** Produto financeiro interno ao qual a conta foi vinculada. */
  produtoId: string | null;
  produtoNome: string | null;
  produtoSaldo: number | null;
}

export interface Conexao {
  id: string;
  /** `provider_connections.external_item_id` — o item na Pluggy, usado na reautenticação. */
  itemId: string;
  instituicao: string | null;
  status: StatusConexao;
  /** Backfill de histórico em andamento nesta conexão (ver nota 2 no topo). */
  importandoHistorico: boolean;
  ultimaSincronizacao: string | null;
  conectadaEm: string | null;
  contas: ContaExterna[];
}

export interface ProdutoOpcao {
  id: string;
  nome: string;
  tipo: string;
  instituicao: string | null;
  saldo: number;
}

export interface OrigemDocumentos {
  id: "gmail" | "local_file";
  /** Houve alguma varredura registrada — se não, a origem nunca foi configurada. */
  jaRodou: boolean;
  ultimaVarredura: string | null;
  status: StatusVarredura | null;
  /** Documentos que a última varredura colocou na fila. */
  documentos: number;
  /** `ingestion_runs.metadata` da última varredura: rótulo do Gmail, pasta escaneada. */
  detalhe: string | null;
}

export interface PainelConexoes {
  /** Só o fato de as credenciais existirem no servidor. Nunca o valor delas. */
  pluggyConfigurado: boolean;
  conexoes: Conexao[];
  produtos: ProdutoOpcao[];
  /** Contas externas de conexões vivas ainda sem decisão do usuário. */
  contasAguardandoVinculo: number;
  /** Movimentações do Open Finance esperando revisão. */
  movimentacoesParaRevisar: number;
  origens: OrigemDocumentos[];
}

const num = (v: unknown) => (v == null ? 0 : Number(v));

/**
 * `status` chega do banco como `text`. O CHECK garante o conjunto, mas o tipo
 * do Drizzle é `string` — estas duas funções fazem o estreitamento explícito
 * em vez de um cast cego, e caem no valor mais conservador se um dia o CHECK
 * mudar sem esta tela saber.
 */
function statusDeConexao(valor: string): StatusConexao {
  return valor === "active" || valor === "error" || valor === "revoked" || valor === "expired"
    ? valor
    : "error";
}

function statusDeContaExterna(valor: string): StatusContaExterna {
  return valor === "mapped" || valor === "ignored" ? valor : "pending_mapping";
}

function statusDeVarredura(valor: string): StatusVarredura {
  return valor === "completed" || valor === "failed" || valor === "cancelled" ? valor : "running";
}

/** Texto curto do `metadata` da varredura, quando ele diz algo útil. */
function detalheDaVarredura(origem: "gmail" | "local_file", metadata: unknown): string | null {
  if (typeof metadata !== "object" || metadata === null) return null;
  const dados = metadata as Record<string, unknown>;
  if (origem === "gmail") {
    const rotulo = dados.label;
    return typeof rotulo === "string" && rotulo ? `Rótulo ${rotulo}` : null;
  }
  const pasta = dados.directory;
  return typeof pasta === "string" && pasta ? pasta : null;
}

export async function carregarPainelConexoes(): Promise<PainelConexoes> {
  const { ownerId } = await requireUser();
  const db = getDb();

  const [conexoesBrutas, produtosBrutos, checkpoints, revisao] = await Promise.all([
    db
      .select({
        id: schema.provider_connections.id,
        itemId: schema.provider_connections.external_item_id,
        instituicao: schema.provider_connections.institution_name,
        status: schema.provider_connections.status,
        ultimaSincronizacao: schema.provider_connections.last_synced_at,
        conectadaEm: schema.provider_connections.created_at,
      })
      .from(schema.provider_connections)
      .where(eq(schema.provider_connections.user_id, ownerId))
      .orderBy(desc(schema.provider_connections.created_at)),

    db
      .select({
        id: schema.financial_products.id,
        nome: schema.financial_products.name,
        tipo: schema.financial_products.type,
        saldo: schema.financial_products.current_balance,
        instituicao: schema.institutions.name,
      })
      .from(schema.financial_products)
      .leftJoin(
        schema.institutions,
        and(
          eq(schema.institutions.id, schema.financial_products.institution_id),
          eq(schema.institutions.user_id, ownerId),
        ),
      )
      .where(
        and(
          eq(schema.financial_products.user_id, ownerId),
          eq(schema.financial_products.is_active, true),
        ),
      )
      .orderBy(asc(schema.financial_products.display_order), asc(schema.financial_products.name)),

    // Backfill vivo: só o backfill grava checkpoint (o sync incremental não
    // usa nenhum), então uma linha "running"/"paused" aqui significa
    // literalmente "está importando histórico agora".
    db
      .select({ scopeKey: schema.ingestion_checkpoints.scope_key })
      .from(schema.ingestion_checkpoints)
      .where(
        and(
          eq(schema.ingestion_checkpoints.user_id, ownerId),
          eq(schema.ingestion_checkpoints.source_type, "pluggy"),
          inArray(schema.ingestion_checkpoints.status, ["running", "paused"]),
        ),
      ),

    db
      .select({ n: sql<number>`count(*)` })
      .from(schema.draft_records)
      .where(
        and(
          eq(schema.draft_records.user_id, ownerId),
          eq(schema.draft_records.status, "pending_review"),
          like(schema.draft_records.external_ref, "pluggy:%"),
        ),
      ),
  ]);

  const idsDeConexao = conexoesBrutas.map((c) => c.id);
  const contasBrutas = idsDeConexao.length
    ? await db
        .select({
          id: schema.external_account_mappings.id,
          conexaoId: schema.external_account_mappings.provider_connection_id,
          nome: schema.external_account_mappings.external_account_name,
          tipo: schema.external_account_mappings.external_account_type,
          status: schema.external_account_mappings.status,
          produtoId: schema.external_account_mappings.financial_product_id,
        })
        .from(schema.external_account_mappings)
        .where(
          and(
            eq(schema.external_account_mappings.user_id, ownerId),
            inArray(schema.external_account_mappings.provider_connection_id, idsDeConexao),
          ),
        )
        .orderBy(asc(schema.external_account_mappings.created_at))
    : [];

  const produtos: ProdutoOpcao[] = produtosBrutos.map((p) => ({
    id: p.id,
    nome: p.nome,
    tipo: p.tipo,
    instituicao: p.instituicao,
    saldo: num(p.saldo),
  }));
  const produtoPorId = new Map(produtos.map((p) => [p.id, p]));

  // `scope_key` do backfill é "<external_item_id>:<external_account_id>";
  // basta o prefixo até o primeiro ":" para saber de que conexão ele é.
  const itensImportando = new Set(checkpoints.map((c) => c.scopeKey.split(":")[0]));

  const conexoes: Conexao[] = conexoesBrutas.map((c) => ({
    id: c.id,
    itemId: c.itemId,
    instituicao: c.instituicao,
    status: statusDeConexao(c.status),
    importandoHistorico: itensImportando.has(c.itemId),
    ultimaSincronizacao: c.ultimaSincronizacao,
    conectadaEm: c.conectadaEm,
    contas: contasBrutas
      .filter((m) => m.conexaoId === c.id)
      .map((m) => {
        const produto = m.produtoId ? produtoPorId.get(m.produtoId) : undefined;
        return {
          id: m.id,
          nome: m.nome,
          tipo: m.tipo,
          status: statusDeContaExterna(m.status),
          produtoId: m.produtoId,
          produtoNome: produto?.nome ?? null,
          produtoSaldo: produto ? produto.saldo : null,
        };
      }),
  }));

  // Conexão revogada não gera pendência: o usuário já decidiu que ela não
  // sincroniza mais, cobrar vínculo das contas dela seria ruído.
  const contasAguardandoVinculo = conexoes
    .filter((c) => c.status !== "revoked")
    .reduce((total, c) => total + c.contas.filter((a) => a.status === "pending_mapping").length, 0);

  return {
    pluggyConfigurado: Boolean(process.env.PLUGGY_CLIENT_ID && process.env.PLUGGY_CLIENT_SECRET),
    conexoes,
    produtos,
    contasAguardandoVinculo,
    movimentacoesParaRevisar: num(revisao[0]?.n),
    origens: await carregarOrigens(ownerId),
  };
}

/**
 * Estado do worker do Gmail e do worker da pasta local, lido do rastro que
 * eles deixam em `ingestion_runs`. Nada de `process.env` aqui: em produção o
 * app web e os workers são deploys diferentes (Vercel × VPS), então a
 * ausência de `GMAIL_REFRESH_TOKEN` no processo do site não diria nada sobre
 * o worker. O que a última varredura registrou, sim.
 */
async function carregarOrigens(ownerId: string): Promise<OrigemDocumentos[]> {
  const db = getDb();
  const origens: OrigemDocumentos["id"][] = ["gmail", "local_file"];

  return Promise.all(
    origens.map(async (id) => {
      const [run] = await db
        .select({
          id: schema.ingestion_runs.id,
          status: schema.ingestion_runs.status,
          iniciadaEm: schema.ingestion_runs.started_at,
          concluidaEm: schema.ingestion_runs.completed_at,
          metadata: schema.ingestion_runs.metadata,
        })
        .from(schema.ingestion_runs)
        .where(
          and(
            eq(schema.ingestion_runs.user_id, ownerId),
            eq(schema.ingestion_runs.source_type, id),
          ),
        )
        .orderBy(desc(schema.ingestion_runs.started_at))
        .limit(1);

      if (!run) {
        return {
          id,
          jaRodou: false,
          ultimaVarredura: null,
          status: null,
          documentos: 0,
          detalhe: null,
        };
      }

      const [jobs] = await db
        .select({ n: sql<number>`count(*)` })
        .from(schema.ingestion_jobs)
        .where(
          and(eq(schema.ingestion_jobs.user_id, ownerId), eq(schema.ingestion_jobs.run_id, run.id)),
        );

      return {
        id,
        jaRodou: true,
        ultimaVarredura: run.concluidaEm ?? run.iniciadaEm,
        status: statusDeVarredura(run.status),
        documentos: num(jobs?.n),
        detalhe: detalheDaVarredura(id, run.metadata),
      };
    }),
  );
}
