# ADR-012 — O núcleo passa a ser Rust, e cada regra tem uma só implementação viva

**Data:** 2026-11-12 · **Status:** aceita

## Contexto e decisão

O CEO quer um app desktop em Rust com todo o poder do Seu Bolso Feliz. Hoje as regras
estão em TypeScript:

- `packages/domain`, com cerca de 2 mil linhas;
- `packages/validation`, com cerca de 600;
- `packages/contracts`, com cerca de 1.400;
- `fn_materialize_draft_record` em plpgsql e os triggers de rateio;
- a conciliação, que já tem duas cópias: uma no worker e outra em `api/reconciliation`.

**Decisão:** o Rust vira o núcleo, e a troca é por estrangulamento.

- Uma regra é portada com os testes TS dela como fixtures de ouro.
- O mesmo PR que entrega a versão Rust apaga a versão TS, e quem a usava passa a chamar
  o Rust.
- Em nenhum momento a mesma regra vive em duas linguagens.

As peças que acompanham a decisão:

- **Web:** o Next.js vira companheiro remoto. Ninguém cria tela nova nele, e ele passa a
  consumir a API Rust (ADR-013). O esqueleto Expo em `apps/mobile` é arquivado.
- **Binário:** é um só, `sbf`, com modos. Sem argumento abre a janela. `sbf worker` roda
  sem janela, `sbf servidor` é a autoridade e `sbf mcp` fala MCP. Uma versão do binário
  é uma versão das regras.
- **Código:** fica em `crates/`, neste monorepo, ao lado do TS que substitui.

## Opções descartadas

- **TS continua a fonte e o desktop é só cliente.** É o caminho mais rápido, mas deixa o
  Rust só com a interface, e o motivo do desktop é ser o motor.
- **Reescrever tudo de uma vez.** O corte único não cabe na regra de evidência do
  `docs/specs/00-estado-real.md`, em que só vale o que tem teste e chamador.
- **Duas implementações em paridade.** O ADR-002 nasceu exatamente de schemas
  divergentes, que reprovavam 100% dos drafts.

## Consequências

- O port da conciliação precisa escolher uma das duas cópias atuais e apagar a outra.
- Os workers Bun se aposentam um a um, à medida que o equivalente Rust chega.
- Durante a transição, o web chama a API Rust para as regras já portadas e as Server
  Actions para as demais.
