/**
 * Semeia o backlog no GitHub a partir do markdown versionado.
 *
 * O markdown em docs/backlog/ é a fonte da verdade; o GitHub é a superfície de
 * controle. Este script cria labels, milestones, épicos, features e issues, e
 * liga a hierarquia com **sub-issues nativas** — que dão rollup de progresso e
 * permitem reparentar, coisas que a convenção antiga de "épico é uma issue com
 * checklist de links" não oferece.
 *
 * Idempotente: casa por título e não duplica ao reexecutar.
 *
 * Uso:
 *   bun run scripts/seed-github-backlog.ts [--dry-run]
 *
 * Requer `gh` autenticado com escopo de repo.
 */
import { spawnSync } from "node:child_process";

const REPO = "ChicoFigueiredo/Seu-Bolso-Feliz";
const DRY = process.argv.includes("--dry-run");

// ── Taxonomia ──────────────────────────────────────────────────────────────

const LABELS: Array<{ name: string; color: string; description: string }> = [
  { name: "epic", color: "5319e7", description: "Bloco de trabalho ligado a um marco" },
  { name: "feature", color: "1d76db", description: "Capacidade dentro de um épico" },
  { name: "task", color: "0e8a16", description: "Unidade executável" },
  { name: "P0", color: "b60205", description: "Verdade e integridade do núcleo" },
  { name: "P1", color: "d93f0b", description: "Jornadas verticais" },
  { name: "P2", color: "fbca04", description: "Agente local" },
  { name: "P3", color: "0052cc", description: "Inteligência financeira" },
  { name: "P4", color: "c5def5", description: "Escala e refinamento" },
  { name: "area:ingestion", color: "bfd4f2", description: "" },
  { name: "area:domain", color: "bfd4f2", description: "" },
  { name: "area:web", color: "bfd4f2", description: "" },
  { name: "area:db", color: "bfd4f2", description: "" },
  { name: "area:mcp", color: "bfd4f2", description: "" },
  { name: "area:agent", color: "bfd4f2", description: "" },
  { name: "area:ci", color: "bfd4f2", description: "" },
  { name: "area:docs", color: "bfd4f2", description: "" },
  { name: "area:deps", color: "bfd4f2", description: "" },
  { name: "size:S", color: "ededed", description: "" },
  { name: "size:M", color: "ededed", description: "" },
  { name: "size:L", color: "ededed", description: "" },
  { name: "size:XL", color: "ededed", description: "" },
  { name: "blocked", color: "000000", description: "Depende de outra issue" },
  { name: "needs-evidence", color: "e99695", description: "Sem teste ou chamador citado" },
];

const MILESTONES = [
  { title: "Marco 1 — Um documento fecha o ciclo", description: "§18 do plano mestre" },
  { title: "Marco 2 — Fatura de cartão utilizável", description: "§18 do plano mestre" },
  { title: "Marco 3 — Backfill supervisionado", description: "§18 do plano mestre" },
  { title: "Marco 4 — Planejamento financeiro", description: "§18 do plano mestre" },
];

interface Node {
  title: string;
  body: string;
  labels: string[];
  milestone?: string;
  children?: Node[];
}

const DOC = "docs/backlog/2026-07-27-backlog-operacionalizacao.md";

const epicBody = (outcome: string, status: string) =>
  `## Resultado esperado\n\n${outcome}\n\n## Estado\n\n${status}\n\n---\nFonte: [\`${DOC}\`](../blob/main/${DOC})`;

const taskBody = (ctx: string, acceptance: string, tests: string) =>
  `## Contexto\n\n${ctx}\n\n## Critérios de aceite\n\n${acceptance}\n\n## Testes\n\n${tests}\n\n---\nFonte: [\`${DOC}\`](../blob/main/${DOC})`;

/**
 * Apenas os épicos ainda abertos entram como issues. Os concluídos em P0 estão
 * registrados no plano mestre com o commit correspondente — abrir issue para
 * trabalho já feito só polui o board.
 */
