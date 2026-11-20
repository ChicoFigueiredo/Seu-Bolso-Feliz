//! O harness de Rita sobre o texto fictício de um print de app bancário (OCR) de prestação da
//! habitação: no máximo um rascunho por pedido, guarda de repetição, diagnóstico que o modelo
//! consegue usar, `testar_regex` e competência sem ano. Nenhum dado real, nenhuma rede.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

use sbf_ia::{
    ChamadaDeFerramenta, Consumo, ErroDeIa, FalsoDeConversa, RegistroFalso, RespostaDeConversa,
};
use sbf_mcp::backup::Backups;
use sbf_regras::catalogo::Catalogo;
use sbf_rita::{
    Contexto, DadosDaTela, EstadoDaSessao, Evento, Parada, Rita, Sessao, SimilaresFixos,
    entrada_de_documento,
};
use serde_json::{Value, json};

const PRINT: &str = include_str!("fixtures/print-prestacao-habitacao-ficticio.txt");
const PEDIDO: &str = "Isto é um comprovante de prestação da habitação CAIXA. O que determina é o título (Prestação atual) e o número do contrato. A data de vencimento determina a competência. Cria o padrão.";

fn cofre() -> tempfile::TempDir {
    let d = tempfile::tempdir().expect("pasta");
    Catalogo::abrir(&d.path().join("regras.sqlite")).expect("catálogo");
    d
}

fn contexto(d: &Path, momento: Option<&str>) -> Contexto {
    let mut item = entrada_de_documento(
        &"aa".repeat(32),
        Some("print.png"),
        Some("image/png"),
        Some(PRINT),
    );
    item.momento = momento.map(str::to_owned);
    Contexto::novo(
        DadosDaTela::de_documento(d, item),
        Arc::new(sbf_script::ExecutorNoProcesso::novo()),
    )
    .com_similares(Arc::new(SimilaresFixos(vec![])))
}

fn sessao(d: &Path, momento: Option<&str>) -> Sessao {
    Sessao::nova(
        contexto(d, momento),
        PEDIDO,
        "qwen-ficticio",
        Backups::novo(d),
    )
}

/// O que o qwen3-coder:30b mandou na calibração que falhou: `caixa` não está no texto, o regex
/// do contrato não casa e a competência não tem ano.
fn definicao_do_modelo_local() -> Value {
    json!({
        "tipo_documental": "comprovante-prestacao-habitacao-caixa",
        "aplica_a": ["documento"],
        "detectar": {"todos": [{"contem": "prestação atual"}, {"contem": "caixa"}]},
        "extrair": {
            "contrato": {"regex": "(\\d{6}\\s*\\d{6}\\s*\\d{2})", "formato": "texto"},
            "competencia": {"regex": "Vencimento em (\\d+ de [a-zA-Z]+)", "formato": "competencia"}
        },
        "obrigatorios": ["contrato", "competencia"]
    })
}

fn definicao_boa() -> Value {
    json!({
        "tipo_documental": "comprovante-prestacao-habitacao-caixa",
        "aplica_a": ["documento"],
        "detectar": {"todos": [{"contem": "prestação atual"}, {"contem": "123456789012-3"}]},
        "extrair": {
            "contrato": {"regex": "(\\d{12}-\\d)", "formato": "texto"},
            "valor_centavos": {"regex": "Prestação atual\\s*R\\$\\s*([\\d.,]+)", "formato": "centavos"},
            "competencia": {"regex": "Vencimento em (\\d+ de [A-Za-zç]+)", "formato": "competencia"}
        },
        "obrigatorios": ["contrato", "valor_centavos", "competencia"]
    })
}

fn propor(def: &Value) -> Value {
    json!({"regra": "prestacao-habitacao-caixa", "nome": "Prestação da habitação", "definicao": def})
}

fn saida(texto: &str) -> Value {
    serde_json::from_str(texto).expect("a saída da ferramenta é JSON")
}

fn versoes(d: &Path) -> usize {
    Catalogo::abrir(&d.join("regras.sqlite"))
        .expect("catálogo")
        .versoes("prestacao-habitacao-caixa")
        .map(|v| v.len())
        .unwrap_or(0)
}

fn chamada(id: &str, nome: &str, args: Value) -> ChamadaDeFerramenta {
    ChamadaDeFerramenta {
        id: id.into(),
        nome: nome.into(),
        argumentos: args,
    }
}

