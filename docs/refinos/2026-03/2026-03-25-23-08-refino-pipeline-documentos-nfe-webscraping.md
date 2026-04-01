---
Título da Reunião: Pipeline Completo de Ingestão — NFe XML/PDF, Webscraping e Visibilidade no Portal
Data e Hora: 2026-03-25 23:08
Participantes:
  - CEO (Chico) — facilitador, demandante
  - Ana Silva (Arquiteta de Software) — visão arquitetural pipeline + webscraping
  - João Pereira (Backend Sênior / Bun) — implementação workers, parsers, MCP
  - Maria Oliveira (Backend Sênior) — testes, segurança, idempotência
  - Pedro Santos (Backend Sênior / Python) — parsers, IA futura, OpenAI
  - André Santos (DBA PostgreSQL) — modelagem dados, performance queries
  - Ricardo Monteiro (Economista) — regras de negócio financeiro, validação de campos NFe
  - Roberto Lima (Frontend Sênior) — UI de ingestão, portal web
  - Camila Duarte (Consultora Finanças Pessoais) — perspectiva do usuário final
  - Fernando Gomes (DevOps) — infraestrutura, segurança, ambientes
Pauta:
  - Pergunta 1: "Era para eu ver esses documentos no http://localhost:3105?"
  - Pergunta 2: "O que vamos fazer agora? Extraímos documentos, e agora?"
  - Pergunta 3: "Como isso vira dado no portal? Como vira registro?"
  - Pergunta 4: Necessidade de webscraping para e-mails com links para boletos
  - Pergunta 5: Necessidade de leitor de NFe XML e PDF para coletar dados
  - Decisões técnicas e roadmap de implementação
---

# Refino: Pipeline de Ingestão → Portal — NFe, Webscraping, Visibilidade

## 1. Contexto e Motivação

O CEO levantou 6 perguntas críticas após a conclusão da fase de scan do Gmail (90 documentos ingeridos, dedup por SHA-256 funcionando, 0 erros em 3 execuções de 300 e-mails). Os documentos estão no banco mas não aparecem na interface web, e o pipeline intermediário não foi executado. Além disso, foram identificados dois gaps importantes:

1. **NFe XML/PDF** — Muitos e-mails contêm notas fiscais em XML (padrão SEFAZ) e PDF que precisam de parser específico
2. **Webscraping** — Alguns e-mails contêm apenas links para páginas externas onde o boleto/segunda-via está disponível

### Estado Atual do Sistema (25/03/2026)

| Componente                   | Status           | Dados                               |
| ---------------------------- | ---------------- | ----------------------------------- |
| Gmail Scanner                | ✅ Operacional   | 90 documentos, 7 runs, 0 duplicatas |
| Worker Ingestion (poll loop) | ✅ Implementado  | Nunca executado com dados reais     |
| Parsers (CEMIG, Boleto)      | ✅ Implementados | 47 testes passando                  |
| Draft Generator              | ✅ Implementado  | 15 testes passando                  |
| MCP Server                   | ✅ 8 tools       | Operacional via VS Code             |
| source_documents             | 90 registros     | Todos em status `new`               |
| ingestion_jobs               | 90 registros     | Todos em status `DISCOVERED`        |
| parsed_document_versions     | 0                | Worker não executou                 |
| extraction_results           | 0                | Worker não executou                 |
| draft_records                | 0                | Worker não executou                 |
| Web UI de ingestão           | ❌ Não existe    | Fase 6 do plano                     |
| Parser NFe XML               | ❌ Não existe    | Novo — não planejado                |
| Parser NFe PDF               | ❌ Não existe    | Novo — não planejado                |
| Webscraping                  | ❌ Não existe    | Novo — não planejado                |

---

## 2. Discussão

### Pergunta 1 — "Era para eu ver esses documentos no localhost:3105?"

**Roberto Lima (Frontend):**

> Não. A interface web atual (`apps/web`) tem 26 rotas financeiras (dashboard, instituições, produtos, cartões, transações, recorrências, faturas, passivos, fornecedores, configurações). Nenhuma dessas rotas mostra documentos ingeridos. O dashboard exibe: receita/despesas do mês, fila de prioridade, recorrências próximas e faturas em aberto — tudo do ledger financeiro, não da camada de ingestão. A UI de ingestão é a **Fase 6 inteira** do nosso plano, com 15 tarefas (rotas E.1 a E.15 no checklist 004).

