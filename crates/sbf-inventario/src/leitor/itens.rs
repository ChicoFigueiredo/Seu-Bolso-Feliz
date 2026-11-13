//! O cabeçalho de cada página de item da fatia 1: o resumo, com todo número clicável.
//!
//! Achado e documento, origem, registro, decisão, lote, pendência, regra de senha, conexão,
//! coleta, fornecedor, ciclo, conta e fatura. Tipo de fatia futura devolve
//! [`ErroDeConsulta::NaoSuportado`]; item que não existe devolve `None`.

use rusqlite::types::Value;

use crate::consulta::{Campo, Celula, Centavos, ErroDeConsulta, Item, Quem, ResultadoDeConsulta};
use crate::referencia::{Ref, Tipo};
use crate::relacoes::{
    self, alvo_da_decisao, alvo_da_pendencia, contagem_do_cabecalho, recorte, rotulo_ou_chave,
    texto_do_json, valor_do_cabecalho,
};
use crate::visao_geral::ciclo_da_data;
use crate::vistas::{
    self, Contexto, Selecao, c_contagem, c_contagem_solta, c_data_opt, c_estado_achado,
    c_estado_registro, c_link, c_link_opt, c_texto, c_texto_opt, c_valor, c_valor_opt, consultar,
    rotulo_da_coleta, texto,
};

fn campo(rotulo: &str, valor: Celula) -> Campo {
    Campo {
        rotulo: rotulo.to_owned(),
        valor,
    }
}

fn inteiro(n: i64) -> Value {
    Value::Integer(n)
}

fn sim_nao(valor: bool) -> Celula {
    Celula::Texto(if valor { "Sim" } else { "Não" }.to_owned())
}

fn confianca(c: Option<f64>) -> Celula {
    c.map_or(Celula::Vazia, |c| {
        Celula::Texto(format!("{:.0}%", c * 100.0))
    })
}

/// Monta o [`Item`] de `alvo`. `hoje` é `AAAA-MM-DD`, para o ciclo atual.
pub fn montar(ctx: &Contexto<'_>, alvo: &Ref, hoje: &str) -> ResultadoDeConsulta<Option<Item>> {
    if let Some(motivo) = alvo.tipo.motivo_da_fatia() {
        return Err(ErroDeConsulta::NaoSuportado(motivo));
    }
    let Some(rotulo) = relacoes::rotulo_do_item(ctx, alvo)? else {
        return Ok(None);
    };
    let campos = match alvo.tipo {
        Tipo::Achado | Tipo::Documento => achado(ctx, alvo)?,
        Tipo::Origem => origem(ctx, alvo)?,
        Tipo::Registro => registro(ctx, alvo)?,
        Tipo::Decisao => decisao(ctx, alvo)?,
        Tipo::Lote => lote(ctx, alvo)?,
        Tipo::Pendencia => pendencia(ctx, alvo)?,
        Tipo::RegraDeSenha => regra(ctx, alvo)?,
        Tipo::Conexao => conexao(ctx, alvo)?,
        Tipo::Coleta => coleta(ctx, alvo)?,
        Tipo::Fornecedor => fornecedor(ctx, alvo, hoje)?,
        Tipo::Ciclo => ciclo(ctx, alvo)?,
        Tipo::Conta => conta(ctx, alvo)?,
        Tipo::Fatura => fatura(ctx, alvo)?,
        _ => return Ok(None),
    };
    Ok(Some(Item {
        alvo: alvo.clone(),
        rotulo,
        campos,
    }))
}

fn contar(ctx: &Contexto<'_>, sql: &str, p: Vec<Value>) -> ResultadoDeConsulta<i64> {
    vistas::escalar(ctx.conn, sql, &p)
}

// ---------------------------------------------------------------------------

