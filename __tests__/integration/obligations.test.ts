/**
 * Testes de integração: convergência de evidências para obrigações canônicas.
 *
 * O que estes testes protegem: as tabelas financial_obligations e
 * financial_obligation_evidences existiam desde maio e nenhuma linha de código
 * jamais as tocou. Agora que o pipeline escreve nelas, o comportamento crítico
 * é que documentos diferentes sobre a MESMA conta convirjam — e que contas
 * diferentes NÃO se fundam.
 *
 * `fn_upsert_financial_obligation` continua sendo uma função do banco (recebe
 * `p_user_id` explícito, então sobreviveu à migração de backend); o que mudou
 * é quem a chama — Drizzle no lugar do driver anterior, e um erro do Postgres
 * agora é exceção em vez de um campo `error` que ninguém precisava ler.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { sql } from "drizzle-orm";
import { source_documents, type Database } from "@sbf/db";
import { buildFinancialIdentityKeys } from "@sbf/domain";
import { getTestDb, limparPorUsuario, linha, linhas, semBancoDeTeste } from "../helpers/db";

const USER_ID = "30000000-0000-4000-8000-000000000001";

/** Filhas antes das pais. */
const TABELAS = [
  "financial_obligation_evidences",
  "financial_obligation_identity_keys",
  "financial_obligations",
  "source_documents",
];

let db: Database;

/** Cria um source_document distinto para simular canais diferentes. */
async function createDocument(originKey: string): Promise<string> {
  const [doc] = await db
    .insert(source_documents)
    .values({
      user_id: USER_ID,
      origin_type: "local_file",
      origin_key: originKey,
      filename: `${originKey}.pdf`,
      mime_type: "application/pdf",
      status: "active",
    })
    .returning({ id: source_documents.id });
  return doc!.id;
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

interface UpsertResult {
  obligation_id: string;
  is_new: boolean;
  matched_by: string;
  evidence_count: number;
}

async function upsert(args: UpsertArgs): Promise<UpsertResult> {
  const keys = buildFinancialIdentityKeys({
    userId: USER_ID,
    supplierName: args.supplierName ?? null,
    institutionName: args.institutionName ?? null,
    amount: args.amount ?? null,
    dueDate: args.dueDate ?? null,
    documentNumber: args.documentNumber ?? null,
    barcodeDigitableLine: args.barcode ?? null,
    cardLast4: args.cardLast4 ?? null,
  });

  const payload = {
    obligation_type: args.intent ?? "bill_to_pay",
    supplier_name_raw: args.supplierName ?? null,
    amount: args.amount ?? null,
    due_date: args.dueDate ?? null,
    document_number: args.documentNumber ?? null,
    barcode_digitable_line: args.barcode ?? null,
    financial_identity_key: keys.primary,
    metadata: {},
  };

  const evidence = { source_document_id: args.documentId, reasons: [] };

  // A função devolve UM jsonb (não uma tabela), daí o alias explícito — mesmo
  // padrão de workers/ingestion/src/obligations/obligation-writer.ts.
  const row = await linha<{ resultado: UpsertResult }>(
    db,
    sql`select public.fn_upsert_financial_obligation(
      ${USER_ID}::uuid,
      ${JSON.stringify(payload)}::jsonb,
      ${JSON.stringify(keys.entries)}::jsonb,
      ${JSON.stringify(evidence)}::jsonb
    ) as resultado`,
  );

  if (!row) throw new Error("upsert: a função não devolveu linha");
  return row.resultado;
}

async function contarObrigacoes(): Promise<number> {
  const row = await linha<{ total: string }>(
    db,
    sql`select count(*)::text as total from public.financial_obligations
         where user_id = ${USER_ID}::uuid`,
  );
  return Number(row!.total);
}

const BARCODE = "83640000001 2 34560000000 3 45670000000 4 56780000000 5";

describe.skipIf(semBancoDeTeste)("obrigações financeiras", () => {
  beforeAll(() => {
    db = getTestDb();
  });

  beforeEach(async () => {
    await limparPorUsuario(db, TABELAS, USER_ID);
  });

  afterAll(async () => {
    await limparPorUsuario(db, TABELAS, USER_ID);
  });

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

      expect(await contarObrigacoes()).toBe(1);
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

      const evidencias = await linhas<{ source_document_id: string; evidence_role: string }>(
        db,
        sql`select source_document_id, evidence_role
              from public.financial_obligation_evidences
             where user_id = ${USER_ID}::uuid`,
      );

      const roles = Object.fromEntries(
        evidencias.map((e) => [e.source_document_id, e.evidence_role]),
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
      // Ambos emitem a chave fraca fornecedor+mês. A função precisa recusar o
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

      const obrigacao = await linha<{
        amount: string;
        due_date: string;
        supplier_name_raw: string;
      }>(
        db,
        sql`select amount, due_date::text as due_date, supplier_name_raw
              from public.financial_obligations where id = ${a.obligation_id}::uuid`,
      );

      expect(Number(obrigacao!.amount)).toBe(245.5);
      expect(obrigacao!.due_date).toBe("2026-04-10");
      expect(obrigacao!.supplier_name_raw).toBe("CEMIG");
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

      expect(await contarObrigacoes()).toBe(1);
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

      const chaves = await linhas<{ key_kind: string }>(
        db,
        sql`select key_kind from public.financial_obligation_identity_keys
             where obligation_id = ${r.obligation_id}::uuid`,
      );

      const kinds = new Set(chaves.map((k) => k.key_kind));
      expect(kinds.has("barcode")).toBe(true);
      expect(kinds.has("docnum")).toBe(true);
      expect(kinds.has("supplier_period_amount")).toBe(true);
    });
  });
});
