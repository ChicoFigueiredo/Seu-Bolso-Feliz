---
Título da Reunião: Estratégia de parsing híbrido para boletos e documentos — IA lite + full sob demanda
Data e Hora: 2026-05-03 10:21
Participantes:
  - CEO (Chico) — facilitador / solicitante
  - Ana Silva (Arquiteta de Software) — decisão de arquitetura
  - João Pereira (Backend Sênior) — implementação do worker
  - Maria Oliveira (Backend Sênior) — IA e integração OpenAI
  - Ricardo Monteiro (Economista / Consultor Financeiro) — validação semântica dos campos extraídos
  - Pedro Santos (Backend Python/Django) — análise de custo/token e batch
  - Thiago Martins (Front Engineer) — UI de revisão e observabilidade
  - André Santos (DBA Sênior) — persistência e modelo de dados
Pauta:
  - Entender o estado atual do parsing de boletos (o que extrai, o que persiste, limitações)
  - Avaliar a proposta de modelo lite barato + full sob demanda
  - Definir a arquitetura híbrida: determinístico → IA lite → IA full
  - Definir o "autopilot único" que faz tudo e devolve decisão explicada
  - Determinar critérios de aceite, backlog e próximos passos
---

## Diagnóstico — Estado atual do parsing de boletos

### O que o sistema extrai hoje (parser determinístico)

O parser de boleto genérico (`workers/ingestion/src/parsers/boleto-parser.ts`) usa regex pura e extrai:

| Campo                  | Como extrai                                             | Limitação                                 |
| ---------------------- | ------------------------------------------------------- | ----------------------------------------- |
| `totalAmount`          | regex em "VALOR DO DOCUMENTO", "VALOR A PAGAR", "TOTAL" | Pode falhar em PDFs com layout não-padrão |
| `dueDate`              | regex em "VENCIMENTO", "DATA DE VENCIMENTO"             | Formato DD/MM/YYYY obrigatório            |
| `competenceDate`       | regex em "REFERÊNCIA", "COMPETÊNCIA", "PERÍODO"         | Às vezes ausente no boleto                |
| `supplierCnpj`         | regex em "CNPJ:"                                        | Depende da label explícita                |
| `supplierNameRaw`      | regex em "CEDENTE:", "BENEFICIÁRIO:"                    | Frequentemente ausente em boletos simples |
| `documentNumber`       | regex em "NÚMERO DO DOCUMENTO", "NOSSO NÚMERO"          | Pode não estar no texto extraído          |
| `barcodeDigitableLine` | regex de linha digitável (47/48 dígitos)                | Ausente em PDFs escaneados/imagem         |

**Confiança calculada:** proporcional ao número de campos extraídos com sucesso (0 a 1.0, máx com 5 hits).

### O que vai para o banco

Dois registros complementares são criados por pipeline:

1. **`parsed_document_versions`** — versão bruta + estruturada:
   - `raw_text`: texto completo extraído do PDF
   - `structured_data`: resultado do parser (todos os campos acima em JSON)
   - `confidence_score`: confiança geral
   - `parser_type`: `local_regex` ou `local_text`

2. **`extraction_results`** — campos normalizados para uso da IA e UI:
   - `supplier_name_raw`, `supplier_id`, `due_date`, `total_amount`
   - `competence_date`, `document_number`, `contract_identifier`
   - `consumption_data` (kWh, m³ — para contas de energia/água)
   - `category_suggestion`, `tags_suggestion`, `priority_suggestion`
   - `breakdown` (itens detalhados da fatura)

3. **`draft_records`** — lançamentos propostos para revisão:
   - Gerado a partir do `extraction_result`
   - Tipos: `transaction`, `recurring_template`, `consumption_metric`, `liability`
   - Contém `draft_data` (JSONB completo), `confidence_score`, `status`

### Como a intenção é reconhecida hoje

A "intenção financeira" é heurística, não semântica:

- Se há `totalAmount` ou `dueDate` → draft tipo `transaction`
- Se há `supplierNameRaw` e `competenceDate` → draft tipo `recurring_template`
- Se há `consumption_data.kwh` → draft tipo `consumption_metric`
- Categoria por nome de fornecedor: CEMIG → energia, COPASA → água, VIVO → telecomunicações
- Prioridade: sempre `"alta"` no MVP (hardcoded)
- Tags: sempre `["essencial", "moradia"]` + tag específica por fornecedor

### Limitações críticas

