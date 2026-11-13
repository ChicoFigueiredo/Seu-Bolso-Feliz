//! Navegação relacional (spec 06, "Navegação relacional: todo nome é link"):
//! as abas de uma página ([`PedidoDeTabela::Ligados`]), os itens que somam cada número
//! ([`PedidoDeTabela::ItensDoTotal`]), o total por ciclo do cabeçalho
//! ([`por_ciclo_de`]) e as listagens das entidades do inventário que não vêm das 12
//! vistas (origem, decisão, lote, pendência, regra de senha, tentativa, conexão).
//!
//! # Pares de `Ligados` (`de` → `tipo`)
//!
//! | `de` | `tipo` |
//! |---|---|
//! | fornecedor | registro, documento, achado, decisão, regra de senha |
//! | ciclo | registro, documento, achado |
//! | conta | registro, fatura |
//! | fatura | registro |
//! | achado / documento | origem, registro, decisão, lote, achado (relações), pendência, fornecedor, ciclo, conexão, coleta, regra de senha |
//! | origem | achado, documento, conexão, coleta, registro |
//! | conexão | coleta, origem, registro, achado, documento |
//! | coleta | origem, conexão, achado, documento |
//! | registro | achado, documento, origem, decisão, lote, conexão, fornecedor, conta, ciclo, fatura, pendência, registro (conciliados) |
//! | lote | registro, achado, documento, decisão |
//! | pendência | o alvo (achado, documento, registro, lote), decisão |
//! | regra de senha | achado e documento (as tentativas), fornecedor |
//! | decisão | o alvo (achado, documento, registro, fornecedor, lote, pendência), decisão (desfaz / desfeita por) |
//!
//! Par fora da lista devolve tabela vazia. Tipo de fatia futura devolve
//! [`ErroDeConsulta::NaoSuportado`] com o motivo de [`Tipo::motivo_da_fatia`].
//!
//! # Recortes de `ItensDoTotal`
//!
//! `recorte` é `base` ou `base:tipo` (o `tipo` filtra `registro.tipo`). Chave `*` em
//! `de` significa "todos" do tipo. Ver [`recorte`].
//!
//! Dono: agente de leitura.

use rusqlite::types::Value;

use crate::consulta::ErroDeConsulta;
use crate::consulta::{
    Celula, Centavos, Filtro, Fonte, Linha, PedidoDeTabela, Quem, ResultadoDeConsulta, Tabela,
    TotalDoCiclo,
};
use crate::movimento;
use crate::referencia::{Ref, Tipo};
use crate::vistas::{
    self, Contexto, LinkDoAchado, Selecao, c_contagem, c_contagem_solta, c_data_opt,
    c_estado_achado, c_link, c_link_opt, c_texto, c_texto_opt, c_valor, c_valor_total, coluna,
    coluna_num, consultar, hash_curto, quem_de, rotulo_da_coleta, rotulo_do_achado,
    rotulo_do_registro, texto,
};

/// Os recortes de `ItensDoTotal` que este módulo entende.
pub mod recorte {
    /// Registros que são gasto (saída que não é movimento entre contas).
    pub const SAIDAS: &str = "saidas";
    /// Registros que são entrada (que não é movimento entre contas).
    pub const ENTRADAS: &str = "entradas";
    /// Registros que saíram como movimento entre contas.
    pub const MOVIDO: &str = "movido";
    /// Todos os registros, qualquer tipo ou sinal.
    pub const REGISTROS: &str = "registros";
    /// As compras ligadas a uma fatura (sem a fatura nem o pagamento dela).
    pub const TRANSACOES: &str = "transacoes";
    /// O registro da própria fatura.
    pub const FATURA: &str = "fatura";
    /// Achados em estado Documento.
    pub const DOCUMENTOS: &str = "documentos";
    /// Achados de qualquer estado.
    pub const ACHADOS: &str = "achados";
    /// Achados com `estado = 'duvida'`.
    pub const DUVIDAS: &str = "duvidas";
    /// Achados com `estado = 'novo'`.
    pub const A_TRIAR: &str = "a_triar";
    /// Achados já copiados para o acervo.
    pub const COPIADOS: &str = "copiados";
    pub const ORIGENS: &str = "origens";
    pub const COLETAS: &str = "coletas";
    /// Achados e registros de um lote, misturados.
    pub const ITENS: &str = "itens";
    /// Pendências ainda abertas.
    pub const ABERTAS: &str = "abertas";
    pub const DECISOES: &str = "decisoes";
    /// Registros que não são movimento entre contas e não têm fornecedor ou categoria.
    pub const NAO_CLASSIFICADOS: &str = "nao_classificados";
    /// Registros que o usuário ainda não aprovou (extraído, conferido ou divergente).
    pub const NAO_APROVADOS: &str = "nao_aprovados";
    /// As tentativas de abrir PDFs protegidos de uma regra de senha.
    pub const TENTATIVAS: &str = "tentativas";
    /// As regras de senha de um fornecedor.
    pub const REGRAS: &str = "regras";
}

fn inteiro(n: i64) -> Value {
    Value::Integer(n)
}

fn id_numerico(chave: &str) -> Option<i64> {
    chave.parse().ok()
}

/// No produto, `documento:<hash>` e `achado:<hash>` são o mesmo conteúdo.
fn normalizar(tipo: Tipo) -> Tipo {
    if tipo == Tipo::Documento {
        Tipo::Achado
    } else {
        tipo
    }
}

fn fora_da_fatia_1(tipo: Tipo) -> Option<ErroDeConsulta> {
    tipo.motivo_da_fatia().map(ErroDeConsulta::NaoSuportado)
}

/// O recorte pedido: `base[:tipo][|categoria=X][|ciclos=DE..ATE][|filtro=JSON]`.
///
/// - `tipo` filtra `registro.tipo`;
/// - `categoria` filtra a categoria provável (`Sem categoria` é a categoria vazia);
/// - `ciclos` vale um intervalo de ciclos;
/// - `filtro` é o [`Filtro`] em JSON, sempre o último modificador: um número que saiu de
///   uma tabela filtrada abre os itens do mesmo filtro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recorte {
    pub base: String,
    pub tipo: Option<String>,
    pub categoria: Option<String>,
    pub ciclos: Option<(String, String)>,
    /// O filtro da aba Início que gerou o número (vale para os registros).
    pub filtro: Option<Filtro>,
}

impl Recorte {
    pub fn ler(texto: Option<&str>) -> Recorte {
        let texto = texto.unwrap_or(recorte::REGISTROS);
        // O filtro é o último modificador e pode conter `|`: separa antes.
        let (texto, filtro) = match texto.split_once("|filtro=") {
            Some((antes, json)) => (antes, serde_json::from_str::<Filtro>(json).ok()),
            None => (texto, None),
        };
        let mut partes = texto.split('|');
        let primeira = partes.next().unwrap_or(recorte::REGISTROS);
        let (base, tipo) = match primeira.split_once(':') {
            Some((b, t)) => (b.to_owned(), Some(t.to_owned())),
            None => (primeira.to_owned(), None),
        };
        let mut r = Recorte {
            base,
            tipo,
            categoria: None,
            ciclos: None,
            filtro,
        };
        for parte in partes {
            if let Some(c) = parte.strip_prefix("categoria=") {
                r.categoria = Some(c.to_owned());
            } else if let Some(c) = parte.strip_prefix("ciclos=")
                && let Some((de, ate)) = c.split_once("..")
            {
                r.ciclos = Some((de.to_owned(), ate.to_owned()));
            }
        }
        r
    }

    /// O texto do recorte, para o pedido que a janela devolve.
    pub fn para_texto(&self) -> String {
        let mut t = self.base.clone();
        if let Some(tipo) = &self.tipo {
            t.push(':');
            t.push_str(tipo);
        }
        if let Some(c) = &self.categoria {
            t.push_str("|categoria=");
            t.push_str(c);
        }
        if let Some((de, ate)) = &self.ciclos {
            t.push_str(&format!("|ciclos={de}..{ate}"));
        }
        if let Some(f) = &self.filtro
            && let Ok(json) = serde_json::to_string(f)
        {
            t.push_str("|filtro=");
            t.push_str(&json);
        }
        t
    }
}

// ---------------------------------------------------------------------------
// Rótulos de itens
// ---------------------------------------------------------------------------

/// O texto de um valor JSON de `decisao.antes` / `depois`, sem as aspas.
pub fn texto_do_json(json: &Option<String>) -> String {
    match json {
        None => "nada".to_owned(),
        Some(t) => match serde_json::from_str::<serde_json::Value>(t) {
            Ok(serde_json::Value::String(s)) => s,
            Ok(serde_json::Value::Null) => "nada".to_owned(),
            Ok(outro) => outro.to_string(),
            Err(_) => t.clone(),
        },
    }
}

/// O nome de uma decisão: `estado: Em dúvida → Documento`.
pub fn rotulo_da_decisao(campo: &str, antes: &Option<String>, depois: &Option<String>) -> String {
    format!(
        "{campo}: {} → {}",
        texto_do_json(antes),
        texto_do_json(depois)
    )
}

fn cortar(texto: &str, n: usize) -> String {
    if texto.chars().count() <= n {
        texto.to_owned()
    } else {
        let corte: String = texto.chars().take(n).collect();
        format!("{corte}…")
    }
}

fn primeiro_texto(
    ctx: &Contexto<'_>,
    sql: &str,
    chave: Value,
) -> ResultadoDeConsulta<Option<String>> {
    let mut lidas = consultar(ctx.conn, sql, &[chave], |l| l.get::<_, Option<String>>(0))?;
    Ok(lidas.pop().flatten())
}