fn achado(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let hash = alvo.chave.as_str();
    #[allow(clippy::type_complexity)]
    let linhas = consultar(
        ctx.conn,
        "SELECT estado, document_type, protegido_senha, data_chave, data_chave_tipo, ciclo_id, valor_centavos,
                supplier_name_raw, confianca, decidido_por, caminho_acervo, camada, extensao, notas
         FROM achado WHERE content_hash = ?",
        &[texto(hash)],
        |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, Option<String>>(1)?,
                l.get::<_, Option<i64>>(2)?,
                l.get::<_, Option<String>>(3)?,
                l.get::<_, Option<String>>(4)?,
                l.get::<_, Option<String>>(5)?,
                l.get::<_, Option<Centavos>>(6)?,
                l.get::<_, Option<String>>(7)?,
                l.get::<_, Option<f64>>(8)?,
                l.get::<_, Option<String>>(9)?,
                l.get::<_, Option<String>>(10)?,
                l.get::<_, String>(11)?,
                l.get::<_, Option<String>>(12)?,
                l.get::<_, Option<String>>(13)?,
            ))
        },
    )?;
    let Some((
        estado,
        tipo,
        protegido,
        data,
        data_tipo,
        ciclo,
        valor,
        fornecedor,
        conf,
        por,
        acervo,
        camada,
        ext,
        notas,
    )) = linhas.into_iter().next()
    else {
        return Ok(Vec::new());
    };
    let origens = contar(
        ctx,
        "SELECT count(*) FROM origem WHERE content_hash = ?",
        vec![texto(hash)],
    )?;
    let registros = contar(
        ctx,
        "SELECT count(*) FROM registro WHERE content_hash = ?",
        vec![texto(hash)],
    )?;
    let decisoes = contar(
        ctx,
        "SELECT count(*) FROM decisao WHERE alvo_tipo = 'achado' AND alvo_id = ?",
        vec![texto(hash)],
    )?;
    let lotes = relacoes::lotes(ctx, &relacoes::selecao_de_lotes(alvo).unwrap_or_default())?;
    let de = Ref::nova(Tipo::Achado, hash);
    Ok(vec![
        campo("Estado", c_estado_achado(&estado, &por)),
        campo("Hash", Celula::Texto(hash.to_owned())),
        campo("Tipo", c_texto_opt(&tipo)),
        campo("Ciclo", c_link_opt(Tipo::Ciclo, &ciclo)),
        campo(
            "Data",
            data.as_ref().map_or(Celula::Vazia, |d| {
                Celula::Texto(match &data_tipo {
                    Some(t) => format!("{d} ({t})"),
                    None => d.clone(),
                })
            }),
        ),
        campo("Valor", c_valor_opt(valor)),
        campo("Fornecedor", c_link_opt(Tipo::Fornecedor, &fornecedor)),
        campo(
            "Origens",
            c_contagem(origens, de.clone(), None, recorte::ORIGENS),
        ),
        campo(
            "Registros que saíram dele",
            c_contagem(registros, de.clone(), None, recorte::REGISTROS),
        ),
        campo(
            "Decisões",
            c_contagem(decisoes, de, None, recorte::DECISOES),
        ),
        campo(
            "Lote",
            if lotes.is_empty() {
                Celula::Vazia
            } else {
                Celula::Links(
                    lotes
                        .iter()
                        .map(|l| (Ref::nova(Tipo::Lote, l.id.to_string()), l.nome.clone()))
                        .collect(),
                )
            },
        ),
        campo(
            "Protegido por senha",
            match protegido {
                Some(1) => Celula::Texto("Sim".to_owned()),
                Some(_) => Celula::Texto("Não".to_owned()),
                None => Celula::Texto("Não verificado".to_owned()),
            },
        ),
        campo("Confiança", confianca(conf)),
        campo("Camada", c_texto(&camada)),
        campo("Extensão", c_texto_opt(&ext)),
        campo("No cofre", c_texto_opt(&acervo)),
        campo("Motivo", c_texto_opt(&notas)),
    ])
}

fn origem(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Ok(id) = alvo.chave.parse::<i64>() else {
        return Ok(Vec::new());
    };
    let linhas = consultar(
        ctx.conn,
        "SELECT o.content_hash, o.fonte_id, o.varredura_id, v.etapa, o.origin_type, o.filename,
                coalesce(o.local_filepath, o.gmail_subject, o.mensagem_ref), o.visto_em, a.estado, a.decidido_por, o.erro
         FROM origem o LEFT JOIN varredura v ON v.id = o.varredura_id
           LEFT JOIN achado a ON a.content_hash = o.content_hash
         WHERE o.id = ?",
        &[inteiro(id)],
        |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, String>(1)?,
                l.get::<_, i64>(2)?,
                l.get::<_, Option<String>>(3)?,
                l.get::<_, String>(4)?,
                l.get::<_, Option<String>>(6)?,
                l.get::<_, String>(7)?,
                l.get::<_, Option<String>>(8)?,
                l.get::<_, Option<String>>(9)?,
                l.get::<_, Option<String>>(10)?,
            ))
        },
    )?;
    let Some((hash, fonte, varredura, etapa, tipo, onde, visto, estado, por, erro)) =
        linhas.into_iter().next()
    else {
        return Ok(Vec::new());
    };
    let outras = match &hash {
        Some(h) => contar(
            ctx,
            "SELECT count(*) FROM origem WHERE content_hash = ? AND id <> ?",
            vec![texto(h.as_str()), inteiro(id)],
        )?,
        None => 0,
    };
    Ok(vec![
        campo("Conexão", c_link(Tipo::Conexao, &fonte, &fonte)),
        campo(
            "Coleta",
            c_link(
                Tipo::Coleta,
                &varredura.to_string(),
                &rotulo_da_coleta(etapa.as_deref().unwrap_or("?"), &fonte, varredura),
            ),
        ),
        campo(
            "Achado",
            hash.as_deref().map_or(Celula::Vazia, |h| {
                c_link(
                    Tipo::Achado,
                    h,
                    &format!("Achado {}", vistas::hash_curto(h)),
                )
            }),
        ),
        campo(
            "Estado do achado",
            estado
                .as_deref()
                .map_or(Celula::Vazia, |e| c_estado_achado(e, &por)),
        ),
        campo("Tipo de origem", c_texto(&tipo)),
        campo("Caminho · assunto", c_texto_opt(&onde)),
        campo("Vista em", Celula::Data(visto)),
        campo(
            "Lugares onde o mesmo conteúdo foi achado",
            match &hash {
                Some(h) => c_contagem(
                    outras + 1,
                    Ref::nova(Tipo::Achado, h.as_str()),
                    None,
                    recorte::ORIGENS,
                ),
                None => Celula::Vazia,
            },
        ),
        campo("Erro", c_texto_opt(&erro)),
    ])
}

