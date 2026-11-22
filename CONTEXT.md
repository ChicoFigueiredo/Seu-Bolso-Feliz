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

**Dinheiro de passagem**:
Dinheiro de terceiros que entra numa conta do usuário só para sair de novo, como as cotas
de um bolão repassadas à lotérica. Fica num centro próprio, com entradas e saídas, e não
é receita nem despesa. Está no disponível agora, mas o livre após compromissos desconta o
que ainda não foi repassado.
_Avoid_: receita, renda extra, reembolso

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

## Contas

**Conta do extrato**:
A conta como o banco a identifica e a conexão a traz: instituição, tipo e número
("BANCO · 000000-1"). É a identidade da conta e não muda por vontade do usuário. A
poupança vinculada é outra conta do extrato, com o mesmo número da corrente.
_Avoid_: produto, subconta

**Código da conta**:
O nome pelo qual a planilha conhece a conta: "CX-0001", "C6", "Nubank". Só passa a
valer para uma conta do extrato quando o usuário confirma o par; um código sem par não
aponta para dinheiro nenhum.
_Avoid_: apelido (para o código), número da conta

**Apelido da conta**:
O nome curto, opcional, que o usuário dá a uma conta para lembrar para que ela serve:
"Consignado", "Bolão do trabalho". Não identifica nada: duas contas podem ter o mesmo apelido.
_Avoid_: código, nome do banco

**Rótulo da conta**:
O que a tela mostra no lugar da conta do extrato: o código, seguido do apelido quando
houver ("CX-0002 · Consignado"). A poupança vinculada leva o rótulo da corrente de mesmo
número com "Poupança" na frente ("Poupança CX-0002 · Consignado"). Conta do extrato sem
par confirmado aparece como o banco a chama.
_Avoid_: nome da conta, label

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

**Modelo de compromisso**:
O molde que vários compromissos compartilham: declara os parâmetros (a diária, o dia da
semana) e o cálculo que transforma esses parâmetros no previsto de cada mês. Cada compromisso
que usa o modelo preenche os próprios valores, com vigência. Ex.: "Diarista por dia da semana".
_Avoid_: template, tipo de compromisso (tipo é natureza ou grupo)

**Cálculo** (de um compromisso):
O script que, a partir dos parâmetros do modelo e dos valores do compromisso, dá o previsto
de um mês (a diária vezes as sextas-feiras do mês). Um ajuste digitado num mês ("6 diárias")
vale só para aquele mês e fica no rastro. Diferente do padrão de reconhecimento: o cálculo
diz quanto deve ser; a regra reconhece o que aconteceu.
_Avoid_: fórmula, regra de cálculo, script de compromisso

**Recorrente**:
Algo que se repete em intervalo previsível: assinatura, conta de consumo, parcelamento,
receita fixa.
_Avoid_: template recorrente, recurring template, série

**Ocorrência**:
Um recorrente num mês específico: a assinatura de janeiro/2027. Existe porque a regra diz
que existe, sem ninguém precisar criá-la, e tem sempre a mesma identidade naquele mês.
Enquanto não é paga, é um compromisso; sozinha, nunca é uma movimentação.
_Avoid_: instância, lançamento futuro, parcela (parcela é de dívida)

**Versão**:
Os dados de um recorrente (valor, vencimento, conta, centro, prioridade) que valem a
partir de uma data. Mudar algo "daqui para a frente" cria uma versão nova; as ocorrências
anteriores continuam explicadas pela versão delas.
_Avoid_: edição, revisão

**Exceção**:
Uma decisão sobre uma única ocorrência: adiar para outra data, dispensar, ou usar outro
valor só naquele mês. Não muda a regra nem os outros meses. Dispensar não é pagar, e uma
ocorrência adiada continua devida.
_Avoid_: ajuste, override, correção pontual

