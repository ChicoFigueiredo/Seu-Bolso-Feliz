# 07 — Spec: Desktop, fatia 2 — Estação, sincronização e livro-caixa só leitura

> **Estado: ✅ decidida no grill do CEO em 2026-11-13** (respostas na seção "Decisões do grill", no fim). Implementação ainda não começou.
> Escrito em 2026-11-13, durante a implementação da fatia 1. Decisões já tomadas em
> ADR-010, ADR-012 a ADR-015 e ADR-018 (`docs/arquitetura/`); vocabulário em `CONTEXT.md`.
> Os fatos da seção "O que existe hoje" foram conferidos no código neste dia, com o arquivo
> citado. Onde o código e uma doc divergem, vale o código. Tudo o que é decisão do CEO
> está em "Perguntas para o grill", no fim, e o corpo da spec marca esses pontos com
> **(grill Qn)**. Exemplos com nomes e valores são fictícios.

## Objetivo

O desktop passa a **mostrar o livro-caixa**, lido do servidor e guardado numa cópia local,
e ganha identidade própria perante o servidor. Ao fim da fatia:

- existe o **`sbf servidor`** na VPS: a API Rust, única autoridade do livro-caixa, que lê
  o Neon (ADR-013);
- cada instalação do desktop é uma **estação**, com credencial própria, que pode ser
  revogada, emitida depois do login Google;
- a **sincronização** traz o livro-caixa para um cache SQLite local, e o desktop o mostra
  mesmo offline;
- as áreas de Dinheiro do painel de navegação funcionam **só para leitura**: Hoje,
  Movimentações, Contas e cartões, Dívidas, Recorrentes, Centros e Fornecedores;
- as páginas das entidades de dinheiro funcionam, e as abas "do livro-caixa" que a
  fatia 1 deixou desabilitadas acendem;
- Relatórios ganha a fonte **Livro-caixa**, pelo mesmo código das 8 perguntas
  (`FonteDeGastos`);
- a área **Sincronização** mostra as estações e o estado da última troca.

Escrever no livro-caixa (alterações, recusa lado a lado em uso real) é a fatia 3, junto
com a caixa de entrada e o envio dos lotes. Esta fatia fixa o **contrato** das
alterações, para a fatia 3 não redesenhar o protocolo (grill Q6 e Q7).

## Fora do escopo desta fatia

- Editar qualquer coisa do livro-caixa no desktop: fatia 3. Movimentações é "2 leitura,
  3 edição" na navegação da spec 06.
- Caixa de entrada, envio de lote e promoção: fatia 3.
- Modo `sbf worker`, conexões, alertas e canais: fatia 4. A Pluggy e os scanners
  continuam em Bun.
- Roteiros (5), IA, RAG, pontos de atenção e MCP (6).
- Apagar o web. Ele continua de pé; esta fatia só decide se as telas dele passam a
  consumir a API para as regras portadas (grill Q12).
- Trocar o Vercel Blob por storage S3 (ADR-014: "fica para quando a API na VPS pedir").

## O que existe hoje (verificado no código)

| Fato                                                                                                                                                                                                                                                                                                   | Onde                                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Não há API de CRUD: a leitura das telas novas é feita por Server Actions `painel-*.ts`, que compõem as actions de CRUD.                                                                                                                                                                                | `apps/web/src/app/actions/painel-*.ts`, `CLAUDE.md`                                                                                                                                               |
| A sessão é um JWT HS256 de 30 dias num cookie, sem revogação e sem identidade de dispositivo. O login é Google (OIDC, JWKS do Google).                                                                                                                                                                 | `apps/web/src/lib/auth/session.ts` (`DURACAO_SESSAO_SEGUNDOS`), `google.ts`                                                                                                                       |
| Todo filtro de dado é por `ownerId` (dono), nunca por `id` (ator). O Neon não tem RLS.                                                                                                                                                                                                                 | `apps/web/src/lib/auth/index.ts`, cabeçalho de `painel-movimentacoes.ts`                                                                                                                          |
| Dinheiro é `numeric(15,2)` no banco, lido pelo Drizzle em `mode: "number"` (ponto flutuante do JS). Os rateios são `numeric(14,2)`.                                                                                                                                                                    | `packages/db/src/schema.ts` (`transactions.amount`, `transaction_allocations.amount`)                                                                                                             |
| 15 tabelas do `schema.ts` não têm `updated_at`, entre elas `transaction_allocations`, `transaction_tags`, `supplier_tags`, `liability_tags` e `recurring_template_tags`. A ADR-013 fala em 14.                                                                                                         | `packages/db/src/schema.ts`                                                                                                                                                                       |
| Nenhuma tabela tem tombstone (`deleted_at`) nem versão por linha. As actions fazem 30 chamadas `.delete(` físicas.                                                                                                                                                                                     | `schema.ts`; `grep '\.delete(' apps/web/src/app/actions/*.ts`                                                                                                                                     |
| `schema.sql` é linha de base; o banco real é ele mais `migrations/0001..0003`.                                                                                                                                                                                                                         | `packages/db/README.md`                                                                                                                                                                           |
| O cartão tem dois UUIDs: um em `cards` e outro em `financial_products`. A fatura (`statement_cycles.card_id`) aponta para `cards`.                                                                                                                                                                     | comentário e junção em `painel-hoje.ts`                                                                                                                                                           |
| O saldo de conta é **gravado**, não derivado das movimentações: `financial_products.current_balance`, digitado no web ou atualizado pela Pluggy.                                                                                                                                                       | `workers/pluggy-sync/src/saldos.ts`, `components/contas/conta-sheet.tsx`                                                                                                                          |
| Sem preferência gravada, o web usa dia de corte **1**; o glossário diz que hoje o corte é **20**, e o inventário da fatia 1 usa 20 fixo.                                                                                                                                                               | `painel-hoje.ts` (`pref?.dia ?? 1`), `painel-centros.ts`, `CONTEXT.md` (Ciclo)                                                                                                                    |
| `getCurrentPeriod` usa os getters locais de `Date`. Rodando em UTC, o início e o fim do ciclo saem **um dia antes** depois do `paraIso`: reproduzido com `TZ=UTC`, o ciclo 2026-10 sai 19/out–18/nov em vez de 20/out–19/nov. Afeta Hoje, Centros, Recorrentes, Movimentações e os relatórios antigos. | `packages/domain/src/financial-cycle/index.ts`, `apps/web/src/lib/datas.ts`; chamadores em `painel-hoje.ts`, `painel-centros.ts`, `painel-recorrentes.ts`, `components/movimentacoes/periodos.ts` |
| A tela Hoje do web chama de "pendências" contagens de movimentações (vencidas, sem categoria, não alocadas, não confirmadas) e documentos a revisar. O glossário reserva "Pendência" para o inventário.                                                                                                | `painel-hoje.ts` (`contarPendencias`), `CONTEXT.md`                                                                                                                                               |
| `adjustment` fica fora de entrou e de saiu em Movimentações, mas conta como **saída** no saldo do centro (tudo que não é `income`/`refund` sai) e entra no "não alocado".                                                                                                                              | `painel-movimentacoes.ts`, `painel-hoje.ts` (`saldoPorCentro`), `painel-centros.ts`, `packages/domain/src/allocation/transaction-direction.ts`                                                    |
| Transferência entre contas é uma tabela à parte (`transfers`), fora de `transactions`.                                                                                                                                                                                                                 | `schema.ts`                                                                                                                                                                                       |
| A fatia 1 já tem as costuras: `Tipo` com os 34 tipos e a fatia de cada um, `Ref { tipo, chave }`, o trait `Consulta` com `Fonte::LivroCaixa` que devolve `NaoSuportado`, e o trait `FonteDeGastos`, que o livro-caixa implementa.                                                                      | `crates/sbf-inventario/src/{referencia,consulta,relatorios}.rs`                                                                                                                                   |
| Na fatia 1, a chave de `conta` e `fornecedor` é **o nome como está no inventário**; no livro-caixa, a identidade é um UUID.                                                                                                                                                                            | doc de `Ref` em `referencia.rs`                                                                                                                                                                   |
| Três das 8 perguntas do §11 (`NaoClassificado`, `PossiveisDuplicidades`, `OQueFaltaParaConfiar`) leem o `Contexto` do inventário, não a `FonteDeGastos`.                                                                                                                                               | `responder_com` em `relatorios.rs`                                                                                                                                                                |
| O `chess-net` é síncrono (`ureq` em threads, sem runtime assíncrono), com portão, catálogo HTTPS, cache SQLite com `ETag` e leitura offline.                                                                                                                                                           | `/mnt/d/Chico/chess-studio/crates/chess-net/src/lib.rs`                                                                                                                                           |

