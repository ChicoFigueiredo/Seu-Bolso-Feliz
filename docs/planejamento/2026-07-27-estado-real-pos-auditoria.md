# Diagnóstico do estado real — pós-auditoria

> Entregável §16.1 do plano mestre. Data: 2026-07-27. Branch: `feat/api-key`.
>
> Este documento registra o que a auditoria **encontrou**. O mapa vivo do estado
> atual é [`docs/specs/00-estado-real.md`](../specs/00-estado-real.md), atualizado a
> cada commit que muda o estado. Este aqui é o retrato do "antes", preservado para
> que a diferença fique rastreável.

## Resumo franco

O pipeline de ingestão funcionava até gerar drafts, e ali o ciclo **quebrava
completamente**. Nenhum documento podia virar um registro financeiro real, por três
defeitos independentes que se somavam. Não era um elo fraco: eram três elos partidos
em série.

Portanto o produto não respondia a nenhuma das sete perguntas do §22, e não
responderia **mesmo com o banco cheio** de documentos processados.

As 18 constatações do §4 foram todas confirmadas. Várias eram piores do que a
auditoria estática suspeitava.

## Classificação por capacidade

Legenda: **pronto** · **parcial** · **quebrado** (existe e não funciona) ·
**inexistente**.

### Quebrado

| Capacidade                        | Evidência                                                                              | Por que é pior do que parecia                                                                                                                                                                 |
| --------------------------------- | -------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Contrato draft ↔ materialização   | `draft-generator.ts:83-98` vs `materialization.ts:15-34`                               | Não era divergência parcial: **100% dos drafts** reprovavam, com no mínimo 4 erros cada                                                                                                       |
| Materializador                    | `materialization.ts` (533 linhas)                                                      | Não tinha **nenhum chamador externo**. Código morto: nem UI, nem action, nem worker, nem tool de IA, nem MCP                                                                                  |
| Aprovação                         | `ingestion.ts:322`, `:376`; `ai/tools.ts:277`, `:782`; MCP `approve-draft-batch.ts:62` | Todos os **cinco** pontos de entrada só trocavam status                                                                                                                                       |
| Governança de status              | `materialization.ts:370`, `:467`                                                       | A guarda incluía `pending_review` apesar do comentário logo acima afirmar o contrário                                                                                                         |
| Atomicidade                       | insert + `markDraftPosted` em 2 round-trips                                            | PostgREST não expõe transação multi-statement: **não era corrigível no cliente**                                                                                                              |
| Idempotência                      | `materialization.ts:358`                                                               | TOCTOU: SELECT depois UPDATE, sem lock nem índice único                                                                                                                                       |
| Auditoria                         | `writeAuditLog` (`:86-103`)                                                            | Gravava em `target_id`/`details`, colunas **inexistentes**, e engolia o erro com `.then(() => {})`. Nunca houve trilha                                                                        |
| Contagem de valor                 | `materialization.ts:400-409`                                                           | `consumption_metric` roteado para `materializeTransaction`: uma conta de luz lançaria o **mesmo valor duas vezes**                                                                            |
| Criptografia de segredos          | `secret-lookup.ts:24-35`; migration `20260322180200`                                   | Quebrada nas duas direções, e a chave vinha de uma GUC de sessão que **ninguém nunca definiu**. Não estava fraca: era inexistente e sequer podia funcionar                                    |
| Identity key                      | `identity-key.ts:59-138`                                                               | Cinco prefixos com cinco aridades diferentes. Fatura e lembrete da mesma conta eram **matematicamente incapazes** de colidir — o oposto do objetivo declarado no próprio cabeçalho do arquivo |
| Orquestrador                      | `financial-evidence-worker/src/index.ts`                                               | Os 5 flags declarados não chegavam a lugar nenhum; `--process` era no-op total apesar do `--help` prometer o contrário; `src/adapters/` vazio                                                 |
| Gmail `--query`                   | `index.ts:57-59`; `gmail-client.ts:167-180`                                            | `options.query` era atribuído e nunca mais lido; o cliente não tinha **sequer um slot** para receber `q`                                                                                      |
| MCP `recompute_financial_periods` | `recompute-financial-periods.ts:41-44`                                                 | Nomes de parâmetro divergem da RPC **e** `auth.uid()` é NULL sob service_role. Nunca funcionou                                                                                                |
| Edge Function `trigger-ingestion` | `supabase/functions/trigger-ingestion/`                                                | Insere 4 colunas que não existem. É chamada pelo chat                                                                                                                                         |
| Deploy web                        | `.gitlab-ci.yml:188,231`                                                               | `echo "configurar provedor"`                                                                                                                                                                  |
| Filtro de testes de integração    | `.gitlab-ci.yml` `changes:`                                                            | Não incluía `workers/**`: **nenhuma** mudança de ingestão jamais disparou os testes de integração                                                                                             |

