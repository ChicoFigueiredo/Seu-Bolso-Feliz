import { defineConfig } from "drizzle-kit";

export default defineConfig({
  dialect: "postgresql",
  schema: "./src/schema.ts",
  // ATENÇÃO: `out` é a pasta de SAÍDA do drizzle-kit (introspect/generate), e
  // NÃO é onde o schema versionado vive. O `drizzle-kit introspect` despeja
  // aqui schema.ts, relations.ts, o snapshot `0000_*.sql` e `meta/_journal.json`
  // de uma vez; por isso `out` NÃO aponta para "./src" (sujaria src/ com
  // artefatos de migration). Depois de introspectar, mova à mão
  // drizzle/schema.ts -> src/schema.ts e drizzle/relations.ts -> src/relations.ts,
  // e reaplique as edições manuais listadas no cabeçalho de src/schema.ts.
  // packages/db/drizzle/ é gitignorado (artefato descartável).
  out: "./drizzle",
  dbCredentials: {
    url: process.env.DATABASE_URL!,
  },
});
