//! Dono: agente do núcleo (spec 06, "O banco"). Ver `lib.rs`.
//!
//! A projeção: o estado das tabelas (`achado.estado`, `document_type`, `registro.estado`,
//! `relacao`, `pendencia`, `lote`…) que as decisões do log determinam.
//!
//! Cada linha de `decisao` diz o valor de um **campo** de um **alvo** antes e depois. O
//! campo tem uma forma JSON fixa (tabela abaixo). [`aplicar`] leva a projeção ao valor
//! `depois`; [`ler`] devolve o valor atual na mesma forma, que vira o `antes` da próxima
//! decisão. [`reprojetar`] reaplica o log inteiro em ordem; como a execução usa o mesmo
//! [`aplicar`], o resultado é o mesmo (INV-05).
//!
//! | alvo / campo | forma |
//! |---|---|
//! | achado ou registro / `estado`, `fornecedor`, `categoria`, `tipo`, `document_type`, `data_e_valor` | objeto `{coluna: valor}` com as colunas que mudam |
//! | achado / `duplicidade`, registro / `conciliacao` | `{"com": id, "existe": bool}` |
//! | achado / `caixa_entrada` | `{"situacao": "aberta"\|"resolvida"\|"dispensada"}` (resolver, dispensar e reabrir na Caixa de entrada; não muda `estado` nem `decidido_por`) |
//! | fornecedor / `recorrente` | `{"recorrente": bool}` (só o log; vira compromisso na fatia 3) |
//! | fornecedor / `regra_senha` | `{"document_type", "estrategia", "ativa": bool ou nulo, "prioridade": número ou nulo}` (`ativa` nulo: a regra não existe; `prioridade` nula: a padrão na regra nova, a mesma na que existe) |
//! | pendencia / `criada` | a linha da pendência, ou nulo (desfeita: a pendência fecha) |
//! | pendencia / `resposta` | `{"resposta", "resolvida_em"}` |
//! | lote / `criado` | `{"nome", "criado_em"}`, ou nulo (desfeito: o lote some) |
//! | lote / `item` | `{"tipo": "achado"\|"registro", "id", "no_lote": bool}` |
//! | lote / `aprovado_em` | `{"aprovado_em": instante ou nulo}` |
//! | lote / `promovido_em` | `{"promovido_em": instante}` (o servidor aceitou os documentos do lote) |
//!
//! `decidido_por` não está em nenhuma forma: é derivado do log por
//! [`recalcular_decidido_por`] (`usuario` enquanto houver decisão sua em vigor sobre o
//! alvo; senão `classificado_por` / `extraido_por`).

pub mod ciclo;

use std::collections::BTreeSet;

use rusqlite::types::{Value as ValorSql, ValueRef};
use rusqlite::{OptionalExtension, Transaction, params};
use serde_json::{Map, Value, json};

use crate::comando::{ErroDeComando, ResultadoDeComando};

/// Os campos de decisão (coluna `decisao.campo`).
pub mod campo {
    pub const ESTADO: &str = "estado";
    pub const FORNECEDOR: &str = "fornecedor";
    pub const CATEGORIA: &str = "categoria";
    pub const TIPO: &str = "tipo";
    pub const DOCUMENT_TYPE: &str = "document_type";
    pub const DATA_E_VALOR: &str = "data_e_valor";
    pub const DUPLICIDADE: &str = "duplicidade";
    pub const CONCILIACAO: &str = "conciliacao";
    pub const RECORRENTE: &str = "recorrente";
    pub const REGRA_SENHA: &str = "regra_senha";
    pub const CRIADA: &str = "criada";
    pub const RESPOSTA: &str = "resposta";
    pub const CRIADO: &str = "criado";
    pub const ITEM: &str = "item";
    pub const APROVADO_EM: &str = "aprovado_em";
    pub const PROMOVIDO_EM: &str = "promovido_em";
    pub const CAIXA_ENTRADA: &str = "caixa_entrada";
}

/// O texto que fecha uma pendência cuja sinalização foi desfeita.
pub const RESPOSTA_DE_SINALIZACAO_DESFEITA: &str = "(sinalização desfeita)";

/// `decisao.alvo_tipo`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum AlvoTipo {
    Achado,
    Registro,
    Fornecedor,
    Lote,
    Pendencia,
}

