---
Título da Reunião: Refino de Arquitetura de Engenharia, CI/CD e Padrão Operacional
Data e Hora: 2026-03-21 14:19
Participantes:
  - Chico (CEO) — facilitador, decisão final
  - Ana Silva (Arquiteta de Software) — líder técnica da discussão
  - Fernando Gomes (DevOps Sênior) — responsável por CI/CD, pipelines e infraestrutura
  - João Pereira (Backend Sênior — Node/Bun) — referência em runtime Bun, build e tooling
  - Maria Oliveira (Backend Sênior — Node/Bun) — segurança de APIs, testes automatizados
  - Roberto Lima (Frontend Sênior — React/Next.js) — arquitetura frontend, build web
  - Sofia Almeida (Frontend Sênior — React/Next.js) — componentização, design system
  - Lucas Ferreira (Mobile Sênior — React Native/Expo) — build mobile, compartilhamento de código
  - Thiago Martins (Front Engineer) — tipagem, performance, padrões de código
  - André Santos (DBA Sênior — PostgreSQL) — migrações, integridade de schema
  - Renata Silva (QA Visual/A11y) — qualidade, gates de merge
  - Ricardo Monteiro (Economista) — validação de regras financeiras em testes
  - Camila Duarte (Consultora Finanças Pessoais) — observadora
Pauta:
  - 1. Estratégia de repositório (monorepo vs polyrepo)
  - 2. Estrutura de pastas do monorepo
  - 3. Estratégia de compartilhamento de código entre web, mobile e backend
  - 4. Convenção de branches
  - 5. Convenção de commits
  - 6. Política de Merge Requests e proteção de branches
  - 7. Estratégia de ambientes (local, staging, production)
  - 8. Supabase no CI/CD com GitLab (sem integração nativa GitHub)
  - 9. Estratégia de migrações no pipeline
  - 10. Deploy de Edge Functions por ambiente
  - 11. Geração e validação de tipos TypeScript a partir do schema
  - 12. Design do pipeline GitLab CI/CD
  - 13. Critérios de qualidade para merge
  - 14. Riscos e armadilhas a evitar
  - 15. Recomendação final de arquitetura operacional
---

# 1. Contexto e Motivação

**Verônica (parecer formal):** A arquitetura funcional e de domínio está aprovada — entidades, relacionamentos, ADRs, estratégia de testes e guia de implementação estão sólidos. Porém, antes de escrever a primeira linha de código, é obrigatório definir o **padrão de engenharia operacional**: como o código será organizado, versionado, testado, integrado e implantado. Sem isso, o risco é alto de builds manuais, migrações descontroladas, deploys ad-hoc e inconsistência entre ambientes.

**Estado atual do repositório:**

- Repositório flat (sem workspaces, sem monorepo)
- Build via `build.ts` (Bun nativo) para SPA simples
- Sem CI/CD configurado (nenhum `.gitlab-ci.yml` ou Actions)
- Sem Supabase inicializado (`supabase init` não executado)
- Sem framework de testes instalado
- Sem linting/formatting configurados
- Sem git hooks
- Sem `.env.example` ou gestão de segredos
- Frontend: React 19 + shadcn/ui + Tailwind — funcional mas sem rotas, estado ou API client
- Mobile: planejado mas inexistente no repositório
- Docs: excelente organização (ADRs, refinos, planejamento)

**Restrição crítica:** O projeto será hospedado no **GitLab** (não GitHub). A integração nativa Supabase↔GitHub **não está disponível**. Toda automação com Supabase será feita via **Supabase CLI** nos pipelines.

---

# 2. Discussão por Tópico

---

## 2.1 Estratégia de Repositório

### Debate

**Ana Silva (Arquiteta):**

> Temos quatro bases de código que precisam conviver: web (Next.js), mobile (React Native/Expo), Supabase (migrações + Edge Functions) e pacotes compartilhados (tipos, validação, domínio, UI tokens). Polyrepo geraria overhead monstruoso de sincronização — versionar tipos entre 3+ repos é pesadelo. Monorepo é o caminho natural.

**Fernando Gomes (DevOps):**

> Concordo. Monorepo simplifica CI/CD: um `.gitlab-ci.yml`, um pipeline, jobs condicionais por pasta alterada. Com polyrepo, precisaríamos de triggers cross-repo, webhooks, e sincronização de versões — complexidade desnecessária no MVP.

**João Pereira (Backend):**

> Bun suporta workspaces nativamente via `package.json`. Temos suporte de primeira classe. Sem necessidade de ferramentas externas como Lerna.

**Lucas Ferreira (Mobile):**

> Para React Native/Expo, monorepo funciona bem. Expo tem suporte oficial a monorepos com workspaces. Podemos compartilhar tipos, validação e até hooks entre web e mobile.

**Roberto Lima (Frontend):**

> Next.js também funciona bem em monorepo com workspaces. Turborepo pode orquestrar builds com cache, mas não é obrigatório no MVP — Bun workspaces já resolvem.

### Prós e Contras

**Monorepo:**

- **Prós:**
  - Compartilhamento de tipos, validação e domínio sem publicação npm
  - Pipeline único — simplicidade operacional
  - Refactors atômicos (mudar tipo → atualizar consumers no mesmo PR)
  - Visibilidade total do estado do projeto
  - Bun workspaces: suporte nativo, sem ferramentas extras
- **Contras:**
  - Pipeline pode ficar lento se não filtrar por pasta alterada
  - Clone inicial maior (mitigável com shallow clone)
  - Permissões granulares mais difíceis (mitigável com CODEOWNERS)

**Polyrepo:**

- **Prós:**
  - Isolamento total de deploys
  - Permissões por repo
- **Contras:**
  - Sincronização de tipos/versões entre repos — overhead enorme
  - Múltiplos pipelines para manter
  - PRs cross-repo para mudanças de contrato
  - Complexidade desproporcional ao tamanho da equipe

### Decisão: **MONOREPO com Bun Workspaces**

Justificativa: projeto tem um time único, tipos compartilhados entre web/mobile/backend, e um único backend (Supabase). O modelo polyrepo adicionaria complexidade sem benefício. Turborepo será avaliado quando/se o pipeline ficar lento — por ora, Bun workspaces + jobs GitLab condicionais são suficientes.

---

## 2.2 Estrutura de Pastas do Monorepo

### Debate

**Ana Silva (Arquiteta):**

> Proponho a seguinte estrutura. `apps/` para deployáveis (web, mobile), `packages/` para bibliotecas internas, `supabase/` no root porque o CLI espera essa localização, e `docs/` como já está.

**Thiago Martins (Front Engineer):**

