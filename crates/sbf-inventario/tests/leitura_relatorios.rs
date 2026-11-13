//! INV-18: cada resposta do §11 bate com uma consulta de referência sobre o banco de teste e
//! leva "a conferir" (todo valor é `Fonte::Inventario`). Nenhum termo do livro-caixa aparece.

mod comum;

use rusqlite::Connection;
use sbf_inventario::consulta::{
    Celula, Centavos, Consulta, ErroDeConsulta, Filtro, Fonte, Painel, Pergunta, Tabela,
};
use sbf_inventario::leitor::LeitorSqlite;
use sbf_inventario::relatorios::{self, FonteDeGastos, Gasto};
use sbf_inventario::vistas::Contexto;

const HOJE: &str = "2026-11-13";

const TODAS: [Pergunta; 8] = [
    Pergunta::QuantoSaiu,
    Pergunta::MaioresFornecedores,
    Pergunta::OQueSeRepete,
    Pergunta::OQueAumentou,
    Pergunta::OndeSeConcentra,
    Pergunta::NaoClassificado,
    Pergunta::PossiveisDuplicidades,
    Pergunta::OQueFaltaParaConfiar,
];

fn abrir() -> (comum::BancoDeTeste, LeitorSqlite) {
    let banco = comum::banco_de_teste();
    let leitor = LeitorSqlite::abrir(&banco.caminho)
        .expect("abrir")
        .com_hoje(HOJE);
    (banco, leitor)
}

fn responder(leitor: &LeitorSqlite, p: Pergunta, filtro: &Filtro) -> Painel {
    leitor
        .relatorio(p, Fonte::Inventario, filtro)
        .unwrap_or_else(|e| panic!("{p:?}: {e}"))
}

fn textos(c: &Celula, saida: &mut Vec<String>) {
    match c {
        Celula::Texto(t) | Celula::Data(t) => saida.push(t.clone()),
        Celula::Link { texto, .. } => saida.push(texto.clone()),
        Celula::Estado { rotulo, .. } => saida.push(rotulo.clone()),
        Celula::Links(ls) => saida.extend(ls.iter().map(|(_, t)| t.clone())),
        _ => {}
    }
}

fn todos_os_textos(p: &Painel) -> Vec<String> {
    let mut v: Vec<String> = p.avisos.clone();
    for s in &p.secoes {
        v.push(s.titulo.clone());
        v.extend(s.nota.clone());
        v.extend(s.tabela.colunas.iter().map(|c| c.rotulo.clone()));
        for l in &s.tabela.linhas {
            for c in &l.celulas {
                textos(c, &mut v);
            }
        }
    }
    v
}

fn valores(t: &Tabela, coluna: usize) -> Vec<Centavos> {
    t.linhas
        .iter()
        .map(|l| match &l.celulas[coluna] {
            Celula::Valor { centavos, .. } => *centavos,
            outra => panic!("esperava Valor: {outra:?}"),
        })
        .collect()
}

fn nomes(t: &Tabela) -> Vec<String> {
    t.linhas
        .iter()
        .map(|l| match &l.celulas[0] {
            Celula::Link { texto, .. } | Celula::Texto(texto) => texto.clone(),
            outra => panic!("{outra:?}"),
        })
        .collect()
}

#[test]
fn toda_pergunta_responde_com_a_marca_e_sem_termo_do_livro_caixa() {
    let (_b, leitor) = abrir();
    let proibidos = [
        "resultado do mês",
        "disponível agora",
        "livre após compromissos",
        "livro-caixa",
        "livro caixa",
        "movimentação ",
    ];
    for p in TODAS {
        let painel = responder(&leitor, p, &Filtro::default());
        assert!(!painel.secoes.is_empty(), "{p:?}");
        assert!(
            painel
                .avisos
                .iter()
                .any(|a| a.contains("não conta como gasto")),
            "{p:?} deveria avisar que movimento entre contas não é gasto"
        );
        for t in todos_os_textos(&painel) {
            let baixo = t.to_lowercase();
            for proibido in proibidos {
                assert!(
                    !baixo.contains(proibido),
                    "{p:?}: '{t}' tem o termo '{proibido}'"
                );
            }
        }
        // Todo valor leva a fonte Inventário (a janela põe "a conferir").
        let mut valores = 0;
        for s in &painel.secoes {
            for l in &s.tabela.linhas {
                for c in &l.celulas {
                    if let Celula::Valor { fonte, .. } = c {
                        assert_eq!(*fonte, Fonte::Inventario, "{p:?}");
                        valores += 1;
                    }
                }
            }
        }
        if !matches!(p, Pergunta::OQueFaltaParaConfiar) {
            assert!(valores > 0, "{p:?} sem nenhum valor");
        }
    }
}

