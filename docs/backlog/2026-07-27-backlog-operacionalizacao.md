# Backlog de operacionalização

> Entregável §16.4 do plano mestre. Data: 2026-07-27.
>
> **Este arquivo é a fonte versionada.** O GitHub é a superfície de controle:
> `scripts/seed-github-backlog.ts` lê daqui e cria épicos, features e issues com
> sub-issues nativas. Reexecutar não duplica.
>
> ⚠️ **Repositório público.** Nenhum título, corpo ou comentário de issue pode conter
> valores reais, nomes de instituições do CEO, ids de projeto Supabase ou trechos de
> documentos.

## Marcos (viram milestones no GitHub)

| Marco | Critério (§18)              |
| ----- | --------------------------- |
| M1    | Um documento fecha o ciclo  |
| M2    | Fatura de cartão utilizável |
| M3    | Backfill supervisionado     |
| M4    | Planejamento financeiro     |

## Épicos

| #   | Épico                                                      | Prioridade | Marco | Estado                                   |
| --- | ---------------------------------------------------------- | ---------- | ----- | ---------------------------------------- |
| E1  | Contrato canônico de draft e materialização atômica        | P0         | M1    | ✅ concluído                             |
| E2  | Obrigações financeiras conectadas ao pipeline              | P0         | M1    | ✅ concluído                             |
| E3  | Segredos: criptografia real e perfis de senha              | P0         | M2    | ✅ base concluída                        |
| E4  | Orquestração única e backfill retomável                    | P0         | M3    | 🟡 CLI feito; checkpoint a escrever      |
| E5  | Testes ponta a ponta e goldens                             | P0         | M1    | 🟡 E2E feito; goldens em P1              |
| E6  | Jornada vertical: fatura protegida de cartão               | P1         | M2    | ⬜ próximo                               |
| E7  | CI/CD em GitHub Actions e deploy de produção               | P0         | M1    | 🟡 workflows escritos; falta 1ª execução |
| E8  | Atualização e higiene de módulos                           | P0         | M1    | 🟡 higiene feita; majors pendentes       |
| E9  | Documentação de estado real, arquitetura e rastreabilidade | P0         | M1    | ✅ concluído                             |
| E10 | Formatos anunciados sem parser                             | P1         | M2    | ⬜                                       |
| E11 | Domínio financeiro sem consumidor                          | P3         | M4    | ⬜                                       |

---

## E6 — Jornada vertical: fatura protegida de cartão

Escolha do CEO para primeira jornada. Depende de E1, E2, E3.

### F6.1 — Telas de cartão _(bloqueador: a jornada é inalcançável sem elas)_

> **Não existe rota `/dashboard/cards`.** Não há como cadastrar um cartão, definir
> `closing_day`, `due_day` ou `credit_limit` pela interface. Verificado: a tabela `cards`
> já tem todas as colunas necessárias — a lacuna é puramente código e UI, **zero trabalho
> de schema**.

| Issue  | Descrição                                                                                                | Complexidade |
| ------ | -------------------------------------------------------------------------------------------------------- | ------------ |
| I6.1.1 | `actions/cards.ts` com CRUD e validação Zod                                                              | S            |
| I6.1.2 | `/dashboard/cards` — lista com limite, fechamento e vencimento                                           | M            |
| I6.1.3 | `/dashboard/cards/new` e `[id]` — formulário                                                             | M            |
| I6.1.4 | Resolver a divergência `cards.credit_limit` × `financial_products.credit_limit` com view `v_card_limits` | S            |

**Aceite:** cadastrar um cartão pela interface e vê-lo disponível na revisão de drafts.
**Testes:** `__tests__/integration/cards.test.ts`.

### F6.2 — Perfis de senha

| Issue  | Descrição                                                                           | Complexidade |
| ------ | ----------------------------------------------------------------------------------- | ------------ |
| I6.2.1 | Escopos ordenados: `card_id` → last4 → instituição → fornecedor → contrato → global | M            |
| I6.2.2 | Tela de gestão de senhas em `/dashboard/settings` (a action já existe)              | M            |
| I6.2.3 | Teto de 5 tentativas por documento, uma linha de auditoria por tentativa            | S            |

