//! O rótulo da conta nas respostas do MCP: `conta_rotulo` ao lado de `conta`, e o filtro por
//! conta que aceita o rótulo, o apelido e o nome do banco. Dados fictícios.
#![allow(clippy::unwrap_used)]

#[path = "../../sbf-triagem/tests/ajuda/mod.rs"]
#[allow(unused_imports)]
mod ajuda;

use std::sync::Arc;

use ajuda::{Banco, Reg, banco_limpo, compra, inserir_registro};
use rusqlite::params;
use sbf_mcp::ferramentas::{ParamsDeRegistros, ParamsDoLivro};
use sbf_mcp::{Fabrica, Nucleo};
use sbf_planejamento::{
    Alteracao, Autor, Banco as BancoDoPlano, Planejamento, PlanejamentoSqlite, SemSaldos,
};
use serde_json::{Value, json};

const COM_PAR: &str = "BANCO EXEMPLO · 000000-1";
const POUPANCA: &str = "BANCO EXEMPLO Poupança · 000000-1";
const SEM_PAR: &str = "OUTRO BANCO · 555555-5";

struct Cenario {
    banco: Banco,
    nucleo: Nucleo,
}

/// Três registros (corrente com par, poupança vinculada, conta sem par) e um planejamento
/// real com `CX-0001` (apelido Reserva) pareado à corrente.
fn cenario(com_planejamento: bool) -> Cenario {
    let banco = banco_limpo();
    let c = banco.conexao();
    for (data, conta) in [
        ("2026-10-03", COM_PAR),
        ("2026-10-04", POUPANCA),
        ("2026-10-05", SEM_PAR),
    ] {
        inserir_registro(
            &c,
            &Reg {
                conta: Some(conta),
                ..compra(data, -1000, "COMPRA EXEMPLO", Some("loja exemplo"))
            },
        );
    }
    drop(c);
    let arquivo = banco.caminho.parent().unwrap().join("planejamento.sqlite");
    let abrir = {
        let a = arquivo.clone();
        move || PlanejamentoSqlite::novo(BancoDoPlano::abrir(&a).unwrap(), Box::new(SemSaldos))
    };
    let mut s = abrir();
    s.aplicar(
        Alteracao::MapearConta {
            codigo: "CX-0001".into(),
            conta_do_extrato: Some(COM_PAR.into()),
        },
        Autor::Usuario,
    )
    .unwrap();
    s.aplicar(
        Alteracao::DefinirApelidoDaConta {
            codigo: "CX-0001".into(),
            apelido: Some("Reserva".into()),
        },
        Autor::Usuario,
    )
    .unwrap();
    drop(s);
    let nucleo = Nucleo::novo(&banco.caminho, None).unwrap();
    let nucleo = if com_planejamento {
        let fabrica: Arc<Fabrica> =
            Arc::new(move || Ok(Box::new(abrir()) as Box<dyn Planejamento>));
        nucleo.com_planejamento(fabrica)
    } else {
        nucleo
    };
    Cenario { banco, nucleo }
}

fn contas(v: &Value) -> Vec<(String, String)> {
    let mut r: Vec<(String, String)> = v["itens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["conta"].as_str().unwrap().to_owned(),
                i["conta_rotulo"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    r.sort();
    r
}

#[test]
fn buscar_registros_traz_conta_e_conta_rotulo() {
    let c = cenario(true);
    let r = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros::default())
        .unwrap();
    let mut esperado = vec![
        (POUPANCA.to_owned(), "Poupança CX-0001 · Reserva".to_owned()),
        (COM_PAR.to_owned(), "CX-0001 · Reserva".to_owned()),
        (SEM_PAR.to_owned(), SEM_PAR.to_owned()),
    ];
    esperado.sort();
    assert_eq!(contas(&r), esperado);
}

#[test]
fn sem_planejamento_o_rotulo_e_a_propria_conta() {
    let c = cenario(false);
    let r = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros::default())
        .unwrap();
    assert!(contas(&r).iter().all(|(conta, rotulo)| conta == rotulo));
}

