//! RITA-07 com o Ollama de verdade: `qwen3-coder:30b` sobre um documento **fictício**.
//! Ignorado por padrão (precisa do Ollama em `localhost:11434` com o modelo); serve para
//! calibrar o prompt e as descrições das ferramentas:
//!
//! ```text
//! cargo test -p sbf-rita --test ollama -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sbf_ia::{
    ChatCompletions, ConfiguracaoChatCompletions, ENDERECO_DO_OLLAMA, MODELO_LOCAL_PADRAO,
    RegistroFalso,
};
use sbf_regras::catalogo::Catalogo;
use sbf_rita::{Contexto, DadosDaTela, Evento, Rita, SimilaresFixos, entrada_de_documento};

const FATURA: &str = "OPERADORA EXEMPLO TELECOM S/A\nFatura de Internet Fibra\nContrato nº 4455-7788\nCliente: Fulano de Tal\nReferente a: 10/2026\nVencimento: 15/11/2026\nValor total a pagar: R$ 129,90\nCódigo de barras: 00000.00000 00000.000000 00000.000000 0 00000000000000\n";

fn fatura(mes: &str, venc: &str, valor: &str, n: u8) -> sbf_regras::Entrada {
    entrada_de_documento(
        &format!("{n:02x}").repeat(32),
        Some(&format!("fatura-{mes}.txt")),
        Some("text/plain"),
        Some(&format!(
            "OPERADORA EXEMPLO TELECOM S/A\nFatura de Internet Fibra\nContrato nº 4455-7788\nCliente: Fulano de Tal\nReferente a: {mes}\nVencimento: {venc}\nValor total a pagar: R$ {valor}\n"
        )),
    )
}

#[test]
#[ignore = "precisa do Ollama local com qwen3-coder:30b"]
fn rita_com_o_ollama_real_cria_um_rascunho_que_passa() {
    let modelo =
        std::env::var("SBF_RITA_MODELO").unwrap_or_else(|_| MODELO_LOCAL_PADRAO.to_owned());
    let endereco =
        std::env::var("SBF_RITA_ENDERECO").unwrap_or_else(|_| ENDERECO_DO_OLLAMA.to_owned());
    let d = tempfile::tempdir().expect("pasta");
    Catalogo::abrir(&d.path().join("regras.sqlite")).expect("catálogo");
    let item = entrada_de_documento(
        &"aa".repeat(32),
        Some("fatura-10-2026.txt"),
        Some("text/plain"),
        Some(FATURA),
    );
    let similares = vec![
        fatura("09/2026", "15/10/2026", "129,90", 1),
        fatura("08/2026", "15/09/2026", "119,90", 2),
        entrada_de_documento(
            &"cc".repeat(32),
            Some("energia.txt"),
            Some("text/plain"),
            Some("Energia Exemplo\nConta de luz\nVencimento: 20/11/2026\nValor: R$ 210,00\n"),
        ),
    ];
    let ctx = Contexto::novo(
        DadosDaTela::de_documento(d.path(), item),
        Arc::new(sbf_script::ExecutorNoProcesso::novo()),
    )
    .com_similares(Arc::new(SimilaresFixos(similares)));
    let provedor =
        ChatCompletions::novo(ConfiguracaoChatCompletions::local(endereco, modelo), None)
            .expect("provedor");
    let mut rita = Rita::nova(ctx);
    let parar = AtomicBool::new(false);
    let t0 = std::time::Instant::now();
    let r = rita
        .pedir(
            &provedor,
            "o título e o número do contrato determinam o padrão; a data de vencimento determina a competência e o valor total é o valor; cria",
            &parar, &RegistroFalso::novo(),
            &mut |e| match e {
                Evento::Chamada(c) => eprintln!("  [{}] {} {} -> {}", c.numero, c.nome, c.argumentos, c.resultado),
                Evento::Fala(t) => eprintln!("  Rita: {t}"),
                Evento::Consumo(_) => {}
            },
        )
        .expect("relatório");
    eprintln!(
        "parada {:?} em {:.1}s; tokens {}/{}",
        r.parada,
        t0.elapsed().as_secs_f64(),
        r.consumo.tokens_entrada,
        r.consumo.tokens_saida
    );
    eprintln!("{}", r.resposta);
    if let Some(m) = r.melhor() {
        let c = Catalogo::abrir(&d.path().join("regras.sqlite")).expect("catálogo");
        if let Ok(v) = c.versao(&m.regra, m.versao) {
            eprintln!(
                "definição gravada: {}",
                serde_json::to_string(&v.definicao).unwrap_or_default()
            );
        }
    }
    assert!(
        !r.rascunhos.is_empty(),
        "o modelo não criou nenhum rascunho"
    );
    assert!(
        r.melhor().is_some_and(|m| m.pre_teste.passou),
        "o rascunho não passou no pré-teste"
    );
}

