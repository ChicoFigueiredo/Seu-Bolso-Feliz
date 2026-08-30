/**
 * Storage S3 em memória, servido por HTTP de verdade.
 *
 * O pipeline de ingestão não recebe um cliente de storage por parâmetro: o
 * worker resolve o seu por ambiente (`getIngestionBucket()` →
 * `createStorageClientFromEnv()`). Então o único jeito honesto de exercitar
 * scanner→worker de ponta a ponta é dar a ele um endpoint S3 real para falar —
 * um dublê de objeto seria injetado num lugar onde a produção não injeta nada.
 *
 * Não reaproveita `packages/storage/src/test-support/fake-s3-server.ts` por um
 * motivo específico: aquele servidor responde `string`, e `res.end(str)` grava
 * em UTF-8 — bytes de PDF passariam por ele corrompidos. Aqui o corpo é
 * `Buffer` do começo ao fim.
 *
 * O que este servidor NÃO faz: validar a assinatura SigV4. Assinatura é
 * assunto de `packages/storage/src/client.test.ts`; aqui só interessa que os
 * bytes que subiram sejam os bytes que descem.
 */
import { createServer, type Server } from "node:http";

export interface FakeStorage {
  /** Endpoint para `S3_ENDPOINT`. */
  url: string;
  /** Chaves guardadas, no formato `bucket/caminho/do/objeto`. */
  chaves(): string[];
  close(): Promise<void>;
}

/** `/bucket/a/b%20c.pdf` → `bucket/a/b c.pdf`. */
function chaveDaUrl(rawUrl: string): { bucket: string; chave: string } {
  const semQuery = rawUrl.split("?")[0] ?? "/";
  const segmentos = semQuery.replace(/^\//, "").split("/").map(decodeURIComponent);
  const bucket = segmentos.shift() ?? "";
  return { bucket, chave: segmentos.join("/") };
}

function escaparXml(valor: string): string {
  return valor
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;");
}

/**
 * Sobe o servidor e aponta as variáveis de ambiente do `@sbf/storage` para ele.
 *
 * Devolver o `close` é obrigação de quem chama (`afterAll`): um servidor
 * pendurado segura o processo do Vitest aberto.
 */
export async function startFakeStorage(): Promise<FakeStorage> {
  const objetos = new Map<string, { corpo: Buffer; contentType: string; gravadoEm: string }>();

  const server: Server = createServer((req, res) => {
    const pedacos: Buffer[] = [];
    req.on("data", (c: Buffer) => pedacos.push(c));
    req.on("end", () => {
      const metodo = req.method ?? "GET";
      const rawUrl = req.url ?? "/";
      const { bucket, chave } = chaveDaUrl(rawUrl);
      const id = `${bucket}/${chave}`;

      // Listagem: GET no bucket com ?list-type=2 (o objeto em si nunca tem
      // chave vazia, então a distinção é inequívoca).
      if (metodo === "GET" && chave === "" && rawUrl.includes("list-type=2")) {
        const prefixo = new URL(rawUrl, "http://x").searchParams.get("prefix") ?? "";
        const itens = [...objetos.entries()]
          .filter(([k]) => k.startsWith(`${bucket}/`))
          .map(([k, v]) => ({ chave: k.slice(bucket.length + 1), ...v }))
          .filter((o) => o.chave.startsWith(prefixo));

        const xml =
          `<?xml version="1.0" encoding="UTF-8"?><ListBucketResult>` +
          `<IsTruncated>false</IsTruncated>` +
          itens
            .map(
              (o) =>
                `<Contents><Key>${escaparXml(o.chave)}</Key>` +
                `<Size>${o.corpo.byteLength}</Size>` +
                `<LastModified>${o.gravadoEm}</LastModified></Contents>`,
            )
            .join("") +
          `</ListBucketResult>`;
        res.writeHead(200, { "Content-Type": "application/xml" });
        res.end(xml);
        return;
      }

      if (metodo === "PUT") {
        objetos.set(id, {
          corpo: Buffer.concat(pedacos),
          contentType: req.headers["content-type"] ?? "application/octet-stream",
          gravadoEm: new Date().toISOString(),
        });
        res.writeHead(200);
        res.end();
        return;
      }

      const objeto = objetos.get(id);

      if (metodo === "HEAD") {
        res.writeHead(objeto ? 200 : 404);
        res.end();
        return;
      }

      if (metodo === "GET") {
        if (!objeto) {
          res.writeHead(404, { "Content-Type": "application/xml" });
          res.end(
            `<?xml version="1.0" encoding="UTF-8"?><Error><Code>NoSuchKey</Code>` +
              `<Message>Objeto inexistente: ${escaparXml(chave)}</Message></Error>`,
          );
          return;
        }
        res.writeHead(200, {
          "Content-Type": objeto.contentType,
          "Content-Length": String(objeto.corpo.byteLength),
        });
        res.end(objeto.corpo);
        return;
      }

      if (metodo === "DELETE") {
        objetos.delete(id);
        res.writeHead(204);
        res.end();
        return;
      }

      res.writeHead(405);
      res.end();
    });
  });

  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const endereco = server.address();
  if (endereco === null || typeof endereco === "string") {
    throw new Error("storage fake: não consegui descobrir a porta");
  }
  const url = `http://127.0.0.1:${endereco.port}`;

  process.env.S3_ENDPOINT = url;
  process.env.S3_ACCESS_KEY_ID = "teste";
  process.env.S3_SECRET_ACCESS_KEY = "teste";
  process.env.S3_REGION = "us-east-1";
  process.env.S3_FORCE_PATH_STYLE = "true";

  return {
    url,
    chaves: () => [...objetos.keys()],
    close: () =>
      new Promise<void>((resolve, reject) => {
        objetos.clear();
        server.close((err) => (err ? reject(err) : resolve()));
      }),
  };
}
