//! Ícones Material Symbols Rounded pelo nome (o mesmo nome do web e do protótipo).
//!
//! A fonte embutida é um subconjunto (`assets/fontes/icones.codepoints`). Nome fora do
//! subconjunto devolve o glifo de reserva (um círculo vazio), nunca entra em pânico.

use std::collections::HashMap;
use std::sync::OnceLock;

use egui::{Color32, FontId, RichText};

use crate::tema::familia_icones;

const CODEPOINTS: &str = include_str!("../assets/fontes/icones.codepoints");

fn mapa() -> &'static HashMap<&'static str, char> {
    static MAPA: OnceLock<HashMap<&'static str, char>> = OnceLock::new();
    MAPA.get_or_init(|| {
        CODEPOINTS
            .lines()
            .filter_map(|linha| {
                let (nome, hex) = linha.split_once(' ')?;
                let c = char::from_u32(u32::from_str_radix(hex.trim(), 16).ok()?)?;
                Some((nome, c))
            })
            .collect()
    })
}

/// O caractere do ícone, ou `None` se ele não está no subconjunto.
pub fn caractere(nome: &str) -> Option<char> {
    mapa().get(nome).copied()
}

/// O ícone como texto (um caractere). Nome desconhecido vira o círculo vazio.
pub fn texto(nome: &str) -> String {
    caractere(nome)
        .or_else(|| caractere("radio_button_unchecked"))
        .unwrap_or('•')
        .to_string()
}

/// O ícone pronto para `ui.label` ou para um átomo de botão.
pub fn icone(nome: &str, tamanho: f32) -> RichText {
    RichText::new(texto(nome)).font(FontId::new(tamanho, familia_icones()))
}

/// O ícone colorido.
pub fn icone_cor(nome: &str, tamanho: f32, cor: Color32) -> RichText {
    icone(nome, tamanho).color(cor)
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_inventario::Tipo;

    #[test]
    fn todo_tipo_tem_icone_no_subconjunto() {
        for tipo in Tipo::TODOS {
            assert!(caractere(tipo.icone()).is_some(), "falta {}", tipo.icone());
        }
    }

    #[test]
    fn nome_desconhecido_nao_entra_em_panico() {
        assert_eq!(texto("nao_existe").chars().count(), 1);
    }
}
