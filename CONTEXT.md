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

**A conferir**:
A marca de todo número de dinheiro calculado a partir de registros que o usuário ainda
não aprovou, como os relatórios sobre o inventário: "Saiu no ciclo 2026-10 · a conferir".
Disponível agora, livre após compromissos e resultado do mês vêm só de movimentações e
nunca levam essa marca. Não é estimativa: estimativa é um valor futuro ainda não
confirmado; "a conferir" é um fato ainda não revisado.
_Avoid_: prévia, provisório, estimativa, não oficial

**Ciclo**:
O período financeiro pessoal: começa no dia de corte escolhido pelo usuário (hoje, o
dia 20) e termina na véspera do mesmo dia no mês seguinte, inclusive. Cada dia pertence
a exatamente um ciclo. Um ciclo é chamado pelo mês em que começa: o
ciclo 2026-10 vai de 20/out a 19/nov. Um registro cai no ciclo da data em que o dinheiro
se moveu; uma compra no cartão, no ciclo da data da compra — o pagamento da fatura é
outro registro. O período de uma fatura de cartão não é um ciclo: chama-se fatura.
_Avoid_: mês (quando se quer dizer ciclo), período, competência, ciclo da fatura

**Para resolver**:
A lista da tela Hoje com o que pede atenção no livro-caixa: compromissos vencidos,
movimentações sem categoria, não alocadas ou não confirmadas e documentos a revisar.
_Avoid_: pendências (reservado ao inventário), a revisar

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

**Prévia**:
O arquivo de um documento ou achado mostrado dentro do app, como ele é: as páginas, a
imagem ou o texto, pequeno num canto ou grande na página do item. Nunca muda o original.
_Avoid_: preview, thumbnail, visualizador

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
O catálogo local de tudo que foi encontrado nas fontes em massa (discos, nuvens,
WhatsApp, o histórico do e-mail), com suas origens, feito **antes** de qualquer análise.
Dessas fontes, nada entra na análise sem ter passado pelo inventário e sido promovido.
Fontes já filtradas (e-mail por consulta financeira, Open Finance, documento mandado ao
bot) entregam direto na caixa de entrada.
_Avoid_: índice, varredura, backup

**Triagem**:
A decisão de que um achado é documento, descartado ou fica em dúvida, tomada por uma
regra ou pelo usuário. A do usuário sempre prevalece: uma nova rodada de regras nunca a
desfaz.
_Avoid_: classificação, veredito, filtro

**Estados do achado** (como o usuário os vê): A triar, Documento, Descartado, Em dúvida
— sempre com quem decidiu, a regra ou o usuário.
_Avoid_: novo, dúvida

**Decisão**:
Uma escolha do usuário ou de uma regra sobre um achado ou um registro do inventário:
triagem, fornecedor, categoria, duplicidade, aprovação. Fica guardada para sempre e pode
ser desfeita por outra decisão; o estado atual é o resultado de todas elas. Não vai ao
servidor: o que vai é a promoção.
_Avoid_: veredito, ajuste, edição, alteração

**Pendência**:
Uma pergunta do sistema que trava um achado, um registro ou um lote até o usuário
responder, como "PDF protegido sem regra". Só vale nesse sentido: um compromisso ou uma
alteração nunca é chamado de pendência.
_Avoid_: dúvida, tarefa, alerta, bloqueio

**Rastro**:
A cadeia que liga um item ao que o originou e ao que ele originou. Para trás, uma
movimentação passa pelo registro, pelo documento, pela origem onde foi achado e pelas
decisões no caminho. Para a frente, uma origem leva às movimentações e aos rateios que
gerou. Todo item tem rastro, e todo total abre os itens que o somam.
_Avoid_: trilha (é o caminho de navegação na tela), histórico, log, auditoria, proveniência

**Promoção**:
A passagem de um lote de achados aprovados do inventário para a caixa de entrada do
servidor. É o único caminho de uma fonte em massa até o livro-caixa.
_Avoid_: upload, importação, envio, sincronização

**Lote**:
Um conjunto nomeado de achados e registros que o usuário aprova junto para promoção. Um
lote aprovado ainda não foi promovido; fica esperando até ser enviado.
_Avoid_: batch, draft batch, pacote, remessa

**Achado**:
Algo encontrado numa fonte que pode ser financeiro — um arquivo, um anexo, uma mensagem.
Vira documento quando se confirma que comprova ou origina um fato financeiro; caso
contrário, fica registrado como descartado, para provar que foi visto.
_Avoid_: evidência, candidato, item

**Cofre**:
O acervo privado que guarda uma cópia de cada documento original, de qualquer origem,
exatamente como foi encontrado, fora de qualquer repositório e de qualquer nuvem não
autorizada. Inclui o que chegou direto na caixa de entrada do servidor. Não guarda
segredos: esses ficam no chaveiro.
_Avoid_: backup, pasta de finanças, vault

