---
Título da Reunião: Refino — Alinhamento do Produto ao Design de Telas (v2 — incorpora resposta da Verônica)
Data e Hora: 2026-04-21 10:36 (v1) · 2026-04-21 atualizado após resposta da Verônica
Status: ✅ LIBERADO PARA CODAR — Verônica 2026-04-21
Participantes:
  - Ana Silva (Arquiteta) — facilitadora · owner: merge de tickets no checklist (Ressalva #1)
  - Carlos Mendes (Designer de Software) — anotador
  - Roberto Lima (Frontend Sênior) — revisor de componentes
  - Sofia Almeida (Frontend Sênior) — revisora de rotas
  - Thiago Martins (Front Engineer) — revisor técnico de componentização
  - Helena Vargas (UX/UI) — validação de fluxos
  - Isabella Torres (UI Designer) — validação visual
  - Renata Silva (QA Visual/A11y) — critérios de aceite
  - João Pereira (Backend Sênior) — server actions e rotas · owner: decisão de ordenação Sprint 2/3 (Ressalva #2)
  - Maria Oliveira (Backend Sênior) — segurança e integridade
  - André Santos (DBA Sênior) — modelagem de dados · owner: view/função Postgres progress bar (Ressalva #3)
  - Ricardo Monteiro (Economista) — regras de domínio financeiro
  - Camila Duarte (Consultora de Finanças) — experiência do usuário
  - Fernando Gomes (DevOps) — deploy e CI/CD
  - Chico (CEO) — facilitado, palavra final
Pauta:
  - Leitura crítica do documento 003-alteracoes-novas-telas.md (Verônica)
  - 3 rodadas de debate com todos os membros (v1)
  - Decisão sobre os 7 pontos arquiteturais levantados pela Verônica
  - Plano de ação concreto e priorizado
  - Rodada 4: incorporação das 3 ressalvas da Verônica e ajustes no plano
---

# Refino — Alinhamento do Produto ao Design de Telas

> **Contexto:** Verônica entregou o documento `docs/Veronica/003-alteracoes-novas-telas.md` diagnosticando gaps críticos entre o design das telas e o que foi implementado. Esta ata documenta as 4 rodadas de debate do time e o plano de ação final, já incorporando a resposta da Verônica de 2026-04-21.

---

## ✅ Veredito da Verônica — 2026-04-21

> **Fonte:** `docs/Veronica/003-alteracoes-novas-telas.md` §9 (atualizado pela Verônica em 2026-04-21)

**LIBERADO PARA CODAR** — com **três ressalvas não-bloqueantes** que precisam ser tratadas durante a execução:

**O que o time entregou bem (reconhecido pela Verônica):**

- Aceitou 100% do diagnóstico sem contestação factual.
- Respondeu às 7 decisões arquiteturais — todas convergem com os votos da Verônica.
- Acrescentou `documents_legacy` por 30 dias antes do drop — mais seguro que a proposta original.
- Propôs `useAISuggest(toolName, params)` — **melhor que a proposta original** da Verônica. Mesma API `/api/chat`, dois modos de consumo. Endossado.
- Acrescentou `link_type = 'support'` em `document_transactions` — aceito.
- Adicionou `next/dynamic` para lazy-load do `react-pdf` — bom hardening.
- Critérios de aceite por ticket, conforme pedido.

**Ressalva #1** — Merge dos 33 tickets no checklist vivo `2026-04-01-implementacao-plano-acao.md` **antes do primeiro commit do Sprint 1**. Owner: Ana Silva. Prazo: 2026-04-22. Bloqueia: primeiro `git push` do Sprint 1.

**Ressalva #2** — S2-005 linka para rota que só nasce em S3-006. Verônica escolheu **opção (b)**: criar skeleton `/dashboard/documents/[id]/page.tsx` dentro do Sprint 2 para evitar 404 no staging. Ticket **S2-007** adicionado. Owner: João Pereira. Decidir antes do início do Sprint 2.

**Ressalva #3** — Progress bar da tela 14 deve ser computada em **view ou função Postgres** (`vw_statement_reconciliation_progress` ou `fn_reconciliation_progress(batch_id)`), não em SELECT ad-hoc na server action. Ticket **S3-009a** adicionado (antes de S3-009). Owner: André Santos.

---

## Diagnóstico Consensual — O Que a Verônica Está Certa

Antes de debater, o time leu o código. Os fatos são:

| Gap                                                                              | Severidade | Confirmado no código                             |
| -------------------------------------------------------------------------------- | ---------- | ------------------------------------------------ |
| `Fornecedores` no grupo Gestão em vez de Financeiro                              | Alta       | ✅ `app-sidebar.tsx` linha 43                    |
| 6 rotas técnicas de ingestão poluindo o sidebar                                  | Alta       | ✅ `app-sidebar.tsx` linhas 44-50                |
| `/dashboard/documents` usa tabela `documents` legada                             | Crítica    | ✅ `documents/page.tsx` faz `.from("documents")` |
| `/dashboard/ingestion/documents` usa `source_documents` mas está no lugar errado | Alta       | ✅ confirmado                                    |
| Não existe `/dashboard/documents/[id]` consumer-friendly                         | Crítica    | ✅ pasta tem só `page.tsx` sem `[id]`            |
| `document_splits` não existe                                                     | Crítica    | ✅ nenhuma migration encontrada                  |
| `document_transactions` não existe                                               | Crítica    | ✅ nenhuma migration encontrada                  |
| `AIChatDrawer` sem contexto de rota                                              | Alta       | ✅ `ai-chat-drawer.tsx` não tem `usePathname`    |
| Nenhum hotspot de IA inline nas telas 13/14                                      | Alta       | ✅ não implementado                              |
| Tela 14 (variant="statement") inexistente                                        | Crítica    | ✅ confirmado                                    |

**O time concorda com 100% do diagnóstico da Verônica.** Não há ponto a contestar factualmente.

---

## Rodada 1 — Priorização e Coragem para Cortar

### Ana Silva (Arquiteta):

O problema não é a lista — é que ela mistura coisas de 2 horas com coisas de 2 semanas. Precisamos separar o que é reorganização (sidebar, rota, redirect) do que é construção (telas 13/14, tabelas novas, IA inline). Proponho 3 blocos: **Reorganização imediata** (sidebar + rotas), **Unificação de documentos** (deprecar legada, nova lista), **Novas telas e IA** (13/14, splits, transactions, hotspots).

### João Pereira (Backend):

Concordo com a separação da Ana. Adiciono que a unificação `documents → source_documents` exige uma migration de dados antes da mudança de UI. A tabela `documents` legada tem uploads reais do usuário (Storage). Precisamos migrar esses registros para `source_documents` com `origin = 'manual_upload'` antes de desativar a UI legada. Sem isso, o usuário perde documentos que já enviou.

### André Santos (DBA):

João está certo. A migration de dados é obrigatória. Proposta: script que lê `documents`, cria registro correspondente em `source_documents` (com `origin = 'manual_upload'`, `status = 'processed'`), e cria `ingestion_job` sintético com status `completed`. Depois disso, a UI legada pode ser redirecionada. Sem apagar `documents` ainda — manter como backup até validar.

### Roberto Lima (Frontend):

Para as telas 13 e 14, concordo com a Verônica: um shell comum + dois paineis laterais distintos. Não dois componentes separados. O `DocumentDetailView` atual tem 17KB de lógica técnica de ingestão — vamos **refatorar** ele, não jogar fora. A lógica de aprovação de drafts continua usada em tela 14. O que muda é: (a) o painel lateral direito tem variação `generic` vs `statement`, e (b) o header é consumer-friendly.

### Helena Vargas (UX/UI):

O design da Verônica é claro: preview à esquerda, metadados à direita. Mas o split col na tela 13 é `[1fr_340px]` e na tela 14 é `[480px_1fr]` — inversão proposital. A 14 precisa do painel de lançamentos (tabela) mais largo. Isso significa que o shell de grid precisa ser configurável via prop, não fixo.

### Camila Duarte (Consultora):

Do ponto de vista do usuário: a prioridade máxima é que o Chico consiga **abrir um documento, ver o PDF, e aprovar ou vincular uma transação**. Isso é o fluxo central. Tudo mais — rateios, IA inline, explicabilidade — pode vir depois, mas o fluxo principal tem que funcionar primeiro.

### Ricardo Monteiro (Economista):

Concordo com a Camila. Uma ressalva sobre `document_splits`: a soma dos splits precisa ser validada contra o valor total do documento no servidor, não só na UI. Isso é regra de negócio, não cosmética.

### Renata Silva (QA):

Quero que cada item da Verônica tenha critério de aceite explícito no plano de ação. A frase "tela funcional" não é aceite. Precisa de: o que o usuário faz, o que o sistema mostra, e como verificar.

---

## Rodada 2 — Decisões Arquiteturais (os 7 pontos da Verônica)

### D4.1 — Roteamento de documento (`/dashboard/documents/[id]`)

**Ana Silva:** Shell único (`DocumentPageShell`) com discriminação por `document_type`. Server component lê `source_documents` e decide qual painel lateral renderizar. `variant: "generic" | "statement"` passa como prop.

**Roberto Lima:** Concordo. O shell compartilha: header com breadcrumb, preview PDF, coluna de metadados base. O que varia é a coluna direita: em `generic` → metadados + splits + transações vinculadas; em `statement` → resumo fatura + tabela de drafts/lançamentos.

**Decisão:** ✅ Shell único, dois paineis laterais distintos via prop `variant`.

---

### D4.2 — Unificação `documents` vs `source_documents`

**André Santos:** Migration de dados obrigatória antes da mudança de UI. Tabela `documents` legada fica como `documents_legacy` por 30 dias, depois é dropped. A UI nova aponta 100% para `source_documents`.

**João Pereira:** O upload manual também muda: `/dashboard/documents` passa a usar o mesmo `uploadDocument` action já existente em `actions/ingestion.ts`, que cria em `source_documents` diretamente.

**Decisão:** ✅ Migration de dados legada → `source_documents`. UI nova só usa `source_documents`. `documents` vira `documents_legacy` e é dropped após validação.

---

### D4.3 — Split / rateio

**André Santos:** Voto em `document_splits(id, source_document_id, category_id, tags UUID[], amount, description, created_at)`. FK em `source_documents`, não em `draft_records` — documento é permanente, draft é efêmero.

**Ricardo Monteiro:** Adicionar constraint `CHECK (amount > 0)` e trigger para validar que soma dos splits não excede valor do documento. Validar no servidor, não só na UI.

**Decisão:** ✅ Tabela `document_splits` com FK em `source_documents`. Trigger server-side valida soma.

---

### D4.4 — Vínculo documento ↔ transação

**André Santos:** Tabela `document_transactions(id, source_document_id, transaction_id, link_type, confidence, created_by, created_at)` onde `link_type ∈ {payment, refund, installment, support}` e `created_by ∈ {user, ai, pattern}`.

**Maria Oliveira:** RLS obrigatória. Usuário só vê seus próprios vínculos. Index em `(source_document_id)` e `(transaction_id)`.

**Decisão:** ✅ Tabela `document_transactions` conforme proposto. RLS + indexes.

---

### D4.5 — PDF preview

**Roberto Lima:** `react-pdf` para telas 13/14 (preview central com paginação e zoom). Iframe simples para hover-preview na lista (tela 12). Motivo: `react-pdf` adiciona ~200KB mas resolve paginação/zoom nativamente e não depende de browser policy para iframes cross-origin de Storage.

**Fernando Gomes:** Confirmo que o Supabase Storage gera URLs assinadas — `react-pdf` carrega bem com URL assinada. Sem problemas de CORS.

**Thiago Martins:** Lazy load do bundle `react-pdf` com `next/dynamic` para não pesar no initial load.

**Decisão:** ✅ `react-pdf` com `next/dynamic` para telas 13/14. Iframe para hover-preview na lista.

---

### D4.6 — Chat contextual

**Roberto Lima:** `ChatContext` React context provider é a solução correta. Cada página que quer passar contexto ao chat chama `useChatContext().setContext({ documentId, documentType, ... })`. O `AIChatDrawer` lê esse contexto e injeta como `body` no `useChat()` call.

**Sofia Almeida:** O context provider vai no layout do dashboard (`dashboard/layout.tsx`), wrapping todos os filhos. Tipagem forte com interface `ChatContextValue`.

**Decisão:** ✅ `ChatContext` provider no layout do dashboard. Páginas injetam contexto via hook.

---

### D4.7 — Sidebar reorganização

**Carlos Mendes:** Trivial tecnicamente. Mover `Fornecedores` para `navFinanceiro`. Remover 6 itens de ingestão de `navGestao`. Adicionar sub-seção "Avançado" dentro de Configurações para expor rotas técnicas de ingestão.

**Renata Silva:** Os redirects 308 para as rotas removidas do sidebar precisam existir para não quebrar bookmarks. `/dashboard/ingestion` → `/dashboard/documents`, `/dashboard/ingestion/documents` → `/dashboard/documents`, `/dashboard/ingestion/review` → `/dashboard/settings#avancado`.

**Decisão:** ✅ Sidebar reorganizado numa única PR. Redirects 308 para rotas técnicas removidas.

---

## Rodada 3 — IA "Total" e Explicabilidade

### Maria Oliveira (Backend/Segurança):

A ADR-005 continua válida: IA nunca grava no ledger sem confirmação humana. Os hotspots inline são **sugestões rejeitáveis**, não ações automáticas.

### João Pereira (Backend):

Para os hotspots de IA nas telas 13/14, as tools já existem em `lib/ai/tools.ts` (suggest_supplier, suggest_document_type, explainClassification). O que falta é a **invocação direta dessas tools na UI** — não pelo chat, mas por botões inline que chamam a API `/api/chat` com um tool call pré-definido.

**Proposta técnica:** Criar hook `useAISuggest(toolName, params)` que faz POST para `/api/chat` com a mensagem já formatada e retorna a sugestão inline. Não abre o drawer — retorna resultado direto para o componente. O drawer fica para conversas livres.

### Ana Silva (Arquiteta):

Concordo. Isso é a diferença entre "copiloto embarcado" e "chat". Dois modos de acesso às mesmas tools: (a) chat livre via drawer, (b) sugestão dirigida via hotspot inline. A API é a mesma — o que muda é o ponto de entrada e o retorno (stream vs JSON direto).

### Helena Vargas (UX/UI):

Para explicabilidade (§3.3 da Verônica): tooltip/popover com `(fonte: padrão X | OpenAI | parser)`, confiança em %, e botão "Por que esta sugestão?" que abre o drawer com `explain_extraction(document_id, field)` pré-preenchido. Isso é implementável com um componente `<AIFieldBadge field="supplier" value="CEMIG" confidence={0.87} source="pattern:fatura-cemig" documentId={id} />`.

### Camila Duarte (Consultora):

A promoção entre ambientes assistida por IA (§3.4) fica fora deste ciclo — criar ticket. O foco agora é: usuário abre documento, IA sugere, usuário aceita ou rejeita. Isso resolve 80% da dor.

### Ricardo Monteiro (Economista):

Para tela 14 (fatura cartão): a progress bar de "8 de 12 conciliados" precisa ser calculada no servidor — `count(reconciliation_status = 'reconciled') / count(*)` nos `draft_records` do batch. Não calcular na UI com dados parciais.

---

## Rodada 4 — Resposta às 3 Ressalvas da Verônica

> Esta rodada foi realizada após a Verônica retornar com veredito de liberação e três ressalvas. O time leu o documento atualizado e debateu ponto a ponto.

### Ana Silva (Arquiteta) — Ressalva #1:

Recebo. Antes do primeiro `git push` do Sprint 1, os 33 tickets S1-001…S4-009 serão transcritos para `docs/checklists/2026-04-01-implementacao-plano-acao.md`, numerados sequencialmente após o último item existente (Marco 5). O refino é ata de decisão — o checklist é a bússola de execução. São documentos diferentes com funções diferentes. O merge é obrigatório. Prazo: 2026-04-22. **Responsabilidade minha, pessoal, não delegável.**

### João Pereira (Backend) — Ressalva #2:

Concordo com a opção (b) da Verônica. Criar skeleton de `/dashboard/documents/[id]/page.tsx` dentro do Sprint 2 — server component que carrega `source_documents` por `id`, exibe header com breadcrumb + metadados crus (nome, tipo, status, data), sem preview PDF e sem IA. Isso garante que o S2-005 (link das linhas da lista) não resulte em 404 para o Chico em staging durante a janela entre Sprint 2 e Sprint 3. O S3-006/007/008 então **enriquece** esse skeleton com preview, cards e variantes. Adicionando ticket S2-007 ao plano.

### André Santos (DBA) — Ressalva #3:

Concordo com a Verônica. O cálculo de progresso de conciliação tem que ser uma **view materializada ou função SQL** — não um SELECT ad-hoc na server action. Razão: o MCP e o chat precisam consumir a mesma fonte. Proposta: `fn_reconciliation_progress(p_batch_id UUID)` que retorna `{ total_count, reconciled_count, progress_pct }`. É mais flexível que view materializada (não precisa de refresh), e o MCP/tools podem chamar via RPC. Adicionando ticket S3-009a antes de S3-009.

### Maria Oliveira (Backend/Segurança):

Sobre o `useAISuggest` (confirmação de não misturar com drawer): registrado que **S4-007** ("Conciliar com IA" → drawer) **é legítimo abrir o drawer** — é uma conversa livre sobre conciliação, não uma sugestão pontual de campo. O que não abre drawer são: S4-003, S4-004, S4-005, S4-006 e S4-008 (sugestões de campo, dropdown de candidatos, splits, categoria inline, CNPJ). Essa distinção precisa estar clara na implementação para não confundir os dois caminhos.

### Thiago Martins (Front Engineer):

Sobre `DocumentSplit` e `DocumentTransactionLink` em `packages/domain` (Princípio 8 da Verônica no §7 do documento dela): concordo. Esses são conceitos de domínio financeiro, não de apresentação. As server actions em `apps/web/src/app/actions` orquestram — não acomodam regra de negócio. Toda validação de `amount > 0` e de `SUM(splits) ≤ document.amount` tem que estar no domínio ou no banco, não na action.

### Renata Silva (QA):

Os 10 critérios de aceite do §6 da Verônica substituem ou complementam os que estávamos usando? Complementam. O §6 dela é o contrato de staging. Nossos critérios por ticket são o contrato de PR. Ambos precisam ser satisfeitos. Estamos adicionando os 10 do §6 na seção de Critérios de Aceite deste documento.

### Camila Duarte (Consultora):

Os lembretes que a Verônica listou no final (§9) precisam entrar no documento — não é texto decorativo. São restrições operacionais que vão ser esquecidas no meio da execução. Adicionando seção específica.

---

## Prós e Contras das Opções Principais

### Opção A: Refatorar `DocumentDetailView` existente (nossa decisão)

- **Prós:** Reutiliza lógica de aprovação de drafts já testada; menor superfície de regressão; mais rápido.
- **Contras:** Componente de 17KB pode ficar grande; exige cuidado com props.

### Opção B: Criar componente novo do zero

- **Prós:** Código mais limpo e consumer-friendly desde o início.
- **Contras:** Duplica toda a lógica de aprovação; risco de regressão; mais lento.

**Decisão:** Opção A, com refatoração clara e testes.

---

### Opção A: Tabela `document_splits` separada (nossa decisão)

- **Prós:** Documento é permanente; splits não precisam de draft; integridade referencial clara.
- **Contras:** Nova tabela para gerenciar.

### Opção B: Splits como campo JSONB em `source_documents`

- **Prós:** Simples, sem join.
- **Contras:** Sem FK para categorias/tags; sem validação de integridade; difícil de consultar/filtrar.

**Decisão:** Opção A.

---

## Decisão Final

O time aprova o diagnóstico completo da Verônica e propõe execução em **4 sprints sequenciais** (ver plano de ação abaixo). Não há ponto a contestar. Ajustes em relação ao documento v1 da Verônica:

- Proposta de `useAISuggest` hook para hotspots inline — mais elegante do que chamar o chat diretamente. **Endossado pela Verônica.**
- Adicionado `link_type = 'support'` em `document_transactions`. **Aceito pela Verônica.**
- Adicionado `documents_legacy` por 30 dias antes do drop. **Aceito pela Verônica.**
- Incorporadas as 3 ressalvas não-bloqueantes da Verônica (ver §Ressalvas abaixo e Rodada 4).

**✅ LIBERADO PARA CODAR** pela Verônica em 2026-04-21. Iniciar pelo Sprint 1.

---

## Plano de Ação — O Que Entra Neste Ciclo

### Sprint 1 — Reorganização (1-2 dias)

| ID     | Tarefa                                                                                            | Tipo     | Critério de Aceite                                                            |
| ------ | ------------------------------------------------------------------------------------------------- | -------- | ----------------------------------------------------------------------------- |
| S1-001 | Mover `Fornecedores` para `navFinanceiro` no `app-sidebar.tsx`                                    | frontend | Sidebar mostra Fornecedores em Financeiro                                     |
| S1-002 | Remover 5 rotas de ingestão de `navGestao` (manter apenas Documentos, Importar, Relatórios)       | frontend | Sidebar não mostra mais Ingestão/Revisão/Padrões/Logs/Documentos Ingeridos    |
| S1-003 | Adicionar sub-seção "Avançado / Pipeline" em `/dashboard/settings` com links para rotas técnicas  | frontend | Settings > Avançado lista links para /dashboard/ingestion e sub-rotas         |
| S1-004 | Criar redirects 308: `/ingestion` → `/documents`, `/ingestion/documents` → `/documents`           | frontend | Bookmarks antigos redirecionam sem erro 404                                   |
| S1-005 | Criar `ChatContext` provider em `dashboard/layout.tsx` com interface tipada                       | frontend | Context acessível via `useChatContext()` hook em qualquer página do dashboard |
| S1-006 | Conectar `AIChatDrawer` ao `ChatContext` (injeta `documentId`, `documentType` no body do useChat) | frontend | Chat drawer recebe contexto da rota atual                                     |

### Sprint 2 — Unificação de Documentos (2-3 dias)

| ID     | Tarefa                                                                                                                                                                                                                                                                                                | Tipo         | Critério de Aceite                                                                                 |
| ------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ | -------------------------------------------------------------------------------------------------- |
| S2-001 | Migration de dados: `documents` → `source_documents` (origin='manual_upload', status='processed')                                                                                                                                                                                                     | db/migration | Script roda sem erros; todos os registros migrados com `source_document_id` válido                 |
| S2-002 | Renomear tabela `documents` para `documents_legacy` (não drop ainda)                                                                                                                                                                                                                                  | db/migration | Tabela legacy existe; UI antiga não quebra imediatamente                                           |
| S2-003 | Refatorar `/dashboard/documents/page.tsx` para usar `source_documents` via `getSourceDocuments()` action                                                                                                                                                                                              | frontend     | Lista de documentos usa dados de `source_documents`; filtros de status/origem funcionam            |
| S2-004 | Upload manual em `/dashboard/documents` passa a usar `uploadDocument` action (que cria em `source_documents`)                                                                                                                                                                                         | frontend     | Upload cria registro em `source_documents` com origin='manual_upload' e dispara ingestion job      |
| S2-005 | Linhas da lista linkam para `/dashboard/documents/[id]` (rota nova, não `/ingestion/documents/[id]`)                                                                                                                                                                                                  | frontend     | Click em linha abre detalhe na nova rota                                                           |
| S2-006 | Adicionar coluna "Fornecedor" e "Status pipeline" na lista de documentos                                                                                                                                                                                                                              | frontend     | Colunas visíveis; fornecedor exibe nome do supplier vinculado                                      |
| S2-007 | **[Ressalva #2]** Criar skeleton `/dashboard/documents/[id]/page.tsx` (server component) que carrega `source_documents` por `id` e exibe header + breadcrumb + metadados crus (nome, tipo, status, data) sem preview PDF nem IA — garante continuidade em staging antes do Sprint 3 enriquecer a tela | frontend     | Rota existe e não retorna 404; header + metadados básicos exibidos; S3-006/007/008 irão enriquecer |

### Sprint 3 — Telas 13 e 14 (3-4 dias)

| ID      | Tarefa                                                                                                                                                                                                                                 | Tipo             | Critério de Aceite                                                                                                                |
| ------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| S3-001  | Migration: tabela `document_splits(id, source_document_id, category_id, tags, amount, description, created_at)` com RLS e trigger de soma                                                                                              | db/migration     | Migration aplicada; INSERT de splits com soma > valor do documento retorna erro                                                   |
| S3-002  | Migration: tabela `document_transactions(id, source_document_id, transaction_id, link_type, confidence, created_by, created_at)` com RLS e indexes                                                                                     | db/migration     | Migration aplicada; RLS ativa                                                                                                     |
| S3-003  | Server actions: `createDocumentSplit`, `listDocumentSplits`, `deleteDocumentSplit`                                                                                                                                                     | backend          | Actions funcionais com validação de soma no servidor                                                                              |
| S3-004  | Server actions: `linkTransactionToDocument`, `unlinkTransactionFromDocument`, `listLinkedTransactions`                                                                                                                                 | backend          | Actions funcionais; link cria registro em `document_transactions`                                                                 |
| S3-005  | Instalar `react-pdf` com `next/dynamic` lazy load                                                                                                                                                                                      | frontend         | Bundle de `/dashboard/documents/[id]` não inclui react-pdf no initial load                                                        |
| S3-006  | Criar rota `/dashboard/documents/[id]/page.tsx` (server component) que resolve `document_type` e decide `variant`                                                                                                                      | frontend         | Rota existe; abre `DocumentDetailView` com variant correta                                                                        |
| S3-007  | Refatorar `DocumentDetailView` para aceitar `variant: "generic" \| "statement"` e prop `gridCols`                                                                                                                                      | frontend         | Componente renderiza corretamente ambas as variantes                                                                              |
| S3-008  | Implementar `variant="generic"`: header consumer, preview PDF com react-pdf, card metadados, card splits, card transações vinculadas                                                                                                   | frontend         | Tela 13 abre PDF com paginação/zoom; cards funcionais; + Vincular transação e + Adicionar rateio aparecem                         |
| S3-009a | **[Ressalva #3]** Criar `fn_reconciliation_progress(p_batch_id UUID)` — função Postgres que retorna `{ total_count, reconciled_count, progress_pct }` para a tela 14 e para consumo via MCP/tools. Não SELECT ad-hoc na server action. | db/migration     | Função criada; server action e chat tool chamam via RPC; retorna valor consistente com tabela de drafts                           |
| S3-009  | Implementar `variant="statement"`: preview PDF, card resumo fatura (total, vencimento, progress bar), tabela de drafts com checkbox batch                                                                                              | frontend         | Tela 14 exibe drafts do batch; progress bar usa `fn_reconciliation_progress(batch_id)`; aprovação individual e em lote funcionais |
| S3-010  | Edição de metadados (fornecedor, data, valor, tipo) inline com save via server action                                                                                                                                                  | frontend/backend | Usuário clica "Editar metadados", altera campos, salva, dados atualizados sem reload                                              |
| S3-011  | Linkar coluna "Documento" em `/dashboard/transactions` para `/dashboard/documents/[id]`                                                                                                                                                | frontend         | Click em documento da transação abre tela 13/14                                                                                   |
| S3-012  | Detalhe do fornecedor (`/dashboard/suppliers/[id]`) lista documentos vinculados                                                                                                                                                        | frontend/backend | Lista de documentos aparece na página do fornecedor                                                                               |

### Sprint 4 — IA Inline e Explicabilidade (2-3 dias)

| ID     | Tarefa                                                                                                        | Tipo     | Critério de Aceite                                                                          |
| ------ | ------------------------------------------------------------------------------------------------------------- | -------- | ------------------------------------------------------------------------------------------- |
| S4-001 | Criar hook `useAISuggest(toolName, params)` que chama `/api/chat` e retorna sugestão JSON (não stream)        | frontend | Hook retorna sugestão sem abrir drawer                                                      |
| S4-002 | Componente `<AIFieldBadge field confidence source documentId />` com tooltip de explicabilidade               | frontend | Badge aparece em campos sugeridos por IA; tooltip mostra fonte, confiança, botão "Por quê?" |
| S4-003 | Hotspot: card Metadados tela 13 — campos vazios/baixa confiança mostram `<AIFieldBadge>` com aceitar/rejeitar | frontend | Campo com confiança < 0.8 mostra badge; aceitar preenche campo; rejeitar descarta           |
| S4-004 | Hotspot: botão `+ Vincular transação` — dropdown com sugestões `suggest_reconciliation` antes da busca manual | frontend | Dropdown pré-carrega candidatos sugeridos pela IA; usuário escolhe ou busca manualmente     |
| S4-005 | Hotspot: botão `+ Adicionar rateio` — "IA sugere desdobramento" pré-preenche splits do documento              | frontend | Click em "IA sugere" pré-preenche categoria e valor dos splits                              |
| S4-006 | Hotspot tela 14: chip "Categoria sugerida" inline editável por lançamento da fatura                           | frontend | Cada draft tem chip de categoria sugerida; click permite editar                             |
| S4-007 | Hotspot tela 14: botão "Conciliar com IA" por lançamento não conciliado → drawer com candidatos               | frontend | Botão abre drawer pré-preenchido com suggest_reconciliation para aquele draft               |
| S4-008 | Hotspot tela 10 (Fornecedores): campo CNPJ pré-preenche nome+aliases via IA                                   | frontend | Ao digitar CNPJ, IA sugere nome e aliases                                                   |
| S4-009 | Botão inline "Explicar" em linha com status "baixa confiança" na lista de documentos                          | frontend | Click abre drawer com explain_classification pré-preenchido                                 |

---

## O Que Fica Fora Deste Ciclo

| Item                                                      | Motivo                        | Ticket          |
| --------------------------------------------------------- | ----------------------------- | --------------- |
| Mobile                                                    | Confirmado fora de escopo     | Backlog futuro  |
| Relatórios completos (tela 11)                            | Sem UX definida               | Backlog futuro  |
| Promoção entre ambientes assistida por IA                 | Pesado para este ciclo        | Criar ticket    |
| Preview avançado (OCR inline, highlight de campos no PDF) | Requer OCR / Vision separado  | Backlog futuro  |
| PWA / modo offline                                        | Fora de escopo MVP            | Backlog futuro  |
| Drop da tabela `documents_legacy`                         | Aguardar 30 dias de validação | Agendar após S2 |

---

## Critérios de Aceite Finais (o que o Chico testa em staging)

1. **Sidebar**: Login → Financeiro tem `Instituições · Fornecedores · Produtos · Transações · Recorrências · Faturas · Dívidas`. Gestão tem `Documentos · Importar · Relatórios`.
2. **Lista de documentos**: `/dashboard/documents` lista documentos de `source_documents` (não da tabela legada); filtros de status e origem funcionam; upload manual cria documento visível na lista.
3. **Detalhe genérico**: Click em documento não-fatura abre `/dashboard/documents/[id]` com preview PDF (paginação/zoom), card de metadados editável, card de rateios (+ Adicionar rateio), card de transações vinculadas (+ Vincular transação).
4. **Detalhe fatura**: Click em documento do tipo fatura de cartão abre tela com preview PDF, resumo fatura (total, vencimento, progress bar conciliação), tabela de lançamentos detectados com aprovação individual e em lote.
5. **IA inline**: Em qualquer campo de metadados com baixa confiança, aparece badge com sugestão de IA + botão "Por quê?" que abre drawer com explicação.
6. **Chat contextual**: Abrir drawer de chat em `/dashboard/documents/[id]` — primeira mensagem da IA menciona o documento aberto, sem precisar explicar ao assistente.
7. **Fornecedor na lista de documentos**: Coluna "Fornecedor" mostra nome do supplier vinculado ou "Não identificado".
8. **Detalhe do fornecedor**: Página de fornecedor lista documentos vinculados a ele.

---

## Ações / Responsáveis / Prazo

| Ação                                                                                                                | Responsável                                | Prazo             |
| ------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ | ----------------- |
| **[BLOQUEANTE]** Transcrever 33 tickets S1-001…S4-009 para `docs/checklists/2026-04-01-implementacao-plano-acao.md` | Ana Silva                                  | 2026-04-22        |
| **[Ressalva #2]** Decidir e implementar skeleton `/dashboard/documents/[id]` (S2-007) dentro do Sprint 2            | João Pereira                               | Antes do Sprint 2 |
| Sprint 1 — Sidebar + Context                                                                                        | Roberto Lima + Sofia Almeida               | 2026-04-22        |
| Sprint 2 — Unificação documentos + S2-007                                                                           | João Pereira + André Santos                | 2026-04-24        |
| Sprint 3 — Telas 13 e 14 + S3-009a                                                                                  | Roberto Lima + João Pereira + André Santos | 2026-04-28        |
| Sprint 4 — IA inline                                                                                                | Sofia Almeida + Maria Oliveira             | 2026-04-30        |
| **[Ressalva #3]** `fn_reconciliation_progress` (S3-009a)                                                            | André Santos                               | Antes de S3-009   |
| CEO: Validar staging após cada sprint                                                                               | Chico                                      | Rolling           |

---

## Ressalvas Não-Bloqueantes da Verônica (2026-04-21)

### Ressalva #1 — Merge dos tickets no checklist vivo

Os tickets S1-001…S4-009 **precisam** ser transcritos para `docs/checklists/2026-04-01-implementacao-plano-acao.md`, numerados sequencialmente após o último item existente, **antes do primeiro commit do Sprint 1**. O CEO pediu explicitamente um checklist único — o refino é ata, não o substituto do checklist operacional.

- **Owner:** Ana Silva
- **Prazo:** 2026-04-22
- **Bloqueia:** primeiro `git push` do Sprint 1

### Ressalva #2 — Ordenação Sprint 2 / Sprint 3

S2-005 ("linhas linkam para `/dashboard/documents/[id]`") aponta para rota que só nasce em S3-006. Verônica escolheu **opção (b)**: criar skeleton em Sprint 2 — ticket **S2-007** adicionado. Skeleton exibe header + metadados crus, sem preview PDF nem IA. Sprint 3 enriquece.

- **Owner:** João Pereira
- **Prazo:** Decidir e implementar antes/durante Sprint 2
- **Por que (b) e não (a):** (a) linkaria para rota técnica `/ingestion/documents/[id]` — contradiz o princípio de "nunca rota técnica no sidebar/link do usuário final".

### Ressalva #3 — Progress bar da tela 14 server-side

O cálculo `count(reconciled) / count(total)` dos drafts **tem que ser** uma função Postgres (`fn_reconciliation_progress(batch_id)`), não um SELECT ad-hoc na server action. O MCP e o chat consomem a mesma fonte via RPC. Ticket **S3-009a** adicionado antes de S3-009.

- **Owner:** André Santos
- **Prazo:** Antes de S3-009 (progresso da tela 14 usa essa função)

---

## Princípios Não-Negociáveis (Verônica §7 + Rodada 4)

1. **Nunca dois componentes quase iguais.** Refatora com `variant` ou composition; não copia-e-cola.
2. **Nunca IA gravando sem confirmação.** Toda sugestão é inline, editável, rejeitável. ADR-005 §3.
3. **Nunca UI sobre tabela legada quando existe tabela canônica.** Deprecar `documents` em favor de `source_documents`.
4. **Nunca feature flag para esconder bug.** Se a tela 14 está quebrada, corrige.
5. **Nunca rota técnica no sidebar do usuário final.** Operador fica em Configurações/Avançado.
6. **Nunca pull request gigante.** Cada tela = uma PR. Migration = PR separada. Sidebar = PR separada.
7. **Commits conventional** (`feat`, `fix`, `docs`, `refactor`, `migration`), lowercase, sem sentence-case.
8. **Domínio em `packages/domain`.** `DocumentSplit` e `DocumentTransactionLink` vivem em `packages/domain`, não em `apps/web`. Server actions orquestram, não acomodam regra de negócio.

---

## Lembretes Operacionais (Verônica §9 — não ignorar)

- Tickets **S1-005 e S1-006** plantam `ChatContext` no Sprint 1 mas só são consumidos no Sprint 4 — **não remover** por "parece sem uso". É scaffolding intencional.
- O hook `useAISuggest` **não abre drawer**. O drawer continua para conversa livre. Não misturar os dois caminhos. (S4-007 abre drawer legitimamente — é conversão livre sobre conciliação. S4-003/004/005/006/008 usam `useAISuggest` sem drawer.)
- `documents_legacy` **não é drop automático** — só sai depois de 30 dias e depois de o CEO confirmar em staging que nenhum documento foi perdido.
- Toda tool de IA nova → atualizar [ADR-005](../../adrs/ADR-005-arquitetura-integracao-ia-chat-openai.md). Não inventar tool escondida.
- `DocumentSplit` e `DocumentTransactionLink` como conceitos de domínio vão em `packages/domain`. Server actions orquestram, não acomodam regra de negócio.
- Tool names seguem os 15 já definidos na ADR-005 §4. Se precisar de tool nova → atualizar a ADR, não inventar à solta.

---

## Nota Final do Time

> "Verônica estava certa em cada ponto. Sem contestação. O diagnóstico virou plano de ação. As 3 ressalvas foram absorvidas com ownerships definidos. Checklist será atualizado antes do primeiro push. Nenhum Sprint começa sem isso."
> — Ana Silva, em nome do time

> **✅ LIBERADO PARA CODAR.** Eu volto no fim do Sprint 1 para auditar.
> — Verônica, 2026-04-21

---