fn resposta(chamadas: Vec<ChamadaDeFerramenta>) -> Result<RespostaDeConversa, ErroDeIa> {
    Ok(RespostaDeConversa {
        custo_ausente: false,
        texto: None,
        chamadas,
        consumo: Consumo {
            tokens_entrada: 100,
            tokens_saida: 10,
            custo_dezmilesimos_usd: 0,
            nominal: false,
        },
        modelo: "falso-1".into(),
    })
}

fn so_texto(t: &str) -> Result<RespostaDeConversa, ErroDeIa> {
    Ok(RespostaDeConversa {
        custo_ausente: false,
        texto: Some(t.into()),
        chamadas: vec![],
        consumo: Consumo::default(),
        modelo: "falso-1".into(),
    })
}

// ---- um rascunho por pedido ----

#[test]
fn tentativa_que_nao_passa_nao_vira_versao_e_o_fim_do_pedido_grava_so_a_melhor() {
    let d = cofre();
    // A segunda tentativa tira `caixa` e fica mais perto: casa o item, só faltam campos.
    let mut segunda = definicao_do_modelo_local();
    segunda["detectar"]["todos"] = json!([{"contem": "prestação atual"}]);
    let falso = FalsoDeConversa::com_roteiro(vec![
        resposta(vec![chamada(
            "1",
            "propor_padrao",
            propor(&definicao_do_modelo_local()),
        )]),
        resposta(vec![chamada("2", "propor_padrao", propor(&segunda))]),
        so_texto("Não consegui."),
    ]);
    let mut rita = Rita::nova(contexto(d.path(), Some("2026-11-20")));
    let raiz = d.path().to_path_buf();
    let r = rita
        .pedir(
            &falso,
            PEDIDO,
            &AtomicBool::new(false),
            &RegistroFalso::novo(),
            &mut |e| {
                if matches!(e, Evento::Chamada(_)) {
                    assert_eq!(versoes(&raiz), 0, "nada é gravado durante o pedido");
                }
            },
        )
        .expect("relatório");
    assert_eq!(r.parada, Parada::Respondeu);
    assert_eq!(versoes(d.path()), 1, "uma só versão no fim");
    assert_eq!(r.rascunhos.len(), 1);
    let m = r.melhor().expect("melhor");
    assert!(!m.pre_teste.passou);
    assert!(m.neste_pedido);
    assert!(m.pre_teste.item.casa, "foi a tentativa que casa o item");
    assert!(r.resposta.contains("não passou"), "{}", r.resposta);
    assert!(r.resposta.contains("Não consegui."), "{}", r.resposta);
    // O modelo foi avisado de que nada foi gravado.
    let pedidos = falso.pedidos();
    let sbf_ia::Mensagem::Ferramenta { conteudo, .. } =
        pedidos[1].mensagens.last().expect("mensagem")
    else {
        panic!("sem resultado")
    };
    assert!(conteudo.contains("\"gravado\":false"), "{conteudo}");
}

#[test]
fn no_teto_sem_nenhuma_que_passe_grava_uma_so_versao() {
    let d = cofre();
    let n = std::sync::atomic::AtomicI32::new(0);
    let falso = FalsoDeConversa::com_funcao(move |_| {
        let n = n.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let mut def = definicao_do_modelo_local();
        def["prioridade"] = json!(n); // cada tentativa é diferente
        resposta(vec![chamada("x", "propor_padrao", propor(&def))])
    });
    let mut rita = Rita::nova(contexto(d.path(), None));
    let r = rita
        .pedir(
            &falso,
            PEDIDO,
            &AtomicBool::new(false),
            &RegistroFalso::novo(),
            &mut |_| {},
        )
        .expect("relatório");
    assert_eq!(r.parada, Parada::Limite);
    assert_eq!(r.chamadas.len(), 8);
    assert_eq!(versoes(d.path()), 1);
    assert!(r.resposta.contains("uma só versão"), "{}", r.resposta);
}

