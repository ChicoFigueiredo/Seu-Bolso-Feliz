---
Título da Reunião: Refino do Ciclo de Ingestão, Automação, MCP, Agentes e Povoamento de Dados
Data e Hora: 2026-03-22 23:54
Participantes:
- Chico (CEO) — facilitador, decisão final
- Ana Silva (Arquiteta de Software) — líder técnica, visão sistêmica
- Carlos Mendes (Designer de Software) — UX/UI, visão de produto
- João Pereira (Backend Sênior — Node/Bun) — worker, MCP, runtime
- Maria Oliveira (Backend Sênior — Node/Bun) — segurança, APIs, testes
- Pedro Santos (Backend Sênior — Python/Django) — pipelines, IA
- Laura Costa (Backend Sênior — Python/Django) — DevOps leve, integrações
- Roberto Lima (Frontend Sênior — React/Next) — interface web, revisão
- Sofia Almeida (Frontend Sênior — React/Next) — componentes, UX
- Lucas Ferreira (Mobile Sênior — React Native) — observador, futuro mobile
- Beatriz Rocha (Mobile Sênior — React Native) — observadora
- Fernando Gomes (DevOps Sênior) — infra, CI/CD, ambientes
- Ricardo Monteiro (Economista / Consultor Financeiro) — validação de domínio
- Camila Duarte (Consultora de Finanças Pessoais) — fluxo do usuário
- Gabriela Nunes (Marketing Digital) — observadora
- Helena Vargas (UX/UI Especialista) — fluxo de revisão
- Isabella Torres (UI Designer) — telas de ingestão
- Thiago Martins (Front Engineer) — componentização, performance
- Renata Silva (QA Visual/A11y) — acessibilidade
- André Santos (DBA Sênior PostgreSQL) — modelagem de ingestão, idempotência
Pauta:
- Análise completa do prompt faseado da Verônica
- Cruzamento com estado atual da codebase e documentação
- Discussão e planejamento de cada uma das 8 fases
- Identificação de gaps, riscos e dependências
- Definição de ordem real de implementação
- Atribuição de responsabilidades e critérios de aceite
---

# Ata de Refino — Ciclo de Ingestão, Automação, MCP, Agentes e Povoamento de Dados

## 1. Abertura e Contexto

**Chico (CEO):** Time, a Verônica preparou um prompt faseado extremamente detalhado para o próximo ciclo do projeto. São 8 fases que cobrem desde o fechamento operacional dos ambientes até integração futura com ChatGPT. Antes de partir para implementação, precisamos analisar cada fase, cruzar com o que já temos, identificar gaps e planejar de verdade. Ninguém coda nada hoje — planejamos.

**Ana Silva (Arquiteta):** Li o prompt inteiro. A Verônica acertou na estrutura geral — as fases estão na ordem certa e a separação de responsabilidades entre Vercel, Supabase, Worker e OpenAI é saudável. O que preciso que o time entenda: **o escopo total é grande**. Precisamos ser disciplinados em não tentar atacar tudo ao mesmo tempo.

**Fernando Gomes (DevOps):** Antes de qualquer coisa, preciso reportar o estado real dos ambientes. Hoje temos:
- Pipeline CI/CD no GitLab com 6 stages funcionais para lint, typecheck, test e build
- Deploy web staging e production **são placeholders** — `echo "configurar provedor"`
- Supabase configurado localmente com 11 migrations, 2 Edge Functions
- **Não há Vercel configurado ainda**
- **Não há staging real do Supabase separado de produção**

Ou seja, a Fase 1 da Verônica é **crítica** e precisa acontecer antes de qualquer coisa.

---

## 2. Análise do Estado Atual vs. Requisitos da Verônica

**André Santos (DBA):** Quero apresentar o que já temos de sólido no banco:

### O que já existe (sólido)
- **30+ tabelas** com modelo financeiro completo: institutions, financial_products, cards, categories, tags, transactions, transfers, recurring_templates, recurring_instances, statement_cycles, liabilities, suppliers, supplier_aliases, supplier_contracts, consumption_metrics, documents, user_secrets, import_jobs, audit_logs
- **RLS em todas as tabelas** — cada usuário só vê seus dados
- **Triggers** de unicidade temporal, auto-alias, updated_at
- **~70 índices** incluindo trigram para busca fuzzy
- **pgcrypto** para secrets (encrypt_secret / decrypt_secret)
- **Storage policies** para documents e imports buckets
- **View v_expenses_deduplicated** (ADR-001)
- **RPCs**: search_suppliers (fuzzy), merge_suppliers (atômico)

