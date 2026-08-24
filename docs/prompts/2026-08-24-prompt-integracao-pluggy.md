# PROMPT DE IMPLEMENTAÇÃO – Integração Pluggy no Seu Bolso Feliz

## Papel da equipe

Atue como uma equipe sênior de engenharia de software, arquitetura, dados, segurança e produto, trabalhando no repositório do **Seu Bolso Feliz**.

O objetivo deste loop é integrar o **Seu Bolso Feliz** à API da **Pluggy**, usando prioritariamente o fluxo **Meu Pluggy** para uso pessoal, transformando o Open Finance em uma nova fonte confiável de dados financeiros para o sistema.

## Goals executivos deste loop

A equipe deve perseguir resultados mensuráveis, não apenas tarefas.

### Goal A - Infraestrutura alvo

Colocar o produto em:

```text
Vercel + Neon + Worker VPS São Paulo
```

### Goal B - Open Finance

Conectar a Pluggy de forma segura e desacoplada.

### Goal C - Histórico

Recuperar e persistir o máximo possível dos últimos **365 dias**.

### Goal D - Reconciliação

Transformar Open Finance em evidência bancária para reconciliar Gmail, PDFs e comprovantes.

### Goal E - Produção

Encerrar somente quando o fluxo estiver rodando de verdade em produção.

A implementação deve ser feita com extremo cuidado para:

- não criar dependência estrutural irreversível da Pluggy;
- não expor `CLIENT_SECRET`, API Keys, tokens ou credenciais bancárias;
- não duplicar transações já capturadas por Gmail, PDF, comprovantes ou importações locais;
- recuperar e organizar, quando disponível, o histórico financeiro dos **últimos 12 meses / até 365 dias**;
- permitir sincronização incremental futura;
- acelerar substancialmente a conciliação financeira;
- preservar a rastreabilidade da origem de cada informação;
- manter o projeto preparado para trocar ou adicionar provedores de Open Finance futuramente;
- produzir uma base confiável para dashboard, cartões, saldos, faturas, dívidas, recorrências e análises financeiras.

---

# 1. CONTEXTO DO PRODUTO

O **Seu Bolso Feliz** é um sistema de gestão financeira pessoal.

O projeto já possui, entre outros componentes:

- aplicação web;
- aplicação mobile;
- workers;
- pipeline de ingestão;
- scanner de Gmail;
- scanner de arquivos locais;
- processamento de comprovantes;
- processamento de PDFs;
- mecanismos de categorização e normalização;
- dashboard financeiro;
- registros manuais;
- estrutura preparada para ingestão automatizada.

A arquitetura de infraestrutura alvo deste projeto passa a ser obrigatoriamente:

- **Vercel** para o site/aplicação web e APIs serverless compatíveis;
- **Neon PostgreSQL** como banco de dados principal de produção;
- **VPS em São Paulo** como ambiente principal e sempre ativo dos workers de longa duração;
- **máquina local do desenvolvedor** como ambiente alternativo para executar os mesmos workers em desenvolvimento, diagnóstico, recuperação e testes;
- acesso operacional à VPS por `ssh root@ssh.chico-figueiredo.com.br`.

A implementação deve abandonar qualquer pressuposto de Neon como dependência estrutural obrigatória. Se ainda existirem componentes Neon no código atual, a equipe deve analisar com cautela o que deve ser migrado, substituído, encapsulado ou mantido temporariamente, mas a arquitetura final de produção deste loop deve estar baseada em **Vercel + Neon + Worker VPS São Paulo**.

A integração Pluggy deve **somar-se** às fontes existentes, e não substituí-las.

A visão arquitetural passa a ser:

```text
                         INTERNET
                             -
               -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -
               -         VERCEL             -
               -  Web / Next.js / APIs      -
               -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -
                             -
                    conexão segura
                             -
               -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -
               -        NEON POSTGRES        -
               -  banco principal produção  -
               -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -
                             -  -
                             -
                 conexão PostgreSQL segura
                             -
        -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -
        -                                           -
 -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -                  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -
| VPS SÃO PAULO        |                | MÁQUINA LOCAL       |
 -  worker always-on      -                  -  worker opcional      -
 -  produção              -                  -  dev / recuperação    -
 -  Pluggy / Gmail / PDF  -                  -  mesmo código         -
 -  reconciliação         -                  -  mesmas migrations    -
 -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -                  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -  -
            -
            -  APIs externas
            -  -
Gmail / PDFs / Comprovantes / Arquivos locais / OFX / CSV / Pluggy
            -
            -  -
Camada de ingestão
            -
            -  -
Normalização financeira
            -
            -  -
Reconciliação
            -
            -  -
Ledger financeiro canônico
            -
            -  -
Neon PostgreSQL
            -
            -  -
Vercel  -  Dashboard / análises / alertas / metas
```

A Pluggy deve representar a fonte bancária de maior autoridade, mas não deve se tornar o domínio central do sistema.

---

# 2. OBJETIVO PRINCIPAL

Implementar a integração entre o **Seu Bolso Feliz** e a API Pluggy de maneira que o usuário possa:

1. conectar suas próprias instituições financeiras via Meu Pluggy / Open Finance;
2. importar contas bancárias;
3. importar saldos;
4. importar transações;
5. importar cartões de crédito;
6. importar faturas, quando suportadas;
7. importar empréstimos e financiamentos, quando suportados;
8. importar investimentos, quando suportados;
9. recuperar o máximo de histórico permitido pela instituição/API, buscando como meta **até os últimos 365 dias**;
10. manter os dados sincronizados;
11. cruzar automaticamente as transações bancárias com dados já obtidos via:

- Gmail;
- PDFs;
- comprovantes;
- importações locais;
- registros manuais;

12. evitar duplicidade;
13. marcar registros conciliados;
14. informar claramente quais registros são confirmados pela instituição financeira;
15. permitir revisão manual quando a conciliação automática não for suficientemente confiável.

