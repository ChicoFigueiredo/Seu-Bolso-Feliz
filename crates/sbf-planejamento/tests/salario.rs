//! O salário no planejamento (spec 12, fase A): as três provas, desconciliar, a baixa dos
//! descontos em folha pelo contracheque e o previsto que segue a rubrica. Dados fictícios.

use std::sync::{Arc, Mutex};

use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Autor, Banco, Centavos, Certeza, Competencia, Composicao,
    Conciliacao, Data, ErroDoPlanejamento, EstadoDaOcorrencia, Excecao, Fonte, FonteDeMovimentos,
    Frequencia, IdOcorrencia, MovimentoDoExtrato, Natureza, NovoRecorrente, Periodo, Planejamento,
    PlanejamentoSqlite, Prazo, Rubrica, SemSaldos, TipoDeDocumento, TipoDeRubrica, Versao,
};

const HOJE: Data = Data {
    ano: 2026,
    mes: 11,
    dia: 16,
};
const RELOGIO: i64 = 1_794_830_400;
const SHA: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CONTA: &str = "BANCO · 1";

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

fn ocorrencia(id: &str) -> IdOcorrencia {
    IdOcorrencia {
        recorrente: id.into(),
        competencia: comp(2026, 11),
        parcela: None,
    }
}

fn rubrica(nome: &str, tipo: TipoDeRubrica, valor: i64, recorrente: Option<&str>) -> Rubrica {
    Rubrica {
        codigo: None,
        nome: nome.into(),
        tipo,
        valor: Centavos(valor),
        competencia: None,
        prazo: None,
        recorrente: recorrente.map(str::to_owned),
    }
}

fn definir(
    s: &mut PlanejamentoSqlite,
    salario: &str,
    desde: Competencia,
    rubricas: Vec<Rubrica>,
    documento: Option<&str>,
) {
    let b = base(s, salario);
    s.aplicar(
        Alteracao::DefinirComposicao {
            composicao: Composicao {
                recorrente: salario.into(),
                desde,
                rubricas,
                documento: documento.map(str::to_owned),
                origem: "contracheque 11/2026".into(),
            },
            versao_base: b,
        },
        Autor::Usuario,
    )
    .expect("composicao");
}

fn vincular_contracheque(s: &mut PlanejamentoSqlite, salario: &str) {
    s.aplicar(
        Alteracao::VincularDocumento {
            alvo: AlvoDoDocumento::Ocorrencia {
                id: ocorrencia(salario),
            },
            sha256: SHA.into(),
            tipo_documento: TipoDeDocumento::Contracheque,
            nome: "contracheque.pdf".into(),
            data: Some(dia(1)),
        },
        Autor::Usuario,
    )
    .expect("vincular");
}

fn mapear(s: &mut PlanejamentoSqlite, apelido: &str, conta: &str) {
    s.aplicar(
        Alteracao::MapearConta {
            apelido: apelido.into(),
            conta_do_extrato: Some(conta.into()),
        },
        Autor::Usuario,
    )
    .expect("mapear");
}

fn conciliacoes(s: &PlanejamentoSqlite, id: &str) -> Vec<Conciliacao> {
    s.conciliacoes(id).expect("conciliacoes")
}

/// Um salário com par de conta, o contracheque vinculado e a composição dele (líquido
/// 4.800,00), e um crédito de `credito` na conta certa. Devolve o id do Salário.
fn cenario(
    extrato: &Extrato,
    credito: i64,
    conta_do_credito: &str,
) -> (PlanejamentoSqlite, String) {
    extrato.por("pluggy:txn_sal", 6, credito, conta_do_credito, "CREDITO");
    let mut s = servico(extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(700_000),
        Some("CX-0001"),
    );
    mapear(&mut s, "CX-0001", CONTA);
    definir(
        &mut s,
        &id,
        comp(2026, 11),
        vec![
            rubrica("Vencimento", TipoDeRubrica::Provento, 600_000, None),
            rubrica("Descontos", TipoDeRubrica::Desconto, 120_000, None),
        ],
        Some(SHA),
    );
    vincular_contracheque(&mut s, &id);
    (s, id)
}

