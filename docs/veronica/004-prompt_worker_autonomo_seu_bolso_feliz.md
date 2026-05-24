# Prompt para agente de código — Worker autônomo de ingestão financeira do Seu Bolso Feliz

> Use este prompt no agente de código conectado ao repositório `ChicoFigueiredo/Seu-Bolso-Feliz`, branch `main`.
> Objetivo: transformar o pipeline atual em um worker meticuloso, autônomo, auditável e seguro para varrer Gmail, varrer pasta local indicada pelo usuário, popular Supabase e preparar registros financeiros revisáveis.

---

## 0. Papel do agente

Você é o agente de implementação responsável por fechar o ciclo operacional de ingestão financeira do projeto **Seu Bolso Feliz**.

Seu trabalho não é criar uma arquitetura paralela nem começar outro produto. Seu trabalho é aproveitar o que já existe na `main` e evoluir o worker para que ele consiga:

1. varrer Gmail por label, query e período;
2. varrer diretórios locais indicados pelo usuário;
3. subir arquivos/evidências para Supabase Storage;
4. criar e manter registros em Supabase;
5. extrair texto e metadados de documentos/e-mails;
6. deduzir intenção financeira;
7. detectar duplicatas por conteúdo e por identidade financeira;
8. gerar drafts revisáveis;
9. permitir aprovação humana;
10. materializar registros financeiros reais depois da aprovação;
11. manter trilha de auditoria e logs claros.

O resultado esperado é que o Supabase seja povoado de forma consistente, idempotente e útil para o dashboard financeiro.

---

## 1. Contexto real do repositório

A `main` já contém uma base relevante. Antes de implementar, leia os arquivos abaixo:

### Workers existentes

- `workers/ingestion/src/index.ts`
- `workers/ingestion/src/processor.ts`
- `workers/ingestion/src/parsers/parse-orchestrator.ts`
- `workers/ingestion/src/parsers/text-extractor.ts`
- `workers/ingestion/src/drafts/draft-generator.ts`
- `workers/ingestion/src/reconciliation/reconciliation.ts`
- `workers/gmail-scanner/src/index.ts`
- `workers/gmail-scanner/src/message-processor.ts`
- `workers/local-scanner/src/index.ts`
- `workers/local-scanner/src/scanner.ts`

### MCP existente

- `apps/mcp-server/src/index.ts`
- `apps/mcp-server/src/tools/scan-local-folder.ts`
- `apps/mcp-server/src/tools/ingest-document.ts`
- `apps/mcp-server/src/tools/approve-draft-batch.ts`
- `apps/mcp-server/src/tools/list-draft-batches.ts`
- `apps/mcp-server/src/tools/reprocess-document.ts`

### UI e ações relacionadas

- `apps/web/src/app/actions/ingestion.ts`
- `apps/web/src/app/dashboard/documents/page.tsx`
- `apps/web/src/app/dashboard/documents/[id]/page.tsx`
- `apps/web/src/app/dashboard/ingestion/page.tsx`
- `apps/web/src/components/document-detail-new.tsx`
- `apps/web/src/components/document-upload-dnd.tsx`

### Banco e migrations

- `supabase/migrations/20260323120200_create_ingestion_tables.sql`
- `supabase/migrations/20260325220000_add_content_hash_dedup.sql`
- `supabase/migrations/20260403100000_create_document_patterns.sql`
- `supabase/migrations/20260403110000_add_reconciliation_to_drafts.sql`
- `supabase/migrations/20260421140800_add_source_document_to_draft_batches.sql`
- `supabase/migrations/20260421141000_add_metadata_to_source_documents.sql`
- `supabase/migrations/20260421141100_add_source_document_id_to_transactions.sql`
- `supabase/migrations/20260503102100_add_ai_enrichment_columns.sql`

### Pacotes internos

- `packages/ingestion-types`
- `packages/operations`
- `packages/domain`
- `packages/shared-types`
- `packages/validation`

---

## 2. Bloqueador de segurança obrigatório antes de evoluir

Antes de qualquer feature, faça uma auditoria de segredos.

