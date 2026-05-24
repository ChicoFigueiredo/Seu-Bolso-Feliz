---
Título da Reunião: Refino Geral — Estado Completo do Projeto e Próximos Passos
Data e Hora: 2026-03-24 09:11
Participantes:
  - Ana Silva (Arquiteta de Software) — facilitadora técnica
  - Carlos Mendes (Designer de Software)
  - João Pereira (Backend Sênior — Node/Bun)
  - Maria Oliveira (Backend Sênior — Node/Bun)
  - Pedro Santos (Backend Sênior — Python/Django)
  - Laura Costa (Backend Sênior — Python/Django)
  - Roberto Lima (Frontend Sênior — React/Next.js)
  - Sofia Almeida (Frontend Sênior — React/Next.js)
  - Lucas Ferreira (Mobile Sênior — React Native)
  - Beatriz Rocha (Mobile Sênior — React Native)
  - Fernando Gomes (DevOps Sênior)
  - Ricardo Monteiro (Economista / Consultor Financeiro)
  - Camila Duarte (Consultora de Finanças Pessoais)
  - Gabriela Nunes (Marketing Digital)
  - Helena Vargas (UX/UI)
  - Isabella Torres (UI Designer)
  - Thiago Martins (Front Engineer)
  - Renata Silva (QA Visual/A11y)
  - André Santos (DBA Sênior — PostgreSQL)
Pauta:
  - 1. Balanço completo do projeto vs. requisitos da Verônica
  - 2. Confirmações do CEO (Google Auth + Gmail + 1000 e-mails)
  - 3. Gaps remanescentes por fase
  - 4. Priorização das próximas entregas
  - 5. O que cada membro precisa fazer a seguir
  - 6. Decisões de arquitetura pendentes
---

## Contexto

O CEO confirmou em 2026-03-24:

1. **Login com Google Auth funciona** — acessou localhost com Gmail pessoal e chegou ao dashboard ✅
2. **Label "Comprovantes" criada no Gmail** — já com e-mails e anexos ✅
3. **Mais de 1000 e-mails** na label de comprovantes
4. Alguns e-mails precisaram de webscraping para ficarem visíveis
5. **"Continua o que dá sem mim — Confio em vocês"** — time é soberano para avançar

---

## 1. Estado Geral do Projeto — Números

| Dimensão                 | Progresso         | Evidência                                             |
| ------------------------ | ----------------- | ----------------------------------------------------- |
| Requisitos da Verônica   | **96,5%**         | 28/29 itens cobertos (checklist 002)                  |
| MVP Feature Complete     | **95%**           | 52/55 itens do checklist 001                          |
| Banco de Dados           | **100%**          | 36 tabelas normalizadas, 17 migrações imutáveis       |
| RLS (Row Level Security) | **100%**          | 27 tabelas protegidas                                 |
| Testes                   | **168+** passando | 0 falhas — domínio, integração, e2e                   |
| Web Frontend             | **100%**          | 26 rotas compiladas, CRUD completo                    |
| Dimensão Fornecedor      | **95%**           | UI completo, audit view pendente                      |
| Autenticação             | **100%**          | Email + Google OAuth funcionando                      |
| Segurança (Hardening)    | **90%**           | H1-H2 críticos resolvidos, H3+ opcionais em progresso |
| Ciclo de Ingestão        | **~65%**          | Sprint 0-5 concluídos, Gmail scanner + UI pendentes   |
| CI/CD                    | **80%**           | Pipeline existe, `deploy-web-real` é placeholder      |
| Mobile                   | **~1%**           | Somente stub em apps/mobile                           |

---

## 2. Discussão da Equipe

### Ana Silva (Arquiteta de Software)