const TREE: Node[] = [
  {
    title: "E6 — Jornada vertical: fatura protegida de cartão",
    body: epicBody(
      "O usuário importa uma fatura protegida e passa a saber quanto do limite do cartão está comprometido nos próximos meses.",
      "Próximo. Depende de E1, E2 e E3, todos concluídos.",
    ),
    labels: ["epic", "P1", "area:ingestion"],
    milestone: MILESTONES[1]!.title,
    children: [
      {
        title: "F6.1 — Telas de cartão",
        body: "Bloqueador: a jornada é inalcançável sem cadastrar um cartão.",
        labels: ["feature", "P1", "area:web"],
        milestone: MILESTONES[1]!.title,
        children: [
          {
            title: "I6.1.1 — actions/cards.ts com CRUD e validação",
            body: taskBody(
              "Não existe rota `/dashboard/cards`: não há como cadastrar um cartão pela interface. A tabela `cards` já tem `credit_limit`, `closing_day` e `due_day` — a lacuna é código e UI, zero trabalho de schema.",
              "- [ ] CRUD completo com validação Zod\n- [ ] Escopo por usuário em toda consulta",
              "`__tests__/integration/cards.test.ts`",
            ),
            labels: ["task", "P1", "area:web", "size:S"],
            milestone: MILESTONES[1]!.title,
          },
          {
            title: "I6.1.2 — /dashboard/cards com limite, fechamento e vencimento",
            body: taskBody(
              "Lista de cartões, hoje inexistente.",
              "- [ ] Cartão cadastrado aparece na revisão de drafts",
              "cobertura via `journey-card-invoice`",
            ),
            labels: ["task", "P1", "area:web", "size:M"],
            milestone: MILESTONES[1]!.title,
          },
          {
            title: "I6.1.4 — Resolver divergência de credit_limit",
            body: taskBody(
              "`cards.credit_limit` e `financial_products.credit_limit` coexistem sem regra de precedência. Projeções podem ler o campo errado.",
              "- [ ] View `v_card_limits` com a regra explícita\n- [ ] Documentado em ADR",
              "`__tests__/integration/cards.test.ts`",
            ),
            labels: ["task", "P1", "area:db", "size:S"],
            milestone: MILESTONES[1]!.title,
          },
        ],
      },
      {
        title: "F6.3 — Motores de domínio da fatura",
        body: "Três módulos puros, test-first.",
        labels: ["feature", "P1", "area:domain"],
        milestone: MILESTONES[1]!.title,
        children: [
          {
            title: "I6.3.1 — statement-cycle: derivar ciclo a partir do cartão",
            body: taskBody(
              "Não existe derivação de ciclo. Casos que quebram implementações ingênuas: fechamento dia 31 em fevereiro, e fechamento maior que vencimento.",
              "- [ ] `deriveCycleForDate(card, date)` retorna referência, início, fim e vencimento\n- [ ] Clamp de mês curto\n- [ ] Wraparound quando fechamento > vencimento",
              "`packages/domain/src/statement-cycle/index.test.ts`",
            ),
            labels: ["task", "P1", "area:domain", "size:M"],
            milestone: MILESTONES[1]!.title,
          },
          {
            title: "I6.3.2 — installments: detectar parcelas",
            body: taskBody(
              "`grep 'installment|parcela'` em `workers/ingestion/src` retorna **zero**. As colunas existem e só são exibidas como texto inerte.",
              "- [ ] Reconhece `01/12`, `PARC 1/12`, `1 de 12`, `(1/12)`, `1ª de 12`",
              "`packages/domain/src/installments/index.test.ts`",
            ),
            labels: ["task", "P1", "area:domain", "size:M"],
            milestone: MILESTONES[1]!.title,
          },
          {
            title: "I6.3.3 — card-commitment: limite comprometido",
            body: taskBody(
              "`cards.credit_limit` não é lido por nenhum código hoje.",
              "- [ ] Por cartão e mês: comprometido, aberto, fechado, parcelas futuras, projetado após pagamento, risco de estouro",
              "`packages/domain/src/card-commitment/index.test.ts`",
            ),
            labels: ["task", "P1", "area:domain", "size:L"],
            milestone: MILESTONES[1]!.title,
          },
        ],
      },
      {
        title: "F6.5 — Goldens e E2E da jornada",
        body: "O ativo de maior alavancagem: nenhum parser jamais foi testado contra documento real até P0-10.",
        labels: ["feature", "P1", "area:ingestion"],
        milestone: MILESTONES[1]!.title,
        children: [
          {
            title: "I6.5.4 — E2E provando o Marco 2",
            body: taskBody(
              "Marco 2 do §18.",
              "- [ ] Senha resolvida por perfil\n- [ ] Cartão identificado\n- [ ] Ciclo e vencimento extraídos\n- [ ] ≥95% dos itens viram `statement_items`\n- [ ] Parcelas sinalizadas\n- [ ] Limite comprometido de 6 meses",
              "`__tests__/e2e/journey-card-invoice.test.ts`",
            ),
            labels: ["task", "P1", "area:ingestion", "size:L"],
            milestone: MILESTONES[1]!.title,
          },
        ],
      },
    ],
  },
  {
    title: "E10 — Formatos anunciados sem parser",
    body: epicBody(
      "A interface deixa de prometer o que o código não faz.",
      "A UI aceita PDF, imagens, XLSX, CSV, DOC, DOCX, OFX e QIF. **Só PDF tem parser.** QIF nem está nas extensões aceitas pelos scanners.",
    ),
    labels: ["epic", "P1", "area:ingestion"],
    milestone: MILESTONES[1]!.title,
    children: [
      {
        title: "I10.1 — supported-formats como fonte única",
        body: taskBody(
          "Três listas `accept` na UI, a allowlist do scanner e a do bucket divergem entre si e do código.",
          "- [ ] Uma constante consumida por todos\n- [ ] Teste de equivalência entre registro, `accept` e bucket",
          "`packages/contracts/src/formats.test.ts`",
        ),
        labels: ["task", "P1", "area:ingestion", "size:S"],
        milestone: MILESTONES[1]!.title,
      },
      {
        title: "I10.5 — OCR de imagem",
        body: taskBody(
          "`extractText` devolve texto vazio para imagens (`image_placeholder`). Sem isso, a jornada de foto é estruturalmente impossível.",
          "- [ ] `tesseract.js` (WASM) como base, sem dependência nativa\n- [ ] `ocrmypdf` como upgrade opcional",
          "golden `receipt-photo`",
        ),
        labels: ["task", "P1", "area:ingestion", "size:L"],
        milestone: MILESTONES[1]!.title,
      },
    ],
  },
  {
    title: "E11 — Domínio financeiro sem consumidor",
    body: epicBody(
      "Quatro módulos testados passam a ser usados por telas reais.",
      "Maior desperdício de código pronto do repositório, e a trilha mais barata de credibilidade: **não tem dependências, pode começar já**.",
    ),
    labels: ["epic", "P3", "area:domain"],
    milestone: MILESTONES[3]!.title,
    children: [
      {
        title: "I11.1 — financial-cycle nos relatórios",
        body: taskBody(
          "`reports/page.tsx:36-46` reimplementa a matemática de ciclo inline em vez de chamar o módulo pronto e testado.",
          "- [ ] Relatórios chamam `@sbf/domain/financial-cycle`\n- [ ] Código inline removido",
          "`__tests__/integration/reports.test.ts`",
        ),
        labels: ["task", "P3", "area:web", "size:S"],
        milestone: MILESTONES[3]!.title,
      },
      {
        title: "I11.2 — deduplicação ADR-001 nos relatórios",
        body: taskBody(
          "Relatórios somam `transactions` cru e ignoram a view `v_expenses_deduplicated`, que existe. Itens de fatura são contados em dobro.",
          "- [ ] Relatórios usam a view\n- [ ] Teste com fatura + itens provando que não há dupla contagem",
          "`__tests__/integration/reports.test.ts`",
        ),
        labels: ["task", "P3", "area:web", "size:M"],
        milestone: MILESTONES[3]!.title,
      },
      {
        title: "I11.4 — Teste de arquitetura contra código morto",
        body: taskBody(
          "Foi assim que quatro módulos ficaram marcados como prontos por meses sem chamador algum.",
          "- [ ] Todo módulo de domínio exportado tem ≥1 importador fora de testes",
          "`__tests__/architecture/domain-usage.test.ts`",
        ),
        labels: ["task", "P3", "area:domain", "size:S"],
        milestone: MILESTONES[3]!.title,
      },
    ],
  },
];

