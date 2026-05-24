# 006 — Plano de Execução: Gmail Scanner, UI de Ingestão, Deploy Real e Validação Staging

**Data:** 2026-03-24
**Referência:** Refino `2026-03-24-09-11-refino-geral-estado-projeto-proximos-passos.md`
**Status:** 📋 Em planejamento

---

## 1. Visão Geral

Este plano cobre os próximos **3 sprints (3 semanas)** com foco em:

1. **Desbloqueio operacional**: deploy-web-real (Vercel) + validação staging
2. **Ingestão real**: Gmail scanner puxando os 1000+ e-mails do CEO
3. **UI de revisão**: tela para aprovar/rejeitar rascunhos de transações
4. **Hardening**: resiliência do pipeline, auditoria a11y, monitoramento

**Regra zero**: NÃO CODAR NADA FORA DO QUE ESTÁ PLANEJADO AQUI. Escopo fechado.

---

## 2. Estado Atual — Pré-requisitos Atendidos

### ✅ Já implementado e funcionando:

- **17 migrações** aplicadas (local + staging)
- **36 tabelas** normalizadas com RLS em todas
- **168+ testes** passando (domínio, integração, e2e)
- **26 rotas web** compiladas e funcionais
- **Pipeline CI/CD** com 6 estágios no GitLab
- **Google Auth** funcionando (CEO testou e confirmou login)
- **Label "Comprovantes"** criada no Gmail com 1000+ e-mails
- **Workers**: local-scanner + ingestion pipeline completos
- **Parsers**: CEMIG, boleto, PDF genérico — todos com testes
- **Draft generator**: cria rascunhos de transações, recorrências e métricas
- **MCP Server**: 8 ferramentas implementadas
- **Edge Functions**: merge-suppliers, refresh-mv, retroactive-association

### 🟡 Parcialmente implementado (gaps a fechar):

- **Gmail scanner**: stub em `workers/gmail-scanner/index.ts`, lógica não implementada
- **UI de ingestão**: rotas stub, fluxo de revisão de rascunhos não implementado
- **deploy-web-real**: placeholder echo no GitLab CI
- **Supplier audit UI**: dados existem em audit_logs, tela não construída
- **@sbf/ui-tokens**: stub com placeholders

---

## 3. Sprint 1 — Desbloqueio Operacional (24-28 Mar)

### 3.1 Deploy Real via Vercel (Fernando)

**Objetivo:** Substituir placeholder `echo` por deploy real via Vercel CLI.

**Tarefas:**

1. Configurar variáveis Vercel no GitLab CI (VERCEL_TOKEN, VERCEL_ORG_ID, VERCEL_PROJECT_ID)
2. Implementar job `deploy-web-staging` — executa `vercel deploy --env preview` na branch develop
3. Implementar job `deploy-web-prod` — executa `vercel deploy --prod` na branch main
4. Configurar protection rules: main requer MR aprovado + pipeline verde
5. Testar fluxo completo: push develop → pipeline → deploy staging → MR → merge main → deploy prod

**Entregável:** Pipeline funcional com deploy automático web.

**Critério de aceite:**

- Job deploy-web-staging conclui com URL do preview
- Job deploy-web-prod conclui com URL de produção
- Branch main protegida contra push direto

---

### 3.2 Início do Gmail Scanner (João)

**Objetivo:** Implementar client OAuth Gmail e scan básico por label.

**Tarefas:**

1. Criar `GmailClient` class em `workers/gmail-scanner/src/gmail-client.ts`
   - OAuth2 client usando credenciais do CEO (`GOOGLE_MAIL_CLIENT_ID` + `SECRET`)
   - Método `getRefreshToken()` — flow para obter refresh token
   - Método `listMessages(labelName, maxResults, pageToken)` — listar mensagens por label
   - Método `getMessage(messageId)` — obter mensagem completa com partes
2. Criar `MessageProcessor` em `workers/gmail-scanner/src/message-processor.ts`
   - Extrair metadados: subject, from, to, date, message-id
   - Identificar partes com anexos (multipart/mixed, application/pdf, etc.)
   - Gerar fingerprint baseado em message-id (para idempotência)
3. Implementar `workers/gmail-scanner/index.ts` — orquestrador principal
   - Conectar ao Gmail via OAuth2
   - Listar mensagens na label "Comprovantes"
   - Para cada mensagem nova (não processada): criar job de ingestão
   - Modo dry-run (flag `--dry-run`)
