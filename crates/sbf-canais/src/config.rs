//! A configuração por variável de ambiente (nunca argv, que aparece em `ps`), para caber
//! num `EnvironmentFile` do systemd.
//!
//! | variável                       | para quê                                              |
//! | ------------------------------ | ----------------------------------------------------- |
//! | `SBF_DATABASE_URL`             | o Postgres (obrigatória)                              |
//! | `SBF_TELEGRAM_TOKEN`           | o token do BotFather (obrigatória para o bot)         |
//! | `SBF_TELEGRAM_CHATS`           | chat_ids autorizados, separados por vírgula           |
//! | `SBF_TELEGRAM_API`             | só testes: base da API (padrão api.telegram.org)      |
//! | `SBF_ALERTAS_DIAS`             | avisa conta que vence em até N dias (padrão 3)        |
//! | `SBF_ALERTAS_HORAS`            | horários da agenda, `HH:MM,HH:MM` (padrão `08:00`)    |
//! | `SBF_ALERTAS_SILENCIO`         | janela sem aviso, `HH:MM-HH:MM` (padrão `22:00-07:00`)|
//! | `SBF_ALERTAS_RESUMO`           | `1` liga o resumo diário no primeiro horário          |
//! | `SBF_ALERTAS_LIMIAR_PERCENTUAL`| fora do padrão: % acima da média (padrão 50)          |
//! | `SBF_ALERTAS_LIMIAR_MINIMO`    | fora do padrão: centavos acima da média (padrão 10000)|
//! | `BLOB_READ_WRITE_TOKEN`        | o armazenamento dos documentos que chegam pelo bot    |
//! | `LOCAL_USER_ID`                | substituto validado do dono (opcional)                |

use std::collections::BTreeSet;

use sbf_coletor::config::{Segredo, exigir, opcional};
use sbf_livro::Centavos;

use crate::alertas::Limites;
use crate::erro::ErroDeCanal;

pub const API_DO_TELEGRAM: &str = "https://api.telegram.org";

#[derive(Debug, Clone)]
pub struct ConfigTelegram {
    pub token: Segredo,
    pub base: String,
    /// Os chat_ids que podem falar com o bot sem código (a lista configurada).
    pub chats_autorizados: BTreeSet<i64>,
}

impl ConfigTelegram {
    pub fn do_ambiente() -> Result<Self, ErroDeCanal> {
        Ok(Self {
            token: Segredo::novo(exigir("SBF_TELEGRAM_TOKEN")?),
            base: opcional("SBF_TELEGRAM_API").unwrap_or_else(|| API_DO_TELEGRAM.to_owned()),
            chats_autorizados: ler_chats(opcional("SBF_TELEGRAM_CHATS").as_deref())?,
        })
    }
}

pub fn ler_chats(texto: Option<&str>) -> Result<BTreeSet<i64>, ErroDeCanal> {
    let mut chats = BTreeSet::new();
    for parte in texto.unwrap_or("").split(',') {
        let parte = parte.trim();
        if parte.is_empty() {
            continue;
        }
        chats.insert(parte.parse::<i64>().map_err(|_| {
            ErroDeCanal::Configuracao("SBF_TELEGRAM_CHATS tem um chat_id que não é número".into())
        })?);
    }
    Ok(chats)
}

/// Um horário do dia em minutos desde a meia-noite.
pub type Minuto = u32;

pub fn ler_hora(texto: &str) -> Option<Minuto> {
    let (h, m) = texto.trim().split_once(':')?;
    let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
    (h < 24 && m < 60).then_some(h * 60 + m)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigAlertas {
    pub limites: Limites,
    /// Os horários da agenda (minutos desde a meia-noite, de São Paulo), em ordem.
    pub horas: Vec<Minuto>,
    /// A janela sem aviso: do primeiro ao segundo horário, passando da meia-noite se preciso.
    pub silencio: (Minuto, Minuto),
    pub resumo_diario: bool,
}

impl Default for ConfigAlertas {
    fn default() -> Self {
        Self {
            limites: Limites::default(),
            horas: vec![8 * 60],
            silencio: (22 * 60, 7 * 60),
            resumo_diario: false,
        }
    }
}

impl ConfigAlertas {
    pub fn do_ambiente() -> Result<Self, ErroDeCanal> {
        let mut c = Self::default();
        let numero = |nome: &str| -> Result<Option<i64>, ErroDeCanal> {
            opcional(nome)
                .map(|v| {
                    v.trim().parse::<i64>().map_err(|_| {
                        ErroDeCanal::Configuracao(format!("{nome} precisa ser um número inteiro"))
                    })
                })
                .transpose()
        };
        if let Some(n) = numero("SBF_ALERTAS_DIAS")? {
            c.limites.dias_a_vencer = n.clamp(0, 30);
        }
        if let Some(n) = numero("SBF_ALERTAS_LIMIAR_PERCENTUAL")? {
            c.limites.limiar_percentual = n.clamp(0, 1000);
        }
        if let Some(n) = numero("SBF_ALERTAS_LIMIAR_MINIMO")? {
            c.limites.limiar_minimo = Centavos(n.max(0));
        }
        if let Some(texto) = opcional("SBF_ALERTAS_HORAS") {
            c.horas = ler_horas(&texto)?;
        }
        if let Some(texto) = opcional("SBF_ALERTAS_SILENCIO") {
            c.silencio = ler_silencio(&texto)?;
        }
        c.resumo_diario = opcional("SBF_ALERTAS_RESUMO").as_deref() == Some("1");
        Ok(c)
    }
}

pub fn ler_horas(texto: &str) -> Result<Vec<Minuto>, ErroDeCanal> {
    let mut horas = Vec::new();
    for parte in texto.split(',').filter(|p| !p.trim().is_empty()) {
        horas.push(ler_hora(parte).ok_or_else(|| {
            ErroDeCanal::Configuracao("SBF_ALERTAS_HORAS usa HH:MM separados por vírgula".into())
        })?);
    }
    if horas.is_empty() {
        return Err(ErroDeCanal::Configuracao(
            "SBF_ALERTAS_HORAS está vazia".into(),
        ));
    }
    horas.sort_unstable();
    horas.dedup();
    Ok(horas)
}

pub fn ler_silencio(texto: &str) -> Result<(Minuto, Minuto), ErroDeCanal> {
    texto
        .split_once('-')
        .and_then(|(a, b)| Some((ler_hora(a)?, ler_hora(b)?)))
        .ok_or_else(|| ErroDeCanal::Configuracao("SBF_ALERTAS_SILENCIO usa HH:MM-HH:MM".into()))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn chats_e_horas() {
        assert_eq!(
            ler_chats(Some("12, -100123 ,")).ok(),
            Some(BTreeSet::from([12, -100_123]))
        );
        assert!(ler_chats(Some("abc")).is_err());
        assert_eq!(ler_horas("18:30, 08:00").ok(), Some(vec![480, 1110]));
        assert!(ler_horas("25:00").is_err());
        assert_eq!(ler_silencio("22:00-07:30").ok(), Some((1320, 450)));
        assert!(ler_silencio("noite").is_err());
    }

    #[test]
    fn debug_do_telegram_nao_mostra_o_token() {
        let c = ConfigTelegram {
            token: Segredo::novo("123:token-ficticio"),
            base: "x".into(),
            chats_autorizados: BTreeSet::new(),
        };
        assert!(!format!("{c:?}").contains("token-ficticio"));
    }
}
