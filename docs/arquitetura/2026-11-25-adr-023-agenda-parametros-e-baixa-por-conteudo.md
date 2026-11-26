# ADR-023 — Agenda, parâmetros, suspensão e baixa por conteúdo nos roteiros escritos

**Data:** 2026-11-25 · **Status:** aceita · **Estende:** [ADR-022](2026-11-24-adr-022-roteiros-escritos-e-motor-na-bancada.md)
· **Spec:** [18](../specs/18-desktop-roteiros-agenda-e-parametros.md)

## Contexto e decisão

O ADR-022 entregou o roteiro escrito com uma agenda só: `agenda_horas`, "a cada N horas desde
a última execução". Isso não serve ao que o usuário precisa: atualizar o Meu Pluggy às 4h, 12h
e 20h, e baixar o contracheque no SouCaixa só nos dias 20, 21, 30 e 1, com o mês corrente e o
anterior. O contracheque do dia 20 (antecipado) e o do dia 30 (final) ocupam o mesmo lugar no
portal, e o PDF é gerado na hora, então o arquivo muda de bytes a cada geração.

O CEO decidiu (grill de 2026-11-25):

- **Agenda no manifesto.** O campo `agenda` é uma lista de entradas, cada uma com `horarios`
  e, se quiser, `dias_do_mes` e `parametros`, no fuso do usuário (UTC−3). `agenda_horas`
  continua aceito; os dois juntos tornam o manifesto inválido.
- **Roda uma vez ao voltar.** Uma entrada está devida se existe um instante agendado entre a
  última execução dela (ou a ativação da versão, o que for mais novo) e agora. Roda uma vez,
  não uma por horário perdido. Ativar uma versão depois do horário não dispara na hora.
- **Parâmetros declarados.** O manifesto declara `parametros` (`mes` ou `booleano`, com
  padrão). Quem entrega os valores é a agenda, o terminal ou a janela. O Luau lê uma tabela
  `parametros` somente leitura, já resolvida (`"2026-10"`). Mês relativo (`0`, `-1`) conta do
  mês do instante agendado numa execução de agenda, e do agora numa execução manual. Nunca
  levam segredo.
- **Suspensão da agenda.** `navegador.entrada_recusada(motivo)` termina o roteiro, grava a
  execução como `entrada_recusada` e suspende a agenda **do roteiro** (não da versão), para
  não errar a senha de hora em hora até o site bloquear a conta. Só o usuário retoma (janela ou
  terminal); o MCP, o worker e a IA não.
- **Baixa por conteúdo.** Como o do dia 30 substitui o do dia 20, "já baixei" não se decide
  pela competência. O arquivo baixado por roteiro é descartado se o sha256 **ou** o hash do
  texto extraído já existe no inventário.
- **Baixa forçada.** Uma execução com `forcar = true` ignora o descarte. Quem confirma é o
  usuário, na janela (diálogo) ou no terminal. O **MCP só propõe**
  (`propor_baixa_forcada`): a proposta fica numa tabela própria do `roteiros.sqlite` e aparece
  em Conexões › Roteiros, onde o usuário aceita ou recusa. O ADR-022 não muda: nada que
  executa um roteiro é do MCP.

## Opções descartadas

- **Cron.** Mais compacto e conhecido, porém mais fácil de errar e menos legível. Os campos
  `horarios` e `dias_do_mes` se validam e se leem em português.
- **Perder o horário se o computador estava desligado.** Um dia desligado às 8h deixaria o
  contracheque do dia 1 sem baixar.
- **"Já baixei" pela competência.** Esconderia a versão final do mês atrás da antecipada.
- **O MCP executar roteiro ativo, só com parâmetros.** Faria o MCP disparar login com a senha
  do cofre, sem o usuário na frente. Contraria o ADR-022.
- **Reaproveitar as propostas do controle mensal.** Elas confirmam dentro de uma transação
  financeira; executar um roteiro (Chrome na tela, MFA) não cabe nesse molde.

## Consequências

- O formato do manifesto passa a ter `agenda` e `parametros`. As versões imutáveis já gravadas
  não mudam: o JSON canônico omite os campos vazios, então o checksum delas é o mesmo. Uma
  versão com `agenda` não roda num binário anterior ao desta decisão.
- O `roteiros.sqlite` ganha colunas aditivas (`suspenso_unix`, `suspenso_motivo` no roteiro;
  `entrada_agenda`, `previsto_unix`, `parametros_json` na execução) e uma tabela
  (`proposta_baixa_forcada`). O estado `entrada_recusada` pede reconstruir a tabela das
  execuções, feita numa transação, preservando linhas, eventos e os gatilhos de imutabilidade.
- A comparação por texto só vale para PDF sem senha e com texto suficiente (200 caracteres);
  o resto compara só pelo sha256. Se um portal mudar o texto entre gerações, o inventário cai
  no aviso de "possível duplicidade" em vez de descartar.
- O login em dois hosts (portal e Keycloak) entra nos `hosts` do manifesto, e a lista de
  bloqueio ganha as frases com possessivo ("alterar meus dados") e "recuperar senha".
