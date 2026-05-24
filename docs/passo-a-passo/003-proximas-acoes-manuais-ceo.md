# 003 — Passo a Passo: Próximas Ações Manuais do CEO

**Data:** 2026-03-24
**Referência:** Plano `006-plano-execucao-gmail-ingestao-deploy-staging.md`
**Status:** 📋 Ativo — atualizado conforme progresso

---

## Contexto

CEO confirmou em 24/03/2026:

- ✅ Login com Google Auth funciona (localhost)
- ✅ Label "Comprovantes" criada no Gmail (1000+ e-mails)
- ✅ "Continua o que dá sem mim — Confio em vocês"

Este documento lista **somente** as ações que precisam da intervenção direta do CEO. O time vai avançar com tudo que não depender dele.

---

## Ações Pendentes do CEO

### 1. 🔴 Gerar Refresh Token do Gmail (PENDENTE)

**O que é:** O refresh token permite que o sistema acesse o Gmail do CEO sem pedir login toda vez. É essencial para o Gmail scanner funcionar automaticamente.

**Como fazer:**

1. O João vai criar um script para gerar o refresh token: `bun run workers/gmail-scanner/get-token.ts`
2. Quando o script estiver pronto, o CEO precisa:
   - Rodar o script no terminal
   - O browser vai abrir pedindo login no Gmail
   - Autorizar o app "Seu Bolso Feliz" a acessar Gmail (somente leitura de e-mails)
   - O script vai imprimir o refresh token no terminal
3. Copiar o refresh token e enviar para o João (ou colocar no `.env` local)

**Quando:** Assim que João avisar que o script está pronto (estimativa: 25-26 Mar)
**Impacto se não fizer:** Gmail scanner não funciona → ingestão automática fica bloqueada.

---

### 2. 🟡 Armazenar Refresh Token no Supabase Vault (PENDENTE)

**O que é:** O refresh token é um segredo sensível. Para ambientes staging/produção, ele deve ficar no vault do Supabase, não em variáveis de ambiente expostas.

**Como fazer:**

1. Acessar o Supabase Dashboard (staging): https://supabase.com/dashboard/project/dcljzgjgnkmxdvhybvpt
2. Ir em **Settings > Vault** (ou **SQL Editor**)
3. Executar:
   ```sql
   SELECT vault.create_secret('GMAIL_REFRESH_TOKEN', '<valor-do-token>', 'Refresh token Gmail CEO');
   ```
4. Repetir para produção quando estiver ativo

**Quando:** Após gerar o refresh token (item 1)
**Impacto se não fizer:** Gmail scanner funciona localmente mas não em staging/produção.

---

### 3. 🟡 Revisar Rascunhos de Transações (FUTURO)

**O que é:** Quando o Gmail scanner processar os 1000+ e-mails, vai gerar rascunhos de transações que precisam da aprovação do CEO.

**Como fazer:**

1. Acessar o dashboard: `/dashboard/ingestion`
2. Ver a lista de rascunhos (organizada por data/fornecedor)
3. Para cada rascunho:
   - **Aprovar** se os dados estiverem corretos (fornecedor, valor, data, categoria)
   - **Editar** se precisar ajustar algo
   - **Rejeitar** se não for relevante
4. Pode aprovar em lote: "Aprovar todos do Nubank", "Aprovar todos de energia"

**Quando:** Após UI de ingestão estar pronta (estimativa: primeira semana de Abril)
**Impacto se não fizer:** Dados não entram no sistema → dashboard fica vazio.

**Dica:** Comece pelos rascunhos de fornecedores que você reconhece (Nubank, CEMIG, etc.). São os mais fáceis de validar.

---

### 4. 🟡 Testar Login em Staging (FUTURO)

**O que é:** Quando o deploy-web-real estiver funcionando, o CEO precisa testar que o login funciona em staging (não só localhost).

**Como fazer:**

1. Fernando vai compartilhar a URL de staging (algo como `staging.seubolsofeliz.com.br` ou preview URL do Vercel)
2. Acessar a URL
3. Clicar em "Entrar com Google"
4. Verificar que chega ao dashboard

**Quando:** Após Fernando completar deploy-web-real (estimativa: 26-27 Mar)
**Impacto se não fizer:** Não temos validação humana de staging com credenciais reais.

---

### 5. 🟢 Informar Senhas de PDFs Protegidos (QUANDO RELEVANTE)

**O que é:** Alguns comprovantes em PDF podem estar protegidos por senha (ex: extratos bancários, boletos específicos). O CEO precisa informar essas senhas para que o sistema possa lê-los.

**Como fazer:**

1. Quando o pipeline encontrar um PDF protegido, ele vai aparecer na UI de ingestão com status "senha necessária"
2. O CEO informa a senha no campo indicado
3. A senha é armazenada criptografada (`pgp_sym_encrypt`) na tabela `user_secrets`
4. O pipeline reprocessa o PDF com a senha

**Quando:** Conforme PDFs protegidos aparecerem durante o processamento
**Impacto se não fizer:** PDFs protegidos ficam como "pendentes" mas o resto funciona normalmente.

---

### 6. 🟢 Validar Categorias e Tags (QUANDO RELEVANTE)

**O que é:** O sistema tem categorias e tags pré-definidas (seed.sql). O CEO pode querer adicionar novas ou ajustar as existentes.

**Como fazer:**

1. Acessar `/dashboard/settings` (quando estiver em staging)
2. Ver categorias existentes (alimentação, transporte, moradia, etc.)
3. Ver tags existentes (essencial, trabalho, pessoal, etc.)
4. Adicionar/editar conforme necessidade

**Quando:** Quando começar a revisar rascunhos (item 3)

---

## Resumo Visual

| #   | Ação                            | Urgência | Depende de        | Status      |
| --- | ------------------------------- | -------- | ----------------- | ----------- |
| 1   | Gerar refresh token Gmail       | 🔴 Alta  | João criar script | ⬜ Pendente |
| 2   | Armazenar token no Vault        | 🟡 Média | Item 1            | ⬜ Pendente |
| 3   | Revisar rascunhos de transações | 🟡 Média | UI de ingestão    | ⬜ Futuro   |
| 4   | Testar login staging            | 🟡 Média | Deploy-web-real   | ⬜ Futuro   |
| 5   | Informar senhas de PDFs         | 🟢 Baixa | Pipeline rodando  | ⬜ Futuro   |
| 6   | Validar categorias e tags       | 🟢 Baixa | Staging pronto    | ⬜ Futuro   |

---

## O Que NÃO Precisa do CEO

O time vai avançar **sem depender do CEO** nos seguintes itens:

- ✅ Implementação do Gmail scanner (João)
- ✅ Deploy-web-real no GitLab CI (Fernando)
- ✅ UI de ingestão (Roberto + Sofia)
- ✅ Resiliência do pipeline (Maria)
- ✅ Supplier audit UI (Roberto)
- ✅ Auditoria a11y (Renata)
- ✅ Tokens visuais (Isabella + Thiago)
- ✅ Validação staging (Fernando + QA)
- ✅ Parser HTML embedded (Pedro)
- ✅ Monitoramento performance (André)

**O único bloqueador real do CEO é o item 1** (gerar refresh token). Tudo mais o time faz sozinho.

---

## Como o CEO Recebe Atualizações

1. **Checklist 003** (`docs/checklists/003-ciclo-ingestao-automacao-mcp-agentes.md`) — atualizado a cada entrega
2. **Novo refino** sempre que houver decisão importante
3. **Este documento** — atualizado quando status de itens mudar
