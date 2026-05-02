# Guia Unificado de Operações — CEO (v2, pé no chão)

> Versão focada em execução real no dia a dia.
> Atualizado em 2026-05-02 com base em:
>
> - `docs/checklists/006-esporro-veronica.md`
> - `docs/Veronica/003-alteracoes-novas-telas.md`
> - `docs/refinos/2026-04/2026-04-21-10-36-refino-alinhamento-design-telas-plano-acao.md`

---

## 1) Objetivo desta versão

Este guia é para o CEO validar, sem firula, três coisas:

1. Visualização de documentos (telas 12, 13 e 14).
2. Uso de IA inline + drawer contextual.
3. Interpretação dos documentos (extração, explicabilidade e reconciliação assistida).

Se esses 3 blocos funcionam localmente, a ida para staging fica muito mais previsível.

---

## 2) O que já está entregue no código (contexto atual)

Conforme o checklist de sprint da Verônica:

- Sprint 1 concluído (sidebar + chat context).
- Sprint 2 concluído (unificação em `source_documents`).
- Sprint 3 concluído (telas 13/14 + PDF + splits + vínculos).
- Sprint 4 concluído (IA inline e explicabilidade).

Fonte operacional: `docs/checklists/006-esporro-veronica.md`.

---

## 3) Pré-requisitos mínimos para teste local

Antes de rodar qualquer teste funcional:

1. Docker em execução.
2. Supabase CLI disponível.
3. Bun instalado.
4. Variáveis locais preenchidas (`.env.local`) com URL/keys do Supabase local.
5. Se quiser testar IA real: `OPENAI_API_KEY` válida no `.env.local`.

---

## 4) Subida rápida do ambiente local

### 4.1 Banco local

```bash
npx supabase start
npx supabase status
```

### 4.2 Dependências e app web

```bash
bun install
bun run dev
```

App web: `http://localhost:3105`

### 4.3 Worker de ingestão (segundo terminal)

```bash
cd workers/ingestion
bun run src/index.ts
```

---

## 5) Roteiro objetivo de validação local (ordem recomendada)

## Etapa A — Tela 12 (lista unificada de documentos)

1. Abrir `/dashboard/documents`.
2. Fazer upload manual de um PDF de teste.
3. Confirmar que o documento aparece na lista com status de pipeline.
4. Testar filtros (status, origem e busca por nome).
5. Testar exclusão em dois cenários:
   - `Só documento`: o item sai da lista e o arquivo original deixa de existir, mas a trilha de ingestão é preservada.
   - `Documento + ingestão`: o item sai da lista e os artefatos de ingestão vinculados são removidos junto.

**Aceite:** documento novo aparece em até ~10s e abre detalhe ao clicar.

## Etapa B — Tela 13 (documento genérico)

1. Na lista, abrir um documento do tipo nota/recibo (não fatura de cartão).
2. Verificar preview PDF (paginação e zoom).
3. Verificar card de metadados e edição.
4. Testar:
   - vincular transação;
   - adicionar rateio.

**Aceite:** preview funcional + edição de metadados + ações de vínculo/rateio operando.

## Etapa C — Tela 14 (fatura/cartão)

1. Abrir documento de fatura.
2. Confirmar `variant="statement"` (resumo da fatura + tabela de drafts).
3. Conferir progress bar de conciliação.
4. Testar seleção em lote e aprovação em lote.

**Aceite:** progress bar e drafts carregam; ações em lote funcionam sem quebrar a tela.

## Etapa D — IA inline (foco Verônica)

No fluxo das telas:

1. Em metadados com baixa confiança, validar `AIFieldBadge`.
2. Clicar em “Por que?” e confirmar abertura do drawer contextual.
3. Na lista de documentos com baixa confiança, validar botão “Explicar”.
4. Em rateio, validar botão “IA sugere rateio”.
5. Em conciliação, validar:
   - “Sugerir via IA” (tela 13);
   - “Conciliar com IA” por draft não aprovado (tela 14).

**Aceite:** sugestões retornam, usuário decide manualmente, sem gravação automática indevida.

## Etapa E — Fornecedores com IA (CNPJ)

1. Abrir `/dashboard/suppliers/new`.
2. Preencher CNPJ/CPF e sair do campo.
3. Validar pré-preenchimento assistido de nome/nome fantasia (quando houver sugestão).

**Aceite:** fluxo de sugestão funciona e continua editável pelo usuário.

---

## 6) Checklist de evidências para homologação

Salvar evidências de cada rodada local:

- IDs dos documentos testados.
- Prints da tela 12, 13 e 14.
- Print de `AIFieldBadge` + explicação no drawer.
- Print de conciliação com IA.
- Print de sugestão por CNPJ.
- Resultado de `bun run typecheck` e `bun run test:unit`.

Essas evidências devem acompanhar o MR quando a rodada for para staging.

---

## 7) Pendências que ainda dependem de ação manual do CEO

Mesmo com código pronto, há bloqueadores externos:

1. Google OAuth no Supabase Staging.
2. API key OpenAI (com limite de gasto configurado).
3. Env vars completas no Vercel (Preview/Production).
4. Domínio final no Vercel + DNS.

Referências detalhadas:

- `docs/passo-a-passo/002-configuracoes-manuais-ingestao-automacao-mcp.md`
- `docs/passo-a-passo/003-proximas-acoes-manuais-ceo.md`
- `docs/passo-a-passo/007-guia-operacional-ceo.md`

---

## 8) Critério de saída local (go/no-go para staging)

Pode avançar para staging quando os itens abaixo estiverem todos verdes:

1. Upload manual aparece na tela 12.
2. Tela 13 abre e mantém preview + edição.
3. Tela 14 abre com progress bar e ações de draft.
4. IA inline responde (explicar/sugerir/conciliar/rateio).
5. Sugestão por CNPJ funciona em fornecedor.
6. Exclusão em ambos os modos funciona sem quebrar jobs/logs/drafts.
7. Typecheck e testes unitários passam.

Se qualquer item falhar, corrigir local antes de abrir nova rodada em staging.