---

# 2.1. ARQUITETURA DE PRODUÇÃO OBRIGATÓRIA

Este loop não termina em código local.

O resultado final obrigatório é um ambiente de produção funcional, observável e documentado, composto por:

```text
Frontend / Web / APIs compatíveis  -  Vercel
Banco PostgreSQL                   -  Neon
Workers de longa duração           -  VPS São Paulo
Worker alternativo                 -  máquina local
```

## Vercel

A Vercel deve hospedar:

- aplicação web;
- Next.js;
- rotas/API serverless apropriadas ao modelo da Vercel;
- autenticação e camada de apresentação;
- endpoints curtos e stateless;
- leitura do ledger e dados consolidados do Neon.

Não colocar na Vercel tarefas long-running inadequadas ao modelo serverless.

## Neon

O Neon será a **fonte de verdade persistente** de produção.

O banco deve armazenar:

- domínio financeiro canônico;
- conexões de provider;
- transações;
- evidências;
- conciliações;
- status de jobs;
- checkpoints;
- recorrências;
- dados consolidados;
- auditoria técnica necessária.

A equipe deve analisar cuidadosamente:

- pooling;
- limites de conexão;
- migrations;
- transações;
- índices;
- concorrência;
- idempotência;
- timezone;
- SSL;
- secrets;
- branches de banco, se forem úteis ao workflow.

## VPS de São Paulo

A VPS será o ambiente **principal e permanente dos workers**.

Acesso operacional:

```bash
ssh root@ssh.chico-figueiredo.com.br
```

O worker de produção deve permanecer sempre ativo e reiniciar automaticamente em caso de:

- reboot da VPS;
- crash;
- atualização;
- falha temporária.

Usar Docker / Docker Compose ou mecanismo equivalente coerente com a infraestrutura real do projeto.

É obrigatório possuir:

- restart policy;
- healthcheck;
- logs;
- configuração por `.env` ou secret store seguro;
- deploy reproduzível;
- rollback razoável;
- mecanismo de atualização;
- documentação operacional.

## Worker local

O mesmo worker deve poder ser executado na máquina local.

Não criar uma implementação diferente para produção.

A diferença deve estar somente em configuração.

O worker local deve servir para:

- desenvolvimento;
- testes;
- diagnóstico;
- execução manual;
- recuperação;
- backfill controlado;
- contingência.

O sistema deve impedir, por locking ou mecanismo equivalente, que dois workers façam processamento conflitante do mesmo job.

## Fluxo preferencial

```text
Pluggy / Gmail / arquivos
         -
Worker VPS São Paulo
         -
Neon PostgreSQL
         -
Vercel
         -
Usuário
```

O frontend não deve chamar Pluggy diretamente.

O browser não deve possuir `CLIENT_SECRET`.

A Vercel também não deve executar backfills ou rotinas longas apenas por conveniência se essas rotinas pertencem ao worker.

## Produção é parte da Definition of Done

Não considerar este loop concluído com:

```text
"funciona localmente"
```

O Definition of Done exige:

```text
Vercel funcionando
+
Neon funcionando
+
VPS São Paulo funcionando
+
worker em execução
+
Pluggy sincronizando
+
dashboard lendo dados reais reconciliados
```

# 3. PRINCÍPIO ARQUITETURAL OBRIGATÓRIO

A Pluggy **NÃO PODE** ser acoplada diretamente ao domínio financeiro central.

Criar uma abstração de provider financeiro.

Sugestão:

```text
packages/
  financial-connectors/
```

ou estrutura equivalente coerente com a arquitetura real já existente no repositório.

Definir um contrato conceitual semelhante a:

```typescript
interface FinancialDataProvider {
  connect(...): Promise<...>
  disconnect(...): Promise<...>
  syncAccounts(...): Promise<...>
  syncTransactions(...): Promise<...>
  syncCreditCards(...): Promise<...>
  syncCreditCardBills(...): Promise<...>
  syncLoans(...): Promise<...>
  syncInvestments(...): Promise<...>
}
```

Implementar:

```text
FinancialDataProvider
  -  -  -  PluggyProvider
  -  -  -  CsvProvider
  -  -  -  OfxProvider
  -  -  -  PdfProvider
  -  -  -  ManualProvider
```

Os providers já existentes não precisam necessariamente ser migrados neste loop caso isso gere risco ou escopo excessivo, mas a arquitetura nova deve permitir a migração gradual.

Nunca espalhar chamadas diretas à Pluggy por controllers, componentes React ou regras de domínio.

Toda comunicação Pluggy deve ficar encapsulada no adapter/provider correspondente.

---

# 4. SEGURANÇA - BLOQUEADORES ABSOLUTOS

## 4.1 Secrets

`CLIENT_ID` e, principalmente, `CLIENT_SECRET`:

- jamais podem aparecer no frontend;
- jamais podem ser incluídos no bundle web;
- jamais podem ser enviados ao browser;
- jamais podem ser commitados;
- jamais podem aparecer em logs;
- jamais podem ser enviados para analytics;
- jamais podem aparecer em mensagens de erro entregues ao usuário.

Armazenar apenas via ambiente seguro no backend/worker.

Exemplo:

```env
PLUGGY_CLIENT_ID=
PLUGGY_CLIENT_SECRET=
```

Adicionar aos arquivos de exemplo somente os nomes, nunca valores reais.

Validar `.gitignore`.

Executar busca no repositório antes de encerrar o loop para garantir que nenhum segredo foi acidentalmente persistido.

---

## 4.2 Credenciais bancárias

O Seu Bolso Feliz não deve coletar, armazenar, transmitir nem solicitar diretamente:

- senha bancária;
- token bancário;
- OTP;
- código de segurança;
- credenciais de internet banking.

A autenticação deve ocorrer no fluxo oficial fornecido pela Pluggy/Open Finance.

