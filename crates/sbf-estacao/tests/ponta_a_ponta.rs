//! Ponta a ponta contra o `sbf servidor` de verdade, com o Postgres de teste em Docker.
//! Só com `SBF_PG_TESTE=1`, e com o servidor já no ar e uma estação já gravada no banco:
//!
//! - `SBF_E2E_SERVIDOR`: a base do `sbf servidor` (`http://127.0.0.1:8787`);
//! - `SBF_E2E_CREDENCIAL`: a credencial fictícia da estação semeada no banco de teste.
//!
//! O roteiro que prepara os dois está no relatório da estação (fatia 2).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use bancada_rede::Segredo;
use sbf_contrato::{Colecao, Movimentacao};
use sbf_estacao::*;
use sbf_inventario::alteracao::{
    Alteracoes, EstadoDaAlteracao, EstadoDaRede, PedidoDeAlteracao, Sincronizacao,
};
use sbf_inventario::referencia::{Ref, Tipo};
use std::sync::Arc;
use std::time::Duration;

#[test]
fn estacao_contra_o_servidor_real() {
    if std::env::var("SBF_PG_TESTE").as_deref() != Ok("1") {
        eprintln!("pulado: precisa de SBF_PG_TESTE=1, SBF_E2E_SERVIDOR e SBF_E2E_CREDENCIAL");
        return;
    }
    let base = std::env::var("SBF_E2E_SERVIDOR").expect("SBF_E2E_SERVIDOR");
    let credencial = std::env::var("SBF_E2E_CREDENCIAL").expect("SBF_E2E_CREDENCIAL");
    let host = "sbf.teste.invalid";
    let pasta = tempfile::tempdir().unwrap();
    let guarda = GuardaEmMemoria::default();
    guarda.gravar(host, Segredo::novo(credencial)).unwrap();
    let config = ConfiguracaoDaEstacao::nova(
        Servidor::novo(&format!("https://{host}")).unwrap(),
        pasta.path(),
        Arc::new(guarda.clone()),
    )
    .rota(host, &base)
    .intervalo(None);
    let mut estacao = EstacaoLocal::abrir(config).unwrap();
    assert!(estacao.esperar_troca(Duration::from_secs(60)));
    assert!(
        matches!(
            estacao.estado().rede,
            EstadoDaRede::Sincronizado | EstadoDaRede::RegrasMaisNovas { .. }
        ),
        "{:?}",
        estacao.estado().rede
    );
    let cache = estacao.cache().unwrap();
    let movs: Vec<Movimentacao> = cache.linhas(Colecao::Movimentacoes).unwrap();
    assert!(!movs.is_empty());
    // EST-06: 0,10 e 0,20 chegam como centavos inteiros.
    assert!(movs.iter().any(|m| m.valor == 10));
    assert!(movs.iter().any(|m| m.valor == 20));

    // Q6 de ponta a ponta: mudar a categoria de uma movimentação.
    let alvo = movs.iter().find(|m| m.valor == 20).unwrap();
    let outra = estacao
        .categorias()
        .unwrap()
        .into_iter()
        .find(|c| Some(c.alvo.chave.as_str()) != alvo.categoria.map(|c| c.to_string()).as_deref())
        .unwrap();
    let a = estacao
        .propor(&PedidoDeAlteracao::MudarCategoria {
            movimentacao: Ref::nova(Tipo::Movimentacao, alvo.id.to_string()),
            categoria: Some(outra.alvo.clone()),
        })
        .unwrap();
    estacao.sincronizar_agora();
    assert!(estacao.esperar_troca(Duration::from_secs(60)));
    assert_eq!(
        estacao.alteracao(&a.id).unwrap().unwrap().estado,
        EstadoDaAlteracao::Aceita
    );
    let depois: Movimentacao = estacao
        .cache()
        .unwrap()
        .linha(Colecao::Movimentacoes, &alvo.id.to_string())
        .unwrap()
        .unwrap();
    assert_eq!(
        depois.categoria.map(|c| c.to_string()),
        Some(outra.alvo.chave)
    );
}
