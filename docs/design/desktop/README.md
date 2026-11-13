# Tokens do desktop do Seu Bolso Feliz

Tradução dos 7 temas do `@sbf/design-system` para o formato de tokens que a shell egui compartilhada (`bancada-casca`, ADR-018) lê. O esquema é o mesmo do Chess Studio (`docs/design/tokens.json` e `themes.json` de lá), para o mesmo leitor servir aos dois apps. Os tokens específicos de tabuleiro do Chess (`board-*`, `eval-*`, `last-move`, `hover-ring`) não entram.

Arquivos:

- `tokens.json`: 39 papéis de cor (valores `dark` = noite e `light` = sálvia), tipografia, espaço, raio, sombra e tamanhos.
- `temas.json`: os 7 temas. Sálvia (base `light`) e noite (base `dark`) têm `colors` vazio porque são as próprias bases; os outros cinco trocam só os papéis que diferem da base.

Fonte dos valores: `packages/design-system/src/tokens.css`, verbatim. Nenhuma cor foi inventada: tudo que não vem direto do SBF está na seção "Derivações".

Os dois arquivos são gerados por `scripts/desktop/gerar-temas.py` e conferidos por `scripts/desktop/contraste.py` (veja "Como regenerar").

## Mapeamento: token do SBF para papel do Chess

| Papel (Chess)    | Token do SBF                  | Observação                                              |
| ---------------- | ----------------------------- | ------------------------------------------------------- |
| `surface-0`      | `--bg`                        | Fundo da janela                                         |
| `surface-1`      | `--surface`                   | Painéis, dock, diálogos                                 |
| `surface-2`      | `--surface-2`                 | Ribbon, cabeçalhos, campos, barra de status             |
| `surface-3`      | `--hover-2`                   | Hover. `--hover` fica sem papel (é quase igual a surface-1) |
| `surface-4`      | `--line`                      | Derivado (D1)                                           |
| `border`         | `--line`                      | Decorativo. `--line-soft` fica sem papel                |
| `border-strong`  | `--text-3`                    | Derivado (D2)                                           |
| `ink`            | `--text`                      |                                                         |
| `ink-muted`      | `--text-3`                    | `--text-2` e `--text-4` ficam sem papel                 |
| `ink-disabled`   | `--text-5`                    | Isento de contraste                                     |
| `accent`         | `--primary`                   | O verde é a cor de ação, no lugar do âmbar do Chess     |
| `accent-hover`   | `--primary-dark`              |                                                         |
| `accent-pressed` | derivado                      | D3                                                      |
| `on-accent`      | `--on-primary`                |                                                         |
| `accent-subtle`  | `--primary-soft`              |                                                         |
| `accent-border`  | `--primary-line`              | Extra do SBF (contorno de chip), decorativo             |
| `accent-ink`     | `--primary-ink`               |                                                         |
| `focus-ring`     | `--text`                      | Derivado (D4)                                           |
| `info`           | `--secondary-ink`             | Texto do azul ardósia (D5)                              |
| `info-subtle`    | `--secondary-soft`            | Extra                                                   |
| `info-border`    | `--secondary-line`            | Extra, decorativo                                       |
| `success`        | `--success`                   |                                                         |
| `success-subtle` | `--primary-soft`              | Extra, como em `toneStyles.success`                     |
| `warning`        | `--warn`                      | Âmbar                                                   |
| `warning-subtle` | `--warn-soft`                 | Extra                                                   |
| `warning-border` | `--warn-line`                 | Extra, decorativo                                       |
| `danger`         | `--danger`                    |                                                         |
| `danger-subtle`  | `--warn-soft`                 | Extra, como em `toneStyles.danger` (o SBF não tem fundo vermelho próprio) |
| `gold`           | `--accent`                    | Extra. O `--accent` do SBF é o dourado, não a cor de ação; por isso o nome mudou, para não colidir com o `accent` do Chess |
| `gold-subtle`    | `--accent-soft`               | Extra                                                   |
| `gold-ink`       | `--accent-ink`                | Extra. `--accent-ink-2` fica sem papel                  |
| `hero-bg`, `hero-text`, `hero-sub`, `hero-accent` | `--hero-*` | Extra: faixa dos dois números principais. `--hero-soft` fica sem papel |
| `toast-bg`, `toast-text`, `toast-icon` | `--toast-*` | Extra: aviso transitório                       |
| `scrim`          | derivado                      | D6                                                      |

