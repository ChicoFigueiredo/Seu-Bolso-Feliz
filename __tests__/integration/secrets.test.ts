/**
 * Testes de integração: criptografia real de segredos.
 *
 * O que estes testes protegem:
 *  - o round-trip encrypt/decrypt, que era IMPOSSÍVEL antes porque a chave
 *    vinha de uma GUC de sessão que ninguém nunca definia;
 *  - que a coluna guarda ciphertext, e não o texto puro que ela guardava na
 *    prática enquanto `encrypt_secret` não tinha chamadores;
 *  - que a senha nunca aparece na trilha de auditoria.
 *
 * Requer `supabase start`.
 */
import { describe, it, expect, beforeAll, afterAll, beforeEach } from "vitest";
import { createClient, type SupabaseClient } from "@supabase/supabase-js";
import {
  listPdfPasswordCandidates,
  markSecretUsed,
} from "../../workers/ingestion/src/parsers/secret-lookup";

const SUPABASE_URL = process.env.SUPABASE_URL ?? "http://127.0.0.1:54321";
const SUPABASE_SERVICE_KEY = exigirSupabaseSecretKey();

/**
 * A chave secreta local é fixa (mesmo valor em qualquer máquina, sai de
 * `supabase status`), mas não pode ficar hardcoded aqui: o padrão
 * `sb_secret_...` aciona o secret scanning do GitHub mesmo sendo local.
 * O CI exporta SUPABASE_SECRET_KEY logo após subir o Supabase local.
 */
function exigirSupabaseSecretKey(): string {
  const key = process.env.SUPABASE_SECRET_KEY;
  if (!key) {
    throw new Error(
      "SUPABASE_SECRET_KEY não definida. Rode `supabase status` e copie SECRET_KEY para .env.",
    );
  }
  return key;
}

/** Sentinela: se este valor aparecer em qualquer lugar indevido, o teste falha. */
const SENTINEL = "SenhaUltraSecreta-42!";

let supabase: SupabaseClient;
let userId: string;
let otherUserId: string;

/**
 * Cria o usuário de teste ou, se já existir de uma execução anterior, faz
 * login para recuperar o id — mesma tolerância que `userId` já tinha.
 */
async function criarOuAcharUsuario(email: string): Promise<string> {
  const created = await supabase.auth.admin.createUser({
    email,
    password: "TestPass123!",
    email_confirm: true,
  });
  if (created.data?.user?.id) return created.data.user.id;

  const probe = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });
  const signedIn = await probe.auth.signInWithPassword({ email, password: "TestPass123!" });
  return signedIn.data!.user!.id;
}

/**
 * Insere um segredo já criptografado.
 *
 * fn_set_secret usa auth.uid() de propósito (não aceita p_user_id, o que
 * elimina a superfície de forja), e sob service_role auth.uid() é NULL. Então
 * o teste grava direto usando encrypt_secret, que é o que a RPC faria.
 */
async function seedSecret(
  plaintext: string,
  opts: { entityType?: string; entityId?: string; label?: string } = {},
): Promise<string> {
  const { data: enc, error: encErr } = await supabase.rpc("encrypt_secret", {
    plaintext,
  } as never);
  if (encErr) throw new Error(`encrypt_secret: ${encErr.message}`);

  const { data: row, error: insErr } = await supabase
    .from("user_secrets")
    .insert({
      user_id: userId,
      secret_type: "pdf_password",
      entity_type: opts.entityType ?? null,
      entity_id: opts.entityId ?? null,
      label: opts.label ?? null,
      encrypted_value: enc as unknown as string,
      encryption_version: 1,
    })
    .select("id")
    .single();

  if (insErr) throw new Error(`insert user_secrets: ${insErr.message}`);
  return row!.id as string;
}

beforeAll(async () => {
  supabase = createClient(SUPABASE_URL, SUPABASE_SERVICE_KEY, {
    auth: { persistSession: false, autoRefreshToken: false },
  });

  userId = await criarOuAcharUsuario("test-secrets@sbf.local");
  otherUserId = await criarOuAcharUsuario("test-secrets-other@sbf.local");
});