## Onde o código mora

Identificadores em português, sem acento (ADR-018).

### Na `bancada`

| Crate          | O que é                                                                                                                                                                                                                                                                                                                             | Vem de                                     |
| -------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------ |
| `bancada-rede` | O portão de rede da estação: catálogo de endereços HTTPS, cabeçalho de autorização vindo do `bancada-segredos` e nunca em `Debug`/log, limites e backoff, cache SQLite com `ETag` e leitura offline, estado da rede para a barra de status, e o OAuth por loopback (navegador do sistema + porta local). Síncrono, como o original. | `chess-net` (portão, cache, `loopback.rs`) |

### Neste monorepo, em `crates/`

| Crate          | O que é                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| -------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `sbf-livro`    | O domínio do livro-caixa, sem rede, sem banco e sem egui: tipos (movimentação, conta, cartão, fatura, dívida, parcela, recorrente, centro, rateio…), dinheiro em centavos `i64`, e as **regras portadas** que as telas de leitura usam: ciclo, direção do rateio, amortização SAC/Price/Misto, equivalente mensal da recorrência e os números de Hoje sem dupla contagem. Usado pelo servidor **e** pela estação: uma regra, um código (ADR-012). |
| `sbf-contrato` | Os tipos do contrato HTTP entre estação e servidor, com a versão do contrato e os erros. Serde, sem lógica.                                                                                                                                                                                                                                                                                                                                       |
| `sbf-servidor` | O modo `sbf servidor`: HTTP, autenticação de estação, leitura do Neon com filtro por dono, montagem das respostas com `sbf-livro`. Só ele tem a `DATABASE_URL`.                                                                                                                                                                                                                                                                                   |
| `sbf-estacao`  | O lado da estação: pareamento, credencial no chaveiro, cache SQLite local do livro-caixa (`bancada-armazem`), sincronização, e a implementação de `Consulta` e `FonteDeGastos` para `Fonte::LivroCaixa` sobre o cache. Sem egui.                                                                                                                                                                                                                  |
| `sbf`          | O binário ganha o modo `sbf servidor`. A janela continua sendo o modo sem argumento.                                                                                                                                                                                                                                                                                                                                                              |

As regras portadas seguem o ADR-012: os testes TS viram fixtures de ouro, e a versão TS
sai no mesmo PR, com quem a usava passando a chamar o Rust. Como o web ainda usa
`getCurrentPeriod`, `allocationDirection` e a amortização, isso amarra esta fatia à
pergunta da transição do web (grill Q12).

## O servidor (`sbf servidor`)

### Onde roda

- Na VPS de São Paulo, como serviço do sistema, atrás de TLS válido (grill Q1).
- Lê o Neon. A ADR-013 manda migrar o Neon para `aws-sa-east-1`, num projeto novo, com o
  portão humano do ADR-007. Quando isso acontece nesta fatia é grill Q2.
- O ADR-010 continua valendo para os workers ao lado do banco; a API é só o canal das
  estações. O servidor nunca chama a estação: a estação puxa.

### Recursos (só leitura nesta fatia)

Todos sob `/api/v{N}/` (versão do contrato, abaixo), JSON, filtrados pelo dono da
credencial.

| Recurso                         | O que devolve                                                                                                                                                                                                                                                                                                                                                                                       |
| ------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `GET /estado`                   | Versão do contrato e das regras (`sbf --version` do servidor), hora do servidor, dia de corte do ciclo, e a identidade da estação que chamou.                                                                                                                                                                                                                                                       |
| `GET /livro/{colecao}`          | As linhas de uma coleção do livro-caixa: `contas` (produtos financeiros), `instituicoes`, `cartoes`, `faturas`, `itens-de-fatura`, `movimentacoes`, `transferencias`, `rateios`, `categorias`, `tags`, `centros`, `fornecedores`, `apelidos`, `contratos`, `dividas`, `parcelas`, `recorrentes`, `ocorrencias`, `documentos` (metadados). Com `ETag` por coleção, ou com cursor, conforme grill Q5. |
| `GET /documentos/{id}/original` | Os bytes do original (hoje no Vercel Blob), para a sincronização copiar no cofre. Só se o grill Q9 trouxer isso para esta fatia.                                                                                                                                                                                                                                                                    |
| `POST /estacoes/pareamento`     | Troca a prova do login Google por uma credencial de estação.                                                                                                                                                                                                                                                                                                                                        |
| `GET /estacoes`                 | As estações do dono, com nome, sistema, último contato e estado da credencial.                                                                                                                                                                                                                                                                                                                      |
| `POST /estacoes/{id}/revogar`   | Revoga a credencial de uma estação. Revogar não apaga nada do servidor.                                                                                                                                                                                                                                                                                                                             |
| `POST /alteracoes`              | **Reservado.** Na fatia 2 responde "ainda não aceito" com o código próprio; o formato do corpo é fixado aqui (grill Q6, Q7).                                                                                                                                                                                                                                                                        |

