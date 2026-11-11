import { resolve } from "node:path";
import { configDefaults, defineConfig } from "vitest/config";

const aliases = {
  // O prefixo "@/" de apps/web só era resolvido pelo TypeScript, nunca pelo
  // Vite. Qualquer teste que importasse um módulo "@/..." sem mocká-lo
  // quebrava na resolução — e a saída de erro não deixa óbvio que a causa é
  // configuração, então o reflexo é mockar mais em vez de consertar isto.
  "@/": resolve(__dirname, "apps/web/src") + "/",
  "@sbf/domain": resolve(__dirname, "packages/domain/src/index.ts"),
  "@sbf/validation": resolve(__dirname, "packages/validation/src/index.ts"),
  "@sbf/shared-types": resolve(__dirname, "packages/shared-types/src/index.ts"),
  "@sbf/operations": resolve(__dirname, "packages/operations/src/index.ts"),
  "@sbf/ingestion-types": resolve(__dirname, "packages/ingestion-types/src/index.ts"),
  "@sbf/storage": resolve(__dirname, "packages/storage/src/index.ts"),
  "@sbf/contracts": resolve(__dirname, "packages/contracts/src/index.ts"),
  "@sbf/db": resolve(__dirname, "packages/db/src/index.ts"),
  "@sbf/cli": resolve(__dirname, "workers/cli/src/index.ts"),
  // Precisa vir antes de "@sbf/worker-ingestion": Vite casa por prefixo, e a
  // entrada genérica intercepta o subpath (o alias vira o caminho do arquivo
  // index.ts + "/reconciliation", que não existe — mesmo se o package.json
  // do pacote já declara esse subpath via "exports").
  "@sbf/worker-ingestion/reconciliation": resolve(
    __dirname,
    "workers/ingestion/src/reconciliation/reconciliation.ts",
  ),
  "@sbf/worker-ingestion": resolve(__dirname, "workers/ingestion/src/index.ts"),
  "@sbf/worker-gmail-scanner": resolve(__dirname, "workers/gmail-scanner/src/index.ts"),
  "@sbf/worker-local-scanner": resolve(__dirname, "workers/local-scanner/src/index.ts"),
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
          include: [
            "packages/*/src/**/*.test.ts",
            "__tests__/domain/**/*.test.ts",
            "workers/*/src/**/*.test.ts",
            // Lógica pura das telas (aritmética de rateio, filtros, períodos).
            // Sem esta linha os testes existem, passam quando rodados à mão e
            // nunca rodam na suíte — código morto que dá a sensação de
            // cobertura. Fica em `unit` porque não toca banco; o que toca
            // banco vive em app/actions e já está no projeto `integration`.
            "apps/web/src/components/**/*.test.ts",
            "apps/web/src/lib/**/*.test.ts",
          ],
          // `lib/ai/` toca o banco e já é do projeto `integration`. Sem esta
          // exclusão os dois projetos coletam o mesmo arquivo: aqui ele se
          // auto-pula por não ter banco, e a suíte passa a relatar 8 testes
          // "pulados" que na verdade rodam — ruído que treina a gente a
          // ignorar a linha de pulados, justo a que denuncia teste morto.
          exclude: [...configDefaults.exclude, "apps/web/src/lib/ai/**"],
        },
      },
      {
        resolve: { alias: aliases },
        test: {
          ...sharedTestConfig,
          name: "integration",
          include: [
            "__tests__/integration/**/*.test.ts",
            "apps/web/src/app/actions/**/*.test.ts",
            "apps/web/src/app/api/**/*.test.ts",
            "apps/web/src/lib/ai/**/*.test.ts",
          ],
          testTimeout: 30_000,
          // Todos os arquivos deste projeto compartilham o mesmo Postgres
          // real e os mesmos UUIDs literais de usuário de teste
          // (USER_A/USER_B). Rodar arquivos em paralelo deixa consultas sem
          // filtro adicional (ex.: "produto financeiro padrão do usuário",
          // sem ORDER BY, por design) verem linhas semeadas por OUTRO
          // arquivo de teste rodando ao mesmo tempo — flakiness real, não
          // hipotética (achado ao rodar a suíte inteira acumulada).
          fileParallelism: false,
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
      include: [
        "packages/domain/src/**",
        "packages/validation/src/**",
        "packages/operations/src/**",
        "packages/contracts/src/**",
      ],
    },
  },
});
