/**
 * Loja de blobs em memória que implementa a costura `BlobApi`.
 *
 * Substitui o servidor HTTP fake da versão S3 deste pacote: com o Vercel
 * Blob não há endpoint configurável para apontar em outro lugar — `get`
 * resolve o host da loja a partir do token e vai direto nele —, então o
 * ponto de troca honesto passou a ser a interface que `client.ts` consome.
 *
 * O que ela reproduz do serviço real, porque `client.ts` depende disso:
 * `put` recusa sobrescrever quando `allowOverwrite` é `false` (com a mensagem
 * "already exists" que o servidor devolve), `get` responde `null` para
 * caminho inexistente em vez de lançar, e `del` é silencioso sobre o que já
 * não existe.
 */
import { BlobError } from "@vercel/blob";
import type {
  BlobApi,
  BlobGetResult,
  BlobListResult,
  BlobPresignedUrl,
  BlobPutBody,
  BlobPutResult,
  BlobSignedToken,
} from "../blob-api";

export interface ObjetoFalso {
  bytes: Uint8Array<ArrayBuffer>;
  contentType?: string;
  gravadoEm: Date;
}

/** Uma chamada como ela chegou na costura — inclusive as opções. */
export interface ChamadaRegistrada {
  operacao: "put" | "get" | "del" | "list" | "issueSignedToken" | "presignUrl";
  alvo?: string | string[];
  options?: Record<string, unknown>;
}

export interface LojaDeBlobsFalsa extends BlobApi {
  /** Caminhos gravados, na ordem de inserção. */
  chaves(): string[];
  bytes(pathname: string): Uint8Array<ArrayBuffer> | undefined;
  semear(pathname: string, bytes: Uint8Array<ArrayBuffer>, gravadoEm?: Date): void;
  chamadas: ChamadaRegistrada[];
  /** Faz a próxima operação lançar este erro — para exercitar a tradução. */
  falharCom(erro: Error): void;
  /**
   * Esvazia objetos, chamadas e erro programado. Existe para quem precisa de
   * uma instância estável entre testes (o `vi.mock` de `blob-api` captura a
   * referência uma vez só e não pode trocá-la depois).
   */
  limpar(): void;
}

async function paraBytes(body: BlobPutBody): Promise<Uint8Array<ArrayBuffer>> {
  return new Uint8Array(await new Response(body).arrayBuffer());
}

export function criarLojaDeBlobsFalsa(): LojaDeBlobsFalsa {
  const objetos = new Map<string, ObjetoFalso>();
  const chamadas: ChamadaRegistrada[] = [];
  let proximoErro: Error | null = null;

  function registrar(chamada: ChamadaRegistrada): void {
    chamadas.push(chamada);
    if (proximoErro) {
      const erro = proximoErro;
      proximoErro = null;
      throw erro;
    }
  }

  return {
    chamadas,
    chaves: () => [...objetos.keys()],
    bytes: (pathname) => objetos.get(pathname)?.bytes,
    semear: (pathname, bytes, gravadoEm = new Date("2026-08-20T00:00:00.000Z")) => {
      objetos.set(pathname, { bytes, gravadoEm });
    },
    falharCom: (erro) => {
      proximoErro = erro;
    },
    limpar: () => {
      objetos.clear();
      chamadas.length = 0;
      proximoErro = null;
    },

    async put(pathname, body, options): Promise<BlobPutResult> {
      registrar({ operacao: "put", alvo: pathname, options: { ...options } });
      if (options.allowOverwrite === false && objetos.has(pathname)) {
        throw new BlobError(
          `This blob already exists, use allowOverwrite: true to overwrite it: ${pathname}`,
        );
      }
      objetos.set(pathname, {
        bytes: await paraBytes(body),
        ...(options.contentType ? { contentType: options.contentType } : {}),
        gravadoEm: new Date(),
      });
      return { pathname };
    },

    get(pathname, options): Promise<BlobGetResult | null> {
      registrar({ operacao: "get", alvo: pathname, options: { ...options } });
      const objeto = objetos.get(pathname);
      if (!objeto) return Promise.resolve(null);
      return Promise.resolve({ stream: new Response(objeto.bytes).body });
    },

    del(urlOrPathname, options): Promise<void> {
      registrar({ operacao: "del", alvo: urlOrPathname, options: { ...options } });
      for (const pathname of Array.isArray(urlOrPathname) ? urlOrPathname : [urlOrPathname]) {
        objetos.delete(pathname);
      }
      return Promise.resolve();
    },

    list(options): Promise<BlobListResult> {
      registrar({ operacao: "list", options: { ...options } });
      const prefixo = options?.prefix ?? "";
      const todos = [...objetos.entries()]
        .filter(([pathname]) => pathname.startsWith(prefixo))
        .map(([pathname, objeto]) => ({
          pathname,
          size: objeto.bytes.byteLength,
          uploadedAt: objeto.gravadoEm,
        }));

      const limite = options?.limit ?? todos.length;
      const blobs = todos.slice(0, limite);
      const hasMore = blobs.length < todos.length;
      return Promise.resolve({
        blobs,
        hasMore,
        ...(hasMore ? { cursor: `cursor-${blobs.length}` } : {}),
      });
    },

    issueSignedToken(options): Promise<BlobSignedToken> {
      registrar({ operacao: "issueSignedToken", options: { ...options } });
      return Promise.resolve({
        delegationToken: `delegacao-para-${options.pathname ?? "*"}`,
        clientSigningToken: "chave-de-assinatura",
      });
    },

    presignUrl(signedToken, options): Promise<BlobPresignedUrl> {
      registrar({ operacao: "presignUrl", alvo: options.pathname, options: { ...options } });
      const url = new URL(`https://loja-falsa.blob.vercel-storage.com/${options.pathname}`);
      url.searchParams.set("vercel-blob-delegation", signedToken.delegationToken);
      if (options.validUntil) url.searchParams.set("valid-until", String(options.validUntil));
      return Promise.resolve({ presignedUrl: url.toString() });
    },
  };
}