---

## 4.3 Logs

Logs devem registrar IDs técnicos e status, mas nunca:

- credenciais;
- tokens;
- número completo de cartão;
- dados bancários desnecessários;
- payloads integrais contendo informação financeira sensível.

Preferir logs estruturados:

```json
{
  "provider": "pluggy",
  "operation": "transactions_sync",
  "itemIdHash": "...",
  "accountIdHash": "...",
  "recordsProcessed": 123,
  "recordsCreated": 40,
  "recordsUpdated": 5,
  "recordsReconciled": 77,
  "durationMs": 1200
}
```

---

# 5. MODELO DE DADOS

Antes de implementar, revisar as entidades já existentes e reaproveitar o máximo possível.

NÃO criar tabelas redundantes se o modelo atual já possuir entidades equivalentes.

Entretanto, o modelo deve ser capaz de representar os seguintes conceitos.

## 5.1 Provider Connection

Representa uma conexão do usuário com um provedor financeiro.

Campos conceituais:

```text
id
user_id
provider
external_item_id
connector_id
institution_name
institution_logo_url
status
last_sync_at
last_successful_sync_at
last_error_at
last_error_code
consent_status
consent_expires_at
created_at
updated_at
deleted_at
```

Nunca armazenar segredo global da Pluggy nessa tabela.

---

## 5.2 External Account Mapping

```text
id
user_id
provider_connection_id
provider
external_account_id
canonical_account_id
account_type
account_subtype
currency
last_sync_at
created_at
updated_at
```

---

## 5.3 Proveniência de dados

Cada registro financeiro importado deve preservar sua origem.

Exemplo:

```text
source_type:
  OPEN_FINANCE
  GMAIL
  PDF
  RECEIPT
  MANUAL
  OFX
  CSV
  LOCAL_FILE
```

E, quando possível:

```text
source_provider = "pluggy"
source_external_id
source_document_id
source_received_at
source_payload_hash
```

O sistema deve ser capaz de responder:

> "De onde veio este dado?"

e:

> "Qual é a evidência bancária que confirma esta despesa?"

---

# 6. IMPORTAÇÃO DO HISTÓRICO DE 12 MESES

Na primeira sincronização de uma conta, executar uma rotina de **backfill**.

Meta:

```text
hoje - 365 dias  -  hoje
```

Considerar limitações específicas do banco/conector.

Se a API retornar menos de 365 dias, registrar de maneira explícita:

```text
requested_history_days = 365
available_history_days = X
```

Nunca inventar histórico que não exista.

O backfill deve:

1. obter contas;
2. obter transações;
3. paginar corretamente;
4. respeitar limites da API;
5. ser retomável;
6. ser idempotente;
7. suportar falha parcial;
8. possuir checkpoint;
9. não duplicar registros em reexecuções.

Criar status do job, como:

```text
PENDING
RUNNING
PARTIAL
COMPLETED
FAILED
RETRYING
```

Registrar progresso:

```text
data inicial
data final
página atual
total processado
total criado
total atualizado
total conciliado
total ignorado
```

---

# 7. SINCRONIZAÇÃO INCREMENTAL

Após o backfill inicial, não baixar novamente um ano inteiro a cada execução.

Criar sincronização incremental.

Estratégia recomendada:

```text
última sincronização bem-sucedida
         -
pequena janela de overlap
         -
hoje
```

Utilizar uma janela de segurança de alguns dias para absorver:

- transações atualizadas;
- transações pendentes que foram liquidadas;
- correções bancárias;
- mudanças de descrição;
- faturas atualizadas.

Toda sincronização deve ser idempotente.

Usar o identificador externo da Pluggy como uma das chaves, mas não como única forma de deduplicação global do sistema.

---

# 8. RECONCILIAÇÃO

Este é um dos pontos mais importantes do projeto.

Criar um **Reconciliation Engine** independente da Pluggy.

A conciliação deve cruzar:

```text
Open Finance / Pluggy
         -
Gmail
PDF
Comprovante
Manual
OFX
CSV
```

---

# 9. ESTADOS DE RECONCILIAÇÃO

Sugestão conceitual:

```text
DISCOVERED
NORMALIZED
POSSIBLE_MATCH
MATCHED
RECONCILED
CONFIRMED
REJECTED_MATCH
NEEDS_REVIEW
```

Uma transação bancária importada via Open Finance deve ser considerada forte evidência de ocorrência financeira.

Isso não significa que todos os metadados externos sejam semanticamente perfeitos.

Exemplo:

```text
UBER *TRIP
```

pode ser confirmado como débito pelo banco, mas a categoria "Transporte" ainda pode ser determinada pelo Seu Bolso Feliz.

---

# 10. ALGORITMO DE MATCHING

Criar score de conciliação.

Considerar, no mínimo:

## Valor

- valor exato;
- tolerância quando justificável;
- sinais débito/crédito normalizados.

## Data

Considerar diferença de:

- 0 dias;
- ±1 dia;
- ±2 dias;
- janela maior somente quando houver justificativa.

Cartão de crédito pode ter diferenças entre:

- data da compra;
- processamento;
- lançamento;
- fechamento da fatura.

## Merchant / descrição

Normalizar:

- caixa;
- acentos;
- pontuação;
- códigos de adquirente;
- prefixos;
- sufixos;
- descrições típicas de cartão;
- `PIX`;
- `PAG*`;
- `MP*`;
- `PG*`;
- `UBER *`;
- identificadores irrelevantes.

Criar uma representação:

```text
merchant_normalized
```

## Documento

Considerar:

- CPF/CNPJ;
- número do documento;
- invoice ID;
- NSU;
- autorização;
- identificadores de Pix;
- referência bancária;
- identificadores presentes em comprovantes.

## Score

Exemplo conceitual:

```text
valor exato                  +40
data exata                   +25
data ±1                      +20
merchant muito semelhante    +20
documento igual              +50
CNPJ igual                   +50
descrição semelhante         +10
```

Não usar necessariamente esses valores literais sem análise.

Definir faixas:

```text
>= limiar alto      -  auto-reconcile
faixa intermediária  -  needs review
baixo               -  não vincular
```

Nunca conciliar automaticamente quando houver dois ou mais candidatos igualmente plausíveis.

---

# 11. DEDUPLICAÇÃO

Não confundir:

```text
duas fontes descrevendo a mesma transação
```

com:

```text
duas transações verdadeiramente distintas de mesmo valor
```

Exemplo:

```text
R$ 49,90 Netflix em janeiro
R$ 49,90 Netflix em fevereiro
```

não são duplicatas.

Mas:

```text
email Nubank  -  Netflix  -  49,90  -  10/08
Pluggy  -  NETFLIX.COM  -  49,90  -  10/08
```

provavelmente representam o mesmo evento financeiro.

Criar fingerprint probabilístico, sem destruir os registros fonte.

O ledger deve apontar para múltiplas evidências.

Exemplo:

```text
canonical_transaction
     -  -  -  evidence: pluggy transaction
     -  -  -  evidence: gmail message
     -  -  -  evidence: pdf invoice
```

---

# 12. PRINCÍPIO DO LEDGER CANÔNICO

Separar:

```text
evento financeiro canônico
```

de:

```text
evidência/origem
```

Uma única despesa pode possuir múltiplas fontes.

Exemplo:

```text
Despesa:
Uber
R$ 42,80
21/08/2026
Transporte
CONFIRMADA

Evidências:
 -  Open Finance
 -  Gmail
 -  Comprovante
```

Essa arquitetura é essencial.

---

# 13. CARTÕES DE CRÉDITO

Importar, quando disponível:

- instituição;
- cartão;
- bandeira;
- limite total;
- limite disponível;
- faturas;
- valor da fatura;
- vencimento;
- pagamento mínimo;
- transações;
- parcelamentos;
- status.

Não armazenar PAN completo.

Se a API fornecer apenas dígitos finais, utilizar somente esses dados.

---

# 14. PARCELAMENTOS

Não assumir que todas as instituições representam parcelas da mesma maneira.

Criar entidade lógica:

```text
purchase_group
```

ou equivalente.

Ela deve permitir agrupar:

```text
Compra Notebook
R$ 6.000
12 parcelas

 -  -  -  1/12
 -  -  -  2/12
 -  -  -  3/12
...
 -  -  -  12/12
```

Usar heurísticas:

- merchant;
- totalAmount;
- installmentNumber;
- totalInstallments;
- datas;
- valor;
- cartão;
- descrição.

Nunca agrupar automaticamente quando a confiança for baixa.

---

# 15. RECORRÊNCIAS

Não criar dependência obrigatória da API premium de recorrências da Pluggy.

Implementar o mecanismo primário dentro do Seu Bolso Feliz.

Detectar padrões como:

```text
Netflix
Spotify
Academia
Condomínio
Escola
Plano de saúde
Internet
Telefone
Salário
Financiamento
Assinaturas
```

Analisar:

- periodicidade;
- variação do valor;
- merchant normalizado;
- intervalo temporal;
- quantidade de ocorrências;
- regularidade.

Persistir algo semelhante a:

```text
recurrence_id
merchant
frequency
average_amount
next_expected_date
confidence_score
occurrences
```

A Pluggy pode futuramente ser usada para enriquecer essa informação, mas o sistema deve funcionar sem feature premium.

---

# 16. CATEGORIZAÇÃO

Mesma regra.

Não depender obrigatoriamente do Transaction Enrichment premium.

O Seu Bolso Feliz deve possuir sua própria camada:

```text
Merchant Resolver
         -
Category Engine
```

A categorização deve poder aprender com correções do usuário.

Exemplo:

```text
UBER  -  Transporte
DROGASIL  -  Saúde
NETFLIX  -  Streaming
SUPERMERCADO XYZ  -  Alimentação
```

Guardar regras e histórico.

---

# 17. CONTAS E SALDOS PARA O DASHBOARD

A integração Pluggy deve alimentar diretamente os cards do novo dashboard.

O usuário deve conseguir ver:

```text
Saldo consolidado
```

e individualmente:

```text
Conta CAIXA
Conta Nubank
Conta C6
outras instituições conectadas
```

Cada card deve apresentar:

- saldo;
- instituição;
- conta;
- data/hora da última atualização;
- status da conexão.

Nunca apresentar dado antigo como se fosse atual.

Se a última sincronização estiver defasada, sinalizar.

---

# 18. CARTÕES NO DASHBOARD

Permitir cards pequenos e claros com:

```text
Cartão A
Fatura atual
Limite disponível
Vencimento
```

```text
Cartão B
Fatura atual
Limite disponível
Vencimento
```

Dados devem vir da fonte Open Finance sempre que disponíveis.

---

# 19. DÍVIDAS / EMPRÉSTIMOS

Quando a Pluggy/instituição fornecer loans/financing:

integrar com a área de dívidas do Seu Bolso Feliz.

Possíveis dados:

```text
saldo devedor
parcela
quantidade de parcelas
taxa
vencimento
status
instituição
```

Não sobrescrever manualmente informação mais rica sem regra explícita de precedência.

Criar estratégia de merge.

---

# 20. UX DA CONEXÃO

Criar uma experiência simples.

Exemplo:

```text
Configurações
   -
Instituições Financeiras
   -
[ + Conectar instituição ]
```

Mostrar:

```text
CAIXA
 -  Conectado
Última atualização: hoje 18:42
[Sincronizar]
[Gerenciar]
```

Possíveis estados:

```text
Conectado
Sincronizando
Requer atenção
Consentimento expirado/revogado
Erro
Desconectado
```

Mensagens devem ser humanas.

