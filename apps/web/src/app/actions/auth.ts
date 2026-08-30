"use server";

/**
 * Login por e-mail e senha — a chave reserva.
 *
 * Existe para um caso só: o dia em que a conta Google falhar. Por isso é
 * deliberadamente austero: sem cadastro, sem recuperação por e-mail, sem SMTP.
 * A senha nasce de `ADMIN_INITIAL_PASSWORD` no primeiro uso e depois vive no
 * banco, onde pode ser trocada sem redeploy.
 */
import { redirect } from "next/navigation";
import { getDb, schema } from "@sbf/db";
import { eq, sql } from "drizzle-orm";
import { abreSessao } from "@/lib/auth";
import { hashSenha, verificaSenha } from "@/lib/auth/password";

const MAX_TENTATIVAS = 5;
const MINUTOS_BLOQUEIO = 15;

export interface ResultadoLogin {
  erro: string;
}

/**
 * Semeia a senha do administrador a partir de `ADMIN_INITIAL_PASSWORD` se a
 * linha ainda não tem hash. Roda no caminho do login, não no boot: um app
 * serverless não tem "boot", e amarrar isto a uma migration exigiria o hash
 * pronto no SQL.
 */
async function semeiaSenhaSePreciso(
  usuarioId: string,
  hashAtual: string | null,
): Promise<string | null> {
  if (hashAtual) return hashAtual;
  const inicial = process.env.ADMIN_INITIAL_PASSWORD;
  if (!inicial || inicial.length < 12) return null;
  const hash = await hashSenha(inicial);
  await getDb()
    .update(schema.users)
    .set({ password_hash: hash })
    .where(eq(schema.users.id, usuarioId));
  return hash;
}

export async function entrarComSenha(
  _anterior: ResultadoLogin | null,
  form: FormData,
): Promise<ResultadoLogin> {
  const email = String(form.get("email") ?? "")
    .trim()
    .toLowerCase();
  const senha = String(form.get("senha") ?? "");

  if (!email || !senha) return { erro: "Informe e-mail e senha." };

  const db = getDb();
  const [usuario] = await db
    .select({
      id: schema.users.id,
      password_hash: schema.users.password_hash,
      failed_login_count: schema.users.failed_login_count,
      locked_until: schema.users.locked_until,
    })
    .from(schema.users)
    .where(eq(schema.users.email, email))
    .limit(1);

  // Mensagem única para e-mail inexistente e senha errada: distinguir os dois
  // conta a quem está tentando quais e-mails existem.
  const generico = { erro: "E-mail ou senha incorretos." };

  if (!usuario) {
    // Gasta tempo comparável ao caminho feliz, para não vazar a existência da
    // conta pela diferença de latência.
    await verificaSenha(senha, null);
    return generico;
  }

  if (usuario.locked_until && new Date(usuario.locked_until) > new Date()) {
    return { erro: `Muitas tentativas. Tente de novo em ${MINUTOS_BLOQUEIO} minutos.` };
  }

  const hash = await semeiaSenhaSePreciso(usuario.id, usuario.password_hash);
  if (!hash) return { erro: "Login por senha não está configurado neste ambiente." };

  if (!(await verificaSenha(senha, hash))) {
    const tentativas = usuario.failed_login_count + 1;
    await db
      .update(schema.users)
      .set({
        failed_login_count: tentativas,
        locked_until:
          tentativas >= MAX_TENTATIVAS
            ? sql`now() + ${`${MINUTOS_BLOQUEIO} minutes`}::interval`
            : null,
      })
      .where(eq(schema.users.id, usuario.id));
    return generico;
  }

  await abreSessao(usuario.id);
  redirect("/dashboard/hoje");
}
