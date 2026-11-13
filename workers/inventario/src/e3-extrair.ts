/**
 * E3 — extração de texto e campos dos achados que são (ou podem ser)
 * documentos. Lê o arquivo do cofre (ou a origem local), tira o texto
 * (pdftotext, XML, OFX, e-mail), guarda até 20 KB em `texto` e preenche
 * data-chave, ciclo, valor, fornecedor e tipo no achado.
 *
 *   bun run workers/inventario/src/e3-extrair.ts --simular --limite 300
 *   bun run workers/inventario/src/e3-extrair.ts [--limite N] [--ext pdf,eml]
 *
 * Regras (LICOES.md): nunca escreve em original, nunca tenta senha (PDF
 * protegido só ganha protegido_senha = 1), nada de OCR aqui (PDF de imagem
 * grava texto vazio com metodo 'pdf-sem-texto'), banco só pelo laço principal.
 * Idempotente: quem já tem linha em `texto` (ou protegido_senha = 1) não volta.
 */
import type { Database } from "bun:sqlite";
import { existsSync, statSync } from "node:fs";
import { readFile } from "node:fs/promises";
import { join } from "node:path";
import { abrirBanco } from "./banco";
import { cicloDe, limitesDoCiclo } from "./ciclo";
import { caminhosDoCofre } from "./cofre";
import { condicaoDoUsuario } from "./decisao";
import {
  escolherDataChave,
  extrairCampos,
  htmlParaTexto,
  parsearEml,
  parsearNfe,
  parsearOfx,
  type Campos,
  type TipoDataChave,
  type TipoDocumento,
} from "./extrair";

const LIMITE_TEXTO = 20 * 1024;
const PARALELO = 6;
const TIMEOUT_PDF_MS = 30_000;
const MAX_LEITURA = 8 * 1024 * 1024;
const PAGINAS = 3;
/** Texto de PDF com menos letras que isso é tratado como imagem. */
const MIN_LETRAS = 15;

export interface Alvo {
  content_hash: string;
  extensao: string | null;
  caminho_acervo: string | null;
  estado: string;
  document_type: string | null;
  data_chave: string | null;
  data_chave_tipo: string | null;
  valor_centavos: number | null;
  supplier_name_raw: string | null;
}

interface OrigemInfo {
  local_filepath: string | null;
  file_size_bytes: number | null;
  local_mtime: string | null;
  gmail_date: string | null;
}

type Situacao = "texto" | "protegido" | "sem-texto" | "sem-arquivo" | "sem-extrator" | "erro";

export interface Resultado {
  alvo: Alvo;
  situacao: Situacao;
  metodo?: string;
  texto?: string;
  fingerprint?: string;
  tipoDoc?: TipoDocumento | null;
  data?: { data: string; tipo: TipoDataChave } | null;
  valorCentavos?: number | null;
  fornecedor?: string | null;
}

// ── Argumentos ────────────────────────────────────────────────────────────

function lerArgumentos(): { simular: boolean; limite: number | null; extensoes: string[] | null } {
  const args = process.argv.slice(2);
  const i = args.indexOf("--limite");
  const limite = i >= 0 ? Number(args[i + 1]) : null;
  if (limite !== null && (!Number.isInteger(limite) || limite < 1)) {
    throw new Error(`--limite espera um inteiro positivo: ${args[i + 1]}`);
  }
  const j = args.indexOf("--ext");
  const extensoes = j >= 0 ? (args[j + 1] ?? "").toLowerCase().split(",").filter(Boolean) : null;
  return { simular: args.includes("--simular"), limite, extensoes };
}

// ── Localizar o arquivo ───────────────────────────────────────────────────

function dataLocal(valor: string | null): string | null {
  if (!valor) return null;
  const d = new Date(valor);
  if (Number.isNaN(d.getTime())) return null;
  return d.toLocaleDateString("sv-SE", { timeZone: "America/Sao_Paulo" });
}

function origensDe(db: Database, hash: string): OrigemInfo[] {
  return db
    .query(
      `SELECT local_filepath, file_size_bytes, local_mtime, gmail_date
         FROM origem WHERE content_hash = ? ORDER BY id`,
    )
    .all(hash) as OrigemInfo[];
}

