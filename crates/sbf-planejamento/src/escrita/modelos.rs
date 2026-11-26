//! Escrita do Modelo de compromisso: criar o modelo, versionar o Cálculo, ligar um
//! compromisso a um modelo e gravar os valores dos parâmetros com vigência. Nada é apagado:
//! desfazer deixa a linha marcada (`desfeito`, `desfeita`, `desligada`).

use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

use super::{Feito, Inversa, carregar_com_base, invalida, nao_vazio, subir_versao};
use crate::banco::{de_json, json, novo_id};
use crate::modelo::{self, checksum_do_calculo, nome_de_parametro_valido, validar_valor};
use crate::{
    Autor, Competencia, ErroDoPlanejamento, ParametroDeModelo, Resultado, ValorDeParametro,
    ValorDoParametro,
};

/// O maior script de Cálculo aceito.
const MAX_BYTES_DO_CALCULO: usize = 64 * 1024;
const MAX_PARAMETROS: usize = 20;
const MAX_VALORES_POR_APLICACAO: usize = 200;

/// O estado de um valor de parâmetro antes de uma alteração.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ValorGravado {
    pub valor: ValorDoParametro,
    pub desfeito: bool,
}

/// Como estava um valor antes: `anterior: None` = a linha não existia.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct ValorAnterior {
    pub parametro: String,
    pub desde: Competencia,
    pub anterior: Option<ValorGravado>,
}

/// O que fazer com a ligação ao desfazer.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "tipo", rename_all = "snake_case")]
pub(crate) enum RestauroDaLigacao {
    /// A alteração não mexeu na ligação.
    Intacta,
    /// Não havia ligação: marca como desligada.
    Desligar,
    /// Havia uma (talvez já desligada): volta a ela.
    Voltar {
        modelo_id: String,
        calculo_id: String,
        desligada: bool,
    },
}

fn modelo_vivo(c: &Connection, id: &str) -> Resultado<crate::Modelo> {
    modelo::ler_modelo(c, id)?
        .ok_or_else(|| ErroDoPlanejamento::NaoEncontrado(format!("modelo {id}")))
}

fn validar_script(fonte: &str) -> Resultado<()> {
    nao_vazio(fonte, "o cálculo")?;
    if fonte.len() > MAX_BYTES_DO_CALCULO {
        return invalida("o cálculo passa de 64 KiB");
    }
    Ok(())
}

pub(super) fn criar_modelo(
    c: &Connection,
    nome: &str,
    parametros: &[ParametroDeModelo],
    calculo: &str,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let nome = nome.trim();
    nao_vazio(nome, "o nome do modelo")?;
    if nome.chars().count() > 120 {
        return invalida("o nome do modelo passa de 120 caracteres");
    }
    if modelo::existe_modelo_vivo(c, nome)? {
        return invalida("já existe um modelo com esse nome");
    }
    if parametros.is_empty() || parametros.len() > MAX_PARAMETROS {
        return invalida("o modelo precisa de 1 a 20 parâmetros");
    }
    for (i, p) in parametros.iter().enumerate() {
        if !nome_de_parametro_valido(&p.nome) {
            return invalida(
                "o nome de um parâmetro usa só minúsculas sem acento, dígitos e `_` (ex.: dia_da_semana)",
            );
        }
        if parametros[..i].iter().any(|o| o.nome == p.nome) {
            return invalida("há dois parâmetros com o mesmo nome");
        }
    }
    validar_script(calculo)?;
    let id = novo_id(c, "mod")?;
    let id_do_calculo = novo_id(c, "calc")?;
    c.execute(
        "INSERT INTO modelos (id, nome, parametros, quando, autor, desfeito, fabrica)
         VALUES (?1, ?2, ?3, ?4, ?5, 0, 0)",
        params![id, nome, json(&parametros)?, agora, json(autor)?],
    )?;
    c.execute(
        "INSERT INTO calculos (id, modelo_id, versao, fonte, checksum, quando, autor, desfeita)
         VALUES (?1, ?2, 1, ?3, ?4, ?5, ?6, 0)",
        params![
            id_do_calculo,
            id,
            calculo,
            checksum_do_calculo(calculo),
            agora,
            json(autor)?
        ],
    )?;
    Ok(Feito {
        alvo: id.clone(),
        recorrente: None,
        resumo: format!("Criou o modelo \u{201c}{nome}\u{201d}."),
        versao_nova: 0,
        grava_historico: true,
        inversa: Some(Inversa::ModeloCriado { modelo: id }),
    })
}