**Pausa**:
Um intervalo em que o recorrente não gera ocorrências. A regra e o contrato com o
fornecedor continuam, e uma cobrança já emitida no intervalo continua devida. Retomar
encerra a pausa numa data.
_Avoid_: suspensão, cancelamento, desativar

**Cancelamento**:
O fim de um recorrente a partir de uma data: nenhuma ocorrência depois dela. Não prova
que o fornecedor aceitou cancelar nem extingue uma dívida real; isso só um documento
comprova.
_Avoid_: exclusão, pausa, encerrar

**Pago informado**:
O valor que o usuário diz ter pago numa ocorrência, sem prova bancária — a coluna "Valor
Pago" da planilha. Fica a conferir até a ocorrência ser conciliada. Vazio quer dizer "não
informado", nunca "não pago".
_Avoid_: pago declarado, quitado, baixado

**Em atraso**:
A ocorrência cujo vencimento passou sem pagamento conhecido, e que continua devida. Não
some do mês seguinte nem vira dispensada: aparece junto do mês corrente até ser paga,
renegociada ou conciliada.
_Avoid_: inadimplente, vencida e esquecida, perdida

**Conciliada** (ocorrência):
A ocorrência ligada à movimentação real que a pagou. É o único "pago" que entra como fato
nos totais; pago informado sozinho, não. Quando três provas batem exatamente — o documento
(o contracheque), o crédito no banco e a conta —, a ocorrência é conciliada sozinha e pode
ser desfeita; com qualquer diferença, a ligação é só sugerida.
_Avoid_: pago confirmado, liquidada

**Desconto em folha**:
O que o empregador retém do salário antes de depositar: consignação, previdência, plano
de saúde. O **salário líquido** é o que de fato cai na conta, já sem esses descontos;
contá-los de novo como saída da conta é dupla contagem. O contracheque do mês é a prova
de que cada desconto foi feito. Cada desconto de consignação é a parcela de um contrato de
empréstimo próprio.
_Avoid_: débito em conta, parcela do salário

**Empréstimo** (contrato):
Uma dívida com contrato: número, valor contratado, taxa, sistema de amortização, prazo e a
evolução. O consignado é pago por desconto em folha, uma parcela por contracheque; cada
contrato é um empréstimo à parte, mesmo quando vários têm o mesmo nome no contracheque.
_Avoid_: consignação (quando se fala do contrato), débito

**Evolução** (de um empréstimo):
A tabela parcela a parcela do contrato — amortização, juros e saldo devedor. O que o banco
informou vale como fato; dali para frente, é calculada pela taxa e pelo sistema.
_Avoid_: extrato do empréstimo, cronograma

**Renovação**:
Quando um contrato é quitado por um novo do mesmo credor, que leva o saldo devedor: o
antigo fica quitado por renovação e o novo nasce ligado a ele.
_Avoid_: refin, refinanciamento, novo empréstimo

**Data de disponibilidade** e **data do crédito**:
Duas datas da mesma entrada. A de disponibilidade é quando o dinheiro já pode ser usado; a
do crédito é quando o banco registra o lançamento. Com feriado, um salário pode estar
disponível num dia e ter o crédito registrado no dia útil seguinte.
_Avoid_: data do salário, dia do pagamento

**Grupo**:
Como o usuário arruma o controle mensal no primeiro nível, do jeito da planilha: Essencial,
Lar, Empréstimos, Parcial, Assinaturas. É só arrumação: não é categoria, nem centro, nem
prioridade, e trocar uma linha de grupo não muda nada no livro-caixa.
_Avoid_: categoria, seção, tipo

**Prioridade**:
A ordem em que as coisas são pagas, igual à ordem da planilha: o grupo de cima vem antes
do de baixo, e dentro do grupo a linha de cima vem antes. Mudar a prioridade é mudar a
posição, não escolher um rótulo.
_Avoid_: essencialidade, importância, urgência, nível

