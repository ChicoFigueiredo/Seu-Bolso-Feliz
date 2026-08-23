# Plano mestre de operacionalização

> Entregável §16.3 do plano mestre. Data: 2026-07-27.
>
> Documentos irmãos: [diagnóstico](2026-07-27-estado-real-pos-auditoria.md) ·
> [arquitetura](../arquitetura/2026-07-27-arquitetura-hibrida-alvo.md) ·
> [backlog](../backlog/2026-07-27-backlog-operacionalizacao.md) ·
> [matriz](../qualidade/2026-07-27-matriz-jornadas-requisitos-testes.md)

## Decisões travadas com o CEO

| Decisão                     | Escolha                                          |
| --------------------------- | ------------------------------------------------ |
| Escopo                      | Documentos do §16 **e** execução de P0           |
| Primeira jornada vertical   | **Fatura protegida de cartão** (§7.2)            |
| `financial-evidence-worker` | Deletado, substituído por CLI único              |
| CI/CD                       | Migrar para GitHub Actions                       |
| Ambientes                   | **Staging eliminado**; deploy direto em produção |
| Backlog                     | Épicos/features/issues no GitHub                 |
| Módulos                     | Atualização como trilha própria                  |

> **Ressalva registrada, decisão respeitada:** fatura protegida é a jornada XL. Ela
> torna P0-8 (segredos) caminho crítico em vez de trilha paralela, e exige construir do
> zero telas de cartão, derivação de ciclo, detecção de parcelas e perfis de emissor. O
> plano a entrega integralmente na ordem escolhida.

## Fases

### Fase 0 — Verdade e integridade do núcleo ✅ **concluída**

Onze itens, todos verificados por execução:

| Item   | Entrega                                            | Commit    |
| ------ | -------------------------------------------------- | --------- |
| P0-1   | `@sbf/contracts` — contrato unificado e versionado | `fb7c698` |
| P0-2   | Identity key que realmente agrupa                  | `0e32a66` |
| P0-3/4 | Materialização atômica e idempotente               | `9e3bd42` |
| P0-5/6 | Aprovação → lançamento nos 5 pontos                | `340e735` |
| P0-7   | Obrigações conectadas ao pipeline                  | `2861dc2` |
| P0-8   | Criptografia real de segredos                      | `2497684` |
| P0-9   | CLI único, Gmail `q`, checkpoints                  | `13e2b7c` |
| P0-10  | Fixtures reais e E2E do ciclo                      | `3d2549c` |
| P0-11  | Correção do documento de estado real               | `40fd2c6` |

**Resultado:** um documento vira transação real, uma única vez, sob transação de banco.
Antes, 100% dos drafts reprovavam na validação e o materializador não tinha chamadores.

### Fase 1 — Fatura protegida de cartão

Ordem: F6.1 (telas de cartão, **bloqueador**) → F6.4 (upload direto) → F6.2 (perfis de
senha) → F6.3 (motores) → F6.5 (goldens) → F6.6 (trocar `pdf-parse`).

Dependência dura: **F6.6 só depois de F6.5.** Trocar o motor de PDF sem a rede dos
goldens é trocar um risco conhecido por um invisível.

### Fase 2 — Agente local como produto

Fora do escopo desta execução. Arquitetura registrada: identidade Ed25519 por
dispositivo, broker de token por Edge Function, `worker_jobs` com lease,
`bun build --compile` + Scheduled Task.

### Fase 3 — Inteligência financeira

**E11 pode começar imediatamente, em paralelo a tudo** — não tem dependências e é a
trilha mais barata de credibilidade: quatro módulos testados esperando um consumidor.

### Fase 4 — Escala e refinamento

Aprendizado supervisionado sobre `document_patterns`, expansão de perfis de emissor,
controle de custo de IA, observabilidade avançada.

## Estratégia de migração de dados

| Mudança                             | Estratégia                                                                                                                                                                                                                                                                                                                                              |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `draft_data` v0 → v1                | Coluna real `draft_schema_version` (não chave no JSONB, para o progresso ser consultável). Backfill em TypeScript importando a **mesma** `migrateV0ToV1` do runtime — duas cópias divergiriam. `parseDraftPayload` tolera v0 para sempre, então backfill parcial nunca é estado quebrado. Linhas `posted`/`rejected` ficam em v0: histórico é imutável. |
| `user_secrets` texto puro → cifrado | Zera `encryption_version` para refletir a verdade, encripta, e só então adiciona `CHECK (>= 1)`.                                                                                                                                                                                                                                                        |
| `materialization_key`               | Trigger `BEFORE INSERT`, não DEFAULT — a chave deriva do id da própria linha, e assim nenhum escritor precisa lembrar dela.                                                                                                                                                                                                                             |
| `financial_obligations`             | Sem backfill: a tabela estava vazia porque nenhum código jamais escreveu nela.                                                                                                                                                                                                                                                                          |

## Estratégia de testes

Cinco camadas (§15). Estado medido na
[matriz](../qualidade/2026-07-27-matriz-jornadas-requisitos-testes.md).

O investimento de maior retorno é o **conjunto de goldens** (F6.5): até P0-10 não havia
fixture binário algum, e nenhum parser jamais foi testado contra documento real.

## Estratégia de rollout

Sem staging, quatro salvaguardas são obrigatórias:

1. `supabase db reset` a cada PR — prova que as migrations reconstroem o schema do zero.
2. Bloco `-- rollback:` em toda migration, cobrado no template de PR.
3. Dump do banco como artifact antes de qualquer `db push`.
4. Portão humano: environment `production` com required reviewer; `workflow_dispatch`.

## Definition of Done

A checklist do §19 está no template de PR. O ponto que mais importa:

> **Um item só é ✅ com duas evidências: um teste que passa E um chamador em produção.**
> "Testado mas nunca chamado" é 🟡 por definição.

Essa regra é o aprendizado central da auditoria. Sem ela, quatro módulos de domínio
ficaram marcados como prontos durante meses sem que nenhuma tela os chamasse.

## Fora de escopo

Agente local instalável · identidade de dispositivo · `worker_jobs` com lease · câmera e
PWA · avalanche/bola-de-neve · projeção de caixa · zod 4 e AI SDK 5 · `apps/mobile`.

## Riscos

| Risco                                                 | Mitigação                                                        |
| ----------------------------------------------------- | ---------------------------------------------------------------- |
| Fatura protegida primeiro concentra risco             | O E2E de PDF de P0-10 antecipou parte dele                       |
| `pdf-parse@1.1.1` sem manutenção é o motor da jornada | Trocar **depois** dos goldens                                    |
| Sem staging, migration ruim vai direto ao banco real  | Quatro salvaguardas acima; risco residual aceito conscientemente |
| Repositório público com backlog financeiro detalhado  | Aviso no template de issue; decisão de privar é do CEO           |
| Layouts de emissor mudam sem aviso                    | Goldens versionados + caminho explícito "parser falhou → IA"     |

## Decisões que dependem exclusivamente do CEO

Listadas ao final do [backlog](../backlog/2026-07-27-backlog-operacionalizacao.md). As
duas mais bloqueantes: **quais emissores** ganham perfil de primeira classe (exige
documentos reais anonimizados) e a **postura LGPD** sobre documentos saírem da máquina.
