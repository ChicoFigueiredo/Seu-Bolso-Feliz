# Seu Bolso Feliz — Prompt Operacional V6: Auditoria Financeira Local + Histórico + Subagentes + PDFs Protegidos

## Missão de hoje

Hoje **não é dia de fechar a arquitetura definitiva em nuvem nem de concluir o redesign**.

O objetivo é usar o **Seu Bolso Feliz rodando localmente** como uma bancada real de auditoria financeira, preenchida com meus próprios dados, para descobrir:

- para onde meu dinheiro está indo;
- quais gastos são recorrentes;
- quais assinaturas e serviços podem ser cortados;
- quais despesas estão duplicadas, faltando ou mal classificadas;
- quais padrões mensais existem;
- o que ainda falta no produto para representar corretamente minha vida financeira.

A meta é sair do dia com **dados reais organizados, rastreáveis e suficientemente confiáveis para tomada de decisão**.

---

## Prioridade temporal

Trabalhar do mais recente para o mais antigo.

1. Prioridade máxima: meses mais recentes.
2. Depois ampliar progressivamente o histórico.
3. Meta final: aproximadamente **12 meses**, sem sacrificar precisão para ganhar cobertura histórica.

---

## 1. Subir o Seu Bolso Feliz localmente

Antes de qualquer evolução estrutural:

- subir a aplicação local;
- subir banco e serviços necessários;
- subir o worker local;
- garantir armazenamento de documentos;
- validar autenticação local;
- corrigir apenas bloqueios reais;
- evitar refatorações cosméticas ou arquiteturais que não contribuam para a auditoria de hoje.

O sistema local será a **fonte operacional da auditoria de hoje**.

---

## 2. Integrar e validar a Pluggy

Usar a integração existente ou completar o mínimo necessário para importar dados reais da Pluggy.

Coletar e normalizar, quando disponíveis:

- contas;
- saldos;
- cartões;
- transações;
- faturas;
- parcelas;
- pagamentos;
- transferências.

O objetivo não é provar que a API responde.  
O objetivo é **colocar meus dados financeiros reais dentro do Seu Bolso Feliz**.

Registrar sempre a origem de cada dado importado.

---

## 3. Varrer meu HD com agentes

Usar agentes/subagentes para fazer trabalho braçal de busca e classificação em todo o meu HD.

Procurar especialmente:

- extratos bancários;
- faturas de cartão;
- comprovantes de pagamento;
- comprovantes de transferência/Pix;
- boletos;
- recibos;
- notas fiscais;
- PDFs financeiros;
- planilhas;
- documentos com valores, vencimentos ou pagamentos;
- arquivos relacionados a assinaturas, financiamentos ou serviços recorrentes.

### Regra de segurança

Não apagar, mover ou sobrescrever arquivos originais.

A operação deve ser inicialmente de:

**descoberta → leitura → indexação → extração → associação**

Guardar caminho/origem e hash quando útil para evitar reprocessamento e duplicidade.

---

## 3A. Criar um cofre financeiro local antes de preencher a aplicação

Antes de transformar documentos e mensagens em lançamentos definitivos, criar dentro do ambiente local do projeto um **acervo financeiro privado**, fora de qualquer commit Git.

Esse acervo será a camada de evidências da auditoria.

### Estrutura sugerida

Criar uma pasta local, por exemplo:

`private-data/finance-vault/`

Ela deve estar obrigatoriamente no `.gitignore` e nunca poderá ser enviada ao GitHub.

Organizar o acervo por ciclos financeiros mensais, com o corte do usuário configurável a partir do **dia 20**. Não codificar a regra de forma rígida: o sistema deve permitir definir claramente o início e o fim de cada ciclo para evitar ambiguidade entre calendários como 20→19 ou 20→20.

Exemplo lógico:

- `2026-11/`
  - `gmail/`
  - `pluggy/`
  - `hd/`
  - `onedrive/`
  - `comprovantes/`
  - `faturas/`
  - `extratos/`
  - `boletos/`
  - `notas-fiscais/`
  - `outros/`

Preservar sempre uma cópia bruta da evidência original.

### SQLite local de evidências

Criar um banco SQLite próprio para indexar o acervo antes da ingestão definitiva no Seu Bolso Feliz.