> O projeto está em excelente estado arquitetural. A separação monorepo com 6 pacotes internos (@sbf/shared-types, validation, domain, ui-tokens, config, operations) garante coesão semântica. A hierarquia de domínio — instituição → produto → conta/contrato → evento financeiro — está sólida desde a migration 001. As 17 migrações são imutáveis e seguem sequência limpa sem dependências circulares.
>
> **Preocupação:** O `@sbf/ui-tokens` ainda é stub. Para avançar com mobile, precisamos materializar os tokens (cores, espaçamentos, tipografia) tanto para web (Tailwind) quanto para React Native. Sugiro que isso entre no roadmap antes do mobile.
>
> **Sobre Gmail scanner:** A arquitetura está pronta — workers/gmail-scanner tem o stub, o pipeline de ingestão (workers/ingestion) já processa documentos end-to-end. Só falta o client OAuth Gmail que efetivamente conecta e puxa mensagens.

### Ricardo Monteiro (Economista / Consultor)

> Do ponto de vista financeiro, o domínio está modelado corretamente. Os 3 sistemas de amortização (SAC, Price, Misto) foram implementados e os 33 testes de amortização cobrem cenários reais — incluindo quitação antecipada. A separação entre amortização, juros e encargos na tabela `liability_installments` é exatamente o que eu recomendaria para um sistema de organização financeira.
>
> **Destaque importante:** As 17 regras críticas da Verônica estão 100% cobertas por testes. Especialmente a regra 1 (pagamento de fatura ≠ nova despesa) e regra 2 (transferência interna ≠ gasto) — que são os erros mais comuns em apps financeiros concorrentes.
>
> **Sobre os 1000 e-mails do CEO:** Isso é uma excelente base de dados real. Para fase futura de IA, ter 1000+ comprovantes reais vai permitir treinar/validar classificação automática com dados do mundo real. Recomendo que priorizemos o Gmail scanner para maximizar essa oportunidade.

### Camila Duarte (Consultora de Finanças Pessoais)

> Estou impressionada com a aderência ao princípio 5 — "reduzir atrito operacional". O sistema de templates recorrentes (`recurring_templates` → `recurring_instances`) com status tracking significa que o usuário não precisa lembrar de registrar cada conta todo mês. E a priorização (essential/high/medium/low/optional) com derivação via tags é exatamente o que um sistema "que funciona mesmo com disciplina imperfeita" precisa.
>
> **Sobre 1000 e-mails:** Dados reais de 1000+ comprovantes vão nos permitir calibrar as sugestões de prioridade e categorização. Programadores como o CEO costumam ter padrão diversificado — streaming, cloud services, hardware, cafeteria, transporte por app. Ótimo para stress-test.
>
> **Minha preocupação:** A "primeira tela" (dashboard orientado a ação) está implementada, mas ainda não temos dados reais alimentando ela. O pipeline precisa funcionar end-to-end com os e-mails do CEO para validar se a experiência realmente responde: "o que pago primeiro?", "o que vence logo?", "quanto precisa durar?".

### João Pereira (Backend Sênior — Node/Bun)

> O pipeline de ingestão que implementei está robusto:
>
> - `workers/local-scanner` → funciona end-to-end
> - `workers/ingestion` → state machine completa (discovered → queued → parsing → classified → drafted → posted)
> - Parsers específicos: CEMIG (energia), boleto, PDF genérico — todos com testes
> - Draft generator: cria rascunhos de transações, recorrências e métricas
>
> **Sobre o Gmail scanner:** O stub existe em `workers/gmail-scanner/index.ts`. Preciso de ~3-5 dias para implementar:
>
> 1. Client OAuth Gmail (usando as credenciais que o CEO já configurou)
> 2. Scan por label ("Comprovantes")
> 3. Download de anexos → upload ao Supabase Storage → criação de jobs de ingestão
> 4. Detecção de mensagens já processadas (fingerprint baseado em message-id)
> 5. Modo dry-run para teste sem efeitos colaterais
>
> **NOTA:** O CEO mencionou que alguns e-mails precisaram de webscraping para ficarem visíveis. Isso pode significar e-mails com conteúdo renderizado via iframe ou imagens base64. Precisamos tratar esse cenário — pode demandar um parser adicional para conteúdo HTML embedded.

