import net from "node:net";
import { spawn } from "node:child_process";

const host = process.env.HOST ?? "0.0.0.0";
const preferredPort = toPositiveInt(process.env.PORT, 3105);
const maxPortScan = toPositiveInt(process.env.MAX_PORT_SCAN, 50);

function toPositiveInt(raw, fallback) {
  const parsed = Number.parseInt(raw ?? "", 10);
  if (!Number.isFinite(parsed) || parsed <= 0) return fallback;
  return parsed;
}

function isPortAvailable(port, bindHost) {
  return new Promise((resolve) => {
    const server = net.createServer();

    server.once("error", () => {
      resolve(false);
    });

    server.once("listening", () => {
      server.close(() => resolve(true));
    });

    server.listen(port, bindHost);
  });
}

async function findAvailablePort(startPort, bindHost, attempts) {
  for (let offset = 0; offset <= attempts; offset += 1) {
    const port = startPort + offset;
    const available = await isPortAvailable(port, bindHost);
    if (available) return port;
  }
  throw new Error(
    `Nenhuma porta livre encontrada entre ${startPort} e ${startPort + attempts}.`,
  );
}

async function main() {
  const port = await findAvailablePort(preferredPort, host, maxPortScan);

  if (port !== preferredPort) {
    console.warn(
      `[web:dev] Porta ${preferredPort} ocupada. Usando porta ${port} automaticamente.`,
    );
  } else {
    console.info(`[web:dev] Usando porta ${port}.`);
  }

  const child = spawn(
    "next",
    ["dev", "--turbopack", "-p", String(port), "-H", host],
    {
      stdio: "inherit",
      env: {
        ...process.env,
        PORT: String(port),
      },
      shell: process.platform === "win32",
    },
  );

  child.on("exit", (code, signal) => {
    if (signal) {
      process.kill(process.pid, signal);
      return;
    }
    process.exit(code ?? 0);
  });

  process.on("SIGINT", () => child.kill("SIGINT"));
  process.on("SIGTERM", () => child.kill("SIGTERM"));
}

main().catch((error) => {
  const message = error instanceof Error ? error.message : String(error);
  console.error(`[web:dev] Falha ao iniciar: ${message}`);
  process.exit(1);
});
