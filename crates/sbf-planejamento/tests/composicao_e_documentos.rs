//! Composição do salário e documentos vinculados (spec 11). Dados fictícios.

use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Autor, Banco, Centavos, Certeza, Competencia, Composicao, Data,
    ErroDoPlanejamento, Fonte, Frequencia, IdOcorrencia, Natureza, NovoRecorrente, Planejamento,
    PlanejamentoSqlite, Prazo, Rubrica, SemSaldos, TipoDeDocumento, TipoDeRubrica, Versao,
};

const HOJE: Data = Data {
    ano: 2026,
    mes: 11,
    dia: 16,
};

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

fn servico() -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
}

fn criar_salario(s: &mut PlanejamentoSqlite) -> String {
    s.aplicar(
        Alteracao::CriarRecorrente {
            recorrente: NovoRecorrente {
                nome: "Salário Exemplo".into(),
                grupo: "Entradas".into(),
                subgrupo: None,
                posicao: 1,
                natureza: Natureza::Entrada,
                prazo: Prazo::SemFim,
                primeira_versao: Versao {
                    id: String::new(),
                    desde: comp(2026, 10),
                    valor: Some(Centavos(75000)),
                    certeza: Certeza::Confirmado,
                    fonte: Fonte::Usuario,
                    frequencia: Frequencia::Mensal { dia: 5 },
                    conta: None,
                    centro: None,
                    fornecedor: None,
                    categoria: None,
                    nota: None,
                },
            },
            origem: None,
        },
        Autor::Usuario,
    )
    .expect("criar")
    .alvo
}

fn base(s: &PlanejamentoSqlite, id: &str) -> i64 {
    s.recorrente(id)
        .expect("ler")
        .expect("existe")
        .versao_do_registro
}

fn rubrica(nome: &str, tipo: TipoDeRubrica, valor: i64) -> Rubrica {
    Rubrica {
        codigo: None,
        nome: nome.into(),
        tipo,
        valor: Centavos(valor),
        competencia: None,
        prazo: None,
        recorrente: None,
    }
}

fn composicao(id: &str, desde: Competencia, bruto: i64, desconto: i64) -> Composicao {
    Composicao {
        recorrente: id.into(),
        desde,
        rubricas: vec![
            rubrica("Vencimento", TipoDeRubrica::Provento, bruto),
            rubrica("Desconto", TipoDeRubrica::Desconto, desconto),
        ],
        documento: Some("a".repeat(64)),
        origem: "contracheque fictício".into(),
    }
}

fn definir(s: &mut PlanejamentoSqlite, c: Composicao) -> Result<(), ErroDoPlanejamento> {
    let versao_base = base(s, &c.recorrente);
    s.aplicar(
        Alteracao::DefinirComposicao {
            composicao: c,
            versao_base,
        },
        Autor::Usuario,
    )
    .map(|_| ())
}

fn alvo_ocorrencia(id: &str, c: Competencia) -> AlvoDoDocumento {
    AlvoDoDocumento::Ocorrencia {
        id: IdOcorrencia {
            recorrente: id.into(),
            competencia: c,
            parcela: None,
        },
    }
}

fn vincular(alvo: AlvoDoDocumento, sha: &str) -> Alteracao {
    Alteracao::VincularDocumento {
        alvo,
        sha256: sha.into(),
        tipo_documento: TipoDeDocumento::Contracheque,
        nome: format!("{sha}.pdf"),
        data: None,
    }
}

#[test]
fn composicao_vigente_e_a_de_maior_desde_ate_a_competencia() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    definir(&mut s, composicao(&id, comp(2026, 10), 100000, 25000)).expect("a");
    definir(&mut s, composicao(&id, comp(2027, 1), 120000, 30000)).expect("b");
    assert_eq!(s.composicao(&id, comp(2026, 9)).expect("lê"), None);
    let out = s.composicao(&id, comp(2026, 12)).expect("lê").expect("há");
    assert_eq!(out.desde, comp(2026, 10));
    assert_eq!(out.liquido(), Centavos(75000));
    let jan = s.composicao(&id, comp(2027, 1)).expect("lê").expect("há");
    assert_eq!(jan.desde, comp(2027, 1));
    assert_eq!(
        s.composicao(&id, comp(2030, 6))
            .expect("lê")
            .expect("há")
            .desde,
        comp(2027, 1)
    );
}