// ── Execução ───────────────────────────────────────────────────────────────

function gh(args: string[], allowFail = false): string {
  if (DRY) {
    console.log(`  [dry-run] gh ${args.join(" ")}`);
    return "";
  }
  const r = spawnSync("gh", args, { encoding: "utf-8" });
  if (r.status !== 0 && !allowFail) {
    throw new Error(`gh ${args.slice(0, 3).join(" ")}: ${r.stderr?.trim()}`);
  }
  return (r.stdout ?? "").trim();
}

function ensureLabels() {
  console.log("Labels...");
  for (const l of LABELS) {
    // --force torna a operação idempotente: cria ou atualiza.
    gh(
      [
        "label",
        "create",
        l.name,
        "--repo",
        REPO,
        "--color",
        l.color,
        "--description",
        l.description,
        "--force",
      ],
      true,
    );
  }
}

function ensureMilestones(): Map<string, number> {
  console.log("Milestones...");
  const existing = new Map<string, number>();

  const raw = gh(
    ["api", `repos/${REPO}/milestones?state=all`, "--jq", '.[] | "\\(.number)\\t\\(.title)"'],
    true,
  );
  for (const line of raw.split("\n").filter(Boolean)) {
    const [num, ...rest] = line.split("\t");
    existing.set(rest.join("\t"), Number(num));
  }

  for (const m of MILESTONES) {
    if (existing.has(m.title)) continue;
    const out = gh(
      [
        "api",
        `repos/${REPO}/milestones`,
        "-f",
        `title=${m.title}`,
        "-f",
        `description=${m.description}`,
        "--jq",
        ".number",
      ],
      true,
    );
    if (out) existing.set(m.title, Number(out));
  }
  return existing;
}

