====================================================================================================================================================================================

Time, a minha consultora tá possessa, eu também tô, e precisamos urgentemente destravar:
Olha o que ela mandou:

```markdown
# Prompt para o time — Refino obrigatório com plano de ação concreto

Time,

quero um **refino técnico, objetivo e executável**, sem resposta vaga, sem arquitetura bonita sem fechamento operacional, sem “podemos fazer depois”.

## Diagnóstico que passa a valer

O último raio-x deixou claro que hoje:

- a **UI de ingestão não existe**;
- o **deploy web real ainda é placeholder**;
- o **Google OAuth em staging/produção não está fechado**;
- o **scanner Gmail existe, mas não foi validado em escala real**;
- o **pipeline de ingestão já existe**, mas está sem fechamento de uso real;
- o **MCP está funcional**, mas ainda incompleto para Gmail e uso real com Copilot/ChatGPT;
- o **parser OpenAI** ainda não foi integrado;
- o **mobile não é prioridade agora**.

Portanto, o foco oficial do projeto agora é:

> **tirar a ingestão do limbo e colocá-la na interface, em staging, com dados reais, revisão humana, trilha de auditoria e integração com OpenAI/ChatGPT como copiloto operacional do fluxo.**

Não quero mais discussão abstrata sobre MVP. Quero **fluxo funcionando**.

---

## O que vocês precisam me entregar neste refino

Quero um documento que devolva **um plano de ação faseado, com passos verificáveis em staging**, contendo obrigatoriamente:

1. Fases em ordem de execução
2. Objetivo de cada fase
3. Entregáveis concretos por fase
4. O que sobe em staging ao final de cada fase
5. Como eu testo cada fase
6. Critérios de aceite por fase
7. Riscos, bloqueadores e dependências minhas
8. Quem faz o quê
9. O que será reaproveitado do que já existe
10. O que precisa ser criado do zero
11. Estimativa por fase
12. O que está fora de escopo agora

Não aceito resposta vaga. Cada fase precisa me permitir dizer:

- “isso já está visível em staging”
- “isso eu consigo testar”
- “isso gera evidência”
- “isso destrava a etapa seguinte”

---

## Objetivo principal agora

Fechar e provar, em staging, o seguinte fluxo ponta a ponta:

1. eu subo um documento manualmente pela interface **ou** via chat;
2. o arquivo vai para o **Supabase Storage**;
3. o sistema cria os registros de ingestão e processamento;
4. o worker processa o documento;
5. a IA ajuda a identificar:
   - fornecedor;
   - tipo de documento;
   - campos extraíveis;
   - possíveis registros financeiros;
   - conciliação com registros existentes;
   - necessidade de intervenção humana;
6. eu vejo isso numa **UI de ingestão/revisão**;
7. eu consigo corrigir manualmente;
8. eu consigo aprovar ou rejeitar;
9. os registros aprovados passam a valer no sistema;
10. isso fica auditado;
11. o dashboard reflete os dados aprovados.

Se isso não estiver fechado, o sistema continua incompleto para uso real.

---

## Entregas obrigatórias do refino

### Fase A — Base operacional mínima para staging

Detalhem como será fechado o mínimo para eu testar fora do localhost:

- deploy web real na Vercel;
- staging funcional de ponta a ponta;
- Google OAuth em staging;
- secrets remotos organizados;
- execução segura dos workers em staging;
- observabilidade mínima;
- logs mínimos para eu saber se um documento entrou, falhou, ficou pendente ou foi aprovado.

**Saída obrigatória da fase:** staging acessível, autenticação funcional e pipeline observável.

---

### Fase B — UI de ingestão mínima, mas usável

Quero sair do zero em ingestão visual.

A fase precisa contemplar no mínimo:

- página `/dashboard/ingestion` com visão geral;
- listagem de documentos ingeridos;
- filtros por status, origem, fornecedor, tipo e período;
- detalhe do documento;
- visualização do PDF/arquivo;
- visualização dos drafts extraídos;
- aprovação individual;
- rejeição;
- reprocessamento;
- indicação clara de erro, pendência, falta de senha e baixa confiança;
- split-view documento x registros propostos;
- edição manual dos campos antes da aprovação.

Eu preciso olhar um documento e enxergar:

- arquivo original;
- texto extraído ou resumo de extração;
- fornecedor sugerido;
- tipo sugerido;
- registros que ele vai gerar;
- confiança da extração;
- o que já foi conciliado;
- o que precisa da minha decisão.

**Saída obrigatória da fase:** tela funcional em staging para revisar e aprovar documentos reais.

---

### Fase C — Integração OpenAI / ChatGPT dentro do Seu Bolso Feliz

Quero uma proposta **concreta e segura** de integração com OpenAI/ChatGPT dentro do app.

Essa integração não deve virar fluxo paralelo. Ela deve ser **a interface inteligente do pipeline já existente**.

Quero que vocês me devolvam:

#### 1. Arquitetura proposta

Definam claramente:

- onde fica a conversa;
- se será página dedicada, drawer lateral ou assistente persistente;
- como o frontend conversa com o backend;
- como o backend conversa com OpenAI;
- como o backend aciona o pipeline de ingestão existente;
- como o backend usa o MCP interno sem expor risco no cliente;
- como ficam autenticação, autorização, auditoria e rate limiting.

#### 2. Casos de uso obrigatórios do chat

O chat precisa ser capaz de:

- receber upload de documento no próprio chat;
- mandar esse documento para o Storage e pipeline interno;
- consultar documentos já ingeridos;
- resumir o que foi encontrado;
- sugerir fornecedor;
- sugerir tipo documental;
- sugerir campos a extrair;
- sugerir conciliação com registros já existentes;
- pedir confirmação humana quando houver ambiguidade;
- explicar por que classificou algo de determinada forma;
- listar pendências de ingestão;
- listar documentos com erro;
- listar documentos sem senha;
- listar documentos sem fornecedor resolvido;
- ajudar a aprovar em lote com segurança.

#### 3. Fora de escopo agora

Digam explicitamente o que **não** entra agora, para evitar escopo solto.

Exemplos: consultoria financeira ampla por chat, multiagentes sofisticados, mobile, automações secundárias.

**Saída obrigatória da fase:** especificação implementável da integração OpenAI/ChatGPT, com primeira versão testável em staging.

---

### Fase D — Padrões de documentos e memória operacional

Eu quero parar de reensinar o sistema toda vez.

Quero que vocês proponham como registrar **padrões de documentos** para reaproveitamento futuro, por exemplo:

- fatura de cartão de crédito;
- extrato bancário;
- conta de energia;
- boleto;
- fatura recorrente por fornecedor;
- conjunto de campos obrigatórios por tipo.

Quero que vocês proponham:

- onde isso será salvo;
- como será versionado;
- como será ligado a fornecedor/instituição/tipo documental;
- como isso afeta parser e IA;
- como eu corrijo um padrão;
- como impedir que padrão ruim contamine extrações futuras;
- como registrar feedback humano.

**Saída obrigatória da fase:** proposta de modelagem + fluxo operacional para memória de padrões documentais.

---

### Fase E — Reconciliação e associação inteligente

Quero que vocês proponham o fechamento da conciliação entre documento e registros financeiros.

O sistema deve ajudar a responder:

- este documento gera quais registros?
- já existe fornecedor correspondente?
- já existe transação parecida?
- já existe fatura relacionada?
- isso duplica algo já lançado?
- isso é lançamento novo, recorrência, consumo métrico, passivo ou pagamento de fatura?
- o que pode ser sugerido e o que precisa de confirmação humana?

Quero proposta para:

- heurísticas de conciliação;
- sinais usados pela IA;
- sinais usados por parser determinístico;
- política de confiança;
- política de bloqueio para autopost;
- interface de revisão da conciliação;
- como o MCP ajuda nessa etapa.

Também quero avaliação objetiva sobre ferramentas MCP adicionais, por exemplo:

- `scan_gmail_label`
- `scan_gmail_query`
- `scan_gmail_period`
- `get_document_details`
- `suggest_document_pattern`
- `register_document_pattern`
- `list_documents_by_pattern`
- `suggest_reconciliation`
- `approve_document`
- `reject_document`
- `reclassify_document`
- `attach_document_to_existing_record`

Não precisam aceitar esses nomes, mas precisam devolver **um conjunto concreto de ferramentas necessárias**.

**Saída obrigatória da fase:** desenho da camada de conciliação com UI + backend + MCP.

---

### Fase F — Ingestão por Gmail e pasta local sem enrolação

Organizem o plano para fechar de verdade os dois canais:

#### Gmail

Precisa haver plano claro para:

- scan por label;
- scan por query;
- scan por período;
- backfill histórico;
- anexos;
- idempotência;
- rate limiting;
- reprocessamento;
- acompanhamento em UI.

#### Pasta local

Precisa haver plano claro para:

- scan manual por comando;
- scan por diretório;
- watch mode, se entrar agora ou depois;
- filtros por extensão;
- hash/idempotência;
- upload para Storage;
- visibilidade em UI.

Eu não quero mais algo que “existe via MCP mas não aparece”. Quero caminho operacional claro.

**Saída obrigatória da fase:** plano para Gmail + pasta local com prioridade, ordem e critério de aceite.

---

### Fase G — Promoção e intercâmbio entre local, staging e produção

Quero uma forma **segura, rastreável e idempotente** de intercambiar dados e configurações entre ambientes.

Isso é importante para eu conseguir:

- preparar dados localmente;
- testar em staging;
- promover para produção quando fizer sentido;
- carregar padrões documentais e catálogos sem retrabalho manual.

Quero proposta concreta para um comando ou conjunto pequeno de comandos que façam promoção controlada entre:

- local → staging
- staging → produção

Mas não aceito “cópia cega” de banco.

Quero solução com:

- `dry-run`;
- escopos explícitos;
- promoção por tipo de entidade;
- idempotência;
- hash/verificação;
- auditoria;
- rollback quando aplicável;
- proteção contra lixo e duplicidade;
- estratégia para documentos, patterns, fornecedores, aliases, classificações e metadados;
- definição clara do que **pode** e do que **não pode** ser promovido.

**Saída obrigatória da fase:** proposta concreta de ferramenta de promoção entre ambientes.

---

## O que deve subir em staging para eu testar

No mínimo, quero um caminho assim:

### Marco 1 — Staging operacional

- login funcionando;
- deploy real;
- observabilidade mínima;
- autenticação e secrets fechados.

### Marco 2 — Ingestão visível

- documentos entram e aparecem na interface;
- filtros básicos;
- detalhe de documento;
- status e erros visíveis.

### Marco 3 — Revisão humana

- split-view;
- edição manual;
- aprovação/rejeição;
- reprocessamento;
- feedback de confiança.

### Marco 4 — IA acoplada ao fluxo

- chat funcional;
- upload pelo chat;
- explicação da classificação;
- sugestão de fornecedor, tipo, campos e conciliação.

### Marco 5 — Padrões e reaproveitamento

- registro de tipo/template documental;
- reaproveitamento em novos documentos;
- feedback humano persistido.

### Marco 6 — Promoção controlada entre ambientes

- comando seguro;
- `dry-run`;
- escopos claros;
- auditoria.

---

## Restrições obrigatórias

1. Nada de criar fluxo paralelo fora do pipeline existente.
2. Nada de IA decidindo tudo sozinha sem revisão humana nas fases iniciais.
3. Nada de esconder ingestão em ferramenta técnica sem UI.
4. Nada de dizer que está pronto sem evidência em staging.
5. Nada de empurrar o problema para o mobile agora.
6. Nada de sync cego entre ambientes.
7. Nada de resposta só arquitetural. Quero backlog prático.
8. Nada de resposta sem critérios de aceite.
9. Nada de depender exclusivamente de OpenAI para tudo. Quero parser determinístico + IA + revisão humana.
10. Nada de esconder bloqueadores meus. Quero lista explícita do que depende de mim.

---

## Quero também um backlog de implementação logo após o refino

Ao final do documento, além do refino, quero um **backlog priorizado** com colunas como:

- ID
- Tarefa
- Tipo (frontend/backend/worker/mcp/infra/db/qa)
- Prioridade
- Dependências
- Responsável sugerido
- Estimativa
- Marco de staging
- Critério de aceite

Não quero backlog genérico. Quero backlog que dê para começar a executar.

---

## Estrutura mínima da resposta de vocês

1. Resumo executivo
2. Diagnóstico consolidado
3. Decisões de arquitetura para esta fase
4. Fases de execução
5. Entregáveis por fase
6. O que sobe em staging por fase
7. Como o CEO testa cada fase
8. Critérios de aceite por fase
9. Proposta de integração OpenAI/ChatGPT
10. Proposta de expansão do MCP
11. Proposta de padrões documentais
12. Proposta de conciliação
13. Proposta de promoção entre ambientes
14. Backlog priorizado
15. Bloqueadores do CEO
16. Itens fora de escopo agora

---

## Fechamento

O objetivo deste refino não é escrever documento bonito.

O objetivo é me devolver um plano que **pare de me enrolar na ingestão** e me permita começar a usar o Seu Bolso Feliz para organizar minhas finanças com documentos reais, revisão assistida por IA e validação em staging.

Quero resposta madura, concreta, honesta e implementável.
```