**Ana Silva (Arquiteta):**

> Essa separação é intencional. O portal web consulta o **ledger financeiro** (transactions, recurring_templates, etc.), enquanto os documentos ingeridos vivem nas **tabelas de ingestão** (source_documents, ingestion_jobs, parsed_document_versions, draft_records). Eles só se encontram quando um draft é **aprovado** e vira registro financeiro real. Por enquanto, o caminho para ver e aprovar drafts é via **MCP** (tools `list_draft_batches`, `approve_draft_batch`).

**CEO:**

> Então no MVP, o processo é: scan → worker → parsers → drafts → aprovar via MCP → vira dado financeiro no portal?

**Ana:**

> Exatamente. A UI de revisão (Fase 6) é a versão "bonita" disso. Mas o fluxo funcional já existe: MCP como interface provisória até a web de revisão estar pronta.

### Pergunta 2 — "O que vamos fazer agora? Extraímos documentos, e agora?"

**João Pereira (Backend):**

> A próxima ação imediata é executar o **worker de ingestão** nos 90 jobs existentes. Esses jobs foram criados pelo Gmail Scanner no status `DISCOVERED`. O worker vai:
>
> 1. **DISCOVERED → DOWNLOADED**: Baixar o arquivo do Supabase Storage
> 2. **DOWNLOADED → HASHED**: Calcular SHA-256, verificar idempotência
> 3. **HASHED → QUEUED → PARSING → PARSED**: Extrair texto e rodar parsers (CEMIG, boleto, genérico)
> 4. **PARSED → CLASSIFIED → RECONCILED → DRAFTED**: Gerar drafts (transação, recorrência, consumo)
> 5. **DRAFTED → PENDING_REVIEW**: Pronto para revisão humana
>
> Comando: `bun run workers/ingestion/src/index.ts`
>
> O resultado: teremos `parsed_document_versions`, `extraction_results`, `draft_records` e `draft_batches` populados. Aí o CEO pode via MCP (`list_draft_batches`) ver os drafts e aprovar (`approve_draft_batch`).

**Maria Oliveira (Backend):**

> Importante: CLASSIFIED e RECONCILED são passthrough no MVP — avançam automaticamente sem matching de fornecedor ou reconciliação. Isso virá na Fase 7 com IA.

**Ricardo Monteiro (Economista):**

> Antes de executar, preciso chamar atenção: muitos dos 90 documentos provavelmente são NFes em XML ou PDF. Se o parser não souber extrair campos de NFe, o resultado vai ser um draft genérico com baixa confiança. O CEO mencionou que precisa de um leitor de NFe — e eu concordo. Os campos mínimos de uma NFe que precisamos extrair são: CNPJ emitente, CNPJ destinatário, número da nota, série, data de emissão, valor total, itens (descrição + valor unitário + quantidade + valor total), impostos (ICMS, IPI, PIS, COFINS) e chave de acesso. Sem isso, estamos perdendo a informação mais rica que os e-mails trazem.

### Pergunta 3 — "Como isso vira dado no portal? Como vira registro?"

**Ana Silva (Arquiteta):**

> O pipeline completo é:
>
> ```
> source_document (raw file)
>   → parsed_document_version (texto extraído + parser usado + confiança)
>     → extraction_result (dados estruturados: valor, vencimento, CNPJ, etc.)
>       → draft_record (rascunho tipado: transaction, recurring_template, consumption_metric)
>         → [REVISÃO HUMANA: MCP ou Web UI]
>           → registro financeiro real (INSERT em transactions, recurring_templates, etc.)
> ```
>
> O campo `posted_record_id` na `draft_records` faz o link reverso: depois de aprovado, referencia o registro criado no ledger.

**Pedro Santos (Backend):**

> Para ficar mais concreto: quando o CEO aprova um draft do tipo `transaction`, o sistema cria um registro na tabela `transactions` com todos os campos preenchidos (amount, description, due_date, category, tags, supplier_id, financial_product_id etc.). O draft fica com status `posted` e `posted_record_id` apontando para a transação criada. Da mesma forma para `recurring_template` (cria template de recorrência) e `consumption_metric` (registra consumo).

**Camila Duarte (Finanças Pessoais):**

> Do ponto de vista do usuário, o ideal é que quando ele acesse o dashboard, os dados já reflitam o que foi aprovado. Se o CEO escaneou 300 e-mails de contas de luz, internet e boletos, e aprovou todos — a fila de prioridade, os vencimentos, e os gastos do mês devem refletir isso imediatamente. O pipeline está desenhado para isso?

