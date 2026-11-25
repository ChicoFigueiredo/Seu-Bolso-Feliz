# Spec 16 — Roteiros escritos em Luau, fatia 1: motor e contracheque

**Data:** 24/11/2026 · **Estado:** decidida (grill com o CEO em 2026-11-24); em implementação na branch `feat/roteiros-luau` · **ADR:**
[ADR-022](../arquitetura/2026-11-24-adr-022-roteiros-escritos-e-motor-na-bancada.md) ·
**Depende de:** [ADR-016](../arquitetura/2026-11-12-adr-016-roteiros.md),
[ADR-021](../arquitetura/2026-11-17-adr-021-regras-de-reconhecimento.md), spec 10 (frente C1)

Termos do glossário (`CONTEXT.md`): **Roteiro gravado**, **Roteiro escrito**, **Verbo**,
**Classe de ação**, **MFA por e-mail**, **Conexão**, **Coleta**, **Worker**, **Achado**.

## 1. Escopo

A fatia 1 entrega o motor e o primeiro roteiro escrito, o **contracheque**, provado inteiro no
**site falso**. Nenhum teste toca site real.

- O crate `bancada-navegador` (repositório `bancada`): Chrome visível por CDP, página
  simplificada, trait `Navegador` e host Luau.
- No `sbf-roteiros`: o manifesto, o ciclo de vida (rascunho, teste, ativo), o executor, a
  lista de bloqueio nos verbos, a injeção de senha e CPF, a busca do código de MFA por e-mail
  no Gmail, o destino do download no inventário.
- Execução por **botão** ("Executar agora") e por **agenda** (`agenda_horas`, no worker).
- Ferramentas do MCP que **só propõem**.
- O roteiro de contracheque contra o site falso e um **modelo comentado** para o portal real,
  com placeholders. Sem URL nem seletor reais.

### Fora desta fatia

- **Meu Pluggy (fatia 2).** O manifesto já aceita `classe: "atualizar_conexao"` e a lista de
  bloqueio já a trata, mas nenhum roteiro dela é entregue aqui.
- **Portal real do contracheque.** A URL ainda não foi informada. Entra quando o usuário a
  passar, sem seletor inventado.
- Código do aplicativo autenticador (TOTP) automático, banco e consentimento do Pluggy:
  continuam manuais.
- **Conta do Gmail do MFA por e-mail.** A fatia usa a conta de e-mail padrão do chaveiro.
  Escolher a conta por roteiro (um campo no manifesto) fica para depois.
- **Commit da bancada no histórico da execução.** A fatia mostra o carimbo no binário e na
  janela (RE-32); gravá-lo também em cada execução fica como evolução futura.
- Janela de edição do Luau: a fatia 1 importa os arquivos pela linha de comando, como o
  `regras importar`.
- Windows (D16 da spec 11).

## 2. Arquitetura

```
 SBF (política)                               bancada (mecânica)
 ──────────────                               ───────────────────
 janela ─┐                                    bancada-navegador
 worker ─┼─> sbf-roteiros                       ┌───────────────────────────────┐
 MCP ────┘     │                                │ host Luau (sandbox, tetos)    │
  (só propõe)  │  manifesto + .luau (rascunho)  │   navegador.* / util.*        │
               │  ciclo de vida, roteiros.sqlite│        │ verbo                │
               │  executor ─────────────────────┼──> trait Navegador            │
               │   · lista de bloqueio          │        │                      │
               │   · allowlist de hosts         │   CDP ─┴─> Chrome visível     │
               │   · rótulo -> Segredo          │   (perfil dedicado, sem       │
               │   · código de MFA (Gmail)      │    disfarce, página           │
               │  achados no inventário         │    simplificada sem valores)  │
               └────────────────────────────────┴───────────────────────────────┘
                     chaveiro (rótulo -> Segredo)       Gmail (busca estreita)
```

O SBF implementa o que o host pede (a lista de bloqueio, o chaveiro, o Gmail) por trás de
traits. A bancada não conhece chaveiro do SBF, Gmail nem regra financeira. O executor do SBF
confere a lista de bloqueio **antes** de chamar o verbo e o navegador a confere **de novo**
no clique (defesa em duas camadas).

## 3. Verbos

O roteiro chama só a tabela `navegador` e a tabela `util`. O ambiente Luau não tem `os`, `io`,
`require`, `loadstring`, `debug`, JavaScript nem rede.

