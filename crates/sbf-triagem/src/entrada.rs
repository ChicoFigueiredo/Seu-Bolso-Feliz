//! A entrada de cada chamada, no nível de confiança **Completo** (spec 09, D2): o texto
//! extraído vai (truncado, veja [`crate::normalizar::truncar_texto`]), junto com os campos
//! do registro ou do achado. O CPF e os números não são mascarados. A imagem não vai nesta
//! versão do contrato (`Pedido.entrada` é texto): o achado sem texto vai só com o nome do
//! arquivo, e a IA responde com confiança baixa.
//!
//! Limites, documentados aqui porque pesam no custo:
//! - texto de achado: até [`crate::normalizar::LIMITE_DO_TEXTO`] caracteres (início 3000 e
//!   fim 1000);
//! - trecho de registro: até [`LIMITE_DO_TRECHO`] caracteres, e só quando o registro não
//!   tem descrição nem fornecedor;
//! - lote: até [`crate::prompt::ITENS_POR_LOTE`] itens (a fila do motor corta; com o texto
//!   limitado a 4000 caracteres por item, um lote cheio tem no máximo ~100 mil caracteres).

use std::fmt::Write as _;

use rusqlite::{Connection, OptionalExtension, params};

use crate::erro::Resultado;
use crate::normalizar::{cortar, truncar_texto};
use crate::tipos::{Alvo, Item, ItemDeAchado, ItemDeRegistro};

pub const LIMITE_DO_TRECHO: usize = 800;
/// Quantos nomes de origem de um achado vão ao modelo.
pub const NOMES_POR_ACHADO: usize = 3;
/// Quantos registros parecidos vão por registro.
pub const PARECIDOS_POR_REGISTRO: usize = 3;

/// Os nomes que o usuário já usa, para a IA reaproveitar (regra 5 do prompt).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Vocabulario {
    pub categorias: Vec<String>,
    pub contas: Vec<String>,
}

impl Vocabulario {
    /// As 40 categorias e as 15 contas mais usadas nos registros.
    ///
    /// # Errors
    /// Falha do SQLite.
    pub fn do_banco(conn: &Connection) -> Resultado<Self> {
        let lista = |coluna: &str, limite: i64| -> Resultado<Vec<String>> {
            let mut c = conn.prepare(&format!(
                "SELECT {coluna} FROM registro WHERE {coluna} IS NOT NULL AND {coluna} <> ''
                  GROUP BY {coluna} ORDER BY COUNT(*) DESC, {coluna} LIMIT ?1"
            ))?;
            Ok(c.query_map([limite], |l| l.get(0))?
                .collect::<Result<Vec<_>, _>>()?)
        };
        Ok(Self {
            categorias: lista("categoria_provavel", 40)?,
            contas: lista("conta_provavel", 15)?,
        })
    }
}

/// `-R$ 23,40`, `R$ 187,50`.
pub fn formatar_reais(centavos: i64) -> String {
    let sinal = if centavos < 0 { "-" } else { "" };
    let abs = centavos.unsigned_abs();
    format!("{sinal}R$ {},{:02}", abs / 100, abs % 100)
}

fn vazio(t: &Option<String>) -> bool {
    t.as_deref().is_none_or(|s| s.trim().is_empty())
}

/// Lê um item do banco. `mostrar_atuais = false` esconde o que o classificador automático
/// já pôs (tipo, fornecedor, data, valor, categoria, conta): o piloto usa isso para não
/// entregar a resposta que o usuário deu.
///
/// # Errors
/// Falha do SQLite.
pub fn carregar_item(
    conn: &Connection,
    alvo: &Alvo,
    mostrar_atuais: bool,
) -> Resultado<Option<Item>> {
    match alvo {
        Alvo::Achado(hash) => carregar_achado(conn, hash, mostrar_atuais),
        Alvo::Registro(id) => carregar_registro(conn, *id, mostrar_atuais),
    }
}