/// O nome de um item para link, trilha e título. `None` quando o item não existe.
pub fn rotulo_do_item(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<Option<String>> {
    let chave = alvo.chave.as_str();
    match alvo.tipo {
        Tipo::Achado | Tipo::Documento => {
            let existe = vistas::escalar(
                ctx.conn,
                "SELECT count(*) FROM achado WHERE content_hash = ?",
                &[texto(chave)],
            )?;
            if existe == 0 {
                return Ok(None);
            }
            let nome = primeiro_texto(
                ctx,
                "SELECT filename FROM origem WHERE content_hash = ? ORDER BY id LIMIT 1",
                texto(chave),
            )?;
            Ok(Some(rotulo_do_achado(&nome, chave)))
        }
        Tipo::Registro => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            let linhas = consultar(
                ctx.conn,
                "SELECT descricao_original FROM registro WHERE id = ?",
                &[inteiro(id)],
                |l| l.get::<_, Option<String>>(0),
            )?;
            Ok(linhas.first().map(|d| rotulo_do_registro(id, d)))
        }
        Tipo::Fornecedor => {
            let n = vistas::escalar(
                ctx.conn,
                &format!(
                    "SELECT count(*) FROM registro r WHERE {} = ?",
                    ctx.sql_fornecedor_do_registro("r.")
                ),
                &[texto(chave)],
            )? + vistas::escalar(
                ctx.conn,
                "SELECT count(*) FROM achado WHERE supplier_name_raw = ?",
                &[texto(chave)],
            )? + vistas::escalar(
                ctx.conn,
                "SELECT count(*) FROM regra_senha WHERE fornecedor = ?",
                &[texto(chave)],
            )?;
            Ok((n > 0).then(|| chave.to_owned()))
        }
        Tipo::Conta => {
            let n = vistas::escalar(
                ctx.conn,
                "SELECT count(*) FROM registro WHERE conta_provavel = ?",
                &[texto(chave)],
            )?;
            Ok((n > 0).then(|| chave.to_owned()))
        }
        Tipo::Ciclo => {
            let n = vistas::escalar(
                ctx.conn,
                "SELECT count(*) FROM ciclo WHERE id = ?",
                &[texto(chave)],
            )?;
            Ok((n > 0).then(|| format!("Ciclo {chave}")))
        }
        Tipo::Fatura => {
            let linhas = vistas::faturas_conferencia(
                ctx,
                &Selecao::nova().onde("f.fatura = ?", vec![texto(chave)]),
            )?;
            Ok(linhas
                .first()
                .map(|f| vistas::rotulo_da_fatura(&f.conta, &f.vencimento)))
        }
        Tipo::Origem => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            primeiro_texto(ctx, "SELECT filename FROM origem WHERE id = ?", inteiro(id))
        }
        Tipo::Decisao => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            let linhas = consultar(
                ctx.conn,
                "SELECT campo, antes, depois FROM decisao WHERE id = ?",
                &[inteiro(id)],
                |l| Ok((l.get::<_, String>(0)?, l.get(1)?, l.get(2)?)),
            )?;
            Ok(linhas.first().map(|(c, a, d)| rotulo_da_decisao(c, a, d)))
        }
        Tipo::Lote => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            primeiro_texto(ctx, "SELECT nome FROM lote WHERE id = ?", inteiro(id))
        }
        Tipo::Pendencia => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            Ok(primeiro_texto(
                ctx,
                "SELECT pergunta FROM pendencia WHERE id = ?",
                inteiro(id),
            )?
            .map(|p| cortar(&p, 60)))
        }
        Tipo::RegraDeSenha => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            let linhas = consultar(
                ctx.conn,
                "SELECT fornecedor, document_type, estrategia FROM regra_senha WHERE id = ?",
                &[inteiro(id)],
                |l| {
                    Ok((
                        l.get::<_, String>(0)?,
                        l.get::<_, String>(1)?,
                        l.get::<_, String>(2)?,
                    ))
                },
            )?;
            Ok(linhas.first().map(|(f, t, e)| rotulo_da_regra(f, t, e)))
        }
        Tipo::Conexao => primeiro_texto(
            ctx,
            "SELECT descricao FROM fonte WHERE id = ?",
            texto(chave),
        ),
        Tipo::Coleta => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            let linhas = consultar(
                ctx.conn,
                "SELECT etapa, fonte_id FROM varredura WHERE id = ?",
                &[inteiro(id)],
                |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
            )?;
            Ok(linhas.first().map(|(e, f)| rotulo_da_coleta(e, f, id)))
        }
        _ => Ok(None),
    }
}

/// O nome de uma regra de senha: `Banco Alfa · fatura · cpf_primeiros:5`.
pub fn rotulo_da_regra(fornecedor: &str, document_type: &str, estrategia: &str) -> String {
    let tipo = if document_type == "*" {
        "qualquer tipo"
    } else {
        document_type
    };
    format!("{fornecedor} · {tipo} · {estrategia}")
}

/// Um rótulo que não falha: usa a chave quando o item não existe.
pub fn rotulo_ou_chave(ctx: &Contexto<'_>, alvo: &Ref) -> ResultadoDeConsulta<String> {
    Ok(rotulo_do_item(ctx, alvo)?.unwrap_or_else(|| alvo.chave.clone()))
}

/// O `Tipo` de `decisao.alvo_tipo`.
pub fn tipo_do_alvo_da_decisao(alvo_tipo: &str) -> Option<Tipo> {
    match alvo_tipo {
        "achado" => Some(Tipo::Achado),
        "registro" => Some(Tipo::Registro),
        "fornecedor" => Some(Tipo::Fornecedor),
        "lote" => Some(Tipo::Lote),
        "pendencia" => Some(Tipo::Pendencia),
        _ => None,
    }
}

/// O item que uma pendência trava, lido de `pendencia.referencia`: `achado:<hash>`,
/// `registro:<id>`… ou um hash puro (achado) / número puro (registro).
pub fn alvo_da_pendencia(ctx: &Contexto<'_>, id: i64) -> ResultadoDeConsulta<Option<Ref>> {
    let referencia = primeiro_texto(
        ctx,
        "SELECT referencia FROM pendencia WHERE id = ?",
        inteiro(id),
    )?;
    Ok(referencia.and_then(|r| ref_da_referencia(&r)))
}

/// Lê o texto de `pendencia.referencia`.
pub fn ref_da_referencia(referencia: &str) -> Option<Ref> {
    if referencia.is_empty() {
        return None;
    }
    if let Ok(r) = referencia.parse::<Ref>() {
        return Some(r);
    }
    if referencia.len() == 64 && referencia.chars().all(|c| c.is_ascii_hexdigit()) {
        return Some(Ref::nova(Tipo::Achado, referencia));
    }
    if referencia.chars().all(|c| c.is_ascii_digit()) {
        return Some(Ref::nova(Tipo::Registro, referencia));
    }
    None
}

/// A decisão: de qual item é e qual item foi decidido.
pub fn alvo_da_decisao(ctx: &Contexto<'_>, id: i64) -> ResultadoDeConsulta<Option<Ref>> {
    let linhas = consultar(
        ctx.conn,
        "SELECT alvo_tipo, alvo_id FROM decisao WHERE id = ?",
        &[inteiro(id)],
        |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
    )?;
    Ok(linhas
        .first()
        .and_then(|(t, a)| tipo_do_alvo_da_decisao(t).map(|t| Ref::nova(t, a.as_str()))))
}

// ---------------------------------------------------------------------------
// Origens
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeOrigemCompleta {
    pub id: i64,
    pub hash: Option<String>,
    pub fonte_id: String,
    pub varredura_id: i64,
    pub etapa: Option<String>,
    pub tipo_de_origem: String,
    pub nome: String,
    pub onde: Option<String>,
    pub visto_em: String,
}

pub fn origens(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDeOrigemCompleta>> {
    let sql = format!(
        "SELECT o.id, o.content_hash, o.fonte_id, o.varredura_id, v.etapa, o.origin_type, o.filename,
           coalesce(o.local_filepath, o.gmail_subject, o.mensagem_ref), o.visto_em
         FROM origem o LEFT JOIN varredura v ON v.id = o.varredura_id
         WHERE 1 = 1{sel} ORDER BY o.visto_em DESC, o.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeOrigemCompleta {
            id: l.get(0)?,
            hash: l.get(1)?,
            fonte_id: l.get(2)?,
            varredura_id: l.get(3)?,
            etapa: l.get(4)?,
            tipo_de_origem: l.get(5)?,
            nome: l.get(6)?,
            onde: l.get(7)?,
            visto_em: l.get(8)?,
        })
    })
}

