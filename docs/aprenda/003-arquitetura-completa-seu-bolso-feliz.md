# Guia Prático: Arquitetura Completa do Seu Bolso Feliz

> Este documento explica, de forma acessível, como todas as peças do **Seu Bolso Feliz** se conectam — desde o banco de dados até a interface web, passando por autenticação, ingestão de documentos e o servidor MCP. Ideal para novos membros da equipe ou para o CEO entender o "mapa geral" do sistema.

---

## Índice

1. [Visão Geral — O Mapa do Sistema](#1-visao-geral)
2. [Monorepo — Como o Código é Organizado](#2-monorepo)
3. [Banco de Dados — 36 Tabelas que Modelam sua Vida Financeira](#3-banco)
4. [Autenticação — Email + Google OAuth](#4-auth)
5. [Row Level Security — Seus Dados são SÓ Seus](#5-rls)
6. [Domínio Financeiro — As Regras de Negócio](#6-dominio)
7. [Pipeline de Ingestão — Do E-mail ao Rascunho](#7-ingestao)
8. [Web Frontend — 26 Telas que Funcionam](#8-web)
9. [Design System — Tokens e Componentes](#9-design)
10. [CI/CD — Pipeline de Qualidade e Deploy](#10-cicd)
11. [MCP Server — Ferramentas para Agentes de IA](#11-mcp)
12. [Ambientes — Local, Staging e Produção](#12-ambientes)

---

## 1. Visão Geral — O Mapa do Sistema {#1-visao-geral}

### Diagrama simplificado

```
┌──────────────┐    ┌──────────────┐    ┌──────────────┐
│   Browser    │    │  Mobile App  │    │  Agentes IA  │
│  (Next.js)   │    │ (React Nat.) │    │  (via MCP)   │
└──────┬───────┘    └──────┬───────┘    └──────┬───────┘
       │                   │                   │
       └───────────┬───────┘                   │
                   │                           │
            ┌──────▼───────┐           ┌───────▼──────┐
            │   Supabase   │           │  MCP Server  │
            │  (Backend)   │◄──────────│  (8 tools)   │
            └──────┬───────┘           └──────────────┘
                   │
    ┌──────────────┼──────────────┐
    │              │              │
┌───▼───┐   ┌─────▼─────┐  ┌────▼────┐
│Postgres│   │  Storage  │  │  Edge   │
│(Dados) │   │ (Arquivos)│  │Functions│
└────────┘   └───────────┘  └─────────┘
                   ▲
                   │
         ┌─────────┴─────────┐
         │    Workers        │
         │ (Gmail Scanner,   │
         │  Local Scanner,   │
         │  Ingestion)       │
         └───────────────────┘
```

### Como os dados fluem

1. **CEO registra algo no browser** → Next.js faz request ao Supabase → Postgres armazena
2. **Worker escaneia Gmail** → baixa anexo → upload pro Storage → cria job de ingestão
3. **Pipeline de ingestão** → pega o job → faz parsing → gera rascunho → CEO aprova → vira transação real
4. **Agente de IA** → chama ferramenta MCP → MCP consulta Supabase → retorna dados estruturados

---

## 2. Monorepo — Como o Código é Organizado {#2-monorepo}

O projeto usa **Bun workspaces** — todo o código está num único repositório organizado em pastas.

### Estrutura principal

```
seu.bolso.feliz/
├── apps/
│   ├── web/          → Frontend web (Next.js + React + Tailwind)
│   ├── mobile/       → App mobile (React Native + Expo) — em desenvolvimento
│   └── mcp-server/   → Servidor MCP para agentes de IA
│
├── packages/
│   ├── shared-types/  → Tipos TypeScript gerados do banco de dados
│   ├── validation/    → Esquemas de validação (Zod) para todas as entidades
│   ├── domain/        → Lógica de negócio (ciclos, amortização, prioridade, deduplicação)
│   ├── ui-tokens/     → Tokens visuais (cores, fontes, espaçamentos)
│   ├── config/        → Configurações compartilhadas (ESLint, TypeScript, Vitest)
│   └── operations/    → Serviço de ingestão (hash, idempotência, fingerprints)
│
├── workers/
│   ├── ingestion/     → Pipeline de processamento de documentos
│   ├── local-scanner/ → Escaneia pasta local para importação
│   └── gmail-scanner/ → Escaneia Gmail por label (em desenvolvimento)
│
├── supabase/
│   ├── migrations/    → 17 migrações SQL (schema do banco)
│   ├── functions/     → 3 Edge Functions (merge, refresh, associação)
│   ├── config.toml    → Configuração do Supabase (auth, storage, etc.)
│   └── seed.sql       → Dados iniciais (categorias, tags)
│
├── __tests__/         → 168+ testes (domínio, integração, e2e)
└── docs/              → Documentação completa do projeto
```

### Por que monorepo?

- **Uma fonte de verdade**: tipos, validações e lógica de domínio são compartilhados entre web, mobile e workers
- **Atualização atômica**: uma mudança no tipo "Transaction" é refletida em TODOS os consumidores automaticamente
- **Build otimizado**: Bun resolve dependências internas sem publicar pacotes NPM

---

## 3. Banco de Dados — 36 Tabelas que Modelam sua Vida Financeira {#3-banco}

### Hierarquia principal

```
Instituição (ex: Nubank, Caixa, C6)
  └── Produto Financeiro (ex: Conta Corrente, Poupança, Cartão)
        └── Conta / Contrato
              └── Transações (receitas, despesas, transferências)
              └── Recorrências (Internet, Academia, Parcela)
              └── Faturas (ciclo de cartão)
              └── Dívidas (empréstimo, financiamento)
```

### Tabelas agrupadas por função

| Grupo             | Tabelas                                                                                                                                                             | Função                                            |
| ----------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| **Estrutural**    | institutions, financial_products, accounts, cards, card_holders                                                                                                     | Banco → Produto → Conta → Cartão                  |
| **Transacional**  | transactions, transaction_tags                                                                                                                                      | Lançamentos do dia-a-dia                          |
| **Recorrências**  | recurring_templates, recurring_instances, recurring_template_tags                                                                                                   | Contas que se repetem todo mês                    |
| **Faturas**       | statement_cycles, statement_items                                                                                                                                   | Ciclo de fechamento e itens de fatura             |
| **Dívidas**       | liabilities, liability_installments                                                                                                                                 | Empréstimos, financiamento, composição da parcela |
| **Fornecedores**  | suppliers, supplier_aliases, supplier_contracts, supplier_tags                                                                                                      | Quem você paga (CEMIG, Nubank, etc.)              |
| **Métricas**      | consumption_metrics                                                                                                                                                 | kWh de energia, litros de água, etc.              |
| **Tempo**         | financial_periods, user_financial_preferences                                                                                                                       | Ciclo financeiro personalizado                    |
| **Classificação** | categories, tags                                                                                                                                                    | Categoria + múltiplas tags por item               |
| **Documentos**    | documents                                                                                                                                                           | Comprovantes, recibos, contratos                  |
| **Ingestão**      | ingestion_runs, ingestion_jobs, source_documents, document_fingerprints, parsed_document_versions, extraction_results, draft_records, draft_batches, ingestion_logs | Pipeline de importação automática                 |
| **Segurança**     | user_secrets, audit_logs                                                                                                                                            | Segredos criptografados + trilha de auditoria     |
| **Relatórios**    | mv_supplier_spending (view materializada)                                                                                                                           | Cache de consultas frequentes                     |

### O que torna único

- **17 migrações imutáveis** — nunca editamos uma migration depois de aplicada
- **BCNF normalizado** — sem redundância ou anomalias
- **Restrições de domínio** — CHECK constraints garantem que amortização ≥ 0, juros ≥ 0, etc.
- **Índices otimizados** — 28 índices para queries recorrentes, incluindo trigram para busca fuzzy de fornecedores

---

## 4. Autenticação — Email + Google OAuth {#4-auth}

### Como funciona

O Supabase Auth gerencia tudo — não precisamos de servidor de autenticação próprio.

```
Usuário → Clica "Entrar com Google" → Google pede permissão →
  → Supabase recebe token → Cria sessão → Redirect ao dashboard
```

### Dois métodos suportados

1. **Email + Senha**: cadastro tradicional, senha mínima 8 chars com upper+lower+digit
2. **Google OAuth**: login com conta Google (o CEO usa este)

### PKCE (Proof Key for Code Exchange)

O fluxo OAuth usa PKCE — um protocolo que protege contra ataques de interceptação de token. Na prática:

1. O browser gera um "code verifier" secreto
2. Envia um hash dele para o Google
3. Quando o Google retorna o código, o browser prova que é quem iniciou o fluxo

Isso é transparente para o usuário — acontece automaticamente via Supabase Auth.

---

## 5. Row Level Security — Seus Dados são SÓ Seus {#5-rls}

### O que é RLS

Row Level Security é uma feature do PostgreSQL que restringe quais linhas cada usuário pode ver/editar. Funciona no nível do banco — mesmo que alguém consiga acessar a API diretamente, não vê dados de outro usuário.

### Regra universal

```sql
-- Cada tabela tem uma política como esta:
CREATE POLICY "Users see own data" ON transactions
  FOR ALL USING (user_id = auth.uid());
```

Resultado: **27 tabelas** protegidas. Não existe cenário onde User A vê dados de User B.

### Caso especial: audit_logs

Logs de auditoria são imutáveis — política permite SELECT e INSERT, mas **não** UPDATE ou DELETE. Uma vez registrado, ninguém apaga.

---

## 6. Domínio Financeiro — As Regras de Negócio {#6-dominio}

O pacote `@sbf/domain` contém lógica financeira pura — sem dependência de banco ou framework.

### 6.1 Ciclos Financeiros Personalizados

O sistema **não assume mês civil**. Cada usuário configura seu próprio ciclo:

- Exemplo: Ciclo de 20/03 a 19/04 → "quanto dinheiro precisa durar esse período?"
- Afeta: dashboards, orçamento, fluxo de caixa, alertas

```typescript
// Exemplo simplificado
const periodo = resolverPeriodoFinanceiro(transacao.data, usuarioConfig);
// Input: 2026-03-25, config: { inicio: 20 }
// Output: { inicio: "2026-03-20", fim: "2026-04-19" }
```

### 6.2 Amortização (SAC, Price, Misto)

Empréstimos e financiamentos decompõem cada parcela em:

- **Amortização**: quanto reduz a dívida
- **Juros**: custo do dinheiro
- **Seguros e taxas**: encargos adicionais

3 sistemas implementados:

- **SAC**: amortização constante, parcelas decrescentes
- **Price**: parcela constante, amortização crescente
- **Misto**: média entre SAC e Price

Inclui simulação de quitação antecipada com recálculo correto.

### 6.3 Deduplicação

Regra crítica: **pagamento de fatura NÃO é nova despesa**.

O sistema usa uma UNION view (`v_expenses_deduplicated`) que:

1. Pega todas as transações marcadas como despesa
2. Exclui as que são pagamento de fatura (origin_type = 'statement_link')
3. Resultado: saldo correto sem dupla contagem

### 6.4 Priorização

Cada item tem prioridade: `essential > high > medium > low > optional`.

A prioridade pode vir de:

- Marcação manual
- Derivação por tags (tag `essencial` → prioridade essential)
- Tipo de obrigação (moradia = essencial)

Isso afeta a primeira tela: "o que pagar primeiro?".

### 6.5 Testes: 168+

| Módulo             | Testes | O que cobre                                                 |
| ------------------ | ------ | ----------------------------------------------------------- |
| Amortização        | 33     | SAC, Price, Misto, quitação antecipada, cenário de hipoteca |
| Deduplicação       | 14     | Exclusão de pagamento de fatura, transferências internas    |
| Ciclos Financeiros | 17     | Resolução de período, geração, cenários de virada           |
| Priorização        | 19     | Scoring, herança de tags, ordenação                         |
| Validação          | 34     | Todos os schemas Zod para CRUD                              |
| Fornecedores       | 24     | Aliases, contratos, métricas                                |
| Ingestão           | 100+   | Hash, idempotência, parsers, state machine, drafts          |
| Integração         | 16     | Fluxos cross-module                                         |
| E2E                | 10     | Jornadas completas do usuário                               |

---

## 7. Pipeline de Ingestão — Do E-mail ao Rascunho {#7-ingestao}

### Fluxo completo

```
📧 Gmail (ou 📁 pasta local)
  ↓
🔍 Scanner (gmail-scanner ou local-scanner)
  ↓ [descobre documento, faz upload ao Storage]
📋 Job criado (status: DISCOVERED)
  ↓
⚙️ Pipeline de Ingestão (workers/ingestion)
  ↓ [parsing → classificação → extração]
📝 Rascunho gerado (draft_records)
  ↓
👤 CEO revisa e aprova
  ↓
✅ Transação real criada (transactions)
```

### Parsers disponíveis

- **CEMIG parser**: regex para contas de energia (kWh, valores, período)
- **Boleto parser**: extrai código de barras, beneficiário, valor, vencimento
- **PDF genérico**: extração de texto via pdf-parse
- **CSV**: para importação de planilhas

### Máquina de estados

Cada job de ingestão passa por estados bem definidos:

```
DISCOVERED → QUEUED → DOWNLOADING → PARSING → PARSED →
  CLASSIFYING → CLASSIFIED → DRAFTING → DRAFTED →
    REVIEWING → APPROVED/REJECTED → POSTED/DISCARDED
```

Transições inválidas são rejeitadas (ex: não pode ir de DISCOVERED direto para POSTED).

### Idempotência

Documentos são "fingerprinted" por SHA-256. Se o mesmo PDF aparecer duas vezes, o sistema detecta e não reprocessa.

---

## 8. Web Frontend — 26 Telas que Funcionam {#8-web}

### Stack: Next.js 15 + React + Tailwind + shadcn/ui

| Rota                      | Função                                                          |
| ------------------------- | --------------------------------------------------------------- |
| `/login`                  | Login (email + Google OAuth)                                    |
| `/dashboard`              | Tela principal — o que pagar primeiro, vencimentos, prioridades |
| `/dashboard/institutions` | CRUD de bancos                                                  |
| `/dashboard/products`     | CRUD de produtos financeiros                                    |
| `/dashboard/transactions` | CRUD de transações + filtros                                    |
| `/dashboard/recurring`    | Templates e instâncias recorrentes                              |
| `/dashboard/statements`   | Faturas de cartão + itens                                       |
| `/dashboard/liabilities`  | Dívidas + cronograma de amortização                             |
| `/dashboard/suppliers`    | Fornecedores + aliases + contratos                              |
| `/dashboard/reports`      | Relatórios (mês civil, período personalizado, por fornecedor)   |
| `/dashboard/documents`    | Upload e gerenciamento de anexos                                |
| `/dashboard/import`       | Importação de CSV/planilha                                      |
| `/dashboard/settings`     | Configuração de ciclo financeiro, categorias e tags             |

### Componentes visuais: 31 componentes shadcn/ui

Button, Card, Dialog, Input, Select, Table, Tabs, Badge, Skeleton, Sidebar, etc. — todos responsivos e acessíveis.

---

## 9. Design System — Tokens e Componentes {#9-design}

### Conceito

Tokens visuais são **variáveis de design** (cores, fontes, espaçamentos) definidas uma vez e usadas em todo o sistema. Garantem consistência visual entre web e mobile.

| Token             | Web (Tailwind)       | Mobile (React Native) |
| ----------------- | -------------------- | --------------------- |
| Cor primária      | `bg-primary`         | `colors.primary`      |
| Espaçamento médio | `p-4` (16px)         | `spacing.md`          |
| Fonte heading     | `text-2xl font-bold` | `typography.heading`  |

### Estado atual

O pacote `@sbf/ui-tokens` é stub — os tokens estão definidos como placeholders. A materialização está planejada para Sprint 2 (Isabella + Thiago).

### shadcn/ui

Biblioteca de componentes para React que:

- Copia o código fonte para dentro do projeto (não é dependência NPM)
- Usa Tailwind para estilização
- Acessível por padrão (ARIA, teclado, foco)
- Personalizável sem limitações

---

## 10. CI/CD — Pipeline de Qualidade e Deploy {#10-cicd}

### Pipeline GitLab (6 estágios)

```
1. VALIDATE → commitlint, secrets scan
2. INSTALL  → bun install, cachear
3. CHECK    → lint, format, typecheck
4. TEST     → unit + integration, cobertura
5. BUILD    → bun run build (Next.js + packages)
6. DEPLOY   → staging (automático) / produção (manual)
```

### Ambientes

| Ambiente | URL                       | Supabase             | Deploy                |
| -------- | ------------------------- | -------------------- | --------------------- |
| Local    | localhost:3105            | localhost:54321      | Manual                |
| Staging  | preview.vercel (em breve) | dcljzgjgnkmxdvhybvpt | Automático no develop |
| Produção | seubolsofeliz.com.br      | opwelsgdhksuuewdbefk | Promoção manual main  |

### Branch strategy

- `feat/001-*` → feature branches → MR para `develop`
- `develop` → staging automático
- `main` → produção (promoção controlada, backup antes)

---

## 11. MCP Server — Ferramentas para Agentes de IA {#11-mcp}

### O que é MCP

Model Context Protocol — padrão aberto para agentes de IA acessarem ferramentas externas. O Seu Bolso Feliz expõe 8 ferramentas via MCP:

| Ferramenta                        | O que faz                            |
| --------------------------------- | ------------------------------------ |
| `scan_local_folder`               | Escaneia pasta local para importação |
| `list_draft_batches`              | Lista batches de rascunhos pendentes |
| `approve_draft_batch`             | Aprova um batch de rascunhos         |
| `list_unparsed_documents`         | Lista documentos não processados     |
| `reprocess_document`              | Reprocessa documento com problemas   |
| `resolve_supplier_candidates`     | Sugere fornecedor para transação     |
| `find_documents_without_password` | Lista PDFs protegidos sem senha      |
| `recompute_financial_periods`     | Recalcula períodos financeiros       |

### Para que serve

Um agente de IA (ex: ChatGPT, Claude) pode usar essas ferramentas para ajudar o CEO:

- "Processe os documentos novos da pasta Downloads"
- "Aprove todos os rascunhos do fornecedor CEMIG"
- "Quais PDFs estão travados por senha?"

---

## 12. Ambientes — Local, Staging e Produção {#12-ambientes}

### Local (desenvolvimento)

```bash
supabase start     # Postgres + Auth + Storage local
bun run dev        # Next.js na porta 3105
```

- Banco: localhost:54321
- Auth: email + Google OAuth (configurado em config.toml)
- Storage: buckets locais (documents, imports, ingestion)

### Staging (testes com dados reais)

- Supabase: `dcljzgjgnkmxdvhybvpt.supabase.co`
- Web: deploy via Vercel preview (em implementação)
- Migrações: 17/17 aplicadas
- Edge Functions: 3/3 deployadas
- Google Auth: habilitado

### Produção (usuários reais)

- Supabase: `opwelsgdhksuuewdbefk.supabase.co`
- Web: seubolsofeliz.com.br (Vercel)
- Status: **standby** — migrações não aplicadas, aguardando validação staging
- Regra: só entra em produção após André validar cada migration com EXPLAIN/ANALYZE

---

## Referências

- Arquitetura: `docs/adrs/ADR-004-arquitetura-operacional-repositorio-cicd.md`
- Modelagem de domínio: `docs/Veronica/001-prompt.inicial.md`
- Fornecedores: `docs/Veronica/002-fornecedor.md`
- Tecnologias de ingestão: `docs/aprenda/002-tecnologias-ciclo-ingestao-automacao-mcp.md`
- Pipeline CI/CD: `docs/planejamento/002-guia-cicd-engenharia-operacional.md`
