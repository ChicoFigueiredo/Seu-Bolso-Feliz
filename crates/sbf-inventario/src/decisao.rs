//! Dono: agente do núcleo (spec 06, "O banco"). Ver `lib.rs`.
//!
//! O log de decisões: só de acréscimo (o trigger recusa UPDATE e DELETE, INV-07). Toda
//! escrita do usuário é uma linha aqui **e** a projeção, na mesma transação (INV-05).
//! A tabela "Como cada decisão do §10 cai no banco" da spec 06 vira [`efeitos_da_acao`].
//!
//! Desfazer é uma nova decisão com `desfaz` preenchido, que restaura o `antes`.

use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Value, json};

use crate::comando::{Acao, ErroDeComando, IdDecisao, PedidoDeDecisao, ResultadoDeComando};
use crate::projecao::{self, AlvoTipo, campo, ciclo, erro_de_banco};
use crate::referencia::{Ref, Tipo};
use crate::{lote, pendencia};

/// O autor de toda decisão tomada na janela.
pub const AUTOR_USUARIO: &str = "usuario";

/// Uma decisão a gravar: o campo do alvo e o valor que ele passa a ter.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Efeito {
    pub alvo: AlvoTipo,
    pub id: String,
    pub campo: &'static str,
    pub depois: Option<Value>,
    /// As chaves para ler o `antes` (quando `depois` é nulo, a forma vem daqui).
    pub forma: Option<Value>,
}

impl Efeito {
    pub fn novo(alvo: AlvoTipo, id: impl Into<String>, campo: &'static str, depois: Value) -> Self {
        Self {
            alvo,
            id: id.into(),
            campo,
            forma: Some(depois.clone()),
            depois: Some(depois),
        }
    }
}

/// Uma linha de `decisao`, como o núcleo a lê.
#[derive(Debug, Clone, PartialEq)]
pub struct Decisao {
    pub id: IdDecisao,
    pub alvo_tipo: String,
    pub alvo_id: String,
    pub campo: String,
    pub antes: Option<Value>,
    pub depois: Option<Value>,
    pub autor: String,
    pub motivo: Option<String>,
    pub em: String,
    pub desfaz: Option<IdDecisao>,
}

fn json_de(texto: Option<String>) -> Option<Value> {
    texto.and_then(|t| serde_json::from_str(&t).ok())
}

fn json_para_texto(valor: &Option<Value>) -> Option<String> {
    valor.as_ref().map(Value::to_string)
}

/// O instante da transação, no formato do `DEFAULT` das tabelas. Uma decisão com vários
/// alvos tem um `em` só.
pub(crate) fn agora(tx: &Transaction<'_>) -> ResultadoDeComando<String> {
    Ok(
        tx.query_row("SELECT strftime('%Y-%m-%dT%H:%M:%fZ', 'now')", [], |l| {
            l.get(0)
        })?,
    )
}

/// Lê uma decisão do log.
///
/// # Errors
/// Falha do SQLite.
pub fn ler_decisao(
    tx: &rusqlite::Connection,
    id: IdDecisao,
) -> ResultadoDeComando<Option<Decisao>> {
    Ok(tx
        .query_row(
            "SELECT id, alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz
               FROM decisao WHERE id = ?1",
            [id],
            |l| {
                Ok(Decisao {
                    id: l.get(0)?,
                    alvo_tipo: l.get(1)?,
                    alvo_id: l.get(2)?,
                    campo: l.get(3)?,
                    antes: json_de(l.get(4)?),
                    depois: json_de(l.get(5)?),
                    autor: l.get(6)?,
                    motivo: l.get(7)?,
                    em: l.get(8)?,
                    desfaz: l.get(9)?,
                })
            },
        )
        .optional()?)
}

