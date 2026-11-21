//! Contas do extrato, assinaturas e conciliação com o extrato (spec 11). Dados fictícios.

use std::sync::{Arc, Mutex};

use sbf_planejamento::{
    Alteracao, Assinatura, Autor, Banco, Centavos, Certeza, Competencia, Composicao, Conciliacao,
    Data, ErroDoPlanejamento, Fonte, FonteDeMovimentos, Frequencia, IdOcorrencia,
    MovimentoDoExtrato, Natureza, NovoRecorrente, Periodo, Planejamento, PlanejamentoSqlite, Prazo,
    Rubrica, SemSaldos, TipoDeRubrica, Versao,
};

const HOJE: Data = Data {
    ano: 2026,
    mes: 11,
    dia: 16,
};

/// O relógio fixado em 2026-11-16 12:00 UTC.
const RELOGIO: i64 = 1_794_830_400;

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

fn novembro() -> Periodo {
    Periodo::MesCivil(comp(2026, 11))
}

fn dia(d: u8) -> Data {
    Data::nova(2026, 11, d).expect("data")
}

#[derive(Clone, Default)]
struct Extrato(Arc<Mutex<Vec<MovimentoDoExtrato>>>);

impl Extrato {
    fn por(&self, id: &str, d: u8, valor: i64, conta: &str, descricao: &str) {
        self.0.lock().expect("trava").push(MovimentoDoExtrato {
            movimento: id.to_owned(),
            data: dia(d),
            valor: Centavos(valor),
            conta_do_extrato: conta.to_owned(),
            descricao: descricao.to_owned(),
            tipo: if valor > 0 { "credito" } else { "debito" }.to_owned(),
            contraparte: None,
            contraparte_doc: None,
        });
    }
}

impl FonteDeMovimentos for Extrato {
    fn movimentos(&self, de: Data, ate: Data) -> Vec<MovimentoDoExtrato> {
        self.0
            .lock()
            .expect("trava")
            .iter()
            .filter(|m| de <= m.data && m.data <= ate)
            .cloned()
            .collect()
    }
}

fn servico(extrato: &Extrato) -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
        .com_movimentos(Box::new(extrato.clone()))
        .com_relogio(|| RELOGIO)
}

