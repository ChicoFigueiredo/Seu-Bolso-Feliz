/**
 * Começo do login pelo Google. Gera `state` e `nonce`, e manda o navegador
 * para o Google. Nada é gravado no servidor: o `state` assinado é o que amarra
 * esta ida à volta em `/auth/callback`.
 */
import { NextResponse, type NextRequest } from "next/server";
import { assinaEstado, urlAutorizacao } from "@/lib/auth/google";
import { redirectUriDe } from "@/lib/auth/url";

export async function GET(request: NextRequest) {
  const proximo = request.nextUrl.searchParams.get("next") ?? "/dashboard/hoje";
  // Só caminho interno: `next=https://outro.site` viraria redirect aberto.
  const destino =
    proximo.startsWith("/") && !proximo.startsWith("//") ? proximo : "/dashboard/hoje";

  const nonce = crypto.randomUUID();
  const state = await assinaEstado({ nonce, proximo: destino });

  return NextResponse.redirect(urlAutorizacao(redirectUriDe(request), state, nonce));
}
