//! O `canonical_fingerprint` do Rust bate com o do TS (`computeCanonicalFingerprint`), sobre
//! saídas geradas pelo próprio TS (`fixtures/gerar-impressao-canonica.ts`), inclusive nas
//! bordas em que `toLowerCase`, `\s` e `trim` do JS divergem do Rust.

#[derive(serde::Deserialize)]
struct Caso {
    texto: String,
    impressao: String,
}

#[test]
fn bate_com_o_ts() {
    let casos: Vec<Caso> =
        serde_json::from_str(include_str!("fixtures/impressao-canonica.json")).expect("fixture");
    assert!(casos.len() >= 10);
    for caso in casos {
        assert_eq!(
            sbf_pdf::impressao_canonica(&caso.texto),
            caso.impressao,
            "{:?}",
            caso.texto
        );
    }
}