#[test]
fn o_filtro_de_conta_casa_rotulo_apelido_codigo_e_nome_do_banco() {
    let c = cenario(true);
    let total = |conta: &str| {
        c.nucleo
            .buscar_registros(&ParamsDeRegistros {
                conta: Some(conta.into()),
                ..Default::default()
            })
            .unwrap()["total"]
            .as_i64()
            .unwrap()
    };
    // O rótulo acha a corrente e a poupança vinculada.
    assert_eq!(total("CX-0001 · Reserva"), 2);
    assert_eq!(total("reserva"), 2);
    assert_eq!(total("cx-0001"), 2);
    // O nome do banco acha as duas do banco (sem esconder nenhuma), e a outra conta é outra.
    assert_eq!(total("banco exemplo"), 2);
    assert_eq!(total("outro banco"), 1);
    assert_eq!(total("555555"), 1);
    assert_eq!(total("nada disso"), 0);
}

#[test]
fn ver_registro_traz_conta_rotulo() {
    let c = cenario(true);
    let id: i64 = c
        .banco
        .conexao()
        .query_row(
            "SELECT id FROM registro WHERE conta_provavel = ?1",
            params![COM_PAR],
            |l| l.get(0),
        )
        .unwrap();
    let v = c.nucleo.ver_registro(id).unwrap();
    assert_eq!(v["conta"], COM_PAR);
    assert_eq!(v["conta_rotulo"], "CX-0001 · Reserva");
}

#[test]
fn livro_movimentacoes_traz_conta_rotulo_e_filtra_pelo_rotulo() {
    let c = cenario(true);
    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join("livro-caixa.sqlite");
    let conn = rusqlite::Connection::open(&caminho).unwrap();
    conn.execute_batch("CREATE TABLE linha (colecao TEXT NOT NULL, id TEXT NOT NULL, versao TEXT, dados TEXT NOT NULL, PRIMARY KEY (colecao, id));").unwrap();
    let grava = |colecao: &str, id: &str, v: Value| {
        conn.execute(
            "INSERT INTO linha (colecao, id, dados) VALUES (?1, ?2, ?3)",
            params![colecao, id, v.to_string()],
        )
        .unwrap();
    };
    grava("contas", "k1", json!({"id": "k1", "nome": COM_PAR}));
    grava("contas", "k2", json!({"id": "k2", "nome": "Carteira"}));
    for (id, conta) in [("m1", "k1"), ("m2", "k2")] {
        grava(
            "movimentacoes",
            id,
            json!({"id": id, "conta": conta, "tipo": "saida", "valor": 100, "descricao": "X", "data": "2026-10-03", "categoria": null, "confirmada": true}),
        );
    }
    drop(conn);
    let n = Nucleo::novo(&c.banco.caminho, Some(&caminho)).unwrap();
    // Este núcleo não tem planejamento: o rótulo é o nome da conta.
    let sem = n.livro_movimentacoes(&ParamsDoLivro::default()).unwrap();
    assert!(
        sem["itens"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["conta_rotulo"] == i["conta"])
    );

    let arquivo = c
        .banco
        .caminho
        .parent()
        .unwrap()
        .join("planejamento.sqlite");
    let fabrica: Arc<Fabrica> = Arc::new(move || {
        Ok(Box::new(PlanejamentoSqlite::novo(
            BancoDoPlano::abrir(&arquivo).unwrap(),
            Box::new(SemSaldos),
        )) as Box<dyn Planejamento>)
    });
    let n = n.com_planejamento(fabrica);
    let todas = n.livro_movimentacoes(&ParamsDoLivro::default()).unwrap();
    let mut rotulos: Vec<(String, String)> = todas["itens"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["conta"].as_str().unwrap().to_owned(),
                i["conta_rotulo"].as_str().unwrap().to_owned(),
            )
        })
        .collect();
    rotulos.sort();
    let mut esperado = vec![
        ("Carteira".to_owned(), "Carteira".to_owned()),
        (COM_PAR.to_owned(), "CX-0001 · Reserva".to_owned()),
    ];
    esperado.sort();
    assert_eq!(rotulos, esperado);
    let filtrada = n
        .livro_movimentacoes(&ParamsDoLivro {
            conta: Some("reserva".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(filtrada["total"], 1);
    // O filtro por nome exato continua valendo.
    let exata = n
        .livro_movimentacoes(&ParamsDoLivro {
            conta: Some("carteira".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(exata["total"], 1);
}
