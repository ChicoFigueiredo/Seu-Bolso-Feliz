# Spec 09 — Desktop: triagem do inventário por IA (antecipada da fatia 6)

**Status:** decidida (planejamento com o CEO em 2026-11-14) · **Depende de:** spec 06, ADR-015

## Objetivo

Arrumar os cerca de 10 mil registros e os achados pendentes do inventário. A regra é: a IA
**sugere**, e você confirma em lote, como nas sugestões de conciliação. Uma sugestão confirmada
vira decisão no log com `decidido_por = usuario`. A sugestão em si nunca é decisão e nunca
passa por cima de decisão sua.

## Decisões do CEO (2026-11-14)

| #   | Decisão                                                                                                                                                                                           |
| --- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| D1  | **Rota principal:** a assinatura Claude Max do CEO, chamando o Claude Code em modo headless (`claude -p`), sem custo por token. **Reserva:** o OpenRouter, com chave no chaveiro e teto de gasto. |
| D2  | **Nível de confiança** do destinatário "triagem" é **completo** (ADR-015 item 1): o texto extraído vai inteiro. A imagem só vai quando não há texto.                                              |
| D3  | A IA sugere e o CEO confirma em lote; nada se aplica sozinho.                                                                                                                                     |
| D4  | Piloto de 200 itens antes da rodada inteira, medindo acerto e consumo.                                                                                                                            |

## Como gasta pouco

1. **Regras antes da IA (de graça).** Saem da fila antes de qualquer chamada:
   - os itens com decisão do usuário;
   - os `promovido`;
   - as duplicatas por `content_hash`;
   - os registros cujo fornecedor normalizado já tem categoria aprendida das suas decisões
     anteriores (o mesmo fornecedor e o mesmo tipo dão a mesma categoria, com confiança alta);
   - os pares de conciliação que a regra existente já sugere.
2. **Lotes.** Cada chamada leva até 25 itens em um único prompt, com resposta em JSON validada
   por esquema. Um item com resposta inválida volta à fila uma vez e, na segunda, vai para
   "não classificado".
3. **Dois degraus.** A primeira passada usa um modelo rápido (Haiku). Só o que voltar com
   confiança abaixo de 0,7 sobe para o modelo forte (Sonnet). Nunca o Opus.
4. **Cache por conteúdo.** A chave é `sha256(prompt_versao + modelo + entrada)`. Repetir a
   triagem do mesmo item não chama de novo.
5. **Ritmo.** A fila respeita o limite da assinatura: com erro de limite (rate limit) ela pausa,
   espera e continua sozinha. Só com o OpenRouter ligado e abaixo do teto é que ela muda de
   provedor.

## O motor (`crates/sbf-ia`)

O contrato está em `crates/sbf-ia/src/lib.rs`, a costura desta fatia. Os provedores são:

- **`ClaudeCli`.**
  - Roda `claude -p` com o prompt pela entrada padrão, `--output-format json` e o modelo
    escolhido, sem ferramentas e com um turno só.
  - O diretório de trabalho é vazio, sob `$XDG_RUNTIME_DIR` com permissão 0700.
  - Lê `result`, `usage` e `total_cost_usd` da saída. O custo, na assinatura, é **nominal**:
    serve para os pontos de atenção.
  - O prompt nunca vai por argumento.
- **`OpenRouter`.** HTTPS pela `bancada-rede`, com chave do chaveiro e teto em centavos de dólar
  por rodada e por mês.
- **`Falso`.** Respostas roteirizadas, para os testes. Nenhum teste chama provedor real.

Todo envio grava uma linha em `saida_ia` no inventário (ADR-015): destinatário, modelo, nível,
bytes enviados, tokens, custo nominal, quando, e o id do lote de triagem. **O texto enviado não
fica gravado.**

## O que a IA sugere

| Para            | Campos sugeridos                                                                                      |
| --------------- | ----------------------------------------------------------------------------------------------------- |
| Achado pendente | `e_financeiro` (sim, não ou incerto), tipo de documento, fornecedor, data-chave, valor                |
| Registro        | categoria, fornecedor normalizado, conta provável, tipo, e se parece duplicata de outro registro (id) |

Cada sugestão traz `confianca` (de 0 a 1) e uma justificativa curta (até 140 caracteres) que
aparece na tela.

## Banco (`sql/0003-triagem.sql` no `sbf-inventario`)

- **`sugestao_ia`:**
  `(id, alvo_tipo, alvo_id, campo, valor_json, confianca, justificativa, modelo, prompt_versao,
rodada_id, criada_em, estado CHECK (estado IN ('aberta','aceita','recusada','obsoleta')))`.
  Uma sugestão fica `obsoleta` quando o usuário decide aquele campo por outro caminho.
- **`rodada_ia`:** `(id, iniciada_em, terminada_em, itens, chamadas, custo_nominal_centavos_usd,
provedor, situacao)`.
- **`saida_ia`:** o registro do que saiu (veja acima).
- **`cache_ia`:** `(chave, resposta_json, criada_em)`.

