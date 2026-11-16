//! `reprojetar` reproduz o estado (INV-05, teste de propriedade): uma sequência aleatória
//! de Ações, lotes e desfazeres leva a projeção ao mesmo estado que o log, reaplicado sobre
//! o banco de antes, produz.
#![allow(clippy::unwrap_used)]

mod nucleo_comum;

use nucleo_comum::*;
use proptest::prelude::*;
use rusqlite::types::Value as ValorSql;
use sbf_inventario::banco::Inventario;
use sbf_inventario::{Acao, Comandos, PedidoDeDecisao, Ref, Tipo};

#[derive(Debug, Clone)]
enum Passo {
    Executar { acao: u8, alvo: u8, extra: u8 },
    Desfazer(u8),
    DesfazerUltima,
    CriarLote,
    Adicionar { lote: u8, item: u8 },
    Remover { lote: u8, item: u8 },
    Aprovar(u8),
    Desaprovar(u8),
}

fn passo() -> impl Strategy<Value = Passo> {
    prop_oneof![
        6 => (any::<u8>(), any::<u8>(), any::<u8>())
            .prop_map(|(acao, alvo, extra)| Passo::Executar { acao, alvo, extra }),
        2 => any::<u8>().prop_map(Passo::Desfazer),
        2 => Just(Passo::DesfazerUltima),
        1 => Just(Passo::CriarLote),
        2 => (any::<u8>(), any::<u8>()).prop_map(|(lote, item)| Passo::Adicionar { lote, item }),
        1 => (any::<u8>(), any::<u8>()).prop_map(|(lote, item)| Passo::Remover { lote, item }),
        1 => any::<u8>().prop_map(Passo::Aprovar),
        1 => any::<u8>().prop_map(Passo::Desaprovar),
    ]
}

fn alvos() -> Vec<Ref> {
    let mut v: Vec<Ref> = (1..=4).map(achado).collect();
    v.extend((1..=3).map(registro));
    v.push(fornecedor("Energia Exemplo"));
    v.push(fornecedor("Banco Fictício"));
    v.extend((1..=3).map(|i| Ref::nova(Tipo::Pendencia, i.to_string())));
    v
}

fn itens() -> Vec<Ref> {
    let mut v: Vec<Ref> = (1..=4).map(achado).collect();
    v.extend((1..=3).map(registro));
    v
}

fn lote(n: u8) -> Ref {
    Ref::nova(Tipo::Lote, (n % 3 + 1).to_string())
}

fn acao(n: u8, extra: u8) -> Acao {
    let nomes = ["Água Exemplo", "Mercado Fictício", "Outro"];
    let nome = nomes[extra as usize % nomes.len()].to_owned();
    let datas = ["2026-10-19", "2026-10-20", "2027-02-28", "2024-02-29"];
    match n % 15 {
        0 => Acao::MarcarDocumento,
        1 => Acao::Descartar,
        2 => Acao::EmDuvida,
        3 => Acao::CorrigirFornecedor(nome),
        4 => Acao::CorrigirCategoria(nome),
        5 => Acao::CorrigirTipo(nome),
        6 => Acao::CorrigirDataEValor {
            data: (!extra.is_multiple_of(3))
                .then(|| datas[extra as usize % datas.len()].to_owned()),
            valor_centavos: extra
                .is_multiple_of(2)
                .then_some(i64::from(extra) * 100 - 9000),
        },
        7 => Acao::MarcarDuplicidade {
            com: achado(extra % 4 + 1),
        },
        8 => Acao::UnirConciliados {
            com: registro(i64::from(extra % 3 + 1)),
        },
        9 => Acao::ConfirmarRecorrencia,
        10 => Acao::Aprovar,
        11 => Acao::Rejeitar,
        12 => Acao::Sinalizar {
            pergunta: format!("pergunta {extra}"),
        },
        13 => Acao::Responder {
            resposta: format!("resposta {extra}"),
        },
        _ => Acao::GuardarRegraDeSenha {
            tipo_de_documento: ["fatura", "*"][extra as usize % 2].to_owned(),
            estrategia: ["cpf_completo", "cpf_5_primeiros"][extra as usize % 2].to_owned(),
            prioridade: [None, Some(i64::from(extra % 7))][extra as usize / 2 % 2],
        },
    }
}