**Controle mensal**:
A visão de um mês, civil ou ciclo, com todos os compromissos: o que se previu, o que se
informou pago, o que está conciliado e o que falta conferir. Qualquer um dos próximos 360
meses pode ser aberto, e o futuro só mostra o que as regras projetam.
_Avoid_: planilha, orçamento, dashboard

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
A fila única de documentos recebidos e ainda não resolvidos, independente de origem. Lista os
que o reconhecimento **reconheceu** (Pronto, Precisa revisar, Possível duplicidade); é onde se
decidem as sugestões de ligação ao planejamento; a de alta confiança já vem feita (**Vínculo
automático**, com Desfazer). O que ele não reconheceu vai para A triar.
_Avoid_: inbox de ingestão, fila de jobs, ingestion queue

**Canal** (da Caixa de entrada):
Por onde o documento chegou **da primeira vez** (Telegram, Gmail, Celular, Pasta, Arrastado,
linha de comando). Um só por documento, na lista "Chegou por". Não é a **Origem**: Origem é
cada lugar onde o conteúdo foi achado, e um documento tem várias.
_Avoid_: origem (para este sentido), fonte, source

**Sem padrão de reconhecimento**:
O documento novo que nenhum padrão de reconhecimento nem leitor casou (estados Não reconhecido e Protegido por
senha). Fica na seção de mesmo nome de A triar, onde se cria o padrão a partir do cartão e se
reconhece de novo; se passar a casar, sai de A triar e entra na Caixa de entrada.
_Avoid_: não classificado, órfão, unmatched

**Estados do documento** (os únicos que o usuário vê): Pronto, Precisa revisar,
Possível duplicidade, Protegido por senha, Não reconhecido, Processando. Os estados
internos do pipeline não aparecem na interface.
_Avoid_: ingestion job, draft batch, reconciliation status, parsed, posted

**Possível duplicidade**:
O estado do documento que já chegou antes, ou por mais de um lugar com os mesmos bytes, ou com
bytes diferentes e o mesmo texto (o recibo que chega por e-mail e pelo Telegram); o mais
antigo segue como está e o mais novo é o marcado.
_Avoid_: duplicata, cópia

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

**Contracheque**:
O documento do empregador que, para um mês de pagamento, lista as rubricas e o salário
líquido. Declara um registro do tipo salário — o líquido — cujas rubricas são
sub-registros; o crédito do banco é outro registro do mesmo fato. A data de crédito impressa
nele não prova quando o dinheiro entrou: quem prova é o banco.
_Avoid_: holerite, demonstrativo, folha

**Rubrica**:
Uma linha do contracheque: um provento ou um desconto, com código, competência e, nos
descontos de contrato, a parcela. É **fixa** quando se repete todo mês, **eventual** quando
vale só naquele (PLR, prêmio, adiantamento, retroativo) e **informativa** quando aparece no
contracheque mas fica fora dos totais dele.
_Avoid_: verba, item, linha do holerite

**Conciliação**:
O ato de reconhecer que dois registros — uma transação importada e um documento, ou
duas importações da mesma coisa — descrevem o mesmo fato financeiro. Conteúdo idêntico
não é conciliação: é o mesmo documento com mais de uma origem.
_Avoid_: reconciliation, match, dedup

## Reconhecimento

**Padrão de reconhecimento**:
Instrução versionada que diz como reconhecer um tipo de documento ou registro, quais
campos tirar dele e o que propor. Muda sem recompilar o sbf e só vale depois que o
usuário ativa a versão. Uma versão nova recomeça como **ativa** (só propõe); depois de 5
propostas aceitas seguidas, sem correção, o sbf sugere torná-la **consolidada**, e só o
usuário aprova. Um padrão consolidado concilia sozinho, mas apenas com as três provas
exatas, e vincula sozinho o documento à ocorrência que ele propõe; sempre com Desfazer. Uma
rejeição, um Desfazer ou uma versão nova o devolve ao estado ativo. Nasce de três jeitos: escrito no editor, rascunho montado a partir de uma correção
na caixa de entrada, ou proposto pela IA ou pelo assistente — e nenhum desses o ativa.
_Avoid_: regra de reconhecimento (nome antigo, até 2026-11-20), plugin, template, parser,
alerta (alerta avisa; padrão de reconhecimento reconhece)