/// Grava a decisão e aplica a projeção. O `antes` é lido da projeção agora.
pub(crate) fn gravar(
    tx: &Transaction<'_>,
    efeito: &Efeito,
    motivo: Option<&str>,
    desfaz: Option<IdDecisao>,
    em: &str,
) -> ResultadoDeComando<IdDecisao> {
    let forma = efeito.forma.as_ref().or(efeito.depois.as_ref());
    let antes = projecao::ler(tx, efeito.alvo, &efeito.id, efeito.campo, forma)?;
    tx.execute(
        "INSERT INTO decisao (alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, em, desfaz)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
        params![
            efeito.alvo.codigo(),
            efeito.id,
            efeito.campo,
            json_para_texto(&antes),
            json_para_texto(&efeito.depois),
            AUTOR_USUARIO,
            motivo,
            em,
            desfaz,
        ],
    )?;
    let id = tx.last_insert_rowid();
    projecao::aplicar(
        tx,
        efeito.alvo,
        &efeito.id,
        efeito.campo,
        efeito.depois.as_ref(),
        em,
    )?;
    projecao::recalcular_decidido_por(tx, efeito.alvo, &efeito.id)?;
    // IA-08: a sugestão aberta do campo que o usuário acabou de decidir deixa de valer.
    crate::sugestao::obsoletar(
        tx,
        efeito.alvo.codigo(),
        &efeito.id,
        efeito.campo,
        efeito.depois.as_ref(),
    )?;
    Ok(id)
}

// ---------------------------------------------------------------------------------------
// Alvos

fn nao_se_aplica(acao: &Acao, alvo: &Ref) -> ErroDeComando {
    let nome = format!("{acao:?}");
    let curto = nome
        .split(['(', ' ', '{'])
        .next()
        .unwrap_or(&nome)
        .to_owned();
    ErroDeComando::NaoSeAplica {
        acao: curto,
        alvo: alvo.clone(),
    }
}

/// O alvo da decisão para uma `Ref`, conferindo que o item existe.
pub(crate) fn resolver_alvo(tx: &Transaction<'_>, alvo: &Ref) -> ResultadoDeComando<AlvoTipo> {
    let (tipo, sql) = match alvo.tipo {
        Tipo::Achado | Tipo::Documento => (
            AlvoTipo::Achado,
            Some("SELECT 1 FROM achado WHERE content_hash = ?1"),
        ),
        Tipo::Registro => (
            AlvoTipo::Registro,
            Some("SELECT 1 FROM registro WHERE id = ?1"),
        ),
        Tipo::Lote => (AlvoTipo::Lote, Some("SELECT 1 FROM lote WHERE id = ?1")),
        Tipo::Pendencia => (
            AlvoTipo::Pendencia,
            Some("SELECT 1 FROM pendencia WHERE id = ?1"),
        ),
        Tipo::Fornecedor => (AlvoTipo::Fornecedor, None),
        _ => return Err(ErroDeComando::NaoEncontrado(alvo.clone())),
    };
    let numerico = matches!(
        tipo,
        AlvoTipo::Registro | AlvoTipo::Lote | AlvoTipo::Pendencia
    );
    if alvo.chave.trim().is_empty() || (numerico && alvo.chave.parse::<i64>().is_err()) {
        return Err(ErroDeComando::NaoEncontrado(alvo.clone()));
    }
    if let Some(sql) = sql
        && !tx.prepare_cached(sql)?.exists([&alvo.chave])?
    {
        return Err(ErroDeComando::NaoEncontrado(alvo.clone()));
    }
    Ok(tipo)
}

fn estado_do_registro(tx: &Transaction<'_>, id: &str) -> ResultadoDeComando<String> {
    Ok(
        tx.query_row("SELECT estado FROM registro WHERE id = ?1", [id], |l| {
            l.get(0)
        })?,
    )
}

// ---------------------------------------------------------------------------------------
// Ações do §10

