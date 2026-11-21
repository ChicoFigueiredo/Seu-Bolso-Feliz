//! A impressão do texto (migração 5): o SHA-256 do texto normalizado de um achado, para achar
//! o mesmo documento que chegou com bytes diferentes (a Stripe gera o PDF de novo a cada
//! envio; o mesmo recibo chega por e-mail e pelo Telegram).
//!
//! - **Normalização**: quebras de linha e espaços em sequência viram um espaço e as pontas
//!   são aparadas. Acentos e maiúsculas ficam (dois textos que só diferem nisso não são
//!   tratados como o mesmo documento).
//! - **Mínimo**: texto normalizado com menos de [`MINIMO_DE_CARACTERES`] não recebe impressão,
//!   para foto com OCR vazio (ou quase) não virar "duplicada" de outra.
//! - **Quem calcula**: o Rust. `gravar_texto` calcula na mesma transação que grava o texto;
//!   quem grava `texto` por fora (os scripts Bun) só deixa a linha *pendente*, por gatilho, e
//!   [`completar_pendentes`] calcula depois. Os gatilhos também desfazem a impressão de um
//!   texto que muda ou some.

use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use sha2::{Digest, Sha256};

/// Texto normalizado com menos caracteres que isto não recebe impressão.
pub const MINIMO_DE_CARACTERES: usize = 200;

/// Quantas linhas de `texto` cada passo lê ao preencher ou completar.
const LOTE: usize = 500;

/// O texto como a impressão o vê: sequências de espaço e quebra de linha viram um espaço e
/// as pontas são aparadas.
#[must_use]
pub fn normalizar(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// A impressão do texto, ou `None` se o texto normalizado é curto demais.
#[must_use]
pub fn impressao_de(texto: &str) -> Option<String> {
    let normalizado = normalizar(texto);
    if normalizado.chars().count() < MINIMO_DE_CARACTERES {
        return None;
    }
    let digest = Sha256::digest(normalizado.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest.iter() {
        hex.push_str(&format!("{byte:02x}"));
    }
    Some(hex)
}

/// O banco já passou pela migração 5?
#[must_use]
pub fn existe_a_tabela(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'impressao_do_texto'",
        [],
        |_| Ok(()),
    )
    .optional()
    .ok()
    .flatten()
    .is_some()
}

fn gravar(conn: &Connection, content_hash: &str, impressao: Option<&str>) -> rusqlite::Result<()> {
    conn.execute(
        "INSERT OR REPLACE INTO impressao_do_texto (content_hash, impressao, pendente)
         VALUES (?1, ?2, 0)",
        params![content_hash, impressao],
    )?;
    Ok(())
}

/// Calcula e grava a impressão do texto do achado, agora. Sem linha em `texto`, nada a fazer.
/// Banco que ainda não passou pela migração 5: não faz nada (a migração preenche tudo). Não
/// abre transação.
///
/// # Errors
/// Falha do SQLite.
pub fn calcular(conn: &Connection, content_hash: &str) -> rusqlite::Result<()> {
    if !existe_a_tabela(conn) {
        return Ok(());
    }
    let conteudo: Option<String> = conn
        .query_row(
            "SELECT conteudo FROM texto WHERE content_hash = ?1",
            [content_hash],
            |l| l.get(0),
        )
        .optional()?;
    if let Some(conteudo) = conteudo {
        gravar(conn, content_hash, impressao_de(&conteudo).as_deref())?;
    }
    Ok(())
}

/// Preenche a impressão de **todos** os textos, em lotes por chave (migração 5; também serve
/// para refazer tudo). Idempotente. Devolve quantos textos viu.
///
/// # Errors
/// Falha do SQLite.
pub fn preencher_tudo(conn: &Connection) -> rusqlite::Result<usize> {
    let mut visto = 0;
    let mut depois_de = String::new();
    loop {
        let mut consulta = conn.prepare_cached(
            "SELECT content_hash, conteudo FROM texto
              WHERE content_hash > ?1 ORDER BY content_hash LIMIT ?2",
        )?;
        let lote = consulta
            .query_map(params![depois_de, LOTE as i64], |l| {
                Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        drop(consulta);
        let Some((ultimo, _)) = lote.last() else {
            return Ok(visto);
        };
        depois_de.clone_from(ultimo);
        visto += lote.len();
        for (hash, conteudo) in &lote {
            gravar(conn, hash, impressao_de(conteudo).as_deref())?;
        }
    }
}

/// Calcula a impressão de quem foi gravado em `texto` sem passar pelo Rust (os scripts Bun) e
/// ficou pendente. Cada lote numa transação IMMEDIATE curta. Devolve quantos completou. Banco
/// que não passou pela migração 5: zero.
///
/// # Errors
/// Falha do SQLite.
pub fn completar_pendentes(conn: &Connection) -> rusqlite::Result<usize> {
    if !existe_a_tabela(conn) {
        return Ok(0);
    }
    let mut feitos = 0;
    loop {
        let tx = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
        let pendentes = {
            let mut consulta = tx.prepare_cached(
                "SELECT i.content_hash, t.conteudo
                   FROM impressao_do_texto i JOIN texto t ON t.content_hash = i.content_hash
                  WHERE i.pendente = 1 LIMIT ?1",
            )?;
            consulta
                .query_map([LOTE as i64], |l| {
                    Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?))
                })?
                .collect::<Result<Vec<_>, _>>()?
        };
        if pendentes.is_empty() {
            return Ok(feitos);
        }
        for (hash, conteudo) in &pendentes {
            gravar(&tx, hash, impressao_de(conteudo).as_deref())?;
        }
        feitos += pendentes.len();
        tx.commit()?;
    }
}

/// O achado mais antigo que tem o mesmo texto de outro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Original {
    pub content_hash: String,
    /// Como o usuário o conhece: o nome na Caixa de entrada ou o do primeiro arquivo.
    pub nome: String,
}