**João Pereira:**

> Sim. Assim que um draft vira `posted`, o registro financeiro real está no ledger e o dashboard já vai buscá-lo nas queries existentes (`upcomingDue`, `monthExpenses`, etc.). Não precisa de sync adicional.

### Pergunta 4 — Webscraping para e-mails com links

**CEO:**

> Para certos e-mails, preciso de webscraping. O e-mail não traz o boleto como anexo — traz um link para a página da empresa (ex: Copasa, Unimed, seguradoras). O sistema precisa seguir o link, com supervisão, encontrar o boleto, os dados da página e coletar dados. Previram isso?

**Ana Silva (Arquiteta):**

> Não estava no plano original. É uma necessidade real e legítima. Precisa ser tratada como uma **nova fonte de dados** no pipeline, não como extensão do Gmail Scanner. Proponho que seja um módulo separado: `workers/web-scraper/`.

**João Pereira (Backend):**

> Concordo com a Ana. O fluxo seria:
>
> 1. O Gmail Scanner detecta que a mensagem tem link e não tem anexo relevante
> 2. Cria um `source_document` do tipo `email_link` com o URL como metadado
> 3. O `ingestion_job` tem status `DISCOVERED` mas com `origin_type = 'email_link'`
> 4. O step de download, ao invés de buscar no Storage, aciona o **web scraper**
> 5. O web scraper navega à URL, encontra o PDF/boleto, baixa e deposita no Storage
> 6. Daí em diante, o pipeline é o mesmo: hash → parse → draft → review

**Maria Oliveira (Backend):**

> Atenção à segurança. Seguir URLs de e-mails é risco de SSRF (Server-Side Request Forgery). Precisamos de:
>
> - **Allowlist de domínios**: só seguir links de domínios conhecidos (copasa.com.br, unimed.com.br, etc.)
> - **Supervisão humana**: o scraper propõe a ação, o CEO confirma antes de navegar
> - **Sandbox**: execução isolada, sem acesso a rede interna
> - **Timeout**: limite de tempo e tamanho de resposta
> - **Rate limiting**: nunca martelar um site

**Pedro Santos (Backend):**

> Sugiro usar **Playwright** para o scraping. Muitos portais de concessionárias usam JavaScript pesado — `fetch` simples não funciona. Playwright roda headless Chrome, suporta login em portais (quando necessário), e pode fazer screenshot para auditoria. É a mesma lib que já usamos como dependência do ecossistema.

**Fernando Gomes (DevOps):**

> Se usar Playwright, precisa de Chromium instalado. Em produção, isso significa container com Chromium headless. Localmente funciona bem com `npx playwright install chromium`. O custo computacional é maior que um simples fetch, mas para uso supervisionado e esporádico é aceitável.

**Ana Silva (Arquiteta):**

> Proponho 3 modos de operação para o web scraper:
>
> 1. **Automático supervisionado**: Scraper propõe a URL + domínio está na allowlist → pede confirmação do CEO via MCP → executa → salva resultado
> 2. **Manual**: CEO cola a URL manualmente via MCP tool → scraper executa
> 3. **Learning mode**: Scraper tenta mas não salva; mostra o que encontraria para o CEO avaliar e cadastrar a regra
>
> No MVP, implementamos apenas o modo 1 (supervisionado) e modo 2 (manual).

**Ricardo Monteiro (Economista):**

> Do ponto de vista financeiro, os boletos de concessionárias acessados via link geralmente contêm: valor total, data de vencimento, código de barras/linha digitável, número do documento, e às vezes detalhamento de consumo. O parser de boleto que já temos captura boa parte disso. O scraper precisa baixar o PDF do boleto da página, e o parser existente cuida do resto.

**Camila Duarte (Finanças Pessoais):**

> Na prática, os principais fornecedores que enviam só link (sem anexo) são: Copasa, Unimed, seguradoras (Porto, Tokio, Bradesco Seguros), operadoras de celular (Vivo, Claro, Tim), e algumas lojas de parcelamento. Se a allowlist cobrir esses 10-15 domínios, já resolve 90% do problema.

### Pergunta 5 — Leitor de NFe XML e PDF

**CEO:**

> Precisamos de um leitor de NFe XML e PDF para coletar dados. Importante.

**Pedro Santos (Backend):**

