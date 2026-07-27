/* eslint-disable @typescript-eslint/no-explicit-any */
/**
 * Testes de integração: convergência de evidências para obrigações canônicas.
 *
 * O que estes testes protegem: as tabelas financial_obligations e
 * financial_obligation_evidences existiam desde maio e nenhuma linha de código
 * jamais as tocou. Agora que o pipeline escreve nelas, o comportamento crítico
 * é que documentos diferentes sobre a MESMA conta convirjam — e que contas
 * diferentes NÃO se fundam.
 *
 * Requer `supabase start`.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import { buildFinancialIdentityKeys } from "@sbf/domain";

const SUPABASE_URL = process.env.SUPABASE_URL ?? "http://127.0.0.1:54321";
const SUPABASE_SERVICE_KEY =
  process.env.SUPABASE_SECRET_KEY ??
  "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZS1kZW1vIiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImV4cCI6MTk4MzgxMjk5Nn0.EGIM96RAZx35lJzdJsyH-qQwv8Hdp7fsn3W0YpN81IU";

let supabase: SupabaseClient;
let userId: string;

/** Cria um source_document distinto para simular canais diferentes. */
async function createDocument(originKey: string): Promise<string> {
  const { data, error } = await supabase
    .from("source_documents")
    .insert({
      user_id: userId,
      origin_type: "local_file",
      origin_key: originKey,
      filename: `${originKey}.pdf`,
      mime_type: "application/pdf",
      status: "active",
    })
    .select("id")
    .single();
  if (error) throw new Error(`createDocument: ${error.message}`);
  return data!.id as string;
}

interface UpsertArgs {
  documentId: string;
  intent?: string;
  supplierName?: string;
  amount?: number | null;
  dueDate?: string;
  documentNumber?: string;
  barcode?: string;
  cardLast4?: string;
  institutionName?: string;
}

async function upsert(args: UpsertArgs) {
  const keys = buildFinancialIdentityKeys({
    userId,
    supplierName: args.supplierName ?? null,
    institutionName: args.institutionName ?? null,
    amount: args.amount ?? null,
    dueDate: args.dueDate ?? null,
    documentNumber: args.documentNumber ?? null,
    barcodeDigitableLine: args.barcode ?? null,
    cardLast4: args.cardLast4 ?? null,
  });

  const { data, error } = await supabase.rpc("fn_upsert_financial_obligation", {
    p_user_id: userId,
    p_payload: {
      obligation_type: args.intent ?? "bill_to_pay",
      supplier_name_raw: args.supplierName ?? null,
      amount: args.amount ?? null,
      due_date: args.dueDate ?? null,
      document_number: args.documentNumber ?? null,
      barcode_digitable_line: args.barcode ?? null,
      financial_identity_key: keys.primary,
      metadata: {},
    },
    p_keys: keys.entries,
    p_evidence: { source_document_id: args.documentId, reasons: [] },
  });

  if (error) throw new Error(`upsert: ${error.message}`);
  return data as {
    obligation_id: string;
    is_new: boolean;
    matched_by: string;
    evidence_count: number;
  };
}

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  const email = "test-obligations@sbf.local";
  const created = await supabase.auth.admin.createUser({
    email,
    password: "TestPass123!",
    email_confirm: true,
  });
  if (created.data?.user?.id) {
    userId = created.data.user.id;
  } else {
    // Cliente descartável: logar no cliente compartilhado o rebaixaria de
    // service_role para usuário comum.
    const probe = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
      auth: { persistSession: false, autoRefreshToken: false },
    });
    const signedIn = await probe.auth.signInWithPassword({ email, password: "TestPass123!" });
    userId = signedIn.data!.user!.id;
  }
});

beforeEach(async () => {
  await supabase.from("financial_obligation_evidences").delete().eq("user_id", userId);
  await supabase.from("financial_obligation_identity_keys").delete().eq("user_id", userId);
  await supabase.from("financial_obligations").delete().eq("user_id", userId);
  await supabase.from("source_documents").delete().eq("user_id", userId);
});