### Existe, testado, sem nenhum consumidor

O achado mais desperdiçado do repositório: código correto e testado que nada chama.

| Módulo                                                     | Linhas                                             | Consumidores                                                         |
| ---------------------------------------------------------- | -------------------------------------------------- | -------------------------------------------------------------------- |
| `financial_obligations` + `_evidences`                     | 2 tabelas com índice de identidade, RLS e policies | **zero** linhas de TypeScript                                        |
| `buildFinancialIdentityKey`                                | 138                                                | zero                                                                 |
| `amortization` (SAC/Price/Misto/quitação)                  | 222                                                | zero fora de testes                                                  |
| `financial-cycle`                                          | 172                                                | zero — `reports/page.tsx:36-46` **reimplementa** a matemática inline |
| `deduplication` (ADR-001) + view `v_expenses_deduplicated` | 140                                                | zero — relatórios somam `transactions` cru                           |
| `EvidenceEnvelope` / `SourceAdapter`                       | contratos bem modelados                            | zero implementadores                                                 |
| Server actions de domínio (9 arquivos)                     | —                                                  | zero — as páginas consultam o Supabase direto                        |

### Inexistente

Câmera (zero ocorrências de `capture=`/`getUserMedia`; `apps/mobile/` tem só
`package.json` e `tsconfig.json`) · OCR de imagem · parsers XLSX/DOCX/OFX/QIF ·
detecção de parcelas · derivação de ciclo de fatura · limite comprometido · rota
`/dashboard/cards` · agenda consolidada · projeção de caixa · avalanche/bola-de-neve ·
`worker_devices`/`worker_jobs`/`worker_job_events` · lease e heartbeat · fixtures
binários · `.github/workflows/`.

### Funcionava de verdade

Extração de PDF com retry de senha · parsers regex boleto/CEMIG · field-consensus ·
enrichers de IA com chamadas reais · máquina de 13 estados com locking otimista em
`transitionJob` · paginação real do Gmail · dedup por `content_hash` e por
`(gmail_message_id, filename)` · chat com streaming e rate limiting · deploy de
migrations e edge functions.

## Divergências documentais

`docs/specs/00-estado-real.md` afirmava:

| Afirmava                    | Realidade medida                                                   |
| --------------------------- | ------------------------------------------------------------------ |
| "TypeScript ✅ passa limpo" | 7 erros, confirmados contra o baseline com as dependências antigas |
| "319/319 (20 arquivos)"     | Desatualizado e misturando projetos                                |
| Domínio financeiro ✅       | 4 dos 5 módulos sem consumidor algum                               |
| Materialização              | **Não era mencionada**, o que escondia que tinha zero chamadores   |
| Formatos suportados         | UI anuncia 8; só PDF tem parser                                    |

`lint` e `format:check` também estavam vermelhos e não eram mencionados em lugar
nenhum.

## Dívida técnica registrada

1. `pdf-parse@1.1.1` — sem manutenção desde 2018, e é o motor exato da jornada P1 escolhida.
2. Lógica de reconciliação **duplicada** entre `reconciliation.ts` e `api/reconciliation/[draftId]/route.ts`.
3. `cards.credit_limit` e `financial_products.credit_limit` coexistem sem regra de precedência.
4. Cobertura de teste exclui `apps/web` e `workers/` inteiramente.
5. Banco local estava **5 migrations atrás** do repositório: os testes de integração rodavam contra schema defasado.
6. `auth.admin.listUsers()` devolve zero usuários com erro vazio contra o GoTrue local.

## Riscos críticos

| Risco                                                | Estado                                                                           |
| ---------------------------------------------------- | -------------------------------------------------------------------------------- |
| Chaves Supabase expostas (commit `38b8126`)          | Aberto — depende do CEO                                                          |
| Repositório **público** com app financeiro pessoal   | Aberto — depende do CEO                                                          |
| Sem staging: migration ruim vai direto ao banco real | Mitigado por `db reset` em CI, rollback obrigatório, dump prévio e portão humano |
| Layouts de PDF de emissor mudam sem aviso            | A mitigar com goldens versionados (P1.5)                                         |

## Mapa de dependências de P0

```
P0-1 contratos ──┬──────────────────► P0-5 materializador ──► P0-6 aprovação→lançamento
                 │                            ▲
P0-3 migration ──┴──► P0-4 RPC atômica ───────┘
                                              │
P0-2 identity key ──► P0-7 obrigações ────────┤
                                              ▼
P0-8 segredos ───────────────────────► P0-10 E2E ──► P0-11 docs
P0-9 orquestração ───────────────────►
```

Todos os onze itens foram executados e verificados. O detalhe de cada um está nas
mensagens de commit correspondentes, e o estado resultante em
[`00-estado-real.md`](../specs/00-estado-real.md).
