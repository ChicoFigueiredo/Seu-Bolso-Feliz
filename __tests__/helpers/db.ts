/**
 * Conexão dos testes de integração/e2e com um Postgres real.
 *
 * Depois da ADR-009, `@sbf/db` fala Drizzle direto com um Postgres qualquer,
 * e **não há RLS** — toda consulta filtra `user_id`
 * explicitamente, inclusive as deste diretório. Um teste que esqueça o filtro
 * passa a enxergar as linhas dos outros arquivos da suíte, então o filtro faz
 * parte do que está sendo testado, não é cerimônia.
 *
 * ## Como rodar
 *
 * ```sh
 * docker run -d --name sbf-integration-test \
 *   -e POSTGRES_PASSWORD=sbf -e POSTGRES_USER=sbf -e POSTGRES_DB=sbf \
 *   -p 55480:5432 postgres:17-alpine
 *
 * docker exec -i sbf-integration-test psql -U sbf -d sbf < packages/db/schema.sql
 * docker exec -i sbf-integration-test psql -U sbf -d sbf < packages/db/migrations/0001_auth_fila_centros.sql
 * docker exec -i sbf-integration-test psql -U sbf -d sbf < packages/db/migrations/0002_crypto_key_e_usuarios.sql
 *
 * INTEGRATION_TEST_DATABASE_URL=postgresql://sbf:sbf@127.0.0.1:55480/sbf \
 *   bun run test:integration
 * ```
 *
 * A imagem é 17 porque `schema.sql` é um dump do Postgres 17.6; no 16 tudo
 * funciona, mas o `SET transaction_timeout` do cabeçalho do dump vira um erro
 * (inofensivo) na saída do psql, o que confunde quem estiver conferindo.
 *
 * `schema.sql` sozinho **não basta**: as colunas de lock da fila
 * (`locked_by`/`locked_at`/`requires`/`available_at`), o trigger de `NOTIFY`,
 * `public.users`, `financial_centers`, `transaction_allocations` e a chave de
 * criptografia (`private.get_crypto_key`, sem a qual `encrypt_secret` quebra)
 * só existem nas duas migrations.
 *
 * ## Por que uma variável separada de `DATABASE_URL`
 *
 * Estes arquivos apagam linhas por `user_id` e semeiam dados. Apontá-los para
 * a `DATABASE_URL` de trabalho — que num `.env` local é o Neon de verdade —
 * escreveria na base real do dono. A variável dedicada torna isso uma decisão
 * consciente, e sem ela a suíte se pula inteira (é o que o CI faz hoje: ele
 * não provisiona Postgres para esta suíte).
 */
import { sql } from "drizzle-orm";
import { getDb, type Database } from "@sbf/db";

/** URL do Postgres descartável; ausente ⇒ suíte pulada. */
export const TEST_DATABASE_URL = process.env.INTEGRATION_TEST_DATABASE_URL;

/** `true` quando não há banco de teste configurado — use em `describe.skipIf`. */
export const semBancoDeTeste = !TEST_DATABASE_URL;

// `getDb()` é um singleton preguiçoso que lê `DATABASE_URL` na PRIMEIRA
// chamada. Cada arquivo de teste roda num worker isolado do Vitest, então
// reescrever a variável aqui vale só para este arquivo e não vaza para
// nenhuma outra suíte — nem para um processo de aplicação.
if (TEST_DATABASE_URL) process.env.DATABASE_URL = TEST_DATABASE_URL;

/**
 * Instância do Drizzle apontada para o banco de teste.
 *
 * Chame dentro de `beforeAll` (e não no topo do módulo): em `skipIf` o corpo
 * do arquivo ainda é avaliado, e `getDb()` sem `DATABASE_URL` lança.
 */
export function getTestDb(): Database {
  return getDb();
}

/**
 * Apaga as linhas de um usuário nas tabelas informadas, na ordem dada.
 *
 * A ordem é responsabilidade de quem chama: filhas antes das pais, senão a FK
 * recusa. Ao contrário do `.delete()` do driver anterior — que devolvia o erro
 * num campo que ninguém lia —, aqui uma violação de FK **lança**, que é como um
 * cleanup quebrado deve se comportar (o silêncio anterior deixava documentos
 * acumulando entre execuções e falseava contagens).
 *
 * `audit_logs` não pode entrar nesta lista: a tabela é imutável por trigger
 * (`trg_audit_logs_immutable` recusa DELETE e UPDATE), o que é a propriedade
 * certa para uma trilha de auditoria. Os testes se adaptam a ela filtrando o
 * que consultam, em vez de tentarem limpá-la.
 */
