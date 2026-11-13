/**
 * E2: traz para o índice, como registros, todas as transações da Pluggy que já
 * estão no app (rascunhos e movimentações). Só lê o Neon. Idempotente pelo
 * external_ref (pluggy:txn_<id>), o mesmo de draft_records.
 *
 *   bun run workers/inventario/src/importar-pluggy.ts
 *
 * Uma cópia bruta de cada linha lida vai para entrada/pluggy/ no cofre.
 */
import { getDb } from "@sbf/db";
import { sql } from "drizzle-orm";
import { chmodSync, mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { cicloDe, hojeIso } from "./ciclo";
import { caminhosDoCofre } from "./cofre";
import { prepararGravacao } from "./gravar-pluggy";

interface Linha {
  external_ref: string;
  status: string;
  posted_record_id: string | null;
  draft_data: {
    direction?: "inflow" | "outflow";
    amount_cents?: number;
    event_date?: string;
    description?: string | null;
    supplier_name_raw?: string | null;
    category_suggestion?: string | null;
    financial_product_id?: string | null;
  };
  produto: string | null;
  tipo_produto: string | null;
}

/** Tipo e data-chave conforme o glossário: compra no cartão conta pela data da compra. */
export function registroDaTransacao(l: Linha) {
  const d = l.draft_data;
  if (!d.event_date || typeof d.amount_cents !== "number" || !d.direction) return null;
  const cartao = l.tipo_produto === "credit_card";
  const entrou = d.direction === "inflow";
  const data = d.event_date.slice(0, 10);
  return {
    external_ref: l.external_ref,
    tipo: cartao ? (entrou ? "estorno" : "compra") : entrou ? "credito" : "debito",
    data,
    data_tipo: cartao ? "compra" : "pagamento",
    ciclo_id: cicloDe(data),
    valor_centavos: entrou ? Math.abs(d.amount_cents) : -Math.abs(d.amount_cents),
    descricao_original: d.description ?? null,
    supplier_name_raw: d.supplier_name_raw ?? null,
    categoria_provavel: d.category_suggestion ?? null,
    conta_provavel: l.produto,
    estado: l.status === "posted" && l.posted_record_id ? "promovido" : "extraido",
    movimentacao_id: l.posted_record_id,
  };
}

async function main(): Promise<void> {
  const cofre = caminhosDoCofre();
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);

  const resultado = await getDb().execute(sql`
    SELECT d.external_ref, d.status, d.posted_record_id, d.draft_data,
           p.name AS produto, p.type AS tipo_produto
      FROM draft_records d
      LEFT JOIN financial_products p ON p.id::text = d.draft_data->>'financial_product_id'
     WHERE d.external_ref LIKE 'pluggy:%'
     ORDER BY d.draft_data->>'event_date'`);
  const linhas = resultado.rows as unknown as Linha[];

  const hoje = hojeIso();
  const pastaBruta = join(cofre.entrada, "pluggy");
  mkdirSync(pastaBruta, { recursive: true, mode: 0o700 });
  const bruto = join(pastaBruta, `draft_records-${hoje}.jsonl`);
  writeFileSync(bruto, linhas.map((l) => JSON.stringify(l)).join("\n"), { mode: 0o600 });
  chmodSync(bruto, 0o600);

  const { id: varreduraId } = db
    .query(
      `INSERT INTO varredura (fonte_id, etapa, escopo, responsavel)
       VALUES ('conexao:pluggy', 'E2', 'draft_records pluggy:*', 'importar-pluggy@1') RETURNING id`,
    )
    .get() as { id: number };

  const existe = db.query("SELECT 1 FROM registro WHERE external_ref = ?");
  const gravar = prepararGravacao(db);

  let novos = 0;
  let repetidos = 0;
  let erros = 0;
  db.transaction(() => {
    for (const l of linhas) {
      const r = registroDaTransacao(l);
      if (!r) {
        erros++;
        continue;
      }
      if (existe.get(r.external_ref)) repetidos++;
      else novos++;
      gravar.run(
        r.external_ref,
        r.tipo,
        r.data,
        r.data_tipo,
        r.ciclo_id,
        r.valor_centavos,
        r.descricao_original,
        r.supplier_name_raw,
        r.categoria_provavel,
        r.conta_provavel,
        r.estado,
        r.movimentacao_id,
      );
    }
  })();

  db.query(
    `UPDATE varredura SET status = 'concluida', concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
       vistos = ?, novos = ?, repetidos = ?, erros = ?, resumo = ? WHERE id = ?`,
  ).run(
    linhas.length,
    novos,
    repetidos,
    erros,
    JSON.stringify({ bruto: `entrada/pluggy/draft_records-${hoje}.jsonl` }),
    varreduraId,
  );
  console.log(
    `pluggy: ${linhas.length} transações lidas, ${novos} registros novos, ${repetidos} atualizados, ${erros} sem data/valor`,
  );
  db.close();
}

if (import.meta.main) {
  main().catch((e: unknown) => {
    console.error(e instanceof Error ? e.message : String(e));
    process.exit(1);
  });
}
