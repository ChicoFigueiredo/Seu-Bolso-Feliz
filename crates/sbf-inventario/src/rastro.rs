//! O rastro: a cadeia de proveniência de um item (spec 06, "Rastro"; `CONTEXT.md`).
//!
//! - **Achado:** acima as origens, cada uma com a coleta (varredura: responsável e quando)
//!   e a conexão (fonte); abaixo os registros extraídos, o lote e as decisões.
//! - **Registro:** acima o documento, as origens e a conexão; abaixo as decisões, o lote
//!   e a movimentação em que virou, quando `movimentacao_id` existe.
//! - **Fatura:** acima o documento de onde ela e as transações saíram (quando vieram de um
//!   PDF) e a conexão (a Pluggy, ou o disco do PDF); abaixo os registros: a fatura e as
//!   transações.
//! - **Conta:** acima as conexões de onde vieram os registros dela.
//! - **Ciclo:** sem rastro, de propósito: o ciclo é um período, não um item que veio de algum
//!   lugar. [`motivo_sem_rastro`] dá a frase que a janela mostra no lugar da cadeia.
//!
//! Cada [`Elo`] diz quem (`decidido_por`, `classificado_por`, `extraido_por` ou o
//! `responsavel` da coleta) e quando. A cadeia vai até a origem para trás; a movimentação
//! é da fatia 2, então o elo dela aponta para uma página que ainda chega.
//!
//! Dono: agente de leitura (INV-20, parte do inventário).

use rusqlite::types::Value;

use crate::consulta::{Elo, Quem, Rastro, ResultadoDeConsulta};
use crate::referencia::{Ref, Tipo};
use crate::relacoes::{
    self, alvo_da_decisao, alvo_da_pendencia, rotulo_da_decisao, rotulo_ou_chave,
};
use crate::vistas::{Contexto, consultar, rotulo_da_coleta, rotulo_do_achado, texto};

/// Quantos elos de "muitos" entram no rastro (origens de uma coleta, itens de um lote…).
const LIMITE_DE_ELOS: usize = 50;

fn elo(
    alvo: Ref,
    rotulo: impl Into<String>,
    papel: &str,
    quem: Option<Quem>,
    quando: Option<String>,
) -> Elo {
    Elo {
        alvo,
        rotulo: rotulo.into(),
        papel: papel.to_owned(),
        quem,
        quando: quando.filter(|q| !q.is_empty()),
    }
}

fn nao_repetir(elos: &mut Vec<Elo>, novo: Elo) {
    if !elos
        .iter()
        .any(|e| e.alvo == novo.alvo && e.papel == novo.papel)
    {
        elos.push(novo);
    }
}

/// As origens de um conteúdo, com a coleta e a conexão de cada uma.
fn elos_das_origens(ctx: &Contexto<'_>, hash: &str) -> ResultadoDeConsulta<Vec<Elo>> {
    let linhas = consultar(
        ctx.conn,
        "SELECT o.id, o.filename, o.fonte_id, f.descricao, o.varredura_id, o.visto_em, v.etapa,
                v.responsavel, v.iniciada_em
         FROM origem o
           JOIN varredura v ON v.id = o.varredura_id
           JOIN fonte f ON f.id = o.fonte_id
         WHERE o.content_hash = ? ORDER BY o.id LIMIT 200",
        &[texto(hash)],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, i64>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, String>(6)?,
                l.get::<_, String>(7)?,
                l.get::<_, String>(8)?,
            ))
        },
    )?;
    let mut elos = Vec::new();
    for (origem, nome, fonte, descricao, varredura, visto, etapa, responsavel, iniciada) in linhas {
        let quem = Some(Quem::Automatico(responsavel));
        nao_repetir(
            &mut elos,
            elo(
                Ref::nova(Tipo::Origem, origem.to_string()),
                nome,
                "achado em",
                quem.clone(),
                Some(visto),
            ),
        );
        nao_repetir(
            &mut elos,
            elo(
                Ref::nova(Tipo::Coleta, varredura.to_string()),
                rotulo_da_coleta(&etapa, &fonte, varredura),
                "coletado na",
                quem,
                Some(iniciada),
            ),
        );
        nao_repetir(
            &mut elos,
            elo(
                Ref::nova(Tipo::Conexao, fonte.as_str()),
                descricao,
                "vindo da conexão",
                None,
                None,
            ),
        );
    }
    Ok(elos)
}