Esse banco deve registrar, no mínimo:

- identificador único;
- origem (`gmail`, `hd`, `onedrive`, `pluggy` etc.);
- caminho ou identificador da origem;
- nome original;
- tipo do arquivo/documento;
- hash do conteúdo;
- tamanho;
- data de descoberta;
- data financeira provável;
- ciclo financeiro;
- remetente/fornecedor quando conhecido;
- valor quando identificado;
- conta/cartão provável;
- texto extraído;
- classificação provável;
- confiança;
- status de processamento;
- relação com outros documentos;
- relação com eventual lançamento do Seu Bolso Feliz.

O objetivo desse SQLite é permitir reprocessamento, deduplicação, auditoria e exploração com scripts próprios sem depender do banco principal da aplicação.

---

## 3B. Preservar e deduplicar as evidências

Nunca modificar o original encontrado no HD, Gmail ou OneDrive.

Quando uma evidência for copiada para o cofre local:

1. calcular hash;
2. verificar se já existe;
3. registrar a origem;
4. preservar metadados;
5. classificar;
6. associar ao ciclo financeiro correto.

Se o mesmo comprovante aparecer no Gmail, no HD e no OneDrive, registrar as três origens, mas manter uma única evidência canônica quando os hashes confirmarem conteúdo idêntico.

---

## 4A. OneDrive como fonte e como cópia organizada

Incluir o OneDrive na auditoria.

Usar dois fluxos distintos:

### Leitura

Varrer as áreas autorizadas do OneDrive procurando:

- comprovantes;
- extratos;
- faturas;
- recibos;
- boletos;
- notas fiscais;
- documentos financeiros relacionados.

Registrar tudo no índice SQLite local.

### Cópia organizada

Quando seguro e autorizado, criar no OneDrive uma estrutura organizada de backup dos documentos financeiros relevantes.

Não apagar, renomear ou reorganizar silenciosamente os arquivos originais.

Preferir inicialmente:

**origem → cópia organizada → índice SQLite → revisão**

A cópia organizada do OneDrive deve poder ser reconstruída a partir do índice local e não deve ser tratada como única fonte da verdade.

---

## 4B. Scripts próprios de coleta

Criar scripts reexecutáveis e idempotentes para:

- Gmail;
- HD local;
- OneDrive;
- Pluggy.

Cada coletor deve:

- aceitar período/ciclo como parâmetro;
- priorizar os ciclos mais recentes;
- registrar progresso;
- preservar origem;
- calcular hash;
- evitar duplicidade;
- permitir execução incremental;
- produzir logs;
- nunca depender de intervenção manual para saber o que já foi processado.

O acervo local e o SQLite devem permitir interromper e retomar a auditoria sem começar do zero.

---

## 4. Varrer meus e-mails

Pesquisar minha caixa de e-mail por evidências financeiras, incluindo:

- extratos;
- faturas;
- comprovantes;
- cobranças;
- boletos;
- notas fiscais;
- recibos;
- renovações;
- assinaturas;
- pagamentos confirmados;
- serviços recorrentes;
- compras;
- financiamentos;
- avisos de débito.

Sempre preservar:

- remetente;
- assunto;
- data;
- identificador da mensagem;
- anexos relevantes;
- relação entre e-mail e lançamento financeiro.

---

## 5. Criar uma esteira de ingestão confiável

Tudo que for descoberto deve entrar em uma esteira rastreável.

Sugestão de estados:

**Descoberto → Extraído → Interpretado → Candidato a lançamento → Conciliado → Confirmado**

Nada encontrado por IA deve virar automaticamente uma verdade financeira sem rastreabilidade.

Cada registro deve conservar, sempre que possível:

- origem;
- data de captura;
- valor;
- data financeira;
- descrição original;
- fornecedor provável;
- categoria provável;
- confiança da interpretação;
- vínculo com documento/e-mail/transação de origem;
- histórico de correções.

---

## 6. Conciliação e prevenção de duplicidade

O sistema precisa tentar perceber quando diferentes evidências representam o mesmo evento.

Exemplo:

- transação da Pluggy;
- item da fatura;
- comprovante em PDF;
- e-mail de confirmação;

podem representar **uma única despesa**.