### Maria Oliveira (Backend Sênior — Node/Bun)

> Complementando o João — a segurança de APIs e secrets está sólida. O `user_secrets` com `pgp_sym_encrypt` garante que senhas de PDF e tokens OAuth não ficam expostos. O `secret-lookup.ts` no pipeline já sabe encontrar a senha do PDF pelo fornecedor/contrato.
>
> **Sobre próximos passos:** Sugiro que enquanto o João faz o Gmail scanner, eu trabalhe na evolução do pipeline para lidar com volume. Com 1000+ e-mails, precisamos garantir que o processamento seja resiliente — retry em falha, limites de concorrência, e logging adequado na tabela `ingestion_logs`.
>
> **Sobre o deploy-web-real:** Esse é um blocker real. O job no GitLab CI é placeholder (echo). Fernando precisa integrar o Vercel CLI para que o fluxo develop→staging→main funcione com deploys reais.

### Fernando Gomes (DevOps Sênior)

> A pipeline CI/CD tem 6 estágios (validate, install, check, test, build, deploy) e funciona — o único gap é o `deploy-web-real` que precisa virar um job real com `vercel deploy --prod`. Preciso de 1-2 dias para resolver.
>
> **Status dos ambientes:**
>
> - LOCAL: ✅ Supabase rodando, Next.js na porta 3105, 17 migrações aplicadas, Google Auth funcionando
> - STAGING: ✅ Supabase dcljzgjgnkmxdvhybvpt, Edge Functions deployadas, Google provider ativado
> - PRODUCTION: 🟡 Supabase opwelsgdhksuuewdbefk criado, standby (sem migrações aplicadas ainda)
>
> **Recomendação:** Ainda não devemos aplicar migrações em produção. Primeiro validar staging end-to-end com dados reais (os e-mails do CEO), depois fazer promoção controlada.

### Roberto Lima (Frontend Sênior — React/Next.js)

> O frontend web está completo: 26 rotas, 31 componentes shadcn/ui, todas as telas de CRUD funcionando. A tela de login tem "Entrar com Google" + email/senha. O dashboard é orientado a ação (não dashboard passivo).
>
> **O que falta no frontend:**
>
> 1. **Tela de auditoria do fornecedor** (`/dashboard/suppliers/[id]/audit`) — timeline visual do que aconteceu com cada fornecedor. Dados já existem em `audit_logs`. Estimativa: 2-3 dias.
> 2. **UI de ingestão** (`/dashboard/ingestion`) — tela para revisar e-mails processados, aprovar/rejeitar rascunhos, ver status dos jobs. Estimativa: 3-5 dias.
> 3. **Refinamentos visuais** — estado vazio, loading skeletons, tratamento de erros consistente. Estimativa: 2-3 dias.
>
> **Sobre design system:** Helena e Isabella precisam definir os tokens visuais antes de eu avançar com mobile. O shadcn/ui funciona bem para web, mas precisamos de uma ponte para React Native.

### Sofia Almeida (Frontend Sênior — React/Next.js)

> Concordo com o Roberto. Além do que ele listou, sugiro que priorizemos a acessibilidade. A Renata precisa fazer uma auditoria a11y antes de irmos para staging com dados reais.
>
> **Sugestão para UI de ingestão:** O fluxo deveria ser:
>
> 1. CEO vê lista de e-mails processados (status: pendente/aprovado/rejeitado)
> 2. Clica num item → vê preview do documento + rascunhos gerados
> 3. Aprova, edita ou rejeita cada rascunho
> 4. Rascunhos aprovados viram transações/recorrências reais
>
> Isso já tem suporte no schema (`draft_records`, `draft_batches`) — é "só" UI.

### Helena Vargas (UX/UI)

