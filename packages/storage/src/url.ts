/**
 * Montagem de URLs de objeto/listagem em path-style
 * (`{endpoint}/{bucket}/{chave}`), que é o que o SeaweedFS exige — ver
 * docs/integrations/storage-seaweedfs.md. Funções puras, sem I/O, para poder
 * testar a forma da URL sem subir servidor nenhum.
 */
import type { ListObjectsOptions, ResolvedStorageConfig } from "./types";

function trimTrailingSlash(value: string): string {
  return value.endsWith("/") ? value.slice(0, -1) : value;
}

/**
 * Encoda cada segmento da chave separadamente e rejunta com "/" literal, para
 * preservar a hierarquia de "pastas" (`userId/manual/arquivo.pdf`) enquanto
 * escapa corretamente espaços/acentos dentro de cada segmento (nomes de
 * arquivo reais têm "março", espaços, etc.). `encodeURIComponent` sozinho
 * escaparia a "/" da hierarquia também — daí o split/join.
 */
export function encodeObjectKey(key: string): string {
  return key
    .split("/")
    .map((segment) => encodeURIComponent(segment))
    .join("/");
}

export function buildObjectUrl(config: ResolvedStorageConfig, bucket: string, key: string): URL {
  if (!config.forcePathStyle) {
    throw new Error(
      "@sbf/storage só implementa endereçamento path-style hoje (S3_FORCE_PATH_STYLE=true). " +
        "SeaweedFS exige path-style; virtual-host (bucket.endpoint) não é suportado.",
    );
  }
  const base = trimTrailingSlash(config.endpoint);
  return new URL(`${base}/${encodeURIComponent(bucket)}/${encodeObjectKey(key)}`);
}

export function buildListUrl(
  config: ResolvedStorageConfig,
  bucket: string,
  options?: { prefix?: string } & ListObjectsOptions,
): URL {
  if (!config.forcePathStyle) {
    throw new Error(
      "@sbf/storage só implementa endereçamento path-style hoje (S3_FORCE_PATH_STYLE=true). " +
        "SeaweedFS exige path-style; virtual-host (bucket.endpoint) não é suportado.",
    );
  }
  const base = trimTrailingSlash(config.endpoint);
  const url = new URL(`${base}/${encodeURIComponent(bucket)}/`);
  url.searchParams.set("list-type", "2");
  if (options?.prefix) url.searchParams.set("prefix", options.prefix);
  if (options?.maxKeys !== undefined) url.searchParams.set("max-keys", String(options.maxKeys));
  if (options?.continuationToken) {
    url.searchParams.set("continuation-token", options.continuationToken);
  }
  return url;
}
