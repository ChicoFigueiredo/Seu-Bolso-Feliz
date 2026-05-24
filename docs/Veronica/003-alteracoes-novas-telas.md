# Prompt de refino — Alinhamento do produto ao design de telas (v1)

> **Autora:** Verônica — consultora sênior, arquitetura/DDD/clean code, sem paciência para código supérfluo e sem paciência para fluxos meia-boca.
> **Para:** time de engenharia, produto e design.
> **Entrada:** [docs/design/Telas Seu Bolso Feliz.html](../design/Telas%20Seu%20Bolso%20Feliz.html) (produzido pelo Claude Design) + código da branch `002-ingestao-dados-e-IA`.
> **Saída esperada:** plano de ação concreto, por tela, verificável em staging, sem inventar documentação paralela.

---

## 📌 Atualização Verônica — 2026-04-21

> **Resposta ao refino do time** em [docs/refinos/2026-04/2026-04-21-10-36-refino-alinhamento-design-telas-plano-acao.md](../refinos/2026-04/2026-04-21-10-36-refino-alinhamento-design-telas-plano-acao.md).
>
> **Veredito:** ✅ **LIBERADO PARA CODAR** — com três ressalvas não-bloqueantes (ver final do §8).
>
> **O que o time entregou bem:**
>
> - Aceitou 100% do diagnóstico sem contestação factual.
> - Respondeu às 7 decisões arquiteturais — todas convergem com o que eu havia votado (shell único com `variant`, `document_splits`, `document_transactions`, `react-pdf` lazy, `ChatContext`, sidebar numa PR só).
> - Acrescentou **`documents_legacy` por 30 dias** antes do drop — plano de migração de dados mais seguro que o meu "deprecar agora".
> - Propôs `useAISuggest(toolName, params)` como hook para hotspots inline — **melhor que minha proposta original** de abrir o drawer pré-preenchido. Mesma API `/api/chat`, dois modos de consumo (stream via drawer, JSON direto via hook). Endossado.
> - Acrescentou `link_type = 'support'` em `document_transactions` — cobre o caso de documento como anexo probatório sem ser pagamento/reembolso. Aceito.
> - Adicionou `next/dynamic` para lazy-load do `react-pdf` — bom hardening.
> - Critérios de aceite por ticket, conforme eu pedi.
>
> As seções §4 e §8 abaixo foram atualizadas com as decisões finais (riscadas as perguntas, acrescentadas as respostas). O resto do doc fica como está — o diagnóstico permanece válido.

---

## 0. Leia isto antes de sugerir qualquer linha de código

Eu vou ser direta. O Chico não tem mais paciência — e eu também não. Já temos:

- um **refino principal** aprovado ([docs/refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md](../refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md));
- um **checklist de execução** vivo ([docs/checklists/2026-04-01-implementacao-plano-acao.md](../checklists/2026-04-01-implementacao-plano-acao.md));
- **ADR-005** (IA/chat como drawer, Vercel AI SDK + Edge proxy, IA nunca grava direto no ledger);
- **ADR-006** (padrões documentais + `pattern_feedback` + auto-desativação).

Este documento **não reabre** essas decisões. Ele aponta o delta entre o que o design exige e o que existe hoje no código, e força o time a responder perguntas antes de codar. **Não é para criar um novo marco.** É para fechar as pontas soltas dos marcos em curso (M5/M6) incorporando o design como contrato de UX.

**Regra de ouro:** o design é autoridade sobre UX; as ADRs são autoridade sobre arquitetura; o refino principal é autoridade sobre fases. Se houver conflito entre design e ADR, **a ADR ganha** (ex.: chat continua drawer, não página). Se o design pedir algo que não tem backend, **o backend é quem se mexe**, não o design.

---

## 1. Diagnóstico — o que temos hoje vs o que o design descreve

### 1.1 Estrutura de navegação (Sidebar)

**O design define** (ver screen 13 [linhas 2594-2641 do HTML](../design/Telas%20Seu%20Bolso%20Feliz.html)):

```
Visão Geral
Financeiro  → Instituições · Fornecedores · Produtos · Transações · Recorrências · Faturas · Dívidas
Gestão       → Documentos · Importar · Relatórios
Footer       → Configurações
```

**O código faz** ([apps/web/src/components/app-sidebar.tsx](../../apps/web/src/components/app-sidebar.tsx)):

```
Visão Geral
Financeiro  → Instituições · Produtos · Transações · Recorrências · Faturas · Dívidas
Gestão       → Fornecedores · Documentos · Ingestão · Documentos Ingeridos · Revisão · Padrões · Logs Ingestão · Importar · Relatórios
Footer       → Configurações
```

**Delta:**