const PRINT: &str = include_str!("fixtures/print-prestacao-habitacao-ficticio.txt");
const PEDIDO_DO_PRINT: &str = "Isto é um comprovante de prestação da habitação CAIXA. O que determina é o título (Prestação atual) e o número do contrato. A data de vencimento determina a competência. Cria o padrão.";

/// A calibração que falhou com o `qwen3-coder:30b`: um print de app bancário (texto de OCR
/// fictício). Meta: passar em até 5 chamadas, com detecção `prestação atual` + o número do
/// contrato, o contrato extraído, o valor (123456 centavos) e a competência 2026-11 (o momento
/// do item é 2026-11-20).
#[test]
#[ignore = "precisa do Ollama local com qwen3-coder:30b"]
fn rita_com_o_ollama_real_calibra_no_print_da_prestacao_da_habitacao() {
    let modelo =
        std::env::var("SBF_RITA_MODELO").unwrap_or_else(|_| MODELO_LOCAL_PADRAO.to_owned());
    let endereco =
        std::env::var("SBF_RITA_ENDERECO").unwrap_or_else(|_| ENDERECO_DO_OLLAMA.to_owned());
    let d = tempfile::tempdir().expect("pasta");
    Catalogo::abrir(&d.path().join("regras.sqlite")).expect("catálogo");
    let mut item = entrada_de_documento(
        &"bb".repeat(32),
        Some("print-prestacao.png"),
        Some("image/png"),
        Some(PRINT),
    );
    item.momento = Some("2026-11-20".to_owned());
    let ctx = Contexto::novo(
        DadosDaTela::de_documento(d.path(), item),
        Arc::new(sbf_script::ExecutorNoProcesso::novo()),
    )
    .com_similares(Arc::new(SimilaresFixos(vec![])));
    let provedor =
        ChatCompletions::novo(ConfiguracaoChatCompletions::local(endereco, modelo), None)
            .expect("provedor");
    let mut rita = Rita::nova(ctx);
    let parar = AtomicBool::new(false);
    let t0 = std::time::Instant::now();
    let r = rita
        .pedir(
            &provedor,
            PEDIDO_DO_PRINT,
            &parar,
            &RegistroFalso::novo(),
            &mut |e| match e {
                Evento::Chamada(c) => eprintln!(
                    "  [{}] {} {} -> {}",
                    c.numero, c.nome, c.argumentos, c.resultado
                ),
                Evento::Fala(t) => eprintln!("  Rita: {t}"),
                Evento::Consumo(_) => {}
            },
        )
        .expect("relatório");
    eprintln!(
        "parada {:?} em {:.1}s; {} chamadas; tokens {}/{}",
        r.parada,
        t0.elapsed().as_secs_f64(),
        r.chamadas.len(),
        r.consumo.tokens_entrada,
        r.consumo.tokens_saida
    );
    eprintln!("{}", r.resposta);
    let c = Catalogo::abrir(&d.path().join("regras.sqlite")).expect("catálogo");
    let mut versoes = 0;
    for regra in c.regras().expect("regras") {
        for v in c.versoes(&regra.id).expect("versões") {
            versoes += 1;
            eprintln!(
                "definição gravada ({} v{}): {}",
                v.regra,
                v.versao,
                serde_json::to_string(&v.definicao).unwrap_or_default()
            );
        }
    }
    assert!(versoes <= 1, "no máximo uma versão por pedido: {versoes}");
    let m = r.melhor().expect("nenhum rascunho");
    assert!(m.pre_teste.passou, "o rascunho não passou no pré-teste");
    assert!(
        r.chamadas.len() <= 5,
        "{} chamadas (a meta é até 5)",
        r.chamadas.len()
    );
    let v = c.versao(&m.regra, m.versao).expect("versão");
    let condicoes = serde_json::to_string(&v.definicao.detectar).unwrap_or_default();
    assert!(condicoes.contains("123456789012-3"), "{condicoes}");
    let campos: std::collections::BTreeMap<_, _> =
        m.pre_teste.item.campos.iter().cloned().collect();
    assert_eq!(
        campos.get("valor_centavos").map(String::as_str),
        Some("R$ 1.234,56")
    );
    assert_eq!(
        campos.get("competencia").map(String::as_str),
        Some("11/2026")
    );
    assert!(
        campos
            .get("contrato")
            .is_some_and(|v| v.contains("123456789012")),
        "{campos:?}"
    );
}
