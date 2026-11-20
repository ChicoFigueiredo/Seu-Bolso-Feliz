/**
 * aplicarSchema sob o Rust dono do esquema (spec 06, INV-02 lado Bun). Roda no
 * `bun test` (usa bun:sqlite); o nome `_test` fica fora do vitest, que roda em Node.
 * Só banco temporário com dados fictícios.
 */
import { Database } from "bun:sqlite";
import { afterEach, describe, expect, it } from "bun:test";
import { mkdtempSync, rmSync } from "node:fs";
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
    expect(() => aplicarSchema(db)).toThrow(/versão de esquema 5/);
    expect(objetos(db)).toEqual([]);
    db.close();
  });
});