function localizarArquivo(raiz: string, alvo: Alvo, origens: OrigemInfo[]): string | null {
  if (alvo.caminho_acervo) {
    const p = join(raiz, alvo.caminho_acervo);
    if (existsSync(p)) return p;
  }
  for (const o of origens) {
    if (!o.local_filepath || !o.file_size_bytes) continue;
    try {
      if (statSync(o.local_filepath).size === o.file_size_bytes) return o.local_filepath;
    } catch {
      // origem sumiu ou NTFS não abre o nome: tenta a próxima
    }
  }
  return null;
}

// ── Extração de texto (roda no pool) ──────────────────────────────────────

async function pdftotext(caminho: string): Promise<{ texto: string; protegido: boolean } | null> {
  const proc = Bun.spawn(["pdftotext", "-layout", "-l", String(PAGINAS), caminho, "-"], {
    stdout: "pipe",
    stderr: "pipe",
  });
  const relogio = setTimeout(() => proc.kill(), TIMEOUT_PDF_MS);
  try {
    const [saida, erro, codigo] = await Promise.all([
      new Response(proc.stdout).text(),
      new Response(proc.stderr).text(),
      proc.exited,
    ]);
    if (/incorrect password|encrypted/i.test(erro) && codigo !== 0) {
      return { texto: "", protegido: true };
    }
    if (codigo !== 0 && saida.trim() === "") return null;
    return { texto: saida, protegido: false };
  } finally {
    clearTimeout(relogio);
  }
}

function letras(s: string): number {
  return (s.match(/\p{L}/gu) ?? []).length;
}

interface Extraido {
  situacao: Situacao;
  metodo?: string;
  texto: string;
  campos?: Campos & { tipoDoc?: TipoDocumento | null };
}

async function lerTexto(caminho: string): Promise<string> {
  const buf = await readFile(caminho);
  const bytes = buf.subarray(0, MAX_LEITURA);
  const utf8 = new TextDecoder("utf-8", { fatal: false }).decode(bytes);
  // arquivo em Latin-1 vira U+FFFD em utf-8: relê como windows-1252
  return utf8.includes("�") ? new TextDecoder("windows-1252").decode(bytes) : utf8;
}

