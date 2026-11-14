//! A fila da triagem (spec 09, "Como gasta pouco", item 1). Saem da fila **antes** de
//! qualquer chamada:
//!
//! - os itens com decisão do usuário e os `promovido` (IA-01);
//! - as duplicatas: registros do mesmo documento com a mesma data, valor e descrição, e
//!   achados com o mesmo `canonical_fingerprint` (fica o de menor hash);
//! - os registros em par óbvio de conciliação, que a regra existente já sugere;
//! - os registros cujo fornecedor normalizado e tipo já têm categoria aprendida das
//!   decisões do usuário (IA-02): viram uma sugestão de regra, sem chamada;
//! - o que já tem sugestão (aberta, aceita ou recusada) nesta versão do prompt, o que
//!   torna repetir a rodada de graça (IA-04) e a retomada depois de uma pausa idempotente;
//! - os registros que já têm categoria, fornecedor e conta (a menos que se peça o
//!   contrário).
//!
//! A seleção só lê. Quem grava as sugestões de regra é [`crate::sugestoes`].

use std::collections::{BTreeMap, HashMap, HashSet};

use rusqlite::Connection;
use sbf_inventario::conciliacao::{Candidato, achar_pares};

use crate::erro::Resultado;
use crate::normalizar::normalizar_fornecedor;
use crate::tipos::Alvo;

/// A confiança de uma categoria aprendida com menos de [`EXEMPLOS_PARA_CONFIANCA_MAXIMA`]
/// decisões.
pub const CONFIANCA_DA_REGRA: f64 = 0.9;
/// A confiança com [`EXEMPLOS_PARA_CONFIANCA_MAXIMA`] decisões ou mais.
pub const CONFIANCA_DA_REGRA_FORTE: f64 = 0.97;
pub const EXEMPLOS_PARA_CONFIANCA_MAXIMA: usize = 3;
/// O `modelo` das sugestões que nasceram de regra.
pub const MODELO_DA_REGRA_DE_CATEGORIA: &str = "regra:categoria-aprendida";
/// O `prompt_versao` das sugestões de regra.
pub const VERSAO_DAS_REGRAS: &str = "regras";

#[derive(Debug, Clone)]
pub struct OpcoesDaFila {
    /// No máximo quantos itens vão à IA (o piloto usa 200).
    pub limite: Option<usize>,
    /// Registros com categoria, fornecedor e conta já preenchidos também entram.
    pub incluir_completos: bool,
    pub prompt_versao: String,
}

impl OpcoesDaFila {
    pub fn padrao() -> Self {
        Self {
            limite: None,
            incluir_completos: false,
            prompt_versao: crate::prompt::PROMPT_VERSAO.to_owned(),
        }
    }
}

/// Uma categoria que as decisões do usuário já ensinaram.
#[derive(Debug, Clone, PartialEq)]
pub struct SugestaoDeRegra {
    pub alvo: Alvo,
    pub campo: &'static str,
    pub valor: String,
    pub confianca: f64,
    pub justificativa: String,
}

/// Para onde foi cada candidato (os números que a tela mostra).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Contagens {
    pub decididos_pelo_usuario: usize,
    pub promovidos: usize,
    pub duplicatas: usize,
    pub em_par_de_conciliacao: usize,
    pub resolvidos_por_regra: usize,
    pub ja_sugeridos: usize,
    pub completos: usize,
    pub na_fila: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Fila {
    /// O que vai à IA, achados primeiro e registros por fornecedor.
    pub alvos: Vec<Alvo>,
    /// O que as regras gratuitas resolveram (a gravar como sugestões).
    pub regras: Vec<SugestaoDeRegra>,
    pub contagens: Contagens,
}

/// A categoria que as decisões do usuário ensinaram para (fornecedor, tipo).
#[derive(Debug, Clone, PartialEq)]
pub struct Aprendida {
    pub categoria: String,
    pub exemplos: usize,
}

