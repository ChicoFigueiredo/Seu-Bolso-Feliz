# Spec 18: roteiros escritos, agenda, parâmetros e suspensão

Continua as [spec 16](16-desktop-roteiros-escritos-fatia-1.md) e
[spec 17](17-desktop-roteiros-escritos-fatia-2-meu-pluggy.md). Os termos (**Agenda**,
**Parâmetros do roteiro**, **Baixa forçada**, **Suspensão da agenda**) estão no `CONTEXT.md`.
Esta entrega o **núcleo**: o manifesto, a agenda, os parâmetros, a suspensão e o terminal. O
descarte por conteúdo vem do inventário (`NoInventario`) e a janela fica para a frente seguinte.

## O problema

O contracheque do dia 30 **substitui** o do dia 20 no mesmo lugar do portal, então "já baixei"
não se decide pelo nome nem pela data: se decide pelo **conteúdo**. E um roteiro precisa rodar
em dias e horários certos, sem disparar o que ficou para trás antes de existir e sem errar a
senha de hora em hora quando o portal recusa a entrada.

## Campos novos do manifesto

| Campo                       | Tipo                            | Regra                                                                                                                          |
| --------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| `agenda`                    | lista de entradas (opcional)    | Até 16. Substitui `agenda_horas`; **os dois juntos tornam o manifesto inválido**. `agenda_horas` continua aceito sozinho.      |
| `horarios`                  | lista de `"HH:MM"` (da entrada) | Obrigatório, de 1 a 24, `00:00` a `23:59` com duas casas em cada parte, sem repetir.                                           |
| `dias_do_mes`               | lista de inteiros (da entrada)  | Opcional, `1` a `31`, sem repetir, não vazio. Sem ele, todo dia. Dia 31 num mês sem 31 **não dispara** nesse mês (sem ajuste). |
| `parametros` (da entrada)   | mapa nome → valor               | Opcional. Só nomes declarados em `parametros` do manifesto, com valor do tipo certo.                                           |
| `parametros` (do manifesto) | lista de declarações            | Até 8. `nome` `[a-z][a-z0-9_]{0,31}` único; `tipo` `mes` ou `booleano`; `padrao` opcional (`"0"` e `false`).                   |

`mes` aceita `"AAAA-MM"` (ano 1 a 9999) ou o relativo ao mês corrente: `"0"`, `"-1"`, `"-12"`
(inteiro menor ou igual a zero, em texto, sem `+`, sem zero à esquerda). `booleano` é `true` ou
`false`. Nunca há segredo aqui: só mês e sim/não. Quando se declaram `de` e `ate` (ambos `mes`),
vale `de` ≤ `ate` e no máximo **60 meses** no intervalo (contando as duas pontas).

```json
{
  "nome": "contracheque-exemplo",
  "hosts": ["portal.exemplo.test"],
  "url_inicial": "https://portal.exemplo.test/entrar",
  "mfa": { "tipo": "nenhum" },
  "movimenta_dinheiro": false,
  "classe": "consulta",
  "agenda": [
    {
      "horarios": ["04:00", "12:00", "20:00"],
      "dias_do_mes": [20, 21, 30, 1],
      "parametros": { "de": "-1", "ate": "0" }
    }
  ],
  "parametros": [
    { "nome": "de", "tipo": "mes", "padrao": "0" },
    { "nome": "ate", "tipo": "mes", "padrao": "0" },
    { "nome": "forcar", "tipo": "booleano", "padrao": false }
  ]
}
```

O JSON canônico (o que o banco guarda e o checksum cobre) **omite** `agenda` e `parametros`
quando vazios, então as versões já importadas mantêm o mesmo checksum.

## Quem entrega os parâmetros

- a **agenda**: os `parametros` da entrada que venceu;
- o **terminal**: `sbf roteiros escrito executar <nome> --inventario <cofre> [--de M] [--ate M] [--forcar]`;
- a **janela**: o botão "Executar agora" (frente seguinte).

O relativo conta a partir do mês do **instante previsto** quando a execução vem da agenda (o
mês para o qual a entrada foi marcada, mesmo que o computador só volte depois) e do mês de
agora na execução manual, sempre no fuso do usuário (UTC−3). O roteiro só vê o `"AAAA-MM"` já
resolvido.

No Luau, a tabela global **somente leitura** `parametros` traz os valores resolvidos
(`parametros.de == "2026-10"`, `parametros.forcar == false`); sem parâmetro declarado ela existe
e fica vazia. `util.meses(de, ate)` devolve a lista de `"AAAA-MM"` em ordem (no máximo 60; erro
se `de` > `ate`). Os parâmetros resolvidos ficam no histórico (`parametros_json`).

`forcar` na linha de comando é a **Baixa forçada** do inventário e não depende de o manifesto
declarar o parâmetro `forcar`; se ele é declarado, o roteiro também o vê verdadeiro.

## Quando a agenda dispara

Uma entrada está **devida** se existe um instante agendado `s` com `base < s <= agora`, onde
`base` é o maior entre (a) o início da última execução **daquela entrada** (da versão em vigor) e
(b) o instante em que a versão foi **ativada**. Roda **uma vez**, com o `s` mais recente, não
uma por horário perdido. Duas entradas devidas na mesma passada rodam em sequência, na ordem da
lista. Execução manual (botão, terminal) não conta como a execução de nenhuma entrada. Quem
está rodando, esperando o usuário ou **suspenso** nunca é devido.

