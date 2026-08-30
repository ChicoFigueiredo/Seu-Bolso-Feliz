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

**Conciliação**:
O ato de reconhecer que dois registros — uma transação importada e um documento, ou
duas importações da mesma coisa — descrevem o mesmo fato financeiro.
_Avoid_: reconciliation, match, dedup

## Fontes

**Conexão**:
Uma fonte de dados ligada ao sistema: um banco via Open Finance, o Gmail, uma pasta
local, um worker. Cada conexão tem estado, última sincronização e histórico de falhas.
_Avoid_: integração, provider, adapter

**Worker**:
Um processo que executa trabalho pesado fora da aplicação web. É parte legítima do
produto e aparece na interface com nome, estado e fila — não é infraestrutura escondida.
_Avoid_: agente, daemon, serviço, job runner
