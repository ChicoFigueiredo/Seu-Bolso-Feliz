//! Pedaços de texto para o índice: ~800 caracteres com sobreposição (spec 10).

/// Tamanho alvo de um pedaço, em caracteres.
pub const TAMANHO: usize = 800;
/// Quanto do fim de um pedaço volta no início do seguinte.
pub const SOBREPOSICAO: usize = 120;

/// Corta `texto` em pedaços de até [`TAMANHO`] caracteres. Procura um espaço ou quebra de
/// linha no último quinto da janela para não cortar palavra. Texto vazio (só espaço) dá nada;
/// texto curto dá um pedaço só. Sempre avança, então termina.
#[must_use]
pub fn dividir(texto: &str) -> Vec<String> {
    let chars: Vec<char> = texto.chars().collect();
    let mut saida = Vec::new();
    let mut inicio = 0;
    while inicio < chars.len() {
        let limite = (inicio + TAMANHO).min(chars.len());
        let mut fim = limite;
        if limite < chars.len() {
            let piso = limite - TAMANHO / 5;
            if let Some(p) = (piso..limite).rev().find(|&i| chars[i].is_whitespace()) {
                fim = p;
            }
        }
        let pedaco: String = chars[inicio..fim].iter().collect();
        let pedaco = pedaco.trim();
        if !pedaco.is_empty() {
            saida.push(pedaco.to_owned());
        }
        if fim >= chars.len() {
            break;
        }
        // Avança sempre: a sobreposição nunca faz voltar ao início do pedaço.
        inicio = fim.saturating_sub(SOBREPOSICAO).max(inicio + 1);
    }
    saida
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn curto_da_um_pedaco_e_vazio_da_nenhum() {
        assert_eq!(dividir("fatura de energia"), vec!["fatura de energia"]);
        assert!(dividir("   \n ").is_empty());
    }

    #[test]
    fn longo_e_cortado_com_sobreposicao_e_cobre_tudo() {
        let palavras: Vec<String> = (0..600).map(|i| format!("palavra{i}")).collect();
        let texto = palavras.join(" ");
        let p = dividir(&texto);
        assert!(p.len() > 5);
        assert!(p.iter().all(|x| x.chars().count() <= TAMANHO));
        // Sobreposição: o fim de um pedaço reaparece no começo do seguinte.
        let ultima = p[0].split(' ').next_back().unwrap_or_default().to_owned();
        assert!(p[1].contains(&ultima));
        assert!(p.last().is_some_and(|u| u.ends_with("palavra599")));
        assert!(p[0].starts_with("palavra0 "));
    }

    #[test]
    fn texto_sem_espaco_nao_trava_e_respeita_acentos() {
        let t = "ç".repeat(2000);
        let p = dividir(&t);
        assert!(p.len() >= 3);
        assert!(p.iter().all(|x| x.chars().count() <= TAMANHO));
    }
}
