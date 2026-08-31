import "./test-db-setup";
import { afterEach, describe, expect, it, vi } from "vitest";
import { eq, inArray, sql } from "drizzle-orm";
import { getDb, schema } from "@sbf/db";
import {
  aprovarItensDaFila,
  carregarPainelInteligencia,
  rejeitarItensDaFila,
} from "./painel-inteligencia";

let currentUserId = "";
// `ownerId` é o dono dos dados; `id` é o ator. Os dois são DIFERENTES de
// propósito (mesmo truque de patterns.test.ts): um filtro feito por `id` em
// vez de `ownerId`, por engano, vira uma falha visível aqui em vez de passar
// despercebido.
vi.mock("@/lib/auth", () => ({
  requireUser: async () => ({
    id: "00000000-0000-0000-0000-00000000ac70",
    ownerId: currentUserId,
    email: "teste@exemplo.com",
    role: "owner" as const,
    name: "Teste",
  }),
}));

const USER_A = "00000000-0000-0000-0000-0000000000a1";
const USER_B = "00000000-0000-0000-0000-0000000000b2";

async function seedDraft(
  userId: string,
  overrides: Partial<typeof schema.draft_records.$inferInsert> = {},
) {
  const db = getDb();
  const [row] = await db
    .insert(schema.draft_records)
    .values({
      user_id: userId,
      draft_type: "transaction",
      status: "pending_review",
      draft_data: { description: "Lançamento de teste", amount_cents: 12345, direction: "outflow" },
      // NOT NULL sem default no Drizzle: quem preenche é o trigger
      // `fn_draft_records_set_materialization_key` (ver draft-generator.ts).
      materialization_key: sql`default`,
      ...overrides,
    })
    .returning();
  if (!row) throw new Error("Falha ao semear draft_record de teste");
  return row;
}

async function seedPattern(
  userId: string,
  overrides: Partial<typeof schema.document_patterns.$inferInsert> = {},
) {
  const db = getDb();
  const [row] = await db
    .insert(schema.document_patterns)
    .values({
      user_id: userId,
      name: `Padrão de teste ${crypto.randomUUID()}`,
      document_type: "boleto",
      ...overrides,
    })
    .returning();
  if (!row) throw new Error("Falha ao semear document_pattern de teste");
  return row;
}

async function seedDocument(userId: string) {
  const db = getDb();
  const [row] = await db
    .insert(schema.source_documents)
    .values({
      user_id: userId,
      origin_type: "manual_upload",
      origin_key: `test::${crypto.randomUUID()}`,
      filename: "boleto-teste.pdf",
      status: "new",
    })
    .returning();
  if (!row) throw new Error("Falha ao semear source_document de teste");
  return row;
}

async function seedBatch(userId: string) {
  const db = getDb();
  const [row] = await db.insert(schema.draft_batches).values({ user_id: userId }).returning();
  if (!row) throw new Error("Falha ao semear draft_batch de teste");
  return row;
}

