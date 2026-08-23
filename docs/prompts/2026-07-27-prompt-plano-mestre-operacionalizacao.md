# Prompt de missão — Plano mestre de transformação do Seu Bolso Feliz

> **Repositório:** `ChicoFigueiredo/Seu-Bolso-Feliz`  
> **Branch de referência:** `feat/api-key`  
> **Data de emissão:** 27 de julho de 2026  
> **Natureza desta demanda:** auditoria, planejamento arquitetural, backlog executivo e plano de implementação.  
> **Não iniciar implementação estrutural antes de concluir e registrar o planejamento exigido neste documento.**

---

## 1. Contexto executivo

O Seu Bolso Feliz precisa deixar de ser um conjunto de telas, tabelas, parsers, workers e experimentos de ingestão para se tornar uma ferramenta financeira pessoal confiável, utilizável no navegador e apoiada por um agente local.

O objetivo do produto é permitir que o usuário consiga:

- registrar despesas pelo navegador e pela câmera do celular;
- importar faturas, extratos, boletos, contratos, comprovantes e planilhas;
- analisar e-mails e anexos do Gmail;
- processar PDFs protegidos por senha;
- identificar fornecedores, cartões, contas, categorias e contratos;
- reconhecer despesas recorrentes com base em histórico real;
- deduplicar evidências diferentes referentes à mesma obrigação;
- enxergar os próximos pagamentos;
- prever o fluxo de caixa;
- saber quanto do limite de cada cartão está comprometido nos próximos meses;
- analisar faturas e compras parceladas;
- acompanhar dívidas e simular estratégias de quitação;
- operar tarefas pesadas em um worker local seguro e facilmente acionável;
- controlar e supervisionar esse worker pela aplicação web e por MCP.

Existe uma necessidade pessoal e financeira concreta por trás do projeto: reduzir incerteza, organizar pagamentos, evitar estouro de limites e viabilizar uma estratégia séria de quitação de dívidas até o fim do ano. Portanto, o sucesso não será medido pela quantidade de componentes entregues. Será medido pelas decisões financeiras confiáveis que o sistema passar a produzir.

---

## 2. Missão da equipe

Realizar uma auditoria completa da branch `feat/api-key`, confirmar o estado real do código e produzir um plano mestre executável para transformar o repositório em uma arquitetura híbrida:

1. **Aplicação web leve**, hospedada no Vercel, construída com React e Next.js.
2. **Control plane e persistência**, hospedados no Supabase.
3. **Agente local instalável**, responsável por Gmail, OCR, PDFs protegidos, parsing pesado, processamento histórico e modelos de IA.
4. **Pipeline canônico único**, compartilhado por câmera, upload web, Gmail, pasta local e MCP.
5. **Domínio financeiro integrado**, capaz de transformar evidências em obrigações, transações, faturas, parcelas, recorrências, dívidas, projeções e decisões.

A equipe deve se responsabilizar pelo diagnóstico, arquitetura, decomposição do trabalho, dependências, riscos, testes, migrações e estratégia de entrega. Não aguardem que o CEO deduza os detalhes técnicos restantes.

---

## 3. Regra principal: provar antes de declarar

Nada poderá ser classificado como pronto apenas porque existe:

- uma tabela;
- uma migration;
- uma interface TypeScript;
- uma tela;
- um botão;
- um worker;
- uma função isolada;
- um comentário no código;
- um item marcado com `✅` em documento;
- um teste unitário que não cobre a jornada completa.

Uma capacidade só poderá ser classificada como pronta quando houver evidência reproduzível, preferencialmente por teste ponta a ponta, demonstrando que a informação percorre o fluxo inteiro e chega ao domínio financeiro definitivo.

Exemplo mínimo:

```text
Documento real
  → ingestão
  → extração
  → normalização
  → resolução de fornecedor e produto
  → obrigação financeira
  → revisão humana
  → materialização
  → transação/fatura/dívida real
  → agenda e relatório atualizados
```

Se qualquer elo estiver quebrado, a capacidade é parcial ou inexistente.

---

## 4. Constatações preliminares que precisam ser verificadas

As constatações abaixo foram identificadas em auditoria estática. A equipe deve confirmá-las no código e por execução. Caso alguma já tenha sido corrigida, apresentar a evidência objetiva.

