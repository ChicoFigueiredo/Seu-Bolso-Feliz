//! As estratégias de senha (Apêndice V6): como montar a candidata a partir do chaveiro. O
//! banco guarda só o **nome** da estratégia (`regra_senha.estrategia`), nunca a senha.

use crate::fonte::{ErroDeFonte, FonteDeSegredos};
use std::fmt;
use std::str::FromStr;
use zeroize::Zeroizing;

/// Uma estratégia de senha.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Estrategia {
    /// Os 11 dígitos do CPF.
    CpfCompleto,
    /// O CPF com pontos e traço (`000.000.000-00`).
    CpfPontuado,
    /// Os N primeiros dígitos do CPF (1 a 11).
    CpfPrimeiros(u8),
    /// Os N últimos dígitos do CPF (1 a 11).
    CpfUltimos(u8),
    /// A senha própria guardada no chaveiro com este rótulo.
    SenhaPropria(String),
}

/// O texto não é uma estratégia conhecida.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("estratégia de senha desconhecida: {0:?}")]
pub struct EstrategiaDesconhecida(pub String);

fn digitos(texto: &str) -> Option<u8> {
    let n: u8 = texto.parse().ok()?;
    (1..=11).contains(&n).then_some(n)
}

impl FromStr for Estrategia {
    type Err = EstrategiaDesconhecida;

    /// Aceita os nomes da spec (`cpf_primeiros:5`) e os do comentário do esquema do Bun
    /// (`cpf_5_primeiros`), que nenhum script grava hoje.
    fn from_str(texto: &str) -> Result<Self, Self::Err> {
        let desconhecida = || EstrategiaDesconhecida(texto.to_owned());
        match texto {
            "cpf_completo" => return Ok(Self::CpfCompleto),
            "cpf_pontuado" => return Ok(Self::CpfPontuado),
            _ => {}
        }
        if let Some(rotulo) = texto.strip_prefix("senha_propria:") {
            return if rotulo.is_empty() || rotulo.chars().any(char::is_control) {
                Err(desconhecida())
            } else {
                Ok(Self::SenhaPropria(rotulo.to_owned()))
            };
        }
        if let Some(n) = texto.strip_prefix("cpf_primeiros:") {
            return digitos(n).map(Self::CpfPrimeiros).ok_or_else(desconhecida);
        }
        if let Some(n) = texto.strip_prefix("cpf_ultimos:") {
            return digitos(n).map(Self::CpfUltimos).ok_or_else(desconhecida);
        }
        if let Some(meio) = texto.strip_prefix("cpf_") {
            if let Some(n) = meio.strip_suffix("_primeiros") {
                return digitos(n).map(Self::CpfPrimeiros).ok_or_else(desconhecida);
            }
            if let Some(n) = meio.strip_suffix("_ultimos") {
                return digitos(n).map(Self::CpfUltimos).ok_or_else(desconhecida);
            }
        }
        Err(desconhecida())
    }
}

impl fmt::Display for Estrategia {
    /// O nome canônico, o que vai para `regra_senha.estrategia` e `tentativa_senha.estrategia`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CpfCompleto => f.write_str("cpf_completo"),
            Self::CpfPontuado => f.write_str("cpf_pontuado"),
            Self::CpfPrimeiros(n) => write!(f, "cpf_primeiros:{n}"),
            Self::CpfUltimos(n) => write!(f, "cpf_ultimos:{n}"),
            Self::SenhaPropria(rotulo) => write!(f, "senha_propria:{rotulo}"),
        }
    }
}

/// Uma senha candidata, só em memória, zerada ao descartar. Não tem `Display`; o `Debug` é
/// `Candidata(***)`.
pub struct Candidata(Zeroizing<String>);

impl Candidata {
    /// A candidata em claro, só para quem tenta abrir o PDF.
    pub fn expor(&self) -> &str {
        &self.0
    }

    /// Candidata montada fora das estratégias (testes e diagnóstico).
    pub fn nova(valor: Zeroizing<String>) -> Self {
        Self(valor)
    }
}

impl fmt::Debug for Candidata {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Candidata(***)")
    }
}

/// Os 11 dígitos do CPF, sem realocar (nenhuma cópia parcial fica para trás).
fn digitos_do_cpf(fonte: &dyn FonteDeSegredos) -> Result<Zeroizing<Vec<u8>>, ErroDeFonte> {
    let cpf = fonte.cpf()?;
    let mut saida = Zeroizing::new(Vec::with_capacity(cpf.len()));
    saida.extend(cpf.bytes().filter(u8::is_ascii_digit));
    if saida.len() == 11 {
        Ok(saida)
    } else {
        Err(ErroDeFonte::CpfInvalido)
    }
}

/// Texto zerável a partir de dígitos ASCII, com a capacidade exata.
fn texto_de(bytes: &[u8]) -> Zeroizing<String> {
    let mut saida = Zeroizing::new(String::with_capacity(bytes.len()));
    for &b in bytes {
        saida.push(char::from(b));
    }
    saida
}

impl Estrategia {
    /// A lista padrão, que só entra com a opção "tentar estratégias padrão" (desligada por
    /// padrão). Em ordem do mais comum nas faturas e extratos brasileiros.
    pub fn padrao() -> Vec<Estrategia> {
        vec![
            Self::CpfPrimeiros(5),
            Self::CpfPrimeiros(4),
            Self::CpfPrimeiros(6),
            Self::CpfCompleto,
            Self::CpfPrimeiros(3),
            Self::CpfUltimos(5),
            Self::CpfUltimos(4),
            Self::CpfUltimos(6),
            Self::CpfPontuado,
        ]
    }