> NFe XML é o caso mais limpo. O XML de NFe segue o **schema da SEFAZ** (nfeProc/NFe/infNFe). Os campos são padronizados e hierárquicos. Posso fazer um parser determinístico (regex/DOM) que:
>
> 1. Detecta se o XML é NFe pelo namespace `http://www.portalfiscal.inf.br/nfe`
> 2. Extrai: chave de acesso, número, série, data emissão, CNPJ/nome emitente, CNPJ/nome destinatário, valor total, lista de itens (descrição, quantidade, valor unitário, valor total), impostos desagregados (ICMS, IPI, PIS, COFINS, ISS)
> 3. Confiança alta (0.95+) por ser XML estruturado
>
> É o parser mais fácil de implementar e o mais rico em dados. Não precisa de IA — o XML já é dado estruturado.

**João Pereira (Backend):**

> Para o text-extractor, XML já é suportado — ele lê como UTF-8 text. O que falta é o **parser especializado de NFe** que entende a estrutura do XML. Sugiro criar `workers/ingestion/src/parsers/nfe-xml-parser.ts` seguindo o mesmo padrão do CEMIG e boleto: uma função `isNFe(text)` para detecção e `parseNFe(text)` para extração.

**Pedro Santos (Backend):**

> Para NFe em PDF, é mais complexo. As NFes em PDF (DANFE) são documentos visuais — o texto extraído via `pdf-parse` vem desorganizado. Os campos que precisamos estão lá mas em posições variáveis. Duas abordagens:
>
> 1. **Parser regex robusto**: Similar ao boleto, mas com patterns específicos para DANFE (chave de acesso de 44 dígitos, padrão de CNPJ na posição de emitente, tabela de itens)
> 2. **OpenAI Vision (Fase 7)**: Enviar imagem da DANFE para GPT-4 Vision com prompt calibrado
>
> No MVP, proponho o parser regex. Não vai capturar 100% dos campos, mas pega: chave de acesso, CNPJ emitente, valor total, data emissão, número da nota. Para itens individuais, defer para Fase 7.

**André Santos (DBA):**

> Para suportar NFe, precisamos modelar os itens da nota fiscal. A tabela `extraction_results` já tem campos `breakdown JSONB` e `consumption_data JSONB` que podem acomodar os itens. Mas sugiro definir um schema JSON padronizado para NFe:
>
> ```json
> {
>   "nfe_access_key": "35260325...",
>   "nfe_number": "123456",
>   "nfe_series": "1",
>   "emitter": {
>     "cnpj": "12.345.678/0001-90",
>     "name": "Empresa X Ltda",
>     "ie": "123456789"
>   },
>   "recipient": {
>     "cpf": "123.456.789-00",
>     "name": "Francisco..."
>   },
>   "items": [{ "description": "Produto A", "qty": 2, "unit_value": 15.9, "total": 31.8 }],
>   "taxes": {
>     "icms": 5.4,
>     "pis": 0.65,
>     "cofins": 3.0,
>     "ipi": 0.0
>   },
>   "total": 40.85,
>   "emission_date": "2026-03-20",
>   "payment_method": "credit_card"
> }
> ```
>
> Isso cabe no `structured_data JSONB` da `parsed_document_versions` e no `breakdown JSONB` da `extraction_results`.

**Ricardo Monteiro (Economista):**

> Excelente proposta do André. Do ponto de vista financeiro, os campos mais importantes da NFe são:
>
> 1. **Valor total** — para a transação
> 2. **Data de emissão** — para competência financeira
> 3. **CNPJ emitente** — para resolução de fornecedor
> 4. **Chave de acesso** — para deduplicação (uma NFe tem chave única de 44 dígitos)
> 5. **Itens** — para categorização detalhada (se comprou comida vs. material de construção)
>
> A chave de acesso é um excelente critério de dedup: se duas NFes têm a mesma chave, são a mesma nota.

**Ana Silva (Arquiteta):**

> Adicionando à proposta: a chave de acesso da NFe deve ser usada como **fingerprint canônico**. Mesmo que o PDF e o XML da mesma NFe tenham hashes de arquivo diferentes, a chave de acesso identifica unicamente a nota. Sugiro:
>
> - `content_hash` continua como SHA-256 do arquivo (dedup de arquivo)
> - `canonical_fingerprint` = chave de acesso da NFe (dedup semântico)
> - Se um XML e um PDF têm a mesma chave de acesso → são a mesma nota, manter apenas a versão mais rica (XML)

