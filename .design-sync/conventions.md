# Seu Bolso Feliz — como construir com este design system

Central financeira pessoal em português do Brasil. A linguagem visual chama-se
**prosperidade tranquila**: calma, clareza, progresso. O produto lida com
dinheiro, assunto que gera ansiedade — o desenho não pode parecer cobrança
bancária, painel de risco nem fintech neon.

## Envolva tudo em `SbfTheme`

`SbfTheme` escreve o atributo `data-sbf-theme` e liga a tipografia. Sem ele os
componentes ainda renderizam (o `:root` de `styles.css` aplica o tema `salvia`
como padrão), mas **trocar de tema deixa de funcionar** e o `background` da
página não é pintado.

```jsx
<SbfTheme theme="salvia" fullBleed>
  <div style={{ display: "flex" }}>
    <AppSidebar items={nav} active="hoje" />
    <main style={{ flex: 1, padding: "var(--sbf-space-6)" }}>…</main>
  </div>
</SbfTheme>
```

Sete temas: `salvia` (padrão), `ocre`, `azul`, `linho`, `menta` (claros),
`noite`, `grafite` (escuros).

## O idioma: tokens CSS, não classes utilitárias

**Não existe vocabulário de classes.** Nenhum Tailwind, nenhum BEM. Os
componentes usam estilo inline lendo os tokens abaixo via `var(…)`, e o seu layout deve fazer o
mesmo. Escrever uma cor literal (`#245C4A`, `green`) quebra os sete temas.

| Família             | Tokens                                                                                                                            |
| ------------------- | --------------------------------------------------------------------------------------------------------------------------------- |
| Superfície          | `--bg` `--surface` `--surface-2` `--muted` `--hover` `--hover-2` `--line` `--line-soft`                                           |
| Texto               | `--text` `--text-2` `--text-3` `--text-4` `--text-5`                                                                              |
| Direção / progresso | `--primary` `--primary-dark` `--primary-ink` `--primary-soft` `--primary-soft-2` `--primary-line` `--primary-tint` `--on-primary` |
| Card de destaque    | `--hero-bg` `--hero-text` `--hero-sub` `--hero-accent` `--hero-soft`                                                              |
| Informação / IA     | `--secondary` `--secondary-ink` `--secondary-soft` `--secondary-line` `--secondary-hover` `--secondary-text`                      |
| Conquista           | `--accent` `--accent-soft` `--accent-ink` `--accent-ink-2`                                                                        |
| Estado              | `--success` `--warn` `--warn-soft` `--warn-bg` `--warn-line` `--danger` `--paid-bg`                                               |
| Cromo               | `--header-bg` `--nav-bg` `--toast-bg` `--toast-text` `--toast-icon`                                                               |
| Escalas             | `--sbf-font-sans` `--sbf-font-serif` `--sbf-font-icon` · `--sbf-radius-sm                                                         | md  | lg  | xl  | pill`·`--sbf-space-1`…`--sbf-space-8`·`--sbf-shadow-card | raised | sheet`·`--sbf-motion-fast | base | slow` `--sbf-ease` |

## Regras de cor que não são negociáveis

- **Vermelho (`--danger`) só para risco real**: atraso, falha, ação urgente.
  Gastar dinheiro não é erro — uma despesa comum fica neutra, com o sinal
  negativo bastando.
- **Compromisso que se aproxima é âmbar (`--warn`)**, nunca vermelho.
- **Dourado (`--accent`) celebra**, em doses pequenas. Nunca como texto pequeno
  sobre branco — o contraste não passa; use `--accent-ink` sobre `--accent-soft`.
- **Cor nunca é o único indicador de estado** (WCAG 2.2 AA). Todo `Pill` leva
  texto; ícone é o terceiro canal.
- **Estimativa nunca se apresenta como fato**: `MoneyMetricCard` tem `estimated`
  e `Field` tem `confidence` justamente para isso.

## Dinheiro e tipografia

`formatBRL(4350)` → `R$ 4.350` (sem centavos por padrão; `{ cents: true }` liga).
Todo número monetário vai em `MetricValue`, que aplica `tabular-nums` — uma
coluna de valores alinha por dígito. A serifa (`hero`, `display`) é reservada aos
dois números mais importantes da tela; `size="hero"` **não fixa cor**, então
sobre o card verde passe `color="var(--hero-text)"`.

Ícones são Material Symbols Rounded via `<Icon name="savings" />` — o `name` é o
nome do glifo. Se você vir a palavra escrita na tela, a fonte não carregou.

## Onde está a verdade

- `_ds/<pasta>/styles.css` e o que ele importa — a definição dos sete temas.
- `components/<grupo>/<Nome>/<Nome>.prompt.md` — API e uso de cada componente.
- Grupos: `general` (SbfTheme), `primitives`, `financial`, `chrome`.

## Exemplo idiomático

```jsx
<SbfTheme theme="salvia">
  <div
    style={{
      display: "grid",
      gridTemplateColumns: "repeat(auto-fit,minmax(232px,1fr))",
      gap: "var(--sbf-space-4)",
    }}
  >
    <MoneyMetricCard
      variant="hero"
      label="Disponível agora"
      value={formatBRL(4350)}
      caption="Somando 3 contas correntes · atualizado hoje, 08:12"
      actionLabel="Como chegamos neste valor?"
    />
    <MoneyMetricCard
      label="Livre após compromissos"
      value={formatBRL(985)}
      estimated
      note="3 despesas ainda sem valor confirmado. O número pode mudar."
    />
  </div>
</SbfTheme>
```