pub(super) fn nova_versao(
    c: &Connection,
    modelo_id: &str,
    calculo: &str,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let modelo = modelo_vivo(c, modelo_id)?;
    validar_script(calculo)?;
    let checksum = checksum_do_calculo(calculo);
    if checksum == modelo.checksum {
        return invalida("o cálculo é igual ao da versão mais recente");
    }
    let proxima: i64 = c.query_row(
        "SELECT COALESCE(MAX(versao), 0) + 1 FROM calculos WHERE modelo_id = ?1",
        params![modelo_id],
        |l| l.get(0),
    )?;
    let id = novo_id(c, "calc")?;
    c.execute(
        "INSERT INTO calculos (id, modelo_id, versao, fonte, checksum, quando, autor, desfeita)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
        params![
            id,
            modelo_id,
            proxima,
            calculo,
            checksum,
            agora,
            json(autor)?
        ],
    )?;
    Ok(Feito {
        alvo: modelo_id.to_owned(),
        recorrente: None,
        resumo: format!(
            "Nova versão ({proxima}) do cálculo do modelo \u{201c}{}\u{201d}.",
            modelo.nome
        ),
        versao_nova: 0,
        grava_historico: true,
        inversa: Some(Inversa::CalculoCriado { calculo: id }),
    })
}

/// Grava um valor de parâmetro e devolve como estava antes.
fn gravar_valor(
    c: &Connection,
    recorrente: &str,
    v: &ValorDeParametro,
    autor: &Autor,
    agora: &str,
) -> Resultado<ValorAnterior> {
    let desde = v.desde.to_string();
    let anterior: Option<(String, bool)> = c
        .query_row(
            "SELECT valor, desfeito FROM valores_de_parametro
             WHERE recorrente_id = ?1 AND parametro = ?2 AND desde = ?3",
            params![recorrente, v.parametro, desde],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()?;
    let anterior = anterior
        .map(|(valor, desfeito)| -> Resultado<ValorGravado> {
            Ok(ValorGravado {
                valor: de_json(&valor)?,
                desfeito,
            })
        })
        .transpose()?;
    if anterior.is_some() {
        c.execute(
            "UPDATE valores_de_parametro SET valor = ?4, quando = ?5, autor = ?6, desfeito = 0
             WHERE recorrente_id = ?1 AND parametro = ?2 AND desde = ?3",
            params![
                recorrente,
                v.parametro,
                desde,
                json(&v.valor)?,
                agora,
                json(autor)?
            ],
        )?;
    } else {
        c.execute(
            "INSERT INTO valores_de_parametro
                 (id, recorrente_id, parametro, desde, valor, quando, autor, desfeito)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 0)",
            params![
                novo_id(c, "val")?,
                recorrente,
                v.parametro,
                desde,
                json(&v.valor)?,
                agora,
                json(autor)?
            ],
        )?;
    }
    Ok(ValorAnterior {
        parametro: v.parametro.clone(),
        desde: v.desde,
        anterior,
    })
}

