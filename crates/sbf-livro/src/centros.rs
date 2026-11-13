//! Saldo do centro e não alocado: quanto foi para cada centro e quanto ainda não foi para
//! centro nenhum.
//!
//! Portado de `apps/web/src/app/actions/painel-centros.ts` (`carregarPainelCentros`) e
//! `financial-centers.ts` (`listFinancialCentersWithBalance`, que usa `centerBalance` de
//! `packages/domain/src/allocation/center-balance.ts`):
//!
//! - **Saldo do centro:** Σ rateios de entradas − Σ rateios de saídas, desde sempre; no
//!   ciclo, o mesmo restrito às datas do ciclo.
//! - **Não alocado:** valor (módulo) das saídas rateáveis do ciclo − Σ rateios delas (o
//!   rateio nunca passa da movimentação). Receita, estorno e pagamento de fatura ficam fora.
//!
//! **Divergência intencional (grill Q15):** o ajuste não entra no saldo do centro nem no
//! não alocado. O TS o conta como saída nos dois. Teste de ouro: `tests/ouro_paineis.rs`.

use std::collections::HashMap;

use crate::ciclo::Ciclo;
use crate::data::Data;
use crate::dinheiro::Centavos;
use crate::rateio::{Direcao, aceita_rateio, direcao_no_centro};
use crate::tipos::{Centro, Id, LivroCaixa, StatusDoCentro, TipoDeMovimentacao};

/// O centro como conta corrente virtual.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SaldoDoCentro {
    pub entrou: Centavos,
    pub saiu: Centavos,
    /// Entrou − saiu, desde sempre. Pode ser negativo.
    pub saldo: Centavos,
    pub entrou_no_ciclo: Centavos,
    pub saiu_no_ciclo: Centavos,
    /// O resultado do ciclo do centro.
    pub resultado_no_ciclo: Centavos,
    /// Quantos rateios apontam para o centro (todos, inclusive de ajuste). Zero libera
    /// excluir.
    pub rateios: u32,
}

/// Um centro com o saldo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CentroComSaldo {
    pub centro: Centro,
    pub saldo: SaldoDoCentro,
}

/// Uma movimentação com parte não alocada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovimentacaoNaoAlocada {
    pub id: Id,
    pub descricao: String,
    pub data: Data,
    pub tipo: TipoDeMovimentacao,
    /// Valor cheio, sempre positivo.
    pub valor: Centavos,
    pub alocado: Centavos,
    pub nao_alocado: Centavos,
    pub conta: Option<String>,
}

/// O não alocado do ciclo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NaoAlocado {
    pub total: Centavos,
    /// Σ saídas rateáveis do ciclo: a base da comparação.
    pub saidas_do_ciclo: Centavos,
    /// Quantas têm alguma parte sem centro.
    pub movimentacoes: u32,
    /// Todas elas, da mais recente para a mais antiga.
    pub pendentes: Vec<MovimentacaoNaoAlocada>,
}

/// A tela Centros.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PainelCentros {
    pub ciclo: Ciclo,
    /// Por nome.
    pub ativos: Vec<CentroComSaldo>,
    pub arquivados: Vec<CentroComSaldo>,
    pub nao_alocado: NaoAlocado,
}