1. `Fornecedores` está em Gestão — o design coloca em **Financeiro**. Mover.
2. Gestão está poluída com **seis rotas técnicas de ingestão** (`/dashboard/ingestion`, `/ingestion/documents`, `/ingestion/review`, `/ingestion/patterns`, `/ingestion/logs`) que **não existem no design**. O usuário não é engenheiro — ele não quer ver "jobs", "drafts" e "logs" como itens de primeiro nível.
3. O chat IA **não aparece na sidebar** (correto — ADR-005 diz drawer). Mas o FAB atual ([components/chat-toggle.tsx](../../apps/web/src/components/chat-toggle.tsx)) também não aparece no design. Precisa ficar, precisa combinar com o header do design, e precisa ser contextual (ver §3).

### 1.2 Inventário de telas — mapa de 16 → código

| #   | Tela (design)                 | Rota (design)              | Rota atual no código                                                           | Estado                               |
| --- | ----------------------------- | -------------------------- | ------------------------------------------------------------------------------ | ------------------------------------ |
| 01  | Landing                       | `/`                        | `/` ([app/page.tsx](../../apps/web/src/app/page.tsx))                          | Existe, revisar cópia/hero           |
| 02  | Login                         | `/login`                   | `/login`                                                                       | Existe                               |
| 03  | Visão Geral                   | `/dashboard`               | `/dashboard` ([dashboard/page.tsx](../../apps/web/src/app/dashboard/page.tsx)) | Existe, alinhado em cards principais |
| 04  | Transações                    | `/dashboard/transactions`  | idem                                                                           | Existe                               |
| 05  | Recorrências                  | `/dashboard/recurring`     | idem                                                                           | Existe                               |
| 06  | Faturas                       | `/dashboard/statements`    | idem                                                                           | Existe                               |
| 07  | Dívidas                       | `/dashboard/liabilities`   | idem                                                                           | Existe                               |
| 08  | Produtos                      | `/dashboard/products`      | idem                                                                           | Existe                               |
| 09  | Instituições                  | `/dashboard/institutions`  | idem                                                                           | Existe                               |
| 10  | Fornecedores                  | `/dashboard/suppliers`     | idem                                                                           | Existe, precisa virar Financeiro     |
| 11  | Relatórios                    | `/dashboard/reports`       | idem                                                                           | Existe (placeholder, revisar)        |
| 12  | Documentos (lista)            | `/dashboard/documents`     | `/dashboard/documents` (legacy) + `/dashboard/ingestion/documents`             | **Duplicado — unificar**             |
| 13  | **Documento (detalhe)**       | `/dashboard/document`      | `/dashboard/ingestion/documents/[id]` (técnica)                                | **Novo para o usuário final**        |
| 14  | **Documento — Fatura cartão** | `/dashboard/statement-doc` | — (inexistente)                                                                | **Novo**                             |
| 15  | Importar                      | `/dashboard/import`        | idem                                                                           | Existe                               |
| 16  | Configurações                 | `/dashboard/settings`      | idem                                                                           | Existe                               |

**Observação crítica sobre rotas 13/14:** o design grafa rotas no singular sem id (`/dashboard/document`, `/dashboard/statement-doc`). Isso é ilustrativo do protótipo, **não é o contrato real**. A rota real deve ser `/dashboard/documents/[id]` com roteamento interno por `source_document.document_type` — ver §4.

### 1.3 Modelo de dados — o elefante na sala

Existe hoje **dois caminhos paralelos** para documento:

1. Tabela legada `documents` (usada em [dashboard/documents/page.tsx:231](../../apps/web/src/app/dashboard/documents/page.tsx)) — upload manual simples, sem estado, sem relacionamento com drafts.
2. Tabela canônica `source_documents` + `ingestion_jobs` + `draft_records` (migration [20260323120200_create_ingestion_tables.sql](../../supabase/migrations/20260323120200_create_ingestion_tables.sql)) — todo o pipeline de ingestão.

O design **não distingue** as duas origens. Para o usuário, documento é documento: vem do Gmail, do upload manual, do chat — tanto faz. O detalhe (telas 13/14) mostra o mesmo PDF, os mesmos metadados, os mesmos registros relacionados.

**Decisão necessária (ver §6):** consolidar toda a UI de documento sobre `source_documents` e **deprecar** a tabela `documents` legada (ou rebatizá-la se houver dado histórico útil). Sem isso, as telas 12/13/14 serão dois componentes quase iguais vivendo em dois mundos — é exatamente o tipo de dívida que eu me recuso a carimbar.

### 1.4 Componente de detalhe já existe, mas está errado