### O que falta conforme gaps já mapeados
- Materialized view `mv_supplier_spending` — pendente
- Edge Function `refresh-mv-supplier-spending` — pendente
- Confirm atômico na retroactive-supplier-association — pendente
- `seed.sql` vazio — sem dados de teste
- Google OAuth — depende de credenciais do CEO

### O que a Verônica pede de NOVO para ingestão
- `ingestion_runs`, `ingestion_jobs`, `source_documents`, `document_blobs`, `document_fingerprints`, `parsed_document_versions`, `extraction_results`, `draft_records`, `draft_batches`
- Máquina de estados explícita
- Filas no Supabase (Queues)
- Hash/fingerprint duplo (bruto + canônico)

**André (DBA) — Opinião:** A estrutura atual do banco está sólida para o domínio financeiro. A camada de ingestão é **totalmente nova** — precisamos de pelo menos 8-10 tabelas adicionais. A boa notícia é que o modelo existente não precisa ser alterado significativamente; é uma **camada adicional** que referencia o modelo existente.

---

**Maria Oliveira (Backend):** Do lado de pacotes e código:

### Pacotes maduros
- **@sbf/domain** — financial-cycle, amortization, deduplication, priority — 4 módulos testados
- **@sbf/validation** — 18 enums Zod, schemas completos para todas as entidades
- **@sbf/shared-types** — tipos auto-gerados + aliases ergonômicos
- **@sbf/config** — configurações compartilhadas de ESLint, TSConfig, Vitest

### Pacotes vazios/stub
- **@sbf/ui-tokens** — exporta `{}`, nenhum token visual

### Testes
- **168 testes** passando (142 unit + 16 integration + 10 e2e)
- Cobertura sobre domain e validation

### O que falta para ingestão
- Nenhum pacote de ingestão existe
- Nenhuma lógica de hash/fingerprint
- Nenhuma integração com Gmail
- Nenhuma integração com OpenAI
- Nenhum MCP server
- Nenhum worker

---

**Roberto Lima (Frontend):** Na web temos:
- **Dashboard funcional** com 12 rotas
- **8 server actions** com CRUD completo
- **28 componentes shadcn/ui**
- Auth com middleware de sessão
- Layout com sidebar e providers

O que falta: **nenhuma tela de ingestão, revisão de drafts, fila de processamento ou interface de backfill**.

---

## 3. Discussão por Fase

### Fase 1 — Fechamento Operacional e Prontidão de Ambientes

**Fernando Gomes (DevOps):** Esta fase é **bloqueante** para tudo o que vem depois. O estado atual:

| Item | Status | Ação necessária |
|------|--------|-----------------|
| Deploy Vercel | ❌ Não existe | Criar conta, conectar GitLab, configurar |
| Preview deployments | ❌ Não existe | Configurar após Vercel |
| Domínio customizado | ❌ Não existe | Registrar/apontar DNS |
| Environments GitLab | ⚠️ Parcial | Variáveis definidas, falta Vercel |
| Segredos por ambiente | ⚠️ Parcial | Supabase local OK, falta staging/prod |
| Supabase staging | ❌ Não existe | Criar projeto staging no Supabase |
| Supabase production | ❌ Não existe | Criar projeto produção no Supabase |
| Migrations promoção | ⚠️ Parcial | Pipeline tem jobs, falta ambientes reais |
| Gaps CI/CD | ⚠️ Parcial | Deploy web é placeholder |
| Estrutura monorepo | ✅ Parcial | Falta apps/mcp-server e workers/* |

**Fernando:** A estrutura do monorepo que a Verônica propõe está quase lá. A gente já tem `apps/web`, `apps/mobile`, `packages/*`, `supabase/*`. Faltam:
- `apps/mcp-server` — novo
- `workers/` — novo (eu sugiro `workers/ingestion/` para início)

**Ana Silva (Arquiteta):** Concordo com o Fernando. A Fase 1 é pré-requisito absoluto. **Sem ambientes reais, não dá pra validar ingestão com dados reais**. Proponho que a Fase 1 seja tratada como **Sprint 0** — sem ela, nada mais avança.

**João Pereira (Backend):** Pergunta pro CEO: o domínio `seubolsofeliz.com.br` já foi registrado?

**Chico (CEO):** Ainda não. Preciso fazer isso manualmente.

**Fernando:** Então precisamos documentar claramente tudo que o CEO precisa fazer manualmente para a Fase 1. Eu estimo que são pelo menos 10 ações manuais.

#### Decisão Fase 1
- **Aceita conforme proposto pela Verônica**, com ajuste: tratada como Sprint 0 bloqueante
- **Responsáveis:** Fernando (DevOps) + Ana (Arquiteta) + CEO (ações manuais)
- **Dependência externa:** CEO precisa criar contas e configurar credenciais

---

### Fase 2 — Estrutura de Ingestão e Idempotência Documental

**André Santos (DBA):** A Verônica listou as tabelas certas. Minha proposta refinada:

```
ingestion_runs          — cada execução do worker (manual ou agendada)
ingestion_jobs          — cada item dentro de uma run (1 documento/anexo = 1 job)
source_documents        — documento canônico (pode vir do Gmail, local, etc.)
document_fingerprints   — hashes associados a um source_document
parsed_document_versions — cada tentativa de parsing (pode haver N versões)
extraction_results      — resultado estruturado de cada parsing
draft_records           — draft de transação/recorrência/liability para revisão
draft_batches           — agrupamento lógico de drafts para aprovação em lote
```

**André:** Quero destacar a **estratégia de hash duplo** que a Verônica exige:

1. **`content_hash` (SHA-256 dos bytes brutos)** — detecta arquivo exatamente igual
2. **`canonical_fingerprint` (hash do conteúdo extraído normalizado)** — detecta documento semanticamente equivalente mesmo com bytes diferentes (ex: re-download com metadados diferentes)
3. **`origin_key` (chave composta de proveniência)** — gmail_message_id + attachment_id, ou filepath + mtime

Essa tríade permite:
- Detectar duplicata exata → rejeitar
- Detectar duplicata semântica → alertar
- Detectar mesmo arquivo de outra origem → correlacionar
- Forçar reprocessamento → flag explícita `force_reprocess`

**Maria Oliveira (Backend):** A máquina de estados que a Verônica propõe faz sentido. Minha sugestão refinada:

```
discovered → downloaded → hashed → queued_for_parsing → parsing → parsed →
  → classified → reconciled → drafted → pending_review → approved → posted
                                                       → rejected → archived
  (qualquer estado) → failed → retryable | dead_letter
```

Ajustes em relação à proposta original:
- Adicionei `queued_for_parsing` e `parsing` para tracking de fila
- Adicionei `pending_review` como estado explícito antes de `approved`
- Adicionei `rejected` e `archived` como destinos de revisão negativa
- Adicionei `retryable` e `dead_letter` para controle de falhas

**João Pereira (Backend):** Para o worker local, proponho usar **Bun** como runtime. Motivos:
- Já é o runtime do projeto
- Performance excelente para I/O
- Suporta TypeScript nativo
- Pode reutilizar os pacotes @sbf/* diretamente

O worker seria em `workers/ingestion/` e rodaria como processo Bun standalone no modo local.

**Pedro Santos (Backend Python):** Quero levantar um ponto: a Verônica mencionou PDF parsing e extração de dados. Em Python temos um ecossistema muito mais maduro para isso (pdfplumber, tabula, tesseract). Mas como o projeto é TypeScript-first, sugiro avaliar:
- `pdf-parse` (JS) para PDFs simples
- `pdf-lib` para manipulação
- Chamada à OpenAI Vision API para PDFs complexos — que é a direção da Fase 4/7 mesmo

**Ana Silva (Arquiteta):** Concordo com o Pedro. Não vamos construir um parser de PDF do zero — vamos usar a OpenAI como parser semântico e manter o parsing local apenas para extração simples (texto bruto). O parsing pesado fica com a IA na Fase 4.

**Ricardo Monteiro (Economista):** Do ponto de vista financeiro, o que a ingestão precisa capturar de cada documento é:
- **Fornecedor** (nome, CNPJ quando disponível)
- **Competência** (mês de referência do serviço/produto)
- **Vencimento** (data de pagamento)
- **Valor total e breakdown** quando disponível (tarifa, impostos, multa)
- **Unidade consumidora / contrato** (para concessionárias)
- **Número do documento** (código de barras, nosso número)

Isso é o mínimo para gerar um draft útil.

**Camila Duarte (Consultora):** Do ponto de vista do usuário final, o mais importante é que ele **não precise digitar nada**. Se o sistema lê um PDF da conta de luz e sugere: "Fornecedor: CEMIG, Competência: Fev/2026, Vencimento: 15/03/2026, Valor: R$ 287,50" — e o usuário só precisa clicar "Aprovar" ou "Corrigir" — isso já é uma vitória enorme.

#### Decisão Fase 2
- **Aceita conforme proposto**, com refinamentos da equipe na máquina de estados e estratégia de hash
- **Responsáveis:** André (DBA para modelagem), João (worker Bun), Maria (idempotência e testes)
- **Nota:** O worker consome do Supabase Queues ou de polling direto na tabela de jobs. Decisão final de Queues vs. polling será na implementação conforme maturidade do Supabase Queues.

---

### Fase 3 — Integração Inicial com Gmail

**João Pereira (Backend):** Concordo 100% com a decisão da Verônica de **não usar push notifications**. Polling é mais simples, controlável e debugável.

A integração com Gmail requer:
1. **Google Cloud Project** com Gmail API habilitada
2. **OAuth 2.0** com consent screen configurada
3. **Escopos mínimos:** `gmail.readonly` (leitura) — idealmente o mais restrito possível
4. **Redirect URI** para o fluxo OAuth — pode apontar para o app web ou para um handler local
5. **Armazenamento de tokens** — refresh token no Supabase Vault

**Maria Oliveira (Backend):** Ponto de segurança: os tokens OAuth do Gmail são **extremamente sensíveis**. Eles dão acesso ao e-mail do CEO. Proponho:
- Refresh token no Supabase Vault (criptografado)
- Access token efêmero, nunca persistido
- Escopos mínimos (`gmail.readonly`)
- Label-based filtering para minimizar exposição
- Logs de acesso auditáveis

**Fernando Gomes (DevOps):** O CEO vai precisar:
1. Criar projeto no Google Cloud Console
2. Habilitar Gmail API
3. Configurar OAuth consent screen (pode ser "Internal" se for Google Workspace, ou "External" em modo teste)
4. Criar credenciais OAuth (Client ID + Client Secret)
5. Definir redirect URIs
6. Se externo: adicionar e-mail como test user
7. Gerar o primeiro refresh token

Isso precisa estar documentado passo a passo.

**Laura Costa (Backend):** Sugiro que na Fase 3 o worker Gmail seja **separado do worker de ingestão de documentos locais**. São dois sources distintos com ciclos diferentes. O Gmail scanner gera jobs na mesma fila que o scanner local, mas o scan em si é isolado.

**Ana Silva (Arquiteta):** Concordo. Estrutura proposta:
```
workers/
  ingestion/          — core do worker (processa fila de jobs)
  gmail-scanner/      — scan de Gmail, gera jobs na fila
  local-scanner/      — scan de pasta local, gera jobs na fila
```

Cada scanner é um módulo que pode ser invocado independentemente. O worker de ingestão é consumer da fila.

#### Decisão Fase 3
- **Aceita conforme proposto**, com separação de scanners
- **Responsáveis:** João (worker Gmail), Fernando (infra OAuth), CEO (configurações Google Cloud)
- **Decisão:** Começar com polling manual via comando/botão. Agendamento opcional depois.
- **Segurança:** Tokens em Vault, escopos mínimos, auditoria de acesso.

---

### Fase 4 — Parsing Documental, Segredos e Geração de Drafts

**Pedro Santos (Backend):** Esta fase é onde a mágica acontece. O pipeline:

```
Documento → Decifragem (se protegido) → Extração de texto → 
  Parsing semântico (OpenAI) → Estruturação → Geração de draft
```

Para PDFs protegidos por senha, já temos a infraestrutura de `user_secrets` com criptografia pgcrypto. O worker busca a senha associada ao fornecedor/contrato e abre o PDF.

**Ricardo Monteiro (Economista):** Ponto crítico: faturas de concessionárias (luz, água, gás, telefone) têm formatos específicos. Para a CEMIG, por exemplo:
- Unidade consumidora
- Bandeira tarifária
- kWh consumidos
- Histórico de consumo dos últimos 12 meses
- Breakdown: tarifa, ICMS, PIS/COFINS, contribuição de iluminação pública

O draft precisa capturar não só o valor total, mas esses metadados — eles são a `consumption_metric` que já temos modelada no banco.

**Camila Duarte (Consultora):** Concordo com o Ricardo. Para o perfil de usuário hardcore como o CEO, saber "paguei R$ 287" não basta. Ele quer saber "consumo de 450 kWh, com bandeira vermelha, tarifa base R$ 180 + ICMS R$ 60 + taxas R$ 47".

**Ana Silva (Arquiteta):** A estratégia de score de confiança é fundamental. Proponho:
- **Alta confiança (>0.85):** Fornecedor, valor e vencimento claramente identificados → draft pronto para aprovação rápida
- **Média confiança (0.5-0.85):** Alguns campos incertos → draft vai para revisão com campos destacados
- **Baixa confiança (<0.5):** Muita incerteza → vai para fila de revisão manual

**Maria Oliveira (Backend):** Cada versão de extração precisa ser persistida. Se o modelo da OpenAI mudar, ou se a gente melhorar o prompt, precisa poder reprocessar documentos e comparar resultados sem perder o histórico.

#### Decisão Fase 4
- **Aceita conforme proposto**, com score de confiança em 3 faixas
- **Responsáveis:** Pedro (pipeline parsing), Maria (segredos/segurança), Ricardo (validação financeira dos campos)
- **Nota:** O parsing pesado via OpenAI só será integrado na Fase 7. Na Fase 4, o parsing é **local/determinístico** para PDFs simples + extração de texto. A integração com OpenAI vem como upgrade na Fase 7.
- **Ajuste em relação à Verônica:** A Verônica coloca OpenAI como responsável na Fase 4 (seção 4.4), mas achamos mais prudente fazer a Fase 4 com parsing local/regex primeiro, e adicionar IA na Fase 7. Isso permite:
  1. Ter o pipeline funcional sem dependência externa
  2. Ter dados de teste para calibrar os prompts da IA
  3. Popular o banco mesmo antes da IA estar pronta
  - **Justificativa forte:** Se a OpenAI estiver fora do ar ou o custo for proibitivo, o sistema ainda funciona com parsing determinístico básico.

---

### Fase 5 — MCP Local + Copilot/VS Code

**João Pereira (Backend):** O MCP (Model Context Protocol) é essencialmente um servidor de ferramentas que expõe operations do domínio para LLMs consumirem. O VS Code com GitHub Copilot pode se conectar a um MCP local via stdio ou SSE.

Estrutura proposta:
```
apps/mcp-server/
  src/
    index.ts          — entrypoint do servidor MCP
    tools/            — cada tool exportada
      scan-gmail.ts
      scan-local.ts
      list-drafts.ts
      approve-batch.ts
      resolve-supplier.ts
      reprocess-document.ts
      recompute-periods.ts
    lib/
      supabase.ts     — client Supabase do MCP
      auth.ts         — autenticação local
```

**Ana Silva (Arquiteta):** O ponto mais importante do MCP é: **as tools precisam ser a mesma lógica usada pela web e pelo worker**. Não pode haver duplicação. Proponho que as tools do MCP chamem os mesmos serviços/funções que as server actions da web chamam.

Isso significa que precisamos de um pacote compartilhado de **operações de domínio** (acima dos server actions):

```
packages/
  operations/         — lógica de negócio reutilizável
    src/
      scan-gmail.ts
      scan-local.ts
      process-document.ts
      generate-draft.ts
      approve-batch.ts
      resolve-supplier.ts
```

Tanto o MCP, quanto as server actions, quanto o worker chamam esse pacote.

**João:** Excelente ponto. Isso evita que a gente tenha 3 cópias da mesma lógica.

**Thiago Martins (Front Engineer):** Do lado do VS Code, a configuração do MCP é simples. Em `.vscode/mcp.json`:
```json
{
  "servers": {
    "sbf": {
      "command": "bun",
      "args": ["run", "apps/mcp-server/src/index.ts"],
      "env": { "SUPABASE_URL": "...", "SUPABASE_KEY": "..." }
    }
  }
}
```

O Copilot descobre as tools automaticamente e pode invocá-las em chat.

#### Decisão Fase 5
- **Aceita conforme proposto**, com criação de `packages/operations` para compartilhamento
- **Responsáveis:** João (MCP server), Ana (arquitetura de packages/operations), Thiago (integração VS Code)
- **Ajuste:** Criar `packages/operations` como camada de serviços reutilizáveis entre MCP, web e workers
- **Ferramentas mínimas:** Conforme listadas pela Verônica, todas aceitas

---

### Fase 6 — Interface Web de Revisão

**Roberto Lima (Frontend):** Esta é a fase onde o frontend ganha as telas de ingestão. Temos o dashboard existente como base. As novas rotas seriam:

```
/dashboard/ingestion           — visão geral do pipeline
/dashboard/ingestion/runs      — histórico de execuções
/dashboard/ingestion/documents — documentos ingeridos
/dashboard/ingestion/drafts    — drafts pendentes de revisão
/dashboard/ingestion/conflicts — conflitos e duplicidades
```

**Helena Vargas (UX):** O fluxo de revisão é o coração desta fase. O padrão:
1. Usuário vê lista de drafts com score de confiança
2. Ordenação por: urgência (vencimento próximo) → confiança (baixa primeiro) → recência
3. Card do draft mostra: documento original ao lado, campos extraídos, sugestões de fornecedor/categoria/tags
4. Ações: Aprovar, Corrigir, Rejeitar, Vincular manualmente
5. Aprovação em lote para drafts de alta confiança

**Isabella Torres (UI):** Para a tela de revisão, sugiro um layout split-view:
- Lado esquerdo: preview do documento (PDF viewer inline ou imagem)
- Lado direito: formulário com campos extraídos, editáveis

**Carlos Mendes (Designer):** Esse padrão é similar ao que apps de contabilidade profissional usam (tipo Hubdoc, Dext). É um padrão validado no mercado.

**Sofia Almeida (Frontend):** Podemos reutilizar muitos dos componentes shadcn/ui que já temos: tables, dialogs, forms, badges para status. O trabalho novo é o layout de revisão e os componentes de fila.

**Camila Duarte (Consultora):** Do ponto de vista de UX prática: o CEO provavelmente vai ter **centenas** de documentos para processar no backfill inicial. A interface precisa ser otimizada para velocidade: atalhos de teclado, aprovação rápida, navegação entre drafts sem voltar à lista.

**Renata Silva (QA):** Preciso destacar a acessibilidade: o fluxo de revisão precisa funcionar por teclado inteiro. Tab, Enter para aprovar, Esc para rejeitar. Score de confiança precisa ter representação textual, não só visual (cor).

#### Decisão Fase 6
- **Aceita conforme proposto**, com split-view para revisão e atalhos de teclado
- **Responsáveis:** Roberto (rotas e server actions), Helena (UX), Isabella (UI), Sofia (componentes), Renata (A11y)
- **Prioridade:** Fluxo de aprovação em lote para backfill massivo

---

### Fase 7 — Agentes OpenAI

**Pedro Santos (Backend):** Aqui entramos na camada de IA. A proposta da Verônica de separar por agentes especializados é sólida:

1. **Agente de Parsing Documental** — Recebe texto bruto/imagem de documento, retorna dados estruturados
2. **Agente de Resolução de Fornecedor** — Recebe nome/padrão, resolve para fornecedor cadastrado
3. **Agente de Classificação Financeira** — Atribui categoria, tags, prioridade
4. **Agente de Reconciliação** — Compara draft com registros existentes, sugere matches

**Ana Silva (Arquiteta):** A integração com OpenAI será via Responses API. Cada agente é uma configuração de system prompt + tools disponíveis. As tools são as mesmas do pacote `packages/operations`, expostas como function calling.

**Maria Oliveira (Backend):** Cada chamada à OpenAI precisa:
- Registrar input, output, model, tokens usados, custo estimado
- Registrar score de confiança retornado
- Ser auditável e reprodutível
- Não ter side effects diretos — tudo via draft

**Ricardo Monteiro (Economista):** Preciso validar os prompts financeiros. Um agente de classificação precisa entender:
- Que "Nubank" pode ser cartão de crédito OU conta corrente
- Que pagamento de fatura ≠ nova despesa
- Que transferência entre contas próprias ≠ gasto
- Que parcela de empréstimo = amortização + juros + encargos

Esses são os mesmos princípios do copilot-instructions.md — precisam ser injetados nos system prompts.

#### Decisão Fase 7
- **Aceita conforme proposto**, com auditoria completa de chamadas à IA
- **Responsáveis:** Pedro (integração OpenAI), Ricardo (validação de prompts financeiros), Ana (arquitetura de agentes)
- **Nota:** A integração com OpenAI depende de uma API key e definição de budget mensal pelo CEO

---

### Fase 8 — Integração com ChatGPT e Expansão

**Ana Silva (Arquiteta):** A Fase 8 é mais um marco de preparação do que implementação pesada. Se fizermos bem as fases 5-7 (MCP + Agentes), a exposição remota é natural.

**Maria Oliveira (Backend):** A classificação de tools em `read-only`, `write-safe` e `write-risky` é fundamental para segurança. No ChatGPT, um tool `drop_all_data` não pode estar disponível.

**João Pereira (Backend):** A autenticação remota é o maior desafio. Localmente, o MCP herda as credenciais do ambiente. Remotamente, precisamos de API keys ou OAuth do Supabase.

#### Decisão Fase 8
- **Aceita como fase de preparação futura**, sem implementação no ciclo atual
- **Responsáveis:** Ana (documentar requisitos), Maria (modelo de autenticação remota)
- **Nota:** Esta fase é explicitamente de avaliação e documentação, não de implementação

---

## 4. Prós e Contras das Decisões Principais

### Decisão: Parsing local primeiro, IA depois (ajuste Fases 4/7)

**Prós:**
- Sistema funciona sem dependência de API externa
- Dados de teste reais para calibrar prompts da IA
- Custo zero de IA durante desenvolvimento
- Permite validar pipeline inteiro antes de adicionar complexidade

**Contras:**
- Parsing local é menos preciso para documentos complexos
- Pode gerar mais trabalho manual no backfill inicial
- Delay na experiência "mágica" de IA

**Decisão:** Parsing local primeiro, IA na Fase 7. Justificativa: robustez e independência.

---

### Decisão: packages/operations como camada compartilhada

**Prós:**
- Zero duplicação entre MCP, web e workers
- Um lugar para testar toda lógica de negócio
- Facilita evolução sem refatoração

**Contras:**
- Mais um pacote para manter
- Pode criar acoplamento se mal desenhado

**Decisão:** Criar packages/operations. Justificativa: sem isso, teremos duplicação inevitável.

---

### Decisão: Workers separados por tipo de scan

**Prós:**
- Isolamento de responsabilidade
- Pode rodar Gmail scanner sem afetar local scanner
- Evolução independente

**Contras:**
- Mais módulos para manter
- Complexidade de setup local

**Decisão:** Workers separados, consumindo mesma fila. Justificativa: a Verônica exige modularidade.

---

## 5. Riscos e Dependências Identificados

### Riscos

| # | Risco | Impacto | Mitigação |
|---|-------|---------|-----------|
| R1 | CEO não consegue configurar Google Cloud/OAuth | Bloqueia Fase 3 | Documentar passo a passo detalhado |
| R2 | Supabase Queues não funcionar como esperado | Atrasa Fase 2 | Fallback: polling de tabela com lock |
| R3 | Custo de OpenAI exceder budget | Bloqueia Fase 7 | Começar com modelos baratos (gpt-4o-mini), definir budget |
| R4 | PDFs protegidos com formatos não-padrão | Afeta Fase 4 | Fallback: revisão manual |
| R5 | Vercel gratuito ou starter não suportar requirements | Afeta Fase 1 | Avaliar planos e limites antes de começar |
| R6 | GitLab → Vercel integration não ser direta | Atrasa Fase 1 | Alternativa: Vercel CLI no pipeline |
| R7 | Concorrência no worker com múltiplas instâncias | Duplicação de dados | Idempotência por hash + locks |
| R8 | Estouro de timeout em Edge Functions | Afeta Fase 2 | Worker local para processamento pesado (conforme Verônica) |

### Dependências Externas

| Dep | Fase | Responsável | Ação |
|-----|------|-------------|------|
| Conta Vercel | 1 | CEO | Criar conta/organização |
| Projeto Google Cloud | 3 | CEO | Criar e configurar |
| API Key OpenAI | 7 | CEO | Criar conta, budget |
| Domínio seubolsofeliz.com.br | 1 | CEO | Registrar |
| Supabase staging project | 1 | CEO | Criar projeto |
| Supabase production project | 1 | CEO | Criar projeto |

---

## 6. Lista de Módulos/Pastas Novas

```
apps/mcp-server/                — MCP server para VS Code/Copilot (Fase 5)
workers/ingestion/              — Worker core de processamento de fila (Fase 2)
workers/gmail-scanner/          — Scanner de Gmail (Fase 3)
workers/local-scanner/          — Scanner de arquivos locais (Fase 2)
packages/operations/            — Operações de domínio compartilhadas (Fase 2-5)
packages/ingestion-types/       — Tipos e enums da camada de ingestão (Fase 2)
```

---

## 7. Lista de Tabelas/Estruturas Novas

### Tabelas de Ingestão (Fase 2)
- `ingestion_runs` — execuções do worker
- `ingestion_jobs` — jobs individuais de processamento
- `source_documents` — documentos canônicos ingeridos
- `document_fingerprints` — hashes e fingerprints por documento
- `parsed_document_versions` — versões de extração
- `extraction_results` — resultados estruturados
- `draft_records` — drafts de transação/recorrência/liability
- `draft_batches` — agrupamento para aprovação em lote
- `ingestion_logs` — logs e telemetria

### Modificações em Tabelas Existentes
- `documents` — adicionar vínculo com `source_documents` (FK opcional)
- `suppliers` — possivelmente adicionar campos de matching patterns

### Filas (Supabase Queues ou tabela com lock)
- `ingestion_queue` — fila de jobs para processamento

### Buckets Storage (novos ou existentes)
- `ingestion-originals` — anexos originais antes de processamento

---

## 8. Ordem Real de Implementação Proposta

| Sprint | Fase | Duração Est. | Pré-requisitos |
|--------|------|-------------|----------------|
| **Sprint 0** | Gaps pendentes: mv_supplier_spending, seed.sql, confirm atômico | 1-2 dias | Nenhum |
| **Sprint 1** | Fase 1: Ambientes (Vercel + Supabase staging + CI/CD) | 3-5 dias | CEO: contas e credenciais |
| **Sprint 2** | Fase 2: Modelagem de ingestão + worker local + scanner local | 5-7 dias | Sprint 1 concluído |
| **Sprint 3** | Fase 3: Gmail integration | 3-5 dias | Sprint 2 + Google Cloud configurado |
| **Sprint 4** | Fase 4: Parsing local + segredos + drafts | 5-7 dias | Sprint 2-3 concluídos |
| **Sprint 5** | Fase 5: MCP server local | 3-5 dias | Sprint 4 concluído |
| **Sprint 6** | Fase 6: Interface web de revisão | 5-7 dias | Sprint 4-5 concluídos |
| **Sprint 7** | Fase 7: Integração OpenAI | 5-7 dias | Sprint 6 + API key |
| **Sprint 8** | Fase 8: Preparação ChatGPT (documentação) | 2-3 dias | Sprint 7 |

---

## 9. Decisão Final

O time aceita o plano faseado da Verônica com os seguintes ajustes:

1. **Sprint 0** adicionado para fechar gaps pendentes do ciclo anterior
2. **Parsing na Fase 4 começa local/determinístico**, IA entra na Fase 7
3. **packages/operations** criado como camada de serviços compartilhada entre MCP, web e workers
4. **Workers separados** por tipo de scan (Gmail, local), consumindo mesma fila
5. **Fase 8 tratada como documentação**, não implementação

Todos os demais requisitos da Verônica são aceitos integralmente.

---

## Ações / Responsáveis / Prazo

| Ação | Responsável | Prazo |
|------|-------------|-------|
| Criar documentos de planejamento detalhado | Ana + Time | 2026-03-22 |
| Criar checklist operacional do CEO | Fernando | 2026-03-22 |
| Criar passo-a-passo de configurações | Fernando + João | 2026-03-22 |
| Criar guias de aprendizado (Vercel, MCP, OpenAI) | Time completo | 2026-03-22 |
| Fechar gaps pendentes (Sprint 0) | André + Maria | 2026-03-23 |
| Iniciar Fase 1 após ações manuais do CEO | Fernando + Ana | A definir |
