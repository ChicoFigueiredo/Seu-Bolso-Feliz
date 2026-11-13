/**
 * A decisão do usuário prevalece (spec 06, "O banco"): os scripts da E2/E3 que
 * gravam em achado e registro não mudam o que tem decidido_por='usuario'.
 * Só bancos temporários e dados fictícios (bun:sqlite, por isso *_test.ts).
 */
import type { Database } from "bun:sqlite";
import { afterEach, describe, expect, it } from "bun:test";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { gravar, type Alvo, type Resultado } from "./e3-extrair";
import { prepararFatura, prepararGravacao } from "./gravar-pluggy";

const pastas: string[] = [];
afterEach(() => {
  for (const p of pastas.splice(0)) rmSync(p, { recursive: true, force: true });
});

const MIGRACAO_2 = readFileSync(
  new URL("../../../crates/sbf-inventario/sql/0002-decisoes-e-lotes.sql", import.meta.url),
  "utf8",
);
const hash = (n: number) => n.toString(16).padStart(2, "0").repeat(32);

function pastaTemp(): string {
  const p = mkdtempSync(join(tmpdir(), "inventario-decisao-"));
  pastas.push(p);
  return p;
}

function banco(pasta: string, migrado: boolean): Database {
  const db = abrirBanco(join(pasta, "inventario.sqlite"));
  aplicarSchema(db);
  if (migrado) {
    db.exec(MIGRACAO_2);
    db.exec("PRAGMA user_version = 2");
  }
  db.exec(`INSERT INTO fonte (id, tipo, descricao) VALUES ('conexao:pluggy', 'conexao', 'fictícia');
           INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:teste', 'disco', 'fictício');
           INSERT INTO ciclo (id, inicio, fim) VALUES ('2026-01', '2026-01-01', '2026-01-31'),
             ('2026-02', '2026-02-01', '2026-02-28'), ('2026-03', '2026-03-01', '2026-03-31');`);
  return db;
}

function rodar(script: string, pasta: string): void {
  const r = Bun.spawnSync(["bun", "run", new URL(script, import.meta.url).pathname], {
    env: { ...process.env, FINANCAS_DIR: pasta },
  });
  if (r.exitCode !== 0) throw new Error(r.stderr.toString() + r.stdout.toString());
}

function achado(db: Database, n: number, decididoPor: string | null): void {
  db.query(
    `INSERT INTO achado (content_hash, file_size_bytes, extensao, camada, estado, document_type,
       data_chave, data_chave_tipo, ciclo_id, valor_centavos, supplier_name_raw
       ${decididoPor !== null ? ", decidido_por" : ""})
     VALUES (?, 10, 'pdf', 'A', 'documento', 'boleto', '2026-01-10', 'vencimento', '2026-01',
       -1000, 'Fornecedor Corrigido' ${decididoPor !== null ? ", ?" : ""})`,
  ).run(hash(n), ...(decididoPor !== null ? [decididoPor] : []));
}

const alvo = (n: number): Alvo => ({
  content_hash: hash(n),
  extensao: "pdf",
  caminho_acervo: null,
  estado: "documento",
  document_type: "boleto",
  data_chave: "2026-01-10",
  data_chave_tipo: "vencimento",
  valor_centavos: -1000,
  supplier_name_raw: "Fornecedor Corrigido",
});

const resultadoDaExtracao = (n: number): Resultado => ({
  alvo: alvo(n),
  situacao: "texto",
  metodo: "pdftotext",
  texto: "texto fictício",
  fingerprint: "fp",
  tipoDoc: "fatura_energia" as Resultado["tipoDoc"],
  data: { data: "2026-03-05", tipo: "vencimento" },
  valorCentavos: -9999,
  fornecedor: "Outro Nome",
});