fn registro(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Ok(id) = alvo.chave.parse::<i64>() else {
        return Ok(Vec::new());
    };
    let Some(l) = vistas::registros(ctx, &Selecao::nova().onde("r.id = ?", vec![inteiro(id)]))?
        .into_iter()
        .next()
    else {
        return Ok(Vec::new());
    };
    let extras = consultar(
        ctx.conn,
        &format!(
            "SELECT r.extraido_por, r.confianca, r.external_ref, r.tipo, {} FROM registro r WHERE r.id = ?",
            ctx.dialeto.fatura_ref("r.")
        ),
        &[inteiro(id)],
        |x| {
            Ok((
                x.get::<_, String>(0)?,
                x.get::<_, Option<f64>>(1)?,
                x.get::<_, Option<String>>(2)?,
                x.get::<_, String>(3)?,
                x.get::<_, Option<String>>(4)?,
            ))
        },
    )?;
    let (extraido_por, conf, externo, tipo, fatura_ref) = extras
        .into_iter()
        .next()
        .unwrap_or_else(|| (String::new(), None, None, String::new(), None));
    let fatura = fatura_ref.or_else(|| {
        (tipo == "fatura_cartao")
            .then(|| {
                externo
                    .as_deref()
                    .and_then(|e| e.get(12..))
                    .map(str::to_owned)
            })
            .flatten()
    });
    let decisoes = contar(
        ctx,
        "SELECT count(*) FROM decisao WHERE alvo_tipo = 'registro' AND alvo_id = ?",
        vec![texto(alvo.chave.as_str())],
    )?;
    let lotes = relacoes::lotes(ctx, &relacoes::selecao_de_lotes(alvo).unwrap_or_default())?;
    let fornecedor = ctx.sql_fornecedor_do_registro("r.");
    let nome_fornecedor = consultar(
        ctx.conn,
        &format!("SELECT {fornecedor} FROM registro r WHERE r.id = ?"),
        &[inteiro(id)],
        |x| x.get::<_, Option<String>>(0),
    )?
    .into_iter()
    .next()
    .flatten();
    Ok(vec![
        campo("Estado", c_estado_registro(&l.estado, &l.decidido_por)),
        campo("Data", Celula::Data(l.data.clone())),
        campo("Valor", c_valor(l.valor)),
        campo("Tipo", c_texto(&l.tipo)),
        campo("Fornecedor", c_link_opt(Tipo::Fornecedor, &nome_fornecedor)),
        campo("Contraparte", c_texto_opt(&l.contraparte)),
        campo("Categoria", c_texto_opt(&l.categoria)),
        campo("Conta", c_link_opt(Tipo::Conta, &l.conta)),
        campo("Ciclo", c_link_opt(Tipo::Ciclo, &l.ciclo)),
        campo("Fatura", c_link_opt(Tipo::Fatura, &fatura)),
        campo("Conexão", c_link(Tipo::Conexao, &l.fonte, &l.fonte)),
        campo(
            "Documento de origem",
            l.content_hash.as_deref().map_or(Celula::Vazia, |h| {
                c_link(
                    Tipo::Documento,
                    h,
                    &format!("Documento {}", vistas::hash_curto(h)),
                )
            }),
        ),
        campo(
            "Movimentação",
            l.movimentacao_id
                .as_deref()
                .filter(|m| !m.is_empty())
                .map_or_else(
                    || Celula::Texto("Ainda não é movimentação".to_owned()),
                    |m| c_link(Tipo::Movimentacao, m, &format!("Movimentação {m}")),
                ),
        ),
        campo(
            "Decisões",
            c_contagem(decisoes, alvo.clone(), None, recorte::DECISOES),
        ),
        campo(
            "Lote",
            if lotes.is_empty() {
                Celula::Vazia
            } else {
                Celula::Links(
                    lotes
                        .iter()
                        .map(|x| (Ref::nova(Tipo::Lote, x.id.to_string()), x.nome.clone()))
                        .collect(),
                )
            },
        ),
        campo("Extraído por", Celula::Texto(extraido_por)),
        campo("Confiança", confianca(conf)),
        campo("Descrição", c_texto_opt(&l.descricao)),
    ])
}

