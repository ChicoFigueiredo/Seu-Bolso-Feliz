/**
 * A pasta `SBF/` do Drive é do próprio Seu Bolso Feliz (estante e cópia de segurança,
 * spec 06). A coleta nunca a lê de volta como se fosse material novo: seria o inventário
 * reinventariando os próprios documentos, e a cópia de segurança é cifrada.
 */
export const PASTA_DO_SBF = "SBF";

/** `true` se o caminho (relativo à raiz do remoto) está dentro de `SBF/` ou é a própria pasta. */
export function dentroDaPastaDoSbf(caminho: string): boolean {
  const normal = caminho.replace(/\\/g, "/").replace(/^\/+/, "");
  return normal === PASTA_DO_SBF || normal.startsWith(`${PASTA_DO_SBF}/`);
}