pub fn tabela_origens(linhas: &[LinhaDeOrigemCompleta]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Origem"),
            coluna("Achado"),
            coluna("Conexão"),
            coluna("Coleta"),
            coluna("Tipo de origem"),
            coluna("Onde"),
            coluna("Vista em"),
        ],
        linhas: linhas
            .iter()
            .map(|l| Linha {
                alvo: Some(Ref::nova(Tipo::Origem, l.id.to_string())),
                celulas: vec![
                    c_link(Tipo::Origem, &l.id.to_string(), &l.nome),
                    l.hash.as_deref().map_or(Celula::Vazia, |h| {
                        c_link(Tipo::Achado, h, &format!("Achado {}", hash_curto(h)))
                    }),
                    c_link(Tipo::Conexao, &l.fonte_id, &l.fonte_id),
                    c_link(
                        Tipo::Coleta,
                        &l.varredura_id.to_string(),
                        &rotulo_da_coleta(
                            l.etapa.as_deref().unwrap_or("?"),
                            &l.fonte_id,
                            l.varredura_id,
                        ),
                    ),
                    c_texto(&l.tipo_de_origem),
                    c_texto_opt(&l.onde),
                    Celula::Data(l.visto_em.clone()),
                ],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Decisões
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeDecisao {
    pub id: i64,
    pub alvo_tipo: String,
    pub alvo_id: String,
    pub campo: String,
    pub antes: Option<String>,
    pub depois: Option<String>,
    pub autor: String,
    pub motivo: Option<String>,
    pub em: String,
    pub desfaz: Option<i64>,
    /// A decisão que desfez esta, se houver.
    pub desfeita_por: Option<i64>,
}

pub fn decisoes(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeDecisao>> {
    let sql = format!(
        "SELECT d.id, d.alvo_tipo, d.alvo_id, d.campo, d.antes, d.depois, d.autor, d.motivo, d.em, d.desfaz,
           (SELECT u.id FROM decisao u WHERE u.desfaz = d.id ORDER BY u.id LIMIT 1)
         FROM decisao d WHERE 1 = 1{sel} ORDER BY d.em DESC, d.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeDecisao {
            id: l.get(0)?,
            alvo_tipo: l.get(1)?,
            alvo_id: l.get(2)?,
            campo: l.get(3)?,
            antes: l.get(4)?,
            depois: l.get(5)?,
            autor: l.get(6)?,
            motivo: l.get(7)?,
            em: l.get(8)?,
            desfaz: l.get(9)?,
            desfeita_por: l.get(10)?,
        })
    })
}

pub fn tabela_decisoes(
    ctx: &Contexto<'_>,
    linhas: &[LinhaDeDecisao],
) -> ResultadoDeConsulta<Tabela> {
    let mut saida = Tabela {
        colunas: vec![
            coluna("Decisão"),
            coluna("Alvo"),
            coluna("Situação"),
            coluna("Quando"),
            coluna("Motivo"),
            coluna("Desfaz"),
            coluna("Desfeita por"),
        ],
        linhas: Vec::with_capacity(linhas.len()),
    };
    for l in linhas {
        let alvo = tipo_do_alvo_da_decisao(&l.alvo_tipo).map(|t| Ref::nova(t, l.alvo_id.as_str()));
        let celula_alvo = match &alvo {
            Some(a) => Celula::Link {
                alvo: a.clone(),
                texto: rotulo_ou_chave(ctx, a)?,
            },
            None => c_texto(&l.alvo_id),
        };
        let situacao = if l.desfeita_por.is_some() {
            "Desfeita"
        } else if l.desfaz.is_some() {
            "Desfaz outra"
        } else {
            "Em vigor"
        };
        let id = l.id.to_string();
        saida.linhas.push(Linha {
            alvo: Some(Ref::nova(Tipo::Decisao, id.as_str())),
            celulas: vec![
                c_link(
                    Tipo::Decisao,
                    &id,
                    &rotulo_da_decisao(&l.campo, &l.antes, &l.depois),
                ),
                celula_alvo,
                Celula::Estado {
                    rotulo: situacao.to_owned(),
                    quem: Some(Quem::de_texto(&l.autor)),
                },
                Celula::Data(l.em.clone()),
                c_texto_opt(&l.motivo),
                l.desfaz.map_or(Celula::Vazia, |d| {
                    c_link(Tipo::Decisao, &d.to_string(), &format!("Decisão {d}"))
                }),
                l.desfeita_por.map_or(Celula::Vazia, |d| {
                    c_link(Tipo::Decisao, &d.to_string(), &format!("Decisão {d}"))
                }),
            ],
        });
    }
    Ok(saida)
}

// ---------------------------------------------------------------------------
// Lotes
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeLote {
    pub id: i64,
    pub nome: String,
    pub criado_em: String,
    pub aprovado_em: Option<String>,
    pub promovido_em: Option<String>,
    pub achados: i64,
    pub registros: i64,
    pub valor_dos_registros: Centavos,
}

impl LinhaDeLote {
    /// `Aberto`, `Aprovado` ou `Promovido`.
    pub fn situacao(&self) -> &'static str {
        if self.promovido_em.is_some() {
            "Promovido"
        } else if self.aprovado_em.is_some() {
            "Aprovado"
        } else {
            "Aberto"
        }
    }
}

pub fn lotes(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeLote>> {
    let sql = format!(
        "SELECT l.id, l.nome, l.criado_em, l.aprovado_em, l.promovido_em,
           (SELECT count(*) FROM lote_item i WHERE i.lote_id = l.id AND i.alvo_tipo = 'achado'),
           (SELECT count(*) FROM lote_item i WHERE i.lote_id = l.id AND i.alvo_tipo = 'registro'),
           (SELECT coalesce(sum(r.valor_centavos), 0) FROM lote_item i JOIN registro r ON r.id = CAST(i.alvo_id AS INTEGER)
              WHERE i.lote_id = l.id AND i.alvo_tipo = 'registro')
         FROM lote l WHERE 1 = 1{sel} ORDER BY l.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeLote {
            id: l.get(0)?,
            nome: l.get(1)?,
            criado_em: l.get(2)?,
            aprovado_em: l.get(3)?,
            promovido_em: l.get(4)?,
            achados: l.get(5)?,
            registros: l.get(6)?,
            valor_dos_registros: l.get(7)?,
        })
    })
}

pub fn tabela_lotes(linhas: &[LinhaDeLote]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Lote"),
            coluna("Situação"),
            coluna("Criado em"),
            coluna("Aprovado em"),
            coluna("Promovido em"),
            coluna_num("Achados"),
            coluna_num("Registros"),
            coluna_num("Valor dos registros"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let de = Ref::nova(Tipo::Lote, l.id.to_string());
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        c_link(Tipo::Lote, &l.id.to_string(), &l.nome),
                        Celula::Estado {
                            rotulo: l.situacao().to_owned(),
                            quem: Some(Quem::Usuario),
                        },
                        Celula::Data(l.criado_em.clone()),
                        c_data_opt(&l.aprovado_em),
                        c_data_opt(&l.promovido_em),
                        c_contagem(l.achados, de.clone(), None, recorte::ACHADOS),
                        c_contagem(l.registros, de.clone(), None, recorte::REGISTROS),
                        c_valor_total(l.valor_dos_registros, de, None, recorte::REGISTROS),
                    ],
                }
            })
            .collect(),
    }
}

/// Os achados e registros de um lote, misturados, com o tipo de cada um.
pub fn tabela_dos_itens_do_lote(ctx: &Contexto<'_>, lote: i64) -> ResultadoDeConsulta<Tabela> {
    let itens = consultar(
        ctx.conn,
        "SELECT alvo_tipo, alvo_id FROM lote_item WHERE lote_id = ? ORDER BY alvo_tipo, CAST(alvo_id AS INTEGER), alvo_id",
        &[inteiro(lote)],
        |l| Ok((l.get::<_, String>(0)?, l.get::<_, String>(1)?)),
    )?;
    let mut tabela = Tabela {
        colunas: vec![coluna("Item"), coluna("Tipo"), coluna("Estado")],
        linhas: Vec::new(),
    };
    for (tipo, id) in itens {
        let Some(t) = tipo_do_alvo_da_decisao(&tipo) else {
            continue;
        };
        let alvo = Ref::nova(t, id.as_str());
        let estado = match t {
            Tipo::Achado => primeiro_texto(
                ctx,
                "SELECT estado FROM achado WHERE content_hash = ?",
                texto(id.as_str()),
            )?
            .map(|e| c_estado_achado(&e, &None)),
            Tipo::Registro => id_numerico(&id)
                .map(|n| {
                    primeiro_texto(ctx, "SELECT estado FROM registro WHERE id = ?", inteiro(n))
                })
                .transpose()?
                .flatten()
                .map(|e| Celula::Estado {
                    rotulo: vistas::rotulo_do_estado_do_registro(&e).to_owned(),
                    quem: None,
                }),
            _ => None,
        };
        tabela.linhas.push(Linha {
            alvo: Some(alvo.clone()),
            celulas: vec![
                Celula::Link {
                    texto: rotulo_ou_chave(ctx, &alvo)?,
                    alvo,
                },
                c_texto(t.singular()),
                estado.unwrap_or(Celula::Vazia),
            ],
        });
    }
    Ok(tabela)
}

// ---------------------------------------------------------------------------
// Pendências
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDePendencia {
    pub id: i64,
    pub etapa: String,
    pub tipo: String,
    pub referencia: Option<String>,
    pub pergunta: String,
    pub criada_em: String,
    pub resolvida_em: Option<String>,
    pub resposta: Option<String>,
}

pub fn pendencias(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDePendencia>> {
    let sql = format!(
        "SELECT p.id, p.etapa, p.tipo, p.referencia, p.pergunta, p.criada_em, p.resolvida_em, p.resposta
         FROM pendencia p WHERE 1 = 1{sel}
         ORDER BY (p.resolvida_em IS NOT NULL), p.criada_em DESC, p.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDePendencia {
            id: l.get(0)?,
            etapa: l.get(1)?,
            tipo: l.get(2)?,
            referencia: l.get(3)?,
            pergunta: l.get(4)?,
            criada_em: l.get(5)?,
            resolvida_em: l.get(6)?,
            resposta: l.get(7)?,
        })
    })
}

