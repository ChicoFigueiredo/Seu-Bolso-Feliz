//! O que a janela usa dos pontos de atenção: listar, contar, resolver, ignorar e analisar
//! agora. [`PontosLocais`] implementa sobre o [`Armazem`]; a análise de verdade (que sabe
//! onde estão o inventário e o cache do livro) vem de fora, como função.

use std::collections::HashMap;

use crate::armazem::{Armazem, Filtro, Resumo};
use crate::tipos::{Area, Ponto};

/// O serviço da área "Pontos de atenção". Erros em texto para a tela.
pub trait ServicoDePontos: Send {
    /// # Errors
    /// Texto para mostrar.
    fn listar(&mut self, filtro: Filtro) -> Result<Vec<Ponto>, String>;
    /// Quantos pontos abertos há por área.
    ///
    /// # Errors
    /// Texto para mostrar.
    fn abertos_por_area(&mut self) -> Result<HashMap<Area, usize>, String>;
    /// # Errors
    /// Motivo vazio, ponto já fechado.
    fn resolver(&mut self, id: i64, motivo: &str) -> Result<(), String>;
    /// # Errors
    /// Motivo vazio, ponto já fechado.
    fn ignorar(&mut self, id: i64, motivo: &str) -> Result<(), String>;
    /// Roda a análise agora (o botão "Analisar agora"; o worker faz de 6 em 6 horas).
    ///
    /// # Errors
    /// Texto para mostrar.
    fn analisar_agora(&mut self) -> Result<Resumo, String>;
}

type Analise = Box<dyn FnMut(&mut Armazem) -> Result<Resumo, String> + Send>;

/// O [`ServicoDePontos`] sobre um [`Armazem`] e uma função de análise.
pub struct PontosLocais {
    armazem: Armazem,
    analise: Analise,
}

impl std::fmt::Debug for PontosLocais {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PontosLocais").finish_non_exhaustive()
    }
}

impl PontosLocais {
    #[must_use]
    pub fn novo(
        armazem: Armazem,
        analise: impl FnMut(&mut Armazem) -> Result<Resumo, String> + Send + 'static,
    ) -> Self {
        Self {
            armazem,
            analise: Box::new(analise),
        }
    }
}

fn agora() -> String {
    let s = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // RFC 3339 UTC, sem depender do sbf-ia: dias desde 1970 pelo algoritmo de Hinnant.
    let (dias, resto) = (i64::try_from(s / 86_400).unwrap_or(0), s % 86_400);
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + i64::from(mes <= 2);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}:{:02}:{:02}Z",
        resto / 3600,
        (resto % 3600) / 60,
        resto % 60
    )
}

impl ServicoDePontos for PontosLocais {
    fn listar(&mut self, filtro: Filtro) -> Result<Vec<Ponto>, String> {
        self.armazem.listar(filtro).map_err(|e| e.to_string())
    }

    fn abertos_por_area(&mut self) -> Result<HashMap<Area, usize>, String> {
        self.armazem.abertos_por_area().map_err(|e| e.to_string())
    }

    fn resolver(&mut self, id: i64, motivo: &str) -> Result<(), String> {
        self.armazem
            .resolver(id, motivo, &agora())
            .map_err(|e| e.to_string())
    }

    fn ignorar(&mut self, id: i64, motivo: &str) -> Result<(), String> {
        self.armazem
            .ignorar(id, motivo, &agora())
            .map_err(|e| e.to_string())
    }

    fn analisar_agora(&mut self) -> Result<Resumo, String> {
        (self.analise)(&mut self.armazem)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn agora_tem_o_formato_rfc3339() {
        let a = agora();
        assert_eq!(a.len(), 20);
        assert!(a.starts_with("20") && a.ends_with('Z') && a.as_bytes()[10] == b'T');
    }
}