| Verbo                                                  | Retorno                   | Observação                                                                                  |
| ------------------------------------------------------ | ------------------------- | ------------------------------------------------------------------------------------------- |
| `navegador.abrir(url)`                                 | nada                      | URL conferida na lista de bloqueio e no host permitido                                      |
| `navegador.esperar(seletor, segundos?)`                | bool                      | espera ficar visível; padrão 15 s, máximo 120 s                                             |
| `navegador.existe(seletor)`                            | bool                      | visível agora?                                                                              |
| `navegador.clicar(seletor)`                            | nada                      | lista de bloqueio no executor e no navegador                                                |
| `navegador.texto(seletor)`                             | string                    | texto **visível** do elemento; nunca o valor de `<input>` nem de senha                      |
| `navegador.tabela(seletor)`                            | lista de listas de string |                                                                                             |
| `navegador.preencher(seletor, texto)`                  | nada                      | recusa campo de senha                                                                       |
| `navegador.preencher_senha(seletor, rotulo)`           | nada                      | segredo do chaveiro pelo rótulo; confere o host                                             |
| `navegador.preencher_cpf(seletor)`                     | nada                      | CPF único do chaveiro; só digita no host aprovado                                           |
| `navegador.preencher_codigo_do_email(seletor, opcoes)` | nada                      | `opcoes.janela_segundos` (padrão 120, máximo 300); remetente e formato vêm do **manifesto** |
| `navegador.baixar(seletor, nome?)`                     | string (nome do arquivo)  | só o nome, nunca o conteúdo                                                                 |
| `navegador.pausa(motivo)`                              | nada                      | passa a vez ao usuário; volta quando ele continua; erro se ele aborta                       |
| `navegador.registrar(mensagem)`                        | nada                      | log sem segredo (o motor limpa)                                                             |
| `util.hoje()`                                          | "AAAA-MM-DD"              | fuso do usuário (UTC−3), nunca o UTC puro                                                   |
| `util.somar_meses(data, n)`                            | "AAAA-MM-DD"              |                                                                                             |

Limites da VM: teto de instruções (interrupção cooperativa), de memória, de tempo total, de
chamadas a verbos e de tamanho do fonte (64 KiB). Estouro é erro claro e o roteiro termina.
Erro de verbo nunca carrega segredo.

## 4. Manifesto

JSON ao lado do `.luau`, importado como o `regras importar`.

```json
{
  "nome": "contracheque-exemplo",
  "descricao": "Baixa o demonstrativo de pagamento do mês no portal fictício.",
  "url_inicial": "https://portal.exemplo.test/entrar",
  "hosts": ["portal.exemplo.test"],
  "rotulo_senha": "senha:portal-exemplo",
  "mfa": { "tipo": "nenhum" },
  "movimenta_dinheiro": false,
  "classe": "consulta",
  "agenda_horas": 168
}
```

- `rotulo_cpf`: campo opcional **reservado**, sem efeito nesta fatia. `preencher_cpf` usa o
  CPF único do chaveiro (o mesmo das senhas de PDF protegido) e só o digita no host aprovado;
  o manifesto não precisa de rótulo de CPF.
- `mfa`: `{"tipo":"nenhum"}`, `{"tipo":"manual"}` ou
  `{"tipo":"email","remetente":"nao-responda@portal.exemplo.test","formato":"\\d{6}"}`.
- **Regra dura:** `mfa.tipo == "email"` exige `movimenta_dinheiro == false`. Importar recusa.
- `hosts` é a allowlist: o roteiro não navega nem digita segredo fora dela.
- `classe`: `consulta` (padrão) ou `atualizar_conexao`.
- `agenda_horas`: intervalo da agenda no worker; ausente, só roda pelo botão.

## 5. Ciclo de vida

1. **Rascunho.** `sbf roteiros escrito importar <manifesto.json> --fonte <roteiro.luau>` (ou a
   proposta do MCP) cria a versão em rascunho.
   Rascunho nunca roda por agenda nem pelo botão contra site real.
2. **Teste no site falso.** `sbf roteiros escrito testar <nome> <versao> --cenario contracheque`
   roda contra o site falso local (127.0.0.1) com um perfil descartável. A ativação exige o
   último teste verde dessa versão.
3. **Ativo.** `sbf roteiros escrito ativar` (ou o botão da janela) só **na janela do app ou
   num terminal interativo**, sempre com confirmação digitada: `nome@versão` para ativar e
   `nome` para desativar, como as regras (ADR-021). Sem janela nem terminal, recusa e nada
   muda. O MCP e a IA **nunca** ativam, desativam, testam nem executam.
4. **Nova versão.** Editar cria uma versão nova em rascunho; a ativa nunca é editada.
   Desativar e voltar à versão anterior preservam o histórico.

O site falso traz login com senha, MFA por e-mail simulado (a "caixa" é uma implementação
falsa do trait de busca do código), uma tabela, um download e um botão "Pagar" que tem de ser
bloqueado. Os testes que precisam de Chrome ficam atrás de `SBF_CHROME_TESTE=1`.

## 6. Execução

