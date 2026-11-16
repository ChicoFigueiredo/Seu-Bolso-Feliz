/**
 * E3: triagem dos achados 'novo' por regras (triagem.ts).
 *
 *   bun run workers/inventario/src/e3-triar.ts --simular [--limite N]
 *   bun run workers/inventario/src/e3-triar.ts [--limite N]
 *
 * Idempotente: só toca achados em estado 'novo'. Com --simular não grava nada
 * e imprime contagens e exemplos (só nome do arquivo ou domínio do remetente).
 *
 * --duvidas-com-texto: segunda passada, depois da extração. Reavalia só as
 * dúvidas que ganharam texto (o sinal que faltava) e grava o novo veredito.
 *
 * A decisão do usuário prevalece (spec 06, INV-06): achado com
 * decidido_por='usuario' (gravado pelo desktop) nunca é tocado.
 */
import type { Database } from "bun:sqlite";
import { aplicarSchema, abrirBanco, temColuna } from "./banco";
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

export interface OpcoesDaTriagem {
  /** Não grava nada; só conta e amostra. */
  simular: boolean;
  /** Segunda passada: só as dúvidas que ganharam texto. */
  duvidasComTexto: boolean;
  limite: number;
}

interface Exemplo {
  rotulo: string;
  motivo: string;
}

export interface ResultadoDaTriagem {
  estadoAlvo: "novo" | "duvida";
  /** Quantos achados entraram na rodada (já sem os decididos pelo usuário). */
  alvo: number;
  contagem: Map<string, number>;
  porTipo: Map<string, number>;
  reservatorio: Record<string, Exemplo[]>;
}

function argumento(nome: string): string | undefined {
  const i = process.argv.indexOf(nome);
  return i >= 0 ? process.argv[i + 1] : undefined;
}

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

function somar(mapa: Map<string, number>, chave: string): void {
  mapa.set(chave, (mapa.get(chave) ?? 0) + 1);
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

/**
 * Tria os achados em `estadoAlvo` e grava o veredito, em lotes curtos.
 * Num banco migrado pelo Rust (com `decidido_por`), pula o que o usuário
 * decidiu e marca `decidido_por` junto com `classificado_por`.
 */
export function triarAchados(
  db: Database,
  opcoes: OpcoesDaTriagem,
  log: (linha: string) => void = console.log,
): ResultadoDaTriagem {
  const estadoAlvo = opcoes.duvidasComTexto ? "duvida" : "novo";
  const comDecididoPor = temColuna(db, "achado", "decidido_por");
  const naoDoUsuario = comDecididoPor
    ? "AND (a.decidido_por IS NULL OR a.decidido_por <> 'usuario')"
    : "";

  const contagem = new Map<string, number>();
  const porTipo = new Map<string, number>();
  const reservatorio: Record<string, Exemplo[]> = { documento: [], descartado: [], duvida: [] };
  const vistosPorEstado: Record<string, number> = { documento: 0, descartado: 0, duvida: 0 };

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
        WHERE a.estado = ?
          AND (? = 0 OR length(t.conteudo) > 0)
          ${naoDoUsuario}
        ORDER BY r.recente DESC`,
    )
    .all(estadoAlvo, opcoes.duvidasComTexto ? 1 : 0) as LinhaAchado[];

  const alvo = lista.slice(0, Number.isFinite(opcoes.limite) ? opcoes.limite : lista.length);
  log(
    `${alvo.length} achados '${estadoAlvo}'${opcoes.duvidasComTexto ? " com texto" : ""}${opcoes.simular ? " (simulação, nada é gravado)" : ""}`,
  );

  // A mesma nota não se repete: a passada de dúvidas com texto roda a cada 30 min e
  // reavalia as mesmas linhas, que só ganham nota quando o motivo muda.
  // A condição de decidido_por se repete no UPDATE: o desktop pode decidir entre a
  // leitura e a escrita.
  const atualizar = db.query(
    `UPDATE achado
        SET estado = ?, document_type = ?, confianca = ?, classificado_por = ?,
            ${comDecididoPor ? "decidido_por = ?," : ""}
            notas = CASE WHEN notas IS NULL OR notas = '' THEN ?
                         WHEN instr(char(10) || notas || char(10), char(10) || ? || char(10)) > 0 THEN notas
                         ELSE notas || char(10) || ? END
      WHERE content_hash = ? AND estado = ?
            ${comDecididoPor ? "AND (decidido_por IS NULL OR decidido_por <> 'usuario')" : ""}`,
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
    if (!opcoes.simular) {
      db.transaction(() => {
        for (const { hash, v } of veredictos) {
          const nota = `E3 ${VERSAO_REGRAS}: ${v.motivo}`;
          atualizar.run(
            v.estado,
            v.document_type,
            v.confianca,
            VERSAO_REGRAS,
            ...(comDecididoPor ? [VERSAO_REGRAS] : []),
            nota,
            nota,
            nota,
            hash,
            estadoAlvo,
          );
        }
      }).immediate();
    }
    if ((i / LOTE) % 20 === 0) log(`${Math.min(i + LOTE, alvo.length)}/${alvo.length}`);
  }

  return { estadoAlvo, alvo: alvo.length, contagem, porTipo, reservatorio };
}

if (import.meta.main) {
  const opcoes: OpcoesDaTriagem = {
    simular: process.argv.includes("--simular"),
    duvidasComTexto: process.argv.includes("--duvidas-com-texto"),
    limite: argumento("--limite") ? Number(argumento("--limite")) : Infinity,
  };
  const db = abrirBanco();
  aplicarSchema(db);
  const { contagem, porTipo, reservatorio } = triarAchados(db, opcoes);

  console.log("\nPor estado:");
  for (const [k, v] of [...contagem].sort()) console.log(`  ${k}: ${v}`);
  console.log("\nPor estado/tipo:");
  for (const [k, v] of [...porTipo].sort((a, b) => b[1] - a[1])) console.log(`  ${k}: ${v}`);
  for (const estado of ["documento", "duvida", "descartado"]) {
    console.log(`\nExemplos de ${estado}:`);
    for (const e of reservatorio[estado]!) console.log(`  ${e.rotulo}  ->  ${e.motivo}`);
  }
  db.close();
}
