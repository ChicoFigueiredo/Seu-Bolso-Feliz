// Gera tests/fixtures/impressao-canonica.json com o próprio computeCanonicalFingerprint de
// @sbf/operations (o mesmo cálculo do e3-extrair.ts). Rodar da raiz do repositório:
//   bun run crates/sbf-pdf/tests/fixtures/gerar-impressao-canonica.ts
import { writeFileSync } from "node:fs";
import { computeCanonicalFingerprint } from "../../../../packages/operations/src/hash";

const entradas = [
  "",
  "   ",
  "Fatura BANCO ALFA",
  "  Vencimento\t10/12/2026\r\n  Valor R$ 1.234,56  \n\n",
  "João da Conceição Fictício — Ação",
  "a  b c d e f　g h",
  "linha separada paragrafo",
  "com﻿bom﻿",
  "nel\u0085nao e espaco no js",
  "zero​width nao e espaco",
  "ΟΔΟΣ ΟΔΟΣ.",
  "İstanbul ǅ ß ẞ Ω",
  "emoji 🔑 e 𝐀",
  "\u000b\u000cvt e ff\u000b",
  "05/11/2026      MERCADO EXEMPLO LTDA                         123,45\n07/11/2026      POSTO FICTICIO     250,00\f",
];

const saida = [];
for (const texto of entradas) {
  saida.push({ texto, impressao: await computeCanonicalFingerprint(texto) });
}
writeFileSync(
  new URL("./impressao-canonica.json", import.meta.url),
  JSON.stringify(saida, null, 2) + "\n",
);
