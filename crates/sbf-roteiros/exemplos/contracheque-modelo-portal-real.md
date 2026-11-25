# Adaptar o contracheque a um portal real

`contracheque-exemplo.json` e `.luau` rodam contra o portal **fictício** do site falso. Nada
neste exemplo vale para um portal real: URL, seletores e remetente abaixo são placeholders que
você descobre olhando o portal (com o roteiro gravado ou à mão).

## O que trocar

No manifesto (`.json`):

- `nome`: kebab-case, por exemplo `contracheque-meu-orgao`.
- `hosts` e `url_inicial`: o nome do servidor do portal (`<HOST-DO-PORTAL>`) e a página de
  entrada (`https://<HOST-DO-PORTAL>/<CAMINHO-DA-ENTRADA>`). Só `https`; o roteiro não navega
  nem digita segredo fora de `hosts`. Se o login mora em outro host, ponha os dois.
- `rotulo_senha` e `rotulo_cpf`: os rótulos do chaveiro onde você guardou a senha e o CPF
  (`<ROTULO-DA-SENHA-NO-CHAVEIRO>`). O roteiro só cita o rótulo.
- `mfa`: `email` com o `remetente` exato que envia o código (`<REMETENTE-DO-CODIGO>`) e o
  `formato` (por exemplo `\\d{6}` para seis dígitos). Se o portal usa token de app
  autenticador ou CAPTCHA, use `{"tipo": "manual"}` e `navegador.pausa(...)` no roteiro.
  MFA por e-mail só é aceito com `movimenta_dinheiro: false`.
- `agenda_horas`: de quantas em quantas horas o worker roda (168 = semanal).

No roteiro (`.luau`), troque a URL da primeira linha e os seletores: `#cpf`, `#senha`,
`#entrar`, `#codigo`, `#confirmar`, `#lista` e o seletor do link do mês (`#baixar-AAAA-MM` no
exemplo). Use o seletor mais estável do portal (id, `name`, `data-*`); evite posição.
Se o portal mostra o mês por texto (por exemplo "Novembro/2026"), leia a lista com
`navegador.tabela("<SELETOR-DA-LISTA>")` e escolha a linha em Luau.

## Importar, testar e ativar

```bash
sbf roteiros escrito importar contracheque-meu-orgao.json --script contracheque-meu-orgao.luau
sbf roteiros escrito listar                              # veja o número da versão criada
sbf roteiros escrito testar contracheque-meu-orgao 1     # no site falso, sem rede externa
sbf roteiros escrito ativar contracheque-meu-orgao 1     # só em terminal interativo, com confirmação digitada
```

- `importar` valida o manifesto e cria a versão em **rascunho**. Editar sempre cria versão
  nova em rascunho; a versão ativa nunca muda.
- `testar` roda o rascunho no site falso (o portal de teste do motor), nunca no portal real.
- `ativar` só aceita versão testada e exige você, num terminal, digitando a confirmação. IA e MCP só propõem rascunhos.

Depois de ativo, o roteiro roda no botão "Executar agora" da janela e pela agenda. Se o portal
pedir token ou CAPTCHA, o roteiro avisa e espera você continuar.