Ou seja:

- Criem um documento de refino que responda a tudo isso, e que me permita ter clareza do estado do projeto, do que foi feito, do que falta, e do que é prioridade
- Um checklist de implementação claro, com tarefas pequenas, claras e verificáveis, para que a gente possa começar a executar imediatamente e que irei acompanhar
- ADRs novas ou atualização das decisões de arquitetura mais importantes, para que a gente tenha registro do porquê de cada decisão, e para que a gente possa ter clareza do estado do projeto, do que foi feito, do que falta, e do que é prioridade
- Criar um documento unificado de passo-a-passos consolidando as já existentes e adicionando coisas que o CEO precise fazer, para que a gente tenha um documento claro e atualizado do que o CEO precisa fazer, e para que a gente possa ter clareza do estado do projeto, do que foi feito, do que falta, e do que é prioridade
  - Ex.:
    - como pegar credenciais do Gmail,
    - como configurar o MCP local,
    - como rodar os workers localmente,
    - como usar o GitLab e o Vercel,
    - como promover para staging,
    - como promover para produção, etc
    - credenciais do ChatGPT, etc
- Outros documentos que julgar necessários para garantir que a gente tenha clareza do estado do projeto, do que foi feito, do que falta, e do que é prioridade
- A INTEGRAÇÃO COM IA TEM QUE SER TOTAL
  - Chat na interface para eu conversar, orientar, mandar ter registros, padrões, memória, etc
  - IA sugerindo fornecedor, tipo, campos, conciliação
  - IA explicando o que ela entendeu do documento, o que ela extraiu, o que ela classificou, o que ela conciliou, o que ela não entendeu, o que ela acha que precisa de revisão humana, etc
  - IA ajudando a revisar, aprovar, rejeitar, dar feedback, etc
  - IA ajudando a criar padrões documentais, a aplicar padrões documentais, a revisar padrões documentais, a dar feedback sobre padrões documentais, etc
  - IA ajudando a conciliar, a revisar conciliações, apropor conciliações, a dar feedback sobre conciliações, etc
  - IA ajudando a promover entre ambientes, a revisar o que vai ser promovido, a dar feedback sobre o que vai ser promovido, etc

