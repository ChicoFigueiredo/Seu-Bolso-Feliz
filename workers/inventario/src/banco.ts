/**
 * O índice do inventário: um SQLite dentro do cofre (ADR-011). WAL para que
 * vários coletores escrevam enquanto a revisão lê.
 */
import { Database } from "bun:sqlite";
import { readFileSync } from "node:fs";
import { caminhosDoCofre } from "./cofre";

export const VERSAO_DO_SCHEMA = "2";

export function abrirBanco(caminho = caminhosDoCofre().banco): Database {
  const db = new Database(caminho, { create: true, strict: true });
  db.exec("PRAGMA journal_mode = WAL;");
  // Um processo por disco escreve ao mesmo tempo: NORMAL é seguro com WAL e
  // evita um fsync por arquivo varrido.
  db.exec("PRAGMA synchronous = NORMAL;");
  db.exec("PRAGMA foreign_keys = ON;");
  db.exec("PRAGMA busy_timeout = 30000;");
  return db;
}

/** CREATE TABLE IF NOT EXISTS não acrescenta coluna nova a tabela que já existe. */
function garantirColuna(db: Database, tabela: string, coluna: string, tipo: string): void {
  const colunas = db.query(`PRAGMA table_info(${tabela})`).all() as { name: string }[];
  if (!colunas.some((c) => c.name === coluna)) {
    db.exec(`ALTER TABLE ${tabela} ADD COLUMN ${coluna} ${tipo}`);
  }
}

export function aplicarSchema(db: Database): void {
  db.exec(readFileSync(new URL("./schema.sql", import.meta.url), "utf8"));
  garantirColuna(db, "varredura", "resumo", "TEXT");
  // O tipo que o registro tinha antes da E3 reclassificar (compra, debito…),
  // para uma regra nova poder recalcular a partir dele.
  garantirColuna(db, "registro", "tipo_original", "TEXT");
  db.query("INSERT OR REPLACE INTO meta (chave, valor) VALUES (?, ?)").run(
    "schema_versao",
    VERSAO_DO_SCHEMA,
  );
}
