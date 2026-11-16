//! O nome da estante é igual ao do TypeScript (INV-11): compara com saídas que o próprio
//! `vista-nome.ts` gerou (`tests/gerar-nomes.ts`, rodado com Bun).

use sbf_estante::nome::{DadosDoNome, nome_na_vista, trecho_de_nome};
use serde_json::Value;
use std::path::PathBuf;
use std::process::Command;

fn pasta() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests")
}

fn fixture() -> Value {
    let texto = std::fs::read_to_string(pasta().join("fixtures/nomes-ts.json")).expect("fixture");
    serde_json::from_str(&texto).expect("json da fixture")
}

fn texto(v: &Value) -> Option<&str> {
    v.as_str()
}

#[test]
fn trechos_iguais_aos_do_typescript() {
    let f = fixture();
    let casos = f["trechos"].as_array().expect("trechos");
    assert!(casos.len() >= 40);
    for caso in casos {
        let max = usize::try_from(caso["max"].as_u64().expect("max")).expect("cabe");
        let obtido = trecho_de_nome(texto(&caso["entrada"]), max);
        assert_eq!(
            obtido,
            caso["saida"].as_str().expect("saida"),
            "entrada {:?}",
            caso["entrada"]
        );
    }
}

#[test]
fn nomes_iguais_aos_do_typescript() {
    let f = fixture();
    let casos = f["nomes"].as_array().expect("nomes");
    assert!(casos.len() >= 10);
    for caso in casos {
        let e = &caso["entrada"];
        let dados = DadosDoNome {
            data_chave: texto(&e["data_chave"]),
            fornecedor: texto(&e["supplier_name_raw"]),
            valor_centavos: e["valor_centavos"].as_i64(),
            content_hash: texto(&e["content_hash"]).expect("hash"),
            extensao: texto(&e["extensao"]),
            nome_original: texto(&e["nome_original"]),
        };
        assert_eq!(
            nome_na_vista(&dados),
            caso["saida"].as_str().expect("saida"),
            "entrada {e}"
        );
    }
}

/// Se o Bun existe, regera a fixture e confere que o TypeScript não mudou desde que ela foi
/// gravada (se mudar, a estante renomearia tudo).
#[test]
fn fixture_ainda_e_o_que_o_typescript_gera() {
    let bun = std::env::var_os("HOME")
        .map(|h| PathBuf::from(h).join(".bun/bin/bun"))
        .filter(|p| p.exists())
        .unwrap_or_else(|| PathBuf::from("bun"));
    let Ok(saida) = Command::new(bun)
        .arg(pasta().join("gerar-nomes.ts"))
        .output()
    else {
        eprintln!("bun ausente: pulando a regeração da fixture");
        return;
    };
    if !saida.status.success() {
        eprintln!("bun falhou: pulando a regeração da fixture");
        return;
    }
    let novo: Value = serde_json::from_slice(&saida.stdout).expect("json do bun");
    assert_eq!(
        novo,
        fixture(),
        "vista-nome.ts mudou: regere tests/fixtures/nomes-ts.json e porte a mudança"
    );
}