#[test]
fn a_que_passa_e_a_unica_versao_mesmo_depois_de_falhas() {
    let d = cofre();
    let falso = FalsoDeConversa::com_roteiro(vec![
        resposta(vec![chamada(
            "1",
            "propor_padrao",
            propor(&definicao_do_modelo_local()),
        )]),
        resposta(vec![chamada(
            "2",
            "propor_padrao",
            propor(&definicao_boa()),
        )]),
    ]);
    let mut rita = Rita::nova(contexto(d.path(), Some("2026-11-20T10:00:00-03:00")));
    let r = rita
        .pedir(
            &falso,
            PEDIDO,
            &AtomicBool::new(false),
            &RegistroFalso::novo(),
            &mut |_| {},
        )
        .expect("relatório");
    assert_eq!(r.parada, Parada::Passou, "{}", r.resposta);
    assert_eq!(versoes(d.path()), 1);
    let campos: std::collections::BTreeMap<_, _> = r
        .melhor()
        .expect("melhor")
        .pre_teste
        .item
        .campos
        .iter()
        .cloned()
        .collect();
    assert_eq!(campos["contrato"], "123456789012-3");
    assert_eq!(campos["valor_centavos"], "R$ 1.234,56");
    assert_eq!(campos["competencia"], "11/2026");
}

#[test]
fn sem_momento_a_competencia_sem_ano_nao_passa_e_o_diagnostico_diz_por_que() {
    let d = cofre();
    let mut s = sessao(d.path(), None);
    let o = saida(&s.executar("propor_padrao", &propor(&definicao_boa())));
    assert_eq!(o["passou"], false, "{o}");
    assert_eq!(versoes(d.path()), 0);
    let diag = o["item"]["diagnostico"].to_string();
    assert!(diag.contains("sem ano"), "{diag}");
    assert!(diag.contains("momento"), "{diag}");
}

#[test]
fn o_fim_do_pedido_tambem_vale_para_o_estado_lido_de_outro_processo() {
    // O servidor MCP do `claude -p` só ensaia; o pai grava a melhor no fim.
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    s.executar("propor_padrao", &propor(&definicao_do_modelo_local()));
    assert_eq!(versoes(d.path()), 0);
    let estado: EstadoDaSessao =
        serde_json::from_str(&serde_json::to_string(&s.estado()).expect("json")).expect("volta");
    assert_eq!(estado.tentativas.len(), 1);
    let mut pai = sessao(d.path(), Some("2026-11-20"));
    pai.restaurar(estado);
    pai.encerrar().expect("grava");
    assert_eq!(versoes(d.path()), 1);
    assert_eq!(pai.estado().rascunhos.len(), 1);
    // Roda uma vez só.
    pai.encerrar().expect("de novo");
    assert_eq!(versoes(d.path()), 1);
}

// ---- guarda de repetição ----

#[test]
fn definicao_identica_e_recusada_sem_rodar_e_conta_no_teto() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let primeira = saida(&s.executar("propor_padrao", &propor(&definicao_do_modelo_local())));
    assert_eq!(primeira["tentativa"], 1);
    // O mesmo, com `contem` escrito de outro jeito e as condições em outra ordem.
    let mut igual = definicao_do_modelo_local();
    igual["detectar"]["todos"] = json!([{"contem": "CAIXA"}, {"contem": "Prestacao  Atual"}]);
    let segunda = saida(&s.executar("propor_padrao", &propor(&igual)));
    let erro = segunda["erro"].as_str().expect("recusada");
    assert!(erro.contains("idêntica à tentativa 1"), "{erro}");
    assert!(erro.contains("o pré-teste dela disse"), "{erro}");
    assert!(erro.contains("`caixa` não aparece"), "{erro}");
    assert!(erro.contains("mude"), "{erro}");
    assert_eq!(s.usadas(), 2, "conta no teto");
    assert!(s.chamadas.last().is_some_and(|c| c.recusada));
    assert_eq!(s.estado().tentativas.len(), 1, "não rodou de novo");
}

#[test]
fn repetir_o_que_passou_no_ensaio_e_permitido_para_gravar() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let ensaio = saida(&s.executar("pre_testar", &json!({"definicao": definicao_boa()})));
    assert_eq!(ensaio["passou"], true, "{ensaio}");
    assert_eq!(ensaio["gravado"], false);
    assert_eq!(versoes(d.path()), 0, "o ensaio não grava");
    let gravada = saida(&s.executar("propor_padrao", &propor(&definicao_boa())));
    assert_eq!(gravada["passou"], true);
    assert_eq!(gravada["versao"], 1);
    assert_eq!(versoes(d.path()), 1);
    assert!(s.passou());
}

