# 001 — Checklist Completo dos Pedidos da Verônica

Objetivo: consolidar, em formato auditável, tudo o que foi pedido pela Verônica e o status real de implantação no codebase.

Legendas:

- [x] Concluído
- [ ] Pendente
- [ ] Parcial (descrito no campo Observações)

## Fase A — Fundação de Arquitetura e Domínio (001-prompt.inicial)

### A1. Stack e arquitetura-base

- [x] Web com React + Next.js + Tailwind
  - Critério de aceite: apps/web com build funcional e rotas do App Router
  - Evidência: apps/web/package.json, apps/web/src/app/\*, bun run build OK
- [x] Mobile em estrutura React Native/Expo preparada
  - Critério de aceite: apps/mobile estruturado no monorepo
  - Evidência: apps/mobile/package.json, apps/mobile/src
- [x] Backend central em Supabase (Postgres/Auth/RLS/Storage/Functions)
  - Critério de aceite: supabase/config.toml + migrations + uso de client no web
  - Evidência: supabase/config.toml, supabase/migrations/_, apps/web/src/lib/supabase/_
- [x] Estratégia serverless-first com baixo acoplamento operacional
  - Critério de aceite: operações centrais no Supabase e pipeline CI/CD com CLI
  - Evidência: .gitlab-ci.yml, docs/adrs/ADR-004-arquitetura-operacional-repositorio-cicd.md

### A2. Princípios de modelagem financeira

- [x] Separação despesa vs transferência interna
  - Critério de aceite: regra explícita de deduplicação e tipos de evento
  - Evidência: packages/domain/src/deduplication/index.ts, docs/adrs/ADR-001-deduplicacao-transacao-item-fatura.md
- [x] Separação de composição da dívida (amortização/juros/encargos)
  - Critério de aceite: modelagem + cálculos e testes
  - Evidência: supabase/migrations/20260321170000_create_core_foundation_tables.sql, packages/domain/src/amortization/index.ts, **tests**/domain/amortization.test.ts
- [x] Ciclos financeiros personalizados
  - Critério de aceite: funções de domínio e tabela de períodos
  - Evidência: packages/domain/src/financial-cycle/index.ts, **tests**/domain/financial-cycle.test.ts
- [x] Categoria principal + múltiplas tags
  - Critério de aceite: tabelas categories/tags/junções e uso em transações
  - Evidência: supabase/migrations/20260321170000_create_core_foundation_tables.sql, apps/web/src/app/actions/transactions.ts
- [x] Priorização de pagamento
  - Critério de aceite: enum/campo + motor de priorização
  - Evidência: packages/domain/src/priority/index.ts, **tests**/domain/priority.test.ts

## Fase B — Dimensão Fornecedor (002-fornecedor)

### B1. Entidade e relações

- [x] Cadastro de fornecedores como entidade própria
  - Critério de aceite: tabela suppliers + ações CRUD
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql, apps/web/src/app/actions/suppliers.ts
- [x] Associação de fornecedor com lançamentos
  - Critério de aceite: supplier_id em transactions
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql
- [x] Associação de fornecedor com recorrências
  - Critério de aceite: supplier_id em recurring_templates
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql
- [x] Associação de fornecedor com documentos
  - Critério de aceite: supplier_id em documents
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql
- [x] Associação de fornecedor com itens de fatura
  - Critério de aceite: supplier_id em statement_items
  - Evidência: supabase/migrations/20260321170200_alter_tables_add_supplier_refs.sql

### B2. Aliases, histórico e contratos

- [x] Registro de aliases e nomes antigos
  - Critério de aceite: tabela supplier_aliases com vigência
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql
- [x] Governança de unicidade temporal de alias
  - Critério de aceite: trigger de validação
  - Evidência: supabase/migrations/20260321170400_create_triggers.sql
- [x] Auto-alias em renomeação
  - Critério de aceite: trigger de renomeação
  - Evidência: supabase/migrations/20260321170400_create_triggers.sql
- [x] Contratos e identificadores externos
  - Critério de aceite: tabela supplier_contracts
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql

### B3. Métricas de consumo (consumption_metrics)

- [x] Estrutura de métricas de consumo criada
  - Critério de aceite: tabela consumption_metrics com campos quantitativos
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql
- [x] Norma de uso formalizada
  - Critério de aceite: ADR publicada
  - Evidência: docs/adrs/ADR-002-norma-consumption-metrics.md
- [x] Constraint técnica de consistência implantada
  - Critério de aceite: validação em banco para métrica vs atributo
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql

### B4. Lacunas da dimensão fornecedor

- [ ] Tela dedicada de fornecedores no dashboard
  - Fase alvo: Fase D
  - Critério de aceite: rotas dashboard/suppliers com CRUD completo e alias/contratos
  - Observações: há ações server-side, mas não há páginas específicas em apps/web/src/app/dashboard/suppliers
- [ ] Filtros compostos avançados com fornecedor em toda a experiência de relatório
  - Fase alvo: Fase E
  - Critério de aceite: filtros por fornecedor + categoria + tags + período + prioridade em relatórios e listagens
  - Observações: relatórios atuais ainda não cobrem tudo que foi pedido para fornecedor

## Fase C — Parecer Formal (Aprovado com ajustes obrigatórios)

### C1. Ajuste obrigatório 1 — Deduplicação transação x item de fatura

- [x] Estratégia formalizada em ADR
  - Critério de aceite: ADR-001 publicada com regras de precedência/soma/exibição
  - Evidência: docs/adrs/ADR-001-deduplicacao-transacao-item-fatura.md