**Vínculo automático**:
O documento vai para o controle sem janela aberta e sem clique, quando a confiança é alta.
Roda assim que o reconhecimento termina, qualquer que seja a rota por onde o documento chegou
(Telegram, Gmail, pasta, celular), no worker; a janela aplica o mesmo no cartão que ainda
não foi tocado. Só estas ligações são automáticas, sempre gravadas com autor "regra" e com
Desfazer: (1) o **vínculo do contracheque**, quando o demonstrativo foi lido, a soma das
rubricas bate com o líquido, há um só recorrente de salário e a ocorrência do mês do
pagamento existe; (2) o **vínculo de um padrão consolidado** (proposta "vincular
ocorrência"); (3) a **conciliação das três provas** (do salário, ou de um padrão
consolidado). Nada mais é automático: usar as rubricas, criar compromissos de desconto em
folha, dar baixa, e a conciliação com menos de três provas continuam esperando o clique, e
um padrão só ativo só propõe. O que o usuário desfaz (documento desvinculado, par
desconciliado) a regra nunca refaz. Aplicar duas vezes não duplica nada.
_Avoid_: ligação silenciosa, auto-aprovação

**Script**:
Trecho curto em Lua anexado a um padrão de reconhecimento para calcular ou transformar
campos — conferir o IRRF de um contracheque, montar a competência de uma rubrica. Só lê o
que recebe e devolve uma proposta; nunca grava, paga, concilia nem acessa arquivo, rede ou
banco.
_Avoid_: plugin, extensão, roteiro (roteiro é o caminho de um site), macro

**Rita**:
O assistente de IA da janela, num painel ao lado do item selecionado. Lê só o que está na
tela, responde sobre isso e, por enquanto, só mexe em padrões de reconhecimento: cria o
rascunho, faz o pré-teste e explica por que um padrão venceu. Quem ativa é o usuário. Pode
rodar numa IA local ou em qualquer provedor escolhido no Chaveiro. Diferente da triagem por
IA, que roda em lote e só sugere classificação.
_Avoid_: agente, chat, copiloto, bot

**Pré-teste**:
O ensaio de um padrão de reconhecimento antes de ativá-lo: roda sobre o documento em mãos,
sobre os exemplos e sobre os documentos do cofre com o mesmo tipo de arquivo, só com o texto
já guardado. Mostra quantos casariam, com quais padrões empataria e os campos tirados. Não
grava nada.
_Avoid_: dry-run, ensaio

**Casou** (marca do documento):
Um documento casou com um padrão quando o padrão o reconheceu em algum momento, mesmo que
depois tenha sido desativado. Na lista: verde claro se algum padrão casou, âmbar se mais de
um casou e discordam (Ambíguo), sem cor se nenhum casou.
_Avoid_: match, bateu

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

**Conta de e-mail**:
Uma Conexão do tipo E-mail: uma caixa (hoje Gmail; Outlook em breve) autorizada só para
leitura. Cada conta tem id estável, apelido, o e-mail da conta (depois do login), os
**rótulos** que a pessoa segue e o próprio checkpoint por rótulo; pode haver várias. A
configuração (sem segredo) fica em `email.json`; o refresh token e as credenciais do
aplicativo ficam no Chaveiro (`email/<id>/...` e `email/<tipo>/app/...`). Autorização
vencida (`invalid_grant`) é um estado da conta, não uma falha de coleta: a coleta para até
a pessoa entrar de novo.
_Avoid_: caixa postal, inbox, carteira (para os segredos)

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
