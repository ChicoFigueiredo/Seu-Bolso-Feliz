//! A análise: roda as regras e entrega os candidatos. Sem IA, sem rede, só leitura.

use std::path::Path;

use rusqlite::Connection;
use sbf_livro::{Ciclo, Data, DiaDeCorte};

use crate::armazem::{Armazem, Resumo};
use crate::erro::Resultado;
use crate::regras;
use crate::tipos::Candidato;

/// Os limites das regras. O padrão é o do produto; os testes ajustam.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Limites {
    /// Aumento mínimo (percentual) para a assinatura contar como reajustada.
    pub reajuste_minimo_pct: i64,
    /// Aumento mínimo em centavos.
    pub reajuste_minimo_centavos: i64,
    /// Meses seguidos de cobrança sem documento.
    pub meses_sem_documento: usize,
    /// Tarifas no ciclo: a partir de quantos centavos e de quantas cobranças.
    pub tarifas_minimo_centavos: i64,
    pub tarifas_minimo_cobrancas: usize,
    /// Dias parado em A triar ou Em dúvida.
    pub dias_parado: i64,
    /// Varreduras seguidas que falharam.
    pub falhas_seguidas: usize,
    /// Tokens da semana abaixo dos quais o consumo nunca é "acima do ritmo".
    pub ia_tokens_minimos: i64,
    /// Sugestões abertas a partir das quais a fila preocupa.
    pub fila_minima: i64,
    /// Quanto as criadas na semana precisam passar das confirmadas.
    pub fila_folga: i64,
}

impl Default for Limites {
    fn default() -> Self {
        Self {
            reajuste_minimo_pct: 5,
            reajuste_minimo_centavos: 100,
            meses_sem_documento: 3,
            tarifas_minimo_centavos: 3000,
            tarifas_minimo_cobrancas: 2,
            dias_parado: 30,
            falhas_seguidas: 3,
            ia_tokens_minimos: 50_000,
            fila_minima: 20,
            fila_folga: 20,
        }
    }
}

/// Tudo o que a análise lê.
#[derive(Debug, Clone, Copy)]
pub struct Contexto<'a> {
    pub inventario: &'a Connection,
    /// O `livro-caixa.sqlite` da estação. `None` ou inexistente: as regras de dinheiro não
    /// rodam (e não fecham pontos).
    pub cache_do_livro: Option<&'a Path>,
    pub hoje: Data,
    /// O volume do cofre é cifrado? `None`: não se sabe, a regra não roda.
    pub disco_cifrado: Option<bool>,
    /// O nome do provedor reserva de Opções › IA (por exemplo `openrouter`).
    pub provedor_reserva: Option<&'a str>,
    pub corte: DiaDeCorte,
}

impl<'a> Contexto<'a> {
    /// Um contexto com o corte padrão (dia 20) e nada opcional.
    #[must_use]
    pub fn novo(inventario: &'a Connection, hoje: Data) -> Self {
        Self {
            inventario,
            cache_do_livro: None,
            hoje,
            disco_cifrado: None,
            provedor_reserva: None,
            corte: DiaDeCorte::da_preferencia(None),
        }
    }

    #[must_use]
    pub fn ciclo(&self) -> Ciclo {
        Ciclo::da_data(self.hoje, self.corte)
    }
}

/// O resultado de uma análise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Analise {
    pub candidatos: Vec<Candidato>,
    /// As regras que rodaram de verdade.
    pub avaliadas: Vec<&'static str>,
}

/// Roda todas as regras.
///
/// # Errors
/// Falha do SQLite ou do cache.
pub fn analisar(ctx: &Contexto<'_>, limites: &Limites) -> Resultado<Analise> {
    let mut a = Analise::default();
    regras::dinheiro::rodar(ctx, limites, &mut a)?;
    regras::operacao::rodar(ctx, limites, &mut a)?;
    regras::custo_ia::rodar(ctx, limites, &mut a)?;
    regras::tempo::rodar(ctx, limites, &mut a)?;
    Ok(a)
}

/// Analisa e grava em `pontos.sqlite`. É o que o `sbf worker` roda.
///
/// # Errors
/// Falha do SQLite ou do cache.
pub fn rodar(
    armazem: &mut Armazem,
    ctx: &Contexto<'_>,
    limites: &Limites,
    agora: &str,
) -> Resultado<Resumo> {
    let a = analisar(ctx, limites)?;
    armazem.aplicar(&a.candidatos, &a.avaliadas, agora)
}