- **Botão.** Conexões › Roteiros ganha "Executar agora" no roteiro ativo. A janela mostra o
  estado (executando, "Sua vez no navegador", terminou, falhou) e o histórico.
- **Agenda.** O worker dispara o roteiro ativo cujo `agenda_horas` venceu. A execução é
  idêntica à do botão.
- **Pausa.** CAPTCHA, MFA manual e `navegador.pausa` avisam (janela e notificação do worker) e
  esperam o usuário. Para o roteiro escrito a janela mostra "Sua vez no navegador" com
  Continuar e Abortar. Sem resposta no prazo da pausa, a execução termina como "não
  concluída", sem erro de segurança. **O tempo que o usuário leva numa pausa não conta no
  tempo total do roteiro**, nem a pausa automática por CAPTCHA ou código do aplicativo.
- **Resultado.** O download entra no inventário como achado, com origem
  `roteiro:<nome>/<execução>/<arquivo>`, e segue o reconhecimento normal (contracheque).
- **Uma execução por vez** por roteiro. Roteiros diferentes disputam o Chrome: o segundo falha
  com mensagem clara.

## 7. MCP só propõe

As ferramentas do MCP para roteiros escritos (nomes a fixar na implementação, no padrão
`propor_*`) criam rascunho e consultam. Não há ferramenta que ative, desative, teste, execute
ou leia segredo. Diferente das regras de reconhecimento (ADR-021, D15), onde o MCP
ativa: um roteiro escrito comanda um navegador com credenciais.

## 8. Segurança

- **Segredo.** Senha, CPF e código de MFA nunca aparecem no fonte, no manifesto, em log, em
  `Debug`, em erro, em argv, em nome de arquivo nem na página simplificada que a IA lê. Vivem
  como `Segredo` (zeroize) e são digitados por `Input.insertText` no campo, só se o host da
  página atual está no manifesto.
- **Allowlist de hosts.** `abrir` e os redirecionamentos fora de `hosts` são recusados; os
  verbos de segredo recusam página fora da lista.
- **Lista de bloqueio.** Clique e envio em elementos cujo texto, rótulo ou URL indique pagar,
  transferir, contratar, alterar cadastro, PIX, TED, investir ou resgatar abortam o roteiro e
  registram o motivo. A lista base **não** continha "atualizar" nem "sincronizar"; a
  implementação trata os dois assim: a classe `consulta` barra clicar em elemento cujo texto é
  exatamente "Atualizar" ou "Sincronizar" (para que a exceção da outra classe seja de fato
  uma exceção); a classe `atualizar_conexao` libera **só** esses dois textos exatos, e os
  demais campos do elemento (destino, rótulo, nome) continuam conferidos: "Atualizar e
  pagar" e "Atualizar dados cadastrais" seguem barrados. "Reconectar" e "atualizar
  conexão" não fazem parte do conjunto (a reautorização no banco fica com o usuário). A defesa
  em duas camadas (executor e navegador) para a classe `atualizar_conexao` vale no executor; a
  ponte do navegador usa a lista base.
- **MFA por e-mail.** Só com `movimenta_dinheiro: false`. A busca no Gmail é estreita:
  remetente fixo do manifesto, mensagem posterior ao início do login, janela curta, formato do
  código; o primeiro código válido mais novo vence e os anteriores são ignorados. O código do
  aplicativo autenticador (TOTP) sempre pausa.
- **Sem disfarce.** Chrome real, visível, perfil dedicado, sem alterar impressão digital nem
  cabeçalhos.
- **Sandbox Luau.** Sem `os`, `io`, `require`, `loadstring`, `debug`, rede nem JavaScript;
  tetos de instruções, memória, tempo, chamadas e tamanho.
- **Versão da bancada.** O `sbf` grava o carimbo do commit do SBF e o da bancada do qual o
  motor foi compilado, mostra os dois no binário (`sbf --versao`) e na janela (Opções e Sobre)
  e avisa árvore com alterações não commitadas.
- **Dados.** Fixtures e testes só com dados fictícios (`exemplo.test`, `CX-0001`, "Fulano").

## 9. Matriz de aceite

Cada linha é testável. O estado é preenchido no relatório de aceite, por um auditor
independente.