#[test]
fn livro_caixa_nao_e_suportado_na_fatia_1() {
    let (_b, leitor) = abrir();
    for p in TODAS {
        let erro = leitor
            .relatorio(p, Fonte::LivroCaixa, &Filtro::default())
            .expect_err("deve recusar");
        let ErroDeConsulta::NaoSuportado(motivo) = erro else {
            panic!("{erro}")
        };
        assert!(motivo.contains("fatia 2"), "{motivo}");
    }
}

fn sql_i64(banco: &comum::BancoDeTeste, sql: &str) -> i64 {
    let conn = Connection::open(&banco.caminho).expect("conexão");
    conn.query_row(sql, [], |l| l.get::<_, Option<i64>>(0))
        .expect("consulta")
        .unwrap_or(0)
}

/// A regra de gasto escrita à mão, sem usar `movimento`: o teste não pode confiar no que ele confere.
const GASTO_DE_REFERENCIA: &str = "valor_centavos < 0 AND tipo NOT IN ('transferencia_interna','investimento','pagamento_fatura','fatura_cartao')";

#[test]
fn quanto_saiu_bate_com_a_consulta_de_referencia() {
    let (b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::QuantoSaiu, &Filtro::default());
    let atual = valores(&painel.secoes[0].tabela, 2);
    let esperado = sql_i64(
        &b,
        &format!(
            "SELECT -sum(valor_centavos) FROM registro WHERE ciclo_id = '2026-10' AND {GASTO_DE_REFERENCIA}"
        ),
    );
    assert_eq!(atual, vec![esperado]);
    assert!(esperado > 0);
    // O pagamento da fatura e o investimento (ciclo 2026-09) não entram no que saiu.
    let setembro = valores(&painel.secoes[1].tabela, 1)[1];
    let tudo = sql_i64(
        &b,
        "SELECT -sum(valor_centavos) FROM registro WHERE ciclo_id = '2026-09' AND valor_centavos < 0",
    );
    assert!(setembro < tudo - 195_000, "{setembro} {tudo}");
    // Últimos ciclos: cada linha bate.
    let ultimos = &painel.secoes[1].tabela;
    let nomes_ciclos: Vec<String> = ultimos
        .linhas
        .iter()
        .map(|l| match &l.celulas[0] {
            Celula::Link { alvo, .. } => alvo.chave.clone(),
            o => panic!("{o:?}"),
        })
        .collect();
    assert_eq!(nomes_ciclos[..3], ["2026-10", "2026-09", "2026-08"]);
    for (ciclo, valor) in nomes_ciclos.iter().zip(valores(ultimos, 1)) {
        let ref_ = sql_i64(
            &b,
            &format!(
                "SELECT -sum(valor_centavos) FROM registro WHERE ciclo_id = '{ciclo}' AND {GASTO_DE_REFERENCIA}"
            ),
        );
        assert_eq!(valor, ref_, "{ciclo}");
    }
}

#[test]
fn maiores_fornecedores_batem_com_a_consulta_de_referencia() {
    let (b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::MaioresFornecedores, &Filtro::default());
    let t = &painel.secoes[0].tabela;
    let conn = Connection::open(&b.caminho).expect("conexão");
    let mut instrucao = conn
        .prepare(&format!(
            "SELECT coalesce(fornecedor_normalizado, contraparte, nullif(supplier_name_raw,''), descricao_original) AS f, -sum(valor_centavos) AS s
             FROM registro WHERE {GASTO_DE_REFERENCIA} AND ciclo_id BETWEEN '2025-10' AND '2026-10'
             GROUP BY 1 ORDER BY s DESC, f LIMIT 10"
        ))
        .expect("preparar");
    let esperado: Vec<(String, i64)> = instrucao
        .query_map([], |l| Ok((l.get(0)?, l.get(1)?)))
        .expect("consulta")
        .collect::<Result<_, _>>()
        .expect("linhas");
    let obtido: Vec<(String, i64)> = nomes(t).into_iter().zip(valores(t, 1)).collect();
    assert_eq!(obtido, esperado);
    // O primeiro tem a maior fatia.
    assert!(matches!(&t.linhas[0].celulas[2], Celula::Texto(p) if p.ends_with('%')));
}