1. **Imagem não extrai texto** — retorna vazio no MVP. OCR não implementado.
2. **PDF escaneado** — mesmo que PDF, se for imagem embutida, não extrai.
3. **PDFs com layout não-padrão** — regex falha silenciosamente.
4. **Fornecedor frequentemente ausente** — muitos boletos não têm "CEDENTE:" explícito.
5. **Intenção financeira é cega** — não distingue boleto recorrente de pontual, passivo de despesa corrente.
6. **Sem score por campo** — confiança é um número único, não campo a campo.
7. **Sem explicação** — UI não mostra o porquê de cada campo ter sido extraído ou não.

---

## Discussão

### Ana Silva (Arquiteta de Software)

A arquitetura atual é correta no modelo de persistência. O problema não é onde guardamos, é o que e como extraímos. Precisamos manter a separação entre camada determinística e camada IA. A proposta de três camadas é a certa para o nosso tamanho:

- **Camada 1** (determinística, sempre ligada): regex + padrões documentais
- **Camada 2** (IA lite, sob trigger): só quando confiança ou completude ficam abaixo do limiar
- **Camada 3** (IA full / visão, sob demanda): imagens, PDFs escaneados, ambiguidade grave

Isso nos mantém controláveis em custo e escala, sem abandonar o parser que já funciona bem para documentos limpos.

### João Pereira (Backend Sênior)

Do lado do worker, o flow já está em state machine. É natural encaixar as três camadas como steps intermediários após o `parsed`:

```
queued → parsing → parsed → ai_lite_enrichment (novo) → classified → reconciled → drafted → pending_review
```

O step `ai_lite_enrichment` só roda se `confidence < THRESHOLD` ou campos críticos faltando. Custo médio previsto com GPT-4o-mini: ~300-500 tokens por boleto simples (texto condensado + prompt fixo), o que dá ~$0.003–$0.005 por documento no pior caso.

Para o modo full (visão), ativamos somente via flag `needs_full_ai_review` ou por ação manual na UI. Custo previsto: ~1.500–4.000 tokens com imagem.

### Maria Oliveira (Backend Sênior — IA)

A chave do modo lite é o prompt estruturado com schema fixo de saída. Não mandar o texto bruto inteiro — mandar só o trecho relevante. Sugestão de estrutura de entrada para a IA lite:

```json
{
  "document_type_hint": "boleto",
  "extracted_so_far": {
    "total_amount": null,
    "due_date": "2026-03-10",
    "supplier_name_raw": null,
    "supplier_cnpj": "12.345.678/0001-90"
  },
  "text_excerpt": "<primeiros 800 chars do texto extraído>",
  "fields_missing": ["total_amount", "supplier_name_raw"]
}
```

E saída esperada (JSON Schema rígido):

```json
{
  "total_amount": 189.9,
  "supplier_name_raw": "Canaã Empreendimentos",
  "financial_intent": "despesa_recorrente",
  "confidence_per_field": {
    "total_amount": 0.95,
    "supplier_name_raw": 0.82,
    "financial_intent": 0.78
  },
  "reasoning": "Documento de cobrança com vencimento fixo e cedente identificado pelo CNPJ."
}
```

Isso é processável, auditável e barato.

### Ricardo Monteiro (Economista)

Do ponto de vista financeiro, o campo `financial_intent` é crítico para o sistema não cometer erros semânticos. Exemplos:

- Boleto de condomínio = `despesa_recorrente` (não `passivo`)
- Boleto de parcela de empréstimo = `amortizacao_passivo`
- Boleto de conta de energia = `despesa_recorrente` + `consumo_metrico`
- Boleto de seguro = `despesa_recorrente` (não `receita`)

A IA precisa aprender essa distinção. No modo lite, dá para guiar com o prompt. No modo full, o documento inteiro dá contexto suficiente para a IA decidir. Em ambos, o sistema não deve autoaprovar sem revisão humana nas primeiras semanas.

### Pedro Santos (Backend Python/Django)

Para batch econômico: processar em fila assíncrona, agrupando documentos similares no mesmo contexto quando possível (reduce de tokens por contexto compartilhado). Para sob demanda: trigger explícito na UI ou via MCP tool. O teto de custo por documento deve ser configurável por usuário (`max_ai_cost_per_doc`), com fallback para modo lite ou revisão manual quando excedido.

### Thiago Martins (Front Engineer)

A UI de revisão precisa mostrar explicitamente:

- **Origem de cada campo**: ícone "regex" vs ícone "IA lite" vs ícone "IA full"
- **Confiança por campo** (ex.: badge colorido: verde >80%, amarelo 50-80%, vermelho <50%)
- **Reasoning da IA** (collapsible, para não poluir)
- **Botão "Analisar com IA full"** disponível quando modo lite não resolveu
- **Status do pipeline**: qual etapa está/parou

