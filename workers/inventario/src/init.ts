/**
 * E0: prepara o cofre e o índice. Idempotente — rodar de novo só acrescenta
 * ciclos novos e fontes novas, nunca apaga nada.
 *
 *   bun run workers/inventario/src/init.ts
 */
import { chmodSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { DIA_DE_CORTE, cicloDe, ciclosEntre, hojeIso } from "./ciclo";
import { caminhosDoCofre } from "./cofre";
import { FONTES } from "./fontes";

/** O sonho é 20 anos de histórico; a tabela de ciclos já cobre isso. */
const PRIMEIRO_CICLO = "2006-10";

function main(): void {
  const c = caminhosDoCofre();
  for (const dir of [
    c.raiz,
    c.acervo,
    c.ciclos,
    c.aClassificar,
    c.entrada,
    c.segredos,
    c.logs,
    c.tmp,
    join(c.raiz, "painel"),
    // Onde você larga o que trouxe de fora antes do scan (E2).
    ...[
      "celular",
      "sms",
      "telegram",
      "whatsapp",
      "onedrive",
      "gdrive",
      "outlook",
      "bancos/faturas",
      "bancos/emprestimos",
      "bancos/ofx",
      "bancos/beneficios",
      "lgpd",
      "contracheques",
    ].map((sub) => join(c.entrada, sub)),
  ]) {
    mkdirSync(dir, { recursive: true, mode: 0o700 });
    chmodSync(dir, 0o700);
  }

  const db = abrirBanco(c.banco);
  chmodSync(c.banco, 0o600);
  aplicarSchema(db);

  const hoje = hojeIso();
  const cicloAtual = cicloDe(hoje);
  const inserirCiclo = db.query("INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?, ?, ?)");
  const inserirFonte = db.query(
    `INSERT INTO fonte (id, tipo, descricao, raiz, escopo, motivo) VALUES (?, ?, ?, ?, ?, ?)
     ON CONFLICT (id) DO UPDATE SET descricao = excluded.descricao, raiz = excluded.raiz,
       escopo = excluded.escopo, motivo = excluded.motivo`,
  );
  const definirMeta = db.query("INSERT OR REPLACE INTO meta (chave, valor) VALUES (?, ?)");

  db.transaction(() => {
    for (const ciclo of ciclosEntre(PRIMEIRO_CICLO, cicloAtual)) {
      inserirCiclo.run(ciclo.id, ciclo.inicio, ciclo.fim);
    }
    for (const f of FONTES) {
      inserirFonte.run(f.id, f.tipo, f.descricao, f.raiz ?? null, f.escopo, f.motivo ?? null);
    }
    definirMeta.run("dia_de_corte", String(DIA_DE_CORTE));
    db.query("INSERT OR IGNORE INTO meta (chave, valor) VALUES (?, ?)").run("criado_em", hoje);
  })();

  const total = (sql: string) => (db.query(sql).get() as { n: number }).n;
  console.log(`Cofre:  ${c.raiz} (700)`);
  console.log(`Índice: ${c.banco}`);
  console.log(
    `Ciclos: ${total("SELECT count(*) n FROM ciclo")} (${PRIMEIRO_CICLO} a ${cicloAtual}, atual)`,
  );
  console.log(
    `Fontes: ${total("SELECT count(*) n FROM fonte WHERE escopo = 'dentro'")} dentro, ` +
      `${total("SELECT count(*) n FROM fonte WHERE escopo = 'fora'")} fora (com motivo)`,
  );
  db.close();
}

main();
