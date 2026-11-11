/**
 * Storage em memória para os testes que rodam o pipeline de ponta a ponta.
 *
 * O pipeline não recebe um cliente de storage por parâmetro: o worker resolve
 * o dele por ambiente (`getIngestionBucket()` → `createStorageClientFromEnv()`).
 * O Vercel Blob não tem endpoint configurável — o SDK resolve o host da loja
 * a partir do token —, então não dá para subir um servidor HTTP local e mandar
 * o worker falar com ele. O ponto de troca é o módulo `blob-api` do
 * `@sbf/storage`, que cada arquivo de teste substitui no topo com:
 *
 * ```ts
 * vi.mock("../../packages/storage/src/blob-api", async () => {
 *   const { moduloBlobApiFalso } = await import("../helpers/fake-storage");
 *   return moduloBlobApiFalso();
 * });
 * ```
 *
 * O que continua valendo do desenho anterior: o `@sbf/storage` de verdade é
 * exercitado (montagem de caminho, tradução de erro, prefixo do bucket), e
 * quem é dublê é só a loja de blobs do outro lado.
 */
import {
  criarLojaDeBlobsFalsa,
  type LojaDeBlobsFalsa,
} from "../../packages/storage/src/test-support/fake-blob-api";

/**
 * Instância única e estável: o `vi.mock` captura a referência quando o módulo
 * é carregado pela primeira vez e não tem como trocá-la depois. Por isso a
 * limpeza entre testes é `limpar()`, e não um objeto novo.
 */
const loja: LojaDeBlobsFalsa = criarLojaDeBlobsFalsa();

/** O que o `vi.mock` devolve no lugar do módulo `blob-api`. */
export function moduloBlobApiFalso(): { vercelBlobApi: LojaDeBlobsFalsa } {
  return { vercelBlobApi: loja };
}

export interface FakeStorage {
  /** Caminhos gravados, no formato `bucket/caminho/do/objeto`. */
  chaves(): string[];
  close(): Promise<void>;
}

/**
 * Zera a loja e põe no ambiente o token que o `@sbf/storage` exige na
 * construção. O token é de mentira de propósito: nada aqui sai para a rede.
 *
 * Devolver o `close` é obrigação de quem chama (`afterAll`), para que um
 * arquivo de teste não herde os objetos gravados pelo anterior.
 */
export function startFakeStorage(): Promise<FakeStorage> {
  loja.limpar();
  process.env.BLOB_READ_WRITE_TOKEN = "vercel_blob_rw_TOKEN_DE_TESTE_SEM_REDE";

  return Promise.resolve({
    chaves: () => loja.chaves(),
    close: () => Promise.resolve(loja.limpar()),
  });
}
