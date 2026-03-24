/**
 * get-token.ts — Obtém refresh token do Gmail via OAuth2
 *
 * Uso: bun run workers/gmail-scanner/get-token.ts
 *   ou: bun run get:gmail-token (via script no package.json raiz)
 *
 * Fluxo:
 *  1. Lê GOOGLE_MAIL_CLIENT_ID e GOOGLE_MAIL_CLIENT_SECRET do .env
 *  2. Sobe servidor HTTP local na porta 8976 para capturar o callback
 *  3. Abre o navegador na URL de consentimento do Google
 *  4. Usuário autoriza → Google redireciona com code
 *  5. Troca o code por access_token + refresh_token
 *  6. Exibe o refresh_token para adicionar ao .env
 */

const SCOPES = ["https://www.googleapis.com/auth/gmail.readonly"];
const REDIRECT_PORT = 8976;
const REDIRECT_URI = `http://127.0.0.1:${REDIRECT_PORT}/oauth2callback`;

const GOOGLE_AUTH_URL = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL = "https://oauth2.googleapis.com/token";

function getEnvOrThrow(name: string): string {
  const value = process.env[name];
  if (!value) {
    console.error(`\n❌ Variável de ambiente "${name}" não encontrada.`);
    console.error(`   Verifique o arquivo .env na raiz do projeto.\n`);
    process.exit(1);
  }
  return value;
}

const clientId = getEnvOrThrow("GOOGLE_MAIL_CLIENT_ID");
const clientSecret = getEnvOrThrow("GOOGLE_MAIL_CLIENT_SECRET");

// Monta URL de autorização
const authParams = new URLSearchParams({
  client_id: clientId,
  redirect_uri: REDIRECT_URI,
  response_type: "code",
  scope: SCOPES.join(" "),
  access_type: "offline",
  prompt: "consent",
});

const authUrl = `${GOOGLE_AUTH_URL}?${authParams.toString()}`;

console.log("\n🔐 Obtendo refresh token do Gmail OAuth2\n");
console.log("📋 Redirect URI:", REDIRECT_URI);
console.log("\n⚠️  Certifique-se de que esta URI está cadastrada no Google Cloud Console:");
console.log(
  `   Console → APIs & Services → Credentials → OAuth 2.0 Client IDs → Authorized redirect URIs\n`,
);

// Abre navegador
const openCmd =
  process.platform === "darwin" ? "open" : process.platform === "win32" ? "start" : "xdg-open";

console.log("🌐 Abrindo navegador para autorização...\n");
Bun.spawn([openCmd, authUrl], { stdout: "ignore", stderr: "ignore" });
console.log("   Se o navegador não abrir automaticamente, acesse esta URL:\n");
console.log(`   ${authUrl}\n`);

// Servidor HTTP temporário para capturar o callback
const server = Bun.serve({
  port: REDIRECT_PORT,

  async fetch(req) {
    const url = new URL(req.url);

    if (url.pathname !== "/oauth2callback") {
      return new Response("Not found", { status: 404 });
    }

    const code = url.searchParams.get("code");
    const error = url.searchParams.get("error");

    if (error) {
      console.error(`\n❌ Erro na autorização: ${error}\n`);
      server.stop();
      process.exit(1);
    }

    if (!code) {
      return new Response("Código de autorização ausente.", { status: 400 });
    }

    console.log("✅ Código de autorização recebido. Trocando por tokens...\n");

    // Troca o code por tokens
    const tokenResponse = await fetch(GOOGLE_TOKEN_URL, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        code,
        client_id: clientId,
        client_secret: clientSecret,
        redirect_uri: REDIRECT_URI,
        grant_type: "authorization_code",
      }),
    });

    if (!tokenResponse.ok) {
      const errorBody = await tokenResponse.text();
      console.error(`\n❌ Erro ao trocar código por token (HTTP ${tokenResponse.status}):\n`);
      console.error(errorBody);
      server.stop();
      process.exit(1);
    }

    const tokens = (await tokenResponse.json()) as {
      access_token?: string;
      refresh_token?: string;
      expires_in?: number;
      token_type?: string;
      scope?: string;
    };

    if (!tokens.refresh_token) {
      console.error("\n❌ Nenhum refresh_token retornado.");
      console.error(
        '   Isso pode acontecer se o app já foi autorizado antes sem "prompt=consent".',
      );
      console.error("   Tente revogar acesso em https://myaccount.google.com/permissions\n");
      server.stop();
      process.exit(1);
    }

    console.log("🎉 Tokens obtidos com sucesso!\n");
    console.log("━".repeat(60));
    console.log("\n📌 Adicione esta linha ao seu arquivo .env na raiz do projeto:\n");
    console.log(`   GMAIL_REFRESH_TOKEN=${tokens.refresh_token}`);
    console.log("\n" + "━".repeat(60));
    console.log("\n📊 Detalhes:");
    console.log(`   Token type:    ${tokens.token_type}`);
    console.log(`   Expires in:    ${tokens.expires_in}s`);
    console.log(`   Scopes:        ${tokens.scope}`);
    console.log(`   Access token:  ${tokens.access_token?.substring(0, 20)}...`);
    console.log(`   Refresh token: ${tokens.refresh_token.substring(0, 20)}...\n`);

    // Encerra o servidor após breve delay
    setTimeout(() => {
      server.stop();
      process.exit(0);
    }, 500);

    return new Response(
      `<html>
        <head><meta charset="utf-8"><title>Seu Bolso Feliz — Token OK</title></head>
        <body style="font-family:system-ui;text-align:center;padding:40px">
          <h1>✅ Autorização concluída!</h1>
          <p>O refresh token foi exibido no terminal.</p>
          <p>Você pode fechar esta aba.</p>
        </body>
      </html>`,
      { headers: { "Content-Type": "text/html; charset=utf-8" } },
    );
  },
});

console.log(`⏳ Aguardando autorização em http://127.0.0.1:${REDIRECT_PORT}...\n`);
