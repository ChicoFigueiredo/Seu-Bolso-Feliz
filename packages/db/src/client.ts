import { neon } from "@neondatabase/serverless";
import { drizzle as drizzleNeonHttp, type NeonHttpDatabase } from "drizzle-orm/neon-http";
import { drizzle as drizzlePg } from "drizzle-orm/node-postgres";
import { Pool } from "pg";
import * as schema from "./schema";

let _db: NeonHttpDatabase<typeof schema> | null = null;

// A API HTTP da Neon (`neon()`) só fala com o proxy da própria Neon — não
// com um Postgres genérico. Testes de integração locais usam um container
// Docker comum (ver .superpowers/sdd/.../progress.md), então a mesma
// DATABASE_URL apontando pra `localhost`/`127.0.0.1` precisa do driver de
// wire protocol real (`pg`) em vez do driver HTTP.
//
// O retorno é fixado em `NeonHttpDatabase` (não a união dos dois tipos):
// uma união faz o TypeScript resolver `.returning({subset})` contra a
// interseção dos overloads dos dois drivers em todo call site do repo,
// colapsando pra uma assinatura sem argumentos (`TS2554`). Os dois
// drivers expõem a mesma API de query builder (select/insert/update/
// delete/returning) — só o transporte muda — e isso já foi confirmado
// empiricamente: a suíte de integração acumulada roda 19/19 verde contra
// Postgres local usando exatamente esse cast.
function buildDb(): NeonHttpDatabase<typeof schema> {
  const url = process.env.DATABASE_URL;
  if (!url) {
    throw new Error("DATABASE_URL é obrigatório para @sbf/db");
  }
  const { hostname } = new URL(url);
  if (hostname === "localhost" || hostname === "127.0.0.1") {
    const pool = new Pool({ connectionString: url });
    return drizzlePg(pool, { schema }) as unknown as NeonHttpDatabase<typeof schema>;
  }
  const sql = neon(url);
  return drizzleNeonHttp(sql, { schema });
}

export function getDb(): NeonHttpDatabase<typeof schema> {
  if (!_db) _db = buildDb();
  return _db;
}
