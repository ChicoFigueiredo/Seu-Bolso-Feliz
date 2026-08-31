/**
 * Cliente de storage sobre o Vercel Blob (ver
 * docs/integrations/storage-vercel-blob.md). A forma
 * `storage.from(bucket).upload(...)` é mantida de propósito: é o que todo
 * call site do monorepo já usa, e o Vercel Blob não tem o conceito de bucket
 * — cada "bucket" daqui é apenas um prefixo de caminho dentro da loja única.
 *
 * PRIVACIDADE: são boletos, faturas e comprovantes. Todo blob é gravado com
 * `access: "private"`, então a URL do objeto não abre para quem não tem
 * credencial, e a única forma de mostrar um arquivo no navegador é a URL
 * assinada de vida curta de `getSignedUrl`. Não existe caminho aqui que
 * produza link público permanente.
 */
import { vercelBlobApi, type BlobApi, type BlobPutBody } from "./blob-api";
import { StorageError, toStorageError } from "./errors";
import type {
  ListObjectsOptions,
  ListObjectsResult,
  StorageConfig,
  UploadBody,
  UploadOptions,
} from "./types";

/**
 * Nunca `"public"`. Um blob público é legível por qualquer um que descubra a
 * URL, e documento financeiro não tem grau de exposição aceitável — nem
 * "difícil de adivinhar".
 */
const ACESSO = "private" as const;

const DEFAULT_SIGNED_URL_TTL_SECONDS = 3600;

export class StorageClient {
  private readonly token: string;
  private readonly api: BlobApi;

  /**
   * @param api Costura de teste — ver `blob-api.ts`. Em produção fica no
   * default e o cliente fala com o Vercel Blob de verdade.
   */
  constructor(config: StorageConfig, api: BlobApi = vercelBlobApi) {
    if (!config.token) {
      throw new Error("@sbf/storage: StorageConfig.token é obrigatório");
    }
    this.token = config.token;
    this.api = api;
  }

  from(bucket: string): StorageBucket {
    return new StorageBucket(this.token, bucket, this.api);
  }
}

export class StorageBucket {
  constructor(
    private readonly token: string,
    private readonly bucket: string,
    private readonly api: BlobApi,
  ) {}

  /**
   * `bucket/chave`. O caminho gravado no Postgres (`storage_path`) continua
   * sendo só a chave, sem o bucket — quem junta os dois é sempre este método,
   * na ida e na volta, e é por isso que `addRandomSuffix` fica desligado no
   * `put`: o nome do blob precisa ser exatamente o que o banco guardou.
   */
  private caminho(path: string): string {
    return `${this.bucket}/${path.replace(/^\/+/, "")}`;
  }

  async upload(path: string, data: UploadBody, options?: UploadOptions): Promise<void> {
    try {
      await this.api.put(this.caminho(path), paraCorpoDoBlob(data), {
        access: ACESSO,
        token: this.token,
        addRandomSuffix: false,
        allowOverwrite: options?.upsert !== false,
        ...(options?.contentType ? { contentType: options.contentType } : {}),
      });
    } catch (err) {
      throw toStorageError(err, path);
    }
  }

  async download(path: string): Promise<Uint8Array> {
    let resultado;
    try {
      resultado = await this.api.get(this.caminho(path), { access: ACESSO, token: this.token });
    } catch (err) {
      throw toStorageError(err, path);
    }

    // `get` devolve `null` para caminho inexistente em vez de lançar. Quem
    // chama espera exceção (o processor traduz em "Failed to download"), então
    // a ausência vira `StorageError` aqui, com o mesmo 404/not_found que uma
    // recusa do servidor produziria.
    if (!resultado?.stream) {
      throw new StorageError(`Objeto não encontrado em "${path}"`, 404, "not_found", path);
    }
    return new Uint8Array(await new Response(resultado.stream).arrayBuffer());
  }

  /**
   * URL temporária para o navegador exibir o arquivo sem que o token da loja
   * saia do servidor. Duas etapas: pedir à API do Vercel Blob uma delegação
   * restrita a este caminho e a esta operação (`issueSignedToken`, uma ida à
   * rede) e assinar a URL com ela (`presignUrl`, HMAC local).
   *
   * O link expira em `expiresInSeconds` — vazar num histórico de navegação
   * não vira acesso permanente ao documento.
   */
  async getSignedUrl(
    path: string,
    expiresInSeconds: number = DEFAULT_SIGNED_URL_TTL_SECONDS,
  ): Promise<string> {
    const pathname = this.caminho(path);
    const validUntil = Date.now() + expiresInSeconds * 1000;
    try {
      const delegacao = await this.api.issueSignedToken({
        token: this.token,
        pathname,
        operations: ["get"],
        validUntil,
      });
      const { presignedUrl } = await this.api.presignUrl(delegacao, {
        operation: "get",
        pathname,
        validUntil,
        access: ACESSO,
      });
      return presignedUrl;
    } catch (err) {
      throw toStorageError(err, path);
    }
  }

  /**
   * Remove um ou mais objetos, numa chamada só (o `del` do SDK aceita lista).
   * Remover o que já não existe é sucesso, não erro — a exclusão de documento
   * pode ser reexecutada depois de uma falha parcial.
   */
  async remove(paths: string | string[]): Promise<void> {
    const chaves = (Array.isArray(paths) ? paths : [paths]).map((path) => this.caminho(path));
    if (chaves.length === 0) return;

    try {
      await this.api.del(chaves, { token: this.token });
    } catch (err) {
      const erro = toStorageError(err, Array.isArray(paths) ? undefined : paths);
      if (erro.code === "not_found") return;
      throw erro;
    }
  }

  async list(prefix?: string, options?: ListObjectsOptions): Promise<ListObjectsResult> {
    const prefixoDoBucket = `${this.bucket}/`;
    try {
      const resultado = await this.api.list({
        token: this.token,
        prefix: prefix ? `${prefixoDoBucket}${prefix}` : prefixoDoBucket,
        ...(options?.maxKeys === undefined ? {} : { limit: options.maxKeys }),
        ...(options?.continuationToken ? { cursor: options.continuationToken } : {}),
      });

      return {
        // O bucket é prefixo de caminho, não parte da chave: quem chama
        // informou `user-1/boleto.pdf` e precisa receber isso de volta, não
        // `ingestion-originals/user-1/boleto.pdf`.
        objects: resultado.blobs.map((blob) => ({
          key: blob.pathname.startsWith(prefixoDoBucket)
            ? blob.pathname.slice(prefixoDoBucket.length)
            : blob.pathname,
          size: blob.size,
          lastModified: blob.uploadedAt.toISOString(),
        })),
        isTruncated: resultado.hasMore,
        ...(resultado.cursor ? { nextContinuationToken: resultado.cursor } : {}),
      };
    } catch (err) {
      throw toStorageError(err);
    }
  }
}

/**
 * O SDK aceita `Buffer`, mas não `Uint8Array` genérico — que é justamente o
 * que os workers têm em mãos. `.slice()` materializa um `ArrayBuffer` próprio
 * porque a view costuma ser uma janela sobre um buffer maior (todo `Buffer`
 * do Node é), e passar `.buffer` cru enviaria os bytes vizinhos junto.
 */
function paraCorpoDoBlob(data: UploadBody): BlobPutBody {
  if (ArrayBuffer.isView(data)) {
    return data.buffer.slice(data.byteOffset, data.byteOffset + data.byteLength);
  }
  return data;
}
