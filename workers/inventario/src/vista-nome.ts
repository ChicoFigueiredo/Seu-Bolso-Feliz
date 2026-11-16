/** Nomes de arquivo da vista por ciclo (puro, testável no vitest). */

/** Trecho seguro para nome de arquivo: sem acento, sem barra, curto. */
export function trechoDeNome(texto: string | null | undefined, max = 40): string {
  if (!texto) return "";
  return texto
    .normalize("NFD")
    .replace(/\p{Diacritic}/gu, "")
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "")
    .slice(0, max)
    .replace(/-+$/, "");
}

export function nomeNaVista(d: {
  data_chave: string | null;
  supplier_name_raw: string | null;
  valor_centavos: number | null;
  content_hash: string;
  extensao: string | null;
  nome_original: string | null;
}): string {
  const partes = [
    d.data_chave ?? "",
    trechoDeNome(d.supplier_name_raw) || trechoDeNome(d.nome_original?.replace(/\.[^.]+$/, "")),
    d.valor_centavos != null ? (Math.abs(d.valor_centavos) / 100).toFixed(2).replace(".", ",") : "",
    d.content_hash.slice(0, 8),
  ].filter(Boolean);
  return `${partes.join("_")}${d.extensao ? `.${d.extensao}` : ""}`;
}
