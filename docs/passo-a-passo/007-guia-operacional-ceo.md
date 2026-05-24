# Passo-a-Passo — Guia Operacional do CEO

> Documento consolidado: onde está cada coisa, como testar local/staging/produção, como promover dados.

---

## 1. Estrutura do Projeto — Onde Está Cada Coisa

```
seu.bolso.feliz/
├── apps/
│   ├── web/                  ← App Next.js (porta 3105)
│   │   └── src/
│   │       ├── app/
│   │       │   ├── dashboard/        ← Todas as páginas do app
│   │       │   │   ├── documents/    ← Upload e gestão de documentos
│   │       │   │   ├── ingestion/    ← [NOVO] Pipeline de ingestão
│   │       │   │   ├── institutions/ ← Bancos e instituições
│   │       │   │   ├── transactions/ ← Transações financeiras
│   │       │   │   ├── suppliers/    ← Fornecedores
│   │       │   │   ├── liabilities/  ← Dívidas
│   │       │   │   ├── recurring/    ← Recorrências
│   │       │   │   ├── statements/   ← Faturas
│   │       │   │   ├── logs/         ← [NOVO] Logs do sistema
│   │       │   │   └── settings/     ← Configurações
│   │       │   ├── actions/          ← Server actions (backend do Next)
│   │       │   ├── api/              ← API routes (chat IA, etc.)
│   │       │   └── login/            ← Tela de login
│   │       ├── components/           ← Componentes reutilizáveis
│   │       └── lib/supabase/         ← Clients Supabase (server/client)
│   ├── mobile/               ← App React Native (não prioridade agora)
│   └── mcp-server/           ← Servidor MCP (8 tools)
├── workers/
│   ├── gmail-scanner/        ← Scanner de e-mails do Gmail
│   ├── ingestion/            ← Worker de processamento de documentos
│   └── local-scanner/        ← Scanner de pasta local
├── packages/
│   ├── domain/               ← Regras de negócio puras
│   ├── shared-types/         ← Tipos TypeScript compartilhados
│   ├── validation/           ← Schemas Zod
│   └── ingestion-types/      ← Tipos do pipeline de ingestão
├── supabase/
│   ├── migrations/           ← 19 migrations SQL
│   ├── functions/            ← Edge Functions (merge-suppliers, etc.)
│   └── config.toml           ← Config local do Supabase
├── secrets/                  ← Credenciais Google OAuth (NÃO commitar)
├── __tests__/                ← Testes (168+ passando)
└── docs/                     ← Documentação completa
```

---

## 2. Como Testar Localmente

### Pré-requisitos

- Bun instalado (`curl -fsSL https://bun.sh/install | bash`)
- Docker rodando (para Supabase local)
- Node.js 20+ (para Next.js)

### Passo 1 — Iniciar Supabase local

```bash
npx supabase start
# Aguardar ~30s. Vai mostrar URLs e keys locais.
npx supabase status
# Anotar: API URL, anon key, service_role key
```

### Passo 2 — Configurar variáveis de ambiente

```bash
# Copiar o .env.example para .env.local na raiz (se não existir, criar):
cp .env.example .env.local

# Preencher com os valores do `supabase status`:
# NEXT_PUBLIC_SUPABASE_URL=http://127.0.0.1:54321
# NEXT_PUBLIC_SUPABASE_ANON_KEY=eyJ...
# SUPABASE_SERVICE_ROLE_KEY=eyJ...
```

### Passo 3 — Instalar dependências

```bash
bun install
```

### Passo 4 — Rodar o app web

```bash
bun run dev
# Abre http://localhost:3105
```

### Passo 5 — Pipeline de ingestão (dados no dashboard)

**Terminal 1 — Scanner Gmail (se tiver credenciais):**

```bash
bun run pipeline:passo01:scan-gmail:10
```

**Terminal 1 (alternativa) — Scanner local:**

```bash
bun run pipeline:passo01:scan-local
```

**Terminal 2 — Worker de ingestão:**

```bash
bun run pipeline:passo02:ingest
```

**Terminal 3 — App web:**

```bash
bun run pipeline:passo03:dev
```

### Passo 6 — Rodar testes

```bash
bun run test          # Todos os testes
bun run test:unit     # Só unidade
```

---

## 3. Credenciais e Serviços Externos

### Google OAuth (Gmail Scanner)

**Onde está:** `secrets/client_secret_*.json`

**Como obter:**

1. Acesse https://console.cloud.google.com
2. Projeto "Seu Bolso Feliz"
3. APIs & Services > Credentials
4. OAuth 2.0 Client IDs > Download JSON
5. Salvar em `secrets/`

**Como gerar token:**

```bash
bun run get:gmail-token
# Vai abrir navegador para autenticar
# Token salvo em secrets/gmail-token.json
```