impl AlvoTipo {
    pub fn codigo(self) -> &'static str {
        match self {
            AlvoTipo::Achado => "achado",
            AlvoTipo::Registro => "registro",
            AlvoTipo::Fornecedor => "fornecedor",
            AlvoTipo::Lote => "lote",
            AlvoTipo::Pendencia => "pendencia",
        }
    }

    pub fn de_codigo(codigo: &str) -> Option<AlvoTipo> {
        [
            AlvoTipo::Achado,
            AlvoTipo::Registro,
            AlvoTipo::Fornecedor,
            AlvoTipo::Lote,
            AlvoTipo::Pendencia,
        ]
        .into_iter()
        .find(|t| t.codigo() == codigo)
    }

    /// A tabela da projeção com a chave do alvo, quando há uma.
    fn tabela(self) -> Option<(&'static str, &'static str)> {
        match self {
            AlvoTipo::Achado => Some(("achado", "content_hash")),
            AlvoTipo::Registro => Some(("registro", "id")),
            AlvoTipo::Lote => Some(("lote", "id")),
            AlvoTipo::Pendencia => Some(("pendencia", "id")),
            AlvoTipo::Fornecedor => None,
        }
    }

    /// As colunas que uma decisão pode mudar por objeto `{coluna: valor}`.
    fn colunas(self) -> &'static [&'static str] {
        match self {
            AlvoTipo::Achado => &[
                "estado",
                "document_type",
                "supplier_name_raw",
                "data_chave",
                "ciclo_id",
                "valor_centavos",
            ],
            AlvoTipo::Registro => &[
                "estado",
                "tipo",
                "fornecedor_normalizado",
                "categoria_provavel",
                "data",
                "ciclo_id",
                "valor_centavos",
            ],
            AlvoTipo::Pendencia => &["resposta", "resolvida_em"],
            AlvoTipo::Lote => &["aprovado_em", "promovido_em"],
            AlvoTipo::Fornecedor => &[],
        }
    }
}

pub(crate) fn erro_de_banco(texto: impl Into<String>) -> ErroDeComando {
    ErroDeComando::Banco(texto.into())
}

fn para_sql(valor: &Value) -> ResultadoDeComando<ValorSql> {
    Ok(match valor {
        Value::Null => ValorSql::Null,
        Value::Bool(b) => ValorSql::Integer(i64::from(*b)),
        Value::Number(n) => match n.as_i64() {
            Some(i) => ValorSql::Integer(i),
            None => ValorSql::Real(n.as_f64().unwrap_or_default()),
        },
        Value::String(s) => ValorSql::Text(s.clone()),
        outro => return Err(erro_de_banco(format!("valor sem coluna: {outro}"))),
    })
}

fn de_sql(valor: ValueRef<'_>) -> Value {
    match valor {
        ValueRef::Null => Value::Null,
        ValueRef::Integer(i) => json!(i),
        ValueRef::Real(r) => json!(r),
        ValueRef::Text(t) => Value::String(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(_) => Value::Null,
    }
}

fn texto_de<'a>(forma: Option<&'a Value>, chave: &str) -> ResultadoDeComando<&'a str> {
    forma
        .and_then(|f| f.get(chave))
        .and_then(Value::as_str)
        .ok_or_else(|| erro_de_banco(format!("decisão sem `{chave}`")))
}

fn bool_de(valor: Option<&Value>, chave: &str) -> bool {
    valor
        .and_then(|v| v.get(chave))
        .and_then(Value::as_bool)
        .unwrap_or(false)
}

fn id_inteiro(id: &str) -> ResultadoDeComando<i64> {
    id.parse()
        .map_err(|_| erro_de_banco(format!("id não numérico: {id}")))
}

// ---------------------------------------------------------------------------------------
// Leitura do valor atual (o `antes`)