---

## 3. Proposta Consolidada de Implementação

### 3.1 Passo 0 — Execução Imediata (sem código novo)

| Ação                         | Comando                                  | Resultado Esperado                                     |
| ---------------------------- | ---------------------------------------- | ------------------------------------------------------ |
| Rodar worker de ingestão     | `bun run workers/ingestion/src/index.ts` | 90 jobs processados: DISCOVERED → ... → PENDING_REVIEW |
| Verificar drafts via MCP     | `list_draft_batches`                     | Batches com drafts tipados                             |
| Aprovar drafts teste via MCP | `approve_draft_batch`                    | Registros financeiros no ledger                        |
| Verificar dashboard          | `http://localhost:3105/dashboard`        | Dados aparecem nas queries                             |

### 3.2 Parser NFe XML (Prioridade Alta)

**Escopo:** `workers/ingestion/src/parsers/nfe-xml-parser.ts`

**Detecção:**

```
isNFe(text): boolean
  → Busca namespace "portalfiscal.inf.br/nfe" OU tag <nfeProc> OU tag <NFe>
```

**Extração (campos):**

| Campo                 | XPath (dentro de infNFe)                  | Uso no Sistema                        |
| --------------------- | ----------------------------------------- | ------------------------------------- |
| Chave de acesso       | `@Id` ou `<chNFe>` (44 dígitos)           | Dedup canônico, canonical_fingerprint |
| Número NFe            | `ide/nNF`                                 | document_number                       |
| Série                 | `ide/serie`                               | Metadado                              |
| Data emissão          | `ide/dhEmi`                               | competence_date                       |
| CNPJ emitente         | `emit/CNPJ`                               | supplier CNPJ → resolução             |
| Nome emitente         | `emit/xNome`                              | supplier_name_raw                     |
| CPF/CNPJ destinatário | `dest/CPF` ou `dest/CNPJ`                 | Validação ownership                   |
| Valor total           | `total/ICMSTot/vNF`                       | total_amount                          |
| Itens                 | `det[]/prod` (xProd, qCom, vUnCom, vProd) | breakdown items                       |
| ICMS                  | `total/ICMSTot/vICMS`                     | taxes.icms                            |
| PIS                   | `total/ICMSTot/vPIS`                      | taxes.pis                             |
| COFINS                | `total/ICMSTot/vCOFINS`                   | taxes.cofins                          |
| Forma pagamento       | `pag/detPag/tPag`                         | payment_method metadata               |

**Confiança:** Como XML estruturado, início em 0.95. Reduz 0.05 por campo mandatório faltante.

**Integração no parse-orchestrator:**

```
1. extractText → detecta XML
2. Se isNFe(text) → parseNFe(text) → retorna NFeResult
3. Salva parsed_document_version com parser_type = 'LOCAL_XML_NFE'
4. Salva extraction_result com dados mapeados
5. Draft generator classifica como 'transaction' (compra avulsa) ou
   'recurring_template' (se for nota de serviço recorrente como internet)
```

**Testes mínimos:**

- NFe XML válida com todos os campos → confiança 0.95+
- NFe XML parcial (sem itens) → confiança reduzida, campos presentes OK
- XML que NÃO é NFe → `isNFe()` retorna false
- NFe c/ múltiplos itens → array de items no breakdown
- Dedup por chave de acesso → canonical_fingerprint match

### 3.3 Parser NFe PDF / DANFE (Prioridade Média)

**Escopo:** `workers/ingestion/src/parsers/nfe-pdf-parser.ts`

**Detecção:**

```
isDANFE(text): boolean
  → Busca padrão de chave de acesso (44 dígitos consecutivos)
  → Busca "DANFE" ou "DOCUMENTO AUXILIAR DA NOTA FISCAL"
  → Busca padrão CNPJ em posição de emitente
```

**Extração (campos viáveis por regex no MVP):**