Não criar quatro lançamentos se existe apenas um evento financeiro.

Criar uma lógica de correspondência baseada em combinação de:

- valor;
- data;
- fornecedor;
- cartão/conta;
- identificadores;
- descrição;
- proximidade temporal;
- evidências documentais.

Itens de baixa confiança devem ficar para revisão.

---

## 7. Detectar recorrências

Uma prioridade de hoje é identificar gastos frequentes e assinaturas.

Exemplos do tipo:

- Vitor CAVE todo mês;
- Adobe;
- streaming;
- ferramentas de IA;
- SaaS;
- academia;
- telefonia;
- seguros;
- tarifas;
- mensalidades;
- serviços cobrados repetidamente.

O sistema deve permitir que eu diga:

> “Esse lançamento é do Vitor CAVE.”

ou

> “Esses R$ 6,99 mensais pertencem a esta assinatura.”

A minha correção deve ser persistida e reaproveitada para classificar ocorrências semelhantes.

---

## 8. Categorizar para responder onde o dinheiro está indo

A classificação precisa permitir análise prática, não apenas armazenamento.

Exemplos de categorias:

- moradia;
- alimentação;
- transporte;
- saúde;
- educação;
- lazer;
- tecnologia;
- assinaturas;
- serviços profissionais;
- dívidas;
- tarifas;
- transferências;
- compras;
- investimentos;
- outros.

Permitir subcategorias e fornecedor normalizado quando necessário.

---

## 9. Usar meus dados reais para testar o produto

Durante a auditoria, tratar qualquer limitação funcional encontrada como possível requisito.

Se eu disser:

- “isso está errado”;
- “isso não deveria estar duplicado”;
- “isso deveria ser uma recorrência”;
- “não consigo representar esse tipo de gasto”;
- “preciso enxergar isso de outra forma”;

registrar o problema e propor a menor evolução possível no Seu Bolso Feliz para resolvê-lo.

O objetivo é fazer **teste de produto com a minha própria vida financeira**.

---

## 10. Interface necessária hoje

Não investir tempo no redesign final.

Hoje a interface precisa ser funcional para:

- listar registros;
- filtrar por período;
- pesquisar;
- ver origem;
- abrir documento associado;
- corrigir fornecedor;
- corrigir categoria;
- confirmar recorrência;
- marcar duplicidade;
- unir registros conciliados;
- aprovar ou rejeitar interpretação;
- sinalizar pendências.

Qualquer melhoria visual deve servir diretamente à auditoria.

---

## 11. Relatórios que precisam começar a funcionar

Ao final do processamento, o sistema deve conseguir responder, com dados reais:

- Quanto gastei neste mês?
- Quanto gastei nos últimos meses?
- Quais são meus maiores fornecedores?
- Quais gastos se repetem todo mês?
- Quais assinaturas possuo?
- Quais gastos aumentaram?
- Quais despesas parecem desnecessárias ou cortáveis?
- Onde meu dinheiro está concentrado?
- Quais despesas ainda não foram classificadas?
- Quais documentos/transações parecem duplicados?
- O que está faltando para a análise ficar confiável?

---

## 12. Relatório obrigatório de encerramento do trabalho de hoje

Gerar um relatório final objetivo contendo:

- contas identificadas;
- cartões identificados;
- período coberto pela Pluggy;
- quantidade de transações importadas;
- quantidade de arquivos financeiros encontrados;
- quantidade de e-mails relevantes encontrados;
- documentos processados;
- lançamentos criados;
- lançamentos conciliados;
- duplicidades detectadas;
- recorrências detectadas;
- fornecedores normalizados;
- registros que exigem minha revisão;
- falhas de integração;
- lacunas de dados;
- funcionalidades do Seu Bolso Feliz que se mostraram insuficientes;
- recomendações objetivas de próximos passos.

---

# Critério de sucesso de hoje

O trabalho de hoje será considerado bem-sucedido se eu conseguir abrir o **Seu Bolso Feliz localmente** e começar a enxergar, com base em dados reais:

- para onde está indo meu dinheiro;
- quais gastos são recorrentes;
- quais custos posso cortar;
- quais registros estão incompletos ou errados;
- quais despesas precisam de revisão;
- quais lacunas ainda existem no produto.