### 4.1 Drafts e materialização falam contratos diferentes

O gerador de drafts aparentemente produz campos como:

- `type: "despesa"`;
- `due_date`;
- `base_amount`;
- `recurrence`;
- categoria textual;
- fornecedor textual;
- sem `financial_product_id` obrigatório;
- sem `event_date` obrigatório.

O materializador aparentemente exige:

- `type: "expense"`;
- `amount`;
- `event_date`;
- `financial_product_id`;
- `category_id`;
- `frequency`.

Isso tende a impedir a materialização dos próprios drafts gerados automaticamente.

### 4.2 Aprovação não fecha o ciclo

A tela de revisão parece apenas trocar o status do draft para `approved`, sem materializar o registro financeiro definitivo. Verificar se clicar em aprovar realmente cria, de forma atômica e idempotente, uma transação, recorrência, fatura, parcela ou dívida.

### 4.3 Materialização indevida de itens pendentes

O materializador aparentemente aceita `pending_review`, apesar da regra de negócio afirmar que somente registros aprovados ou corrigidos podem ser materializados. Isso deve ser tratado como falha crítica de governança.

### 4.4 Obrigações financeiras desconectadas

As tabelas `financial_obligations` e `financial_obligation_evidences` existem, mas precisam ser verificadas quanto à sua utilização real pelo pipeline. Confirmar se toda evidência relevante converge para uma obrigação canônica e se pagamentos, faturas, lembretes e comprovantes são conciliados corretamente.

### 4.5 Chave semântica pode manter duplicatas

A chave de identidade aparentemente usa prefixos diferentes para fatura e lembrete, além de incluir valor e intenção. Isso pode impedir que e-mail de lembrete, PDF de fatura, notificação e comprovante referentes ao mesmo evento sejam agrupados.

### 4.6 Orquestrador unificado promete mais do que executa

O `financial-evidence-worker` aparentemente declara suporte a:

- `--from-date`;
- `--to-date`;
- `--process`;
- `--ai-mode`;
- `--user-id`.

Porém, parte desses parâmetros parece não chegar aos workers executados. Verificar também se o orquestrador realmente processa os jobs ou apenas escaneia fontes.

### 4.7 Query e período do Gmail podem não funcionar

O scanner recebe `--query`, mas o cliente Gmail parece listar mensagens apenas por label. Verificar se queries, datas, paginação, checkpoint, retomada e varredura histórica funcionam de verdade.

### 4.8 PDFs protegidos podem receber ciphertext como senha

A tabela de segredos define criptografia com `pgcrypto`, mas a função que busca senhas aparentemente retorna `encrypted_value` diretamente. Confirmar se há decriptação real, rotação de chave, associação por cartão/fornecedor/contrato e tratamento seguro de tentativas.

### 4.9 Formatos anunciados não equivalem a formatos suportados

A interface aceita PDF, imagens, XLSX, CSV, DOC, DOCX, OFX e QIF. Verificar quais têm parser funcional. Não considerar formato suportado quando ele apenas cai em `Buffer.toString()` ou gera texto ilegível.

### 4.10 Captura por câmera ainda não é uma jornada real

Um input de arquivo não é suficiente. Verificar ausência de `capture`, `getUserMedia`, recorte, perspectiva, rotação, compressão, múltiplas páginas e confirmação antes do envio.

### 4.11 Recorrência parece baseada em palavras-chave ou documento isolado

Uma conta com competência mensal não deve gerar automaticamente uma recorrência. A recorrência precisa ser inferida por histórico, periodicidade, fornecedor, valor, conta/cartão, tolerância e confirmação humana.

### 4.12 Faturas e cartões existem como CRUD, não como inteligência operacional

Verificar se há importação real de fatura, extração de itens, identificação de parcelas, projeção de compras futuras, reconciliação de pagamento e cálculo de limite comprometido.

### 4.13 Dívidas existem como cadastro, não como estratégia de quitação

Verificar se os motores SAC, Price, amortização e quitação antecipada estão conectados às telas e aos cenários. O produto precisa comparar avalanche, bola de neve e cenários personalizados.

### 4.14 Relatórios são descritivos, não preditivos