// ---------------------------------------------------------------------------------------
// AC-03: as três provas
// ---------------------------------------------------------------------------------------

#[test]
fn tres_provas_exatas_conciliam_sozinho_com_autor_regra() {
    let extrato = Extrato::default();
    let (mut s, id) = cenario(&extrato, 480_000, CONTA);
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    let c = conciliacoes(&s, &id);
    assert_eq!(c.len(), 1);
    assert!(c[0].confirmada, "as três provas confirmam sozinhas");
    assert_eq!(c[0].movimento, "pluggy:txn_sal");
    let historico = s
        .historico(Some(&ocorrencia(&id).to_string()), 10)
        .expect("hist");
    assert!(
        historico.iter().any(|h| matches!(
            &h.autor,
            Autor::Regra { regra } if regra == "tres-provas"
        )),
        "a linha do histórico leva a regra"
    );
    let gravadas = s
        .conciliacoes_da_ocorrencia(&ocorrencia(&id))
        .expect("lista");
    assert!(gravadas[0].automatica);
    assert!(gravadas[0].mesma_conta);
}

#[test]
fn um_centavo_de_diferenca_volta_a_sugestao() {
    let extrato = Extrato::default();
    let (mut s, id) = cenario(&extrato, 480_001, CONTA);
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    let c = conciliacoes(&s, &id);
    assert!(!c[0].confirmada);
    assert!(
        !s.conciliacoes_da_ocorrencia(&ocorrencia(&id))
            .expect("lista")[0]
            .automatica
    );
}

#[test]
fn sem_a_conta_certa_sem_o_contracheque_ou_sem_composicao_do_mes_so_sugere() {
    // Sem o par de conta confirmado, a conta não pesa: sugere, mas não confirma.
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_sal", 6, 480_000, CONTA, "CREDITO");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(700_000),
        Some("CX-0001"),
    );
    definir(
        &mut s,
        &id,
        comp(2026, 11),
        vec![
            rubrica("Vencimento", TipoDeRubrica::Provento, 600_000, None),
            rubrica("Descontos", TipoDeRubrica::Desconto, 120_000, None),
        ],
        Some(SHA),
    );
    vincular_contracheque(&mut s, &id);
    s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert!(!conciliacoes(&s, &id)[0].confirmada, "sem par de conta");

    // Sem o contracheque vinculado.
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_sal", 6, 480_000, CONTA, "CREDITO");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(700_000),
        Some("CX-0001"),
    );
    mapear(&mut s, "CX-0001", CONTA);
    definir(
        &mut s,
        &id,
        comp(2026, 11),
        vec![
            rubrica("Vencimento", TipoDeRubrica::Provento, 600_000, None),
            rubrica("Descontos", TipoDeRubrica::Desconto, 120_000, None),
        ],
        Some(SHA),
    );
    s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert!(
        !conciliacoes(&s, &id)[0].confirmada,
        "sem contracheque vinculado"
    );

    // Composição sem documento (digitada à mão): não é prova.
    let extrato = Extrato::default();
    let (mut s, id) = cenario(&extrato, 480_000, CONTA);
    definir(
        &mut s,
        &id,
        comp(2026, 11),
        vec![
            rubrica("Vencimento", TipoDeRubrica::Provento, 600_000, None),
            rubrica("Descontos", TipoDeRubrica::Desconto, 120_000, None),
        ],
        None,
    );
    s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert!(
        !conciliacoes(&s, &id)[0].confirmada,
        "composição sem documento"
    );

    // Composição de um mês anterior (as fixas de outubro): não prova novembro.
    let extrato2 = Extrato::default();
    extrato2.por("pluggy:txn_sal", 6, 480_000, CONTA, "CREDITO");
    let mut s2 = servico(&extrato2);
    let id2 = criar(
        &mut s2,
        "Salário",
        Natureza::Entrada,
        5,
        Some(700_000),
        Some("CX-0001"),
    );
    mapear(&mut s2, "CX-0001", CONTA);
    definir(
        &mut s2,
        &id2,
        comp(2026, 10),
        vec![rubrica(
            "Vencimento",
            TipoDeRubrica::Provento,
            480_000,
            None,
        )],
        Some(SHA),
    );
    vincular_contracheque(&mut s2, &id2);
    s2.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert!(!conciliacoes(&s2, &id2)[0].confirmada, "fixas de outro mês");
}

