import { resolve } from "node:path";
import { defineConfig } from "vitest/config";

const aliases = {
  "@sbf/domain": resolve(__dirname, "packages/domain/src/index.ts"),
  "@sbf/validation": resolve(__dirname, "packages/validation/src/index.ts"),
  "@sbf/shared-types": resolve(__dirname, "packages/shared-types/src/index.ts"),
  "@sbf/operations": resolve(__dirname, "packages/operations/src/index.ts"),
  "@sbf/ingestion-types": resolve(__dirname, "packages/ingestion-types/src/index.ts"),
};

const sharedTestConfig = {
  globals: true as const,
  server: { deps: { inline: ["zod"] } },
};

export default defineConfig({
  resolve: { alias: aliases },
  test: {
    ...sharedTestConfig,
    projects: [
      {
        resolve: { alias: aliases },
        test: {
          ...sharedTestConfig,
          name: "unit",
          include: ["packages/*/src/**/*.test.ts", "__tests__/domain/**/*.test.ts", "workers/*/src/**/*.test.ts"],
        },
      },
      {
        resolve: { alias: aliases },
        test: {
          ...sharedTestConfig,
          name: "integration",
          include: ["__tests__/integration/**/*.test.ts"],
          testTimeout: 30_000,
        },
      },
      {
        resolve: { alias: aliases },
        test: {
          ...sharedTestConfig,
          name: "e2e",
          include: ["__tests__/e2e/**/*.test.ts"],
          testTimeout: 60_000,
        },
      },
    ],
    coverage: {
      provider: "v8",
      include: ["packages/domain/src/**", "packages/validation/src/**", "packages/operations/src/**"],
    },
  },
});
