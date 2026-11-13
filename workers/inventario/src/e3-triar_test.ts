/**
 * A decisão do usuário prevalece (spec 06, INV-06): a E3 rodada depois de uma
 * decisão do desktop não muda o achado. Roda no `bun test` (bun:sqlite); só
 * banco temporário com dados fictícios.
 */
import type { Database } from "bun:sqlite";
import { afterEach, describe, expect, it } from "bun:test";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { triarAchados } from "./e3-triar";

const pastas: string[] = [];

afterEach(() => {
  for (const p of pastas.splice(0)) rmSync(p, { recursive: true, force: true });
});

/** A migração 2 do Rust, a mesma que o desktop aplica. */
const MIGRACAO_2 = (() => {
  const url = new URL(
    "../../../crates/sbf-inventario/sql/0002-decisoes-e-lotes.sql",
    import.meta.url,
  );
  // Na branch do inventário o crate Rust não existe: usa só a parte da migração
  // 2 de que os scripts Bun dependem (as colunas decidido_por).
  if (existsSync(url)) return readFileSync(url, "utf8");
  return `ALTER TABLE achado ADD COLUMN decidido_por TEXT;
          ALTER TABLE registro ADD COLUMN decidido_por TEXT;
          UPDATE achado SET decidido_por = classificado_por;
          UPDATE registro SET decidido_por = extraido_por;`;
})();

const hash = (n: number) => n.toString(16).padStart(2, "0").repeat(32);

function banco({ migradoPeloRust }: { migradoPeloRust: boolean }): Database {
  const pasta = mkdtempSync(join(tmpdir(), "inventario-teste-"));
  pastas.push(pasta);
  const db = abrirBanco(join(pasta, "inventario.sqlite"));
  aplicarSchema(db);
  if (migradoPeloRust) {
    db.exec(MIGRACAO_2);
    db.exec("PRAGMA user_version = 2");
  }
  db.exec(`
    INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:teste', 'disco', 'disco fictício');
    INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
      VALUES (1, 'disco:teste', 'E1', '/ficticio', 'teste@1');
  `);
  return db;
}

function achado(
  db: Database,
  n: number,
  {
    estado,
    documentType = null,
    decididoPor = null,
    texto = null,
  }: {
    estado: string;
    documentType?: string | null;
    decididoPor?: string | null;
    texto?: string | null;
  },
): void {
  const temDecidido = decididoPor !== null;
  db.query(
    `INSERT INTO achado (content_hash, file_size_bytes, extensao, camada, estado, document_type
                         ${temDecidido ? ", decidido_por" : ""})
     VALUES (?, 1000, 'pdf', 'A', ?, ? ${temDecidido ? ", ?" : ""})`,
  ).run(hash(n), estado, documentType, ...(temDecidido ? [decididoPor] : []));
  db.query(
    `INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
                         local_filepath, filename)
     VALUES (?, 'disco:teste', 1, 'local_file', ?, ?, ?)`,
  ).run(hash(n), `local:/ficticio/${n}.pdf:1`, `/ficticio/boleto-${n}.pdf`, `boleto-${n}.pdf`);
  if (texto !== null) {
    db.query("INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?, 'pdftotext', ?)").run(
      hash(n),
      texto,
    );
  }
}

function linha(db: Database, n: number) {
  return db
    .query(
      "SELECT estado, document_type, classificado_por, notas FROM achado WHERE content_hash = ?",
    )
    .get(hash(n)) as Record<string, unknown>;
}

const RODADA = { simular: false, duvidasComTexto: false, limite: Infinity };
const silencio = () => {};

describe("e3-triar", () => {
  it("inv06 pula achado novo decidido pelo usuário e tria os outros", () => {
    const db = banco({ migradoPeloRust: true });
    achado(db, 1, { estado: "novo", documentType: "recibo", decididoPor: "usuario" });
    achado(db, 2, { estado: "novo" });
    const antes = linha(db, 1);

    const r = triarAchados(db, RODADA, silencio);

    expect(r.alvo).toBe(1);
    expect(linha(db, 1)).toEqual(antes);
    const outro = linha(db, 2);
    expect(outro.estado).not.toBe("novo");
    expect(outro.classificado_por).toBe("regras@2");
    const decidido = db
      .query("SELECT decidido_por FROM achado WHERE content_hash = ?")
      .get(hash(2)) as { decidido_por: string };
    expect(decidido.decidido_por).toBe("regras@2");
    db.close();
  });

  it("inv06 a passada das dúvidas com texto também pula a decisão do usuário", () => {
    const db = banco({ migradoPeloRust: true });
    const texto = "BOLETO fictício vencimento 25/10/2026 valor R$ 123,45 linha digitável";
    achado(db, 3, { estado: "duvida", decididoPor: "usuario", texto });
    achado(db, 4, { estado: "duvida", decididoPor: "regras@2", texto });
    const antes = linha(db, 3);

    const r = triarAchados(db, { ...RODADA, duvidasComTexto: true }, silencio);

    expect(r.alvo).toBe(1);
    expect(linha(db, 3)).toEqual(antes);
    expect(linha(db, 4).notas).toContain("E3 regras@2");
    db.close();
  });

  it("num banco só do Bun (sem decidido_por) tria como antes", () => {
    const db = banco({ migradoPeloRust: false });
    achado(db, 5, { estado: "novo" });
    const r = triarAchados(db, RODADA, silencio);
    expect(r.alvo).toBe(1);
    expect(linha(db, 5).classificado_por).toBe("regras@2");
    db.close();
  });

  it("simular não grava nada", () => {
    const db = banco({ migradoPeloRust: true });
    achado(db, 6, { estado: "novo" });
    const antes = linha(db, 6);
    triarAchados(db, { ...RODADA, simular: true }, silencio);
    expect(linha(db, 6)).toEqual(antes);
    db.close();
  });
});
