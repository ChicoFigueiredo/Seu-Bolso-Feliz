//! A marcação das listas de documentos e a passada de fundo (spec 15, D3 e D4).
//!
//! - **Marca**: para uma entrada (o sha256 do documento), quantos padrões distintos casaram
//!   em algum momento (de qualquer versão, ativa ou já desativada) e se a última decisão do
//!   conjunto foi Ambíguo. A janela pinta verde, âmbar ou deixa sem cor. A fonte é a tabela
//!   `execucao` (quem casou) e `decisao_da_entrada` (o âmbar).
//! - **Passada de fundo**: o ponto de parada de cada versão vigente, para o worker retomar
//!   de onde parou e repetir só a versão ativada ou alterada. Nada daqui grava decisão,
//!   proposta nem Caixa de entrada.

use std::collections::BTreeMap;

use rusqlite::params;

use crate::catalogo::{Catalogo, VersaoDeRegra, agora};
use crate::{Decisao, ErroDasRegras, Leitura};

type Resultado<T> = Result<T, ErroDasRegras>;

/// Quantas entradas uma consulta SQL leva por vez (o limite de variáveis do SQLite é maior,
/// mas pedaços pequenos mantêm a consulta rápida).
const PEDACO_DA_CONSULTA: usize = 400;

/// A cor de uma linha de documento.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CorDaMarca {
    /// Algum padrão casou em algum momento.
    Verde,
    /// A última decisão foi Ambíguo: revisão humana.
    Ambar,
    /// Nenhum padrão casou.
    #[default]
    SemCor,
}

/// O que se sabe de uma entrada pelos padrões.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Marca {
    /// Padrões (regras) distintos que casaram em algum momento.
    pub padroes: u32,
    /// A última decisão registrada foi Ambíguo.
    pub ambiguo: bool,
}

impl Marca {
    /// Âmbar vence verde: o ambíguo pede o olhar do usuário.
    #[must_use]
    pub fn cor(&self) -> CorDaMarca {
        if self.ambiguo {
            CorDaMarca::Ambar
        } else if self.padroes > 0 {
            CorDaMarca::Verde
        } else {
            CorDaMarca::SemCor
        }
    }
}

fn banco(e: impl std::fmt::Display) -> ErroDasRegras {
    ErroDasRegras::Banco(e.to_string())
}

fn interrogacoes(n: usize) -> String {
    vec!["?"; n].join(",")
}

impl Catalogo {
    /// A marca de cada entrada pedida, numa consulta em lote (por pedaços). Só entram no
    /// mapa as entradas com alguma marca (casou ou ambíguo): o que falta é sem cor.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn marcas(&self, entradas: &[String]) -> Resultado<BTreeMap<String, Marca>> {
        let mut mapa: BTreeMap<String, Marca> = BTreeMap::new();
        for pedaco in entradas.chunks(PEDACO_DA_CONSULTA) {
            let marcas = interrogacoes(pedaco.len());
            let mut st = self
                .conn
                .prepare_cached(&format!(
                    "SELECT entrada, COUNT(DISTINCT regra) FROM execucao
                      WHERE resultado = 'reconhecido' AND entrada IN ({marcas})
                      GROUP BY entrada"
                ))
                .map_err(banco)?;
            let linhas = st
                .query_map(rusqlite::params_from_iter(pedaco), |r| {
                    Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                })
                .map_err(banco)?;
            for l in linhas {
                let (entrada, n) = l.map_err(banco)?;
                mapa.entry(entrada).or_default().padroes = u32::try_from(n).unwrap_or(u32::MAX);
            }
            let mut st = self
                .conn
                .prepare_cached(&format!(
                    "SELECT entrada FROM decisao_da_entrada
                      WHERE ambiguo = 1 AND entrada IN ({marcas})"
                ))
                .map_err(banco)?;
            let linhas = st
                .query_map(rusqlite::params_from_iter(pedaco), |r| {
                    r.get::<_, String>(0)
                })
                .map_err(banco)?;
            for l in linhas {
                mapa.entry(l.map_err(banco)?).or_default().ambiguo = true;
            }
        }
        Ok(mapa)
    }

    /// Guarda a última decisão do conjunto de padrões sobre a entrada: Ambíguo ou não. Sem
    /// padrão nenhum (e sem linha anterior) não grava nada.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn registrar_decisao(&mut self, leitura: &Leitura) -> Resultado<()> {
        let ambiguo = matches!(leitura.decisao, Decisao::Ambiguo { .. });
        if ambiguo || !matches!(leitura.decisao, Decisao::Nenhuma) {
            self.conn
                .execute(
                    "INSERT INTO decisao_da_entrada (entrada, ambiguo, em) VALUES (?1, ?2, ?3)
                     ON CONFLICT(entrada) DO UPDATE SET ambiguo = ?2, em = ?3",
                    params![leitura.entrada, i64::from(ambiguo), agora()],
                )
                .map_err(banco)?;
        } else {
            // Sem padrão ativo que case agora: o âmbar antigo, se havia, não vale mais.
            self.conn
                .execute(
                    "UPDATE decisao_da_entrada SET ambiguo = 0, em = ?2 WHERE entrada = ?1",
                    params![leitura.entrada, agora()],
                )
                .map_err(banco)?;
        }
        Ok(())
    }

    /// O ponto de parada da passada de fundo para esta versão (o último `rowid` de `texto`
    /// já lido); 0 se a versão (com este checksum) ainda não começou.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn ponto_da_passada(&self, v: &VersaoDeRegra) -> Resultado<i64> {
        use rusqlite::OptionalExtension as _;
        self.conn
            .query_row(
                "SELECT ponto FROM passada_de_fundo
                  WHERE regra = ?1 AND versao = ?2 AND checksum = ?3",
                params![v.regra, v.versao, v.checksum],
                |r| r.get::<_, i64>(0),
            )
            .optional()
            .map_err(banco)
            .map(Option::unwrap_or_default)
    }

    /// Avança o ponto da passada de fundo desta versão e soma `feitos` entradas lidas.
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn avancar_a_passada(
        &mut self,
        v: &VersaoDeRegra,
        ponto: i64,
        feitos: u64,
    ) -> Resultado<()> {
        self.conn
            .execute(
                "INSERT INTO passada_de_fundo (regra, versao, checksum, ponto, feitos, atualizada_em)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(regra, versao, checksum) DO UPDATE SET
                   ponto = MAX(ponto, ?4), feitos = feitos + ?5, atualizada_em = ?6",
                params![
                    v.regra,
                    v.versao,
                    v.checksum,
                    ponto,
                    i64::try_from(feitos).unwrap_or(i64::MAX),
                    agora()
                ],
            )
            .map_err(banco)?;
        Ok(())
    }

    /// Quantas leituras a passada de fundo já fez nas versões vigentes agora (o `N` de
    /// `padroes: N de M`; o `M` é o número de documentos com texto vezes as versões
    /// vigentes, que só o inventário sabe).
    ///
    /// # Errors
    /// [`ErroDasRegras::Banco`].
    pub fn progresso_da_passada(&self) -> Resultado<u64> {
        let n: i64 = self
            .conn
            .query_row(
                "SELECT COALESCE(SUM(p.feitos), 0) FROM passada_de_fundo p
                   JOIN versao_de_regra v
                     ON v.regra = p.regra AND v.versao = p.versao AND v.checksum = p.checksum
                  WHERE v.estado IN ('ativa', 'consolidada')",
                [],
                |r| r.get(0),
            )
            .map_err(banco)?;
        Ok(u64::try_from(n).unwrap_or(0))
    }
}
