---
Título da Reunião: Alinhamento Geral — Google Auth, Gmail Worker e Próximos Passos
Data e Hora: 2026-03-24 07:56
Participantes:
- Chico (CEO) — facilitador, decisor final
- Ana Silva (Arquiteta) — revisão de arquitetura e segurança
- Fernando Gomes (DevOps) — ambientes, CI/CD, variáveis
- João Pereira (Backend Sênior) — Google OAuth, workers, MCP
- Maria Oliveira (Backend Sênior) — testes, segurança de APIs
- Roberto Lima (Frontend Sênior) — login page, dashboard
- Ricardo Monteiro (Economista) — validação de fluxos financeiros
- Camila Duarte (Finanças Pessoais) — experiência do usuário financeiro
- André Santos (DBA) — banco de dados, migrations
- Thiago Martins (Front Engineer) — MCP, integração VS Code

Pauta:
- Revisão das entregas concluídas pelo CEO (1.2, 1.5, 3.5, 3.AA.7)
- Diagnóstico dos bloqueios removidos e novos itens desbloqueados
- Gap crítico: variáveis GOOGLE_OAUTH_* ausentes no .env
- Prioridade do CEO: login com Google funcionando
- Planejamento: fila de trabalho imediata e próximas fases
- Alinhamento sobre Gmail Worker (Fase 3B) e OpenAI (Fase 7)

---

## Discussão

### 1. Entregas Confirmadas pelo CEO

**Fernando (DevOps):**
O CEO confirmou 4 entregas manuais que estavam pendentes:

| Item | Descrição | Status |
|------|-----------|--------|
| 1.2 | Conectar repositório GitLab ao Vercel | ✅ |
| 1.5 | Configurar preview deployments por branch | ✅ |
| 3.5 | Definir redirect URIs no Google Cloud Console | ✅ |
| 3.AA.7 | Adicionar redirect URIs no Google Cloud Console | ✅ |

Screenshot do CEO confirma 6 redirect URIs configurados:
1. `http://localhost:3000/api/auth/callback/google`
2. `http://localhost:8080/oauth/callback`
3. `https://seubolsofeliz.com.br/api/auth/callback/google`
4. `http://127.0.0.1:54321/auth/v1/callback`
5. `https://dcljzgjgnkmxdvhybvpt.supabase.co/auth/v1/callback`
6. `https://opwelsgdhksuuewdbefk.supabase.co/auth/v1/callback`

**André (DBA):** Com 1.2 ✅, os itens 1.22-1.25 (GitLab CI jobs) ficaram desbloqueados. É possível agora substituir o placeholder de deploy-web por um job real com Vercel CLI.

### 2. Diagnóstico de Itens Desbloqueados

**Ana (Arquiteta):**
Com as 4 entregas do CEO, o mapa de desbloqueio ficou assim:

| Item Desbloqueado | Dependia de | Novo Status |
|-------------------|-------------|-------------|
| 1.22 — Job deploy-web real (Vercel CLI) | 1.2 | 🟡 Aguardando execução |
| 1.23 — Validar pipeline branch develop | 1.22 | 🔲 Sequencial |
| 1.24 — Deploy staging end-to-end | 1.23 | 🔲 Sequencial |
| 1.25 — Política promoção staging→production | 1.24 | 🔲 Sequencial |
| 3.6 — Gerar primeiro refresh token | 3.5 | ⬜ Precisa de decisão |
| 3.AA.9 — Google Auth local E2E | 3.AA.7 | 🟡 **PRIORIDADE** |
| 3.AA.10 — Google Auth staging E2E | 3.AA.7 + 3.AA.8 | 🟡 Após 3.AA.9 |

### 3. Gap Crítico: Variáveis GOOGLE_OAUTH_* Ausentes

**João (Backend Sênior):**
Durante a análise, identifiquei um **bloqueador técnico** para o Google Auth funcionar localmente:

- O `supabase/config.toml` referencia `env(GOOGLE_OAUTH_CLIENT_ID)` e `env(GOOGLE_OAUTH_CLIENT_SECRET)`
- O arquivo `.env` possui **apenas** `GOOGLE_MAIL_CLIENT_ID` e `GOOGLE_MAIL_CLIENT_SECRET`
- **Não existe** `GOOGLE_OAUTH_CLIENT_ID` nem `GOOGLE_OAUTH_CLIENT_SECRET` em nenhum arquivo `.env`
- O CEO usa o **mesmo Client ID/Secret** do Google Cloud Console para ambos (Gmail API e OAuth login)

**Solução proposta:** Adicionar ao `.env`:
```
GOOGLE_OAUTH_CLIENT_ID=XXX
GOOGLE_OAUTH_CLIENT_SECRET=XXX
```

Estas variáveis são lidas pelo Supabase local via `config.toml`. Sem elas, o `supabase start` não consegue configurar o provider Google corretamente.