/// Os efeitos de uma Ação sobre um alvo, conforme a tabela da spec 06.
pub(crate) fn efeitos_da_acao(
    tx: &Transaction<'_>,
    acao: &Acao,
    alvo: &Ref,
    motivo: Option<&str>,
    em: &str,
) -> ResultadoDeComando<Vec<Efeito>> {
    // Sinalizar vale para qualquer item com página; os que têm tabela aqui são conferidos.
    if let Acao::Sinalizar { pergunta } = acao {
        let conferivel = matches!(
            alvo.tipo,
            Tipo::Achado | Tipo::Documento | Tipo::Registro | Tipo::Lote | Tipo::Pendencia
        );
        if conferivel {
            resolver_alvo(tx, alvo)?;
        } else if alvo.chave.trim().is_empty() {
            return Err(ErroDeComando::NaoEncontrado(alvo.clone()));
        }
        return Ok(vec![pendencia::sinalizar(tx, alvo, pergunta, em)?]);
    }
    let tipo = resolver_alvo(tx, alvo)?;
    let id = alvo.chave.as_str();
    let nao = || nao_se_aplica(acao, alvo);
    let um = |campo: &'static str, depois: Value| Ok(vec![Efeito::novo(tipo, id, campo, depois)]);
    match (acao, tipo) {
        (Acao::MarcarDocumento, AlvoTipo::Achado) => {
            um(campo::ESTADO, json!({"estado": "documento"}))
        }
        (Acao::Descartar, AlvoTipo::Achado) => um(campo::ESTADO, json!({"estado": "descartado"})),
        (Acao::EmDuvida, AlvoTipo::Achado) => um(campo::ESTADO, json!({"estado": "duvida"})),
        (Acao::CorrigirFornecedor(nome), AlvoTipo::Achado) => {
            um(campo::FORNECEDOR, json!({"supplier_name_raw": nome}))
        }
        (Acao::CorrigirFornecedor(nome), AlvoTipo::Registro) => {
            um(campo::FORNECEDOR, json!({"fornecedor_normalizado": nome}))
        }
        (Acao::CorrigirCategoria(categoria), AlvoTipo::Registro) => {
            um(campo::CATEGORIA, json!({"categoria_provavel": categoria}))
        }
        (Acao::CorrigirTipo(t), AlvoTipo::Achado) => {
            um(campo::DOCUMENT_TYPE, json!({"document_type": t}))
        }
        (Acao::CorrigirTipo(t), AlvoTipo::Registro) => um(campo::TIPO, json!({"tipo": t})),
        (
            Acao::CorrigirDataEValor {
                data,
                valor_centavos,
            },
            AlvoTipo::Achado | AlvoTipo::Registro,
        ) => {
            if data.is_none() && valor_centavos.is_none() {
                return Err(nao());
            }
            let mut colunas = serde_json::Map::new();
            if let Some(d) = data {
                let id_ciclo = ciclo::ciclo_de(d).ok_or_else(nao)?;
                let coluna = if tipo == AlvoTipo::Achado {
                    "data_chave"
                } else {
                    "data"
                };
                colunas.insert(coluna.into(), json!(d));
                colunas.insert("ciclo_id".into(), json!(id_ciclo));
            }
            if let Some(v) = valor_centavos {
                colunas.insert("valor_centavos".into(), json!(v));
            }
            um(campo::DATA_E_VALOR, Value::Object(colunas))
        }
        (Acao::MarcarDuplicidade { com }, AlvoTipo::Achado) => {
            let outro = resolver_alvo(tx, com)?;
            if outro != AlvoTipo::Achado || com.chave == alvo.chave {
                return Err(nao());
            }
            um(
                campo::DUPLICIDADE,
                json!({"com": com.chave, "existe": true}),
            )
        }
        (Acao::UnirConciliados { com }, AlvoTipo::Registro) => {
            let outro = resolver_alvo(tx, com)?;
            if outro != AlvoTipo::Registro || com.chave == alvo.chave {
                return Err(nao());
            }
            um(
                campo::CONCILIACAO,
                json!({"com": com.chave, "existe": true}),
            )
        }
        (Acao::ConfirmarRecorrencia, AlvoTipo::Fornecedor) => {
            um(campo::RECORRENTE, json!({"recorrente": true}))
        }
        (Acao::Aprovar | Acao::Rejeitar, AlvoTipo::Registro) => {
            if estado_do_registro(tx, id)? == "promovido" {
                return Err(nao());
            }
            let estado = if *acao == Acao::Aprovar {
                "aprovado"
            } else {
                if motivo.is_none_or(|m| m.trim().is_empty()) {
                    return Err(ErroDeComando::MotivoObrigatorio);
                }
                "divergente"
            };
            um(campo::ESTADO, json!({"estado": estado}))
        }
        (Acao::Responder { resposta }, AlvoTipo::Pendencia) => {
            Ok(vec![pendencia::responder(id, resposta, em)])
        }
        (
            Acao::GuardarRegraDeSenha {
                tipo_de_documento,
                estrategia,
                prioridade,
            },
            AlvoTipo::Fornecedor,
        ) => {
            if tipo_de_documento.trim().is_empty() || estrategia.trim().is_empty() {
                return Err(nao());
            }
            um(
                campo::REGRA_SENHA,
                json!({
                    "document_type": tipo_de_documento,
                    "estrategia": estrategia,
                    "ativa": true,
                    "prioridade": prioridade,
                }),
            )
        }
        _ => Err(nao()),
    }
}