> Concordo com separar `packages/shared-types` dos outros. Tipos gerados pelo Supabase vão para lá, e web + mobile importam de `@sbf/shared-types`. Validação (Zod schemas) em `packages/validation` compartilhada também.

**Fernando Gomes (DevOps):**

> `scripts/` no root para automações de CI, seeds, helpers. `supabase/` no root é obrigatório — o CLI espera `supabase/migrations/`, `supabase/functions/`, `supabase/config.toml` nessa posição relativa.

**Sofia Almeida (Frontend):**

> Para UI compartilhada entre web e mobile, proponho `packages/ui-tokens` com tokens de design (cores, espaçamento, tipografia) e `packages/ui` para componentes web (shadcn). Mobile terá seus próprios componentes, mas consumindo os mesmos tokens.

### Estrutura Aprovada

```
seu.bolso.feliz/
├── .gitlab-ci.yml                  # Pipeline CI/CD único
├── .gitignore
├── .env.example                    # Template de variáveis (sem segredos)
├── package.json                    # Root — workspaces config
├── bun.lock                        # Lockfile (Bun)
├── bunfig.toml                     # Config Bun
├── tsconfig.base.json              # Config TS base compartilhada
├── commitlint.config.ts            # Conventional Commits
├── .lintstagedrc.json              # Lint-staged config
│
├── apps/
│   ├── web/                        # Next.js + Tailwind + shadcn
│   │   ├── package.json
│   │   ├── tsconfig.json           # extends ../../tsconfig.base.json
│   │   ├── next.config.ts
│   │   ├── tailwind.config.ts
│   │   ├── src/
│   │   │   ├── app/                # App Router (Next.js 14+)
│   │   │   ├── components/         # Componentes específicos web
│   │   │   │   └── ui/             # shadcn/ui components
│   │   │   ├── hooks/              # Hooks específicos web
│   │   │   ├── lib/                # Utilitários web
│   │   │   └── styles/             # CSS/Tailwind
│   │   └── public/                 # Assets estáticos
│   │
│   └── mobile/                     # React Native + Expo
│       ├── package.json
│       ├── tsconfig.json
│       ├── app.json                # Expo config
│       ├── src/
│       │   ├── screens/
│       │   ├── components/
│       │   ├── hooks/
│       │   ├── navigation/
│       │   └── lib/
│       └── assets/
│
├── packages/
│   ├── shared-types/               # Tipos gerados do Supabase + tipos de domínio
│   │   ├── package.json            # name: @sbf/shared-types
│   │   ├── tsconfig.json
│   │   └── src/
│   │       ├── database.types.ts   # Gerado via supabase gen types
│   │       ├── domain/             # Tipos de domínio manual
│   │       └── index.ts
│   │
│   ├── validation/                 # Zod schemas compartilhados
│   │   ├── package.json            # name: @sbf/validation
│   │   ├── tsconfig.json
│   │   └── src/
│   │
│   ├── domain/                     # Lógica de domínio pura (sem I/O)
│   │   ├── package.json            # name: @sbf/domain
│   │   ├── tsconfig.json
│   │   └── src/
│   │       ├── financial-cycle/    # Cálculos de ciclo financeiro
│   │       ├── amortization/       # SAC, Price, cálculos de parcela
│   │       ├── deduplication/      # Lógica de deduplicação
│   │       └── priority/           # Priorização de pagamentos
│   │
│   ├── ui-tokens/                  # Tokens de design compartilhados
│   │   ├── package.json            # name: @sbf/ui-tokens
│   │   └── src/
│   │       ├── colors.ts
│   │       ├── spacing.ts
│   │       ├── typography.ts
│   │       └── index.ts
│   │
│   └── config/                     # Configurações compartilhadas
│       ├── package.json            # name: @sbf/config
│       ├── eslint/                 # ESLint config base
│       ├── tsconfig/               # TS configs compartilhados
│       └── vitest/                 # Vitest config base
│
├── supabase/                       # Supabase CLI (posição obrigatória)
│   ├── config.toml                 # Supabase local config
│   ├── seed.sql                    # Seed data para desenvolvimento
│   ├── migrations/                 # Migrações sequenciais
│   │   ├── 20260321000001_suppliers.sql
│   │   ├── 20260321000002_alter_existing_tables.sql
│   │   └── ...
│   └── functions/                  # Edge Functions (Deno runtime)
│       ├── merge-suppliers/
│       │   └── index.ts
│       └── retroactive-supplier-association/
│           └── index.ts
│
├── docs/                           # Documentação (como está hoje)
│   ├── adrs/
│   ├── planejamento/
│   ├── refinos/
│   └── Veronica/
│
├── scripts/                        # Scripts de automação
│   ├── generate-types.sh           # Gera tipos TS do schema Supabase
│   ├── seed-local.sh               # Seed do banco local
│   └── check-migrations.sh         # Valida migrações pendentes
│
└── __tests__/                      # Testes globais / integração
    ├── domain/                     # Testes de domínio
    ├── integration/                # Testes de integração (Supabase)
    └── e2e/                        # Testes end-to-end (Playwright)
```

### Notas sobre a Estrutura

**André Santos (DBA):**

> As migrações em `supabase/migrations/` usam timestamp como prefixo (`YYYYMMDDHHMMSS_nome.sql`). Isso é padrão do `supabase migration new <nome>` e garante ordenação cronológica.

**Maria Oliveira (Backend):**

> Edge Functions em `supabase/functions/` rodam em Deno runtime (restrição do Supabase). Os tipos compartilhados de `@sbf/shared-types` podem ser importados, mas com cuidado na compatibilidade Deno/Bun.

**Ricardo Monteiro (Economista):**

> Os cálculos financeiros puros (amortização SAC/Price, juros compostos, ciclos) devem ficar em `packages/domain/` — são funções puras, testáveis, sem I/O. Isso permite que tanto web quanto mobile usem a mesma lógica e que os testes mandatórios validem comportamento exato.

---

## 2.3 Estratégia de Compartilhamento de Código

### Debate

**Thiago Martins (Front Engineer):**

> O segredo é definir claramente o que é compartilhável. Proponho três níveis: (1) tipos e contratos — `@sbf/shared-types`, (2) lógica de domínio pura — `@sbf/domain`, (3) validação — `@sbf/validation`. UI NÃO é compartilhada entre web e mobile — apenas tokens de design.

**Lucas Ferreira (Mobile):**

> Concordo. React Native e React web têm primitivas diferentes (`View` vs `div`, `Text` vs `span`). Compartilhar componentes visuais é armadilha. Compartilhar lógica, tipos e tokens é o caminho correto.

**Sofia Almeida (Frontend):**