beforeEach(async () => {
  await supabase.from("user_secrets").delete().eq("user_id", userId);
  await supabase.from("audit_logs").delete().eq("user_id", userId);
  await supabase.from("audit_logs").delete().eq("user_id", otherUserId);
});

afterAll(async () => {
  await supabase.from("user_secrets").delete().eq("user_id", userId);
  await supabase.from("audit_logs").delete().eq("user_id", userId);
  await supabase.from("audit_logs").delete().eq("user_id", otherUserId);
});

describe("criptografia", () => {
  it("faz round-trip do segredo", async () => {
    await seedSecret(SENTINEL);
    const candidates = await listPdfPasswordCandidates(supabase, userId);
    expect(candidates).toHaveLength(1);
    expect(candidates[0]!.value).toBe(SENTINEL);
  });

  it("a coluna guarda ciphertext, nunca o texto puro", async () => {
    await seedSecret(SENTINEL);
    const { data } = await supabase
      .from("user_secrets")
      .select("encrypted_value, encryption_version")
      .eq("user_id", userId)
      .single();

    expect(data!.encrypted_value).not.toBe(SENTINEL);
    expect(data!.encrypted_value).not.toContain(SENTINEL);
    expect(data!.encryption_version).toBeGreaterThanOrEqual(1);
  });

  it("dois segredos iguais produzem ciphertexts diferentes", async () => {
    // pgp_sym_encrypt usa IV aleatório: ciphertext idêntico revelaria que duas
    // contas compartilham a mesma senha.
    await seedSecret(SENTINEL, { entityType: "supplier", entityId: crypto.randomUUID() });
    await seedSecret(SENTINEL, { entityType: "supplier", entityId: crypto.randomUUID() });

    const { data } = await supabase
      .from("user_secrets")
      .select("encrypted_value")
      .eq("user_id", userId);

    expect(data).toHaveLength(2);
    expect(data![0]!.encrypted_value).not.toBe(data![1]!.encrypted_value);
  });
});

describe("a senha não vaza", () => {
  it("não aparece em nenhuma linha de auditoria", async () => {
    await seedSecret(SENTINEL);
    await listPdfPasswordCandidates(supabase, userId);

    const { data: logs } = await supabase
      .from("audit_logs")
      .select("action, entity_type, entity_id, old_values, new_values")
      .eq("user_id", userId);

    expect(logs!.length).toBeGreaterThan(0);
    expect(JSON.stringify(logs)).not.toContain(SENTINEL);
  });

  it("o acesso é registrado, mesmo sem o valor", async () => {
    await seedSecret(SENTINEL);
    await listPdfPasswordCandidates(supabase, userId);

    const { data: logs } = await supabase
      .from("audit_logs")
      .select("action")
      .eq("user_id", userId)
      .eq("action", "secrets_accessed");

    expect(logs!.length).toBeGreaterThan(0);
  });
});

describe("ordenação e escopo", () => {
  it("a senha usada por último vem primeiro", async () => {
    // Escopos distintos: o índice único garante um segredo por escopo, então
    // duas senhas do mesmo usuário precisam estar presas a entidades diferentes.
    const a = await seedSecret("senha-A", {
      label: "A",
      entityType: "supplier",
      entityId: crypto.randomUUID(),
    });
    await seedSecret("senha-B", {
      label: "B",
      entityType: "supplier",
      entityId: crypto.randomUUID(),
    });

    await markSecretUsed(supabase, userId, a);

    const candidates = await listPdfPasswordCandidates(supabase, userId);
    expect(candidates[0]!.label).toBe("A");
  });

  it("respeita o teto de candidatas", async () => {
    for (let i = 0; i < 5; i++) {
      await seedSecret(`senha-${i}`, { entityType: "supplier", entityId: crypto.randomUUID() });
    }
    const candidates = await listPdfPasswordCandidates(supabase, userId, 3);
    expect(candidates).toHaveLength(3);
  });

  it("não devolve segredos de outro usuário", async () => {
    await seedSecret(SENTINEL);
    const candidates = await listPdfPasswordCandidates(supabase, otherUserId);
    expect(candidates.every((c) => c.value !== SENTINEL)).toBe(true);
  });
});
