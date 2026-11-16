//! A regra única de "movimento entre contas" (spec 06, "Áreas e vistas").
//!
//! Um registro de `transferencia_interna`, `investimento`, `pagamento_fatura` ou
//! `fatura_cartao` só muda o dinheiro de lugar: **não conta como gasto**. No Datasette
//! essa lista estava repetida em várias consultas; aqui ela existe uma vez só, como
//! predicado Rust ([`e_movimento_entre_contas`]) e como fragmento SQL
//! ([`sql_e_movimento`], [`sql_e_gasto`]…). Toda vista, relatório e total usa estas
//! funções.
//!
//! Dono: agente de leitura.

/// Os tipos de registro que são movimento entre contas.
pub const TIPOS_DE_MOVIMENTO: [&str; 4] = [
    "transferencia_interna",
    "investimento",
    "pagamento_fatura",
    "fatura_cartao",
];

/// A lista dos tipos como literal SQL: `('transferencia_interna', …)`.
pub fn sql_lista_de_tipos() -> String {
    let itens: Vec<String> = TIPOS_DE_MOVIMENTO
        .iter()
        .map(|t| format!("'{t}'"))
        .collect();
    format!("({})", itens.join(","))
}

/// O predicado Rust: este `tipo` de registro é movimento entre contas?
pub fn e_movimento_entre_contas(tipo: &str) -> bool {
    TIPOS_DE_MOVIMENTO.contains(&tipo)
}

/// `tipo IN (…)`. `prefixo` é o alias da tabela com ponto (`"r."`) ou vazio.
pub fn sql_e_movimento(prefixo: &str) -> String {
    format!("{prefixo}tipo IN {}", sql_lista_de_tipos())
}

/// `tipo NOT IN (…)`.
pub fn sql_nao_e_movimento(prefixo: &str) -> String {
    format!("{prefixo}tipo NOT IN {}", sql_lista_de_tipos())
}

/// Registro que conta como gasto: saiu dinheiro e não foi entre contas.
pub fn sql_e_gasto(prefixo: &str) -> String {
    format!(
        "({prefixo}valor_centavos < 0 AND {})",
        sql_nao_e_movimento(prefixo)
    )
}

/// Registro que conta como entrada: entrou dinheiro e não foi entre contas.
pub fn sql_e_entrada(prefixo: &str) -> String {
    format!(
        "({prefixo}valor_centavos > 0 AND {})",
        sql_nao_e_movimento(prefixo)
    )
}

/// Registro que saiu de uma conta para outra (a parte "movido entre contas" do ciclo).
pub fn sql_e_saida_de_movimento(prefixo: &str) -> String {
    format!(
        "({prefixo}valor_centavos < 0 AND {})",
        sql_e_movimento(prefixo)
    )
}

/// O mesmo predicado de gasto em Rust, para quem já tem o registro na mão.
pub fn e_gasto(tipo: &str, valor_centavos: i64) -> bool {
    valor_centavos < 0 && !e_movimento_entre_contas(tipo)
}

#[cfg(test)]
mod testes {
    use super::*;
    use rusqlite::Connection;

    #[test]
    fn os_quatro_tipos_sao_movimento() {
        for t in [
            "transferencia_interna",
            "investimento",
            "pagamento_fatura",
            "fatura_cartao",
        ] {
            assert!(e_movimento_entre_contas(t), "{t}");
        }
        for t in [
            "compra",
            "pagamento",
            "salario",
            "tarifa",
            "estorno",
            "debito",
        ] {
            assert!(!e_movimento_entre_contas(t), "{t}");
        }
    }

    #[test]
    fn predicado_rust_e_fragmento_sql_concordam() {
        let conn = Connection::open_in_memory().expect("memória");
        let tipos = [
            "transferencia_interna",
            "investimento",
            "pagamento_fatura",
            "fatura_cartao",
            "compra",
            "pagamento",
            "salario",
            "credito",
        ];
        for tipo in tipos {
            for valor in [-5000_i64, 0, 5000] {
                let sql = format!(
                    "SELECT {mov}, {gasto}, {entrada} FROM (SELECT ?1 AS tipo, ?2 AS valor_centavos)",
                    mov = sql_e_movimento(""),
                    gasto = sql_e_gasto(""),
                    entrada = sql_e_entrada(""),
                );
                let (mov, gasto, entrada): (bool, bool, bool) = conn
                    .query_row(&sql, (tipo, valor), |l| {
                        Ok((l.get(0)?, l.get(1)?, l.get(2)?))
                    })
                    .expect("consulta");
                assert_eq!(mov, e_movimento_entre_contas(tipo), "{tipo}");
                assert_eq!(gasto, e_gasto(tipo, valor), "{tipo} {valor}");
                assert_eq!(
                    entrada,
                    valor > 0 && !e_movimento_entre_contas(tipo),
                    "{tipo} {valor}"
                );
            }
        }
    }

    #[test]
    fn movimento_entre_contas_nunca_e_gasto() {
        assert!(!e_gasto("pagamento_fatura", -10_000));
        assert!(!e_gasto("investimento", -10_000));
        assert!(e_gasto("compra", -10_000));
        assert!(!e_gasto("compra", 10_000));
    }
}