Foi identificado que o PR mergeado para a `main` incluiu arquivos de ambiente reais, como:

- `.env.production`
- `.env.staging`

Faça obrigatoriamente:

1. remover arquivos de ambiente reais do repositório;
2. substituir por `.env.example`, `.env.production.example` e `.env.staging.example`, sem valores reais;
3. garantir que `.gitignore` bloqueia `.env`, `.env.*`, exceto arquivos `*.example`;
4. criar um commit específico para a limpeza;
5. registrar no relatório final que as chaves expostas precisam ser rotacionadas no Supabase e nos provedores correspondentes;
6. não imprimir, copiar, repetir ou documentar nenhum valor sensível encontrado.
7. manter `.env` e `.env.local` sem registros no index, para que cada desenvolvedor possa configurar localmente sem risco de vazamento no git.

Critério de aceite:

- `git status` limpo;
- nenhum arquivo `.env.production` ou `.env.staging` real versionado;
- exemplos sem segredo real;
- nenhum segredo novo introduzido;
- relatório indica necessidade de rotação dos segredos já expostos.

---

## 3. Restrições obrigatórias

### Não fazer

- Não criar outro fluxo paralelo fora de `source_documents`, `ingestion_runs`, `ingestion_jobs`, `parsed_document_versions`, `extraction_results`, `draft_batches` e `draft_records`, salvo migrations pequenas e justificadas.
- Não depender exclusivamente de IA.
- Não materializar registros financeiros automaticamente sem revisão humana nesta fase.
- Não gravar secrets no frontend.
- Não usar `SUPABASE_SERVICE_ROLE_KEY` no cliente web.
- Não criar VPS como requisito obrigatório.
- Não quebrar RLS.
- Não esconder erro em `console.log` sem persistir em `ingestion_logs`.
- Não criar documentação extensa para substituir implementação.

### Fazer

- Reaproveitar ao máximo os workers atuais.
- Manter TypeScript strict.
- Adicionar testes automatizados para deduplicação, classificação e materialização.
- Usar Supabase como fonte operacional central.
- Gerar logs auditáveis.
- Implementar idempotência em todas as entradas.
- Fazer o sistema funcionar primeiro localmente, com caminho claro para cron/serverless depois.

---

## 4. Problemas concretos que você deve corrigir

### 4.1 Scanner MCP local com possível inconsistência de coluna

Em `apps/mcp-server/src/tools/scan-local-folder.ts`, revise o insert em `source_documents`.

A migration usa `file_size_bytes`, mas a tool parece usar `file_size`.

Corrija para a coluna real e alinhe com `workers/local-scanner/src/scanner.ts`.

Critério:

- scan local via MCP cria `source_documents.file_size_bytes`;
- não há erro silencioso;
- teste cobrindo esse caso.

### 4.2 Aprovação de draft ainda não materializa registro financeiro real

Em `apps/web/src/app/actions/ingestion.ts`, `approveDraftRecord` e `approveDraftBatch` parecem aprovar status de draft, mas não necessariamente criam/atualizam o registro financeiro final.

Implemente uma camada explícita:

```ts
materializeApprovedDraftRecord(draftRecordId: string): Promise<MaterializationResult>
materializeApprovedDraftBatch(batchId: string): Promise<MaterializationBatchResult>
```

A aprovação deve:

1. validar `draft_data`;
2. identificar `draft_type`;
3. criar ou atualizar registro financeiro real;
4. preencher `posted_record_id`;
5. preencher `posted_record_type`;
6. mudar draft para `posted` ou `approved` conforme política definida;
7. gravar `audit_logs`;
8. vincular `source_document_id` ao registro criado quando a tabela suportar;
9. atualizar contadores de batch;
10. retornar resultado detalhado.

Mapeamento mínimo:

| `draft_type`         | Materialização esperada                                                           |
| -------------------- | --------------------------------------------------------------------------------- |
| `transaction`        | cria/atualiza `transactions`                                                      |
| `recurring_template` | cria/atualiza `recurring_templates` e possivelmente `recurring_instances`         |
| `liability`          | cria/atualiza `liabilities` e, quando houver parcelas, `liability_installments`   |
| `consumption_metric` | cria/atualiza `consumption_metrics`, ligado a fornecedor/contrato quando possível |