Além disso, a auditoria precisa deixar um **backup financeiro local organizado e reprocessável**, contendo:

- cofre privado fora do Git/GitHub;
- evidências preservadas por ciclo financeiro;
- índice SQLite local;
- hashes para deduplicação;
- origem de cada evidência;
- scripts reexecutáveis para Gmail, HD, OneDrive e Pluggy;
- cópia organizada no OneDrive dos comprovantes/documentos relevantes, quando autorizado;
- capacidade de retomar a varredura sem reprocessar tudo;
- separação clara entre **evidência encontrada** e **lançamento aceito no Seu Bolso Feliz**.

O resultado esperado é:

**Seu Bolso Feliz local funcionando + cofre financeiro local privado + SQLite de evidências + Pluggy com dados reais + varredura de HD + varredura de e-mails + varredura do OneDrive + documentos preservados e deduplicados + primeiros registros conciliados + recorrências identificadas + relatório financeiro inicial + backlog das lacunas encontradas.**

---

## Prazo operacional imediato

A prioridade absoluta é executar esta operação **hoje**, com prazo máximo até **amanhã às 06:00**. Até esse horário, o time deve concentrar esforço em coletar, organizar, deduplicar, conferir e sinalizar lacunas nos dados mais recentes, avançando para períodos anteriores conforme a cobertura ficar confiável. Amanhã, a partir das 06:00, farei uma revisão do que foi encontrado, dos buracos restantes, das falhas e das pendências, e então redefinirei pessoalmente as próximas prioridades de trabalho.

---

# Fora do escopo de hoje

Não priorizar hoje:

- migração definitiva para Vercel;
- banco definitivo em nuvem;
- worker permanente da VPS de São Paulo;
- redesign visual completo;
- otimizações prematuras;
- arquitetura definitiva de produção.

Esses itens continuam sendo o destino do projeto, mas só devem avançar depois que a ferramenta provar, **com meus próprios dados**, que consegue representar e analisar minha realidade financeira corretamente.

---

## Regra final para o time

**Prefiram precisão, rastreabilidade e utilidade financeira real a volume, velocidade ou aparência.**

Hoje o Seu Bolso Feliz deve funcionar como uma equipe de auditoria financeira assistida por IA, usando minha própria vida financeira como caso de teste.

---

# Apêndice V4 — Coleta histórica de contracheques

## Objetivo

Adicionar à operação local a capacidade agêntica de coletar e estruturar meus contracheques históricos da CAIXA, começando pelo documento mais recente e retrocedendo mês a mês até o registro mais antigo disponível desde a minha entrada na empresa.

Essa coleta deve servir a dois objetivos:

1. preservar meu histórico profissional e financeiro em um acervo local confiável;
2. transformar contracheques em dados estruturados para análise histórica de renda, descontos, benefícios, empréstimos, encargos, variações salariais e capacidade de poupança.

A finalidade analítica é permitir responder, sem eufemismos, perguntas como:

- Quanto eu já recebi ao longo dos anos?
- Quanto foi para impostos, previdência, empréstimos, benefícios e outros descontos?
- Quanto da minha renda líquida foi consumida ao longo do tempo?
- Em quais períodos minha renda aumentou ou caiu?
- Quais descontos se repetiram por anos?
- Quanto eu poderia ter acumulado sob diferentes hipóteses de poupança/investimento?
- Quanto dinheiro eu provavelmente desperdicei em gastos que poderiam ter sido evitados, sempre deixando claro quando isso for uma estimativa e não um fato contábil?

---

## 1. Navegação agêntica supervisionada

Eu próprio farei a autenticação no sistema de RH/contracheques da empresa no navegador.

Depois de autenticado, o agente poderá operar a interface sob minha supervisão para:

- localizar a área de contracheques;
- identificar o período mais recente disponível;
- baixar o contracheque correspondente;
- retroceder um mês;
- repetir o processo;
- continuar do mais recente para o mais antigo enquanto houver registros disponíveis.

### Regras de segurança

O agente não deve:

- tentar contornar autenticação;
- armazenar minha senha;
- tentar quebrar MFA, CAPTCHA ou controles de acesso;
- acessar informações de outras pessoas;
- alterar dados no sistema corporativo;
- executar operações além das necessárias para consultar e baixar meus próprios documentos.

