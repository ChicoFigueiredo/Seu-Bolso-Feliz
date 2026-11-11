/**
 * O índice do inventário: um SQLite dentro do cofre (ADR-011). WAL para que
 * vários coletores escrevam enquanto a revisão lê.
 */
import { Database } from "bun:sqlite";
import { readFileSync } from "node:fs";
import { caminhosDoCofre } from "./cofre";

export const VERSAO_DO_SCHEMA = "1";

export function abrirBanco(caminho = caminhosDoCofre().banco): Database {
  const db = new Database(caminho, { create: true, strict: true });
  db.exec("PRAGMA journal_mode = WAL;");
  db.exec("PRAGMA foreign_keys = ON;");
  db.exec("PRAGMA busy_timeout = 10000;");
  return db;
}

export function aplicarSchema(db: Database): void {
  db.exec(readFileSync(new URL("./schema.sql", import.meta.url), "utf8"));
  db.query("INSERT OR REPLACE INTO meta (chave, valor) VALUES (?, ?)").run(
    "schema_versao",
    VERSAO_DO_SCHEMA,
  );
}
