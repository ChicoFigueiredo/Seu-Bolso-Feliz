//! Banco de teste do núcleo: um inventário do Bun "2" com dados **fictícios**, montado
//! pelo próprio teste (INV-17). Nada aqui lê o inventário real.
#![allow(dead_code, clippy::unwrap_used, clippy::needless_range_loop)]

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use rusqlite::types::ValueRef;
use sbf_inventario::banco::Inventario;
use sbf_inventario::esquema::MIGRACAO_1_BASE_BUN;
use sbf_inventario::{Ref, Tipo};

/// Hash fictício de 64 caracteres hexadecimais.
pub fn hash(n: u8) -> String {
    format!("{n:02x}").repeat(32)
}

pub fn achado(n: u8) -> Ref {
    Ref::nova(Tipo::Achado, hash(n))
}

pub fn registro(id: i64) -> Ref {
    Ref::nova(Tipo::Registro, id.to_string())
}

pub fn fornecedor(nome: &str) -> Ref {
    Ref::nova(Tipo::Fornecedor, nome)
}

/// O que o `aplicarSchema` do Bun faz num banco novo: `schema.sql`, as colunas do
/// `garantirColuna` e `meta.schema_versao = '2'`.
pub fn aplicar_schema_do_bun(conexao: &Connection) {
    conexao.execute_batch(MIGRACAO_1_BASE_BUN).unwrap();
    for coluna in [
        "tipo_original",
        "contraparte",
        "contraparte_doc",
        "data_compra",
        "merchant_cnpj",
        "fatura_ref",
    ] {
        conexao
            .execute_batch(&format!("ALTER TABLE registro ADD COLUMN {coluna} TEXT"))
            .unwrap();
    }
    conexao
        .execute(
            "INSERT OR REPLACE INTO meta (chave, valor) VALUES ('schema_versao', '2')",
            [],
        )
        .unwrap();
}

/// Achados 1 a 4, registros 1 a 3, uma pendência do Bun e uma possível duplicidade.
pub fn semear(conexao: &Connection) {
    conexao
        .execute_batch(&format!(
            "INSERT INTO fonte (id, tipo, descricao) VALUES
               ('disco:teste', 'disco', 'disco fictício'),
               ('pluggy', 'conexao', 'conexão fictícia');
             INSERT INTO varredura (id, fonte_id, etapa, escopo, responsavel)
               VALUES (1, 'disco:teste', 'E1', '/ficticio', 'teste@1');
             INSERT INTO ciclo (id, inicio, fim) VALUES ('2026-10', '2026-10-20', '2026-11-19');
             INSERT INTO achado (content_hash, file_size_bytes, camada, estado, document_type,
                                 supplier_name_raw, data_chave, ciclo_id, valor_centavos,
                                 classificado_por)
               VALUES ('{h1}', 100, 'A', 'novo', NULL, NULL, NULL, NULL, NULL, NULL),
                      ('{h2}', 200, 'A', 'documento', 'boleto', 'Energia Exemplo',
                       '2026-10-25', '2026-10', 12345, 'regras@2'),
                      ('{h3}', 300, 'B', 'duvida', NULL, NULL, NULL, NULL, NULL, 'regras@2'),
                      ('{h4}', 400, 'A', 'documento', 'fatura', 'Banco Fictício',
                       '2026-11-01', '2026-10', 9900, 'regras@2');
             INSERT INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
                                 local_filepath, filename)
               VALUES ('{h1}', 'disco:teste', 1, 'local_file', 'local:/f/1.pdf:1', '/f/1.pdf', '1.pdf'),
                      ('{h2}', 'disco:teste', 1, 'local_file', 'local:/f/2.pdf:1', '/f/2.pdf', '2.pdf'),
                      ('{h3}', 'disco:teste', 1, 'local_file', 'local:/f/3.jpg:1', '/f/3.jpg', '3.jpg'),
                      ('{h4}', 'disco:teste', 1, 'local_file', 'local:/f/4.pdf:1', '/f/4.pdf', '4.pdf');
             INSERT INTO registro (id, content_hash, external_ref, fonte_id, tipo, data, data_tipo,
                                   ciclo_id, valor_centavos, supplier_name_raw, extraido_por, estado)
               VALUES (1, '{h2}', NULL, 'disco:teste', 'pagamento', '2026-10-25', 'vencimento',
                       '2026-10', -12345, 'Energia Exemplo', 'extrair@1', 'extraido'),
                      (2, NULL, 'pluggy:txn_ficticio_1', 'pluggy', 'compra', '2026-10-28', 'compra',
                       '2026-10', -4500, 'Mercado Fictício', 'pluggy@1', 'conferido'),
                      (3, NULL, 'pluggy:txn_ficticio_2', 'pluggy', 'credito', '2026-11-05', 'pagamento',
                       '2026-10', 100000, 'Empresa Fictícia', 'pluggy@1', 'extraido');
             INSERT INTO pendencia (id, etapa, tipo, referencia, pergunta)
               VALUES (1, 'E3', 'senha', '{h3}', 'Qual a senha deste arquivo fictício?');
             INSERT INTO relacao (a, b, tipo, confianca, motivo)
               VALUES ('{h1}', '{h4}', 'possivel_duplicidade', 0.5, 'mesmo texto');",
            h1 = hash(1),
            h2 = hash(2),
            h3 = hash(3),
            h4 = hash(4),
        ))
        .unwrap();
}

