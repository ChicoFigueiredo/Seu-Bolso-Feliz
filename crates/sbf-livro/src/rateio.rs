//! De que lado do centro uma movimentação entra, e quais aceitam rateio.
//!
//! Portado de `packages/domain/src/allocation/transaction-direction.ts`. Entram no centro
//! `income` e `refund`; o resto sai; tipo desconhecido sai (o lado conservador).
//! `statement_payment` não aceita rateio: as compras da fatura já foram rateadas.
//!
//! **Divergência intencional (grill Q15):** `adjustment` não entra em lado nenhum, nem
//! no resultado, nem no saldo do centro, nem no não alocado; por isso também não aceita
//! rateio. O TS o conta como saída do centro e aceita rateio nele.
//! Teste de ouro: `tests/ouro_rateio.rs`.

use crate::tipos::TipoDeMovimentacao;

/// O lado do centro que a movimentação alimenta.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Direcao {
    Entrada,
    Saida,
}

/// O lado do centro; `None` para o ajuste, que fica fora (grill Q15).
pub fn direcao_no_centro(tipo: TipoDeMovimentacao) -> Option<Direcao> {
    use TipoDeMovimentacao as T;
    match tipo {
        T::Receita | T::Estorno => Some(Direcao::Entrada),
        T::Ajuste => None,
        T::Despesa | T::Juros | T::Tarifa | T::PagamentoDeFatura | T::PagamentoDeDivida => {
            Some(Direcao::Saida)
        }
    }
}

/// O mesmo, a partir do código do banco. Código desconhecido conta como saída, como no TS.
pub fn direcao_do_codigo(codigo: &str) -> Option<Direcao> {
    TipoDeMovimentacao::de_codigo(codigo).map_or(Some(Direcao::Saida), direcao_no_centro)
}

/// Se a movimentação pode ser rateada entre centros.
pub fn aceita_rateio(tipo: TipoDeMovimentacao) -> bool {
    !matches!(
        tipo,
        TipoDeMovimentacao::PagamentoDeFatura | TipoDeMovimentacao::Ajuste
    )
}

/// Por que a movimentação não aceita rateio, em português, para a tela explicar antes.
pub fn motivo_de_nao_aceitar_rateio(tipo: TipoDeMovimentacao) -> Option<&'static str> {
    match tipo {
        TipoDeMovimentacao::PagamentoDeFatura => Some(
            "Pagamento de fatura não aceita rateio: as compras dentro da fatura já foram rateadas uma a uma. Ratear o pagamento contaria o mesmo dinheiro duas vezes no centro.",
        ),
        TipoDeMovimentacao::Ajuste => Some(
            "Ajuste de saldo não aceita rateio: ele corrige o saldo da conta e não entra no resultado nem no saldo de nenhum centro.",
        ),
        _ => None,
    }
}
