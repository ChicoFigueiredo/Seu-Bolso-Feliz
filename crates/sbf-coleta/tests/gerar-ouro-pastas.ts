// Gera o ouro da chave de origem e do mtime ISO que o scan-discos.ts usa.
//   bun crates/sbf-coleta/tests/gerar-ouro-pastas.ts > crates/sbf-coleta/tests/fixtures/pastas-ts.json
import { mkdtempSync, statSync, utimesSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { tmpdir } from "node:os";

const dir = mkdtempSync(join(tmpdir(), "ouro-pastas-"));
// Segundos exatos em binário (.5, .25, .125, .0): sem ruído de ponto flutuante.
const segundos = [1731412345, 1731412345.5, 1731412345.25, 1731412345.125, 1600000000.75, 1, 0.5, 2000000000.875];
const saida = segundos.map((s, i) => {
  const f = join(dir, `a${i}.txt`);
  writeFileSync(f, "x");
  utimesSync(f, s, s);
  const st = statSync(f);
  return { segundos: s, mtimeMs: `${st.mtimeMs}`, iso: st.mtime.toISOString() };
});
console.log(JSON.stringify(saida, null, 1));