#[test]
fn contracheque_ligado_ao_recorrente_tambem_vale() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_sal", 6, 480_000, CONTA, "CREDITO");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(700_000),
        Some("CX-0001"),
    );
    mapear(&mut s, "CX-0001", CONTA);
    definir(
        &mut s,
        &id,
        comp(2026, 11),
        vec![
            rubrica("Vencimento", TipoDeRubrica::Provento, 600_000, None),
            rubrica("Descontos", TipoDeRubrica::Desconto, 120_000, None),
        ],
        Some(SHA),
    );
    s.aplicar(
        Alteracao::VincularDocumento {
            alvo: AlvoDoDocumento::Recorrente { id: id.clone() },
            sha256: SHA.into(),
            tipo_documento: TipoDeDocumento::Contracheque,
            nome: "contracheque.pdf".into(),
            data: None,
        },
        Autor::Usuario,
    )
    .expect("vincular");
    s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert!(conciliacoes(&s, &id)[0].confirmada);
}

#[test]
fn sugestao_que_ja_existia_e_promovida_quando_o_contracheque_chega() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_sal", 6, 480_000, CONTA, "CREDITO");
    let mut s = servico(&extrato);
    let id = criar(
        &mut s,
        "Salário",
        Natureza::Entrada,
        5,
        Some(480_000),
        Some("CX-0001"),
    );
    mapear(&mut s, "CX-0001", CONTA);
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    assert!(!conciliacoes(&s, &id)[0].confirmada);
    definir(
        &mut s,
        &id,
        comp(2026, 11),
        vec![
            rubrica("Vencimento", TipoDeRubrica::Provento, 600_000, None),
            rubrica("Descontos", TipoDeRubrica::Desconto, 120_000, None),
        ],
        Some(SHA),
    );
    vincular_contracheque(&mut s, &id);
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    assert!(conciliacoes(&s, &id)[0].confirmada);
}

// ---------------------------------------------------------------------------------------
// AC-04: desfazer
// ---------------------------------------------------------------------------------------

#[test]
fn desfazer_a_automatica_devolve_a_sugestao_e_nao_e_refeita_sozinha() {
    let extrato = Extrato::default();
    let (mut s, id) = cenario(&extrato, 480_000, CONTA);
    s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert!(conciliacoes(&s, &id)[0].confirmada);

    s.aplicar(
        Alteracao::Desconciliar {
            ocorrencia: ocorrencia(&id),
            movimento: "pluggy:txn_sal".into(),
        },
        Autor::Usuario,
    )
    .expect("desfazer");
    assert!(
        conciliacoes(&s, &id).is_empty(),
        "a conciliação sumiu da projeção"
    );
    let o = &s.proximas(&id, comp(2026, 11), 1, HOJE).expect("proximas")[0];
    assert_eq!(o.conciliado, None);
    assert_ne!(o.estado, EstadoDaOcorrencia::Conciliada);
    assert!(
        s.conciliacao_desfeita(&ocorrencia(&id), "pluggy:txn_sal")
            .expect("desfeita")
    );

    // A ocorrência volta a poder ser sugerida, mas não confirmada sozinha.
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 1);
    let c = conciliacoes(&s, &id);
    assert_eq!(c.len(), 1);
    assert!(!c[0].confirmada);
    // E a rodada seguinte não a promove.
    assert_eq!(s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar"), 0);
    assert!(!conciliacoes(&s, &id)[0].confirmada);

    // O histórico guarda a automática, o desfazer e a sugestão (só cresce).
    let h = s
        .historico(Some(&ocorrencia(&id).to_string()), 20)
        .expect("hist");
    assert!(h.len() >= 3);
    assert!(h.iter().any(|x| x.resumo.contains("Desfez")));

    // O usuário ainda pode confirmar à mão.
    s.aplicar(
        Alteracao::Conciliar {
            conciliacao: Conciliacao {
                confirmada: true,
                ..c[0].clone()
            },
        },
        Autor::Usuario,
    )
    .expect("confirmar");
    assert!(conciliacoes(&s, &id)[0].confirmada);
    assert!(
        !s.conciliacoes_da_ocorrencia(&ocorrencia(&id))
            .expect("lista")[0]
            .automatica
    );
}