// ---- diagnóstico ----

#[test]
fn condicao_contem_ausente_diz_o_trecho_e_sugere_linhas_ou_remover() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let o = saida(&s.executar("propor_padrao", &propor(&definicao_do_modelo_local())));
    let diag = o["item"]["diagnostico"][0].as_str().expect("diagnóstico");
    assert!(diag.contains("todos[1]"), "{diag}");
    assert!(
        diag.contains("`caixa` não aparece no texto do item"),
        "{diag}"
    );
    assert!(diag.contains("remova esta condição"), "{diag}");
    // Uma condição parecida com uma linha do texto recebe a linha como sugestão.
    let mut perto = definicao_boa();
    perto["detectar"]["todos"] = json!([{"contem": "prestacao vigente"}]);
    let o = saida(&s.executar("propor_padrao", &propor(&perto)));
    let diag = o["item"]["diagnostico"][0].as_str().expect("diagnóstico");
    assert!(diag.contains("`prestacao vigente` não aparece"), "{diag}");
    assert!(diag.contains("«Prestação atual»"), "{diag}");
}

#[test]
fn campo_cujo_regex_nao_acha_mostra_ate_tres_linhas_candidatas() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let o = saida(&s.executar("propor_padrao", &propor(&definicao_do_modelo_local())));
    let diag: Vec<String> = o["item"]["diagnostico"]
        .as_array()
        .expect("lista")
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();
    let contrato = diag
        .iter()
        .find(|l| l.contains("`contrato`"))
        .expect("campo contrato");
    assert!(contrato.contains("não acha nada no texto"), "{contrato}");
    assert!(
        contrato.contains("«€ ln) 123456789012-3 é O)»"),
        "{contrato}"
    );
    // Até três linhas candidatas, mais o número da sugestão de regex.
    assert!(contrato.matches('«').count() <= 4, "{contrato}");
    assert!(contrato.contains("serve `(\\d{12}-\\d)`"), "{contrato}");
    assert!(contrato.contains("testar_regex"), "{contrato}");
    // Com `momento` a competência sem ano sai: só o contrato falta.
    assert_eq!(diag.len(), 2, "{diag:?}");
}

#[test]
fn passar_exige_que_todo_campo_declarado_saia_do_item() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let mut def = definicao_boa();
    def["extrair"]["contrato"]["regex"] = json!("Contrato:\\s*(\\d+)");
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert_eq!(o["passou"], false, "{o}");
    assert!(
        o["motivo"].as_str().is_some_and(|m| m.contains("contrato")),
        "{o}"
    );
    assert_eq!(versoes(d.path()), 0);
}

// ---- testar_regex ----

#[test]
fn testar_regex_devolve_casamentos_com_a_linha_e_o_valor_formatado() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let o = saida(&s.executar(
        "testar_regex",
        &json!({"regex": "Vencimento em (\\d+ de [A-Za-zç]+)", "formato": "competencia"}),
    ));
    assert_eq!(o["casamentos"], 1, "{o}");
    assert_eq!(o["itens"][0]["linha"], "Vencimento em 25 de Novembro");
    assert_eq!(o["itens"][0]["captura"], "25 de Novembro");
    assert_eq!(o["itens"][0]["valor"], "11/2026");
    // Sem grupo, vale o casamento inteiro; no máximo cinco casamentos.
    let o = saida(&s.executar("testar_regex", &json!({"regex": "\\d+"})));
    assert!(o["casamentos"].as_u64().expect("n") > 5);
    assert_eq!(o["mostrados"], 5);
    // Sem momento o formato diz por que não saiu.
    let mut sem = sessao(d.path(), None);
    let o = saida(&sem.executar(
        "testar_regex",
        &json!({"regex": "Vencimento em (\\d+ de [A-Za-zç]+)", "formato": "competencia"}),
    ));
    assert!(o["itens"][0]["valor"].is_null());
    assert!(
        o["itens"][0]["motivo"]
            .as_str()
            .is_some_and(|m| m.contains("momento")),
        "{o}"
    );
}

