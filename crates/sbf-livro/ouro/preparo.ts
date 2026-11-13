// Preload dos geradores de ouro que importam Server Actions do web (`bun --preload`).
//
// O web roda no Next com sessão e Neon; aqui ele roda fora dos dois:
//  - `@/lib/auth`: `requireUser()` devolve o dono fictício de `SBF_OURO_DONO`.
//  - `@sbf/db`: o mesmo `schema.ts` deste worktree; `getDb()` fala com o Postgres local
//    de `SBF_OURO_PG` pelo driver SQL do próprio Bun (o worktree não tem o `pg` instalado).
//    Sem `SBF_OURO_PG`, `getDb()` falha: nenhum gerador toca banco por acidente.
// Nunca aponte `SBF_OURO_PG` para o Neon: o gerador recusa endereço que não seja local.
import { plugin } from "bun";

const raiz = new URL("../../../", import.meta.url).pathname;

plugin({
  name: "sbf-ouro",
  setup(build) {
    // Síncrono de propósito: um `setup` assíncrono não termina antes de o gerador carregar.
    const schema = require(`${raiz}packages/db/src/schema.ts`);
    let db: unknown = null;
    const getDb = () => {
      if (db) return db;
      const url = process.env.SBF_OURO_PG;
      if (!url) throw new Error("SBF_OURO_PG não definido: o gerador não toca banco");
      const { hostname } = new URL(url);
      if (hostname !== "127.0.0.1" && hostname !== "localhost") {
        throw new Error("SBF_OURO_PG precisa ser um Postgres local de teste");
      }
      const { drizzle } = require("drizzle-orm/bun-sql");
      const { SQL } = require("bun");
      db = drizzle(new SQL(url), { schema });
      return db;
    };
    const modulos: Record<string, () => Record<string, unknown>> = {
      "@sbf/db": () => ({ ...schema, schema, getDb }),
    };

    const sessao = () => {
      const dono = process.env.SBF_OURO_DONO;
      if (!dono) throw new Error("Não autenticado");
      return { id: dono, ownerId: dono, email: "dono@exemplo.test", role: "owner", name: "Dono" };
    };
    modulos["@/lib/auth"] = () => ({
      requireUser: async () => sessao(),
      getUser: async () => sessao(),
      getUserOrNull: async () => sessao(),
    });

    // Intercepta pelo arquivo real (`onLoad`): o `onResolve` do plugin de runtime não vê
    // os alias do tsconfig (`@/`) nem os pacotes do workspace.
    const arquivos: [RegExp, string][] = [
      [/packages\/db\/src\/index\.ts$/, "@sbf/db"],
      [/apps\/web\/src\/lib\/auth\/index\.ts$/, "@/lib/auth"],
    ];
    for (const [filter, nome] of arquivos) {
      build.onLoad({ filter }, () => ({ exports: modulos[nome]!(), loader: "object" }));
    }
  },
});