Verificar se existem séries temporais, comparação mensal, drill-down, projeção de caixa, agenda futura, previsão de cartão e cenários de dívida zero.

### 4.15 MCP é orientado ao desenvolvedor

O MCP atual parece rodar via `stdio` dentro do VS Code, usando `SUPABASE_SECRET_KEY` e `LOCAL_USER_ID`. Verificar os riscos, a falta de identidade de dispositivo e a ausência de ferramentas orientadas à decisão financeira.

### 4.16 CI/CD não conclui o deploy web

O deploy do Vercel aparentemente continua como placeholder. Verificar staging, produção, migrations, secrets, observabilidade e promoção de ambientes.

### 4.17 Testes de integração podem certificar apenas CSVs fictícios

Testes que usam textos como “Conta de luz — R$ 245,50” não certificam:

- PDFs protegidos;
- PDFs escaneados;
- faturas reais de cartão;
- OFX;
- XLSX;
- comprovantes fotografados;
- contratos de empréstimo;
- extratos com centenas de linhas.

### 4.18 O documento de “estado real” pode superestimar o projeto

Revalidar cada item de `docs/specs/00-estado-real.md`. Corrigir o documento no mesmo ciclo do planejamento e remover qualquer `✅` sem evidência ponta a ponta.

---

## 5. Arquitetura-alvo obrigatória

A equipe pode refinar os detalhes, mas o desenho deve respeitar os limites abaixo.

## 5.1 Aplicação web no Vercel

Responsabilidades:

- autenticação;
- experiência responsiva;
- câmera e upload;
- cadastros financeiros;
- revisão humana;
- agenda de pagamentos;
- dashboards e gráficos;
- cenários financeiros;
- controle e observação do worker;
- histórico e auditoria;
- criação de jobs leves.

Não deve executar:

- OCR pesado;
- rasterização extensa de PDFs;
- varredura histórica do Gmail;
- parsing de centenas de páginas;
- modelos locais;
- loops longos;
- processamento de meses de dados.

Uploads grandes devem preferencialmente ir diretamente ao Supabase Storage por URL assinada, evitando usar Server Actions do Vercel como túnel de arquivo.

## 5.2 Supabase como control plane

Responsabilidades:

- Auth;
- PostgreSQL;
- Storage;
- RLS;
- Realtime;
- filas duráveis;
- estado dos jobs;
- heartbeat dos workers;
- obrigações financeiras;
- evidências;
- drafts;
- materializações;
- projeções persistidas;
- auditoria.

## 5.3 Agente local

Criar um produto local único e operável, por exemplo:

```text
SeuBolsoFeliz.exe
SeuBolsoFeliz.bat
Instalar-SeuBolsoFeliz.ps1
```

Responsabilidades:

- Gmail;
- pasta local;
- OCR;
- PDFs protegidos;
- parsing pesado;
- processamento histórico;
- IA local ou remota;
- MCP;
- fila local;
- retry;
- retomada;
- logs;
- heartbeat;
- atualização.

O usuário deve conseguir iniciar a ferramenta por duplo clique, sem abrir três terminais nem conhecer Bun, Supabase CLI ou comandos internos.

## 5.4 Pipeline canônico único

Todos os canais devem convergir para as mesmas abstrações e serviços:

```text
Câmera ──────┐
Upload web ──┤
Gmail ───────┤
Pasta local ─┼→ SourceAdapter
MCP ─────────┘
                 ↓
            EvidenceEnvelope
                 ↓
        extração + OCR + parsing
                 ↓
           normalização
                 ↓
      fornecedor/produto/categoria
                 ↓
       financial_obligation
                 ↓
     conciliação e deduplicação
                 ↓
           revisão humana
                 ↓
 materialização transacional atômica
                 ↓
 agenda + caixa + cartões + dívidas
```

É proibido manter implementações paralelas e divergentes de scanner, deduplicação, upload, criação de jobs ou materialização para cada canal.

---

## 6. Entidades de infraestrutura a avaliar

Planejar pelo menos as seguintes entidades ou equivalentes:

### `worker_devices`

