//! O saldo das contas: da Pluggy para `financial_products.current_balance` (`saldos.ts`).
//!
//! Só contas `BANK` (corrente/poupança): o `balance` de cartão na Pluggy é o aberto da fatura,
//! não "dinheiro disponível". Saldo negativo entra como veio (cheque especial é posição real).
//! Só mapeamentos com produto vinculado e não ignorados.

use uuid::Uuid;

use super::cliente::ContaNormalizada;

#[derive(Debug, Clone)]
pub struct Mapeamento {
    pub id_externo_da_conta: String,
    pub produto: Option<Uuid>,
    /// `pending_mapping`, `mapped` ou `ignored`.
    pub situacao: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AtualizacaoDeSaldo {
    pub produto: Uuid,
    pub centavos: i64,
}

pub fn planejar_saldos(
    contas: &[ContaNormalizada],
    mapeamentos: &[Mapeamento],
) -> Vec<AtualizacaoDeSaldo> {
    let mut plano = Vec::new();
    for m in mapeamentos {
        let Some(produto) = m.produto else { continue };
        if m.situacao == "ignored" {
            continue;
        }
        // Conta que saiu da Pluggy: não se inventa saldo.
        let Some(conta) = contas
            .iter()
            .find(|c| c.id_externo == m.id_externo_da_conta)
        else {
            continue;
        };
        if !conta.tipo.eq_ignore_ascii_case("BANK") {
            continue;
        }
        let Some(centavos) = conta.saldo_em_centavos else {
            continue;
        };
        plano.push(AtualizacaoDeSaldo { produto, centavos });
    }
    plano
}

#[cfg(test)]
mod testes {
    use super::*;

    fn conta(id: &str, tipo: &str, saldo: Option<i64>) -> ContaNormalizada {
        ContaNormalizada {
            id_externo: id.into(),
            tipo: tipo.into(),
            saldo_em_centavos: saldo,
        }
    }

    fn mapa(id: &str, produto: Option<u128>, situacao: &str) -> Mapeamento {
        Mapeamento {
            id_externo_da_conta: id.into(),
            produto: produto.map(Uuid::from_u128),
            situacao: situacao.into(),
        }
    }

    #[test]
    fn so_conta_corrente_mapeada_e_nao_ignorada() {
        let contas = vec![
            conta("c1", "BANK", Some(123_456)),
            conta("c2", "CREDIT", Some(99_900)),
            conta("c3", "bank", Some(-5_000)),
            conta("c4", "BANK", None),
            conta("c5", "BANK", Some(1)),
            conta("c6", "BANK", Some(2)),
        ];
        let mapas = vec![
            mapa("c1", Some(1), "mapped"),
            mapa("c2", Some(2), "mapped"),
            mapa("c3", Some(3), "pending_mapping"),
            mapa("c4", Some(4), "mapped"),
            mapa("c5", None, "mapped"),
            mapa("c6", Some(6), "ignored"),
            mapa("c9", Some(9), "mapped"),
        ];
        assert_eq!(
            planejar_saldos(&contas, &mapas),
            vec![
                AtualizacaoDeSaldo {
                    produto: Uuid::from_u128(1),
                    centavos: 123_456
                },
                AtualizacaoDeSaldo {
                    produto: Uuid::from_u128(3),
                    centavos: -5_000
                },
            ]
        );
    }
}