Sem papel no desktop (por ora): `--hover`, `--line-soft`, `--text-2`, `--text-4`, `--primary-soft-2`, `--primary-tint`, `--paid-bg`, `--secondary` (o pigmento puro), `--secondary-text*`, `--secondary-hover`, `--accent-ink-2`, `--hero-soft`, `--header-bg`, `--nav-bg` (translúcidos, pensados para o web). Se uma tela precisar de um deles, o papel entra nos dois arquivos, nunca como hex solto.

Não vieram do SBF web, de propósito: os raios 8/12/16/22 px, os espaços de 20 e 44 px e as sombras `card/raised/sheet`. A geometria é a densa do Chess (raio 2/4/6/8, grade de 4 px, controles de 32 e 24 px). Movimento: o SBF web usa 0,18 a 0,35 s; no desktop vale a regra do Chess (100 a 150 ms, só em hover e abertura de menu, nada animado com "reduzir movimento").

## Tipografia e ícones

- Interface em **Manrope** (400 a 800). A escala é a do Chess (`caption` 12/16, `ribbon-label` 12/14, `body` 14/20, `body-strong`, `body-large` 16/22, `subtitle`, `title` 20/28, `display` 28/36), com Manrope no lugar de Inter.
- Números de dinheiro: `amount` (14/20, 600) e `amount-large` (20/28, 700), sempre com números tabulares (`tnum`; Manrope tem).
- **Instrument Serif** só no estilo `hero-number` (40/44, peso 400, único que ela tem), e só para os dois números mais importantes da tela. Nenhum outro texto usa a serifa.
- `mono` é **JetBrains Mono** (OFL, a mesma do Chess Studio), só para `log` e identificadores técnicos; o SBF web não tem fonte mono, então é decisão do desktop. Vem com `ui-monospace, monospace` de reserva.
- Ícones: **Material Symbols Rounded**, como no web, peso 400, FILL 0, GRAD 0. `icon-s` 16 px (opsz 20, o mínimo do eixo) e `icon-l` 32 px.
- O campo `type.fonts` fica vazio, como no Chess: o embutimento dos arquivos de fonte (e o registro de licença, OFL) é trabalho do app, não do token.

## Derivações

- **D1. `surface-4` = `--line`.** O SBF não tem um segundo degrau de hover. `--line` é o próximo neutro mais escuro depois de `--hover-2` em todos os 7 temas, então "pressionado" fica mais fundo que "hover" sem inventar cor. Em tema escuro, também é um degrau acima.
- **D2. `border-strong` = `--text-3`.** O SBF só tem linhas decorativas (`--line`, ~1,3:1). O contorno de controle precisa de 3:1 sobre surface-0, 1 e 2. Testei `--text-4` primeiro: reprova 3:1 em 5 dos 7 temas (de 2,66 a 2,98). `--text-3` passa nos 7 (com os valores atuais do `tokens.css`, já corrigidos).
- **D3. `accent-pressed`.** Nos claros: 30% de mistura entre `--primary-dark` e `--text` (mais escuro que o hover). Nos escuros: 15% de mistura entre `--primary` e `--bg` (mais escuro que o accent, como no Chess; 30% reprovava `on-accent` em noite e grafite, por isso 15%).
- **D4. `focus-ring` = `--text`**, a mesma regra do Chess (cor de `ink`). Vale o anel de 2 px com 1 px de superfície por dentro: sobre o botão primário o anel se separa pela folga, então o par anel/accent não é exigido.
- **D5. `info` = `--secondary-ink`**, não o pigmento `--secondary`: é o tom que o SBF usa para texto azul (`toneStyles.secondary.fg`) e passa onde o pigmento reprovava (ocre, linho, menta). A leitura continua "informação e IA, nunca identidade".
- **D6. `scrim`.** Escuros: `#000000` a 55% (o valor do Chess). Claros: `--text` a 40%. O SBF não tem véu de modal.
- **D7. Sombras.** A forma é a do Chess (blur 12/24/40, offsets 4/8/16). A cor nos claros é a do SBF (`rgba(24,35,31,…)`, de `--sbf-shadow-*`); nos escuros, preto, como no Chess.
- **D8. Bases.** `light` = sálvia (tema canônico) e `dark` = noite (o escuro que o SBF assume com `prefers-color-scheme: dark`). Grafite é escuro sobre a base noite.
- **D9. Famílias de tema** (campo `family`): Padrão (sálvia, noite), Claros (ocre, azul, linho, menta), Escuros (grafite). As frases de `inspiration` são nossas, não do SBF.

## Regras de cor do SBF que continuam valendo

