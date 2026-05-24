# Rotação de Chaves Supabase após Vazamento

> **Quando usar este guia:** Sempre que uma chave (`ANON_KEY`, `SERVICE_ROLE_KEY` ou `DB_PASSWORD`)
> for exposta — em commits, logs, Slack, e-mail ou qualquer lugar público.

---

## Por que rotacionar?

Chaves expostas no git ficam **permanentemente visíveis no histórico**, mesmo após remoção do arquivo.
Qualquer pessoa com acesso ao repositório (ou a um fork/clone anterior) pode usá-las.
A rotação invalida a chave antiga e emite uma nova — a antiga para de funcionar imediatamente.

---

## 1. Rotacionar `ANON_KEY` e `SERVICE_ROLE_KEY` (JWT secrets)

Essas chaves são geradas a partir do **JWT secret** do projeto.
Rotacionar o JWT secret regenera ambas.

### Passo a passo no Supabase Dashboard

1. Abra [supabase.com/dashboard](https://supabase.com/dashboard) e entre na sua conta
2. Selecione o projeto (ex: `seubolsofeliz` ou `staging`)
3. Vá em **Project Settings → API**
4. Role até a seção **JWT Settings**
5. Clique em **Generate a new JWT Secret**
6. Confirme clicando em **Generate new secret**

> ⚠️ **Isso derruba todas as sessões ativas.** Usuários logados serão desconectados.
> Execute fora do horário de uso se houver usuários reais.

7. Copie os novos valores de:
   - `anon` (pública) → `NEXT_PUBLIC_SUPABASE_ANON_KEY`
   - `service_role` (secreta) → `SUPABASE_SERVICE_ROLE_KEY`

8. Atualize o `.env` local e os secrets do ambiente de deploy (Vercel, Railway, etc.)

---

## 2. Rotacionar `SUPABASE_DB_PASSWORD`

A senha do banco é independente do JWT.

1. No Dashboard, vá em **Project Settings → Database**
2. Role até **Database Password**
3. Clique em **Reset database password**
4. Copie a nova senha gerada
5. Atualize `SUPABASE_DB_PASSWORD` em todos os ambientes

> Se você usa `supabase db push` ou `supabase db pull` localmente, também atualize
> o arquivo `~/.config/supabase/access-token` ou o `.env` com a nova senha.

---

## 3. Rotacionar `SUPABASE_ACCESS_TOKEN` (CLI token)

Token pessoal da CLI Supabase — diferente das chaves do projeto.

1. Acesse [supabase.com/dashboard/account/tokens](https://supabase.com/dashboard/account/tokens)
2. Revogue o token antigo (clique no lixeira ao lado dele)
3. Crie um novo token com **Generate new token**
4. Atualize `SUPABASE_ACCESS_TOKEN` onde for usado (CI/CD, scripts locais)

---

## 4. Atualizar em todos os lugares

Após gerar as novas chaves, atualize **obrigatoriamente**:

| Onde                                    | O que atualizar                               |
| --------------------------------------- | --------------------------------------------- |
| `.env` local (produção)                 | `ANON_KEY`, `SERVICE_ROLE_KEY`, `DB_PASSWORD` |
| `.env` local (staging)                  | Mesmas chaves do projeto de staging           |
| Vercel / Railway / plataforma de deploy | Environment variables do projeto              |
| GitHub Actions / CI secrets             | Se usar CI para deploy                        |
| Outros desenvolvedores do time          | Avisar para puxar novo `.env`                 |

---

## 5. Verificar que a chave antiga não funciona mais

```bash
# Tente fazer uma requisição com a chave antiga — deve retornar 401
curl -s https://seu-projeto.supabase.co/rest/v1/ \
  -H "apikey: CHAVE_ANTIGA_AQUI" | jq .
```

Esperado: `{"message":"Invalid API key","hint":"..."}`

---

## 6. Como evitar que aconteça de novo

O `.gitignore` do projeto agora bloqueia `.env.*` (exceto `*.example`).
Mas para garantir:

```bash
# Verificar o que está sendo rastreado antes de commitar
git ls-files | grep '\.env'
# Não deve listar nada além de .env.example e .env.*.example
```

Se aparecer algum `.env.production` ou `.env.staging` real:

```bash
git rm --cached .env.production .env.staging
git commit -m "chore(config): remove env files from git tracking"
```

---

## Referências

- [Supabase Docs: JWT Secret](https://supabase.com/docs/guides/auth/jwts)
- [Supabase Docs: Database Password](https://supabase.com/docs/guides/database/postgres/database-advisors#password-strength)
- [Supabase Docs: Access Tokens](https://supabase.com/docs/reference/cli/introduction)