> Para web, mantemos shadcn/ui dentro de `apps/web/src/components/ui/`. Para mobile, componentes nativos dentro de `apps/mobile/src/components/`. Ambos consomem `@sbf/ui-tokens` para cores, espaçamento e tipografia.

### Decisão: Compartilhamento em 4 Camadas

| Pacote              | Conteúdo                                          | Consumers                           |
| ------------------- | ------------------------------------------------- | ----------------------------------- |
| `@sbf/shared-types` | Tipos TS gerados do Supabase + tipos de domínio   | web, mobile, testes, Edge Functions |
| `@sbf/validation`   | Zod schemas para validação de formulários e API   | web, mobile, Edge Functions         |
| `@sbf/domain`       | Lógica financeira pura (cálculos, regras)         | web, mobile, testes                 |
| `@sbf/ui-tokens`    | Tokens de design (cores, espaçamento, tipografia) | web, mobile                         |

**Regra:** componentes visuais **NÃO** são compartilhados entre plataformas. Apenas lógica, tipos, validação e tokens.

---

## 2.4 Convenção de Branches

### Debate

**Fernando Gomes (DevOps):**

> Para um projeto com time pequeno e MVP, trunk-based simplificado é o ideal. Duas branches protegidas: `main` (produção) e `develop` (staging). Feature branches curtas, merge rápido.

**Ana Silva (Arquiteta):**

> Concordo. GitFlow completo (release branches, hotfix branches) é overkill para a fase atual. Podemos evoluir para isso se necessário.

**Maria Oliveira (Backend):**

> Sugiro prefixos claros para facilitar filtragem no pipeline e legibilidade no histórico.

### Decisão: Trunk-Based Simplificado

**Branches protegidas:**

- `main` — produção. Deploy automático. Somente merge via MR aprovado.
- `develop` — staging. Deploy automático. Somente merge via MR aprovado.

**Branches de trabalho (efêmeras):**

- `feature/<descricao-curta>` — novas funcionalidades
- `fix/<descricao-curta>` — correções de bugs
- `chore/<descricao-curta>` — manutenção, dependências, configs
- `refactor/<descricao-curta>` — refatorações sem mudança de comportamento
- `docs/<descricao-curta>` — apenas documentação
- `test/<descricao-curta>` — apenas testes
- `migration/<descricao-curta>` — migrações de banco (trigger especial no CI)

**Regras:**

- Branches de trabalho **sempre** saem de `develop`
- Merge para `develop` via MR com pipeline verde
- Merge de `develop` para `main` via MR com aprovação explícita do CEO + pipeline verde
- Branches de trabalho devem ser deletadas após merge
- Nomes em **kebab-case**, em inglês: `feature/supplier-crud`, `fix/alias-uniqueness-trigger`

---

## 2.5 Convenção de Commits

### Debate

**João Pereira (Backend):**

> Conventional Commits é padrão de indústria. Além de legibilidade, permite automação: changelog, versionamento semântico, filtragem de CI.

**Thiago Martins (Front Engineer):**

> Proponho usar `commitlint` com `@commitlint/config-conventional` para enforcement automatizado via git hook + CI.

**Fernando Gomes (DevOps):**

> O hook local (`husky` + `commitlint`) pega na hora do commit. O CI valida novamente como safety net — não permite merge se commit message estiver fora do padrão.

### Decisão: Conventional Commits (enforcement obrigatório)

**Formato:**

```
<tipo>(<escopo>): <descrição>

[corpo opcional]

[footer opcional]
```

**Tipos permitidos:**

| Tipo        | Uso                                        |
| ----------- | ------------------------------------------ |
| `feat`      | Nova funcionalidade                        |
| `fix`       | Correção de bug                            |
| `docs`      | Documentação                               |
| `style`     | Formatação (sem mudança de lógica)         |
| `refactor`  | Refatoração (sem mudança de comportamento) |
| `test`      | Adição/correção de testes                  |
| `chore`     | Manutenção, dependências, configs          |
| `ci`        | Mudanças em CI/CD                          |
| `perf`      | Melhoria de performance                    |
| `migration` | Migração de banco de dados                 |

**Escopos sugeridos:**
`web`, `mobile`, `domain`, `types`, `validation`, `supabase`, `ci`, `docs`, `config`, `tokens`

**Exemplos:**

```
feat(web): add supplier CRUD page
fix(domain): correct SAC amortization calculation
migration(supabase): add suppliers and aliases tables
test(domain): add deduplication contract tests
chore(config): setup eslint and prettier
ci: add staging deploy pipeline
docs: add ADR-004 operational architecture
```

**Enforcement:**

- **Local:** `husky` pre-commit hook → `commitlint`
- **CI:** job `validate-commits` verifica todas as mensagens no MR
- **Breaking changes:** uso de `BREAKING CHANGE:` no footer ou `!` após tipo (ex: `feat(types)!: restructure transaction type`)

---

## 2.6 Política de Merge Requests e Proteção de Branches

### Debate

**Ana Silva (Arquiteta):**

> Precisamos de proteção rígida em `main` e `develop`, mas sem burocracia excessiva que mate a velocidade. Para MVP com time único, um reviewer é suficiente.

**Renata Silva (QA):**

> O pipeline **deve** ser green para merge — sem exceção. Se um test quebrou, a correção é obrigatória antes do merge, não depois.

**Fernando Gomes (DevOps):**

> Squash merge para `develop` mantém o histórico limpo. Merge commit de `develop` → `main` preserva contexto de release.

### Decisão: Política de MR

**Para `develop`:**

- Pipeline completo **DEVE** estar verde (lint, typecheck, testes, build)
- Mínimo 1 aprovação (qualquer membro sênior do time)
- Merge method: **squash merge** (histórico limpo)
- Título do MR deve seguir Conventional Commits
- Branch de origem deletada automaticamente após merge
- Auto-merge habilitado quando pipeline está verde + aprovação dada

**Para `main` (produção):**

- Pipeline completo **DEVE** estar verde
- Mínimo 1 aprovação do CEO ou Arquiteta
- Merge method: **merge commit** (preserva contexto de release)
- Tag automática de versão (semver baseado em commits)

**Proteção de branches (GitLab settings):**

- `main`: push direto **proibido**, force push **proibido**, merge somente via MR
- `develop`: push direto **proibido**, force push **proibido**, merge somente via MR
- Todas as outras branches: push livre, sem proteção

---

## 2.7 Estratégia de Ambientes

### Debate

**Fernando Gomes (DevOps):**

> Três ambientes é o mínimo viável: local, staging, production. Cada um com seu projeto Supabase isolado. Preview environments por MR seriam ideais, mas são complexos com Supabase — deixamos para depois.

**André Santos (DBA):**

