//! O gasto do livro-caixa para os Relatórios, **pela data da compra** (grill Q18).
//!
//! Gasto é dinheiro que saiu para fora do bolso, contado uma vez:
//! - **compras no cartão**, no ciclo da data da compra: a movimentação com fatura, e o item
//!   de fatura que ainda não virou movimentação;
//! - **saídas de conta** confirmadas: despesa, tarifa, juros e pagamento de dívida.
//!
//! Fica fora: pagamento de fatura (as compras dentro dela já contaram), transferência entre
//! contas, ajuste (grill Q15), receita e estorno, e movimentação não confirmada (é
//! compromisso, ainda não saiu).
//!
//! A referência mais próxima no web é a view `v_expenses_deduplicated` (itens de fatura sem
//! movimentação + `expense`, `fee`, `interest_charge`). Diferenças intencionais: o pagamento
//! de dívida conta (é saída de conta, grill Q18) e a movimentação não confirmada não conta.
//! Teste de ouro: `tests/ouro_paineis.rs`, contra a consulta de referência de
//! `ouro/paineis.ts` no mesmo banco de teste.

use std::collections::HashMap;

use crate::data::Data;
use crate::dinheiro::Centavos;
use crate::tipos::{Id, LivroCaixa, TipoDeMovimentacao};

/// De onde veio um gasto.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum OrigemDoGasto {
    Movimentacao(Id),
    ItemDeFatura(Id),
}

/// Um gasto: o que saiu, quando foi comprado e com quem.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GastoDoLivro {
    pub origem: OrigemDoGasto,
    /// A data da compra (ou do débito na conta).
    pub data: Data,
    /// Sempre positivo.
    pub valor: Centavos,
    pub fornecedor_id: Option<Id>,
    pub categoria_id: Option<Id>,
    /// A conta (produto) de onde saiu; na compra no cartão, o produto do cartão.
    pub conta_id: Option<Id>,
}

/// Se o tipo conta como gasto.
pub fn e_gasto(tipo: TipoDeMovimentacao) -> bool {
    use TipoDeMovimentacao as T;
    matches!(
        tipo,
        T::Despesa | T::Tarifa | T::Juros | T::PagamentoDeDivida
    )
}

/// Todos os gastos do livro, pela data da compra.
pub fn gastos_pela_data_da_compra(livro: &LivroCaixa) -> Vec<GastoDoLivro> {
    let mut gastos: Vec<GastoDoLivro> = livro
        .movimentacoes
        .iter()
        .filter(|m| m.confirmada && e_gasto(m.tipo))
        .map(|m| GastoDoLivro {
            origem: OrigemDoGasto::Movimentacao(m.id.clone()),
            data: m.data,
            valor: m.valor.abs(),
            fornecedor_id: m.fornecedor_id.clone(),
            categoria_id: m.categoria_id.clone(),
            conta_id: Some(m.conta_id.clone()),
        })
        .collect();

    let produto_do_cartao: HashMap<&str, &str> = livro
        .cartoes
        .iter()
        .map(|c| (c.id.as_str(), c.conta_id.as_str()))
        .collect();
    let faturas: HashMap<&str, &crate::tipos::Fatura> =
        livro.faturas.iter().map(|f| (f.id.as_str(), f)).collect();
    for item in livro
        .itens_de_fatura
        .iter()
        .filter(|i| i.movimentacao_id.is_none())
    {
        let fatura = faturas.get(item.fatura_id.as_str());
        // Sem data da compra, o fechamento da fatura é a melhor aproximação.
        let Some(data) = item.data_da_compra.or(fatura.map(|f| f.fechamento)) else {
            continue;
        };
        gastos.push(GastoDoLivro {
            origem: OrigemDoGasto::ItemDeFatura(item.id.clone()),
            data,
            valor: item.valor.abs(),
            fornecedor_id: item.fornecedor_id.clone(),
            categoria_id: None,
            conta_id: fatura
                .and_then(|f| produto_do_cartao.get(f.cartao_id.as_str()))
                .map(|p| (*p).to_owned()),
        });
    }
    gastos.sort_by_key(|g| g.data);
    gastos
}

#[cfg(feature = "relatorios")]
pub use relatorios::GastosDoLivro;

#[cfg(feature = "relatorios")]
mod relatorios {
    use std::collections::BTreeMap;

    use sbf_inventario::consulta::ResultadoDeConsulta;
    use sbf_inventario::relatorios::{FonteDeGastos, Gasto};

    use super::gastos_pela_data_da_compra;
    use crate::ciclo::Ciclo;
    use crate::tipos::LivroCaixa;

    /// O livro-caixa como fonte das 8 perguntas dos Relatórios (`responder_com`).
    ///
    /// - O ciclo é o do corte do livro, pelo nome `AAAA-MM`.
    /// - `fornecedor`, `categoria` e `conta` saem pelo **nome** (o `Gasto` é texto e as
    ///   perguntas mostram e agrupam por ele); sem categoria vira `Sem categoria`.
    /// - `fontes`, quando não vazio, filtra pela conta: vale o UUID ou o nome.
    #[derive(Debug, Clone, Copy)]
    pub struct GastosDoLivro<'a> {
        pub livro: &'a LivroCaixa,
    }

    impl<'a> GastosDoLivro<'a> {
        pub fn novo(livro: &'a LivroCaixa) -> GastosDoLivro<'a> {
            GastosDoLivro { livro }
        }
    }

    impl FonteDeGastos for GastosDoLivro<'_> {
        fn gastos(
            &self,
            de: &str,
            ate: &str,
            fontes: &[String],
        ) -> ResultadoDeConsulta<Vec<Gasto>> {
            let livro = self.livro;
            let nome = |lista: &str, id: &str| -> Option<String> {
                match lista {
                    "fornecedor" => livro
                        .fornecedores
                        .iter()
                        .find(|f| f.id == id)
                        .map(|f| f.nome.clone()),
                    "categoria" => livro
                        .categorias
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| c.nome.clone()),
                    _ => livro
                        .contas
                        .iter()
                        .find(|c| c.id == id)
                        .map(|c| c.nome.clone()),
                }
            };
            type Chave = (String, Option<String>, String, Option<String>);
            let mut grupos: BTreeMap<Chave, (i64, i64)> = BTreeMap::new();
            for g in gastos_pela_data_da_compra(livro) {
                let ciclo = Ciclo::da_data(g.data, livro.corte).nome();
                if ciclo.as_str() < de || ciclo.as_str() > ate {
                    continue;
                }
                let conta = g.conta_id.as_deref().and_then(|id| nome("conta", id));
                if !fontes.is_empty()
                    && !fontes.iter().any(|f| {
                        Some(f.as_str()) == g.conta_id.as_deref()
                            || Some(f.as_str()) == conta.as_deref()
                    })
                {
                    continue;
                }
                let chave = (
                    ciclo,
                    g.fornecedor_id
                        .as_deref()
                        .and_then(|id| nome("fornecedor", id)),
                    g.categoria_id
                        .as_deref()
                        .and_then(|id| nome("categoria", id))
                        .unwrap_or_else(|| "Sem categoria".to_owned()),
                    conta,
                );
                let e = grupos.entry(chave).or_default();
                e.0 += g.valor.0;
                e.1 += 1;
            }
            Ok(grupos
                .into_iter()
                .map(
                    |((ciclo, fornecedor, categoria, conta), (centavos, vezes))| Gasto {
                        ciclo,
                        fornecedor,
                        categoria,
                        conta,
                        centavos,
                        vezes,
                    },
                )
                .collect())
        }
    }
}
