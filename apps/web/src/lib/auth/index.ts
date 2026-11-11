/**
 * Ponto de entrada da autenticação. Substitui inteiramente a chamada a um
 * provedor de autenticação externo que existia em ~40 pontos do app.
 *
 * Uso em Server Component, Server Action ou Route Handler:
 *
 *     const user = await requireUser();
 *     // ... where(eq(schema.transactions.user_id, user.ownerId))
 *
 * ⚠️ Regra que não pode ser esquecida: **filtre sempre por `user.ownerId`,
 * nunca por `user.id`**. `id` é quem está agindo; `ownerId` é de quem são os
 * dados. Para o dono os dois coincidem, então usar o errado passa despercebido
 * até o dia em que o administrador de emergência entrar e vir um app vazio.
 */
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { getDb, schema } from "@sbf/db";
import { eq } from "drizzle-orm";
import {
  COOKIE_SESSAO,
  assinaSessao,
  opcoesCookie,
  verificaSessao,
  type SessaoUsuario,
} from "./session";

export type { SessaoUsuario };
export { COOKIE_SESSAO } from "./session";

/**
 * Lê a sessão do cookie. Não bate no banco — o token já carrega o que
 * importa, e uma consulta por render numa página que faz dez consultas seria
 * desperdício. A revogação acontece na troca do `AUTH_SECRET`.
 */
export async function getUser(): Promise<SessaoUsuario | null> {
  const jar = await cookies();
  return verificaSessao(jar.get(COOKIE_SESSAO)?.value);
}

/** Igual a `getUser`, mas manda para o login quando não há sessão. */
export async function requireUser(): Promise<SessaoUsuario> {
  const usuario = await getUser();
  if (!usuario) redirect("/login");
  return usuario;
}

/**
 * Versão para Route Handlers de API, onde `redirect()` produziria uma resposta
 * que o cliente não sabe interpretar. Devolve `null` e deixa quem chamou
 * responder 401.
 */
export async function getUserOrNull(): Promise<SessaoUsuario | null> {
  return getUser();
}

/** Monta a sessão a partir de uma linha de `users` e grava o cookie. */
export async function abreSessao(usuarioId: string): Promise<SessaoUsuario> {
  const db = getDb();
  const [linha] = await db
    .select({
      id: schema.users.id,
      email: schema.users.email,
      name: schema.users.name,
      role: schema.users.role,
      owner_id: schema.users.owner_id,
    })
    .from(schema.users)
    .where(eq(schema.users.id, usuarioId))
    .limit(1);

  if (!linha) throw new Error("usuário não encontrado ao abrir sessão");
  if (linha.role !== "owner" && linha.role !== "admin") {
    throw new Error(`papel desconhecido em users.role: ${linha.role}`);
  }

  const sessao: SessaoUsuario = {
    id: linha.id,
    // O dono opera sobre si mesmo; o admin, sobre os dados de quem ele serve.
    ownerId: linha.owner_id ?? linha.id,
    email: linha.email,
    role: linha.role,
    name: linha.name,
  };

  const jar = await cookies();
  jar.set(COOKIE_SESSAO, await assinaSessao(sessao), opcoesCookie());

  await db
    .update(schema.users)
    .set({ last_login_at: new Date().toISOString(), failed_login_count: 0, locked_until: null })
    .where(eq(schema.users.id, usuarioId));

  return sessao;
}

export async function fechaSessao(): Promise<void> {
  const jar = await cookies();
  jar.delete(COOKIE_SESSAO);
}
