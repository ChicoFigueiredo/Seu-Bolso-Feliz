//! O chaveiro como [`FonteDeSegredos`] da abertura de PDF protegido (`sbf-pdf`).

use crate::chaveiro::Chaveiro;
use crate::erro::ErroDoChaveiro;
use bancada_segredos::{ErroDeSegredo, Segredo};
use sbf_pdf::{ErroDeFonte, FonteDeSegredos};
use zeroize::Zeroizing;

fn traduzir(erro: ErroDoChaveiro, ausente: ErroDeFonte) -> ErroDeFonte {
    match erro {
        ErroDoChaveiro::Guarda(ErroDeSegredo::NaoEncontrado) => ausente,
        ErroDoChaveiro::Guarda(ErroDeSegredo::Trancado) => ErroDeFonte::Trancado,
        ErroDoChaveiro::RotuloInvalido => ausente,
        _ => ErroDeFonte::Indisponivel,
    }
}

/// Cópia zerável com a capacidade exata (o `Segredo` original também é zerado ao sair).
fn copiar(segredo: &Segredo) -> Zeroizing<String> {
    let mut saida = Zeroizing::new(String::with_capacity(segredo.expor().len()));
    saida.push_str(segredo.expor());
    saida
}

impl FonteDeSegredos for Chaveiro {
    fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
        let cpf = Chaveiro::cpf(self).map_err(|e| traduzir(e, ErroDeFonte::SemCpf))?;
        Ok(copiar(&cpf))
    }

    fn senha_propria(&self, rotulo: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
        let senha = self
            .senha(rotulo)
            .map_err(|e| traduzir(e, ErroDeFonte::SemSenhaPropria))?;
        Ok(copiar(&senha))
    }
}