/// Executa o pedido: cada alvo, cada efeito, numa transação só (a do chamador).
pub(crate) fn executar(
    tx: &Transaction<'_>,
    pedido: &PedidoDeDecisao,
) -> ResultadoDeComando<Vec<IdDecisao>> {
    let em = agora(tx)?;
    let motivo = pedido.motivo.as_deref();
    let mut ids = Vec::new();
    for alvo in &pedido.alvos {
        for efeito in efeitos_da_acao(tx, &pedido.acao, alvo, motivo, &em)? {
            ids.push(gravar(tx, &efeito, motivo, None, &em)?);
        }
    }
    Ok(ids)
}

// ---------------------------------------------------------------------------------------
// Promover (spec 08): o servidor aceitou o documento

/// Promove registros aprovados (`estado = promovido`, terminal) ou o lote (`promovido_em`).
/// Cada mudança é uma decisão do usuário, com o `motivo`. Um registro já promovido e um lote
/// já promovido não gravam nada (o envio de um lote retoma sem repetir decisão).
///
/// Não é uma [`Acao`] do §10: nenhuma tela do inventário a oferece; só o envio ao servidor a
/// pede.
pub(crate) fn promover(
    tx: &Transaction<'_>,
    alvos: &[Ref],
    motivo: &str,
) -> ResultadoDeComando<Vec<IdDecisao>> {
    let em = agora(tx)?;
    let mut ids = Vec::new();
    for alvo in alvos {
        let tipo = resolver_alvo(tx, alvo)?;
        let nao = || ErroDeComando::NaoSeAplica {
            acao: "Promover".to_owned(),
            alvo: alvo.clone(),
        };
        let efeito = match tipo {
            AlvoTipo::Registro => match estado_do_registro(tx, &alvo.chave)?.as_str() {
                "promovido" => continue,
                "aprovado" => Efeito::novo(
                    tipo,
                    alvo.chave.as_str(),
                    campo::ESTADO,
                    json!({"estado": "promovido"}),
                ),
                _ => return Err(nao()),
            },
            AlvoTipo::Lote => {
                let ja: Option<String> = tx.query_row(
                    "SELECT promovido_em FROM lote WHERE id = ?1",
                    [&alvo.chave],
                    |l| l.get(0),
                )?;
                if ja.is_some() {
                    continue;
                }
                Efeito::novo(
                    tipo,
                    alvo.chave.as_str(),
                    campo::PROMOVIDO_EM,
                    json!({"promovido_em": em}),
                )
            }
            _ => return Err(nao()),
        };
        ids.push(gravar(tx, &efeito, Some(motivo), None, &em)?);
    }
    Ok(ids)
}

// ---------------------------------------------------------------------------------------
// Desfazer

fn ja_desfeita(tx: &Transaction<'_>, id: IdDecisao) -> ResultadoDeComando<bool> {
    Ok(tx
        .prepare_cached("SELECT 1 FROM decisao WHERE desfaz = ?1")?
        .exists([id])?)
}