O servidor devolve **linhas**, não telas: os números (Disponível agora, Resultado do mês…)
são calculados na estação pelo `sbf-livro` sobre o cache, para valerem offline. A
alternativa, o servidor mandar os números prontos, é grill Q8.

Dinheiro trafega como **centavos inteiros**. O servidor lê `numeric` como texto e converte
sem passar por ponto flutuante.

### Autenticação da estação

1. A estação abre o navegador do sistema no login Google (loopback do `bancada-rede`).
2. O servidor valida o `id_token` contra o JWKS do Google e confere se o e-mail é de um
   usuário conhecido (mesma regra do web, `apps/web/src/lib/auth/google.ts`).
3. O servidor emite uma **credencial de estação** e grava só o que precisa para conferir
   (por exemplo, o hash), com o nome da estação, o sistema e a data. O formato é grill Q3.
4. A estação guarda a credencial no chaveiro (`bancada-segredos`, chave `estacao:<servidor>`),
   nunca em arquivo de configuração, log ou SQLite.
5. Todo pedido leva a credencial. O servidor resolve dono e estação por ela e registra o
   último contato.

O cookie de 30 dias do web não muda nesta fatia.

### Revogação

- Pela área Sincronização (Ação "Revogar estação", que já está no protótipo) de qualquer
  outra estação, e por um comando de administração do `sbf servidor` na VPS, para quando
  a única estação foi roubada.
- Uma credencial revogada recebe `401` com o código "estação revogada". A estação para de
  sincronizar e mostra o motivo. O que acontece com o cache local é grill Q11.
- Revogar não apaga o que já foi sincronizado no servidor (texto do protótipo).

### Versão do contrato

- O contrato tem um número inteiro. A estação manda o dela em todo pedido, e o servidor
  responde com o dele em `/estado`.
- O que o servidor faz com uma estação de contrato diferente é grill Q4.
- Uma versão do binário é uma versão das regras (ADR-012): o `/estado` informa a versão
  das regras do servidor, e a barra de status da estação avisa quando a dela é diferente.

## O cache local do livro-caixa

- **Onde:** `livro-caixa.sqlite` na pasta de dados do app (XDG_DATA_HOME, %APPDATA% ou
  Application Support), um arquivo por servidor e dono. **Nunca** dentro do cofre e nunca
  no `inventario.sqlite`: o inventário não sincroniza, e misturar os dois bancos apagaria
  essa fronteira.
- **Dono do esquema:** a estação, pela escada de migrações do `bancada-armazem`. O cache
  é descartável: uma migração pode simplesmente apagar e baixar de novo.
- **O que guarda:** as coleções de `GET /livro/*`, com a marca da última troca de cada
  uma, e o `/estado`. O recorte (tudo, ou só os últimos ciclos) é grill Q10.
- **Como invalida:** conforme grill Q5 (coleção inteira com `ETag`, ou delta por cursor).
  Uma coleção baixada substitui a anterior numa transação só: a tela nunca vê metade de
  uma troca.
- **Quando sincroniza:** ao abrir, por Ação "Sincronizar agora", e de tempos em tempos
  com a janela aberta (intervalo em Opções). Nada roda com a janela fechada até o
  `sbf worker` da fatia 4.
- **Offline:** o desktop mostra o cache. O que a tela diz sobre a idade dos números, em
  especial "Disponível agora", é grill Q13.
- **Leitura e escrita:** a sincronização escreve por uma conexão; a interface lê por
  outra (lição do LICOES.md da fatia 1). Nenhuma transação atravessa um quadro.

## A área Sincronização (grupo Estação)

Como no protótipo (`ganchos.js`, `telaSinc`; `paginas-fontes.js`, `paginas.estacao` e
`paginas.alteracao`):

- **Cabeçalho:** esta estação, a última sincronização, "servidor na VPS de São Paulo" e
  "O inventário não sincroniza". Botão Sincronizar agora.
- **Aguardando:** as alterações ainda não aceitas. Nesta fatia fica vazia, com o texto
  "Editar o livro-caixa chega na fatia 3" (grill Q6).
- **Recusadas:** a lista lado a lado ("Sua versão" × "No servidor agora"), com Refazer
  sobre a nova e Descartar. Nesta fatia, vazia com o mesmo motivo; o componente nasce
  testado com dados de teste, para a fatia 3 só ligar.
- **Estações:** nome, sistema, última sincronização e credencial (ativa ou revogada), com
  a marca "esta". Ações: Revogar estação, Renomear.
- **Página da estação:** abas Sincronização, Alterações e Conexões (esta última
  desabilitada até a fatia 4, com o motivo).
- **Histórico de trocas:** cada sincronização registra início, fim, coleções trocadas,
  quantas linhas e o erro, se houve. Fica na estação, não no servidor.
- **Barra de status:** "Sincronizado às HH:MM", "Sem rede, mostrando a cópia de HH:MM",
  "Estação revogada" ou "Servidor com regras mais novas".

## As áreas de Dinheiro que passam a funcionar

Cada área responde à mesma pergunta da tela do web correspondente, com os mesmos
números. A fórmula de cada número está abaixo, com o arquivo do web de onde vem; o
`sbf-livro` porta a regra e o teste de ouro compara os dois sobre o mesmo banco de teste.

| Área                          | Pergunta                                                      | Tela do web de referência                                                     |
| ----------------------------- | ------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| Hoje                          | Quanto eu tenho, quanto é meu, o que vence, como está o ciclo | `(nova-ia)/hoje`, `painel-hoje.ts`                                            |
| Movimentações                 | O que entrou e o que saiu                                     | `(nova-ia)/movimentacoes`, `painel-movimentacoes.ts`                          |
| Contas e cartões, com faturas | Onde está o dinheiro e quanto devo no cartão                  | `(nova-ia)/contas`, `painel-contas.ts` + `components/contas/montar-painel.ts` |
| Dívidas                       | Quanto falta e quando acaba                                   | `(nova-ia)/dividas` (+ `[id]`), `painel-dividas.ts`                           |
| Recorrentes                   | Quanto do ciclo já está prometido                             | `(nova-ia)/recorrentes`, `painel-recorrentes.ts`                              |
| Centros                       | Quanto foi para cada centro e quanto não foi alocado          | `(nova-ia)/centros` (+ `[id]`), `painel-centros.ts`                           |
| Fornecedores                  | Quanto e com que frequência sai para cada um                  | telas antigas `dashboard/suppliers` (não há tela nova)                        |