[`components/document-detail-view.tsx`](../../apps/web/src/components/document-detail-view.tsx) (17 KB) implementa um split-view técnico: ingestion job, drafts, corrections. O design 13 e 14 pedem algo **mais consumer-friendly**: preview PDF à esquerda, metadados + registros relacionados à direita, sem jargão de pipeline.

Não se cria um componente novo do zero — **refatora-se este** para aceitar um prop `variant: "generic" | "statement"` e renderizar os dois layouts, compartilhando o preview e a coluna de metadados base. DRY, mas com variação explícita, não um god-component mascarado.

---

## 2. Delta detalhado por tela

Formato: **O que existe → O que falta → Decisão pendente** (quando aplicável).

### Telas 01-02 (Landing, Login)

Cosmético. Passar o Tailwind das telas para conferência visual no Storybook/preview; se divergir, alinhar tipografia e espaçamento. **Não bloqueia nada.**

### Tela 03 — Visão Geral (`/dashboard`)

**Existe:** 4 cards (Receita/Despesas/Saldo/Dívida), Fila de Prioridade, Recorrências Próximas, Faturas em Aberto, Últimas Transações. **Falta:** conferir nomenclatura dos badges (`PriorityBadge`) contra o design e garantir que o card de Dívida Total usa a mesma semântica do design (passivos líquidos, não saldo devedor por produto). **Decisão:** manter como está. Ajustes menores apenas.

### Tela 04 — Transações

Coluna "Documento" do design precisa linkar para tela 13 (`/dashboard/documents/[id]`), não para a página técnica de ingestão. **Mudar o link.**

### Tela 05 — Recorrências

Design mostra vínculo com fornecedor + último documento recebido. Validar se a query de listagem já traz essas relações.

### Tela 06 — Faturas

Fatura de cartão (statement) do design deve, ao clicar, abrir **tela 14** (não tela 13 genérica). Requer discriminador por tipo de documento.

### Tela 07 — Dívidas

Sem delta material além de cosmética.

### Tela 08 — Produtos

Sem delta material.

### Tela 09 — Instituições

Sem delta material.

### Tela 10 — Fornecedores

**Mover da seção Gestão para Financeiro.** Validar que o detalhe do fornecedor lista documentos (screens 13/14) vinculados — hoje não lista.

### Tela 11 — Relatórios

Placeholder. Não é o foco deste refino.

### Tela 12 — Documentos (lista) — `/dashboard/documents`

**Existe:** versão legada em `apps/web/src/app/dashboard/documents/page.tsx` usando tabela `documents`.

**Falta:**

- Apontar para `source_documents` (unificar origem).
- Colunas conforme design: Nome, Fornecedor (inferido), Tipo, Data, Status (ingestão), Valor (se extraído), Ações.
- Filtro por origem (Gmail / Upload manual / Chat), status, fornecedor, tipo, período — **já especificado no refino principal, Fase B** ([2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md](../refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md)).
- Upload manual na mesma tela (já existe, manter).
- Linhas devem linkar para `/dashboard/documents/[id]` (que resolve para variante 13 ou 14 conforme tipo).

**Deprecações:**

- `/dashboard/ingestion/documents` → redirect 308 para `/dashboard/documents`.
- `/dashboard/ingestion` (página principal de pipeline) → remover do sidebar, manter rota para debug interno ou também redirecionar.
- `/dashboard/ingestion/review`, `/ingestion/patterns`, `/ingestion/logs` → manter rotas para operador avançado (acessíveis via Configurações → "Avançado / Pipeline"), **fora do sidebar principal**.

### Tela 13 — Documento (detalhe) — `/dashboard/documents/[id]` (variant="generic")

**Não existe** a versão consumer.

**O que o design exige** (ver HTML [linhas 2565-2854](../design/Telas%20Seu%20Bolso%20Feliz.html)):

- **Header:** breadcrumb Documentos / {filename}, título com ícone, metadados curtos ("Nota fiscal · 102 KB · enviado em …").
- **Ações do header:** Baixar · Substituir arquivo · Excluir.
- **Layout split:** `lg:grid-cols-[minmax(0,1fr)_340px]` — preview à esquerda, cards à direita.
- **Preview PDF** com paginação (`Pág. X/Y`), zoom (±, reset 100%), "Abrir em nova aba".
- **Card Metadados:** Fornecedor (nome + CNPJ), Data, Valor, Tipo (chip), Categoria (chip). Botão "Editar metadados".
- **Card Registros Relacionados:**
  - Subsecção **Pagamentos** (lista de transactions vinculadas, valor, data, conta/produto, chip de status). Ação `+ Vincular transação`.
  - Subsecção **Desdobramentos/rateios** (lista de splits com categoria + tags, valor). Ação `+ Adicionar rateio`. **Progress strip:** "Soma dos rateios: R$ X / R$ Total".