Quando houver autenticação, MFA ou confirmação sensível, a ação deverá ser realizada por mim.

---

## 2. Estratégia de coleta

A coleta deve ser incremental, retomável e auditável.

Ordem obrigatória:

**mais recente → mês anterior → mês anterior → ... → registro mais antigo disponível**

Para cada período:

1. identificar competência;
2. baixar o documento original;
3. registrar origem;
4. calcular hash;
5. evitar duplicidade;
6. armazenar no cofre local;
7. extrair os dados;
8. validar a competência;
9. marcar o item como processado;
10. continuar para o mês anterior.

Se o processo for interrompido, deverá continuar do último mês confirmado sem reiniciar toda a coleta.

---

## 3. Organização no cofre local

Criar uma área específica dentro do cofre financeiro privado, por exemplo:

`private-data/finance-vault/contracheques/`

Organização recomendada:

- `AAAA/`
  - `AAAA-MM/`
    - `original/`
    - `extraido/`
    - `metadata.json`

Exemplo:

`private-data/finance-vault/contracheques/2026/2026-10/`

O arquivo original deve ser preservado sem alteração.

Nenhum contracheque, dado extraído ou metadado pessoal poderá ser submetido ao GitHub.

---

## 4. Estruturação em banco local

Adicionar ao SQLite local tabelas próprias para contracheques.

Registrar, quando disponível:

- competência;
- data de pagamento;
- matrícula/identificador mascarado quando necessário;
- salário-base;
- remuneração bruta;
- remuneração líquida;
- proventos;
- descontos;
- imposto de renda;
- previdência;
- FGTS quando informado;
- empréstimos;
- consignados;
- previdência complementar;
- plano de saúde;
- benefícios;
- adicionais;
- gratificações;
- férias;
- 13º salário;
- PLR/participações;
- abonos;
- horas extras;
- outras rubricas;
- código e descrição de cada rubrica;
- valor de cada rubrica;
- total bruto;
- total de descontos;
- líquido;
- origem do documento;
- hash;
- status de validação.

Não limitar o modelo às rubricas conhecidas. Preservar também a descrição original para permitir evolução futura.

---

## 5. Normalização de rubricas

Criar uma camada de normalização capaz de perceber que descrições diferentes ao longo dos anos podem representar a mesma natureza financeira.

Exemplos:

- salário;
- gratificação;
- função;
- previdência;
- IRRF;
- consignado;
- plano de saúde;
- previdência complementar;
- benefício;
- adicional;
- PLR.

Guardar sempre:

**rubrica original + rubrica normalizada + confiança da classificação**

Qualquer classificação duvidosa deve poder ser corrigida por mim e reaproveitada nos demais períodos.

---

## 6. Validação dos dados extraídos

Antes de considerar um contracheque estruturado como válido:

- conferir se a soma dos proventos coincide com o total bruto;
- conferir se a soma dos descontos coincide com o total de descontos;
- conferir se bruto menos descontos é compatível com o líquido;
- sinalizar divergências;
- permitir revisão manual;
- manter vínculo permanente com o documento original.

Nunca descartar o PDF/original após a extração.

---

## 7. Análises históricas desejadas

Quando houver massa suficiente de dados, gerar análises como:

- evolução do salário bruto;
- evolução do salário líquido;
- evolução dos descontos;
- evolução da carga tributária;
- evolução de consignados e empréstimos;
- participação de benefícios;
- mudanças de função/remuneração;
- períodos de maior renda;
- períodos de maior comprometimento da renda;
- média mensal por ano;
- acumulado recebido;
- acumulado descontado;
- acumulado líquido;
- variação real quando houver índice de inflação disponível posteriormente;
- comparação entre renda e despesas já registradas no Seu Bolso Feliz.

---

## 8. Métrica de “quanto eu desperdicei”

Criar essa análise com rigor.

O sistema **não deve inventar um número de desperdício**.

Separar claramente:

### Fatos observados

- renda recebida;
- descontos;
- despesas identificadas;
- gastos recorrentes;
- dívidas;
- juros;
- tarifas;
- assinaturas;
- compras.