async function extrairDoArquivo(
  caminho: string,
  extensao: string,
  origens: OrigemInfo[],
): Promise<Extraido> {
  const ext = extensao.toLowerCase();
  const dataMsg = dataLocal(origens.find((o) => o.gmail_date)?.gmail_date ?? null);

  if (ext === "pdf") {
    const r = await pdftotext(caminho);
    if (!r) return { situacao: "erro", texto: "" };
    if (r.protegido) return { situacao: "protegido", texto: "" };
    if (letras(r.texto) < MIN_LETRAS) {
      return { situacao: "sem-texto", metodo: "pdf-sem-texto", texto: "" };
    }
    return {
      situacao: "texto",
      metodo: "pdftotext",
      texto: r.texto,
      campos: extrairCampos(r.texto),
    };
  }

  if (ext === "xml") {
    const xml = await lerTexto(caminho);
    const nfe = parsearNfe(xml);
    if (nfe) {
      const texto = [
        "NF-e",
        nfe.emitente && `Emitente: ${nfe.emitente}`,
        nfe.cnpj && `CNPJ: ${nfe.cnpj}`,
        nfe.numero && `Número: ${nfe.numero}`,
        nfe.emissao && `Emissão: ${nfe.emissao}`,
        nfe.valorCentavos !== null && `Valor: ${(nfe.valorCentavos / 100).toFixed(2)}`,
      ]
        .filter(Boolean)
        .join("\n");
      return {
        situacao: "texto",
        metodo: "xml-nfe",
        texto,
        campos: {
          tipoDoc: "nfe",
          emissao: nfe.emissao,
          valorCentavos: nfe.valorCentavos,
          fornecedor: nfe.emitente,
          cnpj: nfe.cnpj,
        },
      };
    }
    const texto = htmlParaTexto(xml);
    return letras(texto) < MIN_LETRAS
      ? { situacao: "sem-texto", metodo: "xml", texto: "" }
      : { situacao: "texto", metodo: "xml", texto, campos: extrairCampos(texto) };
  }

  if (ext === "ofx" || ext === "ofc") {
    const bruto = await lerTexto(caminho);
    const ofx = parsearOfx(bruto);
    const texto = bruto.slice(0, LIMITE_TEXTO);
    return {
      situacao: "texto",
      metodo: "ofx",
      texto,
      campos: {
        tipoDoc: "extrato",
        emissao: ofx?.fim ?? null,
        fornecedor: ofx?.banco ?? null,
      },
    };
  }

  if (ext === "eml") {
    const eml = parsearEml(await lerTexto(caminho));
    const texto = [
      eml.deNome || eml.de ? `De: ${[eml.deNome, eml.de].filter(Boolean).join(" ")}` : "",
      eml.assunto ? `Assunto: ${eml.assunto}` : "",
      eml.data ? `Data: ${eml.data}` : "",
      "",
      eml.corpo,
    ]
      .join("\n")
      .trim();
    if (letras(texto) < MIN_LETRAS) return { situacao: "sem-texto", metodo: "eml", texto: "" };
    const campos = extrairCampos(`${eml.assunto ?? ""}\n${eml.corpo}`);
    return {
      situacao: "texto",
      metodo: "eml",
      texto,
      campos: {
        ...campos,
        mensagem: eml.data ?? dataMsg,
        fornecedor: campos.fornecedor ?? eml.deNome ?? eml.de,
      },
    };
  }

  if (ext === "txt" || ext === "csv" || ext === "html" || ext === "htm") {
    const bruto = await lerTexto(caminho);
    const texto = ext === "html" || ext === "htm" ? htmlParaTexto(bruto) : bruto;
    return letras(texto) < MIN_LETRAS
      ? { situacao: "sem-texto", metodo: ext, texto: "" }
      : { situacao: "texto", metodo: ext, texto, campos: extrairCampos(texto) };
  }

  return { situacao: "sem-extrator", metodo: `sem-extrator:${ext || "?"}`, texto: "" };
}

async function fingerprint(texto: string): Promise<string> {
  // Mesmo cálculo de computeCanonicalFingerprint (@sbf/operations), copiado
  // porque o worker não declara essa dependência.
  const normalizado = texto.toLowerCase().replace(/\s+/g, " ").trim();
  const digest = await crypto.subtle.digest("SHA-256", new TextEncoder().encode(normalizado));
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
}

async function processar(raiz: string, alvo: Alvo, origens: OrigemInfo[]): Promise<Resultado> {
  const caminho = localizarArquivo(raiz, alvo, origens);
  if (!caminho) return { alvo, situacao: "sem-arquivo" };
  try {
    const ext = await extrairDoArquivo(caminho, alvo.extensao ?? "", origens);
    if (ext.situacao !== "texto" || !ext.campos) {
      return { alvo, situacao: ext.situacao, metodo: ext.metodo, texto: "" };
    }
    const campos: Campos = { ...ext.campos };
    campos.arquivo ??= dataLocal(origens.find((o) => o.local_mtime)?.local_mtime ?? null);
    campos.mensagem ??= dataLocal(origens.find((o) => o.gmail_date)?.gmail_date ?? null);
    const tipoDoc = ext.campos.tipoDoc ?? null;
    return {
      alvo,
      situacao: "texto",
      metodo: ext.metodo,
      texto: ext.texto,
      fingerprint: await fingerprint(ext.texto),
      tipoDoc,
      data: escolherDataChave(campos, tipoDoc),
      valorCentavos: campos.valorCentavos ?? null,
      fornecedor: campos.fornecedor ?? null,
    };
  } catch (e) {
    console.error(`  erro em ${alvo.content_hash.slice(0, 12)}: ${(e as Error).message}`);
    return { alvo, situacao: "erro" };
  }
}

// ── Pool ──────────────────────────────────────────────────────────────────

