//! Teste de ouro: `areas.rs` contra o que o `areas.ts` produz para os mesmos casos
//! (`tests/gerar-ouro.ts`, rodado com Bun; fixtures fictícias).

use sbf_coleta::areas::{
    Camada, Tratamento, classe_do_caminho, extensao_de, nome_financeiro, tratamento_do_arquivo,
};
use serde::Deserialize;

#[derive(Deserialize)]
struct Ouro {
    classes: Vec<ClasseOuro>,
    nomes: Vec<NomeOuro>,
    tratamentos: Vec<(String, String, String)>,
}
#[derive(Deserialize)]
struct ClasseOuro {
    caminho: String,
    camada: String,
    motivo: Option<String>,
    ignorar: bool,
    raiz: String,
}
#[derive(Deserialize)]
struct NomeOuro {
    nome: String,
    ext: String,
    financeiro: bool,
}

fn ouro() -> Ouro {
    serde_json::from_str(include_str!("fixtures/areas-ts.json")).expect("ouro")
}

#[test]
fn classes_batem_com_o_typescript() {
    let o = ouro();
    assert!(o.classes.len() > 1000);
    for c in &o.classes {
        let k = classe_do_caminho(&c.caminho);
        let camada = match k.camada() {
            Camada::A => "A",
            Camada::B => "B",
            Camada::C => "C",
        };
        assert_eq!(camada, c.camada, "camada de {:?}", c.caminho);
        assert_eq!(
            k.regra.motivo.map(str::to_owned),
            c.motivo,
            "motivo de {:?}",
            c.caminho
        );
        assert_eq!(k.regra.ignorar, c.ignorar, "ignorar de {:?}", c.caminho);
        assert_eq!(k.raiz, c.raiz, "raiz de {:?}", c.caminho);
    }
}

#[test]
fn nomes_batem_com_o_typescript() {
    for n in &ouro().nomes {
        assert_eq!(extensao_de(&n.nome), n.ext, "extensao de {:?}", n.nome);
        assert_eq!(
            nome_financeiro(&n.nome),
            n.financeiro,
            "financeiro {:?}",
            n.nome
        );
    }
}

#[test]
fn tratamentos_batem_com_o_typescript() {
    let o = ouro();
    assert!(o.tratamentos.len() > 500);
    for (caminho, nome, esperado) in &o.tratamentos {
        let t = tratamento_do_arquivo(nome, &classe_do_caminho(caminho));
        assert_eq!(t.nome(), esperado, "{caminho} / {nome}");
        let _: Tratamento = t;
    }
}
