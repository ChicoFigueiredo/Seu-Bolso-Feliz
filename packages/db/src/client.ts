import { neon } from "@neondatabase/serverless";
import { drizzle as drizzleNeonHttp } from "drizzle-orm/neon-http";
import { drizzle as drizzlePg } from "drizzle-orm/node-postgres";
import { Pool } from "pg";
import * as schema from "./schema";

let _db: ReturnType<typeof buildDb> | null = null;

// A API HTTP da Neon (`neon()`) só fala com o proxy da própria Neon — não
// com um Postgres genérico. Testes de integração locais usam um container
// Docker comum (ver .superpowers/sdd/.../progress.md), então a mesma
// DATABASE_URL apontando pra `localhost`/`127.0.0.1` precisa do driver de
// wire protocol real (`pg`) em vez do driver HTTP.
function buildDb() {
  const url = process.env.DATABASE_URL;
  if (!url) {
    throw new Error("DATABASE_URL é obrigatório para @sbf/db");
  }
  const { hostname } = new URL(url);
  if (hostname === "localhost" || hostname === "127.0.0.1") {
    const pool = new Pool({ connectionString: url });
    return drizzlePg(pool, { schema });
  }
  const sql = neon(url);
  return drizzleNeonHttp(sql, { schema });
}

export function getDb() {
  if (!_db) _db = buildDb();
  return _db;
}