/// As decisões sobre um alvo, da mais antiga à mais nova.
fn elos_das_decisoes(
    ctx: &Contexto<'_>,
    alvo_tipo: &str,
    alvo_id: &str,
) -> ResultadoDeConsulta<Vec<Elo>> {
    let linhas = consultar(
        ctx.conn,
        "SELECT id, campo, antes, depois, autor, em FROM decisao
         WHERE alvo_tipo = ? AND alvo_id = ? ORDER BY em, id LIMIT 200",
        &[texto(alvo_tipo), texto(alvo_id)],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, Option<String>>(2)?,
                l.get::<_, Option<String>>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, String>(5)?,
            ))
        },
    )?;
    Ok(linhas
        .into_iter()
        .map(|(id, campo, antes, depois, autor, em)| {
            elo(
                Ref::nova(Tipo::Decisao, id.to_string()),
                rotulo_da_decisao(&campo, &antes, &depois),
                "decidido por",
                Some(Quem::de_texto(&autor)),
                Some(em),
            )
        })
        .collect())
}

/// Os lotes que têm o item.
fn elos_dos_lotes(
    ctx: &Contexto<'_>,
    alvo_tipo: &str,
    alvo_id: &str,
) -> ResultadoDeConsulta<Vec<Elo>> {
    let linhas = consultar(
        ctx.conn,
        "SELECT l.id, l.nome, l.criado_em, l.aprovado_em FROM lote l
           JOIN lote_item i ON i.lote_id = l.id
         WHERE i.alvo_tipo = ? AND i.alvo_id = ? ORDER BY l.id",
        &[texto(alvo_tipo), texto(alvo_id)],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, Option<String>>(3)?,
            ))
        },
    )?;
    Ok(linhas
        .into_iter()
        .map(|(id, nome, criado, aprovado)| {
            let (papel, quando) = match aprovado {
                Some(a) => ("aprovado no lote", a),
                None => ("adicionado ao lote", criado),
            };
            elo(
                Ref::nova(Tipo::Lote, id.to_string()),
                nome,
                papel,
                Some(Quem::Usuario),
                Some(quando),
            )
        })
        .collect())
}

fn rastro_do_achado(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let hash = alvo.chave.as_str();
    let acima = elos_das_origens(ctx, hash)?;

    let mut abaixo = Vec::new();
    // Registros que saíram dele.
    let registros = consultar(
        ctx.conn,
        "SELECT id, descricao_original, extraido_por, criado_em FROM registro
         WHERE content_hash = ? ORDER BY id LIMIT 200",
        &[texto(hash)],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, Option<String>>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
            ))
        },
    )?;
    for (id, descricao, extraido_por, criado) in registros {
        abaixo.push(elo(
            Ref::nova(Tipo::Registro, id.to_string()),
            crate::vistas::rotulo_do_registro(id, &descricao),
            "extraído para",
            Some(Quem::de_texto(&extraido_por)),
            Some(criado),
        ));
    }
    abaixo.extend(elos_dos_lotes(ctx, "achado", hash)?);
    let decisoes = elos_das_decisoes(ctx, "achado", hash)?;
    if decisoes.is_empty() {
        // Sem decisão no log: o estado vem de `classificado_por` (as regras de antes do log).
        let classificado = consultar(
            ctx.conn,
            "SELECT estado, coalesce(decidido_por, classificado_por) FROM achado WHERE content_hash = ?",
            &[texto(hash)],
            |l| Ok((l.get::<_, String>(0)?, l.get::<_, Option<String>>(1)?)),
        )?;
        if let Some((estado, Some(por))) = classificado.into_iter().next()
            && estado != "novo"
        {
            abaixo.push(elo(
                alvo.clone(),
                format!(
                    "Estado: {}",
                    crate::vistas::rotulo_do_estado_do_achado(&estado)
                ),
                "classificado como",
                Some(Quem::de_texto(&por)),
                None,
            ));
        }
    }
    abaixo.extend(decisoes);
    Ok(Rastro { acima, abaixo })
}

