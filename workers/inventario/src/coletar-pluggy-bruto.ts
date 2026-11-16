/**
 * E2/E3: busca o bruto da Pluggy direto na API (só leitura) e enriquece o
 * índice com o que o app descarta: quem recebeu e quem pagou cada Pix/boleto,
 * parcela e data original da compra no cartão, CNPJ do estabelecimento, e as
 * faturas dos cartões. Empréstimos e investimentos são guardados brutos.
 *
 *   bun run workers/inventario/src/coletar-pluggy-bruto.ts [--desde 2025-09-01]
 *
 * Não grava nada no Neon nem na Pluggy. O bruto fica em
 * entrada/pluggy/bruto-<data>/ no cofre. Idempotente pelo external_ref.
 */
import { getDb } from "@sbf/db";
import { PluggyClient } from "@sbf/financial-connectors/pluggy";
import { sql } from "drizzle-orm";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { cicloDe, hojeIso, limitesDoCiclo } from "./ciclo";
import { caminhosDoCofre } from "./cofre";
import { prepararFatura } from "./gravar-pluggy";
import {
  type FaturaPluggyBruta,
  type TransacaoPluggyBruta,
  enriquecer,
  registroDaFatura,
} from "./pluggy-bruto";

const BASE = "https://api.pluggy.ai";

function variavel(nome: string): string {
  const v = process.env[nome];
  if (!v) throw new Error(`${nome} ausente (.env.local)`);
  return v;
}

/** GET simples para os endpoints que o PluggyClient do app não expõe. */
function criarGet() {
  let chave: string | null = null;
  return async function get<T>(
    caminho: string,
  ): Promise<{ ok: true; dado: T } | { ok: false; status: number }> {
    if (!chave) {
      const r = await fetch(`${BASE}/auth`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          clientId: variavel("PLUGGY_CLIENT_ID"),
          clientSecret: variavel("PLUGGY_CLIENT_SECRET"),
        }),
      });
      if (!r.ok) throw new Error(`autenticação na Pluggy falhou (${r.status})`);
      chave = ((await r.json()) as { apiKey: string }).apiKey;
    }
    const r = await fetch(`${BASE}${caminho}`, { headers: { "X-API-KEY": chave } });
    if (!r.ok) return { ok: false, status: r.status };
    return { ok: true, dado: (await r.json()) as T };
  };
}