**Backend necessário:**

- Endpoint/Action para servir o PDF com URL assinado do Supabase Storage (preferencialmente renderização inline; se muito complexo, iframe + `object` tag).
- Modelo de **rateio/split** (tabela `document_splits` ou equivalente — **precisa ser confirmado pelo time**, ver §6). Referenciar por `source_document_id` e por categoria/tags (multi-tag via FK ou array).
- Endpoint para vincular transação existente ao documento: `POST /documents/[id]/links` com `{ transaction_id }`, criando registro em tabela associativa N-N.

### Tela 14 — Documento — Fatura cartão — `/dashboard/documents/[id]` (variant="statement")

**Não existe.**

**O que o design exige** (ver HTML linhas 2855+):

- Mesma estrutura de shell (header + preview + coluna lateral).
- **Layout:** `lg:grid-cols-[minmax(0,480px)_minmax(0,1fr)]` — preview à esquerda (mais estreito), painel de lançamentos à direita (mais largo).
- **Resumo da fatura** (card superior direito): total, vencimento, ciclo, **lançamentos detectados** com progress bar verde ("8 de 12 conciliados").
- **Lançamentos detectados** (tabela):
  - Coluna de checkbox para aprovação em lote.
  - Descrição, data, valor.
  - **Categoria inline** (edição direta sem sair da tabela).
  - **Status chips:** Conciliado · Recorrência · Novo.
  - Filtro "Todos" / "Pendentes" / "Conciliados" + ordenação por Data/Valor.
  - Badge "↳ vinculado a {recorrência/transação}" quando aplicável.

**Backend:**

- O parser de fatura já produz `draft_records` com `draft_type = 'transaction'` para cada lançamento (Marco 5). A UI consome esses drafts, apresenta com a UI de fatura, e permite aprovação individual/lote.
- Coluna de status do draft (`pending_review` / `approved` / `posted` / `reconciled`) mapeia 1:1 para os chips do design (ver [migration 20260323120200_create_ingestion_tables.sql](../../supabase/migrations/20260323120200_create_ingestion_tables.sql)).
- Progress bar = `count(reconciled) / count(total)`.

### Tela 15 — Importar

Existe. Validar se a UI bate com o design (drop zone, lista de arquivos, progresso).

### Tela 16 — Configurações

Existe. Acrescentar sub-seção "Avançado / Pipeline" que expõe as rotas técnicas removidas do sidebar (ingestion runs, jobs, logs, patterns).

---

## 3. Integração com IA — "total" significa o quê, em operação

O Chico disse, literalmente: _"A INTEGRAÇÃO COM IA TEM QUE SER TOTAL"_ ([.github/prompts.md:457](../../.github/prompts.md)). O design **não desenha** o chat. Isso **não significa** que o chat não existe — significa que o drawer é o canal, conforme ADR-005, e que a IA se acopla em **múltiplos pontos da UI sem virar uma tela separada**.

### 3.1 O chat drawer (já existe, manter, polir)

- FAB `bottom-6 right-6` ([chat-toggle.tsx](../../apps/web/src/components/chat-toggle.tsx)) continua.
- Drawer continua acessível de qualquer rota.
- **Requisitos novos:**
  1. Drawer recebe **contexto da rota atual** via prop/props selector — se o usuário está em `/dashboard/documents/[id]`, o chat começa com `document_id` já no contexto (system message ou tool call inicial).
  2. Upload dentro do chat: drag&drop + botão de anexar, enviando arquivo para Storage via mesma pipeline manual da tela 15.
  3. Streaming via Vercel AI SDK (já decidido na ADR-005) — confirmar que o `useChat()` está configurado em [`api/chat`](../../apps/web/src/app/api/chat) com function calling.

### 3.2 IA embarcada em cada tela crítica (não é chat, é copiloto in-place)

Onde a IA tem que aparecer **sem o usuário abrir o drawer**:

| Tela                     | Hotspot de IA                                     | Ação                                                                          |
| ------------------------ | ------------------------------------------------- | ----------------------------------------------------------------------------- |
| 12 (Documentos)          | Linha com status "baixa confiança"                | Botão inline "Explicar" → drawer pré-preenchido                               |
| 13 (Detalhe)             | Card Metadados com campo vazio ou baixa confiança | Chip "IA sugere: {valor}" com aceitar/rejeitar inline                         |
| 13                       | Botão `+ Vincular transação`                      | Dropdown com sugestões de IA antes da busca manual (`suggest_reconciliation`) |
| 13                       | Botão `+ Adicionar rateio`                        | "IA sugere desdobramento baseado no documento" — pré-preenche split sugerido  |
| 14 (Fatura)              | Cada lançamento detectado                         | Chip "Categoria sugerida" inline editável                                     |
| 14                       | Lançamento com status "Novo" (não conciliado)     | Botão "Conciliar com IA" → drawer com candidatos                              |
| 10 (Fornecedores)        | Criação de novo fornecedor                        | Campo de CNPJ pré-preenche nome+aliases via IA                                |
| 16 (Settings > Avançado) | Seção "Padrões Documentais"                       | "IA sugere criar padrão a partir deste documento"                             |