Evitar jargões como:

```text
HTTP 401
ITEM_LOGIN_ERROR
```

Mostrar ao usuário algo como:

```text
Sua conexão com a CAIXA precisa ser renovada.
```

e registrar detalhe técnico apenas nos logs.

---

# 21. PRIMEIRO BACKFILL

Após conexão bem-sucedida:

mostrar progresso.

Exemplo:

```text
Importando seu histórico financeiro

 -  Contas encontradas
 -  Cartões encontrados
 -  Saldos atualizados
 -  -  Importando transações dos últimos 12 meses
 -  -  Procurando correspondências com comprovantes
```

Ao final:

```text
Importação concluída

2.483 transações encontradas
1.072 despesas reconhecidas
680 conciliadas automaticamente
97 recorrências identificadas
45 registros precisam de revisão
```

Números são ilustrativos.

---

# 22. TELA DE REVISÃO DA CONCILIAÇÃO

Criar ou adaptar uma área:

```text
Revisar conciliações
```

Exemplo:

```text
Transação bancária
UBER *TRIP
R$ 42,80
21/08

Possível correspondência
Comprovante Uber
R$ 42,80
21/08

Confiança: 97%

[Confirmar]
[Não é a mesma]
```

Registrar decisões do usuário para melhorar heurísticas futuras.

---

# 23. CONFLITOS

Se duas fontes divergirem:

```text
Pluggy:  R$ 129,90
PDF:     R$ 129,00
```

não sobrescrever silenciosamente.

Registrar conflito.

Criar estados como:

```text
DATA_CONFLICT
AMOUNT_CONFLICT
DATE_CONFLICT
MERCHANT_CONFLICT
```

A fonte bancária pode ter maior autoridade sobre o fato do débito/crédito, porém isso não elimina a necessidade de explicar divergências.

---

# 24. IDEMPOTÊNCIA

Toda operação deverá poder ser executada mais de uma vez sem duplicar dados.

Cobrir:

- importação inicial;
- sincronização;
- retry;
- worker reiniciado;
- webhook repetido;
- timeout;
- erro parcial.

Usar constraints e chaves técnicas adequadas.

---

# 25. WEBHOOKS

Se a modalidade da conta Pluggy usada no ambiente suportar webhooks, implementar adapter para recebê-los.

Entretanto:

**o sistema não pode depender exclusivamente de webhooks para funcionar.**

A sincronização programada deve continuar sendo capaz de recuperar o estado correto.

Eventos devem apenas disparar ou acelerar a sincronização.

Criar endpoint seguro.

Validar autenticidade conforme documentação oficial vigente.

Não confiar cegamente no payload.

---

# 26. WORKER

Criar worker dedicado ou adaptar a arquitetura existente.

Sugestão:

```text
workers/
  pluggy-sync/
```

ou:

```text
workers/ingestion/providers/pluggy/
```

escolher conforme a arquitetura real.

Responsabilidades:

```text
autenticar provider
listar conexões
sincronizar contas
sincronizar saldos
sincronizar transações
sincronizar cartões
sincronizar faturas
sincronizar loans
sincronizar investimentos
normalizar
deduplicar
reconciliar
registrar métricas
```

Evitar lógica de UI no worker.

---

# 27. RATE LIMIT / RESILIÊNCIA

Implementar:

- retry exponencial;
- jitter;
- timeout;
- circuit breaker se apropriado;
- limites de concorrência;
- paginação;
- filas;
- dead-letter ou mecanismo equivalente para erros persistentes.

Não realizar loop agressivo contra API externa.

---

# 28. OBSERVABILIDADE

Criar métricas para:

```text
sync_started
sync_completed
sync_failed
transactions_received
transactions_created
transactions_updated
transactions_reconciled
transactions_needs_review
duplicates_prevented
accounts_synced
cards_synced
bills_synced
sync_duration
```

Criar correlação por job.

---

# 29. TESTES

Implementar testes unitários.

Cobrir:

- normalização;
- fingerprint;
- matching;
- score;
- deduplicação;
- merge;
- parcelamento;
- recorrência.

Criar testes de integração com fixtures Pluggy anonimizadas.

Nunca salvar payload real contendo dados pessoais do usuário no repositório.

Criar fixtures sintéticas.

---

# 30. TESTES DE IDPOTÊNCIA

Criar testes obrigatórios:

## Caso 1

Importar mesma transação duas vezes.

Resultado:

```text
1 canonical transaction
1 external source
0 duplicatas
```

## Caso 2

Gmail primeiro, Pluggy depois.

Resultado:

```text
1 canonical transaction
2 evidências
status RECONCILED
```

## Caso 3

Pluggy primeiro, Gmail depois.

Mesmo resultado.

## Caso 4

Duas compras iguais no mesmo estabelecimento e mesmo valor, mas em datas/horários distintos.

Resultado:

```text
2 transações
```

## Caso 5

Worker morre no meio do backfill.

Após reinício:

```text
retoma corretamente
não duplica
```

---

# 31. PRIVACIDADE

Aplicar minimização de dados.

Importar apenas o necessário para o funcionamento do Seu Bolso Feliz.

Criar mecanismo para:

```text
Desconectar instituição
```

e distinguir:

```text
desconectar provider
```

de:

```text
apagar histórico importado
```

A UX deve deixar essa diferença clara.

Exemplo:

```text
Desconectar a instituição impede novas sincronizações.
Seu histórico já importado continuará no Seu Bolso Feliz.

[Desconectar]
```

e separadamente:

```text
Excluir dados importados desta instituição
```

com confirmação reforçada.

---

# 32. DATA LINEAGE

O sistema deve preservar lineage.

Deve ser possível auditar:

```text
Canonical Transaction #123
   -  provider: Pluggy
   -  external transaction id: XXX
   -  account: CAIXA
   -  imported_at
   -  last_synced_at
   -  matched Gmail message
   -  matched PDF
```

