//! Critérios de aceite do controle mensal que o motor e o banco comprovam (spec 11).
//! Todos os dados são fictícios.

use std::collections::BTreeSet;
use std::time::Instant;

use sbf_planejamento::{
    Alteracao, Autor, Banco, Centavos, Certeza, Ciclo, Competencia, Conciliacao, Data, DiaDeCorte,
    ErroDoPlanejamento, Escopo, EstadoDaOcorrencia, EstadoDaProposta, Excecao, Filtro, Fonte,
    FonteDeSaldos, Frequencia, HORIZONTE_MESES, IdOcorrencia, MudancaDeVersao, Natureza,
    NovoRecorrente, Ocorrencia, OrigemImportada, Periodo, Planejamento, PlanejamentoSqlite, Prazo,
    Saldo, SemSaldos, Sentido, Versao,
};

const HOJE: Data = Data {
    ano: 2026,
    mes: 11,
    dia: 16,
};

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

fn mes(ano: i32, m: u8) -> Periodo {
    Periodo::MesCivil(comp(ano, m))
}

struct SaldoFixo(Vec<Saldo>);

impl FonteDeSaldos for SaldoFixo {
    fn saldos(&self) -> Vec<Saldo> {
        self.0.clone()
    }
}

fn saldo(valor: i64, quando: &str) -> Saldo {
    Saldo {
        conta: "CX-0001".into(),
        valor: Centavos(valor),
        quando: quando.into(),
    }
}

fn servico() -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
}

fn servico_com_saldo(saldos: Vec<Saldo>) -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(
        Banco::em_memoria().expect("banco"),
        Box::new(SaldoFixo(saldos)),
    )
}

#[allow(clippy::too_many_arguments)]
fn novo(
    nome: &str,
    grupo: &str,
    posicao: i64,
    natureza: Natureza,
    prazo: Prazo,
    valor: Option<i64>,
    certeza: Certeza,
    frequencia: Frequencia,
) -> NovoRecorrente {
    NovoRecorrente {
        nome: nome.into(),
        grupo: grupo.into(),
        subgrupo: None,
        posicao,
        natureza,
        prazo,
        primeira_versao: Versao {
            id: String::new(),
            desde: comp(2026, 10),
            valor: valor.map(Centavos),
            certeza,
            fonte: Fonte::Usuario,
            frequencia,
            conta: Some("CX-0001".into()),
            centro: None,
            fornecedor: None,
            categoria: None,
            nota: None,
        },
    }
}

fn streaming() -> NovoRecorrente {
    novo(
        "Streaming Exemplo",
        "Assinaturas",
        1,
        Natureza::Saida,
        Prazo::SemFim,
        Some(3500),
        Certeza::Confirmado,
        Frequencia::Mensal { dia: 5 },
    )
}

fn energia() -> NovoRecorrente {
    novo(
        "Energia Exemplo",
        "Essencial",
        1,
        Natureza::Saida,
        Prazo::SemFim,
        Some(21_000),
        Certeza::Estimado,
        Frequencia::Mensal { dia: 31 },
    )
}

fn emprestimo() -> NovoRecorrente {
    novo(
        "Empréstimo Exemplo",
        "Empréstimos",
        1,
        Natureza::Saida,
        Prazo::Parcelas {
            total: 48,
            primeira: comp(2026, 11),
        },
        Some(120_000),
        Certeza::Confirmado,
        Frequencia::Mensal { dia: 10 },
    )
}

fn salario(valor: i64) -> NovoRecorrente {
    novo(
        "Salário Exemplo",
        "Entradas",
        1,
        Natureza::Entrada,
        Prazo::SemFim,
        Some(valor),
        Certeza::Confirmado,
        Frequencia::Mensal { dia: 5 },
    )
}

fn consignacao() -> NovoRecorrente {
    novo(
        "Consignação Exemplo",
        "Essencial",
        2,
        Natureza::DescontoEmFolha,
        Prazo::SemFim,
        Some(25_000),
        Certeza::Confirmado,
        Frequencia::Mensal { dia: 5 },
    )
}

fn bolao(sentido: Sentido, valor: i64) -> NovoRecorrente {
    novo(
        "Bolão Exemplo",
        "Passagem",
        1,
        Natureza::Passagem { sentido },
        Prazo::SemFim,
        Some(valor),
        Certeza::Confirmado,
        Frequencia::Mensal { dia: 20 },
    )
}

fn desde(mut n: NovoRecorrente, c: Competencia) -> NovoRecorrente {
    n.primeira_versao.desde = c;
    n
}

