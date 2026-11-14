//! O piloto de 200 itens (spec 09, D4 e IA-09): antes da rodada inteira, mede o **acerto**
//! das sugestões contra o que o usuário já decidiu e o **consumo**.
//!
//! Como funciona:
//! 1. [`amostra`] pega até 200 itens que o usuário **já decidiu** (decisão dele em vigor,
//!    não desfeita) em algum campo que a IA sugere. São a resposta de gabarito.
//! 2. A rodada do piloto não mostra à IA o que o classificador já pôs nem a decisão, e grava
//!    as sugestões como `obsoleta` (o usuário já decidiu: elas nunca entram na lista).
//! 3. [`calcular`] compara cada sugestão com a **última decisão do usuário em vigor** sobre
//!    o mesmo campo. Sugestão sem decisão do usuário no campo não entra na conta.
//!
//! O acerto também sai por faixa de confiança, para ver se a confiança está calibrada.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use serde_json::Value;

use crate::erro::Resultado;
use crate::normalizar::normalizar;
use crate::sugestoes::{NAO_CLASSIFICADO, Rodada, ler_rodada};
use crate::tipos::Alvo;

/// O tamanho do piloto.
pub const TAMANHO_DO_PILOTO: usize = 200;

/// Os itens do piloto: os mais recentemente decididos pelo usuário.
///
/// # Errors
/// Falha do SQLite.
pub fn amostra(conn: &Connection, n: usize) -> Resultado<Vec<Alvo>> {
    let mut c = conn.prepare(
        "SELECT d.alvo_tipo, d.alvo_id, MAX(d.id) AS ultima
           FROM decisao d
          WHERE d.autor = 'usuario' AND d.desfaz IS NULL
            AND d.alvo_tipo IN ('achado', 'registro')
            AND d.campo IN ('estado', 'document_type', 'fornecedor', 'categoria', 'tipo', 'data_e_valor')
            AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
          GROUP BY d.alvo_tipo, d.alvo_id
          ORDER BY ultima DESC LIMIT ?1",
    )?;
    let linhas = c
        .query_map([i64::try_from(n).unwrap_or(i64::MAX)], |l| {
            Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(linhas
        .into_iter()
        .filter_map(|(t, i)| Alvo::de(&t, &i))
        .collect())
}

/// O acerto de um grupo de sugestões.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Placar {
    pub comparadas: usize,
    pub acertos: usize,
}

impl Placar {
    /// A taxa de acerto de 0 a 1, ou `None` sem comparações.
    pub fn taxa(&self) -> Option<f64> {
        (self.comparadas > 0).then(|| self.acertos as f64 / self.comparadas as f64)
    }

    fn contar(&mut self, acertou: bool) {
        self.comparadas += 1;
        self.acertos += usize::from(acertou);
    }
}

/// O relatório do piloto.
#[derive(Debug, Clone, PartialEq)]
pub struct RelatorioDoPiloto {
    pub rodada: Rodada,
    pub itens: usize,
    pub nao_classificados: usize,
    /// Sugestões sem decisão do usuário no campo (não entram no acerto).
    pub sem_gabarito: usize,
    pub geral: Placar,
    /// Por campo da sugestão.
    pub por_campo: BTreeMap<String, Placar>,
    /// Por faixa de confiança: "abaixo de 0,7", "0,7 a 0,9", "0,9 ou mais".
    pub por_faixa: BTreeMap<&'static str, Placar>,
    pub tokens_entrada: u64,
    pub tokens_saida: u64,
    /// Nominal na assinatura. Dezmilésimos de dólar.
    pub custo_dezmilesimos_usd: u64,
    /// Estimativa do que a rodada inteira consome, por item, em tokens (entrada + saída).
    pub tokens_por_item: Option<u64>,
}

pub const FAIXA_BAIXA: &str = "abaixo de 0,7";
pub const FAIXA_MEDIA: &str = "0,7 a 0,9";
pub const FAIXA_ALTA: &str = "0,9 ou mais";

fn faixa(confianca: f64) -> &'static str {
    if confianca < 0.7 {
        FAIXA_BAIXA
    } else if confianca < 0.9 {
        FAIXA_MEDIA
    } else {
        FAIXA_ALTA
    }
}