afterAll(async () => {
  await supabase.from("financial_obligation_evidences").delete().eq("user_id", userId);
  await supabase.from("financial_obligation_identity_keys").delete().eq("user_id", userId);
  await supabase.from("financial_obligations").delete().eq("user_id", userId);
  await supabase.from("source_documents").delete().eq("user_id", userId);
});

const BARCODE = "83640000001 2 34560000000 3 45670000000 4 56780000000 5";

describe("convergência entre canais", () => {
  it("o mesmo boleto por dois canais vira 1 obrigação com 2 evidências", async () => {
    const viaGmail = await createDocument("gmail:msg-1");
    const viaPasta = await createDocument("local:/inbox/boleto.pdf");

    const first = await upsert({ documentId: viaGmail, barcode: BARCODE, amount: 245.5 });
    const second = await upsert({ documentId: viaPasta, barcode: BARCODE, amount: 245.5 });

    expect(first.is_new).toBe(true);
    expect(second.is_new).toBe(false);
    expect(second.obligation_id).toBe(first.obligation_id);
    expect(second.matched_by).toBe("barcode");
    expect(second.evidence_count).toBe(2);

    const { count } = await supabase
      .from("financial_obligations")
      .select("id", { count: "exact", head: true })
      .eq("user_id", userId);
    expect(count).toBe(1);
  });

  it("fatura e lembrete da mesma conta convergem", async () => {
    // O defeito original: prefixos e aridades diferentes tornavam a colisão
    // matematicamente impossível.
    const fatura = await createDocument("gmail:fatura");
    const lembrete = await createDocument("gmail:lembrete");

    const a = await upsert({
      documentId: fatura,
      institutionName: "Nubank",
      cardLast4: "1234",
      dueDate: "2026-04-10",
      amount: 1200,
      intent: "invoice_statement",
    });
    const b = await upsert({
      documentId: lembrete,
      institutionName: "Nubank",
      dueDate: "2026-04-10",
      amount: 1200,
      intent: "bill_reminder",
    });

    expect(b.obligation_id).toBe(a.obligation_id);
    expect(b.evidence_count).toBe(2);
  });

  it("a evidência primária é a primeira; as demais são de apoio", async () => {
    const d1 = await createDocument("local:/a.pdf");
    const d2 = await createDocument("local:/b.pdf");
    await upsert({ documentId: d1, barcode: BARCODE });
    await upsert({ documentId: d2, barcode: BARCODE });

    const { data } = await supabase
      .from("financial_obligation_evidences")
      .select("source_document_id, evidence_role")
      .eq("user_id", userId);

    const roles = Object.fromEntries(
      (data ?? []).map((e: any) => [e.source_document_id, e.evidence_role]),
    );
    expect(roles[d1]).toBe("primary");
    expect(roles[d2]).toBe("supporting");
  });

  it("reprocessar o mesmo documento não duplica a evidência", async () => {
    const doc = await createDocument("local:/mesmo.pdf");
    await upsert({ documentId: doc, barcode: BARCODE });
    const again = await upsert({ documentId: doc, barcode: BARCODE });
    expect(again.evidence_count).toBe(1);
  });
});

