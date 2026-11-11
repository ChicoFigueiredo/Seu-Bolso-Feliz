import { describe, it, expect } from "vitest";
import type { Database } from "@sbf/db";
import { completeCheckpoint, recordPageProgress, startOrResumeCheckpoint } from "./checkpoint";

/**
 * Stub do query builder do Drizzle — só as cadeias que checkpoint.ts usa:
 *   select().from().where().limit()
 *   insert().values().onConflictDoUpdate().returning()
 *   update().set().where()
 *
 * Um Postgres de verdade cabe nos testes de integração; aqui o que interessa
 * é a REGRA (checkpoint em andamento vence a janela do chamador, contadores
 * acumulam, cursor aponta pra próxima página), não o SQL gerado.
 */
interface StubCalls {
  insertValues?: Record<string, unknown>;
  conflictSet?: Record<string, unknown>;
  updateSet?: Record<string, unknown>;
  insertCount: number;
}

function makeDb(options: {
  selectRows?: unknown[];
  returningRows?: unknown[];
  insertThrows?: unknown;
}): { db: Database; calls: StubCalls } {
  const calls: StubCalls = { insertCount: 0 };
  const selectRows = options.selectRows ?? [];
  const returningRows = options.returningRows ?? [{ id: "cp-novo" }];

  const stub = {
    select: () => ({
      from: () => ({
        where: () => ({ limit: () => Promise.resolve(selectRows) }),
      }),
    }),
    insert: () => ({
      values: (values: Record<string, unknown>) => {
        calls.insertCount++;
        calls.insertValues = values;
        return {
          onConflictDoUpdate: (config: { set: Record<string, unknown> }) => {
            calls.conflictSet = config.set;
            return {
              returning: () =>
                options.insertThrows
                  ? Promise.reject(options.insertThrows)
                  : Promise.resolve(returningRows),
            };
          },
        };
      },
    }),
    update: () => ({
      set: (values: Record<string, unknown>) => {
        calls.updateSet = values;
        return { where: () => Promise.resolve(undefined) };
      },
    }),
  };

  return { db: stub as unknown as Database, calls };
}

const JANELA_PADRAO = { from: "2026-08-01", to: "2026-08-30" };