> Cada ambiente precisa de seu próprio banco. Compartilhar banco entre ambientes é receita para desastre — uma migração errada em staging derrubaria produção.

**Ana Silva (Arquiteta):**

> O Supabase permite múltiplos projetos vinculados (linked projects). Podemos ter `sbf-local` (emulado via CLI), `sbf-staging` (projeto Supabase gratuito) e `sbf-production` (projeto Supabase com plano adequado).

### Decisão: 3 Ambientes Isolados

| Ambiente       | Supabase                               | Web                                  | Trigger                             |
| -------------- | -------------------------------------- | ------------------------------------ | ----------------------------------- |
| **local**      | `supabase start` (Docker local)        | `bun dev` (localhost:3000)           | Manual (desenvolvedor)              |
| **staging**    | Projeto Supabase dedicado (free tier)  | Vercel/Cloudflare preview ou similar | Auto-deploy no merge para `develop` |
| **production** | Projeto Supabase dedicado (plano pago) | Deploy produção                      | Auto-deploy no merge para `main`    |

**Variáveis de ambiente por contexto:**

```
# .env.example (template — SEM valores reais)
NEXT_PUBLIC_SUPABASE_URL=
NEXT_PUBLIC_SUPABASE_ANON_KEY=
SUPABASE_SERVICE_ROLE_KEY=        # Somente server-side / Edge Functions
SUPABASE_DB_PASSWORD=             # Somente CI para migrações
SUPABASE_PROJECT_ID=              # ID do projeto para CLI
SUPABASE_ACCESS_TOKEN=            # Token pessoal para CLI no CI
```

**Gestão de segredos:**

- **Local:** `.env.local` (gitignored)
- **CI/CD:** GitLab CI/CD Variables (masked, protected)
- **Produção:** Variáveis no Supabase Dashboard + GitLab protected variables
- **NUNCA** no repositório: API keys, tokens, senhas, service role keys

---

## 2.8 Supabase no CI/CD com GitLab

### Debate

**Fernando Gomes (DevOps):**

> O Supabase tem integração nativa GitHub — mas NÃO para GitLab. Precisamos fazer tudo via Supabase CLI. Isso significa: `supabase link`, `supabase db push`, `supabase functions deploy`, `supabase gen types` — tudo em jobs do GitLab CI.

**João Pereira (Backend):**

> O Supabase CLI requer Node.js ou pode ser instalado via npm. No CI, podemos usar a imagem `node:20-alpine` com `npx supabase` ou instalar globalmente.

**André Santos (DBA):**

> Para migrações, o fluxo é: desenvolvedor cria migração local → testa com `supabase start` → PR → CI aplica no staging → promove para produção. O CLI faz `supabase db push` que aplica migrações pendentes.

**Maria Oliveira (Backend):**

> O `SUPABASE_ACCESS_TOKEN` é um token pessoal gerado no dashboard. Para CI, usamos um token de serviço (ou da conta do projeto). Ele precisa ser `masked` e `protected` no GitLab.

### Decisão: Supabase CLI como Ferramenta de CI

**Autenticação no CI:**

```yaml
# Variáveis necessárias no GitLab CI/CD Variables:
# SUPABASE_ACCESS_TOKEN (masked, protected)
# SUPABASE_DB_PASSWORD (masked, protected)
# STAGING_SUPABASE_PROJECT_ID (protected para develop)
# PRODUCTION_SUPABASE_PROJECT_ID (protected para main)
```

**Comandos-chave no pipeline:**

```bash
# Linkar projeto
npx supabase link --project-ref $SUPABASE_PROJECT_ID

# Aplicar migrações pendentes
npx supabase db push

# Deploy Edge Functions
npx supabase functions deploy merge-suppliers
npx supabase functions deploy retroactive-supplier-association

# Gerar tipos TypeScript
npx supabase gen types typescript --project-id $SUPABASE_PROJECT_ID > packages/shared-types/src/database.types.ts
```

---

## 2.9 Estratégia de Migrações no Pipeline

### Debate

**André Santos (DBA):**

> Migrações são a operação mais sensível do pipeline. Uma migração errada em produção pode causar perda de dados. Proponho: (1) migrações sempre revisadas em MR, (2) aplicadas automaticamente em staging, (3) aplicadas em produção somente após validação em staging.

**Ana Silva (Arquiteta):**

> O `supabase db push` é idempotente — aplica apenas migrações que ainda não foram executadas, com base na tabela `supabase_migrations.schema_migrations`. Isso é seguro para CI.

**Fernando Gomes (DevOps):**

> Para rollback, o Supabase não tem migração "down" automática. A estratégia é: se uma migração der errado em staging, criamos uma nova migração corretiva. Em produção, o gate manual dá tempo para validar.

**Maria Oliveira (Backend):**

> Proponho um job específico `check-migrations` que valida: (1) se há migrações novas no MR, (2) se têm SQL válido (lint SQL), (3) se não contêm operações destrutivas sem flag explícita.

### Decisão: Fluxo de Migrações

```
Developer local          CI Staging              CI Production
─────────────          ──────────              ─────────────
1. supabase migration   3. supabase db push     5. supabase db push
   new <nome>              (auto no merge        (auto no merge
                            para develop)          para main)
2. Test local com       4. QA valida em         6. Monitorar logs
   supabase start          staging                  de produção
   + supabase db push
```

**Regras obrigatórias:**

- Migrações são **imutáveis** após merge — nunca editar migração já aplicada
- Correções geram **nova migração** (never alter history)
- Branches com prefixo `migration/` ativam job de validação extra no CI
- Migrações destrutivas (`DROP TABLE`, `DROP COLUMN`, `ALTER COLUMN TYPE`) requerem flag no commit: `BREAKING CHANGE: <motivo>`
- Seed data (`supabase/seed.sql`) é aplicada apenas em local e staging, **nunca** em produção

---

## 2.10 Deploy de Edge Functions por Ambiente

### Debate

**João Pereira (Backend):**

> Edge Functions rodam em Deno no Supabase. O deploy é por função: `supabase functions deploy <nome>`. No CI, podemos detectar quais funções mudaram e deployar apenas essas.

**Fernando Gomes (DevOps):**

> Para simplificar no MVP, deployamos todas as funções em cada merge. Quando tivermos muitas, otimizamos com detecção de mudança por diff de pasta.

**Maria Oliveira (Backend):**

> Cada ambiente (staging/produção) tem suas próprias env vars configuradas no dashboard do Supabase. As funções deployadas usam `Deno.env.get()` — sem secrets no código.

### Decisão: Deploy Completo por Ambiente

**Staging (merge para `develop`):**

```bash
npx supabase functions deploy --project-ref $STAGING_SUPABASE_PROJECT_ID
```