fn decisao(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Ok(id) = alvo.chave.parse::<i64>() else {
        return Ok(Vec::new());
    };
    let Some(d) = relacoes::decisoes(ctx, &Selecao::nova().onde("d.id = ?", vec![inteiro(id)]))?
        .into_iter()
        .next()
    else {
        return Ok(Vec::new());
    };
    let alvo_da_decisao_ref = alvo_da_decisao(ctx, id)?;
    let alvo_celula = match alvo_da_decisao_ref {
        Some(a) => Celula::Link {
            texto: rotulo_ou_chave(ctx, &a)?,
            alvo: a,
        },
        None => c_texto(&d.alvo_id),
    };
    let situacao = if d.desfeita_por.is_some() {
        "Desfeita"
    } else {
        "Em vigor"
    };
    Ok(vec![
        campo("Alvo", alvo_celula),
        campo("Campo", c_texto(&d.campo)),
        campo("Antes", Celula::Texto(texto_do_json(&d.antes))),
        campo("Depois", Celula::Texto(texto_do_json(&d.depois))),
        campo(
            "Situação",
            Celula::Estado {
                rotulo: situacao.to_owned(),
                quem: Some(Quem::de_texto(&d.autor)),
            },
        ),
        campo("Quando", Celula::Data(d.em.clone())),
        campo("Motivo", c_texto_opt(&d.motivo)),
        campo(
            "Desfaz",
            d.desfaz.map_or(Celula::Vazia, |x| {
                c_link(Tipo::Decisao, &x.to_string(), &format!("Decisão {x}"))
            }),
        ),
        campo(
            "Desfeita por",
            d.desfeita_por.map_or(Celula::Vazia, |x| {
                c_link(Tipo::Decisao, &x.to_string(), &format!("Decisão {x}"))
            }),
        ),
    ])
}

fn lote(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Ok(id) = alvo.chave.parse::<i64>() else {
        return Ok(Vec::new());
    };
    let Some(l) = relacoes::lotes(ctx, &Selecao::nova().onde("l.id = ?", vec![inteiro(id)]))?
        .into_iter()
        .next()
    else {
        return Ok(Vec::new());
    };
    Ok(vec![
        campo(
            "Situação",
            Celula::Estado {
                rotulo: l.situacao().to_owned(),
                quem: Some(Quem::Usuario),
            },
        ),
        campo("Criado em", Celula::Data(l.criado_em.clone())),
        campo("Aprovado em", c_data_opt(&l.aprovado_em)),
        campo("Promovido em", c_data_opt(&l.promovido_em)),
        campo(
            "Itens",
            c_contagem(l.achados + l.registros, alvo.clone(), None, recorte::ITENS),
        ),
        campo(
            "Achados",
            c_contagem(l.achados, alvo.clone(), None, recorte::ACHADOS),
        ),
        campo(
            "Registros",
            c_contagem(l.registros, alvo.clone(), None, recorte::REGISTROS),
        ),
        campo(
            "Valor dos registros",
            valor_do_cabecalho(ctx, alvo, recorte::REGISTROS, None)?,
        ),
    ])
}