#[test]
fn desfazer_uma_sugestao_e_o_que_nao_existe() {
    let extrato = Extrato::default();
    extrato.por("pluggy:txn_a", 5, -100_000, CONTA, "ALUGUEL");
    let mut s = servico(&extrato);
    let id = criar(&mut s, "Aluguel", Natureza::Saida, 5, Some(100_000), None);
    s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert_eq!(conciliacoes(&s, &id).len(), 1);
    s.aplicar(
        Alteracao::Desconciliar {
            ocorrencia: ocorrencia(&id),
            movimento: "pluggy:txn_a".into(),
        },
        Autor::Usuario,
    )
    .expect("desfazer a sugestão");
    assert!(conciliacoes(&s, &id).is_empty());
    assert!(matches!(
        s.aplicar(
            Alteracao::Desconciliar {
                ocorrencia: ocorrencia(&id),
                movimento: "pluggy:txn_a".into(),
            },
            Autor::Usuario,
        ),
        Err(ErroDoPlanejamento::NaoEncontrado(_))
    ));
}

// ---------------------------------------------------------------------------------------
// AC-05: baixa pelo contracheque
// ---------------------------------------------------------------------------------------

fn com_descontos(extrato: &Extrato) -> (PlanejamentoSqlite, String, String, String) {
    let mut s = servico(extrato);
    let sal = criar(&mut s, "Salário", Natureza::Entrada, 5, Some(480_000), None);
    let inss = criar(
        &mut s,
        "INSS",
        Natureza::DescontoEmFolha,
        5,
        Some(90_000),
        Some("Salário"),
    );
    let saude = criar(
        &mut s,
        "Saúde",
        Natureza::DescontoEmFolha,
        5,
        Some(25_000),
        Some("Salário"),
    );
    definir(
        &mut s,
        &sal,
        comp(2026, 11),
        vec![
            rubrica("Salário padrão", TipoDeRubrica::Provento, 1_000_000, None),
            rubrica("INSS", TipoDeRubrica::Desconto, 90_000, Some(&inss)),
            rubrica("Imposto de renda", TipoDeRubrica::Desconto, 180_000, None),
            rubrica("Saúde", TipoDeRubrica::Desconto, 26_000, Some(&saude)),
        ],
        Some(SHA),
    );
    (s, sal, inss, saude)
}