Se não houver dados suficientes, não materialize. Marque como pendente/corrigido e gere erro de validação claro.

### 4.3 Estados da worker e CHECK constraints

Audite se os enums TypeScript de `packages/ingestion-types` batem com os `CHECK` constraints das migrations.

Atenção especial a estados usados pelo código, como:

- `ai_lite_enriching`
- `pending_review`
- `posted`
- `failed`

Se houver estado usado no código que não está permitido no banco, criar migration corretiva.

Critério:

- worker não falha por status inválido;
- tests cobrem transições da state machine;
- migration idempotente.

### 4.4 Gmail scanner só cobre anexos

O Gmail scanner atual deve evoluir para processar também:

- corpo do e-mail em texto;
- corpo HTML convertido para texto;
- assunto;
- remetente;
- data;
- thread;
- labels;
- anexos.

E-mail sem anexo também pode ser evidência financeira.

Exemplos:

- "Sua fatura Nubank vence amanhã"
- "Pagamento confirmado"
- "Boleto disponível"
- "Compra aprovada"
- "Débito automático agendado"
- "Comprovante Pix"

---

## 5. Arquitetura alvo: Financial Evidence Worker

Crie ou consolide um worker operacional com o seguinte espírito:

```text
source adapters
  -> evidence envelope
  -> storage + source_documents
  -> hashing + dedup
  -> text extraction
  -> deterministic financial inference
  -> optional AI enrichment
  -> reconciliation
  -> draft generation
  -> human review
  -> materialization after approval
```

Você pode criar um novo worker fino, por exemplo:

```text
workers/financial-evidence-worker/
```

Mas ele deve reaproveitar os módulos existentes, não duplicar tudo.

Alternativa aceitável: evoluir `workers/ingestion`, `workers/gmail-scanner` e `workers/local-scanner` com um orquestrador comum.

Decida com pragmatismo e documente brevemente no relatório final.

---

## 6. Interface de adapter obrigatória

Crie uma interface comum para fontes:

```ts
export type SourceKind =
  | "gmail_label"
  | "gmail_query"
  | "gmail_period"
  | "local_directory"
  | "manual_upload"
  | "chat_upload";

export interface SourceEvent {
  sourceKind: SourceKind;
  originType: "gmail" | "local_file" | "manual_upload";
  originKey: string;
  userId: string;
  occurredAt?: string;
  metadata: Record<string, unknown>;
}

export interface RawEvidenceBundle {
  event: SourceEvent;
  primaryText?: string;
  htmlText?: string;
  subject?: string;
  from?: string;
  to?: string;
  attachments: Array<{
    filename: string;
    mimeType: string;
    sizeBytes: number;
    content: Uint8Array;
    contentHash?: string;
  }>;
  rawPayload?: Record<string, unknown>;
}

export interface SourceAdapter {
  discover(): Promise<SourceEvent[]>;
  fetch(event: SourceEvent): Promise<RawEvidenceBundle>;
}
```

Critérios:

- Gmail label implementado;
- Gmail query implementado;
- Gmail período implementado;
- local directory implementado;
- scan manual/once implementado;
- modo loop/watch implementado com intervalo configurável.

---

## 7. Evidence Envelope

Crie uma estrutura persistível para unificar evidência de e-mail/documento/anexo.

Pode começar em JSONB dentro de `source_documents.metadata` e `parsed_document_versions.structured_data`, mas se ficar melhor, crie migration para tabela própria.

Estrutura mínima:

```ts
export interface EvidenceEnvelope {
  schemaVersion: "1.0";
  userId: string;
  sourceDocumentId: string;
  originType: "gmail" | "local_file" | "manual_upload";
  originKey: string;

  email?: {
    messageId?: string;
    threadId?: string;
    from?: string;
    to?: string;
    subject?: string;
    date?: string;
    labels?: string[];
    bodyText?: string;
  };

  file?: {
    filename: string;
    mimeType?: string;
    sizeBytes?: number;
    storagePath?: string;
    contentHash?: string;
  };

  extractedText?: {
    textHash?: string;
    textLength: number;
    extractionMethod: string;
    ocrApplied: boolean;
  };

  fieldCandidates: {
    supplierName?: FieldCandidate[];
    supplierCnpj?: FieldCandidate[];
    totalAmount?: FieldCandidate[];
    dueDate?: FieldCandidate[];
    paymentDate?: FieldCandidate[];
    competenceDate?: FieldCandidate[];
    documentNumber?: FieldCandidate[];
    barcodeDigitableLine?: FieldCandidate[];
    cardLast4?: FieldCandidate[];
    financialProductHint?: FieldCandidate[];
  };

  financialInference?: {
    intent: FinancialIntent;
    confidence: number;
    reasons: string[];
    missingFields: string[];
  };

  identity?: {
    financialIdentityKey?: string;
    contentHash?: string;
    canonicalFingerprint?: string;
    textHash?: string;
  };

  reconciliation?: {
    status: string;
    candidates: ReconciliationCandidate[];
  };
}

export interface FieldCandidate {
  value: unknown;
  confidence: number;
  source:
    | "gmail_subject"
    | "gmail_body"
    | "attachment_text"
    | "pdf_native"
    | "ocr"
    | "boleto_utils"
    | "regex"
    | "supplier_template"
    | "ai_lite"
    | "ai_full"
    | "manual";
  reason?: string;
}
```

---

## 8. Financial Intent Classifier determinístico

Implemente um classificador determinístico antes da IA.

```ts
export type FinancialIntent =
  | "bill_to_pay"
  | "bill_reminder"
  | "invoice_statement"
  | "bank_statement"
  | "payment_receipt"
  | "payment_confirmation"
  | "transaction_history"
  | "contract_or_debt"
  | "recurring_charge"
  | "unknown";
```

Heurísticas mínimas:

### `bill_to_pay`

Sinais:

- boleto;
- linha digitável;
- código de barras;
- vencimento futuro;
- valor total;
- expressões como "pague até", "vencimento", "boleto disponível".

### `bill_reminder`

Sinais:

- e-mail de lembrete;
- assunto/corpo com "sua fatura vence", "lembrete de pagamento", "vence amanhã";
- pode não ter anexo;
- geralmente valor + vencimento + instituição.

### `invoice_statement`

Sinais:

- fatura de cartão;
- ciclo;
- fechamento;
- vencimento;
- total da fatura;
- itens de fatura;
- termos como "fatura", "cartão", "total da fatura", "melhor dia de compra".

### `bank_statement`

Sinais:

- extrato;
- saldo;
- lista de lançamentos;
- OFX/CSV;
- múltiplas transações.

### `payment_receipt` / `payment_confirmation`

Sinais:

- "pagamento realizado";
- "pagamento confirmado";
- "comprovante";
- Pix;
- autenticação bancária;
- data de pagamento;
- valor pago.

### `contract_or_debt`

Sinais:

- contrato;
- financiamento;
- empréstimo;
- parcela;
- juros;
- amortização;
- CET;
- saldo devedor.

Critérios:

- retornar `intent`, `confidence`, `reasons`, `missingFields`;
- nunca inventar campo inexistente;
- se tiver pouca confiança, retornar `unknown` e `missingFields`.

---

## 9. Financial Identity Key

Implemente chave semântica para deduplicar lembretes e documentos diferentes que se referem à mesma obrigação financeira.

```ts
export function buildFinancialIdentityKey(input: {
  userId: string;
  intent: FinancialIntent;
  supplierName?: string | null;
  supplierCnpj?: string | null;
  institutionName?: string | null;
  financialProductHint?: string | null;
  amount?: number | null;
  dueDate?: string | null;
  competenceDate?: string | null;
  cycleStartDate?: string | null;
  cycleEndDate?: string | null;
  documentNumber?: string | null;
  barcodeDigitableLine?: string | null;
  cardLast4?: string | null;
}): string | null;
```

Regras:

1. se houver linha digitável/código de barras válido, usar como sinal forte;
2. se houver número de documento + fornecedor + vencimento, usar como sinal forte;
3. para fatura de cartão, usar instituição/produto + vencimento + valor + ciclo ou últimos 4 dígitos;
4. para lembrete de fatura sem anexo, usar instituição + valor + vencimento + tipo;
5. normalizar nomes;
6. arredondar valor para centavos;
7. não gerar chave se campos mínimos estiverem ausentes;
8. armazenar a chave em metadata e/ou tabela dedicada.

Critério de aceite:

- dois e-mails Nubank/C6 sobre a mesma fatura não devem criar duas obrigações/drafts independentes;
- devem virar evidências vinculadas ao mesmo item financeiro provável;
- o sistema deve registrar reasons da deduplicação.

---

## 10. Modelo opcional recomendado: `financial_obligations`

Se o repositório ainda não possui uma entidade adequada para obrigações pendentes genéricas, crie migration pequena e segura.

Tabela sugerida:

```sql
CREATE TABLE financial_obligations (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,

  obligation_type text NOT NULL CHECK (obligation_type IN (
    'bill_to_pay',
    'bill_reminder',
    'invoice_statement',
    'recurring_charge',
    'liability_installment',
    'unknown'
  )),

  status text NOT NULL DEFAULT 'open' CHECK (status IN (
    'open',
    'pending_review',
    'approved',
    'paid',
    'cancelled',
    'duplicate',
    'rejected'
  )),

  supplier_id uuid REFERENCES suppliers(id) ON DELETE SET NULL,
  supplier_name_raw text,
  financial_product_id uuid REFERENCES financial_products(id) ON DELETE SET NULL,

  amount numeric(15,2),
  due_date date,
  competence_date date,
  cycle_start_date date,
  cycle_end_date date,

  document_number text,
  barcode_digitable_line text,
  financial_identity_key text,

  confidence_score numeric(3,2),
  metadata jsonb DEFAULT '{}',

  created_at timestamptz NOT NULL DEFAULT now(),
  updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_financial_obligations_identity
  ON financial_obligations(user_id, financial_identity_key)
  WHERE financial_identity_key IS NOT NULL;
```

Tabela de vínculo:

```sql
CREATE TABLE financial_obligation_evidences (
  id uuid PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id uuid NOT NULL REFERENCES auth.users(id) ON DELETE CASCADE,
  obligation_id uuid NOT NULL REFERENCES financial_obligations(id) ON DELETE CASCADE,
  source_document_id uuid NOT NULL REFERENCES source_documents(id) ON DELETE CASCADE,
  evidence_role text NOT NULL DEFAULT 'supporting',
  confidence_score numeric(3,2),
  reasons jsonb DEFAULT '[]',
  created_at timestamptz NOT NULL DEFAULT now(),
  UNIQUE(user_id, obligation_id, source_document_id)
);
```

RLS obrigatório.

Se decidir não criar essa tabela, justifique no relatório final e implemente agrupamento equivalente usando tabelas existentes.

---

## 11. Gmail autônomo

### 11.1 CLI desejada

Implemente comandos ou scripts equivalentes:

```bash
bun run worker:gmail --label Comprovantes --limit 50 --dry-run
bun run worker:gmail --query 'from:(nubank OR c6bank OR caixa) newer_than:180d' --limit 200
bun run worker:gmail --from-date 2026-01-01 --to-date 2026-05-24 --include-body
bun run worker:gmail --label Comprovantes --include-body --include-attachments --process
```

Opções mínimas:

- `--label`
- `--query`
- `--from-date`
- `--to-date`
- `--limit`
- `--batch-size`
- `--dry-run`
- `--include-body`
- `--include-attachments`
- `--process`
- `--ai-mode auto|skip|lite|full`
- `--user-id`
- `--checkpoint-key`
- `--max-pages`
- `--verbose`

### 11.2 Comportamento

Para cada mensagem:

1. listar mensagens;
2. obter metadados;
3. extrair assunto/remetente/data/labels/thread;
4. extrair corpo texto e HTML convertido;
5. localizar anexos aceitos;
6. criar `source_documents` para anexos;
7. criar também evidência para corpo de e-mail quando ele tiver sinal financeiro;
8. calcular hashes;
9. deduplicar por:
   - Gmail message id + attachment id;
   - content hash;
   - text hash;
   - financial identity key;
10. criar `ingestion_runs`;
11. criar `ingestion_jobs`;
12. opcionalmente processar imediatamente.

### 11.3 Corpo do e-mail sem anexo

Quando o e-mail tiver sinal financeiro mas não tiver anexo:

- criar um `source_document` com `origin_type='gmail'`;
- `filename` pode ser algo como `gmail-message-<messageId>.txt`;
- salvar o corpo em Supabase Storage como `.txt` ou `.json`;
- `mime_type='text/plain'` ou `application/json`;
- preencher metadata com subject/from/date/thread/labels;
- criar job normalmente.

---

## 12. Pasta local autônoma

### 12.1 CLI desejada

```bash
bun run worker:local --dir "/caminho/da/pasta" --scan-once
bun run worker:local --dir "/caminho/da/pasta" --recursive --process
bun run worker:local --dir "/caminho/da/pasta" --watch --interval-ms 30000
bun run worker:local --dir "/caminho/da/pasta" --dry-run
```

Opções mínimas:

- `--dir`
- `--recursive`
- `--scan-once`
- `--watch`
- `--interval-ms`
- `--extensions`
- `--dry-run`
- `--process`
- `--ai-mode auto|skip|lite|full`
- `--user-id`
- `--move-processed-to`
- `--verbose`

### 12.2 Regras

1. calcular hash de conteúdo antes de upload quando possível;
2. deduplicar por path+mtime+size e por hash;
3. subir para `ingestion-originals`;
4. criar `source_documents`;
5. criar `document_fingerprints`;
6. criar `ingestion_jobs`;
7. registrar `ingestion_logs`;
8. se `--process`, rodar o pipeline de ingestão;
9. se `--move-processed-to`, mover somente depois de sucesso;
10. não apagar original por padrão.

---

## 13. Extração de texto e formatos

O upload aceita vários formatos. O worker precisa alinhar tipos aceitos com tipos realmente processáveis.

### Suporte mínimo

- PDF com texto nativo;
- PDF com OCR opcional via `ocrmypdf`;
- imagem como pendente de OCR/IA full quando OCR local não disponível;
- CSV;
- XML;
- OFX;
- XLS/XLSX;
- texto/HTML de e-mail;
- DOCX, se viável com dependência simples e justificada.

### Resultado esperado

Cada extrator deve retornar:

```ts
type TextExtractionResult = {
  text: string;
  pages?: number;
  rows?: number;
  extractionMethod: string;
  ocrApplied: boolean;
  confidence: number;
  warnings: string[];
  structuredHints?: Record<string, unknown>;
};
```

Se um formato for aceito mas não processável, ele deve:

- não quebrar a worker;
- gerar `parsed_document_versions` com `needs_manual_review`;
- gravar log claro;
- aparecer na UI como pendente/erro de extração.

---

## 14. IA como camada construtiva, não como muleta

A IA só deve entrar depois da camada determinística, salvo imagem/PDF escaneado onde não há texto.

### `ai_mode=skip`

- só determinístico;
- sem chamada OpenAI.

### `ai_mode=auto`

- chama IA lite somente se campos críticos faltarem ou confiança baixa;
- chama IA full somente se necessário.

### `ai_mode=lite`

- usa texto já extraído;
- retorna JSON estruturado;
- não usa visão.

### `ai_mode=full`

- permitido para imagem/PDF escaneado ou baixa confiança;
- deve registrar custo/uso em metadata/log;
- deve retornar razões e confidências por campo.

### Schema mínimo da IA

