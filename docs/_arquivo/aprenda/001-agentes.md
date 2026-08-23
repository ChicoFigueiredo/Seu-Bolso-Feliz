# Guia Prático de Agentes no Projeto Seu Bolso Feliz

## 1) O que são agentes na pasta .github/agents

Agentes são perfis especializados de execução para tarefas diferentes.
Cada arquivo de agente define:
- nome;
- descrição;
- ferramentas permitidas;
- papel e objetivo;
- regras de atuação;
- formato de saída esperado.

Na prática, você cria “modos de trabalho” focados (ex.: explorar UX, auditar UX, implementar correção cirúrgica, revisar acessibilidade), em vez de usar um único comportamento genérico para tudo.

## 2) Você tem ganho ao ter agentes?

Sim, normalmente tem ganho em:
- previsibilidade: cada tarefa segue o mesmo padrão de qualidade;
- foco: o agente não tenta fazer tudo ao mesmo tempo;
- segurança: limita ferramentas por função (ex.: agente de auditoria sem edição);
- velocidade de revisão: achados vêm no formato que o time espera;
- padronização: facilita handoff entre investigação, implementação e validação.

Quando o ganho é menor:
- projeto muito pequeno com tarefas simples;
- time que ainda não tem processo mínimo definido;
- agentes criados sem escopo claro (viram só “nomes diferentes”).

## 3) Você invoca conforme necessidade?

Sim. O uso ideal é por necessidade da tarefa, não por obrigação.

Padrão recomendado:
- tarefa exploratória: usar agente explorador;
- tarefa analítica: usar agente auditor;
- tarefa de mudança pontual: usar agente implementador;
- validação final: usar agente revisor.

Ou seja: não precisa usar agente em toda solicitação, mas vale usar quando a especialização reduz erro, retrabalho e ambiguidade.

## 4) Criar um agente por pessoa da equipe recrutada vale a pena?

Resposta curta: em geral, não como primeira estratégia.

Melhor estratégia inicial:
- criar agentes por função de trabalho, não por pessoa;
- mapear fluxos reais do produto (descobrir, implementar, validar, revisar);
- só depois, se necessário, dividir em subespecialidades.

Por que evitar 19 agentes de uma vez:
- alto custo de manutenção;
- sobreposição de responsabilidades;
- maior ambiguidade de escolha;
- difícil manter instruções consistentes.

Quando faz sentido representar a equipe:
- se cada papel tiver entregável claramente distinto;
- se houver checklist diferente por papel;
- se houver ganho operacional real (tempo, qualidade, risco).

Modelo recomendado para seu projeto agora:
- Domain Explorer
- Domain Model Reviewer
- Financial Rules Implementer
- Test Guardian
- UX Explorer
- UX Auditor
- UX Implementer
- A11y Reviewer

## 5) Você (assistente) também ganha com isso?

Sim. Eu ganho em:
- contexto mais objetivo por tarefa;
- menor conflito de instruções;
- melhor uso de ferramenta certa para objetivo certo;
- saída mais consistente e auditável.

Mas o principal ganho é seu e do time: processo mais confiável e repetível.

## 6) Estrutura de um arquivo de agente

Local:
.github/agents/nome-do-agente.agent.md

Estrutura mínima:
---
name: Nome do Agente
description: O que ele faz em uma linha
tools: ['codebase', 'search']
---

# Papel
Quem esse agente é.

## Objetivo
Resultado esperado.

## Entradas
Quais evidências recebe.

## Regras
Limites claros (o que pode e não pode fazer).

## Checklist
Itens que sempre valida.

## Formato da resposta
Como devolver resultado (campos fixos).

## Proibições
Coisas que nunca devem acontecer.

## 7) Como escolher as ferramentas (tools)

Princípio: menor permissão possível.

