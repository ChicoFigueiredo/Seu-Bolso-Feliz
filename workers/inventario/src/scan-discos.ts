/**
 * E1: scan básico de um disco em três camadas (regras em areas.ts).
 *
 *   bun run workers/inventario/src/scan-discos.ts --fonte disco:p
 *   bun run workers/inventario/src/scan-discos.ts --fonte disco:p --raiz /mnt/p/OneDrive
 *
 * Ordem: primeiro as áreas A, depois o resto do disco (B), por último a
 * contagem das áreas C — o valioso chega antes. Reexecutar é seguro: uma
 * origem já vista (mesmo caminho e mtime) é pulada sem reler o arquivo.
 * Nunca escreve, move nem renomeia nada fora do cofre.
 */
import type { Database } from "bun:sqlite";
import {
  appendFileSync,
  chmodSync,
  constants,
  copyFileSync,
  type Dirent,
  existsSync,
  mkdirSync,
  readdirSync,
  type Stats,
  statSync,
  utimesSync,
} from "node:fs";
import { dirname, join } from "node:path";
import {
  type Classe,
  REGRAS_POR_CAMINHO,
  classificarPasta,
  extensaoDe,
  tratamentoDoArquivo,
} from "./areas";
import { abrirBanco, aplicarSchema } from "./banco";
import { caminhoNoAcervo, caminhosDoCofre } from "./cofre";
import { FONTES } from "./fontes";

const RESPONSAVEL = "scan-discos@1";
/** Acima disso a origem é registrada sem hash (vira pendência), para não travar o disco. */
const LIMITE_HASH_BYTES = 2 * 1024 ** 3;

const MIME: Record<string, string> = {
  pdf: "application/pdf",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  heic: "image/heic",
  heif: "image/heif",
  webp: "image/webp",
  gif: "image/gif",
  bmp: "image/bmp",
  tif: "image/tiff",
  tiff: "image/tiff",
  xml: "application/xml",
  csv: "text/csv",
  ofx: "application/x-ofx",
  ofc: "application/x-ofc",
  xls: "application/vnd.ms-excel",
  xlsx: "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
  ods: "application/vnd.oasis.opendocument.spreadsheet",
  doc: "application/msword",
  docx: "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
  eml: "message/rfc822",
  msg: "application/vnd.ms-outlook",
  zip: "application/zip",
  rar: "application/vnd.rar",
  "7z": "application/x-7z-compressed",
  txt: "text/plain",
  html: "text/html",
  htm: "text/html",
  json: "application/json",
};

/** Classe de um caminho qualquer, descendo a partir da raiz do sistema. */
function classeDoCaminho(caminho: string): Classe {
  const partes = caminho.split("/").filter(Boolean);
  let classe: Classe | undefined;
  for (let i = 1; i <= partes.length; i++) {
    classe = classificarPasta(`/${partes.slice(0, i).join("/")}`, classe);
  }
  return classe ?? { camada: "B", raiz: "/" };
}

function dentroDe(caminho: string, raiz: string): boolean {
  return caminho === raiz || caminho.startsWith(`${raiz}/`);
}

/** Pasta onde as fotos de B são agregadas: até quatro níveis (/mnt/p/OneDrive/Familia). */
function pastaDeAgregacao(dir: string): string {
  return `/${dir.split("/").filter(Boolean).slice(0, 4).join("/")}`;
}

async function sha256DoArquivo(caminho: string): Promise<string> {
  const hasher = new Bun.CryptoHasher("sha256");
  const leitor = Bun.file(caminho).stream().getReader();
  for (;;) {
    const { done, value } = await leitor.read();
    if (done) break;
    hasher.update(value);
  }
  return hasher.digest("hex");
}

interface Agregado {
  motivo: string;
  arquivos: number;
  bytes: number;
}

class Scanner {
  private readonly cofre = caminhosDoCofre();
  private readonly raizesA: Set<string>;
  private readonly filaC: Array<[string, Classe]> = [];
  private readonly agregados = new Map<string, Agregado>();
  private readonly ignorados = new Map<string, number>();
  private readonly c = {
    vistos: 0,
    novos: 0,
    repetidos: 0,
    excluidos: 0,
    erros: 0,
    conteudosNovos: 0,
    copiados: 0,
    fotosContadas: 0,
    vazios: 0,
    grandes: 0,
    symlinks: 0,
  };
  private ultimoProgresso = Date.now();