    /// Como a estratégia monta a senha, em português, para a frase da proposta ("…usam os 5
    /// últimos dígitos do CPF").
    pub fn descricao(&self) -> String {
        match self {
            Self::CpfCompleto => "o CPF completo".to_owned(),
            Self::CpfPontuado => "o CPF com pontos e traço".to_owned(),
            Self::CpfPrimeiros(1) => "o primeiro dígito do CPF".to_owned(),
            Self::CpfUltimos(1) => "o último dígito do CPF".to_owned(),
            Self::CpfPrimeiros(n) => format!("os {n} primeiros dígitos do CPF"),
            Self::CpfUltimos(n) => format!("os {n} últimos dígitos do CPF"),
            Self::SenhaPropria(rotulo) => format!("a senha própria \"{rotulo}\" do chaveiro"),
        }
    }

    /// Monta a candidata em memória.
    pub fn candidata(&self, fonte: &dyn FonteDeSegredos) -> Result<Candidata, ErroDeFonte> {
        let valor = match self {
            Self::SenhaPropria(rotulo) => fonte.senha_propria(rotulo)?,
            Self::CpfCompleto => texto_de(&digitos_do_cpf(fonte)?),
            Self::CpfPrimeiros(n) => texto_de(&digitos_do_cpf(fonte)?[..usize::from(*n)]),
            Self::CpfUltimos(n) => texto_de(&digitos_do_cpf(fonte)?[11 - usize::from(*n)..]),
            Self::CpfPontuado => {
                let d = digitos_do_cpf(fonte)?;
                let mut saida = Zeroizing::new(String::with_capacity(14));
                for (i, &b) in d.iter().enumerate() {
                    match i {
                        3 | 6 => saida.push('.'),
                        9 => saida.push('-'),
                        _ => {}
                    }
                    saida.push(char::from(b));
                }
                saida
            }
        };
        Ok(Candidata(valor))
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    struct Ficticia;
    impl FonteDeSegredos for Ficticia {
        fn cpf(&self) -> Result<Zeroizing<String>, ErroDeFonte> {
            Ok(Zeroizing::new("529.982.247-25".to_owned()))
        }
        fn senha_propria(&self, rotulo: &str) -> Result<Zeroizing<String>, ErroDeFonte> {
            match rotulo {
                "condominio" => Ok(Zeroizing::new("senha-ficticia".to_owned())),
                _ => Err(ErroDeFonte::SemSenhaPropria),
            }
        }
    }

    #[test]
    fn le_os_nomes_da_spec_e_os_do_esquema_do_bun() {
        for (texto, esperada) in [
            ("cpf_completo", Estrategia::CpfCompleto),
            ("cpf_pontuado", Estrategia::CpfPontuado),
            ("cpf_primeiros:5", Estrategia::CpfPrimeiros(5)),
            ("cpf_5_primeiros", Estrategia::CpfPrimeiros(5)),
            ("cpf_ultimos:4", Estrategia::CpfUltimos(4)),
            ("cpf_4_ultimos", Estrategia::CpfUltimos(4)),
            ("cpf_primeiros:11", Estrategia::CpfPrimeiros(11)),
            (
                "senha_propria:fatura-beta",
                Estrategia::SenhaPropria("fatura-beta".into()),
            ),
        ] {
            assert_eq!(texto.parse::<Estrategia>(), Ok(esperada.clone()), "{texto}");
        }
        for ruim in [
            "",
            "cpf",
            "cpf_primeiros:0",
            "cpf_primeiros:12",
            "cpf_primeiros:x",
            "cpf_12_ultimos",
            "senha_propria:",
            "senha_propria:a\nb",
            "CPF_COMPLETO",
        ] {
            assert!(ruim.parse::<Estrategia>().is_err(), "{ruim:?}");
        }
    }

    #[test]
    fn o_nome_canonico_volta_igual() {
        for e in Estrategia::padrao()
            .into_iter()
            .chain([Estrategia::SenhaPropria("x".into())])
        {
            assert_eq!(e.to_string().parse::<Estrategia>(), Ok(e));
        }
    }

    #[test]
    fn monta_as_candidatas() {
        let c = |e: Estrategia| {
            e.candidata(&Ficticia)
                .expect("candidata")
                .expor()
                .to_owned()
        };
        assert_eq!(c(Estrategia::CpfCompleto), "52998224725");
        assert_eq!(c(Estrategia::CpfPontuado), "529.982.247-25");
        assert_eq!(c(Estrategia::CpfPrimeiros(5)), "52998");
        assert_eq!(c(Estrategia::CpfUltimos(5)), "24725");
        assert_eq!(c(Estrategia::CpfUltimos(11)), "52998224725");
        assert_eq!(
            c(Estrategia::SenhaPropria("condominio".into())),
            "senha-ficticia"
        );
        assert_eq!(
            Estrategia::SenhaPropria("outra".into())
                .candidata(&Ficticia)
                .expect_err("sem senha"),
            ErroDeFonte::SemSenhaPropria
        );
        let candidata = Estrategia::CpfCompleto
            .candidata(&Ficticia)
            .expect("candidata");
        assert_eq!(format!("{candidata:?}"), "Candidata(***)");
    }

    #[test]
    fn descricao_para_a_frase() {
        assert_eq!(
            Estrategia::CpfUltimos(5).descricao(),
            "os 5 últimos dígitos do CPF"
        );
        assert_eq!(
            Estrategia::CpfPrimeiros(1).descricao(),
            "o primeiro dígito do CPF"
        );
    }
}