/// Aprende (fornecedor normalizado, tipo) → categoria das decisões de categoria do usuário
/// que continuam em vigor. Só vale se **todas** concordam (`None` = conflito).
pub fn categorias_aprendidas(conn: &Connection) -> Resultado<HashMap<(String, String), Aprendida>> {
    let mut consulta = conn.prepare(
        "SELECT r.fornecedor_normalizado, r.supplier_name_raw, r.tipo, r.categoria_provavel
           FROM registro r
          WHERE r.decidido_por = 'usuario'
            AND r.categoria_provavel IS NOT NULL AND r.categoria_provavel <> ''
            AND EXISTS (
                SELECT 1 FROM decisao d
                 WHERE d.alvo_tipo = 'registro' AND d.alvo_id = CAST(r.id AS TEXT)
                   AND d.campo = 'categoria' AND d.autor = 'usuario' AND d.desfaz IS NULL
                   AND COALESCE(d.motivo, '') <> ?1
                   AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
                   AND json_extract(d.depois, '$.categoria_provavel') = r.categoria_provavel)",
    )?;
    // Aceitar uma sugestão da IA grava uma decisão "do usuário" com este motivo: não é
    // gabarito, senão a regra gratuita se ensina com as próprias sugestões.
    let linhas = consulta.query_map([crate::aceite::MOTIVO_DA_DECISAO], |l| {
        Ok((
            l.get::<_, Option<String>>(0)?,
            l.get::<_, Option<String>>(1)?,
            l.get::<_, String>(2)?,
            l.get::<_, String>(3)?,
        ))
    })?;
    // (fornecedor, tipo) -> categoria -> exemplos
    let mut contagem: BTreeMap<(String, String), BTreeMap<String, usize>> = BTreeMap::new();
    for linha in linhas {
        let (normalizado, bruto, tipo, categoria) = linha?;
        let Some(nome) = normalizado.filter(|n| !n.trim().is_empty()).or(bruto) else {
            continue;
        };
        let chave = normalizar_fornecedor(&nome);
        if chave.is_empty() {
            continue;
        }
        *contagem
            .entry((chave, tipo))
            .or_default()
            .entry(categoria)
            .or_default() += 1;
    }
    Ok(contagem
        .into_iter()
        .filter_map(|(chave, categorias)| {
            if categorias.len() != 1 {
                return None;
            }
            let (categoria, exemplos) = categorias.into_iter().next()?;
            Some((
                chave,
                Aprendida {
                    categoria,
                    exemplos,
                },
            ))
        })
        .collect())
}

