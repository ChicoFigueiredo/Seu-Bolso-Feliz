//! Aceitar e recusar sugestões em lote (spec 09, "Janela"; IA-07 e IA-08).
//!
//! Aceitar uma sugestão grava **uma decisão do usuário por item**, pelo `Comandos` do
//! inventário, com o motivo "sugestão da IA". Por isso o desfazer é o de sempre
//! (`Comandos::desfazer`, ou "Desfazer decisão"): a decisão é igual à que o usuário
//! tomaria à mão. As sugestões de um mesmo valor viram **um** pedido com vários alvos.
//!
//! Não viram decisão (ficam só para leitura, com o motivo devolvido): `conta` e
//! `duplicata_de` de registro, que não têm Ação correspondente no §10 nesta fatia. Um
//! registro `promovido` é terminal e nunca recebe decisão: a sugestão vira `obsoleta`.

use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension, params};
use sbf_inventario::comando::{Acao, Comandos, IdDecisao, PedidoDeDecisao};
use serde_json::Value;

use crate::erro::Resultado;
use crate::sugestoes::{EstadoDaSugestao, SugestaoListada, marcar, usuario_decidiu_o_campo};
use crate::tipos::Alvo;

/// O motivo gravado em cada decisão aceita.
pub const MOTIVO_DA_DECISAO: &str = "sugestão da IA";

/// O resultado de um aceite em lote.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultadoDoAceite {
    /// As sugestões que viraram decisão.
    pub aceitas: Vec<i64>,
    /// As decisões gravadas (para "Desfazer"), uma por item.
    pub decisoes: Vec<IdDecisao>,
    /// As que não viraram decisão, com o motivo para a tela.
    pub ignoradas: Vec<(i64, String)>,
}

/// A Ação do §10 que a sugestão pede, ou o motivo de não haver.
///
/// # Errors
/// Texto com o motivo, para a tela.
pub fn acao_da_sugestao(s: &SugestaoListada) -> Result<Acao, String> {
    let texto = || {
        s.valor
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| "o valor sugerido não é um texto".to_owned())
    };
    match (&s.alvo, s.campo.as_str()) {
        (Alvo::Achado(_), "e_financeiro") => match s.valor.as_str() {
            Some("sim") => Ok(Acao::MarcarDocumento),
            Some("nao") => Ok(Acao::Descartar),
            Some("incerto") => Ok(Acao::EmDuvida),
            _ => Err("valor de e_financeiro desconhecido".to_owned()),
        },
        (Alvo::Achado(_), "tipo_documento") | (Alvo::Registro(_), "tipo") => {
            Ok(Acao::CorrigirTipo(texto()?))
        }
        (_, "fornecedor") => Ok(Acao::CorrigirFornecedor(texto()?)),
        (Alvo::Registro(_), "categoria") => Ok(Acao::CorrigirCategoria(texto()?)),
        (Alvo::Achado(_), "data_chave") => Ok(Acao::CorrigirDataEValor {
            data: Some(texto()?),
            valor_centavos: None,
        }),
        (Alvo::Achado(_), "valor") => Ok(Acao::CorrigirDataEValor {
            data: None,
            valor_centavos: Some(
                s.valor
                    .as_i64()
                    .ok_or_else(|| "o valor sugerido não é um número".to_owned())?,
            ),
        }),
        (Alvo::Registro(_), "conta" | "duplicata_de") => {
            Err("sem Ação correspondente nesta versão: só leitura".to_owned())
        }
        _ => Err("campo sem Ação correspondente".to_owned()),
    }
}

fn estado_da_sugestao(conn: &Connection, id: i64) -> Resultado<Option<String>> {
    Ok(conn
        .query_row("SELECT estado FROM sugestao_ia WHERE id = ?1", [id], |l| {
            l.get(0)
        })
        .optional()?)
}

pub(crate) fn registro_promovido(conn: &Connection, id: i64) -> Resultado<bool> {
    Ok(conn
        .query_row(
            "SELECT estado = 'promovido' OR movimentacao_id IS NOT NULL
               FROM registro WHERE id = ?1",
            [id],
            |l| l.get::<_, bool>(0),
        )
        .optional()?
        .unwrap_or(false))
}

/// Carrega as sugestões `ids` que ainda estão abertas.
fn abertas(conn: &Connection, ids: &[i64]) -> Resultado<Vec<SugestaoListada>> {
    let mut saida = Vec::new();
    for id in ids {
        if estado_da_sugestao(conn, *id)?.as_deref() != Some("aberta") {
            continue;
        }
        let linha = conn.query_row(
            "SELECT alvo_tipo, alvo_id, campo, valor_json, confianca, justificativa, modelo
               FROM sugestao_ia WHERE id = ?1",
            [id],
            |l| {
                Ok((
                    l.get::<_, String>(0)?,
                    l.get::<_, String>(1)?,
                    l.get::<_, String>(2)?,
                    l.get::<_, Option<String>>(3)?,
                    l.get::<_, f64>(4)?,
                    l.get::<_, String>(5)?,
                    l.get::<_, String>(6)?,
                ))
            },
        )?;
        let Some(alvo) = Alvo::de(&linha.0, &linha.1) else {
            continue;
        };
        saida.push(SugestaoListada {
            id: *id,
            titulo: linha.1.clone(),
            alvo,
            campo: linha.2,
            valor: linha
                .3
                .and_then(|v| serde_json::from_str(&v).ok())
                .unwrap_or(Value::Null),
            confianca: linha.4,
            justificativa: linha.5,
            modelo: linha.6,
        });
    }
    Ok(saida)
}

