/**
 * Migração v0 → v1 e leitura tolerante.
 *
 * Os payloads v0 aqui são cópias literais do que
 * `workers/ingestion/src/drafts/draft-generator.ts` gravava em `draft_data`
 * antes da unificação. Se a migração quebrar, drafts históricos param de
 * abrir na tela de revisão — por isso cada forma antiga tem um caso.
 */
import { describe, it, expect } from "vitest";
import { migrateV0ToV1 } from "./migrate-v0";
import { parseDraftPayload } from "./parse";
import { DRAFT_SCHEMAS, POSTABLE_SCHEMAS } from "./index";

/** Saída literal de buildTransactionDraft (v0). */
const V0_TRANSACTION = {
  type: "despesa",
  description: "CEMIG",
  amount: 245.5,
  currency: "BRL",
  due_date: "2026-04-15",
  competence_date: "2026-03-01",
  category: "energia_eletrica",
  tags: ["essencial"],
  supplier_name: "CEMIG",
  supplier_id: null,
  document_number: "123456",
  contract_identifier: "12345678",
};

/** Saída literal de buildRecurringTemplateDraft (v0). */
const V0_RECURRING = {
  name: "CEMIG - mensal",
  type: "despesa",
  recurrence: "monthly",
  base_amount: 245.5,
  currency: "BRL",
  category: "energia_eletrica",
  tags: [],
  supplier_name: "CEMIG",
  supplier_id: null,
  contract_identifier: "12345678",
};

/** Saída literal de buildConsumptionMetricDraft (v0). */
const V0_CONSUMPTION = {
  supplier_name: "CEMIG",
  contract_identifier: "12345678",
  competence_date: "2026-03-01",
  kwh: 320,
  m3: null,
  days: 30,
  amount: 245.5,
  unit_cost: null,
};

describe("migrateV0ToV1 — transaction", () => {
  const v1 = migrateV0ToV1("transaction", V0_TRANSACTION) as Record<string, unknown>;

  it("produz um payload válido no schema v1", () => {
    expect(DRAFT_SCHEMAS.transaction.safeParse(v1).success).toBe(true);
  });

  it('traduz o literal "despesa" em direction + transaction_type', () => {
    expect(v1.direction).toBe("outflow");
    expect(v1.transaction_type).toBe("expense");
    expect(v1).not.toHaveProperty("type");
  });

  it("converte amount para centavos", () => {
    expect(v1.amount_cents).toBe(24550);
  });

  it("rebaixa a categoria textual para sugestão, sem forjar um category_id", () => {
    expect(v1.category_suggestion).toBe("energia_eletrica");
    expect(v1.category_id).toBeNull();
  });

  it("não inventa event_date, porque v0 nunca o gravou", () => {
    expect(v1.event_date).toBeNull();
    expect(v1.due_date).toBe("2026-04-15");
  });

  it("continua não lançável: a lacuna da conta é preservada, não mascarada", () => {
    expect(POSTABLE_SCHEMAS.transaction.safeParse(v1).success).toBe(false);
  });
});

describe("migrateV0ToV1 — recurring", () => {
  const v1 = migrateV0ToV1("recurring_template", V0_RECURRING) as Record<string, unknown>;

  it("mapeia recurrence → frequency e base_amount → amount_cents", () => {
    expect(v1.frequency).toBe("monthly");
    expect(v1.amount_cents).toBe(24550);
    expect(v1).not.toHaveProperty("recurrence");
    expect(v1).not.toHaveProperty("base_amount");
  });

  it("marca evidence_count = 1, o que impede o lançamento", () => {
    // v0 criava recorrência a partir de um documento só. A migração preserva
    // o dado mas registra a fragilidade, e o schema de lançamento a rejeita.
    expect(v1.evidence_count).toBe(1);
    const res = POSTABLE_SCHEMAS.recurring_template.safeParse(v1);
    expect(res.success).toBe(false);
    if (!res.success) {
      expect(res.error.errors.map((e) => e.path.join("."))).toContain("evidence_count");
    }
  });

  it("registra na explicação de onde o candidato veio", () => {
    expect((v1.explanation as string[])[0]).toMatch(/v0/);
  });
});

describe("migrateV0ToV1 — consumption_metric", () => {
  const v1 = migrateV0ToV1("consumption_metric", V0_CONSUMPTION) as Record<string, unknown>;

  it("reconstrói o trio quantity/metric_name/metric_unit exigido pela CHECK", () => {
    expect(v1.quantity).toBe(320);
    expect(v1.metric_name).toBe("energia");
    expect(v1.metric_unit).toBe("kWh");
  });

  it("usa competence_date como início do período e deixa o fim para revisão", () => {
    expect(v1.reference_period_start).toBe("2026-03-01");
    expect(v1.reference_period_end).toBeNull();
  });
});

describe("migrateV0ToV1 — robustez", () => {
  it.each([[null], [undefined], [{}], ["texto solto"], [42]])(
    "não lança para draft_data = %p",
    (raw) => {
      expect(() => migrateV0ToV1("transaction", raw)).not.toThrow();
      const v1 = migrateV0ToV1("transaction", raw);
      expect(DRAFT_SCHEMAS.transaction.safeParse(v1).success).toBe(true);
    },
  );

  it("é determinística", () => {
    expect(migrateV0ToV1("transaction", V0_TRANSACTION)).toEqual(
      migrateV0ToV1("transaction", V0_TRANSACTION),
    );
  });
});

describe("parseDraftPayload — tolerância a backfill parcial", () => {
  it("migra em memória uma linha ainda em v0", () => {
    const res = parseDraftPayload({
      draft_type: "transaction",
      draft_data: V0_TRANSACTION,
      draft_schema_version: 0,
    });
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.migrated).toBe(true);
      expect(res.payload.draft_type).toBe("transaction");
    }
  });

  it("trata versão ausente como v0", () => {
    const res = parseDraftPayload({ draft_type: "transaction", draft_data: V0_TRANSACTION });
    expect(res.ok).toBe(true);
    if (res.ok) expect(res.migrated).toBe(true);
  });

  it("lê uma linha já migrada sem tocá-la", () => {
    const v1 = migrateV0ToV1("transaction", V0_TRANSACTION);
    const res = parseDraftPayload({
      draft_type: "transaction",
      draft_data: v1,
      draft_schema_version: 1,
    });
    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.migrated).toBe(false);
      expect(res.payload).toEqual(v1);
    }
  });

  it("reporta erro legível para draft_type desconhecido", () => {
    const res = parseDraftPayload({ draft_type: "foo", draft_data: {} });
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.errors[0]).toMatch(/desconhecido/);
  });
});
