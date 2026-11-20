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

/// Imprime, com `SBF_RITA_ESPIAR=1`, o que a ferramenta devolveu e o que o modelo mandou: serve
/// para ver por que uma tentativa falhou.
struct Espiao<P>(P);

impl<P: sbf_ia::ProvedorDeConversa> sbf_ia::ProvedorDeConversa for Espiao<P> {
    fn nome(&self) -> String {
        self.0.nome()
    }
    fn modelo(&self) -> String {
        self.0.modelo()
    }
    fn natureza_do_custo(&self) -> sbf_ia::NaturezaDoCusto {
        self.0.natureza_do_custo()
    }
    fn conversar(
        &self,
        pedido: &sbf_ia::PedidoDeConversa,
        cancelar: &AtomicBool,
    ) -> sbf_ia::ResultadoDeIa<sbf_ia::RespostaDeConversa> {
        let espiar = std::env::var_os("SBF_RITA_ESPIAR").is_some();
        if espiar
            && let Some(sbf_ia::Mensagem::Ferramenta { conteudo, .. }) = pedido.mensagens.last()
        {
            eprintln!("<<< ferramenta devolveu: {conteudo}");
        }
        let r = self.0.conversar(pedido, cancelar)?;
        if espiar {
            for c in &r.chamadas {
                eprintln!(">>> modelo chamou {}: {}", c.nome, c.argumentos);
            }
        }
        Ok(r)
    }
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
            &Espiao(provedor),
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

// ---- medição: variações fictícias do mesmo print ----

/// Uma variação do print fictício: o texto, o `momento` do item, o pedido e o que se espera.
struct Variacao {
    nome: &'static str,
    texto: String,
    momento: Option<&'static str>,
    pedido: &'static str,
    /// A competência esperada (`MM/AAAA`), se o pedido pede competência e há ano.
    competencia: Option<&'static str>,
    /// O trecho do contrato esperado no campo `contrato`.
    contrato: &'static str,
}

fn variacoes() -> Vec<Variacao> {
    let sujo = PRINT
        .replace("123456789012-3", "1234567890l2-3")
        .replace("R$ 1.234,56", "RS 1.234,56")
        .replace("Vencimento em", "Vencimento  em")
        .replace("Prestação atual", "Prestação  atual");
    vec![
        Variacao {
            nome: "original",
            texto: PRINT.to_owned(),
            momento: Some("2026-11-20"),
            pedido: PEDIDO_DO_PRINT,
            competencia: Some("11/2026"),
            contrato: "123456789012",
        },
        Variacao {
            nome: "momento-2025",
            texto: PRINT.to_owned(),
            momento: Some("2025-11-05"),
            pedido: PEDIDO_DO_PRINT,
            competencia: Some("11/2025"),
            contrato: "123456789012",
        },
        Variacao {
            nome: "pedido-reescrito",
            texto: PRINT.to_owned(),
            momento: Some("2026-11-20"),
            pedido: "Quero um padrão para a prestação do financiamento habitacional da Caixa. Reconheça pelo \"Prestação atual\" e pelo número do contrato; o ciclo vem do vencimento. Cria.",
            competencia: Some("11/2026"),
            contrato: "123456789012",
        },
        Variacao {
            nome: "sem-momento",
            texto: PRINT.to_owned(),
            momento: None,
            pedido: "Isto é um comprovante de prestação da habitação CAIXA. O título (Prestação atual) e o número do contrato determinam o padrão. Cria o padrão.",
            competencia: None,
            contrato: "123456789012",
        },
        Variacao {
            nome: "numero-partido",
            texto: PRINT.replace("123456789012-3", "1234 5678 9012-3"),
            momento: Some("2026-11-20"),
            pedido: PEDIDO_DO_PRINT,
            competencia: Some("11/2026"),
            contrato: "1234",
        },
        Variacao {
            nome: "ocr-sujo",
            texto: sujo,
            momento: Some("2026-11-20"),
            pedido: PEDIDO_DO_PRINT,
            competencia: Some("11/2026"),
            contrato: "123456789012",
        },
    ]
}

struct Medicao {
    chamadas: usize,
    entrada: u64,
    saida: u64,
    segundos: f64,
    passou: bool,
    nota: String,
}

fn rodar_variacao(v: &Variacao, modelo: &str, endereco: &str) -> Medicao {
    let d = tempfile::tempdir().expect("pasta");
    Catalogo::abrir(&d.path().join("regras.sqlite")).expect("catálogo");
    let mut item = entrada_de_documento(
        &"bb".repeat(32),
        Some("print-prestacao.png"),
        Some("image/png"),
        Some(&v.texto),
    );
    item.momento = v.momento.map(str::to_owned);
    let ctx = Contexto::novo(
        DadosDaTela::de_documento(d.path(), item),
        Arc::new(sbf_script::ExecutorNoProcesso::novo()),
    )
    .com_similares(Arc::new(SimilaresFixos(vec![])));
    let provedor = ChatCompletions::novo(
        ConfiguracaoChatCompletions::local(endereco.to_owned(), modelo.to_owned()),
        None,
    )
    .expect("provedor");
    let mut rita = Rita::nova(ctx);
    let parar = AtomicBool::new(false);
    let t0 = std::time::Instant::now();
    let r = rita
        .pedir(
            &Espiao(provedor),
            v.pedido,
            &parar,
            &RegistroFalso::novo(),
            &mut |e| {
                if let Evento::Chamada(c) = e {
                    eprintln!(
                        "  [{}] {} {} -> {}",
                        c.numero,
                        c.nome,
                        c.argumentos,
                        c.resultado.chars().take(160).collect::<String>()
                    );
                }
            },
        )
        .expect("relatório");
    let segundos = t0.elapsed().as_secs_f64();
    let nota;
    let mut passou = r.melhor().is_some_and(|m| m.pre_teste.passou);
    if let Some(m) = r.melhor().filter(|m| m.pre_teste.passou) {
        let campos: std::collections::BTreeMap<_, _> =
            m.pre_teste.item.campos.iter().cloned().collect();
        let confere =
            |nome: &str, esperado: &str| campos.get(nome).is_some_and(|v| v.contains(esperado));
        passou = confere("contrato", v.contrato)
            && confere("valor_centavos", "R$ 1.234,56")
            && v.competencia.is_none_or(|c| confere("competencia", c));
        let mut texto = format!("campos {campos:?}");
        if let Ok(ver) = Catalogo::abrir(&d.path().join("regras.sqlite"))
            .and_then(|c| c.versao(&m.regra, m.versao))
        {
            texto.push_str(&format!(
                "; tipo {}; condições {}",
                ver.definicao.tipo_documental,
                serde_json::to_string(&ver.definicao.detectar).unwrap_or_default()
            ));
        }
        nota = texto;
    } else {
        nota = format!("parada {:?}", r.parada);
    }
    Medicao {
        chamadas: r.chamadas.len(),
        entrada: r.consumo.tokens_entrada,
        saida: r.consumo.tokens_saida,
        segundos,
        passou,
        nota,
    }
}

/// Roda cada variação `SBF_RITA_REPETICOES` vezes (padrão 1) e imprime uma linha `MEDICAO|...` por
/// rodada. `SBF_RITA_VARIACOES=a,b` escolhe quais. Meta: todas passam, mediana de até 3 chamadas.
#[test]
#[ignore = "precisa do Ollama local com qwen3-coder:30b"]
fn rita_com_o_ollama_real_mede_as_variacoes_do_print() {
    let modelo =
        std::env::var("SBF_RITA_MODELO").unwrap_or_else(|_| MODELO_LOCAL_PADRAO.to_owned());
    let endereco =
        std::env::var("SBF_RITA_ENDERECO").unwrap_or_else(|_| ENDERECO_DO_OLLAMA.to_owned());
    let vezes: usize = std::env::var("SBF_RITA_REPETICOES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let escolhidas = std::env::var("SBF_RITA_VARIACOES").ok();
    let mut todas_passaram = true;
    for v in variacoes() {
        if escolhidas
            .as_deref()
            .is_some_and(|e| !e.split(',').any(|n| n == v.nome))
        {
            continue;
        }
        let mut chamadas = Vec::new();
        for n in 1..=vezes {
            let m = rodar_variacao(&v, &modelo, &endereco);
            eprintln!(
                "MEDICAO|{}|{n}|{}|{}|{}|{:.0}|{}|{}",
                v.nome, m.chamadas, m.entrada, m.saida, m.segundos, m.passou, m.nota
            );
            todas_passaram &= m.passou;
            chamadas.push(m.chamadas);
        }
        chamadas.sort_unstable();
        eprintln!(
            "MEDIANA|{}|{}",
            v.nome,
            chamadas.get(chamadas.len() / 2).copied().unwrap_or(0)
        );
    }
    assert!(todas_passaram, "alguma rodada não passou (veja MEDICAO)");
}
