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
      "docs-html/",
      // Ferramental e saída do sync do design system (claude.ai/design). São
      // gitignorados e contêm bundle vendorizado do React — ~2400 erros que
      // afogam qualquer erro real do projeto.
      ".ds-sync/",
      "ds-bundle/",
      ".design-sync/previews/",
      "**/*.d.ts",
      "**/database.types.ts", // auto-gerado por introspecção do schema
      // Gerados por drizzle-kit introspect. schema.ts tem 2 edições MANUAIS
      // por cima (documentadas no cabeçalho do próprio arquivo) — está aqui
      // porque o corpo é gerado, não porque seja intocado.
      "packages/db/src/schema.ts",
      "packages/db/src/relations.ts", // auto-gerado por drizzle-kit introspect
      "packages/db/drizzle/**", // saída crua e gitignorada de `drizzle-kit introspect` (Step 2 da Task 1)
      "src/",
      "build.ts",
      "styles/",
    ],
  },
);
