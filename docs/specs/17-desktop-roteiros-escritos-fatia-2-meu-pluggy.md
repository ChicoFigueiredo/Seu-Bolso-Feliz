# Spec 17: roteiros escritos, fatia 2 (Meu Pluggy)

Continua a [spec 16](16-desktop-roteiros-escritos-fatia-1.md) e o ADR-022. A fatia 1 entregou o
motor e o contracheque no site falso. Esta entrega o **roteiro escrito** que atualiza as
conexões do Meu Pluggy todos os dias, e o **modo assistido** que serviu para descobrir como o
site funciona.

## O que o Meu Pluggy é, na prática (observado em 2026-11-25, de forma assistida)

- **Login sem senha.** Entra-se com Google ou com um link enviado ao e-mail (Auth0, vale 5
  minutos). O Google **recusa** navegador controlado por automação; por isso o login é feito à
  mão, num Chrome **sem controle**, e o que fica no perfil dedicado é a **sessão do próprio
  Meu Pluggy**, que dispensa o login até expirar.
- `/connections` lista as conexões ("Banco, Hoje, Ver detalhes"); a tela de detalhes
  (`/connections/<id>`) tem, lado a lado, **Atualizar** e **Excluir**.
- **Atualizar** abre, em menos de 1 s, o widget **Pluggy Connect** num `<iframe>` de outro
  domínio (classe `zoid-pluggy-connect-widget-<hash>-visible`; o atributo `name` leva o token
  da conexão e **nunca** é lido nem registrado). Mostra "Atualizando", **não** pede consentimento
  nem login do banco nas conexões testadas, e some sozinho em menos de ~50 s, sem aviso.
- O leitor de página do motor só lê o quadro principal: o conteúdo do iframe é invisível. O
  sinal do roteiro é **o iframe aparecer e depois sumir**.

## O que a fatia entrega

| Peça                                            | Onde                                                                           |
| ----------------------------------------------- | ------------------------------------------------------------------------------ |
| Verbo `navegador.esperar_sumir(seletor, s?)`    | bancada, `bancada-navegador` (`luau/verbos.rs`, `luau/ponte.rs`)               |
| Roteiro `meu-pluggy-atualizar` (manifesto+Luau) | `crates/sbf-roteiros/exemplos/meu-pluggy-atualizar.{json,luau}`                |
| Site falso do Meu Pluggy e cenário `conexoes`   | bancada `site_falso_conexoes.rs`; SBF `teste_escrito.rs` (`Cenario::Conexoes`) |
| `sbf roteiros escrito entrar --url <https://>`  | login à mão, Chrome do perfil dedicado **sem** a porta de automação            |
| `sbf roteiros escrito observar --url ...`       | modo assistido, só leitura: grava a página simplificada a cada mudança         |

O roteiro é `classe: atualizar_conexao`, `movimenta_dinheiro: false`, `mfa: manual`, sem rótulo
de senha, `agenda_horas: 24`. Hosts: `meu.pluggy.ai` e `my-pluggy.us.auth0.com` (este só para a
página de login que o site pode abrir; o roteiro nunca digita segredo).

## Como o roteiro se comporta

1. Abre `/connections`. Se a lista não aparece em 25 s (sessão expirada), **pausa** e passa a
   vez ao usuário; se continuar sem lista, falha.
2. Para cada conexão: abre os detalhes, espera o botão, confere que o texto do primeiro botão é
   **exatamente** "Atualizar" (o segundo é "Excluir" e nunca é tocado; a lista de bloqueio da
   classe também só libera esse texto), clica, espera o iframe do widget aparecer e depois sumir.
3. Se o widget fica mais de 120 s, **pausa** (o banco pode estar pedindo consentimento ou
   login, e isso é sempre do usuário) e espera de novo.
4. Se alguma conexão não atualizou, o roteiro termina **com erro** e o histórico diz quantas.

## Matriz

| Id    | Verificação                                                                                                         |
| ----- | ------------------------------------------------------------------------------------------------------------------- |
| M2-01 | `esperar_sumir` devolve `true` se o elemento não está e `false` se ele fica até o prazo; recusa prazo não positivo  |
| M2-02 | O roteiro atualiza 3 conexões e clica só no primeiro botão do bloco (nunca "Excluir"), com `Fake` de verbos         |
| M2-03 | O roteiro falha com "não atualizaram" se o widget não abre                                                          |
| M2-04 | `testar --cenario conexoes` passa no site falso com Chrome real: 3 conexões, uma atualização cada, nenhuma exclusão |
| M2-05 | Um roteiro que clica em "Excluir" é barrado pela lista de bloqueio e o cenário falha                                |
| M2-06 | Com sessão expirada o roteiro pausa e, sem usuário, o cenário falha (não "passa")                                   |
| M2-07 | `entrar` sobe o Chrome sem `--remote-debugging-port`; `observar` só lê (nunca clica nem digita)                     |
| M2-08 | O exemplo e o manifesto passam no teste dos exemplos (só verbos do contrato, nada proibido)                         |

## Fora desta fatia

- Ler o **conteúdo** do widget (está em outro domínio) para saber se a atualização deu certo:
  hoje o sinal é o iframe sumir.
- Renovar a sessão sozinho quando expira (hoje: pausa e pede você, via link do e-mail).
- Contracheque no portal real (a URL ainda não foi informada).
