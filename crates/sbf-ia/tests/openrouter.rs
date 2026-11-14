//! O provedor `OpenRouter` contra um servidor HTTP falso em loopback. Nenhuma chamada real.

mod apoio;

use bancada_rede::Segredo;
use bancada_rede::testes::{RespostaFalsa, ServidorFalso};
use sbf_ia::{ConfiguracaoOpenRouter, Degrau, ErroDeIa, OpenRouter, Pedido, Provedor};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

const CAMINHO: &str = "/api/v1/chat/completions";
const CHAVE: &str = "sk-or-chave-ficticia-123";

fn pedido(degrau: Degrau) -> Pedido {
    Pedido {
        prompt_versao: "v1".into(),
        instrucao: "classifique".into(),
        entrada: "entrada ficticia".into(),
        esquema: json!({"type":"object","required":["ok"],"properties":{"ok":{"type":"boolean"}}}),
        degrau,
        max_tokens_saida: 99,
    }
}

fn completion(conteudo: &str, custo: f64) -> String {
    json!({
        "model": "anthropic/claude-haiku-4.5",
        "choices": [{"message": {"role": "assistant", "content": conteudo}}],
        "usage": {"prompt_tokens": 40, "completion_tokens": 6, "cost": custo}
    })
    .to_string()
}

fn provedor(
    servidor: &ServidorFalso,
    ajusta: impl FnOnce(&mut ConfiguracaoOpenRouter),
) -> OpenRouter {
    let mut c = ConfiguracaoOpenRouter {
        base_de_teste: Some(servidor.base()),
        ..ConfiguracaoOpenRouter::default()
    };
    ajusta(&mut c);
    OpenRouter::novo(Arc::new(|| Some(Segredo::novo(CHAVE))), c).expect("provedor")
}

#[test]
fn chamada_boa_traz_modelo_tokens_e_custo_real() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::json(&completion("```json\n{\"ok\":true}\n```", 0.0123)),
    );
    let p = provedor(&s, |_| {});
    let r = p.pedir(&pedido(Degrau::Rapido)).expect("resposta");
    assert_eq!(r.json, json!({"ok": true}));
    assert_eq!(r.consumo.tokens_entrada, 40);
    assert_eq!(r.consumo.tokens_saida, 6);
    assert_eq!(r.consumo.custo_dezmilesimos_usd, 123);
    assert!(!r.consumo.nominal);
    assert_eq!(p.gasto(), (123, 123));
    assert_eq!(p.nome(), "openrouter");

    let recebidos = s.pedidos();
    assert_eq!(recebidos.len(), 1);
    assert_eq!(
        recebidos[0].cabecalho("authorization"),
        Some(format!("Bearer {CHAVE}").as_str())
    );
    let corpo: Value = serde_json::from_slice(&recebidos[0].corpo).expect("json");
    assert_eq!(corpo["model"], "anthropic/claude-haiku-4.5");
    assert_eq!(corpo["max_tokens"], 99);
    assert_eq!(corpo["messages"][1]["content"], "entrada ficticia");
}

#[test]
fn o_degrau_escolhe_o_modelo() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::json(&completion("{\"ok\":true}", 0.0)),
    );
    let p = provedor(&s, |c| c.modelo_forte = "fornecedor/forte".into());
    p.pedir(&pedido(Degrau::Forte)).expect("resposta");
    let corpo: Value = serde_json::from_slice(&s.pedidos()[0].corpo).expect("json");
    assert_eq!(corpo["model"], "fornecedor/forte");
}

#[test]
fn sem_chave_nada_sai() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::json(&completion("{\"ok\":true}", 0.0)),
    );
    let c = ConfiguracaoOpenRouter {
        base_de_teste: Some(s.base()),
        ..ConfiguracaoOpenRouter::default()
    };
    let p = OpenRouter::novo(Arc::new(|| None), c).expect("provedor");
    assert!(matches!(
        p.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Indisponivel(_))
    ));
    assert!(s.pedidos().is_empty());
}

#[test]
fn a_chave_vem_da_fonte_a_cada_pedido() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::json(&completion("{\"ok\":true}", 0.0)),
    );
    let atual = Arc::new(Mutex::new("chave-um".to_owned()));
    let leitura = Arc::clone(&atual);
    let c = ConfiguracaoOpenRouter {
        base_de_teste: Some(s.base()),
        ..ConfiguracaoOpenRouter::default()
    };
    let p = OpenRouter::novo(
        Arc::new(move || Some(Segredo::novo(leitura.lock().expect("trava").clone()))),
        c,
    )
    .expect("provedor");
    p.pedir(&pedido(Degrau::Rapido)).expect("um");
    *atual.lock().expect("trava") = "chave-dois".to_owned();
    p.pedir(&pedido(Degrau::Rapido)).expect("dois");
    let cab: Vec<_> = s
        .pedidos()
        .iter()
        .map(|p| p.cabecalho("authorization").unwrap_or_default().to_owned())
        .collect();
    assert_eq!(cab, ["Bearer chave-um", "Bearer chave-dois"]);
}