/// Os registros que a regra de conciliação já sugere (os dois lados de cada par).
pub fn registros_em_par_obvio(conn: &Connection) -> Resultado<HashSet<i64>> {
    let mut consulta = conn.prepare(
        "SELECT r.id, substr(r.data, 1, 10), r.valor_centavos, r.conta_provavel,
                r.content_hash IS NULL, r.descricao_original
           FROM registro r
          WHERE r.valor_centavos IS NOT NULL AND r.data IS NOT NULL
            AND r.estado <> 'divergente'
            AND r.id NOT IN (
                SELECT a FROM relacao_registro WHERE tipo = 'conciliacao'
                UNION SELECT b FROM relacao_registro WHERE tipo = 'conciliacao')",
    )?;
    let candidatos = consulta
        .query_map([], |l| {
            Ok(Candidato {
                id: l.get(0)?,
                data: l.get(1)?,
                valor: l.get(2)?,
                conta: l.get(3)?,
                de_conexao: l.get(4)?,
                descricao: l.get(5)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(achar_pares(&candidatos)
        .into_iter()
        .flat_map(|p| [p.extrato.id, p.documento.id])
        .collect())
}

fn ja_sugeridos(conn: &Connection, versao: &str) -> Resultado<HashSet<(String, String)>> {
    let mut consulta = conn.prepare(
        "SELECT DISTINCT alvo_tipo, alvo_id FROM sugestao_ia
          WHERE prompt_versao = ?1 AND estado IN ('aberta', 'aceita', 'recusada')",
    )?;
    let linhas = consulta
        .query_map([versao], |l| Ok((l.get(0)?, l.get(1)?)))?
        .collect::<Result<HashSet<_>, _>>()?;
    Ok(linhas)
}

fn contar(conn: &Connection, sql: &str) -> Resultado<usize> {
    let n: i64 = conn.query_row(sql, [], |l| l.get(0))?;
    Ok(usize::try_from(n).unwrap_or(0))
}

/// Monta a fila. Só lê.
///
/// # Errors
/// Falha do SQLite.
pub fn selecionar(conn: &Connection, opcoes: &OpcoesDaFila) -> Resultado<Fila> {
    let mut contagens = Contagens {
        decididos_pelo_usuario: contar(
            conn,
            "SELECT (SELECT COUNT(*) FROM achado WHERE estado IN ('novo', 'duvida')
                        AND decidido_por = 'usuario')
                  + (SELECT COUNT(*) FROM registro WHERE estado <> 'promovido'
                        AND movimentacao_id IS NULL AND decidido_por = 'usuario')",
        )?,
        promovidos: contar(
            conn,
            "SELECT COUNT(*) FROM registro WHERE estado = 'promovido' OR movimentacao_id IS NOT NULL",
        )?,
        ..Contagens::default()
    };
    let sugeridos = ja_sugeridos(conn, &opcoes.prompt_versao)?;
    let em_par = registros_em_par_obvio(conn)?;
    let aprendidas = categorias_aprendidas(conn)?;

    let mut alvos: Vec<Alvo> = Vec::new();
    let mut regras: Vec<SugestaoDeRegra> = Vec::new();

    // Achados pendentes: novo ou em dúvida, sem decisão do usuário, sem duplicata.
    let mut achados = conn.prepare(
        "SELECT a.content_hash,
                a.canonical_fingerprint IS NOT NULL AND EXISTS (
                    SELECT 1 FROM achado o
                     WHERE o.canonical_fingerprint = a.canonical_fingerprint
                       AND o.content_hash < a.content_hash)
           FROM achado a
          WHERE a.estado IN ('novo', 'duvida') AND COALESCE(a.decidido_por, '') <> 'usuario'
          ORDER BY a.content_hash",
    )?;
    for linha in achados.query_map([], |l| Ok((l.get::<_, String>(0)?, l.get::<_, bool>(1)?)))? {
        let (hash, duplicata) = linha?;
        if duplicata {
            contagens.duplicatas += 1;
        } else if sugeridos.contains(&("achado".to_owned(), hash.clone())) {
            contagens.ja_sugeridos += 1;
        } else {
            alvos.push(Alvo::Achado(hash));
        }
    }

    // Registros: nunca promovido, nunca com decisão do usuário.
    let mut registros = conn.prepare(
        "SELECT r.id, r.fornecedor_normalizado, r.supplier_name_raw, r.tipo,
                r.categoria_provavel, r.conta_provavel,
                r.content_hash IS NOT NULL AND EXISTS (
                    SELECT 1 FROM registro o
                     WHERE o.content_hash = r.content_hash AND o.id < r.id
                       AND o.data = r.data AND o.valor_centavos = r.valor_centavos
                       AND COALESCE(o.descricao_original, '') = COALESCE(r.descricao_original, ''))
           FROM registro r
          WHERE r.estado <> 'promovido' AND r.movimentacao_id IS NULL
            AND COALESCE(r.decidido_por, '') <> 'usuario'
          ORDER BY COALESCE(r.fornecedor_normalizado, r.supplier_name_raw, ''), r.id",
    )?;
    let linhas = registros.query_map([], |l| {
        Ok((
            l.get::<_, i64>(0)?,
            l.get::<_, Option<String>>(1)?,
            l.get::<_, Option<String>>(2)?,
            l.get::<_, String>(3)?,
            l.get::<_, Option<String>>(4)?,
            l.get::<_, Option<String>>(5)?,
            l.get::<_, bool>(6)?,
        ))
    })?;
    let vazio = |t: &Option<String>| t.as_deref().is_none_or(|s| s.trim().is_empty());
    for linha in linhas {
        let (id, normalizado, bruto, tipo, categoria, conta, duplicata) = linha?;
        if duplicata {
            contagens.duplicatas += 1;
            continue;
        }
        if em_par.contains(&id) {
            contagens.em_par_de_conciliacao += 1;
            continue;
        }
        let nome = normalizado
            .clone()
            .filter(|n| !n.trim().is_empty())
            .or(bruto.clone());
        if let Some(nome) = &nome
            && let Some(a) = aprendidas.get(&(normalizar_fornecedor(nome), tipo.clone()))
        {
            contagens.resolvidos_por_regra += 1;
            let ja = sugeridos.contains(&("registro".to_owned(), id.to_string()));
            if !ja && categoria.as_deref() != Some(a.categoria.as_str()) {
                regras.push(SugestaoDeRegra {
                    alvo: Alvo::Registro(id),
                    campo: "categoria",
                    valor: a.categoria.clone(),
                    confianca: if a.exemplos >= EXEMPLOS_PARA_CONFIANCA_MAXIMA {
                        CONFIANCA_DA_REGRA_FORTE
                    } else {
                        CONFIANCA_DA_REGRA
                    },
                    justificativa: format!(
                        "você pôs {} nesse fornecedor e tipo em {} decis{}",
                        a.categoria,
                        a.exemplos,
                        if a.exemplos == 1 { "ão" } else { "ões" }
                    ),
                });
            }
            continue;
        }
        if sugeridos.contains(&("registro".to_owned(), id.to_string())) {
            contagens.ja_sugeridos += 1;
            continue;
        }
        if !opcoes.incluir_completos && !vazio(&categoria) && !vazio(&nome) && !vazio(&conta) {
            contagens.completos += 1;
            continue;
        }
        alvos.push(Alvo::Registro(id));
    }
    // As sugestões de regra são de graça: todas entram. O limite vale só para a IA.
    if let Some(limite) = opcoes.limite {
        alvos.truncate(limite);
    }
    contagens.na_fila = alvos.len();
    Ok(Fila {
        alvos,
        regras,
        contagens,
    })
}
