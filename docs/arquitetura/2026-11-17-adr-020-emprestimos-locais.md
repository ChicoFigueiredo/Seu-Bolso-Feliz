# ADR-020 — Empréstimos nascem no planejamento local, na forma da Dívida

**Data:** 2026-11-17 · **Status:** aceita

## Contexto e decisão

O livro-caixa no servidor já tem Dívida (saldo devedor, taxa, sistema, parcelas), e o ADR-013
manda toda escrita dele passar pelo servidor. Mas falta o número do contrato, a evolução
parcela a parcela e a renovação; o servidor ainda não está no ar para a estação; e os
consignados precisam viver junto do controle mensal, que já é local (ADR-019).

O CEO decidiu: cada empréstimo nasce no `planejamento.sqlite` do cofre, com os campos da
Dívida do livro-caixa **mais** número do contrato, evolução e renovação. A área Dívidas da
janela mostra os locais. Subir para o servidor é uma fatia própria, depois.

## Consequências

- **Segunda exceção ao ADR-013**, nos mesmos termos do ADR-019: identidade estável desde o
  primeiro dia, para a subida não duplicar.
- **Mesma forma da Dívida.** Os nomes e tipos dos campos seguem `sbf_contrato::Divida`; o que
  é novo (contrato, evolução, renovação) vira proposta de mudança no contrato quando subir.
- **Dívidas do servidor e locais convivem** na área Dívidas até a subida; o que for o mesmo
  contrato nos dois lados é casado pelo número do contrato.