- `id`;
- `user_id`;
- nome da máquina;
- sistema operacional;
- versão do agente;
- capacidades instaladas;
- chave pública;
- status;
- último heartbeat;
- última sincronização;
- data de revogação.

### `worker_jobs`

- `id`;
- `user_id`;
- `worker_device_id` opcional;
- tipo de job;
- payload versionado;
- prioridade;
- status;
- progresso;
- etapa atual;
- lease;
- tentativas;
- erro;
- resultado;
- cancelamento;
- timestamps.

### `worker_job_events`

- linha do tempo do job;
- nível;
- etapa;
- mensagem;
- métricas;
- artefatos;
- tokens e custo de IA;
- contexto técnico sanitizado.

### Segurança do agente

O agente local não deve depender de uma `service_role` global distribuída. Planejar:

- matrícula de dispositivo;
- credencial revogável;
- escopo por usuário;
- RPCs específicas;
- chave pública/privada;
- rotação;
- Windows Credential Manager ou equivalente;
- proteção dos refresh tokens do Gmail;
- proteção das senhas de PDF;
- proteção das chaves de IA.

---

## 7. Jornadas verticais obrigatórias

O planejamento deve fechar, no mínimo, as jornadas abaixo.

## 7.1 Fotografia de despesa

```text
Câmera do celular
  → detecção e correção do documento
  → confirmação
  → upload direto
  → job
  → OCR
  → extração
  → draft editável
  → aprovação
  → materialização
  → transação e relatório
```

Planejar:

- `capture="environment"` e/ou `getUserMedia`;
- correção de perspectiva;
- rotação;
- compressão;
- múltiplas páginas;
- metadados;
- offline/retry;
- LGPD;
- confirmação visual.

## 7.2 Fatura protegida de cartão

```text
Gmail ou upload
  → identificação do cartão
  → resolução segura da senha
  → extração do ciclo
  → extração dos itens
  → identificação de parcelas
  → compras futuras
  → revisão
  → fatura, itens e transações
  → limite comprometido
```

Planejar perfis de senha por:

- instituição;
- cartão;
- final do cartão;
- fornecedor;
- contrato;
- padrão de documento.

## 7.3 Extrato bancário

```text
OFX/CSV/XLSX/PDF
  → parser específico
  → lançamentos
  → deduplicação
  → resolução de fornecedor
  → categorização
  → transferências internas
  → conciliação
  → revisão
  → transações
```

## 7.4 Varredura histórica do Gmail

```text
Período definido
  → estimativa do volume
  → execução supervisionada
  → paginação e checkpoint
  → pausa/retomada
  → deduplicação
  → anexos e corpo
  → processamento
  → revisão por lotes
```

A interface deve mostrar:

- período;
- mensagens encontradas;
- mensagens processadas;
- anexos;
- documentos criados;
- duplicatas;
- erros;
- pendências de senha;
- pendências de revisão;
- custo de IA;
- previsão baseada em throughput, sem promessas artificiais.

## 7.5 Agenda financeira

Uma única visão deve consolidar:

- contas;
- obrigações;
- faturas;
- parcelas de dívidas;
- recorrências;
- receitas previstas;
- pagamentos confirmados;
- itens atrasados;
- itens com dados incompletos;
- conflitos e duplicatas.

## 7.6 Planejamento de limites

Para cada cartão e mês:

- limite total;
- fatura aberta;
- fatura fechada;
- compras parceladas futuras;
- compras previstas;
- pagamentos previstos;
- limite comprometido;
- limite projetado após pagamento;
- risco de estouro;
- melhor cartão disponível para uma compra simulada.

## 7.7 Plano de quitação de dívidas

O produto deve permitir:

- cenário atual;
- avalanche;
- bola de neve;
- cenário personalizado;
- renda adicional;
- amortização extraordinária;
- comparação de juros;
- data projetada de quitação;
- impacto mensal no caixa;
- visualização do caminho até dívida zero.

---

## 8. Detecção de recorrência

Não usar apenas palavras-chave ou um único documento.

Planejar um motor de candidatos de recorrência baseado em:

- fornecedor canônico e aliases;
- descrição normalizada;
- cartão ou conta;
- categoria;
- intervalos entre eventos;
- tolerância de datas;
- tolerância de valores;
- padrão semanal, quinzenal, mensal, trimestral e anual;
- quantidade mínima de ocorrências;
- meses ausentes;
- cancelamentos;
- confiança;
- explicabilidade;
- confirmação humana.