  private readonly q: ReturnType<Scanner["prepararConsultas"]>;

  constructor(
    private readonly db: Database,
    private readonly fonteId: string,
    private readonly varreduraId: number,
    private readonly log: string,
    raiz: string,
  ) {
    this.raizesA = new Set(
      [...REGRAS_POR_CAMINHO]
        .filter(([caminho, regra]) => regra.camada === "A" && dentroDe(caminho, raiz))
        .map(([caminho]) => caminho)
        .filter((caminho) => caminho !== raiz && existsSync(caminho)),
    );
    this.q = this.prepararConsultas();
  }

  private prepararConsultas() {
    return {
      origemExiste: this.db.query("SELECT 1 FROM origem WHERE origin_key = ?"),
      achado: this.db.query("SELECT caminho_acervo FROM achado WHERE content_hash = ?"),
      inserirAchado: this.db.query(
        `INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada)
         VALUES (?, ?, ?, ?, ?)`,
      ),
      promoverParaA: this.db.query(
        "UPDATE achado SET camada = 'A' WHERE content_hash = ? AND camada = 'B'",
      ),
      marcarCopia: this.db.query("UPDATE achado SET caminho_acervo = ? WHERE content_hash = ?"),
      inserirOrigem: this.db.query(
        `INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id, origin_type,
           origin_key, local_filepath, local_mtime, filename, file_size_bytes, erro)
         VALUES (?, ?, ?, 'local_file', ?, ?, ?, ?, ?, ?)`,
      ),
      progresso: this.db.query(
        `UPDATE varredura SET vistos = ?, novos = ?, repetidos = ?, excluidos = ?, erros = ?,
           cursor = ? WHERE id = ?`,
      ),
    };
  }

  private escreverLog(linha: string): void {
    appendFileSync(this.log, `${new Date().toISOString()} ${linha}\n`);
  }

  private erro(caminho: string, e: unknown): void {
    this.c.erros++;
    this.escreverLog(`ERRO ${caminho}: ${e instanceof Error ? e.message : String(e)}`);
  }

  private salvarProgresso(cursor: string): void {
    const { vistos, novos, repetidos, excluidos, erros } = this.c;
    this.q.progresso.run(vistos, novos, repetidos, excluidos, erros, cursor, this.varreduraId);
    if (Date.now() - this.ultimoProgresso > 60_000) {
      this.ultimoProgresso = Date.now();
      const linha = `${this.fonteId}: ${vistos} vistos, ${novos} novos, ${this.c.copiados} copiados, ${erros} erros — ${cursor}`;
      console.log(linha);
      this.escreverLog(linha);
    }
  }

  private agregar(caminho: string, motivo: string, bytes: number): void {
    const chave = `${motivo}\u0000${caminho}`;
    const a = this.agregados.get(chave) ?? { motivo, arquivos: 0, bytes: 0 };
    a.arquivos++;
    a.bytes += bytes;
    this.agregados.set(chave, a);
  }

  async varrer(raiz: string): Promise<void> {
    for (const raizA of this.raizesA) await this.andar(raizA, classeDoCaminho(raizA), "A");

    const classeRaiz = classeDoCaminho(raiz);
    if (classeRaiz.ignorar) return;
    if (classeRaiz.camada === "C") this.filaC.push([raiz, classeRaiz]);
    else await this.andar(raiz, classeRaiz, "B");

    for (let item = this.filaC.shift(); item; item = this.filaC.shift()) {
      await this.andar(item[0], item[1], "C");
    }
  }

