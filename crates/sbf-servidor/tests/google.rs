//! A verificação do `id_token` do pareamento, contra um JWKS falso (sem rede, sem banco).

mod comum;

use comum::*;
use sbf_servidor::google::ErroDoGoogle;
use serde_json::json;

#[tokio::test]
async fn login_valido_devolve_a_identidade_com_email_em_minusculas() {
    let token = id_token("Pessoa.Ficticia@Example.com", "sub-1", "n1");
    let id = verificador().verificar(&token, "n1").await.unwrap();
    assert_eq!(id.email, "pessoa.ficticia@example.com");
    assert_eq!(id.sub, "sub-1");
}

#[tokio::test]
async fn nonce_diferente_ou_vazio_e_recusado() {
    let token = id_token("a@example.com", "sub-1", "n1");
    assert_eq!(
        verificador().verificar(&token, "outro").await,
        Err(ErroDoGoogle::NonceDiferente)
    );
    let mut r = reivindicacoes("a@example.com", "sub-1", "");
    r["nonce"] = json!("");
    let token = assinar_com(google(), KID, &r);
    assert_eq!(
        verificador().verificar(&token, "").await,
        Err(ErroDoGoogle::NonceDiferente)
    );
}

#[tokio::test]
async fn email_nao_verificado_e_recusado() {
    let mut r = reivindicacoes("a@example.com", "sub-1", "n1");
    r["email_verified"] = json!(false);
    let token = assinar_com(google(), KID, &r);
    assert_eq!(
        verificador().verificar(&token, "n1").await,
        Err(ErroDoGoogle::EmailNaoVerificado)
    );
    r["email_verified"] = json!("true");
    let token = assinar_com(google(), KID, &r);
    assert!(verificador().verificar(&token, "n1").await.is_ok());
}

#[tokio::test]
async fn audiencia_emissor_e_validade_sao_conferidos() {
    for (campo, valor) in [
        ("aud", json!("outro-cliente.apps.googleusercontent.com")),
        ("iss", json!("https://impostor.example.com")),
        ("exp", json!(agora() - 3600)),
    ] {
        let mut r = reivindicacoes("a@example.com", "sub-1", "n1");
        r[campo] = valor;
        let token = assinar_com(google(), KID, &r);
        assert!(
            matches!(
                verificador().verificar(&token, "n1").await,
                Err(ErroDoGoogle::TokenInvalido(_))
            ),
            "{campo} deveria ser recusado"
        );
    }
}

#[tokio::test]
async fn assinatura_de_outra_chave_e_recusada() {
    let token = assinar_com(
        google_impostor(),
        KID,
        &reivindicacoes("a@example.com", "sub-1", "n1"),
    );
    assert!(matches!(
        verificador().verificar(&token, "n1").await,
        Err(ErroDoGoogle::TokenInvalido(_))
    ));
}

#[tokio::test]
async fn kid_desconhecida_e_algoritmo_simetrico_sao_recusados() {
    let token = assinar_com(
        google(),
        "outra-kid",
        &reivindicacoes("a@example.com", "sub-1", "n1"),
    );
    assert!(matches!(
        verificador().verificar(&token, "n1").await,
        Err(ErroDoGoogle::TokenInvalido(_))
    ));
    // HS256 com um segredo qualquer: o verificador só aceita RS256.
    let mut cabecalho = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::HS256);
    cabecalho.kid = Some(KID.into());
    let token = jsonwebtoken::encode(
        &cabecalho,
        &reivindicacoes("a@example.com", "sub-1", "n1"),
        &jsonwebtoken::EncodingKey::from_secret(b"segredo-de-teste"),
    )
    .unwrap();
    assert!(matches!(
        verificador().verificar(&token, "n1").await,
        Err(ErroDoGoogle::TokenInvalido(_))
    ));
    assert!(matches!(
        verificador().verificar("nao.e.jwt", "n1").await,
        Err(ErroDoGoogle::TokenInvalido(_))
    ));
}