**Production (merge para `main`):**

```bash
npx supabase functions deploy --project-ref $PRODUCTION_SUPABASE_PROJECT_ID
```

**Secrets das funções:**

- Configurados via `supabase secrets set` no dashboard ou CLI
- **NUNCA** no repositório
- Cada ambiente tem seus próprios secrets

---

## 2.11 Geração e Validação de Tipos TypeScript

### Debate

**Thiago Martins (Front Engineer):**

> Os tipos gerados do Supabase são o contrato entre banco e código TypeScript. Se o schema muda (migração), os tipos devem ser regenerados. Proponho que o CI gere os tipos e compare com o que está commitado — se houver divergência, o pipeline falha.

**Ana Silva (Arquiteta):**

> Duas abordagens: (1) gerar no CI e commitar automaticamente, (2) gerar no CI e validar que o dev commitou a versão atualizada. Prefiro (2) — o dev deve ser responsável por atualizar tipos como parte do PR.

**João Pereira (Backend):**

> Concordo com (2). Se o CI gerar e commitar, temos commits automáticos poluindo o histórico. Se o CI apenas valida, o dev é forçado a rodar `bun run generate-types` localmente e commitar o resultado.

### Decisão: Geração Local + Validação no CI

**Fluxo:**

1. Dev cria migração e aplica localmente
2. Dev roda `bun run generate-types` (script que executa `supabase gen types typescript > packages/shared-types/src/database.types.ts`)
3. Dev commita o `database.types.ts` atualizado junto com a migração
4. CI regenera os tipos e compara via `diff` — se diferente do commitado, **falha o pipeline**

**Script local (`scripts/generate-types.sh`):**

```bash
#!/usr/bin/env bash
set -euo pipefail
npx supabase gen types typescript --local > packages/shared-types/src/database.types.ts
echo "✅ Tipos gerados em packages/shared-types/src/database.types.ts"
```

**Job CI (`validate-types`):**

```yaml
validate-types:
  stage: validate
  script:
    - npx supabase gen types typescript --project-id $SUPABASE_PROJECT_ID > /tmp/generated-types.ts
    - diff packages/shared-types/src/database.types.ts /tmp/generated-types.ts
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
      changes:
        - supabase/migrations/**
        - packages/shared-types/**
```

---

## 2.12 Design do Pipeline GitLab CI/CD

### Debate

**Fernando Gomes (DevOps):**

> Proponho 6 stages com execução condicional. O pipeline completo roda em MRs. Deploy roda apenas em merges para branches protegidas.

**Ana Silva (Arquiteta):**

> Cada stage deve ter timeout razoável. Se o pipeline total ultrapassar 15 minutos, precisamos otimizar. Para MVP, 10 minutos é o target.

**Renata Silva (QA):**

> Testes devem rodar em paralelo quando possível. Testes de domínio (unitários) são rápidos. Testes de integração (Supabase) podem ser mais lentos.

### Decisão: Pipeline em 6 Stages

```
┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────┐   ┌──────────────┐   ┌──────────────┐
│ validate │ → │ install  │ → │  check   │ → │  test    │ → │ build        │ → │ deploy       │
│          │   │          │   │          │   │          │   │              │   │              │
│ commits  │   │ bun      │   │ lint     │   │ unit     │   │ web          │   │ staging      │
│ types    │   │ install  │   │ typecheck│   │ integr.  │   │ (mobile TBD) │   │ production   │
│ (MR only)│   │          │   │ format   │   │ e2e      │   │              │   │ (main only)  │
└──────────┘   └──────────┘   └──────────┘   └──────────┘   └──────────────┘   └──────────────┘
```

**Pipeline YAML completo (`.gitlab-ci.yml`):**

```yaml
# ============================================================
# Seu Bolso Feliz — GitLab CI/CD Pipeline
# ============================================================

stages:
  - validate
  - install
  - check
  - test
  - build
  - deploy

# ── Cache global ──
default:
  cache:
    key:
      files:
        - bun.lock
    paths:
      - node_modules/
      - apps/web/node_modules/
      - apps/mobile/node_modules/
      - packages/*/node_modules/

# ── Variables ──
variables:
  BUN_VERSION: "1.1"
  NODE_VERSION: "20"

# ============================================================
# STAGE: validate (somente em MRs)
# ============================================================

validate-commits:
  stage: validate
  image: node:${NODE_VERSION}-alpine
  script:
    - npx commitlint --from $CI_MERGE_REQUEST_DIFF_BASE_SHA --to HEAD
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'

validate-types:
  stage: validate
  image: node:${NODE_VERSION}-alpine
  script:
    - npx supabase gen types typescript --project-id $STAGING_SUPABASE_PROJECT_ID > /tmp/generated.ts
    - diff packages/shared-types/src/database.types.ts /tmp/generated.ts
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
      changes:
        - supabase/migrations/**
        - packages/shared-types/**

# ============================================================
# STAGE: install
# ============================================================

install-deps:
  stage: install
  image: oven/bun:${BUN_VERSION}
  script:
    - bun install --frozen-lockfile
  artifacts:
    paths:
      - node_modules/
      - apps/*/node_modules/
      - packages/*/node_modules/
    expire_in: 1 hour

# ============================================================
# STAGE: check (lint + typecheck + format)
# ============================================================

lint:
  stage: check
  image: oven/bun:${BUN_VERSION}
  needs: [install-deps]
  script:
    - bun run lint

typecheck:
  stage: check
  image: oven/bun:${BUN_VERSION}
  needs: [install-deps]
  script:
    - bun run typecheck

format-check:
  stage: check
  image: oven/bun:${BUN_VERSION}
  needs: [install-deps]
  script:
    - bun run format:check

# ============================================================
# STAGE: test
# ============================================================

test-unit:
  stage: test
  image: oven/bun:${BUN_VERSION}
  needs: [install-deps]
  script:
    - bun run test:unit
  coverage: '/All files[^|]*\|[^|]*\s+([\d\.]+)/'

test-integration:
  stage: test
  image: oven/bun:${BUN_VERSION}
  needs: [install-deps]
  script:
    - bun run test:integration
  rules:
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
      changes:
        - supabase/**
        - packages/domain/**
        - packages/validation/**

# ============================================================
# STAGE: build
# ============================================================

build-web:
  stage: build
  image: oven/bun:${BUN_VERSION}
  needs: [lint, typecheck, test-unit]
  script:
    - cd apps/web && bun run build
  artifacts:
    paths:
      - apps/web/.next/
      - apps/web/out/
    expire_in: 1 day
  rules:
    - if: '$CI_COMMIT_BRANCH == "develop" || $CI_COMMIT_BRANCH == "main"'
    - if: '$CI_PIPELINE_SOURCE == "merge_request_event"'
      changes:
        - apps/web/**
        - packages/**

# ============================================================
# STAGE: deploy
# ============================================================

# ── Staging (develop) ──

deploy-migrations-staging:
  stage: deploy
  image: node:${NODE_VERSION}-alpine
  needs: [build-web]
  script:
    - npx supabase link --project-ref $STAGING_SUPABASE_PROJECT_ID
    - npx supabase db push
  environment:
    name: staging
  rules:
    - if: '$CI_COMMIT_BRANCH == "develop"'
      changes:
        - supabase/migrations/**

deploy-functions-staging:
  stage: deploy
  image: node:${NODE_VERSION}-alpine
  needs: [build-web]
  script:
    - npx supabase link --project-ref $STAGING_SUPABASE_PROJECT_ID
    - npx supabase functions deploy
  environment:
    name: staging
  rules:
    - if: '$CI_COMMIT_BRANCH == "develop"'
      changes:
        - supabase/functions/**

deploy-web-staging:
  stage: deploy
  image: oven/bun:${BUN_VERSION}
  needs: [build-web]
  script:
    - echo "Deploy web staging — configurar provedor (Vercel/Cloudflare)"
    # Comando real depende do provedor escolhido
  environment:
    name: staging
    url: https://staging.seubolsofeliz.com.br
  rules:
    - if: '$CI_COMMIT_BRANCH == "develop"'

# ── Production (main) ──

deploy-migrations-production:
  stage: deploy
  image: node:${NODE_VERSION}-alpine
  needs: [build-web]
  script:
    - npx supabase link --project-ref $PRODUCTION_SUPABASE_PROJECT_ID
    - npx supabase db push
  environment:
    name: production
  rules:
    - if: '$CI_COMMIT_BRANCH == "main"'
      changes:
        - supabase/migrations/**
  when: manual # Gate manual para produção

deploy-functions-production:
  stage: deploy
  image: node:${NODE_VERSION}-alpine
  needs: [deploy-migrations-production]
  script:
    - npx supabase link --project-ref $PRODUCTION_SUPABASE_PROJECT_ID
    - npx supabase functions deploy
  environment:
    name: production
  rules:
    - if: '$CI_COMMIT_BRANCH == "main"'
      changes:
        - supabase/functions/**

deploy-web-production:
  stage: deploy
  image: oven/bun:${BUN_VERSION}
  needs: [build-web]
  script:
    - echo "Deploy web production — configurar provedor"
  environment:
    name: production
    url: https://app.seubolsofeliz.com.br
  rules:
    - if: '$CI_COMMIT_BRANCH == "main"'
  when: manual # Gate manual para produção
```

