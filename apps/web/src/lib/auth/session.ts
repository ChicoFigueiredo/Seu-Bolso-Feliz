/**
 * Sessão própria: um JWT assinado por nós, guardado num cookie httpOnly.
 *
 * Este módulo é deliberadamente livre de `node:*` e de acesso ao banco, porque
 * o `middleware.ts` roda no runtime Edge e precisa importar daqui. Tudo que
 * exige Node (hash de senha) ou Postgres (carregar o usuário) vive noutro
 * arquivo.
 *
 * O que o token carrega, e por quê:
 *
 *   sub      quem está agindo (o ator)
 *   ownerId  de quem são os dados que este ator opera
 *
 * Os dois são separados de propósito. Quando o Chico entra pelo Google, são a
 * mesma pessoa. Quando o administrador de emergência entra por senha, o ator é
 * o admin e o dono continua sendo o Chico — é assim que a chave reserva enxerga
 * os dados de verdade em vez de um app vazio. Ver CONTEXT.md e a tabela `users`.
 */
import { SignJWT, jwtVerify } from "jose";

export const COOKIE_SESSAO = "sbf_session";
export const DURACAO_SESSAO_SEGUNDOS = 60 * 60 * 24 * 30; // 30 dias

export interface SessaoUsuario {
  /** Quem está agindo. Vai para o log de auditoria. */
  id: string;
  /** De quem são os dados. **Todo filtro `user_id` usa este campo.** */
  ownerId: string;
  email: string;
  role: "owner" | "admin";
  name: string | null;
}

let segredoCache: Uint8Array | null = null;

function segredo(): Uint8Array {
  if (segredoCache) return segredoCache;
  const bruto = process.env.AUTH_SECRET;
  if (!bruto || bruto.length < 32) {
    throw new Error(
      "AUTH_SECRET é obrigatório e precisa ter pelo menos 32 caracteres. " +
        "Gere um com: openssl rand -base64 48",
    );
  }
  segredoCache = new TextEncoder().encode(bruto);
  return segredoCache;
}

export async function assinaSessao(usuario: SessaoUsuario): Promise<string> {
  const agora = Math.floor(Date.now() / 1000);
  return new SignJWT({
    ownerId: usuario.ownerId,
    email: usuario.email,
    role: usuario.role,
    name: usuario.name,
  })
    .setProtectedHeader({ alg: "HS256" })
    .setSubject(usuario.id)
    .setIssuedAt(agora)
    .setExpirationTime(agora + DURACAO_SESSAO_SEGUNDOS)
    .setIssuer("seu-bolso-feliz")
    .setAudience("seu-bolso-feliz")
    .sign(segredo());
}

/** Devolve `null` para qualquer token inválido, expirado ou ausente. */
export async function verificaSessao(token: string | undefined): Promise<SessaoUsuario | null> {
  if (!token) return null;
  try {
    const { payload } = await jwtVerify(token, segredo(), {
      issuer: "seu-bolso-feliz",
      audience: "seu-bolso-feliz",
      algorithms: ["HS256"],
    });
    const role = payload.role;
    if (!payload.sub || typeof payload.ownerId !== "string" || typeof payload.email !== "string") {
      return null;
    }
    if (role !== "owner" && role !== "admin") return null;
    return {
      id: payload.sub,
      ownerId: payload.ownerId,
      email: payload.email,
      role,
      name: typeof payload.name === "string" ? payload.name : null,
    };
  } catch {
    return null;
  }
}

export function opcoesCookie(): {
  httpOnly: true;
  secure: boolean;
  sameSite: "lax";
  path: string;
  maxAge: number;
} {
  return {
    httpOnly: true,
    // Em desenvolvimento o app roda em http://localhost; exigir `secure` ali
    // faria o cookie ser descartado em silêncio e o login "não funcionar".
    secure: process.env.NODE_ENV === "production",
    sameSite: "lax",
    path: "/",
    maxAge: DURACAO_SESSAO_SEGUNDOS,
  };
}