fn pendencia(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Ok(id) = alvo.chave.parse::<i64>() else {
        return Ok(Vec::new());
    };
    let Some(p) = relacoes::pendencias(ctx, &Selecao::nova().onde("p.id = ?", vec![inteiro(id)]))?
        .into_iter()
        .next()
    else {
        return Ok(Vec::new());
    };
    let travado = match alvo_da_pendencia(ctx, id)? {
        Some(a) => Celula::Link {
            texto: rotulo_ou_chave(ctx, &a)?,
            alvo: a,
        },
        None => c_texto_opt(&p.referencia),
    };
    let decisoes = contar(
        ctx,
        "SELECT count(*) FROM decisao WHERE alvo_tipo = 'pendencia' AND alvo_id = ?",
        vec![texto(alvo.chave.as_str())],
    )?;
    Ok(vec![
        campo(
            "Situação",
            Celula::Estado {
                rotulo: if p.resolvida_em.is_some() {
                    "Respondida"
                } else {
                    "Aberta"
                }
                .to_owned(),
                quem: p.resolvida_em.as_ref().map(|_| Quem::Usuario),
            },
        ),
        campo("Pergunta", c_texto(&p.pergunta)),
        campo("Item travado", travado),
        campo("Etapa", c_texto(&p.etapa)),
        campo("Tipo", c_texto(&p.tipo)),
        campo("Criada em", Celula::Data(p.criada_em.clone())),
        campo("Respondida em", c_data_opt(&p.resolvida_em)),
        campo("Resposta", c_texto_opt(&p.resposta)),
        campo(
            "Decisões",
            c_contagem(decisoes, alvo.clone(), None, recorte::DECISOES),
        ),
    ])
}

fn regra(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Ok(id) = alvo.chave.parse::<i64>() else {
        return Ok(Vec::new());
    };
    let Some(r) =
        relacoes::regras_de_senha(ctx, &Selecao::nova().onde("s.id = ?", vec![inteiro(id)]))?
            .into_iter()
            .next()
    else {
        return Ok(Vec::new());
    };
    let tentativas = relacoes::tentativas(
        ctx,
        &Selecao::nova().onde(
            "EXISTS (SELECT 1 FROM regra_senha s WHERE s.id = ? AND s.fornecedor = a.supplier_name_raw AND (s.document_type = '*' OR s.document_type = a.document_type))",
            vec![inteiro(id)],
        ),
    )?;
    let total = r.sucessos + r.falhas;
    Ok(vec![
        campo(
            "Fornecedor",
            c_link(Tipo::Fornecedor, &r.fornecedor, &r.fornecedor),
        ),
        campo("Tipo de documento", c_texto(&r.tipo_de_documento)),
        campo("Estratégia", c_texto(&r.estrategia)),
        campo("Prioridade", c_contagem_solta(r.prioridade)),
        campo(
            "Situação",
            Celula::Estado {
                rotulo: if r.ativa { "Ativa" } else { "Desligada" }.to_owned(),
                quem: None,
            },
        ),
        campo("Sucessos", c_contagem_solta(r.sucessos)),
        campo("Falhas", c_contagem_solta(r.falhas)),
        campo(
            "Taxa de acerto",
            if total == 0 {
                Celula::Vazia
            } else {
                Celula::Texto(format!("{}%", 100 * r.sucessos / total))
            },
        ),
        campo("Último sucesso", c_data_opt(&r.ultimo_sucesso_em)),
        campo("Última falha", c_data_opt(&r.ultima_falha_em)),
        campo(
            "Tentativas",
            c_contagem(
                i64::try_from(tentativas.len()).unwrap_or(0),
                alvo.clone(),
                None,
                recorte::TENTATIVAS,
            ),
        ),
    ])
}

fn conexao(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Some(c) = relacoes::conexoes(
        ctx,
        &Selecao::nova().onde("f.id = ?", vec![texto(alvo.chave.as_str())]),
    )?
    .into_iter()
    .next() else {
        return Ok(Vec::new());
    };
    let raiz = consultar(
        ctx.conn,
        "SELECT raiz, criada_em FROM fonte WHERE id = ?",
        &[texto(alvo.chave.as_str())],
        |l| Ok((l.get::<_, Option<String>>(0)?, l.get::<_, String>(1)?)),
    )?
    .into_iter()
    .next()
    .unwrap_or((None, String::new()));
    let ultima = relacoes::selecao_de_coletas(alvo)
        .map(|s| vistas::varreduras(ctx, &s.com_limite(1)))
        .transpose()?
        .and_then(|v| v.into_iter().next());
    Ok(vec![
        campo("Descrição", c_texto(&c.descricao)),
        campo("Tipo", c_texto(&c.tipo)),
        campo("Escopo", c_texto(&c.escopo)),
        campo("Motivo de estar fora", c_texto_opt(&c.motivo)),
        campo(
            "Situação",
            Celula::Texto(if c.escopo == "fora" {
                "fora".to_owned()
            } else {
                c.situacao.clone().unwrap_or_else(|| "ausente".to_owned())
            }),
        ),
        campo("Raiz", c_texto_opt(&raiz.0)),
        campo(
            "Coletas",
            c_contagem(c.coletas, alvo.clone(), None, recorte::COLETAS),
        ),
        campo(
            "Origens",
            c_contagem(c.origens, alvo.clone(), None, recorte::ORIGENS),
        ),
        campo(
            "Registros",
            c_contagem(c.registros, alvo.clone(), None, recorte::REGISTROS),
        ),
        campo(
            "Última coleta",
            ultima.map_or(Celula::Vazia, |u| {
                c_link(
                    Tipo::Coleta,
                    &u.id.to_string(),
                    &rotulo_da_coleta(&u.etapa, &u.fonte_id, u.id),
                )
            }),
        ),
        campo("Criada em", c_data_opt(&Some(raiz.1))),
    ])
}

