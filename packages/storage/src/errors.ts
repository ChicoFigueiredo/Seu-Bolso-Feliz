/**
 * Erro tipado para qualquer falha de storage (HTTP não-2xx, corpo de erro
 * S3 inesperado, etc.). Espelha o formato de `PluggyApiError` em
 * `@sbf/financial-connectors` (message + status + detalhe cru) para manter
 * o mesmo hábito de tratamento de erro de API externa no monorepo.
 */
export class StorageError extends Error {
  constructor(
    message: string,
    /** Status HTTP da resposta, quando a falha veio de uma requisição. */
    readonly status?: number,
    /** `<Code>` do XML de erro do S3 (ex.: "NoSuchKey", "AccessDenied"). */
    readonly code?: string,
    /** Caminho do objeto envolvido, quando aplicável. */
    readonly key?: string,
  ) {
    super(message);
    this.name = "StorageError";
  }
}
