# Planejamento — Seu Bolso Feliz

Este diretório contém a documentação de planejamento técnico do projeto, incluindo ADRs (Architecture Decision Records) e guias de implementação.

---

## Índice de Documentos

### ADRs (Architecture Decision Records)

| #       | Documento                                                                                                            | Status      | Resumo                                                                              |
| ------- | -------------------------------------------------------------------------------------------------------------------- | ----------- | ----------------------------------------------------------------------------------- |
| ADR-001 | [Deduplicação Transação vs Item de Fatura](ADR-001-deduplicacao-transacao-item-fatura.md)                            | ✅ Aprovado | Vínculo explícito via FK, regras de precedência, view `v_expenses_deduplicated`     |
| ADR-002 | [Norma de `consumption_metrics`](ADR-002-norma-consumption-metrics.md)                                               | ✅ Aprovado | Distinção métrica vs atributo vs metadado, constraint no banco, convenções por tipo |
| ADR-003 | [Governança de Aliases de Fornecedor](ADR-003-governanca-aliases-fornecedor.md)                                      | ✅ Aprovado | Unicidade temporal, auto-alias, merge atômico, revisão humana                       |
| ADR-004 | [Arquitetura Operacional: Repositório, CI/CD e Engenharia](adrs/ADR-004-arquitetura-operacional-repositorio-cicd.md) | ✅ Aprovado | Monorepo Bun workspaces, GitLab CI/CD 6 stages, trunk-based, Conventional Commits   |

### Guias

| Documento                                                                                      | Descrição                                                                                      |
| ---------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------- |
| [Guia de Implementação Passo a Passo](planejamento/001-guia-implementacao-passo-a-passo.md)    | 5 etapas sequenciais (Etapas 1–5) com entregáveis, SQL, critérios de aceitação e ações manuais |
| [Guia de CI/CD e Engenharia Operacional](planejamento/002-guia-cicd-engenharia-operacional.md) | Etapa 0 (Sprint 0): setup monorepo, pipeline, ambientes, migrações, deploy, fluxo de trabalho  |

---

## Contexto

Estes documentos foram gerados a partir dos 3 ajustes obrigatórios identificados no **Parecer Formal de Revisão** da consultora Verônica, que aprovou o refino da dimensão fornecedor com as seguintes condições de pré-implementação:

1. **Fechar estratégia de deduplicação** entre `transaction` e `statement_item` → ADR-001
2. **Fechar norma de uso** de `consumption_metrics` → ADR-002
3. **Fechar governança técnica** de aliases de fornecedor → ADR-003

Adicionalmente, decidiu-se que `supplier_tags` entra no schema (Etapa 1) mas sem UI no MVP.

## Sequenciamento Aprovado

```
Etapa 0: Setup Monorepo + CI/CD  → Sprint 0: estrutura, workspaces, pipeline, tooling (ADR-004)
Etapa 1: Base Estrutural         → Migrations, RLS, índices, constraints, triggers
Etapa 2: Contrato Comportamental → 27+ testes mandatórios escritos (red)
Etapa 3: Núcleo Funcional        → CRUD fornecedor, aliases, autocomplete, associações
Etapa 4: Relatórios e Filtros    → View dedup, relatórios, filtros compostos, home
Etapa 5: Recursos Avançados      → Métricas, merge atômico, retroatividade, audit
```

## Documentos de Referência

| Documento                                   | Caminho                                                                                      |
| ------------------------------------------- | -------------------------------------------------------------------------------------------- |
| Ata do checkpoint pré-implementação         | `docs/refinos/2026-03/2026-03-21-13-30-checkpoint-pre-implementacao-ajustes-obrigatorios.md` |
| Refino técnico-funcional (kickoff)          | `docs/refinos/2026-03/2026-03-21-10-40-refino-tecnico-funcional-kickoff-seu-bolso-feliz.md`  |
| Revisão dimensão fornecedor                 | `docs/refinos/2026-03/2026-03-21-11-57-revisao-refino-dimensao-fornecedor.md`                |
| Prompt inicial Verônica                     | `docs/Veronica/001-prompt.inicial.md`                                                        |
| Especificação fornecedor                    | `docs/Veronica/002-fornecedor.md`                                                            |
| Refino de Arquitetura de Engenharia e CI/CD | `docs/refinos/2026-03/2026-03-21-14-19-refino-arquitetura-engenharia-cicd.md`                |