/// Desfaz `id`: uma nova decisão, com `desfaz = id`, que restaura o `antes`. A aprovação
/// de um lote se desfaz desaprovando (e a desaprovação, aprovando de novo).
pub(crate) fn desfazer(tx: &Transaction<'_>, id: IdDecisao) -> ResultadoDeComando<IdDecisao> {
    let decisao = ler_decisao(tx, id)?.ok_or(ErroDeComando::NadaADesfazer(id))?;
    if ja_desfeita(tx, id)? {
        return Err(ErroDeComando::NadaADesfazer(id));
    }
    let alvo = AlvoTipo::de_codigo(&decisao.alvo_tipo)
        .ok_or_else(|| erro_de_banco(format!("alvo desconhecido: {}", decisao.alvo_tipo)))?;
    if e_promocao(&decisao) {
        return Err(ErroDeComando::NaoSeAplica {
            acao: "Desfazer (a promoção não se desfaz: o servidor já aceitou o documento)".into(),
            alvo: Ref::nova(
                match alvo {
                    AlvoTipo::Lote => Tipo::Lote,
                    _ => Tipo::Registro,
                },
                decisao.alvo_id.clone(),
            ),
        });
    }
    let em = agora(tx)?;
    if alvo == AlvoTipo::Lote {
        let lote_ref = Ref::nova(Tipo::Lote, decisao.alvo_id.clone());
        match decisao.campo.as_str() {
            campo::APROVADO_EM => {
                let era_aprovacao = decisao
                    .depois
                    .as_ref()
                    .and_then(|v| v.get("aprovado_em"))
                    .is_some_and(|v| !v.is_null());
                return if era_aprovacao {
                    lote::desaprovar(tx, &lote_ref, &em, Some(id))
                } else {
                    lote::aprovar(tx, &lote_ref, &em, Some(id))
                };
            }
            campo::ITEM => lote::exigir_editavel(tx, &lote_ref)?,
            campo::CRIADO => {
                lote::exigir_editavel(tx, &lote_ref)?;
                if lote::tem_itens(tx, &decisao.alvo_id)? {
                    return Err(ErroDeComando::NaoSeAplica {
                        acao: "Desfazer (o lote tem itens)".into(),
                        alvo: lote_ref,
                    });
                }
            }
            _ => {}
        }
    }
    desfazer_simples(tx, &decisao, alvo, &em)
}

/// Desfaz sem regra de lote: a projeção volta ao `antes` da decisão.
pub(crate) fn desfazer_simples(
    tx: &Transaction<'_>,
    decisao: &Decisao,
    alvo: AlvoTipo,
    em: &str,
) -> ResultadoDeComando<IdDecisao> {
    let campo = campo_estatico(&decisao.campo)?;
    let efeito = Efeito {
        alvo,
        id: decisao.alvo_id.clone(),
        campo,
        depois: decisao.antes.clone(),
        forma: decisao.depois.clone().or_else(|| decisao.antes.clone()),
    };
    gravar(tx, &efeito, None, Some(decisao.id), em)
}

fn campo_estatico(texto: &str) -> ResultadoDeComando<&'static str> {
    use campo::*;
    [
        ESTADO,
        FORNECEDOR,
        CATEGORIA,
        TIPO,
        DOCUMENT_TYPE,
        DATA_E_VALOR,
        DUPLICIDADE,
        CONCILIACAO,
        RECORRENTE,
        REGRA_SENHA,
        CRIADA,
        RESPOSTA,
        CRIADO,
        ITEM,
        APROVADO_EM,
    ]
    .into_iter()
    .find(|c| *c == texto)
    .ok_or_else(|| erro_de_banco(format!("campo desconhecido: {texto}")))
}

/// A decisão é uma promoção (registro `promovido` ou lote com `promovido_em`)? Terminal.
fn e_promocao(d: &Decisao) -> bool {
    d.campo == campo::PROMOVIDO_EM
        || (d.campo == campo::ESTADO
            && d.depois
                .as_ref()
                .and_then(|v| v.get("estado"))
                .and_then(Value::as_str)
                == Some("promovido"))
}

/// A última decisão do usuário ainda em vigor que não é um desfazer: o que "Desfazer
/// decisão" desfaz. Chamadas seguidas voltam no tempo. As aprovações de registro feitas
/// pela aprovação de um lote ficam de fora: desfazem-se com o lote. As promoções também
/// ficam de fora (o servidor já aceitou o documento; promover não se desfaz).
pub(crate) fn ultima_desfazivel(tx: &Transaction<'_>) -> ResultadoDeComando<Option<IdDecisao>> {
    Ok(tx
        .query_row(
            "SELECT d.id FROM decisao d
              WHERE d.autor = ?1 AND d.desfaz IS NULL
                AND (d.motivo IS NULL OR d.motivo NOT LIKE ?2)
                AND d.campo <> 'promovido_em'
                AND NOT (d.campo = 'estado' AND json_extract(d.depois, '$.estado') = 'promovido')
                AND NOT EXISTS (SELECT 1 FROM decisao x WHERE x.desfaz = d.id)
              ORDER BY d.id DESC LIMIT 1",
            params![AUTOR_USUARIO, format!("{}%", lote::PREFIXO_DO_MOTIVO)],
            |l| l.get(0),
        )
        .optional()?)
}
