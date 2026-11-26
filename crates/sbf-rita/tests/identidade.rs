//! O pré-teste conta quantos documentos parecidos do cofre seriam repetidos pela identidade
//! do padrão (o mesmo documento por outro canal). Dados fictícios, sem rede.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;

use sbf_regras::catalogo::{Catalogo, NovaVersao};
use sbf_regras::definicao::{Definicao, Extracao, FonteDaExtracao, Formato};
use sbf_regras::{Autor, Entrada};
use sbf_rita::cofre::FonteDeSimilares;
use sbf_rita::pre_teste::pre_testar;
use sbf_rita::{Contexto, DadosDaTela, entrada_de_documento};

const RECIBO_A: &str = "Recibo Exemplo\nRecibo n 4001\nValor R$ 10,00\n";
const RECIBO_A_DE_NOVO: &str = "Recibo Exemplo (via telegram)\nRecibo n 4001 \nValor R$ 10,00\n";
const RECIBO_B: &str = "Recibo Exemplo\nRecibo n 4002\nValor R$ 10,00\n";

struct Parecidos(Vec<Entrada>);

impl FonteDeSimilares for Parecidos {
    fn similares(&self, _item: &Entrada, maximo: usize) -> Vec<Entrada> {
        self.0.iter().take(maximo).cloned().collect()
    }
}

fn doc(sha: &str, texto: &str) -> Entrada {
    entrada_de_documento(sha, Some("recibo.txt"), Some("text/plain"), Some(texto))
}

fn rascunho(identidade: &[&str]) -> NovaVersao {
    let mut d = Definicao::simples("recibo-exemplo", "recibo exemplo");
    d.extrair.insert(
        "numero_do_recibo".into(),
        Extracao {
            regex: r"Recibo n (\d+)".into(),
            grupo: 1,
            formato: Formato::Texto,
            fonte: FonteDaExtracao::Texto,
        },
    );
    d.identidade = identidade.iter().map(|s| (*s).to_owned()).collect();
    NovaVersao {
        regra: "recibo-exemplo".into(),
        nome: "Recibo exemplo".into(),
        definicao: d,
        script: None,
        autor: Autor::Usuario,
        motivo: "teste".into(),
    }
}

fn pre(identidade: &[&str]) -> sbf_rita::pre_teste::PreTeste {
    let raiz = tempfile::tempdir().unwrap();
    let mut catalogo = Catalogo::abrir(&raiz.path().join("regras.sqlite")).unwrap();
    let v = catalogo.salvar_rascunho(rascunho(identidade)).unwrap();
    let item = doc(&"aa".repeat(32), RECIBO_A);
    let ctx = Contexto::novo(
        DadosDaTela::de_documento(raiz.path(), item),
        Arc::new(sbf_script::ExecutorNoProcesso::novo()),
    )
    .com_similares(Arc::new(Parecidos(vec![
        doc(&"01".repeat(32), RECIBO_A),
        doc(&"02".repeat(32), RECIBO_A_DE_NOVO),
        doc(&"03".repeat(32), RECIBO_B),
    ])));
    pre_testar(&ctx, &catalogo, &v.regra, v.versao).unwrap()
}

#[test]
fn o_pre_teste_conta_os_repetidos_pela_identidade() {
    let p = pre(&["numero_do_recibo"]);
    assert_eq!(p.amostra.casam, 3);
    assert_eq!(p.amostra.repetidas, 1, "o 4001 aparece duas vezes");
    assert!(p.linhas().iter().any(|l| l.contains("seriam repetidos")));
    assert_eq!(
        p.para_o_modelo_json()["cofre"]["repetidas_pela_identidade"],
        1
    );
}

#[test]
fn sem_identidade_nao_ha_o_que_mostrar() {
    let p = pre(&[]);
    assert_eq!(p.amostra.repetidas, 0);
    assert!(p.linhas().iter().all(|l| !l.contains("repetidos")));
}