**Princípio:** toda sugestão de IA aparece **inline, editável, rejeitável**. Nunca grava sozinha ([ADR-005 §3](../adrs/ADR-005-arquitetura-integracao-ia-chat-openai.md)).

> **✏️ Verônica (2026-04-21):** a implementação dos hotspots **não** é via abertura do drawer pré-preenchido. O time propôs — e eu aceito — um hook `useAISuggest(toolName, params)` que faz `POST /api/chat` formatado e retorna JSON direto (sem stream, sem drawer). Duas formas de acessar a mesma API:
>
> - **Chat livre** (drawer) → stream conversacional.
> - **Copiloto embarcado** (hotspots inline) → `useAISuggest()` → JSON síncrono no componente.
>
> Tool names seguem os 15 já definidos na [ADR-005 §4](../adrs/ADR-005-arquitetura-integracao-ia-chat-openai.md). Se precisar de tool nova → atualizar a ADR, não inventar à solta.

### 3.3 Explicabilidade — obrigatória

Para cada campo sugerido por IA no card Metadados da tela 13, exigir um tooltip/popover com:

- o valor extraído,
- a fonte (padrão aplicado, OpenAI, parser determinístico),
- a confiança (%),
- botão "Por que esta sugestão?" que abre o drawer pré-preenchido com `explain_extraction(document_id, field)`.

Isso destrava o uso de IA como copiloto auditável — que é o que o Chico pediu em [prompts.md:460](../../.github/prompts.md).

### 3.4 Promoção entre ambientes assistida por IA

O Chico pediu explicitamente: _"IA ajudando a promover entre ambientes, a revisar o que vai ser promovido, a dar feedback sobre o que vai ser promovido"_ ([prompts.md:464](../../.github/prompts.md)).

Proposta: em Settings → Avançado → "Promoção entre ambientes", botão "Revisar com IA" que:

- lista o que está em staging pronto para produção (migrations, padrões aprovados, seeds);
- IA resume as diferenças e aponta riscos (tool call `summarize_promotion_diff`);
- decisão final continua humana.

Fora de escopo deste ciclo se o time julgar pesado — mas **não pode** ficar sem ticket.

---

## 4. Decisões arquiteturais que o time tem que tomar **antes** de codar

~~Não aceito "a gente decide no meio". Tragam resposta escrita no refino de resposta:~~

> **✏️ Verônica (2026-04-21):** as 7 decisões abaixo foram **todas respondidas** no refino de [2026-04-21 10:36](../refinos/2026-04/2026-04-21-10-36-refino-alinhamento-design-telas-plano-acao.md). Mantive o texto original e acrescentei o veredito final do time sob cada item.

### 4.1 Roteamento de documento

- `/dashboard/document` e `/dashboard/statement-doc` do design são **ilustrativos**. Rota real recomendada:
  - `/dashboard/documents/[id]` (server component) resolve `source_document.document_type`.
  - Se `type ∈ {credit_card_statement, bank_statement}` → renderiza `<DocumentDetailView variant="statement" />`.
  - Caso contrário → `<DocumentDetailView variant="generic" />`.
- ~~**Decidir:** a distinção é apenas visual (mesmo componente, dois layouts) **ou** são dois componentes com um shell comum? Eu voto no shell comum + dois paineis laterais distintos. Mas o time decide e justifica.~~
- **✅ DECIDIDO (Ana Silva + Roberto Lima):** shell único `DocumentPageShell` com discriminação server-side por `document_type`, prop `variant: "generic" | "statement"`. Coluna direita varia; header, breadcrumb, preview PDF e metadados base são compartilhados.

### 4.2 Unificação de `documents` vs `source_documents`

- ~~**Decidir:** depreca `documents` legada agora ou em migration futura? Se há dado histórico, plano de migração obrigatório.~~
- Toda a UI nova aponta para `source_documents`. **Não codar em cima da legada.**
- **✅ DECIDIDO (André Santos + João Pereira):** migration de dados obrigatória antes da mudança de UI. Lê `documents`, cria registro em `source_documents` com `origin = 'manual_upload'`, `status = 'processed'`, e `ingestion_job` sintético `completed`. Tabela `documents` é renomeada para `documents_legacy` e mantida por **30 dias** para rollback; drop agendado após validação.