async function pool<T>(
  itens: T[],
  paralelo: number,
  tarefa: (item: T) => Promise<void>,
): Promise<void> {
  let proximo = 0;
  const corredores = Array.from({ length: Math.min(paralelo, itens.length) }, async () => {
    while (proximo < itens.length) {
      const item = itens[proximo++]!;
      await tarefa(item);
    }
  });
  await Promise.all(corredores);
}

// ── Gravação ──────────────────────────────────────────────────────────────

/** Datas reais (pagamento, vencimento, emissão, compra) valem mais que 'arquivo'/'mensagem'. */
const DATA_FRACA: ReadonlySet<TipoDataChave> = new Set(["arquivo", "mensagem"]);

export function gravar(db: Database, r: Resultado): void {
  const { alvo } = r;
  if (r.situacao === "protegido") {
    db.query("UPDATE achado SET protegido_senha = 1 WHERE content_hash = ?").run(alvo.content_hash);
    return;
  }
  if (r.situacao === "sem-arquivo" || r.situacao === "erro") return;

  db.query("INSERT OR IGNORE INTO texto (content_hash, metodo, conteudo) VALUES (?, ?, ?)").run(
    alvo.content_hash,
    r.metodo ?? "desconhecido",
    (r.texto ?? "").slice(0, LIMITE_TEXTO),
  );
  if (r.situacao !== "texto") {
    if (r.metodo === "pdf-sem-texto") {
      db.query("UPDATE achado SET protegido_senha = 0 WHERE content_hash = ?").run(
        alvo.content_hash,
      );
    }
    return;
  }

  let dataChave = alvo.data_chave;
  let dataTipo = alvo.data_chave_tipo;
  let ciclo: string | null = null;
  if (r.data && (!alvo.data_chave || !DATA_FRACA.has(r.data.tipo))) {
    dataChave = r.data.data;
    dataTipo = r.data.tipo;
  }
  if (dataChave) {
    ciclo = cicloDe(dataChave);
    const l = limitesDoCiclo(ciclo);
    db.query("INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?, ?, ?)").run(
      l.id,
      l.inicio,
      l.fim,
    );
  }
  // Com decisão do usuário, o que ele já tem (data, valor, fornecedor) fica; só se
  // preenche o que está vazio.
  const usuario = condicaoDoUsuario(db, "achado");
  db.query(
    `UPDATE achado SET
       protegido_senha = CASE WHEN ? = 'pdf' THEN 0 ELSE protegido_senha END,
       canonical_fingerprint = ?,
       document_type = COALESCE(document_type, ?),
       data_chave = CASE WHEN ${usuario} AND data_chave IS NOT NULL THEN data_chave ELSE ? END,
       data_chave_tipo = CASE WHEN ${usuario} AND data_chave IS NOT NULL THEN data_chave_tipo ELSE ? END,
       ciclo_id = CASE WHEN ${usuario} AND data_chave IS NOT NULL THEN ciclo_id ELSE ? END,
       valor_centavos = CASE WHEN ${usuario} THEN COALESCE(valor_centavos, ?) ELSE COALESCE(?, valor_centavos) END,
       supplier_name_raw = CASE WHEN ${usuario} THEN COALESCE(supplier_name_raw, ?) ELSE COALESCE(?, supplier_name_raw) END
     WHERE content_hash = ?`,
  ).run(
    (alvo.extensao ?? "").toLowerCase(),
    r.fingerprint ?? null,
    r.tipoDoc ?? null,
    dataChave,
    dataTipo,
    ciclo,
    r.valorCentavos ?? null,
    r.valorCentavos ?? null,
    r.fornecedor ?? null,
    r.fornecedor ?? null,
    alvo.content_hash,
  );
}

// ── Execução ──────────────────────────────────────────────────────────────

function contar(mapa: Map<string, number>, chave: string): void {
  mapa.set(chave, (mapa.get(chave) ?? 0) + 1);
}

function formatarMapa(mapa: Map<string, number>): string {
  return [...mapa.entries()]
    .sort((a, b) => b[1] - a[1])
    .map(([k, v]) => `${k}=${v}`)
    .join(", ");
}

