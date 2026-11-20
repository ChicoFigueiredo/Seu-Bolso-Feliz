//! A cola do OCR das imagens novas: monta, sobre o `sbf-ocr`, o que o `sbf worker` usa.
//! Só lê e grava texto no inventário; nada sai da máquina.

use std::sync::atomic::AtomicBool;
use std::time::{SystemTime, UNIX_EPOCH};

use sbf_ocr::{LOTE_PADRAO, Ocr, ler_imagens_novas};
use sbf_worker::laco::{Dados, LeitorDeImagens, ResumoDoOcr};
use sbf_worker::registro::Falha;

/// O leitor de imagens do worker: o tesseract do sistema, um lote por volta.
#[derive(Debug)]
pub struct LeitorReal {
    dados: Dados,
    ocr: Ocr,
}

impl LeitorReal {
    #[must_use]
    pub fn novo(dados: Dados) -> Self {
        Self {
            dados,
            ocr: Ocr::do_sistema(),
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
            agora(),
            cancelar,
        )
        .map_err(|_| Falha::Banco)?;
        Ok(ResumoDoOcr {
            lidas: r.lidas,
            sem_texto: r.sem_texto,
            falhas: r.falhas,
            pendentes: r.pendentes > 0,
            indisponivel: r.indisponivel,
            causa: r.causa,
        })
    }
}

/// Segundos desde 1970 (o recuo das imagens que falham é contado nesse relógio).
fn agora() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