### Cenários estimados

Exemplos:

- quanto teria acumulado se poupasse 5%, 10% ou 20% da renda líquida;
- quanto teria acumulado se determinados gastos recorrentes não existissem;
- quanto paguei em juros/tarifas evitáveis;
- quanto determinados hábitos consumiram ao longo do tempo.

Toda estimativa deve apresentar hipótese e metodologia.

---

## 9. Integração com o Seu Bolso Feliz

Os contracheques não devem ser apenas arquivos armazenados.

Eles devem alimentar uma visão histórica de renda dentro do Seu Bolso Feliz, permitindo cruzar:

**renda → descontos → compromissos → despesas → saldo disponível → poupança/dívida**

A renda identificada no contracheque deve poder ser conciliada com o crédito correspondente encontrado na conta bancária/Pluggy, evitando dupla contagem.

---

## 10. Definição de pronto deste apêndice

Este apêndice estará funcional quando:

- eu autenticar manualmente no sistema de RH;
- o agente conseguir navegar pelos meus próprios contracheques;
- baixar os documentos do mais recente para o mais antigo;
- retomar uma coleta interrompida;
- preservar os originais;
- organizar tudo no cofre financeiro local;
- registrar hashes e metadados;
- extrair rubricas para o SQLite;
- validar totais;
- mostrar períodos com erro ou falta de documento;
- produzir uma primeira série histórica de renda;
- conciliar, quando possível, salário líquido com créditos bancários;
- manter tudo fora do Git/GitHub.

O objetivo de longo prazo é chegar ao histórico completo disponível desde minha entrada na CAIXA, sem colocar a segurança da conta corporativa em risco.

---

# Apêndice V5 — Estratégia de subagentes, modelos fortes e conferência de qualidade

## Premissa operacional

Usar o **máximo razoável de subagentes em paralelo** sempre que isso reduzir tempo, aumentar cobertura ou melhorar a qualidade da auditoria.

Não concentrar toda a operação em um único agente quando a tarefa puder ser dividida com segurança.

Exemplos de frentes independentes:

- agente de Gmail;
- agente de HD local;
- agente de OneDrive;
- agente de Pluggy;
- agente de contracheques;
- agente de extração de PDFs;
- agente de conciliação;
- agente de recorrências;
- agente de normalização de fornecedores;
- agente de validação contábil;
- agente de deduplicação;
- agente de auditoria final do backup.

---

## 1. Seleção de modelo por tarefa

Para cada subagente, escolher um modelo adequado à natureza da tarefa.

Dar preferência a modelos com maior capacidade de raciocínio para:

- conciliação difícil;
- identificação de duplicidade;
- leitura de documentos complexos;
- interpretação de rubricas;
- classificação ambígua;
- detecção de inconsistências;
- auditoria de resultados;
- revisão de decisões de outros agentes.

Modelos mais leves podem ser usados em tarefas repetitivas e determinísticas, como:

- renomear;
- catalogar;
- calcular hash;
- mover cópias para estrutura organizada;
- executar scripts;
- gerar índices;
- coletar metadados;
- pré-classificar itens simples.

A regra é:

**modelo forte para decisão; modelo barato/rápido para execução mecânica.**

---

## 2. Trabalho paralelo com coordenação central

Criar uma camada supervisora para:

- distribuir trabalho;
- evitar que dois agentes processem o mesmo lote;
- registrar quem processou cada item;
- acompanhar falhas;
- redistribuir tarefas;
- impedir conflito de escrita;
- consolidar resultados;
- encaminhar itens duvidosos para revisão.

Todo lote deve possuir:

- origem;
- responsável;
- status;
- horário de início;
- horário de conclusão;
- contagem de itens;
- falhas;
- itens pendentes;
- itens que exigem revisão humana.

---

## 3. Conferência mínima obrigatória

Nenhum lote deve ser considerado concluído apenas porque um script ou agente terminou sem erro.

Exigir uma conferência mínima independente.

### Para documentos

Verificar, por amostragem e/ou segundo agente:

- competência correta;
- valor;
- fornecedor/remetente;
- tipo do documento;
- ciclo financeiro;
- existência de duplicidade;
- vínculo com a origem.

### Para transações

Verificar:

- valor;
- data;
- conta/cartão;
- fornecedor normalizado;
- categoria;
- recorrência;
- vínculo com documento/e-mail quando disponível.

### Para contracheques

Verificar:

- competência;
- bruto;
- descontos;
- líquido;
- soma de rubricas;
- coerência matemática;
- arquivo original correspondente.

---

## 4. Dupla checagem por agentes

Quando a informação for materialmente importante, ambígua ou de baixa confiança:

1. primeiro agente extrai/classifica;
2. segundo agente revisa sem assumir que o primeiro está correto;
3. divergências são registradas;
4. caso necessário, um terceiro agente ou eu faço a decisão final.

Priorizar dupla checagem em:

- valores altos;
- lançamentos recorrentes;
- empréstimos;
- dívidas;
- rendimentos;
- impostos;
- contracheques;
- faturas;
- possíveis duplicidades;
- dados usados em relatórios financeiros.

---

## 5. Auditoria do backup local

Criar um agente especificamente responsável por conferir a qualidade do cofre local.

Esse agente não deve apenas conferir se os arquivos existem.

Ele deve verificar:

- se todos os arquivos possuem hash;
- se cada arquivo possui origem;
- se o ciclo financeiro está preenchido;
- se o SQLite aponta para um arquivo existente;
- se arquivos duplicados foram identificados;
- se nenhuma evidência ficou órfã;
- se documentos processados possuem status coerente;
- se existem lacunas temporais;
- se há meses com volume anormalmente baixo;
- se existem meses sem extratos, faturas ou contracheques;
- se scripts podem ser reexecutados sem gerar duplicação.

---

## 6. Relatório de cobertura

Gerar uma matriz de cobertura por mês/ciclo.

Para cada período, mostrar:

- Gmail: completo / parcial / ausente;
- HD: completo / parcial / ausente;
- OneDrive: completo / parcial / ausente;
- Pluggy: completo / parcial / ausente;
- extratos: quantidade;
- faturas: quantidade;
- comprovantes: quantidade;
- contracheques: quantidade;
- lançamentos conciliados;
- pendências;
- confiança geral do período.

O objetivo é tornar visível onde o backup está forte e onde ainda existem buracos.

---

## 7. Critério de qualidade antes de alimentar o Seu Bolso Feliz

Antes de considerar um lote apto a alimentar a aplicação principal:

- origem preservada;
- hash gerado;
- indexação no SQLite;
- classificação mínima;
- deduplicação executada;
- conferência independente concluída;
- inconsistências registradas;
- confiança acima do limiar definido;
- itens duvidosos separados para revisão.

A prioridade é **qualidade e rastreabilidade**, não simplesmente volume processado.

---

## 8. Definição de pronto deste apêndice

A operação de agentes estará adequada quando:

- houver paralelização efetiva das fontes;
- tarefas de alto raciocínio forem entregues a modelos fortes;
- tarefas mecânicas forem automatizadas;
- todo lote tiver responsável e status;
- houver pelo menos uma conferência independente;
- itens críticos tiverem dupla checagem;
- o cofre local tiver auditoria própria;
- existir relatório de cobertura por período;
- for possível identificar rapidamente qualquer lacuna ou inconsistência;
- nenhum dado importante seja considerado definitivo sem validação mínima.

A regra final é:

**não basta coletar tudo; precisamos ter confiança de que o backup está completo, organizado, rastreável e consistente.**

---

# Apêndice V6 — PDFs protegidos por senha e regras por fornecedor

## Objetivo

Adicionar ao pipeline local a capacidade de abrir, indexar e extrair dados de PDFs protegidos por senha quando eu tiver autorização para acessá-los.

Na prática, muitos documentos financeiros usam como senha:

- CPF completo;
- CPF somente números;
- partes do CPF;
- combinações derivadas do CPF;
- regras específicas de determinado fornecedor/emissor.

O sistema deve permitir associar **fornecedor/tipo de documento → estratégia de senha**, sem gravar minha senha real nem meu CPF em arquivos versionados.

---

## 1. Entrada segura do CPF

O CPF deverá ser fornecido **somente em execução local**, por entrada protegida.

Preferência:

- prompt interativo ocultando os caracteres; ou
- cofre de credenciais do sistema operacional.