```ts
type AiFinancialExtraction = {
  financial_intent: FinancialIntent;
  supplier_name_raw: string | null;
  supplier_cnpj: string | null;
  total_amount: number | null;
  due_date: string | null;
  payment_date: string | null;
  competence_date: string | null;
  document_number: string | null;
  barcode_digitable_line: string | null;
  card_last4: string | null;
  financial_product_hint: string | null;
  confidence_per_field: Record<string, number>;
  overall_confidence: number;
  reasoning_short: string;
  missing_fields: string[];
  should_require_human_review: boolean;
};
```

Regras:

- validar com Zod;
- se JSON inválido, não quebrar job;
- gravar erro em log;
- nunca inventar valor sem evidência;
- sempre preservar o texto bruto e os candidatos determinísticos.

---

## 15. Reconciliação

Evolua a reconciliação para buscar candidatos em:

- `transactions`;
- `statement_cycles`;
- `statement_items`;
- `liabilities`;
- `liability_installments`;
- `recurring_templates`;
- `recurring_instances`;
- `draft_records`;
- `financial_obligations`, se criada.

Interface sugerida:

```ts
export interface ReconciliationCandidate {
  targetType:
    | "transaction"
    | "statement_cycle"
    | "statement_item"
    | "liability"
    | "liability_installment"
    | "recurring_template"
    | "recurring_instance"
    | "draft_record"
    | "financial_obligation";

  targetId: string;
  score: number;
  reasons: string[];
  blockingConflicts: string[];
}

export interface ReconciliationResult {
  status:
    | "no_match"
    | "weak_match"
    | "strong_match"
    | "duplicate_risk"
    | "same_obligation"
    | "conflict";

  candidates: ReconciliationCandidate[];
  recommendedAction:
    | "create_new"
    | "attach_as_evidence"
    | "update_existing"
    | "manual_review"
    | "reject_duplicate";
}
```

Sinais obrigatórios:

- valor;
- vencimento;
- data de pagamento;
- fornecedor;
- CNPJ;
- linha digitável;
- número de documento;
- instituição/produto;
- últimos 4 dígitos do cartão;
- ciclo de fatura;
- thread/e-mail/remetente;
- histórico de correções humanas.

---

## 16. Materialização após aprovação

O sistema só passa a "valer" financeiramente após aprovação humana.

### 16.1 Aprovar draft individual

Fluxo:

```text
draft pending_review/corrected
  -> validar
  -> reconciliar novamente
  -> se houver conflito bloqueante: impedir materialização
  -> criar/atualizar registro financeiro real
  -> preencher posted_record_id/post_record_type
  -> status posted
  -> audit log
```

### 16.2 Aprovar batch

- processar item a item;
- retornar sucessos e falhas;
- não falhar tudo por causa de um item problemático, salvo se transação atômica for necessária;
- atualizar contadores do batch;
- preservar erros por draft.

### 16.3 Validação

Use Zod em `packages/validation` quando possível.

Cada `draft_type` deve ter schema.

---

## 17. Supabase: povoamento esperado

Ao final, um ciclo real deve popular:

- `ingestion_runs`
- `source_documents`
- `document_fingerprints`
- `ingestion_jobs`
- `ingestion_logs`
- `parsed_document_versions`
- `extraction_results`
- `draft_batches`
- `draft_records`
- `financial_obligations`, se criada
- `financial_obligation_evidences`, se criada
- `transactions`, `statement_cycles`, `statement_items`, `liabilities`, `liability_installments`, `recurring_templates` ou `recurring_instances`, após aprovação/materialização
- `audit_logs`

Critério de aceite:

- o dashboard financeiro precisa refletir registros materializados;
- documentos precisam aparecer na UI;
- drafts precisam ser revisáveis;
- logs precisam explicar cada falha.

---

## 18. Testes obrigatórios

Crie ou atualize testes em:

- `__tests__/domain`
- `__tests__/integration`
- `workers/*/*.test.ts`, se aplicável.

### Casos mínimos

1. **Dedup por hash**
   - mesmo arquivo local duas vezes não cria dois documentos ativos.

2. **Dedup por financial identity**
   - dois e-mails diferentes sobre a mesma fatura Nubank/C6 geram uma mesma obrigação ou o mesmo grupo de evidência.

3. **Gmail sem anexo**
   - e-mail com "sua fatura vence" cria evidência processável.

