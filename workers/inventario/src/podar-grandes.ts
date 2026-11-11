/**
 * Tira do cofre as cópias acima do limite (LIMITE_COPIA_BYTES), só quando o
 * original ainda existe num disco com o mesmo tamanho. O achado continua no
 * índice com hash e origens; só a cópia sai.
 *
 *   bun run workers/inventario/src/podar-grandes.ts [--aplicar]
 *
 * Sem --aplicar só mostra o que faria.
 */
import { existsSync, rmSync, statSync } from "node:fs";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { caminhosDoCofre } from "./cofre";

const LIMITE = 100 * 1024 ** 2;

function main(): void {
  const aplicar = process.argv.includes("--aplicar");
  const cofre = caminhosDoCofre();
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);
  const grandes = db
    .query(
      `SELECT content_hash, caminho_acervo, file_size_bytes FROM achado
        WHERE caminho_acervo IS NOT NULL AND file_size_bytes > ? AND estado <> 'documento'`,
    )
    .all(LIMITE) as { content_hash: string; caminho_acervo: string; file_size_bytes: number }[];
  const origens = db.query(
    "SELECT local_filepath FROM origem WHERE content_hash = ? AND local_filepath IS NOT NULL",
  );
  const soltar = db.query(
    `UPDATE achado SET caminho_acervo = NULL,
       notas = coalesce(notas || ' | ', '') || 'cópia removida do cofre: acima de 100 MB, original em ' || ?
     WHERE content_hash = ?`,
  );

  let removidos = 0;
  let bytes = 0;
  let mantidos = 0;
  for (const g of grandes) {
    const original = (origens.all(g.content_hash) as { local_filepath: string }[])
      .map((o) => o.local_filepath)
      .find(
        (p) =>
          !p.startsWith(`${cofre.raiz}/`) &&
          existsSync(p) &&
          statSync(p).size === g.file_size_bytes,
      );
    if (!original) {
      mantidos++;
      continue;
    }
    if (aplicar) {
      rmSync(join(cofre.raiz, g.caminho_acervo), { force: true });
      soltar.run(original, g.content_hash);
    }
    removidos++;
    bytes += g.file_size_bytes;
  }
  console.log(
    `${aplicar ? "removidas" : "removeria"} ${removidos} cópias (${(bytes / 1024 ** 3).toFixed(1)} GB); ` +
      `${mantidos} mantidas por não ter original confirmado em disco`,
  );
  db.close();
}

main();