/// O valor atual do campo do alvo, na forma do campo. `forma` dá as chaves (as colunas do
/// objeto, o `com` de uma relação, o item de um lote).
///
/// # Errors
/// Campo desconhecido para o alvo, ou falha do SQLite.
pub fn ler(
    tx: &Transaction<'_>,
    alvo: AlvoTipo,
    id: &str,
    campo: &str,
    forma: Option<&Value>,
) -> ResultadoDeComando<Option<Value>> {
    use campo::*;
    match (alvo, campo) {
        (AlvoTipo::Achado, DUPLICIDADE) | (AlvoTipo::Registro, CONCILIACAO) => {
            let com = texto_de(forma, "com")?;
            Ok(Some(
                json!({"com": com, "existe": relacao_existe(tx, alvo, campo, id, com)?}),
            ))
        }
        (AlvoTipo::Achado, CAIXA_ENTRADA) => crate::caixa_de_entrada::forma_atual(tx, id).map(Some),
        (AlvoTipo::Pendencia, CRIADA) => ler_pendencia(tx, id),
        (AlvoTipo::Lote, CRIADO) => ler_lote(tx, id),
        (AlvoTipo::Lote, ITEM) => {
            let tipo = texto_de(forma, "tipo")?;
            let item = texto_de(forma, "id")?;
            let existe = tx
                .prepare_cached(
                    "SELECT 1 FROM lote_item WHERE lote_id = ?1 AND alvo_tipo = ?2 AND alvo_id = ?3",
                )?
                .exists(params![id_inteiro(id)?, tipo, item])?;
            Ok(Some(json!({"tipo": tipo, "id": item, "no_lote": existe})))
        }
        (AlvoTipo::Fornecedor, RECORRENTE) => {
            let ultimo: Option<Option<String>> = tx
                .query_row(
                    "SELECT depois FROM decisao WHERE alvo_tipo = 'fornecedor' AND alvo_id = ?1
                       AND campo = ?2 ORDER BY id DESC LIMIT 1",
                    params![id, RECORRENTE],
                    |l| l.get(0),
                )
                .optional()?;
            let recorrente = ultimo
                .flatten()
                .and_then(|t| serde_json::from_str::<Value>(&t).ok())
                .is_some_and(|v| bool_de(Some(&v), "recorrente"));
            Ok(Some(json!({"recorrente": recorrente})))
        }
        (AlvoTipo::Fornecedor, REGRA_SENHA) => {
            let tipo_de_documento = texto_de(forma, "document_type")?;
            let estrategia = texto_de(forma, "estrategia")?;
            let linha: Option<(i64, i64)> = tx
                .query_row(
                    "SELECT ativa, prioridade FROM regra_senha
                      WHERE fornecedor = ?1 AND document_type = ?2 AND estrategia = ?3",
                    params![id, tipo_de_documento, estrategia],
                    |l| Ok((l.get(0)?, l.get(1)?)),
                )
                .optional()?;
            // `ativa: null` = a regra não existe.
            Ok(Some(json!({
                "document_type": tipo_de_documento,
                "estrategia": estrategia,
                "ativa": linha.map(|(a, _)| a != 0),
                "prioridade": linha.map(|(_, p)| p),
            })))
        }
        (AlvoTipo::Fornecedor, _) => Err(erro_de_banco(format!("campo {campo} de fornecedor"))),
        _ => ler_colunas(tx, alvo, id, forma).map(Some),
    }
}

fn ler_colunas(
    tx: &Transaction<'_>,
    alvo: AlvoTipo,
    id: &str,
    forma: Option<&Value>,
) -> ResultadoDeComando<Value> {
    let (tabela, chave) = alvo
        .tabela()
        .ok_or_else(|| erro_de_banco("alvo sem tabela"))?;
    let objeto = forma
        .and_then(Value::as_object)
        .ok_or_else(|| erro_de_banco("decisão sem colunas"))?;
    let mut atual = Map::new();
    for coluna in objeto.keys() {
        conferir_coluna(alvo, coluna)?;
        let valor = tx
            .query_row(
                &format!("SELECT {coluna} FROM {tabela} WHERE {chave} = ?1"),
                [id],
                |l| Ok(de_sql(l.get_ref(0)?)),
            )
            .optional()?
            .unwrap_or(Value::Null);
        atual.insert(coluna.clone(), valor);
    }
    Ok(Value::Object(atual))
}

fn conferir_coluna(alvo: AlvoTipo, coluna: &str) -> ResultadoDeComando<()> {
    if alvo.colunas().contains(&coluna) {
        Ok(())
    } else {
        Err(erro_de_banco(format!(
            "a coluna {coluna} de {} não é da projeção",
            alvo.codigo()
        )))
    }
}

