//! O número de telefone do login, no formato internacional que o Telegram exige (`+55...`).
//!
//! Quem digita um número brasileiro quase nunca digita o código do país: `61 9xxxx-xxxx`,
//! `0619xxxxxxxx`, `(61) 9xxxx-xxxx`. Sem o `55`, o Telegram recusa com PHONE_NUMBER_INVALID.

/// Normaliza a entrada do usuário para `+<dígitos>`.
///
/// - Só os dígitos são mantidos (espaços, parênteses, traços e pontos somem).
/// - Se a entrada começa com `+`, o código do país é confiado como veio.
/// - Senão, um prefixo internacional `00` é removido e o número também é tratado como
///   internacional (o código do país já está lá).
/// - Senão, é nacional: um tronco `0` inicial é removido quando sobram 10 ou 11 dígitos
///   (`0` + DDD + número); e se o resultado tem 10 ou 11 dígitos, prefixa `55`.
#[must_use]
pub fn normalizar_numero(entrada: &str) -> String {
    let apenas_digitos = |s: &str| -> String { s.chars().filter(char::is_ascii_digit).collect() };
    let limpo = entrada.trim_start();

    let digitos = if limpo.starts_with('+') {
        apenas_digitos(limpo)
    } else {
        let d = apenas_digitos(limpo);
        if let Some(internacional) = d.strip_prefix("00") {
            internacional.to_owned()
        } else {
            let sem_tronco = match d.strip_prefix('0') {
                Some(resto) if matches!(resto.len(), 10 | 11) => resto.to_owned(),
                _ => d.clone(),
            };
            if matches!(sem_tronco.len(), 10 | 11) {
                format!("55{sem_tronco}")
            } else {
                sem_tronco
            }
        }
    };
    format!("+{digitos}")
}

/// O número para mostrar ao usuário: tudo mascarado com `•`, menos os 4 últimos caracteres.
#[must_use]
pub fn mascarar_numero(numero: &str) -> String {
    let total = numero.chars().count();
    let visiveis = 4.min(total);
    let mut saida = "•".repeat(total - visiveis);
    saida.extend(numero.chars().skip(total - visiveis));
    saida
}

#[cfg(test)]
mod testes {
    use super::{mascarar_numero, normalizar_numero};

    #[test]
    fn celular_brasileiro_com_ddd_ganha_o_55() {
        assert_eq!(normalizar_numero("11 91234-5678"), "+5511912345678");
    }

    #[test]
    fn tronco_zero_e_ddd_entre_parenteses() {
        assert_eq!(normalizar_numero("(11) 91234-5678"), "+5511912345678");
        assert_eq!(normalizar_numero("0 11 91234-5678"), "+5511912345678");
        assert_eq!(normalizar_numero("01191234-5678"), "+5511912345678");
    }

    #[test]
    fn com_mais_confia_no_codigo_do_pais() {
        assert_eq!(normalizar_numero("+1 415 555 0100"), "+14155550100");
        assert_eq!(normalizar_numero("+55 11 91234 5678"), "+5511912345678");
    }

    #[test]
    fn prefixo_internacional_00_nao_ganha_55() {
        assert_eq!(normalizar_numero("0055 11 91234-5678"), "+5511912345678");
        assert_eq!(normalizar_numero("00 55 11 91234 5678"), "+5511912345678");
        assert_eq!(normalizar_numero("001 415 555 0100"), "+14155550100");
    }

    #[test]
    fn ddi_sem_mais_e_sem_zeros_fica_como_esta() {
        assert_eq!(normalizar_numero("55 11 91234 5678"), "+5511912345678");
    }

    #[test]
    fn fixo_brasileiro_com_tronco_ganha_o_55() {
        assert_eq!(normalizar_numero("0 11 3456-7890"), "+551134567890");
    }

    #[test]
    fn entrada_vazia_vira_so_o_mais() {
        assert_eq!(normalizar_numero(""), "+");
    }

    #[test]
    fn mascara_tudo_menos_os_quatro_ultimos() {
        assert_eq!(mascarar_numero("+5511912345678"), "••••••••••5678");
        assert_eq!(mascarar_numero("12"), "12");
    }
}
