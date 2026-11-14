# Bot do Telegram e alertas (`sbf canais`)

O `sbf canais` roda 24 h na VPS, ao lado do `sbf servidor` e do `sbf coletor-vps`. Faz duas
coisas (ADR-014, ADR-017):

- **Entrada:** documento ou foto mandado ao bot cai direto na caixa de entrada (o bot é uma
  fonte já filtrada). Grava `source_documents` + `ingestion_jobs` em `discovered` pelo mesmo
  caminho do servidor (`sbf_servidor::documentos::guardar`), idempotente pelo SHA-256 do dono.
- **Avisos:** as regras de alerta sobre o livro-caixa saem pelo Telegram (o canal só leva
  avisos; não traz dados, fora os documentos acima).

É um modo e um crate (`sbf-canais`) à parte do coletor de propósito: o coletor puxa dados de
fora em intervalos; o canal tem outro segredo (o token do bot), um laço contínuo de long
polling e precisa reiniciar sem atrasar as coletas.

## Criar o bot (BotFather)

1. No Telegram, abra o `@BotFather` e mande `/newbot`. Escolha um nome e um usuário que
   termine em `bot`.
2. O BotFather devolve o **token** (`123456:ABC...`). Ele é a senha do bot: guarde só no
   `EnvironmentFile` abaixo, nunca em argv, repositório ou chat.
3. Opcional: `/setprivacy` e `/setjoingroups` (desligar) para o bot não entrar em grupos.
4. Descubra o seu `chat_id`: mande qualquer mensagem ao bot e use o fluxo de vinculação
   abaixo (não precisa saber o número). Se preferir a lista fixa, `@userinfobot` mostra o id.

## Variáveis

| variável | para quê |
| --- | --- |
| `SBF_DATABASE_URL` | o Postgres (obrigatória) |
| `SBF_TELEGRAM_TOKEN` | o token do BotFather (obrigatória para servir) |
| `SBF_TELEGRAM_CHATS` | chat_ids autorizados sem código, separados por vírgula (opcional) |
| `BLOB_READ_WRITE_TOKEN` | o armazenamento dos documentos que chegam pelo bot |
| `LOCAL_USER_ID` | substituto validado do dono (opcional; o padrão é o `owner` do banco) |
| `SBF_ALERTAS_DIAS` | avisa conta que vence em até N dias (padrão `3`) |
| `SBF_ALERTAS_HORAS` | horários da agenda, de São Paulo, `HH:MM,HH:MM` (padrão `08:00`) |
| `SBF_ALERTAS_SILENCIO` | janela sem aviso, `HH:MM-HH:MM` (padrão `22:00-07:00`) |
| `SBF_ALERTAS_RESUMO` | `1` liga o resumo diário, no primeiro horário do dia |
| `SBF_ALERTAS_LIMIAR_PERCENTUAL` | gasto fora do padrão: % acima da média (padrão `50`) |
| `SBF_ALERTAS_LIMIAR_MINIMO` | gasto fora do padrão: centavos acima da média (padrão `10000`) |

O token nunca aparece em log, `Debug` nem mensagem de erro (a Bot API o leva na URL; os erros
de rede são impressos sem ela).

## Vincular um chat

Só falam com o bot os chat_ids da lista `SBF_TELEGRAM_CHATS` e os chats vinculados. Qualquer
outro chat é ignorado em silêncio. Para vincular um chat novo:

```bash
sbf canais vincular-telegram
# Código de vinculação: 7KQ2M9XR  (vale 10 minutos e só uma vez)
```

No Telegram, mande ao bot `/vincular 7KQ2M9XR`. O código é de uso único, expira em 10 minutos
e só o hash dele fica no banco; depois de 5 tentativas erradas em 10 minutos o chat é ignorado.
Um destinatário novo nasce **mascarado** (ADR-015).

Outros comandos da linha de comando:

```bash
sbf canais nivel <chat_id> <mascarado|parcial|completo>
sbf canais desvincular <chat_id>
sbf canais alertas --simular   # imprime os alertas de hoje (nível mascarado), sem rede
sbf canais alertas             # avalia e avisa agora (respeita a deduplicação)
```