/// A coluna da decisão que carrega o gabarito de um campo de sugestão.
pub(crate) fn gabarito_do_campo(
    alvo_tipo: &str,
    campo: &str,
) -> Option<(&'static str, &'static str)> {
    match (alvo_tipo, campo) {
        ("achado", "e_financeiro") => Some(("estado", "estado")),
        ("achado", "tipo_documento") => Some(("document_type", "document_type")),
        ("achado", "fornecedor") => Some(("fornecedor", "supplier_name_raw")),
        ("registro", "fornecedor") => Some(("fornecedor", "fornecedor_normalizado")),
        ("registro", "categoria") => Some(("categoria", "categoria_provavel")),
        ("registro", "tipo") => Some(("tipo", "tipo")),
        ("achado", "data_chave") => Some(("data_e_valor", "data_chave")),
        ("achado", "valor") => Some(("data_e_valor", "valor_centavos")),
        _ => None,
    }
}

/// O valor que a decisão em vigor do usuário deu à `coluna` do `campo_da_decisao`.
fn resposta_do_usuario(
    conn: &Connection,
    alvo: &Alvo,
    campo_da_decisao: &str,
    coluna: &str,
) -> Resultado<Option<Value>> {
    let depois: Option<String> = conn
        .query_row(
            "SELECT d.depois FROM decisao d
              WHERE d.alvo_tipo = ?1 AND d.alvo_id = ?2 AND d.campo = ?3
                AND d.autor = 'usuario' AND d.desfaz IS NULL
                AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
                AND json_extract(d.depois, '$.' || ?4) IS NOT NULL
              ORDER BY d.id DESC LIMIT 1",
            params![alvo.tipo(), alvo.id(), campo_da_decisao, coluna],
            |l| l.get(0),
        )
        .optional()?
        .flatten();
    Ok(depois
        .and_then(|d| serde_json::from_str::<Value>(&d).ok())
        .and_then(|v| v.get(coluna).cloned()))
}

/// A sugestão bate com a resposta do usuário?
fn bate(campo: &str, sugerido: &Value, resposta: &Value) -> bool {
    match campo {
        "e_financeiro" => {
            let esperado = match sugerido.as_str() {
                Some("sim") => "documento",
                Some("nao") => "descartado",
                Some("incerto") => "duvida",
                _ => return false,
            };
            resposta.as_str() == Some(esperado)
        }
        "valor" => sugerido.as_i64().is_some() && sugerido.as_i64() == resposta.as_i64(),
        _ => match (sugerido.as_str(), resposta.as_str()) {
            (Some(a), Some(b)) => normalizar(a) == normalizar(b),
            _ => false,
        },
    }
}