| Campo           | Regex/Heurística                                                                          | Confiança                        |
| --------------- | ----------------------------------------------------------------------------------------- | -------------------------------- | ----- |
| Chave de acesso | `/\d{4}\s*\d{4}\s*\d{4}\s*\d{4}\s*\d{4}\s*\d{4}\s*\d{4}\s*\d{4}\s*\d{4}\s*\d{4}\s*\d{4}/` | Alta                             |
| Número NFe      | `/N[ºo°]?\s*(\d{3,9})/i` próximo a "Nº"                                                   | Média                            |
| CNPJ emitente   | Primeiro CNPJ no topo do documento                                                        | Média                            |
| Nome emitente   | Texto antes/após o primeiro CNPJ                                                          | Baixa                            |
| Valor total     | `/VALOR\s*TOTAL\s*(?:DA\s*NOTA)?[:\s]*R?\$?\s*([\d.,]+)/i`                                | Média                            |
| Data emissão    | `/(?:DATA\s*(?:DE\s*)?EMISS[ÃA]O                                                          | EMISS[ÃA]O)[:\s]\*([\d\/\-]+)/i` | Média |

**Confiança:** Início em 0.7 (PDF menos confiável que XML). +0.1 por campo confirmado, -0.15 se chave de acesso ausente.

**Testes mínimos:**

- DANFE com chave de acesso → extrai corretamente
- DANFE sem chave → confiança reduzida, campos parciais
- PDF que NÃO é DANFE → `isDANFE()` retorna false
- Cross-check: XML e PDF da mesma NFe → mesma chave de acesso

### 3.4 Webscraping Supervisionado (Prioridade Média)

**Escopo:** `workers/web-scraper/`

**Arquitetura proposta:**

```
┌─────────────────────────────────────────────┐
│              Gmail Scanner                   │
│  Detecta: mensagem com link, sem anexo útil │
│  Cria: source_document (origin_type=email)  │
│         + scraping_request (PENDENTE)        │
└─────────────┬───────────────────────────────┘
              │
              ▼
┌─────────────────────────────────────────────┐
│         MCP Tool: request_web_scrape         │
│  Lista: scraping_requests pendentes          │
│  CEO decide: aprovar/rejeitar URL            │
│  Validação: URL está na allowlist?           │
└─────────────┬───────────────────────────────┘
              │ (CEO aprova)
              ▼
┌─────────────────────────────────────────────┐
│            Web Scraper Worker                │
│  Playwright headless Chrome                  │
│  1. Navega à URL aprovada                   │
│  2. Detecta PDFs/links de download          │
│  3. Baixa artefatos encontrados             │
│  4. Screenshot da página (auditoria)        │
│  5. Upload para Supabase Storage            │
│  6. Cria source_document + ingestion_job    │
│  → Pipeline normal assume daqui             │
└─────────────────────────────────────────────┘
```

**Tabela nova sugerida:**

```sql
CREATE TABLE scraping_requests (
  id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
  user_id UUID REFERENCES auth.users(id) NOT NULL,
  source_document_id UUID REFERENCES source_documents(id),
  url TEXT NOT NULL,
  domain TEXT NOT NULL,  -- extraído da URL para allowlist check
  status TEXT NOT NULL DEFAULT 'pending',
  -- pending → approved → scraping → completed → linked
  -- pending → rejected
  -- scraping → failed
  approved_at TIMESTAMPTZ,
  approved_by UUID REFERENCES auth.users(id),
  scrape_result JSONB,      -- {artifacts_found, screenshot_path, time_ms, errors}
  allowlisted BOOLEAN DEFAULT false,
  created_at TIMESTAMPTZ DEFAULT now(),
  updated_at TIMESTAMPTZ DEFAULT now()
);
```

**Allowlist inicial (domínios seguros):**

```
copasa.com.br
cemig.com.br
unimed*.com.br
portoseguro.com.br
tokiomarine.com.br
bradescoseguros.com.br
vivo.com.br
claro.com.br
tim.com.br
oi.com.br
netcabo.com.br
sabesp.com.br
cpfl.com.br
enel.com.br
light.com.br
energisa.com.br
```

**Medidas de segurança (obrigatórias):**

1. **Allowlist de domínios** — scraper recusa URLs fora da lista
2. **Confirmação humana** — nenhum scrape sem aprovação do CEO
3. **Sandbox** — Playwright em modo isolado, sem acesso a rede interna
4. **Timeout** — máximo 30s por navegação, 10MB limite por download
5. **Rate limiting** — máximo 5 scrapes/minuto, 50/hora
6. **Auditoria** — screenshot + logs de cada scrape
7. **Sem credenciais** — MVP não faz login em portais (apenas páginas públicas)
8. **DNS resolution check** — rejeitar IPs privados/reservados (anti-SSRF)

**MCP Tools novas:**