fn rastro_do_registro(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(id) = alvo.chave.parse::<i64>().ok() else {
        return Ok(Rastro::default());
    };
    let linhas = consultar(
        ctx.conn,
        "SELECT content_hash, fonte_id, extraido_por, criado_em, movimentacao_id FROM registro WHERE id = ?",
        &[Value::Integer(id)],
        |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
                l.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    let Some((hash, fonte, extraido_por, criado, movimentacao)) = linhas.into_iter().next() else {
        return Ok(Rastro::default());
    };
    let quem = Some(Quem::de_texto(&extraido_por));
    let mut acima = Vec::new();
    match hash {
        Some(h) => {
            let nome = consultar(
                ctx.conn,
                "SELECT filename FROM origem WHERE content_hash = ? ORDER BY id LIMIT 1",
                &[texto(h.as_str())],
                |l| l.get::<_, Option<String>>(0),
            )?
            .into_iter()
            .next()
            .flatten();
            acima.push(elo(
                Ref::nova(Tipo::Documento, h.as_str()),
                rotulo_do_achado(&nome, &h),
                "extraído de",
                quem,
                Some(criado),
            ));
            acima.extend(elos_das_origens(ctx, &h)?);
        }
        None => {
            let descricao = rotulo_ou_chave(ctx, &Ref::nova(Tipo::Conexao, fonte.as_str()))?;
            acima.push(elo(
                Ref::nova(Tipo::Conexao, fonte.as_str()),
                descricao,
                "vindo da conexão",
                quem,
                Some(criado),
            ));
        }
    }

    let mut abaixo = elos_das_decisoes(ctx, "registro", &alvo.chave)?;
    abaixo.extend(elos_dos_lotes(ctx, "registro", &alvo.chave)?);
    if let Some(mov) = movimentacao.filter(|m| !m.is_empty()) {
        let promovido_em = consultar(
            ctx.conn,
            "SELECT l.promovido_em FROM lote l JOIN lote_item i ON i.lote_id = l.id
             WHERE i.alvo_tipo = 'registro' AND i.alvo_id = ? AND l.promovido_em IS NOT NULL
             ORDER BY l.id LIMIT 1",
            &[texto(alvo.chave.as_str())],
            |l| l.get::<_, String>(0),
        )?
        .into_iter()
        .next();
        abaixo.push(elo(
            Ref::nova(Tipo::Movimentacao, mov.as_str()),
            format!("Movimentação {mov}"),
            "promovido para",
            Some(Quem::Usuario),
            promovido_em,
        ));
    }
    Ok(Rastro { acima, abaixo })
}

fn rastro_da_origem(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(id) = alvo.chave.parse::<i64>().ok() else {
        return Ok(Rastro::default());
    };
    let linhas = consultar(
        ctx.conn,
        "SELECT o.content_hash, o.fonte_id, f.descricao, o.varredura_id, v.etapa, v.responsavel, v.iniciada_em
         FROM origem o
           JOIN varredura v ON v.id = o.varredura_id
           JOIN fonte f ON f.id = o.fonte_id
         WHERE o.id = ?",
        &[Value::Integer(id)],
        |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, i64>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, String>(6)?,
            ))
        },
    )?;
    let Some((hash, fonte, descricao, varredura, etapa, responsavel, iniciada)) =
        linhas.into_iter().next()
    else {
        return Ok(Rastro::default());
    };
    let acima = vec![
        elo(
            Ref::nova(Tipo::Coleta, varredura.to_string()),
            rotulo_da_coleta(&etapa, &fonte, varredura),
            "coletado na",
            Some(Quem::Automatico(responsavel)),
            Some(iniciada),
        ),
        elo(
            Ref::nova(Tipo::Conexao, fonte.as_str()),
            descricao,
            "vindo da conexão",
            None,
            None,
        ),
    ];
    let mut abaixo = Vec::new();
    if let Some(h) = hash {
        let achado = rastro_do_achado(ctx, &Ref::nova(Tipo::Achado, h.as_str()))?;
        let nome = rotulo_ou_chave(ctx, &Ref::nova(Tipo::Achado, h.as_str()))?;
        abaixo.push(elo(
            Ref::nova(Tipo::Achado, h.as_str()),
            nome,
            "conteúdo",
            None,
            None,
        ));
        abaixo.extend(achado.abaixo);
    }
    Ok(Rastro { acima, abaixo })
}