/// Se outro achado, **mais antigo**, tem a mesma impressão do texto de `sha256`, devolve o
/// mais antigo deles. Antigo é pela chegada na Caixa de entrada, ou, sem linha nela, pelo
/// `primeiro_visto_em`; empate vai pelo hash, para um par nunca marcar os dois lados.
/// Sem impressão (texto curto, pendente, banco sem a migração 5) devolve `None`.
///
/// # Errors
/// Falha do SQLite.
pub fn original_do_mesmo_texto(
    conn: &Connection,
    sha256: &str,
) -> rusqlite::Result<Option<Original>> {
    if !existe_a_tabela(conn) {
        return Ok(None);
    }
    crate::caixa_de_entrada::garantir_tabela(conn)?;
    conn.query_row(
        "WITH meu AS (
           SELECT i.impressao, COALESCE(c.chegou_em, a.primeiro_visto_em) AS chegada
             FROM impressao_do_texto i
             JOIN achado a ON a.content_hash = i.content_hash
             LEFT JOIN caixa_entrada c ON c.content_hash = i.content_hash
            WHERE i.content_hash = ?1 AND i.impressao IS NOT NULL
         )
         SELECT o.content_hash,
                COALESCE(c.nome,
                         (SELECT og.filename FROM origem og
                           WHERE og.content_hash = o.content_hash AND og.filename <> ''
                           ORDER BY og.id LIMIT 1),
                         substr(o.content_hash, 1, 12))
           FROM meu
           JOIN impressao_do_texto o ON o.impressao = meu.impressao AND o.content_hash <> ?1
           JOIN achado a ON a.content_hash = o.content_hash
           LEFT JOIN caixa_entrada c ON c.content_hash = o.content_hash
          WHERE COALESCE(c.chegou_em, a.primeiro_visto_em) < meu.chegada
             OR (COALESCE(c.chegou_em, a.primeiro_visto_em) = meu.chegada
                 AND o.content_hash < ?1)
          ORDER BY COALESCE(c.chegou_em, a.primeiro_visto_em), o.content_hash
          LIMIT 1",
        [sha256],
        |l| {
            Ok(Original {
                content_hash: l.get(0)?,
                nome: l.get(1)?,
            })
        },
    )
    .optional()
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn normaliza_espacos_e_quebras_sem_tocar_em_acento_nem_caixa() {
        assert_eq!(
            normalizar("  Recibo \r\n\t  de  Pagamento\u{a0}\n"),
            "Recibo de Pagamento"
        );
        assert_ne!(normalizar("Pagamento"), normalizar("pagamento"));
        assert_ne!(normalizar("ação"), normalizar("acao"));
    }

    #[test]
    fn so_texto_com_200_caracteres_ou_mais_tem_impressao() {
        let curto = "a".repeat(MINIMO_DE_CARACTERES - 1);
        let justo = "a".repeat(MINIMO_DE_CARACTERES);
        assert_eq!(impressao_de(&curto), None);
        assert_eq!(impressao_de(&"   \n ".repeat(200)), None);
        assert_eq!(impressao_de(&justo).map(|i| i.len()), Some(64));
        // Conta caracteres, não bytes: 199 letras acentuadas não bastam.
        assert_eq!(impressao_de(&"é".repeat(MINIMO_DE_CARACTERES - 1)), None);
        assert!(impressao_de(&"é".repeat(MINIMO_DE_CARACTERES)).is_some());
        // O mínimo vale para o texto normalizado: o espaço sobrando não conta.
        let com_folga = format!("   {}   \n\n", "a".repeat(MINIMO_DE_CARACTERES - 1));
        assert_eq!(impressao_de(&com_folga), None);
    }

    #[test]
    fn bytes_diferentes_com_o_mesmo_texto_dao_a_mesma_impressao() {
        let a = "Recibo de pagamento Exemplo S.A. ".repeat(10);
        let b = a.replace(' ', "  \r\n").trim_end().to_owned();
        assert_ne!(a, b);
        assert_eq!(impressao_de(&a), impressao_de(&b));
        assert_ne!(impressao_de(&a), impressao_de(&a.to_uppercase()));
    }
}