> A experiência de revisão de rascunhos é crítica para reduzir atrito operacional. Com 1000 e-mails, se cada um gerar 1-3 rascunhos, estamos falando de 1000-3000 itens para o CEO revisar. Isso precisa de:
>
> - Filtros eficientes (por fornecedor, tipo, data, status)
> - Aprovação em lote ("aprovar todos do Nubank" por exemplo)
> - Preview rápido (não pode precisar navegar entre páginas)
> - Undo fácil (desfazer aprovação/rejeição)
>
> **Sobre a primeira tela:** Preciso redesenhar levando em conta volume real. 1000+ e-mails muda a dinâmica UX — não é igual a 10-20 lançamentos manuais.

### Isabella Torres (UI Designer)

> Preciso trabalhar com Helena na UI de ingestão. Para alta volume (1000+ itens), proponho:
>
> - Cards condensados com informações essenciais (fornecedor, valor, data, tipo)
> - Seleção múltipla para ações em lote
> - Micro-interações para feedback visual (aprovado = slide verde, rejeitado = fade vermelho)
> - Skeleton loading para listas longas com paginação infinita
>
> **Sobre tokens:** Preciso formalizar os tokens visuais (cores, tipografia, espaçamentos, sombras) em `@sbf/ui-tokens`. Hoje é stub. Sem isso, mobile não tem base para começar.

### Thiago Martins (Front Engineer)

> A componentização web está sólida — shadcn/ui + Tailwind dá consistência. Mas concordo que precisamos materializar `@sbf/ui-tokens` antes de mobile. Proponho:
>
> - Definir tokens em JSON/TS neutro
> - Gerar automaticamente: variáveis CSS (web) + StyleSheet constants (React Native)
> - Manter 1 fonte de verdade, 2 consumidores
>
> **Performance:** Com 1000+ itens na UI de ingestão, precisamos virtualizar listas (react-window ou similiar). Sem isso, o browser vai travar com tantos DOM nodes.

### Lucas Ferreira (Mobile Sênior — React Native)

> O stub em `apps/mobile` existe, mas não tem nenhuma tela implementada. Para iniciar mobile de verdade, preciso:
>
> 1. Tokens visuais materializados (Isabella/Thiago)
> 2. Pelo menos 3 fluxos core funcionando no backend (login, dashboard, transações)
> 3. Supabase client React Native configurado
>
> **Sugestão de prioridade mobile:**
>
> 1. Tela de login (email + Google)
> 2. Dashboard principal (vencimentos, prioridades)
> 3. Registrar transação rápida
> 4. Visualizar faturas
>
> **Estimativa honesta:** Mobile funcional mínimo = 20-30 dias de trabalho dedicado. Sugiro iniciar na Fase 6 após validação de staging.

### Beatriz Rocha (Mobile Sênior — React Native)

> Complementando Lucas — com Expo, podemos reaproveitar bastante lógica. Os pacotes @sbf/validation e @sbf/domain são pure TypeScript, vão funcionar direto no React Native. O que precisa ser adaptado é a camada de UI e os hooks do Supabase.
>
> **Prioridade real:** Concordo que mobile é Fase 6+. O que importa agora é validar o MVP web com dados reais.

### Pedro Santos (Backend Sênior — Python/Django)

> Embora o backend principal seja Supabase/Bun, minha expertise em Python é relevante para a futura camada de IA. Estou mapeando:
>
> - **Classificação automática** de transações (modelo de texto → categoria + tags)
> - **Leitura de PDFs/comprovantes** via OCR + extração de dados estruturados
> - **Conciliação bancária** — match entre transações e extratos
>
> Para o momento atual, posso contribuir com a lógica de parsing mais complexa — PDFs protegidos, HTML embedded dos e-mails que o CEO mencionou, extração de tabelas.

### Laura Costa (Backend Sênior — Python/Django)

