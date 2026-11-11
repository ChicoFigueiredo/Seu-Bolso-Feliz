/**
 * A URL de callback precisa ser idêntica na ida e na volta, senão o Google
 * recusa a troca do código. Derivar do request evita ter que manter uma
 * variável de ambiente por ambiente (local, preview da Vercel, produção) —
 * e é justamente em preview que uma URL fixa quebra.
 */
import type { NextRequest } from "next/server";

export function origemDe(request: NextRequest): string {
  // Atrás do proxy da Vercel, `request.nextUrl.origin` pode vir como http
  // interno; os cabeçalhos encaminhados são a fonte confiável.
  const host = request.headers.get("x-forwarded-host") ?? request.headers.get("host");
  const proto = request.headers.get("x-forwarded-proto") ?? "https";
  if (host) return `${proto}://${host}`;
  return request.nextUrl.origin;
}

export function redirectUriDe(request: NextRequest): string {
  return `${origemDe(request)}/auth/callback`;
}