### Google OAuth (Login no app — Supabase Auth)

**Configurar em cada ambiente:**

1. Console Supabase > Authentication > Providers > Google
2. Preencher Client ID e Client Secret (do Google Cloud Console)
3. Redirect URL: `https://<supabase-url>/auth/v1/callback`

### OpenAI (Marco 4)

**Como obter:**

1. Acesse https://platform.openai.com/api-keys
2. Crie uma key
3. Adicione em `.env.local`:
   ```
   OPENAI_API_KEY=sk-...
   ```
4. Em staging: Vercel > Settings > Environment Variables > OPENAI_API_KEY

---

## 4. Ambientes

| Ambiente     | Supabase Project           | URL Web               | Uso                    |
| ------------ | -------------------------- | --------------------- | ---------------------- |
| **Local**    | Docker local (porta 54321) | http://localhost:3105 | Desenvolvimento        |
| **Staging**  | `dcljzgjgnkmxdvhybvpt`     | Vercel (a configurar) | Testes com dados reais |
| **Produção** | `opwelsgdhksuuewdbefk`     | Vercel (a configurar) | Uso final              |

### Como acessar o Supabase de cada ambiente

```bash
# Local
npx supabase status

# Staging
npx supabase db push --linked --project-ref dcljzgjgnkmxdvhybvpt

# Produção
npx supabase db push --linked --project-ref opwelsgdhksuuewdbefk
```

---

## 5. Como Testar em Staging

### Pré-requisitos

- [ ] Deploy Vercel configurado (Marco 1)
- [ ] Google OAuth configurado no Supabase Staging
- [ ] Env vars no Vercel

### Passos

1. Push na branch principal → Vercel faz deploy automático
2. Acesse a URL do Vercel
3. Faça login com Google
4. Dashboard deve aparecer
5. `/dashboard/ingestion` para ver documentos (depois do Marco 2)
6. `/dashboard/logs` para ver logs do sistema (Marco 1)

### Como enviar dados para staging

```bash
# Opção A: Upload manual pela UI (Marco 3)
# Arraste PDF na página /dashboard/ingestion

# Opção B: Via Edge Function trigger-ingestion (Marco 1)
# POST para a Edge Function com source_type e parâmetros

# Opção C: Promoção local → staging (Marco 6)
bun run scripts/promote.ts --from local --to staging --scope suppliers,categories --dry-run
```

---

## 6. Como Promover para Produção

### Entidades promovíveis

| Entidade                 | Promovível? | Como                                 |
| ------------------------ | ----------- | ------------------------------------ |
| Suppliers (fornecedores) | ✅          | promote.ts --scope suppliers         |
| Categories               | ✅          | promote.ts --scope categories        |
| Document patterns        | ✅          | promote.ts --scope document_patterns |
| Migrations               | ✅          | supabase db push                     |
| Source documents         | ❌          | Re-ingerir no ambiente destino       |
| Drafts                   | ❌          | Re-ingerir/re-processar              |
| Transactions             | ❌          | Geradas via aprovação de drafts      |
| Logs                     | ❌          | Específicos do ambiente              |

### Comando (Marco 6)

```bash
# Dry-run primeiro (sempre!)
bun run scripts/promote.ts \
  --from staging \
  --to production \
  --scope suppliers,document_patterns \
  --dry-run

# Se tudo ok, executar de verdade
bun run scripts/promote.ts \
  --from staging \
  --to production \
  --scope suppliers,document_patterns
```

---

## 7. Scripts Úteis

```bash
# Desenvolvimento
bun run dev                     # App web local (porta 3105)
bun run test                    # Todos os testes
bun run typecheck               # Verificar tipos

# Pipeline de ingestão
bun run pipeline:passo01:scan-gmail:10   # Scan 10 emails
bun run pipeline:passo01:scan-local      # Scan pasta local
bun run pipeline:passo02:ingest          # Worker de processamento

# Banco de dados
npx supabase start              # Inicia Supabase local
npx supabase stop               # Para Supabase local
npx supabase db reset           # Reset completo (cuidado!)
npx supabase db push            # Aplica migrations

# Tipos
bun run generate-types          # Gera tipos do Supabase

# Lint e formato
bun run lint                    # ESLint
bun run format                  # Prettier
```

---

## 8. Fluxo de Trabalho Git

```
Branch principal: 002-ingestao-dados-e-IA

Commits: pequenos, claros, frequentes
Formato: tipo(escopo): descrição
Exemplos:
  feat(web): add /dashboard/ingestion route
  feat(web): add system_logs migration
  fix(worker): handle null supplier_id
  docs(checklist): update M1 progress
```

**Push fica por conta do CEO.** O time faz commits locais.