**Estante**:
Uma cópia legível dos documentos, arrumada por ciclo e tipo no Google Drive, para
folhear de qualquer lugar. Só documentos, nunca achados descartados. Não é fonte: a
coleta do Drive ignora a estante. A cópia de segurança do cofre, cifrada, é outra coisa.
_Avoid_: espelho, biblioteca, backup, vista

**Chaveiro**:
O lugar protegido onde ficam os segredos: senhas de PDF e de sites, CPF, tokens de
acesso às conexões e chaves de API. Nunca fica junto dos documentos, nunca aparece em
texto claro e nunca é mostrado a uma IA.
_Avoid_: cofre, cofre de senhas, carteira, vault, keyring

## Fontes

**Conexão**:
Uma fonte de dados ligada ao sistema: um banco via Open Finance, uma caixa de e-mail,
uma pasta local, o WhatsApp, o Telegram, um site visitado pelo navegador (Receita,
internet banking), um worker. Cada conexão tem estado, agenda, última coleta e histórico
de falhas. Um lugar varrido uma vez só, como um disco do inventário, também é uma
conexão, só que sem agenda.
_Avoid_: integração, provider, adapter, serviço, vigia, fonte (como nome de um item;
"fonte em massa" continua valendo como descrição)

**Roteiro**:
O caminho gravado que uma conexão de site percorre para consultar e baixar documentos.
Nasce da primeira coleta, guiada por IA, e se repete sem IA até o site mudar. Um roteiro
só consulta e baixa: nunca paga, transfere, contrata ou altera cadastro.
_Avoid_: script, macro, robô, automação

**Coleta**:
Uma passagem de uma conexão trazendo o que há de novo na fonte. É sempre de fora para
dentro; a troca entre o desktop e o servidor não é coleta.
_Avoid_: sincronização, sync, importação

**Worker**:
Um processo que executa trabalho pesado fora da janela do app, numa estação ou no
servidor. É parte legítima do produto e aparece na interface com nome, estado e fila —
não é infraestrutura escondida.
_Avoid_: agente, daemon, serviço, job runner

## Avisos

**Alerta**:
Uma regra que observa documentos, registros, compromissos ou conexões e, quando a
condição acontece, avisa o usuário por um ou mais canais.
_Avoid_: notificação, gatilho, serviço, monitor

**Canal**:
Um destino por onde o sistema avisa o usuário: um bot do Telegram, a notificação do
sistema operacional. Um canal só leva avisos; não traz dados, mesmo quando o aplicativo
do outro lado também é uma conexão.
_Avoid_: notificador, serviço, destino

## Análise

**Ponto de atenção**:
Algo que a análise aponta como digno de ação, em qualquer área: dinheiro, operação do
sistema, custo de IA e de serviços, tempo do usuário. Traz a evidência e o impacto, com a
estimativa marcada como estimativa, e fica aberto, resolvido ou ignorado.
_Avoid_: achado, insight, diagnóstico, sinal, alerta

## Privacidade

**Destinatário**:
Qualquer lugar fora das máquinas do usuário para onde dados saem: um modelo de IA, um
cliente MCP, um canal.
_Avoid_: provedor, consumidor, destino

**Nível de confiança**:
Quanto de dado identificável um destinatário pode receber: mascarado, parcial ou
completo. Cada destinatário tem o seu, e um destinatário novo nasce mascarado.
_Avoid_: permissão, privacidade, escopo

## Desktop e servidor

**Estação**:
Uma instalação do app desktop numa máquina do usuário, com janela e worker local, com
identidade própria perante o servidor e uma credencial que pode ser revogada.
_Avoid_: dispositivo, máquina, cliente, instância

**Sincronização**:
A troca entre o app desktop e o servidor: o servidor manda o que mudou, o desktop manda
o que o usuário alterou. Quem decide o livro-caixa é sempre o servidor; a cópia local é
cache. O inventário não sincroniza. O cofre só recebe: a sincronização traz para ele uma
cópia de cada documento novo do servidor, e nada sai do cofre por ela, só por promoção.
_Avoid_: sync, replicação, coleta

**Alteração**:
Uma mudança que o usuário fez no desktop e que o servidor ainda não aceitou. O servidor
a aplica inteira ou a recusa, nunca pela metade.
_Avoid_: intenção, pendência, edição local, rascunho

**Alteração recusada**:
Uma alteração que o servidor não aplicou porque deixou de valer: o fato mudou depois
que ela foi feita, ou uma regra a reprovou. Fica visível até o usuário resolver.
_Avoid_: conflito, erro de sincronização