pub fn tabela_pendencias(
    ctx: &Contexto<'_>,
    linhas: &[LinhaDePendencia],
) -> ResultadoDeConsulta<Tabela> {
    let mut tabela = Tabela {
        colunas: vec![
            coluna("Pendência"),
            coluna("Situação"),
            coluna("Item travado"),
            coluna("Etapa"),
            coluna("Tipo"),
            coluna("Criada em"),
            coluna("Respondida em"),
            coluna("Resposta"),
        ],
        linhas: Vec::with_capacity(linhas.len()),
    };
    for l in linhas {
        let travado = match l.referencia.as_deref().and_then(ref_da_referencia) {
            Some(r) => Celula::Link {
                texto: rotulo_ou_chave(ctx, &r)?,
                alvo: r,
            },
            None => c_texto_opt(&l.referencia),
        };
        let id = l.id.to_string();
        tabela.linhas.push(Linha {
            alvo: Some(Ref::nova(Tipo::Pendencia, id.as_str())),
            celulas: vec![
                c_link(Tipo::Pendencia, &id, &cortar(&l.pergunta, 80)),
                Celula::Estado {
                    rotulo: if l.resolvida_em.is_some() {
                        "Respondida"
                    } else {
                        "Aberta"
                    }
                    .to_owned(),
                    quem: l.resolvida_em.as_ref().map(|_| Quem::Usuario),
                },
                travado,
                c_texto(&l.etapa),
                c_texto(&l.tipo),
                Celula::Data(l.criada_em.clone()),
                c_data_opt(&l.resolvida_em),
                c_texto_opt(&l.resposta),
            ],
        });
    }
    Ok(tabela)
}

// ---------------------------------------------------------------------------
// Regras de senha e tentativas (nunca a senha)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeRegra {
    pub id: i64,
    pub fornecedor: String,
    pub document_type: String,
    pub estrategia: String,
    pub prioridade: i64,
    pub ativa: bool,
    pub sucessos: i64,
    pub falhas: i64,
    pub ultimo_sucesso_em: Option<String>,
    pub ultima_falha_em: Option<String>,
}

pub fn regras_de_senha(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDeRegra>> {
    let sql = format!(
        "SELECT s.id, s.fornecedor, s.document_type, s.estrategia, s.prioridade, s.ativa, s.sucessos, s.falhas,
           s.ultimo_sucesso_em, s.ultima_falha_em
         FROM regra_senha s WHERE 1 = 1{sel} ORDER BY s.fornecedor, s.prioridade, s.id{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeRegra {
            id: l.get(0)?,
            fornecedor: l.get(1)?,
            document_type: l.get(2)?,
            estrategia: l.get(3)?,
            prioridade: l.get(4)?,
            ativa: l.get::<_, i64>(5)? != 0,
            sucessos: l.get(6)?,
            falhas: l.get(7)?,
            ultimo_sucesso_em: l.get(8)?,
            ultima_falha_em: l.get(9)?,
        })
    })
}

pub fn tabela_regras_de_senha(linhas: &[LinhaDeRegra]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Regra"),
            coluna("Fornecedor"),
            coluna("Tipo de documento"),
            coluna("Estratégia"),
            coluna_num("Prioridade"),
            coluna("Situação"),
            coluna_num("Sucessos"),
            coluna_num("Falhas"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let id = l.id.to_string();
                Linha {
                    alvo: Some(Ref::nova(Tipo::RegraDeSenha, id.as_str())),
                    celulas: vec![
                        c_link(
                            Tipo::RegraDeSenha,
                            &id,
                            &rotulo_da_regra(&l.fornecedor, &l.document_type, &l.estrategia),
                        ),
                        c_link(Tipo::Fornecedor, &l.fornecedor, &l.fornecedor),
                        c_texto(&l.document_type),
                        c_texto(&l.estrategia),
                        c_contagem_solta(l.prioridade),
                        Celula::Estado {
                            rotulo: if l.ativa { "Ativa" } else { "Desligada" }.to_owned(),
                            quem: None,
                        },
                        c_contagem_solta(l.sucessos),
                        c_contagem_solta(l.falhas),
                    ],
                }
            })
            .collect(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeTentativa {
    pub id: i64,
    pub hash: String,
    pub nome: Option<String>,
    pub estrategia: String,
    pub sucesso: bool,
    pub em: String,
}

pub fn tentativas(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeTentativa>> {
    let sql = format!(
        "SELECT t.id, t.content_hash,
           (SELECT filename FROM origem o WHERE o.content_hash = t.content_hash ORDER BY o.id LIMIT 1),
           t.estrategia, t.sucesso, t.em
         FROM tentativa_senha t JOIN achado a ON a.content_hash = t.content_hash
         WHERE 1 = 1{sel} ORDER BY t.em DESC, t.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeTentativa {
            id: l.get(0)?,
            hash: l.get(1)?,
            nome: l.get(2)?,
            estrategia: l.get(3)?,
            sucesso: l.get::<_, i64>(4)? != 0,
            em: l.get(5)?,
        })
    })
}