async function main(): Promise<void> {
  const { simular, limite, extensoes } = lerArgumentos();
  const cofre = caminhosDoCofre();
  const db = abrirBanco();

  const alvos = db
    .query(
      `SELECT content_hash, extensao, caminho_acervo, estado, document_type, data_chave,
              data_chave_tipo, valor_centavos, supplier_name_raw
         FROM achado a
        WHERE (estado IN ('documento', 'duvida')
               OR (estado = 'novo' AND camada = 'A'
                   AND lower(extensao) IN ('pdf', 'xml', 'ofx', 'ofc', 'eml')))
          AND (protegido_senha IS NULL OR protegido_senha = 0)
          ${extensoes ? `AND lower(extensao) IN (${extensoes.map(() => "?").join(",")})` : ""}
          AND NOT EXISTS (SELECT 1 FROM texto t WHERE t.content_hash = a.content_hash)
        ORDER BY primeiro_visto_em DESC
        ${limite ? "LIMIT ?" : ""}`,
    )
    .all(...(extensoes ?? []), ...(limite ? [limite] : [])) as Alvo[];

  console.log(`${simular ? "[simulação] " : ""}${alvos.length} achados para extrair`);

  const situacoes = new Map<string, number>();
  const porTipoData = new Map<string, number>();
  const porTipoDoc = new Map<string, number>();
  let comValor = 0;
  let comFornecedor = 0;
  const exemplos: Resultado[] = [];
  let feitos = 0;

  // Nada de transação aberta atravessando await: com outros processos gravando,
  // uma leitura dentro dela fixa um snapshot e a escrita seguinte falha com
  // SQLITE_BUSY_SNAPSHOT. Os resultados esperam em memória e vão em lotes
  // curtos, numa transação IMMEDIATE sem await dentro.
  const pendentes: Resultado[] = [];
  const descarregar = db.transaction((lista: Resultado[]) => {
    for (const r of lista) gravar(db, r);
  });
  const flush = () => {
    if (simular || !pendentes.length) return;
    descarregar.immediate(pendentes.splice(0));
  };
  await pool(alvos, PARALELO, async (alvo) => {
    const r = await processar(cofre.raiz, alvo, origensDe(db, alvo.content_hash));
    contar(situacoes, r.situacao);
    if (r.situacao === "texto") {
      contar(porTipoData, r.data?.tipo ?? "nenhuma");
      contar(porTipoDoc, r.tipoDoc ?? alvo.document_type ?? "?");
      if (r.valorCentavos != null) comValor++;
      if (r.fornecedor) comFornecedor++;
      exemplos.push(r);
    }
    if (!simular) {
      pendentes.push(r);
      if (pendentes.length >= 200) flush();
    }
    if (++feitos % 500 === 0) console.log(`  ${feitos}/${alvos.length}`);
  });
  flush();

  const n = (k: string): number => situacoes.get(k) ?? 0;
  console.log(`\nprocessados: ${feitos}`);
  console.log(`com texto: ${n("texto")}`);
  console.log(`protegidos por senha: ${n("protegido")}`);
  console.log(`PDF só imagem / sem texto: ${n("sem-texto")}`);
  console.log(`sem arquivo localizável: ${n("sem-arquivo")}`);
  console.log(`sem extrator (imagem etc.): ${n("sem-extrator")}`);
  console.log(`erros: ${n("erro")}`);
  console.log(`data-chave por tipo: ${formatarMapa(porTipoData) || "-"}`);
  console.log(`tipo de documento: ${formatarMapa(porTipoDoc) || "-"}`);
  console.log(`com valor: ${comValor}`);
  console.log(`com fornecedor: ${comFornecedor}`);

  if (simular) {
    const sorteio = [...exemplos].sort(() => Math.random() - 0.5).slice(0, 15);
    console.log("\nexemplos (tipo | data-chave | valor | fornecedor):");
    for (const e of sorteio) {
      const valor = e.valorCentavos != null ? (e.valorCentavos / 100).toFixed(2) : "-";
      console.log(
        `  ${e.tipoDoc ?? e.alvo.document_type ?? "-"} | ${e.data ? `${e.data.data} (${e.data.tipo})` : "-"} | ${valor} | ${e.fornecedor ?? "-"}`,
      );
    }
  }
  db.close();
}

if (import.meta.main) await main();
