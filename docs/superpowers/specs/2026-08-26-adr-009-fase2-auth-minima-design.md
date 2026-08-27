# ADR-009 Fase 2 — Autenticação mínima (Google OAuth) — Spec de Design

**Status:** Aprovada pelo CEO em 2026-08-26 (design confirmado diretamente na conversa
com a sessão de execução, e novamente na sessão da Verônica-Bolso).

**ADR de origem:** `docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md` (ADR-009).
Esta spec detalha só a Fase 2: "Auth mínimo: Google OAuth + cookie de sessão assinado,
substituindo `middleware.ts` e os 17 pontos de `getUser()`."

**Pré-requisito:** ADR-009 Fase 1 (Neon + Drizzle, PR #22) — o pacote `@sbf/db` já
existe, mas ainda não é consumido pelo `apps/web`. Esta fase introduz a primeira
tabela nova no Neon (`public.users`) e o mecanismo de sessão; a Fase 4 (reescrita das
17 Server Actions para Drizzle) é quem vai efetivamente trocar `supabase-js` por
`@sbf/db` nos dados — até lá, Server Actions continuam lendo/escrevendo no Supabase
Postgres via `supabase-js` com a chave de serviço, só a **identidade do usuário**
muda de fonte nesta fase.

## Contexto

Hoje `apps/web` usa `@supabase/ssr` + `@supabase/supabase-js` para autenticação:
`supabase.auth.getUser()` em ~40 call sites (17 Server Actions, 6 Route Handlers,
Server/Client Components), refresh de sessão em `middleware.ts` via
`lib/supabase/middleware.ts`, e um fluxo de login em `app/login/page.tsx` que hoje
expõe Google OAuth, magic link e login por senha via Supabase Auth.

Confirmado com o CEO (registrado na ADR-009): só **Google OAuth** é usado de fato.
Magic link e senha são superfície morta e não recebem substituto.

O app é de usuário único (ADR-007) — mas por decisão do CEO nesta spec, o modelo de
dados mantém uma tabela `public.users` (não um UUID solto), prevendo um segundo
usuário no futuro sem precisar reintroduzir a tabela depois.

## Escopo

**Dentro:**

- Tabela `public.users` no Neon (schema `packages/db/schema.sql` +
  `packages/db/src/schema.ts`).
- Fluxo de login Google OAuth mão-na-massa (sem Auth.js/NextAuth/Arctic).
- Cookie de sessão assinado (JWT HS256 via `jose`), emitido e verificado pelo
  próprio app.
- Middleware substituindo o refresh de sessão do Supabase pela verificação/renovação
  do cookie próprio.
- Helper `requireUser()` substituindo o padrão
  `supabase.auth.getUser()` em todos os ~40 call sites listados no Apêndice A.
- Substituição da Supabase Edge Function `trigger-ingestion` (achado desta sessão de
  design: está quebrada contra o schema real — ver Seção "trigger-ingestion") por uma
  rota própria no Next.js.

**Fora (fica para fases seguintes da ADR-009):**

- Migração de dados/queries do Supabase Postgres para `@sbf/db`/Neon nas 17 Server
  Actions (Fase 4).
- Storage (`@vercel/blob`, Fase 3).
- As outras 3 Supabase Edge Functions (`merge-suppliers`, `refresh-mv-supplier-spending`,
  `retroactive-supplier-association`) — sem call site encontrado em `apps/web/src`;
  ficam para a Fase 6, reavaliadas então.
- Reintroduzir amarração de `p_user_id` nas 4 funções `SECURITY DEFINER`
  (`fn_get_secrets`, `fn_mark_secret_used`, `fn_materialize_draft_record`,
  `fn_upsert_financial_obligation`) — essas só passam a ser chamadas via Neon/Drizzle
  na Fase 4; a obrigação de segurança documentada em `packages/db/schema.sql` (nota da
  Fase 1) é responsabilidade de quem escrever essas chamadas na Fase 4, não desta fase.

## Modelo de dados: `public.users`

```sql
CREATE TABLE public.users (
    id uuid DEFAULT gen_random_uuid() PRIMARY KEY,
    email text NOT NULL UNIQUE,
    google_sub text NOT NULL UNIQUE,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);
```

- `google_sub` é o campo `sub` do `id_token` do Google (identificador estável da
  conta Google — mais confiável que e-mail como chave de correspondência, já que
  e-mails podem em teoria ser reaproveitados/alterados).
- Nenhum auto-cadastro: a linha é semeada manualmente (INSERT direto) com o e-mail e
  `google_sub` do CEO antes do primeiro login funcionar. Login com uma conta Google
  cujo e-mail/sub não bate com nenhuma linha é rejeitado (403), não cria usuário novo.
- `packages/db/schema.ts`/`relations.ts`: regenerar via `drizzle-kit introspect`
  (mesmo processo documentado no cabeçalho de `schema.ts` da Fase 1) depois de aplicar
  a migration.

## Fluxo de login

### 1. `GET /auth/google` (Route Handler, `apps/web/src/app/auth/google/route.ts`)

- Gera um `state` aleatório (16 bytes, base64url), grava num cookie temporário
  (`oauth_state`, `httpOnly`, `secure`, `sameSite=lax`, `maxAge=600`).
- Redireciona para:
  ```
  https://accounts.google.com/o/oauth2/v2/auth
    ?client_id=${GOOGLE_OAUTH_CLIENT_ID}
    &redirect_uri=${NEXT_PUBLIC_APP_URL}/auth/callback
    &response_type=code
    &scope=openid email profile
    &state=${state}
  ```

### 2. `GET /auth/callback` (Route Handler, `apps/web/src/app/auth/callback/route.ts`)

- Lê `code` e `state` da query string; compara `state` com o cookie `oauth_state`
  (rejeita com 400 se não bater ou o cookie não existir — proteção CSRF). Apaga o
  cookie `oauth_state` de qualquer forma (usado uma vez).
- Troca `code` por tokens via `POST https://oauth2.googleapis.com/token`
  (`client_id`, `client_secret`, `code`, `grant_type=authorization_code`,
  `redirect_uri` idêntico ao passo 1).
- Verifica o `id_token` retornado: busca o JWKS do Google
  (`https://www.googleapis.com/oauth2/v3/certs`) via
  `jose.createRemoteJWKSet` (cacheado pelo próprio `jose`) e `jose.jwtVerify`,
  conferindo `iss=https://accounts.google.com` (ou `accounts.google.com`) e
  `aud=${GOOGLE_OAUTH_CLIENT_ID}`. Rejeita com 401 se a verificação falhar.
- Extrai `sub` (google_sub) e `email` do payload verificado. Busca
  `SELECT id, email FROM users WHERE google_sub = $1` (via `@sbf/db`). Se não
  encontrar, responde 403 com uma mensagem clara ("conta não autorizada") — não
  cria linha nova.
- Emite o cookie de sessão (ver próxima seção) e redireciona para `/dashboard`.

### 3. Cookie de sessão

- Nome: `sbf_session`. `httpOnly`, `secure` (exceto em `NODE_ENV=development`),
  `sameSite=lax`, `path=/`.
- Payload do JWT: `{ sub: users.id, email }`, assinado com `jose.SignJWT` usando
  `HS256` e o segredo `AUTH_COOKIE_SECRET` (novo, dedicado — nunca reaproveita outro
  segredo do projeto). `exp` = agora + 30 dias.
- Geração do segredo (documentar no `.env.example` e na spec, não gerar
  automaticamente): `openssl rand -base64 32`.

## Middleware

`apps/web/src/middleware.ts` passa a chamar `verifySession(request)`
(`apps/web/src/lib/auth/middleware.ts`), substituindo `updateSession`:

- Lê o cookie `sbf_session`. Ausente ou inválido (assinatura ou `exp` expirados) em
  rota protegida → redireciona para `/login`, preservando o guard de rota existente
  (`/`, `/login*`, `/auth*` continuam públicas; tudo mais exige sessão).
- Válido com mais de 15 dias restantes → segue sem alteração.
- Válido com 15 dias ou menos restantes → reemite o cookie com novo `exp` = agora +
  30 dias, na resposta (renovação silenciosa, sem round-trip ao Google).
- **Sem leitura ao banco** — o middleware roda a cada request; a verificação é só
  criptográfica (JWT local), não consulta `public.users`.

## Helper de identidade

`apps/web/src/lib/auth/session.ts`:

```ts
export interface SessionUser {
  id: string;
  email: string;
}

export async function getSession(): Promise<SessionUser | null> { ... }

export async function requireUser(): Promise<SessionUser> {
  const user = await getSession();
  if (!user) throw new Error("Não autenticado");
  return user;
}
```

Substitui, em cada um dos call sites do Apêndice A:

```diff
- const supabase = await createClient();
- const {
-   data: { user },
- } = await supabase.auth.getUser();
- if (!user) throw new Error("Não autenticado");
+ const user = await requireUser();
```

O restante da função (chamadas a `supabase.from(...)`) não muda nesta fase — só a
origem de `user.id` muda, de "confirmado pelo Supabase" para "confirmado pelo nosso
cookie". `user.id` continua sendo o UUID usado nas queries `WHERE user_id = ...`, sem
mudança de contrato pros call sites.

**Client Components** (`ai-chat-drawer.tsx`, `suppliers/[id]/page.tsx` e outros que
hoje chamam `supabase.auth.getUser()` no browser): o cookie é `httpOnly`, então não dá
pra ler no client. Passam a receber o usuário via um `AuthContext` React, populado
pelo `apps/web/src/app/dashboard/layout.tsx` (Server Component, já chama
`requireUser()` hoje como guard de rota) e propagado aos filhos via
`<AuthProvider user={user}>`.

## Substituição do `trigger-ingestion`

**Movida para a Fase 4a-bis (ruling da Veronica-Bolso, 2026-08-27):** o desenho
abaixo não depende de nenhuma mudança de auth — `requireUser()` continua
chamando `supabase.auth.getUser()` normalmente até a Fase 2 trocar só a
_origem_ de `user.id`. Como o upload-via-chat já está quebrado em produção
hoje (achado desta seção), implementar esta correção na Fase 4a-bis conserta o
bug sem esperar a Fase 2 — a seção permanece aqui como registro do desenho
original, mas a implementação e o plano vivem na spec/plano da Fase 4a-bis.

**Achado durante o design** (não estava no levantamento de impacto original da
ADR-009): a Supabase Edge Function `trigger-ingestion` está quebrada contra o schema
real hoje — insere `file_name`/`file_path`/`source_type` em `source_documents` (as
colunas reais são `filename`, `local_filepath`/`storage_path`,
`origin_type`/`origin_key`, sendo os dois últimos `NOT NULL`), insere `current_step`
em `ingestion_jobs` (coluna inexistente), e passa `source_type: "manual"` em
`ingestion_runs` (o `CHECK` só aceita `gmail`/`local_file`/`manual_upload`). O
comentário em `supabase/config.toml:401-402` ("Quebrada contra o schema real...
Prevista para deleção") está correto, não estale. Upload-via-chat
(`ai-chat-drawer.tsx:138`, `upload-documents.tsx:61`) está quebrado em produção hoje.

**Correção (achado na revisão da Verônica-Bolso, verificado contra
`supabase/migrations/20260323120500_create_storage_bucket_ingestion.sql`):** o
parágrafo original desta seção dizia que o upload do arquivo pro bucket
`ingestion-originals` "não depende de RLS — já é assim hoje". Isso é **falso**. O
bucket é privado (`public=false`) e tem a policy `"Users manage own ingestion
files"` exigindo `auth.uid()::text = (storage.foldername(name))[1]` — o upload de
hoje só funciona porque o `supabase-js` do browser carrega o JWT da sessão Supabase
Auth real. Assim que a Fase 2 tirar essa sessão do browser, `auth.uid()` vira NULL e
o upload em si passa a ser rejeitado pela policy — quebrando de novo, agora um passo
antes do que o achado original cobria.

**Escopo do que muda (revisado):** tanto o _upload do arquivo_ quanto o _registro do
job_ mudam nesta fase — não só o registro, como a versão anterior desta spec dizia.

**Solução — signed upload URL:** nova Route Handler
`apps/web/src/app/api/ingestion/upload-url/route.ts` (POST):

1. Chama `requireUser()` — 401 se não autenticado.
2. Recebe `{ filename: string }` no corpo.
3. Monta o path `${user.id}/${crypto.randomUUID()}/${filename}` (mesma convenção
   de path que a policy já exige e que `workers/local-scanner` já usa).
4. Chama, com o client Supabase de **service role** (nunca exposto ao browser):
   `supabase.storage.from("ingestion-originals").createSignedUploadUrl(path)`.
5. Retorna `{ signedUrl, token, path }` ao browser.

Os 2 componentes de browser (`ai-chat-drawer.tsx`, `upload-documents.tsx`) passam a:
chamar essa rota primeiro para cada arquivo → usar
`supabase.storage.from("ingestion-originals").uploadToSignedUrl(path, token, file)`
(chave publicável, sem RLS envolvida — o token assinado já autoriza aquele path
específico) → só então chamar a rota de registro do job (abaixo) passando o `path`
retornado.

Essa é a menor mudança que resolve o problema nesta fase — mantém upload direto
browser→Storage (sem o arquivo trafegar pela rota Next.js). A Fase 3
(`@vercel/blob`) decide o modelo definitivo de upload; isso aqui é só o suficiente
pra não quebrar em produção entre as duas fases.

**Solução — registro do job:** nova Route Handler
`apps/web/src/app/api/ingestion/trigger/route.ts` (POST), chamada pelos mesmos 2
componentes no lugar de `supabase.functions.invoke("trigger-ingestion", ...)`, depois
que o upload via signed URL terminar. A rota:

1. Chama `requireUser()` — 401 se não autenticado.
2. Recebe `{ source_type: "manual_upload", uploaded_paths: string[] }` no corpo
   (os `path`s retornados pela rota de signed URL — nunca um `user_id`; se o corpo
   incluir um campo `user_id`, a rota ignora e usa só o da sessão).
3. Cria `ingestion_runs` (`user_id` da sessão, `source_type: "manual_upload"`,
   `status: "running"`).
4. Para cada `path`: cria `source_documents` com as colunas reais —
   `user_id`, `origin_type: "manual_upload"`, `origin_key` (o próprio `path`, que já
   é único por upload), `filename` (derivado do `path`), `storage_path: path`,
   `status: "new"` — seguindo exatamente o padrão de
   `workers/local-scanner/src/scanner.ts:180-196`.
5. Para cada `source_document` criado: cria `ingestion_jobs`
   (`run_id`, `user_id`, `source_document_id`, `status: "discovered"` — sem
   `current_step`, que não existe).
6. A chave de serviço só é usada dentro da rota de signed URL (passo anterior),
   nunca chega ao browser; esta rota de registro nem precisa dela, já que só grava
   metadados, não acessa Storage.

A Edge Function `trigger-ingestion` é removida (`supabase/functions/trigger-ingestion/`
deletada, entrada correspondente tirada de `supabase/config.toml`) só depois de a
rota nova estar no ar e verificada — não antes, para não haver uma janela sem
nenhum dos dois funcionando.

## Variáveis de ambiente novas

```
# .env.example — seção Auth
GOOGLE_OAUTH_CLIENT_ID=          # mesmo Client ID já usado pelo Supabase (config.toml:[auth.external.google])
GOOGLE_OAUTH_CLIENT_SECRET=      # idem — mesma credencial, precisa virar variável do Next também
AUTH_COOKIE_SECRET=              # novo — gerar com: openssl rand -base64 32
```

**Pré-requisito externo (🤚 CEO — não delegável):** o Client ID do Google OAuth já
existe (usado hoje pelo Supabase Auth), mas seu "Authorized redirect URIs" no Google
Cloud Console precisa ganhar `${NEXT_PUBLIC_APP_URL}/auth/callback` (produção e
`http://localhost:3000/auth/callback` para dev) — isso é uma ação no console do
Google, fora do alcance de qualquer agente. Sem isso, o passo 2 do fluxo de login
falha com `redirect_uri_mismatch`. Documentar isso claramente no plano de
implementação como bloqueio de verificação end-to-end (o código pode ser
implementado e testado por partes sem isso, mas o fluxo completo de login real
depende dele).

## Segurança

- `state` de CSRF: obrigatório, cookie de uso único, expira em 10 min.
- `id_token` sempre verificado contra o JWKS do Google (nunca decodificado sem
  checar assinatura) — confere `iss` e `aud`.
- Login rejeitado (403) se e-mail/`google_sub` não corresponder a uma linha
  existente em `public.users` — sem auto-cadastro.
- Cookie de sessão: `httpOnly` + `secure` + `sameSite=lax`, segredo dedicado
  (`AUTH_COOKIE_SECRET`), nunca reaproveita outro segredo do projeto.
- Rota `/api/ingestion/trigger`: `user_id` só vem da sessão do servidor
  (`requireUser()`), nunca aceito como campo do corpo da requisição.

## Testes

- `apps/web/src/lib/auth/session.test.ts`: `getSession()`/`requireUser()` com
  cookie válido, expirado, ausente, assinado com segredo errado.
- `apps/web/src/lib/auth/middleware.test.ts`: renovação de cookie perto de expirar,
  redirecionamento em rota protegida sem sessão, passagem livre em `/login`/`/auth/*`.
- `apps/web/src/app/api/ingestion/upload-url/route.test.ts`: rejeição sem sessão
  (401), happy path emitindo `signedUrl`/`token`/`path` (mockando o client de service
  role do Storage), path sempre prefixado por `${user.id}/` vindo da sessão — nunca
  de um campo do corpo.
- `apps/web/src/app/api/ingestion/trigger/route.test.ts`: rejeição sem sessão (401),
  happy path criando `ingestion_runs` + `source_documents` + `ingestion_jobs` com as
  colunas reais, verificação de que `user_id` do corpo da requisição (se enviado) é
  ignorado.
- Não escrever teste de integração real contra o Google (sem mock de rede
  disponível para OAuth de terceiro) — mockar `fetch` para o token exchange e o JWKS
  nos testes de `/auth/callback`.

## Não-objetivos

- Múltiplos provedores de OAuth (só Google).
- Magic link, login por senha, recuperação de senha.
- Rate limiting no endpoint de login (fora de escopo; superfície pequena, usuário
  único).
- RLS ou qualquer policy de banco (ADR-009 já descartou RLS deliberadamente).
- Migrar as queries de dados das Server Actions para Drizzle (Fase 4).

## Apêndice A — Call sites de `getUser()` a substituir

Ver levantamento completo no relatório de exploração desta sessão de design
(reproduzido aqui por arquivo, sem números de linha exatos — conferir no momento da
implementação, pois podem ter mudado):

**Server Actions** (`apps/web/src/app/actions/`): `institutions.ts`, `cards.ts`,
`document-metadata.ts`, `materialization.ts`, `suppliers.ts`, `financial-periods.ts`,
`pluggy-connections.ts`, `statement-cycles.ts`, `recurring.ts`, `secrets.ts`,
`document-transactions.ts`, `ingestion.ts` (6 call sites), `transactions.ts`,
`liabilities.ts`, `patterns.ts`, `document-splits.ts`, `financial-products.ts`.

**Route Handlers**: `app/api/chat/route.ts`, `app/api/ai-suggest/route.ts`,
`app/api/reconciliation/[draftId]/route.ts` (2), `app/api/reconciliation/[draftId]/progress/route.ts`.

**Server Components**: `app/dashboard/layout.tsx` (guard de rota),
`app/dashboard/page.tsx`, `.../new/page.tsx` (products, transactions, institutions,
suppliers, liabilities, recurring), `app/dashboard/settings/page.tsx` (3).

**Client Components**: `components/ai-chat-drawer.tsx`,
`app/dashboard/import/page.tsx`, `app/dashboard/suppliers/[id]/page.tsx` (2).

**`lib/ai/tools.ts`**: 19 call sites (ferramentas do agente de IA, mesmo padrão).

**Helpers removidos ao final da fase** (depois de todos os call sites migrados):
`apps/web/src/lib/supabase/server.ts`, `lib/supabase/client.ts`,
`lib/supabase/middleware.ts` — só a parte de autenticação; se algum desses arquivos
tiver outro uso não relacionado a `getUser()`, isolar antes de remover.