/// Cria `<pasta>/inventario.sqlite` como o Bun deixa: WAL, esquema "2", dados fictícios.
pub fn banco_do_bun(pasta: &Path) -> PathBuf {
    let caminho = pasta.join("inventario.sqlite");
    let conexao = Connection::open(&caminho).unwrap();
    conexao
        .query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))
        .unwrap();
    aplicar_schema_do_bun(&conexao);
    semear(&conexao);
    conexao.close().unwrap();
    caminho
}

/// Um banco do Bun já migrado pelo Rust (versão 2).
pub fn inventario_migrado(pasta: &Path) -> Inventario {
    let caminho = banco_do_bun(pasta);
    let mut inventario = Inventario::abrir(&caminho).unwrap();
    inventario.migrar(pasta.join("copia.sqlite")).unwrap();
    inventario
}

/// Um retrato da projeção (tudo menos o log), para comparar dois bancos.
pub fn retrato(conexao: &Connection) -> String {
    let mut saida = String::new();
    for tabela in [
        "achado",
        "registro",
        "relacao",
        "relacao_registro",
        "pendencia",
        "lote",
        "lote_item",
        "regra_senha",
        "ciclo",
    ] {
        saida.push_str(&format!("## {tabela}\n"));
        let mut consulta = conexao
            .prepare(&format!("SELECT * FROM {tabela} ORDER BY 1, 2, 3"))
            .unwrap();
        let colunas = consulta.column_count();
        let nomes: Vec<String> = consulta.column_names().map(str::to_owned).collect();
        let mut linhas = consulta.query([]).unwrap();
        while let Some(linha) = linhas.next().unwrap() {
            for i in 0..colunas {
                let v = match linha.get_ref(i).unwrap() {
                    ValueRef::Null => "∅".to_owned(),
                    ValueRef::Integer(n) => n.to_string(),
                    ValueRef::Real(r) => r.to_string(),
                    ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                    ValueRef::Blob(b) => format!("blob:{}", b.len()),
                };
                saida.push_str(&format!("{}={v} ", nomes[i]));
            }
            saida.push('\n');
        }
    }
    saida
}

/// Uma coluna de uma linha, como texto (`∅` para nulo).
pub fn valor(conexao: &Connection, sql: &str) -> String {
    conexao
        .query_row(sql, [], |l| {
            Ok(match l.get_ref(0)? {
                ValueRef::Null => "∅".to_owned(),
                ValueRef::Integer(n) => n.to_string(),
                ValueRef::Real(r) => r.to_string(),
                ValueRef::Text(t) => String::from_utf8_lossy(t).into_owned(),
                ValueRef::Blob(_) => "blob".to_owned(),
            })
        })
        .unwrap()
}