fn criar(s: &mut PlanejamentoSqlite, n: NovoRecorrente) -> String {
    s.aplicar(
        Alteracao::CriarRecorrente {
            recorrente: n,
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

fn ocorrencia(
    s: &PlanejamentoSqlite,
    id: &str,
    ano: i32,
    m: u8,
    filtro: Filtro,
) -> Option<Ocorrencia> {
    s.controle(&mes(ano, m), filtro, None, HOJE)
        .expect("controle")
        .linhas
        .into_iter()
        .find(|o| o.id.recorrente == id)
}

fn pago_informado(s: &mut PlanejamentoSqlite, id: &str, ano: i32, m: u8, valor: i64) {
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: IdOcorrencia {
                recorrente: id.into(),
                competencia: comp(ano, m),
                parcela: None,
            },
            excecao: Excecao::PagoInformado {
                valor: Centavos(valor),
                data: None,
            },
        },
        Autor::Usuario,
    )
    .expect("pago informado");
}

fn pausar(s: &mut PlanejamentoSqlite, id: &str, desde: Competencia, ate: Option<Competencia>) {
    let b = base(s, id);
    s.aplicar(
        Alteracao::Pausar {
            recorrente: id.into(),
            desde,
            ate,
            motivo: "teste".into(),
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("pausar");
}

fn contagens(s: &PlanejamentoSqlite) -> Vec<i64> {
    [
        "grupos",
        "recorrentes",
        "versoes",
        "pausas",
        "cancelamentos",
        "excecoes",
        "conciliacoes",
        "propostas",
        "origens_importadas",
        "historico",
    ]
    .iter()
    .map(|t| {
        s.banco
            .conexao()
            .query_row(&format!("SELECT count(*) FROM {t}"), [], |l| l.get(0))
            .expect("contar")
    })
    .collect()
}

fn dia(ano: i32, m: u8, d: u8) -> Data {
    Data::nova(ano, m, d).expect("data")
}

// ---------------------------------------------------------------------------------------
// AC-15, AC-16, AC-34: horizonte e desempenho
// ---------------------------------------------------------------------------------------

#[test]
fn ac_15_trezentos_e_sessenta_meses_consecutivos_sem_faltar_mes() {
    let mut s = servico();
    criar(&mut s, streaming());
    let calendario = s
        .calendario(comp(2026, 11), HORIZONTE_MESES, HOJE)
        .expect("calendário");
    assert_eq!(calendario.len(), 360);
    for (i, mes) in calendario.iter().enumerate() {
        assert_eq!(mes.competencia, comp(2026, 11).somar_meses(i as i32));
        assert_eq!(
            mes.saidas_de_caixa,
            Centavos(3500),
            "mês {}",
            mes.competencia
        );
    }
    assert_eq!(calendario[359].competencia, comp(2056, 10));
    assert!(matches!(
        s.calendario(comp(2026, 11), 361, HOJE),
        Err(ErroDoPlanejamento::Invalida(_))
    ));
}

#[test]
fn ac_16_janeiro_de_2050_sem_materializar_nada() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    criar(&mut s, emprestimo());
    criar(&mut s, energia());
    let antes = contagens(&s);
    let inicio = Instant::now();
    let c = s
        .controle(&mes(2050, 1), Filtro::Todos, None, HOJE)
        .expect("controle");
    eprintln!("AC-16: controle de 2050-01 em {:?}", inicio.elapsed());
    assert_eq!(contagens(&s), antes, "ler não grava nada");
    let streaming = c
        .linhas
        .iter()
        .find(|o| o.id.recorrente == id)
        .expect("linha");
    assert_eq!(streaming.id.to_string(), format!("{id}:2050-01"));
    assert_eq!(streaming.estado, EstadoDaOcorrencia::Prevista);
    // O empréstimo de 48 parcelas já acabou em 2050; a energia (prazo aberto) continua.
    assert_eq!(c.linhas.len(), 2);
}

#[test]
fn ac_34_calendario_de_360_meses_com_100_regras_tem_a_latencia_medida() {
    let mut s = servico();
    for i in 0..100_i64 {
        let n = match i % 4 {
            0 => novo(
                &format!("Regra {i}"),
                "Grupo A",
                i,
                Natureza::Saida,
                Prazo::SemFim,
                Some(1000 + i),
                Certeza::Confirmado,
                Frequencia::Mensal {
                    dia: 1 + (i % 31) as u8,
                },
            ),
            1 => novo(
                &format!("Regra {i}"),
                "Grupo B",
                i,
                Natureza::Saida,
                Prazo::Parcelas {
                    total: 48,
                    primeira: comp(2026, 11),
                },
                Some(2000 + i),
                Certeza::Confirmado,
                Frequencia::Mensal { dia: 10 },
            ),
            2 => novo(
                &format!("Regra {i}"),
                "Grupo C",
                i,
                Natureza::Entrada,
                Prazo::Desconhecido,
                Some(5000),
                Certeza::Estimado,
                Frequencia::CadaNMeses {
                    n: 3,
                    dia: 15,
                    primeira: comp(2026, 11),
                },
            ),
            _ => novo(
                &format!("Regra {i}"),
                "Grupo D",
                i,
                Natureza::DescontoEmFolha,
                Prazo::SemFim,
                None,
                Certeza::Estimado,
                Frequencia::Anual { mes: 12, dia: 31 },
            ),
        };
        criar(&mut s, n);
    }
    let inicio = Instant::now();
    let calendario = s.calendario(comp(2026, 11), 360, HOJE).expect("calendário");
    let tempo = inicio.elapsed();
    eprintln!("AC-34: calendario(2026-11, 360) com 100 regras em {tempo:?}");
    assert_eq!(calendario.len(), 360);
    assert!(calendario.iter().any(|m| m.sem_valor > 0));

    let inicio = Instant::now();
    s.controle(&mes(2030, 6), Filtro::Todos, None, HOJE)
        .expect("controle");
    eprintln!(
        "AC-34: controle de um mês com 100 regras em {:?}",
        inicio.elapsed()
    );
}

// ---------------------------------------------------------------------------------------
// AC-17, AC-18, AC-19, AC-26: pausa, retomada e cancelamento
// ---------------------------------------------------------------------------------------

#[test]
fn ac_17_pausa_em_dezembro_tira_dezembro_e_janeiro_e_preserva_novembro() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pausar(&mut s, &id, comp(2026, 12), Some(comp(2027, 2)));
    assert!(
        ocorrencia(&s, &id, 2026, 11, Filtro::Todos).is_some(),
        "novembro preservado"
    );
    assert!(ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_none());
    assert!(ocorrencia(&s, &id, 2027, 1, Filtro::Todos).is_none());
    assert!(
        ocorrencia(&s, &id, 2027, 2, Filtro::Todos).is_some(),
        "fevereiro volta"
    );
    let suprimida = ocorrencia(&s, &id, 2026, 12, Filtro::PausadosECancelados).expect("suprimida");
    assert_eq!(suprimida.estado, EstadoDaOcorrencia::SuprimidaPorPausa);
    // A pausa não entra nos totais do mês.
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert_eq!(c.resumo.saidas_de_caixa_previstas, Centavos(0));
}

#[test]
fn ac_18_retomada_em_marco_volta_marco_e_mantem_dezembro_a_fevereiro_suprimidos() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pausar(&mut s, &id, comp(2026, 12), None);
    let b = base(&s, &id);
    s.aplicar(
        Alteracao::Retomar {
            recorrente: id.clone(),
            desde: comp(2027, 3),
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("retomar");
    for (a, m) in [(2026, 12), (2027, 1), (2027, 2)] {
        assert!(
            ocorrencia(&s, &id, a, m, Filtro::Todos).is_none(),
            "{a}-{m}"
        );
        assert_eq!(
            ocorrencia(&s, &id, a, m, Filtro::PausadosECancelados)
                .expect("suprimida")
                .estado,
            EstadoDaOcorrencia::SuprimidaPorPausa
        );
    }
    assert!(
        ocorrencia(&s, &id, 2027, 3, Filtro::Todos).is_some(),
        "março volta"
    );
    assert!(ocorrencia(&s, &id, 2027, 4, Filtro::Todos).is_some());
    // Sem pausa aberta, retomar de novo é recusado.
    let b = base(&s, &id);
    assert!(matches!(
        s.aplicar(
            Alteracao::Retomar {
                recorrente: id,
                desde: comp(2027, 6),
                versao_base: b
            },
            Autor::Usuario
        ),
        Err(ErroDoPlanejamento::Invalida(_))
    ));
}

#[test]
fn ac_19_cancelamento_em_fevereiro_nao_gera_nada_depois_e_o_ja_devido_continua() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pago_informado(&mut s, &id, 2027, 2, 3500);
    let b = base(&s, &id);
    s.aplicar(
        Alteracao::Cancelar {
            recorrente: id.clone(),
            desde: comp(2027, 2),
            motivo: "fim do contrato".into(),
            comprovado: false,
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("cancelar");
    assert!(
        ocorrencia(&s, &id, 2027, 1, Filtro::Todos).is_some(),
        "janeiro não muda"
    );
    // Fevereiro tinha pago informado: continua visível.
    assert_eq!(
        ocorrencia(&s, &id, 2027, 2, Filtro::Todos)
            .expect("fevereiro")
            .estado,
        EstadoDaOcorrencia::PagoInformado
    );
    // Nada novo depois, até o fim do horizonte.
    let depois = s.proximas(&id, comp(2027, 3), 360, HOJE).expect("próximas");
    assert!(depois.is_empty());
    assert_eq!(
        ocorrencia(&s, &id, 2027, 3, Filtro::PausadosECancelados)
            .expect("suprimida")
            .estado,
        EstadoDaOcorrencia::SuprimidaPorCancelamento
    );
    // O que já estava em atraso antes continua devido.
    let c = s
        .controle(&mes(2026, 11), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert_eq!(c.atrasadas_de_antes.len(), 1);
    assert_eq!(c.atrasadas_de_antes[0].id.competencia, comp(2026, 10));
    assert_eq!(c.resumo.em_atraso, Centavos(3500 * 2), "outubro e novembro");
}

#[test]
fn ac_26_pausa_ou_cancelamento_nao_apaga_cobranca_devida_nem_conciliacao() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pago_informado(&mut s, &id, 2026, 12, 3500);
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: Conciliacao {
                ocorrencia: IdOcorrencia {
                    recorrente: id.clone(),
                    competencia: comp(2027, 1),
                    parcela: None,
                },
                movimento: "pluggy:txn_exemplo_1".into(),
                valor: Centavos(3500),
                data: dia(2027, 1, 5),
                confirmada: true,
            },
        },
        Autor::Usuario,
    )
    .expect("conciliar");
    pausar(&mut s, &id, comp(2026, 12), None);
    let dez = ocorrencia(&s, &id, 2026, 12, Filtro::Todos).expect("dezembro segue visível");
    assert_eq!(dez.estado, EstadoDaOcorrencia::PagoInformado);
    let jan = ocorrencia(&s, &id, 2027, 1, Filtro::Todos).expect("janeiro segue visível");
    assert_eq!(jan.estado, EstadoDaOcorrencia::Conciliada);
    assert!(
        ocorrencia(&s, &id, 2027, 2, Filtro::Todos).is_none(),
        "o resto some"
    );

    let b = base(&s, &id);
    s.aplicar(
        Alteracao::Cancelar {
            recorrente: id.clone(),
            desde: comp(2026, 12),
            motivo: "teste".into(),
            comprovado: true,
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("cancelar");
    assert!(ocorrencia(&s, &id, 2027, 1, Filtro::Todos).is_some());
    assert_eq!(
        s.conciliacoes(&id).expect("conciliações").len(),
        1,
        "nada foi apagado"
    );
}

// ---------------------------------------------------------------------------------------
// AC-20: exclusão lógica
// ---------------------------------------------------------------------------------------

#[test]
fn ac_20_exclusao_logica_com_motivo_autor_e_hora_e_restauravel() {
    let agora = dia(2026, 11, 16).dias_desde_1970() * 86_400 + 12 * 3600;
    let mut s = servico().com_relogio(move || agora);
    let id = criar(&mut s, streaming());
    let b = base(&s, &id);
    s.aplicar(
        Alteracao::Excluir {
            recorrente: id.clone(),
            motivo: "criado por engano".into(),
            versao_base: b,
        },
        Autor::Ia {
            modelo: "modelo-exemplo".into(),
        },
    )
    .expect("excluir");
    let rec = s.recorrente(&id).expect("ler").expect("segue no banco");
    let exclusao = rec.excluido.expect("excluído");
    assert_eq!(exclusao.motivo, "criado por engano");
    assert_eq!(exclusao.quando, "2026-11-16T12:00:00Z");
    assert!(matches!(exclusao.autor, Autor::Ia { .. }));
    assert!(s.recorrentes(false).expect("lista").is_empty());
    assert_eq!(s.recorrentes(true).expect("lista").len(), 1);
    assert!(ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_none());
    // Restaurar com base velha é recusado.
    let b = base(&s, &id);
    assert!(
        s.aplicar(
            Alteracao::Restaurar {
                recorrente: id.clone(),
                versao_base: b + 5
            },
            Autor::Usuario
        )
        .is_err()
    );
    s.aplicar(
        Alteracao::Restaurar {
            recorrente: id.clone(),
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("restaurar");
    assert!(
        s.recorrente(&id)
            .expect("ler")
            .expect("existe")
            .excluido
            .is_none()
    );
    assert!(ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_some());
    assert_eq!(s.historico(Some(&id), 10).expect("histórico").len(), 3);
}

#[test]
fn excluir_sem_motivo_e_recusado() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let b = base(&s, &id);
    let erro = s
        .aplicar(
            Alteracao::Excluir {
                recorrente: id,
                motivo: "  ".into(),
                versao_base: b,
            },
            Autor::Usuario,
        )
        .expect_err("recusa");
    assert!(matches!(erro, ErroDoPlanejamento::Invalida(_)));
}

// ---------------------------------------------------------------------------------------
// AC-21, AC-22: edição e reprojeção
// ---------------------------------------------------------------------------------------

#[test]
fn ac_21_editar_so_este_mes_ou_daqui_em_diante_sem_duplicar() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let valor = |s: &PlanejamentoSqlite, a, m| {
        ocorrencia(s, &id, a, m, Filtro::Todos)
            .expect("linha")
            .previsto
    };

    let b = base(&s, &id);
    s.aplicar(
        Alteracao::Editar {
            recorrente: id.clone(),
            escopo: Escopo::SoEsteMes(comp(2027, 1)),
            mudanca: MudancaDeVersao {
                valor: Some(Some(Centavos(5000))),
                ..Default::default()
            },
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("só este mês");
    assert_eq!(valor(&s, 2026, 12), Some(Centavos(3500)));
    assert_eq!(valor(&s, 2027, 1), Some(Centavos(5000)));
    assert_eq!(valor(&s, 2027, 2), Some(Centavos(3500)));
    assert_eq!(
        s.recorrente(&id)
            .expect("ler")
            .expect("existe")
            .versoes
            .len(),
        1
    );

    // "Só este mês" só aceita o valor.
    let b = base(&s, &id);
    assert!(matches!(
        s.aplicar(
            Alteracao::Editar {
                recorrente: id.clone(),
                escopo: Escopo::SoEsteMes(comp(2027, 1)),
                mudanca: MudancaDeVersao {
                    conta: Some(Some("Nubank".into())),
                    ..Default::default()
                },
                versao_base: b,
            },
            Autor::Usuario
        ),
        Err(ErroDoPlanejamento::Invalida(_))
    ));

    // Daqui em diante: versão nova; as anteriores seguem iguais.
    let daqui = |s: &mut PlanejamentoSqlite, valor: i64| {
        let b = base(s, &id);
        s.aplicar(
            Alteracao::Editar {
                recorrente: id.clone(),
                escopo: Escopo::DaquiEmDiante(comp(2027, 3)),
                mudanca: MudancaDeVersao {
                    valor: Some(Some(Centavos(valor))),
                    ..Default::default()
                },
                versao_base: b,
            },
            Autor::Usuario,
        )
        .expect("daqui em diante");
    };
    daqui(&mut s, 4000);
    daqui(&mut s, 4200); // mesma data de início: substitui só ela
    let rec = s.recorrente(&id).expect("ler").expect("existe");
    assert_eq!(rec.versoes.len(), 2, "sem duplicar a versão");
    assert_eq!(valor(&s, 2027, 2), Some(Centavos(3500)));
    assert_eq!(valor(&s, 2027, 3), Some(Centavos(4200)));
    assert_eq!(valor(&s, 2030, 1), Some(Centavos(4200)));
    assert_eq!(
        valor(&s, 2027, 1),
        Some(Centavos(5000)),
        "a exceção do mês segue valendo"
    );

    let proximas = s.proximas(&id, comp(2026, 11), 48, HOJE).expect("próximas");
    let ids: BTreeSet<_> = proximas.iter().map(|o| o.id.to_string()).collect();
    assert_eq!(ids.len(), proximas.len(), "nenhuma ocorrência duplicada");
}

#[test]
fn ac_22_reprojetar_da_os_mesmos_ids_e_totais() {
    let mut s = servico_com_saldo(vec![saldo(100_000, "2026-11-16T09:00:00Z")]);
    let id = criar(&mut s, streaming());
    criar(&mut s, energia());
    criar(&mut s, emprestimo());
    let a = s
        .controle(&mes(2027, 3), Filtro::Todos, None, HOJE)
        .expect("controle");
    let b = s
        .controle(&mes(2027, 3), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert_eq!(a, b);
    assert_eq!(
        s.calendario(comp(2026, 11), 360, HOJE).expect("a"),
        s.calendario(comp(2026, 11), 360, HOJE).expect("b")
    );
    // Trocar a versão não muda a identidade das ocorrências.
    let ids_antes: Vec<_> = a.linhas.iter().map(|o| o.id.clone()).collect();
    let base_atual = base(&s, &id);
    s.aplicar(
        Alteracao::Editar {
            recorrente: id,
            escopo: Escopo::DaquiEmDiante(comp(2027, 1)),
            mudanca: MudancaDeVersao {
                valor: Some(Some(Centavos(9900))),
                ..Default::default()
            },
            versao_base: base_atual,
        },
        Autor::Usuario,
    )
    .expect("editar");
    let c = s
        .controle(&mes(2027, 3), Filtro::Todos, None, HOJE)
        .expect("controle");
    let ids_depois: Vec<_> = c.linhas.iter().map(|o| o.id.clone()).collect();
    assert_eq!(ids_antes, ids_depois);
    assert_ne!(
        a.resumo.saidas_de_caixa_previstas,
        c.resumo.saidas_de_caixa_previstas
    );
}

// ---------------------------------------------------------------------------------------
// AC-23, AC-24, AC-25: datas, parcelas e pago por competência
// ---------------------------------------------------------------------------------------

#[test]
fn ac_23_dia_31_bissexto_trimestral_anual_e_virada_de_ano() {
    let mut s = servico();
    let energia = criar(&mut s, energia());
    let trimestral = criar(
        &mut s,
        novo(
            "Seguro Exemplo",
            "Essencial",
            3,
            Natureza::Saida,
            Prazo::SemFim,
            Some(10_000),
            Certeza::Confirmado,
            Frequencia::CadaNMeses {
                n: 3,
                dia: 10,
                primeira: comp(2026, 11),
            },
        ),
    );
    let anual = criar(
        &mut s,
        novo(
            "Anuidade Exemplo",
            "Essencial",
            4,
            Natureza::Saida,
            Prazo::SemFim,
            Some(60_000),
            Certeza::Confirmado,
            Frequencia::Anual { mes: 12, dia: 15 },
        ),
    );
    let e = s
        .proximas(&energia, comp(2027, 1), 30, HOJE)
        .expect("energia");
    let dia_de = |a, m| {
        e.iter()
            .find(|o| o.id.competencia == comp(a, m))
            .map(|o| o.vencimento.dia)
    };
    assert_eq!(dia_de(2027, 2), Some(28));
    assert_eq!(dia_de(2028, 2), Some(29), "fevereiro bissexto");
    assert_eq!(dia_de(2027, 4), Some(30));
    assert_eq!(dia_de(2027, 12), Some(31));

    let t = s
        .proximas(&trimestral, comp(2026, 11), 16, HOJE)
        .expect("trimestral");
    let comps: Vec<_> = t.iter().map(|o| o.id.competencia).collect();
    assert_eq!(
        comps,
        [
            comp(2026, 11),
            comp(2027, 2),
            comp(2027, 5),
            comp(2027, 8),
            comp(2027, 11),
            comp(2028, 2)
        ]
    );

    let a = s.proximas(&anual, comp(2026, 11), 36, HOJE).expect("anual");
    let comps: Vec<_> = a.iter().map(|o| o.id.competencia).collect();
    assert_eq!(comps, [comp(2026, 12), comp(2027, 12), comp(2028, 12)]);
}

#[test]
fn ac_24_quarenta_e_oito_parcelas_geram_quarenta_e_oito_ocorrencias() {
    let mut s = servico();
    let id = criar(&mut s, emprestimo());
    let todas = s.proximas(&id, comp(2026, 1), 360, HOJE).expect("próximas");
    assert_eq!(todas.len(), 48);
    assert_eq!(todas[0].parcela, Some((1, 48)));
    assert_eq!(todas[47].parcela, Some((48, 48)));
    assert_eq!(todas[47].id.to_string(), format!("{id}:2030-10:p48"));
    // O prazo do contrato vence o horizonte: depois da última, nada.
    assert!(
        s.proximas(&id, comp(2030, 11), 300, HOJE)
            .expect("depois")
            .is_empty()
    );
    // Pago informado de uma parcela leva o número da parcela.
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: IdOcorrencia::de_texto(&format!("{id}:2026-11:p1")).expect("id"),
            excecao: Excecao::PagoInformado {
                valor: Centavos(120_000),
                data: None,
            },
        },
        Autor::Usuario,
    )
    .expect("pago");
    let p1 = ocorrencia(&s, &id, 2026, 11, Filtro::Todos).expect("parcela 1");
    assert_eq!(p1.estado, EstadoDaOcorrencia::PagoInformado);
}

#[test]
fn ac_25_pago_em_outubro_nao_aparece_pago_em_novembro() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pago_informado(&mut s, &id, 2026, 10, 3500);
    let out = ocorrencia(&s, &id, 2026, 10, Filtro::Todos).expect("outubro");
    assert_eq!(out.estado, EstadoDaOcorrencia::PagoInformado);
    assert_eq!(out.pago_informado, Some(Centavos(3500)));
    for (a, m) in [(2026, 11), (2026, 12), (2030, 5)] {
        let o = ocorrencia(&s, &id, a, m, Filtro::Todos).expect("mês");
        assert_eq!(o.pago_informado, None, "{a}-{m}");
        assert_ne!(o.estado, EstadoDaOcorrencia::PagoInformado, "{a}-{m}");
    }
}

// ---------------------------------------------------------------------------------------
// AC-27, AC-33: resumo
// ---------------------------------------------------------------------------------------

#[test]
fn ac_27_o_livre_nao_desconta_de_novo_o_desconto_em_folha() {
    let mut s = servico_com_saldo(vec![saldo(100_000, "2026-11-16T09:00:00Z")]);
    // Tudo a partir de dezembro: nada de antes ficou em atraso.
    criar(&mut s, desde(salario(500_000), comp(2026, 12)));
    criar(&mut s, desde(consignacao(), comp(2026, 12)));
    criar(&mut s, desde(streaming(), comp(2026, 12)));
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    let r = &c.resumo;
    assert_eq!(r.entradas_previstas, Centavos(500_000));
    assert_eq!(
        r.saidas_de_caixa_previstas,
        Centavos(3500),
        "só o streaming sai do caixa"
    );
    assert_eq!(r.descontos_em_folha, Centavos(25_000));
    assert_eq!(
        r.livre_apos_compromissos,
        Centavos(100_000 + 500_000 - 3500)
    );
    assert!(!r.livre_e_estimativa);
    assert!(r.lacunas.is_empty());
    // A consignação aparece como linha, mas não vira saída de caixa.
    assert!(
        c.linhas
            .iter()
            .any(|o| o.natureza == Natureza::DescontoEmFolha)
    );
}

#[test]
fn passagem_fica_fora_de_receita_e_despesa_e_o_pendente_desconta_do_livre() {
    let mut s = servico_com_saldo(vec![saldo(100_000, "2026-11-16T09:00:00Z")]);
    criar(&mut s, bolao(Sentido::Entrada, 30_000));
    let saida = criar(&mut s, bolao(Sentido::Saida, 10_000));
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert_eq!(c.resumo.entradas_previstas, Centavos(0));
    assert_eq!(c.resumo.saidas_de_caixa_previstas, Centavos(0));
    assert_eq!(c.resumo.passagem_pendente, Centavos(20_000));
    assert_eq!(c.resumo.livre_apos_compromissos, Centavos(100_000 - 20_000));
    let cal = s.calendario(comp(2026, 12), 1, HOJE).expect("calendário");
    assert_eq!(cal[0].entradas, Centavos(0));
    assert_eq!(cal[0].saidas_de_caixa, Centavos(0));
    let _ = saida;
}

#[test]
fn ac_33_cada_total_e_rastreavel_as_linhas_e_o_desconhecido_impede_o_livre_confirmado() {
    let mut s = servico_com_saldo(vec![saldo(100_000, "2026-11-16T09:00:00Z")]);
    criar(&mut s, salario(500_000));
    criar(&mut s, streaming());
    criar(&mut s, consignacao());
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    let soma = |f: &dyn Fn(&Ocorrencia) -> bool| -> Centavos {
        c.linhas
            .iter()
            .filter(|o| f(o))
            .map(|o| o.previsto.unwrap_or(Centavos(0)))
            .sum()
    };
    assert_eq!(
        c.resumo.entradas_previstas,
        soma(&|o| o.natureza == Natureza::Entrada)
    );
    assert_eq!(
        c.resumo.saidas_de_caixa_previstas,
        soma(&|o| o.natureza == Natureza::Saida)
    );
    assert_eq!(
        c.resumo.descontos_em_folha,
        soma(&|o| o.natureza == Natureza::DescontoEmFolha)
    );
    assert!(!c.resumo.livre_e_estimativa);

    // Um compromisso sem valor torna o livre estimativa, e a lacuna diz isso sem nomes.
    criar(
        &mut s,
        novo(
            "Seguro Sem Valor Exemplo",
            "Essencial",
            9,
            Natureza::Saida,
            Prazo::SemFim,
            None,
            Certeza::Estimado,
            Frequencia::Mensal { dia: 8 },
        ),
    );
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert!(c.resumo.livre_e_estimativa);
    assert_eq!(c.resumo.a_conferir, 1);
    assert!(!c.resumo.lacunas.is_empty());
    for lacuna in &c.resumo.lacunas {
        assert!(!lacuna.contains("Seguro"), "lacuna sem nome: {lacuna}");
        assert!(!lacuna.contains("R$"), "lacuna sem valor: {lacuna}");
    }
    // O sem valor nunca vira zero: continua None na linha.
    let sem_valor = s
        .controle(&mes(2026, 12), Filtro::SemValor, None, HOJE)
        .expect("controle");
    assert_eq!(sem_valor.linhas.len(), 1);
    assert_eq!(sem_valor.linhas[0].previsto, None);
}

#[test]
fn saldo_ausente_ou_com_mais_de_um_dia_marca_o_livre_como_estimativa() {
    let mut s = servico();
    criar(&mut s, streaming());
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert!(c.resumo.livre_e_estimativa, "sem saldo");

    let mut s = servico_com_saldo(vec![saldo(100_000, "2026-11-13T09:00:00Z")]);
    criar(&mut s, streaming());
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert!(c.resumo.livre_e_estimativa, "saldo de três dias atrás");
    assert!(c.resumo.lacunas.iter().any(|l| l.contains("desatualizado")));

    let mut s = servico_com_saldo(vec![saldo(100_000, "2026-11-15T20:00:00Z")]);
    criar(&mut s, streaming());
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert!(!c.resumo.livre_e_estimativa, "saldo de ontem ainda vale");
}

#[test]
fn em_atraso_entra_no_resumo_e_desconta_do_livre_uma_vez_so() {
    let mut s = servico_com_saldo(vec![saldo(100_000, "2026-11-16T09:00:00Z")]);
    let id = criar(&mut s, streaming());
    // Novembro (dia 5) já passou e está em atraso; dezembro ainda não venceu.
    let nov = s
        .controle(&mes(2026, 11), Filtro::Todos, None, HOJE)
        .expect("novembro");
    assert_eq!(
        nov.resumo.em_atraso,
        Centavos(3500 * 2),
        "outubro e novembro"
    );
    assert_eq!(nov.atrasadas_de_antes.len(), 1);
    assert_eq!(nov.resumo.livre_apos_compromissos, Centavos(100_000 - 7000));
    // Informar o pago de novembro tira o novembro do atraso.
    pago_informado(&mut s, &id, 2026, 11, 3500);
    let nov = s
        .controle(&mes(2026, 11), Filtro::Todos, None, HOJE)
        .expect("novembro");
    assert_eq!(nov.resumo.em_atraso, Centavos(3500));
    assert_eq!(
        nov.resumo.a_conferir, 1,
        "pago só informado fica a conferir"
    );
}

// ---------------------------------------------------------------------------------------
// AC-29: origem importada
// ---------------------------------------------------------------------------------------

#[test]
fn ac_29_a_mesma_origem_duas_vezes_devolve_o_recorrente_existente() {
    let mut s = servico();
    let origem = OrigemImportada {
        arquivo_sha256: "ab".repeat(32),
        aba: "2026.11".into(),
        linha: 14,
    };
    let criar_com_origem = |s: &mut PlanejamentoSqlite| {
        s.aplicar(
            Alteracao::CriarRecorrente {
                recorrente: streaming(),
                origem: Some(origem.clone()),
            },
            Autor::Importacao,
        )
        .expect("criar")
    };
    let primeiro = criar_com_origem(&mut s);
    let historico = s.historico(None, 100).expect("histórico").len();
    let segundo = criar_com_origem(&mut s);
    assert_eq!(primeiro.alvo, segundo.alvo);
    assert_eq!(
        s.recorrentes(true).expect("lista").len(),
        1,
        "zero duplicatas"
    );
    assert_eq!(s.historico(None, 100).expect("histórico").len(), historico);
    assert_eq!(
        s.origem_importada(&origem).expect("origem"),
        Some(primeiro.alvo)
    );
    // Outra linha da mesma aba é outra origem.
    let outra = OrigemImportada {
        linha: 15,
        ..origem
    };
    assert_eq!(s.origem_importada(&outra).expect("origem"), None);
}

// ---------------------------------------------------------------------------------------
// AC-30, AC-31: propostas
// ---------------------------------------------------------------------------------------

fn pausa_proposta(id: &str, base: i64) -> Alteracao {
    Alteracao::Pausar {
        recorrente: id.into(),
        desde: comp(2026, 12),
        ate: None,
        motivo: "proposta".into(),
        versao_base: base,
    }
}

#[test]
fn ac_30_proposta_nao_muda_nada_e_so_a_confirmacao_aplica() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let antes_recorrentes = s.recorrentes(true).expect("lista");
    let antes_historico = s.historico(None, 100).expect("histórico").len();
    let b = base(&s, &id);

    let p = s
        .propor(pausa_proposta(&id, b), "mcp", "chave-1")
        .expect("propor");
    assert_eq!(p.estado, EstadoDaProposta::Pendente);
    assert_eq!(s.recorrentes(true).expect("lista"), antes_recorrentes);
    assert_eq!(
        s.historico(None, 100).expect("histórico").len(),
        antes_historico
    );
    assert!(
        ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_some(),
        "nada mudou"
    );

    // A mesma chave devolve a mesma proposta, sem criar outra.
    let de_novo = s
        .propor(pausa_proposta(&id, b), "mcp", "chave-1")
        .expect("propor");
    assert_eq!(de_novo.id, p.id);
    assert_eq!(s.propostas(true).expect("propostas").len(), 1);

    let recibo = s.confirmar_proposta(&p.id).expect("confirmar");
    assert_eq!(recibo.alvo, id);
    assert!(ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_none());
    assert_eq!(s.propostas(true).expect("propostas").len(), 0);
    let h = s.historico(Some(&id), 1).expect("histórico");
    assert_eq!(h[0].autor, Autor::Usuario, "a confirmação é do usuário");
    assert!(matches!(
        s.confirmar_proposta(&p.id),
        Err(ErroDoPlanejamento::Invalida(_))
    ));
}

#[test]
fn ac_30_recusar_grava_o_motivo_e_nada_muda() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let b = base(&s, &id);
    let p = s
        .propor(pausa_proposta(&id, b), "mcp", "chave-2")
        .expect("propor");
    s.recusar_proposta(&p.id, "não é isso").expect("recusar");
    let todas = s.propostas(false).expect("propostas");
    assert_eq!(todas[0].estado, EstadoDaProposta::Recusada);
    assert_eq!(todas[0].motivo.as_deref(), Some("não é isso"));
    assert!(ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_some());
    assert!(s.confirmar_proposta(&p.id).is_err());
}

#[test]
fn ac_31_proposta_com_base_velha_e_recusada_sem_alteracao_parcial() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let b = base(&s, &id);
    let p = s
        .propor(pausa_proposta(&id, b), "mcp", "chave-3")
        .expect("propor");

    // Enquanto a proposta espera, o usuário muda o recorrente (a base sobe).
    s.aplicar(
        Alteracao::Editar {
            recorrente: id.clone(),
            escopo: Escopo::DaquiEmDiante(comp(2027, 1)),
            mudanca: MudancaDeVersao {
                valor: Some(Some(Centavos(4000))),
                ..Default::default()
            },
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("editar");
    let antes = contagens(&s);
    let rec_antes = s.recorrente(&id).expect("ler");

    let erro = s.confirmar_proposta(&p.id).expect_err("base velha");
    assert!(matches!(erro, ErroDoPlanejamento::VersaoVelha { atual } if atual == b + 1));
    assert_eq!(
        s.recorrente(&id).expect("ler"),
        rec_antes,
        "o recorrente não mudou"
    );
    assert_eq!(contagens(&s), antes, "nenhuma linha nova, nem de histórico");
    let propostas = s.propostas(true).expect("propostas");
    assert_eq!(propostas.len(), 1, "continua pendente");
    assert!(propostas[0].motivo.is_some(), "com o motivo da falha");
}

#[test]
fn aplicar_com_base_velha_nao_muda_nada() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let antes = contagens(&s);
    let erro = s
        .aplicar(
            Alteracao::Pausar {
                recorrente: id,
                desde: comp(2026, 12),
                ate: None,
                motivo: "x".into(),
                versao_base: 99,
            },
            Autor::Usuario,
        )
        .expect_err("base velha");
    assert!(matches!(erro, ErroDoPlanejamento::VersaoVelha { atual: 1 }));
    assert_eq!(contagens(&s), antes);
}

#[test]
fn proposta_expira_em_sete_dias() {
    let t0 = dia(2026, 11, 16).dias_desde_1970() * 86_400;
    let relogio = std::sync::Arc::new(std::sync::atomic::AtomicI64::new(t0));
    let r = relogio.clone();
    let mut s = servico().com_relogio(move || r.load(std::sync::atomic::Ordering::SeqCst));
    let id = criar(&mut s, streaming());
    let b = base(&s, &id);
    let p = s
        .propor(pausa_proposta(&id, b), "mcp", "chave-4")
        .expect("propor");
    assert_eq!(p.expira_em, "2026-11-23T00:00:00Z");
    relogio.store(t0 + 7 * 86_400, std::sync::atomic::Ordering::SeqCst);
    assert_eq!(s.propostas(true).expect("propostas").len(), 0);
    assert_eq!(
        s.propostas(false).expect("propostas")[0].estado,
        EstadoDaProposta::Expirada
    );
    assert!(matches!(
        s.confirmar_proposta(&p.id),
        Err(ErroDoPlanejamento::Invalida(_))
    ));
    assert!(
        ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_some(),
        "nada mudou"
    );
}

// ---------------------------------------------------------------------------------------
// AC-35 e demais regras de ocorrência
// ---------------------------------------------------------------------------------------

#[test]
fn ac_35_evento_futuro_avulso_aparece_uma_vez_so() {
    let mut s = servico();
    let evento = novo(
        "Presente Exemplo",
        "Eventos",
        1,
        Natureza::Saida,
        Prazo::SemFim,
        Some(15_000),
        Certeza::Estimado,
        Frequencia::Unica {
            data: dia(2027, 6, 12),
        },
    );
    let id = criar(&mut s, evento);
    let cal = s.calendario(comp(2026, 11), 360, HOJE).expect("calendário");
    let com_saida: Vec<_> = cal
        .iter()
        .filter(|m| m.saidas_de_caixa != Centavos(0))
        .collect();
    assert_eq!(com_saida.len(), 1);
    assert_eq!(com_saida[0].competencia, comp(2027, 6));
    assert_eq!(com_saida[0].saidas_de_caixa, Centavos(15_000));
    assert_eq!(
        s.proximas(&id, comp(2026, 11), 360, HOJE)
            .expect("próximas")
            .len(),
        1
    );
    let c = s
        .controle(&mes(2027, 6), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert_eq!(c.linhas.len(), 1);
    assert_eq!(c.linhas[0].vencimento, dia(2027, 6, 12));
    assert_eq!(
        s.controle(&mes(2027, 7), Filtro::Todos, None, HOJE)
            .expect("controle")
            .linhas
            .len(),
        0
    );
}

#[test]
fn ac_09_adiar_muda_o_vencimento_sem_sumir_e_adiar_sem_data_continua_devida() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let oc = |a, m| IdOcorrencia {
        recorrente: id.clone(),
        competencia: comp(a, m),
        parcela: None,
    };
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: oc(2026, 12),
            excecao: Excecao::Adiar {
                nova_data: Some(dia(2027, 1, 20)),
            },
        },
        Autor::Usuario,
    )
    .expect("adiar");
    // A ocorrência de dezembro agora vence em janeiro: sai de dezembro e entra em janeiro.
    assert!(ocorrencia(&s, &id, 2026, 12, Filtro::Todos).is_none());
    let o = s
        .controle(&mes(2027, 1), Filtro::Todos, None, HOJE)
        .expect("janeiro")
        .linhas
        .into_iter()
        .find(|o| o.id.competencia == comp(2026, 12))
        .expect("dezembro adiado aparece em janeiro");
    assert_eq!(o.vencimento, dia(2027, 1, 20));
    assert_eq!(o.vencimento_original, dia(2026, 12, 5));
    assert_eq!(o.estado, EstadoDaOcorrencia::Adiada);

    // Adiada sem data: continua devida e vira atraso depois do vencimento original.
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: oc(2026, 10),
            excecao: Excecao::Adiar { nova_data: None },
        },
        Autor::Usuario,
    )
    .expect("adiar sem data");
    let out = ocorrencia(&s, &id, 2026, 10, Filtro::Todos).expect("outubro");
    assert_eq!(out.estado, EstadoDaOcorrencia::EmAtraso);
    assert_eq!(out.vencimento, out.vencimento_original);
    // Dispensar não é pagar: sai do exigível, mas a ocorrência segue marcada.
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: oc(2026, 11),
            excecao: Excecao::Dispensar {
                motivo: "brinde".into(),
            },
        },
        Autor::Usuario,
    )
    .expect("dispensar");
    let set = ocorrencia(&s, &id, 2026, 11, Filtro::Todos).expect("novembro");
    assert_eq!(set.estado, EstadoDaOcorrencia::Dispensada);
    assert_eq!(set.pago_informado, None);
}