### Os números e as fórmulas

| Número                                       | Fórmula hoje                                                                                                                                                                                                                                            | Arquivo                                                                                      |
| -------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------- |
| **Disponível agora**                         | Σ `current_balance` dos produtos ativos de tipo `checking_account` ou `savings_account`.                                                                                                                                                                | `painel-hoje.ts`                                                                             |
| **Fatura atual** de um cartão                | `total_amount − paid_amount` do `statement_cycle` não pago do cartão, casado por `cards.financial_product_id`. Limite livre = `credit_limit − fatura atual`.                                                                                            | `painel-hoje.ts`, `montar-painel.ts`                                                         |
| **Compromissos**                             | Saídas não confirmadas do ciclo **sem** `statement_cycle_id` e **sem** `liability_installment_id` + ocorrências pendentes de recorrentes ativos, sem `transaction_id`, até o fim do ciclo + faturas abertas com saldo > 0.                              | `painel-hoje.ts`                                                                             |
| **Livre após compromissos**                  | Disponível agora − Σ compromissos. É estimativa sempre que alguma ocorrência não tem valor esperado.                                                                                                                                                    | `painel-hoje.ts`                                                                             |
| Ainda vai sair                               | Σ compromissos com data entre hoje e o fim do ciclo. "Falta até o fim" = máx(0, ainda vai sair − disponível agora). Livre projetado = disponível agora − ainda vai sair.                                                                                | `painel-hoje.ts`                                                                             |
| **Resultado do mês** (do ciclo)              | Entrou − saiu. Entrou = `income` + `refund` confirmados. Saiu = `expense`, `fee`, `interest_charge`, `statement_payment`, `liability_payment` confirmados **fora** do cartão. A compra no cartão vai para "No cartão", à parte. `adjustment` fica fora. | `painel-hoje.ts`, `painel-movimentacoes.ts`                                                  |
| Dívida total / compromisso mensal de dívidas | Σ `outstanding_balance` das dívidas ativas; mensal = Σ saldo ÷ parcelas restantes. Informativo: não entra no livre após compromissos.                                                                                                                   | `painel-hoje.ts`                                                                             |
| Previsão de fim e próxima parcela            | Da parcela lançada quando existe; senão projeção pela cadência, marcada `estimativa` com a premissa.                                                                                                                                                    | `painel-dividas.ts`                                                                          |
| Simulação SAC × Price                        | `generateSchedule` / `simulateEarlyPayoff`. O protótipo recalcula em JS só para ilustrar.                                                                                                                                                               | `packages/domain/src/amortization/index.ts`                                                  |
| Total mensal comprometido (recorrentes)      | Σ equivalente mensal: semanal ×52/12, quinzenal ×26/12, trimestral ÷3, anual ÷12, personalizado × dias/mês ÷ intervalo. Estimativa quando há valor variável ou ausente.                                                                                 | `painel-recorrentes.ts`                                                                      |
| Saldo do centro                              | Σ rateios de `income`/`refund` − Σ rateios do resto, desde sempre.                                                                                                                                                                                      | `painel-hoje.ts`, `packages/domain/src/allocation/{transaction-direction,center-balance}.ts` |
| Não alocado                                  | Valor das saídas rateáveis do ciclo (tudo menos `income`, `refund` e `statement_payment`) − Σ rateios delas.                                                                                                                                            | `painel-centros.ts`                                                                          |
| Ciclo                                        | `getCurrentPeriod({ startDay })`, com o dia de corte de `user_financial_preferences`.                                                                                                                                                                   | `packages/domain/src/financial-cycle/index.ts`                                               |

Ao portar, o `sbf-livro` corrige o ciclo em UTC (calcula em America/Sao_Paulo, como o
inventário da fatia 1 já faz) e precisa de uma decisão sobre `adjustment` (grill Q15) e
sobre o dia de corte padrão (grill Q14). O nome que substitui "pendências" na Hoje é
grill Q16.

### Páginas e abas que acendem

Pela regra da spec 06 ("todo nome é link"), estas páginas passam a funcionar com dados
reais, com as abas do protótipo (`paginas-dinheiro.js`):

| Página         | Abas que passam a funcionar                                                                                                                   |
| -------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| Movimentação   | Rateio por centro, Documento, Ligações. Registro de origem · a conferir só quando houver promoção (fatia 3). Alterações: vazia, com o motivo. |
| Transferência  | Movimentações, Contas, Documentos                                                                                                             |
| Conta          | Movimentações, Transferências, Extratos e documentos. Conexão de origem desabilitada até a fatia 4.                                           |
| Instituição    | Contas, Cartões, Produtos, Dívidas                                                                                                            |
| Produto        | Movimentações, Instituição e conta, Documentos                                                                                                |
| Cartão         | Faturas, Itens, Documentos, Compromissos e dívidas                                                                                            |
| Fatura         | Itens, Pagamento, Documento da fatura                                                                                                         |
| Ciclo          | Movimentações e Compromissos acendem ao lado de Registros · a conferir                                                                        |
| Categoria, Tag | Movimentações, Fornecedores, Compromissos e recorrentes, Documentos                                                                           |
| Centro         | Rateios, Movimentações, Compromissos e recorrentes, Documentos                                                                                |
| Compromisso    | Origem, Pagamento, Documentos                                                                                                                 |
| Recorrente     | Ocorrências, Próximos compromissos, Fornecedor e contrato, Documentos                                                                         |
| Dívida         | Parcelas, Simulação SAC × Price (desabilitada quando não há juros), Movimentações, Contrato e documentos                                      |
| Parcela        | Pagamento, Boleto e documentos, Ligações                                                                                                      |
| Fornecedor     | **Movimentações** e **Compromissos e recorrentes** acendem ao lado de Registros · a conferir e Documentos                                     |
| Contrato       | Movimentações, Compromissos e recorrentes, Documentos, Dívidas e parcelas                                                                     |
| Estação        | Sincronização, Alterações                                                                                                                     |
| Alteração      | Mudança, Item alterado, Estação (com dados de teste até a fatia 3)                                                                            |