A saída deve ser um **candidato de recorrência**, nunca um template definitivo criado silenciosamente.

---

## 9. Materialização e consistência

Projetar um contrato canônico e versionado para cada tipo de draft:

- transação;
- transferência;
- obrigação;
- ciclo de fatura;
- item de fatura;
- recorrência;
- parcela;
- dívida;
- métrica de consumo.

Requisitos obrigatórios:

- schemas compartilhados;
- nenhuma duplicação de nomes incompatíveis;
- conversão explícita entre DTOs;
- validação com Zod ou equivalente;
- transação de banco;
- idempotência;
- chave de materialização;
- locking ou controle de concorrência;
- auditoria;
- compensação em caso de falha;
- proibição de materializar `pending_review`;
- aprovação e materialização claramente distinguidas;
- estados intermediários visíveis;
- testes de dupla aprovação;
- testes de retry.

---

## 10. Fornecedores, categorias e produtos financeiros

Planejar resolução canônica de:

- fornecedor bruto;
- CNPJ;
- aliases;
- nome no extrato;
- nome na fatura;
- nome no boleto;
- instituição;
- cartão;
- conta;
- contrato;
- categoria;
- tags;
- prioridade.

Nenhum relatório confiável deve depender apenas de strings como `energia_eletrica` ou `NUBANK` sem associação ao respectivo ID canônico.

Planejar fila de resolução para casos ambíguos e aprendizado supervisionado a partir das decisões do usuário.

---

## 11. Relatórios e gráficos

O plano deve substituir resumos estáticos por uma camada analítica real.

Incluir:

- receitas e despesas por período;
- categorias;
- fornecedores;
- cartões;
- contas;
- evolução mensal;
- média móvel;
- recorrente versus variável;
- essencial versus discricionário;
- gastos novos;
- aumentos anormais;
- assinaturas não utilizadas;
- compras parceladas futuras;
- fluxo de caixa previsto;
- comparação previsto versus realizado;
- endividamento;
- juros pagos;
- evolução da quitação.

Os gráficos devem permitir drill-down até a evidência original.

---

## 12. MCP alvo

O MCP deve ser uma interface do mesmo domínio, não um segundo sistema.

Planejar ferramentas como:

- `get_worker_status`;
- `scan_gmail_period`;
- `scan_local_directory`;
- `list_jobs`;
- `cancel_job`;
- `retry_job`;
- `list_documents_needing_password`;
- `list_review_batches`;
- `review_obligation`;
- `list_next_payments`;
- `build_cash_forecast`;
- `analyze_card_limits`;
- `detect_recurring_candidates`;
- `simulate_debt_payoff`;
- `explain_classification`;
- `explain_reconciliation`.

Cada tool deve respeitar autenticação, usuário, auditoria, idempotência e limites de ação.

---

## 13. Segurança, privacidade e retenção

O planejamento deve abordar:

- RLS;
- segregação por usuário;
- princípio do menor privilégio;
- credenciais do worker;
- Gmail OAuth;
- refresh tokens;
- senhas de PDFs;
- chaves de IA;
- criptografia em trânsito e repouso;
- logs sem dados sensíveis;
- política de retenção dos originais;
- exclusão completa;
- exportação de dados;
- revogação de dispositivo;
- auditoria;
- backups;
- resposta a vazamento;
- LGPD.

Verificar migrations que criam policies permissivas para `service_role` e documentar por que cada acesso administrativo é necessário.

---

## 14. Observabilidade

Planejar dashboards e alertas para:

- documentos por status;
- jobs por status;
- duração por etapa;
- taxa de sucesso por parser;
- taxa de OCR;
- documentos protegidos;
- taxa de resolução de fornecedor;
- taxa de recorrência confirmada;
- duplicatas;
- retries;
- dead letters;
- custos e tokens de IA;
- worker online/offline;
- versão do worker;
- throughput de backfill;
- falhas por instituição ou formato.

---

## 15. Testes obrigatórios

Criar uma estratégia de testes em camadas.

### 15.1 Unitários