#[test]
fn desfazer_excecao_devolve_a_regra_e_substituir_nao_ressuscita_a_antiga() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pago_informado(&mut s, &id, 2026, 10, 1000);
    pago_informado(&mut s, &id, 2026, 10, 3500); // substitui a anterior
    let excecoes = s.excecoes(&id).expect("exceções");
    assert_eq!(excecoes.len(), 2);
    assert_eq!(excecoes.iter().filter(|e| !e.desfeita).count(), 1);
    let vigente = excecoes.iter().find(|e| !e.desfeita).expect("vigente");
    s.aplicar(
        Alteracao::DesfazerExcecao {
            id: vigente.id.clone(),
        },
        Autor::Usuario,
    )
    .expect("desfazer");
    let o = ocorrencia(&s, &id, 2026, 10, Filtro::Todos).expect("outubro");
    assert_eq!(
        o.estado,
        EstadoDaOcorrencia::EmAtraso,
        "voltou ao que a regra diz"
    );
    assert!(matches!(
        s.aplicar(
            Alteracao::DesfazerExcecao {
                id: "nao-existe".into()
            },
            Autor::Usuario
        ),
        Err(ErroDoPlanejamento::NaoEncontrado(_))
    ));
}

#[test]
fn conciliacao_confirmada_e_o_unico_pago_e_sugestao_nunca_rebaixa_a_confirmada() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    let c = |confirmada| Conciliacao {
        ocorrencia: IdOcorrencia {
            recorrente: id.clone(),
            competencia: comp(2026, 10),
            parcela: None,
        },
        movimento: "pluggy:txn_exemplo_9".into(),
        valor: Centavos(3500),
        data: dia(2026, 10, 5),
        confirmada,
    };
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: c(false),
        },
        Autor::Importacao,
    )
    .expect("sugestão");
    let o = ocorrencia(&s, &id, 2026, 10, Filtro::Todos).expect("outubro");
    assert!(o.sugestao_de_conciliacao);
    assert_eq!(o.estado, EstadoDaOcorrencia::EmAtraso, "sugestão não paga");
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: c(true),
        },
        Autor::Usuario,
    )
    .expect("confirmar");
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: c(false),
        },
        Autor::Importacao,
    )
    .expect("reimportar a sugestão");
    let o = ocorrencia(&s, &id, 2026, 10, Filtro::Todos).expect("outubro");
    assert_eq!(
        o.estado,
        EstadoDaOcorrencia::Conciliada,
        "a decisão do usuário fica"
    );
    assert!(!o.sugestao_de_conciliacao);
    assert_eq!(s.conciliacoes(&id).expect("lista").len(), 1);
}