> Concordo com Pedro. Para o pipeline de ingestão processar 1000+ e-mails de forma confiável, sugiro:
>
> - Fila de processamento com retry exponencial
> - Threshold de confiança para classificação (acima de X% auto-classifica, abaixo do threshold vai para revisão manual)
> - Logs detalhados para debugging de parsing failures
>
> O schema já suporta isso (`ingestion_logs`, `draft_records` com status), é questão de robustez na implementação.

### André Santos (DBA Sênior — PostgreSQL)

> O schema está excelente — 36 tabelas normalizadas, sem FK solta, todas com RLS. Os 28 índices cobrem os padrões de query previstos.
>
> **Preocupação com volume:** 1000+ e-mails gerando rascunhos vai criar carga nas tabelas de ingestão. Recomendo:
>
> - Monitorar query performance com EXPLAIN/ANALYZE
> - A materialized view `mv_supplier_spending` precisa de refresh schedule (Edge Function já existe para isso)
> - Considerar particionamento em `transactions` se o volume mensal passar de 50k registros por usuário (improvável no MVP, mas preparar a arquitetura)
>
> **Sobre produção:** Quando aplicarmos migrações em produção, recomendo:
>
> 1. Aplicar uma migration por vez
> 2. Validar cada uma com SELECT/EXPLAIN
> 3. Backup completo antes da primeira migration

### Renata Silva (QA Visual/A11y)

> Preciso fazer uma auditoria completa de acessibilidade antes de staging. Checklist a11y:
>
> - [ ] Contraste mínimo 4.5:1 em todos os textos
> - [ ] Navegação por teclado funcional em todas as rotas
> - [ ] Focus visible em todos os interativos
> - [ ] ARIA labels em elementos não-textuais
> - [ ] Estados de erro/loading/vazio acessíveis
> - [ ] Leitor de tela navegável na tela principal
>
> **Estimativa:** 3-4 dias para auditoria + correções prioritárias.

### Gabriela Nunes (Marketing Digital)

> O site está live em https://seubolsofeliz.com.br/. Para quando o MVP estiver pronto para usuários externos:
>
> - Preparar landing page com value proposition
> - SEO básico (meta tags, sitemap, robots.txt)
> - Analytics (Google Analytics / Plausible)
>
> **Para agora:** Minha contribuição é garantir que a nomenclatura e copy do sistema sejam consistentes e amigáveis. Posso revisar textos da UI antes de staging.

---

## 3. Gaps Remanescentes por Fase

### Fase 1 — MVP (95% concluído)

| Gap                          | Responsável   | Esforço  | Prioridade |
| ---------------------------- | ------------- | -------- | ---------- |
| Supplier audit UI            | Roberto       | 2-3 dias | Média      |
| Deploy-web-real (Vercel CI)  | Fernando      | 1-2 dias | **Alta**   |
| Validação staging end-to-end | Fernando + QA | 1 dia    | **Alta**   |

### Fase 2 — Automação (60% concluído)

| Gap                                          | Responsável     | Esforço  | Prioridade  |
| -------------------------------------------- | --------------- | -------- | ----------- |
| Gmail scanner client                         | João            | 3-5 dias | **Crítica** |
| UI de ingestão (revisão de rascunhos)        | Roberto + Sofia | 3-5 dias | **Alta**    |
| Resiliência do pipeline (retry, concurrency) | Maria           | 2-3 dias | Média       |

### Fase 3 — Documentos e Leitura Assistida (20% concluído)

| Gap                                           | Responsável   | Esforço   | Prioridade      |
| --------------------------------------------- | ------------- | --------- | --------------- |
| Parser HTML embedded (webscraping de e-mails) | Pedro         | 3-5 dias  | Média           |
| OCR/Vision API para PDFs imagem               | Pedro + Laura | 5-10 dias | Baixa (pós-MVP) |
| Parser adicional: extratos bancários          | João          | 3-5 dias  | Média           |

### Fase 4 — IA (planejado, não iniciado)

