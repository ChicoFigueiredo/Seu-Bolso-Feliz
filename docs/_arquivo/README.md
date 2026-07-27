# Arquivo histórico — NÃO é fonte da verdade

> ⚠️ **Atenção.** Tudo nesta pasta é **histórico**. Foi produzido pelo processo antigo
> de "roleplay de time" (Verônica/CEO) e **derivou do código real**. Vários documentos
> afirmam que features estão "não iniciadas" quando estão construídas e funcionando
> (ex.: `checklists/004-consolidado-gaps-pendentes.md` diz que a UI de ingestão e a
> integração OpenAI estão 0% — ambas existem e passam nos testes).

## Onde está a verdade agora

A fonte da verdade do projeto passou a ser:

1. **O código + os testes** (`bun run typecheck`, `bun run test`).
2. **`docs/specs/`** — specs autoritativos, um por capacidade, verificados contra o código.
   Comece por [`docs/specs/00-estado-real.md`](../specs/00-estado-real.md).
3. **`CLAUDE.md`** (raiz) — guia operacional para qualquer agente/dev.

## Por que mantivemos este arquivo

Valor histórico e rastreabilidade: as ADRs (`adrs/`) registram o *porquê* de decisões,
os refinos (`refinos/`) registram o raciocínio, e os prompts da Verônica (`veronica/`)
registram a intenção funcional original. **Consulte para contexto, nunca para saber
o estado atual.** Se um documento daqui contradisser o código, o código vence.

O índice original de `docs/` está preservado em [`README-docs-original.md`](README-docs-original.md).

_Arquivado em 2026-06-20._
