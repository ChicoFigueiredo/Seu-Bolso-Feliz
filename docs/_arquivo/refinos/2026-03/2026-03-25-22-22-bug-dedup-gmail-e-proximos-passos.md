---
Título da Reunião: Bug de Deduplicação no Gmail Scanner + Próximos Passos da Ingestão
Data e Hora: 2026-03-25 22:22
Participantes:
- Francisco "CEO" Figueiredo — facilitador
- Ana Silva (Arquiteta de Software) — revisão técnica
- João Pereira (Backend Sênior - Node/Bun) — implementação scanner
- Maria Oliveira (Backend Sênior - Node/Bun) — segurança e testes
- André Santos (DBA Sênior PostgreSQL) — modelagem e limpeza de dados
- Ricardo Monteiro (Economista/Consultor Financeiro) — validação de domínio
- Fernando Gomes (DevOps Sênior) — pipelines e observabilidade
Pauta:
- Diagnóstico do bug de deduplicação por `attachmentId` efêmero do Gmail
- Proposta de correção: `origin_key` baseado em `messageId:filename`
- Limpeza de duplicatas no banco
- Roadmap dos próximos passos da ingestão
---

## 1. Contexto — O que aconteceu

O primeiro scan real do Gmail Scanner (200 mensagens, label "Comprovantes") foi executado com sucesso em 2026-03-25. Dois bugs de código foram corrigidos na sessão (timestamp RFC 2822 e filename com caracteres especiais). Porém, ao executar o scan múltiplas vezes, descobriu-se que **o sistema está duplicando registros**.

### Dados concretos do banco

| Métrica                                    | Valor                          |
| ------------------------------------------ | ------------------------------ |
| `source_documents` total                   | 125                            |
| Combinações únicas `(messageId, filename)` | 69                             |
| **Duplicatas**                             | **56**                         |
| `ingestion_jobs` total                     | 125 (1:1 com source_documents) |
| Runs executados                            | 4 (1 test + 3 scans de 200)    |

### Exemplo: arquivo `Demonst_IRPF2026_10643.PDF` (4 cópias)

| Run | `gmail_attachment_id` | `created_at` |
| --- | --------------------- | ------------ |
| 1   | `ANGjdJ_NlWsD...`     | 00:20        |
| 2   | `ANGjdJ--l1HD...`     | 00:52        |
| 3   | `ANGjdJ8XqZNl...`     | 00:57        |
| 4   | `ANGjdJ_SBnkH...`     | 01:13        |

O arquivo é o mesmo, o `gmail_message_id` é o mesmo, mas o `attachment_id` muda a cada chamada.

---

## 2. Diagnóstico Técnico — Causa Raiz

### O bug

A chave de deduplicação (`origin_key`) usa o `attachmentId` retornado pela Gmail API:

```typescript
// packages/operations/src/origin-key.ts
return `gmail:${origin.messageId}:${origin.attachmentId}`;
```

**Problema:** O Gmail API retorna `attachmentId` como um token **efêmero** — ele muda a cada chamada da API, mesmo para o mesmo anexo no mesmo e-mail. Portanto, `origin_key` nunca colide e a deduplicação (`isAlreadyProcessed`) retorna `false` em runs subsequentes.

A constraint `UNIQUE(user_id, origin_type, origin_key)` no banco também não ajuda porque a chave é diferente a cada vez.

### A defesa que existia mas não funcionou

```typescript
// workers/gmail-scanner/src/index.ts
async function isAlreadyProcessed(supabase, userId, originKey) {
  const { data } = await supabase
    .from("source_documents")
    .select("id")
    .eq("user_id", userId)
    .eq("origin_type", "gmail")
    .eq("origin_key", originKey) // ← nunca vai bater
    .limit(1);
  return (data?.length ?? 0) > 0;
}
```

---

## 3. Discussão

### Ana Silva (Arquiteta de Software)