/// O saldo de cada centro (todos os centros do livro, mesmo sem rateio).
pub fn saldos_dos_centros(livro: &LivroCaixa, ciclo: &Ciclo) -> HashMap<Id, SaldoDoCentro> {
    let movs: HashMap<&str, (TipoDeMovimentacao, Data)> = livro
        .movimentacoes
        .iter()
        .map(|m| (m.id.as_str(), (m.tipo, m.data)))
        .collect();
    let mut saldos: HashMap<Id, SaldoDoCentro> = livro
        .centros
        .iter()
        .map(|c| (c.id.clone(), SaldoDoCentro::default()))
        .collect();
    for r in &livro.rateios {
        let s = saldos.entry(r.centro_id.clone()).or_default();
        s.rateios += 1;
        let Some((tipo, data)) = movs.get(r.movimentacao_id.as_str()) else {
            continue;
        };
        let no_ciclo = ciclo.contem(*data);
        match direcao_no_centro(*tipo) {
            Some(Direcao::Entrada) => {
                s.entrou += r.valor;
                if no_ciclo {
                    s.entrou_no_ciclo += r.valor;
                }
            }
            Some(Direcao::Saida) => {
                s.saiu += r.valor;
                if no_ciclo {
                    s.saiu_no_ciclo += r.valor;
                }
            }
            None => {}
        }
    }
    for s in saldos.values_mut() {
        s.saldo = s.entrou - s.saiu;
        s.resultado_no_ciclo = s.entrou_no_ciclo - s.saiu_no_ciclo;
    }
    saldos
}

/// O não alocado do ciclo: saídas que aceitam rateio (sem receita, estorno, pagamento de
/// fatura e ajuste).
pub fn nao_alocado(livro: &LivroCaixa, ciclo: &Ciclo) -> NaoAlocado {
    let mut alocado: HashMap<&str, Centavos> = HashMap::new();
    for r in &livro.rateios {
        *alocado.entry(r.movimentacao_id.as_str()).or_default() += r.valor;
    }
    let nome_da_conta: HashMap<&str, &str> = livro
        .contas
        .iter()
        .map(|c| (c.id.as_str(), c.nome.as_str()))
        .collect();
    let mut resultado = NaoAlocado {
        total: Centavos::ZERO,
        saidas_do_ciclo: Centavos::ZERO,
        movimentacoes: 0,
        pendentes: Vec::new(),
    };
    let mut ja_alocado = Centavos::ZERO;
    for m in livro.movimentacoes.iter().filter(|m| {
        ciclo.contem(m.data)
            && direcao_no_centro(m.tipo) == Some(Direcao::Saida)
            && aceita_rateio(m.tipo)
    }) {
        let valor = m.valor.abs();
        let feito = alocado
            .get(m.id.as_str())
            .copied()
            .unwrap_or_default()
            .min(valor);
        resultado.saidas_do_ciclo += valor;
        ja_alocado += feito;
        if feito < valor {
            resultado.movimentacoes += 1;
            resultado.pendentes.push(MovimentacaoNaoAlocada {
                id: m.id.clone(),
                descricao: m
                    .descricao
                    .clone()
                    .unwrap_or_else(|| "Sem descrição".to_owned()),
                data: m.data,
                tipo: m.tipo,
                valor,
                alocado: feito,
                nao_alocado: valor - feito,
                conta: nome_da_conta
                    .get(m.conta_id.as_str())
                    .map(|n| (*n).to_owned()),
            });
        }
    }
    resultado.total = (resultado.saidas_do_ciclo - ja_alocado).ou_zero();
    resultado
        .pendentes
        .sort_by(|a, b| (b.data, &a.id).cmp(&(a.data, &b.id)));
    resultado
}

/// A tela Centros no ciclo.
pub fn painel_centros(livro: &LivroCaixa, ciclo: &Ciclo) -> PainelCentros {
    let saldos = saldos_dos_centros(livro, ciclo);
    let mut centros: Vec<CentroComSaldo> = livro
        .centros
        .iter()
        .map(|c| CentroComSaldo {
            centro: c.clone(),
            saldo: saldos.get(&c.id).copied().unwrap_or_default(),
        })
        .collect();
    centros.sort_by(|a, b| (&a.centro.nome, &a.centro.id).cmp(&(&b.centro.nome, &b.centro.id)));
    let (ativos, arquivados) = centros
        .into_iter()
        .partition(|c| c.centro.status == StatusDoCentro::Ativo);
    PainelCentros {
        ciclo: *ciclo,
        ativos,
        arquivados,
        nao_alocado: nao_alocado(livro, ciclo),
    }
}
