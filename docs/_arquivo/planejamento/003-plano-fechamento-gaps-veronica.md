# 003 — Plano de Fechamento dos Gaps da Verônica

Objetivo: transformar os resultados da auditoria em execução prática, com fases, entregáveis, critérios de aceite e atividades manuais para garantir início de implementação com clareza total.

## 1. Premissas

- Não reabrir discussão estrutural já decidida em ADRs e refinos.
- Priorizar fechamento dos gaps que afetam utilidade operacional, confiabilidade e entrega contínua.
- Manter quality gates obrigatórios antes de push e merge.

## 2. Resumo dos Gaps Prioritários

### Gaps P0 (bloqueiam maturidade de execução)

- G1: Falta de testes de integração
- G2: Falta de testes e2e
- G3: Fluxos avançados de fornecedor ainda não completos na UI
- G4: Edge Functions estratégicas (merge/associação retroativa) não implementadas
- G5: Deploy web no CI ainda em placeholder

### Gaps P1 (importantes para previsibilidade de operação)

- G6: Validação operacional final de GitLab (proteções, approvals, variáveis)
- G7: Relatórios compostos por fornecedor ainda incompletos
- G8: Auditoria histórica por fornecedor ainda não exposta de forma completa na interface

## 3. Sequenciamento Recomendado

## Fase 1 — Confiabilidade de Fluxo e Testes

Objetivo: garantir que o que já existe esteja protegido contra regressão ponta a ponta.

Entregáveis:

- E1.1: suíte de integração para fluxos críticos
- E1.2: suíte e2e para jornada principal do usuário
- E1.3: matriz de cobertura teste x regra de negócio crítica

Critérios de aceite:

- CA1.1: testes de integração executam em pipeline sem flakiness relevante
- CA1.2: testes e2e cobrem login + dashboard + transação + relatório
- CA1.3: pelo menos um cenário de fornecedor validado ponta a ponta

O que fazer manualmente:

1. Definir cenários mínimos obrigatórios com QA e arquitetura.
2. Rodar cenários em ambiente local e em pipeline de MR.
3. Registrar evidências no MR (logs e artefatos).

## Fase 2 — Fechamento Funcional da Dimensão Fornecedor

Objetivo: tornar fornecedor plenamente utilizável no dia a dia operacional do produto.

Entregáveis:

- E2.1: área de fornecedor no dashboard com CRUD completo
- E2.2: gestão de aliases e contratos em fluxo visual
- E2.3: associação fluida de fornecedor em transação/recorrência/documento

Critérios de aceite:

- CA2.1: usuário consegue operar fornecedor sem SQL/manual
- CA2.2: aliases e histórico funcionam com rastreabilidade
- CA2.3: filtros por fornecedor funcionam em fluxos principais

O que fazer manualmente:

1. Validar com casos reais (Vivo, Neoenergia, GitHub, ChatGPT etc.).
2. Revisar UX com Renata/Helena para reduzir atrito.
3. Confirmar coerência com regras R18-R27 do refino.

## Fase 3 — Relatórios Compostos e Auditoria Histórica

Objetivo: entregar o valor analítico que motivou a dimensão fornecedor.

Entregáveis:

- E3.1: relatórios por fornecedor com cruzamentos (categoria, tags, prioridade, período)
- E3.2: visão de variação por fornecedor
- E3.3: indicadores de pendências/documentos ausentes por fornecedor

Critérios de aceite:

- CA3.1: perguntas-chave do prompt 002 são respondíveis na interface
- CA3.2: deduplicação não distorce totais por fornecedor
- CA3.3: filtros compostos funcionam com consistência temporal

O que fazer manualmente:

1. Rodar roteiro de validação analítica com dados de teste controlados.
2. Comparar resultados com cálculos manuais de amostra.
3. Registrar discrepâncias e ajustes antes de liberar para staging.

## Fase 4 — Edge Functions Estratégicas

Objetivo: implantar operações atômicas e sensíveis previstas nos ADRs.

Entregáveis:

- E4.1: função de merge de fornecedores
- E4.2: função de associação retroativa por alias/matching assistido
- E4.3: contrato operacional e de segurança de cada função

Critérios de aceite:

- CA4.1: merge preserva histórico e rastreabilidade
- CA4.2: conflitos críticos exigem revisão humana
- CA4.3: políticas e segredos por ambiente validados

O que fazer manualmente:

1. Configurar secrets por ambiente no Supabase/GitLab.
2. Executar testes de rollback/reversão em cenário controlado.
3. Validar logs e trilha de auditoria das operações.

## Fase 5 — Fechamento de Entrega Contínua Remota

Objetivo: transformar o pipeline em operação real de staging/produção com governança completa.

Entregáveis:

- E5.1: deploy web real no CI (sem placeholder)
- E5.2: checklist de ambientes e variáveis confirmado
- E5.3: fluxo oficial MR -> staging -> promoção para main validado

Critérios de aceite:

- CA5.1: deploy automático em develop funcionando
- CA5.2: deploy manual controlado em main funcionando
- CA5.3: políticas de branch protection e approvals comprovadas

O que fazer manualmente:

1. Configurar provedor de deploy web e integrar no job deploy-web.
2. Validar variáveis protegidas/masked no GitLab.
3. Executar smoke test em staging antes de qualquer promoção.

## 4. Matriz de Dono por Fase

- Fase 1: Renata (QA), Maria (Backend), Thiago (Front Engineer)
- Fase 2: Roberto (Frontend), Sofia (Frontend), João (Backend)
- Fase 3: Ricardo (Economista), Camila (Finanças), André (DBA)
- Fase 4: João (Backend), Maria (Backend), Fernando (DevOps)
- Fase 5: Fernando (DevOps), Ana (Arquiteta), Chico (CEO)

## 5. Definition of Done do Ciclo de Fechamento

Um gap só é considerado fechado quando:

1. Implementação concluída
2. Testes relevantes aprovados
3. Critérios de aceite atendidos
4. Evidência registrada em MR
5. Atualização de documentação realizada

## 6. Roteiro de Execução Semanal (sugestão)

- Semana 1: Fase 1
- Semana 2: Fase 2
- Semana 3: Fase 3
- Semana 4: Fase 4
- Semana 5: Fase 5

Ajustes de calendário devem ser feitos por capacidade real da equipe e risco de cada entrega.

## 7. Qualidade Obrigatória por Entrega

Antes de concluir qualquer fase:

- bun run lint
- bun run typecheck
- bun run test
- bun run build

Para merge:

- pipeline verde
- revisão aprovada
- checklist da fase anexado
- riscos remanescentes declarados

## 8. Encaminhamento Final

Com este plano, o projeto segue com execução disciplinada, previsível e auditável. A recomendação é iniciar imediatamente pela Fase 1, sem abrir novas frentes paralelas fora dos gaps priorizados.