  private async andar(inicio: string, classeInicio: Classe, fase: "A" | "B" | "C"): Promise<void> {
    const pilha: Array<[string, Classe]> = [[inicio, classeInicio]];
    for (let item = pilha.pop(); item; item = pilha.pop()) {
      const [dir, classe] = item;
      let entradas: Dirent[];
      try {
        entradas = readdirSync(dir, { withFileTypes: true });
      } catch (e) {
        this.erro(dir, e);
        continue;
      }
      for (const ent of entradas) {
        const caminho = join(dir, ent.name);
        if (ent.isSymbolicLink()) {
          this.c.symlinks++;
          continue;
        }
        if (ent.isDirectory()) {
          let sub = classificarPasta(caminho, classe);
          if (sub.ignorar) continue;
          if (sub.camada === "C" && !REGRAS_POR_CAMINHO.has(caminho)) {
            // C decidido pelo nome da pasta (node_modules, .git, Daiane…): conta na
            // área-mãe em vez de abrir uma linha por pasta.
            sub = classe.camada === "C" ? classe : { ...sub, raiz: pastaDeAgregacao(caminho) };
          }
          if (fase !== "A" && this.raizesA.has(caminho)) continue; // já varrida na fase A
          if (fase === "B" && sub.camada === "C" && classe.camada !== "C") {
            this.filaC.push([caminho, sub]);
            continue;
          }
          pilha.push([caminho, sub]);
        } else if (ent.isFile()) {
          await this.arquivo(caminho, ent.name, classe);
        }
      }
      this.salvarProgresso(dir);
    }
  }

  private async arquivo(caminho: string, nome: string, classe: Classe): Promise<void> {
    this.c.vistos++;
    const tratamento = tratamentoDoArquivo(nome, classe);
    const ext = extensaoDe(nome);
    if (tratamento === "ignorar") {
      this.ignorados.set(ext || "(sem)", (this.ignorados.get(ext || "(sem)") ?? 0) + 1);
      return;
    }

    let st: Stats;
    try {
      st = statSync(caminho);
    } catch (e) {
      this.erro(caminho, e);
      return;
    }

    if (tratamento === "contar") {
      this.c.excluidos++;
      this.agregar(classe.raiz, classe.motivo ?? "fora", st.size);
      return;
    }
    if (tratamento === "foto-contada") {
      this.c.fotosContadas++;
      this.agregar(pastaDeAgregacao(dirname(caminho)), "fotos-fora-de-area-pessoal", st.size);
      return;
    }

    const originKey = `local:${caminho}:${st.mtimeMs}`;
    if (this.q.origemExiste.get(originKey)) {
      this.c.repetidos++;
      return;
    }
    if (st.size === 0) {
      this.c.vazios++;
      return;
    }

    const mtime = st.mtime.toISOString();
    if (st.size > LIMITE_HASH_BYTES) {
      this.c.grandes++;
      this.q.inserirOrigem.run(
        null,
        this.fonteId,
        this.varreduraId,
        originKey,
        caminho,
        mtime,
        nome,
        st.size,
        "acima-de-2GB",
      );
      return;
    }

    let hash: string;
    try {
      hash = await sha256DoArquivo(caminho);
    } catch (e) {
      this.erro(caminho, e);
      return;
    }

    const camada = classe.camada === "A" ? "A" : "B";
    const existente = this.q.achado.get(hash) as { caminho_acervo: string | null } | null;
    if (!existente) {
      this.q.inserirAchado.run(hash, st.size, MIME[ext] ?? null, ext || null, camada);
      this.c.conteudosNovos++;
    } else if (camada === "A") {
      this.q.promoverParaA.run(hash);
    }
    this.q.inserirOrigem.run(
      hash,
      this.fonteId,
      this.varreduraId,
      originKey,
      caminho,
      mtime,
      nome,
      st.size,
      null,
    );
    this.c.novos++;

    if (tratamento === "copiar" && !existente?.caminho_acervo) {
      this.copiarAoAcervo(caminho, hash, ext, st);
    }
  }

  private copiarAoAcervo(caminho: string, hash: string, ext: string, st: Stats): void {
    const relativo = caminhoNoAcervo(hash, ext);
    const destino = join(this.cofre.raiz, relativo);
    try {
      mkdirSync(dirname(destino), { recursive: true, mode: 0o700 });
      try {
        copyFileSync(caminho, destino, constants.COPYFILE_EXCL);
      } catch (e) {
        // Outro processo pode ter copiado o mesmo conteúdo um instante antes.
        if ((e as NodeJS.ErrnoException).code !== "EEXIST") throw e;
      }
      if (statSync(destino).size !== st.size) throw new Error("cópia com tamanho diferente");
      chmodSync(destino, 0o600);
      utimesSync(destino, st.atime, st.mtime);
      this.q.marcarCopia.run(relativo, hash);
      this.c.copiados++;
    } catch (e) {
      this.erro(caminho, e);
    }
  }

