//! O que o reconhecimento registra na Caixa de entrada (`sbf_inventario::caixa_de_entrada`):
//! o tipo, uma linha de resumo e o estado do documento no vocabulário do glossário. Só
//! categorias e contagens: **nunca** conteúdo do documento.

use sbf_inventario::caixa_de_entrada::{Canal, Estado, Leitura};

use crate::cofre::rotulo_do_tipo;
use crate::{EstadoDoTexto, Reconhecido, Resultado};

/// O resumo da linha quando a leitura falhou (o usuário escolhe Reconhecer de novo).
pub const RESUMO_LEITURA_FALHOU: &str = "a leitura falhou — Reconhecer de novo";

/// O canal da Caixa de entrada que corresponde ao canal das regras; `None` para os canais
/// que não entram nela (cada um listado, para um canal novo obrigar a decidir aqui).
#[must_use]
pub fn canal_da_caixa(canal: sbf_regras::Canal) -> Option<Canal> {
    use sbf_regras::Canal as Regras;
    match canal {
        Regras::Gmail => Some(Canal::Gmail),
        Regras::Telegram => Some(Canal::Telegram),
        Regras::Pasta => Some(Canal::Pasta),
        Regras::Arrastado => Some(Canal::Arrastado),
        // Já estava no cofre, vem de ferramenta ou foi digitado: não há "chegada".
        Regras::Cofre | Regras::Pluggy | Regras::Mcp | Regras::Manual => None,
    }
}

/// O estado do documento depois do reconhecimento. `repetido`: o mesmo conteúdo já tinha
/// sido achado em outro lugar (só vale para o que foi reconhecido).
#[must_use]
pub fn estado_do_resultado(r: &Resultado, repetido: bool) -> Estado {
    match &r.texto {
        EstadoDoTexto::Protegido(_) => return Estado::ProtegidoPorSenha,
        // A leitura falhou: pede revisão, nunca cai em "sem padrão de reconhecimento".
        EstadoDoTexto::Falhou(_) => return Estado::PrecisaRevisar,
        EstadoDoTexto::Extraido | EstadoDoTexto::JaHavia | EstadoDoTexto::SemTexto(_) => {}
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

/// A leitura completa para a Caixa de entrada.
#[must_use]
pub fn leitura_do_resultado(r: &Resultado, repetido: bool) -> Leitura {
    let (tipo, resumo) = tipo_e_resumo(r);
    let resumo = match &r.texto {
        EstadoDoTexto::Protegido(m) => Some(m.clone()),
        EstadoDoTexto::Falhou(_) => Some(RESUMO_LEITURA_FALHOU.to_owned()),
        EstadoDoTexto::Extraido | EstadoDoTexto::JaHavia | EstadoDoTexto::SemTexto(_) => resumo,
    };
    Leitura {
        content_hash: r.sha256.clone(),
        nome: r.nome.clone(),
        tipo,
        resumo,
        estado: estado_do_resultado(r, repetido),
    }
}