#[test]
fn teto_por_rodada_bloqueia_e_nova_rodada_libera() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::json(&completion("{\"ok\":true}", 0.0123)),
    );
    let p = provedor(&s, |c| c.teto_rodada_dezmilesimos = 100);
    assert!(p.dentro_do_teto());
    p.pedir(&pedido(Degrau::Rapido)).expect("primeira"); // gasta 123 >= 100
    assert!(!p.dentro_do_teto());
    assert!(matches!(
        p.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Teto)
    ));
    assert_eq!(s.pedidos().len(), 1, "a segunda nem saiu");
    p.nova_rodada();
    assert!(p.dentro_do_teto());
    p.pedir(&pedido(Degrau::Rapido)).expect("nova rodada");
    assert_eq!(s.pedidos().len(), 2);
}

#[test]
fn teto_do_mes_vale_entre_rodadas_e_zera_na_virada() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::json(&completion("{\"ok\":true}", 0.0001)),
    );
    let mes = Arc::new(Mutex::new("2026-11".to_owned()));
    let lido = Arc::clone(&mes);
    let p = provedor(&s, |c| c.teto_mes_dezmilesimos = 1000)
        .com_mes_atual(move || lido.lock().expect("trava").clone());
    p.definir_gasto_do_mes(1000);
    p.nova_rodada();
    assert!(matches!(
        p.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Teto)
    ));
    assert!(!p.dentro_do_teto());
    *mes.lock().expect("trava") = "2026-12".to_owned();
    assert!(p.dentro_do_teto());
    p.pedir(&pedido(Degrau::Rapido)).expect("mês novo");
    assert_eq!(p.gasto(), (1, 1));
}

#[test]
fn teto_zero_bloqueia_tudo() {
    let s = ServidorFalso::iniciar();
    let p = provedor(&s, |c| c.teto_mes_dezmilesimos = 0);
    assert!(matches!(
        p.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Teto)
    ));
    assert!(s.pedidos().is_empty());
}

#[test]
fn http_429_vira_limite_com_a_espera_do_servidor() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::status(429).cabecalho("retry-after", "30"),
    );
    let p = provedor(&s, |_| {});
    match p.pedir(&pedido(Degrau::Rapido)) {
        Err(ErroDeIa::Limite { espera_segundos }) => {
            assert!((25..=31).contains(&espera_segundos), "{espera_segundos}");
        }
        outro => panic!("{outro:?}"),
    }
}

#[test]
fn http_402_creditos_acabaram_vira_teto() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::erro_json(402, r#"{"error":{"message":"sem creditos"}}"#),
    );
    let p = provedor(&s, |_| {});
    assert!(matches!(
        p.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Teto)
    ));
}

#[test]
fn http_401_e_indisponivel_e_500_e_outro_sem_corpo_na_mensagem() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::erro_json(401, r#"{"error":"entrada ficticia"}"#),
    );
    let p = provedor(&s, |_| {});
    assert!(matches!(
        p.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::Indisponivel(_))
    ));

    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::erro_json(500, r#"{"eco":"entrada ficticia"}"#),
    );
    let p = provedor(&s, |_| {});
    let erro = p.pedir(&pedido(Degrau::Rapido)).unwrap_err();
    assert!(matches!(erro, ErroDeIa::Outro(_)));
    assert!(!erro.to_string().contains("ficticia"), "{erro}");
}

#[test]
fn resposta_fora_do_esquema_e_invalida_mas_o_gasto_conta() {
    let s = ServidorFalso::iniciar();
    s.rota(
        CAMINHO,
        RespostaFalsa::json(&completion("{\"ok\":\"talvez\"}", 0.0050)),
    );
    let p = provedor(&s, |_| {});
    assert!(matches!(
        p.pedir(&pedido(Degrau::Rapido)),
        Err(ErroDeIa::RespostaInvalida(_))
    ));
    assert_eq!(p.gasto(), (50, 50));
}

#[test]
fn a_chave_nao_aparece_no_debug() {
    let s = ServidorFalso::iniciar();
    let p = provedor(&s, |_| {});
    assert!(!format!("{p:?}").contains(CHAVE));
}
