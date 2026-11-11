/**
 * E3: triagem dos achados 'novo' por regras (triagem.ts).
 *
 *   bun run workers/inventario/src/e3-triar.ts --simular [--limite N]
 *   bun run workers/inventario/src/e3-triar.ts [--limite N]
 *
 * Idempotente: só toca achados em estado 'novo'. Com --simular não grava nada
 * e imprime contagens e exemplos (só nome do arquivo ou domínio do remetente).
 */
import type { Database } from "bun:sqlite";
import { aplicarSchema, abrirBanco } from "./banco";
import {
  type AchadoParaTriagem,
  type OrigemResumo,
  type Veredito,
  VERSAO_REGRAS,
  triar,
} from "./triagem";

const LOTE = 2000;

interface LinhaAchado {
  content_hash: string;
  extensao: string | null;
  file_size_bytes: number;
  camada: "A" | "B";
  texto: string | null;
}

function argumento(nome: string): string | undefined {
  const i = process.argv.indexOf(nome);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

const simular = process.argv.includes("--simular");
const limite = argumento("--limite") ? Number(argumento("--limite")) : Infinity;

function dominioDe(remetente: string | null): string {
  return remetente?.match(/@([a-z0-9.-]+)/i)?.[1]?.toLowerCase() ?? "";
}

function rotulo(a: AchadoParaTriagem): string {
  const o = a.origens[0];
  if (!o) return "(sem origem)";
  return o.origin_type === "gmail"
    ? `gmail:${dominioDe(o.gmail_from)} [${o.filename}]`
    : o.filename;
}

interface Exemplo {
  rotulo: string;
  motivo: string;
}

const contagem = new Map<string, number>();
const porTipo = new Map<string, number>();
const reservatorio: Record<string, Exemplo[]> = { documento: [], descartado: [], duvida: [] };
const vistosPorEstado: Record<string, number> = { documento: 0, descartado: 0, duvida: 0 };

function somar(mapa: Map<string, number>, chave: string): void {
  mapa.set(chave, (mapa.get(chave) ?? 0) + 1);
}

/** Amostragem em reservatório: 15 exemplos aleatórios por estado. */
function amostrar(a: AchadoParaTriagem, v: Veredito): void {
  const n = ++vistosPorEstado[v.estado]!;
  const lista = reservatorio[v.estado]!;
  const ex = { rotulo: rotulo(a), motivo: v.motivo };
  if (lista.length < 15) lista.push(ex);
  else {
    const j = Math.floor(Math.random() * n);
    if (j < 15) lista[j] = ex;
  }
}

function carregarOrigens(db: Database, hashes: string[]): Map<string, OrigemResumo[]> {
  const mapa = new Map<string, OrigemResumo[]>();
  const marcas = hashes.map(() => "?").join(",");
  const linhas = db
    .query(
      `SELECT content_hash, origin_type, local_filepath, filename, gmail_from, gmail_subject,
              gmail_label, mensagem_ref
         FROM origem WHERE content_hash IN (${marcas})`,
    )
    .all(...hashes) as Array<OrigemResumo & { content_hash: string }>;
  for (const { content_hash, ...o } of linhas) {
    const lista = mapa.get(content_hash) ?? [];
    lista.push(o);
    mapa.set(content_hash, lista);
  }
  return mapa;
}

const db = abrirBanco();
aplicarSchema(db);

// Mais recentes primeiro: a data mais nova entre o mtime local e a data do e-mail.
const lista = db
  .query(
    `SELECT a.content_hash, a.extensao, a.file_size_bytes, a.camada, t.conteudo AS texto
       FROM achado a
       LEFT JOIN texto t ON t.content_hash = a.content_hash
       LEFT JOIN (
         SELECT content_hash, MAX(COALESCE(local_mtime, gmail_date, '')) AS recente
           FROM origem GROUP BY content_hash
       ) r ON r.content_hash = a.content_hash
      WHERE a.estado = 'novo'
      ORDER BY r.recente DESC`,
  )
  .all() as LinhaAchado[];

const alvo = lista.slice(0, Number.isFinite(limite) ? limite : lista.length);
console.log(`${alvo.length} achados 'novo'${simular ? " (simulação, nada é gravado)" : ""}`);

const atualizar = db.query(
  `UPDATE achado
      SET estado = ?, document_type = ?, confianca = ?, classificado_por = ?,
          notas = CASE WHEN notas IS NULL OR notas = '' THEN ? ELSE notas || char(10) || ? END
    WHERE content_hash = ? AND estado = 'novo'`,
);

for (let i = 0; i < alvo.length; i += LOTE) {
  const lote = alvo.slice(i, i + LOTE);
  const origens = carregarOrigens(
    db,
    lote.map((l) => l.content_hash),
  );
  const veredictos = lote.map((l) => {
    const a: AchadoParaTriagem = { ...l, origens: origens.get(l.content_hash) ?? [] };
    const v = triar(a);
    somar(contagem, v.estado);
    somar(porTipo, `${v.estado}/${v.document_type ?? "-"}`);
    amostrar(a, v);
    return { hash: l.content_hash, v };
  });
  if (!simular) {
    db.transaction(() => {
      for (const { hash, v } of veredictos) {
        const nota = `E3 ${VERSAO_REGRAS}: ${v.motivo}`;
        atualizar.run(v.estado, v.document_type, v.confianca, VERSAO_REGRAS, nota, nota, hash);
      }
    })();
  }
  if ((i / LOTE) % 20 === 0) console.log(`${Math.min(i + LOTE, alvo.length)}/${alvo.length}`);
}

console.log("\nPor estado:");
for (const [k, v] of [...contagem].sort()) console.log(`  ${k}: ${v}`);
console.log("\nPor estado/tipo:");
for (const [k, v] of [...porTipo].sort((a, b) => b[1] - a[1])) console.log(`  ${k}: ${v}`);
for (const estado of ["documento", "duvida", "descartado"]) {
  console.log(`\nExemplos de ${estado}:`);
  for (const e of reservatorio[estado]!) console.log(`  ${e.rotulo}  ->  ${e.motivo}`);
}
db.close();