---

# 33. PRECEDÊNCIA DE FONTES

Criar regras documentadas.

Exemplo:

### Valor efetivamente debitado

Preferência:

```text
Open Finance > OFX > PDF > Gmail > Manual inferred
```

### Categoria

Preferência:

```text
User confirmed > learned rule > category engine > provider enrichment
```

### Merchant

Pode combinar:

```text
documento fiscal + descrição bancária + regra interna
```

Não criar regras ocultas.

---

# 34. NÃO DESTRUIR DADOS MANUAIS

Se o usuário corrigiu:

```text
Categoria = Saúde
```

uma sincronização posterior não pode voltar para:

```text
Categoria = Outros
```

Criar mecanismo de:

```text
user_override
```

Campos corrigidos manualmente devem ter precedência.

---

# 35. RESULTADO ESPERADO NO DASHBOARD

Após integração, o usuário deve entrar no Seu Bolso Feliz e enxergar algo próximo de:

```text
PATRIMÔNIO DISPONÍVEL

CAIXA                 R$ ...
Nubank                R$ ...
C6                    R$ ...
TOTAL                 R$ ...
```

```text
CARTÕES

Nubank
Fatura atual          R$ ...
Limite disponível     R$ ...

C6
Fatura atual          R$ ...
Limite disponível     R$ ...
```

```text
ESTE MÊS

Receitas              R$ ...
Despesas              R$ ...
Saldo                  R$ ...
```

Os valores precisam derivar do ledger canônico, evitando dupla contagem.

---

# 35.1. GOALS, METAS E GATES DE EXECUÇÃO

A equipe deve trabalhar com goals claros e verificáveis.

Não iniciar o loop como uma sequência aberta de tarefas.

## GOAL 1 - Arquitetura preparada

Entregáveis:

- aplicação configurada para Vercel;
- acesso ao Neon validado;
- migrations funcionando;
- worker separado do runtime Vercel;
- configuração local e VPS compartilhando o mesmo código.

Gate:

```text
build  -
conexão Neon  -
worker inicia  -
Vercel inicia  -
```

## GOAL 2 - Provider Pluggy funcional

Entregáveis:

- adapter `PluggyProvider`;
- autenticação server-side;
- secrets protegidos;
- conexão com Item/conta;
- importação de contas e saldos.

Gate:

```text
Pluggy autenticada  -
nenhum secret no frontend  -
contas reais retornadas  -
saldos persistidos no Neon  -
```

## GOAL 3 - Backfill do último ano

Entregáveis:

- rotina de backfill;
- paginação;
- checkpoint;
- idempotência;
- retomada após falha;
- busca de até 365 dias quando disponível.

Gate:

```text
365 dias solicitados  -
limitação da instituição registrada  -
reexecução sem duplicatas  -
job retomável  -
```

## GOAL 4 - Reconciliação

Entregáveis:

- ledger canônico;
- evidências;
- matching;
- score;
- revisão manual;
- deduplicação.

Gate:

```text
Gmail + Pluggy reconciliam  -
PDF + Pluggy reconciliam  -
duplicatas evitadas  -
ambiguidades vão para revisão  -
```

## GOAL 5 - Worker de produção em São Paulo

Entregáveis:

- deploy do worker na VPS;
- container/processo sempre ativo;
- restart automático;
- healthcheck;
- logs;
- acesso ao Neon;
- acesso à Pluggy;
- execução programada.

Gate obrigatório:

```text
ssh root@ssh.chico-figueiredo.com.br  -
worker ativo  -
healthcheck saudável  -
sync real concluído  -
restart testado  -
```

## GOAL 6 - Produção Vercel + Neon

Entregáveis:

- aplicação implantada na Vercel;
- banco Neon de produção;
- variáveis de ambiente corretas;
- migrations aplicadas;
- dashboard lendo ledger real.

Gate:

```text
deploy Vercel  -
Neon produção  -
dashboard abre  -
dados reais aparecem  -
nenhum dado duplicado  -
```

## GOAL 7 - Operação ponta a ponta

Executar um cenário real:

```text
Pluggy
   -
worker VPS
   -
Neon
   -
reconciliação
   -
Vercel
   -
dashboard
```

Gate final:

- sincronização concluída;
- histórico disponível;
- reconciliação processada;
- saldo consistente;
- cards consistentes;
- logs sem erro crítico;
- sistema permanece operacional após reinício do worker.

Somente depois desse gate o loop poderá ser encerrado.

# 36. CRITÉRIOS DE ACEITE FUNCIONAIS

O loop somente poderá ser considerado concluído se:

- [ ] Aplicação estiver implantada e acessível na Vercel.
- [ ] Banco principal de produção for Neon PostgreSQL.
- [ ] Migrations de produção estiverem aplicadas no Neon.
- [ ] Worker principal estiver implantado e ativo na VPS de São Paulo.
- [ ] Worker reiniciar automaticamente após reboot/crash.
- [ ] Healthcheck do worker estiver funcional.
- [ ] Mesmo worker puder rodar localmente por configuração.
- [ ] Locking impedir processamento conflitante entre worker VPS/local.
- [ ] Fluxo ponta a ponta Pluggy - VPS - Neon - Vercel estiver validado.
- [ ] Pluggy estiver encapsulada em provider/adapter.
- [ ] Nenhum segredo estiver no frontend.
- [ ] Nenhum segredo estiver no Git.
- [ ] Conexão com instituição puder ser estabelecida.
- [ ] Contas forem importadas.
- [ ] Saldos forem importados.
- [ ] Transações forem importadas.
- [ ] Histórico inicial buscar até 365 dias quando disponível.
- [ ] Paginação estiver implementada corretamente.
- [ ] Backfill for retomável.
- [ ] Sincronização incremental existir.
- [ ] Sincronização for idempotente.
- [ ] Cartões forem importados quando disponíveis.
- [ ] Faturas forem importadas quando disponíveis.
- [ ] Loans forem importados quando disponíveis.
- [ ] Investimentos forem importados quando disponíveis.
- [ ] Proveniência estiver registrada.
- [ ] Matching com Gmail/PDF/comprovantes existir.
- [ ] Duplicidade entre fontes for evitada.
- [ ] Tela de revisão existir ou fluxo equivalente estiver funcional.
- [ ] Overrides manuais forem preservados.
- [ ] Dashboard não contar duas vezes o mesmo evento financeiro.
- [ ] Falhas de conexão forem tratadas.
- [ ] Retry existir.
- [ ] Logs não vazarem dados sensíveis.
- [ ] Testes automatizados cobrirem o core.
- [ ] Build passar.
- [ ] Typecheck passar.
- [ ] Lint passar.
- [ ] Testes passarem.

