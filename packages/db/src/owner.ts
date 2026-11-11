/**
 * Quem é o dono dos dados, para processos que rodam sem sessão.
 *
 * A web sabe quem é o usuário pelo cookie de sessão. Os workers e o MCP server
 * não têm sessão nenhuma — rodam por cron, por systemd ou por stdio — e
 * precisavam de outra forma de responder "gravar isto em nome de quem?".
 *
 * A resposta era `LOCAL_USER_ID`: um UUID copiado à mão para o `.env` de cada
 * máquina. O problema desse desenho não é teórico. Quando o banco foi
 * recriado no Neon, o dono ganhou um `id` novo e o `.env` continuou com o
 * antigo — e nada quebrou. Os workers gravariam centenas de linhas em nome de
 * um usuário que não existe, e a aplicação, que filtra por `owner_id`, não
 * mostraria nenhuma delas. Um backfill "bem-sucedido" e uma tela vazia, sem
 * erro em lugar nenhum.
 *
 * Então o banco passa a ser a fonte da verdade: existe exatamente uma linha
 * com `role = 'owner'` e é dela que sai o id. `LOCAL_USER_ID` continua
 * aceito como override — útil em teste, onde o usuário é criado na hora — mas
 * agora é **validado**: se apontar para alguém que não existe, o processo
 * falha na largada em vez de escrever linhas órfãs em silêncio.
 */
import { sql } from "drizzle-orm";
import { getDb } from "./client";

let cache: string | null = null;

/** Esvazia o cache. Existe para os testes, que trocam de usuário entre casos. */
export function resetOwnerCache(): void {
  cache = null;
}

export async function resolveOwnerId(): Promise<string> {
  if (cache) return cache;

  const db = getDb();
  const override = process.env.LOCAL_USER_ID?.trim();

  if (override) {
    const { rows } = await db.execute<{ id: string }>(
      sql`SELECT id FROM public.users WHERE id = ${override}::uuid LIMIT 1`,
    );
    if (!rows[0]) {
      throw new Error(
        `LOCAL_USER_ID=${override} não corresponde a nenhum usuário em public.users. ` +
          "Ou o valor está velho (o banco foi recriado e os ids mudaram), ou aponta para " +
          "outro ambiente. Corrija o .env ou remova a variável para usar o dono do banco.",
      );
    }
    cache = rows[0].id;
    return cache;
  }

  const { rows } = await db.execute<{ id: string }>(
    sql`SELECT id FROM public.users WHERE role = 'owner' ORDER BY created_at LIMIT 1`,
  );
  if (!rows[0]) {
    throw new Error(
      "Nenhum usuário com role='owner' em public.users — rode as migrations " +
        "(packages/db/migrations/0002_*.sql semeia o dono) antes de subir o worker.",
    );
  }
  cache = rows[0].id;
  return cache;
}