// ---------------------------------------------------------------------------------------
// Controle: ordem, filtros e busca
// ---------------------------------------------------------------------------------------

#[test]
fn controle_ordena_por_grupo_e_depois_por_posicao_e_filtra_e_busca() {
    let mut s = servico();
    s.aplicar(
        Alteracao::CriarGrupo {
            nome: "Essencial".into(),
            posicao: 1,
        },
        Autor::Usuario,
    )
    .expect("grupo");
    s.aplicar(
        Alteracao::CriarGrupo {
            nome: "Assinaturas".into(),
            posicao: 2,
        },
        Autor::Usuario,
    )
    .expect("grupo");
    let stream = criar(&mut s, streaming());
    let consig = criar(&mut s, consignacao()); // Essencial, posição 2
    let ener = criar(&mut s, energia()); // Essencial, posição 1
    let emp = criar(&mut s, emprestimo()); // grupo novo, vai para o fim

    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    let ordem: Vec<_> = c.linhas.iter().map(|o| o.id.recorrente.clone()).collect();
    assert_eq!(ordem, [ener.clone(), consig, stream.clone(), emp.clone()]);
    assert_eq!(
        c.grupos.iter().map(|g| g.nome.as_str()).collect::<Vec<_>>(),
        ["Essencial", "Assinaturas", "Empréstimos"]
    );

    // Mover muda a prioridade, não o dinheiro.
    let b = base(&s, &stream);
    s.aplicar(
        Alteracao::Mover {
            recorrente: stream.clone(),
            grupo: "Essencial".into(),
            posicao: 0,
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("mover");
    let c = s
        .controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
        .expect("controle");
    assert_eq!(c.linhas[0].id.recorrente, stream);

    // Busca sem acento nem caixa.
    let achados = s
        .controle(&mes(2026, 12), Filtro::Todos, Some("EMPRESTIMO"), HOJE)
        .expect("busca");
    assert_eq!(achados.linhas.len(), 1);
    assert_eq!(achados.linhas[0].id.recorrente, emp);
    // A busca não muda o resumo do período.
    assert_eq!(
        achados.resumo,
        s.controle(&mes(2026, 12), Filtro::Todos, None, HOJE)
            .expect("c")
            .resumo
    );

    // Filtros: em atraso em novembro (dia 5 já passou; dia 10 e 31 ainda não).
    let atraso = s
        .controle(&mes(2026, 11), Filtro::EmAtraso, None, HOJE)
        .expect("filtro");
    assert!(
        atraso
            .linhas
            .iter()
            .all(|o| o.estado == EstadoDaOcorrencia::EmAtraso)
    );
    assert_eq!(
        atraso.linhas.len(),
        3,
        "streaming, consignação (dia 5) e empréstimo (dia 10)"
    );
    let _ = ener;
}

#[test]
fn ciclo_de_20_a_19_inclui_o_vencimento_pelo_dia_e_nao_pelo_mes() {
    let mut s = servico();
    criar(&mut s, streaming()); // dia 5
    criar(
        &mut s,
        novo(
            "Aluguel Exemplo",
            "Essencial",
            1,
            Natureza::Saida,
            Prazo::SemFim,
            Some(100_000),
            Certeza::Confirmado,
            Frequencia::Mensal { dia: 25 },
        ),
    );
    // Ciclo que começa em 20/11 e termina em 19/12: aluguel de novembro (25/11) e streaming
    // de dezembro (05/12).
    let ciclo = Ciclo::do_mes(2026, 11, DiaDeCorte::PADRAO);
    let c = s
        .controle(&Periodo::Ciclo(ciclo), Filtro::Todos, None, HOJE)
        .expect("controle");
    let datas: Vec<_> = c.linhas.iter().map(|o| o.vencimento).collect();
    assert_eq!(datas.len(), 2);
    assert!(datas.contains(&dia(2026, 11, 25)));
    assert!(datas.contains(&dia(2026, 12, 5)));
}

// ---------------------------------------------------------------------------------------
// Simulação e histórico
// ---------------------------------------------------------------------------------------

#[test]
fn simular_compara_antes_e_depois_e_nunca_grava() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pago_informado(&mut s, &id, 2027, 2, 3500);
    let antes = contagens(&s);
    let b = base(&s, &id);
    let sim = s
        .simular(
            &Alteracao::Pausar {
                recorrente: id.clone(),
                desde: comp(2027, 1),
                ate: Some(comp(2027, 4)),
                motivo: "simulação".into(),
                versao_base: b,
            },
            HOJE,
        )
        .expect("simular");
    assert_eq!(contagens(&s), antes, "rollback: nada foi gravado");
    assert_eq!(base(&s, &id), b);
    // Janeiro e março saem; fevereiro tem pago informado e continua.
    assert_eq!(sim.meses_afetados, [comp(2027, 1), comp(2027, 3)]);
    assert_eq!(
        sim.mudancas,
        [
            (comp(2027, 1), Some(Centavos(3500)), None),
            (comp(2027, 3), Some(Centavos(3500)), None)
        ]
    );
    assert_eq!(sim.conflitos.len(), 1);
    assert!(sim.conflitos[0].contains("2027-02"));

    // Simular uma alteração inválida também não deixa rastro.
    assert!(matches!(
        s.simular(
            &Alteracao::Pausar {
                recorrente: id,
                desde: comp(2027, 1),
                ate: None,
                motivo: "x".into(),
                versao_base: 99,
            },
            HOJE
        ),
        Err(ErroDoPlanejamento::VersaoVelha { .. })
    ));
    assert_eq!(contagens(&s), antes);

    // Simular a criação mostra os meses novos.
    let sim = s
        .simular(
            &Alteracao::CriarRecorrente {
                recorrente: emprestimo(),
                origem: None,
            },
            HOJE,
        )
        .expect("simular criação");
    // A janela de 24 meses começa em 2026-10 e a primeira parcela é de 2026-11.
    assert_eq!(sim.meses_afetados.len(), 23);
    assert_eq!(contagens(&s), antes);
}

#[test]
fn cada_alteracao_grava_uma_linha_de_historico_com_a_alteracao_em_json() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pausar(&mut s, &id, comp(2026, 12), None);
    pago_informado(&mut s, &id, 2026, 10, 3500);
    let h = s.historico(Some(&id), 10).expect("histórico");
    assert_eq!(h.len(), 3);
    assert!(h[0].resumo.contains("Informou pago"));
    assert!(h[1].alteracao.contains("\"tipo\":\"pausar\""));
    assert!(h[2].alteracao.contains("\"tipo\":\"criar_recorrente\""));
    assert!(
        h.iter()
            .all(|e| e.autor == Autor::Usuario && !e.quando.is_empty())
    );
    assert_eq!(s.historico(None, 2).expect("limite").len(), 2);
}