### Sobre o Pipeline

**Fernando Gomes (DevOps):**

> Os jobs de deploy para produção têm `when: manual` — isso cria um gate no GitLab que requer clique explícito para prosseguir. Staging é automático. Essa é a safety net contra deploy acidental em produção.

**Ana Silva (Arquiteta):**

> O pipeline é conditional — jobs de build/deploy só rodam quando as pastas relevantes são alteradas. Um MR que só muda `docs/` não vai triggar build ou deploy.

**Renata Silva (QA):**

> Coverage tracking no `test-unit` via regex JUnit — permite acompanhar evolução da cobertura no GitLab MR.

---

## 2.13 Critérios de Qualidade para Merge

### Debate

**Renata Silva (QA):**

> Proponho critérios claros e mensuráveis. Sem ambiguidade sobre o que é "pipeline verde".

**Maria Oliveira (Backend):**

> Cobertura mínima é polêmico. Ao invés de % global, proponho cobertura mandatória nos módulos críticos (domain, validation) e opcional nos demais.

**Thiago Martins (Front Engineer):**

> Zero warnings de TypeScript é inegociável. Se o strict mode pega algo, é porque importa.

### Decisão: Critérios de Merge (Quality Gates)

**Obrigatórios (pipeline deve passar):**

1. ✅ Commits seguem Conventional Commits
2. ✅ Lint sem erros (`eslint --max-warnings 0`)
3. ✅ TypeScript sem erros (`tsc --noEmit`)
4. ✅ Formatação correta (`prettier --check`)
5. ✅ Testes unitários passando (100% green)
6. ✅ Build sem erros
7. ✅ Tipos Supabase atualizados (quando migração presente)

**Obrigatórios para módulos críticos:** 8. ✅ Cobertura em `packages/domain/` ≥ 90% 9. ✅ Cobertura em `packages/validation/` ≥ 90% 10. ✅ Testes de integração passando (quando módulos Supabase alterados)

**Recomendados (não bloqueantes no MVP):** 11. ⚠️ Cobertura global ≥ 70% (meta, não gate) 12. ⚠️ Nenhum `any` explícito em código novo (revisão humana) 13. ⚠️ Performance do bundle < 300kB gzip (meta)

---

## 2.14 Riscos e Armadilhas a Evitar

### Debate

Toda a equipe contribuiu com riscos mapeados:

**Ana Silva (Arquiteta):**

| Risco                                           | Mitigação                                                                         |
| ----------------------------------------------- | --------------------------------------------------------------------------------- |
| Over-engineering o pipeline antes de ter código | Começar com pipeline mínimo e expandir iterativamente                             |
| Monorepo com build lento                        | Filtro por pasta no CI, cache de dependências, considerar Turborepo se necessário |
| Lockfile conflicts em monorepo                  | `bun install --frozen-lockfile` no CI, um único `bun.lock` no root                |

**Fernando Gomes (DevOps):**

| Risco                             | Mitigação                                                                       |
| --------------------------------- | ------------------------------------------------------------------------------- |
| Supabase CLI quebrando em update  | Pinar versão no CI (`npx supabase@1.x.x`)                                       |
| Migrações aplicadas fora de ordem | Sempre gerar via `supabase migration new`, nunca criar SQL manual com timestamp |
| Secrets vazados em log do CI      | Variáveis `masked` no GitLab, nunca `echo` de variáveis sensíveis               |

**André Santos (DBA):**

| Risco                              | Mitigação                                                    |
| ---------------------------------- | ------------------------------------------------------------ |
| Migração destrutiva sem rollback   | Nova migração corretiva, nunca editar migração existente     |
| Drift entre schema local e staging | Job de validação de tipos no CI como detector de drift       |
| Seed data em produção              | Seed apenas em local/staging, bloqueio explícito em produção |

**Maria Oliveira (Backend):**

