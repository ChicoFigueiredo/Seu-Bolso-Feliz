/**
 * Confere o cofre contra o índice e repara o que dá para reparar:
 *
 * 1. Documento da camada A, até o limite de cópia, sem cópia no cofre (cópia
 *    interrompida, por exemplo): copia de novo de uma origem em disco e só
 *    marca depois de conferir o SHA-256 da cópia.
 * 2. Cópia marcada no índice que sumiu ou tem tamanho errado: idem.
 * 3. Arquivo no acervo que nenhum achado aponta (sobra de cópia interrompida):
 *    removido. É sempre cópia; o original nunca fica no acervo.
 *
 *   bun run workers/inventario/src/reparar-acervo.ts [--aplicar] [--documentos]
 *
 * --documentos: inclui achados confirmados como documento em qualquer camada
 * (os da camada B só tinham hash; a E3 os promoveu e eles passam a ter cópia).
 */
import {
  chmodSync,
  copyFileSync,
  existsSync,
  mkdirSync,
  readdirSync,
  renameSync,
  rmSync,
  statSync,
} from "node:fs";
import { dirname, join } from "node:path";
import { DOCUMENTOS, FOTOS, TEXTOS_PESSOAIS } from "./areas";
import { abrirBanco, aplicarSchema } from "./banco";
import { caminhoNoAcervo, caminhosDoCofre } from "./cofre";

const LIMITE = 100 * 1024 ** 2;

async function sha256(caminho: string): Promise<string> {
  const hasher = new Bun.CryptoHasher("sha256");
  const leitor = Bun.file(caminho).stream().getReader();
  for (;;) {
    const { done, value } = await leitor.read();
    if (done) break;
    hasher.update(value);
  }
  return hasher.digest("hex");
}

async function main(): Promise<void> {
  const aplicar = process.argv.includes("--aplicar");
  const documentos = process.argv.includes("--documentos");
  const cofre = caminhosDoCofre();
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);

  const copiaveis = [...DOCUMENTOS, ...TEXTOS_PESSOAIS];
  const marcas = copiaveis.map(() => "?").join(",");
  // Foto só ganha cópia quando a E3 confirma que é documento (print de
  // comprovante do WhatsApp, por exemplo); foto comum fica só com o hash.
  const fotos = [...FOTOS];
  const marcasFotos = fotos.map(() => "?").join(",");
  const candidatos = db
    .query(
      `SELECT content_hash, extensao, file_size_bytes, caminho_acervo FROM achado
        WHERE (camada = 'A' OR (? = 1 AND estado = 'documento'))
          AND file_size_bytes <= ?
          AND (extensao IN (${marcas}) OR (? = 1 AND estado = 'documento' AND extensao IN (${marcasFotos})))
          AND (notas IS NULL OR notas NOT LIKE '%cópia removida%')`,
    )
    .all(documentos ? 1 : 0, LIMITE, ...copiaveis, documentos ? 1 : 0, ...fotos) as {
    content_hash: string;
    extensao: string | null;
    file_size_bytes: number;
    caminho_acervo: string | null;
  }[];
  const origens = db.query(
    "SELECT local_filepath FROM origem WHERE content_hash = ? AND local_filepath IS NOT NULL",
  );
  const marcar = db.query("UPDATE achado SET caminho_acervo = ? WHERE content_hash = ?");

  const c = { conferidos: 0, refeitos: 0, semOrigem: 0, hashDivergente: 0, sobras: 0 };
  for (const a of candidatos) {
    c.conferidos++;
    const relativo = a.caminho_acervo ?? caminhoNoAcervo(a.content_hash, a.extensao ?? "");
    const destino = join(cofre.raiz, relativo);
    if (a.caminho_acervo && existsSync(destino) && statSync(destino).size === a.file_size_bytes) {
      continue;
    }
    const fonte = (origens.all(a.content_hash) as { local_filepath: string }[])
      .map((o) => o.local_filepath)
      .find((p) => existsSync(p) && statSync(p).size === a.file_size_bytes);
    if (!fonte) {
      c.semOrigem++;
      continue;
    }
    if (!aplicar) {
      c.refeitos++;
      continue;
    }
    const temporario = `${destino}.parcial-${process.pid}`;
    mkdirSync(dirname(destino), { recursive: true, mode: 0o700 });
    copyFileSync(fonte, temporario);
    if ((await sha256(temporario)) !== a.content_hash) {
      rmSync(temporario, { force: true });
      c.hashDivergente++;
      continue;
    }
    chmodSync(temporario, 0o600);
    renameSync(temporario, destino);
    marcar.run(relativo, a.content_hash);
    c.refeitos++;
  }

  // Sobras: arquivos do acervo que nenhum achado aponta. Os coletores gravam o
  // arquivo antes de registrá-lo (o Gmail grava e só depois insere o achado; o
  // scanner copia para .parcial-<pid> e renomeia), então só conta como sobra o
  // que não é cópia em andamento e já tem mais de uma hora.
  const umaHoraAtras = Date.now() - 60 * 60 * 1000;
  const apontados = new Set(
    (
      db.query("SELECT caminho_acervo FROM achado WHERE caminho_acervo IS NOT NULL").all() as {
        caminho_acervo: string;
      }[]
    ).map((r) => r.caminho_acervo),
  );
  for (const sub of readdirSync(cofre.acervo)) {
    for (const nome of readdirSync(join(cofre.acervo, sub))) {
      const relativo = join("acervo", sub, nome);
      if (apontados.has(relativo) || nome.includes(".parcial-")) continue;
      if (statSync(join(cofre.raiz, relativo)).mtimeMs > umaHoraAtras) continue;
      c.sobras++;
      if (aplicar) rmSync(join(cofre.raiz, relativo), { force: true });
    }
  }

  console.log(`${aplicar ? "reparo" : "simulação"}: ${JSON.stringify(c)}`);
  db.close();
}

main().catch((e: unknown) => {
  console.error(e instanceof Error ? e.message : String(e));
  process.exit(1);
});