  finalizar(status: "concluida" | "interrompida" | "falhou"): void {
    const inserir = this.db.query(
      `INSERT OR REPLACE INTO area_excluida (fonte_id, varredura_id, caminho, motivo, arquivos, bytes)
       VALUES (?, ?, ?, ?, ?, ?)`,
    );
    this.db.transaction(() => {
      for (const [chave, a] of this.agregados) {
        const caminho = chave.slice(chave.indexOf("\u0000") + 1);
        inserir.run(this.fonteId, this.varreduraId, caminho, a.motivo, a.arquivos, a.bytes);
      }
    })();
    const ignorados = Object.fromEntries(
      [...this.ignorados].sort((x, y) => y[1] - x[1]).slice(0, 40),
    );
    const { vistos, novos, repetidos, excluidos, erros } = this.c;
    this.db
      .query(
        `UPDATE varredura SET status = ?, concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
           vistos = ?, novos = ?, repetidos = ?, excluidos = ?, erros = ?, resumo = ? WHERE id = ?`,
      )
      .run(
        status,
        vistos,
        novos,
        repetidos,
        excluidos,
        erros,
        JSON.stringify({ ...this.c, ignorados }),
        this.varreduraId,
      );
    const linha = `${this.fonteId} ${status}: ${JSON.stringify(this.c)}`;
    console.log(linha);
    this.escreverLog(linha);
  }
}

function lerArgs(argv: string[]): { fonte: string; raiz: string } {
  const valor = (flag: string) => {
    const i = argv.indexOf(flag);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  const fonte = valor("--fonte");
  const definida = FONTES.find((f) => f.id === fonte && f.tipo === "disco");
  if (!definida?.raiz) {
    const discos = FONTES.filter((f) => f.tipo === "disco").map((f) => f.id);
    throw new Error(`--fonte deve ser um disco: ${discos.join(", ")}`);
  }
  const raiz = valor("--raiz") ?? definida.raiz;
  if (!dentroDe(raiz, definida.raiz)) throw new Error(`--raiz fora de ${definida.raiz}`);
  return { fonte: definida.id, raiz };
}

async function main(): Promise<void> {
  const { fonte, raiz } = lerArgs(process.argv.slice(2));
  const cofre = caminhosDoCofre();
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);

  // Varredura anterior da mesma fonte que ficou "rodando" morreu sem avisar.
  db.query(
    "UPDATE varredura SET status = 'interrompida' WHERE fonte_id = ? AND etapa = 'E1' AND status = 'rodando'",
  ).run(fonte);
  const { id } = db
    .query(
      `INSERT INTO varredura (fonte_id, etapa, escopo, responsavel, parametros)
       VALUES (?, 'E1', ?, ?, ?) RETURNING id`,
    )
    .get(fonte, raiz, RESPONSAVEL, JSON.stringify({ limiteHashBytes: LIMITE_HASH_BYTES })) as {
    id: number;
  };

  const log = join(cofre.logs, `E1-${fonte.replace(":", "-")}-${id}.log`);
  const scanner = new Scanner(db, fonte, id, log, raiz);
  const interromper = () => {
    scanner.finalizar("interrompida");
    process.exit(130);
  };
  process.on("SIGINT", interromper);
  process.on("SIGTERM", interromper);

  console.log(`${fonte}: varredura ${id} de ${raiz} (log em ${log})`);
  try {
    await scanner.varrer(raiz);
    scanner.finalizar("concluida");
  } catch (e) {
    scanner.finalizar("falhou");
    throw e;
  } finally {
    db.close();
  }
}

main().catch((e: unknown) => {
  console.error(e instanceof Error ? e.message : String(e));
  process.exit(1);
});