**Aceite:** senha resolvida por perfil, sem varrer todas as senhas do usuário.

### F6.3 — Motores de domínio da fatura

Três módulos puros, **test-first**.

| Issue  | Descrição                                                                                                                                                  | Complexidade |
| ------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| I6.3.1 | `statement-cycle/` — `deriveCycleForDate(card, date)`, com clamp de mês curto (fechamento 31 em fevereiro) e wraparound quando fechamento > vencimento     | M            |
| I6.3.2 | `installments/` — reconhece `01/12`, `PARC 1/12`, `1 de 12`, `(1/12)`, `1ª de 12`. Hoje `grep installment\|parcela` em `workers/ingestion/src` dá **zero** | M            |
| I6.3.3 | `card-commitment/` — por cartão e mês: comprometido, aberto, fechado, parcelas futuras, projetado após pagamento, risco de estouro                         | L            |
| I6.3.4 | Perfis de emissor sobre `supplier-templates.ts`                                                                                                            | L            |

**Riscos:** layouts mudam sem aviso — mitigar com goldens versionados (F6.5).

### F6.4 — Upload direto ao Storage

> `uploadDocument` tuneliza o arquivo por Server Action. **O limite padrão do Vercel é
> 4,5 MB** — uma fatura escaneada de 10 MB falha hoje. Essa é a razão concreta, não
> elegância.

| Issue  | Descrição                                                                                                                 | Complexidade |
| ------ | ------------------------------------------------------------------------------------------------------------------------- | ------------ |
| I6.4.1 | Hash SHA-256 no cliente sobre `File.stream()`                                                                             | S            |
| I6.4.2 | `createUploadTicket` + tabela `upload_tickets`                                                                            | M            |
| I6.4.3 | `uploadToSignedUrl` com progresso; TUS acima de 6 MB                                                                      | M            |
| I6.4.4 | `confirmUpload` verificando objeto antes de enfileirar                                                                    | S            |
| I6.4.5 | Deletar `supabase/functions/trigger-ingestion/` (quebrada: insere 4 colunas inexistentes) e repontar `ai-chat-drawer.tsx` | S            |

### F6.5 — Goldens e E2E da jornada

> **O ativo de maior alavancagem do plano.** Até P0-10 não havia fixture binário algum, e
> nenhum parser jamais foi testado contra documento real.

| Issue  | Descrição                                                                     | Complexidade |
| ------ | ----------------------------------------------------------------------------- | ------------ |
| I6.5.1 | `__tests__/fixtures/golden/` com `input` + `expected.json` por documento      | M            |
| I6.5.2 | `scripts/anonymize-fixture.ts` para contribuir documentos reais com segurança | M            |
| I6.5.3 | Projeto vitest `golden` com `--update-goldens`                                | S            |
| I6.5.4 | `journey-card-invoice.test.ts` provando o Marco 2                             | L            |

**🔒 Depende do CEO:** quais três emissores ganham perfil de primeira classe. Exige
documentos reais anonimizados, e nenhum planejamento substitui tê-los.

### F6.6 — Substituir `pdf-parse`

> `pdf-parse@1.1.1` é de 2018, sem manutenção, e é **o motor exato desta jornada**.
> Migrar só é seguro **depois** de F6.5 — os goldens são a rede.

| Issue  | Descrição                                            | Complexidade |
| ------ | ---------------------------------------------------- | ------------ |
| I6.6.1 | Migrar para `unpdf`/`pdfjs-dist` com suporte a senha | M            |

---

## E10 — Formatos anunciados sem parser

> A UI aceita PDF, imagens, XLSX, CSV, DOC, DOCX, OFX e QIF. **Só PDF tem parser.** QIF
> nem está nas extensões aceitas pelos scanners. Ou o parser entra, ou a promessa sai.

