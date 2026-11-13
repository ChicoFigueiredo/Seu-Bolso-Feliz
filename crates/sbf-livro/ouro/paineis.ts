// Gera a fixture de ouro dos painéis: grava `livro-ficticio.json` num Postgres local de
// teste (Docker, ver gerar.sh) e roda as Server Actions do web sobre ele, no instante fixo
// da semente, em TZ=America/Sao_Paulo:
//   carregarPainelHoje      (apps/web/src/app/actions/painel-hoje.ts)
//   carregarPainelCentros   (apps/web/src/app/actions/painel-centros.ts)
//   carregarPainelRecorrentes (apps/web/src/app/actions/painel-recorrentes.ts)
// e uma consulta de referência do gasto pela data da compra (grill Q18).
//
// Rodar com `bun --preload ./preparo.ts paineis.ts` e SBF_OURO_PG apontando para o banco.
import { SQL } from "bun";

const semente = await Bun.file(new URL("./livro-ficticio.json", import.meta.url)).json();
const url = process.env.SBF_OURO_PG;
if (!url) throw new Error("SBF_OURO_PG não definido");
if (!["127.0.0.1", "localhost"].includes(new URL(url).hostname)) {
  throw new Error("SBF_OURO_PG precisa ser um Postgres local de teste");
}
const sql = new SQL(url);

const reais = (c: number | null | undefined) => (c == null ? null : (c / 100).toFixed(2));
const taxa = (t: { milionesimos: number } | null) => (t == null ? null : (t.milionesimos / 1e6).toFixed(6));

async function inserir(tabela: string, linhas: Record<string, unknown>[]) {
  for (const linha of linhas) {
    await sql`INSERT INTO ${sql(tabela)} ${sql(linha)}`;
  }
}

async function gravar(dono: string, l: any) {
  const u = (x: Record<string, unknown>) => ({ user_id: dono, ...x });
  await inserir("user_financial_preferences", [u({ financial_cycle_start_day: l.corte })]);
  await inserir(
    "institutions",
    (l.instituicoes ?? []).map((i: any) => u({ id: i.id, name: i.nome, type: i.tipo, is_active: i.ativa })),
  );
  await inserir(
    "financial_products",
    (l.contas ?? []).map((c: any) =>
      u({
        id: c.id,
        institution_id: c.instituicao_id,
        name: c.nome,
        type: c.tipo,
        current_balance: reais(c.saldo),
        credit_limit: reais(c.limite),
        is_active: c.ativa,
        display_order: c.ordem,
      }),
    ),
  );
  await inserir(
    "cards",
    (l.cartoes ?? []).map((c: any) =>
      u({
        id: c.id,
        financial_product_id: c.conta_id,
        last_four_digits: c.final_do_numero,
        card_brand: c.bandeira,
        credit_limit: reais(c.limite),
        closing_day: c.dia_de_fechamento,
        due_day: c.dia_de_vencimento,
        is_active: c.ativo,
      }),
    ),
  );
  await inserir(
    "statement_cycles",
    (l.faturas ?? []).map((f: any) =>
      u({
        id: f.id,
        card_id: f.cartao_id,
        reference_month: f.mes_de_referencia,
        cycle_start_date: f.inicio,
        cycle_end_date: f.fechamento,
        due_date: f.vencimento,
        total_amount: reais(f.total),
        paid_amount: reais(f.pago),
        status: f.status,
      }),
    ),
  );
  await inserir(
    "categories",
    (l.categorias ?? []).map((c: any) => u({ id: c.id, name: c.nome, parent_id: c.pai_id, is_active: c.ativa })),
  );
  await inserir(
    "suppliers",
    (l.fornecedores ?? []).map((f: any) =>
      u({ id: f.id, name: f.nome, trade_name: f.nome_fantasia, legal_name: f.razao_social, is_active: f.ativo }),
    ),
  );
  await inserir(
    "liabilities",
    (l.dividas ?? []).map((d: any) =>
      u({
        id: d.id,
        financial_product_id: d.conta_id,
        name: d.nome,
        type: d.tipo,
        original_amount: reais(d.valor_original),
        outstanding_balance: reais(d.saldo_devedor),
        interest_rate: taxa(d.taxa),
        rate_type: d.tipo_de_taxa,
        amortization_system: d.sistema,
        total_installments: d.total_de_parcelas,
        paid_installments: d.parcelas_pagas,
        start_date: d.inicio,
        end_date: d.fim,
        status: d.status,
        supplier_id: d.fornecedor_id,
      }),
    ),
  );
  await inserir(
    "liability_installments",
    (l.parcelas ?? []).map((p: any) =>
      u({
        id: p.id,
        liability_id: p.divida_id,
        installment_number: p.numero,
        due_date: p.vencimento,
        total_amount: reais(p.total),
        paid_amount: reais(p.pago),
        paid_date: p.data_do_pagamento,
        status: p.status,
      }),
    ),
  );
  await inserir(
    "recurring_templates",
    (l.recorrentes ?? []).map((r: any) =>
      u({
        id: r.id,
        name: r.nome,
        type: r.tipo,
        financial_product_id: r.conta_id,
        amount: reais(r.valor),
        is_variable_amount: r.valor_variavel,
        frequency: r.frequencia,
        day_of_month: r.dia_do_mes,
        custom_interval_days: r.intervalo_dias,
        category_id: r.categoria_id,
        supplier_id: r.fornecedor_id,
        starts_at: r.inicia_em,
        ends_at: r.termina_em,
        is_active: r.ativo,
      }),
    ),
  );
  // Movimentações antes das ocorrências (a ocorrência aponta para a movimentação), e o
  // `recurring_instance_id` da movimentação depois, quando a ocorrência já existe.
  await inserir(
    "transactions",
    (l.movimentacoes ?? []).map((m: any) =>
      u({
        id: m.id,
        financial_product_id: m.conta_id,
        type: m.tipo,
        amount: reais(m.valor),
        description: m.descricao,
        event_date: m.data,
        competence_date: m.data_competencia,
        statement_cycle_id: m.fatura_id,
        liability_installment_id: m.parcela_id,
        category_id: m.categoria_id,
        supplier_id: m.fornecedor_id,
        is_confirmed: m.confirmada,
      }),
    ),
  );
  await inserir(
    "recurring_instances",
    (l.ocorrencias ?? []).map((o: any) =>
      u({
        id: o.id,
        recurring_template_id: o.recorrente_id,
        expected_date: o.data_esperada,
        expected_amount: reais(o.valor_esperado),
        actual_amount: reais(o.valor_real),
        status: o.status,
        paid_date: o.data_do_pagamento,
        transaction_id: o.movimentacao_id,
      }),
    ),
  );
  for (const m of l.movimentacoes ?? []) {
    if (m.ocorrencia_id) {
      await sql`UPDATE transactions SET recurring_instance_id = ${m.ocorrencia_id} WHERE id = ${m.id}`;
    }
  }
  await inserir(
    "statement_items",
    (l.itens_de_fatura ?? []).map((i: any) =>
      u({
        id: i.id,
        statement_cycle_id: i.fatura_id,
        transaction_id: i.movimentacao_id,
        description: i.descricao,
        amount: reais(i.valor),
        transaction_date: i.data_da_compra,
        installment_number: i.parcela,
        total_installments: i.total_de_parcelas,
        supplier_id: i.fornecedor_id,
      }),
    ),
  );
  await inserir(
    "transfers",
    (l.transferencias ?? []).map((t: any) =>
      u({
        id: t.id,
        source_product_id: t.conta_origem_id,
        target_product_id: t.conta_destino_id,
        amount: reais(t.valor),
        description: t.descricao,
        event_date: t.data,
        is_confirmed: t.confirmada,
      }),
    ),
  );
  await inserir(
    "financial_centers",
    (l.centros ?? []).map((c: any) =>
      u({
        id: c.id,
        name: c.nome,
        type: c.tipo,
        status: c.status,
        description: c.descricao,
        icon: c.icone,
        color: c.cor,
      }),
    ),
  );
  await inserir(
    "transaction_allocations",
    (l.rateios ?? []).map((r: any) =>
      u({
        id: r.id,
        transaction_id: r.movimentacao_id,
        financial_center_id: r.centro_id,
        amount: reais(r.valor),
        note: r.nota,
      }),
    ),
  );
  await inserir("tags", (l.tags ?? []).map((t: any) => u({ id: t.id, name: t.nome })));
  await inserir(
    "transaction_tags",
    (l.tags_das_movimentacoes ?? []).map(([t, g]: [string, string]) => ({ transaction_id: t, tag_id: g })),
  );
}

