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
      "supabase/.temp/**", // artefato gitignorado do `supabase start`, não é código do projeto
      "docs-html/",
      "**/*.d.ts",
      "**/database.types.ts", // auto-gerado pelo Supabase CLI
      // Gerados por drizzle-kit introspect. schema.ts tem 2 edições MANUAIS
      // por cima (documentadas no cabeçalho do próprio arquivo) — está aqui
      // porque o corpo é gerado, não porque seja intocado.
      "packages/db/src/schema.ts",
      "packages/db/src/relations.ts", // auto-gerado por drizzle-kit introspect
      "src/",
      "build.ts",
      "styles/",
    ],
  },
);