fn coleta(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Ok(id) = alvo.chave.parse::<i64>() else {
        return Ok(Vec::new());
    };
    let Some(v) = vistas::varreduras(ctx, &Selecao::nova().onde("v.id = ?", vec![inteiro(id)]))?
        .into_iter()
        .next()
    else {
        return Ok(Vec::new());
    };
    let origens = contar(
        ctx,
        "SELECT count(*) FROM origem WHERE varredura_id = ?",
        vec![inteiro(id)],
    )?;
    Ok(vec![
        campo("Conexão", c_link(Tipo::Conexao, &v.fonte_id, &v.fonte_id)),
        campo("Etapa", c_texto(&v.etapa)),
        campo("Situação", c_texto(&v.status)),
        campo("Responsável", Celula::Texto(v.responsavel.clone())),
        campo("Iniciada em", Celula::Data(v.iniciada_em.clone())),
        campo("Concluída em", c_data_opt(&v.concluida_em)),
        campo("Escopo", c_texto(&v.escopo)),
        campo("Vistos", c_contagem_solta(v.vistos)),
        campo("Novos", c_contagem_solta(v.novos)),
        campo("Repetidos", c_contagem_solta(v.repetidos)),
        campo("Excluídos", c_contagem_solta(v.excluidos)),
        campo("Erros", c_contagem_solta(v.erros)),
        campo(
            "Origens",
            c_contagem(origens, alvo.clone(), None, recorte::ORIGENS),
        ),
    ])
}

fn fornecedor(ctx: &Contexto<'_>, alvo: &Ref, hoje: &str) -> ResultadoDeConsulta<Vec<Campo>> {
    let nome = alvo.chave.as_str();
    let atual = ciclo_da_data(hoje).unwrap_or_default();
    let anterior = vistas::mes_anterior(&atual);
    let documentos = contar(
        ctx,
        "SELECT count(*) FROM achado WHERE supplier_name_raw = ? AND estado = 'documento'",
        vec![texto(nome)],
    )?;
    let achados = contar(
        ctx,
        "SELECT count(*) FROM achado WHERE supplier_name_raw = ?",
        vec![texto(nome)],
    )?;
    let ciclos_com_saida =
        match relacoes::selecao_dos_registros_do_total(ctx, alvo, recorte::SAIDAS, None)? {
            Some(sel) => contar(
                ctx,
                &format!(
                    "SELECT count(DISTINCT r.ciclo_id) FROM registro r WHERE 1 = 1{}",
                    sel.sql_e()
                ),
                sel.params().to_vec(),
            )?,
            None => 0,
        };
    let periodo = match relacoes::selecao_de_registros(ctx, alvo)? {
        Some(sel) => consultar(
            ctx.conn,
            &format!(
                "SELECT min(r.data), max(r.data) FROM registro r WHERE 1 = 1{}",
                sel.sql_e()
            ),
            sel.params(),
            |l| {
                Ok((
                    l.get::<_, Option<String>>(0)?,
                    l.get::<_, Option<String>>(1)?,
                ))
            },
        )?
        .into_iter()
        .next()
        .unwrap_or((None, None)),
        None => (None, None),
    };
    // Recorrente: a última decisão `recorrente` que ninguém desfez.
    let recorrente = consultar(
        ctx.conn,
        "SELECT d.depois FROM decisao d
         WHERE d.alvo_tipo = 'fornecedor' AND d.alvo_id = ? AND d.campo = 'recorrente'
           AND NOT EXISTS (SELECT 1 FROM decisao u WHERE u.desfaz = d.id)
         ORDER BY d.em DESC, d.id DESC LIMIT 1",
        &[texto(nome)],
        |l| l.get::<_, Option<String>>(0),
    )?
    .into_iter()
    .next()
    .flatten()
    .is_some_and(|d| d.trim() == "true");
    let regras = contar(
        ctx,
        "SELECT count(*) FROM regra_senha WHERE fornecedor = ?",
        vec![texto(nome)],
    )?;
    let apelidos = apelidos_do_fornecedor(ctx, alvo)?;
    let de_achados = Ref::nova(Tipo::Fornecedor, nome);
    Ok(vec![
        campo(
            "Registros · a conferir",
            contagem_do_cabecalho(ctx, alvo, recorte::REGISTROS, None)?,
        ),
        campo(
            "Documentos",
            c_contagem(documentos, de_achados.clone(), None, recorte::DOCUMENTOS),
        ),
        campo(
            "Achados",
            c_contagem(achados, de_achados.clone(), None, recorte::ACHADOS),
        ),
        campo(
            "Saiu · a conferir",
            valor_do_cabecalho(ctx, alvo, recorte::SAIDAS, None)?,
        ),
        campo(
            &format!("Saiu no ciclo {atual}"),
            valor_do_cabecalho(ctx, alvo, recorte::SAIDAS, Some(&atual))?,
        ),
        campo(
            &format!("Saiu no ciclo {anterior}"),
            valor_do_cabecalho(ctx, alvo, recorte::SAIDAS, Some(&anterior))?,
        ),
        campo("Ciclos com saída", c_contagem_solta(ciclos_com_saida)),
        campo("Primeiro registro", c_data_opt(&periodo.0)),
        campo("Último registro", c_data_opt(&periodo.1)),
        campo("Recorrente", sim_nao(recorrente)),
        campo(
            "Apelidos",
            if apelidos.is_empty() {
                Celula::Vazia
            } else {
                Celula::Texto(apelidos.join(" · "))
            },
        ),
        campo("Contratos", Celula::Texto(CONTRATOS_NA_FATIA_2.to_owned())),
        campo(
            "Regras de senha",
            c_contagem(regras, de_achados, None, recorte::REGRAS),
        ),
    ])
}