4. Testes unitários para `GmailClient` e `MessageProcessor` (mocks)

**Entregável:** Scan funcional da label "Comprovantes" com listagem de mensagens.

**Critério de aceite:**

- `bun run workers/gmail-scanner -- --label Comprovantes --dry-run` lista mensagens
- Fingerprint não permite reprocessamento de mensagem já vista
- Logs mostram subject, date, quantity de anexos por mensagem

**Dependências:**

- ✅ Credenciais OAuth do CEO configuradas em `.env`
- ⬜ Refresh token (tarefa 3.6 do checklist 003 — João precisa gerar via fluxo OAuth)

---

### 3.3 Supplier Audit UI (Roberto)

**Objetivo:** Construir tela de timeline de auditoria para cada fornecedor.

**Tarefas:**

1. Criar rota `/dashboard/suppliers/[id]/audit`
2. Query `audit_logs` filtrado por supplier_id + tipo de ação
3. Exibir timeline: data, ação (criado, editado, merge, alias adicionado), detalhes
4. Incluir link "voltar ao fornecedor"

**Entregável:** Tela funcional com timeline de auditoria de fornecedor.

---

### 3.4 Resiliência do Pipeline (Maria)

**Objetivo:** Fortalecer o pipeline de ingestão para processar volume (1000+ docs).

**Tarefas:**

1. Implementar retry com backoff exponencial em `workers/ingestion/processor.ts`
   - Max 3 retries por job
   - Delay: 1s → 4s → 16s
   - Após 3 falhas: mover job para status `failed` com motivo
2. Implementar limit de concorrência (5 jobs simultâneos por padrão)
3. Melhorar logging em `ingestion_logs` (duração, bytes processados, erros detalhados)
4. Adicionar timeout por job (30s para PDF, 10s para CSV)

**Entregável:** Pipeline resiliente com retry, limites e logs detalhados.

---

### 3.5 Início da Auditoria A11y (Renata)

**Objetivo:** Identificar problemas de acessibilidade nas rotas existentes.

**Tarefas:**

1. Executar Lighthouse/axe em todas as 26 rotas
2. Verificar contraste, foco, teclado, ARIA em telas críticas (login, dashboard, transações)
3. Gerar relatório com issues + severidade + sugestão de fix
4. Priorizar: P0 (bloqueador), P1 (importante), P2 (nice-to-have)

**Entregável:** Relatório de a11y com lista priorizada de issues.

---

## 4. Sprint 2 — Ingestão Real (28 Mar - 04 Abr)

### 4.1 Gmail Scanner Completo (João)

**Objetivo:** Download de anexos + integração com pipeline de ingestão.

**Tarefas:**

1. Implementar download de anexos em `workers/gmail-scanner/src/attachment-handler.ts`
   - Download base64 das partes attachment
   - Upload para Supabase Storage (bucket `ingestion`)
   - Suportar múltiplos anexos por mensagem
2. Implementar criação de jobs de ingestão
   - Para cada anexo: criar registro em `ingestion_jobs` com referência ao `source_documents`
   - Metadata: subject do e-mail, remetente, data, message-id
   - Status inicial: `discovered`
3. Implementar detecção de duplicatas
   - Fingerprint por message-id + attachment filename + size
   - Registrar em `document_fingerprints`
   - Pular mensagens/anexos já processados
4. Testes de integração: scan → download → upload → job criado

**Entregável:** Pipeline Gmail → Storage → Ingestão funcionando end-to-end.

**Critério de aceite:**

- `bun run workers/gmail-scanner -- --label Comprovantes --limit 10` processa 10 primeiros
- Anexos aparecem no Supabase Storage
- Jobs aparecem em `ingestion_jobs` com status `discovered`
- Reexecução não cria duplicatas

---

### 4.2 UI de Ingestão — Revisão de Rascunhos (Roberto + Sofia)

**Objetivo:** Tela para CEO revisar e aprovar/rejeitar rascunhos gerados pelo pipeline.

**Tarefas:**

1. Criar rota `/dashboard/ingestion` — lista de batches de rascunhos
   - Filtros: status (pendente/aprovado/rejeitado), data, fornecedor
   - Ordenação: data mais recente, ou por fornecedor
   - Paginação com scroll infinito
