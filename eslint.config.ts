import eslint from "@eslint/js";
import tseslint from "typescript-eslint";
import prettier from "eslint-config-prettier";

export default tseslint.config(
  eslint.configs.recommended,
  ...tseslint.configs.recommended,
  prettier,
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
      "src/",
      "build.ts",
      "styles/",
    ],
  },
);