fn criar(
    s: &mut PlanejamentoSqlite,
    nome: &str,
    natureza: Natureza,
    dia_do_mes: u8,
    valor: Option<i64>,
    conta: Option<&str>,
) -> String {
    s.aplicar(
        Alteracao::CriarRecorrente {
            recorrente: NovoRecorrente {
                nome: nome.into(),
                grupo: "Geral".into(),
                subgrupo: None,
                posicao: 1,
                natureza,
                prazo: Prazo::SemFim,
                primeira_versao: Versao {
                    id: String::new(),
                    desde: comp(2026, 1),
                    valor: valor.map(Centavos),
                    certeza: Certeza::Confirmado,
                    fonte: Fonte::Usuario,
                    frequencia: Frequencia::Mensal { dia: dia_do_mes },
                    conta: conta.map(str::to_owned),
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

fn mapear(s: &mut PlanejamentoSqlite, codigo: &str, conta: Option<&str>) {
    s.aplicar(
        Alteracao::MapearConta {
            codigo: codigo.into(),
            conta_do_extrato: conta.map(str::to_owned),
        },
        Autor::Usuario,
    )
    .expect("mapear");
}

fn assinar(s: &mut PlanejamentoSqlite, id: &str, texto: &str, janela: u8) {
    let b = base(s, id);
    s.aplicar(
        Alteracao::DefinirAssinatura {
            recorrente: id.into(),
            assinatura: Some(Assinatura {
                texto: texto.into(),
                janela_dias: janela,
            }),
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("assinar");
}

fn ocorrencia(id: &str) -> IdOcorrencia {
    IdOcorrencia {
        recorrente: id.into(),
        competencia: comp(2026, 11),
        parcela: None,
    }
}

fn sugestoes(s: &PlanejamentoSqlite, id: &str) -> Vec<Conciliacao> {
    s.conciliacoes(id).expect("conciliacoes")
}

// ---------------------------------------------------------------------------------------
// Contas e assinaturas
// ---------------------------------------------------------------------------------------

#[test]
fn mapear_conta_grava_confirma_desfaz_e_deixa_historico() {
    let extrato = Extrato::default();
    let mut s = servico(&extrato);
    criar(
        &mut s,
        "Aluguel",
        Natureza::Saida,
        5,
        Some(100_000),
        Some("CX-0001"),
    );
    criar(
        &mut s,
        "Luz",
        Natureza::Saida,
        8,
        Some(10_000),
        Some("Nubank"),
    );
    assert_eq!(
        s.contas()
            .expect("contas")
            .iter()
            .map(|c| (c.codigo.as_str(), c.conta_do_extrato.as_deref()))
            .collect::<Vec<_>>(),
        [("CX-0001", None), ("Nubank", None)]
    );
    mapear(&mut s, "CX-0001", Some("BANCO · 1"));
    let contas = s.contas().expect("contas");
    assert_eq!(contas[0].conta_do_extrato.as_deref(), Some("BANCO · 1"));
    assert_eq!(contas[1].conta_do_extrato, None);
    assert_eq!(
        s.historico(Some("CX-0001"), 10).expect("historico").len(),
        1
    );

    // Trocar e desfazer: a linha continua, o histórico só cresce.
    mapear(&mut s, "CX-0001", Some("BANCO · 2"));
    mapear(&mut s, "CX-0001", None);
    assert_eq!(s.contas().expect("contas")[0].conta_do_extrato, None);
    assert_eq!(
        s.historico(Some("CX-0001"), 10).expect("historico").len(),
        3
    );
}

#[test]
fn mapear_conta_valida_os_campos() {
    let mut s = servico(&Extrato::default());
    for (codigo, conta) in [("  ", Some("B")), ("CX", Some(" "))] {
        let erro = s.aplicar(
            Alteracao::MapearConta {
                codigo: codigo.into(),
                conta_do_extrato: conta.map(str::to_owned),
            },
            Autor::Usuario,
        );
        assert!(matches!(erro, Err(ErroDoPlanejamento::Invalida(_))));
    }
}

#[test]
fn contas_incluem_o_par_mesmo_sem_versao_usando_o_codigo() {
    let mut s = servico(&Extrato::default());
    mapear(&mut s, "Antiga", Some("BANCO · 9"));
    let contas = s.contas().expect("contas");
    assert_eq!(contas.len(), 1);
    assert_eq!(contas[0].conta_do_extrato.as_deref(), Some("BANCO · 9"));
}

#[test]
fn assinatura_grava_le_tira_e_deixa_historico() {
    let mut s = servico(&Extrato::default());
    let id = criar(&mut s, "Salário", Natureza::Entrada, 5, Some(500_000), None);
    assert_eq!(s.assinatura(&id).expect("assinatura"), None);
    assinar(&mut s, &id, "SALARIO EMPRESA", 6);
    assert_eq!(
        s.assinatura(&id).expect("assinatura"),
        Some(Assinatura {
            texto: "SALARIO EMPRESA".into(),
            janela_dias: 6
        })
    );
    let b = base(&s, &id);
    s.aplicar(
        Alteracao::DefinirAssinatura {
            recorrente: id.clone(),
            assinatura: None,
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("tirar");
    assert_eq!(s.assinatura(&id).expect("assinatura"), None);
    // criar + definir + tirar.
    assert_eq!(s.historico(Some(&id), 10).expect("historico").len(), 3);
}

#[test]
fn assinatura_com_versao_velha_nao_grava_e_valida_texto_e_janela() {
    let mut s = servico(&Extrato::default());
    let id = criar(&mut s, "Salário", Natureza::Entrada, 5, Some(500_000), None);
    let b = base(&s, &id);
    s.aplicar(
        Alteracao::Editar {
            recorrente: id.clone(),
            escopo: sbf_planejamento::Escopo::DaquiEmDiante(comp(2026, 11)),
            mudanca: sbf_planejamento::MudancaDeVersao {
                valor: Some(Some(Centavos(510_000))),
                ..Default::default()
            },
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("editar");
    let tentativa = |texto: &str, janela: u8, versao_base: i64| {
        s.simular(
            &Alteracao::DefinirAssinatura {
                recorrente: id.clone(),
                assinatura: Some(Assinatura {
                    texto: texto.into(),
                    janela_dias: janela,
                }),
                versao_base,
            },
            HOJE,
        )
    };
    assert!(matches!(
        tentativa("salario", 5, b),
        Err(ErroDoPlanejamento::VersaoVelha { .. })
    ));
    let atual = base(&s, &id);
    assert!(matches!(
        tentativa("   ", 5, atual),
        Err(ErroDoPlanejamento::Invalida(_))
    ));
    assert!(matches!(
        tentativa("salario", 0, atual),
        Err(ErroDoPlanejamento::Invalida(_))
    ));
    assert!(matches!(
        tentativa("salario", 61, atual),
        Err(ErroDoPlanejamento::Invalida(_))
    ));
    assert!(tentativa("salario", 5, atual).is_ok());
    assert_eq!(
        s.assinatura(&id).expect("assinatura"),
        None,
        "simular não grava"
    );
}

#[test]
fn contas_do_extrato_sao_as_distintas_dos_ultimos_400_dias() {
    let extrato = Extrato::default();
    extrato.por("a", 5, -100, "BANCO · 2", "X");
    extrato.por("b", 6, -100, "BANCO · 1", "X");
    extrato.por("c", 7, -100, "BANCO · 1", "X");
    extrato.por("d", 8, -100, "", "X");
    let s = servico(&extrato);
    assert_eq!(
        s.contas_do_extrato().expect("contas"),
        ["BANCO · 1", "BANCO · 2"]
    );
}

// ---------------------------------------------------------------------------------------
// Conciliação
// ---------------------------------------------------------------------------------------

#[test]
fn buscar_concilia_por_valor_e_data_sem_par_a_conta_nao_pesa() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_a", 6, -100_000, "BANCO · 2", "IMOBILIARIA");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Aluguel",
        Natureza::Saida,
        5,
        Some(100_000),
        Some("CX-0001"),
    );
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    let c = sugestoes(&s, &id);
    assert_eq!(c.len(), 1);
    assert_eq!(c[0].movimento, "pluggy:txn_a");
    assert!(!c[0].confirmada);
}

#[test]
fn com_par_confirmado_a_conta_certa_e_exigida() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_errada", 5, -100_000, "BANCO · 2", "IMOBILIARIA");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Aluguel",
        Natureza::Saida,
        5,
        Some(100_000),
        Some("CX-0001"),
    );
    mapear(&mut s, "CX-0001", Some("BANCO · 1"));
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 0);
    assert!(sugestoes(&s, &id).is_empty());
    extrato.por("pluggy:txn_certa", 7, -100_000, "BANCO · 1", "IMOBILIARIA");
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    assert_eq!(sugestoes(&s, &id)[0].movimento, "pluggy:txn_certa");
}

#[test]
fn buscar_nao_duplica_nem_mexe_em_confirmada() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_a", 5, -100_000, "B", "IMOBILIARIA");
    extrato.por("pluggy:txn_l", 8, -10_000, "B", "ENERGIA");
    let mut s = servico(&extrato);
    let aluguel = criar(&mut s, "Aluguel", Natureza::Saida, 5, Some(100_000), None);
    let luz = criar(&mut s, "Luz", Natureza::Saida, 8, Some(10_000), None);
    // O aluguel já está confirmado com outro movimento.
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: Conciliacao {
                ocorrencia: ocorrencia(&aluguel),
                movimento: "pluggy:txn_a".into(),
                valor: Centavos(100_000),
                data: dia(5),
                confirmada: true,
            },
        },
        Autor::Usuario,
    )
    .expect("confirmar");
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    assert_eq!(
        s.buscar_conciliacoes(&novembro(), HOJE).expect("de novo"),
        0
    );
    assert_eq!(sugestoes(&s, &luz).len(), 1);
    let a = sugestoes(&s, &aluguel);
    assert_eq!(a.len(), 1);
    assert!(a[0].confirmada);
}

#[test]
fn movimento_ja_conciliado_a_outra_ocorrencia_nao_serve() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_1", 5, -10_000, "B", "X");
    let mut s = servico(&extrato);
    let a = criar(
        &mut s,
        "Assinatura A",
        Natureza::Saida,
        5,
        Some(10_000),
        None,
    );
    let b = criar(
        &mut s,
        "Assinatura B",
        Natureza::Saida,
        6,
        Some(10_000),
        None,
    );
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: Conciliacao {
                ocorrencia: ocorrencia(&a),
                movimento: "pluggy:txn_1".into(),
                valor: Centavos(10_000),
                data: dia(5),
                confirmada: true,
            },
        },
        Autor::Usuario,
    )
    .expect("confirmar");
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 0);
    assert!(sugestoes(&s, &b).is_empty());
    assert!(
        s.candidatos_de_conciliacao(&ocorrencia(&b))
            .expect("candidatos")
            .is_empty()
    );
}