No Telegram: `/ajuda`, `/resumo` e o envio de arquivos.

## Entrada de documentos

PDF, CSV, XLSX, OFX, XML, PNG, JPG ou WEBP, até 20 MB (limite do Telegram para bots). O bot
responde "Recebido, vai para a caixa de entrada." O mesmo conteúdo enviado de novo responde
que já estava lá, sem criar nada. Fotos viram `foto-<id>.jpg`. Formato que o pipeline não
abre é recusado com a razão.

## Alertas

| alerta | regra |
| --- | --- |
| conta a vencer | compromisso aberto (saída não confirmada, ocorrência pendente de recorrente, fatura de cartão em aberto) que vence em até N dias |
| conta vencida | o mesmo, com vencimento no passado (até 90 dias) |
| cobrança duplicada | mesmo fornecedor e mesmo valor, mais de uma vez no mesmo ciclo (atual e anterior); parcelas ficam de fora |
| gasto fora do padrão | gasto da categoria no ciclo acima da média dos 3 ciclos anteriores + limiar (50% e R$ 100,00 por padrão), só com histórico de pelo menos 2 ciclos |
| sugestões da triagem | **não existe na VPS**: as sugestões prontas são locais. O ponto de extensão é `alertas::FonteDeAlertas`; hoje nenhuma fonte extra está ligada |

As duas primeiras portam os compromissos da tela Hoje (`painel-hoje.ts`) sem o corte do
ciclo, para não perder o vencimento do ciclo seguinte nem a vencida de ciclo antigo. As duas
seguintes são novas: o domínio TS não tinha regra de alerta.

- **Deduplicação:** cada alerta tem uma chave e um estado. Ele só sai de novo quando o
  estado muda (novo valor ou vencimento, virou vencida, mais uma cobrança, a faixa do gasto
  dobrou). Resolvido e depois reaberto, avisa de novo. O passar dos dias não repete.
- **Agenda:** nos horários de `SBF_ALERTAS_HORAS`. Um horário que cai no silêncio sai no
  primeiro minuto depois dele. Cada horário roda uma vez por dia (guardado em `canais_estado`).
- **Resumo diário:** só contagens e totais, sem nomes.

## Nível de confiança (ADR-015)

| nível | o que sai |
| --- | --- |
| mascarado (padrão) | nome do fornecedor e valor; nenhuma sequência de 6 ou mais dígitos (conta, cartão, CPF, linha digitável) vira `•••`; sem o nome da conta |
| parcial | os mesmos números, com os 4 últimos dígitos; com o nome da conta |
| completo | o texto como está no livro |

A tabela `canais_saidas` registra cada mensagem: destinatário, tipo, nível, tamanho em bytes,
se foi entregue e quando. **Nunca o texto.** A migração é `packages/db/migrations/0006_canais.sql`
(vai ao Neon só pelo portão humano, ADR-007).

## Unit systemd

`/etc/sbf/canais.env` (modo `0600`, dono `sbf`):

```bash
SBF_DATABASE_URL=postgres://...
SBF_TELEGRAM_TOKEN=123456:ABC...
BLOB_READ_WRITE_TOKEN=...
SBF_ALERTAS_HORAS=08:00,18:00
SBF_ALERTAS_RESUMO=1
```

`/etc/systemd/system/sbf-canais.service`:

```ini
[Unit]
Description=Seu Bolso Feliz: bot do Telegram e alertas
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=sbf
EnvironmentFile=/etc/sbf/canais.env
ExecStart=/usr/local/bin/sbf canais
Restart=on-failure
RestartSec=10
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

```bash
sudo systemctl daemon-reload && sudo systemctl enable --now sbf-canais
journalctl -u sbf-canais -f   # o log nunca leva token nem texto de mensagem
```

## Testes

```bash
cargo test -p sbf-canais                                  # regras, máscara, agenda (sem Docker)
scripts/desktop/postgres-de-teste.sh testar-canais        # + bot e alertas, Postgres em Docker
```

O Telegram dos testes é um servidor HTTP em loopback; nenhum teste chama a API real.
