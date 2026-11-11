# Seu Bolso Feliz

Central pessoal de operação financeira: dinheiro, compromissos, dívidas, recorrências
e documentos de uma pessoa só. Este arquivo é **glossário**, não especificação — define
o que cada palavra significa, nunca como está implementada.

## Dinheiro

O produto separa três coisas que a linguagem cotidiana funde. Nunca use uma no lugar
da outra, nem na UI nem no código.

**Disponível agora**:
A soma dos saldos das contas bancárias, neste instante. Não desconta nada.
_Avoid_: saldo, saldo total, dinheiro

**Livre após compromissos**:
O disponível agora menos os compromissos já conhecidos e ainda não pagos. É uma
estimativa e deve ser sinalizada como tal sempre que houver compromisso sem valor
confirmado.
_Avoid_: saldo livre, sobra, disponível real

**Resultado do mês**:
Entrou menos saiu dentro do período. É uma medida de fluxo, não de posição — pode ser
positivo com a conta no vermelho.
_Avoid_: saldo do mês, balanço

**Ciclo**:
O período financeiro pessoal: começa no dia de corte escolhido pelo usuário (hoje, o
dia 20) e termina na véspera do mesmo dia no mês seguinte, inclusive. Cada dia pertence
a exatamente um ciclo. Um ciclo é chamado pelo mês em que começa: o
ciclo 2026-10 vai de 20/out a 19/nov. Um registro cai no ciclo da data em que o dinheiro
se moveu; uma compra no cartão, no ciclo da data da compra — o pagamento da fatura é
outro registro. O período de uma fatura de cartão não é um ciclo: chama-se fatura.
_Avoid_: mês (quando se quer dizer ciclo), período, competência, ciclo da fatura

## Classificação

Três eixos independentes. Uma movimentação pode ter os três ao mesmo tempo, e nenhum
substitui o outro.

**Categoria**:
A _natureza_ do gasto ou da receita — Energia, Software, Alimentação. Uma por
movimentação.
_Avoid_: tipo, classificação

**Tag**:
Rótulo livre, sem valor e sem saldo. Serve para achar coisas, não para medir.
_Avoid_: etiqueta, marcador, centro

**Centro**:
_Para quem_ ou _para qual projeto_ o dinheiro foi — Edulingo, Imob Max, Daiane. Tem
saldo, entradas e saídas, e se comporta como uma conta corrente virtual. É a única das
três que aceita rateio.
_Avoid_: projeto, tag, centro de custo, cost center

**Rateio**:
A divisão do valor de uma movimentação entre um ou mais centros. Guarda **valor**, nunca
percentual — o percentual é derivado na exibição. Uma movimentação real gera N rateios
analíticos **sem duplicar o lançamento**.
_Avoid_: split, alocação, divisão, apportionment

**Não alocado**:
A parte do valor de uma movimentação que ainda não foi rateada a nenhum centro. É um
estado legítimo e visível, não um erro: a soma dos rateios pode ser menor que a
movimentação, nunca maior.
_Avoid_: sem centro, resto, sobra

## Movimentações e compromissos

**Movimentação**:
Um fato financeiro que ocorreu ou vai ocorrer: entrou ou saiu dinheiro. É o ledger
principal; evite criar registros paralelos que representem o mesmo fato.
_Avoid_: lançamento, transação, transaction, despesa

**Compromisso**:
Uma movimentação futura já conhecida — vence em uma data, com um valor previsto ou
confirmado.
_Avoid_: conta a pagar, obrigação, pendência

**Recorrente**:
Algo que se repete em intervalo previsível: assinatura, conta de consumo, parcelamento,
receita fixa.
_Avoid_: template recorrente, recurring template, série

## Documentos

**Documento**:
Um arquivo que comprova ou origina um fato financeiro — boleto, fatura, comprovante,
extrato — venha ele da câmera, de upload, do Gmail ou da pasta local. A origem é um
atributo, não uma categoria à parte.
_Avoid_: anexo, arquivo, source document

**Caixa de entrada**:
A fila única de documentos recebidos e ainda não resolvidos, independente de origem.
_Avoid_: inbox de ingestão, fila de jobs, ingestion queue

**Estados do documento** (os únicos que o usuário vê): Pronto, Precisa revisar,
Possível duplicidade, Protegido por senha, Não reconhecido, Processando. Os estados
internos do pipeline não aparecem na interface.
_Avoid_: ingestion job, draft batch, reconciliation status, parsed, posted

**Origem**:
Cada lugar onde um mesmo conteúdo foi encontrado — um caminho no disco, um anexo do
Gmail, uma conversa do WhatsApp. O mesmo comprovante, byte a byte, salvo em três lugares
é **um** documento com três origens, nunca três documentos.
_Avoid_: cópia, localização, fonte

**Registro**:
Um fato financeiro que um documento ou uma conexão declara — uma compra numa fatura,
uma linha de extrato, o pagamento de um contracheque, uma transação do banco. Ainda não
é movimentação: só vira uma quando o usuário aprova.
_Avoid_: lançamento, transação, linha, item

**Sub-registro**:
Um detalhe que compõe um registro sem ser um fato à parte — a parcela 3/10 de uma
compra, o IOF que veio junto, cada rubrica de um contracheque.
_Avoid_: item, sub-item, linha

**Conciliação**:
O ato de reconhecer que dois registros — uma transação importada e um documento, ou
duas importações da mesma coisa — descrevem o mesmo fato financeiro. Conteúdo idêntico
não é conciliação: é o mesmo documento com mais de uma origem.
_Avoid_: reconciliation, match, dedup

## Inventário

**Inventário**:
O catálogo de tudo que foi encontrado em todas as fontes, com suas origens, feito
**antes** de qualquer análise. Nada entra na análise sem ter passado pelo inventário.
_Avoid_: índice, varredura, backup

**Achado**:
Algo encontrado numa fonte que pode ser financeiro — um arquivo, um anexo, uma mensagem.
Vira documento quando se confirma que comprova ou origina um fato financeiro; caso
contrário, fica registrado como descartado, para provar que foi visto.
_Avoid_: evidência, candidato, item

**Cofre**:
O acervo privado que guarda uma cópia de cada documento original, exatamente como foi
encontrado, fora de qualquer repositório e de qualquer nuvem não autorizada.
_Avoid_: backup, pasta de finanças, vault

## Fontes

**Conexão**:
Uma fonte de dados ligada ao sistema: um banco via Open Finance, o Gmail, uma pasta
local, um worker. Cada conexão tem estado, última sincronização e histórico de falhas.
_Avoid_: integração, provider, adapter

**Worker**:
Um processo que executa trabalho pesado fora da aplicação web. É parte legítima do
produto e aparece na interface com nome, estado e fila — não é infraestrutura escondida.
_Avoid_: agente, daemon, serviço, job runner