- normalização;
- datas;
- valores brasileiros;
- fornecedores;
- identidade financeira;
- recorrência;
- projeções;
- limites;
- amortização;
- schemas.

### 15.2 Contrato

- adapters;
- EvidenceEnvelope;
- draft DTO;
- materialização;
- job payload;
- MCP tools.

### 15.3 Integração

- Supabase local;
- Storage;
- filas;
- RLS;
- transações;
- idempotência;
- concorrência;
- retry;
- segredo criptografado.

### 15.4 Ponta a ponta

Usar documentos reais ou anonimizados representativos:

- fatura protegida;
- fatura sem senha;
- PDF escaneado;
- boleto;
- comprovante fotografado;
- OFX;
- CSV bancário;
- XLSX;
- extrato PDF;
- contrato de empréstimo;
- comprovante de pagamento;
- e-mail sem anexo;
- e-mail com link;
- duplicatas por canais diferentes.

### 15.5 Goldens

Criar um conjunto versionado e anonimizado de documentos com saída esperada para evitar regressões nos parsers.

---

## 16. Entregáveis do planejamento

Antes de iniciar a implementação ampla, produzir e versionar os seguintes documentos.

### 16.1 Diagnóstico do estado real

Arquivo sugerido:

```text
docs/planejamento/2026-07-27-estado-real-pos-auditoria.md
```

Deve conter:

- inventário das capacidades;
- evidência por arquivo, teste ou execução;
- classificação: pronto, parcial, quebrado, inexistente;
- divergências documentais;
- dívida técnica;
- riscos críticos;
- mapa de dependências.

### 16.2 Arquitetura-alvo

Arquivo sugerido:

```text
docs/arquitetura/2026-07-27-arquitetura-hibrida-alvo.md
```

Deve conter:

- diagramas C4;
- componentes;
- responsabilidades;
- limites Vercel/Supabase/local;
- fluxos;
- segurança;
- filas;
- contratos;
- sequência de jobs;
- falhas e recuperação;
- ADRs necessários.

### 16.3 Plano mestre

Arquivo sugerido:

```text
docs/planejamento/2026-07-27-plano-mestre-operacionalizacao.md
```

Deve conter:

- fases;
- marcos;
- dependências;
- riscos;
- estratégia de migração;
- estratégia de testes;
- estratégia de rollout;
- critérios de aceite;
- Definition of Done;
- itens fora de escopo;
- decisões que dependem do CEO.

### 16.4 Backlog hierárquico

Arquivo sugerido:

```text
docs/backlog/2026-07-27-backlog-operacionalizacao.md
```

Estruturar em:

```text
Épico
  → Feature
      → Issue
          → subtarefas
          → critérios de aceite
          → testes
          → dependências
          → riscos
```

Criar os épicos e issues no GitHub somente depois que a hierarquia estiver coerente. Não criar dezenas de issues genéricas sem dependências e critérios.

### 16.5 Matriz de rastreabilidade

Arquivo sugerido:

```text
docs/qualidade/2026-07-27-matriz-jornadas-requisitos-testes.md
```

Para cada requisito, informar:

- jornada;
- componente;
- issue;
- teste;
- evidência;
- ambiente;
- status.

---

## 17. Priorização mínima

A equipe deve propor estimativas e dependências, mas respeitar a ordem lógica abaixo.

## P0 — Verdade e integridade do núcleo

- corrigir documentação de estado real;
- unificar schemas de draft;
- fechar aprovação e materialização;
- garantir transação e idempotência;
- ligar obrigação financeira ao pipeline;
- corrigir segredos;
- corrigir orquestração;
- corrigir Gmail query/período;
- criar testes ponta a ponta mínimos.

## P1 — Três jornadas verticais

- foto de comprovante;
- fatura protegida;
- extrato bancário.

## P2 — Agente local como produto

- bootstrap;
- instalador;
- serviço/bandeja;
- matrícula;
- heartbeat;
- fila;
- logs;
- retry;
- supervisão web;
- MCP.

## P3 — Inteligência financeira

- agenda;
- projeção de caixa;
- recorrências;
- cartões e limites;
- plano de dívidas;
- cenários.

## P4 — Escala e refinamento