fn ler_pendencia(tx: &Transaction<'_>, id: &str) -> ResultadoDeComando<Option<Value>> {
    Ok(tx
        .query_row(
            "SELECT etapa, tipo, referencia, pergunta, criada_em, resolvida_em, resposta
               FROM pendencia WHERE id = ?1",
            [id],
            |l| {
                let mut m = Map::new();
                for (i, nome) in [
                    "etapa",
                    "tipo",
                    "referencia",
                    "pergunta",
                    "criada_em",
                    "resolvida_em",
                    "resposta",
                ]
                .into_iter()
                .enumerate()
                {
                    m.insert(nome.to_owned(), de_sql(l.get_ref(i)?));
                }
                Ok(Value::Object(m))
            },
        )
        .optional()?)
}

fn ler_lote(tx: &Transaction<'_>, id: &str) -> ResultadoDeComando<Option<Value>> {
    Ok(tx
        .query_row(
            "SELECT nome, criado_em FROM lote WHERE id = ?1",
            [id],
            |l| {
                Ok(json!({
                    "nome": de_sql(l.get_ref(0)?),
                    "criado_em": de_sql(l.get_ref(1)?),
                }))
            },
        )
        .optional()?)
}

fn par_normalizado(alvo: AlvoTipo, a: &str, b: &str) -> ResultadoDeComando<(ValorSql, ValorSql)> {
    if alvo == AlvoTipo::Registro {
        let (x, y) = (id_inteiro(a)?, id_inteiro(b)?);
        Ok((ValorSql::Integer(x.min(y)), ValorSql::Integer(x.max(y))))
    } else {
        let (x, y) = if a <= b { (a, b) } else { (b, a) };
        Ok((ValorSql::Text(x.to_owned()), ValorSql::Text(y.to_owned())))
    }
}

fn tabela_de_relacao(alvo: AlvoTipo) -> &'static str {
    if alvo == AlvoTipo::Registro {
        "relacao_registro"
    } else {
        "relacao"
    }
}

fn relacao_existe(
    tx: &Transaction<'_>,
    alvo: AlvoTipo,
    tipo: &str,
    a: &str,
    b: &str,
) -> ResultadoDeComando<bool> {
    let (x, y) = par_normalizado(alvo, a, b)?;
    let tabela = tabela_de_relacao(alvo);
    Ok(tx
        .prepare_cached(&format!(
            "SELECT 1 FROM {tabela} WHERE tipo = ?1 AND ((a = ?2 AND b = ?3) OR (a = ?3 AND b = ?2))"
        ))?
        .exists(params![tipo, x, y])?)
}

// ---------------------------------------------------------------------------------------
// Aplicação do valor `depois`