- [x] Estruturas de suporte implantadas
  - Critério de aceite: FK e view de deduplicação
  - Evidência: migrations 20260321170200 e 20260321170600
- [x] Regras cobertas por testes unitários
  - Critério de aceite: suíte dedup passando
  - Evidência: **tests**/domain/deduplication.test.ts

### C2. Ajuste obrigatório 2 — Norma de consumption_metrics

- [x] Norma formalizada e versionada
  - Critério de aceite: ADR-002 publicada
  - Evidência: docs/adrs/ADR-002-norma-consumption-metrics.md
- [x] Regras de dados implantadas no schema
  - Critério de aceite: campos + constraint coerentes
  - Evidência: supabase/migrations/20260321170100_create_supplier_tables.sql

### C3. Ajuste obrigatório 3 — Governança técnica de aliases

- [x] Governança formalizada e versionada
  - Critério de aceite: ADR-003 publicada
  - Evidência: docs/adrs/ADR-003-governanca-aliases-fornecedor.md
- [x] Regras técnicas básicas implantadas
  - Critério de aceite: triggers e colunas de vigência
  - Evidência: supabase/migrations/20260321170400_create_triggers.sql
- [ ] Fluxo completo de merge/reversão de fornecedores implementado operacionalmente
  - Fase alvo: Fase F
  - Critério de aceite: operação atômica disponível e testada em função server-side
  - Observações: ADR descreve a solução, mas supabase/functions está vazio

## Fase D — Governança de Engenharia (GitLab + Supabase)

### D1. Padrão de repositório e branches

- [x] Monorepo com workspaces e estrutura modular
  - Critério de aceite: apps/_ + packages/_ + supabase/_ + docs/_
  - Evidência: package.json, árvore do repositório
- [x] Convenção de commit convencional aplicada
  - Critério de aceite: commitlint configurado
  - Evidência: commitlint.config.ts
- [x] Hook de bloqueio de push com qualidade mínima
  - Critério de aceite: pre-push roda lint/typecheck/test:unit
  - Evidência: .husky/pre-push

### D2. CI/CD e ambientes

- [x] Pipeline GitLab com estágios principais
  - Critério de aceite: validate/install/check/test/build/deploy
  - Evidência: .gitlab-ci.yml
- [x] Deploy de migrations/functions previsto por ambiente
  - Critério de aceite: jobs staging e production existentes
  - Evidência: .gitlab-ci.yml
- [ ] Deploy web real automatizado (sem placeholder)
  - Fase alvo: Fase G
  - Critério de aceite: job deploy-web-\* executa integração real com provedor
  - Observações: hoje usa echo de placeholder
- [ ] Comprovação em ambiente remoto da política completa (proteções, approvals, variáveis)
  - Fase alvo: Fase G
  - Critério de aceite: checklist operacional validado no GitLab do projeto
  - Observações: depende de configuração manual na plataforma

## Fase E — Produto Web MVP (fluxo operacional)

### E1. Funcionalidades principais já disponíveis

- [x] Login/autenticação com Supabase
- [x] Dashboard operacional
- [x] CRUD de instituições
- [x] CRUD de produtos financeiros
- [x] CRUD de transações
- [x] CRUD de recorrências
- [x] CRUD de faturas
- [x] CRUD de dívidas
- [x] Upload/listagem de documentos
- [x] Importação CSV
- [x] Relatórios por período

Critério de aceite E1: rotas compilam no build e páginas acessíveis no app web.
Evidência: bun run build OK com rotas /dashboard/\*.

### E2. Lacunas funcionais ainda abertas no MVP

- [ ] Gestão visual completa da dimensão fornecedor no dashboard
  - Critério de aceite: fluxo completo (listar, criar, editar, aliases, contratos, associação assistida)
- [ ] Relatórios avançados por fornecedor com filtros compostos completos
  - Critério de aceite: respostas operacionais para perguntas do prompt 002
- [ ] Auditoria histórica visível por fornecedor na interface
  - Critério de aceite: visão de histórico consolidado e variações

## Fase F — Testes e confiabilidade

- [x] Testes unitários de domínio/validação consolidados
  - Critério de aceite: 142 testes unitários passando
  - Evidência: bun run test:unit
- [ ] Testes de integração implementados
  - Critério de aceite: suíte em **tests**/integration com cenários críticos de banco+ações
  - Observações: pasta existe, sem testes
- [ ] Testes e2e implementados
  - Critério de aceite: suíte em **tests**/e2e para fluxos principais
  - Observações: pasta existe, sem testes

## Fase G — Prontidão para entrega contínua total

- [ ] Fluxo develop -> feature -> MR -> pipeline -> staging validado ponta a ponta
  - Critério de aceite: evidência em execução real do GitLab
- [ ] Promotion controlada para main/produção com gates ativos
  - Critério de aceite: deploy de produção manual controlado e comprovado
- [ ] Checklist de operação contínua assinado pelo time
  - Critério de aceite: todos os itens críticos de segurança/qualidade/deploy fechados

## Conclusão do Checklist

- Núcleo de arquitetura, domínio e qualidade local: PRONTO
- Dimensão fornecedor na base de dados, ações e validações: MAJORITARIAMENTE PRONTA
- Camada de fechamento operacional e governança de entrega contínua remota: AINDA FALTAM ETAPAS

Este checklist deve ser usado como contrato de execução para o próximo ciclo.