fn carregar_achado(conn: &Connection, hash: &str, atuais: bool) -> Resultado<Option<Item>> {
    let base = conn
        .query_row(
            "SELECT a.mime_type, a.document_type, a.supplier_name_raw, a.data_chave,
                    a.valor_centavos, t.conteudo
               FROM achado a LEFT JOIN texto t ON t.content_hash = a.content_hash
              WHERE a.content_hash = ?1",
            [hash],
            |l| {
                Ok((
                    l.get::<_, Option<String>>(0)?,
                    l.get::<_, Option<String>>(1)?,
                    l.get::<_, Option<String>>(2)?,
                    l.get::<_, Option<String>>(3)?,
                    l.get::<_, Option<i64>>(4)?,
                    l.get::<_, Option<String>>(5)?,
                ))
            },
        )
        .optional()?;
    let Some((mime, tipo, fornecedor, data, valor, texto)) = base else {
        return Ok(None);
    };
    let mut origens = conn.prepare(
        "SELECT COALESCE(NULLIF(gmail_subject, ''), filename) FROM origem
          WHERE content_hash = ?1 ORDER BY id LIMIT ?2",
    )?;
    let nomes = origens
        .query_map(params![hash, NOMES_POR_ACHADO as i64], |l| {
            l.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(Item::Achado(ItemDeAchado {
        content_hash: hash.to_owned(),
        nomes,
        mime_type: mime,
        document_type: tipo.filter(|_| atuais),
        fornecedor: fornecedor.filter(|_| atuais),
        data_chave: data.filter(|_| atuais),
        valor_centavos: valor.filter(|_| atuais),
        texto,
    })))
}

fn carregar_registro(conn: &Connection, id: i64, atuais: bool) -> Resultado<Option<Item>> {
    let base = conn
        .query_row(
            "SELECT r.tipo, substr(r.data, 1, 10), r.valor_centavos, r.descricao_original,
                    r.fonte_id, r.supplier_name_raw, r.fornecedor_normalizado,
                    r.categoria_provavel, r.conta_provavel, t.conteudo
               FROM registro r LEFT JOIN texto t ON t.content_hash = r.content_hash
              WHERE r.id = ?1",
            [id],
            |l| {
                Ok(ItemDeRegistro {
                    id,
                    tipo: l.get(0)?,
                    data: l.get(1)?,
                    valor_centavos: l.get(2)?,
                    descricao: l.get(3)?,
                    fonte: l.get(4)?,
                    fornecedor_bruto: l.get(5)?,
                    fornecedor: l.get::<_, Option<String>>(6)?.filter(|_| atuais),
                    categoria: l.get::<_, Option<String>>(7)?.filter(|_| atuais),
                    conta: l.get::<_, Option<String>>(8)?.filter(|_| atuais),
                    trecho: l.get::<_, Option<String>>(9)?,
                    parecidos: Vec::new(),
                })
            },
        )
        .optional()?;
    let Some(mut r) = base else {
        return Ok(None);
    };
    if !(vazio(&r.descricao) && vazio(&r.fornecedor_bruto)) {
        r.trecho = None;
    } else {
        r.trecho = r
            .trecho
            .map(|t| cortar(&crate::normalizar::compactar_espacos(&t), LIMITE_DO_TRECHO));
    }
    let mut parecidos = conn.prepare(
        "SELECT id FROM registro
          WHERE id <> ?1 AND valor_centavos = ?2
            AND data BETWEEN date(?3, '-3 day') AND date(?3, '+3 day') || 'Z'
          ORDER BY ABS(julianday(substr(data, 1, 10)) - julianday(?3)), id LIMIT ?4",
    )?;
    r.parecidos = parecidos
        .query_map(
            params![id, r.valor_centavos, r.data, PARECIDOS_POR_REGISTRO as i64],
            |l| l.get(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Some(Item::Registro(r)))
}

fn campo(saida: &mut String, rotulo: &str, valor: &Option<String>) {
    if let Some(v) = valor.as_deref().filter(|v| !v.trim().is_empty()) {
        let _ = writeln!(saida, "{rotulo}: {}", cortar(&v.replace('\n', " "), 200));
    }
}

/// O bloco de um item no prompt.
pub fn bloco_do_item(n: usize, item: &Item) -> String {
    let mut s = String::new();
    match item {
        Item::Achado(a) => {
            let _ = writeln!(s, "=== ITEM {n} (achado) ===");
            if !a.nomes.is_empty() {
                let _ = writeln!(s, "arquivo: {}", cortar(&a.nomes.join(" | "), 300));
            }
            campo(&mut s, "mime", &a.mime_type);
            campo(&mut s, "tipo atual", &a.document_type);
            campo(&mut s, "fornecedor atual", &a.fornecedor);
            campo(&mut s, "data atual", &a.data_chave);
            if let Some(v) = a.valor_centavos {
                let _ = writeln!(s, "valor atual: {}", formatar_reais(v));
            }
            match a.texto.as_deref().map(truncar_texto) {
                Some(t) if !t.is_empty() => {
                    let _ = writeln!(s, "texto:\n{t}");
                }
                _ => s.push_str("texto: (sem texto extraído)\n"),
            }
        }
        Item::Registro(r) => {
            let _ = writeln!(s, "=== ITEM {n} (registro {}) ===", r.id);
            let mut linha = Vec::new();
            if let Some(d) = r.descricao.as_deref().filter(|d| !d.trim().is_empty()) {
                linha.push(format!("descricao: {}", cortar(&d.replace('\n', " "), 200)));
            }
            if let Some(f) = r
                .fornecedor_bruto
                .as_deref()
                .filter(|d| !d.trim().is_empty())
            {
                linha.push(format!("fornecedor bruto: {}", cortar(f, 100)));
            }
            linha.push(format!("data: {}", r.data));
            linha.push(format!("valor: {}", formatar_reais(r.valor_centavos)));
            linha.push(format!("tipo atual: {}", r.tipo));
            linha.push(format!("fonte: {}", r.fonte));
            let _ = writeln!(s, "{}", linha.join(" | "));
            campo(&mut s, "fornecedor atual", &r.fornecedor);
            campo(&mut s, "categoria atual", &r.categoria);
            campo(&mut s, "conta atual", &r.conta);
            if !r.parecidos.is_empty() {
                let ids: Vec<String> = r.parecidos.iter().map(i64::to_string).collect();
                let _ = writeln!(s, "parecidos: {}", ids.join(", "));
            }
            if let Some(t) = r.trecho.as_deref().filter(|t| !t.is_empty()) {
                let _ = writeln!(s, "trecho do documento:\n{t}");
            }
        }
    }
    s
}

/// O cabeçalho do lote: o vocabulário que o usuário já usa.
pub fn cabecalho(vocabulario: &Vocabulario) -> String {
    let mut s = String::new();
    if !vocabulario.categorias.is_empty() {
        let _ = writeln!(
            s,
            "categorias já usadas: {}",
            vocabulario.categorias.join(", ")
        );
    }
    if !vocabulario.contas.is_empty() {
        let _ = writeln!(s, "contas já usadas: {}", vocabulario.contas.join(", "));
    }
    s
}

/// Carrega os itens de `alvos` (os que sumiram do banco ficam de fora). `mostrar_atuais`
/// como em [`carregar_item`].
///
/// # Errors
/// Falha do SQLite.
pub fn carregar_itens(
    conn: &Connection,
    alvos: &[Alvo],
    mostrar_atuais: bool,
) -> Resultado<Vec<Item>> {
    let mut itens = Vec::with_capacity(alvos.len());
    for alvo in alvos {
        if let Some(item) = carregar_item(conn, alvo, mostrar_atuais)? {
            itens.push(item);
        }
    }
    Ok(itens)
}

/// As decisões do usuário parecidas com cada item, por `tipo:id` do alvo (v2 do prompt).
pub type Exemplos = std::collections::HashMap<String, Vec<String>>;

/// A chave de um item em [`Exemplos`].
pub fn chave_de_exemplos(alvo: &crate::tipos::Alvo) -> String {
    format!("{}:{}", alvo.tipo(), alvo.id())
}

/// A entrada de um lote: o cabeçalho e os itens numerados de 1, na ordem recebida.
pub fn montar_entrada(vocabulario: &Vocabulario, itens: &[&Item]) -> String {
    montar_entrada_com_exemplos(vocabulario, itens, &Exemplos::new())
}

/// Como [`montar_entrada`], com as decisões parecidas de cada item logo depois do bloco dele.
pub fn montar_entrada_com_exemplos(
    vocabulario: &Vocabulario,
    itens: &[&Item],
    exemplos: &Exemplos,
) -> String {
    let mut entrada = cabecalho(vocabulario);
    for (i, item) in itens.iter().enumerate() {
        entrada.push_str(&bloco_do_item(i + 1, item));
        if let Some(lista) = exemplos.get(&chave_de_exemplos(&item.alvo()))
            && !lista.is_empty()
        {
            entrada.push_str("decisões suas parecidas:\n");
            for e in lista {
                let _ = writeln!(entrada, "- {e}");
            }
        }
    }
    entrada
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn reais_com_sinal_e_centavos() {
        assert_eq!(formatar_reais(18750), "R$ 187,50");
        assert_eq!(formatar_reais(-2340), "-R$ 23,40");
        assert_eq!(formatar_reais(5), "R$ 0,05");
    }

    #[test]
    fn achado_sem_texto_diz_que_nao_tem() {
        let bloco = bloco_do_item(
            2,
            &Item::Achado(ItemDeAchado {
                content_hash: "x".into(),
                nomes: vec!["IMG_0042.jpg".into()],
                ..ItemDeAchado::default()
            }),
        );
        assert!(bloco.starts_with("=== ITEM 2 (achado) ==="));
        assert!(bloco.contains("texto: (sem texto extraído)"));
    }
}