/// Leva a projeção do campo do alvo ao `valor`. `em` é o instante da decisão (fecha a
/// pendência de uma sinalização desfeita).
///
/// # Errors
/// Campo desconhecido, coluna fora da projeção ou falha do SQLite.
pub fn aplicar(
    tx: &Transaction<'_>,
    alvo: AlvoTipo,
    id: &str,
    campo: &str,
    valor: Option<&Value>,
    em: &str,
) -> ResultadoDeComando<()> {
    use campo::*;
    match (alvo, campo) {
        (AlvoTipo::Achado, DUPLICIDADE) | (AlvoTipo::Registro, CONCILIACAO) => {
            let com = texto_de(valor, "com")?;
            let (x, y) = par_normalizado(alvo, id, com)?;
            let tabela = tabela_de_relacao(alvo);
            if bool_de(valor, "existe") {
                tx.execute(
                    &format!("INSERT OR IGNORE INTO {tabela} (a, b, tipo, motivo) VALUES (?1, ?2, ?3, 'decisão do usuário')"),
                    params![x, y, campo],
                )?;
            } else {
                tx.execute(
                    &format!("DELETE FROM {tabela} WHERE tipo = ?1 AND ((a = ?2 AND b = ?3) OR (a = ?3 AND b = ?2))"),
                    params![campo, x, y],
                )?;
            }
        }
        (AlvoTipo::Achado, CAIXA_ENTRADA) => {
            crate::caixa_de_entrada::aplicar_forma(tx, id, valor, em)?;
        }
        (AlvoTipo::Pendencia, CRIADA) => match valor {
            Some(v) if !v.is_null() => {
                let campo_de = |c: &str| v.get(c).map(para_sql).transpose();
                tx.execute(
                    "INSERT INTO pendencia
                       (id, etapa, tipo, referencia, pergunta, criada_em, resolvida_em, resposta)
                     VALUES (?1, ?2, ?3, ?4, ?5, COALESCE(?6, ?9), ?7, ?8)
                     ON CONFLICT(id) DO UPDATE SET resolvida_em = excluded.resolvida_em,
                                                   resposta = excluded.resposta",
                    params![
                        id_inteiro(id)?,
                        campo_de("etapa")?.unwrap_or(ValorSql::Text("desktop".into())),
                        campo_de("tipo")?.unwrap_or(ValorSql::Text("sinalizada".into())),
                        campo_de("referencia")?.unwrap_or(ValorSql::Null),
                        campo_de("pergunta")?.unwrap_or(ValorSql::Text(String::new())),
                        campo_de("criada_em")?.unwrap_or(ValorSql::Null),
                        campo_de("resolvida_em")?.unwrap_or(ValorSql::Null),
                        campo_de("resposta")?.unwrap_or(ValorSql::Null),
                        em,
                    ],
                )?;
            }
            _ => {
                tx.execute(
                    "UPDATE pendencia SET resolvida_em = COALESCE(resolvida_em, ?2),
                                          resposta = COALESCE(resposta, ?3)
                      WHERE id = ?1",
                    params![id_inteiro(id)?, em, RESPOSTA_DE_SINALIZACAO_DESFEITA],
                )?;
            }
        },
        (AlvoTipo::Lote, CRIADO) => match valor {
            Some(v) if !v.is_null() => {
                tx.execute(
                    "INSERT INTO lote (id, nome, criado_em) VALUES (?1, ?2, COALESCE(?3, ?4))
                     ON CONFLICT(id) DO UPDATE SET nome = excluded.nome",
                    params![
                        id_inteiro(id)?,
                        texto_de(Some(v), "nome")?,
                        v.get("criado_em").and_then(Value::as_str),
                        em
                    ],
                )?;
            }
            _ => {
                let lote = id_inteiro(id)?;
                tx.execute("DELETE FROM lote_item WHERE lote_id = ?1", [lote])?;
                tx.execute("DELETE FROM lote WHERE id = ?1", [lote])?;
            }
        },
        (AlvoTipo::Lote, ITEM) => {
            let tipo = texto_de(valor, "tipo")?;
            let item = texto_de(valor, "id")?;
            if bool_de(valor, "no_lote") {
                tx.execute(
                    "INSERT OR IGNORE INTO lote_item (lote_id, alvo_tipo, alvo_id) VALUES (?1, ?2, ?3)",
                    params![id_inteiro(id)?, tipo, item],
                )?;
            } else {
                tx.execute(
                    "DELETE FROM lote_item WHERE lote_id = ?1 AND alvo_tipo = ?2 AND alvo_id = ?3",
                    params![id_inteiro(id)?, tipo, item],
                )?;
            }
        }
        (AlvoTipo::Fornecedor, RECORRENTE) => {}
        (AlvoTipo::Fornecedor, REGRA_SENHA) => {
            let tipo_de_documento = texto_de(valor, "document_type")?;
            let estrategia = texto_de(valor, "estrategia")?;
            // Nula: a padrão na regra nova, a mesma na que já existe.
            let prioridade = valor
                .and_then(|v| v.get("prioridade"))
                .and_then(Value::as_i64);
            if bool_de(valor, "ativa") {
                tx.execute(
                    "INSERT INTO regra_senha (fornecedor, document_type, estrategia, ativa, prioridade)
                     VALUES (?1, ?2, ?3, 1, COALESCE(?4, ?5))
                     ON CONFLICT(fornecedor, document_type, estrategia) DO UPDATE
                        SET ativa = 1, prioridade = COALESCE(?4, regra_senha.prioridade)",
                    params![
                        id,
                        tipo_de_documento,
                        estrategia,
                        prioridade,
                        crate::protegidos::PRIORIDADE_PADRAO_DA_REGRA
                    ],
                )?;
            } else {
                // Desfazer a criação apaga a regra que nunca foi usada; a que já tem
                // tentativas só se desativa, para não perder a contagem.
                let ausente = valor
                    .and_then(|v| v.get("ativa"))
                    .is_none_or(Value::is_null);
                if ausente {
                    tx.execute(
                        "DELETE FROM regra_senha
                          WHERE fornecedor = ?1 AND document_type = ?2 AND estrategia = ?3
                            AND sucessos = 0 AND falhas = 0",
                        params![id, tipo_de_documento, estrategia],
                    )?;
                }
                tx.execute(
                    "UPDATE regra_senha SET ativa = 0, prioridade = COALESCE(?4, prioridade)
                      WHERE fornecedor = ?1 AND document_type = ?2 AND estrategia = ?3",
                    params![id, tipo_de_documento, estrategia, prioridade],
                )?;
            }
        }
        (AlvoTipo::Fornecedor, _) => {
            return Err(erro_de_banco(format!("campo {campo} de fornecedor")));
        }
        _ => aplicar_colunas(tx, alvo, id, valor)?,
    }
    Ok(())
}

