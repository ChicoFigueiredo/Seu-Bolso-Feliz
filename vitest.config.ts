import { resolve } from "node:path";
import { defineConfig } from "vitest/config";

export default defineConfig({
  resolve: {
    alias: {
      "@sbf/domain": resolve(__dirname, "packages/domain/src/index.ts"),
      "@sbf/validation": resolve(__dirname, "packages/validation/src/index.ts"),
      "@sbf/shared-types": resolve(__dirname, "packages/shared-types/src/index.ts"),
      zod: resolve(__dirname, "node_modules/zod/index.js"),
    },
  },
  test: {
    globals: true,
    projects: [
      {
        test: {
          name: "unit",
          include: ["packages/*/src/**/*.test.ts", "__tests__/domain/**/*.test.ts"],
        },
      },
      {
        test: {
          name: "integration",
          include: ["__tests__/integration/**/*.test.ts"],
          testTimeout: 30_000,
        },
      },
    ],
    coverage: {
      provider: "v8",
      include: ["packages/domain/src/**", "packages/validation/src/**"],
    },
  },
});