pub fn tabela_tentativas(linhas: &[LinhaDeTentativa]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Achado"),
            coluna("Estratégia"),
            coluna("Resultado"),
            coluna("Quando"),
        ],
        linhas: linhas
            .iter()
            .map(|l| Linha {
                alvo: Some(Ref::nova(Tipo::Achado, l.hash.as_str())),
                celulas: vec![
                    c_link(Tipo::Achado, &l.hash, &rotulo_do_achado(&l.nome, &l.hash)),
                    c_texto(&l.estrategia),
                    Celula::Estado {
                        rotulo: if l.sucesso { "Abriu" } else { "Falhou" }.to_owned(),
                        quem: None,
                    },
                    Celula::Data(l.em.clone()),
                ],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Conexões (as fontes do inventário)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeConexao {
    pub id: String,
    pub tipo: String,
    pub descricao: String,
    pub escopo: String,
    pub motivo: Option<String>,
    pub situacao: Option<String>,
    pub origens: i64,
    pub coletas: i64,
    pub registros: i64,
}

pub fn conexoes(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeConexao>> {
    let sql = format!(
        "SELECT f.id, f.tipo, f.descricao, f.escopo, f.motivo,
           (SELECT status FROM varredura w WHERE w.fonte_id = f.id ORDER BY (w.status = 'rodando') DESC, w.id DESC LIMIT 1),
           (SELECT count(*) FROM origem o WHERE o.fonte_id = f.id),
           (SELECT count(*) FROM varredura w WHERE w.fonte_id = f.id),
           (SELECT count(*) FROM registro r WHERE r.fonte_id = f.id)
         FROM fonte f WHERE 1 = 1{sel} ORDER BY f.escopo, f.id{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeConexao {
            id: l.get(0)?,
            tipo: l.get(1)?,
            descricao: l.get(2)?,
            escopo: l.get(3)?,
            motivo: l.get(4)?,
            situacao: l.get(5)?,
            origens: l.get(6)?,
            coletas: l.get(7)?,
            registros: l.get(8)?,
        })
    })
}

pub fn tabela_conexoes(linhas: &[LinhaDeConexao]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Conexão"),
            coluna("Tipo"),
            coluna("Escopo"),
            coluna("Situação"),
            coluna_num("Origens"),
            coluna_num("Coletas"),
            coluna_num("Registros"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let de = Ref::nova(Tipo::Conexao, l.id.as_str());
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        c_link(Tipo::Conexao, &l.id, &l.descricao),
                        c_texto(&l.tipo),
                        c_texto(&l.escopo),
                        Celula::Texto(if l.escopo == "fora" {
                            "fora".to_owned()
                        } else {
                            l.situacao.clone().unwrap_or_else(|| "ausente".to_owned())
                        }),
                        c_contagem(l.origens, de.clone(), None, recorte::ORIGENS),
                        c_contagem(l.coletas, de.clone(), None, recorte::COLETAS),
                        c_contagem(l.registros, de, None, recorte::REGISTROS),
                    ],
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// Listas de nomes (fornecedor, conta, ciclo, fatura)
// ---------------------------------------------------------------------------

/// Uma tabela de fornecedores a partir dos nomes, com os totais de cada um.
pub fn tabela_de_fornecedores_por_nome(
    ctx: &Contexto<'_>,
    nomes: &[String],
) -> ResultadoDeConsulta<Tabela> {
    let mut tabela = Tabela {
        colunas: vec![
            coluna("Fornecedor"),
            coluna_num("Registros · a conferir"),
            coluna_num("Saiu"),
            coluna_num("Documentos"),
        ],
        linhas: Vec::new(),
    };
    let expr = ctx.sql_fornecedor_do_registro("r.");
    for nome in nomes {
        let de = Ref::nova(Tipo::Fornecedor, nome.as_str());
        let registros = vistas::escalar(
            ctx.conn,
            &format!("SELECT count(*) FROM registro r WHERE {expr} = ?"),
            &[texto(nome.as_str())],
        )?;
        let saiu = vistas::escalar(
            ctx.conn,
            &format!(
                "SELECT -coalesce(sum(r.valor_centavos), 0) FROM registro r WHERE {expr} = ? AND {}",
                movimento::sql_e_gasto("r.")
            ),
            &[texto(nome.as_str())],
        )?;
        let documentos = vistas::escalar(
            ctx.conn,
            "SELECT count(*) FROM achado WHERE supplier_name_raw = ? AND estado = 'documento'",
            &[texto(nome.as_str())],
        )?;
        tabela.linhas.push(Linha {
            alvo: Some(de.clone()),
            celulas: vec![
                c_link(Tipo::Fornecedor, nome, nome),
                c_contagem(registros, de.clone(), None, recorte::REGISTROS),
                c_valor_total(saiu, de.clone(), None, recorte::SAIDAS),
                c_contagem(documentos, de, None, recorte::DOCUMENTOS),
            ],
        });
    }
    Ok(tabela)
}

/// Uma tabela de contas a partir dos nomes.
pub fn tabela_de_contas_por_nome(
    ctx: &Contexto<'_>,
    nomes: &[String],
) -> ResultadoDeConsulta<Tabela> {
    let mut tabela = Tabela {
        colunas: vec![
            coluna("Conta"),
            coluna_num("Registros · a conferir"),
            coluna_num("Saiu"),
        ],
        linhas: Vec::new(),
    };
    for nome in nomes {
        let de = Ref::nova(Tipo::Conta, nome.as_str());
        let registros = vistas::escalar(
            ctx.conn,
            "SELECT count(*) FROM registro WHERE conta_provavel = ?",
            &[texto(nome.as_str())],
        )?;
        let saiu = vistas::escalar(
            ctx.conn,
            &format!(
                "SELECT -coalesce(sum(r.valor_centavos), 0) FROM registro r WHERE r.conta_provavel = ? AND {}",
                movimento::sql_e_gasto("r.")
            ),
            &[texto(nome.as_str())],
        )?;
        tabela.linhas.push(Linha {
            alvo: Some(de.clone()),
            celulas: vec![
                c_link(Tipo::Conta, nome, nome),
                c_contagem(registros, de.clone(), None, recorte::REGISTROS),
                c_valor_total(saiu, de, None, recorte::SAIDAS),
            ],
        });
    }
    Ok(tabela)
}

/// Uma tabela de ciclos a partir dos ids.
pub fn tabela_de_ciclos_por_id(ctx: &Contexto<'_>, ids: &[String]) -> ResultadoDeConsulta<Tabela> {
    let mut tabela = Tabela {
        colunas: vec![
            coluna("Ciclo"),
            coluna("Período"),
            coluna_num("Registros · a conferir"),
            coluna_num("Saiu"),
        ],
        linhas: Vec::new(),
    };
    for id in ids {
        let de = Ref::nova(Tipo::Ciclo, id.as_str());
        let registros = vistas::escalar(
            ctx.conn,
            "SELECT count(*) FROM registro WHERE ciclo_id = ?",
            &[texto(id.as_str())],
        )?;
        let saiu = vistas::escalar(
            ctx.conn,
            &format!(
                "SELECT -coalesce(sum(r.valor_centavos), 0) FROM registro r WHERE r.ciclo_id = ? AND {}",
                movimento::sql_e_gasto("r.")
            ),
            &[texto(id.as_str())],
        )?;
        let periodo = format!(
            "{} a {}",
            vistas::inicio_do_ciclo(id),
            vistas::fim_do_ciclo(id)
        );
        tabela.linhas.push(Linha {
            alvo: Some(de.clone()),
            celulas: vec![
                c_link(Tipo::Ciclo, id, &format!("Ciclo {id}")),
                Celula::Texto(periodo),
                c_contagem(registros, de.clone(), Some(id), recorte::REGISTROS),
                c_valor_total(saiu, de, Some(id), recorte::SAIDAS),
            ],
        });
    }
    Ok(tabela)
}

/// Uma tabela de faturas a partir dos ids.
pub fn tabela_de_faturas_por_id(ctx: &Contexto<'_>, ids: &[String]) -> ResultadoDeConsulta<Tabela> {
    if ids.is_empty() {
        return Ok(Tabela::default());
    }
    let sel = Selecao::nova().onde(
        format!("f.fatura IN ({})", vec!["?"; ids.len()].join(",")),
        ids.iter().map(|i| texto(i.as_str())).collect(),
    );
    Ok(vistas::tabela_faturas(&vistas::faturas_conferencia(
        ctx, &sel,
    )?))
}

// ---------------------------------------------------------------------------
// As seleções: o que está ligado a quê
// ---------------------------------------------------------------------------

/// `*` na chave significa "todos os itens do tipo".
fn todos(de: &Ref) -> bool {
    de.chave == "*"
}

/// A seleção dos registros ligados a `de` (alias `r`). `None` quando o par não faz sentido.
pub fn selecao_de_registros(ctx: &Contexto<'_>, de: &Ref) -> ResultadoDeConsulta<Option<Selecao>> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Ok(Some(Selecao::nova()));
    }
    let s = Selecao::nova();
    Ok(match normalizar(de.tipo) {
        Tipo::Fornecedor => Some(s.onde(
            format!("{} = ?", ctx.sql_fornecedor_do_registro("r.")),
            vec![texto(chave)],
        )),
        Tipo::Ciclo => Some(s.onde("r.ciclo_id = ?", vec![texto(chave)])),
        Tipo::Conta => Some(s.onde("r.conta_provavel = ?", vec![texto(chave)])),
        Tipo::Fatura => Some(s.onde(
            format!(
                "{} = ? OR r.external_ref = 'pluggy:bill:' || ?",
                ctx.dialeto.fatura_ref("r.")
            ),
            vec![texto(chave), texto(chave)],
        )),
        Tipo::Achado => Some(s.onde("r.content_hash = ?", vec![texto(chave)])),
        Tipo::Conexao => Some(s.onde("r.fonte_id = ?", vec![texto(chave)])),
        Tipo::Origem => id_numerico(chave).map(|id| {
            s.onde(
                "r.content_hash = (SELECT content_hash FROM origem WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Coleta => id_numerico(chave).map(|id| {
            s.onde(
                "r.content_hash IN (SELECT content_hash FROM origem WHERE varredura_id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Lote => id_numerico(chave).map(|id| {
            s.onde(
                "r.id IN (SELECT CAST(alvo_id AS INTEGER) FROM lote_item WHERE lote_id = ? AND alvo_tipo = 'registro')",
                vec![inteiro(id)],
            )
        }),
        Tipo::Registro => id_numerico(chave).map(|id| s.onde("r.id = ?", vec![inteiro(id)])),
        _ => None,
    })
}

/// A seleção dos achados ligados a `de` (alias `a`).
pub fn selecao_de_achados(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Some(Selecao::nova());
    }
    let s = Selecao::nova();
    match normalizar(de.tipo) {
        Tipo::Fornecedor => Some(s.onde("a.supplier_name_raw = ?", vec![texto(chave)])),
        Tipo::Ciclo => Some(s.onde("a.ciclo_id = ?", vec![texto(chave)])),
        Tipo::Origem => id_numerico(chave).map(|id| {
            s.onde(
                "a.content_hash = (SELECT content_hash FROM origem WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Conexao => Some(s.onde(
            "a.content_hash IN (SELECT content_hash FROM origem WHERE fonte_id = ?)",
            vec![texto(chave)],
        )),
        Tipo::Coleta => id_numerico(chave).map(|id| {
            s.onde(
                "a.content_hash IN (SELECT content_hash FROM origem WHERE varredura_id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Registro => id_numerico(chave).map(|id| {
            s.onde(
                "a.content_hash = (SELECT content_hash FROM registro WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Lote => id_numerico(chave).map(|id| {
            s.onde(
                "a.content_hash IN (SELECT alvo_id FROM lote_item WHERE lote_id = ? AND alvo_tipo = 'achado')",
                vec![inteiro(id)],
            )
        }),
        // Relações entre achados: possível duplicidade, anexo de, mesma fatura.
        Tipo::Achado => Some(s.onde(
            "a.content_hash IN (SELECT b FROM relacao WHERE a = ? UNION SELECT a FROM relacao WHERE b = ?)",
            vec![texto(chave), texto(chave)],
        )),
        _ => None,
    }
}

/// A seleção das origens ligadas a `de` (alias `o`).
pub fn selecao_de_origens(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Some(Selecao::nova());
    }
    let s = Selecao::nova();
    match normalizar(de.tipo) {
        Tipo::Achado => Some(s.onde("o.content_hash = ?", vec![texto(chave)])),
        Tipo::Conexao => Some(s.onde("o.fonte_id = ?", vec![texto(chave)])),
        Tipo::Coleta => id_numerico(chave).map(|id| s.onde("o.varredura_id = ?", vec![inteiro(id)])),
        Tipo::Registro => id_numerico(chave).map(|id| {
            s.onde(
                "o.content_hash = (SELECT content_hash FROM registro WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Fornecedor => Some(s.onde(
            "o.content_hash IN (SELECT content_hash FROM achado WHERE supplier_name_raw = ?)",
            vec![texto(chave)],
        )),
        Tipo::Ciclo => Some(s.onde(
            "o.content_hash IN (SELECT content_hash FROM achado WHERE ciclo_id = ?)",
            vec![texto(chave)],
        )),
        Tipo::Lote => id_numerico(chave).map(|id| {
            s.onde(
                "o.content_hash IN (SELECT alvo_id FROM lote_item WHERE lote_id = ? AND alvo_tipo = 'achado')",
                vec![inteiro(id)],
            )
        }),
        _ => None,
    }
}

/// A seleção das decisões ligadas a `de` (alias `d`).
pub fn selecao_de_decisoes(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Some(Selecao::nova());
    }
    let s = Selecao::nova();
    let do_alvo = |alvo_tipo: &str| {
        s.clone().onde(
            "d.alvo_tipo = ? AND d.alvo_id = ?",
            vec![texto(alvo_tipo), texto(chave)],
        )
    };
    match normalizar(de.tipo) {
        Tipo::Achado => Some(do_alvo("achado")),
        Tipo::Registro => Some(do_alvo("registro")),
        Tipo::Fornecedor => Some(do_alvo("fornecedor")),
        Tipo::Lote => Some(do_alvo("lote")),
        Tipo::Pendencia => Some(do_alvo("pendencia")),
        Tipo::Decisao => id_numerico(chave).map(|id| {
            s.onde(
                "d.desfaz = ? OR d.id = (SELECT desfaz FROM decisao WHERE id = ?)",
                vec![inteiro(id), inteiro(id)],
            )
        }),
        _ => None,
    }
}

/// A seleção dos lotes ligados a `de` (alias `l`).
pub fn selecao_de_lotes(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Some(Selecao::nova());
    }
    let s = Selecao::nova();
    match normalizar(de.tipo) {
        Tipo::Achado => Some(s.onde(
            "l.id IN (SELECT lote_id FROM lote_item WHERE alvo_tipo = 'achado' AND alvo_id = ?)",
            vec![texto(chave)],
        )),
        Tipo::Registro => Some(s.onde(
            "l.id IN (SELECT lote_id FROM lote_item WHERE alvo_tipo = 'registro' AND alvo_id = ?)",
            vec![texto(chave)],
        )),
        _ => None,
    }
}

/// A seleção das pendências ligadas a `de` (alias `p`): as que travam o item.
pub fn selecao_de_pendencias(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Some(Selecao::nova());
    }
    let s = Selecao::nova();
    let por_referencia = |prefixos: &[&str]| {
        let mut possiveis: Vec<String> = prefixos.iter().map(|p| format!("{p}:{chave}")).collect();
        if matches!(normalizar(de.tipo), Tipo::Achado | Tipo::Registro) {
            possiveis.push(chave.to_owned());
        }
        let marcas = vec!["?"; possiveis.len()].join(",");
        s.clone().onde(
            format!("p.referencia IN ({marcas})"),
            possiveis.into_iter().map(Value::Text).collect(),
        )
    };
    match normalizar(de.tipo) {
        Tipo::Achado => Some(por_referencia(&["achado", "documento"])),
        Tipo::Registro => Some(por_referencia(&["registro"])),
        Tipo::Lote => Some(por_referencia(&["lote"])),
        Tipo::Fornecedor => Some(por_referencia(&["fornecedor"])),
        _ => None,
    }
}

/// A seleção das coletas ligadas a `de` (alias `v`).
pub fn selecao_de_coletas(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Some(Selecao::nova());
    }
    let s = Selecao::nova();
    match normalizar(de.tipo) {
        Tipo::Conexao => Some(s.onde("v.fonte_id = ?", vec![texto(chave)])),
        Tipo::Origem => id_numerico(chave).map(|id| {
            s.onde(
                "v.id = (SELECT varredura_id FROM origem WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Achado => Some(s.onde(
            "v.id IN (SELECT varredura_id FROM origem WHERE content_hash = ?)",
            vec![texto(chave)],
        )),
        _ => None,
    }
}

/// A seleção das conexões ligadas a `de` (alias `f`).
pub fn selecao_de_conexoes(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    if todos(de) {
        return Some(Selecao::nova());
    }
    let s = Selecao::nova();
    match normalizar(de.tipo) {
        Tipo::Origem => id_numerico(chave).map(|id| {
            s.onde(
                "f.id = (SELECT fonte_id FROM origem WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Coleta => id_numerico(chave).map(|id| {
            s.onde(
                "f.id = (SELECT fonte_id FROM varredura WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        Tipo::Achado => Some(s.onde(
            "f.id IN (SELECT fonte_id FROM origem WHERE content_hash = ?)",
            vec![texto(chave)],
        )),
        Tipo::Registro => id_numerico(chave).map(|id| {
            s.onde(
                "f.id = (SELECT fonte_id FROM registro WHERE id = ?)",
                vec![inteiro(id)],
            )
        }),
        _ => None,
    }
}

/// A seleção das regras de senha ligadas a `de` (alias `s`).
pub fn selecao_de_regras(de: &Ref) -> Option<Selecao> {
    let chave = de.chave.as_str();
    let s = Selecao::nova();
    match normalizar(de.tipo) {
        Tipo::Fornecedor => Some(s.onde("s.fornecedor = ?", vec![texto(chave)])),
        Tipo::Achado => Some(s.onde(
            "EXISTS (SELECT 1 FROM achado a WHERE a.content_hash = ? AND a.supplier_name_raw = s.fornecedor
               AND (s.document_type = '*' OR s.document_type = a.document_type))",
            vec![texto(chave)],
        )),
        _ => None,
    }
}

/// Os nomes (fornecedor, conta, ciclo, fatura) ligados a `de`.
pub fn nomes_ligados(
    ctx: &Contexto<'_>,
    de: &Ref,
    tipo: Tipo,
) -> ResultadoDeConsulta<Option<Vec<String>>> {
    let chave = de.chave.as_str();
    let de_tipo = normalizar(de.tipo);
    let (sql, params): (String, Vec<Value>) = match (de_tipo, tipo) {
        (Tipo::Achado, Tipo::Fornecedor) => (
            "SELECT supplier_name_raw FROM achado WHERE content_hash = ? AND supplier_name_raw <> ''".to_owned(),
            vec![texto(chave)],
        ),
        (Tipo::Achado, Tipo::Ciclo) => (
            "SELECT ciclo_id FROM achado WHERE content_hash = ? AND ciclo_id IS NOT NULL".to_owned(),
            vec![texto(chave)],
        ),
        (Tipo::Registro, Tipo::Fornecedor) => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            (
                format!(
                    "SELECT {} FROM registro r WHERE r.id = ? AND {} IS NOT NULL",
                    ctx.sql_fornecedor_do_registro("r."),
                    ctx.sql_fornecedor_do_registro("r.")
                ),
                vec![inteiro(id)],
            )
        }
        (Tipo::Registro, Tipo::Conta) => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            (
                "SELECT conta_provavel FROM registro WHERE id = ? AND conta_provavel IS NOT NULL".to_owned(),
                vec![inteiro(id)],
            )
        }
        (Tipo::Registro, Tipo::Ciclo) => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            (
                "SELECT ciclo_id FROM registro WHERE id = ? AND ciclo_id IS NOT NULL".to_owned(),
                vec![inteiro(id)],
            )
        }
        (Tipo::Registro, Tipo::Fatura) => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            (
                format!(
                    "SELECT coalesce({fr}, CASE WHEN tipo = 'fatura_cartao' THEN substr(external_ref, 13) END)
                       FROM registro WHERE id = ?
                        AND coalesce({fr}, CASE WHEN tipo = 'fatura_cartao' THEN substr(external_ref, 13) END) IS NOT NULL",
                    fr = ctx.dialeto.fatura_ref("")
                ),
                vec![inteiro(id)],
            )
        }
        (Tipo::Conta, Tipo::Fatura) => (
            "SELECT substr(external_ref, 13) FROM registro WHERE tipo = 'fatura_cartao' AND conta_provavel = ? ORDER BY data".to_owned(),
            vec![texto(chave)],
        ),
        (Tipo::RegraDeSenha, Tipo::Fornecedor) => {
            let Some(id) = id_numerico(chave) else {
                return Ok(None);
            };
            ("SELECT fornecedor FROM regra_senha WHERE id = ?".to_owned(), vec![inteiro(id)])
        }
        _ => return Ok(None),
    };
    Ok(Some(consultar(ctx.conn, &sql, &params, |l| {
        l.get::<_, String>(0)
    })?))
}