Evitar:

- argumento de linha de comando que fique no histórico do shell;
- arquivo Markdown;
- Git;
- `.env` versionado;
- logs;
- SQLite em texto puro.

O sistema pode manter o CPF somente em memória durante a execução ou em um armazenamento local criptografado protegido pelo sistema operacional.

---

## 2. Estratégias de senha derivadas do CPF

A partir do CPF fornecido em memória, o coletor pode gerar candidatos autorizados conforme regras configuradas.

Exemplos de estratégias, sem gravar os valores resultantes:

- CPF completo sem pontuação;
- CPF com pontuação;
- primeiros N dígitos;
- últimos N dígitos;
- combinações explicitamente cadastradas para determinado fornecedor.

Não fazer tentativa cega ilimitada.

Usar apenas estratégias previamente autorizadas e registrar:

- fornecedor;
- tipo do documento;
- estratégia usada;
- sucesso ou falha;
- número de tentativas.

Nunca registrar a senha efetivamente utilizada.

---

## 3. Associação fornecedor/tipo → regra de senha

Criar no SQLite uma tabela de regras semelhante a:

- `provider_id`
- `document_type`
- `password_strategy`
- `priority`
- `enabled`
- `last_success_at`
- `last_failure_at`
- `notes`

Exemplos conceituais:

- Banco A + fatura → `cpf_full_digits`
- Banco B + extrato → `cpf_last_5`
- Fornecedor C + boleto → `cpf_first_4`
- Emissor D + demonstrativo → estratégia específica autorizada

Guardar apenas a **estratégia**, nunca o CPF ou a senha resultante.

---

## 4. Fluxo de abertura de PDF

Para cada PDF protegido:

1. identificar fornecedor/tipo;
2. buscar estratégias autorizadas;
3. gerar candidatos somente em memória;
4. tentar abrir o PDF;
5. ao obter sucesso, registrar a estratégia vencedora;
6. extrair metadados e conteúdo necessário;
7. calcular hash do original;
8. manter vínculo com a origem;
9. seguir para classificação/conciliação;
10. apagar qualquer cópia temporária descriptografada.

---

## 5. Preservação do original

O backup deve conservar o **PDF original exatamente como foi encontrado**, inclusive quando estiver protegido por senha.

Se for necessário gerar uma cópia descriptografada para processamento:

- ela deve ser temporária;
- ficar em área privada;
- ter permissões restritas;
- ser removida após a extração;
- não ser enviada ao GitHub;
- não ser sincronizada para o OneDrive por padrão.

O OneDrive deve receber preferencialmente o original protegido.

---

## 6. Aprendizado de regra

Quando eu confirmar que um fornecedor usa determinada estratégia de senha, o sistema deve registrar:

**fornecedor/tipo → estratégia**

para que os próximos documentos sejam processados automaticamente.

Exemplo:

> “Faturas desse emissor usam os últimos 5 dígitos do CPF.”

Essa confirmação passa a ser uma regra reutilizável.

---

## 7. Auditoria e limites

Registrar:

- PDF encontrado;
- fornecedor/tipo provável;
- protegido ou não;
- estratégia tentada;
- sucesso/falha;
- motivo de falha quando conhecido;
- status de extração.

Nunca registrar:

- CPF em texto puro;
- senha derivada;
- senha final usada;
- segredo em logs.

Após um número pequeno e configurável de falhas, marcar o documento para revisão em vez de continuar tentando indefinidamente.

---

## 8. Definição de pronto deste apêndice

O suporte a PDFs protegidos estará adequado quando:

- o CPF puder ser informado de forma local e protegida;
- nenhuma credencial sensível aparecer em Git, Markdown, SQLite ou logs;
- regras puderem ser associadas a fornecedor/tipo;
- candidatos forem gerados apenas em memória;
- PDFs autorizados forem abertos automaticamente quando uma regra conhecida funcionar;
- o original protegido for preservado;
- cópias temporárias descriptografadas forem eliminadas;
- sucessos e falhas forem auditáveis sem revelar segredos;
- novas regras confirmadas por mim puderem ser reaproveitadas nos próximos documentos.

A regra final é:

**guardar a estratégia, nunca o segredo.**