NÃO CODAR NADA ANTES DE ENTREGAR O DOCUMENTO DE REFINO COM O PLANO DE AÇÃO CONCRETO, E ANTES DE ALINHAR COMIGO SOBRE O DOCUMENTO DE REFINO, PARA GARANTIR QUE A GENTE ESTEJA NA MESMA PÁGINA SOBRE O ESTADO DO PROJETO, SOBRE O QUE FOI FEITO, SOBRE O QUE FALTA, E SOBRE O QUE É PRIORIDADE.

==========================================================================================================================================================================================================

Tente novamente, continue do ponto que parou, criando o documento em pedaços menores para evitar o erro `Motivo: Please check your firewall rules and network connection then try again. Error Code: terminated: HTTP/2: "stream timeout after 300000".`

---

==========================================================================================================================================================================================================

Veronica tá atacada:

```bash
Time,

**aprovado com ressalvas**.

Eu **aprovo para execução imediata** o que está descrito para:

- **Marco 1**
- **Marco 2**
- **Marco 3**
- **preparação arquitetural do Marco 4**

A direção geral ficou boa e, pela primeira vez, o material está utilizável como base de execução.

## Antes de avançar para o restante, quero correções obrigatórias **nos documentos já existentes**, sem criar novos documentos

Façam os ajustes **editando os arquivos que já foram produzidos**, mantendo versionamento e rastreabilidade, e **sem abrir uma nova documentação paralela**.

### Correções obrigatórias

1. **Antecipar o upload manual via UI**
   - Mover o upload manual de documentos pela interface para **Marco 2 ou Marco 3**.
   - Isso é prioridade e não pode ficar só no Marco 5.

2. **Reduzir dependência de operação local para staging**
   - Deixar claro como vou testar ingestão em staging sem depender de fluxo solto local.
   - Quero pelo menos uma forma objetiva e centralizada de disparar scan/reprocessamento para staging.

3. **Corrigir inconsistência de modelagem em padrões documentais**
   - Revisar a divergência entre referências como `job_id`, `document_id`, `ingested_documents`, `source_documents` e estruturas correlatas.
   - A modelagem precisa ficar consistente em todos os documentos.

4. **Corrigir a contradição sobre logging/observabilidade**
   - Hoje o material trata logging como item inicial em um ponto e pós-Marco 6 em outro.
   - Unifiquem a decisão e deixem isso coerente.

5. **Explicitar a matriz de decisão documental**
   - Quero claramente definido:
     - que tipo de documento gera qual tipo de registro;
     - quando vira transação;
     - quando vira recorrência;
     - quando vira fatura;
     - quando vira passivo;
     - quando vira apenas documento de apoio.

6. **Esclarecer promoção entre ambientes**
   - Deixar explícito o que pode e o que não pode ser promovido entre local, staging e produção.
   - Também quero a alternativa prevista para documentos/drafts, caso eles não sejam promovíveis.

## Regra importante

**Não criem novos documentos para responder isso.**
Quero essas correções **nos documentos que vocês já entregaram**.

## Entrega esperada

Na devolutiva, quero apenas:

- confirmação objetiva do que foi corrigido;
- lista dos documentos já existentes que foram atualizados;
- resumo curto das decisões ajustadas.

Sem documento novo. Sem texto ornamental. Sem reabrir discussão conceitual.
```