#[test]
fn testar_regex_sem_casamento_da_dica_e_regex_invalido_da_erro_e_conta_no_teto() {
    let d = cofre();
    let mut s = sessao(d.path(), None);
    let o = saida(&s.executar(
        "testar_regex",
        &json!({"regex": "(\\d{6}\\s*\\d{6}\\s*\\d{2})"}),
    ));
    assert_eq!(o["casamentos"], 0);
    assert!(o["dica"].as_str().is_some());
    let o = saida(&s.executar("testar_regex", &json!({"regex": "(abc"})));
    assert!(
        o["erro"]
            .as_str()
            .is_some_and(|e| e.contains("regex inválido"))
    );
    let o = saida(&s.executar("testar_regex", &json!({"regex": "a", "grupo": 3})));
    assert!(o["erro"].as_str().is_some_and(|e| e.contains("grupo")));
    assert_eq!(s.usadas(), 3, "conta no teto como qualquer ferramenta");
}

#[test]
fn as_ferramentas_anunciadas_sao_oito_e_incluem_testar_regex() {
    let nomes: Vec<String> = sbf_rita::definicoes().into_iter().map(|f| f.nome).collect();
    assert_eq!(nomes.len(), 8);
    assert!(nomes.iter().any(|n| n == "testar_regex"));
    assert_eq!(sbf_rita::FERRAMENTAS.len(), 8);
    for f in &nomes {
        for proibida in ["ativ", "consolid", "revert", "vincul", "concili", "desativ"] {
            assert!(!f.contains(proibida), "{f}");
        }
    }
}

// ---- escape a mais e número pela metade ----

#[test]
fn barra_dupla_nos_regexes_e_consertada_com_aviso() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let mut def = definicao_boa();
    def["extrair"]["contrato"]["regex"] = json!("(\\\\d{12}-\\\\d)");
    def["extrair"]["valor_centavos"]["regex"] =
        json!("Prestação atual\\\\s*R\\\\$\\\\s*([\\\\d.,]+)");
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert_eq!(o["passou"], true, "{o}");
    let avisos = o["avisos"].to_string();
    assert!(avisos.contains("`contrato`"), "{avisos}");
    assert!(avisos.contains("barra invertida dupla"), "{avisos}");
    // O que foi gravado já tem a barra só.
    let c = Catalogo::abrir(&d.path().join("regras.sqlite")).expect("catálogo");
    let v = c.versao("prestacao-habitacao-caixa", 1).expect("versão");
    assert_eq!(v.definicao.extrair["contrato"].regex, "(\\d{12}-\\d)");
    // `testar_regex` também testa com uma barra só e avisa.
    let o = saida(&s.executar("testar_regex", &json!({"regex": "(\\\\d{12}-\\\\d)"})));
    assert_eq!(o["casamentos"], 1, "{o}");
    assert!(o["avisos"].to_string().contains("barra invertida dupla"));
}

#[test]
fn regex_que_pega_so_parte_do_numero_nao_passa() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let mut def = definicao_boa();
    def["extrair"]["contrato"]["regex"] = json!("(\\d{11}-\\d)");
    def["detectar"]["todos"] = json!([{"contem": "prestação atual"}]);
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert_eq!(o["passou"], false, "{o}");
    let diag = o["item"]["diagnostico"].to_string();
    assert!(diag.contains("só parte do número"), "{diag}");
    assert!(diag.contains("123456789012-3"), "{diag}");
    assert!(diag.contains("`(\\\\d{12}-\\\\d)`"), "{diag}");
    // O pedido também diz que o contrato determina, e as duas falhas vêm juntas.
    assert!(
        diag.contains("nenhuma condição de `detectar` usa o valor dele"),
        "{diag}"
    );
    assert_eq!(versoes(d.path()), 0);
}

#[test]
fn o_numero_do_contrato_que_determina_precisa_estar_nas_condicoes() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let mut def = definicao_boa();
    def["detectar"]["todos"] = json!([{"contem": "prestação atual"}]);
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert_eq!(o["passou"], false, "{o}");
    let diag = o["item"]["diagnostico"].to_string();
    assert!(
        diag.contains("nenhuma condição de `detectar` usa o valor dele"),
        "{diag}"
    );
    assert!(diag.contains("123456789012-3"), "{diag}");
}