pub(super) fn aplicar_modelo(
    c: &Connection,
    recorrente: &str,
    modelo_id: &str,
    valores: &[ValorDeParametro],
    base: i64,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let rec = carregar_com_base(c, recorrente, base)?;
    let modelo = modelo_vivo(c, modelo_id)?;
    if valores.len() > MAX_VALORES_POR_APLICACAO {
        return invalida("valores demais numa aplicação (o máximo é 200)");
    }
    for v in valores {
        validar_valor(&modelo.parametros, v).map_err(ErroDoPlanejamento::Invalida)?;
    }
    let calculo_id: String = c.query_row(
        "SELECT id FROM calculos WHERE modelo_id = ?1 AND desfeita = 0
         ORDER BY versao DESC LIMIT 1",
        params![modelo_id],
        |l| l.get(0),
    )?;
    let antes: Option<(String, String, bool)> = c
        .query_row(
            "SELECT modelo_id, calculo_id, desligada FROM ligacoes_de_modelo
             WHERE recorrente_id = ?1",
            params![rec.id],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .optional()?;
    c.execute(
        "INSERT INTO ligacoes_de_modelo (recorrente_id, modelo_id, calculo_id, quando, autor,
                                         desligada)
         VALUES (?1, ?2, ?3, ?4, ?5, 0)
         ON CONFLICT (recorrente_id) DO UPDATE
         SET modelo_id = excluded.modelo_id, calculo_id = excluded.calculo_id,
             quando = excluded.quando, autor = excluded.autor, desligada = 0",
        params![rec.id, modelo_id, calculo_id, agora, json(autor)?],
    )?;
    let mut anteriores = Vec::new();
    for v in valores {
        anteriores.push(gravar_valor(c, &rec.id, v, autor, agora)?);
    }
    let versao_nova = subir_versao(c, &rec.id)?;
    let restauro = match antes {
        None => RestauroDaLigacao::Desligar,
        Some((modelo_id, calculo_id, desligada)) => RestauroDaLigacao::Voltar {
            modelo_id,
            calculo_id,
            desligada,
        },
    };
    Ok(Feito {
        alvo: rec.id.clone(),
        recorrente: Some(rec.id.clone()),
        resumo: format!(
            "Ligou ao modelo \u{201c}{}\u{201d} e preencheu {} valor(es) de parâmetro.",
            modelo.nome,
            valores.len()
        ),
        versao_nova,
        grava_historico: true,
        inversa: Some(Inversa::Modelo {
            recorrente: rec.id,
            ligacao: restauro,
            valores: anteriores,
        }),
    })
}

pub(super) fn definir_parametro(
    c: &Connection,
    recorrente: &str,
    valor: &ValorDeParametro,
    base: i64,
    autor: &Autor,
    agora: &str,
) -> Resultado<Feito> {
    let rec = carregar_com_base(c, recorrente, base)?;
    let Some(ligacao) = modelo::ler_ligacoes(c, Some(&rec.id))?.remove(&rec.id) else {
        return invalida("o compromisso não usa um modelo: ligue-o antes (aplicar_modelo)");
    };
    validar_valor(&ligacao.parametros, valor).map_err(ErroDoPlanejamento::Invalida)?;
    let anterior = gravar_valor(c, &rec.id, valor, autor, agora)?;
    let versao_nova = subir_versao(c, &rec.id)?;
    Ok(Feito {
        alvo: rec.id.clone(),
        recorrente: Some(rec.id.clone()),
        resumo: format!(
            "Definiu o parâmetro {} desde {}.",
            valor.parametro, valor.desde
        ),
        versao_nova,
        grava_historico: true,
        inversa: Some(Inversa::Modelo {
            recorrente: rec.id,
            ligacao: RestauroDaLigacao::Intacta,
            valores: vec![anterior],
        }),
    })
}

/// O compromisso está ligado a um modelo (a quantidade de um mês só faz sentido assim).
pub(super) fn exige_modelo(c: &Connection, recorrente: &str) -> Resultado<()> {
    if modelo::ler_ligacoes(c, Some(recorrente))?.contains_key(recorrente) {
        Ok(())
    } else {
        invalida("o compromisso não usa um modelo: não há cálculo para ajustar a quantidade")
    }
}

// ---------------------------------------------------------------------------------------
// Desfazer
// ---------------------------------------------------------------------------------------