| Risco                                          | Mitigação                                                       |
| ---------------------------------------------- | --------------------------------------------------------------- |
| Edge Functions fora de sync entre ambientes    | Deploy de todas as funções no merge (sem deploy parcial no MVP) |
| Secrets diferentes entre staging e produção    | Checklist de secrets por ambiente na documentação de deploy     |
| Deno runtime incompatível com tipos TypeScript | Testar Edge Functions localmente com `supabase functions serve` |

**João Pereira (Backend):**

| Risco                                | Mitigação                                                  |
| ------------------------------------ | ---------------------------------------------------------- |
| Bun workspaces com incompatibilidade | Testar setup do workspace antes de migrar código existente |
| Dependência de Bun no CI             | Usar imagem oficial `oven/bun` e pinar versão              |

**Camila Duarte (Consultora):**

| Risco                                          | Mitigação                                                      |
| ---------------------------------------------- | -------------------------------------------------------------- |
| Processo pesado demais matando velocidade      | Revisão trimestral do processo — cortar o que não agrega       |
| Deploy manual "emergencial" bypassing pipeline | Documentar procedimento de emergência com rollback obrigatório |

---

## 2.15 Recomendação Final de Arquitetura Operacional

### Resumo Executivo

**Fernando Gomes (DevOps)** apresentou o resumo consolidado, aprovado pela equipe:

> O **Seu Bolso Feliz** adotará uma arquitetura de **monorepo com Bun workspaces**, hospedado no **GitLab**, com pipeline CI/CD em 6 stages (validate → install → check → test → build → deploy). O backend é **serverless-first via Supabase**, com migrações gerenciadas by CLI e deploy de Edge Functions automatizado. A estratégia de branches é **trunk-based simplificada** (`main` + `develop` + branches efêmeras), com Conventional Commits enforced via commitlint. Três ambientes isolados (local, staging, produção) com Supabase projects dedicados. Quality gates rigorosos mas não burocráticos: lint, typecheck, testes, build — todos obrigatórios. Deploy staging automático, produção com gate manual.

### Diagrama de Fluxo

```
Developer                     GitLab                        Supabase
─────────                     ──────                        ────────

1. Branch feature/xxx   ───→  2. Pipeline MR:
   de develop                    ├─ validate (commits, types)
                                 ├─ install (bun)
3. Push commits         ───→     ├─ check (lint, ts, format)
                                 ├─ test (unit, integration)
4. MR para develop      ───→     └─ build (web)
                                      ↓
5. Aprovação + Merge    ───→  6. Pipeline develop:
                                 ├─ check + test + build
                                 ├─ deploy migrations  ───→  7. Staging DB ✅
                                 ├─ deploy functions   ───→  8. Staging Functions ✅
                                 └─ deploy web         ───→  9. Staging Web ✅

10. Validação staging   ───→  11. MR develop → main
                                      ↓
12. Aprovação CEO       ───→  13. Pipeline main:
                                 ├─ check + test + build
                                 ├─ deploy migrations  ───→  14. Prod DB ✅ (manual)
                                 ├─ deploy functions   ───→  15. Prod Functions ✅
                                 └─ deploy web         ───→  16. Prod Web ✅ (manual)
```

---

# 3. Análise de Impacto nos Documentos Existentes

A equipe analisou todos os documentos existentes para mapear impactos da nova arquitetura operacional.

## 3.1 Impacto no Guia de Implementação (`docs/planejamento/001-guia-implementacao-passo-a-passo.md`)

**Thiago Martins (Front Engineer):**

> O guia atual referencia caminhos flat (`src/__tests__/`, `src/components/supplier/`). Com o monorepo, os caminhos mudam para: testes de domínio em `__tests__/domain/` ou `packages/domain/src/__tests__/`, componentes web em `apps/web/src/components/`, etc.

**Impactos identificados:**

| Seção                    | Impacto                                                                           | Ação                           |
| ------------------------ | --------------------------------------------------------------------------------- | ------------------------------ |
| Etapa 1 (Migrações)      | Caminho `supabase/migrations/` **não muda** (já está correto)                     | Nenhuma                        |
| Etapa 2 (Testes)         | `src/__tests__/domain/` → `__tests__/domain/` ou `packages/domain/src/__tests__/` | Atualizar caminhos             |
| Etapa 3 (CRUD)           | `src/components/supplier/` → `apps/web/src/components/supplier/`                  | Atualizar caminhos             |
| Etapa 5 (Edge Functions) | `supabase/functions/` **não muda**                                                | Nenhuma                        |
| Comandos `npx supabase`  | Manter — funciona com monorepo                                                    | Nenhuma                        |
| Framework de testes      | Referência a Vitest — precisa ser instalado                                       | Adicionar no setup do monorepo |

**Decisão:** Atualizar o guia com caminhos monorepo e adicionar seção "Pré-requisito: Setup do Monorepo" no início.

## 3.2 Impacto nos ADRs (001, 002, 003)

**Ana Silva (Arquiteta):**

> Os ADRs definem decisões de domínio e modelagem — são independentes de estrutura de repositório. Os caminhos de migração SQL (`supabase/migrations/`) já estão corretos. Impacto: mínimo.

| ADR                    | Impacto                                                            | Ação    |
| ---------------------- | ------------------------------------------------------------------ | ------- |
| ADR-001 (Deduplicação) | View `v_expenses_deduplicated` — migração OK                       | Nenhuma |
| ADR-002 (Métricas)     | CHECK constraint — migração OK                                     | Nenhuma |
| ADR-003 (Aliases)      | Edge Functions referenciadas — caminhos OK (`supabase/functions/`) | Nenhuma |

**Decisão:** ADRs não precisam de atualização. A nova ADR-004 referenciará as anteriores para rastreabilidade.

## 3.3 Impacto nos Testes Mandatórios

**Renata Silva (QA):**

> Os 27+ testes mandatórios do guia continuam válidos. O que muda é: (1) framework agora é Vitest (confirmar), (2) caminhos dos arquivos de teste, (3) integração com CI pipeline.

**Maria Oliveira (Backend):**

> Os testes de domínio/unitários ficam em `packages/domain/src/__tests__/` ou `__tests__/domain/`. Os testes de integração (que precisam de Supabase) ficam em `__tests__/integration/`. Essa separação permite rodar unitários rápido e integrações condicionalmente.

| Tipo de Teste         | Localização Monorepo                                    | Job CI             |
| --------------------- | ------------------------------------------------------- | ------------------ |
| Domínio / Unitário    | `__tests__/domain/` ou `packages/domain/src/__tests__/` | `test-unit`        |
| Integração (Supabase) | `__tests__/integration/`                                | `test-integration` |
| E2E (Playwright)      | `__tests__/e2e/`                                        | futuro             |
| Componente (web)      | `apps/web/src/__tests__/`                               | `test-unit`        |

