/**
 * aplicarSchema sob o Rust dono do esquema (spec 06, INV-02 lado Bun). Roda no
 * `bun test` (usa bun:sqlite); o nome `_test` fica fora do vitest, que roda em Node.
 * Só banco temporário com dados fictícios.
 */
import { Database } from "bun:sqlite";
import { afterEach, describe, expect, it } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { VERSAO_RUST_CONHECIDA, abrirBanco, aplicarSchema, versaoDoBanco } from "./banco";

const pastas: string[] = [];

function bancoTemporario(): Database {
  const pasta = mkdtempSync(join(tmpdir(), "inventario-teste-"));
  pastas.push(pasta);
  return abrirBanco(join(pasta, "inventario.sqlite"));
}

afterEach(() => {
  for (const p of pastas.splice(0)) rmSync(p, { recursive: true, force: true });
});

function objetos(db: Database): string[] {
  return (
    db.query("SELECT type || ':' || name AS n FROM sqlite_master ORDER BY 1").all() as {
      n: string;
    }[]
  ).map((l) => l.n);
}

describe("aplicarSchema", () => {
  it("num banco do Bun (user_version 0) cria o esquema e grava schema_versao", () => {
    const db = bancoTemporario();
    aplicarSchema(db);
    expect(versaoDoBanco(db)).toBe(0);
    const meta = db.query("SELECT valor FROM meta WHERE chave = 'schema_versao'").get() as {
      valor: string;
    };
    expect(meta.valor).toBe("2");
    expect(objetos(db)).toContain("table:achado");
    db.close();
  });

  it("inv02 com user_version >= 1 não roda DDL nem grava meta", () => {
    for (const versao of [1, 2]) {
      const db = bancoTemporario();
      db.exec("CREATE TABLE meta (chave TEXT PRIMARY KEY, valor TEXT NOT NULL)");
      db.exec(`PRAGMA user_version = ${versao}`);
      const antes = objetos(db);
      aplicarSchema(db);
      expect(objetos(db)).toEqual(antes);
      expect(db.query("SELECT COUNT(*) AS n FROM meta").get()).toEqual({ n: 0 });
      db.close();
    }
  });

  it("inv02 recusa banco numa versão do Rust que não conhece", () => {
    const db = bancoTemporario();
    db.exec(`PRAGMA user_version = ${VERSAO_RUST_CONHECIDA + 1}`);
    expect(() => aplicarSchema(db)).toThrow(/versão de esquema 6/);
    expect(objetos(db)).toEqual([]);
    db.close();
  });
});

describe("migração 5 (impressão do texto) vista pelos scripts do Bun", () => {
  /** O que o e3-extrair.ts e o e3-ocr.ts gravam em `texto`, com os gatilhos da v5 no banco. */
  it("o INSERT OR IGNORE e o upsert dos scripts seguem valendo e só deixam a linha pendente", () => {
    const db = bancoTemporario();
    db.exec(`CREATE TABLE texto (
      content_hash TEXT PRIMARY KEY, metodo TEXT NOT NULL, conteudo TEXT NOT NULL,
      extraido_em TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')))`);
    db.exec(
      readFileSync(
        new URL("../../../crates/sbf-inventario/sql/0005-impressao-do-texto.sql", import.meta.url),
        "utf8",
      ),
    );
    const pendente = (hash: string) =>
      db
        .query("SELECT impressao, pendente FROM impressao_do_texto WHERE content_hash = ?")
        .get(hash);

    // e3-extrair.ts: INSERT OR IGNORE (a segunda gravação é ignorada).
    const inserir = db.query(
      "INSERT OR IGNORE INTO texto (content_hash, metodo, conteudo) VALUES (?, ?, ?)",
    );
    inserir.run("h1", "sem-extrator:jpg", "");
    inserir.run("h1", "pdftotext", "outro");
    expect(pendente("h1")).toEqual({ impressao: null, pendente: 1 });

    // e3-ocr.ts: o upsert que troca o "sem-extrator" vazio pelo texto do OCR.
    db.query(
      `INSERT INTO texto (content_hash, metodo, conteudo) VALUES (?, ?, ?)
       ON CONFLICT (content_hash) DO UPDATE SET
         metodo = excluded.metodo, conteudo = excluded.conteudo,
         extraido_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
       WHERE texto.metodo LIKE 'sem-extrator:%'`,
    ).run("h1", "ocr-por", "texto lido pelo ocr");
    expect(db.query("SELECT conteudo FROM texto WHERE content_hash = 'h1'").get()).toEqual({
      conteudo: "texto lido pelo ocr",
    });
    expect(pendente("h1")).toEqual({ impressao: null, pendente: 1 });
    db.close();
  });
});
