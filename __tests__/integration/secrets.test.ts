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
 * A gravação passou a ser `gravarSegredo()` de `@sbf/db`: `fn_set_secret` era
 * uma das funções que dependiam de `auth.uid()` e não sobreviveu à migração
 * para o Neon. Semear pelo mesmo caminho que a aplicação usa é de propósito —
 * a versão anterior deste arquivo montava o INSERT à mão e, por isso, não
 * teria percebido se o caminho real estivesse quebrado.
 *
 * Requer o Postgres de teste — ver `__tests__/helpers/db.ts`.
 */
import { describe, it, expect, beforeAll, beforeEach } from "vitest";
import { sql } from "drizzle-orm";
import { gravarSegredo, type Database } from "@sbf/db";
import {
  listPdfPasswordCandidates,
  markSecretUsed,
} from "../../workers/ingestion/src/parsers/secret-lookup";
import { getTestDb, limparPorUsuario, linha, linhas, semBancoDeTeste } from "../helpers/db";

/** Sentinela: se este valor aparecer em qualquer lugar indevido, o teste falha. */
const SENTINEL = "SenhaUltraSecreta-42!";

// Sem `auth.users` não há mais serviço de identidade para pedir um id: os
// usuários de teste são UUIDs fixos, únicos por arquivo, e nenhuma FK os exige
// (as chaves para `auth.users` caíram na migração — ver packages/db/schema.sql).
const USER_ID = "10000000-0000-4000-8000-000000000001";
const OUTRO_USER_ID = "10000000-0000-4000-8000-000000000002";

let db: Database;

/** Grava um segredo pelo mesmo caminho da aplicação. */
async function seedSecret(
  plaintext: string,
  opts: { entityType?: string; entityId?: string; label?: string } = {},
): Promise<string> {
  const { id } = await gravarSegredo({
    userId: USER_ID,
    plaintext,
    secretType: "pdf_password",
    entityType: opts.entityType ?? null,
    entityId: opts.entityId ?? null,
    label: opts.label ?? null,
    origem: "teste",
  });
  return id;
}

describe.skipIf(semBancoDeTeste)("segredos", () => {
  beforeAll(() => {
    db = getTestDb();
  });

  // `audit_logs` fica de fora: a tabela é imutável por trigger
  // (`trg_audit_logs_immutable` recusa DELETE), o que é a propriedade certa
  // para uma trilha de auditoria. As asserções abaixo filtram por ação e nunca
  // dependem de a tabela estar vazia.
  beforeEach(async () => {
    await limparPorUsuario(db, ["user_secrets"], USER_ID, OUTRO_USER_ID);
  });

  describe("criptografia", () => {
    it("faz round-trip do segredo", async () => {
      await seedSecret(SENTINEL);
      const candidates = await listPdfPasswordCandidates(db, USER_ID);
      expect(candidates).toHaveLength(1);
      expect(candidates[0]!.value).toBe(SENTINEL);
    });

    it("a coluna guarda ciphertext, nunca o texto puro", async () => {
      await seedSecret(SENTINEL);
      const row = await linha<{ encrypted_value: string; encryption_version: number }>(
        db,
        sql`select encrypted_value, encryption_version from public.user_secrets
            where user_id = ${USER_ID}::uuid`,
      );

      expect(row!.encrypted_value).not.toBe(SENTINEL);
      expect(row!.encrypted_value).not.toContain(SENTINEL);
      expect(Number(row!.encryption_version)).toBeGreaterThanOrEqual(1);
    });

    it("dois segredos iguais produzem ciphertexts diferentes", async () => {
      // pgp_sym_encrypt usa IV aleatório: ciphertext idêntico revelaria que duas
      // contas compartilham a mesma senha.
      await seedSecret(SENTINEL, { entityType: "supplier", entityId: crypto.randomUUID() });
      await seedSecret(SENTINEL, { entityType: "supplier", entityId: crypto.randomUUID() });

      const rows = await linhas<{ encrypted_value: string }>(
        db,
        sql`select encrypted_value from public.user_secrets where user_id = ${USER_ID}::uuid`,
      );

      expect(rows).toHaveLength(2);
      expect(rows[0]!.encrypted_value).not.toBe(rows[1]!.encrypted_value);
    });
  });

  describe("a senha não vaza", () => {
    it("não aparece em nenhuma linha de auditoria", async () => {
      await seedSecret(SENTINEL);
      await listPdfPasswordCandidates(db, USER_ID);

      const logs = await linhas(
        db,
        sql`select action, entity_type, entity_id, old_values, new_values
              from public.audit_logs where user_id = ${USER_ID}::uuid`,
      );

      expect(logs.length).toBeGreaterThan(0);
      expect(JSON.stringify(logs)).not.toContain(SENTINEL);
    });

    it("o acesso é registrado, mesmo sem o valor", async () => {
      await seedSecret(SENTINEL);
      await listPdfPasswordCandidates(db, USER_ID);

      const logs = await linhas(
        db,
        sql`select action from public.audit_logs
             where user_id = ${USER_ID}::uuid and action = 'secrets_accessed'`,
      );

      expect(logs.length).toBeGreaterThan(0);
    });

    it("a gravação também é registrada, e também sem o valor", async () => {
      await seedSecret(SENTINEL, { label: "Fatura do banco" });

      const logs = await linhas<{ new_values: Record<string, unknown> }>(
        db,
        sql`select new_values from public.audit_logs
             where user_id = ${USER_ID}::uuid and action = 'secret_written'`,
      );

      expect(logs.length).toBeGreaterThan(0);
      expect(JSON.stringify(logs)).not.toContain(SENTINEL);
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

      await markSecretUsed(db, USER_ID, a);

      const candidates = await listPdfPasswordCandidates(db, USER_ID);
      expect(candidates[0]!.label).toBe("A");
    });

    it("respeita o teto de candidatas", async () => {
      for (let i = 0; i < 5; i++) {
        await seedSecret(`senha-${i}`, { entityType: "supplier", entityId: crypto.randomUUID() });
      }
      const candidates = await listPdfPasswordCandidates(db, USER_ID, 3);
      expect(candidates).toHaveLength(3);
    });

    it("não devolve segredos de outro usuário", async () => {
      await seedSecret(SENTINEL);
      const candidates = await listPdfPasswordCandidates(db, OUTRO_USER_ID);
      expect(candidates.every((c) => c.value !== SENTINEL)).toBe(true);
    });
  });
});
