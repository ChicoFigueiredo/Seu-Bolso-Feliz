# Coletas da VPS (`sbf coletor-vps`)

Porte para Rust dos workers Bun `workers/gmail-scanner` e `workers/pluggy-sync` (ADR-014: fontes
já filtradas entregam direto na caixa de entrada e rodam na VPS 24 h; ADR-010: fila no banco;
ADR-012: os workers Bun se aposentam um a um). O código é o crate `crates/sbf-coletor`; o
binário é o mesmo `sbf`.

## Por que um modo próprio, e não dentro do `sbf servidor`

`sbf coletor-vps` é um modo à parte, com unit própria, ao lado do `sbf servidor`:

- ciclos de vida diferentes: a API HTTP precisa estar sempre no ar e responder em milissegundos;
  uma coleta de Gmail pode demorar minutos e falhar por causa de rede alheia;
- credenciais diferentes: o servidor não precisa do refresh token do Gmail nem do segredo da
  Pluggy, e o coletor não precisa do client id do login Google da estação (menor superfície);
- o corte do Bun é desligar uma unit e ligar a outra, sem mexer no servidor.

## O que o modo faz

```
sbf coletor-vps                       agenda: Gmail e Pluggy em laço, até SIGTERM
sbf coletor-vps gmail   [--rotulo N] [--consulta Q] [--de AAAA-MM-DD] [--ate AAAA-MM-DD]
                        [--limite N] [--lote N] [--completa] [--simular]
sbf coletor-vps pluggy  [--de AAAA-MM-DD] [--ate AAAA-MM-DD] [--backfill] [--simular]
sbf coletor-vps comparar [--dias N]   só lê: contagens para o corte do Bun (padrão 8 dias)
```

As flags são as do Bun, em português (`--label` vira `--rotulo`, `--query` vira `--consulta`,
`--from-date`/`--to-date` viram `--de`/`--ate`, `--dry-run` vira `--simular`).

**Gmail.** Varre o rótulo (padrão `Comprovantes`), baixa os anexos aceitos (os formatos de
`@sbf/contracts`), grava os bytes no `Armazenamento` (Vercel Blob, o mesmo trait do upload do
desktop) e cria `source_documents` (`origin_type = gmail`, `origin_key = gmail:<mensagem>:<sha256>`)
mais `ingestion_jobs` em `discovered`, com `requires = ['ocr']` para PDF e imagem. Deduplica em
duas fases como o Bun: mensagem+nome antes de baixar, SHA-256 (`document_fingerprints`) depois.

**Pluggy.** Para cada `provider_connections` ativa, lista as transações das contas mapeadas na
`/v2/transactions` e cria `draft_records` (`draft_type = transaction`, `status = pending_review`,
`confidence_score = 1.00`, `external_ref = pluggy:txn_<id>`, único por usuário), com fornecedor
por apelido exato, categoria traduzida (criada na primeira vez) e reconciliação. No fim, espelha o
saldo das contas `BANK` em `financial_products.current_balance` e carimba `last_synced_at`.
`--backfill` usa 365 dias com checkpoint por conta (`ingestion_checkpoints`, `source_type =
pluggy`, `scope_key = <item>:<conta>`), retomável: uma execução interrompida volta da página
onde parou. O incremental usa `last_synced_at` (ou 7 dias) e não usa checkpoint.

## Variáveis de ambiente

Nada de segredo em argv (aparece em `ps`) nem em log. `Debug` e mensagens de erro nunca os
mostram.

| Variável                    | Obrigatória | Uso                                                            |
| --------------------------- | ----------- | -------------------------------------------------------------- |
| `SBF_DATABASE_URL`          | sim         | Postgres (Neon). A mesma do `sbf servidor`.                    |
| `GOOGLE_MAIL_CLIENT_ID`     | Gmail       | OAuth2 do Gmail (as mesmas do worker Bun).                     |
| `GOOGLE_MAIL_CLIENT_SECRET` | Gmail       | Segredo do cliente OAuth2.                                     |
| `GMAIL_REFRESH_TOKEN`       | Gmail       | Refresh token. Só permissão de leitura.                        |
| `PLUGGY_CLIENT_ID`          | Pluggy      | Credenciais da Pluggy (as mesmas do worker Bun).               |
| `PLUGGY_CLIENT_SECRET`      | Pluggy      |                                                                |
| `BLOB_READ_WRITE_TOKEN`     | Gmail       | Vercel Blob, onde os anexos ficam (a mesma do `sbf servidor`). |
| `LOCAL_USER_ID`             | não         | Substituto do dono, **validado** contra `public.users`.        |
| `SBF_COLETOR_GMAIL_INTERVALO_MIN`  | não  | Minutos entre ciclos do Gmail. Padrão 15. `0` desliga.         |
| `SBF_COLETOR_PLUGGY_INTERVALO_MIN` | não  | Minutos entre ciclos da Pluggy. Padrão 360. `0` desliga.       |
| `SBF_COLETOR_GMAIL_ROTULO`  | não         | Rótulo varrido na agenda. Padrão `Comprovantes`.               |
| `SBF_COLETOR_GMAIL_CONSULTA`| não         | Consulta extra do Gmail na agenda. Padrão vazia.               |
| `SBF_COLETOR_GMAIL_COMPLETA_HORAS` | não  | De quantas em quantas horas o Gmail faz varredura completa. Padrão 168 (7 dias). |