### André Santos (DBA Sênior)

A model `extraction_results` já comporta os campos necessários. Precisamos de:

1. Coluna `ai_enrichment_type` em `extraction_results` (null / lite / full)
2. Coluna `ai_enrichment_at` em `extraction_results`
3. Coluna `confidence_per_field` JSONB em `extraction_results`
4. Coluna `reasoning` text em `extraction_results`
5. Flag `needs_full_ai_review` em `ingestion_jobs` (ou no metadata — preferir coluna real)

Sem migration nova impossível auditar corretamente.

---

## Prós e Contras

### Opção A — Só determinístico (status quo)

- **Prós:** custo zero de tokens, previsível, sem dependência externa
- **Contras:** falha em ~30-40% dos documentos reais (imagem, layout não-padrão, fornecedor ausente); experiência ruim para o usuário

### Opção B — IA sempre ligada (sem fase determinística)

- **Prós:** maior taxa de acerto; menos manutenção de regex
- **Contras:** custo alto por documento; latência maior; vendor lock-in; desnecessário para documentos limpos

### Opção C — Híbrida 3 camadas (determinístico → lite → full) ✅ APROVADO

- **Prós:** custo mínimo para documentos simples; qualidade alta para difíceis; auditável; extensível; controle de teto de custo
- **Contras:** mais complexidade de implementação; dois prompts para manter; necessita migration de banco

---

## Decisão Final

**Adotar arquitetura híbrida 3 camadas** com as seguintes regras operacionais:

1. **Camada 1 sempre roda** (regex + padrões documentais existentes)
2. **Camada 2 (IA lite) roda quando:**
   - `confidence < 0.6` após camada 1, **ou**
   - campos críticos faltando: `total_amount` **ou** `due_date` **ou** `supplier_name_raw`
   - Entrada: texto condensado + campos já extraídos + schema de saída fixo
   - Modelo sugerido: `gpt-4o-mini` ou equivalente
   - Teto: 500 tokens de entrada + 300 saída por documento
3. **Camada 3 (IA full / visão) roda quando:**
   - flag `needs_full_ai_review = true` após camada 2, **ou**
   - documento é imagem (MIME `image/*`), **ou**
   - ação manual do usuário na UI
   - Envia o arquivo original (base64 ou URL signed) para modelo de visão
   - Modelo sugerido: `gpt-4o` com visão
4. **Autopilot único** exposto como:
   - Server action `ingestDocumentAutopilot(documentId, mode: 'lite' | 'full')`
   - MCP tool `ingest_document` com parâmetro `ai_mode`
   - Retorna: campos extraídos, confiança por campo, intenção, reasoning, pendências

---

## Arquitetura Proposta do Worker

```
queued
  ↓
[STEP 1] download + hash + deduplicação
  ↓
[STEP 2] extração de texto (pdf-parse / fallback vazio para imagem)
  ↓
[STEP 3] parser determinístico (regex + padrões)
         → salva parsed_document_versions
         → salva extraction_results (campos extraídos)
  ↓
[STEP 4] validação de completude
         se confidence >= 0.6 e campos críticos OK → pula step 5
         se não → marca ai_lite_needed = true
  ↓
[STEP 5] IA lite (condicional)
         → enriquece extraction_results
         → atualiza confidence_per_field + reasoning + ai_enrichment_type = 'lite'
         se ainda incompleto → marca needs_full_ai_review = true
  ↓
[STEP 6] IA full (sob demanda / imagem)
         → reprocessa via visão
         → atualiza extraction_results com ai_enrichment_type = 'full'
  ↓
[STEP 7] classificação de intenção financeira
         → financial_intent (transaction / recurring / metric / liability)
  ↓
[STEP 8] reconciliação determinística
  ↓
[STEP 9] geração de drafts
  ↓
pending_review (+ explicação visível na UI)
```

---

## Campos do Autopilot (saída única para o CEO)

```typescript
interface AutopilotResult {
  documentId: string;
  status: "complete" | "needs_review" | "failed";
  extraction: {
    supplier_name: string | null;
    total_amount: number | null;
    due_date: string | null;
    competence_date: string | null;
    financial_intent: "transaction" | "recurring" | "metric" | "liability" | "unknown";
    document_type: string | null;
  };
  confidence: {
    overall: number;
    per_field: Record<string, number>;
  };
  pipeline: {
    parser_used: "local_regex" | "ai_lite" | "ai_full";
    ai_enriched: boolean;
    reasoning: string | null;
  };
  drafts: {
    count: number;
    batch_id: string | null;
    pending_review: string[];
  };
  issues: string[]; // o que ficou sem resolver
  decision_suggestion: "approve" | "review_required" | "reject";
}
```