#[test]
fn ambiguo_nao_sugere() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_1", 6, -3_990, "B", "STREAMING");
    let mut s = servico(&extrato);
    let a = criar(&mut s, "Streaming A", Natureza::Saida, 6, Some(3_990), None);
    let b = criar(&mut s, "Streaming B", Natureza::Saida, 6, Some(3_990), None);
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 0);
    assert!(sugestoes(&s, &a).is_empty() && sugestoes(&s, &b).is_empty());
}

fn definir_liquido(s: &mut PlanejamentoSqlite, id: &str, bruto: i64, desconto: i64) {
    let rubrica = |nome: &str, tipo, valor| Rubrica {
        codigo: None,
        nome: nome.into(),
        tipo,
        valor: Centavos(valor),
        competencia: None,
        prazo: None,
        recorrente: None,
    };
    let b = base(s, id);
    s.aplicar(
        Alteracao::DefinirComposicao {
            composicao: Composicao {
                recorrente: id.into(),
                desde: comp(2026, 11),
                rubricas: vec![
                    rubrica("Vencimento", TipoDeRubrica::Provento, bruto),
                    rubrica("Descontos", TipoDeRubrica::Desconto, desconto),
                ],
                documento: None,
                origem: "teste".into(),
            },
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("composicao");
}

#[test]
fn o_liquido_da_composicao_e_o_alvo_do_valor() {
    let extrato = Extrato::default();
    // O digitado é 7.000,00; o líquido do contracheque, 4.800,00.
    extrato.por("pluggy:txn_digitado", 5, 700_000, "B", "OUTRO");
    extrato.por("pluggy:txn_liquido", 6, 480_000, "B", "OUTRO");
    let mut s = servico(&extrato);
    let id = criar(&mut s, "Salário", Natureza::Entrada, 5, Some(700_000), None);
    definir_liquido(&mut s, &id, 600_000, 120_000);
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    assert_eq!(sugestoes(&s, &id)[0].movimento, "pluggy:txn_liquido");
}

#[test]
fn assinatura_acha_o_credito_com_valor_diferente_na_conta_certa() {
    let extrato = Extrato::default();
    // PLR: o valor não bate com nada, mas a descrição e a conta sim.
    extrato.por(
        "pluggy:txn_plr",
        7,
        912_345,
        "BANCO · 1",
        "SALARIO EMPRESA LTDA",
    );
    extrato.por(
        "pluggy:txn_outra",
        6,
        500_000,
        "BANCO · 2",
        "SALARIO EMPRESA LTDA",
    );
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(500_000),
        Some("CX-0001"),
    );
    mapear(&mut s, "CX-0001", Some("BANCO · 1"));
    assinar(&mut s, &id, "salário empresa", 5);
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    let c = sugestoes(&s, &id);
    assert_eq!(c[0].movimento, "pluggy:txn_plr");
    assert_eq!(c[0].valor, Centavos(912_345));
    assert!(!c[0].confirmada);
}

#[test]
fn candidatos_vem_ordenados_e_marcam_assinatura_conta_e_diferenca() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_a", 7, 912_345, "BANCO · 1", "SALARIO EMPRESA");
    extrato.por("pluggy:txn_b", 5, 500_000, "BANCO · 2", "OUTRA COISA");
    extrato.por("pluggy:txn_c", 6, 400_000, "BANCO · 1", "OUTRA COISA");
    extrato.por("pluggy:txn_saida", 5, -500_000, "BANCO · 1", "SAIDA");
    extrato.por(
        "pluggy:txn_longe",
        28,
        500_000,
        "BANCO · 1",
        "SALARIO EMPRESA",
    );
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(500_000),
        Some("CX-0001"),
    );
    mapear(&mut s, "CX-0001", Some("BANCO · 1"));
    assinar(&mut s, &id, "SALARIO EMPRESA", 5);
    let c = s
        .candidatos_de_conciliacao(&ocorrencia(&id))
        .expect("candidatos");
    let ids: Vec<&str> = c.iter().map(|x| x.movimento.movimento.as_str()).collect();
    assert_eq!(ids, ["pluggy:txn_a", "pluggy:txn_c", "pluggy:txn_b"]);
    assert!(c[0].bate_assinatura && c[0].mesma_conta);
    assert_eq!(c[0].diferenca, Some(Centavos(412_345)));
    assert!(!c[1].bate_assinatura && c[1].mesma_conta);
    assert!(!c[2].mesma_conta);
    assert_eq!(c[2].diferenca, Some(Centavos(0)));
}

