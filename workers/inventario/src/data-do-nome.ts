/**
 * Data embutida no nome de arquivo de celular: WhatsApp (IMG-20261104-WA0019.jpg,
 * "WhatsApp Image 2026-11-04 at 10.15.32.jpeg"), capturas de tela
 * (Screenshot_2026-11-04-10-15-32.png, Screenshot_20261104_101532.jpg) e câmera
 * (IMG_20261104_101532.jpg, PXL_20261104_101532.jpg).
 *
 * Puro: sem banco nem Bun, para o vitest. Data inválida ou fora de 1990–2100
 * vira null.
 */
import { montarData } from "./extrair";

const PADROES: RegExp[] = [
  /(?:^|[^0-9])((?:19|20)\d{2})(\d{2})(\d{2})(?:[-_ ]|wa|$)/i,
  /(?:^|[^0-9])((?:19|20)\d{2})-(\d{2})-(\d{2})(?:[^0-9]|$)/,
];

export function dataDoNome(nome: string): string | null {
  const base = nome.split("/").pop() ?? nome;
  for (const re of PADROES) {
    const m = re.exec(base);
    if (!m) continue;
    const data = montarData(Number(m[1]), Number(m[2]), Number(m[3]));
    if (data && data >= "1990-01-01" && data <= "2100-12-31") return data;
  }
  return null;
}