- `list_scraping_requests` — URLs pendentes de aprovação
- `approve_scraping_request` — CEO aprova URL individual
- `reject_scraping_request` — CEO rejeita URL
- `manage_scraping_allowlist` — CEO adiciona/remove domínios

**Testes mínimos:**

- URL na allowlist → scrape executado (mock Playwright)
- URL fora da allowlist → rejeitada automaticamente
- URL de rede interna/IP privado → rejeitada (SSRF protection)
- Timeout excedido → status FAILED com mensagem
- PDF encontrado → upload para Storage + source_document criado

### 3.5 Atualização do Parse Orchestrator

O `parse-orchestrator.ts` atual tem a seguinte cadeia de detecção:

```
1. isCemig(text) → parseCemig
2. parseBoleto(text) → se confiança > 0.2
3. Fallback genérico
```

Nova cadeia proposta:

```
1. Se MIME é XML E isNFe(text) → parseNFe       [NOVO]
2. Se isCemig(text) → parseCemig                  [existente]
3. Se isDANFE(text) → parseDANFE                   [NOVO]
4. parseBoleto(text) → se confiança > 0.2         [existente]
5. Fallback genérico                               [existente]
```

A prioridade do NFe XML antes do CEMIG é justificada: NFe é formato universal e mais rico. Se um documento for ao mesmo tempo parecer CEMIG e ser NFe, a versão NFe tem mais dados.

---

## 4. Prós e Contras

### Opção A: Implementar NFe XML + Webscraping Agora (antes da UI de ingestão)

**Prós:**

- Maximiza valor dos dados já ingeridos — muitos dos 90 documentos são NFes
- Webscraping captura fornecedores que só enviam links (problema real do CEO)
- Aumenta cobertura do parser antes de processar os 90 jobs
- Quando a UI de ingestão ficar pronta, os dados já estarão ricos
- NFe XML é o parser mais fácil e rico — alto ROI

**Contras:**

- Atrasa Fase 6 (UI de ingestão) que tem 15 tarefas
- Webscraping adiciona complexidade (Playwright, Chromium, segurança)
- Aumenta superfície de ataque (scraping de URLs externas)
- MVP cresce em escopo

### Opção B: Primeiro a UI de ingestão (Fase 6), depois parsers novos

**Prós:**

- CEO vê resultados na interface web mais cedo
- Pode aprovar/rejeitar drafts visualmente
- Scope mais previsível

**Contras:**

- Drafts existentes terão dados pobres (parser genérico em NFes)
- CEO terá que re-processar documentos quando o parser de NFe ficar pronto
- Webscraping postergado = fornecedores com link continuam sem dados

### Opção C: Híbrido — NFe XML agora, webscraping depois, UI em paralelo

**Prós:**

- NFe XML é rápido de implementar (< 1 dia) e não tem dependência externa
- UI pode começar em paralelo (Roberto)
- Webscraping fica para quando a UI estiver madura (modo de aprovação de scrape)
- Melhor equilíbrio escopo/valor

**Contras:**

- Webscraping fica postergado (fornecedores com link continuam manuais)

---

## 5. Decisão Final

**Decisão: Opção C — Híbrido**

Justificativa: O parser de NFe XML é a ação de maior ROI: fácil de implementar (XML estruturado, sem dependência externa), rico em dados, e beneficia imediatamente os 90 documentos já ingeridos. O parser NFe PDF (DANFE) pode vir logo depois. A UI de ingestão começa em paralelo com o Roberto. O webscraping, por envolver Playwright, segurança anti-SSRF e aprovação humana, requer uma fase dedicada — mas pode ser implementado antes da Fase 7 (IA).

### Ordem de execução:

```
AGORA (esta semana):
  1. Rodar worker nos 90 jobs existentes (sem código novo)
  2. Validar fluxo via MCP (list_draft_batches → approve)
  3. Implementar parser NFe XML
  4. Re-processar documentos XML com novo parser

PRÓXIMA SEMANA:
  5. Implementar parser NFe PDF (DANFE)
  6. Iniciar UI de ingestão (rotas E.1-E.5)

SEMANA SEGUINTE:
  7. Webscraping supervisionado (allowlist + MCP tools)
  8. Continuar UI de ingestão (E.6-E.15)
```

### Faseamento revisado (entre Fase 5 e Fase 6 do plano original):