Regras que continuam:

- **Duas fontes, nunca somadas.** Movimentações (livro-caixa, sem marca) e Registros · a
  conferir (inventário, com a marca) ficam em abas separadas. Nenhum total soma as duas.
- **Todo total é clicável** e abre as movimentações que o somam.
- **Rastro:** para a frente, de movimentação a rateio e centro, passa a funcionar. Para
  trás, até o documento do servidor. A ligação registro → movimentação nasce com a
  promoção (fatia 3).
- A identidade de conta e fornecedor entre as duas fontes (nome no inventário, UUID no
  livro-caixa) é grill Q17.
- O Ribbon da área mostra as Ações de leitura (Pesquisar, Período, Filtros, Atualizar,
  Sincronizar agora). As Ações de edição aparecem desabilitadas com "Chega na fatia 3".
- `Tipo::fatia()` em `referencia.rs` passa a devolver 2 só para os tipos que esta fatia
  liga de verdade; os que dependem de edição continuam com o motivo da fatia 3.

## Relatórios com a fonte Livro-caixa

- O seletor de fonte da área Relatórios habilita "Livro-caixa". `responder` passa a
  aceitar `Fonte::LivroCaixa` e chama `responder_com` com a `FonteDeGastos` do cache.
  **O código das perguntas não muda** (ADR-012).
- Os números do livro-caixa **não** levam "a conferir" (`CONTEXT.md`).
- O que conta como "gasto" no livro-caixa é grill Q18. O trait exige "nunca inclui
  movimento entre contas"; no livro-caixa isso pede excluir `transfers` e o pagamento de
  fatura, para não contar a compra no cartão duas vezes.
- As três perguntas que leem o inventário (`NaoClassificado`, `PossiveisDuplicidades`,
  `OQueFaltaParaConfiar`) precisam de uma versão por fonte ou de uma resposta "não se
  aplica" no livro-caixa. A proposta é: "O que ainda não foi classificado" usa as
  movimentações sem categoria ou não alocadas; duplicidades e confiabilidade continuam
  só no inventário, com o motivo. Isso cabe no ticket T11 sem mudar o trait.
- Com o livro-caixa ligado, a área pode mostrar Resultado do mês por ciclo. O teste
  `leitura_relatorios` da fatia 1, que proíbe esses termos, passa a valer só para
  `Fonte::Inventario`.

## Segurança

- **A `DATABASE_URL` só existe na VPS.** Nenhuma estação, worker local ou arquivo do
  desktop a tem (ADR-013). Teste: busca pela URL de teste em todo arquivo tocado pela
  estação.
- **Credencial de estação:** gerada no servidor, guardada no chaveiro da estação,
  conferida por hash no servidor, nunca em log, argv, SQLite ou mensagem de erro.
  Revogável por estação.
- **Filtro por dono em toda consulta do servidor**, porque o Neon não tem RLS. Um teste
  de dois donos confere que nenhuma coleção vaza linha do outro.
- **TLS** de ponta a ponta. A estação só fala com o endereço do catálogo do
  `bancada-rede`; HTTP sem TLS é recusado antes de abrir conexão.
- **Superfície exposta:** a ADR-013 aceitou o custo. Mitigações: só leitura nesta fatia,
  limite de pedidos por credencial e por IP, `401` sem detalhe para credencial
  desconhecida, pareamento só com login Google válido de usuário conhecido, e log de
  acesso sem corpo.
- **Cache em repouso:** é dado financeiro em claro no disco da estação. Vale o ADR-015:
  a cifra é do disco, e o aviso de disco sem cifra da fatia 1 passa a cobrir também a
  pasta de dados do app.
- **Nada do inventário sobe.** A sincronização não lê o `inventario.sqlite` nem o cofre
  (só escreve nele, se grill Q9 trouxer os documentos).
- **Privacidade (ADR-015):** o servidor é do usuário, não é destinatário. Nenhum dado vai
  para IA nesta fatia.

## Matriz de aceite

Cada item termina como PASS, FAIL, UNVERIFIED ou N/A, com teste ou evidência (regra do
`00-estado-real.md`).

| ID     | Critério                                                                                                                                                                                                                        |
| ------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| EST-01 | O pareamento com login Google de teste emite uma credencial; com e-mail desconhecido, recusa. A credencial fica no chaveiro e não aparece em nenhum arquivo, log ou banco tocado (busca pelo valor).                            |
| EST-02 | Revogar a estação faz o próximo pedido dela receber "estação revogada"; a estação para de sincronizar, mostra o motivo e aplica a política do grill Q11. As outras estações continuam.                                          |
| EST-03 | Nenhum arquivo da estação contém a `DATABASE_URL` de teste. O binário em modo janela não abre conexão Postgres (conferido por teste de rede ou `strace`).                                                                       |
| EST-04 | Dois donos no banco de teste: nenhum recurso devolve linha do outro dono.                                                                                                                                                       |
| EST-05 | Estação com contrato diferente recebe a resposta do grill Q4, e a barra de status diz o que fazer.                                                                                                                              |
| EST-06 | Dinheiro trafega em centavos inteiros: valores de teste com 2 casas, inclusive `0,10 + 0,20`, chegam sem erro de arredondamento.                                                                                                |
| EST-07 | A sincronização é atômica por coleção: um corte de rede no meio deixa o cache na versão anterior, e a tela não mostra mistura.                                                                                                  |
| EST-08 | Offline, todas as áreas de Dinheiro abrem com o cache e mostram a idade dos números conforme o grill Q13.                                                                                                                       |
| EST-09 | Hoje mostra os mesmos Disponível agora, Livre após compromissos, Ainda vai sair, Resultado do mês, No cartão e Dívida total que o `carregarPainelHoje` do web sobre o mesmo banco de teste, com o ciclo calculado em São Paulo. |
| EST-10 | Os testes de dupla contagem do web passam no Rust: compra no cartão × fatura, recorrente × movimentação gerada, parcela × pagamento.                                                                                            |
| EST-11 | Movimentações, Contas e cartões, Dívidas, Recorrentes e Centros batem com os `painel-*.ts` correspondentes sobre o banco de teste, nos números da tabela de fórmulas.                                                           |
| EST-12 | O ciclo do `sbf-livro` bate dia a dia, de 2024 a 2026, com o do inventário da fatia 1 e com o `getCurrentPeriod` rodando em `TZ=America/Sao_Paulo`. O caso UTC das 21h às 23h59 dá o dia de São Paulo.                          |
| EST-13 | Amortização SAC, Price e Misto e a quitação antecipada batem com as fixtures geradas pelo TS.                                                                                                                                   |
| EST-14 | Toda página da tabela "Páginas e abas que acendem" abre com dados de teste. Abas sem dado mostram o motivo. Registros · a conferir e Movimentações nunca são somados.                                                           |
| EST-15 | De qualquer conta, cartão, dívida, centro e fornecedor se chega a cada movimentação ligada. Todo total abre os itens que o somam. O Rastro de uma movimentação leva até os centros.                                             |
| EST-16 | Relatórios com "Livro-caixa" respondem as perguntas pela mesma `responder_com`; os números batem com uma consulta de referência e não levam "a conferir". Com "Inventário", nada muda da fatia 1 (INV-18 continua PASS).        |
| EST-17 | A área Sincronização mostra as estações e o histórico de trocas; Aguardando e Recusadas mostram o motivo da fatia 3; o componente lado a lado passa nos testes com dados de teste.                                              |
| EST-18 | `POST /alteracoes` responde "ainda não aceito" com o código do contrato; o formato do corpo está documentado e coberto por teste de serialização.                                                                               |
| EST-19 | O `livro-caixa.sqlite` fica na pasta de dados do app, nunca no cofre, e o `inventario.sqlite` não ganha nenhuma tabela do livro-caixa.                                                                                          |
| EST-20 | Com o cache de um dono de teste do tamanho esperado (grill Q10), a janela abre em menos de 3 s e filtra Movimentações em menos de 300 ms. A sincronização completa termina em menos de 30 s em rede doméstica.                  |
| EST-21 | O `sbf servidor` sobe na VPS como serviço, com TLS válido, e reinicia sozinho depois de queda. Evidência: o roteiro de implantação rodado pelo CEO.                                                                             |
| EST-22 | Nenhum dado real em fixture, teste ou protótipo do repositório (INV-17 continua valendo).                                                                                                                                       |

