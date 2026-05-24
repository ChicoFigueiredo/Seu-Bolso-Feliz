import { serve } from "bun";
import index from "./index.html";

const DEFAULT_PORT = 3105;
const MAX_PORT_ATTEMPTS = 20;

const requestedPort = Number(process.env.PORT);
const startPort =
  Number.isFinite(requestedPort) && requestedPort > 0 ? requestedPort : DEFAULT_PORT;

function createServer(port: number) {
  return serve({
    port,
    routes: {
      // Serve index.html for all unmatched routes.
      "/*": index,

      "/api/hello": {
        async GET(req) {
          return Response.json({
            message: "Hello, world!",
            method: "GET",
          });
        },
        async PUT(req) {
          return Response.json({
            message: "Hello, world!",
            method: "PUT",
          });
        },
      },

      "/api/hello/:name": async (req) => {
        const name = req.params.name;
        return Response.json({
          message: `Hello, ${name}!`,
        });
      },
    },

    development: process.env.NODE_ENV !== "production" && {
      // Enable browser hot reloading in development
      hmr: true,

      // Echo console logs from the browser to the server
      console: true,
    },
  });
}

let server: ReturnType<typeof serve> | null = null;
let port = startPort;

for (let attempt = 0; attempt < MAX_PORT_ATTEMPTS; attempt += 1) {
  try {
    server = createServer(port);
    break;
  } catch (error) {
    const err = error as { code?: string };
    if (err.code === "EADDRINUSE") {
      port += 1;
      continue;
    }
    throw error;
  }
}

if (!server) {
  throw new Error(`Nao foi possivel iniciar o servidor entre as portas ${startPort} e ${port}.`);
}

console.log(`🚀 Server running at ${server.url}`);
