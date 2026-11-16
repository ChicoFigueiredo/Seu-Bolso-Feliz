/**
 * E2: coletor local do Gmail (ADR-011). Busca financeira em todo o histórico,
 * do mais novo para o mais antigo. Para cada mensagem guarda no cofre o .eml
 * bruto e cada anexo, como achados com hash; remetente, assunto, data e IDs
 * vão na origem. Nada vai ao Neon nem ao Blob.
 *
 *   bun run workers/inventario/src/coletar-gmail.ts [--limite N] [--query "..."]
 *
 * Retoma de onde parou: a página da lista fica no cursor da varredura e uma
 * mensagem já guardada é pulada sem nova chamada.
 */
import { createGmailClient, type GmailClient } from "@sbf/worker-gmail-scanner/src/gmail-client";
import type { Database } from "bun:sqlite";
import { appendFileSync, existsSync, mkdirSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { extensaoDe } from "./areas";
import { abrirBanco, aplicarSchema } from "./banco";
import { caminhoNoAcervo, caminhosDoCofre } from "./cofre";
import { anexosDe, cabecalho, consultaFinanceira, erroTransitorio } from "./gmail-mensagem";

const RESPONSAVEL = "coletar-gmail@1";
/** Mensagens em paralelo; o ritmo de verdade é o de aguardarVez. */
const PARALELO = 2;

function sha256(bytes: Uint8Array): string {
  return new Bun.CryptoHasher("sha256").update(bytes).digest("hex");
}

/**
 * A cota do Gmail é 15.000 unidades por minuto por usuário, e cada chamada
 * que fazemos custa 5. Uma chamada a cada 50 ms dá 6.000/min: folga para o
 * resto da conta e para os picos de anexos.
 */
const INTERVALO_MS = 50;
let proximaChamada = 0;
async function aguardarVez(): Promise<void> {
  const agora = Date.now();
  const espera = Math.max(0, proximaChamada - agora);
  proximaChamada = Math.max(agora, proximaChamada) + INTERVALO_MS;
  if (espera) await Bun.sleep(espera);
}

async function comRetentativa<T>(fazer: () => Promise<T>): Promise<T> {
  for (let tentativa = 1; ; tentativa++) {
    await aguardarVez();
    try {
      // fetch sem tempo limite pode ficar pendurado para sempre: foi assim que
      // o coletor parou sem erro depois de 21 mil mensagens.
      return await Promise.race([
        fazer(),
        new Promise<never>((_, rejeitar) =>
          setTimeout(() => rejeitar(new Error("sem resposta em 60 s")), 60_000),
        ),
      ]);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      if (!erroTransitorio(msg) || tentativa >= 6) throw e;
      // A janela da cota é de um minuto: esperar menos que isso só gasta tentativa.
      await Bun.sleep(/\(403\)|\(429\)/.test(msg) ? 65_000 : 2 ** tentativa * 1000);
    }
  }
}

interface Meta {
  messageId: string;
  threadId: string;
  from: string | null;
  subject: string | null;
  date: string;
  labels: string;
}

class Coletor {
  private readonly cofre = caminhosDoCofre();
  readonly c = { vistos: 0, novos: 0, repetidos: 0, anexos: 0, erros: 0 };
  private readonly q: ReturnType<Coletor["consultas"]>;

  constructor(
    private readonly db: Database,
    private readonly gmail: GmailClient,
    private readonly varreduraId: number,
    private readonly log: string,
  ) {
    this.q = this.consultas();
  }

  private consultas() {
    const db = this.db;
    return {
      jaGuardada: db.query(
        "SELECT 1 FROM origem WHERE origin_type = 'gmail' AND gmail_message_id = ? AND mensagem_ref = 'eml'",
      ),
      achado: db.query("SELECT caminho_acervo FROM achado WHERE content_hash = ?"),
      inserirAchado: db.query(
        `INSERT INTO achado (content_hash, file_size_bytes, mime_type, extensao, camada, caminho_acervo)
         VALUES (?, ?, ?, ?, 'A', ?)`,
      ),
      promover: db.query(
        "UPDATE achado SET camada = 'A', caminho_acervo = coalesce(caminho_acervo, ?) WHERE content_hash = ?",
      ),
      marcarRefazer: db.query(
        `INSERT INTO refazer (fonte_id, referencia, motivo) VALUES ('email:gmail', ?, ?)
         ON CONFLICT (fonte_id, referencia) DO UPDATE SET motivo = excluded.motivo, feito_em = NULL`,
      ),
      refeito: db.query(
        `UPDATE refazer SET feito_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
          WHERE fonte_id = 'email:gmail' AND referencia = ? AND feito_em IS NULL`,
      ),
      inserirOrigem: db.query(
        `INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
           filename, file_size_bytes, gmail_message_id, gmail_thread_id, gmail_attachment_id,
           gmail_from, gmail_subject, gmail_date, gmail_label, mensagem_ref)
         VALUES (?, 'email:gmail', ?, 'gmail', ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      ),
    };
  }

  private escreverLog(linha: string): void {
    appendFileSync(this.log, `${new Date().toISOString()} ${linha}\n`);
  }

  /** Guarda bytes no acervo (uma vez por hash) e registra achado + origem. */
  private guardar(
    bytes: Uint8Array,
    nome: string,
    mime: string | null,
    meta: Meta,
    ref: "eml" | "anexo",
    attachmentId: string | null,
  ): void {
    const hash = sha256(bytes);
    const ext = extensaoDe(nome);
    const relativo = caminhoNoAcervo(hash, ext);
    const destino = join(this.cofre.raiz, relativo);
    if (!existsSync(destino)) {
      mkdirSync(dirname(destino), { recursive: true, mode: 0o700 });
      writeFileSync(destino, bytes, { mode: 0o600 });
    }
    if (this.q.achado.get(hash)) this.q.promover.run(relativo, hash);
    else this.q.inserirAchado.run(hash, bytes.byteLength, mime, ext || null, relativo);
    this.q.inserirOrigem.run(
      hash,
      this.varreduraId,
      `gmail:${meta.messageId}:${hash}`,
      nome,
      bytes.byteLength,
      meta.messageId,
      meta.threadId,
      attachmentId,
      meta.from,
      meta.subject,
      meta.date,
      meta.labels,
      ref,
    );
  }

  async mensagem(id: string): Promise<void> {
    this.c.vistos++;
    if (this.q.jaGuardada.get(id)) {
      this.c.repetidos++;
      this.q.refeito.run(id);
      return;
    }
    try {
      const [cheia, bruta] = await Promise.all([
        comRetentativa(() => this.gmail.getMessage(id)),
        comRetentativa(() => this.gmail.getRawMessage(id)),
      ]);
      const meta: Meta = {
        messageId: id,
        threadId: cheia.threadId,
        from: cabecalho(cheia.payload, "From"),
        subject: cabecalho(cheia.payload, "Subject"),
        date: new Date(Number(cheia.internalDate)).toISOString(),
        labels: (cheia.labelIds ?? []).join(","),
      };
      // Anexos antes do .eml: o .eml marca a mensagem como concluída.
      for (const a of anexosDe(cheia.payload)) {
        const dados = a.attachmentId
          ? (await comRetentativa(() => this.gmail.getAttachment(id, a.attachmentId!))).data
          : a.data!;
        this.guardar(
          Buffer.from(dados, "base64url"),
          a.filename,
          a.mimeType,
          meta,
          "anexo",
          a.attachmentId ?? null,
        );
        this.c.anexos++;
      }
      this.guardar(
        Buffer.from(bruta.raw, "base64url"),
        `${id}.eml`,
        "message/rfc822",
        meta,
        "eml",
        null,
      );
      this.c.novos++;
      this.q.refeito.run(id);
    } catch (e) {
      this.c.erros++;
      const msg = e instanceof Error ? e.message : String(e);
      this.q.marcarRefazer.run(id, msg.slice(0, 200));
      this.escreverLog(`ERRO mensagem ${id}: ${msg}`);
    }
  }
}

function lerArgs(argv: string[]) {
  const valor = (flag: string) => {
    const i = argv.indexOf(flag);
    return i >= 0 ? argv[i + 1] : undefined;
  };
  return {
    limite: Number(valor("--limite") ?? Number.POSITIVE_INFINITY),
    query: valor("--query") ?? consultaFinanceira(),
  };
}

async function main(): Promise<void> {
  const { limite, query } = lerArgs(process.argv.slice(2));
  const cofre = caminhosDoCofre();
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);

  // Retoma a última varredura não concluída com a mesma busca.
  const anterior = db
    .query(
      `SELECT id, cursor FROM varredura WHERE fonte_id = 'email:gmail' AND etapa = 'E2'
         AND status IN ('rodando', 'interrompida') AND escopo = ? ORDER BY id DESC LIMIT 1`,
    )
    .get(query) as { id: number; cursor: string | null } | null;
  const varreduraId =
    anterior?.id ??
    (
      db
        .query(
          `INSERT INTO varredura (fonte_id, etapa, escopo, responsavel) VALUES ('email:gmail', 'E2', ?, ?)
           RETURNING id`,
        )
        .get(query, RESPONSAVEL) as { id: number }
    ).id;
  db.query("UPDATE varredura SET status = 'rodando' WHERE id = ?").run(varreduraId);

  const log = join(cofre.logs, `E2-gmail-${varreduraId}.log`);
  const gmail = createGmailClient();
  const coletor = new Coletor(db, gmail, varreduraId, log);
  const salvar = (status: string, cursor: string | null) => {
    const { vistos, novos, repetidos, erros } = coletor.c;
    db.query(
      `UPDATE varredura SET status = ?, cursor = ?, vistos = vistos + ?, novos = novos + ?,
         repetidos = repetidos + ?, erros = erros + ?, resumo = ?,
         concluida_em = CASE WHEN ? = 'concluida' THEN strftime('%Y-%m-%dT%H:%M:%fZ', 'now') END
       WHERE id = ?`,
    ).run(
      status,
      cursor,
      vistos,
      novos,
      repetidos,
      erros,
      JSON.stringify(coletor.c),
      status,
      varreduraId,
    );
    Object.assign(coletor.c, { vistos: 0, novos: 0, repetidos: 0, erros: 0 });
  };

  let pagina: string | undefined = anterior?.cursor ?? undefined;
  const interromper = () => {
    salvar("interrompida", pagina ?? null);
    process.exit(130);
  };
  process.on("SIGINT", interromper);
  process.on("SIGTERM", interromper);
  console.log(`gmail: varredura ${varreduraId}${anterior ? " (retomada)" : ""}, log em ${log}`);

  let total = 0;
  try {
    // Primeiro o que ficou para trás em rodadas anteriores (cota, rede).
    const atrasadas = (
      db
        .query("SELECT referencia FROM refazer WHERE fonte_id = 'email:gmail' AND feito_em IS NULL")
        .all() as { referencia: string }[]
    ).map((r) => r.referencia);
    for (let i = 0; i < atrasadas.length; i += PARALELO) {
      await Promise.all(atrasadas.slice(i, i + PARALELO).map((id) => coletor.mensagem(id)));
    }
    if (atrasadas.length) console.log(`gmail: ${atrasadas.length} mensagens atrasadas refeitas`);
    do {
      const lista = await comRetentativa(() =>
        gmail.listMessages({ q: query, maxResults: 100, pageToken: pagina }),
      );
      const ids = (lista.messages ?? []).map((m) => m.id);
      for (let i = 0; i < ids.length && total < limite; i += PARALELO) {
        const lote = ids.slice(i, Math.min(i + PARALELO, i + (limite - total)));
        await Promise.all(lote.map((id) => coletor.mensagem(id)));
        total += lote.length;
      }
      pagina = lista.nextPageToken;
      salvar("rodando", pagina ?? null);
      console.log(`gmail: ${total} mensagens processadas`);
    } while (pagina && total < limite);
  } catch (e) {
    // Token vencido, cota estourada: a varredura fica retomável a partir da página salva.
    appendFileSync(
      log,
      `${new Date().toISOString()} FALHOU: ${e instanceof Error ? e.message : String(e)}\n`,
    );
    salvar("interrompida", pagina ?? null);
    db.close();
    throw e;
  }

  salvar(pagina ? "interrompida" : "concluida", pagina ?? null);
  console.log(`gmail: ${pagina ? "parou no limite" : "concluída"} com ${total} mensagens`);
  db.close();
}

if (import.meta.main) {
  main().catch((e: unknown) => {
    console.error(e instanceof Error ? e.message : String(e));
    process.exit(1);
  });
}
