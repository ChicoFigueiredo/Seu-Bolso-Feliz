import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import prettier from "eslint-config-prettier";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  prettier,
  {
    files: ["scripts/**/*.mjs", "apps/**/scripts/**/*.mjs"],
    languageOptions: {
      globals: {
        console: "readonly",
        process: "readonly",
        setTimeout: "readonly",
      },
    },
  },
  {
    ignores: [
      "node_modules/",
      "dist/",
      ".next/",
      "**/.next/",
      "supabase/functions/**",
      "docs-html/",
      "**/*.d.ts",
      "**/database.types.ts", // auto-gerado pelo Supabase CLI
      "packages/db/src/schema.ts", // auto-gerado por drizzle-kit introspect
      "packages/db/src/relations.ts", // auto-gerado por drizzle-kit introspect
      "src/",
      "build.ts",
      "styles/",
    ],
  },
);