## Tickets e paralelização

Mesmo regime da spec 06: um ticket, uma branch `feat/*`, um PR; até 6 subagentes com
arquivos disjuntos; PR de segurança leva `ready-for-human`.

| Fase | Ticket                                                                                                                                               | Modelo sugerido  | Depende de           |
| ---- | ---------------------------------------------------------------------------------------------------------------------------------------------------- | ---------------- | -------------------- |
| A    | T1 `bancada-rede` a partir do `chess-net`: portão, catálogo, autorização do chaveiro, cache com `ETag`, loopback OAuth, estado da rede               | Opus (segurança) | —                    |
| A    | T2 `sbf-contrato`: tipos das coleções em centavos, `/estado`, erros, versão, corpo reservado de `/alteracoes`                                        | Opus             | grill Q4, Q5, Q6, Q7 |
| A    | T3 `sbf-livro` · regras: ciclo em São Paulo, direção do rateio, amortização, equivalente mensal; fixtures de ouro geradas pelo TS                    | Sonnet           | —                    |
| A    | T4 Banco de teste do livro-caixa: Postgres local com o `schema.sql` + migrações e um dono fictício com casos de dupla contagem, mais um segundo dono | Sonnet           | —                    |
| B    | T5 `sbf-livro` · números: Hoje, Movimentações, Contas e cartões, Dívidas, Recorrentes, Centros, com o teste de ouro contra os `painel-*.ts`          | Opus             | T3, T4               |
| B    | T6 `sbf-servidor`: HTTP, leitura do Neon com filtro por dono, coleções, conversão para centavos                                                      | Opus             | T2, T4               |
| B    | T7 Estações no servidor: tabela de estações (migração `0004` no Neon), pareamento Google, credencial, revogação, comando de administração            | Opus (segurança) | T2, T6               |
| B    | T8 `sbf-estacao`: pareamento do lado da estação, cache SQLite, sincronização, histórico de trocas                                                    | Opus             | T1, T2               |
| C    | T9 `sbf-estacao` · `Consulta` para `Fonte::LivroCaixa`: tabelas, itens, rastro e totais por ciclo sobre o cache                                      | Sonnet           | T5, T8               |
| C    | T10 Janela: áreas de Dinheiro, páginas e abas da tabela, Ribbon de leitura, barra de status da sincronização                                         | Sonnet           | T9                   |
| C    | T11 Relatórios com a fonte Livro-caixa (`FonteDeGastos` do cache e as 3 perguntas próprias)                                                          | Sonnet           | T9, grill Q18        |
| C    | T12 Área Sincronização e páginas de Estação e Alteração (lado a lado com dados de teste)                                                             | Sonnet           | T8, T10              |
| C    | T13 Documentos do servidor no cofre e na estante                                                                                                     | Sonnet           | T6, T8, grill Q9     |
| C    | T14 Transição do web para as regras portadas                                                                                                         | Sonnet           | T3, T6, grill Q12    |
| D    | T15 Implantação na VPS (serviço, TLS, roteiro para o CEO) e, se o grill Q2 mandar, a migração do Neon para `aws-sa-east-1` com o portão do ADR-007   | Sonnet + CEO     | T6, T7               |
| D    | T16 Relatório da matriz de aceite                                                                                                                    | Sonnet           | todos                |

## Perguntas para o grill

Cada pergunta tem a opção recomendada primeiro.

**Q1 — Como o `sbf servidor` roda na VPS?**

1. **(Recomendado)** Rust assíncrono (axum + tokio, Postgres por `tokio-postgres` ou
   `sqlx`) num serviço systemd, atrás de um proxy com TLS automático (Caddy). Separa o
   certificado do binário e não pesa no `sbf` da janela, que segue síncrono.
2. Rust síncrono (servidor HTTP simples em threads), TLS no próprio binário (`rustls`).
   Menos peças, mas renovar o certificado vira problema do `sbf`.
3. Rust atrás de um túnel (Cloudflare Tunnel), sem porta aberta na VPS. Some o endpoint
   exposto, mas entra um terceiro no caminho do dado financeiro.
4. Só por rede privada (Tailscale/WireGuard), sem endpoint público. Mais fechado, mas cada
   estação precisa do daemon, e o web na Vercel não alcança a API.

**Q2 — Quando o Neon migra para `aws-sa-east-1`?**

1. **(Recomendado)** Nesta fatia, antes de a API ir para produção (T15), com o portão
   humano do ADR-007. A API em São Paulo lendo um banco em `us-east-1` paga a viagem a
   cada consulta.
2. Depois desta fatia, numa tarefa própria. A fatia 2 fica com a latência entre regiões,
   aceitável para leitura de um usuário só.