// ---------------------------------------------------------------------------------------
// Banco: só cresce, migração, permissão
// ---------------------------------------------------------------------------------------

#[test]
fn o_historico_so_aceita_insert_e_nada_de_fato_e_apagado() {
    let mut s = servico();
    let id = criar(&mut s, streaming());
    pago_informado(&mut s, &id, 2026, 10, 3500);
    pausar(&mut s, &id, comp(2027, 1), None);
    let c = s.banco.conexao();
    for sql in [
        "UPDATE historico SET resumo = 'x'",
        "DELETE FROM historico",
        "DELETE FROM recorrentes",
        "DELETE FROM excecoes",
        "DELETE FROM versoes",
        "DELETE FROM pausas",
    ] {
        assert!(c.execute(sql, []).is_err(), "deveria falhar: {sql}");
    }
    assert!(contagens(&s).iter().all(|n| *n >= 0));
    assert_eq!(s.historico(None, 10).expect("histórico").len(), 3);
}

#[test]
fn o_codigo_nao_tem_delete_fisico_nem_update_do_historico() {
    for (nome, fonte) in [
        ("escrita.rs", include_str!("../src/escrita.rs")),
        ("servico.rs", include_str!("../src/servico.rs")),
    ] {
        assert!(!fonte.contains("DELETE FROM"), "{nome} apaga de fato");
        assert!(
            !fonte.contains("UPDATE historico"),
            "{nome} altera o histórico"
        );
    }
}

