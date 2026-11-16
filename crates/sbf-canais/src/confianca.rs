//! O nível de confiança do destinatário (ADR-015) e a máscara de dados identificáveis.
//!
//! - **mascarado** (o padrão de todo destinatário novo): nome do fornecedor e valor sim;
//!   nenhum número de conta, cartão, CPF ou linha digitável;
//! - **parcial**: os mesmos números, mas só com os 4 últimos dígitos;
//! - **completo**: o texto como está.

use crate::erro::ErroDeCanal;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Nivel {
    #[default]
    Mascarado,
    Parcial,
    Completo,
}

impl Nivel {
    pub fn codigo(self) -> &'static str {
        match self {
            Nivel::Mascarado => "mascarado",
            Nivel::Parcial => "parcial",
            Nivel::Completo => "completo",
        }
    }

    pub fn de_codigo(texto: &str) -> Result<Nivel, ErroDeCanal> {
        match texto.trim() {
            "mascarado" => Ok(Nivel::Mascarado),
            "parcial" => Ok(Nivel::Parcial),
            "completo" => Ok(Nivel::Completo),
            _ => Err(ErroDeCanal::Entrada(
                "o nível é mascarado, parcial ou completo".into(),
            )),
        }
    }
}

/// Menos que isto de dígitos seguidos não é número de conta, cartão ou código.
const DIGITOS_MINIMOS: usize = 6;

/// Troca por `•••` cada sequência de seis ou mais dígitos (com espaço, ponto ou hífen
/// entre eles: linha digitável, cartão, CPF). No nível parcial, os 4 últimos dígitos ficam.
pub fn proteger(texto: &str, nivel: Nivel) -> String {
    if nivel == Nivel::Completo {
        return texto.to_owned();
    }
    let chars: Vec<char> = texto.chars().collect();
    let mut saida = String::with_capacity(texto.len());
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            saida.push(chars[i]);
            i += 1;
            continue;
        }
        // Estende a sequência: dígitos, e separador só se um dígito vier logo depois.
        let inicio = i;
        let mut fim = i;
        let mut j = i;
        while j < chars.len() {
            if chars[j].is_ascii_digit() {
                fim = j + 1;
                j += 1;
            } else if matches!(chars[j], ' ' | '.' | '-')
                && chars.get(j + 1).is_some_and(char::is_ascii_digit)
            {
                j += 1;
            } else {
                break;
            }
        }
        let trecho = &chars[inicio..fim];
        let digitos: Vec<char> = trecho
            .iter()
            .copied()
            .filter(char::is_ascii_digit)
            .collect();
        if digitos.len() >= DIGITOS_MINIMOS {
            saida.push_str("•••");
            if nivel == Nivel::Parcial {
                saida.extend(&digitos[digitos.len() - 4..]);
            }
        } else {
            saida.extend(trecho);
        }
        i = fim;
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn mascarado_esconde_linha_digitavel_cartao_e_cpf() {
        let linha = "Boleto 23790.12345 60000.123456 78901.234567 8 12340000012345";
        assert_eq!(proteger(linha, Nivel::Mascarado), "Boleto •••");
        assert_eq!(
            proteger("Cartão 4111 1111 1111 1234 fim", Nivel::Mascarado),
            "Cartão ••• fim"
        );
        assert_eq!(proteger("CPF 123.456.789-09", Nivel::Mascarado), "CPF •••");
    }

    #[test]
    fn numeros_curtos_ficam() {
        assert_eq!(
            proteger("Parcela 3/12 Posto 24h Ag 1234", Nivel::Mascarado),
            "Parcela 3/12 Posto 24h Ag 1234"
        );
        assert_eq!(
            proteger("Mercado Exemplo", Nivel::Mascarado),
            "Mercado Exemplo"
        );
    }

    #[test]
    fn parcial_guarda_os_quatro_ultimos() {
        assert_eq!(
            proteger("Cartão 4111 1111 1111 1234", Nivel::Parcial),
            "Cartão •••1234"
        );
    }

    #[test]
    fn completo_nao_mexe() {
        let t = "Conta 123456-7";
        assert_eq!(proteger(t, Nivel::Completo), t);
    }

    #[test]
    fn nivel_ida_e_volta() {
        for n in [Nivel::Mascarado, Nivel::Parcial, Nivel::Completo] {
            assert_eq!(Nivel::de_codigo(n.codigo()).ok(), Some(n));
        }
        assert!(Nivel::de_codigo("tudo").is_err());
        assert_eq!(Nivel::default(), Nivel::Mascarado);
    }
}