> A raiz do problema é arquitetural: estamos usando um identificador externo que não é estável como chave de deduplicação. O `attachmentId` do Gmail é um token de acesso, não um identificador persistente — exatamente como um `download_url` temporário.
>
> A correção correta é usar `messageId + filename` como chave de deduplicação, pois:
>
> 1. O `messageId` é imutável (identificador real da mensagem no Gmail)
> 2. O `filename` é estável dentro de uma mesma mensagem
> 3. A combinação é única por mensagem (o Gmail não permite dois anexos com o mesmo nome no mesmo e-mail)
>
> O `attachmentId` ainda é necessário para **baixar** o arquivo, mas não deve fazer parte da chave de identidade.
>
> Devemos ter cuidado com edge cases: e-mails com múltiplos anexos de mesmo nome (raro mas possível — ex: imagens inline). Sugiro adicionar um índice ordinal como fallback: `gmail:msgId:filename:index`.

### João Pereira (Backend Sênior - Node/Bun)

> Concordo com a Ana. A correção no código é simples:
>
> 1. Mudar `buildOriginKey` para usar `messageId + filename` (ou `messageId + filename + index`)
> 2. Mudar `isAlreadyProcessed` para buscar por `gmail_message_id + filename` em vez de `origin_key` (mais rápido e direto)
> 3. Manter o `gmail_attachment_id` gravado na tabela para uso no download, mas sem participar da chave
>
> Ponto de atenção: o filename pode ter sido sanitizado pelo `sanitizeFilename()` antes de gravar. Precisamos garantir que a comparação use o filename **original** (antes da sanitização), pois é esse que vem da API.
>
> Implementação estimada: ~30 min (código + ajuste no `buildOriginKey` + teste unitário).

### Maria Oliveira (Backend Sênior - Node/Bun)

> A correção do João faz sentido. Adicionalmente, precisamos:
>
> 1. **Criar teste para o cenário de re-run** — rodar scanner com os mesmos dados e verificar que `skippedAlreadyProcessed` é incrementado
> 2. **Teste para o edge case de mesma mensagem com 2 anexos de mesmo nome** — se existir, o índice ordinal é necessário
> 3. **Não quebrar a constraint existente** — se mudarmos o format do `origin_key`, os registros antigos no banco terão formato diferente. Precisamos de uma migration ou tratar ambos os formatos
>
> Sobre segurança: o `attachmentId` efêmero pode conter informações sensíveis (tokens de sessão). Não é um risco de segurança grave, mas é bom não usá-lo como chave persistente.

### André Santos (DBA Sênior PostgreSQL)

> Do lado do banco, recomendo:
>
> 1. **Adicionar constraint parcial** mais robusta:
>    ```sql
>    CREATE UNIQUE INDEX IF NOT EXISTS idx_source_documents_gmail_dedup
>    ON source_documents (user_id, gmail_message_id, filename)
>    WHERE origin_type = 'gmail' AND gmail_message_id IS NOT NULL;
>    ```
>    Isso impede duplicatas a nível de banco mesmo que o código falhe.
> 2. **Limpeza das duplicatas existentes**: manter apenas o registro mais antigo de cada `(messageId, filename)`, deletar os demais + seus `ingestion_jobs` e arquivos no Storage.
> 3. **Não remover a constraint existente** `UNIQUE(user_id, origin_type, origin_key)` — ela protege contra outros tipos de duplicação. Apenas mudar o formato do `origin_key` gerado.
> 4. Para limpeza, o SQL seria:
>    ```sql
>    -- Identificar registros a manter (mais antigo de cada grupo)
>    WITH ranked AS (
>      SELECT id, ROW_NUMBER() OVER (
>        PARTITION BY user_id, gmail_message_id, filename
>        ORDER BY created_at ASC
>      ) as rn
>      FROM source_documents
>      WHERE origin_type = 'gmail'
>    )
>    -- Deletar jobs dos duplicados
>    DELETE FROM ingestion_jobs WHERE source_document_id IN (
>      SELECT id FROM ranked WHERE rn > 1
>    );
>    -- Deletar source_documents duplicados
>    DELETE FROM source_documents WHERE id IN (
>      SELECT id FROM ranked WHERE rn > 1
>    );
>    ```
>
> Impacto esperado: de 125 registros para ~69.