/// O texto do campo "Contratos" do fornecedor: o contrato nasce com o livro-caixa.
pub const CONTRATOS_NA_FATIA_2: &str = "chegam na fatia 2";

/// Quantos apelidos o cabeçalho mostra; o resto vira "e mais N".
const APELIDOS_NO_CABECALHO: usize = 8;

/// Junta espaços e tira as pontas (a forma mostrada).
fn espacos_juntos(texto: &str) -> String {
    texto.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// As grafias distintas que viraram este fornecedor nos registros: `supplier_name_raw`,
/// `contraparte` e, quando foi ela que deu o nome (sem as outras), `descricao_original`.
/// Normalizadas (espaços juntos, sem diferença de caixa), sem o próprio nome, da mais usada à
/// menos usada.
fn apelidos_do_fornecedor(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<String>> {
    let Some(sel) = relacoes::selecao_de_registros(ctx, alvo)? else {
        return Ok(Vec::new());
    };
    let contraparte = ctx.dialeto.contraparte("r.");
    let filtro = sel.sql_e();
    let sql = format!(
        "SELECT v, count(*) AS n FROM (
           SELECT r.supplier_name_raw AS v FROM registro r WHERE 1 = 1{filtro}
           UNION ALL SELECT {contraparte} FROM registro r WHERE 1 = 1{filtro}
           UNION ALL SELECT r.descricao_original FROM registro r
            WHERE r.fornecedor_normalizado IS NULL AND {contraparte} IS NULL
              AND nullif(r.supplier_name_raw, '') IS NULL{filtro}
         ) WHERE v IS NOT NULL AND trim(v) <> ''
         GROUP BY v ORDER BY n DESC, v"
    );
    let mut params = Vec::new();
    for _ in 0..3 {
        params.extend_from_slice(sel.params());
    }
    let grafias = consultar(ctx.conn, &sql, &params, |l| {
        Ok((l.get::<_, String>(0)?, l.get::<_, i64>(1)?))
    })?;
    let chave = |t: &str| espacos_juntos(t).to_lowercase();
    let proprio = chave(&alvo.chave);
    // Soma as grafias que só diferem por caixa ou espaço; mostra a mais usada delas.
    let mut juntas: Vec<(String, String, i64)> = Vec::new();
    for (grafia, n) in grafias {
        let k = chave(&grafia);
        if k == proprio {
            continue;
        }
        match juntas.iter_mut().find(|(c, _, _)| *c == k) {
            Some((_, _, total)) => *total += n,
            None => juntas.push((k, espacos_juntos(&grafia), n)),
        }
    }
    juntas.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.1.cmp(&b.1)));
    let total = juntas.len();
    let mut apelidos: Vec<String> = juntas
        .into_iter()
        .take(APELIDOS_NO_CABECALHO)
        .map(|(_, grafia, _)| grafia)
        .collect();
    if total > APELIDOS_NO_CABECALHO {
        apelidos.push(format!("e mais {}", total - APELIDOS_NO_CABECALHO));
    }
    Ok(apelidos)
}