2. Criar rota `/dashboard/ingestion/[batchId]` — detalhe de um batch
   - Preview do documento original (link para Storage)
   - Lista de rascunhos gerados (transação, recorrência, métrica)
   - Ações: aprovar individual, aprovar todos, rejeitar, editar
3. Implementar aprovação em lote
   - "Aprovar todos deste batch"
   - "Aprovar todos do fornecedor X"
   - Confirmação antes de ação em lote
4. Integrar com MCP tools existentes: `approve_draft_batch`, `list_draft_batches`

**Entregável:** UI funcional para revisão de rascunhos com ações em lote.

**Critério de aceite:**

- Lista de batches carrega paginada
- Preview do documento mostra PDF/imagem
- Aprovar rascunho cria transação real na tabela `transactions`
- Rejeitar rascunho move para status `rejected`

---

### 4.3 Validação Staging End-to-End (Fernando + QA)

**Objetivo:** Validar que staging funciona com o fluxo completo real.

**Tarefas:**

1. Verificar que migrações estão aplicadas em staging (17/17)
2. Verificar que Edge Functions estão deployadas (3/3)
3. Deploy web via pipeline (usar o novo job deploy-web-staging)
4. Testar login (email + Google) em staging
5. Testar CRUD de uma entidade (instituição + produto + transação)
6. Verificar RLS (dados de user A não aparecem para user B)
7. Documentar resultado em relatório

**Entregável:** Staging validado e documentado, pronto para dados reais.

---

### 4.4 UX/UI para Ingestão de Alta Volume (Helena + Isabella)

**Objetivo:** Design especificamente pensado para revisão de 1000+ itens.

**Tarefas:**

1. Wireframe da tela de listagem (cards condensados, bulk select)
2. Wireframe da tela de detalhe (split view: documento | rascunhos)
3. Design de micro-interações (aprovação, rejeição, undo)
4. Considerações de performance (virtualização, lazy loading)
5. Handoff para Roberto + Sofia

**Entregável:** Wireframes + specs para implementação.

---

### 4.5 Materializar Tokens Visuais (Isabella + Thiago)

**Objetivo:** Transformar `@sbf/ui-tokens` de stub em pacote funcional.

**Tarefas:**

1. Definir paleta de cores (primary, secondary, semantic: success, warning, error, info)
2. Definir escala tipográfica (heading 1-6, body, caption, monospace)
3. Definir espaçamentos (4px grid: xs=4, sm=8, md=16, lg=24, xl=32, 2xl=48)
4. Definir sombras e border-radius
5. Exportar como:
   - CSS custom properties (web/Tailwind)
   - TypeScript constants (React Native StyleSheet)
6. Integrar Tailwind theme com tokens do pacote

**Entregável:** Pacote `@sbf/ui-tokens` funcional com tokens para web e mobile.

---

## 5. Sprint 3 — Validação com Dados Reais (04-11 Abr)

### 5.1 Processamento dos 1000+ E-mails do CEO (João + Maria)

**Objetivo:** Executar o pipeline completo com os e-mails reais do CEO.

**Tarefas:**

1. Executar Gmail scanner em modo dry-run primeiro (verificar contagem, tipos de anexo)
2. Executar em batches de 50 (monitorar performance, erros, timeouts)
3. Verificar rascunhos gerados (quantos transações, recorrências, métricas?)
4. Identificar e-mails problemáticos (webscraping, HTML embedded, PDFs protegidos)
5. Ajustar parsers conforme necessário para tipos não suportados
6. Relatório: X e-mails processados, Y rascunhos gerados, Z falhas

**Entregável:** Base de rascunhos reais pronta para revisão pelo CEO.

**Critério de aceite:**

- ≥90% dos e-mails processados sem erro
- Rascunhos gerados com fornecedor, valor e data corretos (amostra de 20)
- Nenhuma duplicata
- Falhas documentadas com motivo e solução proposta

---

### 5.2 Parser HTML Embedded (Pedro)

**Objetivo:** Tratar e-mails cujo conteúdo é renderizado dinamicamente.

**Tarefas:**

1. Analisar amostra de e-mails problemáticos (os que CEO mencionou precisar webscraping)
2. Implementar extração de texto de HTML (cheerio ou similar)
3. Tratar iframes, imagens base64, tabelas HTML
4. Fallback: quando extração falha, marcar como `needs_review`
5. Testes com e-mails reais do CEO

**Entregável:** Parser funcional para HTML embedded em e-mails de comprovantes.

