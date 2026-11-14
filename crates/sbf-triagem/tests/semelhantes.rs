//! Exemplos por semelhança no agente (spec 10, uso 2): decisões suas sobre itens de texto
//! parecido entram no prompt, além das do mesmo fornecedor. Embedder falso: sem modelo.

mod ajuda;

use std::sync::Arc;

use ajuda::*;
use sbf_indice::fontes::atualizar;
use sbf_indice::{Busca, BuscaLocal, Embedder, EmbedderFalso, Indice};
use sbf_inventario::comando::{Acao, Comandos, PedidoDeDecisao};
use sbf_inventario::{Ref, Tipo};
use sbf_triagem::Alvo;
use sbf_triagem::semelhantes::completar_exemplos;

fn decidir(banco: &Banco, id: i64, categoria: &str) {
    banco
        .inventario()
        .executar(&PedidoDeDecisao {
            acao: Acao::CorrigirCategoria(categoria.into()),
            alvos: vec![Ref::nova(Tipo::Registro, id.to_string())],
            motivo: None,
        })
        .expect("decidir");
}

fn registro(banco: &Banco, dia: u32, descricao: &str, fornecedor: &str) -> i64 {
    inserir_registro(
        &banco.conexao(),
        &compra(
            &format!("2026-09-{dia:02}"),
            -15000,
            descricao,
            Some(fornecedor),
        ),
    )
}

fn indexar(banco: &Banco, pasta: &std::path::Path) -> BuscaLocal {
    let e: Arc<dyn Embedder> = Arc::new(EmbedderFalso::novo());
    let caminho = pasta.join("indice.sqlite");
    let mut ind = Indice::abrir(&caminho).expect("índice");
    atualizar(&mut ind, &banco.conexao(), None, e.as_ref()).expect("atualizar");
    BuscaLocal::nova(caminho, e)
}

#[test]
fn texto_parecido_traz_decisoes_de_outro_fornecedor_e_respeita_o_teto() {
    let banco = banco_limpo();
    let pasta = tempfile::tempdir().expect("pasta");
    // Dez decisões suas sobre energia, cada uma com um fornecedor diferente do alvo.
    for n in 1..=10 {
        let id = registro(
            &banco,
            n,
            &format!("PAGAMENTO ENERGIA ELETRICA CEMIG {n}"),
            &format!("Distribuidora {n}"),
        );
        decidir(&banco, id, "Energia");
    }
    let padaria = registro(&banco, 20, "PADARIA PAO QUENTE", "Padaria Beta");
    decidir(&banco, padaria, "Padaria");
    let alvo = registro(
        &banco,
        25,
        "PAGAMENTO ENERGIA ELETRICA CEMIG novembro",
        "Cemig S.A.",
    );

    let busca = indexar(&banco, pasta.path());
    let mut mapa = sbf_triagem::entrada::Exemplos::new();
    let n = completar_exemplos(&banco.conexao(), &[Alvo::Registro(alvo)], &busca, &mut mapa)
        .expect("completar");
    let lista = &mapa[&format!("registro:{alvo}")];
    assert_eq!(n, 8, "até o teto de oito: {lista:?}");
    assert_eq!(lista.len(), 8);
    assert!(
        lista
            .iter()
            .all(|l| l.contains("ENERGIA") && l.ends_with("-> categoria Energia")),
        "{lista:?}"
    );
    assert!(!lista.iter().any(|l| l.contains("PADARIA")), "{lista:?}");
}

#[test]
fn completa_o_que_o_fornecedor_deixou_sem_repetir_e_sem_passar_do_teto() {
    let banco = banco_limpo();
    let pasta = tempfile::tempdir().expect("pasta");
    let um = registro(&banco, 1, "ENERGIA CEMIG 1", "Distribuidora A");
    decidir(&banco, um, "Energia");
    let alvo = registro(&banco, 25, "ENERGIA CEMIG novembro", "Cemig S.A.");
    let busca = indexar(&banco, pasta.path());

    let ja = "\"ENERGIA CEMIG 1\" (compra) -> categoria Energia".to_owned();
    let mut mapa = sbf_triagem::entrada::Exemplos::new();
    mapa.insert(format!("registro:{alvo}"), vec![ja.clone()]);
    completar_exemplos(&banco.conexao(), &[Alvo::Registro(alvo)], &busca, &mut mapa)
        .expect("completar");
    assert_eq!(
        mapa[&format!("registro:{alvo}")],
        vec![ja],
        "linha repetida não entra"
    );
}

#[test]
fn sem_indice_o_prompt_segue_so_com_os_exemplos_do_fornecedor() {
    let banco = banco_limpo();
    let pasta = tempfile::tempdir().expect("pasta");
    let alvo = registro(&banco, 25, "ENERGIA CEMIG novembro", "Cemig S.A.");
    let e: Arc<dyn Embedder> = Arc::new(EmbedderFalso::novo());
    let busca: &dyn Busca = &BuscaLocal::nova(pasta.path().join("nao-existe.sqlite"), e);
    let mut mapa = sbf_triagem::entrada::Exemplos::new();
    let n = completar_exemplos(&banco.conexao(), &[Alvo::Registro(alvo)], busca, &mut mapa)
        .expect("completar");
    assert_eq!(n, 0);
    assert!(mapa.is_empty());
}
