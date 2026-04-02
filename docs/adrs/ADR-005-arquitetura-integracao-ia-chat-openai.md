---
ADR: 005
Título: Arquitetura de Integração com IA — Chat como Drawer + Vercel AI SDK + Edge Function Proxy
Status: Aceito
Data: 2026-03-31
Participantes:
  - Ana Silva (Arquiteta) — autora
  - João Pereira (Backend) — coautor
  - Maria Oliveira (Backend/Segurança) — revisora
  - Roberto Lima (Frontend) — revisor
  - Ricardo Monteiro (Economista) — consultor domínio
  - Camila Duarte (Consultora) — validação produto
---

# ADR-005 — Arquitetura de Integração com IA

## Contexto

O sistema precisa de integração com OpenAI (GPT-4o) para:

1. **Chat operacional** — usuario conversa para classificar, aprovar, analisar documentos
2. **Parser fallback** — quando regex/determinístico falha, IA extrai dados do documento
3. **Análise de imagem** — OpenAI Vision para documentos escaneados/fotos
4. **Reconciliação assistida** — sugerir matches entre drafts e transações existentes

A decisão envolve: onde o chat vive na UI, como a API key é protegida, como a IA interage com o domínio, e o que a IA pode ou não fazer.

## Decisão

### 1. Chat como Drawer (não como página dedicada)

O chat IA será um **drawer lateral** acessível de qualquer página do dashboard, não uma rota dedicada.

**Justificativa:**

- O chat é ferramenta contextual — o usuário está olhando um documento e quer perguntar algo sobre ele
- Drawer mantém o contexto visual (PDF, dados do documento) visível enquanto conversa
- Padrão já familiar: Intercom, ChatGPT sidebar, GitHub Copilot Chat
- shadcn/ui Sheet já implementa esse padrão com acessibilidade

### 2. Vercel AI SDK + Edge Function como proxy

```
Browser → useChat() → /api/chat (Route Handler) → Edge Function (Supabase) → OpenAI API
```

- **Vercel AI SDK** (`ai` package) no frontend com `useChat()` para streaming
- **API Route** (`/api/chat`) no Next.js como entry point (valida auth, rate limit)
- **Edge Function** (Supabase) como proxy final para OpenAI (mantém API key no servidor)

**Justificativa:**

- API key nunca toca o browser
- Edge Function permite rate limiting server-side e auditoria centralizada
- Vercel AI SDK resolve streaming, parsing de tool calls, e estado de UI automaticamente
- Se migrar de OpenAI para outro provider, só muda a Edge Function

### 3. IA nunca grava diretamente no ledger

A IA pode:

- ✅ Listar documentos, drafts, transações (read)
- ✅ Sugerir classificação, valores, fornecedor (suggest)
- ✅ Pré-preencher formulário de revisão (prefill)
- ✅ Executar aprovação/rejeição **quando o usuário confirma** (action with confirmation)

A IA **não pode**:

- ❌ Inserir transaction diretamente sem confirmação
- ❌ Deletar registros
- ❌ Alterar saldo diretamente
- ❌ Auto-aprovar drafts sem revisão humana

**Justificativa:**

- Dados financeiros são sensíveis. Alucinação → registro contábil errado
- Modelo "copiloto" vs "autopiloto": IA sugere, humano decide
- Em fase futura, com confiança >98% e histórico de acertos, podemos reavaliar auto-aprovação para padrões conhecidos

### 4. Function Calling com 15 tools específicas

A IA opera via **function calling** (tool_choice: auto), com tools que mapeiam operações reais:

| Grupo   | Tools                                                                                                  |
| ------- | ------------------------------------------------------------------------------------------------------ |
| Leitura | list_pending_documents, get_document_details, list_drafts, get_draft_details, list_recent_transactions |
| Análise | analyze_document_image, extract_document_data, suggest_supplier, suggest_category_tags                 |
| Ação    | approve_draft, reject_draft, reprocess_document, register_pattern                                      |
| Padrão  | list_patterns, suggest_reconciliation                                                                  |

**Justificativa:**

- Function calling é mais confiável que prompt engineering para ações estruturadas
- Cada tool tem schema Zod definido — validação automática
- Tools de ação requerem confirmação explícita antes de executar
- Log de toda tool call para auditoria

### 5. Rate limiting e controle de custo

- 10 mensagens/minuto por usuário
- 100 mensagens/dia por usuário (1.0)
- Limite de tokens por sessão: 50.000 tokens
- Log de custo estimado por mensagem
- Alert se gasto diário > US$ 5

## Alternativas Consideradas

### A. Chat como página dedicada `/dashboard/chat`

- **Rejeitado:** Perde contexto visual. Usuário precisa alternar entre abas.

### B. API key no Next.js Route Handler (sem Edge Function)

- **Rejeitado:** Funciona, mas centralizar na Edge Function permite reuso por MCP e workers futuramente.

### C. IA com auto-aprovação imediata

- **Rejeitado:** Risco muito alto em fase inicial. Revisão humana obrigatória.

### D. Streaming via WebSocket

- **Rejeitado:** Vercel AI SDK resolve com HTTP streaming (SSE). WebSocket adiciona complexidade sem ganho.

## Consequências

### Positivas

- API key segura, nunca exposta
- IA como ferramenta auxiliar, não decisora
- Auditoria completa de toda interação
- Extensível para novos providers e tools
- Custo controlado

### Negativas

- Latência extra por Edge Function proxy (~50ms)
- Dependência de 3 serviços (Vercel, Supabase Edge, OpenAI)
- Custo OpenAI proporcional ao uso (sem cache de respostas por enquanto)

## Referências

- [Vercel AI SDK Docs](https://sdk.vercel.ai/docs)
- [OpenAI Function Calling](https://platform.openai.com/docs/guides/function-calling)
- Refino: docs/refinos/2026-03/2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md — Fase C
