/**
 * E3: campos (data-chave, ciclo, valor, fornecedor) dos documentos cujo texto
 * veio do OCR. O e3-extrair só tira campos do texto que ele mesmo extrai (PDF,
 * XML, OFX, e-mail); imagem passa pelo e3-ocr e chegava aqui sem data, e por
 * isso caía em a-classificar/.
 *
 *   bun run workers/inventario/src/e3-campos-ocr.ts [--simular]
 *
 * Usa as mesmas regras do e3-extrair (extrairCampos + escolherDataChave). Sem
 * data no texto, a data fraca é a do nome do arquivo (IMG-AAAAMMDD-WA…) e, por
 * último, a do arquivo no disco. Só preenche o que está vazio.
 */
import { cicloDe, limitesDoCiclo } from "./ciclo";
import { abrirBanco, aplicarSchema } from "./banco";
import { dataDoNome } from "./data-do-nome";
import { escolherDataChave, extrairCampos, type Campos } from "./extrair";

interface Linha {
  content_hash: string;
  document_type: string | null;
  conteudo: string;
}

const simular = process.argv.includes("--simular");
const db = abrirBanco();
aplicarSchema(db);

const linhas = db
  .query(
    `SELECT a.content_hash, a.document_type, t.conteudo
       FROM achado a JOIN texto t USING (content_hash)
      WHERE a.estado = 'documento' AND t.metodo = 'ocr-por'
        -- Documento novo do OCR chega sem data; valor e fornecedor saem na mesma passada.
        AND a.data_chave IS NULL`,
  )
  .all() as Linha[];
const origens = db.query(
  `SELECT filename, local_filepath, local_mtime FROM origem
    WHERE content_hash = ? ORDER BY local_mtime`,
);

const c = {
  documentos: linhas.length,
  comData: 0,
  dataDoTexto: 0,
  dataDoNome: 0,
  comValor: 0,
  comFornecedor: 0,
};
const atualizacoes: unknown[][] = [];
const ciclos = new Set<string>();

for (const l of linhas) {
  const campos: Campos = { ...extrairCampos(l.conteudo) };
  const os = origens.all(l.content_hash) as {
    filename: string | null;
    local_filepath: string | null;
    local_mtime: string | null;
  }[];
  const doNome =
    os.map((o) => dataDoNome(o.filename ?? o.local_filepath ?? "")).find(Boolean) ?? null;
  campos.arquivo = doNome ?? os.find((o) => o.local_mtime)?.local_mtime?.slice(0, 10) ?? null;
  const data = escolherDataChave(campos, l.document_type);
  if (data) {
    c.comData++;
    if (data.tipo === "arquivo") c.dataDoNome += doNome ? 1 : 0;
    else c.dataDoTexto++;
    ciclos.add(cicloDe(data.data));
  }
  if (campos.valorCentavos) c.comValor++;
  if (campos.fornecedor) c.comFornecedor++;
  atualizacoes.push([
    data?.data ?? null,
    data?.tipo ?? null,
    data ? cicloDe(data.data) : null,
    campos.valorCentavos ?? null,
    campos.fornecedor ?? null,
    l.content_hash,
  ]);
}

if (!simular) {
  const ciclo = db.query("INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?, ?, ?)");
  const upd = db.query(
    `UPDATE achado SET
       data_chave = COALESCE(data_chave, ?), data_chave_tipo = COALESCE(data_chave_tipo, ?),
       ciclo_id = COALESCE(ciclo_id, ?),
       valor_centavos = COALESCE(valor_centavos, ?),
       supplier_name_raw = COALESCE(supplier_name_raw, ?)
     WHERE content_hash = ?`,
  );
  db.transaction(() => {
    for (const id of ciclos) {
      const lim = limitesDoCiclo(id);
      ciclo.run(lim.id, lim.inicio, lim.fim);
    }
    for (const a of atualizacoes)
      upd.run(...(a as [string, string, string, number, string, string]));
  }).immediate();
}

console.log(`${simular ? "[simulação] " : ""}${JSON.stringify(c)}`);
db.close();
