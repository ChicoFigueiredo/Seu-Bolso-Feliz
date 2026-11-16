/**
 * Gera a vista navegável do cofre a partir do índice (decisão da E1):
 *
 *   ciclos/2026-10/faturas/2026-10-25_nubank_1234,56_ab12cd34.pdf
 *   a-classificar/comprovantes/<nome>_ab12cd34.pdf   ← documento sem data-chave
 *
 * Cada arquivo é um hardlink para a única cópia em acervo/: zero espaço extra,
 * e o conteúdo é o mesmo byte a byte. As duas pastas são vista e são refeitas
 * inteiras a cada execução — não guarde nada à mão dentro delas.
 *
 *   bun run workers/inventario/src/vista-ciclos.ts
 */
import { existsSync, linkSync, mkdirSync, rmSync } from "node:fs";
import { join } from "node:path";
import { abrirBanco, aplicarSchema } from "./banco";
import { caminhosDoCofre } from "./cofre";
import { nomeNaVista } from "./vista-nome";

const PASTA_DO_TIPO: Record<string, string> = {
  fatura: "faturas",
  boleto: "boletos",
  comprovante: "comprovantes",
  extrato: "extratos",
  nota_fiscal: "notas-fiscais",
  nfe: "notas-fiscais",
  contracheque: "contracheques",
  informe_rendimentos: "informes-de-rendimentos",
  irpf: "irpf",
  contrato: "contratos",
  recibo: "recibos",
  pedido: "pedidos",
  cobranca: "cobrancas",
};

function main(): void {
  const cofre = caminhosDoCofre();
  const db = abrirBanco(cofre.banco);
  aplicarSchema(db);

  for (const vista of [cofre.ciclos, cofre.aClassificar]) {
    rmSync(vista, { recursive: true, force: true });
    mkdirSync(vista, { recursive: true, mode: 0o700 });
  }

  const docs = db
    .query(
      `SELECT a.content_hash, a.extensao, a.document_type, a.data_chave, a.ciclo_id,
              a.supplier_name_raw, a.valor_centavos, a.caminho_acervo,
              (SELECT filename FROM origem o WHERE o.content_hash = a.content_hash LIMIT 1) AS nome_original
         FROM achado a
        WHERE a.estado = 'documento' AND a.caminho_acervo IS NOT NULL`,
    )
    .all() as Array<{
    content_hash: string;
    extensao: string | null;
    document_type: string | null;
    data_chave: string | null;
    ciclo_id: string | null;
    supplier_name_raw: string | null;
    valor_centavos: number | null;
    caminho_acervo: string;
    nome_original: string | null;
  }>;

  const c = { ligados: 0, aClassificar: 0, semCopia: 0 };
  for (const d of docs) {
    const original = join(cofre.raiz, d.caminho_acervo);
    if (!existsSync(original)) {
      c.semCopia++;
      continue;
    }
    const tipo = PASTA_DO_TIPO[d.document_type ?? ""] ?? "outros";
    const base = d.ciclo_id ? join(cofre.ciclos, d.ciclo_id, tipo) : join(cofre.aClassificar, tipo);
    mkdirSync(base, { recursive: true, mode: 0o700 });
    linkSync(original, join(base, nomeNaVista(d)));
    if (d.ciclo_id) c.ligados++;
    else c.aClassificar++;
  }
  const semCopiaNoAcervo = (
    db
      .query("SELECT count(*) n FROM achado WHERE estado = 'documento' AND caminho_acervo IS NULL")
      .get() as { n: number }
  ).n;
  console.log(
    `vista: ${c.ligados} em ciclos/, ${c.aClassificar} em a-classificar/, ` +
      `${c.semCopia + semCopiaNoAcervo} documentos sem cópia no cofre (fotos, acima de 100 MB ou faltando)`,
  );
  db.close();
}

if (import.meta.main) main();
