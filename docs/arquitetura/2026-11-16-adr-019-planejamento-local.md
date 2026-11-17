# ADR-019 — O planejamento nasce local, no cofre, e sincroniza depois

**Data:** 2026-11-16 · **Status:** aceita

## Contexto e decisão

O controle mensal precisa de recorrentes com versões, pausas, cancelamentos e exceções por
mês, projetados por 360 meses. O ADR-013 manda toda escrita do livro-caixa passar pelo
servidor. Seguir isso hoje exigiria Ações novas no contrato, migração no Neon, deploy na VPS
e o pareamento da estação antes de a primeira tela ficar útil.

O CEO decidiu: o planejamento nasce **local**, num `planejamento.sqlite` dentro do cofre, ao
lado do inventário. Entra na cópia cifrada e na Estante, porque guarda valores reais. A
sincronização com `recurring_templates`, `recurring_instances` e `liabilities` no Neon vem
depois, como uma fatia própria.

## Consequências

- **Exceção ao ADR-013, só para o planejamento.** Movimentações, rascunhos e a caixa de
  entrada seguem pelo servidor. Quando a sincronização chegar, cada regra local vira uma
  Ação com versão de base, e o servidor volta a ser a autoridade.
- **Ocorrências não são gravadas.** Elas são calculadas a partir das regras; só exceções,
  versões, pausas e cancelamentos são gravados. Sincronizar depois é mandar regras, não 360
  meses de linhas.
- **Identidade estável.** Toda regra e exceção tem um id que não muda, para a sincronização
  futura não duplicar nada.
- **Leitura do livro-caixa.** A conciliação com movimentações reais lê o cache da estação ou
  o inventário (Pluggy); o planejamento nunca escreve movimentação.