---

# 37. CRITÉRIOS DE ACEITE DE RECONCILIAÇÃO

Criar um dataset de teste sintético com pelo menos:

```text
100 transações bancárias
40 emails
20 PDFs/comprovantes
15 recorrências
10 compras parceladas
10 conflitos
10 casos deliberadamente ambíguos
```

A implementação deve demonstrar:

- correspondências exatas;
- correspondências aproximadas;
- não correspondência quando o score é insuficiente;
- detecção de ambiguidade;
- prevenção de falso positivo;
- preservação de evidências.

Não buscar artificialmente 100% de auto-conciliação.

É preferível:

```text
90% correto + 10% revisão
```

a:

```text
99% automático com falsos positivos
```

---

# 38. MIGRAÇÕES

Toda mudança no banco deve possuir migration versionada.

Não alterar banco manualmente sem migration.

Criar:

- índices;
- constraints;
- unique keys;
- foreign keys;

onde necessário.

Pensar em escala de alguns anos de histórico.

---

# 39. PERFORMANCE

A tela principal não deve consultar a Pluggy diretamente.

Fluxo correto:

```text
Pluggy
   -
Worker VPS São Paulo
   -
Neon PostgreSQL
   -
API / aplicação na Vercel
   -
frontend
```

O dashboard deve ler dados locais.

Isso melhora:

- velocidade;
- resiliência;
- custo;
- experiência offline parcial;
- independência do provider.

---

# 40. FEATURE FLAGS

A integração deve poder ser habilitada/desabilitada.

Exemplo:

```env
FEATURE_PLUGGY_ENABLED=true
```

Se desligada:

- o Seu Bolso Feliz continua funcionando;
- dados já importados continuam acessíveis;
- novas sincronizações não são realizadas.

---

# 41. DOCUMENTAÇÃO

Criar documentação técnica:

```text
docs/integrations/pluggy.md
```

Cobrir:

- arquitetura;
- configuração;
- variáveis de ambiente;
- fluxo de autenticação;
- fluxo de conexão;
- backfill;
- sync incremental;
- modelo de dados;
- reconciliação;
- erros;
- retry;
- testes;
- segurança;
- procedimento de desconexão.

Criar também:

```text
docs/integrations/pluggy-troubleshooting.md
```

---

# 42. SETUP DO USUÁRIO

Documentar passo a passo:

1. criar/usar conta Meu Pluggy;
2. conectar as próprias instituições;
3. acessar Dashboard Pluggy;
4. obter `CLIENT_ID`;
5. obter `CLIENT_SECRET`;
6. configurar ambiente local;
7. iniciar worker;
8. verificar conexão;
9. iniciar backfill;
10. acompanhar logs;
11. validar saldos;
12. validar transações;
13. validar reconciliações.

Nunca incluir valores reais dos secrets na documentação.

---

# 43. DEPENDÊNCIA DA PLUGGY

O código precisa assumir que:

- instituições podem ficar temporariamente indisponíveis;
- campos podem não existir em todos os conectores;
- Open Finance não é semanticamente uniforme;
- cartões podem variar;
- parcelamentos podem variar;
- empréstimos podem variar;
- consentimentos podem ser revogados;
- conexões podem exigir renovação.

Portanto:

**nunca tratar ausência de dado opcional como corrupção geral da sincronização.**

---

# 44. PRINCÍPIO DE DEGRADAÇÃO GRACIOSA

Se uma instituição oferecer:

```text
contas + transações
```

mas não:

```text
investimentos
```

o sistema deve continuar funcionando normalmente.

Se cartão não retornar faturas completas:

mostrar apenas o que é conhecido.

Nunca inventar.

---

# 45. FORNECEDOR NÃO É DOMÍNIO

Não usar nomes como:

```text
pluggy_transaction
pluggy_account
```

como entidades centrais de domínio.

Preferir:

```text
external_transaction
financial_account
canonical_transaction
provider_connection
```

Pluggy deve ser atributo/provider.

---

# 46. CAMADA DE NORMALIZAÇÃO

Definir DTO interno.

Exemplo conceitual:

```typescript
type NormalizedTransaction = {
  provider: "pluggy";
  externalId: string;
  accountId: string;
  direction: "credit" | "debit";
  amount: number;
  currency: string;
  transactionDate: Date;
  postingDate?: Date;
  descriptionRaw?: string;
  merchantRaw?: string;
  merchantNormalized?: string;
  document?: string;
  paymentMethod?: string;
  installment?: {
    current?: number;
    total?: number;
    totalAmount?: number;
  };
  metadata?: Record<string, unknown>;
};
```

Toda transformação Pluggy - domínio deve ocorrer nessa camada.

---

# 46.1. DEPLOY E OPERAÇÃO DE PRODUÇÃO

Este loop deve incluir deploy real.

## Vercel

Configurar e validar:

- projeto;
- build command;
- runtime;
- environment variables;
- conexão Neon;
- domínio de produção já definido no projeto, se existente;
- preview environments quando útil;
- health;
- login;
- dashboard;
- queries reais.

