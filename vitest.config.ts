import { defineConfig } from "vitest/config";

export default defineConfig({
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