Falha de parâmetros numa execução de agenda (por exemplo `de` depois de `ate` num par
relativo/absoluto misto) deixa uma execução `falhou` no histórico, para a entrada não repetir a
cada passada do worker.

## Suspensão da agenda

Se o roteiro termina em `navegador.entrada_recusada(motivo)` (verbo novo; o motivo passa pela
limpeza de segredo e vai a no máximo 200 caracteres), a execução fica no estado
`entrada_recusada`, a agenda **do roteiro** é suspensa (`suspenso_unix` e `suspenso_motivo` em
`roteiro_escrito`, mais uma linha no histórico da versão) e `escritos_devidos` deixa de
devolvê-lo. O erro não se captura com `pcall`. O modo teste nunca suspende.

- Só o usuário retoma: `sbf roteiros escrito retomar <nome>` (terminal interativo, sem
  confirmação digitada); a janela vem depois. O MCP, o worker e a IA **nunca** retomam
  (`tests/autoridade.rs` proíbe `retomar_agenda` nesses crates).
- Execução manual de roteiro suspenso é permitida e, se der certo, **não retoma** sozinha.
- A suspensão acompanha o roteiro, não a versão: ativar uma versão nova não a retoma.

## Banco

Migração **aditiva** no `roteiros.sqlite` (nada se apaga; o banco real nunca é aberto pelos
testes):

- `roteiro_escrito`: `suspenso_unix`, `suspenso_motivo`;
- `execucao_escrita`: `entrada_agenda`, `previsto_unix`, `parametros_json` e o estado
  `entrada_recusada`. Como um `CHECK` não se altera, um banco antigo tem a tabela reconstruída
  pela receita do SQLite (chaves estrangeiras desligadas, cópia, troca), numa transação, sem
  perder execuções nem eventos. Os gatilhos de imutabilidade das versões não são tocados.

## Matriz de verificação

| Id    | Verificação                                                                                                    | Onde                                   |
| ----- | -------------------------------------------------------------------------------------------------------------- | -------------------------------------- |
| A1-01 | Horário e dia do mês: catch-up roda uma vez só, com o `s` mais recente                                         | `src/agenda.rs` (testes)               |
| A1-02 | Dia 31 não dispara em mês sem 31; 29/02 só em bissexto; virada de mês e de ano                                 | `src/agenda.rs`                        |
| A1-03 | Fuso: 21h do dia 30 (UTC−3) é "dia 30" (00h UTC do dia 1), não "dia 1"                                         | `src/agenda.rs`                        |
| A1-04 | Ativação posterior ao instante agendado não dispara; a ativação entra na base                                  | `src/agenda.rs`, `tests/agenda.rs`     |
| A1-05 | Manifesto: `agenda` + `agenda_horas` inválido; 16 entradas sim, 17 não; horário, dia e parâmetro de borda      | `tests/agenda.rs`                      |
| A1-06 | Manifesto antigo mantém o mesmo JSON canônico (checksum)                                                       | `tests/agenda.rs`                      |
| A1-07 | Parâmetros: `AAAA-MM` e relativo, de ≤ ate, 60 meses, nome fora da declaração e tipo errado são erro           | `src/parametros.rs`, `tests/agenda.rs` |
| A1-08 | Luau: `parametros` já resolvidos, somente leitura (alterar é erro), vazia sem declaração; `util.meses`         | bancada `tests/luau_parametros.rs`     |
| A1-09 | `navegador.entrada_recusada` encerra com erro próprio, não se captura com `pcall`, limpa segredo, corta em 200 | bancada `tests/luau_parametros.rs`     |
| A1-10 | Banco antigo migra sem perder linhas nem eventos, aceita `entrada_recusada` e mantém os gatilhos               | `tests/agenda.rs`                      |
| A1-11 | Suspensa não é devida; retomar a devolve; manual de suspensa roda e não retoma; modo teste não suspende        | `tests/agenda.rs`                      |
| A1-12 | A passada da agenda roda cada entrada devida uma vez, com os parâmetros dela e o previsto no histórico         | `tests/agenda.rs`                      |
| A1-13 | Baixa por conteúdo: repetido é descartado (conta em `descartados`, não em `novos`); forçada baixa de novo      | `tests/agenda.rs`                      |
| A1-14 | CLI: montagem dos argumentos, uso errado, `--forcar` e `retomar` sem terminal recusados e nada é alterado      | `crates/sbf/src/roteiros.rs` (testes)  |
| A1-15 | O MCP, o worker e a IA não citam `retomar_agenda`                                                              | `tests/autoridade.rs`                  |
| A1-16 | O `meu-pluggy-atualizar` (v2) tem a agenda 04:00, 12:00 e 20:00 todo dia e o `.luau` não mudou                 | `tests/exemplos.rs`                    |

## Fora desta entrega

- A janela: botão "Executar agora" com período, confirmação da Baixa forçada, retomar a agenda e
  mostrar a agenda do manifesto (`texto_da_agenda` ainda só conhece `agenda_horas`).
- A proposta de Baixa forçada pelo MCP (o MCP só propõe).
- Pausa de MFA/CAPTCHA numa execução pelo terminal: a pausa espera o `continuar` que a janela
  grava, então pelo terminal o prazo da pausa (30 minutos) é o limite.
- Contracheque no portal real: segue um modelo com placeholders em
  `crates/sbf-roteiros/exemplos/contracheque-modelo-portal-real.md`, com o exemplo comentado de
  agenda.