describe("e3-extrair: gravar", () => {
  it("achado do usuário: guarda data, valor e fornecedor, mas ganha o texto", () => {
    const db = banco(pastaTemp(), true);
    achado(db, 1, "usuario");
    gravar(db, resultadoDaExtracao(1));
    const a = db.query("SELECT * FROM achado WHERE content_hash = ?").get(hash(1)) as Record<
      string,
      unknown
    >;
    expect(a.data_chave).toBe("2026-01-10");
    expect(a.ciclo_id).toBe("2026-01");
    expect(a.valor_centavos).toBe(-1000);
    expect(a.supplier_name_raw).toBe("Fornecedor Corrigido");
    expect(a.document_type).toBe("boleto");
    expect(a.canonical_fingerprint).toBe("fp");
    const t = db.query("SELECT conteudo FROM texto WHERE content_hash = ?").get(hash(1)) as {
      conteudo: string;
    };
    expect(t.conteudo).toBe("texto fictício");
  });

  it("achado do usuário sem valor: o valor vazio ainda é preenchido", () => {
    const db = banco(pastaTemp(), true);
    achado(db, 1, "usuario");
    db.exec("UPDATE achado SET valor_centavos = NULL");
    gravar(db, resultadoDaExtracao(1));
    const a = db.query("SELECT valor_centavos FROM achado").get() as { valor_centavos: number };
    expect(a.valor_centavos).toBe(-9999);
  });

  it("achado das regras: continua sendo atualizado", () => {
    const db = banco(pastaTemp(), true);
    achado(db, 1, "e3-triar@1");
    gravar(db, resultadoDaExtracao(1));
    const a = db.query("SELECT * FROM achado").get() as Record<string, unknown>;
    expect(a.valor_centavos).toBe(-9999);
    expect(a.supplier_name_raw).toBe("Outro Nome");
  });

  it("banco sem decidido_por (só do Bun): grava como antes", () => {
    const db = banco(pastaTemp(), false);
    achado(db, 1, null);
    gravar(db, resultadoDaExtracao(1));
    const a = db.query("SELECT * FROM achado").get() as Record<string, unknown>;
    expect(a.valor_centavos).toBe(-9999);
    expect(a.data_chave).toBe("2026-03-05");
  });
});

function registro(db: Database, ref: string, decididoPor: string | null, descricao: string): void {
  db.query(
    `INSERT INTO registro (external_ref, fonte_id, tipo, data, data_tipo, valor_centavos,
       descricao_original, supplier_name_raw, categoria_provavel, extraido_por, estado
       ${decididoPor !== null ? ", decidido_por" : ""})
     VALUES (?, 'conexao:pluggy', 'debito', '2026-01-10', 'pagamento', -5000, ?, 'Mercado Fictício',
       'categoria-original', 'pluggy-sync', 'extraido' ${decididoPor !== null ? ", ?" : ""})`,
  ).run(ref, descricao, ...(decididoPor !== null ? [decididoPor] : []));
}

describe("e3-pluggy (script)", () => {
  it("não reclassifica registro do usuário; reclassifica o resto", () => {
    const pasta = pastaTemp();
    const db = banco(pasta, true);
    registro(db, "pluggy:a", "usuario", "PAGAMENTO DE FATURA");
    registro(db, "pluggy:b", "pluggy-sync", "PAGAMENTO DE FATURA");
    db.close();
    rodar("e3-pluggy.ts", pasta);
    const d = banco2(pasta);
    const tipo = (ref: string) =>
      (d.query("SELECT tipo FROM registro WHERE external_ref = ?").get(ref) as { tipo: string })
        .tipo;
    expect(tipo("pluggy:a")).toBe("debito");
    expect(tipo("pluggy:b")).toBe("pagamento_fatura");
  });

  it("banco sem decidido_por: reclassifica como antes", () => {
    const pasta = pastaTemp();
    const db = banco(pasta, false);
    registro(db, "pluggy:a", null, "PAGAMENTO DE FATURA");
    db.close();
    rodar("e3-pluggy.ts", pasta);
    const d = banco2(pasta);
    expect((d.query("SELECT tipo FROM registro").get() as { tipo: string }).tipo).toBe(
      "pagamento_fatura",
    );
  });
});

function banco2(pasta: string): Database {
  return abrirBanco(join(pasta, "inventario.sqlite"));
}

