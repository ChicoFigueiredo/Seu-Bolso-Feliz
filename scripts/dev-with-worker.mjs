import { spawn } from "node:child_process";

const children = [];
let shuttingDown = false;

function startProcess(name, command, envOverrides = {}) {
  const child = spawn(command, {
    stdio: "inherit",
    shell: true,
    env: {
      ...process.env,
      ...envOverrides,
    },
  });

  child.on("exit", (code, signal) => {
    if (shuttingDown) return;

    const codeLabel = signal ? `signal ${signal}` : `code ${code ?? 0}`;
    console.log(`[dev] Processo ${name} finalizou com ${codeLabel}. Encerrando os demais...`);
    shutdown(code ?? (signal ? 1 : 0));
  });

  children.push(child);
}

function shutdown(exitCode = 0) {
  if (shuttingDown) return;
  shuttingDown = true;

  for (const child of children) {
    if (child.exitCode === null) {
      child.kill("SIGTERM");
    }
  }

  setTimeout(() => {
    for (const child of children) {
      if (child.exitCode === null) {
        child.kill("SIGKILL");
      }
    }
    process.exit(exitCode);
  }, 500);
}

process.on("SIGINT", () => shutdown(0));
process.on("SIGTERM", () => shutdown(0));

startProcess("web", "bun run dev:web");
startProcess("worker", "bun run dev:worker", {
  INGESTION_ENABLE_OCRMYPDF: process.env.INGESTION_ENABLE_OCRMYPDF ?? "true",
  OCRMYPDF_BIN: process.env.OCRMYPDF_BIN ?? "ocrmypdf",
});