1. **Vermelho (`danger`) só para risco real**: atraso, falha, Ação destrutiva. Nunca para "gasto" ou "saída" comum.
2. **Compromisso próximo em âmbar (`warning`)**, nunca vermelho.
3. **Dourado (`gold*`) celebra, em doses pequenas**: conquista e meta. Nunca como cor de ação nem de texto corrido.
4. **Cor nunca é o único sinal.** `success`, `warning`, `danger`, `info` e `gold` vêm sempre com ícone ou palavra; `ink-disabled` idem. Um controle ligado e desabilitado continua mostrando que está ligado.
5. **Estimativa nunca como fato** (CONTEXT.md). "Livre após compromissos" leva a marca de estimativa sempre que houver compromisso sem valor confirmado, em texto ("estimativa"), não só em cor. "A conferir" é outra marca (fato ainda não revisado) e não se confunde com estimativa. Nenhum token de cor sinaliza isso sozinho.
6. **Nunca escreva um hex na interface**: use o papel. Tema novo só entra se passar nos pares (ou se a falha ficar registrada aqui).

## Como regenerar

A partir da raiz do repositório:

```bash
python3 -I scripts/desktop/gerar-temas.py   # lê packages/design-system/src/tokens.css, escreve tokens.json e temas.json aqui
python3 -I scripts/desktop/contraste.py     # confere os pares; sai com código 1 se algum reprovar
```

Os valores de cor vêm do `tokens.css`; as derivações (D1 a D9) estão no próprio `gerar-temas.py`. Mudou cor no `tokens.css`: rode os dois comandos. Não edite os JSON à mão.

## Checagem de contraste

WCAG 2. `contraste.py` lê os dois JSON e funde cada tema com a base.

Pares: texto 4,5:1 (`ink` sobre surface-0 a 4 e accent-subtle; `ink-muted` sobre surface-0 a 3 e accent-subtle; `accent-ink` sobre surface-1 a 3 e accent-subtle; `on-accent` sobre accent, hover e pressed; `info`, `success`, `warning` sobre surface-1 a 3; `danger` sobre surface-1 a 4; cada estado sobre o seu `-subtle`; `gold-ink` sobre `gold-subtle`; textos de `hero` e `toast`) e 3:1 (`border-strong` sobre surface-0 a 2; `focus-ring` sobre surface-0 a 4 e accent-subtle; `accent` como marca sobre surface-0 a 2; `toast-icon`). `border` e `*-border` são decorativos e ficam isentos.

**Resultado final: 371 pares conferidos, 0 reprovados, nos 7 temas.**

### Correção na fonte (rodada 2)

A primeira checagem reprovou 22 pares, todos em temas claros e todos em cores do próprio SBF. Por decisão do CEO, a correção foi feita no `tokens.css`: só a luminosidade (HSL, mesmo matiz e saturação) de `--text-3` e `--success` nos 5 temas claros e de `--warn` no ocre, o mínimo para o pior par de cada cor chegar a 4,6:1 ou mais. O `:root` (cópia da sálvia) recebeu os mesmos valores. Os temas escuros não mudaram. `--text-3` continua bem mais claro que `--text-2` (contraste de `--text-2` sobre `--bg` entre 8,4 e 9,1:1, contra 4,6:1 do `--text-3`).

"Pior par" é o menor contraste da cor entre as superfícies onde ela aparece (fundo, superfícies 1 a 3, `accent-subtle`/`primary-soft`, ou `warn-soft` no caso do `--warn`); em todos os casos o pior era sobre `primary-soft` (ou `warn-soft`).

| Tema   | Token      | Antes → depois    | Pior par: antes → depois |
| ------ | ---------- | ----------------- | ------------------------ |
| sálvia | `--text-3` | #66736d → #616d68 | 4,24 → 4,61              |
| sálvia | `--success`| #2f7d5a → #2d7756 | 4,28 → 4,63              |
| ocre   | `--text-3` | #746b5a → #716858 | 4,42 → 4,61              |
| ocre   | `--success`| #367d57 → #337552 | 4,17 → 4,63              |
| ocre   | `--warn`   | #93601f → #905e1e | 4,50 → 4,65              |
| azul   | `--text-3` | #63737e → #5c6b76 | 4,11 → 4,61              |
| azul   | `--success`| #2f7d5a → #2c7554 | 4,19 → 4,66              |
| linho  | `--text-3` | #6e6f66 → #696a61 | 4,27 → 4,60              |
| linho  | `--success`| #3a7a5c → #377458 | 4,28 → 4,64              |
| menta  | `--text-3` | #5f7268 → #5c6e65 | 4,37 → 4,61              |
| menta  | `--success`| #2a8060 → #277759 | 4,10 → 4,62              |

Efeito colateral: qualquer cópia do hex antigo fora do `tokens.css` (por exemplo `apps/web/public/offline.html`, `#66736d`) fica defasada em relação ao tema.
