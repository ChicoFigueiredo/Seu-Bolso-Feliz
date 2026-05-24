---
ADR: 006
Título: Padrões Documentais e Aprendizado — Tabelas, Feedback Loop e Auto-desativação
Status: Aceito
Data: 2026-03-31
Participantes:
  - Ana Silva (Arquiteta) — autora
  - João Pereira (Backend) — coautor
  - Maria Oliveira (Backend/Segurança) — revisora
  - André Santos (DBA) — design de tabelas
  - Ricardo Monteiro (Economista) — regras de extração financeira
---

# ADR-006 — Padrões Documentais e Aprendizado

## Contexto

O pipeline de ingestão extrai dados de documentos usando parsers determinísticos (regex). Quando o parser genérico falha ou extrai dados incorretos, o usuário corrige manualmente. Essas correções precisam virar **padrões reutilizáveis** para que o próximo documento do mesmo tipo seja extraído corretamente na primeira tentativa.

Problema atual: correções se perdem. Cada documento similar requer a mesma correção manual.

## Decisão

### 1. Tabela `document_patterns`

Armazena padrões de extração aprendidos a partir de correções do usuário.

```sql
CREATE TABLE document_patterns (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES auth.users(id),
  name TEXT NOT NULL,                           -- Ex: "Fatura CEMIG"
  supplier_id UUID REFERENCES suppliers(id),    -- Fornecedor associado
  document_type TEXT NOT NULL,                  -- Ex: "utility_bill", "credit_card_statement"
  institution_id UUID REFERENCES institutions(id),
  extraction_rules JSONB NOT NULL DEFAULT '{}', -- Regras de extração: campos, posições, regex
  field_mappings JSONB NOT NULL DEFAULT '{}',   -- Mapeamento: campo_extraído → campo_destino
  sample_fingerprints TEXT[] DEFAULT '{}',      -- Hashes de documentos-modelo
  confidence_threshold NUMERIC(3,2) DEFAULT 0.80, -- Mínimo para auto-sugestão
  version INT NOT NULL DEFAULT 1,
  is_active BOOLEAN DEFAULT true,
  feedback_count INT DEFAULT 0,                 -- Quantas vezes foi corrigido
  success_count INT DEFAULT 0,                  -- Quantas vezes acertou
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now(),
  UNIQUE(user_id, name, version)
);

ALTER TABLE document_patterns ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users manage own patterns"
  ON document_patterns FOR ALL
  USING (auth.uid() = user_id);
```

**Mudanças em relação à versão anterior do ADR:**

- `supplier_match` (regex) → `supplier_id` (FK real para `suppliers`): integridade referencial
- `content_match` removido: match será feito via `extraction_rules` + `sample_fingerprints`
- `document_type` agora NOT NULL
- Adicionados: `institution_id`, `field_mappings`, `sample_fingerprints`, `confidence_threshold`, `version`, `feedback_count`
- Removidos: `priority`, `failure_count`, `last_used_at` (simplificação MVP)
- UNIQUE constraint `(user_id, name, version)` para versionamento

### 2. Tabela `pattern_feedback`

Registra feedback do usuário sobre cada aplicação de padrão.

```sql
CREATE TABLE pattern_feedback (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID NOT NULL REFERENCES auth.users(id),
  pattern_id UUID NOT NULL REFERENCES document_patterns(id),
  source_document_id UUID REFERENCES source_documents(id),
  feedback_type TEXT NOT NULL CHECK (feedback_type IN ('correct', 'incorrect', 'partial', 'improved')),
  corrections JSONB DEFAULT '{}',               -- O que o CEO corrigiu
  notes TEXT,
  created_at TIMESTAMPTZ DEFAULT now()
);

ALTER TABLE pattern_feedback ENABLE ROW LEVEL SECURITY;
CREATE POLICY "Users manage own feedback"
  ON pattern_feedback FOR ALL
  USING (auth.uid() = user_id);
```

**Mudanças em relação à versão anterior do ADR:**

