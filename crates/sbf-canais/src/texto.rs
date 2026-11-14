//! O texto dos avisos, no nível de confiança do destinatário (ADR-015). Mascarado: nome do
//! fornecedor e valor sim; nenhum número longo (conta, cartão, CPF, linha digitável).

use sbf_livro::{Centavos, Data};

use crate::alertas::{Alerta, Dados, TipoDeAlerta};
use crate::confianca::{Nivel, proteger};

/// O limite de uma mensagem do Telegram é 4096 caracteres; sobra margem.
pub const LIMITE_DA_MENSAGEM: usize = 3800;

/// `R$ 1.234,56`.
pub fn reais(valor: Centavos) -> String {
    let a = valor.0.unsigned_abs();
    let inteiro = (a / 100).to_string();
    let mut milhar = String::new();
    for (i, c) in inteiro.chars().enumerate() {
        if i > 0 && (inteiro.len() - i).is_multiple_of(3) {
            milhar.push('.');
        }
        milhar.push(c);
    }
    let sinal = if valor.0 < 0 { "-" } else { "" };
    format!("{sinal}R$ {milhar},{:02}", a % 100)
}

/// `14/11`.
pub fn dia_e_mes(d: Data) -> String {
    format!("{:02}/{:02}", d.dia, d.mes)
}

fn quando(dias: i64) -> String {
    match dias {
        0 => "vence hoje".to_owned(),
        1 => "vence amanhã".to_owned(),
        n if n > 1 => format!("vence em {n} dias"),
        -1 => "venceu ontem".to_owned(),
        n => format!("venceu há {} dias", -n),
    }
}

/// A linha de um alerta.
pub fn linha(alerta: &Alerta, nivel: Nivel) -> String {
    match &alerta.dados {
        Dados::Conta {
            descricao,
            conta,
            valor,
            estimado,
            vence_em,
            dias,
        } => {
            let aprox = if *estimado { " (valor estimado)" } else { "" };
            let origem = match (nivel, conta) {
                (Nivel::Mascarado, _) | (_, None) => String::new(),
                (_, Some(c)) => format!(" · {}", proteger(c, nivel)),
            };
            format!(
                "• {}: {}{aprox}, {} ({}){origem}",
                proteger(descricao, nivel),
                reais(*valor),
                quando(*dias),
                dia_e_mes(*vence_em),
            )
        }
        Dados::Duplicada {
            fornecedor,
            valor,
            quantidade,
        } => format!(
            "• {}: {} cobrado {quantidade} vezes no mesmo ciclo",
            proteger(fornecedor, nivel),
            reais(*valor)
        ),
        Dados::ForaDoPadrao {
            categoria,
            gasto,
            media,
        } => format!(
            "• {}: {} neste ciclo, contra média de {} nos 3 anteriores",
            proteger(categoria, nivel),
            reais(*gasto),
            reais(*media)
        ),
        Dados::Triagem { quantidade } => {
            format!("• {quantidade} sugestões da triagem esperam você no desktop")
        }
    }
}

fn titulo(tipo: TipoDeAlerta) -> &'static str {
    match tipo {
        TipoDeAlerta::ContaAVencer => "Contas a vencer",
        TipoDeAlerta::ContaVencida => "Contas vencidas",
        TipoDeAlerta::CobrancaDuplicada => "Possível cobrança duplicada",
        TipoDeAlerta::GastoForaDoPadrao => "Gasto fora do padrão",
        TipoDeAlerta::SugestoesDaTriagem => "Triagem",
    }
}

/// A mensagem de um tipo, cortada no limite (com "e mais N" no fim).
pub fn mensagem_do_tipo(tipo: TipoDeAlerta, alertas: &[&Alerta], nivel: Nivel) -> String {
    let mut texto = titulo(tipo).to_owned();
    for (i, a) in alertas.iter().enumerate() {
        let l = linha(a, nivel);
        if texto.chars().count() + l.chars().count() + 40 > LIMITE_DA_MENSAGEM {
            texto.push_str(&format!("\n… e mais {}", alertas.len() - i));
            break;
        }
        texto.push('\n');
        texto.push_str(&l);
    }
    texto
}