fn aplicar_colunas(
    tx: &Transaction<'_>,
    alvo: AlvoTipo,
    id: &str,
    valor: Option<&Value>,
) -> ResultadoDeComando<()> {
    let (tabela, chave) = alvo
        .tabela()
        .ok_or_else(|| erro_de_banco("alvo sem tabela"))?;
    let objeto = valor
        .and_then(Value::as_object)
        .ok_or_else(|| erro_de_banco("decisão sem colunas"))?;
    for (coluna, v) in objeto {
        conferir_coluna(alvo, coluna)?;
        if alvo == AlvoTipo::Registro && coluna == "estado" && registro_promovido(tx, id)? {
            // Promovido é terminal e vem de fora do log (o `importar-pluggy` marca o que já
            // foi lançado no app). Nenhuma decisão, desfazer ou `reprojetar` tira um
            // registro de promovido; Aprovar e Rejeitar já recusam antes de gravar.
            continue;
        }
        if coluna == "ciclo_id"
            && let Some(id_ciclo) = v.as_str()
        {
            garantir_ciclo(tx, id_ciclo)?;
        }
        tx.execute(
            &format!("UPDATE {tabela} SET {coluna} = ?1 WHERE {chave} = ?2"),
            params![para_sql(v)?, id],
        )?;
    }
    Ok(())
}

/// Se o registro está promovido (já virou movimentação no app).
fn registro_promovido(tx: &Transaction<'_>, id: &str) -> ResultadoDeComando<bool> {
    let estado: Option<String> = tx
        .query_row("SELECT estado FROM registro WHERE id = ?1", [id], |l| {
            l.get(0)
        })
        .optional()?;
    Ok(estado.as_deref() == Some("promovido"))
}