| ID    | Critério                                                                                                                                                |
| ----- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| RE-01 | A senha nunca aparece no fonte do roteiro, no manifesto, no log, em `Debug`, em erro nem na página simplificada da IA                                   |
| RE-02 | `preencher_senha` recebe o rótulo do chaveiro; rótulo inexistente dá erro sem revelar nada                                                              |
| RE-03 | `preencher_senha`, `preencher_cpf` e `preencher_codigo_do_email` recusam página de host fora da allowlist                                               |
| RE-04 | `abrir` em host fora de `hosts` é recusado, inclusive por redirecionamento                                                                              |
| RE-05 | `clicar` em "Pagar", "Transferir", "Contratar", "PIX", "TED", "Investir", "Resgatar" e "Alterar cadastro" aborta                                        |
| RE-06 | O bloqueio vale no executor e de novo no navegador: com o executor falso que deixa passar, o navegador ainda barra                                      |
| RE-07 | `texto` nunca devolve o valor de `<input>` nem de campo de senha; `preencher` recusa campo de senha                                                     |
| RE-08 | Importar manifesto com `mfa.tipo = "email"` e `movimenta_dinheiro = true` é recusado                                                                    |
| RE-09 | Com `movimenta_dinheiro = false` e MFA por e-mail, o código é buscado e digitado, e o roteiro nunca o recebe como texto                                 |
| RE-10 | A busca de código usa o remetente e o formato do manifesto; e-mail de outro remetente é ignorado                                                        |
| RE-11 | Código mais velho que o início do login é ignorado; vale o mais novo dentro da janela                                                                   |
| RE-12 | Sem código dentro de `janela_segundos` (máximo 300), o roteiro falha com erro claro e sem segredo                                                       |
| RE-13 | O código de MFA é `Segredo`: não aparece em log, `Debug` nem erro, e é zerado depois do uso                                                             |
| RE-14 | MFA `manual`, TOTP e CAPTCHA pausam e esperam o usuário; "Continuar" retoma, "Abortar" encerra com erro                                                 |
| RE-15 | `classe = "consulta"` não libera "Atualizar" nem "Sincronizar"; `atualizar_conexao` libera só isso e continua barrando "Pagar"                          |
| RE-16 | Importar cria rascunho; rascunho não roda por agenda nem contra site real                                                                               |
| RE-17 | Ativar e desativar só na janela ou em terminal interativo, com a confirmação digitada (`nome@versão` / `nome`); sem nenhum dos dois, recusa e nada muda |
| RE-18 | Ativar exige o teste no site falso verde para aquela versão                                                                                             |
| RE-19 | IA e MCP só propõem: nenhuma ferramenta do MCP ativa, desativa, testa, executa ou lê segredo                                                            |
| RE-20 | Editar cria versão nova em rascunho; a ativa não muda; voltar à anterior preserva o histórico                                                           |
| RE-21 | Laço infinito no Luau termina pelo teto de instruções, com erro claro; o app e a fila continuam vivos                                                   |
| RE-22 | Alocação sem fim no Luau termina pelo teto de memória                                                                                                   |
| RE-23 | Fonte acima de 64 KiB é recusado na importação                                                                                                          |
| RE-24 | O roteiro não alcança `os`, `io`, `require`, `loadstring`, `debug` nem rede                                                                             |
| RE-25 | Teto de chamadas a verbos e de tempo total encerram o roteiro com erro claro                                                                            |
| RE-26 | `esperar` com mais de 120 s é truncado em 120 s; o padrão é 15 s                                                                                        |
| RE-27 | `baixar` devolve só o nome do arquivo; o conteúdo vai ao inventário, não ao roteiro                                                                     |
| RE-28 | O download vira achado com origem `roteiro:<nome>/<execução>/<arquivo>`; repetir o mesmo arquivo não duplica o achado                                   |
| RE-29 | `util.hoje` e `util.somar_meses` usam o fuso UTC−3: às 22h de 24/11 a data é 2026-11-24, não 2026-11-25                                                 |
| RE-30 | "Executar agora" dispara o roteiro ativo; a agenda dispara quando `agenda_horas` vence; as duas passam pelo mesmo caminho                               |
| RE-31 | Duas execuções do mesmo roteiro ao mesmo tempo não acontecem; roteiros diferentes disputam o Chrome e o segundo falha com mensagem clara                |
| RE-32 | O binário (`sbf --versao`) e a janela (Opções e Sobre) mostram o commit do SBF e da bancada e avisam árvore com alterações não commitadas               |
| RE-33 | O roteiro de contracheque roda de ponta a ponta no site falso e entrega o PDF fictício como achado                                                      |
| RE-34 | O modelo comentado do portal real traz só placeholders: nenhuma URL nem seletor reais                                                                   |
| RE-35 | O roteiro gravado (JSON) da spec 10 continua funcionando igual, com a mesma lista de bloqueio                                                           |
| RE-36 | `cargo test --workspace` do SBF e da bancada sem regressão: nenhum teste anterior à fatia some ou muda de resultado                                     |

Testes que precisam de Chrome ficam atrás de `SBF_CHROME_TESTE=1`; o resto roda com o
`Navegador` falso. Verificações da fatia: `cargo fmt`, `cargo clippy -p sbf-roteiros
--all-targets -- -D warnings`, `cargo test -p sbf-roteiros` e os mesmos para
`bancada-navegador`, na bancada.
