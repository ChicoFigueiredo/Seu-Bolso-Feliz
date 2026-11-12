# ADR-018 — A shell egui é compartilhada com o Chess Studio, num repositório próprio

**Data:** 2026-11-12 · **Status:** aceita

## Contexto e decisão

O Chess Studio (`/mnt/d/Chico/chess-studio`, ADR-0005 de lá) já tem quase tudo o que o
desktop precisa:

- Ribbon com KeyTips;
- lista única de Ações, atalhos e Paleta;
- dock e painéis;
- temas por tokens;
- i18n Fluent;
- `chess-vault`, `chess-net` e as migrações do `chess-store`.

O problema é que a shell vive dentro do binário Bevy, e os painéis recebem o `World` do
Bevy.

**Decisão:**

- **Base do desktop:** o desktop do SBF usa eframe, ou seja, egui sem Bevy.
- **Biblioteca compartilhada:**
  - Num repositório próprio, sem Bevy, vão quatro peças: a shell, o chaveiro (a partir do
    `chess-vault`), o portão de rede (a partir do `chess-net`) e a mecânica de SQLite
    (migrações e PRAGMAs).
  - Os dois projetos a usam como dependência git fixada.
  - O SBF é o primeiro cliente. O Chess Studio migra depois de fechar a homologação.
- **Licenças:**
  - A biblioteca segue o ADR-0004 do Chess Studio: MIT OR Apache-2.0, sem a família GPL.
  - O SBF em si não tem política de licenças, porque é software pessoal e não é
    distribuído.
- **Visual:**
  - Do Chess Studio vêm shell, componentes e densidade.
  - Do `@sbf/design-system` e dos `ui-tokens` vêm cores, tipografia e temas.
  - Cada tela nasce como protótipo navegável e só vira egui depois de aprovada.

## Opções descartadas

- **Bevy + bevy_egui como está.** É um motor de jogo num app de finanças, e o modo sem
  janela teria que fugir do Bevy de qualquer jeito.
- **Copiar a shell e divergir.** Dois Ribbons para manter.
- **A biblioteca dentro do repositório do Chess Studio.** O SBF passaria a seguir o
  ritmo de outro projeto.

## Consequências

- Até o Chess Studio migrar, existe código duplicado.
- Os painéis passam a receber um contexto genérico no lugar do `World`.
- As janelas destacadas viram viewports do egui.