#[test]
fn a_baixa_cobre_cada_rubrica_de_desconto_ligada_e_mostra_a_diferenca() {
    let extrato = Extrato::default();
    let (mut s, _sal, inss, saude) = com_descontos(&extrato);
    let baixas = s.baixas_pelo_contracheque(comp(2026, 11)).expect("baixas");
    assert_eq!(baixas.len(), 2, "só as rubricas ligadas a um compromisso");
    let do_inss = baixas
        .iter()
        .find(|b| b.conciliacao.ocorrencia.recorrente == inss)
        .expect("inss");
    assert_eq!(
        do_inss.conciliacao.movimento,
        format!("contracheque:{SHA}:1")
    );
    assert_eq!(do_inss.conciliacao.valor, Centavos(90_000));
    assert!(do_inss.conciliacao.confirmada);
    assert_eq!(do_inss.diferenca, None);
    assert!(!do_inss.ja_baixada);
    let da_saude = baixas
        .iter()
        .find(|b| b.conciliacao.ocorrencia.recorrente == saude)
        .expect("saude");
    assert_eq!(
        da_saude.conciliacao.movimento,
        format!("contracheque:{SHA}:3")
    );
    // O previsto segue a rubrica (26.000), então a diferença some; a baixa vale 26.000.
    assert_eq!(da_saude.conciliacao.valor, Centavos(26_000));

    // Calcular não grava nada (AC-07).
    assert!(s.conciliacoes(&inss).expect("c").is_empty());

    // Aplicar concilia, uma vez só.
    for b in &baixas {
        s.aplicar(
            Alteracao::Conciliar {
                conciliacao: b.conciliacao.clone(),
            },
            Autor::Usuario,
        )
        .expect("baixa");
    }
    let o = &s
        .proximas(&inss, comp(2026, 11), 1, HOJE)
        .expect("proximas")[0];
    assert_eq!(o.estado, EstadoDaOcorrencia::Conciliada);
    assert_eq!(o.conciliado, Some(Centavos(90_000)));
    let de_novo = s.baixas_pelo_contracheque(comp(2026, 11)).expect("baixas");
    assert!(de_novo.iter().all(|b| b.ja_baixada));
}

#[test]
fn a_diferenca_entre_a_rubrica_e_o_previsto_aparece() {
    let extrato = Extrato::default();
    let (mut s, _sal, inss, _saude) = com_descontos(&extrato);
    // O usuário fixou um valor só deste mês diferente da rubrica.
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: ocorrencia(&inss),
            excecao: Excecao::ValorSoDesteMes {
                valor: Centavos(88_000),
            },
        },
        Autor::Usuario,
    )
    .expect("excecao");
    let baixas = s.baixas_pelo_contracheque(comp(2026, 11)).expect("baixas");
    let b = baixas
        .iter()
        .find(|b| b.conciliacao.ocorrencia.recorrente == inss)
        .expect("inss");
    assert_eq!(b.previsto, Some(Centavos(88_000)));
    assert_eq!(b.diferenca, Some(Centavos(2_000)));
}

#[test]
fn so_o_contracheque_do_mes_da_baixa() {
    let extrato = Extrato::default();
    let (s, ..) = com_descontos(&extrato);
    // A composição vale desde novembro: dezembro não tem contracheque próprio.
    assert!(
        s.baixas_pelo_contracheque(comp(2026, 12))
            .expect("baixas")
            .is_empty()
    );
    assert!(
        s.baixas_pelo_contracheque(comp(2026, 10))
            .expect("baixas")
            .is_empty()
    );
}

// ---------------------------------------------------------------------------------------
// AC-06: o previsto segue a rubrica
// ---------------------------------------------------------------------------------------