## Neon

Configurar:

- projeto/banco de produção;
- connection string segura;
- pooling adequado à Vercel;
- acesso do worker VPS;
- migrations;
- índices;
- backup/restore conforme recursos disponíveis.

Não expor connection string em código.

## VPS São Paulo

A equipe deve efetivamente validar acesso e implantação em:

```bash
ssh root@ssh.chico-figueiredo.com.br
```

Estrutura recomendada:

```text
/opt/seu-bolso-feliz/
  docker-compose.yml
  .env
  worker/
```

ou estrutura equivalente mais adequada.

O processo deve suportar comandos operacionais documentados:

```text
start
stop
restart
status
logs
healthcheck
deploy/update
```

Não armazenar dados bancários em arquivos locais da VPS quando o dado puder permanecer no Neon.

## Scheduler

O worker deve possuir estratégia clara de execução:

- loop interno resiliente;
- cron;
- fila de jobs;
- scheduler persistente;

ou solução equivalente justificada.

Evitar execução duplicada.

## Segurança de rede

- Neon somente via conexão segura/SSL;
- secrets somente no servidor;
- firewall da VPS revisado;
- portas desnecessárias fechadas;
- nenhum painel administrativo exposto sem proteção;
- logs sem dados financeiros desnecessários.

# 47. AUDITORIA FINAL

Antes de concluir:

executar obrigatoriamente:

```text
build
typecheck
lint
tests
```

Inspecionar:

```text
git diff
```

Buscar:

```text
CLIENT_SECRET
PLUGGY_CLIENT_SECRET
token
apiKey
password
```

garantindo ausência de valores reais.

---

# 48. ENTREGA DO LOOP

Ao terminar, entregar relatório contendo:

## Implementado

Lista objetiva.

## Arquitetura

Diagrama Mermaid.

## Banco de dados

Migrations e novas entidades.

## Integração

Endpoints/worker implementados.

## Histórico

Quantidade de dias suportada e estratégia usada.

## Reconciliação

Descrição do algoritmo e thresholds.

## Testes

Resultados.

## Segurança

Validações realizadas.

## Limitações

Instituições/campos que apresentaram restrições.

## Próximos passos

Somente o que realmente ficou fora do escopo.

---

# 49. REGRAS DE TRABALHO DA EQUIPE

- Trabalhe diretamente sobre a arquitetura atual.
- Leia o repositório antes de alterar.
- Não invente componentes que já existam.
- Reutilize código.
- Evite grandes reescritas sem necessidade.
- Faça mudanças incrementais.
- Mantenha compatibilidade.
- Evite dependências desnecessárias.
- Não deixe TODO crítico.
- Não use mocks como substituição da implementação real.
- Fixtures são permitidas exclusivamente para testes.
- Não declare concluído algo não testado.

---

# 50. DEFINITION OF DONE

A funcionalidade estará realmente concluída somente quando existir **produção operacional ponta a ponta**.

O usuário deve conseguir:

1. acessar o Seu Bolso Feliz implantado na **Vercel**;
2. usar o banco principal de produção no **Neon PostgreSQL**;
3. ter o worker de produção permanentemente ativo na **VPS de São Paulo**;
4. acessar operacionalmente a VPS por `ssh root@ssh.chico-figueiredo.com.br`;
5. opcionalmente executar o mesmo worker localmente sem fork de implementação;
6. conectar a integração Pluggy;
7. enxergar suas instituições;
8. executar o primeiro sync;
9. trazer o histórico disponível de até 12 meses;
10. enxergar contas e saldos;
11. enxergar transações;
12. enxergar cartões/faturas quando suportados;
13. cruzar automaticamente transações com Gmail/PDF/comprovantes;
14. revisar matches incertos;
15. não sofrer duplicidade;
16. sincronizar novamente sem duplicar;
17. preservar correções manuais;
18. visualizar o dashboard usando o ledger reconciliado;
19. desligar a Pluggy sem quebrar o restante do Seu Bolso Feliz;
20. reiniciar o worker/VPS e comprovar recuperação automática;
21. comprovar o fluxo real:

```text
Pluggy  -  Worker VPS São Paulo  -  Neon  -  Vercel  -  Dashboard
```

Não aceitar como Definition of Done:

- somente código commitado;
- somente testes locais;
- somente mocks;
- somente preview;
- somente banco local;
- somente worker local.

**Definition of Done = site em produção na Vercel + banco Neon de produção + worker ativo na VPS de São Paulo + integração Pluggy operando + histórico e reconciliação funcionando ponta a ponta.**

---

# 51. RESULTADO DE PRODUTO DESEJADO

O objetivo não é apenas - integrar uma API - .

O objetivo é fazer o **Seu Bolso Feliz** evoluir para um sistema em que:

```text
o banco confirma que o dinheiro saiu
+
o documento explica o que foi comprado
+
o sistema entende a categoria
+
a recorrência é detectada
+
o usuário só revisa as exceções
```

O resultado final deve reduzir drasticamente o trabalho manual de conciliação financeira.

A experiência esperada é:

> conectar as contas uma vez, importar o último ano, deixar o sistema organizar o histórico e, a partir daí, trabalhar predominantemente por exceção.

Esse é o verdadeiro critério de sucesso deste loop.

E há um critério adicional, igualmente obrigatório:

> **o trabalho termina em produção, não no repositório.**

A entrega final deve comprovar:

```text
VERCEL        -> PRODUÇÃO OK
NEON          -> PRODUÇÃO OK
VPS SP         -  WORKER OK
PLUGGY         -  SYNC OK
BACKFILL      -> ATÉ 12 MESES OK
RECONCILIAÇÃO  -  OK
DASHBOARD      -  DADOS REAIS OK
```

Se qualquer um desses componentes estiver apenas "preparado para deploy", o loop ainda não terminou.
