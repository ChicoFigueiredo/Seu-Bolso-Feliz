/**
 * Parsing mínimo do XML que a API S3 devolve (ListObjectsV2 e corpo de erro).
 * Sem dependência de biblioteca XML de propósito geral — de caso pensado,
 * já que o pedido original foi manter o pacote leve (aws4fetch em vez de
 * @aws-sdk/client-s3). Cobre só as tags que o S3/SeaweedFS realmente emite
 * nessas duas respostas; não é um parser XML genérico.
 */
import type { StorageObjectInfo } from "./types";

const XML_ENTITIES: Record<string, string> = {
  "&amp;": "&",
  "&lt;": "<",
  "&gt;": ">",
  "&quot;": '"',
  "&apos;": "'",
  "&#39;": "'",
};

export function decodeXmlEntities(value: string): string {
  return value.replace(
    /&(?:amp|lt|gt|quot|apos|#39);/g,
    (entity) => XML_ENTITIES[entity] ?? entity,
  );
}

function extractTag(xml: string, tag: string): string | undefined {
  const match = xml.match(new RegExp(`<${tag}>([\\s\\S]*?)</${tag}>`));
  const content = match?.[1];
  return content === undefined ? undefined : decodeXmlEntities(content);
}

function extractBlocks(xml: string, tag: string): string[] {
  const re = new RegExp(`<${tag}>([\\s\\S]*?)</${tag}>`, "g");
  return [...xml.matchAll(re)].map((match) => match[1] ?? "");
}

export interface ParsedListObjectsXml {
  objects: StorageObjectInfo[];
  isTruncated: boolean;
  nextContinuationToken?: string;
}

/** Espera o corpo de uma resposta `GET /{bucket}/?list-type=2`. */
export function parseListObjectsXml(xml: string): ParsedListObjectsXml {
  const objects = extractBlocks(xml, "Contents").map((block) => ({
    key: extractTag(block, "Key") ?? "",
    size: Number(extractTag(block, "Size") ?? "0"),
    lastModified: extractTag(block, "LastModified") ?? "",
  }));

  return {
    objects,
    isTruncated: extractTag(xml, "IsTruncated") === "true",
    nextContinuationToken: extractTag(xml, "NextContinuationToken"),
  };
}

export interface ParsedS3Error {
  code?: string;
  message?: string;
}

/** Espera o corpo `<Error><Code>...</Code><Message>...</Message></Error>` do S3. */
export function parseS3ErrorXml(xml: string): ParsedS3Error {
  return {
    code: extractTag(xml, "Code"),
    message: extractTag(xml, "Message"),
  };
}