/// Cria a linha de `ciclo` que a chave estrangeira de `ciclo_id` exige.
fn garantir_ciclo(tx: &Transaction<'_>, id: &str) -> ResultadoDeComando<()> {
    let c = ciclo::limites_do_ciclo(id)
        .ok_or_else(|| erro_de_banco(format!("ciclo inválido: {id}")))?;
    tx.execute(
        "INSERT OR IGNORE INTO ciclo (id, inicio, fim) VALUES (?1, ?2, ?3)",
        params![c.id, c.inicio, c.fim],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Quem decidiu

/// Recalcula `decidido_por` do achado ou registro a partir do log: `usuario` se alguma
/// decisão sua sobre o alvo está em vigor (não foi desfeita, ou o desfazer foi desfeito);
/// senão o autor automático (`classificado_por` / `extraido_por`).
///
/// # Errors
/// Falha do SQLite.
pub fn recalcular_decidido_por(
    tx: &Transaction<'_>,
    alvo: AlvoTipo,
    id: &str,
) -> ResultadoDeComando<()> {
    let automatico = match alvo {
        AlvoTipo::Achado => "classificado_por",
        AlvoTipo::Registro => "extraido_por",
        _ => return Ok(()),
    };
    let (tabela, chave) = alvo
        .tabela()
        .ok_or_else(|| erro_de_banco("alvo sem tabela"))?;
    let mut consulta = tx.prepare_cached(
        "SELECT id, desfaz, autor FROM decisao WHERE alvo_tipo = ?1 AND alvo_id = ?2
          AND campo <> 'caixa_entrada'
          ORDER BY id DESC",
    )?;
    let linhas = consulta
        .query_map(params![alvo.codigo(), id], |l| {
            Ok((
                l.get::<_, i64>(0)?,
                l.get::<_, Option<i64>>(1)?,
                l.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    // Em ordem decrescente: quem desfaz vem sempre depois do desfeito.
    let mut desfeita_por_vigente: BTreeSet<i64> = BTreeSet::new();
    let mut do_usuario = false;
    for (decisao, desfaz, autor) in &linhas {
        let vigente = !desfeita_por_vigente.contains(decisao);
        if vigente {
            if let Some(d) = desfaz {
                desfeita_por_vigente.insert(*d);
            } else if autor == "usuario" {
                do_usuario = true;
            }
        }
    }
    let sql = if do_usuario {
        format!("UPDATE {tabela} SET decidido_por = 'usuario' WHERE {chave} = ?1")
    } else {
        format!("UPDATE {tabela} SET decidido_por = {automatico} WHERE {chave} = ?1")
    };
    tx.execute(&sql, [id])?;
    Ok(())
}

// ---------------------------------------------------------------------------------------
// Reprojetar

/// O que [`reprojetar`] fez.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResumoDaReprojecao {
    /// Decisões reaplicadas.
    pub decisoes: usize,
    /// Achados e registros cujo `decidido_por` foi recalculado.
    pub alvos: usize,
}

/// Refaz a projeção a partir do log: reaplica o `depois` de cada decisão, em ordem, e
/// recalcula `decidido_por`. O que nenhuma decisão tocou fica como está (é dos scripts).
///
/// # Errors
/// Decisão ilegível ou falha do SQLite. A transação do chamador é desfeita inteira.
pub fn reprojetar(tx: &Transaction<'_>) -> ResultadoDeComando<ResumoDaReprojecao> {
    let decisoes = {
        let mut consulta =
            tx.prepare("SELECT alvo_tipo, alvo_id, campo, depois, em FROM decisao ORDER BY id")?;
        consulta
            .query_map([], |l| {
                Ok((
                    l.get::<_, String>(0)?,
                    l.get::<_, String>(1)?,
                    l.get::<_, String>(2)?,
                    l.get::<_, Option<String>>(3)?,
                    l.get::<_, String>(4)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    // O `id` da regra de senha é um rowid: reaplicar o log sobre linhas que já existem
    // (apaga, recria) o reatribuiria fora de ordem. As regras que o log cria e que ninguém
    // usou ainda saem antes; o log as recria na ordem em que nasceram, com os mesmos ids de
    // quem o aplicou pela primeira vez. As usadas (têm contagem) ficam, e as dos scripts
    // (que o log nunca citou) também.
    tx.execute(
        "DELETE FROM regra_senha
          WHERE sucessos = 0 AND falhas = 0 AND EXISTS (
            SELECT 1 FROM decisao d
             WHERE d.alvo_tipo = 'fornecedor' AND d.campo = 'regra_senha'
               AND d.alvo_id = regra_senha.fornecedor
               AND json_extract(d.depois, '$.document_type') = regra_senha.document_type
               AND json_extract(d.depois, '$.estrategia') = regra_senha.estrategia)",
        [],
    )?;
    let mut alvos = BTreeSet::new();
    for (alvo_tipo, alvo_id, campo, depois, em) in &decisoes {
        let alvo = AlvoTipo::de_codigo(alvo_tipo)
            .ok_or_else(|| erro_de_banco(format!("alvo desconhecido: {alvo_tipo}")))?;
        let valor = depois
            .as_deref()
            .map(serde_json::from_str::<Value>)
            .transpose()
            .map_err(|e| erro_de_banco(format!("JSON da decisão: {e}")))?;
        aplicar(tx, alvo, alvo_id, campo, valor.as_ref(), em)?;
        if matches!(alvo, AlvoTipo::Achado | AlvoTipo::Registro) {
            alvos.insert((alvo, alvo_id.clone()));
        }
    }
    for (alvo, id) in &alvos {
        recalcular_decidido_por(tx, *alvo, id)?;
    }
    Ok(ResumoDaReprojecao {
        decisoes: decisoes.len(),
        alvos: alvos.len(),
    })
}