| Gap                                    | Responsável    | Esforço    | Prioridade      |
| -------------------------------------- | -------------- | ---------- | --------------- |
| Classificação automática de transações | Pedro          | 5-7 dias   | Baixa (pós-MVP) |
| Sugestão de tags/prioridade            | Pedro + Camila | 3-5 dias   | Baixa (pós-MVP) |
| Assistente conversacional              | Laura          | 10-15 dias | Baixa (Fase 5+) |

### Fase 5 — MCP/Agentes (80% da base pronta)

| Gap                                 | Responsável | Esforço  | Prioridade      |
| ----------------------------------- | ----------- | -------- | --------------- |
| Integração MCP com agentes externos | João        | 2-3 dias | Baixa (pós-MVP) |
| Testes E2E do MCP server            | Maria       | 1-2 dias | Média           |

### Fase 6 — Mobile (1% concluído)

| Gap                           | Responsável       | Esforço    | Prioridade     |
| ----------------------------- | ----------------- | ---------- | -------------- |
| Tokens visuais materializados | Isabella + Thiago | 3-5 dias   | Média          |
| App shell + login             | Lucas             | 5-7 dias   | Baixa (Fase 6) |
| Dashboard mobile              | Lucas + Beatriz   | 10-15 dias | Baixa (Fase 6) |

---

## 4. Prós e Contras das Opções de Priorização

### Opção A: "Gmail First" — Priorizar Gmail scanner + UI de ingestão

**Prós:**

- CEO tem 1000+ e-mails prontos para processar → feedback imediato com dados reais
- Valida o pipeline end-to-end com volume real
- Alimenta o dashboard com dados reais → experiência mais rica
- Base de dados real para futura IA
- Demonstra valor tangível rapidamente

**Contras:**

- Deploy-web-real continua placeholder (staging manual)
- Mobile fica para depois
- Auditoria a11y adiada

### Opção B: "Deploy First" — Priorizar CI/CD + validação staging

**Prós:**

- Pipeline funcional garante que mudanças chegam a staging automaticamente
- Validação staging dá confiança para produção
- Infraestrutura sólida antes de features

**Contras:**

- Não gera valor visível para o CEO imediatamente
- Gmail scanner adiado = dados reais atrasam
- Deploy sem dados é deploy de sistema vazio

### Opção C: "Paralelo" — Gmail + Deploy em paralelo (RECOMENDADA)

**Prós:**

- João faz Gmail scanner enquanto Fernando faz deploy-web-real
- Roberto inicia UI de ingestão enquanto Maria fortalece pipeline
- Progresso em múltiplas frentes sem bloqueio
- Em 1-2 semanas, temos: Gmail processando + staging com deploy real

**Contras:**

- Mais coordenação necessária
- Risco de integração (Gmail + pipeline + UI tudo junto)
- Pode ser difícil debugar problemas distribuídos

---

## 5. Decisão Final

**A equipe decide por unanimidade pela Opção C: "Paralelo".**

### Justificativa

1. O CEO confia no time e quer progresso contínuo
2. Temos capacidade para trabalhar em paralelo — cada dev tem sua especialidade
3. O prazo implícito (CEO quer zerar dívidas até o fim do ano) exige velocidade
4. Os 1000+ e-mails são oportunidade de ouro para validação com dados reais
5. Deploy é prerequisito operacional — precisa acontecer independentemente

### Priorização Final (próximas 2 semanas)

**Semana 1 (24-28 Mar):**

1. 🔴 **Fernando** — deploy-web-real (Vercel CLI no GitLab)
2. 🔴 **João** — início do Gmail scanner client (OAuth + scan por label)
3. 🟡 **Roberto** — supplier audit UI
4. 🟡 **Maria** — resiliência do pipeline (retry, concurrency limits)
5. 🟢 **Renata** — iniciar auditoria a11y

**Semana 2 (28 Mar - 04 Abr):**

1. 🔴 **João** — conclusão Gmail scanner (download + fingerprint + dry-run)
2. 🔴 **Roberto + Sofia** — UI de ingestão (revisão de rascunhos)
3. 🟡 **Fernando + QA** — validação staging end-to-end
4. 🟡 **Helena + Isabella** — UX/UI da tela de ingestão (alta volume)
5. 🟢 **Isabella + Thiago** — materializar tokens visuais