fn rastro_da_coleta(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(id) = alvo.chave.parse::<i64>().ok() else {
        return Ok(Rastro::default());
    };
    let linhas = consultar(
        ctx.conn,
        "SELECT v.fonte_id, f.descricao FROM varredura v JOIN fonte f ON f.id = v.fonte_id WHERE v.id = ?",
        &[Value::Integer(id)],
        |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
    )?;
    let Some((fonte, descricao)) = linhas.into_iter().next() else {
        return Ok(Rastro::default());
    };
    let acima = vec![elo(
        Ref::nova(Tipo::Conexao, fonte.as_str()),
        descricao,
        "vindo da conexão",
        None,
        None,
    )];
    let origens = consultar(
        ctx.conn,
        &format!(
            "SELECT id, filename, visto_em FROM origem WHERE varredura_id = ? ORDER BY id LIMIT {LIMITE_DE_ELOS}"
        ),
        &[Value::Integer(id)],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
            ))
        },
    )?;
    let abaixo = origens
        .into_iter()
        .map(|(o, nome, visto)| {
            elo(
                Ref::nova(Tipo::Origem, o.to_string()),
                nome,
                "viu",
                None,
                Some(visto),
            )
        })
        .collect();
    Ok(Rastro { acima, abaixo })
}

fn rastro_da_conexao(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let coletas = consultar(
        ctx.conn,
        &format!(
            "SELECT id, etapa, responsavel, iniciada_em FROM varredura WHERE fonte_id = ?
             ORDER BY id DESC LIMIT {LIMITE_DE_ELOS}"
        ),
        &[texto(alvo.chave.as_str())],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
            ))
        },
    )?;
    Ok(Rastro {
        acima: Vec::new(),
        abaixo: coletas
            .into_iter()
            .map(|(id, etapa, responsavel, iniciada)| {
                elo(
                    Ref::nova(Tipo::Coleta, id.to_string()),
                    rotulo_da_coleta(&etapa, &alvo.chave, id),
                    "coletada em",
                    Some(Quem::Automatico(responsavel)),
                    Some(iniciada),
                )
            })
            .collect(),
    })
}

fn rastro_da_decisao(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(id) = alvo.chave.parse::<i64>().ok() else {
        return Ok(Rastro::default());
    };
    let mut acima = Vec::new();
    if let Some(a) = alvo_da_decisao(ctx, id)? {
        let rotulo = rotulo_ou_chave(ctx, &a)?;
        let autor = consultar(
            ctx.conn,
            "SELECT autor, em FROM decisao WHERE id = ?",
            &[Value::Integer(id)],
            |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
        )?;
        let (quem, quando) = autor
            .into_iter()
            .next()
            .map_or((None, None), |(a, e)| (Some(Quem::de_texto(&a)), Some(e)));
        acima.push(elo(a, rotulo, "decisão sobre", quem, quando));
    }
    let relacionadas = consultar(
        ctx.conn,
        "SELECT d.id, d.campo, d.antes, d.depois, d.autor, d.em, coalesce(d.id = (SELECT desfaz FROM decisao WHERE id = ?1), 0)
         FROM decisao d
         WHERE d.id = (SELECT desfaz FROM decisao WHERE id = ?1) OR d.desfaz = ?1
         ORDER BY d.id",
        &[Value::Integer(id)],
        |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, Option<String>>(2)?,
                l.get::<_, Option<String>>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, String>(5)?,
                l.get::<_, bool>(6)?,
            ))
        },
    )?;
    let mut abaixo = Vec::new();
    for (outra, campo, antes, depois, autor, em, e_a_desfeita) in relacionadas {
        let e = elo(
            Ref::nova(Tipo::Decisao, outra.to_string()),
            rotulo_da_decisao(&campo, &antes, &depois),
            if e_a_desfeita {
                "desfeita por esta"
            } else {
                "desfeita por"
            },
            Some(Quem::de_texto(&autor)),
            Some(em),
        );
        if e_a_desfeita {
            acima.push(e);
        } else {
            abaixo.push(e);
        }
    }
    Ok(Rastro { acima, abaixo })
}

fn rastro_do_lote(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(id) = alvo.chave.parse::<i64>().ok() else {
        return Ok(Rastro::default());
    };
    let itens = consultar(
        ctx.conn,
        &format!(
            "SELECT alvo_tipo, alvo_id FROM lote_item WHERE lote_id = ? ORDER BY alvo_tipo, alvo_id LIMIT {LIMITE_DE_ELOS}"
        ),
        &[Value::Integer(id)],
        |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
    )?;
    let mut acima = Vec::new();
    for (tipo, chave) in itens {
        if let Some(t) = relacoes::tipo_do_alvo_da_decisao(&tipo) {
            let r = Ref::nova(t, chave.as_str());
            let rotulo = rotulo_ou_chave(ctx, &r)?;
            acima.push(elo(r, rotulo, "contém", Some(Quem::Usuario), None));
        }
    }
    Ok(Rastro {
        acima,
        abaixo: elos_das_decisoes(ctx, "lote", &alvo.chave)?,
    })
}

