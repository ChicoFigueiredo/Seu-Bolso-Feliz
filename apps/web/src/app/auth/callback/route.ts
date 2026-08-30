/**
 * Volta do Google. Valida o `state`, troca o código pela identidade, casa com
 * a linha de `users` pelo e-mail e abre a sessão.
 *
 * A linha do dono é semeada pela migration sem `google_sub` — ele é preenchido
 * aqui, no primeiro login. Contas fora da lista de permitidos não criam
 * usuário: são recusadas em `trocaCodigoPorIdentidade`.
 */
import { NextResponse, type NextRequest } from "next/server";
import { getDb, schema } from "@sbf/db";
import { eq } from "drizzle-orm";
import { leEstado, trocaCodigoPorIdentidade } from "@/lib/auth/google";
import { redirectUriDe, origemDe } from "@/lib/auth/url";
import { abreSessao } from "@/lib/auth";

function voltaComErro(request: NextRequest, motivo: string) {
  const url = new URL("/login", origemDe(request));
  url.searchParams.set("erro", motivo);
  return NextResponse.redirect(url);
}

export async function GET(request: NextRequest) {
  const params = request.nextUrl.searchParams;

  if (params.get("error")) {
    return voltaComErro(request, "Login pelo Google cancelado.");
  }

  const code = params.get("code");
  const state = params.get("state");
  if (!code || !state) return voltaComErro(request, "Resposta incompleta do Google.");

  const estado = await leEstado(state);
  if (!estado) return voltaComErro(request, "O login expirou. Tente de novo.");

  let identidade;
  try {
    identidade = await trocaCodigoPorIdentidade(code, redirectUriDe(request), estado.nonce);
  } catch (e) {
    return voltaComErro(request, e instanceof Error ? e.message : "Falha no login pelo Google.");
  }

  const db = getDb();
  const [existente] = await db
    .select({ id: schema.users.id, google_sub: schema.users.google_sub })
    .from(schema.users)
    .where(eq(schema.users.email, identidade.email))
    .limit(1);

  if (!existente) {
    // O e-mail passou na lista de permitidos mas não tem linha. Isso é um erro
    // de configuração (migration não rodou), não um usuário novo — este sistema
    // não faz auto-cadastro.
    return voltaComErro(request, "Conta autorizada, mas sem cadastro no sistema.");
  }

  if (existente.google_sub && existente.google_sub !== identidade.sub) {
    // O e-mail bate mas a conta Google é outra: alguém recriou a conta, ou é
    // um e-mail reciclado. Recusar é mais seguro que reamarrar em silêncio.
    return voltaComErro(request, "Esta conta Google não é a que está vinculada a este e-mail.");
  }

  if (!existente.google_sub) {
    await db
      .update(schema.users)
      .set({ google_sub: identidade.sub, name: identidade.name })
      .where(eq(schema.users.id, existente.id));
  }

  await abreSessao(existente.id);
  return NextResponse.redirect(new URL(estado.proximo, origemDe(request)));
}