/// Aceita as sugestões `ids`: uma decisão por item, pelo `Comandos`.
///
/// `conn` é a conexão de escrita da triagem (para ler e marcar as sugestões); `comandos`
/// é o `Inventario` (ou a implementação fictícia). Se um grupo falha inteiro (por exemplo,
/// um item sumiu), refaz item a item para a falha de um não derrubar os outros.
///
/// # Errors
/// Falha do SQLite. Falha de decisão de um item vai para `ignoradas`.
pub fn aceitar(
    conn: &Connection,
    comandos: &mut dyn Comandos,
    ids: &[i64],
) -> Resultado<ResultadoDoAceite> {
    let mut resultado = ResultadoDoAceite::default();
    // Os grupos: a mesma Ação, vários alvos, numa chamada só.
    let mut grupos: BTreeMap<String, (Acao, Vec<SugestaoListada>)> = BTreeMap::new();
    let abertas = abertas(conn, ids)?;
    let abertas_ids: Vec<i64> = abertas.iter().map(|s| s.id).collect();
    for id in ids {
        if !abertas_ids.contains(id) {
            resultado
                .ignoradas
                .push((*id, "a sugestão não está mais aberta".to_owned()));
        }
    }
    for s in abertas {
        if let Alvo::Registro(id) = s.alvo
            && registro_promovido(conn, id)?
        {
            marcar(conn, &[s.id], EstadoDaSugestao::Obsoleta)?;
            resultado
                .ignoradas
                .push((s.id, "o registro já foi promovido".to_owned()));
            continue;
        }
        // O usuário já decidiu este campo depois da sugestão (por outro caminho): ela é velha.
        let criada_em: String = conn.query_row(
            "SELECT criada_em FROM sugestao_ia WHERE id = ?1",
            [s.id],
            |l| l.get(0),
        )?;
        if usuario_decidiu_o_campo(conn, &s.alvo, &s.campo, Some(&criada_em))? {
            marcar(conn, &[s.id], EstadoDaSugestao::Obsoleta)?;
            resultado.ignoradas.push((
                s.id,
                "o campo já foi decidido por você depois desta sugestão".to_owned(),
            ));
            continue;
        }
        match acao_da_sugestao(&s) {
            Ok(acao) => {
                let chave = serde_json::to_string(&acao).unwrap_or_default();
                grupos
                    .entry(chave)
                    .or_insert_with(|| (acao, Vec::new()))
                    .1
                    .push(s);
            }
            Err(motivo) => resultado.ignoradas.push((s.id, motivo)),
        }
    }
    for (_, (acao, sugestoes)) in grupos {
        let pedido = |lista: &[SugestaoListada]| PedidoDeDecisao {
            acao: acao.clone(),
            alvos: lista.iter().map(|s| s.alvo.referencia()).collect(),
            motivo: Some(MOTIVO_DA_DECISAO.to_owned()),
        };
        match comandos.executar(&pedido(&sugestoes)) {
            Ok(decisoes) => {
                resultado.decisoes.extend(decisoes);
                resultado.aceitas.extend(sugestoes.iter().map(|s| s.id));
            }
            Err(_) if sugestoes.len() > 1 => {
                for s in &sugestoes {
                    match comandos.executar(&pedido(std::slice::from_ref(s))) {
                        Ok(decisoes) => {
                            resultado.decisoes.extend(decisoes);
                            resultado.aceitas.push(s.id);
                        }
                        Err(e) => resultado.ignoradas.push((s.id, e.to_string())),
                    }
                }
            }
            Err(e) => {
                for s in &sugestoes {
                    resultado.ignoradas.push((s.id, e.to_string()));
                }
            }
        }
    }
    // A decisão obsoletou a sugestão (IA-08); aqui ela passa a `aceita`.
    marcar(conn, &resultado.aceitas, EstadoDaSugestao::Aceita)?;
    Ok(resultado)
}

/// Recusa as sugestões `ids` (não grava decisão). Devolve quantas estavam abertas.
///
/// # Errors
/// Falha do SQLite.
pub fn recusar(conn: &Connection, ids: &[i64]) -> Resultado<usize> {
    let mut n = 0;
    for id in ids {
        n += conn.execute(
            "UPDATE sugestao_ia SET estado = 'recusada' WHERE id = ?1 AND estado = 'aberta'",
            params![id],
        )?;
    }
    Ok(n)
}
