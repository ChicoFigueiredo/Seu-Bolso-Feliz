/**
 * Testes de integração: Pipeline de Ingestão
 *
 * 2.27 — Scanner local → Worker → Banco (fluxo completo)
 * 2.28 — Deduplicação: mesmo documento 2x não duplica
 * 2.29 — Reprocessamento forçado (force_reprocess)
 *
 * Estes testes usam o Supabase LOCAL real (service role).
 * Requerem `supabase start` rodando.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import { mkdir, writeFile, rm } from "node:fs/promises";
import { join } from "node:path";
import { scanDirectory } from "../../workers/local-scanner/src/scanner";
import { processJob } from "../../workers/ingestion/src/processor";
import { IngestionJobStatus, IngestionRunStatus, SourceDocumentOrigin } from "@sbf/ingestion-types";
import { computeContentHash, computeCanonicalFingerprint } from "@sbf/operations";

// ══════════════════════════════════════════════════════════════
// Setup: Supabase local (service_role) + diretório temp
// ══════════════════════════════════════════════════════════════

const SUPABASE_URL = "http://127.0.0.1:54321";
const SUPABASE_SERVICE_KEY = process.env.SUPABASE_SERVICE_ROLE_KEY
  ?? "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJpc3MiOiJzdXBhYmFzZS1kZW1vIiwicm9sZSI6InNlcnZpY2Vfcm9sZSIsImV4cCI6MTk4MzgxMjk5Nn0.EGIM96RAZx35lJzdJsyH-qQwv8Hdp7fsn3W0YpN81IU";
const TEST_USER_ID = "00000000-0000-0000-0000-000000000001";
const INBOX_DIR = join(process.cwd(), "__tests__/integration/.test-inbox");

let supabase: SupabaseClient;

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  process.env.LOCAL_USER_ID = TEST_USER_ID;
  process.env.SUPABASE_URL = SUPABASE_URL;
  process.env.SUPABASE_SERVICE_ROLE_KEY = SUPABASE_SERVICE_KEY;

  // Garantir que o user existe no auth (necessário para RLS/FK)
  await supabase.auth.admin.createUser({
    email: "test-ingestion@sbf.local",
    password: "TestPass123!",
    user_metadata: { name: "Test Ingestion" },
    email_confirm: true,
  }).catch(() => {
    // Ignora se o user já existir
  });

  // Buscar o user real criado (o ID pode não ser o nosso TEST_USER_ID)
  const { data: users } = await supabase.auth.admin.listUsers();
  const testUser = users?.users?.find(u => u.email === "test-ingestion@sbf.local");
  if (testUser) {
    process.env.LOCAL_USER_ID = testUser.id;
  }
});

afterAll(async () => {
  // Limpar diretório de teste
  await rm(INBOX_DIR, { recursive: true, force: true }).catch(() => {});

  // Limpar dados de teste (ordem respeita FKs)
  const userId = process.env.LOCAL_USER_ID!;
  await supabase.from("draft_records").delete().eq("user_id", userId);
  await supabase.from("draft_batches").delete().eq("user_id", userId);
  await supabase.from("extraction_results").delete().eq("user_id", userId);
  await supabase.from("parsed_document_versions").delete().eq("user_id", userId);
  await supabase.from("ingestion_logs").delete().eq("user_id", userId);
  await supabase.from("document_fingerprints").delete().eq("user_id", userId);
  await supabase.from("ingestion_jobs").delete().eq("user_id", userId);
  await supabase.from("source_documents").delete().eq("user_id", userId);
  await supabase.from("ingestion_runs").delete().eq("user_id", userId);

  // Limpar storage
  const { data: files } = await supabase.storage.from("ingestion-originals").list(userId);
  if (files?.length) {
    for (const folder of files) {
      const { data: inner } = await supabase.storage
        .from("ingestion-originals")
        .list(`${userId}/${folder.name}`);
      if (inner?.length) {
        const paths = inner.map(f => `${userId}/${folder.name}/${f.name}`);
        await supabase.storage.from("ingestion-originals").remove(paths);
      }
    }
  }
});

beforeEach(async () => {
  // Limpar inbox e dados antes de cada teste
  await rm(INBOX_DIR, { recursive: true, force: true }).catch(() => {});
  await mkdir(INBOX_DIR, { recursive: true });

  const userId = process.env.LOCAL_USER_ID!;
  await supabase.from("draft_records").delete().eq("user_id", userId);
  await supabase.from("draft_batches").delete().eq("user_id", userId);
  await supabase.from("extraction_results").delete().eq("user_id", userId);
  await supabase.from("parsed_document_versions").delete().eq("user_id", userId);
  await supabase.from("ingestion_logs").delete().eq("user_id", userId);
  await supabase.from("document_fingerprints").delete().eq("user_id", userId);
  await supabase.from("ingestion_jobs").delete().eq("user_id", userId);
  await supabase.from("source_documents").delete().eq("user_id", userId);
  await supabase.from("ingestion_runs").delete().eq("user_id", userId);
});

// ══════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════

async function createTestFile(name: string, content: string): Promise<string> {
  const path = join(INBOX_DIR, name);
  await writeFile(path, content);
  return path;
}

async function getJobs(userId: string) {
  const { data } = await supabase
    .from("ingestion_jobs")
    .select("*")
    .eq("user_id", userId)
    .order("created_at", { ascending: true });
  return data ?? [];
}

async function getRuns(userId: string) {
  const { data } = await supabase
    .from("ingestion_runs")
    .select("*")
    .eq("user_id", userId)
    .order("created_at", { ascending: true });
  return data ?? [];
}

async function getDocuments(userId: string) {
  const { data } = await supabase
    .from("source_documents")
    .select("*")
    .eq("user_id", userId)
    .order("created_at", { ascending: true });
  return data ?? [];
}

async function getFingerprints(userId: string) {
  const { data } = await supabase
    .from("document_fingerprints")
    .select("*")
    .eq("user_id", userId)
    .order("created_at", { ascending: true });
  return data ?? [];
}

async function getLogs(userId: string) {
  const { data } = await supabase
    .from("ingestion_logs")
    .select("*")
    .eq("user_id", userId)
    .order("created_at", { ascending: true });
  return data ?? [];
}

// ══════════════════════════════════════════════════════════════
// 2.27 — Scanner local → Worker → Banco (fluxo completo)
// ══════════════════════════════════════════════════════════════

describe("2.27: Scanner → Worker → Banco (fluxo integrado)", () => {
  it("escaneia arquivo, cria job, processa até 'pending_review'", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    const content = "Conta de luz CEMIG - Março 2026 - R$ 245,50";
    await createTestFile("conta-luz.csv", content);

    // === ETAPA 1: Scanner escaneia o diretório ===
    const discovered = await scanDirectory(supabase, INBOX_DIR);
    expect(discovered).toBe(1);

    // Verificar run criado
    const runs = await getRuns(userId);
    expect(runs).toHaveLength(1);
    expect(runs[0]!.status).toBe(IngestionRunStatus.COMPLETED);
    expect(runs[0]!.source_type).toBe(SourceDocumentOrigin.LOCAL_FILE);

    // Verificar source_document criado
    const docs = await getDocuments(userId);
    expect(docs).toHaveLength(1);
    expect(docs[0]!.filename).toBe("conta-luz.csv");
    expect(docs[0]!.mime_type).toBe("text/csv");
    expect(docs[0]!.storage_path).toBeTruthy();
    expect(docs[0]!.origin_key).toContain("local:");

    // Verificar ingestion_job criado no status DISCOVERED
    const jobs = await getJobs(userId);
    expect(jobs).toHaveLength(1);
    expect(jobs[0]!.status).toBe(IngestionJobStatus.DISCOVERED);
    expect(jobs[0]!.source_document_id).toBe(docs[0]!.id);

    // === ETAPA 2: Worker processa o job ===
    await processJob(supabase, {
      id: jobs[0]!.id,
      run_id: jobs[0]!.run_id,
      user_id: userId,
      source_document_id: jobs[0]!.source_document_id,
      status: jobs[0]!.status,
      retry_count: jobs[0]!.retry_count ?? 0,
      max_retries: jobs[0]!.max_retries ?? 3,
      metadata: (jobs[0]!.metadata as Record<string, unknown>) ?? {},
    });

    // Verificar job avançou para PENDING_REVIEW (pipeline completo)
    const updatedJobs = await getJobs(userId);
    expect(updatedJobs[0]!.status).toBe(IngestionJobStatus.PENDING_REVIEW);

    // Verificar fingerprint criado
    const fps = await getFingerprints(userId);
    expect(fps).toHaveLength(1);
    expect(fps[0]!.content_hash).toBeTruthy();
    expect(fps[0]!.canonical_fingerprint).toBeTruthy();
    expect(fps[0]!.source_document_id).toBe(docs[0]!.id);

    // Verificar logs gerados
    const logs = await getLogs(userId);
    expect(logs.length).toBeGreaterThanOrEqual(2);
    const messages = logs.map((l: any) => l.message);
    expect(messages.some((m: string) => m.includes("Downloaded"))).toBe(true);
    expect(messages.some((m: string) => m.includes("queued") || m.includes("proceeding"))).toBe(true);
  });

  it("escaneia múltiplos arquivos e cria jobs independentes", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    await createTestFile("fatura-nubank.csv", "Fatura Nubank Abril 2026 R$1.200,00");
    await createTestFile("boleto-internet.csv", "Boleto Vivo Internet R$99,90 Vencimento 15/04/2026");

    const discovered = await scanDirectory(supabase, INBOX_DIR);
    expect(discovered).toBe(2);

    const docs = await getDocuments(userId);
    expect(docs).toHaveLength(2);

    const jobs = await getJobs(userId);
    expect(jobs).toHaveLength(2);
    expect(jobs.every((j: any) => j.status === IngestionJobStatus.DISCOVERED)).toBe(true);

    // Processar ambos os jobs
    for (const job of jobs) {
      await processJob(supabase, {
        id: job.id,
        run_id: job.run_id,
        user_id: userId,
        source_document_id: job.source_document_id,
        status: job.status,
        retry_count: job.retry_count ?? 0,
        max_retries: job.max_retries ?? 3,
        metadata: (job.metadata as Record<string, unknown>) ?? {},
      });
    }

    const updatedJobs = await getJobs(userId);
    expect(updatedJobs.every((j: any) => j.status === IngestionJobStatus.PENDING_REVIEW)).toBe(true);

    const fps = await getFingerprints(userId);
    expect(fps).toHaveLength(2);
  });

  it("ignora arquivos com extensão não aceita", async () => {
    await createTestFile("readme.txt", "Este arquivo não é aceito");
    await createTestFile("conta.csv", "CSV aceito");

    const discovered = await scanDirectory(supabase, INBOX_DIR);
    expect(discovered).toBe(1);

    const docs = await getDocuments(process.env.LOCAL_USER_ID!);
    expect(docs).toHaveLength(1);
    expect(docs[0]!.filename).toBe("conta.csv");
  });
});

// ══════════════════════════════════════════════════════════════
// 2.28 — Deduplicação: mesmo documento 2x não duplica
// ══════════════════════════════════════════════════════════════

describe("2.28: Deduplicação — mesmo doc não duplica", () => {
  it("scanner ignora arquivo já processado (mesma origin_key)", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    await createTestFile("conta-cemig.csv", "CEMIG Março 2026");

    // Primeira scan
    const first = await scanDirectory(supabase, INBOX_DIR);
    expect(first).toBe(1);

    // Segunda scan (mesmo arquivo, mesmo mtime) → deve ignorar
    const second = await scanDirectory(supabase, INBOX_DIR);
    expect(second).toBe(0);

    // Confirmar que só existe 1 documento e 1 job
    const docs = await getDocuments(userId);
    expect(docs).toHaveLength(1);

    const jobs = await getJobs(userId);
    expect(jobs).toHaveLength(1);
  });

  it("worker rejeita documento com hash idêntico no step de hash", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    const content = "Mesmo conteúdo exato para testar dedup";

    // Criar e escanear primeiro arquivo
    await createTestFile("doc-a.csv", content);
    await scanDirectory(supabase, INBOX_DIR);

    // Processar primeiro job (vai até PARSED)
    let jobs = await getJobs(userId);
    await processJob(supabase, {
      id: jobs[0]!.id,
      run_id: jobs[0]!.run_id,
      user_id: userId,
      source_document_id: jobs[0]!.source_document_id,
      status: jobs[0]!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (jobs[0]!.metadata as Record<string, unknown>) ?? {},
    });

    // Criar segundo arquivo com conteúdo igual mas nome diferente
    // Precisa ter nome/mtime diferente para o scanner não ignorar por origin_key
    await createTestFile("doc-b.csv", content);

    // Escanear segundo
    const discovered = await scanDirectory(supabase, INBOX_DIR);
    expect(discovered).toBe(1); // Scanner acha novo (origin_key diferente)

    // Processar segundo job
    jobs = await getJobs(userId);
    const secondJob = jobs.find((j: any) => j.status === IngestionJobStatus.DISCOVERED);
    expect(secondJob).toBeTruthy();

    await processJob(supabase, {
      id: secondJob!.id,
      run_id: secondJob!.run_id,
      user_id: userId,
      source_document_id: secondJob!.source_document_id,
      status: secondJob!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (secondJob!.metadata as Record<string, unknown>) ?? {},
    });

    // Segundo job deve ter falhado por dedup (reject_exact ou alert_semantic)
    jobs = await getJobs(userId);
    const processedSecond = jobs.find((j: any) => j.id === secondJob!.id);
    expect([IngestionJobStatus.FAILED, IngestionJobStatus.DOWNLOADED]).toContain(
      processedSecond!.status,
    );

    // Verificar que o log menciona duplicata
    const logs = await getLogs(userId);
    const dedupLog = logs.find(
      (l: any) =>
        l.job_id === secondJob!.id &&
        (l.message.includes("reject_exact") ||
          l.message.includes("alert_semantic") ||
          l.message.includes("Duplicate")),
    );
    expect(dedupLog).toBeTruthy();
  });
});

// ══════════════════════════════════════════════════════════════
// 2.29 — Reprocessamento forçado (force_reprocess)
// ══════════════════════════════════════════════════════════════

describe("2.29: Reprocessamento forçado", () => {
  it("job com force_reprocess=true ignora dedup e vai até PENDING_REVIEW", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    const content = "Documento para reprocessar";

    // Criar e processar normalmente
    await createTestFile("reprocess-me.csv", content);
    await scanDirectory(supabase, INBOX_DIR);

    let jobs = await getJobs(userId);
    await processJob(supabase, {
      id: jobs[0]!.id,
      run_id: jobs[0]!.run_id,
      user_id: userId,
      source_document_id: jobs[0]!.source_document_id,
      status: jobs[0]!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (jobs[0]!.metadata as Record<string, unknown>) ?? {},
    });

    // Confirmar que primeiro está PENDING_REVIEW
    jobs = await getJobs(userId);
    expect(jobs[0]!.status).toBe(IngestionJobStatus.PENDING_REVIEW);

    // Criar segundo arquivo com conteúdo igual
    await createTestFile("reprocess-me-v2.csv", content);
    await scanDirectory(supabase, INBOX_DIR);

    // Pegar o job novo e setar force_reprocess
    jobs = await getJobs(userId);
    const newJob = jobs.find((j: any) => j.status === IngestionJobStatus.DISCOVERED);
    expect(newJob).toBeTruthy();

    // Atualizar metadata com force_reprocess
    await supabase
      .from("ingestion_jobs")
      .update({
        metadata: { ...(newJob!.metadata as object), force_reprocess: true },
      })
      .eq("id", newJob!.id);

    // Re-ler o job atualizado
    const { data: freshJob } = await supabase
      .from("ingestion_jobs")
      .select("*")
      .eq("id", newJob!.id)
      .single();

    // Processar com force_reprocess
    await processJob(supabase, {
      id: freshJob!.id,
      run_id: freshJob!.run_id,
      user_id: userId,
      source_document_id: freshJob!.source_document_id,
      status: freshJob!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (freshJob!.metadata as Record<string, unknown>) ?? {},
    });

    // Deve ter avançado para PENDING_REVIEW (não rejeitado)
    const { data: finalJob } = await supabase
      .from("ingestion_jobs")
      .select("*")
      .eq("id", freshJob!.id)
      .single();

    expect(finalJob!.status).toBe(IngestionJobStatus.PENDING_REVIEW);
  });

  it("job FAILED pode ser retomado de DISCOVERED com retry", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    await createTestFile("retry-test.csv", "Documento de retry");
    await scanDirectory(supabase, INBOX_DIR);

    let jobs = await getJobs(userId);
    const jobId = jobs[0]!.id;

    // Simular falha manual
    await supabase
      .from("ingestion_jobs")
      .update({
        status: IngestionJobStatus.FAILED,
        error_message: "Simulated failure for retry test",
        error_details: { previous_status: IngestionJobStatus.DISCOVERED },
      })
      .eq("id", jobId);

    // Resetar para DISCOVERED (como faria um retry manual)
    await supabase
      .from("ingestion_jobs")
      .update({
        status: IngestionJobStatus.DISCOVERED,
        retry_count: 1,
        error_message: null,
        error_details: null,
      })
      .eq("id", jobId);

    // Re-ler job
    const { data: retryJob } = await supabase
      .from("ingestion_jobs")
      .select("*")
      .eq("id", jobId)
      .single();

    // Processar novamente
    await processJob(supabase, {
      id: retryJob!.id,
      run_id: retryJob!.run_id,
      user_id: userId,
      source_document_id: retryJob!.source_document_id,
      status: retryJob!.status,
      retry_count: retryJob!.retry_count ?? 1,
      max_retries: retryJob!.max_retries ?? 3,
      metadata: (retryJob!.metadata as Record<string, unknown>) ?? {},
    });

    // Deve ter avançado para PENDING_REVIEW
    const { data: finalJob } = await supabase
      .from("ingestion_jobs")
      .select("*")
      .eq("id", jobId)
      .single();

    expect(finalJob!.status).toBe(IngestionJobStatus.PENDING_REVIEW);
  });
});

// ══════════════════════════════════════════════════════════════
// 4.16 — Fornecedor e período sugeridos corretamente
// ══════════════════════════════════════════════════════════════

describe("4.16: Fornecedor e período sugeridos corretamente", () => {
  it("documento CEMIG gera extraction_result com categoria e tags", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    const cemigText = [
      "CEMIG DISTRIBUIÇÃO S.A.",
      "Mês Referência MARÇO/2026",
      "Valor a Pagar R$ 245,50",
      "Vencimento: 15/04/2026",
      "Nota Fiscal 123456",
      "Unidade Consumidora: 12345678",
      "Consumo kWh: 320",
      "Quantidade de dias: 30",
    ].join("\n");
    await createTestFile("conta-cemig-detalhada.csv", cemigText);

    await scanDirectory(supabase, INBOX_DIR);
    const jobs = await getJobs(userId);

    await processJob(supabase, {
      id: jobs[0]!.id,
      run_id: jobs[0]!.run_id,
      user_id: userId,
      source_document_id: jobs[0]!.source_document_id,
      status: jobs[0]!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (jobs[0]!.metadata as Record<string, unknown>) ?? {},
    });

    // Verificar extraction_result
    const { data: results } = await supabase
      .from("extraction_results")
      .select("*")
      .eq("user_id", userId);

    expect(results).toBeTruthy();
    expect(results!.length).toBeGreaterThanOrEqual(1);

    const er = results![0]!;
    expect(er.supplier_name_raw).toContain("CEMIG");
    expect(er.category_suggestion).toBe("energia_eletrica");
    expect(er.tags_suggestion).toContain("essencial");
    expect(er.tags_suggestion).toContain("energia");
    expect(er.competence_date).toBeTruthy();
    expect(er.total_amount).toBe(245.5);
  });
});

// ══════════════════════════════════════════════════════════════
// 4.17 — Drafts não poluem ledger principal
// ══════════════════════════════════════════════════════════════

describe("4.17: Drafts não poluem ledger principal", () => {
  it("processar documento até PENDING_REVIEW não cria transactions", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    await createTestFile("boleto-nao-polui.csv", "Boleto Vivo R$99,90 Vencimento 15/04/2026");

    await scanDirectory(supabase, INBOX_DIR);
    const jobs = await getJobs(userId);

    await processJob(supabase, {
      id: jobs[0]!.id,
      run_id: jobs[0]!.run_id,
      user_id: userId,
      source_document_id: jobs[0]!.source_document_id,
      status: jobs[0]!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (jobs[0]!.metadata as Record<string, unknown>) ?? {},
    });

    // Job deve estar em PENDING_REVIEW
    const updatedJobs = await getJobs(userId);
    expect(updatedJobs[0]!.status).toBe(IngestionJobStatus.PENDING_REVIEW);

    // Draft_records devem existir
    const { data: drafts } = await supabase
      .from("draft_records")
      .select("*")
      .eq("user_id", userId);
    expect(drafts!.length).toBeGreaterThanOrEqual(1);
    expect(drafts![0]!.status).toBe("pending_review");

    // Tabela transactions NÃO deve ter sido afetada pelo pipeline de ingestão
    // O draft_record.posted_record_id deve ser null (não postou nada)
    expect(drafts![0]!.posted_record_id).toBeNull();
    expect(drafts![0]!.posted_record_type).toBeNull();
  });
});

// ══════════════════════════════════════════════════════════════
// 4.18 — Baixa confiança vai para revisão
// ══════════════════════════════════════════════════════════════

describe("4.18: Baixa confiança vai para revisão", () => {
  it("documento com texto genérico gera draft com status pending_review", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    // Texto sem padrão reconhecível → baixa confiança (0.3 = texto bruto)
    await createTestFile("generico.csv", "Algum texto qualquer sem padrão de boleto ou CEMIG");

    await scanDirectory(supabase, INBOX_DIR);
    const jobs = await getJobs(userId);

    await processJob(supabase, {
      id: jobs[0]!.id,
      run_id: jobs[0]!.run_id,
      user_id: userId,
      source_document_id: jobs[0]!.source_document_id,
      status: jobs[0]!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (jobs[0]!.metadata as Record<string, unknown>) ?? {},
    });

    // Verificar que o job chegou a PENDING_REVIEW mesmo com baixa confiança
    const updatedJobs = await getJobs(userId);
    expect(updatedJobs[0]!.status).toBe(IngestionJobStatus.PENDING_REVIEW);

    // Verificar draft_records status e confiança
    const { data: drafts } = await supabase
      .from("draft_records")
      .select("*")
      .eq("user_id", userId);

    expect(drafts!.length).toBeGreaterThanOrEqual(1);
    expect(drafts![0]!.status).toBe("pending_review");
    // Confiança deve ser baixa (texto genérico = 0.3 ou menos)
    expect(drafts![0]!.confidence_score).toBeLessThanOrEqual(0.3);
  });
});

// ══════════════════════════════════════════════════════════════
// 4.19 — Reprocessamento gera nova versão
// ══════════════════════════════════════════════════════════════

describe("4.19: Reprocessamento gera nova versão", () => {
  it("reprocessar documento cria parsed_document_versions v1 e v2", async () => {
    const userId = process.env.LOCAL_USER_ID!;
    await createTestFile("multi-versao.csv", "CEMIG DISTRIBUIÇÃO Conta MARÇO/2026 R$ 100,00");

    await scanDirectory(supabase, INBOX_DIR);
    const jobs = await getJobs(userId);
    const jobId = jobs[0]!.id;
    const sourceDocId = jobs[0]!.source_document_id;

    // Primeira processamento
    await processJob(supabase, {
      id: jobId,
      run_id: jobs[0]!.run_id,
      user_id: userId,
      source_document_id: sourceDocId,
      status: jobs[0]!.status,
      retry_count: 0,
      max_retries: 3,
      metadata: (jobs[0]!.metadata as Record<string, unknown>) ?? {},
    });

    // Verificar v1
    const { data: versions1 } = await supabase
      .from("parsed_document_versions")
      .select("*")
      .eq("source_document_id", sourceDocId)
      .order("version_number", { ascending: true });

    expect(versions1).toHaveLength(1);
    expect(versions1![0]!.version_number).toBe(1);

    // Simular reprocessamento: resetar job para QUEUED
    await supabase
      .from("ingestion_jobs")
      .update({ status: IngestionJobStatus.QUEUED, retry_count: 1 })
      .eq("id", jobId);

    // Limpar drafts anteriores para evitar conflito
    await supabase.from("draft_records").delete().eq("user_id", userId);
    await supabase.from("draft_batches").delete().eq("user_id", userId);

    // Re-ler job
    const { data: freshJob } = await supabase
      .from("ingestion_jobs")
      .select("*")
      .eq("id", jobId)
      .single();

    // Reprocessar (stepParse + stepDraft)
    await processJob(supabase, {
      id: freshJob!.id,
      run_id: freshJob!.run_id,
      user_id: userId,
      source_document_id: freshJob!.source_document_id,
      status: freshJob!.status,
      retry_count: freshJob!.retry_count ?? 1,
      max_retries: 3,
      metadata: (freshJob!.metadata as Record<string, unknown>) ?? {},
    });

    // Verificar v1 e v2
    const { data: versions2 } = await supabase
      .from("parsed_document_versions")
      .select("*")
      .eq("source_document_id", sourceDocId)
      .order("version_number", { ascending: true });

    expect(versions2).toHaveLength(2);
    expect(versions2![0]!.version_number).toBe(1);
    expect(versions2![1]!.version_number).toBe(2);
  });
});