4. **Gmail com anexo**
   - anexo PDF cria source_document, job, parsed_version, extraction_result e draft.

5. **Local scanner**
   - pasta com PDF/CSV/OFX cria documentos e jobs.

6. **MCP local**
   - `scan_local_folder` usa `file_size_bytes` corretamente.

7. **IA skip**
   - `ai_mode=skip` não chama OpenAI e ainda gera draft quando regex resolve.

8. **IA auto**
   - campos críticos faltantes acionam IA lite/full conforme política.

9. **Aprovação materializa**
   - draft de transaction aprovado cria `transactions` e preenche `posted_record_id`.

10. **Conflito bloqueia**

- se reconciliação retorna duplicata forte, aprovação não cria registro duplicado sem decisão humana.

11. **RLS**

- usuário A não enxerga documentos/jobs/drafts do usuário B.

12. **Segurança**

- nenhum `.env.production` ou `.env.staging` real permanece versionado.

---

## 19. Scripts esperados no `package.json`

Adicione scripts claros, sem quebrar os atuais:

```json
{
  "worker:ingestion": "cd workers/ingestion && bun run src/index.ts",
  "worker:gmail": "cd workers/gmail-scanner && bun run src/index.ts",
  "worker:local": "cd workers/local-scanner && bun run src/index.ts",
  "worker:financial": "bun run workers/financial-evidence-worker/src/index.ts",
  "worker:financial:dry": "bun run workers/financial-evidence-worker/src/index.ts --dry-run",
  "pipeline:scan-gmail": "bun run worker:gmail --label Comprovantes --include-body --include-attachments",
  "pipeline:scan-local": "bun run worker:local --dir ./inbox --scan-once",
  "pipeline:process": "bun run worker:ingestion"
}
```

Se optar por não criar `workers/financial-evidence-worker`, ajuste nomes, mas mantenha comandos equivalentes.

---

## 20. Observabilidade

Todo job deve registrar:

- início;
- fonte;
- arquivo/e-mail;
- hashes calculados;
- duplicata detectada;
- texto extraído;
- parser usado;
- campos encontrados;
- classificação financeira;
- IA usada ou não usada;
- reconciliação;
- drafts gerados;
- erros;
- decisão final.

Use `ingestion_logs`.

Não basta `console.log`.

---

## 21. Relatório final obrigatório do agente

Ao terminar, entregue um relatório em Markdown com:

1. resumo executivo;
2. arquivos alterados;
3. migrations criadas;
4. comandos adicionados;
5. como configurar Gmail;
6. como rodar scanner Gmail;
7. como rodar scanner local;
8. como rodar processamento;
9. como testar na UI;
10. como verificar dados no Supabase;
11. riscos restantes;
12. pendências que exigem ação manual do CEO;
13. confirmação de que segredos foram removidos do repositório;
14. aviso de rotação obrigatória de segredos previamente expostos.

---

## 22. Critérios finais de aceite

A entrega só pode ser considerada boa se:

- `bun install` funciona;
- `bun run lint` passa;
- `bun run typecheck` passa;
- `bun run test` passa;
- Gmail scanner roda em `--dry-run`;
- Gmail scanner cria registros reais quando não está em `--dry-run`;
- local scanner roda em pasta indicada;
- worker processa jobs até `pending_review`;
- UI mostra documentos, status, erro e drafts;
- aprovação cria/atualiza registros financeiros reais;
- deduplicação evita repetição de lembretes da mesma fatura;
- logs explicam o que aconteceu;
- nenhum segredo real permanece versionado;
- o Supabase fica povoado com dados rastreáveis e úteis.

---

## 23. Meta funcional que deve guiar tudo

O teste narrativo principal é este:

> "Eu recebo vários e-mails do C6/Nubank sobre a mesma fatura. O sistema deve perceber que são evidências da mesma obrigação financeira, não criar várias despesas, mostrar isso na UI, permitir minha revisão e, após aprovação, refletir corretamente no meu dashboard financeiro."

Implemente para que esse cenário funcione de ponta a ponta.
