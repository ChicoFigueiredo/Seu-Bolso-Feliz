//! A área Sugestões de conciliação sobre a fixture: pares óbvios, ambíguos, já
//! conciliados e rejeitados.

mod comum;

use comum::{H_MERCADO, Reg, inserir_registro};
use rusqlite::Connection;
use sbf_inventario::consulta::{Area, Celula, Consulta, Filtro, PedidoDeTabela, Tabela};
use sbf_inventario::leitor::LeitorSqlite;

fn extrato<'a>(data: &'a str, valor: i64) -> Reg<'a> {
    Reg {
        fonte: "conexao:pluggy",
        tipo: "debito",
        data,
        valor,
        descricao: "Compra no extrato",
        conta: Some("Banco Alfa"),
        ..Reg::default()
    }
}

fn documento<'a>(data: &'a str, valor: i64) -> Reg<'a> {
    Reg {
        hash: Some(comum::hash(H_MERCADO)),
        fonte: "gmail",
        tipo: "compra",
        data,
        valor,
        descricao: "Compra no documento",
        conta: Some("Banco Alfa"),
        ..Reg::default()
    }
}

fn sugestoes(caminho: &std::path::Path) -> Tabela {
    LeitorSqlite::abrir(caminho)
        .expect("abrir")
        .com_hoje("2026-11-13")
        .tabela(&PedidoDeTabela::Area {
            area: Area::SugestoesDeConciliacao,
            filtro: Filtro::default(),
        })
        .expect("tabela")
}

fn id_do_alvo(t: &Tabela, i: usize) -> String {
    t.linhas[i].alvo.as_ref().expect("alvo").chave.clone()
}

#[test]
fn sugere_so_o_par_obvio_e_exclui_o_resto() {
    let banco = comum::banco_de_teste();
    let conn = Connection::open(&banco.caminho).expect("conexão");
    // Par óbvio, com um dia de diferença.
    let e1 = inserir_registro(&conn, &extrato("2026-10-20", -77701));
    let d1 = inserir_registro(&conn, &documento("2026-10-21", -77701));
    // Ambíguo: um extrato, dois documentos.
    inserir_registro(&conn, &extrato("2026-10-20", -77702));
    inserir_registro(&conn, &documento("2026-10-20", -77702));
    inserir_registro(&conn, &documento("2026-10-22", -77702));
    // Valor diferente.
    inserir_registro(&conn, &extrato("2026-10-20", -77703));
    inserir_registro(&conn, &documento("2026-10-20", -77704));
    // Já conciliado.
    let e5 = inserir_registro(&conn, &extrato("2026-10-20", -77705));
    let d5 = inserir_registro(&conn, &documento("2026-10-20", -77705));
    conn.execute(
        "INSERT INTO relacao_registro (a, b, tipo, confianca, motivo) VALUES (?, ?, 'conciliacao', 1.0, 'usuario')",
        [e5, d5],
    )
    .expect("relação");
    // Rejeitado.
    let mut rejeitado = documento("2026-10-20", -77706);
    rejeitado.estado = "divergente";
    inserir_registro(&conn, &extrato("2026-10-20", -77706));
    inserir_registro(&conn, &rejeitado);
    drop(conn);

    let t = sugestoes(&banco.caminho);
    // A fixture já traz um par óbvio (energia de setembro, id 8 e 2); o par já conciliado
    // dela (-18750) não aparece. Dos casos acima, só e1 + d1 entra.
    assert_eq!(t.linhas.len(), 2, "{t:?}");
    let i = (0..2)
        .find(|i| id_do_alvo(&t, *i) == e1.to_string())
        .expect("o par obvio");
    assert_eq!(id_do_alvo(&t, 1 - i), "8");
    let Celula::Link { alvo, .. } = &t.linhas[i].celulas[1] else {
        panic!("a segunda coluna é o link do documento")
    };
    assert_eq!(alvo.chave, d1.to_string());
    let Celula::Texto(motivo) = &t.linhas[i].celulas[5] else {
        panic!("motivo")
    };
    assert_eq!(motivo, "mesmo valor, 1 dia de diferença, mesma conta");
}