#[test]
fn definicao_mandada_como_texto_com_escapes_invalidos_e_reparada() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    // O que o qwen3-coder manda: o objeto como texto, com `\d` e `\s` crus.
    let texto = r#"{"tipo_documental":"comprovante-prestacao-habitacao-caixa","detectar":{"todos":[{"contem":"prestação atual"},{"contem":"123456789012-3"}]},"extrair":{"contrato":{"regex":"(\d{12}-\d)","formato":"texto"},"valor_centavos":{"regex":"R\$\s*([\d.,]+)","formato":"centavos"},"competencia":{"regex":"Vencimento em (\d{1,2} de [A-Za-zç]+)","formato":"competencia"}},"obrigatorios":["contrato","competencia"]}"#;
    let o = saida(&s.executar("propor_padrao", &propor(&json!(texto))));
    assert_eq!(o["passou"], true, "{o}");
    // E o texto que nem JSON é volta com o motivo.
    let o = saida(&s.executar("propor_padrao", &propor(&json!("{não é json"))));
    assert!(
        o["erro"]
            .as_str()
            .is_some_and(|e| e.contains("objeto JSON") && e.contains("line 1")),
        "{o}"
    );
}

// ---- esqueleto sem IA no primeiro prompt ----

/// A linha `definicao: {...}` do bloco do esqueleto no primeiro prompt.
fn definicao_do_esqueleto(prompt: &str) -> Value {
    // Vai como texto (as barras uma vez só, como nas instruções): a ferramenta repara os escapes.
    let linha = prompt
        .lines()
        .find_map(|l| l.strip_prefix("definicao: "))
        .expect("o primeiro prompt traz o esqueleto");
    Value::String(linha.to_owned())
}

#[test]
fn o_primeiro_prompt_traz_o_esqueleto_e_o_pre_teste_dele_e_uma_chamada_basta() {
    let d = cofre();
    let vistos = std::sync::Arc::new(std::sync::Mutex::new(String::new()));
    let guardados = std::sync::Arc::clone(&vistos);
    let falso = FalsoDeConversa::com_funcao(move |p| {
        let prompt = p
            .mensagens
            .iter()
            .find_map(|m| match m {
                sbf_ia::Mensagem::Usuario(t) => Some(t.clone()),
                _ => None,
            })
            .expect("pedido");
        *guardados.lock().expect("trava") = prompt.clone();
        resposta(vec![chamada(
            "1",
            "propor_padrao",
            propor(&definicao_do_esqueleto(&prompt)),
        )])
    });
    let mut rita = Rita::nova(contexto(d.path(), Some("2026-11-20")));
    let r = rita
        .pedir(
            &falso,
            PEDIDO,
            &AtomicBool::new(false),
            &RegistroFalso::novo(),
            &mut |_| {},
        )
        .expect("relatório");
    assert_eq!(r.parada, Parada::Passou, "{}", r.resposta);
    assert_eq!(r.chamadas.len(), 1, "o esqueleto já passava");
    assert_eq!(versoes(d.path()), 1);
    let prompt = vistos.lock().expect("trava").clone();
    assert!(prompt.contains("Esqueleto de definição, montado sem IA"));
    assert!(
        prompt.contains("pré-teste do esqueleto: PASSOU"),
        "{prompt}"
    );
    assert!(prompt.contains("123456789012-3"));
    let campos: std::collections::BTreeMap<_, _> = r
        .melhor()
        .expect("melhor")
        .pre_teste
        .item
        .campos
        .iter()
        .cloned()
        .collect();
    assert_eq!(campos["valor_centavos"], "R$ 1.234,56");
    assert_eq!(campos["competencia"], "11/2026");
    assert_eq!(campos["contrato"], "123456789012-3");
}

#[test]
fn o_esqueleto_sem_momento_deixa_a_competencia_de_fora_e_o_prompt_diz_por_que() {
    let d = cofre();
    let r = sbf_rita::esqueleto_resumido(&contexto(d.path(), None), PEDIDO).expect("esqueleto");
    assert!(r.contains("ficou de fora do esqueleto"), "{r}");
    assert!(r.contains("sem ano"), "{r}");
    assert!(!r.contains("\"competencia\":{"), "{r}");
}

