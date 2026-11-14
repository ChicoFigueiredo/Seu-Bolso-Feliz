//! Exemplos por semelhança no prompt `triagem-v2` (spec 09, camada 2; spec 10, "Usos" item 2).
//!
//! Além das decisões do usuário sobre o **mesmo fornecedor** ([`crate::agente::Agente::exemplos_para`]),
//! o índice local acha os itens de **texto parecido** e entram as decisões do usuário sobre
//! eles, até o teto de [`crate::prompt::EXEMPLOS_NO_PROMPT`] por item. Só vão decisões do
//! usuário (nunca sugestão da IA nem decisão de regra), como no caso do fornecedor.
//!
//! A busca é de melhor esforço: se o índice falha ou está vazio, o prompt segue só com os
//! exemplos do fornecedor.

use rusqlite::{Connection, OptionalExtension, params};
use sbf_indice::{Busca, ItemRef};

use crate::aceite::MOTIVO_DA_DECISAO;
use crate::entrada::{Exemplos, chave_de_exemplos};
use crate::erro::Resultado;
use crate::normalizar::cortar;
use crate::prompt::EXEMPLOS_NO_PROMPT;
use crate::tipos::Alvo;

/// Quantos trechos parecidos se pedem por item (vários trechos podem ser do mesmo item).
const TRECHOS_PEDIDOS: usize = 16;

/// O texto que descreve o item para a busca.
fn consulta_do_alvo(inv: &Connection, alvo: &Alvo) -> Resultado<Option<String>> {
    Ok(match alvo {
        Alvo::Registro(id) => inv
            .query_row(
                "SELECT TRIM(COALESCE(descricao_original, '') || ' ' ||
                             COALESCE(NULLIF(fornecedor_normalizado, ''), supplier_name_raw, ''))
                   FROM registro WHERE id = ?1",
                [id],
                |l| l.get::<_, String>(0),
            )
            .optional()?,
        Alvo::Achado(hash) => inv
            .query_row(
                "SELECT COALESCE((SELECT o.filename FROM origem o WHERE o.content_hash = a.content_hash
                                   ORDER BY o.id LIMIT 1), '') || ' ' ||
                        COALESCE((SELECT substr(t.conteudo, 1, 400) FROM texto t
                                   WHERE t.content_hash = a.content_hash), '')
                   FROM achado a WHERE a.content_hash = ?1",
                [hash],
                |l| l.get::<_, String>(0),
            )
            .optional()?,
    }
    .filter(|t| !t.trim().is_empty()))
}

/// A decisão mais recente do usuário sobre um item parecido, já como linha de exemplo.
fn linha_do_parecido(inv: &Connection, item: &ItemRef) -> Resultado<Option<String>> {
    Ok(match item.tipo.as_str() {
        "registro" => inv
            .query_row(
                "SELECT COALESCE(NULLIF(r.descricao_original, ''), r.supplier_name_raw, ''),
                        json_extract(d.depois, '$.categoria_provavel'), r.tipo
                   FROM decisao d JOIN registro r ON CAST(r.id AS TEXT) = d.alvo_id
                  WHERE d.alvo_tipo = 'registro' AND d.alvo_id = ?1 AND d.campo = 'categoria'
                    AND d.autor = 'usuario' AND d.desfaz IS NULL AND COALESCE(d.motivo, '') <> ?2
                    AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
                  ORDER BY d.em DESC, d.id DESC LIMIT 1",
                params![item.id, MOTIVO_DA_DECISAO],
                |l| {
                    let cat: Option<String> = l.get(1)?;
                    Ok(cat.map(|c| {
                        format!(
                            "\"{}\" ({}) -> categoria {c}",
                            cortar(&l.get::<_, String>(0).unwrap_or_default(), 80),
                            l.get::<_, String>(2).unwrap_or_default()
                        )
                    }))
                },
            )
            .optional()?
            .flatten(),
        "achado" => inv
            .query_row(
                "SELECT COALESCE((SELECT o.filename FROM origem o WHERE o.content_hash = d.alvo_id
                                   ORDER BY o.id LIMIT 1), ''),
                        json_extract(d.depois, '$.document_type')
                   FROM decisao d
                  WHERE d.alvo_tipo = 'achado' AND d.alvo_id = ?1 AND d.campo = 'document_type'
                    AND d.autor = 'usuario' AND d.desfaz IS NULL AND COALESCE(d.motivo, '') <> ?2
                    AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
                  ORDER BY d.em DESC, d.id DESC LIMIT 1",
                params![item.id, MOTIVO_DA_DECISAO],
                |l| {
                    let tipo: Option<String> = l.get(1)?;
                    Ok(tipo.map(|t| {
                        format!(
                            "arquivo \"{}\" -> tipo {t}",
                            cortar(&l.get::<_, String>(0).unwrap_or_default(), 80)
                        )
                    }))
                },
            )
            .optional()?
            .flatten(),
        _ => None,
    })
}

/// Completa `mapa` (os exemplos do mesmo fornecedor) com decisões do usuário sobre itens de
/// texto parecido, sem passar de [`EXEMPLOS_NO_PROMPT`] por item nem repetir linha.
/// Devolve quantas linhas entraram.
///
/// # Errors
/// Falha do SQLite. Falha da busca não é erro: o item fica só com o que já tinha.
pub fn completar_exemplos(
    inv: &Connection,
    alvos: &[Alvo],
    busca: &dyn Busca,
    mapa: &mut Exemplos,
) -> Resultado<usize> {
    let mut entraram = 0;
    for alvo in alvos {
        let chave = chave_de_exemplos(alvo);
        let ja = mapa.get(&chave).map_or(0, Vec::len);
        if ja >= EXEMPLOS_NO_PROMPT {
            continue;
        }
        let Some(consulta) = consulta_do_alvo(inv, alvo)? else {
            continue;
        };
        let eu = ItemRef::novo(alvo.tipo(), alvo.id());
        let Ok(parecidos) = busca.parecidos(&consulta, TRECHOS_PEDIDOS, Some(&eu)) else {
            continue;
        };
        let linhas = mapa.entry(chave).or_default();
        let mut vistos: Vec<ItemRef> = Vec::new();
        for p in parecidos {
            if linhas.len() >= EXEMPLOS_NO_PROMPT {
                break;
            }
            if p.item == eu || vistos.contains(&p.item) {
                continue;
            }
            vistos.push(p.item.clone());
            if let Some(linha) = linha_do_parecido(inv, &p.item)?
                && !linhas.contains(&linha)
            {
                linhas.push(linha);
                entraram += 1;
            }
        }
    }
    mapa.retain(|_, v| !v.is_empty());
    Ok(entraram)
}
