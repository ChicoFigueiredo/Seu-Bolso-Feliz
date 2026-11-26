/**
 * O índice do inventário: um SQLite dentro do cofre (ADR-011). WAL para que
 * vários coletores escrevam enquanto a revisão lê.
 */
import { Database } from "bun:sqlite";
import { readFileSync } from "node:fs";
import { caminhosDoCofre } from "./cofre";

export const VERSAO_DO_SCHEMA = "2";

/**
 * A maior versão de esquema do Rust (PRAGMA user_version) que estes scripts
 * conhecem. Transitório (spec 06): a partir da fatia 1 o desktop em Rust é o
 * dono do esquema. Com user_version >= 1 o Bun não roda DDL; acima desta
 * versão, recusa o banco.
 */
export const VERSAO_RUST_CONHECIDA = 5;

export function versaoDoBanco(db: Database): number {
  const linha = db.query("PRAGMA user_version").get() as { user_version: number };
  return linha.user_version;
}

export function temColuna(db: Database, tabela: string, coluna: string): boolean {
  const colunas = db.query(`PRAGMA table_info(${tabela})`).all() as { name: string }[];
  return colunas.some((c) => c.name === coluna);
}

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
  if (!temColuna(db, tabela, coluna)) {
    db.exec(`ALTER TABLE ${tabela} ADD COLUMN ${coluna} ${tipo}`);
  }
}

export function aplicarSchema(db: Database): void {
  const versao = versaoDoBanco(db);
  if (versao > VERSAO_RUST_CONHECIDA) {
    throw new Error(
      `inventário na versão de esquema ${versao}, mas estes scripts só conhecem até a ` +
        `${VERSAO_RUST_CONHECIDA}: atualize o repositório antes de rodar`,
    );
  }
  // O esquema é do Rust (migrações por user_version): nada de DDL aqui.
  if (versao >= 1) return;
  db.exec(readFileSync(new URL("./schema.sql", import.meta.url), "utf8"));
  garantirColuna(db, "varredura", "resumo", "TEXT");
  // O tipo que o registro tinha antes da E3 reclassificar (compra, debito…),
  // para uma regra nova poder recalcular a partir dele.
  garantirColuna(db, "registro", "tipo_original", "TEXT");
  // Do bruto da Pluggy (coletar-pluggy-bruto): quem recebeu/pagou, data
  // original da compra no cartão, CNPJ do estabelecimento e a fatura.
  for (const coluna of [
    "contraparte",
    "contraparte_doc",
    "data_compra",
    "merchant_cnpj",
    "fatura_ref",
  ]) {
    garantirColuna(db, "registro", coluna, "TEXT");
  }
  db.query("INSERT OR REPLACE INTO meta (chave, valor) VALUES (?, ?)").run(
    "schema_versao",
    VERSAO_DO_SCHEMA,
  );
}
