/**
 * A costura entre este pacote e o SDK `@vercel/blob`.
 *
 * O SDK expõe funções de módulo (`put`, `get`, `del`…), não um objeto de
 * cliente. Um teste que quisesse exercitar `StorageBucket` teria, por isso,
 * duas saídas ruins: falar com a loja de verdade pela rede, ou mockar o
 * módulo inteiro e passar a testar o mock. `BlobApi` é a terceira saída — a
 * lista exata de operações que este pacote usa, injetável no construtor, com
 * a ligação real (`vercelBlobApi`) como default.
 *
 * Os tipos de retorno declarados aqui são **subconjuntos** dos do SDK: só os
 * campos que `client.ts` de fato lê. Os retornos reais são atribuíveis a
 * eles, e um dublê de teste não precisa inventar `etag`, `downloadUrl` e
 * companhia só para satisfazer o compilador.
 */
import { del, get, issueSignedToken, list, presignUrl, put } from "@vercel/blob";
import type {
  BlobAccessType,
  DeleteCommandOptions,
  GetCommandOptions,
  IssueSignedTokenOptions,
  ListCommandOptions,
  PresignUrlOptions,
  PutCommandOptions,
} from "@vercel/blob";

/**
 * Corpos que o `put` do SDK aceita, na fatia que este pacote usa.
 *
 * Não inclui `ArrayBufferView` (o `Uint8Array` dos workers) de propósito: o
 * SDK só aceita `Buffer`, e converter a view em `ArrayBuffer` é trabalho de
 * `client.ts`, feito uma vez, antes de chegar aqui.
 */
export type BlobPutBody = Blob | ArrayBuffer | string;

export interface BlobPutResult {
  pathname: string;
}

export interface BlobGetResult {
  /** `null` na resposta 304 (não modificado), que este pacote nunca pede. */
  stream: ReadableStream<Uint8Array> | null;
}

export interface BlobListedObject {
  pathname: string;
  size: number;
  uploadedAt: Date;
}

export interface BlobListResult {
  blobs: BlobListedObject[];
  cursor?: string;
  hasMore: boolean;
}

export interface BlobSignedToken {
  delegationToken: string;
  clientSigningToken: string;
}

export interface BlobPresignedUrl {
  presignedUrl: string;
}

export interface BlobApi {
  put(pathname: string, body: BlobPutBody, options: PutCommandOptions): Promise<BlobPutResult>;
  get(urlOrPathname: string, options: GetCommandOptions): Promise<BlobGetResult | null>;
  del(urlOrPathname: string | string[], options?: DeleteCommandOptions): Promise<void>;
  list(options?: ListCommandOptions): Promise<BlobListResult>;
  issueSignedToken(options: IssueSignedTokenOptions): Promise<BlobSignedToken>;
  presignUrl(
    signedToken: BlobSignedToken,
    options: PresignUrlOptions & { access: BlobAccessType },
  ): Promise<BlobPresignedUrl>;
}

/** A ligação real com o Vercel Blob — o default de todo `StorageClient`. */
export const vercelBlobApi: BlobApi = { put, get, del, list, issueSignedToken, presignUrl };