#[test]
fn mesma_desde_substitui_sem_duplicar_e_nao_mexe_em_versoes_nem_valor() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    let antes = s.recorrente(&id).expect("ler").expect("existe");
    definir(&mut s, composicao(&id, comp(2026, 10), 100000, 25000)).expect("a");
    definir(&mut s, composicao(&id, comp(2026, 10), 110000, 25000)).expect("b");
    let linhas: i64 = s
        .banco
        .conexao()
        .query_row("SELECT COUNT(*) FROM composicoes", [], |l| l.get(0))
        .expect("conta");
    assert_eq!(linhas, 1);
    assert_eq!(
        s.composicao(&id, comp(2026, 10))
            .expect("lê")
            .expect("há")
            .liquido(),
        Centavos(85000)
    );
    let depois = s.recorrente(&id).expect("ler").expect("existe");
    assert_eq!(depois, antes);
}

#[test]
fn base_velha_nao_muda_nada() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    let err = s
        .aplicar(
            Alteracao::DefinirComposicao {
                composicao: composicao(&id, comp(2026, 10), 100000, 25000),
                versao_base: 99,
            },
            Autor::Usuario,
        )
        .expect_err("base velha");
    assert!(matches!(err, ErroDoPlanejamento::VersaoVelha { atual: 1 }));
    assert_eq!(s.composicao(&id, comp(2026, 12)).expect("lê"), None);
    let err = s
        .aplicar(
            Alteracao::DefinirComposicao {
                composicao: composicao("rec_inexistente", comp(2026, 10), 1, 0),
                versao_base: 1,
            },
            Autor::Usuario,
        )
        .expect_err("inexistente");
    assert!(matches!(err, ErroDoPlanejamento::NaoEncontrado(_)));
}

#[test]
fn divergencia_e_contra_o_valor_digitado_da_ocorrencia() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    definir(&mut s, composicao(&id, comp(2026, 10), 100000, 25000)).expect("a");
    let c = s.composicao(&id, comp(2026, 11)).expect("lê").expect("há");
    let o = s
        .proximas(&id, comp(2026, 11), 1, HOJE)
        .expect("próximas")
        .remove(0);
    assert_eq!(o.previsto, Some(Centavos(75000)));
    assert_eq!(c.divergencia(o.previsto), None);
    definir(&mut s, composicao(&id, comp(2026, 10), 100000, 30000)).expect("b");
    let c = s.composicao(&id, comp(2026, 11)).expect("lê").expect("há");
    assert_eq!(c.divergencia(o.previsto), Some(Centavos(-5000)));
}

#[test]
fn vincular_e_idempotente_e_desvincular_marca() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    let alvo = alvo_ocorrencia(&id, comp(2026, 11));
    let r1 = s
        .aplicar(vincular(alvo.clone(), "aa"), Autor::Usuario)
        .expect("vincula");
    let r2 = s
        .aplicar(vincular(alvo.clone(), "aa"), Autor::Usuario)
        .expect("repete");
    assert_eq!(r1.alvo, r2.alvo);
    assert!(r2.alteracao_id.is_empty());
    assert_eq!(s.documentos(&alvo).expect("docs").len(), 1);
    s.aplicar(
        Alteracao::DesvincularDocumento {
            id: r1.alvo.clone(),
        },
        Autor::Usuario,
    )
    .expect("desvincula");
    assert!(s.documentos(&alvo).expect("docs").is_empty());
    // Vincular de novo depois de desvincular cria outro ativo; o marcado continua gravado.
    s.aplicar(vincular(alvo.clone(), "aa"), Autor::Usuario)
        .expect("revincula");
    assert_eq!(s.documentos(&alvo).expect("docs").len(), 1);
    let total: i64 = s
        .banco
        .conexao()
        .query_row("SELECT COUNT(*) FROM documentos_vinculados", [], |l| {
            l.get(0)
        })
        .expect("conta");
    assert_eq!(total, 2);
    let err = s
        .aplicar(
            Alteracao::DesvincularDocumento { id: "doc_x".into() },
            Autor::Usuario,
        )
        .expect_err("inexistente");
    assert!(matches!(err, ErroDoPlanejamento::NaoEncontrado(_)));
}