- `document_id` → `source_document_id`: nome consistente com tabela `source_documents`
- `source_document_id` agora nullable (feedback pode existir sem documento específico)
- Enum expandido: `('success', 'partial', 'failure')` → `('correct', 'incorrect', 'partial', 'improved')`
  - `correct`: padrão acertou na íntegra
  - `incorrect`: padrão errou completamente
  - `partial`: acertou parcialmente
  - `improved`: CEO corrigiu e melhorou o padrão
- `corrections` agora DEFAULT `'{}'` (consistência com refino)

### 3. Fluxo de aplicação

1. **Parser processa documento** → extrai campos
2. **Sistema busca padrões existentes** para o mesmo fornecedor/tipo (`supplier_id` + `document_type`)
3. Se encontra padrão com `is_active = true`:
   - Aplica `extraction_rules` e `field_mappings`
   - Compara resultado com extração bruta
   - Se confiança ≥ `confidence_threshold` → sugere automaticamente
   - Se confiança < threshold → mostra para revisão com indicação "padrão existente com baixa confiança"
4. Se sem match → OpenAI fallback (quando disponível)
5. **CEO revisa e aprova/corrige**
6. Se corrigiu:
   - Cria registro em `pattern_feedback` com tipo `improved`
   - Incrementa `feedback_count` no padrão
   - Se feedback negativo > 3x → marca padrão como `is_active = false` (precisa de revisão)
7. Se aprovou sem correção:
   - Incrementa `success_count`
   - Cria registro em `pattern_feedback` com tipo `correct`

### 4. Auto-desativação de padrões ruins

Regra: se `feedback_count > 3` (feedback negativo: `incorrect` + `improved`) e a taxa de sucesso (`success_count / (success_count + feedback_count)`) for inferior a 50%, o padrão é desativado automaticamente (`is_active = false`).

**Justificativa:** Padrões que geram mais correções do que acertos estão atrapalhando ao invés de ajudar. Desativar automaticamente evita degradação silenciosa.

### 4.1 Versionamento de padrões

Quando o CEO corrige significativamente um padrão, o sistema cria uma nova versão (incrementa `version`, copia `extraction_rules` e `field_mappings`, CEO ajusta). Versões anteriores ficam com `is_active = false` mas permanecem para auditoria. A constraint `UNIQUE(user_id, name, version)` garante unicidade por versão.

### 5. Integração com IA

A IA pode via function calling:

- `list_document_patterns` — listar padrões ativos do usuário
- `register_document_pattern` — criar padrão a partir de uma correção
- `update_pattern` — ajustar `extraction_rules` e `field_mappings` de um padrão existente

A IA **não pode** desativar ou deletar padrões automaticamente.

**Nomes das tools alinhados com o refino principal** (seção "Expansão MCP proposta" e Fase D).

## Alternativas Consideradas

### A. Padrões globais (compartilhados entre usuários)

- **Rejeitado no MVP:** Cada usuário tem fornecedores e formatos diferentes. Padrões globais adicionam complexidade de moderação. Pode ser revisitado no futuro.

### B. Aprendizado automático sem feedback explícito

- **Rejeitado:** Sem feedback, não há como medir qualidade. O loop de feedback é essencial para confiabilidade.

### C. Padrões como prompts de IA (sem tabela estruturada)

- **Rejeitado:** Prompts não são auditáveis, versionáveis ou desativáveis. Tabela estruturada permite controle fino.

## Consequências

### Positivas

- Correções viram conhecimento persistente
- Qualidade da extração melhora com o uso
- Padrões ruins se auto-desativam
- Auditável via `pattern_feedback`
- Versionamento permite evolução sem perda de histórico
- FKs reais (`supplier_id`, `institution_id`) garantem integridade referencial
- `confidence_threshold` permite calibrar quando sugerir automaticamente
- `field_mappings` separa extração de mapeamento para destino
- Extensível para padrões globais no futuro

### Negativas

- Usuário precisa acumular correções para ter padrões úteis (cold start)
- `extraction_rules` e `field_mappings` em JSONB exigem validação cuidadosa
- Múltiplos padrões para o mesmo fornecedor podem conflitar (resolvido via `confidence_threshold` e `version`)

## Referências

- Refino: docs/refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md — Fase D
- ADR-005: Arquitetura de Integração IA (function calling tools)
