//! O que o reconhecimento registra na Caixa de entrada (`sbf_inventario::caixa_de_entrada`):
//! o tipo, uma linha de resumo e o estado do documento no vocabulário do glossário. Só
//! categorias e contagens: **nunca** conteúdo do documento.

use sbf_inventario::caixa_de_entrada::{Estado, Origem, Registro};

use crate::cofre::rotulo_do_tipo;
use crate::{EstadoDoTexto, Reconhecido, Resultado};

/// De que canal da coleta vem a origem da fila (os canais que a Caixa de entrada nomeia).
#[must_use]
pub fn origem_do_canal(canal: sbf_regras::Canal) -> Option<Origem> {
    match canal {
        sbf_regras::Canal::Gmail => Some(Origem::Gmail),
        sbf_regras::Canal::Telegram => Some(Origem::Telegram),
        sbf_regras::Canal::Pasta => Some(Origem::Pasta),
        sbf_regras::Canal::Arrastado => Some(Origem::Arrastado),
        _ => None,
    }
}

/// O estado do documento depois do reconhecimento. `repetido`: o mesmo conteúdo já tinha
/// sido achado em outro lugar (só vale para o que foi reconhecido).
#[must_use]
pub fn estado_do_resultado(r: &Resultado, repetido: bool) -> Estado {
    if matches!(r.texto, EstadoDoTexto::Protegido(_)) {
        return Estado::ProtegidoPorSenha;
    }
    let reconhecido = match &r.reconhecido {
        Reconhecido::Nada { .. } => return Estado::NaoReconhecido,
        Reconhecido::Ambiguo { .. } | Reconhecido::PelaIa { .. } => Estado::PrecisaRevisar,
        Reconhecido::PelaRegra { reconhecimento } => {
            if reconhecimento.revisar || !r.avisos.is_empty() {
                Estado::PrecisaRevisar
            } else {
                Estado::Pronto
            }
        }
        Reconhecido::Contracheque { .. } => {
            if r.avisos.is_empty() {
                Estado::Pronto
            } else {
                Estado::PrecisaRevisar
            }
        }
    };
    if repetido {
        Estado::PossivelDuplicidade
    } else {
        reconhecido
    }
}

/// O tipo e o resumo curto do que foi reconhecido.
#[must_use]
pub fn tipo_e_resumo(r: &Resultado) -> (Option<String>, Option<String>) {
    match &r.reconhecido {
        Reconhecido::Contracheque {
            contracheque: c, ..
        } => (
            Some("Contracheque".to_owned()),
            Some(format!(
                "Contracheque · {:02}/{} · {} rubricas",
                c.pagamento.mes,
                c.pagamento.ano,
                c.rubricas.len()
            )),
        ),
        Reconhecido::PelaIa { resumo, .. } => (
            resumo.split(" · ").next().map(str::to_owned),
            Some(resumo.clone()),
        ),
        Reconhecido::PelaRegra { reconhecimento: r } => {
            let tipo = rotulo_do_tipo(&r.tipo_documental);
            (
                Some(tipo.to_owned()),
                Some(format!("{tipo} · regra {} (versão {})", r.regra, r.versao)),
            )
        }
        Reconhecido::Ambiguo { candidatas, .. } => (
            None,
            Some(format!(
                "{} regras empatadas discordam: escolha uma",
                candidatas.len()
            )),
        ),
        Reconhecido::Nada { motivo } => (None, Some(motivo.clone())),
    }
}

/// O registro completo da fila para o resultado.
#[must_use]
pub fn registro_do_resultado(r: &Resultado, origem: Origem, repetido: bool) -> Registro {
    let (tipo, resumo) = tipo_e_resumo(r);
    let resumo = match (&r.texto, resumo) {
        (EstadoDoTexto::Protegido(m), _) => Some(m.clone()),
        (_, resumo) => resumo,
    };
    Registro {
        content_hash: r.sha256.clone(),
        nome: r.nome.clone(),
        origem,
        tipo,
        resumo,
        estado: estado_do_resultado(r, repetido),
    }
}