#[test]
fn o_esqueleto_de_um_texto_simples_so_com_titulo_e_valor_passa() {
    let d = cofre();
    // Pedido só com o título citado: o esqueleto leva o título e o valor.
    let texto = "Prestação atual\nR$ 10,00\n";
    let mut item = entrada_de_documento("aa", Some("a.txt"), Some("text/plain"), Some(texto));
    item.momento = None;
    let ctx = Contexto::novo(
        DadosDaTela::de_documento(d.path(), item),
        Arc::new(sbf_script::ExecutorNoProcesso::novo()),
    )
    .com_similares(Arc::new(SimilaresFixos(vec![])));
    let r = sbf_rita::esqueleto_resumido(&ctx, "O título (Prestação atual) determina; cria")
        .expect("esqueleto");
    assert!(r.contains("PASSOU"), "{r}");
}

// ---- esquema que ensina ----

#[test]
fn definicao_invalida_devolve_a_causa_o_exemplo_e_as_variantes() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let mut def = definicao_boa();
    def["extrair"]["contrato"]["formato"] = json!("numero");
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    let e = o["erro"].as_str().expect("erro");
    assert!(e.starts_with("definição inválida: "), "{e}");
    assert!(e.contains("unknown variant `numero`"), "{e}");
    assert!(e.contains("Exemplo mínimo válido:"), "{e}");
    assert!(e.contains("\"detectar\":{\"todos\""), "{e}");
    assert!(e.contains("\"obrigatorios\""), "{e}");
    assert!(e.contains("contem | regex | mime"), "{e}");
    assert!(e.contains("texto | centavos | data | competencia"), "{e}");
    assert!(e.contains("documento | registro | mensagem"), "{e}");
}

#[test]
fn aplica_a_desconhecido_vira_documento_e_condicao_texto_vira_contem_com_aviso() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let mut def = definicao_boa();
    def["aplica_a"] = json!(["contrato"]);
    def["detectar"]["todos"] = json!([{"texto": "prestação atual"}, {"contem": "123456789012-3"}]);
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert_eq!(o["passou"], true, "{o}");
    let avisos = o["avisos"].to_string();
    assert!(avisos.contains("`aplica_a` com `contrato`"), "{avisos}");
    assert!(avisos.contains("`documento`"), "{avisos}");
    assert!(avisos.contains("todos[0]"), "{avisos}");
    assert!(avisos.contains("contem"), "{avisos}");
}

#[test]
fn nao_conserta_o_que_e_ambiguo() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    // Duas chaves na condição, ou uma chave desconhecida diferente de `texto`: continua erro.
    let mut def = definicao_boa();
    def["detectar"]["todos"] = json!([{"titulo": "prestação atual"}]);
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert!(
        o["erro"]
            .as_str()
            .is_some_and(|e| e.starts_with("definição inválida")),
        "{o}"
    );
    let mut def = definicao_boa();
    def["detectar"]["todos"] = json!([{"texto": "x", "contem": "prestação atual"}]);
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert!(o["erro"].is_string(), "{o}");
    let mut def = definicao_boa();
    def["aplica_a"] = json!([1]);
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    assert!(o["erro"].is_string(), "{o}");
}

// ---- diagnóstico com regex pronto ----

#[test]
fn o_diagnostico_de_data_por_extenso_e_de_valor_traz_o_regex_pronto() {
    let d = cofre();
    let mut s = sessao(d.path(), Some("2026-11-20"));
    let mut def = definicao_boa();
    def["extrair"]["competencia"]["regex"] = json!("(\\d{2}/\\d{2}/\\d{4})");
    def["extrair"]["valor_centavos"]["regex"] = json!("Valor:\\s*([\\d.,]+)");
    let o = saida(&s.executar("propor_padrao", &propor(&def)));
    let diag = o["item"]["diagnostico"].to_string();
    assert!(
        diag.contains("Para a data «Vencimento em 25 de Novembro» serve"),
        "{diag}"
    );
    assert!(
        diag.contains("Vencimento\\\\s+em\\\\s*[:\\\\-]?\\\\s*(\\\\d{1,2}\\\\s+de"),
        "{diag}"
    );
    assert!(diag.contains("Para o valor «R$ 1.234,56» serve"), "{diag}");
    assert!(diag.contains("Prestação\\\\s+atual"), "{diag}");
    assert!(diag.contains("`testar_regex` só se ainda falhar"), "{diag}");
}
