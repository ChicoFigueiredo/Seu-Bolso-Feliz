/**
 * Os INSERT ... ON CONFLICT do Pluggy (importar-pluggy e coletar-pluggy-bruto).
 * Ficam aqui para o teste prepará-los sem importar a API do Pluggy.
 */
import type { Database } from "bun:sqlite";
import { condicaoDoUsuario } from "./decisao";

/**
 * Reimportar não sobrescreve o que o usuário decidiu: estado e categoria ficam
 * quando o registro tem decidido_por='usuario'.
 */
export function prepararGravacao(db: Database) {
  const usuario = condicaoDoUsuario(db, "registro", "registro.");
  return db.query(
    `INSERT INTO registro (external_ref, fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos,
       descricao_original, supplier_name_raw, categoria_provavel, conta_provavel, confianca,
       extraido_por, estado, movimentacao_id)
     VALUES (?, 'conexao:pluggy', ?, ?, ?, ?, ?, ?, ?, ?, ?, 1.0, 'pluggy-sync', ?, ?)
     ON CONFLICT (external_ref) DO UPDATE SET
       estado = CASE WHEN ${usuario} THEN registro.estado ELSE excluded.estado END,
       movimentacao_id = excluded.movimentacao_id,
       categoria_provavel = CASE WHEN ${usuario} THEN registro.categoria_provavel ELSE excluded.categoria_provavel END`,
  );
}

/** Reimportar fatura não muda registro com decidido_por='usuario'. */
export function prepararFatura(db: Database) {
  return db.query(
    `INSERT INTO registro (external_ref, fonte_id, tipo, data, data_tipo, ciclo_id, valor_centavos,
       descricao_original, conta_provavel, confianca, extraido_por, estado)
     VALUES (?, 'conexao:pluggy', 'fatura_cartao', ?, 'vencimento', ?, ?, ?, ?, 1.0, 'pluggy-bruto', 'extraido')
     ON CONFLICT (external_ref) DO UPDATE SET valor_centavos = excluded.valor_centavos,
       data = excluded.data, ciclo_id = excluded.ciclo_id
       WHERE NOT ${condicaoDoUsuario(db, "registro", "registro.")}`,
  );
}