// ---------------------------------------------------------------------------
// Ligados
// ---------------------------------------------------------------------------

fn tabela_vazia() -> Tabela {
    Tabela::default()
}

fn tabela_de_achados_ligados(
    ctx: &Contexto<'_>,
    sel: Selecao,
    como: Tipo,
) -> ResultadoDeConsulta<Tabela> {
    if como == Tipo::Documento {
        Ok(vistas::tabela_de_achados(
            &vistas::documentos(ctx, &sel)?,
            LinkDoAchado::Documento,
        ))
    } else {
        Ok(vistas::tabela_de_achados(
            &vistas::achados(ctx, &sel)?,
            LinkDoAchado::Achado,
        ))
    }
}

/// O próprio item, na forma de tabela do tipo `como` (para "pendência → alvo" e
/// "decisão → alvo").
fn tabela_do_proprio(ctx: &Contexto<'_>, alvo: &Ref, como: Tipo) -> ResultadoDeConsulta<Tabela> {
    if normalizar(alvo.tipo) != normalizar(como) {
        return Ok(tabela_vazia());
    }
    let chave = alvo.chave.as_str();
    match normalizar(como) {
        Tipo::Achado => tabela_de_achados_ligados(
            ctx,
            Selecao::nova().onde("a.content_hash = ?", vec![texto(chave)]),
            como,
        ),
        Tipo::Registro => match id_numerico(chave) {
            Some(id) => Ok(vistas::tabela_registros(&vistas::registros(
                ctx,
                &Selecao::nova().onde("r.id = ?", vec![inteiro(id)]),
            )?)),
            None => Ok(tabela_vazia()),
        },
        Tipo::Lote => match id_numerico(chave) {
            Some(id) => Ok(tabela_lotes(&lotes(
                ctx,
                &Selecao::nova().onde("l.id = ?", vec![inteiro(id)]),
            )?)),
            None => Ok(tabela_vazia()),
        },
        Tipo::Pendencia => match id_numerico(chave) {
            Some(id) => tabela_pendencias(
                ctx,
                &pendencias(ctx, &Selecao::nova().onde("p.id = ?", vec![inteiro(id)]))?,
            ),
            None => Ok(tabela_vazia()),
        },
        Tipo::Fornecedor => tabela_de_fornecedores_por_nome(ctx, &[chave.to_owned()]),
        _ => Ok(tabela_vazia()),
    }
}