### Ricardo Monteiro (Economista/Consultor Financeiro)

> Do ponto de vista de domínio financeiro, a integridade dos dados é crítica. Se esses documentos duplicados seguirem pelo pipeline de ingestão, podemos ter:
>
> - Faturas contabilizadas em dobro
> - Comprovantes de pagamento duplicados
> - Receitas/despesas infladas nos relatórios
>
> A correção é urgente e deve ser feita **antes** de rodar o worker de ingestão. Os 125 jobs em status `discovered` não devem ser processados até a limpeza.
>
> Recomendo também que, no futuro, o pipeline de ingestão (não só o scanner) tenha uma segunda camada de dedup por `content_hash` — a infraestrutura da tabela `document_fingerprints` já existe para isso.

### Fernando Gomes (DevOps Sênior)

> Para observabilidade, sugiro:
>
> 1. Log claro quando um documento é **skipped por dedup** (já existe `stats.skippedAlreadyProcessed`)
> 2. Adicionar uma métrica de duplicatas detectadas no summary do run
> 3. No futuro, alertar se a taxa de duplicatas por run ficar acima de 5% — pode indicar regressão no mecanismo de dedup
>
> Para o CI: adicionar teste automatizado que simula 2 scans do mesmo e-mail e verifica que nenhuma duplicata é criada.

---

## 4. Prós e Contras das Opções

### Opção A: `origin_key = gmail:messageId:filename`

**Prós:**

- Simples, direto, cobre 99%+ dos casos
- O `messageId` é imutável e o `filename` é estável
- Permite dedup tanto no código quanto via constraint de banco

**Contras:**

- Edge case: 2 anexos com mesmo nome no mesmo e-mail (extremamente raro)
- Se o remetente reprocessar o e-mail com filename diferente, será tratado como novo (correto)

### Opção B: `origin_key = gmail:messageId:filename:index`

**Prós:**

- Resolve o edge case de múltiplos anexos com mesmo nome
- Mais robusto

**Contras:**

- A ordem dos anexos retornada pela API precisa ser consistente (geralmente é, mas não há garantia formal)
- Complexidade adicional marginal

### Opção C: `origin_key = gmail:messageId:attachmentId` (atual)

**Contras:**

- **Não funciona** — `attachmentId` é efêmero
- Comprovado em produção: 56 duplicatas em 4 runs

---

## 5. Decisão Final

### Bug de deduplicação

**Adotar Opção A** (`gmail:messageId:filename`) com constraint de banco adicional:

1. **Alterar `buildOriginKey`** para usar `messageId + filename` para tipo `gmail`
2. **Alterar `isAlreadyProcessed`** para buscar por `gmail_message_id + filename` (mais eficiente que `origin_key`)
3. **Adicionar constraint parcial** `UNIQUE(user_id, gmail_message_id, filename) WHERE origin_type = 'gmail'`
4. **Limpar duplicatas existentes** (manter mais antigo de cada grupo)
5. **Limpar jobs e arquivos no Storage** dos registros removidos
6. **Adicionar testes** para cenário de re-run

**Justificativa:** A opção A é simples, cobre a totalidade dos casos reais (nenhum dos 69 anexos únicos tem colisão de nome no mesmo e-mail), e a constraint de banco serve como safety net. O edge case de múltiplos anexos com mesmo nome pode ser tratado quando/se aparecer.

### Próximos passos da ingestão

Consenso do time: concluir a correção de dedup antes de avançar com o pipeline.