/** Índice de issues existentes por título, para não duplicar. */
function existingIssues(): Map<string, number> {
  const raw = gh(
    [
      "issue",
      "list",
      "--repo",
      REPO,
      "--state",
      "all",
      "--limit",
      "500",
      "--json",
      "number,title",
      "--jq",
      '.[] | "\\(.number)\\t\\(.title)"',
    ],
    true,
  );
  const map = new Map<string, number>();
  for (const line of raw.split("\n").filter(Boolean)) {
    const [num, ...rest] = line.split("\t");
    map.set(rest.join("\t"), Number(num));
  }
  return map;
}

function createIssue(node: Node, index: Map<string, number>): number {
  const known = index.get(node.title);
  if (known) {
    console.log(`  = #${known} ${node.title}`);
    return known;
  }

  const args = ["issue", "create", "--repo", REPO, "--title", node.title, "--body", node.body];
  for (const l of node.labels) args.push("--label", l);
  if (node.milestone) args.push("--milestone", node.milestone);

  const url = gh(args);
  const num = Number(url.split("/").pop());
  console.log(`  + #${num} ${node.title}`);
  if (!DRY) index.set(node.title, num);
  return num;
}

/** Liga filho ao pai com sub-issue nativa (só via GraphQL). */
function linkSubIssue(parent: number, child: number) {
  if (DRY) {
    console.log(`  [dry-run] sub-issue #${child} -> #${parent}`);
    return;
  }
  const parentId = gh(["api", `repos/${REPO}/issues/${parent}`, "--jq", ".node_id"], true);
  const childId = gh(["api", `repos/${REPO}/issues/${child}`, "--jq", ".node_id"], true);
  if (!parentId || !childId) return;

  gh(
    [
      "api",
      "graphql",
      "-H",
      "GraphQL-Features: sub_issues",
      "-f",
      `query=mutation { addSubIssue(input: {issueId: "${parentId}", subIssueId: "${childId}"}) { subIssue { number } } }`,
    ],
    true,
  );
}

function walk(nodes: Node[], index: Map<string, number>, parent?: number) {
  for (const node of nodes) {
    const num = createIssue(node, index);
    if (parent) linkSubIssue(parent, num);
    if (node.children) walk(node.children, index, num);
  }
}

function main() {
  console.log(`Semeando backlog em ${REPO}${DRY ? " (dry-run)" : ""}\n`);
  ensureLabels();
  ensureMilestones();
  console.log("Issues...");
  walk(TREE, DRY ? new Map() : existingIssues());
  console.log("\nPronto. Reexecutar não duplica.");
}

if (import.meta.main) {
  try {
    main();
  } catch (err) {
    console.error(err instanceof Error ? err.message : err);
    process.exit(1);
  }
}