Exemplos:
- investigação: codebase, search, playwright/*;
- auditoria: codebase, search;
- implementação pequena: codebase, search, editFiles, runTasks;
- revisão final de UX/A11y: codebase, search, playwright/*, runTasks.

Se o agente não precisa editar, não inclua editFiles.

## 8) Fluxo recomendado no seu projeto

Fluxo de 4 etapas:
1. Explorar problema (evidência).
2. Auditar e priorizar (achados claros).
3. Implementar correção pequena e verificável.
4. Revisar regressão e acessibilidade.

Esse fluxo combina com os agentes que você já possui em .github/agents.

## 9) Boas práticas

- Defina escopo pequeno por agente.
- Defina formato de saída obrigatório.
- Inclua proibições explícitas.
- Reutilize checklist entre agentes quando fizer sentido.
- Evite agente “faz tudo”.
- Revise instruções quando houver retrabalho recorrente.

## 10) Sinais de que um agente está mal definido

- respostas vagas;
- recomendações sem evidência;
- edição de código fora do escopo;
- sugestões grandes para tickets pequenos;
- resultados inconsistentes entre execuções.

## 11) Como evoluir sem virar burocracia

- começar com 3 a 5 agentes principais;
- medir qualidade de saída por 2 semanas;
- criar novo agente apenas se houver lacuna real;
- arquivar agente sem uso recorrente.

## 12) Exemplo de decisão para o Seu Bolso Feliz

Decisão prática inicial:
- consolidar UX em um agente único de ponta a ponta;
- adicionar um revisor técnico de código orientado aos princípios do arquiteto;
- adicionar no futuro agentes de domínio financeiro e testes críticos;
- evitar um agente por pessoa recrutada nesta fase;
- priorizar agentes por fluxo de trabalho do MVP.

## 13) Resumo direto das suas perguntas

- Tenho ganho com agentes?
Sim, principalmente em qualidade, previsibilidade e velocidade de execução por especialidade.

- Você invoca conforme necessidade?
Sim. Uso por tipo de tarefa, quando a especialização reduz risco e retrabalho.

- Transformar toda equipe recrutada em agentes independentes gera ganho?
Só parcialmente. O melhor custo-benefício inicial é por função/fluxo, não por pessoa.

- Você também ganha?
Sim, em foco e consistência, mas o ganho maior é do seu processo.

## 14) Próximo passo sugerido

Criar uma versão 002 deste guia com:
- matriz de decisão de qual agente usar por tipo de ticket;
- templates prontos de novos agentes de domínio financeiro;
- checklist de governança para manter qualidade dos agentes ao longo do tempo.

## 15) Versão enxuta (até 10 agentes) com guarda de arquitetura

Objetivo desta versão: reduzir sobreposição, manter velocidade de entrega e aumentar segurança técnica.

Agentes recomendados (9 no total):

1. Architecture Guard
- Foco: proteger princípios arquiteturais, limites de domínio e coerência entre web, mobile, Supabase e pacotes compartilhados.
- Entrega: parecer de conformidade, riscos de acoplamento e recomendações pequenas e executáveis.
- Tools: codebase, search.

2. Dev Code Reviewer
- Foco: revisar implementações e garantir aderência aos princípios pregados pelo arquiteto.
- Critérios obrigatórios: serverless-first, simplicidade de manutenção, segurança por padrão (RLS/segregação), separação entre núcleo financeiro determinístico e camada futura de IA, consistência de padrões entre web/mobile e respeito ao design system.
- Entrega: checklist de conformidade, violações com evidência e correções objetivas por prioridade.
- Tools: codebase, search, runTasks.

3. Finance Domain Guardian
- Foco: validar sem ambiguidade eventos financeiros (despesa, transferência interna, pagamento de fatura, passivo, amortização, juros, estorno, ajuste).
- Entrega: checklist de regra crítica e impacto em comportamento.
- Tools: codebase, search.

4. Financial Time Engine Implementer
- Foco: implementar ciclo financeiro personalizado, competência, mês civil e ciclo de fatura sem inconsistência temporal.
- Entrega: mudança de código + critérios de validação temporal.
- Tools: codebase, search, editFiles, runTasks.

5. Liability and Amortization Implementer
- Foco: empréstimos e financiamento com decomposição de parcela (amortização, juros, taxas, encargos), saldo devedor e quitação antecipada.
- Entrega: implementação matemática e testes associados.
- Tools: codebase, search, editFiles, runTasks.

6. Import and Ledger Integrity Implementer
- Foco: importação de histórico, deduplicação, idempotência e integridade do razão financeiro.
- Entrega: fluxo robusto de import + prevenção de duplicidade.
- Tools: codebase, search, editFiles, runTasks.

7. Supabase Security and RLS Guard
- Foco: políticas RLS, segregação de permissões, tratamento de segredos e segurança operacional de dados financeiros.
- Entrega: parecer de segurança + ajustes pontuais de configuração/migração quando solicitado.
- Tools: codebase, search, runTasks.

8. Test Contract Guardian
- Foco: garantir testes como contrato vivo das regras críticas de domínio e regressão.
- Entrega: cobertura de cenários obrigatórios, sem afrouxar regra por conveniência.
- Tools: codebase, search, editFiles, runTasks.

9. UX Full-Cycle Agent
- Foco: unificar exploração, auditoria, implementação UX e validação A11y básica em um único fluxo de execução.
- Entrega: evidência do problema, proposta objetiva, alteração pequena e validação final (desktop/mobile básico, foco, labels, contraste e feedback).
- Tools: codebase, search, playwright/*, editFiles, runTasks.

O que foi condensado:
- UX Explorer, UX Auditor, UX Implementer e A11y Reviewer foram unificados em um único agente UX full-cycle.
- Domínio financeiro foi dividido por blocos de risco alto: temporal, passivos e integridade de razão.
- Guarda de arquitetura entrou como camada transversal de governança.
- Revisão técnica de código virou papel explícito com o Dev Code Reviewer.

O que foi eliminado:
- agentes por pessoa da equipe recrutada;
- agentes generalistas sem fronteira clara;
- papéis duplicados sem ganho operacional.

Sequência de adoção com maior retorno:
1. Architecture Guard
2. Dev Code Reviewer
3. Finance Domain Guardian
4. Test Contract Guardian
5. Financial Time Engine Implementer
6. Supabase Security and RLS Guard
7. Liability and Amortization Implementer
8. Import and Ledger Integrity Implementer
9. UX Full-Cycle Agent
