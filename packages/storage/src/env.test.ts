import { describe, expect, it } from "vitest";
import { loadStorageConfigFromEnv } from "./env";

function fullEnv(overrides: Record<string, string | undefined> = {}): Record<string, string> {
  return {
    S3_ENDPOINT: "http://127.0.0.1:9000",
    S3_ACCESS_KEY_ID: "key-id",
    S3_SECRET_ACCESS_KEY: "secret",
    ...overrides,
  } as Record<string, string>;
}

describe("loadStorageConfigFromEnv", () => {
  it("lê as variáveis obrigatórias e aplica defaults", () => {
    const config = loadStorageConfigFromEnv(fullEnv());
    expect(config).toEqual({
      endpoint: "http://127.0.0.1:9000",
      accessKeyId: "key-id",
      secretAccessKey: "secret",
      region: "us-east-1",
      forcePathStyle: true,
    });
  });

  it("respeita S3_REGION quando informado", () => {
    const config = loadStorageConfigFromEnv(fullEnv({ S3_REGION: "sa-east-1" }));
    expect(config.region).toBe("sa-east-1");
  });

  it("só desliga forcePathStyle com o literal 'false'", () => {
    expect(loadStorageConfigFromEnv(fullEnv({ S3_FORCE_PATH_STYLE: "false" })).forcePathStyle).toBe(
      false,
    );
    expect(loadStorageConfigFromEnv(fullEnv({ S3_FORCE_PATH_STYLE: "true" })).forcePathStyle).toBe(
      true,
    );
    expect(
      loadStorageConfigFromEnv(fullEnv({ S3_FORCE_PATH_STYLE: "qualquer-outra-coisa" }))
        .forcePathStyle,
    ).toBe(true);
  });

  it.each(["S3_ENDPOINT", "S3_ACCESS_KEY_ID", "S3_SECRET_ACCESS_KEY"])(
    "lança com o nome da variável quando %s está ausente",
    (missingKey) => {
      const env = fullEnv({ [missingKey]: undefined });
      expect(() => loadStorageConfigFromEnv(env)).toThrow(new RegExp(missingKey));
    },
  );
});