describe("reimportação do Pluggy", () => {
  const params = (ref: string) =>
    [
      ref,
      "debito",
      "2026-02-01",
      "pagamento",
      "2026-02",
      -7000,
      "desc",
      "Mercado",
      "nova-categoria",
      "conta",
      "aprovado",
      "mov-1",
    ] as const;

  it("importar-pluggy: estado e categoria do usuário ficam; movimentação atualiza", () => {
    const db = banco(pastaTemp(), true);
    registro(db, "pluggy:a", "usuario", "x");
    registro(db, "pluggy:b", "pluggy-sync", "x");
    const q = prepararGravacao(db);
    q.run(...params("pluggy:a"));
    q.run(...params("pluggy:b"));
    const ler = (ref: string) =>
      db
        .query(
          "SELECT estado, categoria_provavel, movimentacao_id FROM registro WHERE external_ref = ?",
        )
        .get(ref) as { estado: string; categoria_provavel: string; movimentacao_id: string };
    expect(ler("pluggy:a")).toEqual({
      estado: "extraido",
      categoria_provavel: "categoria-original",
      movimentacao_id: "mov-1",
    });
    expect(ler("pluggy:b").categoria_provavel).toBe("nova-categoria");
    expect(ler("pluggy:b").estado).toBe("aprovado");
  });

  it("importar-pluggy num banco sem decidido_por atualiza como antes", () => {
    const db = banco(pastaTemp(), false);
    registro(db, "pluggy:b", null, "x");
    prepararGravacao(db).run(...params("pluggy:b"));
    const r = db.query("SELECT categoria_provavel FROM registro").get() as {
      categoria_provavel: string;
    };
    expect(r.categoria_provavel).toBe("nova-categoria");
  });

  it("fatura: registro do usuário não é sobrescrito; os outros são", () => {
    const db = banco(pastaTemp(), true);
    for (const [ref, por] of [
      ["pluggy:fat_a", "usuario"],
      ["pluggy:fat_b", "pluggy-bruto"],
    ] as const) {
      registro(db, ref, por, "fatura");
    }
    const q = prepararFatura(db);
    for (const ref of ["pluggy:fat_a", "pluggy:fat_b"])
      q.run(ref, "2026-02-10", "2026-02", -99900, "fatura", "conta");
    const v = (ref: string) =>
      (
        db.query("SELECT valor_centavos FROM registro WHERE external_ref = ?").get(ref) as {
          valor_centavos: number;
        }
      ).valor_centavos;
    expect(v("pluggy:fat_a")).toBe(-5000);
    expect(v("pluggy:fat_b")).toBe(-99900);
  });

  it("fatura num banco sem decidido_por atualiza como antes", () => {
    const db = banco(pastaTemp(), false);
    registro(db, "pluggy:fat_b", null, "fatura");
    prepararFatura(db).run("pluggy:fat_b", "2026-02-10", "2026-02", -99900, "fatura", "conta");
    expect((db.query("SELECT valor_centavos v FROM registro").get() as { v: number }).v).toBe(
      -99900,
    );
  });
});

describe("e3-campos-ocr (script)", () => {
  it("só preenche o vazio: valor e fornecedor corrigidos pelo usuário ficam", () => {
    const pasta = pastaTemp();
    const db = banco(pasta, true);
    achado(db, 1, "usuario");
    db.exec("UPDATE achado SET data_chave = NULL, data_chave_tipo = NULL, ciclo_id = NULL");
    db.exec(`INSERT INTO texto (content_hash, metodo, conteudo)
             VALUES ('${hash(1)}', 'ocr-por', 'Vencimento 05/03/2026 Valor R$ 99,99')`);
    db.close();
    rodar("e3-campos-ocr.ts", pasta);
    const d = banco2(pasta);
    const a = d.query("SELECT data_chave, valor_centavos, supplier_name_raw FROM achado").get() as {
      data_chave: string;
      valor_centavos: number;
      supplier_name_raw: string;
    };
    expect(a.data_chave).toBe("2026-03-05"); // estava vazia: preenche
    expect(a.valor_centavos).toBe(-1000); // o valor corrigido fica
    expect(a.supplier_name_raw).toBe("Fornecedor Corrigido");
  });
});