describe("contas distintas não podem se fundir", () => {
  it("mesma conta em meses diferentes gera obrigações separadas", async () => {
    const marco = await createDocument("local:/cemig-03.pdf");
    const abril = await createDocument("local:/cemig-04.pdf");

    const a = await upsert({
      documentId: marco,
      supplierName: "CEMIG",
      dueDate: "2026-03-10",
      amount: 245.5,
    });
    const b = await upsert({
      documentId: abril,
      supplierName: "CEMIG",
      dueDate: "2026-04-10",
      amount: 245.5,
    });

    expect(b.obligation_id).not.toBe(a.obligation_id);
  });

  it("mesmo fornecedor e mês, valores diferentes, não se fundem pela chave fraca", async () => {
    // Ambos emitem a chave fraca fornecedor+mês. A RPC precisa recusar o
    // casamento porque os valores conhecidos divergem — caso contrário duas
    // contas distintas do mesmo fornecedor virariam uma só.
    const luz = await createDocument("local:/cemig-luz.pdf");
    const outra = await createDocument("local:/cemig-outra.pdf");

    const a = await upsert({
      documentId: luz,
      supplierName: "CEMIG",
      dueDate: "2026-03-10",
      amount: 245.5,
    });
    const b = await upsert({
      documentId: outra,
      supplierName: "CEMIG",
      dueDate: "2026-03-20",
      amount: 99,
    });

    expect(b.obligation_id).not.toBe(a.obligation_id);
  });

  it("fornecedores diferentes no mesmo mês e valor não se fundem", async () => {
    const cemig = await createDocument("local:/cemig.pdf");
    const vivo = await createDocument("local:/vivo.pdf");

    const a = await upsert({
      documentId: cemig,
      supplierName: "CEMIG",
      dueDate: "2026-04-10",
      amount: 100,
    });
    const b = await upsert({
      documentId: vivo,
      supplierName: "Vivo",
      dueDate: "2026-04-10",
      amount: 100,
    });

    expect(b.obligation_id).not.toBe(a.obligation_id);
  });
});

describe("propriedades da obrigação", () => {
  it("um documento posterior preenche lacunas sem apagar o que já se sabia", async () => {
    const semValor = await createDocument("local:/sem-valor.pdf");
    const comValor = await createDocument("local:/com-valor.pdf");

    const a = await upsert({
      documentId: semValor,
      supplierName: "CEMIG",
      documentNumber: "NF-1",
      amount: null,
    });
    await upsert({
      documentId: comValor,
      supplierName: "CEMIG",
      documentNumber: "NF-1",
      amount: 245.5,
      dueDate: "2026-04-10",
    });

    const { data } = await supabase
      .from("financial_obligations")
      .select("amount, due_date, supplier_name_raw")
      .eq("id", a.obligation_id)
      .single();

    expect(Number(data!.amount)).toBe(245.5);
    expect(data!.due_date).toBe("2026-04-10");
    expect(data!.supplier_name_raw).toBe("CEMIG");
  });

  it("upserts concorrentes da mesma chave produzem uma obrigação só", async () => {
    const d1 = await createDocument("local:/c1.pdf");
    const d2 = await createDocument("local:/c2.pdf");
    const d3 = await createDocument("local:/c3.pdf");

    const results = await Promise.allSettled([
      upsert({ documentId: d1, barcode: BARCODE }),
      upsert({ documentId: d2, barcode: BARCODE }),
      upsert({ documentId: d3, barcode: BARCODE }),
    ]);

    const ok = results.filter((r) => r.status === "fulfilled");
    expect(ok.length).toBeGreaterThan(0);

    const { count } = await supabase
      .from("financial_obligations")
      .select("id", { count: "exact", head: true })
      .eq("user_id", userId);
    expect(count).toBe(1);
  });

  it("todas as chaves computáveis viram aliases da obrigação", async () => {
    const doc = await createDocument("local:/rico.pdf");
    const r = await upsert({
      documentId: doc,
      barcode: BARCODE,
      supplierName: "CEMIG",
      documentNumber: "NF-9",
      dueDate: "2026-04-10",
      amount: 245.5,
    });

    const { data } = await supabase
      .from("financial_obligation_identity_keys")
      .select("key_kind")
      .eq("obligation_id", r.obligation_id);

    const kinds = new Set((data ?? []).map((k: any) => k.key_kind));
    expect(kinds.has("barcode")).toBe(true);
    expect(kinds.has("docnum")).toBe(true);
    expect(kinds.has("supplier_period_amount")).toBe(true);
  });
});