### 4.3 Split / rateio

- ~~**Decidir:** tabela `document_splits` ou `draft_splits`? Chave: vincular ao documento, não ao draft? (Documento é permanente; draft pode ser recriado.) Eu voto em `document_splits(source_document_id, category_id, tags[], amount)`.~~
- Regras: soma dos splits = valor total do documento (validação server-side, UI mostra progresso).
- **✅ DECIDIDO (André Santos + Ricardo Monteiro):** `document_splits(id, source_document_id, category_id, tags UUID[], amount, description, created_at)` com `CHECK (amount > 0)` e **trigger server-side** validando `SUM(splits) ≤ document.amount`. FK em `source_documents`, não em `draft_records`.

### 4.4 Vínculo documento ↔ transação

- ~~**Decidir:** tabela associativa `document_transactions(source_document_id, transaction_id, link_type, confidence, created_by)` onde `link_type ∈ {payment, refund, installment}` e `created_by ∈ {user, ai, pattern}`.~~
- **✅ DECIDIDO (André Santos + Maria Oliveira):** `document_transactions(id, source_document_id, transaction_id, link_type, confidence, created_by, created_at)` — `link_type ∈ {payment, refund, installment, support}` (adicionado `support` para documento-anexo sem ser pagamento), `created_by ∈ {user, ai, pattern}`, **RLS obrigatória**, indexes em `(source_document_id)` e `(transaction_id)`.

### 4.5 PDF preview

- ~~**Decidir:** `pdf.js` via `react-pdf` ou iframe para URL assinado? `react-pdf` dá paginação/zoom nativos mas adiciona ~200 KB no bundle. Iframe é mais leve mas limita controles.~~
- Eu voto em `react-pdf` para tela 13/14 (onde preview é central) e iframe simples para tela 12 (hover preview).
- **✅ DECIDIDO (Roberto Lima + Fernando Gomes + Thiago Martins):** `react-pdf` via `next/dynamic` (lazy load) para telas 13/14; iframe simples para hover-preview na tela 12. URLs assinadas do Supabase Storage confirmadas sem problema de CORS.

### 4.6 Chat contextual

- ~~**Decidir:** como o drawer aprende a rota atual? Via `usePathname()` + regex de rota? Ou via `ChatContext` React context provider que cada página popula? Eu voto no context — dá tipagem forte.~~
- **✅ DECIDIDO (Roberto Lima + Sofia Almeida):** `ChatContext` provider no `dashboard/layout.tsx` com interface `ChatContextValue` tipada. Páginas injetam contexto via `useChatContext().setContext({ documentId, documentType, ... })`. `AIChatDrawer` lê e injeta no `body` do `useChat()`.

### 4.7 Sidebar — como mover "Fornecedores"

- Mudança trivial em [`app-sidebar.tsx`](../../apps/web/src/components/app-sidebar.tsx).
- ~~**Decidir:** rotas técnicas de ingestão — remover do sidebar hoje (release 1) e mover para Settings/Avançado **na mesma PR**. Sem deixar meia-entrega.~~
- **✅ DECIDIDO (Carlos Mendes + Renata Silva):** reorganização numa única PR. Redirects 308 explícitos: `/dashboard/ingestion` → `/dashboard/documents`, `/dashboard/ingestion/documents` → `/dashboard/documents`, `/dashboard/ingestion/review` → `/dashboard/settings#avancado`. Sub-seção "Avançado / Pipeline" criada em Configurações.

---

## 5. O que entra neste ciclo, o que fica fora

### Entra (obrigatório)

1. Reorganização do sidebar (Fornecedores → Financeiro; ingestion técnica → Settings/Avançado).
2. Unificação de `/dashboard/documents` sobre `source_documents`.
3. Tela 13 (`DocumentDetailView variant="generic"`) com preview PDF + metadados + registros relacionados + rateios.
4. Tela 14 (`variant="statement"`) com preview PDF + resumo fatura + lançamentos detectados.
5. Tabelas/endpoints para: `document_splits`, `document_transactions`, edição de metadados.
6. Hotspots de IA inline em tela 13 e 14 (sugestão de fornecedor, categoria, rateio, conciliação — **usando as 15 tools já previstas na ADR-005**; não inventar tool nova sem atualizar a ADR).
7. Contexto de rota no chat drawer.
8. Explicabilidade de campos sugeridos por IA (tooltip + "Por que esta sugestão?").

### Fica fora deste ciclo (criar tickets para depois)

- Mobile (confirmado em [.github/prompts.md:24](../../.github/prompts.md)).
- Relatórios completos (tela 11 continua placeholder).
- Promoção entre ambientes assistida por IA (ticket, não implementação agora).
- Preview avançado (OCR inline, highlight de campos extraídos no PDF).
- Modo offline / PWA.