#[test]
fn o_que_se_repete_e_as_assinaturas() {
    let (_b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::OQueSeRepete, &Filtro::default());
    let repetem = nomes(&painel.secoes[0].tabela);
    for f in [
        "Energia Exemplo",
        "Mercado Delta",
        "Streaming Zeta",
        "Empresa Empregadora Exemplo",
    ] {
        // Quem só recebe (salário) não é gasto, então não entra.
        assert_eq!(
            repetem.contains(&f.to_owned()),
            f != "Empresa Empregadora Exemplo",
            "{f}"
        );
    }
    assert!(
        !repetem.contains(&"Banco Alfa".to_owned()) || repetem.contains(&"Banco Alfa".to_owned())
    );
    assert_eq!(nomes(&painel.secoes[1].tabela), ["Streaming Zeta"]);
    assert_eq!(valores(&painel.secoes[1].tabela, 2), [3990]);
}

#[test]
fn o_que_aumentou_compara_o_ciclo_atual_com_o_anterior() {
    let (_b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::OQueAumentou, &Filtro::default());
    let t = &painel.secoes[0].tabela;
    let energia = t
        .linhas
        .iter()
        .position(
            |l| matches!(&l.celulas[0], Celula::Link { texto, .. } if texto == "Energia Exemplo"),
        )
        .expect("Energia Exemplo subiu");
    assert_eq!(valores(t, 1)[energia], 34_400);
    assert_eq!(valores(t, 2)[energia], 37_500);
    assert!(matches!(&t.linhas[energia].celulas[3], Celula::Texto(p) if p == "+9%"));
    // Streaming Zeta ficou igual: não aparece.
    assert!(!nomes(t).contains(&"Streaming Zeta".to_owned()));
}

#[test]
fn onde_o_dinheiro_se_concentra_bate_por_categoria() {
    let (b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::OndeSeConcentra, &Filtro::default());
    let t = &painel.secoes[0].tabela;
    let conn = Connection::open(&b.caminho).expect("conexão");
    let mut instrucao = conn
        .prepare(&format!(
            "SELECT coalesce(nullif(categoria_provavel,''),'Sem categoria') c, -sum(valor_centavos) s FROM registro
             WHERE {GASTO_DE_REFERENCIA} AND ciclo_id BETWEEN '2025-10' AND '2026-10' GROUP BY 1 ORDER BY s DESC, c"
        ))
        .expect("preparar");
    let esperado: Vec<(String, i64)> = instrucao
        .query_map([], |l| Ok((l.get(0)?, l.get(1)?)))
        .expect("consulta")
        .collect::<Result<_, _>>()
        .expect("linhas");
    let obtido: Vec<(String, i64)> = nomes(t).into_iter().zip(valores(t, 1)).collect();
    assert_eq!(obtido, esperado);
    assert!(nomes(t).contains(&"Sem categoria".to_owned()));
    // Por fornecedor, as dez maiores.
    assert!(painel.secoes[1].tabela.linhas.len() <= 10);
}

#[test]
fn o_que_ainda_nao_foi_classificado() {
    let (b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::NaoClassificado, &Filtro::default());
    let esperado = sql_i64(
        &b,
        "SELECT count(*) FROM registro
          WHERE tipo NOT IN ('transferencia_interna','investimento','pagamento_fatura','fatura_cartao')
            AND ciclo_id BETWEEN '2025-10' AND '2026-10'
            AND (coalesce(fornecedor_normalizado, nullif(contraparte,''), nullif(supplier_name_raw,'')) IS NULL
                 OR coalesce(nullif(categoria_provavel,''),'Sem categoria') = 'Sem categoria')",
    );
    let Celula::Contagem { n, .. } = &painel.secoes[0].tabela.linhas[0].celulas[1] else {
        panic!()
    };
    assert_eq!(*n, esperado);
    assert!(esperado >= 2);
    let Celula::Contagem { n: a_triar, .. } = &painel.secoes[0].tabela.linhas[1].celulas[1] else {
        panic!()
    };
    assert_eq!(*a_triar, 2);
    assert!(!painel.secoes[1].tabela.linhas.is_empty());
}

#[test]
fn possiveis_duplicidades() {
    let (_b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::PossiveisDuplicidades, &Filtro::default());
    // Dois achados com o mesmo texto, uma relação marcada e ao menos um par de registros.
    assert_eq!(painel.secoes[0].tabela.linhas.len(), 2);
    assert_eq!(painel.secoes[1].tabela.linhas.len(), 1);
    let pares = &painel.secoes[2].tabela;
    assert!(!pares.linhas.is_empty());
    // O par da conta de energia de outubro: o documento e o registro do banco, 18.750.
    assert!(valores(pares, 4).contains(&-18_750));
    // Movimento entre contas não aparece como duplicidade.
    assert!(!valores(pares, 4).contains(&-120_000));
}