#[test]
fn documentos_da_ocorrencia_incluem_os_da_regra_mais_recentes_primeiro() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    let regra = AlvoDoDocumento::Recorrente { id: id.clone() };
    let nov = alvo_ocorrencia(&id, comp(2026, 11));
    let dez = alvo_ocorrencia(&id, comp(2026, 12));
    let tempo = std::sync::atomic::AtomicI64::new(1_794_000_000);
    s = s.com_relogio(move || tempo.fetch_add(60, std::sync::atomic::Ordering::SeqCst));
    s.aplicar(vincular(regra.clone(), "contrato"), Autor::Usuario)
        .expect("regra");
    s.aplicar(vincular(nov.clone(), "nov"), Autor::Usuario)
        .expect("nov");
    s.aplicar(vincular(dez.clone(), "dez"), Autor::Usuario)
        .expect("dez");
    let nomes = |alvo: &AlvoDoDocumento| -> Vec<String> {
        s.documentos(alvo)
            .expect("docs")
            .into_iter()
            .map(|d| d.sha256)
            .collect()
    };
    assert_eq!(nomes(&nov), ["nov", "contrato"]);
    assert_eq!(nomes(&dez), ["dez", "contrato"]);
    assert_eq!(nomes(&regra), ["contrato"]);
}

#[test]
fn simular_nao_grava_nem_muda_meses_e_aplicar_deixa_historico() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    let alvo = AlvoDoDocumento::Recorrente { id: id.clone() };
    let alteracoes = [
        Alteracao::DefinirComposicao {
            composicao: composicao(&id, comp(2026, 10), 100000, 25000),
            versao_base: base(&s, &id),
        },
        vincular(alvo.clone(), "bb"),
    ];
    for a in &alteracoes {
        let sim = s.simular(a, HOJE).expect("simula");
        assert!(sim.meses_afetados.is_empty() && sim.mudancas.is_empty());
        assert!(!sim.conflitos.is_empty());
    }
    assert!(s.documentos(&alvo).expect("docs").is_empty());
    assert_eq!(s.composicao(&id, comp(2026, 12)).expect("lê"), None);
    let antes = s.historico(None, 50).expect("hist").len();
    for a in alteracoes {
        s.aplicar(a, Autor::Usuario).expect("aplica");
    }
    let doc = s.documentos(&alvo).expect("docs").remove(0);
    let sim = s
        .simular(
            &Alteracao::DesvincularDocumento { id: doc.id.clone() },
            HOJE,
        )
        .expect("simula desvincular");
    assert!(sim.mudancas.is_empty());
    assert_eq!(s.documentos(&alvo).expect("docs").len(), 1);
    s.aplicar(Alteracao::DesvincularDocumento { id: doc.id }, Autor::Mcp)
        .expect("desvincula");
    assert_eq!(s.historico(None, 50).expect("hist").len(), antes + 3);
    assert_eq!(s.historico(Some(&id), 50).expect("hist").len(), antes + 3);
}

#[test]
fn delete_fisico_e_bloqueado() {
    let mut s = servico();
    let id = criar_salario(&mut s);
    definir(&mut s, composicao(&id, comp(2026, 10), 100000, 25000)).expect("a");
    s.aplicar(
        vincular(AlvoDoDocumento::Recorrente { id }, "cc"),
        Autor::Usuario,
    )
    .expect("vincula");
    let c = s.banco.conexao();
    for sql in [
        "DELETE FROM composicoes",
        "DELETE FROM documentos_vinculados",
    ] {
        assert!(c.execute(sql, []).is_err(), "deveria falhar: {sql}");
    }
}
