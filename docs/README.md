# Documentação — Seu Bolso Feliz

## Fonte da verdade

➡️ **[`specs/`](specs/)** — specs autoritativos, um por capacidade, verificados contra o código.
Comece por [`specs/00-estado-real.md`](specs/00-estado-real.md).

O guia operacional para agentes/devs é o [`CLAUDE.md`](../CLAUDE.md) na raiz.

## Histórico

📦 **[`_arquivo/`](_arquivo/)** — os ~54 documentos do processo antigo (refinos, checklists,
planejamento, passo-a-passo, ADRs, prompts da Verônica). **Histórico, não fonte da verdade.**
Vários afirmam que features estão "não iniciadas" quando estão construídas. Consulte só para
contexto e para o *porquê* das decisões (ADRs). Se contradisser o código, o código vence.

## Regra de ouro

Nenhum documento substitui rodar `bun run typecheck` e `bun run test`. A verdade é o que
compila e passa nos testes; os specs apenas a descrevem e apontam os gaps reais.
