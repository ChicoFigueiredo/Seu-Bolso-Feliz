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
- `rotulo_senha`: o rótulo do chaveiro onde você guardou a senha
  (`<ROTULO-DA-SENHA-NO-CHAVEIRO>`). O roteiro só cita o rótulo. O CPF não precisa de rótulo:
  `navegador.preencher_cpf` usa o CPF único do chaveiro (`rotulo_cpf` é campo opcional
  reservado, sem efeito nesta fatia).
- `mfa`: `email` com o `remetente` exato que envia o código (`<REMETENTE-DO-CODIGO>`) e o
  `formato` (por exemplo `\\d{6}` para seis dígitos). Se o portal usa o código de um aplicativo
  autenticador (TOTP) ou CAPTCHA, use `{"tipo": "manual"}` e `navegador.pausa(...)` no roteiro.
  MFA por e-mail só é aceito com `movimenta_dinheiro: false`.
- `agenda`: quando o worker roda (substitui `agenda_horas`, que segue aceito mas não junto
  com `agenda`). Veja "Agenda e parâmetros" abaixo.

No roteiro (`.luau`), troque a URL da primeira linha e os seletores: `#cpf`, `#senha`,
`#entrar`, `#codigo`, `#confirmar`, `#lista` e o seletor do link do mês (`#baixar-AAAA-MM` no
exemplo). Use o seletor mais estável do portal (id, `name`, `data-*`); evite posição.
Se o portal mostra o mês por texto (por exemplo "Novembro/2026"), leia a lista com
`navegador.tabela("<SELETOR-DA-LISTA>")` e escolha a linha em Luau.

## Agenda e parâmetros (exemplo comentado)

O contracheque do dia 30 **substitui** o do dia 20 no mesmo lugar do portal. Por isso o
manifesto roda o roteiro em alguns dias do mês, pede o mês anterior e o corrente, e o inventário
decide pelo **conteúdo** o que é novo (um arquivo cujo sha256, ou cujo texto extraído, já está no
inventário é descartado e aparece no histórico como "já estava no inventário").

```jsonc
{
  // ...nome, hosts, url_inicial, rotulo_senha, mfa como acima...
  "agenda": [
    {
      // Em cada um destes dias do mês, nestes horários (fuso do usuário, UTC-3).
      // O dia 31 num mês sem 31 não dispara nesse mês.
      "dias_do_mes": [20, 21, 30, 1],
      "horarios": ["04:00", "12:00", "20:00"],
      // O que esta entrada entrega aos parâmetros declarados abaixo.
      // "-1" = o mês anterior, "0" = o mês corrente (ou "AAAA-MM" fixo).
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

(O arquivo `.json` de verdade não aceita comentários: tire as linhas `//` ao copiar.)

No `.luau`, os valores já chegam resolvidos na tabela `parametros`, e `util.meses` lista o
intervalo:

```lua
-- parametros.de == "2026-10", parametros.ate == "2026-11" (rodando em novembro de 2026)
for _, mes in ipairs(util.meses(parametros.de, parametros.ate)) do
  -- <SELETOR-DO-LINK-DO-MES>: troque pelo do portal real
  navegador.clicar("#baixar-" .. mes)
  navegador.baixar("<SELETOR-DO-DOWNLOAD>", "contracheque-" .. mes)
end
-- Se o portal recusar a entrada (senha expirada, cadastro a atualizar...), pare e suspenda:
-- navegador.entrada_recusada("o portal pediu para trocar a senha")
```

- Se o PC estava desligado, a entrada roda **uma vez** ao voltar (não uma por horário perdido).
- `navegador.entrada_recusada(motivo)` encerra o roteiro e suspende a agenda dele até você
  rodar `sbf roteiros escrito retomar <nome>` (terminal interativo). Execução manual continua
  possível e não retoma a agenda sozinha.
- Baixar de novo mesmo o que o inventário já tem (Baixa forçada):
  `sbf roteiros escrito executar <nome> --forcar` (terminal interativo) ou o botão da janela.
- Mês de teste fora da rotina: `sbf roteiros escrito executar <nome> --de 2026-01 --ate 2026-03`.

## Importar, testar e ativar

```bash
sbf roteiros escrito importar contracheque-meu-orgao.json --fonte contracheque-meu-orgao.luau
sbf roteiros escrito listar                              # veja o número da versão criada
sbf roteiros escrito testar contracheque-meu-orgao 1 --cenario contracheque  # no site falso, sem rede externa
sbf roteiros escrito ativar contracheque-meu-orgao 1     # na janela ou em terminal interativo, com confirmação digitada (contracheque-meu-orgao@1)
```

- `importar` valida o manifesto e cria a versão em **rascunho**. Editar sempre cria versão
  nova em rascunho; a versão ativa nunca muda.
- `testar` roda o rascunho no site falso (o portal de teste do motor), nunca no portal real.
- `ativar` só aceita versão testada e exige você, na janela ou num terminal interativo,
  digitando a confirmação (`nome@versão`); `desativar` pede o `nome`. O MCP e a IA só propõem
  rascunhos: nunca ativam, desativam, testam nem executam.

Depois de ativo, o roteiro roda no botão "Executar agora" da janela e pela agenda. Se o portal
pedir o código do aplicativo autenticador (TOTP) ou CAPTCHA, o roteiro avisa e espera você continuar.