Aceitar sugestões em lote grava **uma decisão por item**, pelo `Comandos` existente.

## Janela

- **Área "Triagem por IA"** no grupo Inventário.
  - No topo, a rodada atual: barra de andamento, itens, chamadas e o consumo nominal, com os
    botões Pausar e Retomar.
  - Abaixo, a lista de sugestões agrupada por campo e valor: "142 registros → categoria
    Mercado". Você aceita o grupo inteiro, aceita só os marcados ou recusa.
  - Filtro por confiança. A prévia do documento fica ao lado.
- **Opções → IA:** provedor principal, reserva, tetos e o nível do destinatário, além de um
  botão de teste de conexão.

## Matriz de aceite

| ID    | Critério                                                                                   |
| ----- | ------------------------------------------------------------------------------------------ |
| IA-01 | Itens com decisão do usuário ou `promovido` nunca entram na fila                           |
| IA-02 | O fornecedor com categoria aprendida é resolvido sem chamada                               |
| IA-03 | Resposta inválida tenta de novo uma vez e depois vira "não classificado"                   |
| IA-04 | Repetir uma rodada sobre os mesmos itens faz zero chamadas (cache)                         |
| IA-05 | O prompt nunca aparece em argv nem em log; `saida_ia` não guarda texto                     |
| IA-06 | Um erro de limite pausa e retoma; o OpenRouter só entra se estiver ligado e abaixo do teto |
| IA-07 | Aceitar em lote grava uma decisão por item, e o desfazer funciona                          |
| IA-08 | Uma decisão do usuário depois da sugestão a torna `obsoleta`                               |
| IA-09 | O piloto de 200 itens mostra acerto contra as suas decisões já tomadas e o consumo         |

## Agente contínuo (pedido do CEO em 2026-11-14; entra com o `sbf worker`, onda B)

A triagem deixa de ser só uma rodada que você dispara. O `sbf worker` roda um **agente de triagem**
que trabalha sozinho, dentro das mesmas regras: ele sugere e você confirma.

- **Monitorar.** Cada achado ou registro novo, que vem da coleta de pastas, nuvens, e-mail ou
  WhatsApp, entra na fila assim que chega. O agente processa em pequenos lotes, no ritmo da
  assinatura, e em horário configurável.
- **Aprender sem treinar modelo.** O aprendizado sai das suas decisões, em três camadas:
  1. **Regras aprendidas (de graça).** Fornecedor e tipo dão a categoria, e padrão de remetente
     ou de nome de arquivo dá o tipo de documento. A regra nasce quando você confirma o mesmo
     resultado 3 vezes sem nenhuma recusa, e morre na primeira recusa.
  2. **Exemplos no prompt.** Para cada item, vão até 8 decisões suas parecidas (mesmo
     fornecedor, mesmo remetente, texto parecido pelo índice local da fatia 6). Até o índice
     chegar, vão as decisões do mesmo fornecedor.
  3. **Calibração.** O agente acompanha o acerto por campo e por faixa de confiança contra o
     que você aceita e recusa. Uma faixa que erra mais do que diz passa a valer menos.
- **Mostrar.** A área Triagem por IA ganha um painel com o que chegou, o que o agente sugeriu,
  as regras aprendidas (você pode ver e apagar cada uma) e o acerto da semana.
- **Avisar.** Um lote grande de sugestões de alta confiança vira um ponto de atenção, e pela
  fatia 4 chega no Telegram: "37 sugestões prontas para confirmar".
- **Nunca.** O agente não aplica nada sozinho, não passa por cima de decisão sua e não reabre
  item `promovido`.

### Implementação do agente contínuo (onda B, `sbf worker`)

- Binário: `sbf worker rodar|instalar|desinstalar|estado --inventario <cofre>`. `instalar` escreve
  `~/.config/systemd/user/sbf-worker.service` (reinício `on-failure`) e roda `systemctl --user enable --now`;
  `--sem-ligar` só gera o arquivo. Windows e macOS: "ainda não suportado" (use `sbf worker rodar` à mão).
- Arquivos na pasta de dados do app (fora do cofre): `worker.json` (pastas vigiadas, intervalos, horário e
  lote da triagem; a triagem nasce **desligada**), `worker-estado.json` (lido pela barra de status),
  `worker.log` (rotacionado, só eventos tipados), `worker.lock`, `agente.sqlite` (regras, calibração, chegadas).
- Prompt `triagem-v2` (v1 segue no cache): até 8 decisões suas do mesmo fornecedor por item.
- Regra aprendida: 3 confirmações suas sem conflito nem recusa; recusa de sugestão dela a mata; apagar à mão
  só deixa voltar com 3 confirmações novas. Aceite de sugestão da IA não conta.
- Fora desta onda: texto extraído (E3) das pastas, índice de texto parecido (fatia 6), aviso no Telegram,
  OpenRouter no worker (exige chaveiro aberto), regra por remetente de e-mail, launchd e serviço do Windows.
