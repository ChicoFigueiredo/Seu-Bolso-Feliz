//! Os descontos em folha do contracheque (spec 12, fase A): propor, criar e ligar os
//! compromissos e dar baixa. Contracheques fictícios, planejamento em memória.

use sbf_planejamento::{
    Alteracao, Autor, Banco, Centavos, Certeza, Competencia, Data, EstadoDaOcorrencia, Fonte,
    Frequencia, Natureza, NovoRecorrente, Planejamento, PlanejamentoSqlite, Prazo, SemSaldos,
    Versao,
};
use sbf_planilha::contracheque;
use sbf_reconhecimento::descontos::{self, GRUPO};
use sbf_reconhecimento::{Sugestao, UsoDeRubricas};

const CONTRACHEQUE: &str = include_str!("../../sbf-planilha/tests/contracheque-ficticio.txt");
const CONTRACHEQUE_2026: &str =
    include_str!("../../sbf-planilha/tests/contracheque-2026-ficticio.txt");
const SHA: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

fn hoje() -> Data {
    Data::nova(2026, 11, 16).expect("data")
}

fn servico() -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos))
}

fn criar(
    s: &mut PlanejamentoSqlite,
    nome: &str,
    grupo: &str,
    natureza: Natureza,
    valor: i64,
) -> String {
    s.aplicar(
        Alteracao::CriarRecorrente {
            recorrente: NovoRecorrente {
                nome: nome.into(),
                grupo: grupo.into(),
                subgrupo: None,
                posicao: 1,
                natureza,
                prazo: Prazo::SemFim,
                primeira_versao: Versao {
                    id: String::new(),
                    desde: comp(2026, 1),
                    valor: Some(Centavos(valor)),
                    certeza: Certeza::Confirmado,
                    fonte: Fonte::Planilha,
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

fn uso(plan: &PlanejamentoSqlite, salario: &str, texto: &str) -> UsoDeRubricas {
    let c = contracheque::ler(texto).expect("ler");
    UsoDeRubricas {
        recorrente: salario.to_owned(),
        nome: plan.recorrente(salario).expect("r").expect("existe").nome,
        documento: SHA.to_owned(),
        contracheque: c,
        digitado_no_mes: None,
        digitado_depois: None,
    }
}

fn aplicar_rubricas(plan: &mut PlanejamentoSqlite, uso: &UsoDeRubricas) {
    for composicao in uso.composicoes(&uso.eventuais_iniciais()) {
        let versao_base = plan
            .recorrente(&uso.recorrente)
            .expect("r")
            .expect("existe")
            .versao_do_registro;
        plan.aplicar(
            Alteracao::DefinirComposicao {
                composicao,
                versao_base,
            },
            Autor::Usuario,
        )
        .expect("composicao");
    }
}

fn criar_descontos(s: &Sugestao, plan: &mut PlanejamentoSqlite) -> usize {
    let Sugestao::CriarDescontosEmFolha { uso, descontos, .. } = s else {
        panic!("{s:?}");
    };
    descontos::aplicar_descontos(plan, uso, descontos, &uso.eventuais_iniciais()).expect("criar")
}

#[test]
fn propoe_so_as_quatro_familias_sem_contrato_e_nunca_consignacao() {
    let mut plan = servico();
    let sal = criar(&mut plan, "Salário", "Entradas", Natureza::Entrada, 569_950);
    let u = uso(&plan, &sal, CONTRACHEQUE);
    let s = descontos::propor_descontos(&plan, &u)
        .expect("propor")
        .expect("há o que criar");
    let Sugestao::CriarDescontosEmFolha {
        rotulo, descontos, ..
    } = &s
    else {
        panic!("{s:?}");
    };
    assert_eq!(
        rotulo,
        "Criar compromissos de desconto em folha (INSS, Imposto de renda, FUNCEF, Saúde CAIXA)"
    );
    assert_eq!(descontos.len(), 4);
    assert!(descontos.iter().all(|d| d.ligar_a.is_none()));
    let por_nome = |n: &str| descontos.iter().find(|d| d.nome == n).expect(n);
    assert_eq!(por_nome("INSS").valor, Centavos(90_000));
    assert_eq!(por_nome("Imposto de renda").valor, Centavos(180_000));
    assert_eq!(por_nome("FUNCEF").valor, Centavos(60_000));
    assert_eq!(por_nome("Saúde CAIXA").valor, Centavos(25_000));
    // Propor não grava nada.
    assert_eq!(plan.recorrentes(false).expect("l").len(), 1);
}

#[test]
fn varias_rubricas_da_mesma_familia_viram_um_compromisso_so_as_fixas() {
    let mut plan = servico();
    let sal = criar(
        &mut plan,
        "Salário",
        "Entradas",
        Natureza::Entrada,
        2_000_000,
    );
    let u = uso(&plan, &sal, CONTRACHEQUE_2026);
    let Some(Sugestao::CriarDescontosEmFolha { descontos, .. }) =
        descontos::propor_descontos(&plan, &u).expect("propor")
    else {
        panic!("nada proposto");
    };
    let saude = descontos
        .iter()
        .find(|d| d.nome == "Saúde CAIXA")
        .expect("saúde");
    // O ajuste retroativo de 09/2026 (R$ 42,10) é eventual e fica de fora; as duas
    // mensalidades de 10/2026 (R$ 981,99 e R$ 318,00) viram um compromisso só.
    assert_eq!(saude.rubricas.len(), 2);
    assert_eq!(saude.valor, Centavos(98_199 + 31_800));
    // O imposto sobre a PLR e as consignações não são desconto fixo sem contrato.
    assert_eq!(descontos.len(), 4);
}

#[test]
fn confirmar_cria_o_grupo_liga_as_rubricas_e_o_previsto_segue_a_rubrica() {
    let mut plan = servico();
    let sal = criar(&mut plan, "Salário", "Entradas", Natureza::Entrada, 569_950);
    let u = uso(&plan, &sal, CONTRACHEQUE);
    aplicar_rubricas(&mut plan, &u);
    let s = descontos::propor_descontos(&plan, &u)
        .expect("propor")
        .expect("sugestão");
    assert_eq!(criar_descontos(&s, &mut plan), 4);

    let compromissos: Vec<_> = plan
        .recorrentes(false)
        .expect("l")
        .into_iter()
        .filter(|r| r.natureza == Natureza::DescontoEmFolha)
        .collect();
    assert_eq!(compromissos.len(), 4);
    assert!(compromissos.iter().all(|r| r.grupo == GRUPO));
    assert!(
        compromissos
            .iter()
            .all(|r| r.versoes[0].conta.as_deref() == Some("Salário"))
    );
    // As rubricas de desconto das quatro famílias apontam para os compromissos.
    let comp_outubro = plan
        .composicao(&sal, comp(2026, 10))
        .expect("c")
        .expect("existe");
    let ligadas = comp_outubro
        .rubricas
        .iter()
        .filter(|r| r.recorrente.is_some())
        .count();
    assert_eq!(ligadas, 4);
    // As consignações continuam sem ligação.
    assert!(
        comp_outubro
            .rubricas
            .iter()
            .filter(|r| r.nome.contains("CONSIGNACOES"))
            .all(|r| r.recorrente.is_none())
    );

    // O previsto de um mês futuro segue a rubrica vigente.
    let inss = compromissos
        .iter()
        .find(|r| r.nome == "INSS")
        .expect("inss");
    let o = &plan
        .proximas(&inss.id, comp(2027, 3), 1, hoje())
        .expect("p")[0];
    assert_eq!(o.previsto, Some(Centavos(90_000)));

    // Sem nada novo a criar, a sugestão some; e a baixa passa a ser oferecida.
    assert!(
        descontos::propor_descontos(&plan, &u)
            .expect("propor")
            .is_none()
    );
    let Some(Sugestao::DarBaixa { rotulo, baixas }) =
        descontos::sugestao_de_baixa(&plan, comp(2026, 10), SHA).expect("baixa")
    else {
        panic!("sem baixa");
    };
    assert_eq!(rotulo, "Dar baixa nos 4 descontos em folha");
    assert_eq!(baixas.len(), 4);
    for a in (Sugestao::DarBaixa { rotulo, baixas }).alteracoes(SHA, "c.pdf", 0, None) {
        plan.aplicar(a, Autor::Usuario).expect("baixa");
    }
    let o = &plan
        .proximas(&inss.id, comp(2026, 10), 1, hoje())
        .expect("p")[0];
    assert_eq!(o.estado, EstadoDaOcorrencia::Conciliada);
    assert_eq!(o.conciliado, Some(Centavos(90_000)));
    assert!(
        descontos::sugestao_de_baixa(&plan, comp(2026, 10), SHA)
            .expect("baixa")
            .is_none(),
        "já baixados"
    );
    // Outro contracheque (sha diferente) não herda a baixa deste.
    assert!(
        descontos::sugestao_de_baixa(&plan, comp(2026, 10), "outro")
            .expect("baixa")
            .is_none()
    );
}

#[test]
fn compromisso_que_ja_existe_com_o_nome_da_familia_e_ligado_nao_duplicado() {
    let mut plan = servico();
    let sal = criar(&mut plan, "Salário", "Entradas", Natureza::Entrada, 569_950);
    let inss = criar(
        &mut plan,
        "INSS",
        "Descontos",
        Natureza::DescontoEmFolha,
        80_000,
    );
    let u = uso(&plan, &sal, CONTRACHEQUE);
    let Some(s) = descontos::propor_descontos(&plan, &u).expect("propor") else {
        panic!("nada proposto");
    };
    let Sugestao::CriarDescontosEmFolha {
        rotulo, descontos, ..
    } = &s
    else {
        panic!("{s:?}");
    };
    assert_eq!(
        rotulo,
        "Criar 3 e ligar 1 compromissos de desconto em folha (INSS, Imposto de renda, FUNCEF, Saúde CAIXA)"
    );
    let do_inss = descontos.iter().find(|d| d.nome == "INSS").expect("inss");
    assert_eq!(do_inss.ligar_a.as_deref(), Some(inss.as_str()));
    assert_eq!(
        criar_descontos(&s, &mut plan),
        3,
        "o INSS não é criado de novo"
    );
    let o = &plan.proximas(&inss, comp(2026, 10), 1, hoje()).expect("p")[0];
    assert_eq!(o.previsto, Some(Centavos(90_000)), "agora segue a rubrica");
    assert_eq!(
        plan.recorrentes(false)
            .expect("l")
            .iter()
            .filter(|r| r.nome == "INSS")
            .count(),
        1
    );
}

#[test]
fn confirmar_as_rubricas_de_novo_nao_desliga_o_que_foi_ligado() {
    let mut plan = servico();
    let sal = criar(&mut plan, "Salário", "Entradas", Natureza::Entrada, 569_950);
    let u = uso(&plan, &sal, CONTRACHEQUE);
    // Os compromissos primeiro (que também gravam a composição ligada)...
    let s = descontos::propor_descontos(&plan, &u)
        .expect("p")
        .expect("s");
    criar_descontos(&s, &mut plan);
    // ...e depois "usar estas rubricas", como a janela faz, preservando as ligações.
    for mut composicao in u.composicoes(&u.eventuais_iniciais()) {
        if let Some(anterior) = plan.composicao(&sal, composicao.desde).expect("c") {
            descontos::preservar_ligacoes(&mut composicao, &anterior);
        }
        let versao_base = plan
            .recorrente(&sal)
            .expect("r")
            .expect("e")
            .versao_do_registro;
        plan.aplicar(
            Alteracao::DefinirComposicao {
                composicao,
                versao_base,
            },
            Autor::Usuario,
        )
        .expect("composicao");
    }
    let k = plan
        .composicao(&sal, comp(2026, 10))
        .expect("c")
        .expect("e");
    assert_eq!(
        k.rubricas.iter().filter(|r| r.recorrente.is_some()).count(),
        4
    );
    assert!(descontos::propor_descontos(&plan, &u).expect("p").is_none());
}

#[test]
fn contracheque_sem_desconto_fixo_nao_propoe_nada() {
    let mut plan = servico();
    let sal = criar(&mut plan, "Salário", "Entradas", Natureza::Entrada, 100_000);
    let mut u = uso(&plan, &sal, CONTRACHEQUE);
    u.contracheque
        .rubricas
        .retain(|r| r.tipo == sbf_planejamento::TipoDeRubrica::Provento);
    u.contracheque
        .eventuais
        .truncate(u.contracheque.rubricas.len());
    u.contracheque
        .informativas
        .truncate(u.contracheque.rubricas.len());
    assert!(descontos::propor_descontos(&plan, &u).expect("p").is_none());
}
