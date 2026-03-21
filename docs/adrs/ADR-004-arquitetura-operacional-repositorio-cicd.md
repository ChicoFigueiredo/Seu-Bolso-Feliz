# ADR-004 — Arquitetura Operacional: Repositório, CI/CD e Padrão de Engenharia

## Status

**Aprovada** — 2026-03-21

## Contexto

O projeto Seu Bolso Feliz possui arquitetura funcional e de domínio aprovadas (ADRs 001–003, guia de implementação em 5 etapas, refinos de kickoff, fornecedor e checkpoint). Porém, antes de iniciar a implementação, é necessário definir formalmente como o código será organizado, versionado, testado, integrado e implantado.

**Restrições relevantes:**

- O repositório será hospedado no **GitLab** (não GitHub)
- A integração nativa Supabase↔GitHub **não está disponível** — toda automação com Supabase será feita via **Supabase CLI**
- O projeto possui múltiplas bases de código: web (Next.js), mobile (React Native/Expo), backend (Supabase), pacotes compartilhados
- Bun é o runtime e gerenciador de pacotes do projeto
- serverless-first — sem VPS no MVP

**Referências:**

- Ata de refino: `docs/refinos/2026-03/2026-03-21-14-19-refino-arquitetura-engenharia-cicd.md`
- ADR-001: Deduplicação transação↔item de fatura
- ADR-002: Norma consumption_metrics
- ADR-003: Governança de aliases de fornecedor
- Guia de implementação: `docs/planejamento/001-guia-implementacao-passo-a-passo.md`

## Decisão

### 1. Estratégia de Repositório: Monorepo com Bun Workspaces

**Monorepo** com **Bun workspaces** nativos. Justificativa: tipos compartilhados entre web/mobile/backend, pipeline único, refactors atômicos, time único.

Turborepo será avaliado quando/se performance do pipeline degradar. Por ora, Bun workspaces + jobs GitLab condicionais são suficientes.

### 2. Estrutura de Pastas

```
seu.bolso.feliz/
├── .gitlab-ci.yml
├── .env.example
├── package.json              # workspaces: ["apps/*", "packages/*"]
├── bun.lock
├── bunfig.toml
├── tsconfig.base.json
├── commitlint.config.ts
├── .lintstagedrc.json
│
├── apps/
│   ├── web/                  # Next.js + Tailwind + shadcn/ui
│   │   ├── package.json
│   │   ├── tsconfig.json     # extends ../../tsconfig.base.json
│   │   ├── next.config.ts
│   │   └── src/
│   │       ├── app/          # App Router
│   │       ├── components/
│   │       ├── hooks/
│   │       ├── lib/
│   │       └── styles/
│   │
│   └── mobile/               # React Native + Expo
│       ├── package.json
│       ├── tsconfig.json
│       ├── app.json
│       └── src/
│
├── packages/
│   ├── shared-types/         # @sbf/shared-types — tipos TS gerados + domínio
│   ├── validation/           # @sbf/validation — Zod schemas
│   ├── domain/               # @sbf/domain — lógica financeira pura
│   ├── ui-tokens/            # @sbf/ui-tokens — tokens de design
│   └── config/               # @sbf/config — eslint, tsconfig, vitest configs
│
├── supabase/                 # Supabase CLI (posição fixa)
│   ├── config.toml
│   ├── seed.sql
│   ├── migrations/
│   └── functions/
│
├── docs/                     # Documentação (inalterada)
├── scripts/                  # Automações (generate-types, seed, etc.)
└── __tests__/                # Testes globais (domain, integration, e2e)
```

### 3. Compartilhamento de Código

Quatro pacotes internos via Bun workspaces:

| Pacote              | Escopo                                            | Consumers                           |
| ------------------- | ------------------------------------------------- | ----------------------------------- |
| `@sbf/shared-types` | Tipos TS gerados do Supabase + tipos de domínio   | web, mobile, testes, Edge Functions |
| `@sbf/validation`   | Zod schemas                                       | web, mobile, Edge Functions         |
| `@sbf/domain`       | Lógica financeira pura (cálculos, regras)         | web, mobile, testes                 |
| `@sbf/ui-tokens`    | Tokens de design (cores, espaçamento, tipografia) | web, mobile                         |

**Regra:** componentes visuais NÃO são compartilhados entre web e mobile — apenas lógica, tipos, validação e tokens.

### 4. Convenção de Branches

**Trunk-based simplificado:**

| Branch    | Propósito | Proteção                                                 |
| --------- | --------- | -------------------------------------------------------- |
| `main`    | Produção  | Push proibido, force push proibido, merge somente via MR |
| `develop` | Staging   | Push proibido, force push proibido, merge somente via MR |

**Branches efêmeras (de trabalho):**
`feature/`, `fix/`, `chore/`, `refactor/`, `docs/`, `test/`, `migration/`

- Saem sempre de `develop`
- Nomes em kebab-case, em inglês
- Deletadas após merge

### 5. Convenção de Commits

**Conventional Commits** com enforcement via `commitlint`:

```
<tipo>(<escopo>): <descrição>
```

Tipos: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `ci`, `perf`, `migration`

Escopos: `web`, `mobile`, `domain`, `types`, `validation`, `supabase`, `ci`, `docs`, `config`, `tokens`

Enforcement: husky pre-commit hook (local) + job CI (safety net).

### 6. Política de Merge Requests

**Para `develop`:**

- Pipeline verde obrigatório
- 1 aprovação (qualquer sênior)
- Squash merge
- Branch deletada após merge

**Para `main`:**

- Pipeline verde obrigatório
- 1 aprovação do CEO ou Arquiteta
- Merge commit (preserva contexto de release)
- Tag de versão automática

### 7. Estratégia de Ambientes