---

## 6. Roadmap — Próximos Passos

### Imediato (antes de processar jobs)

| #   | Ação                                                               | Responsável    | Prioridade |
| --- | ------------------------------------------------------------------ | -------------- | ---------- |
| 1   | Fix `buildOriginKey` para `gmail:msgId:filename`                   | João Pereira   | 🔴 Crítica |
| 2   | Fix `isAlreadyProcessed` para buscar `gmail_message_id + filename` | João Pereira   | 🔴 Crítica |
| 3   | Migration: `UNIQUE INDEX` parcial no banco                         | André Santos   | 🔴 Crítica |
| 4   | Script de limpeza: remover duplicatas + jobs + Storage             | André Santos   | 🔴 Crítica |
| 5   | Testes unitários para dedup e re-run                               | Maria Oliveira | 🔴 Crítica |
| 6   | Re-executar scan (validar que dedup funciona)                      | CEO            | 🟡 Alta    |

### Curto prazo (pipeline de ingestão)

| #   | Ação                                                                       | Responsável      | Prioridade |
| --- | -------------------------------------------------------------------------- | ---------------- | ---------- |
| 7   | Rodar worker de ingestão nos ~69 jobs limpos                               | CEO              | 🟡 Alta    |
| 8   | Validar que drafts são gerados corretamente                                | Ricardo Monteiro | 🟡 Alta    |
| 9   | Implementar content_hash (SHA-256) no scanner para segunda camada de dedup | João Pereira     | 🟢 Média   |
| 10  | Popular `document_fingerprints` durante o hashing                          | Maria Oliveira   | 🟢 Média   |

### Médio prazo (completar pipeline)

| #   | Ação                                                                       | Responsável    | Prioridade |
| --- | -------------------------------------------------------------------------- | -------------- | ---------- |
| 11  | Criar MCP tools para aprovação de drafts                                   | João Pereira   | 🟢 Média   |
| 12  | Dashboard web para visualizar documentos ingeridos                         | Roberto Lima   | 🟢 Média   |
| 13  | Classificação automática de documentos por tipo (fatura, boleto, NF, etc.) | Pedro Santos   | 🟢 Média   |
| 14  | Observabilidade: métricas de scan/ingestão                                 | Fernando Gomes | 🟢 Média   |
| 15  | Scan incremental (só mensagens novas desde último run)                     | João Pereira   | 🟢 Média   |

### Longo prazo (IA e automação)

| #   | Ação                                                    | Responsável                | Prioridade |
| --- | ------------------------------------------------------- | -------------------------- | ---------- |
| 16  | OCR/parsing de PDFs (faturas, boletos)                  | Pedro Santos / Laura Costa | 🔵 Futura  |
| 17  | Classificação automática por IA                         | Pedro Santos               | 🔵 Futura  |
| 18  | Extração de dados financeiros (valor, data, fornecedor) | Ricardo Monteiro + IA      | 🔵 Futura  |
| 19  | Conciliação automática com lançamentos manuais          | Camila Duarte + IA         | 🔵 Futura  |
| 20  | Agente MCP conversacional                               | Ana Silva                  | 🔵 Futura  |

---

## 7. Ações / Responsáveis / Prazo

| Ação                                                 | Responsável  | Prazo                 |
| ---------------------------------------------------- | ------------ | --------------------- |
| Fix `buildOriginKey` + `isAlreadyProcessed` + testes | João / Maria | 2026-03-25 (hoje)     |
| Migration + limpeza de duplicatas                    | André        | 2026-03-25 (hoje)     |
| Re-scan de validação                                 | CEO          | 2026-03-25 (após fix) |
| Worker de ingestão nos dados limpos                  | CEO          | 2026-03-26            |
| Content hash no scanner                              | João         | 2026-03-28            |
| ADR documentando decisão `origin_key`                | Ana          | 2026-03-26            |