fn ciclo(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let id = alvo.chave.as_str();
    let documentos = contar(
        ctx,
        "SELECT count(*) FROM achado WHERE ciclo_id = ? AND estado = 'documento'",
        vec![texto(id)],
    )?;
    let achados = contar(
        ctx,
        "SELECT count(*) FROM achado WHERE ciclo_id = ?",
        vec![texto(id)],
    )?;
    Ok(vec![
        campo(
            "Período",
            Celula::Texto(format!(
                "{} a {}",
                vistas::inicio_do_ciclo(id),
                vistas::fim_do_ciclo(id)
            )),
        ),
        campo(
            "Registros · a conferir",
            contagem_do_cabecalho(ctx, alvo, recorte::REGISTROS, None)?,
        ),
        campo(
            "Entrou · a conferir",
            valor_do_cabecalho(ctx, alvo, recorte::ENTRADAS, None)?,
        ),
        campo(
            "Saiu · a conferir",
            valor_do_cabecalho(ctx, alvo, recorte::SAIDAS, None)?,
        ),
        campo(
            "Movido entre contas",
            valor_do_cabecalho(ctx, alvo, recorte::MOVIDO, None)?,
        ),
        campo(
            "Documentos",
            c_contagem(documentos, alvo.clone(), None, recorte::DOCUMENTOS),
        ),
        campo(
            "Achados",
            c_contagem(achados, alvo.clone(), None, recorte::ACHADOS),
        ),
    ])
}

fn conta(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let nome = alvo.chave.as_str();
    let periodo = consultar(
        ctx.conn,
        "SELECT min(data), max(data) FROM registro WHERE conta_provavel = ?",
        &[texto(nome)],
        |l| {
            Ok((
                l.get::<_, Option<String>>(0)?,
                l.get::<_, Option<String>>(1)?,
            ))
        },
    )?
    .into_iter()
    .next()
    .unwrap_or((None, None));
    let lacunas = vistas::contas_buracos(
        ctx,
        &Selecao::nova().onde("ct.conta = ?", vec![texto(nome)]),
    )?
    .into_iter()
    .next()
    .map(|l| l.meses_sem_nada)
    .unwrap_or_default();
    let faturas = contar(
        ctx,
        "SELECT count(*) FROM registro WHERE conta_provavel = ? AND tipo = 'fatura_cartao'",
        vec![texto(nome)],
    )?;
    Ok(vec![
        campo(
            "Registros · a conferir",
            contagem_do_cabecalho(ctx, alvo, recorte::REGISTROS, None)?,
        ),
        campo(
            "Entrou · a conferir",
            valor_do_cabecalho(ctx, alvo, recorte::ENTRADAS, None)?,
        ),
        campo(
            "Saiu · a conferir",
            valor_do_cabecalho(ctx, alvo, recorte::SAIDAS, None)?,
        ),
        campo(
            "Movido entre contas",
            valor_do_cabecalho(ctx, alvo, recorte::MOVIDO, None)?,
        ),
        campo("Primeiro registro", c_data_opt(&periodo.0)),
        campo("Último registro", c_data_opt(&periodo.1)),
        campo("Faturas", c_contagem_solta(faturas)),
        campo(
            "Meses sem registro",
            if lacunas.is_empty() {
                Celula::Vazia
            } else {
                let mut meses = lacunas;
                meses.sort();
                Celula::Links(
                    meses
                        .into_iter()
                        .map(|m| (Ref::nova(Tipo::Ciclo, m.as_str()), m))
                        .collect(),
                )
            },
        ),
    ])
}

fn fatura(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Vec<Campo>> {
    let Some(f) = vistas::faturas_conferencia(
        ctx,
        &Selecao::nova().onde("f.fatura = ?", vec![texto(alvo.chave.as_str())]),
    )?
    .into_iter()
    .next() else {
        return Ok(Vec::new());
    };
    Ok(vec![
        campo("Conta", c_link_opt(Tipo::Conta, &f.conta)),
        campo("Vencimento", Celula::Data(f.vencimento.clone())),
        campo(
            "Total da fatura",
            valor_do_cabecalho(ctx, alvo, recorte::FATURA, None)?,
        ),
        campo(
            "Transações",
            c_contagem(f.transacoes, alvo.clone(), None, recorte::TRANSACOES),
        ),
        campo(
            "Soma das transações",
            valor_do_cabecalho(ctx, alvo, recorte::TRANSACOES, None)?,
        ),
        campo("Diferença", c_valor(f.diferenca)),
        campo("Situação", c_texto(&f.situacao)),
    ])
}
