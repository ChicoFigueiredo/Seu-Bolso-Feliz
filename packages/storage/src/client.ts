/**
 * Cliente S3-compatível para SeaweedFS. Assina cada requisição com SigV4 via
 * `aws4fetch` (ver docs/integrations/storage-seaweedfs.md para o porquê dessa
 * lib em vez de @aws-sdk/client-s3). A forma `storage.from(bucket).upload(...)`
 * espelha de propósito `supabase.storage.from(bucket).upload(...)`, que é o
 * que todo call site atual já usa — a ideia é que trocar o import seja quase
 * mecânico para quem for migrar `apps/web`/`workers`.
 */
import { AwsClient } from "aws4fetch";
import { StorageError } from "./errors";
import type {
  ListObjectsOptions,
  ListObjectsResult,
  ResolvedStorageConfig,
  StorageConfig,
  UploadBody,
  UploadOptions,
} from "./types";
import { buildListUrl, buildObjectUrl } from "./url";
import { parseListObjectsXml, parseS3ErrorXml } from "./xml";

const DEFAULT_REGION = "us-east-1";
const DEFAULT_SIGNED_URL_TTL_SECONDS = 3600;
// O default do aws4fetch é 10 (backoff exponencial chega a ~51s acumulados).
// Bom demais para um worker em background, ruim demais para uma Server
// Action esperando resposta — ver o comentário em StorageConfig.retries.
const DEFAULT_RETRIES = 2;

export class StorageClient {
  private readonly aws: AwsClient;
  private readonly config: ResolvedStorageConfig;

  constructor(config: StorageConfig) {
    if (!config.endpoint) throw new Error("@sbf/storage: StorageConfig.endpoint é obrigatório");
    if (!config.accessKeyId) {
      throw new Error("@sbf/storage: StorageConfig.accessKeyId é obrigatório");
    }
    if (!config.secretAccessKey) {
      throw new Error("@sbf/storage: StorageConfig.secretAccessKey é obrigatório");
    }

    this.config = {
      endpoint: config.endpoint,
      region: config.region ?? DEFAULT_REGION,
      forcePathStyle: config.forcePathStyle ?? true,
    };

    // service/region são passados explicitamente porque o aws4fetch só os
    // deduz do hostname para domínios *.amazonaws.com — um endpoint próprio
    // (subdomínio do CEO, ou 127.0.0.1 em dev) não bate em nenhum desses
    // padrões e ficaria sem service/region, quebrando a assinatura.
    this.aws = new AwsClient({
      accessKeyId: config.accessKeyId,
      secretAccessKey: config.secretAccessKey,
      service: "s3",
      region: this.config.region,
      retries: config.retries ?? DEFAULT_RETRIES,
    });
  }

  from(bucket: string): StorageBucket {
    return new StorageBucket(this.aws, this.config, bucket);
  }
}

export class StorageBucket {
  constructor(
    private readonly aws: AwsClient,
    private readonly config: ResolvedStorageConfig,
    private readonly bucket: string,
  ) {}

  async upload(path: string, data: UploadBody, options?: UploadOptions): Promise<void> {
    if (options?.upsert === false && (await this.exists(path))) {
      throw new StorageError(`Objeto já existe em "${path}"`, 409, "already_exists", path);
    }

    const url = buildObjectUrl(this.config, this.bucket, path);
    const headers: Record<string, string> = {};
    if (options?.contentType) headers["Content-Type"] = options.contentType;

    const response = await this.aws.fetch(url.toString(), { method: "PUT", body: data, headers });
    if (!response.ok) throw await toStorageError(response, path);
  }

  async download(path: string): Promise<Uint8Array> {
    const url = buildObjectUrl(this.config, this.bucket, path);
    const response = await this.aws.fetch(url.toString(), { method: "GET" });
    if (!response.ok) throw await toStorageError(response, path);
    return new Uint8Array(await response.arrayBuffer());
  }

  /** URL GET pré-assinada, para o navegador exibir o arquivo sem ver a credencial. */
  async getSignedUrl(
    path: string,
    expiresInSeconds: number = DEFAULT_SIGNED_URL_TTL_SECONDS,
  ): Promise<string> {
    const url = buildObjectUrl(this.config, this.bucket, path);
    // Precisa ir na URL antes de assinar: o aws4fetch só respeita um
    // X-Amz-Expires pré-existente na query; se ausente, aplica 86400 (24h)
    // por padrão dele, não o TTL que pedimos aqui.
    url.searchParams.set("X-Amz-Expires", String(expiresInSeconds));
    const signedRequest = await this.aws.sign(url.toString(), {
      method: "GET",
      aws: { signQuery: true },
    });
    return signedRequest.url;
  }

  /**
   * Remove um ou mais objetos. Sequencial (uma requisição DELETE por chave),
   * não o batch-delete do S3 (`POST /?delete`) — nenhum call site atual
   * remove mais que um punhado de chaves por vez, então o round-trip extra
   * não pesa e evita ter que montar/parsear XML de batch aqui.
   */
  async remove(paths: string | string[]): Promise<void> {
    const keys = Array.isArray(paths) ? paths : [paths];
    for (const path of keys) {
      const url = buildObjectUrl(this.config, this.bucket, path);
      const response = await this.aws.fetch(url.toString(), { method: "DELETE" });
      // DELETE é idempotente no S3: 404 não é erro, "já não existe" é sucesso.
      if (!response.ok && response.status !== 404) throw await toStorageError(response, path);
    }
  }

  async list(prefix?: string, options?: ListObjectsOptions): Promise<ListObjectsResult> {
    const url = buildListUrl(this.config, this.bucket, { prefix, ...options });
    const response = await this.aws.fetch(url.toString(), { method: "GET" });
    if (!response.ok) throw await toStorageError(response);
    return parseListObjectsXml(await response.text());
  }

  private async exists(path: string): Promise<boolean> {
    const url = buildObjectUrl(this.config, this.bucket, path);
    const response = await this.aws.fetch(url.toString(), { method: "HEAD" });
    return response.ok;
  }
}

async function toStorageError(response: Response, key?: string): Promise<StorageError> {
  const body = await response.text().catch(() => "");
  const parsed = parseS3ErrorXml(body);
  const message =
    parsed.message ?? `Requisição de storage falhou (${response.status} ${response.statusText})`;
  return new StorageError(message, response.status, parsed.code, key);
}