#[test]
fn candidatos_de_ocorrencia_que_nao_existe_e_erro() {
    let s = servico(&Extrato::default());
    assert!(
        s.candidatos_de_conciliacao(&ocorrencia("rec_nao_existe"))
            .is_err()
    );
}

// ---------------------------------------------------------------------------------------
// Pares de conta sugeridos
// ---------------------------------------------------------------------------------------

#[test]
fn pares_sugeridos_vem_do_historico_com_evidencia_e_motivo() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_1", 5, -100_000, "BANCO · 1", "IMOBILIARIA");
    extrato.por("pluggy:txn_2", 8, -10_000, "BANCO · 1", "ENERGIA");
    extrato.por("pluggy:txn_3", 9, -5_000, "BANCO · 2", "NET");
    let mut s = servico(&extrato);
    let aluguel = criar(
        &mut s,
        "Aluguel",
        Natureza::Saida,
        5,
        Some(100_000),
        Some("CX-0001"),
    );
    let luz = criar(
        &mut s,
        "Luz",
        Natureza::Saida,
        8,
        Some(10_000),
        Some("CX-0001"),
    );
    let net = criar(
        &mut s,
        "Internet",
        Natureza::Saida,
        9,
        Some(5_000),
        Some("Nubank"),
    );
    let conciliar = |s: &mut PlanejamentoSqlite, id: &str, mov: &str, v: i64, d: u8, ok: bool| {
        s.aplicar(
            Alteracao::Conciliar {
                conciliacao: Conciliacao {
                    ocorrencia: ocorrencia(id),
                    movimento: mov.into(),
                    valor: Centavos(v),
                    data: dia(d),
                    confirmada: ok,
                },
            },
            Autor::Usuario,
        )
        .expect("conciliar");
    };
    conciliar(&mut s, &aluguel, "pluggy:txn_1", 100_000, 5, true);
    conciliar(&mut s, &luz, "pluggy:txn_2", 10_000, 8, false);
    conciliar(&mut s, &net, "pluggy:txn_3", 5_000, 9, false);
    let pares = s.pares_de_conta_sugeridos().expect("pares");
    assert_eq!(pares.len(), 2);
    assert_eq!(pares[0].codigo, "CX-0001");
    assert_eq!(pares[0].conta_do_extrato, "BANCO · 1");
    assert_eq!(pares[0].evidencias, 2);
    assert_eq!(
        pares[0].motivo,
        "1 conciliação confirmada e 1 conciliação sugerida"
    );
    assert_eq!(pares[1].codigo, "Nubank");
    assert_eq!(pares[1].conta_do_extrato, "BANCO · 2");

    // Com o par confirmado, o código sai da lista.
    mapear(&mut s, "CX-0001", Some("BANCO · 1"));
    let pares = s.pares_de_conta_sugeridos().expect("pares");
    assert_eq!(pares.len(), 1);
    assert_eq!(pares[0].codigo, "Nubank");
}