export async function limparPorUsuario(
  db: Database,
  tabelas: readonly string[],
  ...userIds: string[]
): Promise<void> {
  for (const tabela of tabelas) {
    for (const userId of userIds) {
      // `sql.raw` no nome da tabela: identificador não é parâmetro em SQL. Os
      // nomes vêm de literais deste próprio diretório, nunca de entrada externa.
      await db.execute(sql`delete from public.${sql.raw(tabela)} where user_id = ${userId}::uuid`);
    }
  }
}

/**
 * Garante que os usuários de teste existem em `public.users`.
 *
 * Antes ninguém semeava essa tabela: os testes gravavam `user_id` de um
 * usuário que não existia, e passavam — nenhuma FK aponta para `public.users`,
 * então o Postgres não reclamava. Era exatamente o mesmo defeito que existia em
 * produção, onde o `LOCAL_USER_ID` do `.env` ficou apontando para o id de um
 * banco antigo e os workers escreviam no vazio.
 *
 * Com `resolveOwnerId` validando o id na largada, a lacuna virou falha visível
 * — que é o comportamento certo. Este helper fecha a lacuna no teste em vez de
 * afrouxar a validação.
 */
export async function semearUsuarios(db: Database, ...userIds: string[]): Promise<void> {
  for (const userId of userIds) {
    await db.execute(sql`
      INSERT INTO public.users (id, email, role)
      VALUES (${userId}::uuid, ${`teste-${userId}@exemplo.invalido`}, 'owner')
      ON CONFLICT (id) DO NOTHING
    `);
  }
}

/** Linhas de um `db.execute`, tipadas — os dois drivers devolvem `{ rows }`. */
export async function linhas<T = Record<string, unknown>>(
  db: Database,
  query: ReturnType<typeof sql>,
): Promise<T[]> {
  const resultado = (await db.execute(query)) as unknown as { rows: T[] };
  return resultado.rows ?? [];
}

/** Primeira linha de um `db.execute`, ou `undefined`. */
export async function linha<T = Record<string, unknown>>(
  db: Database,
  query: ReturnType<typeof sql>,
): Promise<T | undefined> {
  return (await linhas<T>(db, query))[0];
}

/**
 * Resultado de uma chamada que pode falhar, no formato `{ data, error }`.
 *
 * Existe porque metade destes testes afirma sobre a MENSAGEM de erro de uma
 * função do banco (`/pending_review/`, `/not allowed/i`) e a outra metade
 * sobre o valor devolvido. Com o driver anterior isso vinha pronto; com
 * Drizzle um erro do Postgres é exceção. Encapsular aqui evita espalhar
 * try/catch por
 * cada asserção.
 */
export interface Resultado<T> {
  data: T | null;
  error: Error | null;
}

export async function tentar<T>(fn: () => Promise<T>): Promise<Resultado<T>> {
  try {
    return { data: await fn(), error: null };
  } catch (err) {
    return { data: null, error: comCadeiaDeCausas(err) };
  }
}

/**
 * Devolve um erro cuja mensagem inclui a cadeia inteira de `cause`.
 *
 * Duas camadas embrulham erro neste repositório e as asserções precisam
 * enxergar as duas pontas:
 *
 *  - o Drizzle troca a mensagem do Postgres por `"Failed query: <sql>"` e
 *    empurra a original para `cause` — sem juntar, `/pending_review/` e
 *    `/duplicate key/` nunca casariam;
 *  - o processador de ingestão faz o contrário, embrulhando o erro do storage
 *    numa mensagem sua (`Failed to download …`) — pegar só a raiz perderia
 *    justamente a camada que diz em que passo o pipeline quebrou.
 *
 * O erro original fica em `cause`, para quem precisar de `code`/`constraint`.
 */
function comCadeiaDeCausas(err: unknown): Error {
  const mensagens: string[] = [];
  let atual: unknown = err;
  while (atual instanceof Error) {
    mensagens.push(atual.message);
    atual = atual.cause;
  }
  if (mensagens.length === 0) return new Error(String(err));

  const erro = new Error(mensagens.join("\ncausado por: "));
  erro.cause = err;
  return erro;
}