| Issue | Descrição                                                                                                                                      | Complexidade |
| ----- | ---------------------------------------------------------------------------------------------------------------------------------------------- | ------------ |
| I10.1 | `supported-formats.ts` como fonte única, consumido pelas 3 listas `accept`, pelo scanner e pelo allowlist do bucket, com teste de equivalência | S            |
| I10.2 | CSV real (`csv-parse`, `;` e latin-1 — exportações brasileiras)                                                                                | M            |
| I10.3 | OFX (`ofx-js`); `FITID` vira chave exata de dedup                                                                                              | M            |
| I10.4 | XLSX (`exceljs`)                                                                                                                               | M            |
| I10.5 | OCR de imagem (`tesseract.js` WASM como base; `ocrmypdf` como upgrade)                                                                         | L            |
| I10.6 | Remover DOCX e QIF da UI até haver arquivo real                                                                                                | S            |

---

## E11 — Domínio financeiro sem consumidor

> Quatro módulos testados que **nenhuma tela chama**. O maior desperdício de código
> pronto do repositório, e a trilha mais barata de credibilidade: não tem dependências.

| Issue | Descrição                                                                                                            | Complexidade |
| ----- | -------------------------------------------------------------------------------------------------------------------- | ------------ |
| I11.1 | `financial-cycle` substitui a matemática inline de `reports/page.tsx:36-46`                                          | S            |
| I11.2 | `deduplication` + view `v_expenses_deduplicated` nos relatórios (hoje somam `transactions` cru, ignorando a ADR-001) | M            |
| I11.3 | `amortization` na tela de dívida: cronograma projetado + simulador de quitação                                       | M            |
| I11.4 | Teste de arquitetura: todo módulo de domínio exportado precisa de ≥1 importador fora de testes                       | S            |

---

## E4 — pendências

| Issue | Descrição                                                                                                 | Complexidade |
| ----- | --------------------------------------------------------------------------------------------------------- | ------------ |
| I4.1  | Escrever checkpoints durante a varredura (tabela criada em P0-9)                                          | M            |
| I4.2  | Resume por `internal_date` — **não** por `pageToken`, que não é durável entre sessões                     | M            |
| I4.3  | Painel de backfill no §7.4: período, encontradas, processadas, duplicatas, erros, pendências, custo de IA | L            |

## E7 — pendências

| Issue | Descrição                                                             | Complexidade |
| ----- | --------------------------------------------------------------------- | ------------ |
| I7.1  | Primeira execução do CI, confirmando paridade                         | S            |
| I7.2  | Configurar secrets e o environment `production` com required reviewer | S 🔒         |
| I7.3  | Corrigir os 5 erros de typecheck e remover o `continue-on-error`      | M            |
| I7.4  | Estender cobertura a `apps/web` e `workers/`                          | S            |

## E8 — pendências (majors, deliberadamente fora de P0)

| Issue | Descrição                                        | Complexidade |
| ----- | ------------------------------------------------ | ------------ |
| I8.1  | zod 3 → 4 **junto** com AI SDK 4 → 5 (acoplados) | L            |
| I8.2  | `lucide-react` 0.577 → 1.x                       | S            |
| I8.3  | `react-day-picker` 9 → 10                        | S            |

> Mantidos fora de P0 de propósito: pôr uma migração major no caminho crítico da
> correção de contrato trocaria um risco conhecido por dois.

---

## Decisões que dependem exclusivamente do CEO

1. **Quais três emissores** ganham perfil de primeira classe — exige documentos reais anonimizados.
2. **Postura LGPD:** documentos podem sair da máquina por padrão (IA) ou OCR local primeiro? Define a arquitetura de P2.
3. **Teto mensal de custo de IA.**
4. **`apps/mobile`** (hoje só `package.json` + `tsconfig.json`): manter ou assumir PWA.
5. **Runner Windows + certificado de assinatura** para o agente — `.exe` sem assinatura dispara SmartScreen.
6. **Tornar o repositório privado?** App financeiro pessoal, público, com histórico de vazamento de chaves.
7. **Rotação das chaves Supabase expostas** (commit `38b8126`).
