# design-sync — notas do repositorio

## Origem do design system

O pacote `packages/design-system` foi **derivado do canvas** "Seu Bolso Feliz —
Redesign" (projeto Claude Design `68788755-fc51-42d5-97e2-cef429a241ad`,
arquivo `Seu Bolso Feliz.dc.html`), nao de codigo pre-existente. O canvas segue
sendo a fonte da verdade visual:

- os 7 temas em `src/tokens.css` foram extraidos **verbatim** dos blocos
  `[data-sbf-theme="..."]` do canvas — nao reescreva valores a mao;
- o contrato de produto esta em `uploads/seu-bolso-feliz-redesign-kit/PROMPT-REDESIGN-SEU-BOLSO-FELIZ.md`
  no mesmo projeto (secao 4 = paleta, secao 21 = lista de componentes semanticos);
- `uploads/Seu bolso feliz/sbf-ui.jsx` no canvas e a replica do **estado antigo**
  (tokens neutros do shadcn). Nao e referencia para o design system.

`apps/web` ainda usa os tokens neutros do shadcn em `src/app/globals.css`. Adotar
`@sbf/design-system` na web e um passo separado, deliberadamente fora desta sync.

## Gotchas do conversor

- **Passe `--entry` e `--node-modules` como caminhos absolutos.** A raiz do repo
  tem um `src/APITester.tsx` e um `styles/globals.css` legados (template do Bun).
  Com caminho relativo o conversor nao acha `./dist/index.js`, cai em modo
  synth-entry e bundla esse `src/` legado — o build morre em
  `Could not resolve "@/components/ui/button"`. Use `.design-sync/build.sh`.
- Bun usa linker isolado: `react`/`react-dom`/`@types/react` so existem em
  `packages/design-system/node_modules` (symlinks), nunca na raiz. E por isso que
  `--node-modules` aponta para la.
- `playwright@1.62.1` fixa chromium **build 1234**, que ja esta em
  `~/.cache/ms-playwright/`. Nao ha download de browser nesta maquina.
- `cfg.provider` = `SbfTheme`. Os componentes renderizam mesmo sem ele (o `:root`
  de tokens.css aplica salvia como padrao), mas o provider garante o atributo
  `data-sbf-theme` nos cards.

## Known render warns

- `[FONT_REMOTE]` para `Manrope`, `Instrument Serif` e `Material Symbols Rounded`
  — **esperado**. As familias chegam por `@import` do Google Fonts no topo de
  `tokens.css`; o design system nao empacota woff2 de proposito.
- `tokens: 72 defined, 8 referenced (1 missing, below threshold)` — esperado. Os
  componentes usam estilo inline com `var(--token)`, entao quase nao ha CSS de
  componente para o scrape casar.

## Ordem de trabalho

- **`package-build.mjs` apaga `ds-bundle/_screenshots/`.** As notas ficam em
  `.design-sync/.cache/review/` e sobrevivem, mas os sheets nao. Capture _depois_
  do build e leia os sheets antes do proximo build completo — ou recapture.
  Rebuild pontual de preview (`lib/preview-rebuild.mjs --components X`) nao apaga.

## Correcoes aplicadas nesta sync

- **Textos visíveis sem acento.** Os fontes foram escritos em ASCII e vários
  _defaults_ que aparecem na tela saíram errados ("Situacao do periodo",
  "Pendencias", "Possivel duplicidade", "provavel"). Corrigidos um a um.
  **Regra:** comentário e JSDoc podem ficar em ASCII, mas toda string que o
  usuário lê carrega acento. Ao adicionar componente, confira os defaults.
- **Linha dupla em lista encostada.** `Card variant="flush"` desenha borda e
  cada linha desenha `borderTop`: a primeira linha ficava com traço duplo.
  Resolvido em CSS (`.sbf-flush > *:first-child { border-top: 0 }`) para o
  consumidor não precisar saber disso. `Field` ganhou `divider` pelo mesmo
  motivo, no fim da lista.
- **`MetricValue size="hero"` fixava `--hero-text`.** Fora do card verde isso
  produzia texto branco no branco. A cor saiu do tamanho; quem usa sobre o hero
  passa `color="var(--hero-text)"` — e `MoneyMetricCard` já faz isso.
- **`ConnectionStatusCard` oferecia "Sincronizar agora" desconectado.** A ação
  primária agora segue o estado (`Conectar` / `Sincronizando…` / `Sincronizar`).
- **Eixo `FILL` dos Material Symbols.** A URL do Google Fonts herdada do canvas
  fixava `FILL` em `0` (`...FILL,GRAD@24,400,0,0`), entao `<Icon filled>` nao
  fazia nada e a aba ativa do `BottomNav` ficava identica as inativas. Corrigido
  para a faixa `0..1` (`@24,400,0..1,0`), verificado no sheet do `Icon`. Se
  alguem recopiar a URL do canvas, o defeito volta.

## Decisoes de preview

- `Pill` teve a celula `Tamanhos` removida: `xs` e `sm` renderizam praticamente
  identicos e a celula nao ensinava nada. O prop segue documentado no `.d.ts`.

## Estado desta sync

- 36 componentes, todos com preview autorado em `.design-sync/previews/` e nota
  `good` em todas as células. Nenhum floor card.
- `package-validate.mjs` sai 0 sem nenhum aviso além do `[FONT_REMOTE]` esperado.
- Overrides de apresentação (`cardMode: column`) em `AppSidebar`, `BottomNav`,
  `MobileHeader`, `TopBar` e `Sheet` — histórias largas que o grid recortava.
- `.design-sync/build.sh` existe porque o conversor precisa de caminhos
  absolutos neste repo; use-o em vez de montar a linha de comando à mão.

## Riscos de re-sync

- **Os tokens podem divergir do canvas.** Se alguem editar o canvas, `tokens.css`
  nao acompanha sozinho. Antes de um re-sync grande, rebaixe os blocos
  `[data-sbf-theme]` do `.dc.html` e compare.
- **`formatBRL` arredonda para o real inteiro por padrao.** Se o produto passar a
  exigir centavos em painel, os previews mudam de aparencia e todas as notas
  visuais devem ser refeitas.
- **`apps/web` não usa este pacote.** Adotar `@sbf/design-system` na web troca a
  cara do produto inteiro e é decisão separada; até lá o design system e o app
  divergem de propósito, e ninguém deve "corrigir" um pelo outro sem combinar.
- **A ordem das seções da `AppSidebar` é contrato de produto** (§6 do prompt
  mestre), não preferência estética. Mudar exige revisar o prompt.
- As fontes vem da rede. Um ambiente sem acesso ao Google Fonts renderiza tudo em
  fallback de sistema e as notas visuais ficam invalidas — confira o
  `[FONT_REMOTE]` antes de confiar num sheet.