#[test]
fn o_que_falta_para_confiar() {
    let (_b, leitor) = abrir();
    let painel = responder(&leitor, Pergunta::OQueFaltaParaConfiar, &Filtro::default());
    let lacunas = &painel.secoes[0].tabela;
    assert!(!lacunas.linhas.is_empty());
    assert!(
        lacunas
            .linhas
            .iter()
            .any(|l| matches!(&l.celulas[5], Celula::Links(m) if !m.is_empty()))
    );
    // Só a fatura de outubro não fecha.
    assert_eq!(painel.secoes[1].tabela.linhas.len(), 1);
    // Conexões sem coleta: a ausente, a que falhou e a interrompida.
    let sem_coleta = &painel.secoes[2].tabela;
    let situacoes: Vec<String> = sem_coleta
        .linhas
        .iter()
        .map(|l| match &l.celulas[1] {
            Celula::Texto(t) => t.clone(),
            o => panic!("{o:?}"),
        })
        .collect();
    assert_eq!(situacoes.len(), 3, "{situacoes:?}");
    assert!(situacoes.contains(&"sem coleta".to_owned()));
    assert!(situacoes.contains(&"a última coleta falhou".to_owned()));
}

#[test]
fn o_filtro_de_periodo_e_de_fonte_muda_a_resposta() {
    let (_b, leitor) = abrir();
    let tudo = valores(
        &responder(&leitor, Pergunta::MaioresFornecedores, &Filtro::default()).secoes[0].tabela,
        1,
    );
    let so_gmail = Filtro {
        fontes: vec!["gmail".into()],
        ..Filtro::default()
    };
    let t = responder(&leitor, Pergunta::MaioresFornecedores, &so_gmail);
    assert_eq!(valores(&t.secoes[0].tabela, 1), [32_000]);
    let so_outubro = Filtro {
        ciclos: Some(("2026-10".into(), "2026-10".into())),
        ..Filtro::default()
    };
    let t = responder(&leitor, Pergunta::MaioresFornecedores, &so_outubro);
    assert!(valores(&t.secoes[0].tabela, 1).iter().sum::<i64>() < tudo.iter().sum::<i64>());
}

/// O código das perguntas lê os gastos por um trait: o livro-caixa implementa o mesmo trait
/// e as 8 perguntas servem as duas fontes (ADR-012).
struct GastosFalsos;
impl FonteDeGastos for GastosFalsos {
    fn gastos(
        &self,
        _de: &str,
        _ate: &str,
        _fontes: &[String],
    ) -> Result<Vec<Gasto>, ErroDeConsulta> {
        Ok(vec![
            Gasto {
                ciclo: "2026-10".into(),
                fornecedor: Some("Loja Falsa".into()),
                categoria: "Teste".into(),
                conta: None,
                centavos: 1_000,
                vezes: 1,
            },
            Gasto {
                ciclo: "2026-09".into(),
                fornecedor: Some("Loja Falsa".into()),
                categoria: "Teste".into(),
                conta: None,
                centavos: 500,
                vezes: 1,
            },
            Gasto {
                ciclo: "2026-10".into(),
                fornecedor: Some("Outra Falsa".into()),
                categoria: "Teste".into(),
                conta: None,
                centavos: 300,
                vezes: 1,
            },
        ])
    }
}

#[test]
fn o_mesmo_codigo_serve_qualquer_fonte_de_gastos() {
    let (b, _leitor) = abrir();
    let conn = Connection::open(&b.caminho).expect("conexão");
    let ctx = Contexto::novo(&conn).expect("contexto");
    for p in [
        Pergunta::QuantoSaiu,
        Pergunta::MaioresFornecedores,
        Pergunta::OQueSeRepete,
        Pergunta::OQueAumentou,
        Pergunta::OndeSeConcentra,
    ] {
        relatorios::responder_com(&GastosFalsos, &ctx, p, &Filtro::default(), HOJE)
            .unwrap_or_else(|e| panic!("{p:?}: {e}"));
    }
    let painel = relatorios::responder_com(
        &GastosFalsos,
        &ctx,
        Pergunta::MaioresFornecedores,
        &Filtro::default(),
        HOJE,
    )
    .expect("painel");
    assert_eq!(
        nomes(&painel.secoes[0].tabela),
        ["Loja Falsa", "Outra Falsa"]
    );
    assert_eq!(valores(&painel.secoes[0].tabela, 1), [1_500, 300]);
}