3. Junto com a fatia 3, quando a escrita começa.

**Q3 — Que formato tem a credencial da estação?**

1. **(Recomendado)** Token opaco aleatório de 256 bits, mostrado uma vez, guardado no
   chaveiro da estação e só como hash no servidor; vai no cabeçalho `Authorization`.
   Revogar é apagar o hash. Simples, e o TLS cuida do resto.
2. Par de chaves Ed25519 por estação: a privada nunca sai do chaveiro, e cada pedido é
   assinado. Um vazamento do tráfego não basta, mas o protocolo fica mais complexo.
3. Certificado de cliente (mTLS). Forte, mas exige uma CA própria e complica o proxy.
4. Token de acesso curto + token de renovação longo. Útil com muitos clientes; para um
   usuário, é complexidade sem ganho claro.

**Q4 — O que o servidor faz com uma estação de contrato diferente?**

1. **(Recomendado)** Aceita a versão atual e a anterior; recusa as outras com
   "atualize o app". A estação recusa servidor mais novo que ela para escrita (fatia 3),
   mas continua lendo o cache.
2. Exige versão igual. Mais seguro, mas toda atualização do servidor deixa as estações
   paradas até serem atualizadas.
3. Versão no caminho (`/api/v1`, `/api/v2`) e o servidor mantém as duas por um tempo.

**Q5 — Como a sincronização traz o que mudou?**

1. **(Recomendado)** Coleção inteira com `ETag` nesta fatia: se a coleção mudou, vem
   inteira e substitui a do cache. Para um usuário só, o volume cabe, e não exige
   tombstone nem versão por linha no Neon agora. O delta fica para a fatia 3, quando a
   escrita traz essas colunas de qualquer jeito.
2. Delta por cursor já nesta fatia: versão por linha, `updated_at` nas 15 tabelas e
   tombstone no lugar dos 30 deletes físicos. É o que a ADR-013 pede no fim, mas mexe no
   web inteiro antes de qualquer escrita no desktop.
3. Coleção inteira para as tabelas pequenas e delta por `updated_at` só para
   movimentações, sem tombstone (apagados descobertos por uma lista de ids).

**Q6 — A fatia 2 já escreve alguma coisa?**

1. **(Recomendado)** Não. Fica só leitura, como o título; a área Sincronização nasce com
   Aguardando e Recusadas vazias e o motivo, e o contrato das alterações fica fixado e
   testado.
2. Uma escrita pequena de ponta a ponta (por exemplo, mudar a categoria de uma
   movimentação), para provar o protocolo antes da fatia 3.
3. Esconder Aguardando e Recusadas até a fatia 3.

**Q7 — Qual é a granularidade de uma alteração (contrato para a fatia 3)?**

1. **(Recomendado)** Uma Ação do usuário sobre um item, com a versão de base de cada
   linha tocada: "Rateio da movimentação X" leva a movimentação e todos os rateios dela.
   O servidor aplica inteira ou recusa, e a recusa mostra o item inteiro lado a lado,
   como no protótipo.
2. Um campo de um item (categoria, data, valor…). Recusa mais rara, mas um rateio de três
   centros vira três alterações que podem ser aceitas pela metade, o que o glossário
   proíbe.
3. Um lote de Ações que o usuário envia junto. Mais poderoso, mas uma recusa trava tudo.

**Q8 — Quem calcula os números das telas?**

1. **(Recomendado)** A estação, com o `sbf-livro`, sobre o cache. Funciona offline, e o
   mesmo código roda no servidor quando o web pedir os números.
2. O servidor manda os números prontos, e a estação guarda a última resposta. Uma fonte
   só, mas offline os números param no tempo e um filtro novo exige rede.

**Q9 — A sincronização já traz os documentos do servidor para o cofre nesta fatia?**

1. **(Recomendado)** Sim, só os originais novos, gravados no cofre e entregues à estante
   (spec 06 e ADR-014). As abas Documento passam a abrir o arquivo offline.
2. Não; só os metadados nesta fatia, e o arquivo é baixado ao abrir. O cofre fica
   completo só na fatia 3.
3. Só metadados, e abrir o documento usa uma URL assinada curta do Vercel Blob.

**Q10 — Quanto do livro-caixa o cache guarda?**

1. **(Recomendado)** Tudo do dono. O volume é de uma pessoa, e "todo total é clicável"
   exige o histórico inteiro.
2. Os últimos N ciclos (padrão 24) e as dívidas e recorrentes ativos. Menor, mas os totais
   "desde sempre" (saldo do centro) ficam errados.
3. Só o que já foi visto, sob demanda. Mínimo, mas offline quase nada funciona.

**Q11 — O que acontece com o cache quando a estação é revogada?**

1. **(Recomendado)** No primeiro contato depois da revogação, a estação apaga o
   `livro-caixa.sqlite` e a credencial, e avisa. O cofre e o inventário ficam.
2. Mantém o cache só para leitura, com a marca "estação revogada".
3. Apaga e também sobrescreve o arquivo antes, para dificultar recuperação.

**Q12 — Como o web acompanha as regras portadas (ADR-012)?**

1. **(Recomendado)** Regra a regra, no mesmo PR: quando `sbf-livro` porta o ciclo, a
   direção do rateio ou a amortização, a versão TS sai e o web passa a pedir à API.
   Começa pelo ciclo, que corrige o erro do UTC no web também.
2. Exceção temporária: o web mantém o TS durante a fatia 2, e a troca acontece num ticket
   no fim da fatia, com o teste de ouro mantendo os dois iguais até lá.
3. O web congela: nenhuma regra nova nele, e o TS só sai quando o web encolher.

**Q13 — O que o desktop mostra quando está offline?**

1. **(Recomendado)** Mostra tudo do cache, com uma faixa "Sem rede, mostrando a cópia de
   HH:MM" e, em Disponível agora, a hora da cópia ao lado do número, porque o glossário
   define esse número como "neste instante".
2. Mostra igual, só com o aviso na barra de status.
3. Esconde Disponível agora e Livre após compromissos até voltar a rede, e mostra o resto.

**Q14 — Qual é o dia de corte do ciclo e onde ele mora?**

1. **(Recomendado)** Em `user_financial_preferences`, no servidor, que o devolve em
   `/estado`; o padrão passa a ser 20, como o glossário e o inventário. O inventário da
   fatia 1 passa a ler o mesmo valor.
2. No servidor, mas com padrão 1, como o web hoje.
3. Em Opções, por estação. Duas estações poderiam ver ciclos diferentes.

**Q15 — `adjustment` (ajuste de saldo) entra no centro?**