describe.skipIf(!process.env.TEST_DATABASE_URL)("painel-inteligencia actions", () => {
  const draftIds: string[] = [];
  const patternIds: string[] = [];
  const docIds: string[] = [];
  const batchIds: string[] = [];

  afterEach(async () => {
    const db = getDb();
    for (const id of draftIds.splice(0)) {
      await db.delete(schema.draft_records).where(eq(schema.draft_records.id, id));
    }
    for (const id of patternIds.splice(0)) {
      await db.delete(schema.document_patterns).where(eq(schema.document_patterns.id, id));
    }
    for (const id of batchIds.splice(0)) {
      await db.delete(schema.draft_batches).where(eq(schema.draft_batches.id, id));
    }
    for (const id of docIds.splice(0)) {
      await db.delete(schema.source_documents).where(eq(schema.source_documents.id, id));
    }
  });

  it("isola fila e padrões por usuário", async () => {
    currentUserId = USER_A;
    const draftA = await seedDraft(USER_A);
    draftIds.push(draftA.id);
    const patternA = await seedPattern(USER_A);
    patternIds.push(patternA.id);

    currentUserId = USER_B;
    const draftB = await seedDraft(USER_B);
    draftIds.push(draftB.id);
    const patternB = await seedPattern(USER_B);
    patternIds.push(patternB.id);

    currentUserId = USER_A;
    const painel = await carregarPainelInteligencia();
    expect(painel.fila.map((i) => i.id)).toContain(draftA.id);
    expect(painel.fila.map((i) => i.id)).not.toContain(draftB.id);
    expect(painel.padroes.map((p) => p.id)).toContain(patternA.id);
    expect(painel.padroes.map((p) => p.id)).not.toContain(patternB.id);
  });

  it("ordena a fila do item mais antigo para o mais novo", async () => {
    currentUserId = USER_A;
    const antigo = await seedDraft(USER_A, { created_at: "2020-01-01T00:00:00.000Z" });
    draftIds.push(antigo.id);
    const novo = await seedDraft(USER_A, { created_at: "2030-01-01T00:00:00.000Z" });
    draftIds.push(novo.id);

    const painel = await carregarPainelInteligencia();
    const ids = painel.fila.map((i) => i.id);
    expect(ids.indexOf(antigo.id)).toBeLessThan(ids.indexOf(novo.id));
  });

  it("resume um draft de transação em nome/valor/detalhe e nunca finge saber a direção", async () => {
    currentUserId = USER_A;
    const draft = await seedDraft(USER_A, {
      draft_data: {
        description: "Conta de luz",
        amount_cents: 23456,
        // `direction` propositalmente ausente: a extração pode não saber.
        category_suggestion: "Utilidades",
        event_date: "2026-03-10",
      },
    });
    draftIds.push(draft.id);

    const painel = await carregarPainelInteligencia();
    const item = painel.fila.find((i) => i.id === draft.id);
    expect(item?.nome).toBe("Conta de luz");
    expect(item?.valor).toBeCloseTo(234.56);
    expect(item?.detalhe).toContain("uma movimentação");
    expect(item?.detalhe).not.toContain("uma saída");
    expect(item?.detalhe).toContain("Utilidades");
    expect(item?.icone).toBe("receipt_long");
  });

  it("marca confiança baixa em palavras e avisa para conferir os dados", async () => {
    currentUserId = USER_A;
    const draft = await seedDraft(USER_A, { confidence_score: 0.3 });
    draftIds.push(draft.id);

    const painel = await carregarPainelInteligencia();
    const item = painel.fila.find((i) => i.id === draft.id);
    expect(item?.confidenceLabel).toBe("Confiança baixa · 30%");
    expect(item?.detalhe).toContain("Confira os dados originais antes de confirmar.");
  });

  it("sinaliza item já corrigido a partir de corrections.manual_edit", async () => {
    currentUserId = USER_A;
    const draft = await seedDraft(USER_A, {
      status: "corrected",
      corrections: { manual_edit: true, edited_at: new Date().toISOString() },
    });
    draftIds.push(draft.id);

    const painel = await carregarPainelInteligencia();
    const item = painel.fila.find((i) => i.id === draft.id);
    expect(item?.detalhe).toContain("Você já corrigiu este item");
  });

  it("aponta o link de correção para o lote quando há batch_id, para o documento quando não há, e null sem nenhum dos dois", async () => {
    currentUserId = USER_A;
    const batch = await seedBatch(USER_A);
    batchIds.push(batch.id);
    const doc = await seedDocument(USER_A);
    docIds.push(doc.id);

    const comLote = await seedDraft(USER_A, { batch_id: batch.id, source_document_id: doc.id });
    draftIds.push(comLote.id);
    const soComDocumento = await seedDraft(USER_A, { batch_id: null, source_document_id: doc.id });
    draftIds.push(soComDocumento.id);
    const semNenhum = await seedDraft(USER_A, { batch_id: null, source_document_id: null });
    draftIds.push(semNenhum.id);

    const painel = await carregarPainelInteligencia();
    const acha = (id: string) => painel.fila.find((i) => i.id === id);

    // batch_id manda, mesmo quando source_document_id também existe.
    expect(acha(comLote.id)?.linkCorrigir).toBe(`/dashboard/ingestion/review/${batch.id}`);
    expect(acha(soComDocumento.id)?.linkCorrigir).toBe(`/dashboard/documents/${doc.id}`);
    expect(acha(semNenhum.id)?.linkCorrigir).toBeNull();
  });

  it("calcula desempenho honestamente: sem correção, com correção e rejeitado não se confundem", async () => {
    currentUserId = USER_A;
    const antes = await carregarPainelInteligencia();

    const semCorrecao = await seedDraft(USER_A, { status: "approved", corrections: null });
    draftIds.push(semCorrecao.id);
    const comCorrecao = await seedDraft(USER_A, {
      status: "approved",
      corrections: { manual_edit: true, edited_at: new Date().toISOString() },
    });
    draftIds.push(comCorrecao.id);
    const rejeitado = await seedDraft(USER_A, { status: "rejected" });
    draftIds.push(rejeitado.id);
    // `corrections` com outro formato (motivo de rejeição, não edição manual)
    // não pode ser contado como "correção" — é só um registro de rejeição.
    const rejeitadoComMotivo = await seedDraft(USER_A, {
      status: "rejected",
      corrections: { rejection_reason: "duplicado" },
    });
    draftIds.push(rejeitadoComMotivo.id);

    const depois = await carregarPainelInteligencia();
    expect(depois.desempenho.semCorrecao - antes.desempenho.semCorrecao).toBe(1);
    expect(depois.desempenho.comCorrecao - antes.desempenho.comCorrecao).toBe(1);
    expect(depois.desempenho.rejeitados - antes.desempenho.rejeitados).toBe(2);
    expect(depois.desempenho.processados - antes.desempenho.processados).toBe(2);
  });

  it("taxaSemCorrecaoPct é null sem nenhum documento processado — não existe taxa de 0 itens", async () => {
    // UUID aleatório, não USER_A/USER_B: garante zero draft concluído
    // pré-existente, mesmo neste banco de teste compartilhado entre arquivos
    // (ver o comentário sobre fileParallelism em vitest.config.ts).
    currentUserId = crypto.randomUUID();
    const painel = await carregarPainelInteligencia();
    expect(painel.desempenho.processados).toBe(0);
    expect(painel.desempenho.taxaSemCorrecaoPct).toBeNull();
  });

  it("calcula taxa de sucesso de padrão e marca 'em risco' com o mesmo critério da tela antiga", async () => {
    currentUserId = USER_A;
    const arriscado = await seedPattern(USER_A, { success_count: 1, feedback_count: 4 }); // 20%
    patternIds.push(arriscado.id);
    const saudavel = await seedPattern(USER_A, { success_count: 9, feedback_count: 1 }); // 90%
    patternIds.push(saudavel.id);
    const semHistorico = await seedPattern(USER_A); // 0/0 -> 100%, não em risco
    patternIds.push(semHistorico.id);

    const painel = await carregarPainelInteligencia();
    const pArriscado = painel.padroes.find((p) => p.id === arriscado.id);
    const pSaudavel = painel.padroes.find((p) => p.id === saudavel.id);
    const pSemHistorico = painel.padroes.find((p) => p.id === semHistorico.id);

    expect(pArriscado?.taxaSucessoPct).toBe(20);
    expect(pArriscado?.emRisco).toBe(true);
    expect(pSaudavel?.taxaSucessoPct).toBe(90);
    expect(pSaudavel?.emRisco).toBe(false);
    expect(pSemHistorico?.taxaSucessoPct).toBe(100);
    expect(pSemHistorico?.emRisco).toBe(false);
  });

  it("aprova o item do dono e reporta falha isolada para item de outro usuário", async () => {
    currentUserId = USER_A;
    const meu = await seedDraft(USER_A);
    draftIds.push(meu.id);

    currentUserId = USER_B;
    const alheio = await seedDraft(USER_B);
    draftIds.push(alheio.id);

    currentUserId = USER_A;
    const resultado = await aprovarItensDaFila([meu.id, alheio.id]);
    expect(resultado.ok).toEqual([meu.id]);
    expect(resultado.falhas.map((f) => f.id)).toEqual([alheio.id]);

    const db = getDb();
    const [linhaAlheia] = await db
      .select({ status: schema.draft_records.status })
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, alheio.id));
    // A falha reportada precisa corresponder a NENHUMA escrita — não só a um
    // erro relatado enquanto o dado de outro usuário muda por baixo.
    expect(linhaAlheia?.status).toBe("pending_review");

    const [linhaMinha] = await db
      .select({ status: schema.draft_records.status })
      .from(schema.draft_records)
      .where(eq(schema.draft_records.id, meu.id));
    expect(linhaMinha?.status).toBe("approved");
  });

  it("rejeita itens do dono em lote", async () => {
    currentUserId = USER_A;
    const um = await seedDraft(USER_A);
    draftIds.push(um.id);
    const dois = await seedDraft(USER_A);
    draftIds.push(dois.id);

    const resultado = await rejeitarItensDaFila([um.id, dois.id]);
    expect(resultado.ok.sort()).toEqual([um.id, dois.id].sort());
    expect(resultado.falhas).toEqual([]);

    const db = getDb();
    const linhas = await db
      .select({ id: schema.draft_records.id, status: schema.draft_records.status })
      .from(schema.draft_records)
      .where(inArray(schema.draft_records.id, [um.id, dois.id]));
    for (const id of [um.id, dois.id]) {
      expect(linhas.find((l) => l.id === id)?.status).toBe("rejected");
    }
  });
});