#[test]
fn pares_sugeridos_contam_os_creditos_da_assinatura() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_s10", 5, 500_000, "BANCO · 1", "SALARIO EMPRESA");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(500_000),
        Some("CX-0001"),
    );
    // Sem assinatura, nada indica a conta.
    assert!(s.pares_de_conta_sugeridos().expect("pares").is_empty());
    assinar(&mut s, &id, "salario empresa", 5);
    let pares = s.pares_de_conta_sugeridos().expect("pares");
    assert_eq!(pares.len(), 1);
    assert_eq!(pares[0].conta_do_extrato, "BANCO · 1");
    assert_eq!(pares[0].evidencias, 1);
    assert_eq!(pares[0].motivo, "1 crédito da assinatura");
}

#[test]
fn sem_extrato_nada_e_sugerido() {
    let mut s = PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
        .com_relogio(|| RELOGIO);
    criar(
        &mut s,
        "Aluguel",
        Natureza::Saida,
        5,
        Some(100_000),
        Some("CX"),
    );
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 0);
    assert!(s.pares_de_conta_sugeridos().expect("pares").is_empty());
    assert!(s.contas_do_extrato().expect("contas").is_empty());
}

#[test]
fn conciliacoes_do_movimento_dizem_a_que_ocorrencia_ele_esta_ligado() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_s", 20, 822_600, "BANCO · 1", "SALARIO EMPRESA");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        20,
        Some(845_000),
        Some("CX-0001"),
    );
    assert!(
        s.conciliacoes_do_movimento("pluggy:txn_s")
            .expect("vazio")
            .is_empty()
    );
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: Conciliacao {
                ocorrencia: ocorrencia(&id),
                movimento: "pluggy:txn_s".into(),
                valor: Centavos(822_600),
                data: dia(20),
                confirmada: true,
            },
        },
        Autor::Usuario,
    )
    .expect("confirmar");
    let v = s.conciliacoes_do_movimento("pluggy:txn_s").expect("lista");
    assert_eq!(v.len(), 1);
    assert_eq!(v[0].ocorrencia, ocorrencia(&id));
    assert!(
        s.conciliacoes_do_movimento("pluggy:outro")
            .expect("vazio")
            .is_empty()
    );
    // Desfeita, deixa de ligar.
    s.aplicar(
        Alteracao::Desconciliar {
            ocorrencia: ocorrencia(&id),
            movimento: "pluggy:txn_s".into(),
        },
        Autor::Usuario,
    )
    .expect("desfazer");
    assert!(
        s.conciliacoes_do_movimento("pluggy:txn_s")
            .expect("vazio")
            .is_empty()
    );
}

