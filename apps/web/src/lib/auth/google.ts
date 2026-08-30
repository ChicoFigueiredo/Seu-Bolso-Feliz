/**
 * Google OAuth 2.0 / OpenID Connect na mão.
 *
 * Sem Auth.js, sem NextAuth, sem Arctic. São três requisições HTTP e uma
 * verificação de assinatura; a dependência custaria mais do que o código que
 * ela substitui, e prenderia o fluxo de login a decisões de outra pessoa.
 *
 * O que aqui é levado a sério, porque é onde este tipo de implementação
 * costuma errar:
 *
 *   - o `state` é assinado e casado na volta (proteção contra CSRF no callback);
 *   - o `nonce` vai no pedido e é conferido dentro do `id_token`;
 *   - o `id_token` é verificado contra o JWKS do Google, não apenas decodificado;
 *   - `email_verified` é exigido;
 *   - só e-mails de uma lista curta entram — este sistema é de uma pessoa, e
 *     "qualquer conta Google" não é a mesma coisa que "a minha conta Google".
 */
import { createRemoteJWKSet, jwtVerify, SignJWT, jwtVerify as verificaEstado } from "jose";

const AUTORIZACAO = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN = "https://oauth2.googleapis.com/token";
const EMISSORES = ["https://accounts.google.com", "accounts.google.com"];

const jwks = createRemoteJWKSet(new URL("https://www.googleapis.com/oauth2/v3/certs"));

function config() {
  const clientId = process.env.GOOGLE_OAUTH_CLIENT_ID;
  const clientSecret = process.env.GOOGLE_OAUTH_CLIENT_SECRET;
  if (!clientId || !clientSecret) {
    throw new Error("GOOGLE_OAUTH_CLIENT_ID e GOOGLE_OAUTH_CLIENT_SECRET são obrigatórios");
  }
  return { clientId, clientSecret };
}

function segredoEstado(): Uint8Array {
  const bruto = process.env.AUTH_SECRET;
  if (!bruto) throw new Error("AUTH_SECRET é obrigatório");
  return new TextEncoder().encode(bruto);
}

/** E-mails autorizados a entrar. Sem lista configurada, só o dono. */
export function emailsPermitidos(): string[] {
  const bruto = process.env.AUTH_ALLOWED_EMAILS ?? "fran.fig@gmail.com";
  return bruto
    .split(",")
    .map((e) => e.trim().toLowerCase())
    .filter(Boolean);
}

export interface EstadoOAuth {
  nonce: string;
  proximo: string;
}

/**
 * O `state` é ele mesmo um JWT curto assinado por nós. Assim não precisamos de
 * um cookie extra nem de armazenamento de servidor para amarrar a ida à volta:
 * o que volta ou foi assinado por nós há menos de dez minutos, ou é descartado.
 */
export async function assinaEstado(estado: EstadoOAuth): Promise<string> {
  return new SignJWT({ ...estado })
    .setProtectedHeader({ alg: "HS256" })
    .setIssuedAt()
    .setExpirationTime("10m")
    .sign(segredoEstado());
}

export async function leEstado(state: string): Promise<EstadoOAuth | null> {
  try {
    const { payload } = await verificaEstado(state, segredoEstado(), { algorithms: ["HS256"] });
    if (typeof payload.nonce !== "string" || typeof payload.proximo !== "string") return null;
    return { nonce: payload.nonce, proximo: payload.proximo };
  } catch {
    return null;
  }
}

export function urlAutorizacao(redirectUri: string, state: string, nonce: string): string {
  const { clientId } = config();
  const p = new URLSearchParams({
    client_id: clientId,
    redirect_uri: redirectUri,
    response_type: "code",
    scope: "openid email profile",
    state,
    nonce,
    // `select_account` evita o caso irritante de o Google entrar sozinho com a
    // conta errada quando há várias logadas no navegador.
    prompt: "select_account",
  });
  return `${AUTORIZACAO}?${p.toString()}`;
}

export interface IdentidadeGoogle {
  sub: string;
  email: string;
  name: string | null;
}

/**
 * Troca o `code` pelo `id_token` e valida a identidade. Lança com mensagem
 * curta e sem detalhe sensível — o que a página de login mostra ao usuário.
 */
export async function trocaCodigoPorIdentidade(
  code: string,
  redirectUri: string,
  nonceEsperado: string,
): Promise<IdentidadeGoogle> {
  const { clientId, clientSecret } = config();

  const resposta = await fetch(TOKEN, {
    method: "POST",
    headers: { "content-type": "application/x-www-form-urlencoded" },
    body: new URLSearchParams({
      code,
      client_id: clientId,
      client_secret: clientSecret,
      redirect_uri: redirectUri,
      grant_type: "authorization_code",
    }),
  });

  if (!resposta.ok) {
    throw new Error("o Google recusou a troca do código de autorização");
  }

  const corpo = (await resposta.json()) as { id_token?: string };
  if (!corpo.id_token) throw new Error("o Google não devolveu um id_token");

  const { payload } = await jwtVerify(corpo.id_token, jwks, {
    issuer: EMISSORES,
    audience: clientId,
  });

  if (payload.nonce !== nonceEsperado) {
    throw new Error("nonce não confere — o login foi interrompido ou adulterado");
  }
  if (payload.email_verified !== true) {
    throw new Error("esta conta Google está com o e-mail não verificado");
  }
  const email = typeof payload.email === "string" ? payload.email.toLowerCase() : null;
  if (!email || !payload.sub) throw new Error("o Google não devolveu e-mail");

  if (!emailsPermitidos().includes(email)) {
    throw new Error("esta conta não tem acesso ao Seu Bolso Feliz");
  }

  return {
    sub: payload.sub,
    email,
    name: typeof payload.name === "string" ? payload.name : null,
  };
}