- Ajusta nas documentações criadas hoje tudo que ela pediu
- Não crie documento novo, ajuste os que já existem
- NÃO CODAR NADA ANTES DE ENTREGAR O DOCUMENTO DE REFINO COM O PLANO DE AÇÃO CONCRETO, E ANTES DE ALINHAR COMIGO SOBRE O DOCUMENTO DE REFINO, PARA GARANTIR QUE A GENTE ESTEJA NA MESMA PÁGINA SOBRE O ESTADO DO PROJETO, SOBRE O QUE FOI FEITO, SOBRE O QUE FALTA, E SOBRE O QUE É PRIORIDADE.

Conto com vocês!

==========================================================================================================================================================================================================

Veronica mandou um feedback importante sobre os documentos:

```markdown
Time,

A revisão melhorou bastante e **eu libero o início imediato do desenvolvimento** para:

- **Marco 1**
- **Marco 2**
- **Marco 3**
- **preparação arquitetural do Marco 4**

As principais ressalvas anteriores foram atendidas:

- upload manual via UI foi antecipado;
- logging/observabilidade foi trazido para o início;
- staging ficou mais testável;
- a matriz documental foi explicitada;
- a promoção entre ambientes ficou mais clara.

## Ressalva final obrigatória

Antes de eu considerar o pacote documental **integralmente aprovado**, alinhem o **ADR-006** ao **refino principal**, porque ainda há divergência de modelagem e nomenclatura em:

- `document_patterns`
- `pattern_feedback`

Hoje o ADR e o refino principal ainda não descrevem exatamente a mesma implementação.

## Diretriz

- **Podem começar a desenvolver agora** o que já está liberado acima.
- **Não abram nova documentação** para isso.
- Façam apenas o **ajuste no ADR-006**, alinhando-o ao refino principal.
- Na devolutiva, quero só a confirmação objetiva de que esse alinhamento foi feito.

Vamos destravar a execução sem perder consistência.
```