#[test]
fn o_previsto_do_desconto_em_folha_segue_a_rubrica_vigente() {
    let extrato = Extrato::default();
    let (mut s, sal, inss, _saude) = com_descontos(&extrato);
    // Antes da composição: o último valor (a versão).
    let outubro = &s.proximas(&inss, comp(2026, 10), 1, HOJE).expect("out")[0];
    assert_eq!(outubro.previsto, Some(Centavos(90_000)));
    // Em novembro e depois: a rubrica.
    definir(
        &mut s,
        &sal,
        comp(2026, 12),
        vec![
            rubrica("Salário padrão", TipoDeRubrica::Provento, 1_000_000, None),
            rubrica("INSS", TipoDeRubrica::Desconto, 95_000, Some(&inss)),
        ],
        Some(SHA),
    );
    fn previsto_em(s: &PlanejamentoSqlite, id: &str, mes: u8) -> Option<Centavos> {
        s.proximas(id, comp(2026, mes), 1, HOJE).expect("p")[0].previsto
    }
    assert_eq!(previsto_em(&s, &inss, 11), Some(Centavos(90_000)));
    assert_eq!(previsto_em(&s, &inss, 12), Some(Centavos(95_000)));
    let janeiro = &s.proximas(&inss, comp(2027, 1), 1, HOJE).expect("jan")[0];
    assert_eq!(
        janeiro.previsto,
        Some(Centavos(95_000)),
        "vale daí em diante"
    );
    // O calendário e o controle usam o mesmo previsto.
    let cal = s.calendario(comp(2026, 12), 1, HOJE).expect("cal");
    assert!(cal[0].descontos_em_folha >= Centavos(95_000));
    // O valor só deste mês vence a rubrica.
    s.aplicar(
        Alteracao::Excecao {
            ocorrencia: IdOcorrencia {
                recorrente: inss.clone(),
                competencia: comp(2026, 12),
                parcela: None,
            },
            excecao: Excecao::ValorSoDesteMes { valor: Centavos(1) },
        },
        Autor::Usuario,
    )
    .expect("excecao");
    assert_eq!(previsto_em(&s, &inss, 12), Some(Centavos(1)));
}

#[test]
fn composicao_que_deixa_de_citar_o_compromisso_volta_ao_ultimo_valor() {
    let extrato = Extrato::default();
    let (mut s, sal, inss, _saude) = com_descontos(&extrato);
    // As fixas de dezembro não têm mais a rubrica do INSS.
    definir(
        &mut s,
        &sal,
        comp(2026, 12),
        vec![rubrica(
            "Salário padrão",
            TipoDeRubrica::Provento,
            1_000_000,
            None,
        )],
        Some(SHA),
    );
    let dezembro = &s.proximas(&inss, comp(2026, 12), 1, HOJE).expect("dez")[0];
    assert_eq!(
        dezembro.previsto,
        Some(Centavos(90_000)),
        "o valor da versão"
    );
}

#[test]
fn compromisso_que_nao_e_desconto_em_folha_nao_segue_a_rubrica() {
    let extrato = Extrato::default();
    let mut s = servico(&extrato);
    let sal = criar(&mut s, "Salário", Natureza::Entrada, 5, Some(480_000), None);
    let plano = criar(&mut s, "Plano", Natureza::Saida, 8, Some(30_000), None);
    definir(
        &mut s,
        &sal,
        comp(2026, 11),
        vec![
            rubrica("Salário padrão", TipoDeRubrica::Provento, 1_000_000, None),
            rubrica("Plano", TipoDeRubrica::Desconto, 25_000, Some(&plano)),
        ],
        Some(SHA),
    );
    let o = &s.proximas(&plano, comp(2026, 11), 1, HOJE).expect("plano")[0];
    assert_eq!(o.previsto, Some(Centavos(30_000)));
    // A baixa pelo contracheque ainda mostra a diferença.
    let baixas = s.baixas_pelo_contracheque(comp(2026, 11)).expect("baixas");
    assert_eq!(baixas[0].diferenca, Some(Centavos(-5_000)));
}

// ---------------------------------------------------------------------------------------
// AC-07: nada muda sem clique, exceto as três provas
// ---------------------------------------------------------------------------------------

#[test]
fn buscar_sem_as_tres_provas_so_sugere_e_nada_mais_muda() {
    let extrato = Extrato::default();
    let (mut s, id) = cenario(&extrato, 479_000, CONTA);
    let antes = s.historico(None, 100).expect("hist").len();
    s.buscar_conciliacoes(&novembro(), HOJE).expect("buscar");
    assert!(!conciliacoes(&s, &id)[0].confirmada);
    let depois = s.historico(None, 100).expect("hist");
    // Só a sugestão entrou no histórico, e por importação (não por regra nem pelo usuário).
    assert_eq!(depois.len(), antes + 1);
    assert!(matches!(depois[0].autor, Autor::Importacao));
}
