/**
 * Troca o valor de uma variável num arquivo .env sem tocar no resto: mantém
 * comentários, ordem e as outras linhas. Acrescenta a variável no fim se ela
 * não existir.
 */
export function trocarVariavel(texto: string, nome: string, valor: string): string {
  const linha = `${nome}=${valor}`;
  const padrao = new RegExp(`^${nome}=.*$`, "m");
  if (padrao.test(texto)) return texto.replace(padrao, () => linha);
  const final = texto.length === 0 || texto.endsWith("\n") ? "" : "\n";
  return `${texto}${final}${linha}\n`;
}