---

## Migration necessária

```sql
-- Enriquecer extraction_results para auditoria de IA
ALTER TABLE extraction_results
  ADD COLUMN IF NOT EXISTS ai_enrichment_type text,      -- null | 'lite' | 'full'
  ADD COLUMN IF NOT EXISTS ai_enrichment_at timestamptz,
  ADD COLUMN IF NOT EXISTS confidence_per_field jsonb,    -- { field: score }
  ADD COLUMN IF NOT EXISTS reasoning text,
  ADD COLUMN IF NOT EXISTS financial_intent text;         -- 'transaction' | 'recurring' | 'metric' | 'liability'

-- Flag no job para rastrear necessidade de análise full
ALTER TABLE ingestion_jobs
  ADD COLUMN IF NOT EXISTS needs_full_ai_review boolean DEFAULT false;
```

---

## Prompt Lite (template)

```
Você é um parser financeiro especializado em documentos brasileiros.

Documento: boleto/conta
Campos já extraídos (podem estar incorretos ou ausentes):
{{ extracted_so_far }}

Trecho do documento:
"""
{{ text_excerpt }}
"""

Campos que faltam ou têm baixa confiança: {{ fields_missing }}

Responda APENAS com JSON válido, sem texto adicional:
{
  "total_amount": number | null,
  "due_date": "YYYY-MM-DD" | null,
  "supplier_name_raw": string | null,
  "document_type": "boleto" | "conta_energia" | "conta_agua" | "conta_telefone" | "condominio" | "outros",
  "financial_intent": "transaction" | "recurring_expense" | "metric" | "liability_payment" | "unknown",
  "confidence_per_field": { "total_amount": 0.0-1.0, "due_date": 0.0-1.0, "supplier_name_raw": 0.0-1.0 },
  "reasoning": "frase curta explicando a classificação"
}
```

---

## Prós e Contras da Migration

### Migration agora vs. depois

- **Prós de migrar agora:** evita refatoração de extraction_results mais tarde; campos de IA se tornam primeira classe; auditoria completa desde o início
- **Contras de migrar agora:** work extra antes de implementar a IA; risco de quebrar fluxo existente se não feita com cuidado

**Decisão:** migrar junto com implementação do step 4/5 (IA lite), não antes. Usar `IF NOT EXISTS` para idempotência.

---

## Ações / Responsáveis / Prazo

| #   | Ação                                                                       | Responsável                 | Prazo      |
| --- | -------------------------------------------------------------------------- | --------------------------- | ---------- |
| 1   | Migration: adicionar colunas em `extraction_results` e `ingestion_jobs`    | André Santos + João Pereira | 2026-05-06 |
| 2   | Implementar step de validação de completude no worker                      | João Pereira                | 2026-05-07 |
| 3   | Implementar `ai-lite-enricher.ts` no worker com prompt template            | Maria Oliveira              | 2026-05-09 |
| 4   | Integrar step IA lite na state machine do processor                        | João Pereira                | 2026-05-10 |
| 5   | Implementar `ai-full-enricher.ts` (visão) para imagem/PDF escaneado        | Maria Oliveira              | 2026-05-14 |
| 6   | Criar server action `ingestDocumentAutopilot`                              | João Pereira                | 2026-05-12 |
| 7   | UI: card de campo com badge de confiança e origem (regex/IA)               | Thiago Martins              | 2026-05-13 |
| 8   | UI: botão "Analisar com IA full" no detalhe do documento                   | Thiago Martins              | 2026-05-13 |
| 9   | MCP tool `ingest_document` com parâmetro `ai_mode`                         | João Pereira                | 2026-05-15 |
| 10  | Testes: cobertura do step lite e full com docs reais                       | Maria Oliveira              | 2026-05-16 |
| 11  | Validação financeira dos campos extraídos (intenção vs. tipo de documento) | Ricardo Monteiro            | 2026-05-17 |

## Fora de escopo agora

- Multiagente sofisticado ou agente autônomo de aprovação
- Treinamento de modelo próprio
- OCR avançado local (Tesseract) — IA full cobre o caso de uso
- Integração com Open Banking / Bacen
- Mobile
- Automação de Gmail por scan em lote nesta fase