#[cfg(unix)]
#[test]
fn abrir_cria_o_arquivo_com_permissao_0600_e_os_dados_sobrevivem() {
    use std::os::unix::fs::PermissionsExt;
    let pasta = tempfile::tempdir().expect("pasta");
    let caminho = pasta.path().join("planejamento.sqlite");
    let id = {
        let mut s =
            PlanejamentoSqlite::novo(Banco::abrir(&caminho).expect("abrir"), Box::new(SemSaldos));
        criar(&mut s, streaming())
    };
    let modo = std::fs::metadata(&caminho)
        .expect("arquivo")
        .permissions()
        .mode()
        & 0o777;
    assert_eq!(modo, 0o600);
    let banco = Banco::abrir(&caminho).expect("reabrir");
    assert_eq!(banco.versao_do_esquema().expect("versão"), 1);
    let s = PlanejamentoSqlite::novo(banco, Box::new(SemSaldos));
    assert!(s.recorrente(&id).expect("ler").is_some());
}

#[test]
fn banco_de_uma_versao_mais_nova_e_recusado() {
    let pasta = tempfile::tempdir().expect("pasta");
    let caminho = pasta.path().join("planejamento.sqlite");
    drop(Banco::abrir(&caminho).expect("abrir"));
    rusqlite_user_version(&caminho, 99);
    assert!(matches!(
        Banco::abrir(&caminho),
        Err(ErroDoPlanejamento::Banco(_))
    ));
}

fn rusqlite_user_version(caminho: &std::path::Path, versao: u32) {
    let banco = Banco::abrir(caminho).expect("abrir");
    banco
        .conexao()
        .pragma_update(None, "user_version", i64::from(versao))
        .expect("versão");
}
