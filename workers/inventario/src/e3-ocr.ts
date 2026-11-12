/**
 * E3 — OCR das imagens que a triagem deixou em dúvida ("precisa de OCR").
 * Roda tesseract (por+eng) e grava o texto em `texto` com metodo 'ocr-por', no
 * mesmo formato do e3-extrair; depois `e3-triar.ts --duvidas-com-texto`
 * reavalia essas imagens com o texto.
 *
 *   bun run workers/inventario/src/e3-ocr.ts [--limite N] [--paralelo N]
 *
 * Regras (LICOES.md): original só é lido (prefere a cópia do cofre); timeout
 * por arquivo; nenhuma transação atravessa await (resultados em memória, lotes
 * curtos IMMEDIATE). Incremental: quem já tem linha em `texto` não volta.
 * Falha de leitura/timeout grava texto vazio (ocr-timeout / ocr-erro) para não
 * repetir; arquivo sem localização não grava nada e volta na próxima rodada.
 * Saída só com contagens: nunca imprime texto, nomes ou valores.
 */
import type { Database } from "bun:sqlite";
import { existsSync, statSync } from "node:fs";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { caminhosDoCofre } from "./cofre";
import { EXTENSOES_OCR, textoParaGravar } from "./ocr-texto";

const TIMEOUT_MS = 60_000;
const PARALELO_PADRAO = 4;
const LOTE_GRAVACAO = 50;

interface Alvo {
  content_hash: string;
  extensao: string | null;
  caminho_acervo: string | null;
}

type Situacao = "texto" | "sem-texto" | "timeout" | "erro" | "sem-arquivo";

interface Resultado {
  hash: string;
  situacao: Situacao;
  texto: string;
}

function inteiro(nome: string, padrao: number | null): number | null {
  const i = process.argv.indexOf(nome);
  if (i < 0) return padrao;
  const n = Number(process.argv[i + 1]);
  if (!Number.isInteger(n) || n < 1) throw new Error(`${nome} espera um inteiro positivo`);
  return n;
}

function localizarArquivo(db: Database, raiz: string, alvo: Alvo): string | null {
  if (alvo.caminho_acervo) {
    const p = join(raiz, alvo.caminho_acervo);
    if (existsSync(p)) return p;
  }
  const origens = db
    .query(
      `SELECT local_filepath, file_size_bytes FROM origem
        WHERE content_hash = ? AND local_filepath IS NOT NULL ORDER BY id`,
    )
    .all(alvo.content_hash) as Array<{ local_filepath: string; file_size_bytes: number | null }>;
  for (const o of origens) {
    if (!o.file_size_bytes) continue;
    try {
      if (statSync(o.local_filepath).size === o.file_size_bytes) return o.local_filepath;
    } catch {
      // origem sumiu ou NTFS não abre o nome: tenta a próxima
    }
  }
  return null;
}

async function tesseract(caminho: string): Promise<{ situacao: Situacao; saida: string }> {
  const proc = Bun.spawn(["tesseract", caminho, "stdout", "-l", "por+eng"], {
    stdout: "pipe",
    stderr: "pipe",
    env: { ...process.env, OMP_THREAD_LIMIT: "1" },
  });
  let estourou = false;
  const relogio = setTimeout(() => {
    estourou = true;
    proc.kill("SIGKILL");
  }, TIMEOUT_MS);
  try {
    const [saida, , codigo] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    if (estourou) return { situacao: "timeout", saida: "" };
    if (codigo !== 0) return { situacao: "erro", saida: "" };
    return { situacao: "texto", saida };
  } finally {
    clearTimeout(relogio);
  }
}

async function processar(db: Database, raiz: string, alvo: Alvo): Promise<Resultado> {
  const caminho = localizarArquivo(db, raiz, alvo);
  if (!caminho) return { hash: alvo.content_hash, situacao: "sem-arquivo", texto: "" };
  try {
    const r = await tesseract(caminho);
    if (r.situacao !== "texto") return { hash: alvo.content_hash, situacao: r.situacao, texto: "" };
    const texto = textoParaGravar(r.saida);
    return {
      hash: alvo.content_hash,
      situacao: texto ? "texto" : "sem-texto",
      texto,
    };
  } catch {
    return { hash: alvo.content_hash, situacao: "erro", texto: "" };
  }
}

async function pool<T>(
  itens: T[],
  paralelo: number,
  tarefa: (item: T) => Promise<void>,
): Promise<void> {
  let proximo = 0;
  const corredores = Array.from({ length: Math.min(paralelo, itens.length) }, async () => {
    while (proximo < itens.length) await tarefa(itens[proximo++]!);
  });
  await Promise.all(corredores);
}

const METODO: Record<Situacao, string | null> = {
  texto: "ocr-por",
  "sem-texto": "ocr-sem-texto",
  timeout: "ocr-timeout",
  erro: "ocr-erro",
  "sem-arquivo": null,
};

async function main(): Promise<void> {
  const limite = inteiro("--limite", null);
  const paralelo = inteiro("--paralelo", PARALELO_PADRAO)!;
  const cofre = caminhosDoCofre();
  const db = abrirBanco();
  aplicarSchema(db);

  const alvos = db
    .query(
      `SELECT content_hash, extensao, caminho_acervo
         FROM achado a
        WHERE estado = 'duvida'
          AND lower(extensao) IN (${EXTENSOES_OCR.map(() => "?").join(",")})
          AND notas LIKE '%precisa de OCR%'
          AND NOT EXISTS (SELECT 1 FROM texto t WHERE t.content_hash = a.content_hash)
        ORDER BY primeiro_visto_em DESC
        ${limite ? "LIMIT ?" : ""}`,
    )
    .all(...EXTENSOES_OCR, ...(limite ? [limite] : [])) as Alvo[];
  console.log(`${alvos.length} imagens para OCR (paralelo ${paralelo})`);

  const inicio = Date.now();
  const contagem = new Map<Situacao, number>();
  const pendentes: Resultado[] = [];
  const gravar = db.transaction((lista: Resultado[]) => {
    const q = db.query(
      "INSERT OR IGNORE INTO texto (content_hash, metodo, conteudo) VALUES (?, ?, ?)",
    );
    for (const r of lista) {
      const metodo = METODO[r.situacao];
      if (metodo) q.run(r.hash, metodo, r.texto);
    }
  });
  const flush = () => {
    if (pendentes.length) gravar.immediate(pendentes.splice(0));
  };

  let feitos = 0;
  await pool(alvos, paralelo, async (alvo) => {
    const r = await processar(db, cofre.raiz, alvo);
    contagem.set(r.situacao, (contagem.get(r.situacao) ?? 0) + 1);
    pendentes.push(r);
    if (pendentes.length >= LOTE_GRAVACAO) flush();
    if (++feitos % 100 === 0) console.log(`  ${feitos}/${alvos.length}`);
  });
  flush();

  const n = (k: Situacao) => contagem.get(k) ?? 0;
  console.log(`\nprocessadas: ${feitos}`);
  console.log(`com texto: ${n("texto")}`);
  console.log(`sem texto legível: ${n("sem-texto")}`);
  console.log(`timeout: ${n("timeout")}`);
  console.log(`erros: ${n("erro")}`);
  console.log(`sem arquivo localizável: ${n("sem-arquivo")}`);
  console.log(`tempo: ${((Date.now() - inicio) / 1000).toFixed(1)}s`);
  db.close();
}

await main();
