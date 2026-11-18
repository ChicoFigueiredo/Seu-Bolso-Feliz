//! A cola do OCR das imagens novas: monta, sobre o `sbf-ocr`, o que o `sbf worker` usa.
//! Só lê e grava texto no inventário; nada sai da máquina.

use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

use sbf_ocr::{LOTE_PADRAO, Ocr, ler_imagens_novas};
use sbf_worker::laco::{Dados, LeitorDeImagens, ResumoDoOcr};
use sbf_worker::registro::Falha;

/// O leitor de imagens do worker: o tesseract do sistema, um lote por volta.
#[derive(Debug)]
pub struct LeitorReal {
    dados: Dados,
    ocr: Ocr,
    /// Imagens que falharam nesta execução (não são tentadas a cada volta).
    falharam: HashSet<String>,
}

impl LeitorReal {
    #[must_use]
    pub fn novo(dados: Dados) -> Self {
        Self {
            dados,
            ocr: Ocr::do_sistema(),
            falharam: HashSet::new(),
        }
    }
}

impl LeitorDeImagens for LeitorReal {
    fn ler_novas(&mut self, cancelar: &AtomicBool) -> Result<ResumoDoOcr, Falha> {
        if !self.ocr.disponivel() {
            return Ok(ResumoDoOcr {
                indisponivel: true,
                ..ResumoDoOcr::default()
            });
        }
        let inv = sbf_worker::banco::abrir(&self.dados.banco).map_err(|_| Falha::Banco)?;
        let r = ler_imagens_novas(
            &inv,
            &self.dados.cofre,
            &self.ocr,
            LOTE_PADRAO,
            &mut self.falharam,
            cancelar,
        )
        .map_err(|_| Falha::Banco)?;
        Ok(ResumoDoOcr {
            lidas: r.lidas,
            sem_texto: r.sem_texto,
            falhas: r.falhas,
            pendentes: r.pendentes > 0,
            indisponivel: r.indisponivel,
        })
    }
}