| Ambiente   | Supabase                  | Web                    | Trigger              |
| ---------- | ------------------------- | ---------------------- | -------------------- |
| local      | `supabase start` (Docker) | `bun dev`              | Manual               |
| staging    | Projeto dedicado (free)   | Deploy automático      | Merge para `develop` |
| production | Projeto dedicado (pago)   | Deploy com gate manual | Merge para `main`    |

Segredos gerenciados via **GitLab CI/CD Variables** (masked, protected). Nunca no repositório.

### 8. Supabase no CI/CD

Toda interação com Supabase no CI via **Supabase CLI** (`npx supabase`):

- `supabase link --project-ref $PROJECT_ID` — linkar projeto
- `supabase db push` — aplicar migrações
- `supabase functions deploy` — deploy de Edge Functions
- `supabase gen types typescript` — gerar tipos

Variáveis GitLab necessárias:

- `SUPABASE_ACCESS_TOKEN` (masked, protected)
- `SUPABASE_DB_PASSWORD` (masked, protected)
- `STAGING_SUPABASE_PROJECT_ID` (protected para develop)
- `PRODUCTION_SUPABASE_PROJECT_ID` (protected para main)

### 9. Estratégia de Migrações

```
Local → PR (review) → CI Staging (auto) → CI Production (manual gate)
```

- Migrações criadas via `supabase migration new <nome>` (timestamp automático)
- Migrações são **imutáveis** após merge — correções geram nova migração
- Migrações destrutivas requerem `BREAKING CHANGE:` no commit
- Seed data aplicada apenas em local e staging
- Job `validate-types` detecta drift schema vs tipos commitados

### 10. Deploy de Edge Functions

Deploy completo por ambiente no merge:

- `supabase functions deploy --project-ref $STAGING_PROJECT_ID` (staging, automático)
- `supabase functions deploy --project-ref $PRODUCTION_PROJECT_ID` (produção, manual gate)

Secrets configurados via dashboard/CLI por ambiente, nunca no repo.

### 11. Geração de Tipos TypeScript

Fluxo: **geração local + validação no CI**

1. Dev cria migração → aplica localmente
2. Dev roda `bun run generate-types`
3. Dev commita `database.types.ts` junto com a migração
4. CI regenera e compara via `diff` — divergência = falha

### 12. Pipeline GitLab CI/CD (6 Stages)

```
validate → install → check → test → build → deploy
```

- **validate:** commits (commitlint), tipos (diff)
- **install:** `bun install --frozen-lockfile`
- **check:** lint, typecheck, format
- **test:** unitários, integração (condicional)
- **build:** web (Next.js), futuro: mobile
- **deploy:** migrações + functions + web, por ambiente

Jobs condicionais por pasta alterada (`changes:`). Deploy produção com `when: manual`.

### 13. Critérios de Qualidade (Quality Gates)

**Bloqueantes (pipeline deve passar):**

1. Commits seguem Conventional Commits
2. ESLint sem erros (`--max-warnings 0`)
3. TypeScript sem erros (`tsc --noEmit`)
4. Prettier formatação correta
5. Testes unitários 100% green
6. Build sem erros
7. Tipos Supabase atualizados (quando migração presente)
8. Cobertura `packages/domain/` ≥ 90%
9. Cobertura `packages/validation/` ≥ 90%

**Metas (não bloqueantes no MVP):**

- Cobertura global ≥ 70%
- Zero `any` explícito em código novo
- Bundle < 300kB gzip

### 14. Etapa 0 — Setup do Monorepo (Sprint 0)

Nova etapa que **precede** todas as etapas do guia de implementação:

```
0.1  Criar estrutura de pastas
0.2  Configurar Bun workspaces
0.3  Migrar código web existente para apps/web/
0.4  Criar pacotes internos (@sbf/*)
0.5  tsconfig.base.json + extends
0.6  ESLint 9 + Prettier
0.7  Vitest
0.8  Husky + lint-staged + commitlint
0.9  supabase init
0.10 .env.example + .gitignore
0.11 .gitlab-ci.yml inicial
0.12 Commit: chore: setup monorepo with Bun workspaces and CI/CD
```

## Consequências

### Positivas

- Projeto nasce com disciplina de engenharia e entrega contínua
- Migrações controladas — sem drift entre ambientes
- Tipos TypeScript sempre sincronizados com schema real
- Quality gates previnem regressões antes de merge
- Compartilhamento de código entre web e mobile sem publicação npm
- Auditabilidade total via histórico Git + pipeline

### Negativas / Trade-offs

- Sprint 0 antes de qualquer feature — investimento inicial em infraestrutura
- Pipeline pode ficar lento conforme projeto cresce — mitigável com Turborepo e cache
- Supabase CLI sem integração nativa GitLab — requer manutenção manual dos scripts de CI
- Monorepo exige disciplina no gerenciamento de dependências

### Riscos Mitigados

- Drift de schema entre ambientes → validação de tipos no CI
- Deploy acidental em produção → gate manual no pipeline
- Secrets vazados → variáveis masked/protected no GitLab
- Migração destrutiva sem controle → imutabilidade de migrações + flag BREAKING CHANGE
- Processo pesado matando velocidade → revisão trimestral do processo

## Rastreabilidade

| Decisão                       | Referência                                                  |
| ----------------------------- | ----------------------------------------------------------- |
| Deduplicação transação↔fatura | ADR-001                                                     |
| Norma consumption_metrics     | ADR-002                                                     |
| Governança aliases            | ADR-003                                                     |
| Edge Functions deploy         | ADR-003 (merge-suppliers, retroactive-supplier-association) |
| Testes mandatórios            | Guia de implementação, Etapa 2                              |
| Migrações SQL                 | Guia de implementação, Etapa 1                              |