describe("startOrResumeCheckpoint", () => {
  it("retoma checkpoint em andamento — janela e cursor vêm do banco, não do argumento", async () => {
    const { db, calls } = makeDb({
      selectRows: [
        {
          id: "cp-1",
          cursor_value: "cursor-do-banco",
          window_start: "2025-09-01",
          window_end: "2026-08-30",
          status: "running",
        },
      ],
    });

    const state = await startOrResumeCheckpoint(db, "user-1", "item:acc-1", JANELA_PADRAO);

    expect(state).toEqual({
      id: "cp-1",
      from: "2025-09-01",
      to: "2026-08-30",
      startCursor: "cursor-do-banco",
    });
    // Retomar nunca reescreve o checkpoint: o upsert só roda no outro ramo.
    expect(calls.insertCount).toBe(0);
  });

  it("checkpoint sem cursor_value começa do início da janela (nada foi processado ainda)", async () => {
    const { db } = makeDb({
      selectRows: [
        {
          id: "cp-2",
          cursor_value: null,
          window_start: null,
          window_end: null,
          status: "paused",
        },
      ],
    });

    const state = await startOrResumeCheckpoint(db, "user-1", "item:acc-1", JANELA_PADRAO);

    expect(state.startCursor).toBeNull();
    // window_start/window_end nulos caem na janela do chamador.
    expect(state.from).toBe(JANELA_PADRAO.from);
    expect(state.to).toBe(JANELA_PADRAO.to);
  });

  it("checkpoint 'completed' NÃO é retomado — a rodada nova reseta a janela", async () => {
    const { db, calls } = makeDb({
      selectRows: [
        {
          id: "cp-antigo",
          cursor_value: "42",
          window_start: "2024-01-01",
          window_end: "2024-12-31",
          status: "completed",
        },
      ],
      returningRows: [{ id: "cp-antigo" }],
    });

    const state = await startOrResumeCheckpoint(db, "user-1", "item:acc-1", JANELA_PADRAO);

    expect(state).toEqual({
      id: "cp-antigo",
      from: JANELA_PADRAO.from,
      to: JANELA_PADRAO.to,
      startCursor: null,
    });
    expect(calls.insertCount).toBe(1);
    expect(calls.conflictSet).toMatchObject({ status: "running", cursor_value: null });
  });

  it("sem checkpoint existente, cria com contadores zerados e cursor_kind page_token", async () => {
    const { db, calls } = makeDb({ selectRows: [] });

    const state = await startOrResumeCheckpoint(db, "user-9", "item:acc-7", JANELA_PADRAO);

    expect(state).toEqual({
      id: "cp-novo",
      from: JANELA_PADRAO.from,
      to: JANELA_PADRAO.to,
      startCursor: null,
    });
    expect(calls.insertValues).toEqual({
      user_id: "user-9",
      source_type: "pluggy",
      scope_key: "item:acc-7",
      cursor_kind: "page_token",
      cursor_value: null,
      window_start: JANELA_PADRAO.from,
      window_end: JANELA_PADRAO.to,
      status: "running",
      messages_seen: 0,
      documents_created: 0,
      duplicates_skipped: 0,
      errors: 0,
    });
    // O SET do ON CONFLICT reescreve tudo menos as colunas do target
    // (user_id, source_type, scope_key) — reescrevê-las seria no-op.
    expect(calls.conflictSet).toEqual({
      cursor_kind: "page_token",
      cursor_value: null,
      window_start: JANELA_PADRAO.from,
      window_end: JANELA_PADRAO.to,
      status: "running",
      messages_seen: 0,
      documents_created: 0,
      duplicates_skipped: 0,
      errors: 0,
    });
  });

  it("erro do banco no upsert vira mensagem que identifica o scope_key", async () => {
    const { db } = makeDb({
      selectRows: [],
      insertThrows: new Error("connection terminated unexpectedly"),
    });

    await expect(
      startOrResumeCheckpoint(db, "user-1", "item:acc-3", JANELA_PADRAO),
    ).rejects.toThrow("Falha ao criar checkpoint (item:acc-3): connection terminated unexpectedly");
  });

  it("upsert sem linha de retorno também falha explicitamente", async () => {
    const { db } = makeDb({ selectRows: [], returningRows: [] });

    await expect(
      startOrResumeCheckpoint(db, "user-1", "item:acc-4", JANELA_PADRAO),
    ).rejects.toThrow("Falha ao criar checkpoint (item:acc-4)");
  });
});

describe("recordPageProgress", () => {
  it("acumula sobre os contadores atuais e grava o cursor da PRÓXIMA página", async () => {
    const { db, calls } = makeDb({
      selectRows: [{ messages_seen: 10, documents_created: 3, duplicates_skipped: 2, errors: 1 }],
    });

    await recordPageProgress(db, "cp-1", {
      page: 4,
      nextCursor: "eyJvZmZzZXQiOjIwMDB9",
      transactionsSeen: 5,
      created: 2,
      duplicates: 1,
      errors: 0,
    });

    expect(calls.updateSet).toEqual({
      cursor_value: "eyJvZmZzZXQiOjIwMDB9",
      messages_seen: 15,
      documents_created: 5,
      duplicates_skipped: 3,
      errors: 1,
    });
  });

  it("checkpoint sem linha lida trata os contadores atuais como zero", async () => {
    const { db, calls } = makeDb({ selectRows: [] });

    await recordPageProgress(db, "cp-1", {
      page: 1,
      nextCursor: null,
      transactionsSeen: 7,
      created: 7,
      duplicates: 0,
      errors: 0,
    });

    expect(calls.updateSet).toEqual({
      cursor_value: null,
      messages_seen: 7,
      documents_created: 7,
      duplicates_skipped: 0,
      errors: 0,
    });
  });
});

describe("completeCheckpoint", () => {
  it("marca status completed", async () => {
    const { db, calls } = makeDb({});

    await completeCheckpoint(db, "cp-1");

    expect(calls.updateSet).toEqual({ status: "completed" });
  });
});
