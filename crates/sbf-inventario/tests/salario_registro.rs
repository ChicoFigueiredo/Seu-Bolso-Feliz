//! Spec 12, AC-01 e AC-02: o registro `salario` do contracheque e o elo com o crédito.
#![allow(clippy::unwrap_used)]

mod nucleo_comum;

use nucleo_comum::*;
use sbf_inventario::Comandos;
use sbf_inventario::banco::Inventario;
use sbf_inventario::consulta::Consulta;
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::salario::{
    RubricaDoContracheque, SalarioDoContracheque, ligar_salario_ao_credito, registrar_salario,
};

const CREDITO: &str = "pluggy:txn_ficticio_2";

fn novo() -> (tempfile::TempDir, Inventario) {
    let pasta = tempfile::tempdir().unwrap();
    let inventario = inventario_migrado(pasta.path());
    (pasta, inventario)
}

fn rubrica(
    codigo: &str,
    valor: i64,
    desconto: bool,
    parcela: Option<u32>,
) -> RubricaDoContracheque {
    RubricaDoContracheque {
        codigo: Some(codigo.to_owned()),
        descricao: format!("Rubrica {codigo}"),
        valor_centavos: valor,
        desconto,
        parcela,
    }
}

fn salario(n: u8) -> SalarioDoContracheque {
    SalarioDoContracheque {
        sha256: hash(n),
        data: "2026-11-05".to_owned(),
        liquido_centavos: 100_000,
        rubricas: vec![
            rubrica("0001", 150_000, false, None),
            rubrica("0200", 50_000, true, Some(3)),
        ],
    }
}

#[test]
fn registrar_cria_registro_e_rubricas_uma_vez_so() {
    let (_p, inv) = novo();
    let c = inv.conexao();
    let id = registrar_salario(c, &salario(2)).unwrap();
    assert_eq!(registrar_salario(c, &salario(2)).unwrap(), id);
    let col = |x: &str| valor(c, &format!("SELECT {x} FROM registro WHERE id = {id}"));
    assert_eq!(
        col("tipo || '|' || data_tipo || '|' || valor_centavos"),
        "salario|pagamento|100000"
    );
    assert_eq!(col("extraido_por"), "contracheque@1");
    assert_eq!(col("fonte_id"), "disco:teste");
    assert_eq!(col("ciclo_id"), "2026-10");
    assert_eq!(
        valor(c, "SELECT COUNT(*) FROM registro WHERE tipo = 'salario'"),
        "1"
    );
    assert_eq!(
        valor(
            c,
            &format!(
                "SELECT group_concat(codigo || ':' || valor_centavos || ':' || IFNULL(parcela_numero, '-'), ',') FROM sub_registro WHERE registro_id = {id} AND tipo = 'rubrica'"
            )
        ),
        "0001:150000:-,0200:-50000:3"
    );
}

#[test]
fn registrar_achado_inexistente_erra() {
    let (_p, inv) = novo();
    assert!(registrar_salario(inv.conexao(), &salario(99)).is_err());
}

#[test]
fn ligar_cria_a_relacao_muda_o_tipo_e_e_idempotente() {
    let (_p, mut inv) = novo();
    let id = registrar_salario(inv.conexao(), &salario(2)).unwrap();
    assert!(ligar_salario_ao_credito(inv.conexao(), &hash(2), CREDITO).unwrap());
    assert!(ligar_salario_ao_credito(inv.conexao(), &hash(2), CREDITO).unwrap());
    let c = inv.conexao();
    assert_eq!(
        valor(
            c,
            "SELECT COUNT(*) FROM relacao_registro WHERE tipo = 'conciliacao'"
        ),
        "1"
    );
    assert_eq!(
        valor(c, "SELECT tipo FROM registro WHERE id = 3"),
        "salario"
    );
    assert_eq!(valor(c, "SELECT COUNT(*) FROM decisao"), "2");

    // Rastro do crédito mostra as decisões.
    let leitor = LeitorSqlite::abrir(inv.caminho()).unwrap();
    let rastro = leitor.rastro(&registro(3)).unwrap();
    assert!(!rastro.abaixo.is_empty());
    let _ = id;

    // Desfazer as duas decisões volta tudo.
    inv.desfazer_ultima().unwrap();
    inv.desfazer_ultima().unwrap();
    let c = inv.conexao();
    assert_eq!(
        valor(c, "SELECT tipo FROM registro WHERE id = 3"),
        "credito"
    );
    assert_eq!(valor(c, "SELECT COUNT(*) FROM relacao_registro"), "0");
}

#[test]
fn ligar_sem_um_dos_registros_devolve_false() {
    let (_p, inv) = novo();
    let c = inv.conexao();
    // Sem registro do contracheque.
    assert!(!ligar_salario_ao_credito(c, &hash(2), CREDITO).unwrap());
    registrar_salario(c, &salario(2)).unwrap();
    // Sem o crédito.
    assert!(!ligar_salario_ao_credito(c, &hash(2), "pluggy:nao_existe").unwrap());
    assert_eq!(valor(c, "SELECT COUNT(*) FROM decisao"), "0");
}