interface Conta {
  id: string;
  itemId: string;
  name: string;
  type: string;
  subtype?: string;
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2);
  const desde = argv[argv.indexOf("--desde") + 1] ?? "2025-09-01";
  const hoje = hojeIso();
  const cofre = caminhosDoCofre();
  const pasta = join(cofre.entrada, "pluggy", `bruto-${hoje}`);
  mkdirSync(pasta, { recursive: true, mode: 0o700 });
  const guardar = (nome: string, linhas: unknown[]) =>
    writeFileSync(join(pasta, nome), linhas.map((l) => JSON.stringify(l)).join("\n"), {
      mode: 0o600,
    });

  // Itens conectados: lidos do app, só leitura.
  const itens = (
    await getDb().execute(
      sql`SELECT external_item_id FROM provider_connections WHERE provider = 'pluggy' AND status = 'active'`,
    )
  ).rows as Array<{ external_item_id: string }>;

  const cliente = new PluggyClient({
    clientId: variavel("PLUGGY_CLIENT_ID"),
    clientSecret: variavel("PLUGGY_CLIENT_SECRET"),
  });
  const get = criarGet();

  const contas: Conta[] = [];
  const transacoes: Array<TransacaoPluggyBruta & { _conta: Conta }> = [];
  const faturas: Array<FaturaPluggyBruta & { _conta: Conta }> = [];
  const extras: Record<string, unknown> = {};

  for (const { external_item_id: item } of itens) {
    const doItem = (await cliente.listAccounts(item)) as unknown as Conta[];
    contas.push(...doItem);
    for (const conta of doItem) {
      let cursor: string | undefined;
      do {
        const pagina = await cliente.listTransactions(conta.id, { from: desde, to: hoje, cursor });
        for (const t of pagina.results as unknown as TransacaoPluggyBruta[]) {
          transacoes.push({ ...t, _conta: conta });
        }
        cursor = pagina.next ?? undefined;
      } while (cursor);
      if (conta.type === "CREDIT") {
        const r = await get<{ results: FaturaPluggyBruta[] }>(
          `/bills?accountId=${encodeURIComponent(conta.id)}`,
        );
        if (r.ok) for (const f of r.dado.results ?? []) faturas.push({ ...f, _conta: conta });
        else extras[`bills:${conta.id}`] = `indisponível (${r.status})`;
      }
    }
    for (const recurso of ["loans", "investments"] as const) {
      const r = await get<{ results: unknown[] }>(`/${recurso}?itemId=${encodeURIComponent(item)}`);
      extras[`${recurso}:${item}`] = r.ok ? r.dado.results : `indisponível (${r.status})`;
    }
  }

  guardar("contas.jsonl", contas);
  guardar("transacoes.jsonl", transacoes);
  guardar("faturas.jsonl", faturas);
  writeFileSync(join(pasta, "emprestimos-investimentos.json"), JSON.stringify(extras, null, 1), {
    mode: 0o600,
  });

  // Enriquecimento do índice, em transações curtas e sem await dentro.
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);
  const q = {
    registro: db.query("SELECT id FROM registro WHERE external_ref = ?"),
    enriquecer: db.query(
      `UPDATE registro SET contraparte = ?, contraparte_doc = ?, data_compra = ?, merchant_cnpj = ?,
         fatura_ref = ? WHERE id = ?`,
    ),
    novo: db.query(
      `INSERT INTO registro (external_ref, fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos,
         descricao_original, supplier_name_raw, conta_provavel, confianca, extraido_por, estado,
         contraparte, contraparte_doc, data_compra, merchant_cnpj, fatura_ref)
       VALUES (?, 'conexao:pluggy', ?, ?, ?, ?, ?, ?, ?, ?, 1.0, 'pluggy-bruto', 'extraido', ?, ?, ?, ?, ?)
       RETURNING id`,
    ),
    limparParcela: db.query("DELETE FROM sub_registro WHERE registro_id = ? AND tipo = 'parcela'"),
    parcela: db.query(
      `INSERT INTO sub_registro (registro_id, tipo, descricao_original, valor_centavos,
         parcela_numero, parcela_total, confianca)
       VALUES (?, 'parcela', ?, ?, ?, ?, 1.0)`,
    ),
    fatura: prepararFatura(db),
    ciclo: db.query("INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?, ?, ?)"),
  };

  const c = { transacoes: transacoes.length, novas: 0, comContraparte: 0, parcelas: 0, faturas: 0 };
  const lote = db.transaction((itensDoLote: typeof transacoes) => {
    for (const t of itensDoLote) {
      const e = enriquecer(t);
      if (e.contraparte) c.comContraparte++;
      const ref = `pluggy:txn_${t.id}`;
      let id = (q.registro.get(ref) as { id: number } | null)?.id;
      if (id) {
        q.enriquecer.run(
          e.contraparte,
          e.contraparte_doc,
          e.data_compra,
          e.merchant_cnpj,
          e.fatura_id,
          id,
        );
      } else if (t.date && typeof t.amount === "number") {
        const cartao = t._conta.type === "CREDIT";
        const saiu = t.type === "DEBIT" || t.amount < 0;
        const data = t.date.slice(0, 10);
        id = (
          q.novo.get(
            ref,
            cartao ? (saiu ? "compra" : "estorno") : saiu ? "debito" : "credito",
            data,
            cartao ? "compra" : "pagamento",
            cicloDe(data),
            saiu ? -Math.round(Math.abs(t.amount) * 100) : Math.round(Math.abs(t.amount) * 100),
            t.description ?? null,
            t.merchant?.businessName ?? t.merchant?.name ?? null,
            t._conta.name,
            e.contraparte,
            e.contraparte_doc,
            e.data_compra,
            e.merchant_cnpj,
            e.fatura_id,
          ) as { id: number }
        ).id;
        c.novas++;
      }
      if (id && e.parcela_numero && e.parcela_total) {
        q.limparParcela.run(id);
        const total = e.valor_total_compra_centavos;
        q.parcela.run(
          id,
          `parcela ${e.parcela_numero}/${e.parcela_total}${total != null ? ` de ${(total / 100).toFixed(2)}` : ""}`,
          typeof t.amount === "number" ? -Math.round(Math.abs(t.amount) * 100) : null,
          e.parcela_numero,
          e.parcela_total,
        );
        c.parcelas++;
      }
    }
  });
  for (let i = 0; i < transacoes.length; i += 500) lote.immediate(transacoes.slice(i, i + 500));

  db.transaction(() => {
    for (const f of faturas) {
      const r = registroDaFatura(f);
      if (!r) continue;
      const ciclo = cicloDe(r.data);
      const limites = limitesDoCiclo(ciclo);
      q.ciclo.run(ciclo, limites.inicio, limites.fim);
      q.fatura.run(r.external_ref, r.data, ciclo, r.valor_centavos, r.descricao, f._conta.name);
      c.faturas++;
    }
  }).immediate();

  db.query(
    `INSERT INTO varredura (fonte_id, etapa, escopo, responsavel, status, concluida_em, vistos, novos, resumo)
     VALUES ('conexao:pluggy', 'E2', ?, 'coletar-pluggy-bruto@1', 'concluida',
             strftime('%Y-%m-%dT%H:%M:%fZ', 'now'), ?, ?, ?)`,
  ).run(
    `api bruto ${desde} a ${hoje}`,
    transacoes.length,
    c.novas,
    JSON.stringify({
      ...c,
      contas: contas.length,
      extras: Object.keys(extras).length,
      pasta: pasta.replace(`${cofre.raiz}/`, ""),
    }),
  );
  console.log(`pluggy bruto: ${JSON.stringify({ ...c, contas: contas.length })}`);
  for (const [k, v] of Object.entries(extras)) {
    console.log(`  ${k}: ${Array.isArray(v) ? `${v.length} itens` : String(v)}`);
  }
  db.close();
}

if (import.meta.main) {
  main().catch((e: unknown) => {
    console.error(e instanceof Error ? e.message : String(e));
    process.exit(1);
  });
}