**Maria (Backend Sênior):**
Precisamos também verificar se o `apps/web/.env.local` existe. O item 3.AA.2 está marcado ✅ mas não encontrei o arquivo. Para o app web se conectar ao Supabase local, ele precisa de:
```
NEXT_PUBLIC_SUPABASE_URL=http://127.0.0.1:54321
NEXT_PUBLIC_SUPABASE_ANON_KEY=<anon_key_local>
```

**Ana (Arquiteta):**
Concordo com ambos. A correção é simples mas bloqueadora. Devemos:
1. Adicionar `GOOGLE_OAUTH_*` ao `.env` raiz
2. Verificar/criar `apps/web/.env.local` com as variáveis do Supabase local
3. Reiniciar Supabase local (`supabase stop && supabase start`)
4. Testar o fluxo de login com Google

### 4. Prioridade do CEO: Login com Google 🔥

**Roberto (Frontend Sênior):**
A página `/login` já tem tudo implementado:
- Botão "Entrar com Google" com `signInWithOAuth({ provider: "google" })`
- Rota `/auth/callback` com `exchangeCodeForSession(code)`
- Rota `/auth/signout` funcional
- UI com estados de loading, error, sucesso
- Redirect para `/dashboard` após login

**O que falta para funcionar:**
1. Variáveis `GOOGLE_OAUTH_*` no `.env` (gap identificado acima)
2. `apps/web/.env.local` apontando para Supabase local
3. Reiniciar Supabase local para carregar o provider
4. Testar o fluxo end-to-end

**Thiago (Front Engineer):**
A coerência de URIs está correta. O CEO configurou tanto:
- `http://127.0.0.1:54321/auth/v1/callback` (Supabase Auth local recebe o callback do Google)
- `http://localhost:3000/api/auth/callback/google` (app web — mas nosso app usa outra rota)

Uma observação: nossa rota de callback no Next.js é `/auth/callback`, não `/api/auth/callback/google`. O Supabase Auth intercepta o callback OAuth em `/auth/v1/callback` e depois redireciona para a URL configurada no `signInWithOAuth`. Então o fluxo real é:

```
Login button → Google OAuth → Redireciona para Supabase /auth/v1/callback → 
Supabase troca código → Redireciona para app /auth/callback → 
App troca sessão → Vai para /dashboard
```

A URI `http://127.0.0.1:54321/auth/v1/callback` é a que realmente importa para o fluxo local. ✅

### 5. Fila de Trabalho Imediata

**Ana (Arquiteta):**
Proposta de priorização, alinhada com o CEO que quer "entrar na ferramenta com Gmail":

**Bloco A — Google Auth E2E (URGENTE)**
1. Adicionar `GOOGLE_OAUTH_CLIENT_ID` e `GOOGLE_OAUTH_CLIENT_SECRET` ao `.env`
2. Criar/verificar `apps/web/.env.local` para o Supabase local
3. Reiniciar Supabase local
4. Testar login com Google localmente (3.AA.9)
5. Testar login com Google em staging (3.AA.10)
6. Verificar se dashboard carrega corretamente após login

**Bloco B — GitLab CI (DESBLOQUEADO)**
7. Substituir placeholder deploy-web por job real com Vercel CLI (1.22)
8. Validar pipeline completo em branch develop (1.23)
9. Deploy staging end-to-end via pipeline (1.24)
10. Documentar política staging → production (1.25)

**Bloco C — Preparação Gmail Worker (PLANEJAMENTO)**
11. CEO: criar label "Comprovantes" no Gmail (3.8)
12. CEO: mover e-mails de teste para a label (3.9)
13. Gerar primeiro refresh token do Gmail (3.6)
14. Armazenar segredos no Supabase Vault (3.7)
15. Iniciar implementação do Gmail OAuth client (3.10)

### 6. Discussão: Gmail Worker (Fase 3B)

**João (Backend Sênior):**
O Gmail Scanner atualmente é um **stub vazio** (`export {};`). Para implementá-lo, precisamos:

1. **OAuth client** para Gmail API (diferente do login OAuth — precisa de scopes específicos como `gmail.readonly`)
2. **Refresh token** para acesso offline (CEO precisa autorizar via fluxo OAuth interativo)
3. **Gmail API v1** — listMessages, getMessage, getAttachment
4. **Integração** com o pipeline de ingestão existente (criar `source_document` + `ingestion_job`)

**Maria (Backend Sênior):**
Importante: o Google OAuth para **login** (Supabase Auth) e o OAuth para **Gmail API** (acesso a e-mails) são fluxos diferentes, mesmo usando o mesmo Client ID. O login só precisa de profile/email scopes. O Gmail precisa de `gmail.readonly` no mínimo.

O refresh token (3.6) precisa ser gerado via fluxo interativo com o CEO. Vamos precisar de um script helper para isso.

