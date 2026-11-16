//! A prévia de "Adicionar compromisso": simular um CriarRecorrente mostra os meses que ele
//! cria e não grava nada.

use sbf_planejamento::*;

#[test]
fn simular_compromisso_novo_mostra_os_meses_e_nao_grava() {
    let p = PlanejamentoSqlite::novo(Banco::em_memoria().expect("banco"), Box::new(SemSaldos));
    let nov = Competencia::nova(2026, 11).expect("nov");
    let novo = Alteracao::CriarRecorrente {
        recorrente: NovoRecorrente {
            nome: "Presente Exemplo".into(),
            grupo: "Planos".into(),
            subgrupo: None,
            posicao: 1,
            natureza: Natureza::Saida,
            prazo: Prazo::SemFim,
            primeira_versao: Versao {
                id: String::new(),
                desde: nov,
                valor: Some(Centavos(20_000)),
                certeza: Certeza::Confirmado,
                fonte: Fonte::Usuario,
                frequencia: Frequencia::Unica {
                    data: Data::nova(2026, 11, 25).expect("data"),
                },
                conta: None,
                centro: None,
                fornecedor: None,
                categoria: None,
                nota: None,
            },
        },
        origem: None,
    };
    let hoje = Data::nova(2026, 11, 16).expect("hoje");
    let s = p.simular(&novo, hoje).expect("simular");
    assert_eq!(s.meses_afetados, vec![nov], "evento único: um mês só");
    assert_eq!(s.mudancas, vec![(nov, None, Some(Centavos(20_000)))]);
    assert!(p.recorrentes(true).expect("recorrentes").is_empty(), "simular não grava");
}