- aprendizado supervisionado;
- novos fornecedores;
- melhoria de IA;
- performance;
- custos;
- observabilidade avançada;
- experiência mobile refinada.

---

## 18. Critérios de aceite executivos

O planejamento deve culminar em marcos demonstráveis.

### Marco 1 — Um documento fecha o ciclo

Dado um documento real, o sistema:

1. ingere;
2. extrai;
3. identifica fornecedor;
4. cria obrigação;
5. permite correção;
6. aprova;
7. materializa uma única vez;
8. atualiza agenda e relatório.

### Marco 2 — Fatura de cartão utilizável

Dada uma fatura real protegida:

1. resolve a senha com segurança;
2. identifica o cartão;
3. extrai ciclo e vencimento;
4. extrai itens;
5. identifica parcelas;
6. projeta parcelas futuras;
7. calcula limite comprometido;
8. reconcilia o pagamento.

### Marco 3 — Backfill supervisionado

Dado um período de seis meses:

1. Gmail é paginado;
2. há checkpoint;
3. a execução pode pausar e retomar;
4. duplicatas não viram despesas duplicadas;
5. erros são isolados;
6. progresso aparece na web;
7. nenhuma materialização ocorre sem política de revisão definida.

### Marco 4 — Planejamento financeiro

O sistema apresenta:

- próximos pagamentos;
- saldo projetado;
- cartões e limites futuros;
- dívidas;
- estratégia de quitação;
- cenário de receita adicional;
- data projetada de dívida zero.

---

## 19. Definition of Done

Uma issue somente pode ser concluída quando:

- código implementado;
- migrations aplicáveis e reversíveis;
- tipos atualizados;
- testes relevantes passam;
- logs e erros são adequados;
- segurança revisada;
- documentação atualizada;
- critérios de aceite demonstrados;
- evidência anexada;
- nenhuma informação sensível foi commitada;
- comportamento validado em ambiente definido;
- fluxo de retry/idempotência foi considerado;
- telemetria mínima existe;
- impacto nas jornadas foi registrado.

“Compila” não é Definition of Done.

---

## 20. Restrições

Não fazer agora:

- mais telas desconectadas;
- mais tabelas sem fluxo;
- mais workers paralelos;
- mais documentação otimista;
- mais formatos anunciados sem parser;
- dashboards baseados em dados não materializados;
- automação irrestrita sem revisão;
- uso distribuído de `service_role`;
- refatoração estética que não ajude uma jornada real;
- criação de recorrência definitiva a partir de uma ocorrência;
- criação massiva de issues antes da arquitetura e dependências.

---

## 21. Formato da resposta da equipe

Ao concluir este trabalho de planejamento, apresentar:

1. resumo executivo franco;
2. estado real comprovado;
3. principais falhas;
4. arquitetura-alvo;
5. decisões arquiteturais;
6. plano faseado;
7. backlog por épico/feature/issue;
8. dependências;
9. riscos;
10. migrações;
11. testes;
12. marcos demonstráveis;
13. estimativas por complexidade, não por promessa arbitrária de prazo;
14. questões que dependem exclusivamente do CEO;
15. lista de arquivos criados ou alterados.

Não encerrar com frases vagas como “o projeto está bem encaminhado”. Informar de maneira objetiva:

- o que funciona;
- o que parece funcionar;
- o que está quebrado;
- o que falta;
- qual é o caminho crítico;
- qual é a primeira jornada que ficará utilizável.

---

## 22. Comando final

Façam uma auditoria completa da branch `feat/api-key`, validem cada constatação, corrijam a documentação enganosa e produzam o plano mestre com arquitetura, fases, backlog, testes, riscos e critérios de aceite.

Não esperem que o CEO desenhe as integrações restantes. Assumam a responsabilidade técnica por fechar as pontas.

O produto só será considerado em evolução quando começar a responder, com dados rastreáveis e confiáveis:

- **O que eu preciso pagar?**
- **Quando eu preciso pagar?**
- **Quanto dinheiro faltará ou sobrará?**
- **Quanto do meu limite está comprometido?**
- **Quais gastos estão se repetindo?**
- **Qual dívida devo atacar primeiro?**
- **Em que cenário eu consigo chegar a dívida zero?**