/// O resumo diário: contagens e totais, sem nome de ninguém.
pub fn resumo_do_dia(hoje: Data, alertas: &[Alerta]) -> String {
    let conta = |t: TipoDeAlerta| alertas.iter().filter(|a| a.tipo == t).count();
    let total = |t: TipoDeAlerta| -> Centavos {
        alertas
            .iter()
            .filter(|a| a.tipo == t)
            .filter_map(|a| match &a.dados {
                Dados::Conta { valor, .. } => Some(*valor),
                _ => None,
            })
            .sum()
    };
    let mut texto = format!("Resumo de {}", dia_e_mes(hoje));
    if alertas.is_empty() {
        texto.push_str("\nNada pede atenção hoje.");
        return texto;
    }
    for (t, rotulo) in [
        (TipoDeAlerta::ContaVencida, "vencidas"),
        (TipoDeAlerta::ContaAVencer, "a vencer"),
    ] {
        let n = conta(t);
        if n > 0 {
            texto.push_str(&format!("\n• {n} contas {rotulo}: {}", reais(total(t))));
        }
    }
    for (t, rotulo) in [
        (
            TipoDeAlerta::CobrancaDuplicada,
            "possíveis cobranças duplicadas",
        ),
        (TipoDeAlerta::GastoForaDoPadrao, "categorias fora do padrão"),
    ] {
        let n = conta(t);
        if n > 0 {
            texto.push_str(&format!("\n• {n} {rotulo}"));
        }
    }
    texto
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn dinheiro_em_reais() {
        assert_eq!(reais(Centavos(123_456)), "R$ 1.234,56");
        assert_eq!(reais(Centavos(5)), "R$ 0,05");
        assert_eq!(reais(Centavos(100_000_000)), "R$ 1.000.000,00");
        assert_eq!(reais(Centavos(-250)), "-R$ 2,50");
    }

    fn conta(descricao: &str, dias: i64) -> Alerta {
        Alerta {
            tipo: TipoDeAlerta::ContaAVencer,
            chave: "k".into(),
            estado: "e".into(),
            dados: Dados::Conta {
                descricao: descricao.into(),
                conta: Some("Conta 123456-7".into()),
                valor: Centavos(14230),
                estimado: false,
                vence_em: Data::nova(2026, 11, 17).expect("data"),
                dias,
            },
        }
    }

    #[test]
    fn mascarado_mostra_fornecedor_e_valor_mas_nao_numeros() {
        let a = conta("Boleto 23790.12345 60000.123456 Cemig", 3);
        let t = linha(&a, Nivel::Mascarado);
        assert_eq!(t, "• Boleto ••• Cemig: R$ 142,30, vence em 3 dias (17/11)");
        assert!(!t.contains("23790"));
        assert!(!t.contains("123456"));
        let c = linha(&a, Nivel::Completo);
        assert!(c.contains("23790.12345") && c.contains("Conta 123456-7"));
    }

    #[test]
    fn mensagem_longa_e_cortada() {
        let alertas: Vec<Alerta> = (0..200)
            .map(|i| {
                conta(
                    &format!("Fornecedor longo número {i} com descrição extensa"),
                    1,
                )
            })
            .collect();
        let refs: Vec<&Alerta> = alertas.iter().collect();
        let t = mensagem_do_tipo(TipoDeAlerta::ContaAVencer, &refs, Nivel::Mascarado);
        assert!(t.chars().count() < 4096);
        assert!(t.contains("… e mais"));
    }

    #[test]
    fn resumo_nao_tem_nomes() {
        let r = resumo_do_dia(
            Data::nova(2026, 11, 14).expect("data"),
            &[conta("Cemig", 3), conta("Água", 2)],
        );
        assert!(r.contains("2 contas a vencer: R$ 284,60"));
        assert!(!r.contains("Cemig"));
    }
}