---

### 5.3 Monitoramento de Performance (André)

**Objetivo:** Garantir que o banco aguenta o volume de rascunhos + transações.

**Tarefas:**

1. EXPLAIN ANALYZE nas queries mais frequentes (dashboard, list drafts, reports)
2. Verificar índices utilizados vs. seq scans
3. Monitorar tempo de refresh da materialized view
4. Avaliar necessidade de vacuum/analyze manual
5. Relatório com recomendações

**Entregável:** Relatório de performance + ajustes necessários.

---

### 5.4 Prototype Mobile — App Shell (Lucas)

**Objetivo:** Criar casca do app mobile com login funcional.

**Tarefas:**

1. Configurar Expo com Supabase client
2. Tela de login (email + Google OAuth via expo-auth-session)
3. Navegação básica (tabs: Dashboard, Transações, Configurações)
4. Integrar tokens visuais do `@sbf/ui-tokens`
5. Tela de dashboard placeholder (lista de vencimentos do mês)

**Entregável:** App mobile rodando no Expo Go com login funcional.

---

## 6. Diagrama de Dependências

```
Sprint 1 (24-28 Mar)
├── Fernando: deploy-web-real ────────────────┐
├── João: Gmail scanner início ───────────────┤
├── Roberto: supplier audit UI                │
├── Maria: pipeline resilience                │
└── Renata: auditoria a11y                    │
                                              │
Sprint 2 (28 Mar - 04 Abr)                   │
├── João: Gmail scanner completo ─────────────┤
├── Roberto+Sofia: UI de ingestão             │
├── Fernando+QA: validação staging ◄──────────┘
├── Helena+Isabella: UX ingestão alta volume
└── Isabella+Thiago: tokens visuais ──────────┐
                                              │
Sprint 3 (04-11 Abr)                         │
├── João+Maria: processar 1000+ e-mails      │
├── Pedro: parser HTML embedded               │
├── André: monitoramento performance          │
└── Lucas: prototype mobile ◄─────────────────┘
```

---

## 7. Riscos e Mitigações

| Risco                                     | Probabilidade | Impacto | Mitigação                                                |
| ----------------------------------------- | ------------- | ------- | -------------------------------------------------------- |
| Refresh token do Gmail não funciona       | Baixa         | Alta    | João testa fluxo OAuth antes de implementar scanner      |
| Volume de 1000 e-mails causa timeout      | Média         | Média   | Maria implementa batching + retry antes do scan real     |
| E-mails com webscraping não parseiam      | Alta          | Média   | Pedro implementa parser HTML; fallback para needs_review |
| Vercel deploy falha no GitLab CI          | Baixa         | Alta    | Fernando testa localmente antes de configurar CI         |
| Performance do banco com volume           | Baixa         | Média   | André monitora desde o primeiro batch de 50              |
| Tokens visuais atrasam e bloqueiam mobile | Média         | Baixa   | Mobile é Fase 6, não é bloqueador do MVP                 |

---

## 8. Métricas de Sucesso (Fim do Sprint 3)

| Métrica                   | Target                                    |
| ------------------------- | ----------------------------------------- |
| E-mails processados       | ≥900/1000 (90%)                           |
| Rascunhos gerados         | ≥1000 (pelo menos 1 por e-mail com anexo) |
| Taxa de erro no pipeline  | ≤10%                                      |
| Deploy staging funcional  | ✅                                        |
| Deploy produção funcional | ✅                                        |
| Tests passing             | ≥180 (12+ novos)                          |
| Rotas web com a11y P0     | 0 issues bloqueadores                     |
| Tokens visuais definidos  | ✅ pacote publicado internamente          |

---

## 9. Referências

- Checklist principal: `docs/checklists/001-implementacao-geral.md`
- Checklist Verônica: `docs/checklists/002-pedidos-veronica.md`
- Checklist ingestão: `docs/checklists/003-ciclo-ingestao-automacao-mcp-agentes.md`
- Plano de ingestão anterior: `docs/planejamento/005-plano-faseado-ingestao-automacao-mcp-agentes.md`
- Refino de referência: `docs/refinos/2026-03/2026-03-24-09-11-refino-geral-estado-projeto-proximos-passos.md`
- Requisitos da Verônica: `docs/Veronica/001-prompt.inicial.md`, `docs/Veronica/002-fornecedor.md`