fn rastro_da_pendencia(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(id) = alvo.chave.parse::<i64>().ok() else {
        return Ok(Rastro::default());
    };
    let mut acima = Vec::new();
    if let Some(a) = alvo_da_pendencia(ctx, id)? {
        let rotulo = rotulo_ou_chave(ctx, &a)?;
        let criada = consultar(
            ctx.conn,
            "SELECT criada_em, etapa FROM pendencia WHERE id = ?",
            &[Value::Integer(id)],
            |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
        )?;
        let (quando, etapa) = criada
            .into_iter()
            .next()
            .map_or((None, String::new()), |(c, e)| (Some(c), e));
        acima.push(elo(
            a,
            rotulo,
            "trava",
            Some(Quem::Automatico(etapa)),
            quando,
        ));
    }
    Ok(Rastro {
        acima,
        abaixo: elos_das_decisoes(ctx, "pendencia", &alvo.chave)?,
    })
}

fn rastro_da_regra(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(id) = alvo.chave.parse::<i64>().ok() else {
        return Ok(Rastro::default());
    };
    let regra = consultar(
        ctx.conn,
        "SELECT fornecedor, ultimo_sucesso_em FROM regra_senha WHERE id = ?",
        &[Value::Integer(id)],
        |l| Ok((l.get::<_, String>(0)?, l.get::<_, Option<String>>(1)?)),
    )?;
    let Some((fornecedor, ultimo)) = regra.into_iter().next() else {
        return Ok(Rastro::default());
    };
    let acima = vec![elo(
        Ref::nova(Tipo::Fornecedor, fornecedor.as_str()),
        fornecedor.clone(),
        "regra do fornecedor",
        None,
        ultimo,
    )];
    let tentativas = relacoes::tentativas(
        ctx,
        &crate::vistas::Selecao::nova()
            .onde(
                "EXISTS (SELECT 1 FROM regra_senha s WHERE s.id = ? AND s.fornecedor = a.supplier_name_raw AND (s.document_type = '*' OR s.document_type = a.document_type))",
                vec![Value::Integer(id)],
            )
            .com_limite(LIMITE_DE_ELOS),
    )?;
    let abaixo = tentativas
        .into_iter()
        .map(|t| {
            elo(
                Ref::nova(Tipo::Achado, t.hash.as_str()),
                rotulo_do_achado(&t.nome, &t.hash),
                if t.sucesso { "abriu" } else { "não abriu" },
                None,
                Some(t.em),
            )
        })
        .collect();
    Ok(Rastro { acima, abaixo })
}

fn rastro_do_fornecedor(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    Ok(Rastro {
        acima: Vec::new(),
        abaixo: elos_das_decisoes(ctx, "fornecedor", &alvo.chave)?,
    })
}

/// Um registro de uma fatura ou conta, com a origem: o documento e a conexão.
struct RegistroLigado {
    id: i64,
    descricao: Option<String>,
    hash: Option<String>,
    fonte: String,
    extraido_por: String,
    criado: String,
}

/// Os registros de uma fatura ou conta (a mesma seleção das abas da página).
fn registros_ligados(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<RegistroLigado>> {
    let Some(sel) = relacoes::selecao_de_registros(ctx, alvo)? else {
        return Ok(Vec::new());
    };
    consultar(
        ctx.conn,
        &format!(
            "SELECT r.id, r.descricao_original, r.content_hash, r.fonte_id, r.extraido_por,
                    r.criado_em
             FROM registro r WHERE 1 = 1{}
             ORDER BY r.tipo <> 'fatura_cartao', r.data, r.id",
            sel.sql_e()
        ),
        sel.params(),
        |l| {
            Ok(RegistroLigado {
                id: l.get(0)?,
                descricao: l.get(1)?,
                hash: l.get(2)?,
                fonte: l.get(3)?,
                extraido_por: l.get(4)?,
                criado: l.get(5)?,
            })
        },
    )
}

/// As conexões (sem repetir, na ordem em que aparecem) de onde vieram os registros.
fn elos_das_conexoes<'a>(
    ctx: &Contexto<'_>,
    fontes: impl Iterator<Item = &'a str>,
) -> ResultadoDeConsulta<Vec<Elo>> {
    let mut elos = Vec::new();
    for fonte in fontes {
        let conexao = Ref::nova(Tipo::Conexao, fonte);
        if elos.iter().any(|e: &Elo| e.alvo == conexao) {
            continue;
        }
        let rotulo = rotulo_ou_chave(ctx, &conexao)?;
        elos.push(elo(conexao, rotulo, "vindo da conexão", None, None));
    }
    Ok(elos)
}