/// As abas de uma página: os itens de `tipo` ligados a `de`, em qualquer direção.
pub fn tabela_ligados(ctx: &Contexto<'_>, de: &Ref, tipo: Tipo) -> ResultadoDeConsulta<Tabela> {
    if let Some(erro) = fora_da_fatia_1(de.tipo).or_else(|| fora_da_fatia_1(tipo)) {
        return Err(erro);
    }
    let de_tipo = normalizar(de.tipo);

    // Pendência e decisão apontam para um alvo.
    if de_tipo == Tipo::Pendencia {
        if let Some(id) = id_numerico(&de.chave) {
            if tipo == Tipo::Decisao {
                let sel = selecao_de_decisoes(&Ref::nova(Tipo::Pendencia, de.chave.as_str()))
                    .unwrap_or_default();
                return tabela_decisoes(ctx, &decisoes(ctx, &sel)?);
            }
            return match alvo_da_pendencia(ctx, id)? {
                Some(alvo) => tabela_do_proprio(ctx, &alvo, tipo),
                None => Ok(tabela_vazia()),
            };
        }
        return Ok(tabela_vazia());
    }
    if de_tipo == Tipo::Decisao {
        let Some(id) = id_numerico(&de.chave) else {
            return Ok(tabela_vazia());
        };
        if tipo == Tipo::Decisao {
            let sel = selecao_de_decisoes(de).unwrap_or_default();
            return tabela_decisoes(ctx, &decisoes(ctx, &sel)?);
        }
        return match alvo_da_decisao(ctx, id)? {
            Some(alvo) => tabela_do_proprio(ctx, &alvo, tipo),
            None => Ok(tabela_vazia()),
        };
    }
    // Regra de senha: as tentativas (achados) e o fornecedor.
    if de_tipo == Tipo::RegraDeSenha {
        let Some(id) = id_numerico(&de.chave) else {
            return Ok(tabela_vazia());
        };
        return match normalizar(tipo) {
            Tipo::Achado => {
                let sel = Selecao::nova().onde(
                    "EXISTS (SELECT 1 FROM regra_senha s WHERE s.id = ? AND s.fornecedor = a.supplier_name_raw AND (s.document_type = '*' OR s.document_type = a.document_type))",
                    vec![inteiro(id)],
                );
                Ok(tabela_tentativas(&tentativas(ctx, &sel)?))
            }
            Tipo::Fornecedor => {
                let nomes = nomes_ligados(ctx, de, Tipo::Fornecedor)?.unwrap_or_default();
                tabela_de_fornecedores_por_nome(ctx, &nomes)
            }
            _ => Ok(tabela_vazia()),
        };
    }

    match tipo {
        Tipo::Registro if de_tipo == Tipo::Registro => {
            // Unir conciliados: registros que descrevem o mesmo fato.
            let Some(id) = id_numerico(&de.chave) else {
                return Ok(tabela_vazia());
            };
            let sel = Selecao::nova().onde(
                "r.id IN (SELECT b FROM relacao_registro WHERE a = ? UNION SELECT a FROM relacao_registro WHERE b = ?)",
                vec![inteiro(id), inteiro(id)],
            );
            Ok(vistas::tabela_registros(&vistas::registros(ctx, &sel)?))
        }
        Tipo::Registro => match selecao_de_registros(ctx, de)? {
            Some(sel) if de_tipo != Tipo::Registro => {
                Ok(vistas::tabela_registros(&vistas::registros(ctx, &sel)?))
            }
            _ => Ok(tabela_vazia()),
        },
        Tipo::Achado | Tipo::Documento => {
            // Achado → documento não faz sentido; achado → achado são as relações.
            if de_tipo == Tipo::Achado && tipo == Tipo::Documento {
                return Ok(tabela_vazia());
            }
            match selecao_de_achados(de) {
                Some(sel) => tabela_de_achados_ligados(ctx, sel, tipo),
                None => Ok(tabela_vazia()),
            }
        }
        Tipo::Origem => match selecao_de_origens(de) {
            Some(sel) => Ok(tabela_origens(&origens(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        Tipo::Decisao => match selecao_de_decisoes(de) {
            Some(sel) => tabela_decisoes(ctx, &decisoes(ctx, &sel)?),
            None => Ok(tabela_vazia()),
        },
        Tipo::Lote => match selecao_de_lotes(de) {
            Some(sel) => Ok(tabela_lotes(&lotes(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        Tipo::Pendencia => match selecao_de_pendencias(de) {
            Some(sel) => tabela_pendencias(ctx, &pendencias(ctx, &sel)?),
            None => Ok(tabela_vazia()),
        },
        Tipo::Coleta => match selecao_de_coletas(de) {
            Some(sel) => Ok(vistas::tabela_varreduras(&vistas::varreduras(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        Tipo::Conexao => match selecao_de_conexoes(de) {
            Some(sel) => Ok(tabela_conexoes(&conexoes(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        Tipo::RegraDeSenha => match selecao_de_regras(de) {
            Some(sel) => Ok(tabela_regras_de_senha(&regras_de_senha(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        Tipo::Fornecedor | Tipo::Conta | Tipo::Ciclo | Tipo::Fatura => {
            match nomes_ligados(ctx, de, tipo)? {
                Some(nomes) => match tipo {
                    Tipo::Fornecedor => tabela_de_fornecedores_por_nome(ctx, &nomes),
                    Tipo::Conta => tabela_de_contas_por_nome(ctx, &nomes),
                    Tipo::Ciclo => tabela_de_ciclos_por_id(ctx, &nomes),
                    _ => tabela_de_faturas_por_id(ctx, &nomes),
                },
                None => Ok(tabela_vazia()),
            }
        }
        _ => Ok(tabela_vazia()),
    }
}

// ---------------------------------------------------------------------------
// ItensDoTotal
// ---------------------------------------------------------------------------

/// A categoria de um registro, com a categoria vazia chamada `Sem categoria`.
pub const SQL_CATEGORIA_DO_REGISTRO: &str =
    "coalesce(nullif(r.categoria_provavel, ''), 'Sem categoria')";

fn clausulas_de_recorte(ctx: &Contexto<'_>, rec: &Recorte, ciclo: Option<&str>) -> Selecao {
    let mut s = Selecao::nova();
    match rec.base.as_str() {
        recorte::SAIDAS => s = s.onde(movimento::sql_e_gasto("r."), vec![]),
        recorte::ENTRADAS => s = s.onde(movimento::sql_e_entrada("r."), vec![]),
        recorte::MOVIDO => s = s.onde(movimento::sql_e_saida_de_movimento("r."), vec![]),
        recorte::TRANSACOES => {
            s = s.onde(
                "r.tipo NOT IN ('fatura_cartao', 'pagamento_fatura')".to_owned(),
                vec![],
            );
        }
        recorte::FATURA => s = s.onde("r.tipo = 'fatura_cartao'", vec![]),
        recorte::NAO_CLASSIFICADOS => {
            s = s.onde(movimento::sql_nao_e_movimento("r."), vec![]).onde(
                format!(
                    "coalesce(nullif(r.fornecedor_normalizado, ''), nullif({}, ''), nullif(r.supplier_name_raw, '')) IS NULL
                     OR {SQL_CATEGORIA_DO_REGISTRO} = 'Sem categoria'",
                    ctx.dialeto.contraparte("r.")
                ),
                vec![],
            );
        }
        recorte::NAO_APROVADOS => {
            s = s.onde(
                "r.estado IN ('extraido', 'conferido', 'divergente')",
                vec![],
            );
        }
        _ => {}
    }
    if let Some(t) = &rec.tipo {
        s = s.onde("r.tipo = ?", vec![texto(t.as_str())]);
    }
    if let Some(c) = &rec.categoria {
        s = s.onde(
            format!("{SQL_CATEGORIA_DO_REGISTRO} = ?"),
            vec![texto(c.as_str())],
        );
    }
    if let Some((de, ate)) = &rec.ciclos {
        s = s.onde(
            "r.ciclo_id BETWEEN ? AND ?",
            vec![texto(de.as_str()), texto(ate.as_str())],
        );
    }
    if let Some(f) = &rec.filtro {
        s = s.junta(vistas::selecao_do_filtro(ctx, f, vistas::Alvo::Registro));
    }
    if let Some(c) = ciclo {
        s = s.onde("r.ciclo_id = ?", vec![texto(c)]);
    }
    s
}

/// Os itens que somam (ou contam) um número. Ver [`recorte`].
pub fn itens_do_total(
    ctx: &Contexto<'_>,
    de: &Ref,
    fonte: Fonte,
    ciclo: Option<&str>,
    recorte_pedido: Option<&str>,
) -> ResultadoDeConsulta<Tabela> {
    if fonte == Fonte::LivroCaixa {
        return Err(ErroDeConsulta::NaoSuportado(
            Tipo::Movimentacao
                .motivo_da_fatia()
                .unwrap_or_else(|| "Chega na fatia 2, com o livro-caixa".to_owned()),
        ));
    }
    if let Some(erro) = fora_da_fatia_1(de.tipo) {
        return Err(erro);
    }
    let rec = Recorte::ler(recorte_pedido);
    match rec.base.as_str() {
        recorte::SAIDAS
        | recorte::ENTRADAS
        | recorte::MOVIDO
        | recorte::REGISTROS
        | recorte::TRANSACOES
        | recorte::FATURA
        | recorte::NAO_CLASSIFICADOS
        | recorte::NAO_APROVADOS => {
            let Some(base) = selecao_de_registros(ctx, de)? else {
                return Ok(tabela_vazia());
            };
            let sel = base.junta(clausulas_de_recorte(ctx, &rec, ciclo));
            Ok(vistas::tabela_registros(&vistas::registros(ctx, &sel)?))
        }
        recorte::DOCUMENTOS
        | recorte::ACHADOS
        | recorte::DUVIDAS
        | recorte::A_TRIAR
        | recorte::COPIADOS => {
            let Some(mut sel) = selecao_de_achados(de) else {
                return Ok(tabela_vazia());
            };
            sel = match rec.base.as_str() {
                recorte::DOCUMENTOS => sel.onde("a.estado = 'documento'", vec![]),
                recorte::DUVIDAS => sel.onde("a.estado = 'duvida'", vec![]),
                recorte::A_TRIAR => sel.onde("a.estado = 'novo'", vec![]),
                recorte::COPIADOS => sel.onde("a.caminho_acervo IS NOT NULL", vec![]),
                _ => sel,
            };
            if let Some(c) = ciclo {
                sel = sel.onde("a.ciclo_id = ?", vec![texto(c)]);
            }
            let como = if rec.base == recorte::DOCUMENTOS {
                LinkDoAchado::Documento
            } else {
                LinkDoAchado::Achado
            };
            Ok(vistas::tabela_de_achados(
                &vistas::achados(ctx, &sel)?,
                como,
            ))
        }
        recorte::ORIGENS => match selecao_de_origens(de) {
            Some(sel) => Ok(tabela_origens(&origens(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        recorte::COLETAS => match selecao_de_coletas(de) {
            Some(sel) => Ok(vistas::tabela_varreduras(&vistas::varreduras(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        recorte::DECISOES => match selecao_de_decisoes(de) {
            Some(sel) => tabela_decisoes(ctx, &decisoes(ctx, &sel)?),
            None => Ok(tabela_vazia()),
        },
        recorte::ABERTAS => {
            let sel = selecao_de_pendencias(de)
                .unwrap_or_default()
                .onde("p.resolvida_em IS NULL", vec![]);
            tabela_pendencias(ctx, &pendencias(ctx, &sel)?)
        }
        recorte::TENTATIVAS => tabela_ligados(ctx, de, Tipo::Achado),
        recorte::REGRAS => match selecao_de_regras(de) {
            Some(sel) => Ok(tabela_regras_de_senha(&regras_de_senha(ctx, &sel)?)),
            None => Ok(tabela_vazia()),
        },
        recorte::ITENS => match (normalizar(de.tipo), id_numerico(&de.chave)) {
            (Tipo::Lote, Some(id)) => tabela_dos_itens_do_lote(ctx, id),
            _ => Ok(tabela_vazia()),
        },
        outro => Err(ErroDeConsulta::NaoSuportado(format!(
            "recorte desconhecido: {outro}"
        ))),
    }
}

// ---------------------------------------------------------------------------
// Total por ciclo
// ---------------------------------------------------------------------------

/// O total de `alvo` (o que saiu, positivo) nos últimos `ciclos` ciclos até `ciclo_atual`,
/// do mais antigo ao mais novo. Ciclos sem registro valem 0. Cada total abre os itens que
/// o somam.
pub fn por_ciclo_de(
    ctx: &Contexto<'_>,
    alvo: &Ref,
    fonte: Fonte,
    ciclos: usize,
    ciclo_atual: &str,
) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
    if fonte == Fonte::LivroCaixa {
        return Err(ErroDeConsulta::NaoSuportado(
            Tipo::Movimentacao
                .motivo_da_fatia()
                .unwrap_or_else(|| "Chega na fatia 2, com o livro-caixa".to_owned()),
        ));
    }
    if let Some(erro) = fora_da_fatia_1(alvo.tipo) {
        return Err(erro);
    }
    let Some(base) = selecao_de_registros(ctx, alvo)? else {
        return Ok(Vec::new());
    };
    if ciclos == 0 {
        return Ok(Vec::new());
    }
    // Os últimos `ciclos` ciclos conhecidos, até o atual.
    let mut ids: Vec<String> = consultar(
        ctx.conn,
        "SELECT id FROM ciclo WHERE id <= ? ORDER BY id DESC LIMIT ?",
        &[
            texto(ciclo_atual),
            inteiro(i64::try_from(ciclos).unwrap_or(i64::MAX)),
        ],
        |l| l.get::<_, String>(0),
    )?;
    ids.reverse();
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let sel = base.onde(movimento::sql_e_gasto("r."), vec![]).onde(
        format!("r.ciclo_id IN ({})", vec!["?"; ids.len()].join(",")),
        ids.iter().map(|i| texto(i.as_str())).collect(),
    );
    let sql = format!(
        "SELECT r.ciclo_id, -sum(r.valor_centavos) FROM registro r WHERE 1 = 1{} GROUP BY 1",
        sel.sql_e()
    );
    let somas = consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok((l.get::<_, String>(0)?, l.get::<_, Centavos>(1)?))
    })?;
    Ok(ids
        .into_iter()
        .map(|c| {
            let centavos = somas.iter().find(|(id, _)| *id == c).map_or(0, |(_, v)| *v);
            TotalDoCiclo {
                itens: Some(vistas::pedido_total(
                    alvo.clone(),
                    Some(&c),
                    recorte::SAIDAS,
                )),
                ciclo: c,
                centavos,
            }
        })
        .collect())
}

/// O pedido que a janela manda para abrir os itens de um total.
pub fn pedido_dos_itens(de: Ref, ciclo: Option<&str>, recorte_pedido: &str) -> PedidoDeTabela {
    PedidoDeTabela::ItensDoTotal {
        de,
        fonte: Fonte::Inventario,
        ciclo: ciclo.map(str::to_owned),
        recorte: Some(recorte_pedido.to_owned()),
    }
}

/// Usado por quem monta cabeçalhos: o link do estado de um achado sem decisor.
pub fn estado_sem_quem(estado: &str) -> Celula {
    c_estado_achado(estado, &None)
}

/// Usado por quem monta cabeçalhos: o decisor de um texto `decidido_por`.
pub fn quem_do_texto(decidido_por: &Option<String>) -> Option<Quem> {
    quem_de(decidido_por)
}

/// Link para um nome opcional (reexportado para o leitor).
pub fn link_opcional(tipo: Tipo, nome: &Option<String>) -> Celula {
    c_link_opt(tipo, nome)
}

/// Valor sem itens (reexportado para o leitor).
pub fn valor_simples(centavos: Centavos) -> Celula {
    c_valor(centavos)
}

// ---------------------------------------------------------------------------
// Totais para os cabeçalhos
// ---------------------------------------------------------------------------

/// A seleção dos registros que um número do cabeçalho soma (`de`, `recorte`, `ciclo`).
pub fn selecao_dos_registros_do_total(
    ctx: &Contexto<'_>,
    de: &Ref,
    recorte_pedido: &str,
    ciclo: Option<&str>,
) -> ResultadoDeConsulta<Option<Selecao>> {
    let rec = Recorte::ler(Some(recorte_pedido));
    Ok(selecao_de_registros(ctx, de)?
        .map(|base| base.junta(clausulas_de_recorte(ctx, &rec, ciclo))))
}

/// Quantos registros a seleção tem e quanto somam (com sinal).
pub fn contar_e_somar(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<(i64, Centavos)> {
    let linhas = consultar(
        ctx.conn,
        &format!(
            "SELECT count(*), coalesce(sum(r.valor_centavos), 0) FROM registro r WHERE 1 = 1{}",
            sel.sql_e()
        ),
        sel.params(),
        |l| Ok((l.get::<_, i64>(0)?, l.get::<_, Centavos>(1)?)),
    )?;
    Ok(linhas.into_iter().next().unwrap_or((0, 0)))
}

/// Um valor do cabeçalho que abre os itens que o somam. Para `saidas`, `movido`,
/// `transacoes` e `fatura` o valor é a magnitude (positivo); para os outros, a soma com sinal.
pub fn valor_do_cabecalho(
    ctx: &Contexto<'_>,
    de: &Ref,
    recorte_pedido: &str,
    ciclo: Option<&str>,
) -> ResultadoDeConsulta<Celula> {
    let Some(sel) = selecao_dos_registros_do_total(ctx, de, recorte_pedido, ciclo)? else {
        return Ok(Celula::Vazia);
    };
    let (_, soma) = contar_e_somar(ctx, &sel)?;
    let base = Recorte::ler(Some(recorte_pedido)).base;
    let magnitude = matches!(
        base.as_str(),
        recorte::SAIDAS | recorte::MOVIDO | recorte::TRANSACOES | recorte::FATURA
    );
    Ok(c_valor_total(
        if magnitude { -soma } else { soma },
        de.clone(),
        ciclo,
        recorte_pedido,
    ))
}

/// Uma contagem do cabeçalho que abre os registros que ela conta.
pub fn contagem_do_cabecalho(
    ctx: &Contexto<'_>,
    de: &Ref,
    recorte_pedido: &str,
    ciclo: Option<&str>,
) -> ResultadoDeConsulta<Celula> {
    let Some(sel) = selecao_dos_registros_do_total(ctx, de, recorte_pedido, ciclo)? else {
        return Ok(Celula::Vazia);
    };
    let (n, _) = contar_e_somar(ctx, &sel)?;
    Ok(c_contagem(n, de.clone(), ciclo, recorte_pedido))
}