| Fase | Nome                       | Escopo                                                                           |
| ---- | -------------------------- | -------------------------------------------------------------------------------- |
| 5.5a | Parser NFe XML             | nfe-xml-parser.ts + integração no orchestrator + testes                          |
| 5.5b | Parser NFe PDF             | nfe-pdf-parser.ts + integração no orchestrator + testes                          |
| 5.5c | Webscraping Supervisionado | web-scraper worker + scraping_requests table + MCP tools + allowlist + segurança |
| 6    | UI de Ingestão             | Conforme plano original (15 tarefas)                                             |

---

## 6. Ações / Responsáveis / Prazo

| #   | Ação                                                                       | Responsável      | Prazo      |
| --- | -------------------------------------------------------------------------- | ---------------- | ---------- |
| 1   | Executar worker de ingestão nos 90 jobs e validar resultados               | CEO + João       | 2026-03-26 |
| 2   | Validar fluxo completo via MCP (list → approve → verificar dashboard)      | CEO              | 2026-03-26 |
| 3   | Implementar parser NFe XML (`nfe-xml-parser.ts`)                           | Pedro + João     | 2026-03-27 |
| 4   | Integrar parser NFe XML no parse-orchestrator                              | Pedro            | 2026-03-27 |
| 5   | Escrever testes do parser NFe XML (mínimo 10 testes)                       | Maria            | 2026-03-27 |
| 6   | Re-processar documentos XML com novo parser (via MCP `reprocess_document`) | CEO              | 2026-03-27 |
| 7   | Implementar parser NFe PDF / DANFE (`nfe-pdf-parser.ts`)                   | Pedro            | 2026-03-28 |
| 8   | Testes do parser NFe PDF (mínimo 8 testes)                                 | Maria            | 2026-03-28 |
| 9   | Criar migration `scraping_requests` + RLS                                  | André            | 2026-03-29 |
| 10  | Implementar web scraper worker com Playwright                              | João + Fernando  | 2026-03-31 |
| 11  | Implementar MCP tools de webscraping (list/approve/reject)                 | João             | 2026-03-31 |
| 12  | Implementar segurança anti-SSRF (allowlist, DNS check, sandbox)            | Maria + Fernando | 2026-03-31 |
| 13  | Testes do webscraping (mínimo 10 testes)                                   | Maria            | 2026-04-01 |
| 14  | Iniciar UI de ingestão — rotas E.1 a E.5                                   | Roberto + Helena | 2026-03-29 |
| 15  | Atualizar checklist 003 e 004 com novas fases                              | João             | Contínuo   |

---

## 7. Respostas Diretas ao CEO

### "Era para eu ver esses documentos no localhost:3105?"

**Não.** A interface web atual mostra o **ledger financeiro** (transações, recorrências, faturas, passivos). Os documentos ingeridos vivem nas tabelas de ingestão e só aparecem no portal **depois de aprovados** (virando registros financeiros). A UI de ingestão (Fase 6) permitirá ver e gerenciar os documentos. Até lá, o **MCP** é a interface de operação.

### "O que vamos fazer agora?"

**Rodar o worker de ingestão** nos 90 jobs existentes (`bun run workers/ingestion/src/index.ts`). Os documentos passam pelo pipeline (download → hash → parse → draft) e ficam prontos para revisão. Depois, implementar o parser de NFe XML para enriquecer os dados.

### "Como isso vira dado no portal?"

Pipeline: `source_document → parsed_version → extraction_result → draft_record → [REVISÃO HUMANA via MCP] → registro financeiro real`. Quando você aprova um draft, ele vira transação, recorrência ou métrica de consumo no ledger — e aparece no dashboard.

### "Como vira registro?"

Via aprovação. O CEO revisa os drafts (MCP tool `list_draft_batches` / `approve_draft_batch`), e os aprovados são inseridos nas tabelas financeiras (transactions, recurring_templates, etc.) com referência reversa (`posted_record_id`).

### "Para certos e-mails preciso de webscraping"

Planejado como **Fase 5.5c** — webscraping supervisionado com Playwright, allowlist de domínios, confirmação humana obrigatória, proteção anti-SSRF. Não estava no plano original; agora está.

### "Precisamos de um leitor de NFe XML e PDF"

Planejado como **Fase 5.5a** (NFe XML — prioridade alta, implementação imediata) e **Fase 5.5b** (NFe PDF/DANFE — prioridade média, próxima semana). O XML é o formato mais rico e mais fácil de parsear. O PDF usa regex heurístico no MVP, com upgrade para OpenAI Vision na Fase 7.