fn rastro_da_fatura(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let registros = registros_ligados(ctx, alvo)?;
    let mut acima = Vec::new();
    for r in &registros {
        let Some(hash) = r.hash.as_deref().filter(|h| !h.is_empty()) else {
            continue;
        };
        let documento = Ref::nova(Tipo::Documento, hash);
        if acima.iter().any(|e: &Elo| e.alvo == documento) {
            continue;
        }
        let nome = consultar(
            ctx.conn,
            "SELECT filename FROM origem WHERE content_hash = ? ORDER BY id LIMIT 1",
            &[texto(hash)],
            |l| l.get::<_, Option<String>>(0),
        )?
        .into_iter()
        .next()
        .flatten();
        acima.push(elo(
            documento,
            rotulo_do_achado(&nome, hash),
            "extraída do documento",
            Some(Quem::de_texto(&r.extraido_por)),
            Some(r.criado.clone()),
        ));
    }
    acima.extend(elos_das_conexoes(
        ctx,
        registros.iter().map(|r| r.fonte.as_str()),
    )?);
    let abaixo = registros
        .iter()
        .take(LIMITE_DE_ELOS)
        .map(|r| {
            elo(
                Ref::nova(Tipo::Registro, r.id.to_string()),
                crate::vistas::rotulo_do_registro(r.id, &r.descricao),
                "registro da fatura",
                Some(Quem::de_texto(&r.extraido_por)),
                Some(r.criado.clone()),
            )
        })
        .collect();
    Ok(Rastro { acima, abaixo })
}

fn rastro_da_conta(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    let Some(sel) = relacoes::selecao_de_registros(ctx, alvo)? else {
        return Ok(Rastro::default());
    };
    // A conta pode ter milhares de registros: só as conexões distintas.
    let fontes = consultar(
        ctx.conn,
        &format!(
            "SELECT r.fonte_id, min(r.id) AS primeiro FROM registro r WHERE 1 = 1{}
             GROUP BY r.fonte_id ORDER BY primeiro",
            sel.sql_e()
        ),
        sel.params(),
        |l| l.get::<_, String>(0),
    )?;
    Ok(Rastro {
        acima: elos_das_conexoes(ctx, fontes.iter().map(String::as_str))?,
        abaixo: Vec::new(),
    })
}

/// A frase que a aba Rastro mostra no lugar da cadeia quando o tipo não tem proveniência
/// por natureza (hoje, só o ciclo). `None` para os outros tipos.
#[must_use]
pub fn motivo_sem_rastro(tipo: Tipo) -> Option<&'static str> {
    match tipo {
        Tipo::Ciclo => Some("O ciclo é um período: não tem proveniência."),
        _ => None,
    }
}

/// O rastro de um item da fatia 1. Item que não existe devolve um rastro vazio; o ciclo
/// também (ver [`motivo_sem_rastro`]); tipo de fatia futura devolve o rastro vazio também
/// (a janela mostra o selo da fatia).
pub fn rastro_de(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
    match alvo.tipo {
        Tipo::Achado | Tipo::Documento => rastro_do_achado(ctx, alvo),
        Tipo::Registro => rastro_do_registro(ctx, alvo),
        Tipo::Origem => rastro_da_origem(ctx, alvo),
        Tipo::Coleta => rastro_da_coleta(ctx, alvo),
        Tipo::Conexao => rastro_da_conexao(ctx, alvo),
        Tipo::Decisao => rastro_da_decisao(ctx, alvo),
        Tipo::Lote => rastro_do_lote(ctx, alvo),
        Tipo::Pendencia => rastro_da_pendencia(ctx, alvo),
        Tipo::RegraDeSenha => rastro_da_regra(ctx, alvo),
        Tipo::Fornecedor => rastro_do_fornecedor(ctx, alvo),
        Tipo::Fatura => rastro_da_fatura(ctx, alvo),
        Tipo::Conta => rastro_da_conta(ctx, alvo),
        _ => Ok(Rastro::default()),
    }
}