pub(super) fn desfazer_modelo_criado(c: &Connection, modelo_id: &str) -> Resultado<Feito> {
    let modelo = modelo_vivo(c, modelo_id)?;
    let em_uso: i64 = c.query_row(
        "SELECT COUNT(*) FROM ligacoes_de_modelo WHERE modelo_id = ?1 AND desligada = 0",
        params![modelo_id],
        |l| l.get(0),
    )?;
    if em_uso > 0 {
        return invalida("há compromissos usando o modelo: desfaça as ligações antes");
    }
    c.execute(
        "UPDATE modelos SET desfeito = 1 WHERE id = ?1",
        params![modelo_id],
    )?;
    Ok(Feito {
        alvo: modelo_id.to_owned(),
        recorrente: None,
        resumo: format!(
            "Desfez a criação do modelo \u{201c}{}\u{201d}.",
            modelo.nome
        ),
        versao_nova: 0,
        grava_historico: true,
        inversa: None,
    })
}

pub(super) fn desfazer_calculo_criado(c: &Connection, calculo_id: &str) -> Resultado<Feito> {
    let linha: Option<(String, u32)> = c
        .query_row(
            "SELECT modelo_id, versao FROM calculos WHERE id = ?1",
            params![calculo_id],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .optional()?;
    let Some((modelo_id, versao)) = linha else {
        return Err(ErroDoPlanejamento::NaoEncontrado(format!(
            "cálculo {calculo_id}"
        )));
    };
    let preso: i64 = c.query_row(
        "SELECT COUNT(*) FROM ligacoes_de_modelo WHERE calculo_id = ?1 AND desligada = 0",
        params![calculo_id],
        |l| l.get(0),
    )?;
    if preso > 0 {
        return invalida("há compromissos presos a esta versão do cálculo");
    }
    c.execute(
        "UPDATE calculos SET desfeita = 1 WHERE id = ?1",
        params![calculo_id],
    )?;
    Ok(Feito {
        alvo: modelo_id,
        recorrente: None,
        resumo: format!("Desfez a versão {versao} do cálculo."),
        versao_nova: 0,
        grava_historico: true,
        inversa: None,
    })
}

pub(super) fn desfazer_ligacao(
    c: &Connection,
    recorrente: &str,
    ligacao: &RestauroDaLigacao,
    valores: &[ValorAnterior],
) -> Resultado<Feito> {
    match ligacao {
        RestauroDaLigacao::Intacta => {}
        RestauroDaLigacao::Desligar => {
            c.execute(
                "UPDATE ligacoes_de_modelo SET desligada = 1 WHERE recorrente_id = ?1",
                params![recorrente],
            )?;
        }
        RestauroDaLigacao::Voltar {
            modelo_id,
            calculo_id,
            desligada,
        } => {
            c.execute(
                "UPDATE ligacoes_de_modelo SET modelo_id = ?2, calculo_id = ?3, desligada = ?4
                 WHERE recorrente_id = ?1",
                params![recorrente, modelo_id, calculo_id, desligada],
            )?;
        }
    }
    for v in valores {
        let (valor, desfeito) = match &v.anterior {
            Some(g) => (json(&g.valor)?, g.desfeito),
            None => (String::new(), true),
        };
        if v.anterior.is_some() {
            c.execute(
                "UPDATE valores_de_parametro SET valor = ?4, desfeito = ?5
                 WHERE recorrente_id = ?1 AND parametro = ?2 AND desde = ?3",
                params![
                    recorrente,
                    v.parametro,
                    v.desde.to_string(),
                    valor,
                    desfeito
                ],
            )?;
        } else {
            c.execute(
                "UPDATE valores_de_parametro SET desfeito = 1
                 WHERE recorrente_id = ?1 AND parametro = ?2 AND desde = ?3",
                params![recorrente, v.parametro, v.desde.to_string()],
            )?;
        }
    }
    let versao_nova = subir_versao(c, recorrente)?;
    Ok(Feito {
        alvo: recorrente.to_owned(),
        recorrente: Some(recorrente.to_owned()),
        resumo: "Desfez a ligação a um modelo ou um valor de parâmetro.".to_owned(),
        versao_nova,
        grava_historico: true,
        inversa: None,
    })
}