**Semana 3+ (04 Abr - 11 Abr):**

1. 🔴 **Teste com dados reais** — processar os 1000+ e-mails do CEO
2. 🟡 **Pedro** — parser HTML embedded para e-mails com webscraping
3. 🟡 **André** — monitoramento de performance com volume real
4. 🟢 **Lucas** — prototype mobile (app shell + login)

---

## 6. Ações / Responsáveis / Prazo

| Ação                                            | Responsável       | Prazo      | Dependência        |
| ----------------------------------------------- | ----------------- | ---------- | ------------------ |
| Implementar deploy-web-real no GitLab CI        | Fernando          | 2026-03-26 | —                  |
| Iniciar Gmail scanner client                    | João              | 2026-03-28 | Credenciais CEO ✅ |
| Implementar supplier audit UI                   | Roberto           | 2026-03-28 | —                  |
| Fortalecer pipeline (retry/concurrency)         | Maria             | 2026-03-28 | —                  |
| Iniciar auditoria a11y                          | Renata            | 2026-03-28 | —                  |
| Concluir Gmail scanner (download + fingerprint) | João              | 2026-04-02 | Gmail client       |
| UI de ingestão (revisão de rascunhos)           | Roberto + Sofia   | 2026-04-04 | Schema OK ✅       |
| Validação staging end-to-end                    | Fernando + QA     | 2026-04-04 | Deploy-web-real    |
| UX/UI tela de ingestão (alta volume)            | Helena + Isabella | 2026-04-04 | —                  |
| Materializar tokens visuais                     | Isabella + Thiago | 2026-04-04 | —                  |
| Processar 1000+ e-mails do CEO                  | João + Maria      | 2026-04-08 | Gmail scanner      |
| Parser HTML embedded                            | Pedro             | 2026-04-11 | —                  |
| Monitoramento performance real                  | André             | 2026-04-11 | Dados reais        |
| Prototype mobile (app shell)                    | Lucas             | 2026-04-11 | Tokens visuais     |

---

## 7. Decisões de Arquitetura Registradas

### D1: Gmail scanner será worker local (não Edge Function)

**Razão:** Edge Functions têm timeout de 150s no plano free. Com 1000 e-mails, o scan precisa rodar por vários minutos. Worker local com Bun é mais adequado.

### D2: Processamento de e-mails será incremental

**Razão:** Fingerprint baseado em `message-id` do Gmail garante idempotência. Re-execuções não duplicam.

### D3: E-mails com webscraping serão marcados como "needs_review"

**Razão:** E-mails cujo conteúdo depende de renderização dinâmica (iframe, scripts, imagens base64) terão flag especial. O parser tentará extrair o que puder, mas o rascunho vai para revisão manual com prioridade.

### D4: Produção só recebe migrações após validação completa em staging

**Razão:** André recomenda aplicar uma migration por vez com backup. Fernando configura pipeline de promoção controlada.

### D5: Mobile é Fase 6 — após validação staging com dados reais

**Razão:** Time concorda que validar MVP web com dados reais do CEO é mais importante que mobile. Mobile começa quando tokens visuais estiverem prontos e staging validado.

---

## 8. Notas Finais

- **Próximo refino:** Após a Semana 1, para avaliar progresso do Gmail scanner e deploy-web-real
- **CEO pode testar staging** assim que Fernando completar deploy-web-real + aplicar migrações staging
- **Ricardo e Camila** ficam disponíveis para validar regras de negócio conforme dados reais aparecem
- **Gabriela** prepara copy review para todas as strings da UI antes de staging ir para users externos

**"Continua o que dá sem mim — Confio em vocês"** — CEO, 2026-03-24

O time aceita a confiança e se compromete a entregar com qualidade. 🤝