/// Calcula o relatório do piloto da rodada `rodada_id`.
///
/// # Errors
/// Falha do SQLite, ou rodada inexistente.
pub fn calcular(conn: &Connection, rodada_id: i64) -> Resultado<RelatorioDoPiloto> {
    let rodada = ler_rodada(conn, rodada_id)?
        .ok_or_else(|| crate::ErroDaTriagem::Outro(format!("rodada {rodada_id} não existe")))?;
    let mut c = conn.prepare(
        "SELECT alvo_tipo, alvo_id, campo, valor_json, confianca FROM sugestao_ia
          WHERE rodada_id = ?1 ORDER BY id",
    )?;
    let linhas = c
        .query_map([rodada_id], |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, Option<String>>(3)?,
                l.get::<_, f64>(4)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut relatorio = RelatorioDoPiloto {
        itens: usize::try_from(rodada.itens).unwrap_or(0),
        nao_classificados: 0,
        sem_gabarito: 0,
        geral: Placar::default(),
        por_campo: BTreeMap::new(),
        por_faixa: BTreeMap::new(),
        tokens_entrada: 0,
        tokens_saida: 0,
        custo_dezmilesimos_usd: 0,
        tokens_por_item: None,
        rodada,
    };
    for (tipo, id, campo, valor, confianca) in linhas {
        if campo == NAO_CLASSIFICADO {
            relatorio.nao_classificados += 1;
            continue;
        }
        let (Some(alvo), Some((campo_da_decisao, coluna))) =
            (Alvo::de(&tipo, &id), gabarito_do_campo(&tipo, &campo))
        else {
            relatorio.sem_gabarito += 1;
            continue;
        };
        let Some(resposta) = resposta_do_usuario(conn, &alvo, campo_da_decisao, coluna)? else {
            relatorio.sem_gabarito += 1;
            continue;
        };
        let sugerido = valor
            .and_then(|v| serde_json::from_str::<Value>(&v).ok())
            .unwrap_or(Value::Null);
        let acertou = bate(&campo, &sugerido, &resposta);
        relatorio.geral.contar(acertou);
        relatorio
            .por_campo
            .entry(campo)
            .or_default()
            .contar(acertou);
        relatorio
            .por_faixa
            .entry(faixa(confianca))
            .or_default()
            .contar(acertou);
    }
    let (ent, sai, custo): (i64, i64, i64) = conn.query_row(
        "SELECT COALESCE(SUM(tokens_entrada), 0), COALESCE(SUM(tokens_saida), 0),
                COALESCE(SUM(custo_dezmilesimos_usd), 0)
           FROM saida_ia WHERE rodada_id = ?1",
        [rodada_id],
        |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
    )?;
    relatorio.tokens_entrada = u64::try_from(ent).unwrap_or(0);
    relatorio.tokens_saida = u64::try_from(sai).unwrap_or(0);
    relatorio.custo_dezmilesimos_usd = u64::try_from(custo).unwrap_or(0);
    if relatorio.itens > 0 {
        relatorio.tokens_por_item =
            Some((relatorio.tokens_entrada + relatorio.tokens_saida) / relatorio.itens as u64);
    }
    Ok(relatorio)
}

/// Roda o piloto: a amostra de [`TAMANHO_DO_PILOTO`] itens já decididos, a rodada em modo
/// piloto e o relatório. Os argumentos de fila são os de [`crate::rodada::rodar`].
///
/// # Errors
/// Falha do SQLite.
#[allow(clippy::too_many_arguments)]
pub fn rodar_piloto(
    conn: &Connection,
    fila: &std::sync::Arc<sbf_ia::Fila>,
    provedor: &str,
    n: usize,
    controle: &sbf_ia::Controle,
    progresso: &std::sync::Mutex<crate::rodada::Progresso>,
    ao_abrir: &dyn Fn(i64),
) -> Resultado<RelatorioDoPiloto> {
    let alvos = amostra(conn, n)?;
    let config = crate::rodada::ConfigDaRodada {
        piloto: true,
        ..crate::rodada::ConfigDaRodada::default()
    };
    let resumo = crate::rodada::rodar(
        conn, fila, provedor, &alvos, &config, controle, progresso, ao_abrir,
    )?;
    let id = resumo
        .rodada_id
        .ok_or_else(|| crate::ErroDaTriagem::Outro("a rodada não abriu".to_owned()))?;
    calcular(conn, id)
}

#[cfg(test)]
mod testes {
    use serde_json::json;

    use super::*;

    #[test]
    fn faixas_de_confianca() {
        assert_eq!(faixa(0.69), FAIXA_BAIXA);
        assert_eq!(faixa(0.7), FAIXA_MEDIA);
        assert_eq!(faixa(0.9), FAIXA_ALTA);
    }

    #[test]
    fn e_financeiro_compara_com_o_estado() {
        assert!(bate("e_financeiro", &json!("sim"), &json!("documento")));
        assert!(bate("e_financeiro", &json!("nao"), &json!("descartado")));
        assert!(!bate("e_financeiro", &json!("sim"), &json!("descartado")));
    }

    #[test]
    fn texto_compara_sem_acento_nem_caixa() {
        assert!(bate("categoria", &json!("Saúde"), &json!("saude")));
        assert!(!bate("categoria", &json!("Mercado"), &json!("Saúde")));
        assert!(bate("valor", &json!(18750), &json!(18750)));
    }
}