Chamem o time e formulem um plano para ajustar o ADR-006, alinhando a modelagem de `document_patterns` e `pattern_feedback` com o que está descrito no refino principal, garantindo que ambos os documentos estejam coerentes e descrevam a mesma implementação.

---

TIME, liberado CODAR!!

- Unico ponto da Veronica: Minha única observação prática é esta: tratem a versão mais nova como a canônica, porque ainda existem versões antigas desses arquivos no histórico com modelagem anterior (supplier_match, content_match, document_id, job_id, etc.). Isso não invalida a revisão atual, mas vale evitar que alguém implemente olhando o arquivo antigo por engano.
- fonte da verdade: a versão mais nova desses arquivos, que tem modelagem consistente entre si e com o refino principal. O histórico tem versões antigas com modelagem divergente, mas a versão mais nova é a que deve ser seguida.
- Criem um novo checklist, em `docs/checklists/<<data-sem-hora>>-implementacao-plano-acao.md`, com as todas as tarefas de implementação e de checagem, priorizadas e numeradas, para que a gente possa começar a executar imediatamente. O checklist deve ser detalhado o suficiente para que cada tarefa seja pequena, clara e verificável, e para que a gente possa acompanhar o progresso.
  - CRITÉRIOS DE ACEITE:
    - Interface com:
      - upload manual de documentos;
      - revisão de documentos ingeridos;
      - integração com IA para sugestão de fornecedor, tipo, registros, campos e conciliação;
    - Checklist completo, com tarefas pequenas, claras e verificáveis, e com critérios de aceite claros para cada tarefa
    - Passo-a-Passo do que preciso fazer para:
      - oNDE ESTÁ CADA COISA E COMO USAR
      - Testar o app localmente
      - Testar o app em staging, conseguindo mover dados de local para staging
      - Promover para produção, conseguindo mover dados de staging para produção
