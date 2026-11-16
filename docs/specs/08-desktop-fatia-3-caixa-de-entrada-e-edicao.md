# Spec 08 — Desktop, fatia 3: caixa de entrada, edição e envio de lotes

**Status:** decidida (planejamento com o CEO em 2026-11-14) · **Depende de:** spec 06, spec 07

## Objetivo

O desktop passa a **escrever** no livro-caixa. Ao fim da fatia:

- **Caixa de entrada.** A área Documentos → Caixa de entrada lista os rascunhos que o pipeline de
  ingestão gerou (`draft_batches`, `draft_records` em `pending_review`/`corrected`). Você pode
  corrigir campos, aprovar, recusar e aprovar o lote inteiro. Aprovar lança no livro-caixa.
- **Edição.** As páginas de dinheiro ganham edição pelas Ações abaixo. Toda escrita é uma
  **alteração** (spec 07, Q6 e Q7): uma Ação com a versão de base de cada linha tocada, que se
  aplica inteira ou é recusada com o lado a lado.
- **Envio de lote.** O botão Enviar ao servidor, desabilitado desde a fatia 1, passa a funcionar.
  Um lote aprovado no inventário sobe os documentos ao servidor, que cria `source_documents` e
  `ingestion_jobs`. O pipeline Bun atual processa esses jobs, e os rascunhos voltam pela caixa de
  entrada. Com o aceite do servidor, os registros do lote ficam `promovido`, que é terminal.
- **Delta por cursor.** A sincronização deixa de trazer a coleção inteira: passa a trazer o que
  mudou desde o cursor (Q5 da spec 07 adiou isso para cá).

## Fora do escopo

- O worker Rust, as conexões, os alertas e os canais: fatia 4.
- O port do pipeline de ingestão para Rust. Os jobs continuam no worker Bun.
- O port de `fn_materialize_draft_record`. O servidor a **chama** na mesma transação em que
  aprova; o port vem depois, com teste de ouro.

## Contrato (crate `sbf-contrato`, versão 2; o servidor aceita v1 e v2)

Novas coleções, só leitura pela sincronização:

| Coleção              | Tabela          | Observação                               |
| -------------------- | --------------- | ---------------------------------------- |
| `lotes-de-rascunhos` | `draft_batches` | todas as do dono                         |
| `rascunhos`          | `draft_records` | `draft_data` vai como JSON v1 já migrado |

Novas Ações (`Acao`), além de `MudarCategoriaDaMovimentacao`:

| Ação                                 | Toca                                 | Efeito no servidor                                                     |
| ------------------------------------ | ------------------------------------ | ---------------------------------------------------------------------- |
| `CorrigirRascunho`                   | o rascunho                           | grava `draft_data` v1, `status = corrected`, `corrections.manual_edit` |
| `AprovarRascunho`                    | o rascunho                           | `approved` e chama `fn_materialize_draft_record` na mesma transação    |
| `RecusarRascunho { motivo }`         | o rascunho                           | `rejected`, motivo em `corrections.rejection_reason`                   |
| `AprovarLoteDeRascunhos`             | o lote e cada rascunho pendente dele | aprova e materializa um a um, tudo ou nada                             |
| `EditarMovimentacao`                 | a movimentação                       | descrição, data, valor (centavos), conta, fornecedor, centro, notas    |
| `ExcluirMovimentacao`                | a movimentação e os rateios dela     | apaga as duas coisas                                                   |
| `RatearMovimentacao`                 | a movimentação e os rateios dela     | troca o conjunto de rateios inteiro; a soma tem de bater com o valor   |
| `ConciliarRascunho { movimentacao }` | o rascunho e a movimentação          | `reconciliation_status = confirmed_duplicate`, aponta a movimentação   |

A versão de base é sempre `updated_at` da linha. Uma linha que não tem `updated_at` ganha a coluna
na migração `0005_versoes.sql`, com trigger que a atualiza.

**Envio de documentos:** `POST /v2/documentos` em multipart, com os bytes, o `sha256`, o nome
original e o id do registro no inventário. É idempotente pelo `sha256` do dono: reenviar devolve
o mesmo `source_document_id`. A resposta traz `source_document_id` e `job_id`. O servidor grava
os bytes por um trait `Armazenamento`, com uma implementação falsa para os testes e uma do Vercel
Blob (PUT na API do Blob, com o token só no `EnvironmentFile` da VPS).

**Delta:** `GET /v2/colecoes/{c}?desde=<cursor>`. O cursor é opaco e carrega o maior
`updated_at` visto mais o desempate por `id`. Para as exclusões, uma tabela `exclusoes(colecao,
id, excluida_em)` gravada por trigger.

## Estação (`sbf-estacao`)

- **Fila de alterações.** A fila fica no `livro-caixa.sqlite` e sobrevive a fechar o app. O envio
  é em ordem e reenvia com o mesmo `id` quando a rede cai.
- **Aceite e recusa.** Um aceite grava as linhas devolvidas no cache. Uma recusa guarda "Sua
  versão" e "No servidor agora" para a área Sincronização mostrar.
- **Aplicação otimista.** O cache mostra a alteração como **pendente**, com marca visual, até o
  aceite.
- **Envio de lote.** Envia os documentos e, a cada aceite, grava a decisão de promover no
  inventário (`decidido_por = usuario`). Um envio pela metade retoma de onde parou.

## Janela (`sbf-janela`)

- **Caixa de entrada.** A lista vem agrupada por lote, com a prévia do documento ao lado (o
  componente da fatia 1). O formulário de correção traz os campos do `draft_data` v1.
- **Botões.** Aprovar, Recusar e Aprovar o lote, com atalhos de teclado (A, R e Shift+A).
- **Páginas de dinheiro.** Ganham Editar, Ratear e Excluir. Excluir pede confirmação.
- **Sincronização.** A área mostra a fila pendente e as recusas lado a lado, com Refazer sobre a
  nova versão e Descartar.

## Matriz de aceite

| ID    | Critério                                                                                |
| ----- | --------------------------------------------------------------------------------------- |
| CX-01 | Aprovar um rascunho cria a movimentação no Postgres de teste, pela função existente     |
| CX-02 | Aprovar um lote com um rascunho inválido não aprova nenhum (tudo ou nada)               |
| CX-03 | Alteração com versão de base velha é recusada e nada muda no banco                      |
| CX-04 | Reenviar a mesma proposta devolve a mesma resposta                                      |
| CX-05 | Ratear com soma diferente do valor é pedido inválido                                    |
| CX-06 | Delta traz só o que mudou e as exclusões; duas leituras seguidas sem mudança vêm vazias |
| CX-07 | Envio de lote idempotente por `sha256`; registros viram `promovido` só após o aceite    |
| CX-08 | A fila de alterações sobrevive a reabrir a estação e esvazia em ordem                   |
| CX-09 | Contrato v1 continua aceito (a estação da fatia 2 não quebra)                           |
| CX-10 | A janela mostra pendente, aceita e recusada; atalhos A, R e Shift+A funcionam           |

Todos os testes usam o Postgres de teste em Docker com dados **fictícios**
(`scripts/desktop/postgres-de-teste.sh`).
