//! A conversa com ferramentas contra um servidor HTTP falso em loopback (RITA-07): o formato
//! do OpenAI (local, OpenRouter, compatível) e o adaptador da Anthropic. Nenhuma chamada real.

use bancada_rede::Segredo;
use bancada_rede::testes::{RespostaFalsa, ServidorFalso};
use sbf_ia::{
    Anthropic, ChamadaDeFerramenta, ChatCompletions, ConfiguracaoAnthropic,
    ConfiguracaoChatCompletions, DefinicaoDeFerramenta, ErroDeIa, Mensagem, NaturezaDoCusto,
    PedidoDeConversa, ProvedorDeConversa,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

const CHAVE: &str = "sk-chave-ficticia-123";

fn pedido() -> PedidoDeConversa {
    PedidoDeConversa {
        mensagens: vec![
            Mensagem::Sistema("sistema".into()),
            Mensagem::Usuario("crie o padrao".into()),
        ],
        ferramentas: vec![DefinicaoDeFerramenta {
            nome: "ver_item".into(),
            descricao: "texto do item".into(),
            parametros: json!({"type": "object", "properties": {}}),
        }],
        max_tokens_saida: 700,
    }
}

fn nao() -> AtomicBool {
    AtomicBool::new(false)
}

#[test]
fn local_sem_chave_chama_ferramenta_e_custa_zero() {
    let s = ServidorFalso::iniciar();
    s.rota(
        "/v1/chat/completions",
        RespostaFalsa::json(
            &json!({
                "model": "qwen3-coder:30b",
                "choices": [{"message": {"role": "assistant", "content": "",
                    "tool_calls": [{"id": "c1", "type": "function",
                        "function": {"name": "ver_item", "arguments": "{}"}}]}}],
                "usage": {"prompt_tokens": 120, "completion_tokens": 9}
            })
            .to_string(),
        ),
    );
    let config = ConfiguracaoChatCompletions::local(format!("{}/v1", s.base()), "qwen3-coder:30b");
    let p = ChatCompletions::novo(config, None).expect("provedor");
    let r = p.conversar(&pedido(), &nao()).expect("resposta");
    assert_eq!(r.chamadas.len(), 1);
    assert_eq!(r.chamadas[0].nome, "ver_item");
    assert_eq!(r.consumo.tokens_entrada, 120);
    assert_eq!(r.consumo.tokens_saida, 9);
    assert_eq!(r.consumo.custo_dezmilesimos_usd, 0);
    assert_eq!(p.natureza_do_custo(), NaturezaDoCusto::Zero);

    let recebidos = s.pedidos();
    assert_eq!(recebidos.len(), 1);
    assert!(recebidos[0].cabecalho("authorization").is_none());
    let corpo: Value = serde_json::from_slice(&recebidos[0].corpo).expect("json");
    assert_eq!(corpo["model"], "qwen3-coder:30b");
    assert_eq!(corpo["tools"][0]["function"]["name"], "ver_item");
    assert_eq!(corpo["max_tokens"], 700);
}

#[test]
fn compativel_com_chave_manda_bearer_e_le_o_custo_real() {
    let s = ServidorFalso::iniciar();
    s.rota(
        "/v1/chat/completions",
        RespostaFalsa::json(
            &json!({
                "choices": [{"message": {"content": "pronto"}}],
                "usage": {"prompt_tokens": 50, "completion_tokens": 5, "cost": 0.0123}
            })
            .to_string(),
        ),
    );
    let mut config = ConfiguracaoChatCompletions::openrouter("a/b");
    config.endereco = format!("{}/v1", s.base());
    let p = ChatCompletions::novo(config, Some(Arc::new(|| Some(Segredo::novo(CHAVE)))))
        .expect("provedor");
    let r = p.conversar(&pedido(), &nao()).expect("resposta");
    assert_eq!(r.texto.as_deref(), Some("pronto"));
    assert!(r.chamadas.is_empty());
    assert_eq!(r.consumo.custo_dezmilesimos_usd, 123);
    assert!(!r.consumo.nominal);
    let recebidos = s.pedidos();
    assert_eq!(
        recebidos[0].cabecalho("authorization"),
        Some(format!("Bearer {CHAVE}").as_str())
    );
    let corpo: Value = serde_json::from_slice(&recebidos[0].corpo).expect("json");
    assert_eq!(corpo["usage"]["include"], true);
}

#[test]
fn erro_do_provedor_nao_vaza_corpo_nem_chave() {
    let s = ServidorFalso::iniciar();
    s.rota(
        "/v1/chat/completions",
        RespostaFalsa::erro_json(500, "{\"erro\":\"texto do documento FICTICIO-SECRETO\"}"),
    );
    let mut config =
        ConfiguracaoChatCompletions::compativel("meu", format!("{}/v1", s.base()), "m");
    config.prazo = std::time::Duration::from_secs(10);
    let p = ChatCompletions::novo(config, Some(Arc::new(|| Some(Segredo::novo(CHAVE)))))
        .expect("provedor");
    let e = p.conversar(&pedido(), &nao()).expect_err("erro");
    let texto = format!("{e} {e:?}");
    assert!(!texto.contains("FICTICIO-SECRETO"), "{texto}");
    assert!(!texto.contains(CHAVE), "{texto}");
}

#[test]
fn endereco_http_fora_de_loopback_e_recusado() {
    let config = ConfiguracaoChatCompletions::compativel("x", "http://exemplo.com/v1", "m");
    assert!(matches!(
        ChatCompletions::novo(config, None),
        Err(ErroDeIa::Indisponivel(_))
    ));
}

#[test]
fn parar_interrompe_a_espera() {
    let s = ServidorFalso::iniciar();
    s.rota(
        "/v1/chat/completions",
        RespostaFalsa::json("{}").demorar(std::time::Duration::from_secs(5)),
    );
    let config = ConfiguracaoChatCompletions::local(format!("{}/v1", s.base()), "m");
    let p = ChatCompletions::novo(config, None).expect("provedor");
    let parar = Arc::new(AtomicBool::new(false));
    let p2 = Arc::clone(&parar);
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(200));
        p2.store(true, Ordering::Release);
    });
    let t0 = std::time::Instant::now();
    let e = p.conversar(&pedido(), &parar).expect_err("interrompido");
    assert!(matches!(e, ErroDeIa::Interrompido), "{e:?}");
    assert!(t0.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn anthropic_manda_x_api_key_e_junta_os_resultados() {
    let s = ServidorFalso::iniciar();
    s.rota(
        "/v1/messages",
        RespostaFalsa::json(
            &json!({
                "model": "claude-ficticio",
                "content": [{"type": "tool_use", "id": "t1", "name": "ver_item", "input": {}}],
                "usage": {"input_tokens": 1000, "output_tokens": 100}
            })
            .to_string(),
        ),
    );
    let mut config = ConfiguracaoAnthropic::nova("claude-ficticio");
    config.endereco = format!("{}/v1", s.base());
    let p = Anthropic::novo(config, Arc::new(|| Some(Segredo::novo(CHAVE)))).expect("provedor");
    let mut ped = pedido();
    ped.mensagens.push(Mensagem::Assistente {
        texto: None,
        chamadas: vec![ChamadaDeFerramenta {
            id: "t0".into(),
            nome: "ver_item".into(),
            argumentos: json!({}),
        }],
    });
    ped.mensagens.push(Mensagem::Ferramenta {
        id: "t0".into(),
        nome: "ver_item".into(),
        conteudo: "dados".into(),
    });
    let r = p.conversar(&ped, &nao()).expect("resposta");
    assert_eq!(r.chamadas[0].id, "t1");
    assert_eq!(r.consumo.tokens_entrada, 1000);
    assert!(r.consumo.custo_dezmilesimos_usd > 0);
    let recebidos = s.pedidos();
    assert_eq!(recebidos[0].cabecalho("x-api-key"), Some(CHAVE));
    assert_eq!(
        recebidos[0].cabecalho("anthropic-version"),
        Some("2023-06-01")
    );
    assert!(recebidos[0].cabecalho("authorization").is_none());
    let corpo: Value = serde_json::from_slice(&recebidos[0].corpo).expect("json");
    assert_eq!(corpo["system"], "sistema");
    assert_eq!(corpo["messages"][2]["content"][0]["type"], "tool_result");
}