**Ana (Arquiteta):**
Concordo. A separação é:
- **Login OAuth** → Supabase Auth gerencia tudo (já implementado, falta env vars)
- **Gmail API OAuth** → Nosso worker gerencia tokens (refresh token + access token), precisa de implementação completa

Devemos documentar essa distinção no passo-a-passo para evitar confusão futura.

### 7. Discussão: OpenAI (Fase 7) — Planejamento Inicial

**Ricardo (Economista):**
Do ponto de vista de negócio, a IA deve ser usada para:
- Classificação automática de documentos (conta de luz, boleto, comprovante)
- Extração de dados estruturados de PDFs não padronizados
- Sugestão de categoria/tags/fornecedor
- Assistente conversacional para o usuário fazer perguntas sobre suas finanças

**Camila (Finanças Pessoais):**
O mais valioso para o CEO no curto prazo é a **classificação automática**. Quando os anexos do Gmail chegarem, o sistema precisa:
1. Identificar o tipo de documento (utility bill, bank statement, receipt, etc.)
2. Extrair os dados relevantes (valor, data, fornecedor, parcela)
3. Mapear para o modelo de domínio correto (transaction, recurring_template, consumption_metric)

**Ana (Arquiteta):**
A Fase 7 é posterior à Fase 3B. Não devemos implementar nada de IA agora, mas é importante que o pipeline de parsing (Fase 4, já implementada) tenha a extensão correta para receber um "parser IA" no futuro. Verificando: sim, o `parse-orchestrator.ts` já tem estrutura de pluggable parsers. Estamos preparados.

---

## Prós e Contras

### Opção A: Resolver Google Auth primeiro, depois Gmail Worker

**Prós:**
- CEO consegue entrar na ferramenta imediatamente
- Validação rápida do fluxo completo de autenticação
- Desbloqueia o dashboard e a experiência de uso
- Baixo esforço (apenas variáveis de ambiente + teste)

**Contras:**
- Gmail Worker fica para a próxima iteração

### Opção B: Resolver Google Auth + GitLab CI em paralelo

**Prós:**
- Maximiza produtividade aproveitando os 4 itens desbloqueados
- CI/CD funcional acelera todas as entregas seguintes
- Sem dependência entre os dois blocos

**Contras:**
- Mais complexo para acompanhar em paralelo
- CI/CD não é prioridade imediata do CEO

### Opção C: Google Auth + Planejamento Gmail Worker em paralelo

**Prós:**
- CEO tem login funcionando rapidamente
- Planejamento do Gmail Worker não trava — quando Auth funcionar, já temos roadmap
- Alinhado com pedido do CEO ("entrar na ferramenta" + "fase de integração")

**Contras:**
- CI/CD (1.22-1.25) fica adiado

---

## Decisão Final

**CEO (Chico):** A prioridade é clara — quero entrar na ferramenta com meu Gmail. Depois disso, quero ver o Gmail Worker andando.

**Decisão:** Opção C — Google Auth + Planejamento Gmail Worker em paralelo.

1. **Imediato** (Bloco A): Corrigir variáveis de ambiente, reiniciar Supabase, testar Google Auth local e staging
2. **Em seguida**: CEO prepara label Gmail + e-mails teste (3.8, 3.9) enquanto equipe implementa
3. **Próxima sessão**: Implementação do Gmail Worker (Fase 3B) — início com OAuth client e scan por label
4. **GitLab CI** (Bloco B): Fica para quando houver capacidade, não é bloqueador imediato

---

## Ações / Responsáveis / Prazo

| # | Ação | Responsável | Prazo |
|---|------|-------------|-------|
| A1 | Adicionar `GOOGLE_OAUTH_CLIENT_ID` e `GOOGLE_OAUTH_CLIENT_SECRET` ao `.env` | Fernando | Imediato |
| A2 | Criar/verificar `apps/web/.env.local` para Supabase local | Fernando | Imediato |
| A3 | Reiniciar Supabase local com Google Auth provider | Fernando | Imediato |
| A4 | Testar login com Google localmente (3.AA.9) | Fernando + Roberto | Imediato |
| A5 | Testar login com Google em staging (3.AA.10) | Fernando + Roberto | Após A4 |
| A6 | Configurar Google Auth no Supabase Production | CEO | Quando produção for prioridade |
| B1 | CEO: criar label "Comprovantes" no Gmail | CEO | 2026-03-24 |
| B2 | CEO: mover e-mails teste para a label | CEO | 2026-03-24 |
| C1 | Implementar script para gerar refresh token Gmail (3.6) | João | 2026-03-25 |
| C2 | Início do Gmail Worker — OAuth client (3.10) | João | 2026-03-25 |
| D1 | Job real deploy-web no GitLab CI (1.22) | Fernando | Quando houver capacidade |

---
