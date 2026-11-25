# ADR-022 — Roteiros escritos em Luau, com o motor de navegador na bancada

**Data:** 2026-11-24 · **Status:** aceita · **Revisa:** [ADR-016](2026-11-12-adr-016-roteiros.md)
em dois pontos (MFA por e-mail; classe de ação "atualizar conexão") · **Spec:**
[14](../specs/14-desktop-roteiros-escritos-fatia-1.md)

## Contexto e decisão

O ADR-016 deu ao SBF roteiros gravados pela IA: passos em JSON que consultam e baixam, com a
senha no chaveiro e o MFA com o usuário. Falta o caso em que o usuário sabe o caminho e quer
escrevê-lo, e dois sites que o ADR-016 deixa de fora: o portal do contracheque (login com
código por e-mail) e o Meu Pluggy (onde se manda atualizar uma conexão).

O CEO decidiu (grill de 2026-11-24):

- **Dois tipos de roteiro.** O **roteiro gravado** (JSON, gravado pela IA) continua como é. O
  **roteiro escrito** é um programa em Luau, escrito à mão, ao lado de um manifesto JSON.
  Luau é só a linguagem; "script" segue como palavra a evitar no glossário.
- **O motor mora na bancada, a política no SBF.**
  - O crate novo `bancada-navegador` (repositório `bancada`) tem o Chrome visível por CDP com
    perfil dedicado, a página simplificada (sem valor de campo), o trait `Navegador` e o host
    Luau. É mecânica sem regra de negócio, como o chaveiro e o SQLite do ADR-018, e o Chess
    Studio pode usá-la.
  - O `sbf-roteiros` mantém a **lista de bloqueio**, os roteiros, o executor, o banco, a
    injeção de senha e a busca do código no Gmail.
- **Luau só chama verbos de alto nível** (`navegador.abrir`, `clicar`, `texto`, `baixar`,
  `pausa`, `preencher_senha` e semelhantes). Sem JavaScript solto, sem CDP cru, sem `os`,
  `io`, `require`, `loadstring`, `debug` nem rede. A VM tem tetos de instruções, memória,
  tempo, chamadas a verbos e tamanho do fonte.
- **O script nunca recebe segredo como texto.** Os verbos de segredo recebem o **rótulo** do
  chaveiro; o motor digita direto no campo (`Input.insertText`), e só se a página atual é de
  um host do manifesto. O mesmo vale para o CPF e para o código de MFA.
- **Ativação como as regras (ADR-021):** rascunho, teste no site falso, ativação só num
  terminal interativo com confirmação digitada. Editar cria uma versão nova em rascunho; a
  versão ativa nunca é editada. A IA e o MCP **só propõem** (criam rascunho). Isto difere do
  ADR-021 (D15), em que o MCP ativa regras: um roteiro escrito comanda um navegador com
  senhas do chaveiro, e o risco justifica o degrau a mais.
- **Chrome real, visível, sem disfarce.** Nada de anti-detecção. CAPTCHA e MFA de token (TOTP)
  param o roteiro e passam a vez ao usuário.
- **Roda por botão e por agenda.** "Executar agora" na janela e `agenda_horas` do manifesto
  no worker. Se pede MFA manual ou CAPTCHA, o roteiro avisa e espera.
- **Download vira achado**, com origem `roteiro:<nome>/<execução>/<arquivo>`, como no ADR-016.

**Revisão do ADR-016, ponto 1: MFA por e-mail.** O ADR-016 deixava todo MFA com o usuário.
Agora, um roteiro cujo manifesto declara `movimenta_dinheiro: false` pode declarar
`mfa.tipo = "email"`. O sistema busca o código no Gmail com uma consulta estreita (remetente
fixo, janela curta, formato do código, tudo vindo do manifesto), ignora e-mails anteriores ao
início do login e entrega o código como `Segredo` (zeroize), que não vai a log, `Debug`, erro
nem ao script. Importar o manifesto recusa `mfa.tipo = "email"` com `movimenta_dinheiro: true`.
MFA de token que muda por minuto continua sempre manual, e o banco e o consentimento do
Pluggy também.

**Revisão do ADR-016, ponto 2: classe de ação.** O manifesto declara `classe`: `consulta` (o
padrão) ou `atualizar_conexao`. A lista de bloqueio continua valendo para as duas. A segunda
só abre uma exceção nomeada e restrita: clicar em "Atualizar" ou "Sincronizar" nas conexões
de um agregador (o Meu Pluggy). Pagar, transferir, contratar e alterar cadastro seguem
barrados em qualquer classe.

## Opções descartadas

- **Disfarce anti-robô** (stealth, perfil falso, atraso humano simulado). É contornar um
  controle de acesso, que o V4 e o ADR-016 já proíbem, e quebra a cada versão do site. Se o
  site pede CAPTCHA, é a vez do usuário.
- **JavaScript solto no roteiro.** Daria ao roteiro a leitura de qualquer campo, inclusive da
  senha digitada, e a escrita em qualquer lugar da página. Os verbos limitam o que o roteiro
  vê e faz.
- **MFA por e-mail em qualquer site.** Quem tem o chaveiro e o Gmail abriria um banco sozinho.
  Só vale onde o roteiro não movimenta dinheiro.
- **Substituir o JSON pelo Luau.** O roteiro gravado é o que a IA sabe gerar, consertar e o
  usuário aprovar passo a passo. O Luau é a segunda forma, não a troca.
- **Tudo no SBF.** O Chess Studio e o próximo projeto precisariam copiar o navegador, a
  mecânica de CDP e a página simplificada.
- **Tudo na bancada.** A lista de bloqueio, o banco de roteiros, o chaveiro do SBF e o Gmail
  são do SBF. Regra financeira na biblioteca compartilhada quebraria o ADR-018.
- **Só botão, sem agenda.** O contracheque sai todo mês e a atualização de conexão é
  repetitiva; sem agenda o roteiro vira tarefa do usuário de novo. A agenda só dispara o que
  está ativo, e a pausa continua valendo.

## Consequências

- **Quem tem o chaveiro e o Gmail abre o portal de consulta.** O risco aceito do ADR-016
  cresce um degrau, mas só para sites que não movimentam dinheiro. A senha do banco e o
  consentimento do Pluggy não entram.
- **A bancada ganha um crate com Chrome e Luau.** É dependência pesada (CDP, Luau compilado)
  na biblioteca compartilhada, e o crate precisa de licença MIT OR Apache-2.0 como o resto
  (ADR-018).
- **O SBF depende do commit da bancada.** A dependência é git fixada, então o binário do
  `sbf` grava o carimbo da versão da bancada, que o `sbf` mostra e a spec 14 testa. Mudar o
  motor exige subir a bancada e fixar o commit no SBF no mesmo PR; a CI baixa a bancada pela
  deploy key (`BANCADA_DEPLOY_KEY`).
- **Dois formatos de roteiro** para manter: o gravado (JSON) e o escrito (Luau), com a mesma
  lista de bloqueio, o mesmo chaveiro e o mesmo destino de download.
- **A página simplificada nunca traz valor de campo.** A IA do roteiro gravado e o verbo
  `navegador.texto` leem só texto visível, nunca o `value` de um `<input>`.
- **O fuso importa.** Os verbos `util.hoje` e `util.somar_meses` usam o fuso do usuário
  (UTC−3), não o UTC do servidor.
- **Nada toca site real nos testes.** O site falso local cobre login, MFA por e-mail
  simulado, tabela, download e um botão "Pagar" bloqueado. O portal real do contracheque
  entra como modelo comentado, sem URL nem seletor, até o usuário informar o endereço.
