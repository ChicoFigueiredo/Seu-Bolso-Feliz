/**
 * Porteiro das rotas privadas. Roda no runtime Edge, então só verifica a
 * assinatura do cookie — nada de banco, nada de `node:crypto`.
 *
 * A checagem aqui é a primeira linha, não a única: cada Server Action e cada
 * página chama `requireUser()` por conta própria. Middleware protege a
 * navegação; o filtro `user_id` em cada query protege os dados.
 */
import { NextResponse, type NextRequest } from "next/server";
import {
  COOKIE_SESSAO,
  DURACAO_SESSAO_SEGUNDOS,
  assinaSessao,
  opcoesCookie,
  verificaSessao,
} from "@/lib/auth/session";

const PUBLICAS = ["/login", "/auth/google", "/auth/callback", "/auth/signout"];

// Renova o cookie quando falta menos de uma semana, para que quem usa o app
// todo dia nunca seja deslogado do nada.
const LIMIAR_RENOVACAO_SEGUNDOS = 60 * 60 * 24 * 7;

export async function middleware(request: NextRequest) {
  const { pathname } = request.nextUrl;

  if (PUBLICAS.some((p) => pathname === p || pathname.startsWith(`${p}/`))) {
    return NextResponse.next();
  }

  const sessao = await verificaSessao(request.cookies.get(COOKIE_SESSAO)?.value);

  if (!sessao) {
    const url = new URL("/login", request.url);
    if (pathname !== "/") url.searchParams.set("next", pathname + request.nextUrl.search);
    return NextResponse.redirect(url);
  }

  if (pathname === "/") {
    return NextResponse.redirect(new URL("/dashboard/hoje", request.url));
  }

  const resposta = NextResponse.next();

  const restante = await tempoRestante(request.cookies.get(COOKIE_SESSAO)?.value);
  if (restante !== null && restante < LIMIAR_RENOVACAO_SEGUNDOS) {
    resposta.cookies.set(COOKIE_SESSAO, await assinaSessao(sessao), opcoesCookie());
  }

  return resposta;
}

/** Segundos até expirar, lendo o `exp` do payload sem reverificar. */
async function tempoRestante(token: string | undefined): Promise<number | null> {
  if (!token) return null;
  try {
    const parte = token.split(".")[1];
    if (!parte) return null;
    const payload = JSON.parse(atob(parte.replace(/-/g, "+").replace(/_/g, "/"))) as {
      exp?: number;
    };
    if (typeof payload.exp !== "number") return null;
    const restante = payload.exp - Math.floor(Date.now() / 1000);
    return Math.min(restante, DURACAO_SESSAO_SEGUNDOS);
  } catch {
    return null;
  }
}

export const config = {
  matcher: [
    // Tudo, exceto estáticos do Next, imagens e os arquivos do PWA — o service
    // worker e o manifest precisam ser alcançáveis sem sessão, senão o app não
    // instala na tela inicial.
    "/((?!_next/static|_next/image|favicon.ico|manifest.webmanifest|sw.js|icons/|.*\\.(?:svg|png|jpg|jpeg|gif|webp|ico)$).*)",
  ],
};