---

## 6. Critérios de aceite — como o Chico testa em staging

O time só pode fechar este ciclo quando o Chico conseguir fazer, em staging, **todos** os passos abaixo:

1. **Login** funciona; sidebar mostra Fornecedores em Financeiro; Gestão tem apenas Documentos, Importar, Relatórios.
2. Navegar em `/dashboard/documents` e ver **uma única lista** com todos os documentos (Gmail + upload manual + chat), filtráveis.
3. Fazer upload manual de um PDF de nota fiscal. O sistema cria `source_document`, executa pipeline, volta em menos de 10s com metadados extraídos.
4. Abrir o documento e ver a **tela 13** — preview navegável, metadados, botão editar, botão "+ Vincular transação", botão "+ Adicionar rateio". Todos funcionando.
5. Receber sugestão de IA (chip ou "IA sugere") em pelo menos 1 campo de metadados. Clicar em "Por que esta sugestão?" e obter explicação no drawer.
6. Fazer upload (ou puxar do Gmail) uma **fatura de cartão**. Ao clicar em Documentos → abrir o documento, cai na **tela 14**.
7. Na tela 14: ver progress bar de conciliação, editar categoria inline de um lançamento, conciliar com IA um lançamento marcado "Novo", aprovar em lote 3 lançamentos via checkbox.
8. Após aprovação, as transações aparecem em `/dashboard/transactions` e afetam a Visão Geral.
9. Abrir o chat drawer enquanto está na tela 13 — o chat já sabe qual documento está aberto (contexto da rota) e responde perguntas sobre ele.
10. Pedir ao chat: "crie um padrão a partir deste documento" — a IA abre confirmação, salva em `document_patterns` via tool call, próximo documento similar é extraído com mais precisão.

Se qualquer um desses dez itens falhar, **o ciclo não está fechado**.

---

## 7. Princípios não-negociáveis (eu repito porque vocês esquecem)

1. **Nunca dois componentes quase iguais.** Refatora com `variant` ou composition; não copia-e-cola.
2. **Nunca IA gravando sem confirmação** ([ADR-005 §3](../adrs/ADR-005-arquitetura-integracao-ia-chat-openai.md)).
3. **Nunca UI sobre tabela legada quando existe tabela canônica.** Deprecar `documents` em favor de `source_documents`.
4. **Nunca feature flag para esconder bug.** Se a tela 14 está quebrada, corrige. Não flagueia.
5. **Nunca mais uma rota técnica no sidebar de um usuário final.** O Chico não é o operador — é o usuário. Operador é Configurações.
6. **Nunca pull request gigante.** Cada tela (12, 13, 14) = uma PR. Migration = PR separada. Refatoração do sidebar = PR separada.
7. **Commits: conventional** (`feat`, `fix`, `docs`, `refactor`, `migration`), lowercase, sem sentence-case ([prompts.md:634-651](../../.github/prompts.md)).
8. **DDD na camada de domínio em [packages/domain](../../packages/domain).** Novos conceitos (`DocumentSplit`, `DocumentTransactionLink`) vivem lá, não em `apps/web`. Server actions em `apps/web` só orquestram.

---

## 8. O que eu quero de volta do time

~~Entrega esperada **antes de abrir uma linha de código**:~~

> **✏️ Verônica (2026-04-21):** entrega **recebida e aceita** no refino de [2026-04-21 10:36](../refinos/2026-04/2026-04-21-10-36-refino-alinhamento-design-telas-plano-acao.md). Veredito por item:

1. ~~**Resposta escrita** às 7 decisões do §4, cada uma com uma linha de justificativa e o nome de quem decidiu.~~ → **✅ Entregue** (§4 acima espelha o veredito, cada item nominado).
2. ~~**Quebra em tickets** no checklist vivo ([2026-04-01-implementacao-plano-acao.md](../checklists/2026-04-01-implementacao-plano-acao.md)) — **sem criar novo checklist**, conforme instrução do Chico em [prompts.md:492-494](../../.github/prompts.md). Numerar sequencialmente após o último ticket atual.~~ → **⚠️ Parcial:** time listou 33 tickets (S1-001 a S4-009) no refino, mas **ainda não os adicionou** ao checklist `2026-04-01-implementacao-plano-acao.md`. Ana Silva ficou responsável por esse merge até **2026-04-22**. **Ressalva #1 — ver abaixo.**
3. ~~**Mapa de PRs previstas**, nessa ordem:~~ → **✅ Entregue** como **4 sprints** em vez de 7 PRs individuais. Mapeamento: Sprint 1 ≡ (PR 1 + PR 7), Sprint 2 ≡ (PR 3 + migration de dados legacy), Sprint 3 ≡ (PR 2 + PR 4 + PR 5), Sprint 4 ≡ PR 6. Aceito — a agregação por sprint não viola o princípio "sem PR gigante", desde que cada ticket dentro do sprint vire um commit/PR seu.
4. ~~**Estimativa por PR** (S/M/L — sem números fictícios).~~ → **✅ Entregue** em dias (1-2 / 2-3 / 3-4 / 2-3). Aceito como tradução de S/M/L.
5. ~~**Lista do que reaproveita** (ex: `DocumentDetailView` existente) **vs** o que nasce agora.~~ → **✅ Entregue** implicitamente: refatora `DocumentDetailView`, `uploadDocument` action, 15 tools em `lib/ai/tools.ts`; nasce `document_splits`, `document_transactions`, `useAISuggest`, `AIFieldBadge`, `ChatContext`, rota `/dashboard/documents/[id]`.

