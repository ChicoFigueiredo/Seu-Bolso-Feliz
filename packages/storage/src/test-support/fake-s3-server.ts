/**
 * Servidor HTTP fake em loopback para os testes de `client.test.ts`. Não é
 * um mock de rede (msw/nock) nem fala com um SeaweedFS de verdade — é um
 * `node:http` simples que grava cada requisição recebida (método, path,
 * headers, corpo) e devolve o que o handler do teste mandar. Isso permite
 * afirmar sobre a URL montada e os headers de assinatura SigV4 exatamente
 * como chegaram na rede, sem depender de infra externa.
 */
import { createServer, type IncomingMessage, type Server } from "node:http";

export interface RecordedRequest {
  method: string;
  url: string;
  headers: Record<string, string | string[] | undefined>;
  body: Buffer;
}

export interface FakeResponse {
  status: number;
  body?: string;
  headers?: Record<string, string>;
}

export type FakeS3Handler = (request: RecordedRequest) => FakeResponse | Promise<FakeResponse>;

export interface FakeS3Server {
  url: string;
  requests: RecordedRequest[];
  close(): Promise<void>;
}

export async function startFakeS3Server(handler: FakeS3Handler): Promise<FakeS3Server> {
  const requests: RecordedRequest[] = [];

  const server: Server = createServer((req: IncomingMessage, res) => {
    const chunks: Buffer[] = [];
    req.on("data", (chunk: Buffer) => chunks.push(chunk));
    req.on("end", () => {
      const recorded: RecordedRequest = {
        method: req.method ?? "GET",
        url: req.url ?? "/",
        headers: req.headers,
        body: Buffer.concat(chunks),
      };
      requests.push(recorded);

      void Promise.resolve(handler(recorded)).then((response) => {
        res.writeHead(response.status, response.headers ?? {});
        res.end(response.body ?? "");
      });
    });
  });

  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const address = server.address();
  if (address === null || typeof address === "string") {
    throw new Error("Falha ao iniciar o servidor fake de teste (endereço inválido)");
  }

  return {
    url: `http://127.0.0.1:${address.port}`,
    requests,
    close: () =>
      new Promise<void>((resolve, reject) => {
        server.close((err) => (err ? reject(err) : resolve()));
      }),
  };
}
