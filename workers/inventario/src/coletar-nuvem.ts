/**
 * E2: OneDrive e Google Drive ao vivo, via rclone somente leitura (decisão da
 * E2: "listar tudo, baixar só o que falta").
 *
 *   bun run workers/inventario/src/coletar-nuvem.ts --fonte nuvem:onedrive
 *   bun run workers/inventario/src/coletar-nuvem.ts --fonte nuvem:gdrive [--so-listar]
 *
 * 1. Lista o remoto inteiro (nada é baixado) e guarda a listagem no cofre.
 * 2. Classifica cada item pelas mesmas regras de área dos discos, como se
 *    estivesse na cópia local: o que o disco descartaria, a nuvem também.
 * 3. Documento candidato que já existe numa cópia local (mesmo caminho e
 *    tamanho) só ganha a origem da nuvem ligada ao conteúdo já indexado.
 * 4. O que falta é baixado para entrada/<pasta>/ e passa pelo scanner.
 *
 * O remoto tem escopo de leitura: nada na nuvem é criado, movido ou apagado.
 */
import type { Database } from "bun:sqlite";
import { existsSync, mkdirSync, statSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { classeDoCaminho, tratamentoDoArquivo } from "./areas";
import { abrirBanco, aplicarSchema } from "./banco";
import { hojeIso } from "./ciclo";
import { caminhosDoCofre } from "./cofre";

interface Nuvem {
  remoto: string;
  origem: string;
  pasta: string;
  /** Cópias locais antigas, a mais completa primeiro (a primeira dá a classe). */
  espelhos: string[];
}

const NUVENS: Record<string, Nuvem> = {
  "nuvem:onedrive": {
    remoto: "onedrive:",
    origem: "onedrive",
    pasta: "onedrive",
    espelhos: [
      "/mnt/p/OneDrive",
      "/mnt/p2/$Nuvem/OneDrive",
      "/mnt/bak/$Nuvem/OneDrive",
      "/mnt/e2/Pessoal/Chico/OneDrive",
      "/mnt/e2/Pessoal/Chico/@OneDrive",
    ],
  },
  "nuvem:gdrive": {
    remoto: "gdrive:",
    origem: "gdrive",
    pasta: "gdrive",
    espelhos: [
      "/mnt/p2/$Nuvem/GoogleDrive",
      "/mnt/bak/$Nuvem/GoogleDrive",
      "/mnt/e2/Pessoal/Chico/@GoogleDrive",
    ],
  },
};

interface Item {
  Path: string;
  Name: string;
  Size: number;
  ModTime: string;
  IsDir: boolean;
}

async function rodar(cmd: string[], saida?: string): Promise<void> {
  const proc = Bun.spawn(cmd, {
    stdout: saida ? Bun.file(saida) : "inherit",
    stderr: "inherit",
  });
  const codigo = await proc.exited;
  if (codigo !== 0) throw new Error(`${cmd.slice(0, 2).join(" ")} saiu com código ${codigo}`);
}

function hashLocal(db: Database, caminho: string, tamanho: number): string | null {
  if (!existsSync(caminho) || statSync(caminho).size !== tamanho) return null;
  const linha = db
    .query(
      "SELECT content_hash FROM origem WHERE local_filepath = ? AND content_hash IS NOT NULL LIMIT 1",
    )
    .get(caminho) as { content_hash: string } | null;
  return linha?.content_hash ?? null;
}

async function main(): Promise<void> {
  const argv = process.argv.slice(2);
  const fonte = argv[argv.indexOf("--fonte") + 1] ?? "";
  const nuvem = NUVENS[fonte];
  if (!nuvem) throw new Error(`--fonte deve ser ${Object.keys(NUVENS).join(" ou ")}`);
  const soListar = argv.includes("--so-listar");

  const cofre = caminhosDoCofre();
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);
  const hoje = hojeIso();
  const destino = join(cofre.entrada, nuvem.pasta);
  mkdirSync(destino, { recursive: true, mode: 0o700 });

  // 1. Listagem completa, guardada como retrato da nuvem neste dia.
  const listagem = join(cofre.entrada, `${nuvem.pasta}-lista-${hoje}.json`);
  console.log(`${fonte}: listando ${nuvem.remoto} (nada é baixado nesta fase)…`);
  await rodar(["rclone", "lsjson", "-R", "--files-only", "--fast-list", nuvem.remoto], listagem);
  const itens = (JSON.parse(await Bun.file(listagem).text()) as Item[]).filter((i) => !i.IsDir);

  const { id: varreduraId } = db
    .query(
      `INSERT INTO varredura (fonte_id, etapa, escopo, responsavel) VALUES (?, 'E2', ?, 'coletar-nuvem@1')
       RETURNING id`,
    )
    .get(fonte, nuvem.remoto) as { id: number };

  const inserirOrigem = db.query(
    `INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
       filename, file_size_bytes, local_mtime, mensagem_ref)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)`,
  );
  const c = { listados: itens.length, candidatos: 0, jaLocais: 0, aBaixar: 0, foraDeEscopo: 0 };
  const baixar: string[] = [];

  // 2 e 3. Classificar e ligar ao que já existe localmente.
  db.transaction(() => {
    for (const item of itens) {
      const virtual = join(nuvem.espelhos[0]!, item.Path);
      const classe = classeDoCaminho(dirname(virtual));
      const tratamento = tratamentoDoArquivo(item.Name, classe);
      if (classe.ignorar || (tratamento !== "copiar" && tratamento !== "hash")) {
        c.foraDeEscopo++;
        continue;
      }
      c.candidatos++;
      let hash: string | null = null;
      for (const espelho of nuvem.espelhos) {
        hash = hashLocal(db, join(espelho, item.Path), item.Size);
        if (hash) break;
      }
      if (hash) c.jaLocais++;
      else if (!existsSync(join(destino, item.Path))) {
        baixar.push(item.Path);
        c.aBaixar++;
      }
      inserirOrigem.run(
        hash,
        fonte,
        varreduraId,
        nuvem.origem,
        `${nuvem.origem}:${item.Path}:${item.ModTime}`,
        item.Name,
        item.Size,
        item.ModTime,
        `${nuvem.remoto}${item.Path}`,
      );
    }
  })();
  console.log(`${fonte}: ${JSON.stringify(c)}`);

  // 4. Baixar só o que falta e indexar.
  if (!soListar && baixar.length) {
    const lista = join(cofre.tmp, `${nuvem.pasta}-baixar-${varreduraId}.txt`);
    writeFileSync(lista, `${baixar.join("\n")}\n`, { mode: 0o600 });
    await rodar([
      "rclone",
      "copy",
      nuvem.remoto,
      destino,
      "--files-from-raw",
      lista,
      "--transfers",
      "4",
      "--checkers",
      "8",
    ]);
    await rodar([
      "bun",
      "run",
      new URL("./scan-discos.ts", import.meta.url).pathname,
      "--fonte",
      fonte,
      "--raiz",
      destino,
    ]);
    // A origem da nuvem herda o hash do arquivo baixado.
    db.query(
      `UPDATE origem SET content_hash = (
         SELECT l.content_hash FROM origem l
          WHERE l.local_filepath = ? || substr(origem.mensagem_ref, ?) AND l.content_hash IS NOT NULL)
       WHERE varredura_id = ? AND content_hash IS NULL`,
    ).run(`${destino}/`, nuvem.remoto.length + 1, varreduraId);
  }

  const semHash = (
    db
      .query("SELECT count(*) n FROM origem WHERE varredura_id = ? AND content_hash IS NULL")
      .get(varreduraId) as { n: number }
  ).n;
  db.query(
    `UPDATE varredura SET status = ?, concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
       vistos = ?, novos = ?, repetidos = ?, excluidos = ?, resumo = ? WHERE id = ?`,
  ).run(
    soListar && baixar.length ? "interrompida" : "concluida",
    c.listados,
    c.aBaixar,
    c.jaLocais,
    c.foraDeEscopo,
    JSON.stringify({
      ...c,
      semHashNoFim: semHash,
      listagem: listagem.replace(`${cofre.raiz}/`, ""),
    }),
    varreduraId,
  );
  console.log(`${fonte}: pronto; ${semHash} candidatos ainda sem conteúdo ligado`);
  db.close();
}

main().catch((e: unknown) => {
  console.error(e instanceof Error ? e.message : String(e));
  process.exit(1);
});