- Ao final de cada ciclo de prompt que eu fizer, quero:
  - um resumo do que foi feito, do que falta, e do que é prioridade
  - um link para o checklist atualizado
  - um link para o passo-a-passo atualizado
- Chamem o time, instruam eles sobre o que fazer, e formulem um plano para entregar o checklist e o passo-a-passo, garantindo que ambos estejam completos, claros e atualizados.
- faça commits pequenos, claros e frequentes, para que a gente possa acompanhar o progresso e ter rastreabilidade. PUSH por minha conta

VAMOS CODAR!!! CONTO COM VOCÊS!!

---

Vamos continuar

- Atualizar o #2026-04-01-implementacao-plano-acao com o que foi feito
- faça commits pequenos, claros e frequentes, para que a gente possa acompanhar o progresso e ter rastreabilidade. PUSH por minha conta
- Ao final de cada ciclo de prompt que eu fizer, quero:
  - um resumo do que foi feito, do que falta, e do que é prioridade
  - um link para o checklist atualizado
  - um link para o passo-a-passo atualizado

---

Ao fazer os commits, levar em conta os erros dos hooks do husky:

```bash
❯ git save-n "Chico: Evoluções do marco 4"
✔ Backed up original state in git stash (a346507)
✔ Running tasks for staged files...
✔ Applying modifications from tasks...
✔ Cleaning up temporary files...
⧗   input: Chico: Evoluções do marco 4
✖   subject must not be sentence-case, start-case, pascal-case, upper-case [subject-case]
✖   type must be lower-case [type-case]
✖   type must be one of [feat, fix, docs, style, refactor, test, chore, ci, perf, migration] [type-enum]

✖   found 3 problems, 0 warnings
ⓘ   Get help: https://github.com/conventional-changelog/commitlint/#what-is-commitlint

husky - commit-msg script failed (code 1)
fatal: invalid refspec ''
```

---

Vamos continuar a execução do plano de ação #2026-04-01-implementacao-plano-acao e #2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging :

- Atualizar o #2026-04-01-implementacao-plano-acao com o que foi feito
- faça commits pequenos, claros e frequentes, para que a gente possa acompanhar o progresso e ter rastreabilidade. PUSH por minha conta
- Ao final de cada ciclo de prompt que eu fizer, quero:
  - um resumo do que foi feito, do que falta, e do que é prioridade
  - um link para o checklist atualizado
  - um link para o passo-a-passo atualizado
    Vamos evoluir!

---

TODO:

Vamos continuar a execução do plano de ação #file:2026-04-01-implementacao-plano-acao.md e #file:2026-03-31-19-40-refino-plano-acao-ingestao-ia-staging.md :

- Atualizar o #file:2026-04-01-implementacao-plano-acao.md com o que foi feito
- faça commits pequenos, claros e frequentes, para que a gente possa acompanhar o progresso e ter rastreabilidade. PUSH por minha conta
- Ao final de cada ciclo de prompt que eu fizer, quero:
  - um resumo do que foi feito, do que falta, e do que é prioridade
  - um link para o checklist atualizado
  - um link para o passo-a-passo atualizado
    Vamos evoluir!
