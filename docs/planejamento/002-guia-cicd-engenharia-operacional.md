# 002 — Guia de CI/CD e Engenharia Operacional

> **Referência formal:** ADR-004 — Arquitetura Operacional: Repositório, CI/CD e Padrão de Engenharia
> **Ata de refino:** `docs/refinos/2026-03/2026-03-21-14-19-refino-arquitetura-engenharia-cicd.md`

---

## Sumário

1. [Pré-requisitos](#1-pré-requisitos)
2. [Etapa 0 — Setup do Monorepo (Sprint 0)](#2-etapa-0--setup-do-monorepo-sprint-0)
3. [Configuração do GitLab](#3-configuração-do-gitlab)
4. [Pipeline CI/CD — Referência Completa](#4-pipeline-cicd--referência-completa)
5. [Fluxo de Trabalho Diário](#5-fluxo-de-trabalho-diário)
6. [Gestão de Ambientes e Segredos](#6-gestão-de-ambientes-e-segredos)
7. [Fluxo de Migrações](#7-fluxo-de-migrações)
8. [Deploy de Edge Functions](#8-deploy-de-edge-functions)
9. [Geração de Tipos TypeScript](#9-geração-de-tipos-typescript)
10. [Troubleshooting](#10-troubleshooting)
11. [Checklist de Novo Membro](#11-checklist-de-novo-membro)

---

## 1. Pré-requisitos

### Ferramentas locais obrigatórias

| Ferramenta       | Versão mínima | Instalação                                        |
| ---------------- | ------------- | ------------------------------------------------- |
| **Bun**          | 1.1+          | `curl -fsSL https://bun.sh/install \| bash`       |
| **Docker**       | 24+           | Necessário para `supabase start`                  |
| **Supabase CLI** | 1.x           | Via `bun add -D supabase` (instalado no monorepo) |
| **Git**          | 2.40+         | Já instalado no SO                                |

### Verificação rápida

```bash
bun --version       # >= 1.1
docker --version    # >= 24
git --version       # >= 2.40
```

---

## 2. Etapa 0 — Setup do Monorepo (Sprint 0)

Esta etapa **precede** todas as Etapas do guia de implementação. Execute os passos na ordem.

### 2.1 Estrutura de Pastas

```bash
# Na raiz do repositório
mkdir -p apps/web/src/{app,components/ui,hooks,lib,styles}
mkdir -p apps/web/public
mkdir -p apps/mobile/src/{screens,components,hooks,navigation,lib}
mkdir -p apps/mobile/assets
mkdir -p packages/shared-types/src/domain
mkdir -p packages/validation/src
mkdir -p packages/domain/src/{financial-cycle,amortization,deduplication,priority}
mkdir -p packages/ui-tokens/src
mkdir -p packages/config/{eslint,tsconfig,vitest}
mkdir -p scripts
mkdir -p __tests__/{domain,integration,e2e}
```

### 2.2 Configurar Bun Workspaces

**`package.json` (root):**

```json
{
  "name": "seu-bolso-feliz",
  "private": true,
  "workspaces": ["apps/*", "packages/*"],
  "scripts": {
    "dev": "cd apps/web && bun run dev",
    "dev:mobile": "cd apps/mobile && bun run start",
    "build": "cd apps/web && bun run build",
    "lint": "eslint .",
    "lint:fix": "eslint . --fix",
    "format": "prettier --write .",
    "format:check": "prettier --check .",
    "typecheck": "tsc -b",
    "test": "vitest",
    "test:unit": "vitest run --project unit",
    "test:integration": "vitest run --project integration",
    "test:coverage": "vitest run --coverage",
    "generate-types": "bash scripts/generate-types.sh",
    "db:migrate": "npx supabase db push",
    "db:reset": "npx supabase db reset",
    "prepare": "husky"
  }
}
```

### 2.3 Migrar Código Web Existente

```bash
# Mover arquivos atuais para apps/web/
# (adaptar conforme necessário — preservar componentes shadcn/ui)
mv src/components/ apps/web/src/components/
mv src/lib/ apps/web/src/lib/
mv src/styles/ apps/web/src/styles/ 2>/dev/null || true
mv styles/ apps/web/src/styles/ 2>/dev/null || true
# O App.tsx, frontend.tsx e index.ts serão refatorados para Next.js App Router
```

> **Nota:** O frontend atual é uma SPA com Bun HTTP server. A migração para Next.js App Router é parte da Etapa 0. Os componentes shadcn/ui são compatíveis diretamente.

### 2.4 Pacotes Internos

Cada pacote em `packages/` deve ter seu próprio `package.json`:

**`packages/shared-types/package.json`:**

```json
{
  "name": "@sbf/shared-types",
  "version": "0.0.1",
  "private": true,
  "main": "src/index.ts",
  "types": "src/index.ts"
}
```

**`packages/validation/package.json`:**

```json
{
  "name": "@sbf/validation",
  "version": "0.0.1",
  "private": true,
  "main": "src/index.ts",
  "types": "src/index.ts",
  "dependencies": {
    "zod": "^3.x"
  }
}
```

**`packages/domain/package.json`:**

```json
{
  "name": "@sbf/domain",
  "version": "0.0.1",
  "private": true,
  "main": "src/index.ts",
  "types": "src/index.ts"
}
```

**`packages/ui-tokens/package.json`:**

```json
{
  "name": "@sbf/ui-tokens",
  "version": "0.0.1",
  "private": true,
  "main": "src/index.ts",
  "types": "src/index.ts"
}
```

### 2.5 TypeScript Base

**`tsconfig.base.json` (root):**

```json
{
  "compilerOptions": {
    "lib": ["ESNext", "DOM"],
    "target": "ESNext",
    "module": "Preserve",
    "moduleDetection": "force",
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "verbatimModuleSyntax": true,
    "strict": true,
    "skipLibCheck": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "declaration": true,
    "declarationMap": true,
    "composite": true
  }
}
```

Cada workspace estende com `"extends": "../../tsconfig.base.json"` e define seu `paths` e `references`.

### 2.6 ESLint + Prettier

```bash
# Na raiz do monorepo
bun add -D eslint @eslint/js typescript-eslint prettier eslint-config-prettier
```

**`eslint.config.ts` (root, flat config ESLint 9):**

```typescript
import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import prettier from "eslint-config-prettier";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  prettier,
  {
    ignores: [
      "node_modules",
      "dist",
      ".next",
      "supabase/functions/**", // Edge Functions usam Deno
    ],
  },
);
```

**`.prettierrc`:**

```json
{
  "semi": true,
  "singleQuote": false,
  "trailingComma": "all",
  "tabWidth": 2,
  "printWidth": 100
}
```

### 2.7 Vitest

```bash
bun add -D vitest @vitest/coverage-v8
```

**`vitest.config.ts` (root):**

```typescript
import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    globals: true,
    projects: [
      {
        test: {
          name: "unit",
          include: ["packages/*/src/**/*.test.ts", "__tests__/domain/**/*.test.ts"],
        },
      },
      {
        test: {
          name: "integration",
          include: ["__tests__/integration/**/*.test.ts"],
          testTimeout: 30_000,
        },
      },
    ],
    coverage: {
      provider: "v8",
      include: ["packages/domain/src/**", "packages/validation/src/**"],
      thresholds: {
        "packages/domain/src/**": { lines: 90, branches: 90 },
        "packages/validation/src/**": { lines: 90, branches: 90 },
      },
    },
  },
});
```

### 2.8 Husky + lint-staged + commitlint

```bash
bun add -D husky lint-staged @commitlint/cli @commitlint/config-conventional
bunx husky init
```

**`commitlint.config.ts`:**

```typescript
export default {
  extends: ["@commitlint/config-conventional"],
  rules: {
    "type-enum": [
      2,
      "always",
      ["feat", "fix", "docs", "style", "refactor", "test", "chore", "ci", "perf", "migration"],
    ],
    "scope-enum": [
      1,
      "always",
      [
        "web",
        "mobile",
        "domain",
        "types",
        "validation",
        "supabase",
        "ci",
        "docs",
        "config",
        "tokens",
      ],
    ],
  },
};
```

**`.husky/commit-msg`:**

```bash
bunx commitlint --edit $1
```

**`.husky/pre-commit`:**

```bash
bunx lint-staged
```

**`.lintstagedrc.json`:**

```json
{
  "*.{ts,tsx}": ["eslint --fix", "prettier --write"],
  "*.{json,md,yml,yaml}": ["prettier --write"]
}
```

### 2.9 Supabase Init

```bash
npx supabase init
# Isso cria: supabase/config.toml, supabase/seed.sql
```

### 2.10 Variáveis de Ambiente

**`.env.example`:**

```bash
# ── Supabase ──
NEXT_PUBLIC_SUPABASE_URL=http://localhost:54321
NEXT_PUBLIC_SUPABASE_ANON_KEY=sua-anon-key-aqui
SUPABASE_SERVICE_ROLE_KEY=sua-service-role-key-aqui
SUPABASE_DB_PASSWORD=sua-senha-aqui

# ── Supabase CLI (para scripts e CI) ──
SUPABASE_ACCESS_TOKEN=
SUPABASE_PROJECT_ID=

# ── App ──
NEXT_PUBLIC_APP_URL=http://localhost:3000
NODE_ENV=development
```

**`.gitignore`** (adicionar):

```
.env
.env.local
.env.*.local
supabase/.temp/
```

### 2.11 GitLab CI/CD

Criar `.gitlab-ci.yml` conforme documentado na ata de refino (seção 2.12 da ata). O YAML completo está referenciado lá.

### 2.12 Commit Final do Sprint 0

```bash
git add -A
git commit -m "chore: setup monorepo with Bun workspaces and CI/CD

- Create apps/web, apps/mobile workspace structure
- Create packages: shared-types, validation, domain, ui-tokens, config
- Migrate existing web code to apps/web/
- Setup ESLint 9 (flat config) + Prettier
- Setup Vitest with unit and integration projects
- Setup Husky + lint-staged + commitlint
- Initialize Supabase CLI
- Add .gitlab-ci.yml with 6-stage pipeline
- Add .env.example and update .gitignore"
```

---

## 3. Configuração do GitLab

### 3.1 Proteção de Branches

No GitLab → Settings → Repository → Protected Branches:

| Branch    | Push   | Merge                    | Force Push |
| --------- | ------ | ------------------------ | ---------- |
| `main`    | No one | Maintainers              | ❌         |
| `develop` | No one | Developers + Maintainers | ❌         |

### 3.2 Merge Request Settings

GitLab → Settings → Merge Requests:

- ✅ **Pipelines must succeed** (obrigatório)
- ✅ **All threads must be resolved**
- ✅ **Squash commits** default para MRs em `develop`
- ✅ **Delete source branch** por padrão
- Approvals: mínimo 1

### 3.3 CI/CD Variables

GitLab → Settings → CI/CD → Variables:

| Variável                         | Tipo     | Masked | Protected | Environments |
| -------------------------------- | -------- | ------ | --------- | ------------ |
| `SUPABASE_ACCESS_TOKEN`          | Variable | ✅     | ✅        | All          |
| `SUPABASE_DB_PASSWORD`           | Variable | ✅     | ✅        | All          |
| `STAGING_SUPABASE_PROJECT_ID`    | Variable | ❌     | ✅        | staging      |
| `PRODUCTION_SUPABASE_PROJECT_ID` | Variable | ❌     | ✅        | production   |

---

## 4. Pipeline CI/CD — Referência Completa

### 4.1 Visão Geral dos Stages

```
validate → install → check → test → build → deploy
```

| Stage    | Jobs                       | Quando roda                                                     |
| -------- | -------------------------- | --------------------------------------------------------------- |
| validate | commits, types             | Somente MRs                                                     |
| install  | deps                       | Sempre                                                          |
| check    | lint, typecheck, format    | Sempre                                                          |
| test     | unit, integration          | Sempre (integration condicional)                                |
| build    | web                        | MR com mudança em apps/web ou packages/; merges em develop/main |
| deploy   | migrations, functions, web | Merges em develop (auto) ou main (manual)                       |

### 4.2 Tempos Esperados

| Job                       | Tempo esperado     |
| ------------------------- | ------------------ |
| install                   | ~15s (com cache)   |
| lint + typecheck + format | ~30s (paralelo)    |
| test:unit                 | ~15s (MVP)         |
| test:integration          | ~45s (condicional) |
| build:web                 | ~60s               |
| deploy (staging)          | ~30s               |
| **Total (MR)**            | **~2min**          |
| **Total (develop merge)** | **~3min**          |

---

## 5. Fluxo de Trabalho Diário

### 5.1 Criar Feature

```bash
# 1. Atualizar develop
git checkout develop && git pull

# 2. Criar branch
git checkout -b feature/supplier-crud

# 3. Desenvolver (hot reload)
bun run dev

# 4. Testes locais
bun run test:unit
bun run lint
bun run typecheck

# 5. Commits (Conventional Commits — enforced pelo hook)
git add -A
git commit -m "feat(web): add supplier list page"
git commit -m "test(domain): add supplier validation tests"

# 6. Push e abrir MR
git push -u origin feature/supplier-crud
# Abrir MR no GitLab: feature/supplier-crud → develop
```

### 5.2 Com Migração

```bash
# 1. Criar migração
npx supabase migration new add_suppliers_table

# 2. Editar SQL gerado
# supabase/migrations/YYYYMMDDHHMMSS_add_suppliers_table.sql

# 3. Aplicar localmente
npx supabase db reset  # Reset + aplica todas as migrações + seed

# 4. Gerar tipos
bun run generate-types

# 5. Commitar migração + tipos
git add supabase/migrations/ packages/shared-types/src/database.types.ts
git commit -m "migration(supabase): add suppliers table"
```

### 5.3 Review e Merge

```bash
# Na MR do GitLab:
# 1. Pipeline roda automaticamente
# 2. Reviewer aprova
# 3. Squash Merge para develop
# 4. Branch deletada automaticamente
# 5. Pipeline de develop: auto-deploy staging
```

---

## 6. Gestão de Ambientes e Segredos

### 6.1 Ambiente Local

```bash
# Iniciar Supabase local (Docker)
npx supabase start

# Variáveis impressas no terminal — copiar para .env.local
# SUPABASE_URL, ANON_KEY, SERVICE_ROLE_KEY, etc.

# Iniciar dev server
bun run dev

# Parar Supabase local
npx supabase stop
```

### 6.2 Staging

- **URL Supabase:** configurada no dashboard Supabase (projeto staging)
- **URL Web:** `https://staging.seubolsofeliz.com.br` (provedor TBD)
- **Deploy:** automático no merge para `develop`
- **Seed:** aplicada no reset, não no deploy push

### 6.3 Production

- **URL Supabase:** configurada no dashboard Supabase (projeto produção)
- **URL Web:** `https://app.seubolsofeliz.com.br`
- **Deploy:** gate manual no GitLab (botão "Play" no job)
- **Seed:** NUNCA aplicada em produção

### 6.4 Regra de Ouro

> **NUNCA** armazene segredos no repositório. Nem em `.env`, nem em comentários, nem em código. Use `.env.local` (gitignored) para local e GitLab CI/CD Variables para CI.

---

## 7. Fluxo de Migrações

### 7.1 Criar Migração

```bash
npx supabase migration new <nome_descritivo>
# Gera: supabase/migrations/YYYYMMDDHHMMSS_nome_descritivo.sql
```

### 7.2 Testar Localmente

```bash
# Resetar banco local (aplica todas as migrações + seed)
npx supabase db reset

# OU aplicar apenas pendentes
npx supabase db push --local
```

### 7.3 Gerar Tipos

```bash
bun run generate-types
# Atualiza packages/shared-types/src/database.types.ts
```

### 7.4 Regras de Ouro

1. **Nunca editar** migração já mergeada — crie nova migração corretiva
2. **Sempre usar** `supabase migration new` — nunca criar SQL com timestamp manual
3. **Sempre commitar** tipos atualizados junto com a migração
4. **Migrações destrutivas** (DROP, ALTER TYPE) → `BREAKING CHANGE:` no commit
5. **Seed data** → apenas em `supabase/seed.sql`, nunca em migrações

---

## 8. Deploy de Edge Functions

### 8.1 Criar Nova Função

```bash
npx supabase functions new <nome-da-funcao>
# Cria: supabase/functions/<nome-da-funcao>/index.ts
```

### 8.2 Testar Localmente

```bash
npx supabase functions serve <nome-da-funcao>
# Inicia servidor local para a função
```

### 8.3 Deploy

Deploy é automático no merge para `develop` (staging) e manual para `main` (produção).

Para deploy manual emergencial (usar com cautela):

```bash
npx supabase functions deploy <nome-da-funcao> --project-ref <project-id>
```

### 8.4 Secrets das Funções

```bash
# Configurar via CLI
npx supabase secrets set MY_SECRET=value --project-ref <project-id>

# Listar secrets
npx supabase secrets list --project-ref <project-id>
```

---

## 9. Geração de Tipos TypeScript

### 9.1 Script

**`scripts/generate-types.sh`:**

```bash
#!/usr/bin/env bash
set -euo pipefail
npx supabase gen types typescript --local > packages/shared-types/src/database.types.ts
echo "✅ Tipos gerados em packages/shared-types/src/database.types.ts"
```

### 9.2 Uso no CI

O job `validate-types` regenera os tipos a partir do schema do projeto staging e compara com o arquivo commitado. Se houver divergência, o pipeline falha — indicando que o desenvolvedor esqueceu de regenerar os tipos após a migração.

---

## 10. Troubleshooting

### Pipeline falha em `validate-types`

**Causa:** Tipos commitados não correspondem ao schema atual.

**Solução:**

```bash
bun run generate-types
git add packages/shared-types/src/database.types.ts
git commit -m "chore(types): regenerate database types"
git push
```

### Pipeline falha em `validate-commits`

**Causa:** Mensagem de commit fora do padrão Conventional Commits.

**Solução:** Reescrever commits com `git rebase -i` e corrigir mensagens. Ou fazer squash na MR com título correto.

### `supabase db push` falha no CI

**Causa possível:** Migração com erro SQL, conflito de schema, ou token expirado.

**Solução:**

1. Testar migração localmente: `npx supabase db reset`
2. Verificar que `SUPABASE_ACCESS_TOKEN` está válido no GitLab
3. Verificar logs do job para mensagem de erro específica

### `bun install --frozen-lockfile` falha

**Causa:** `bun.lock` desatualizado em relação a `package.json`.

**Solução:**

```bash
bun install
git add bun.lock
git commit -m "chore: update lockfile"
git push
```

---

## 11. Checklist de Novo Membro

Para um novo desenvolvedor que entra no projeto:

- [ ] Instalar Bun (`curl -fsSL https://bun.sh/install | bash`)
- [ ] Instalar Docker
- [ ] Clonar repositório (`git clone ...`)
- [ ] `bun install` na raiz
- [ ] Copiar `.env.example` para `.env.local` e preencher
- [ ] `npx supabase start` (iniciar banco local)
- [ ] `npx supabase db reset` (aplicar migrações + seed)
- [ ] `bun run generate-types` (gerar tipos TS)
- [ ] `bun run dev` (iniciar dev server)
- [ ] `bun run test` (rodar testes)
- [ ] Ler ADRs em `docs/adrs/`
- [ ] Ler guia de implementação em `docs/planejamento/001-guia-implementacao-passo-a-passo.md`
- [ ] Ler esta documentação de CI/CD