#[test]
fn conciliacoes_da_ocorrencia_trazem_o_movimento_e_a_conta() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_s", 20, 822_600, "BANCO · 1", "SALARIO EMPRESA");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        20,
        Some(845_000),
        Some("CX-0001"),
    );
    assert!(
        s.conciliacoes_da_ocorrencia(&ocorrencia(&id))
            .expect("vazio")
            .is_empty()
    );
    mapear(&mut s, "CX-0001", Some("BANCO · 1"));
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: Conciliacao {
                ocorrencia: ocorrencia(&id),
                movimento: "pluggy:txn_s".into(),
                valor: Centavos(822_600),
                data: dia(20),
                confirmada: true,
            },
        },
        Autor::Usuario,
    )
    .expect("confirmar");
    let v = s
        .conciliacoes_da_ocorrencia(&ocorrencia(&id))
        .expect("conciliações");
    assert_eq!(v.len(), 1);
    assert!(v[0].conciliacao.confirmada);
    assert!(v[0].mesma_conta);
    let m = v[0].movimento.as_ref().expect("movimento");
    assert_eq!(m.descricao, "SALARIO EMPRESA");
    // Sem par, a conta não bate (mas o movimento continua achado).
    mapear(&mut s, "CX-0001", None);
    let v = s
        .conciliacoes_da_ocorrencia(&ocorrencia(&id))
        .expect("conciliações");
    assert!(!v[0].mesma_conta);
    assert!(v[0].movimento.is_some());
}