## 3.4 Impacto no README de Planejamento (`docs/planejamento/README.md`)

**Decisão:** Adicionar links para:

- ADR-004 (Arquitetura Operacional)
- Guia CI/CD e Engenharia
- Esta ata de refino

---

# 4. Ferramentas e Dependências a Instalar

**João Pereira (Backend)** consolidou a lista de ferramentas necessárias para o setup:

## 4.1 Dependências de Desenvolvimento (root)

```json
{
  "devDependencies": {
    "vitest": "^3.x",
    "@vitest/coverage-v8": "^3.x",
    "eslint": "^9.x",
    "@eslint/js": "^9.x",
    "typescript-eslint": "^8.x",
    "prettier": "^3.x",
    "eslint-config-prettier": "^10.x",
    "husky": "^9.x",
    "lint-staged": "^15.x",
    "@commitlint/cli": "^19.x",
    "@commitlint/config-conventional": "^19.x",
    "supabase": "^1.x"
  }
}
```

## 4.2 Dependências por Workspace

| Workspace               | Dependências-chave                                                                              |
| ----------------------- | ----------------------------------------------------------------------------------------------- |
| `apps/web`              | `next`, `react`, `react-dom`, `@supabase/supabase-js`, `@supabase/ssr`, `tailwindcss`, `@sbf/*` |
| `apps/mobile`           | `expo`, `react-native`, `@supabase/supabase-js`, `@sbf/*`                                       |
| `packages/shared-types` | (sem deps externas — somente tipos)                                                             |
| `packages/validation`   | `zod`                                                                                           |
| `packages/domain`       | (sem deps externas — lógica pura)                                                               |
| `packages/ui-tokens`    | (sem deps externas — constantes)                                                                |
| `packages/config`       | configs compartilhadas de eslint, tsconfig, vitest                                              |

## 4.3 Scripts Root (`package.json`)

```json
{
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
    "db:seed": "npx supabase db reset --seed-only",
    "prepare": "husky"
  }
}
```

---

# 5. Sequência de Implementação do Setup

**Fernando Gomes (DevOps)** propôs a sequência para montar a infraestrutura, aprovada pela equipe:

### Etapa 0 — Setup do Monorepo (PRÉ-REQUISITO para todo o resto)

```
0.1  Criar estrutura de pastas (apps/, packages/, supabase/, scripts/, __tests__/)
0.2  Configurar Bun workspaces no package.json root
0.3  Migrar código web existente para apps/web/
0.4  Criar pacotes iniciais (shared-types, validation, domain, ui-tokens, config)
0.5  Configurar tsconfig.base.json + extends por workspace
0.6  Instalar e configurar ESLint 9 + Prettier
0.7  Instalar e configurar Vitest
0.8  Instalar e configurar Husky + lint-staged + commitlint
0.9  Inicializar Supabase (supabase init)
0.10 Criar .env.example e .gitignore atualizado
0.11 Criar .gitlab-ci.yml inicial
0.12 Commitar como: chore: setup monorepo with Bun workspaces and CI/CD
```

**Ana Silva (Arquiteta):**

> Essa Etapa 0 precede a Etapa 1 do guia de implementação. É a fundação sobre a qual tudo será construído. Proponho tratá-la como **Sprint 0** — setup, sem features.

### Relação com o Guia de Implementação Existente

```
Sprint 0  → Etapa 0 (Setup Monorepo + CI/CD)     ← NOVO
Sprint 1  → Etapa 1 (Base Estrutural — Migrações)
Sprint 2  → Etapa 2 (Contrato Comportamental — Testes)
Sprint 3  → Etapa 3 (Núcleo Funcional — CRUD)
Sprint 4  → Etapa 4 (Relatórios e Filtros)
Sprint 5  → Etapa 5 (Recursos Avançados)
```

---

# 6. Decisão Final Consolidada

O CEO (Chico), após ouvir toda a equipe, aprovou as seguintes decisões:

| #   | Tópico           | Decisão                                                                             |
| --- | ---------------- | ----------------------------------------------------------------------------------- |
| 1   | Repositório      | Monorepo com Bun workspaces                                                         |
| 2   | Estrutura        | `apps/` + `packages/` + `supabase/` + `docs/` + `scripts/` + `__tests__/`           |
| 3   | Compartilhamento | 4 pacotes: shared-types, validation, domain, ui-tokens                              |
| 4   | Branches         | Trunk-based: `main` (prod) + `develop` (staging) + branches efêmeras                |
| 5   | Commits          | Conventional Commits com commitlint                                                 |
| 6   | MR Policy        | Pipeline verde + 1 aprovação (develop), CEO/Arquiteta (main), squash merge          |
| 7   | Ambientes        | 3: local (supabase start), staging (projeto free), production (projeto pago)        |
| 8   | Supabase CI      | CLI via `npx supabase` com token de acesso em variáveis GitLab                      |
| 9   | Migrações        | Criação local → CI staging auto → CI produção manual                                |
| 10  | Edge Functions   | Deploy completo por ambiente no merge                                               |
| 11  | Tipos TS         | Geração local + validação diff no CI                                                |
| 12  | Pipeline         | 6 stages: validate → install → check → test → build → deploy                        |
| 13  | Quality Gates    | Lint + TS + format + tests + build obrigatórios; cobertura 90% em domain/validation |
| 14  | Riscos           | 15 riscos mapeados com mitigação                                                    |
| 15  | Implementação    | Etapa 0 (Sprint 0) precede todas as etapas do guia existente                        |

---

# 7. Ações / Responsáveis / Prazo

| #   | Ação                                                            | Responsável                   | Prazo      |
| --- | --------------------------------------------------------------- | ----------------------------- | ---------- |
| 1   | Criar ADR-004 (Arquitetura Operacional)                         | Ana Silva + Fernando Gomes    | 2026-03-21 |
| 2   | Criar guia de CI/CD e engenharia (`docs/planejamento/`)         | Fernando Gomes                | 2026-03-21 |
| 3   | Atualizar guia de implementação com Etapa 0 e caminhos monorepo | Ana Silva + Thiago Martins    | 2026-03-22 |
| 4   | Atualizar README de planejamento                                | Thiago Martins                | 2026-03-21 |
| 5   | Executar Etapa 0 (Sprint 0 — setup monorepo)                    | João Pereira + Fernando Gomes | 2026-03-23 |
| 6   | Configurar projetos Supabase (staging + produção)               | André Santos + Fernando Gomes | 2026-03-23 |
| 7   | Configurar variáveis CI/CD no GitLab                            | Fernando Gomes                | 2026-03-23 |
| 8   | Validar pipeline com MR de teste                                | Toda a equipe                 | 2026-03-24 |