~~Não aceito resposta genérica. Cada item numerado. Cada decisão nomeada. Cada PR com owner.~~

~~Quando isso estiver pronto, eu libero para codar.~~

---

## 9. Liberação para codar — 2026-04-21 (Verônica)

> **✅ LIBERADO.** Começar pelo **Sprint 1** conforme cronograma do time. Abaixo, as três ressalvas **não-bloqueantes** que precisam ser tratadas durante a execução:

### Ressalva #1 — Merge dos 33 tickets no checklist vivo

Os tickets S1-001 … S4-009 **têm que** ser transcritos para [docs/checklists/2026-04-01-implementacao-plano-acao.md](../checklists/2026-04-01-implementacao-plano-acao.md), numerados sequencialmente após o último item existente, antes do primeiro commit do Sprint 1. O CEO pediu explicitamente **um checklist único** em [prompts.md:492-494](../../.github/prompts.md) — o refino do time é ata de decisão, não substitui o checklist. **Owner:** Ana Silva. **Prazo:** 2026-04-22. **Bloqueia:** primeiro `git push` do Sprint 1.

### Ressalva #2 — Ordenação entre Sprint 2 e Sprint 3

O ticket **S2-005** ("linhas da lista linkam para `/dashboard/documents/[id]`") aponta para uma rota que **só nasce em S3-006** (Sprint 3). Se a ordem for estrita, o usuário do staging terá 404 durante a janela entre Sprint 2 e Sprint 3. **Correção exigida:** uma de duas opções:

- (a) Em S2-005, linkar temporariamente para `/dashboard/ingestion/documents/[id]` (rota técnica ainda viva) e trocar o destino em S3-006; **ou**
- (b) Antecipar um skeleton de `/dashboard/documents/[id]/page.tsx` (server component que apenas carrega e mostra metadados crus) para dentro do Sprint 2, e enriquecer na S3-006/007/008.
  Eu voto na (b) — dá continuidade ao usuário em staging desde o fim do Sprint 2. **Owner:** João Pereira. **Decidir até início do Sprint 2.**

### Ressalva #3 — Progress bar da tela 14 server-side

Ricardo Monteiro sinalizou que `count(reconciliation_status='reconciled') / count(*)` tem que ser computado no servidor. **Reforço:** quero esse cálculo numa **view ou função Postgres**, não num SELECT ad-hoc na server action. `vw_statement_reconciliation_progress(batch_id)` ou `fn_reconciliation_progress(batch_id)` — assim o MCP e o chat conseguem consumir a mesma fonte. **Owner:** André Santos. **Ticket:** adicionar antes do S3-009.

---

### Lembretes que valem repetir (para não escorregar)

- Tickets **S1-005 e S1-006** plantam `ChatContext` no Sprint 1 mas só são consumidos no Sprint 4 — **não removam** por "parece não ter uso". É scaffolding intencional.
- O hook `useAISuggest` **não abre drawer**. O drawer continua para conversa livre. Não misturar os dois caminhos.
- `documents_legacy` não é drop automático — só sai depois de 30 dias e depois de o CEO confirmar em staging que nenhum documento foi perdido.
- Toda tool de IA nova → atualizar [ADR-005](../adrs/ADR-005-arquitetura-integracao-ia-chat-openai.md). Não inventar tool escondida.
- `DocumentSplit` e `DocumentTransactionLink` como conceitos de domínio vão em [packages/domain](../../packages/domain). Server actions orquestram, não acomodam regra de negócio.
- Commits conventional (`feat`, `fix`, `docs`, `refactor`, `migration`), lowercase ([prompts.md:634-651](../../.github/prompts.md)).

---

**Time: podem codar. Eu volto no fim do Sprint 1 para auditar.**

— Verônica