1. **(Recomendado)** Não entra em lado nenhum, como em Movimentações: ajuste é correção de
   registro, não fluxo. O saldo do centro e o não alocado passam a ignorá-lo.
2. Continua como está: saída no centro, fora do resultado. O desktop reproduz a
   divergência de propósito.
3. O ajuste entra com o sinal do valor, nos dois lugares.

**Q16 — Como se chama, no desktop, a lista "pendências" da tela Hoje?**

1. **(Recomendado)** "Para resolver", com os mesmos itens (vencidas, sem categoria, não
   alocadas, não confirmadas, documentos a revisar). Respeita o glossário, que reserva
   Pendência para o inventário.
2. "A revisar". Curto, mas encosta em "Precisa revisar", estado do documento.
3. Sem título: cada contagem aparece como um aviso solto na Hoje.

**Q17 — Como conta e fornecedor do inventário encontram os do livro-caixa?**

1. **(Recomendado)** A página é uma só, chaveada pelo UUID do livro-caixa quando existe.
   O nome do inventário vira um apelido ligado por `supplier_aliases` e, para conta, por
   uma correspondência que o usuário confirma como decisão. Sem correspondência, a página
   continua chaveada pelo nome, como na fatia 1.
2. Duas páginas, uma por fonte, com um link "possivelmente o mesmo".
3. Casar pelo nome normalizado, sem confirmação.

**Q18 — O que é "gasto" para os Relatórios sobre o livro-caixa?**

1. **(Recomendado)** Saídas confirmadas pela data do fato: compras no cartão na data da
   compra (regra do Ciclo no glossário) e saídas de conta, **sem** pagamento de fatura e
   sem transferências. É a mesma ideia do "movimento entre contas" do inventário.
2. O "Saiu" da Hoje: dinheiro que deixou uma conta, com pagamento de fatura e sem a
   compra no cartão. Bate com a Hoje, mas joga um ciclo inteiro de compras no ciclo do
   pagamento.
3. As duas visões, com um seletor "por compra" × "por caixa".

São 18 perguntas.

## Referências

- Decisões: `docs/arquitetura/2026-11-12-adr-012-nucleo-rust.md`, `-013-servidor-autoritativo.md`,
  `-014-inventario-no-desktop.md`, `-015-privacidade.md`, `-018-shell-compartilhada.md` e
  `2026-08-30-adr-010-fila-no-banco.md`.
- Spec da fatia 1: `docs/specs/06-desktop-fatia-1-inventario.md`.
- Web: `apps/web/src/app/dashboard/(nova-ia)/`, `apps/web/src/app/actions/painel-*.ts`,
  `apps/web/src/components/contas/montar-painel.ts`, `apps/web/src/lib/auth/`,
  `apps/web/src/lib/datas.ts`.
- Banco: `packages/db/README.md`, `packages/db/src/schema.ts`, `packages/db/migrations/`.
- Domínio: `packages/domain/src/{financial-cycle,allocation,amortization}/`.
- Costuras da fatia 1: `crates/sbf-inventario/src/{referencia,consulta,comando,relatorios,movimento}.rs`.
- Protótipo: `paginas-dinheiro.js`, `paginas-fontes.js` (estação e alteração), `ganchos.js`
  e `extra.js` (área Sincronização), no protótipo navegável aprovado
  (https://claude.ai/artifact/G2oqge11uFBBdVHQnr61fE, privado, dados fictícios).
- Origem do portão de rede: `/mnt/d/Chico/chess-studio/crates/chess-net/`.

## Decisões do grill (2026-11-13)

| Q   | Decisão                                                                                                                                                                                                                                   |
| --- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Q1  | Rust assíncrono (axum + tokio) em serviço systemd, atrás do Caddy com TLS automático.                                                                                                                                                     |
| Q2  | O Neon migra para `aws-sa-east-1` **nesta fatia**, antes de a API ir para produção, com o portão humano do ADR-007.                                                                                                                       |
| Q3  | Token opaco de 256 bits no chaveiro da estação; só o hash no servidor; revogar apaga o hash.                                                                                                                                              |
| Q4  | O servidor aceita a versão de contrato atual e a anterior; recusa as outras com "atualize o app".                                                                                                                                         |
| Q5  | Coleção inteira com `ETag`; o delta por cursor fica para a fatia 3.                                                                                                                                                                       |
| Q6  | **Uma escrita de ponta a ponta já nesta fatia: mudar a categoria de uma movimentação** (desktop → alteração → servidor → Neon), com versão de base (`transactions.updated_at`), aceite e recusa lado a lado. O resto continua só leitura. |
| Q7  | A alteração é uma Ação sobre um item, com a versão de base de cada linha tocada; aplica inteira ou recusa.                                                                                                                                |
| Q8  | Os números são calculados na estação, pelo `sbf-livro`, sobre o cache.                                                                                                                                                                    |
| Q9  | A sincronização traz para o cofre os originais novos do servidor e os entrega à Estante.                                                                                                                                                  |
| Q10 | O cache guarda todo o livro-caixa do dono.                                                                                                                                                                                                |
| Q11 | Estação revogada apaga o `livro-caixa.sqlite` e a credencial no primeiro contato e avisa; cofre e inventário ficam.                                                                                                                       |
| Q12 | Regra a regra, no mesmo PR: a versão TS sai e o web passa a pedir à API, começando pelo ciclo (corrige o erro de fuso do web).                                                                                                            |
| Q13 | Offline mostra tudo do cache, com a faixa "Sem rede, mostrando a cópia de HH:MM" e a hora ao lado de Disponível agora.                                                                                                                    |
| Q14 | Dia de corte em `user_financial_preferences`, no servidor, devolvido em `/estado`; o padrão passa a ser 20, e o inventário lê o mesmo valor.                                                                                              |
| Q15 | `adjustment` não entra em lado nenhum: nem no resultado, nem no saldo do centro, nem no não alocado.                                                                                                                                      |
| Q16 | A lista da Hoje se chama **Para resolver**.                                                                                                                                                                                               |
| Q17 | Uma página só, chaveada pelo UUID do livro-caixa; o nome do inventário vira apelido; a correspondência de conta é confirmada pelo usuário, como decisão.                                                                                  |
| Q18 | Gasto é pela data da compra: compras no cartão e saídas de conta, sem pagamento de fatura nem transferências.                                                                                                                             |

Depende do CEO para ir a produção: acesso à VPS, a migração do Neon (ADR-007) e o
domínio para o Caddy. O código, os testes e um servidor local de teste não dependem.