fn contar_decisoes(inv: &Inventario) -> i64 {
    inv.conexao()
        .query_row("SELECT COUNT(*) FROM decisao", [], |l| l.get(0))
        .unwrap()
}

fn rodar(inv: &mut Inventario, passos: &[Passo]) {
    let alvos = alvos();
    let itens = itens();
    for p in passos {
        let antes = contar_decisoes(inv);
        match p {
            Passo::Executar {
                acao: a,
                alvo,
                extra,
            } => {
                let pedido = PedidoDeDecisao {
                    acao: acao(*a, *extra),
                    alvos: vec![alvos[*alvo as usize % alvos.len()].clone()],
                    motivo: extra
                        .is_multiple_of(2)
                        .then(|| "motivo de teste".to_owned()),
                };
                if let Ok(ids) = inv.executar(&pedido) {
                    // Toda Ação que dá certo grava uma decisão por efeito.
                    assert!(!ids.is_empty());
                    assert_eq!(contar_decisoes(inv), antes + ids.len() as i64);
                }
            }
            Passo::Desfazer(n) => {
                let ids: Vec<i64> = {
                    let mut c = inv.conexao().prepare("SELECT id FROM decisao").unwrap();
                    c.query_map([], |l| l.get(0))
                        .unwrap()
                        .collect::<Result<_, _>>()
                        .unwrap()
                };
                if !ids.is_empty() {
                    let _ = inv.desfazer(ids[*n as usize % ids.len()]);
                }
            }
            Passo::DesfazerUltima => {
                let _ = inv.desfazer_ultima();
            }
            Passo::CriarLote => {
                let _ = inv.criar_lote("lote de teste");
            }
            Passo::Adicionar { lote: l, item } => {
                let _ = inv
                    .adicionar_ao_lote(&lote(*l), &[itens[*item as usize % itens.len()].clone()]);
            }
            Passo::Remover { lote: l, item } => {
                let _ =
                    inv.remover_do_lote(&lote(*l), &[itens[*item as usize % itens.len()].clone()]);
            }
            Passo::Aprovar(l) => {
                let _ = inv.aprovar_lote(&lote(*l));
            }
            Passo::Desaprovar(l) => {
                let _ = inv.desaprovar_lote(&lote(*l));
            }
        }
        // Um passo que falha não deixa decisão pela metade.
        assert!(contar_decisoes(inv) >= antes);
    }
}

/// Copia o log de `de` para `para` (as linhas, como estão).
fn copiar_log(de: &Inventario, para: &Inventario) {
    let mut consulta = de
        .conexao()
        .prepare(
            "SELECT id, alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz
               FROM decisao ORDER BY id",
        )
        .unwrap();
    let linhas: Vec<Vec<ValorSql>> = consulta
        .query_map([], |l| (0..10).map(|i| l.get::<_, ValorSql>(i)).collect())
        .unwrap()
        .collect::<Result<_, _>>()
        .unwrap();
    for linha in linhas {
        para.conexao()
            .execute(
                "INSERT INTO decisao (id, alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                rusqlite::params_from_iter(linha),
            )
            .unwrap();
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 64, ..ProptestConfig::default() })]

    #[test]
    fn inv05_reprojetar_reproduz_o_estado(passos in prop::collection::vec(passo(), 1..40)) {
        let pasta = tempfile::tempdir().unwrap();
        let mut feito = inventario_migrado(&pasta.path().join("a").tap_mkdir());
        rodar(&mut feito, &passos);
        let esperado = retrato(feito.conexao());

        // Sobre o próprio banco, reprojetar não muda nada.
        feito.reprojetar().unwrap();
        prop_assert_eq!(&retrato(feito.conexao()), &esperado);

        // Sobre o banco de antes, o log reaplicado chega ao mesmo estado.
        let mut refeito = inventario_migrado(&pasta.path().join("b").tap_mkdir());
        copiar_log(&feito, &refeito);
        refeito.reprojetar().unwrap();
        prop_assert_eq!(&retrato(refeito.conexao()), &esperado);
    }
}

trait CriarPasta {
    fn tap_mkdir(self) -> Self;
}

impl CriarPasta for std::path::PathBuf {
    fn tap_mkdir(self) -> Self {
        std::fs::create_dir_all(&self).unwrap();
        self
    }
}