O dono sai do banco (a linha com `role = 'owner'`), como no Bun (`resolveOwnerId`);
`LOCAL_USER_ID` só substitui, e um valor que não existe faz o processo falhar na largada.

O primeiro ciclo do Gmail é sempre completo. Os seguintes são incrementais (`after:` a partir do
checkpoint, com 2 dias de folga). **Uma mensagem que o dono rotula depois da data dela só é
vista na varredura completa** (a cada 7 dias por padrão, ou `sbf coletor-vps gmail --completa` à
mão).

## Unit systemd (agenda contínua)

`/etc/systemd/system/sbf-coletor-vps.service`:

```ini
[Unit]
Description=Seu Bolso Feliz - coletas da VPS (Gmail e Pluggy)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=sbf
EnvironmentFile=/etc/sbf/coletor-vps.env
ExecStart=/usr/local/bin/sbf coletor-vps
Restart=on-failure
RestartSec=30
# O processo só fala com o Postgres, o Gmail, a Pluggy e o Vercel Blob.
NoNewPrivileges=true
ProtectSystem=strict
ProtectHome=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

`/etc/sbf/coletor-vps.env` (modo `0600`, dono `root`; a unit lê como root antes de largar para
`sbf`): as variáveis da tabela, uma por linha, `NOME=valor` sem aspas. Pode ser o mesmo arquivo
que o worker Bun já usa; o coletor ignora o que não conhece. O processo encerra limpo no
`SIGTERM` (termina o item em andamento e sai).

Alternativa a um laço contínuo: um `.timer` chamando `sbf coletor-vps gmail` e
`sbf coletor-vps pluggy`. Os subcomandos saem com código 1 se houve erro,
então `OnFailure=` serve. A agenda contínua é a recomendada: guarda a cadência da varredura
completa e não paga o custo de abrir o banco a cada disparo.

## Roteiro de corte (uma semana em paralelo)

Pré-condições: o `sbf servidor` já roda na VPS com `BLOB_READ_WRITE_TOKEN`; o coletor foi
testado à mão com `--simular`.

1. **Dia 0, simulação.** `sbf coletor-vps gmail --simular --limite 20` lista o que viria, sem
   gravar nada. `sbf coletor-vps pluggy --simular` conta quantos drafts criaria (se o Bun está em
   dia, `0 draft(s) seriam criados`).
2. **Dia 1, liga em paralelo.** Habilite `sbf-coletor-vps.service` **sem desligar** o worker Bun.
   A deduplicação é a mesma (mensagem+nome e SHA-256 no Gmail; `external_ref` único na Pluggy;
   `ON CONFLICT DO NOTHING`), então os dois convivem: quem roda primeiro cria, o outro pula. Não
   há risco de documento ou draft duplicado.
3. **Diariamente, compare.** `sbf coletor-vps comparar --dias 8` (só lê: transação `READ ONLY`).
   Distingue as execuções do Gmail pela origem (`rust` tem `metadata.origem = coletor-vps`,
   `bun` não) e mostra por dia: mensagens, anexos achados, jobs criados, já processados, erros.
   - **Gmail:** o que vale é `anexos_achados` igual entre `bun` e `rust` no mesmo dia (cada um
     o que ENCONTROU, não o que criou, já que o primeiro a rodar cria). O relatório imprime
     `DIVERGE em <dia>` quando os dois rodaram e discordam. Confira também
     `documentos do Gmail sem job` (deve ser 0).
   - **Pluggy:** o total de drafts por dia de criação e `sbf coletor-vps pluggy --simular` (o que
     o Rust ainda criaria). Se o Bun trouxe tudo, dá `0`; se dá mais que 0 após o Bun rodar,
     investigue antes de desligar.
   - Confira os **checkpoints** impressos (situação `completed` ou `running` sem estacionar) e o
     `last_synced_at` das conexões.
4. **Critério de corte.** Sete dias seguidos sem `DIVERGE`, sem documentos sem job, sem erros do
   lado Rust, e, para a Pluggy, `--simular` dando 0 depois de cada ciclo do Bun. Rode também uma
   vez `sbf coletor-vps pluggy --backfill` e confira que termina com `0 draft(s) criado(s)` (o
   Bun já trouxe a janela) e o checkpoint `completed`.
5. **Corte.** Desligue o worker Bun correspondente, um de cada vez (primeiro o `pluggy-sync`, de
   menor risco; depois o `gmail-scanner`), e mantenha o outro. Registre a data. Depois de mais uma
   semana com o Rust sozinho, `sbf coletor-vps comparar` deve mostrar só linhas `rust`.
6. **Volta.** Para reverter, religue o worker Bun: nada mudou de formato no banco, então ele
   retoma sem migração.

## Diferenças em relação ao Bun

Fiéis ao Bun: formato de `source_documents`/`ingestion_jobs`/`draft_records`, `requires = ['ocr']`
só para PDF e imagem, `origin_key`, `external_ref`, as duas fases de dedup, a tradução de
categorias (103 entradas), o rascunho (o `draft_data` de uma transação de exemplo foi conferido contra a saída do `build-draft.ts` real, rodado com o Bun), o
cursor da `/v2/transactions` (só o token `after`), a regra de saldo (só `BANK`), o arredondamento
de centavos (`Math.round` do JavaScript).

Diferentes, de propósito:

| Assunto | Bun | Rust |
| --- | --- | --- |
| Documento + job | duas gravações; documento sem job se o job falhar, e a fase 1 o esconde para sempre | uma transação; some tudo ou nada |
| `ingestion_runs` (Gmail) | uma execução por varredura, mesmo vazia | só nasce no primeiro documento novo |
| `draft_batches` (Pluggy) | um lote por sync, mesmo vazio | só nasce no primeiro draft novo |
| Chave no blob | `<dono>/<uuid>/<nome>` | `<dono>/<sha256>/<nome>` (repetir o envio não deixa blob órfão) |
| Falha ao ler uma mensagem | aborta a varredura inteira | conta erro e segue |
| Falha de uma conexão Pluggy | aborta o processo, as outras conexões não rodam | conta falha, segue, sai com código 1 |
| Checkpoint do Gmail | não usava (`ingestion_checkpoints` só na Pluggy) | `source_type = gmail`, `after:` incremental |
| Contadores do checkpoint | lê e reescreve (corrida) | `SET col = col + n` no banco |
| Reconciliação, regra 3 | consultava até `AAAA-MM-31`, que o Postgres recusa em meses de 28/30 dias (a reconciliação do draft falhava "sem ser crítico") | até o último dia real do mês |
| Data torta no draft | exceção do Zod interrompe a conexão | erro do item; segue |
| `last_synced_at` | `split("T")[0]` do texto do driver | data UTC do instante |

## O que ficou de fora

- **`--include-body` / `--no-attachments` do Gmail** (corpo do e-mail como evidência, com o
  classificador `classifyFinancialIntent`, 800 linhas em `@sbf/domain`). Não é o caminho da
  agenda (o padrão do Bun é desligado). Se o corpo importar, é o próximo porte.
- **Regra 1 da reconciliação** (hash duplicado): nunca dispara para a Pluggy (o draft não tem
  documento de origem). As regras 2 e 3 estão portadas.
- **Retentativa e backoff em 429/5xx**: o Bun também não tinha; o ciclo seguinte repete.
- **Deploy, unit instalada e Caddy**: nada foi instalado; este documento é o roteiro.
- Nada foi testado contra o Gmail, a Pluggy ou o Vercel Blob de verdade (fora do escopo desta
  rodada). Os testes usam servidores HTTP falsos locais com respostas gravadas à mão em
  `crates/sbf-coletor/tests/dados/` e o Postgres de teste em Docker:

```
scripts/desktop/postgres-de-teste.sh testar-coletor
```

O protocolo do Vercel Blob (`VercelBlob`, já existente no `sbf-servidor`) segue **UNVERIFIED**
contra a API real: a primeira gravação de verdade confirma.