for (const { id, livro } of Object.values(semente.donos) as any[]) {
  await gravar(id, livro);
}

// O relógio fica parado no instante da semente: `new Date()` e `Date.now()` das actions.
const instante = new Date(semente.instante).getTime();
const DateReal = Date;
class DateParado extends DateReal {
  constructor(...args: any[]) {
    if (args.length === 0) super(instante);
    else super(...(args as [any]));
  }
  static now() {
    return instante;
  }
}
globalThis.Date = DateParado as DateConstructor;

process.env.SBF_OURO_DONO = semente.donos.a.id;
const { carregarPainelHoje } = await import("../../../apps/web/src/app/actions/painel-hoje.ts");
const { carregarPainelCentros } = await import("../../../apps/web/src/app/actions/painel-centros.ts");
const { carregarPainelRecorrentes } = await import(
  "../../../apps/web/src/app/actions/painel-recorrentes.ts"
);

const hoje = await carregarPainelHoje();
const centros = await carregarPainelCentros();
const recorrentes = await carregarPainelRecorrentes();

// Consulta de referência do gasto pela data da compra (grill Q18): compras no cartão
// (movimentações com fatura e itens de fatura que não viraram movimentação, na data da
// compra) e saídas de conta, sem pagamento de fatura, sem transferência e sem ajuste. Só
// movimentação confirmada: a não confirmada é compromisso, ainda não saiu.
const gastos = await sql`
  SELECT data::text AS data, sum(valor)::text AS valor, count(*)::int AS vezes FROM (
    SELECT t.event_date AS data, abs(t.amount) AS valor FROM transactions t
     WHERE t.user_id = ${semente.donos.a.id}
       AND t.type IN ('expense', 'fee', 'interest_charge', 'liability_payment')
       AND t.is_confirmed
    UNION ALL
    SELECT si.transaction_date, abs(si.amount) FROM statement_items si
     WHERE si.user_id = ${semente.donos.a.id} AND si.transaction_id IS NULL
  ) g GROUP BY data ORDER BY data`;

console.log(
  JSON.stringify(
    {
      fonte: [
        "apps/web/src/app/actions/painel-hoje.ts",
        "apps/web/src/app/actions/painel-centros.ts",
        "apps/web/src/app/actions/painel-recorrentes.ts",
      ],
      tz: process.env.TZ ?? "",
      instante: semente.instante,
      hoje,
      centros,
      recorrentes,
      gastosPorDia: gastos,
    },
    null,
    1,
  ),
);
process.exit(0);
