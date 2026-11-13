//! As 12 consultas do Datasette (`workers/inventario/datasette/metadata.json`) como
//! funções com linhas tipadas, mais a conversão de cada uma para [`Tabela`].
//!
//! - INV-03: cada vista devolve as mesmas linhas que a SQL original sobre o mesmo banco
//!   (o teste `leitura_vistas` roda a SQL do `metadata.json` e compara).
//! - "Movimento entre contas" vem de [`crate::movimento`]: nunca conta como gasto.
//! - Todo nome vira [`Celula::Link`] e todo número vira [`Celula::Valor`] ou
//!   [`Celula::Contagem`] com o [`PedidoDeTabela::ItensDoTotal`] que o soma.
//! - O Datasette limitava as linhas (`LIMIT 1000`); a janela virtualiza, então as áreas
//!   pedem sem limite. As funções `*_datasette` reproduzem os parâmetros e o limite
//!   originais.
//!
//! Este módulo também guarda a infraestrutura comum da leitura: [`Contexto`],
//! [`Selecao`] e os filtros da aba Início ([`selecao_do_filtro`]).
//!
//! Dono: agente de leitura.

use rusqlite::types::Value;
use rusqlite::{Connection, Row};

use crate::consulta::{
    Area, Celula, Centavos, Coluna, ErroDeConsulta, Filtro, Fonte, Linha, PedidoDeTabela, Quem,
    ResultadoDeConsulta, Tabela,
};
use crate::movimento;
use crate::referencia::{Ref, Tipo};

// ---------------------------------------------------------------------------
// Infraestrutura
// ---------------------------------------------------------------------------

/// Quais colunas opcionais o banco aberto tem.
///
/// A migração 1 congela o esquema do Bun "2", mas `aplicarSchema` do Bun acrescenta
/// `contraparte`, `data_compra` e `fatura_ref` em `registro` (`garantirColuna`). Um banco
/// que ainda não passou por elas lê `NULL` no lugar, em vez de falhar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dialeto {
    pub contraparte: bool,
    pub data_compra: bool,
    pub fatura_ref: bool,
}

impl Dialeto {
    /// Todas as colunas presentes (o banco do Bun em uso).
    pub const COMPLETO: Dialeto = Dialeto {
        contraparte: true,
        data_compra: true,
        fatura_ref: true,
    };

    pub fn detectar(conn: &Connection) -> ResultadoDeConsulta<Dialeto> {
        let mut instrucao = conn.prepare("PRAGMA table_info(registro)")?;
        let nomes: Vec<String> = instrucao
            .query_map([], |l| l.get::<_, String>(1))?
            .collect::<Result<_, _>>()?;
        let tem = |n: &str| nomes.iter().any(|c| c == n);
        Ok(Dialeto {
            contraparte: tem("contraparte"),
            data_compra: tem("data_compra"),
            fatura_ref: tem("fatura_ref"),
        })
    }

    fn coluna(presente: bool, alias: &str, nome: &str) -> String {
        if presente {
            format!("{alias}{nome}")
        } else {
            "NULL".to_owned()
        }
    }

    /// `r.contraparte`, ou `NULL` se o banco não tem a coluna. `alias` com ponto (`"r."`).
    pub fn contraparte(&self, alias: &str) -> String {
        Self::coluna(self.contraparte, alias, "contraparte")
    }
    pub fn data_compra(&self, alias: &str) -> String {
        Self::coluna(self.data_compra, alias, "data_compra")
    }
    pub fn fatura_ref(&self, alias: &str) -> String {
        Self::coluna(self.fatura_ref, alias, "fatura_ref")
    }
}

/// A conexão de leitura e o que se sabe do banco.
#[derive(Debug, Clone, Copy)]
pub struct Contexto<'a> {
    pub conn: &'a Connection,
    pub dialeto: Dialeto,
}

impl<'a> Contexto<'a> {
    pub fn novo(conn: &'a Connection) -> ResultadoDeConsulta<Self> {
        Ok(Self {
            conn,
            dialeto: Dialeto::detectar(conn)?,
        })
    }

    /// O nome do fornecedor de um registro, como a vista `fornecedores` o agrupa.
    /// É a chave de `fornecedor:<nome>` para registros. `alias` com ponto.
    pub fn sql_fornecedor_do_registro(&self, alias: &str) -> String {
        format!(
            "coalesce({a}fornecedor_normalizado, {c}, nullif({a}supplier_name_raw, ''), {a}descricao_original)",
            a = alias,
            c = self.dialeto.contraparte(alias)
        )
    }
}

/// Um valor de parâmetro de texto.
pub fn texto(valor: impl Into<String>) -> Value {
    Value::Text(valor.into())
}

/// Condições (`AND`) e parâmetros posicionais (`?`) que restringem uma listagem, mais o
/// limite de linhas. A ordem dos `?` nas cláusulas é a ordem dos parâmetros.
#[derive(Debug, Clone, Default)]
pub struct Selecao {
    clausulas: Vec<String>,
    params: Vec<Value>,
    pub limite: Option<usize>,
}

impl Selecao {
    pub fn nova() -> Self {
        Self::default()
    }

    /// Acrescenta uma condição. Use `?` para cada parâmetro, na ordem.
    #[must_use]
    pub fn onde(mut self, clausula: impl Into<String>, params: Vec<Value>) -> Self {
        self.clausulas.push(clausula.into());
        self.params.extend(params);
        self
    }

    #[must_use]
    pub fn com_limite(mut self, n: usize) -> Self {
        self.limite = Some(n);
        self
    }

    /// `" AND (c1) AND (c2)"`, ou vazio.
    pub fn sql_e(&self) -> String {
        self.clausulas
            .iter()
            .map(|c| format!(" AND ({c})"))
            .collect()
    }

    /// `" LIMIT n"`, ou vazio.
    pub fn sql_limite(&self) -> String {
        self.limite
            .map(|n| format!(" LIMIT {n}"))
            .unwrap_or_default()
    }

    pub fn params(&self) -> &[Value] {
        &self.params
    }

    #[must_use]
    pub fn junta(mut self, outra: Selecao) -> Self {
        self.clausulas.extend(outra.clausulas);
        self.params.extend(outra.params);
        if outra.limite.is_some() {
            self.limite = outra.limite;
        }
        self
    }
}

/// Roda `sql` com os parâmetros e mapeia cada linha.
pub fn consultar<T>(
    conn: &Connection,
    sql: &str,
    params: &[Value],
    mapear: impl FnMut(&Row<'_>) -> rusqlite::Result<T>,
) -> ResultadoDeConsulta<Vec<T>> {
    let mut instrucao = conn.prepare_cached(sql)?;
    let linhas = instrucao.query_map(rusqlite::params_from_iter(params.iter()), mapear)?;
    linhas
        .collect::<Result<Vec<T>, _>>()
        .map_err(ErroDeConsulta::from)
}

/// Um número só (contagem, soma). `NULL` vira 0.
pub fn escalar(conn: &Connection, sql: &str, params: &[Value]) -> ResultadoDeConsulta<i64> {
    let mut instrucao = conn.prepare_cached(sql)?;
    let n: Option<i64> =
        instrucao.query_row(rusqlite::params_from_iter(params.iter()), |l| l.get(0))?;
    Ok(n.unwrap_or(0))
}

fn nao_vazio(s: &Option<String>) -> Option<&str> {
    s.as_deref().filter(|t| !t.is_empty())
}

// ---------------------------------------------------------------------------
// Ciclos em texto (`AAAA-MM`)
// ---------------------------------------------------------------------------

/// O mês seguinte de um `AAAA-MM`. Texto que não for `AAAA-MM` volta como veio.
pub fn proximo_mes(ciclo: &str) -> String {
    let (Some(ano), Some(mes)) = (
        ciclo.get(0..4).and_then(|a| a.parse::<i32>().ok()),
        ciclo.get(5..7).and_then(|m| m.parse::<i32>().ok()),
    ) else {
        return ciclo.to_owned();
    };
    if mes >= 12 {
        format!("{:04}-01", ano + 1)
    } else {
        format!("{ano:04}-{:02}", mes + 1)
    }
}

/// O mês anterior de um `AAAA-MM`.
pub fn mes_anterior(ciclo: &str) -> String {
    let (Some(ano), Some(mes)) = (
        ciclo.get(0..4).and_then(|a| a.parse::<i32>().ok()),
        ciclo.get(5..7).and_then(|m| m.parse::<i32>().ok()),
    ) else {
        return ciclo.to_owned();
    };
    if mes <= 1 {
        format!("{:04}-12", ano - 1)
    } else {
        format!("{ano:04}-{:02}", mes - 1)
    }
}

/// O primeiro dia do ciclo (dia de corte 20).
pub fn inicio_do_ciclo(ciclo: &str) -> String {
    format!("{ciclo}-20")
}

/// O último dia do ciclo (véspera do corte no mês seguinte).
pub fn fim_do_ciclo(ciclo: &str) -> String {
    format!("{}-19", proximo_mes(ciclo))
}

// ---------------------------------------------------------------------------
// Os filtros da aba Início
// ---------------------------------------------------------------------------

/// Em qual tabela o filtro vale: muda o nome das colunas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Alvo {
    /// Linhas de `registro` (alias `r`).
    Registro,
    /// Linhas de `achado` (alias `a`).
    Achado,
}

const ESTADOS_DO_REGISTRO: [&str; 5] = [
    "extraido",
    "conferido",
    "divergente",
    "aprovado",
    "promovido",
];

fn texto_do_filtro(filtro: &Filtro) -> Option<&str> {
    filtro
        .texto
        .as_deref()
        .map(str::trim)
        .filter(|t| !t.is_empty())
}

fn marcas(n: usize) -> String {
    vec!["?"; n].join(",")
}

/// Traduz o [`Filtro`] da aba Início em condições SQL.
///
/// - `texto`: pesquisa em descrição, fornecedor, notas, nome do arquivo e tipo;
/// - `ciclos`: do ciclo ao ciclo, inclusive (só linhas com ciclo);
/// - `estados`: estados do achado; em registros só valem os estados de registro;
/// - `tipos`: `document_type` do achado ou `tipo` do registro;
/// - `fontes`: `fonte_id` do registro, ou qualquer origem do achado;
/// - `quem_decidiu`: `decidido_por` (`Automatico("")` = qualquer rodada de regras).
pub fn selecao_do_filtro(ctx: &Contexto<'_>, filtro: &Filtro, alvo: Alvo) -> Selecao {
    let mut s = Selecao::nova();
    let p = match alvo {
        Alvo::Registro => "r.",
        Alvo::Achado => "a.",
    };
    if let Some(t) = texto_do_filtro(filtro) {
        let padrao = Value::Text(format!("%{t}%"));
        s = match alvo {
            Alvo::Registro => s.onde(
                format!(
                    "r.descricao_original LIKE ? OR r.supplier_name_raw LIKE ? OR r.fornecedor_normalizado LIKE ? OR {} LIKE ? OR r.categoria_provavel LIKE ? OR r.conta_provavel LIKE ?",
                    ctx.dialeto.contraparte("r.")
                ),
                vec![padrao; 6],
            ),
            Alvo::Achado => s.onde(
                "a.supplier_name_raw LIKE ? OR a.notas LIKE ? OR a.document_type LIKE ? OR EXISTS (SELECT 1 FROM origem o WHERE o.content_hash = a.content_hash AND (o.filename LIKE ? OR o.local_filepath LIKE ? OR o.gmail_subject LIKE ? OR o.gmail_from LIKE ?))",
                vec![padrao; 7],
            ),
        };
    }
    if let Some((de, ate)) = &filtro.ciclos {
        s = s.onde(
            format!("{p}ciclo_id BETWEEN ? AND ?"),
            vec![texto(de.as_str()), texto(ate.as_str())],
        );
    }
    let estados: Vec<&String> = match alvo {
        Alvo::Achado => filtro.estados.iter().collect(),
        Alvo::Registro => filtro
            .estados
            .iter()
            .filter(|e| ESTADOS_DO_REGISTRO.contains(&e.as_str()))
            .collect(),
    };
    if !estados.is_empty() {
        s = s.onde(
            format!("{p}estado IN ({})", marcas(estados.len())),
            estados.iter().map(|e| texto(e.as_str())).collect(),
        );
    }
    if !filtro.tipos.is_empty() {
        let coluna = match alvo {
            Alvo::Registro => "r.tipo",
            Alvo::Achado => "a.document_type",
        };
        s = s.onde(
            format!("{coluna} IN ({})", marcas(filtro.tipos.len())),
            filtro.tipos.iter().map(|t| texto(t.as_str())).collect(),
        );
    }
    if !filtro.fontes.is_empty() {
        let m = marcas(filtro.fontes.len());
        let params: Vec<Value> = filtro.fontes.iter().map(|t| texto(t.as_str())).collect();
        s = match alvo {
            Alvo::Registro => s.onde(format!("r.fonte_id IN ({m})"), params),
            Alvo::Achado => s.onde(
                format!("EXISTS (SELECT 1 FROM origem o WHERE o.content_hash = a.content_hash AND o.fonte_id IN ({m}))"),
                params,
            ),
        };
    }
    if let Some(quem) = &filtro.quem_decidiu {
        s = match quem {
            Quem::Usuario => s.onde(format!("{p}decidido_por = 'usuario'"), vec![]),
            Quem::Automatico(t) if t.is_empty() => s.onde(
                format!("{p}decidido_por IS NOT NULL AND {p}decidido_por <> 'usuario'"),
                vec![],
            ),
            Quem::Automatico(t) => s.onde(format!("{p}decidido_por = ?"), vec![texto(t.as_str())]),
        };
    }
    s
}

// ---------------------------------------------------------------------------
// Células e colunas
// ---------------------------------------------------------------------------

pub(crate) fn coluna(rotulo: &str) -> Coluna {
    Coluna {
        rotulo: rotulo.to_owned(),
        numerica: false,
    }
}

pub(crate) fn coluna_num(rotulo: &str) -> Coluna {
    Coluna {
        rotulo: rotulo.to_owned(),
        numerica: true,
    }
}

pub(crate) fn c_texto(valor: &str) -> Celula {
    if valor.is_empty() {
        Celula::Vazia
    } else {
        Celula::Texto(valor.to_owned())
    }
}

pub(crate) fn c_texto_opt(valor: &Option<String>) -> Celula {
    nao_vazio(valor).map_or(Celula::Vazia, c_texto)
}

pub(crate) fn c_data_opt(valor: &Option<String>) -> Celula {
    nao_vazio(valor).map_or(Celula::Vazia, |d| Celula::Data(d.to_owned()))
}

pub(crate) fn c_link(tipo: Tipo, chave: &str, texto: &str) -> Celula {
    Celula::Link {
        alvo: Ref::nova(tipo, chave),
        texto: if texto.is_empty() {
            chave.to_owned()
        } else {
            texto.to_owned()
        },
    }
}

/// Link para um nome (fornecedor, conta, ciclo, conexão…) quando há nome; vazio senão.
pub(crate) fn c_link_opt(tipo: Tipo, nome: &Option<String>) -> Celula {
    nao_vazio(nome).map_or(Celula::Vazia, |n| c_link(tipo, n, n))
}

pub(crate) fn c_valor(centavos: Centavos) -> Celula {
    Celula::Valor {
        centavos,
        fonte: Fonte::Inventario,
        itens: None,
    }
}

pub(crate) fn c_valor_opt(centavos: Option<Centavos>) -> Celula {
    centavos.map_or(Celula::Vazia, c_valor)
}

pub(crate) fn pedido_total(de: Ref, ciclo: Option<&str>, recorte: &str) -> Box<PedidoDeTabela> {
    Box::new(PedidoDeTabela::ItensDoTotal {
        de,
        fonte: Fonte::Inventario,
        ciclo: ciclo.map(str::to_owned),
        recorte: Some(recorte.to_owned()),
    })
}

/// Um valor em dinheiro clicável: abre os itens que o somam.
pub(crate) fn c_valor_total(
    centavos: Centavos,
    de: Ref,
    ciclo: Option<&str>,
    recorte: &str,
) -> Celula {
    Celula::Valor {
        centavos,
        fonte: Fonte::Inventario,
        itens: Some(pedido_total(de, ciclo, recorte)),
    }
}

/// Uma contagem clicável: abre o que ela conta.
pub(crate) fn c_contagem(n: i64, de: Ref, ciclo: Option<&str>, recorte: &str) -> Celula {
    Celula::Contagem {
        n,
        itens: Some(pedido_total(de, ciclo, recorte)),
    }
}

pub(crate) fn c_contagem_solta(n: i64) -> Celula {
    Celula::Contagem { n, itens: None }
}

/// O rótulo do estado do achado, como o usuário o vê.
pub fn rotulo_do_estado_do_achado(estado: &str) -> &'static str {
    match estado {
        "novo" => "A triar",
        "documento" => "Documento",
        "descartado" => "Descartado",
        "duvida" => "Em dúvida",
        _ => "Estado desconhecido",
    }
}

/// O rótulo do estado do registro.
pub fn rotulo_do_estado_do_registro(estado: &str) -> &'static str {
    match estado {
        "extraido" => "Extraído",
        "conferido" => "Conferido",
        "divergente" => "Divergente",
        "aprovado" => "Aprovado",
        "promovido" => "Promovido",
        _ => "Estado desconhecido",
    }
}

pub(crate) fn quem_de(decidido_por: &Option<String>) -> Option<Quem> {
    nao_vazio(decidido_por).map(Quem::de_texto)
}

pub(crate) fn c_estado_achado(estado: &str, decidido_por: &Option<String>) -> Celula {
    Celula::Estado {
        rotulo: rotulo_do_estado_do_achado(estado).to_owned(),
        // "A triar" ainda não foi decidido por ninguém.
        quem: if estado == "novo" {
            None
        } else {
            quem_de(decidido_por)
        },
    }
}

pub(crate) fn c_estado_registro(estado: &str, decidido_por: &Option<String>) -> Celula {
    Celula::Estado {
        rotulo: rotulo_do_estado_do_registro(estado).to_owned(),
        quem: quem_de(decidido_por),
    }
}

/// Os 8 primeiros caracteres do hash, para nomear um achado sem nome de arquivo.
pub fn hash_curto(hash: &str) -> &str {
    hash.get(..8).unwrap_or(hash)
}

/// O nome de um registro: a descrição original, ou `Registro N`.
pub fn rotulo_do_registro(id: i64, descricao: &Option<String>) -> String {
    nao_vazio(descricao).map_or_else(|| format!("Registro {id}"), str::to_owned)
}

// ---------------------------------------------------------------------------
// 1. por_ciclo
// ---------------------------------------------------------------------------

/// Uma linha de `por_ciclo`: o que entrou, saiu e foi movido entre contas no ciclo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeCiclo {
    pub ciclo: String,
    /// Positivo: soma das entradas que não são movimento entre contas.
    pub entrou: Centavos,
    /// Positivo: soma dos gastos (saídas que não são movimento entre contas).
    pub saiu: Centavos,
    /// Positivo: soma do que saiu como movimento entre contas.
    pub movido_entre_contas: Centavos,
    pub registros: i64,
}

pub fn por_ciclo(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeCiclo>> {
    let sql = format!(
        "SELECT r.ciclo_id AS ciclo,
           coalesce(sum(CASE WHEN {entrada} THEN r.valor_centavos END), 0),
           -coalesce(sum(CASE WHEN {gasto} THEN r.valor_centavos END), 0),
           -coalesce(sum(CASE WHEN {movido} THEN r.valor_centavos END), 0),
           count(*)
         FROM registro r WHERE r.ciclo_id IS NOT NULL{sel}
         GROUP BY 1 ORDER BY 1 DESC{limite}",
        entrada = movimento::sql_e_entrada("r."),
        gasto = movimento::sql_e_gasto("r."),
        movido = movimento::sql_e_saida_de_movimento("r."),
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeCiclo {
            ciclo: l.get(0)?,
            entrou: l.get(1)?,
            saiu: l.get(2)?,
            movido_entre_contas: l.get(3)?,
            registros: l.get(4)?,
        })
    })
}

/// O ciclo mais antigo que a consulta do Datasette mostra, sem `:desde`.
pub const DESDE_PADRAO_DO_DATASETTE: &str = "2025-10";
/// O último ciclo que `fornecedores` mostra, sem `:ate`.
pub const ATE_PADRAO_DO_DATASETTE: &str = "2026-10";

/// `por_ciclo` com o parâmetro `:desde` do Datasette (vazio = `2025-10`).
pub fn por_ciclo_datasette(
    ctx: &Contexto<'_>,
    desde: &str,
) -> ResultadoDeConsulta<Vec<LinhaDeCiclo>> {
    let sel = Selecao::nova().onde(
        format!("r.ciclo_id >= coalesce(nullif(?, ''), '{DESDE_PADRAO_DO_DATASETTE}')"),
        vec![texto(desde)],
    );
    por_ciclo(ctx, &sel)
}

pub fn tabela_por_ciclo(linhas: &[LinhaDeCiclo]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Ciclo"),
            coluna_num("Entrou"),
            coluna_num("Saiu"),
            coluna_num("Movido entre contas"),
            coluna_num("Registros"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let de = Ref::nova(Tipo::Ciclo, &l.ciclo);
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        c_link(Tipo::Ciclo, &l.ciclo, &l.ciclo),
                        c_valor_total(l.entrou, de.clone(), Some(&l.ciclo), "entradas"),
                        c_valor_total(l.saiu, de.clone(), Some(&l.ciclo), "saidas"),
                        c_valor_total(l.movido_entre_contas, de.clone(), Some(&l.ciclo), "movido"),
                        c_contagem(l.registros, de, Some(&l.ciclo), "registros"),
                    ],
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 2. fornecedores
// ---------------------------------------------------------------------------

/// Uma linha de `fornecedores`: saídas por fornecedor e tipo, sem movimento entre contas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeFornecedor {
    pub fornecedor: Option<String>,
    pub tipo: String,
    pub vezes: i64,
    /// Positivo: o que saiu.
    pub saiu: Centavos,
    pub primeira: String,
    pub ultima: String,
    /// Os ciclos em que houve saída (ordem do SQLite, sem garantia).
    pub ciclos: Vec<String>,
}

pub fn fornecedores(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDeFornecedor>> {
    let nome = ctx.sql_fornecedor_do_registro("r.");
    let sql = format!(
        "SELECT {nome} AS fornecedor, r.tipo, count(*), -sum(r.valor_centavos), min(r.data), max(r.data),
           group_concat(DISTINCT r.ciclo_id)
         FROM registro r
         WHERE r.valor_centavos < 0 AND {nao_mov}{sel}
         GROUP BY 1, 2 ORDER BY sum(r.valor_centavos), 1, 2{limite}",
        nao_mov = movimento::sql_nao_e_movimento("r."),
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        let ciclos: Option<String> = l.get(6)?;
        Ok(LinhaDeFornecedor {
            fornecedor: l.get(0)?,
            tipo: l.get(1)?,
            vezes: l.get(2)?,
            saiu: l.get(3)?,
            primeira: l.get(4)?,
            ultima: l.get(5)?,
            ciclos: ciclos
                .map(|c| c.split(',').map(str::to_owned).collect())
                .unwrap_or_default(),
        })
    })
}

/// `fornecedores` com `:de` e `:ate` do Datasette (vazio = `2025-10` e `2026-10`) e `LIMIT 300`.
pub fn fornecedores_datasette(
    ctx: &Contexto<'_>,
    de: &str,
    ate: &str,
) -> ResultadoDeConsulta<Vec<LinhaDeFornecedor>> {
    let sel = Selecao::nova()
        .onde(
            format!(
                "r.ciclo_id BETWEEN coalesce(nullif(?, ''), '{DESDE_PADRAO_DO_DATASETTE}') AND coalesce(nullif(?, ''), '{ATE_PADRAO_DO_DATASETTE}')"
            ),
            vec![texto(de), texto(ate)],
        )
        .com_limite(300);
    fornecedores(ctx, &sel)
}

pub fn tabela_fornecedores(linhas: &[LinhaDeFornecedor]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Fornecedor"),
            coluna("Tipo"),
            coluna_num("Vezes"),
            coluna_num("Saiu"),
            coluna("Primeira"),
            coluna("Última"),
            coluna("Ciclos"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let alvo = nao_vazio(&l.fornecedor).map(|n| Ref::nova(Tipo::Fornecedor, n));
                let recorte = format!("saidas:{}", l.tipo);
                let (nome, vezes, saiu) = match &alvo {
                    Some(de) => (
                        c_link(Tipo::Fornecedor, &de.chave, &de.chave),
                        c_contagem(l.vezes, de.clone(), None, &recorte),
                        c_valor_total(l.saiu, de.clone(), None, &recorte),
                    ),
                    None => (Celula::Vazia, c_contagem_solta(l.vezes), c_valor(l.saiu)),
                };
                let mut ciclos = l.ciclos.clone();
                ciclos.sort();
                Linha {
                    alvo,
                    celulas: vec![
                        nome,
                        c_texto(&l.tipo),
                        vezes,
                        saiu,
                        Celula::Data(l.primeira.clone()),
                        Celula::Data(l.ultima.clone()),
                        Celula::Links(
                            ciclos
                                .iter()
                                .map(|c| (Ref::nova(Tipo::Ciclo, c), c.clone()))
                                .collect(),
                        ),
                    ],
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 3. registros
// ---------------------------------------------------------------------------

/// Uma linha de `registros`.
#[derive(Debug, Clone, PartialEq)]
pub struct LinhaDeRegistro {
    pub id: i64,
    pub data: String,
    pub ciclo: Option<String>,
    pub valor: Centavos,
    pub tipo: String,
    /// `coalesce(fornecedor_normalizado, contraparte, supplier_name_raw)`, como na SQL original.
    pub fornecedor: Option<String>,
    pub contraparte: Option<String>,
    pub data_compra: Option<String>,
    pub descricao: Option<String>,
    pub categoria: Option<String>,
    pub conta: Option<String>,
    pub estado: String,
    pub fonte: String,
    pub decidido_por: Option<String>,
    pub content_hash: Option<String>,
    pub movimentacao_id: Option<String>,
}

/// O SELECT-base de registros (alias `r`).
pub fn sql_base_de_registros(ctx: &Contexto<'_>) -> String {
    format!(
        "SELECT r.id, r.data, r.ciclo_id, r.valor_centavos, r.tipo,
           coalesce(r.fornecedor_normalizado, {c}, r.supplier_name_raw) AS fornecedor,
           {c} AS contraparte, {dc} AS data_compra, r.descricao_original, r.categoria_provavel,
           r.conta_provavel, r.estado, r.fonte_id, r.decidido_por, r.content_hash, r.movimentacao_id
         FROM registro r WHERE 1 = 1",
        c = ctx.dialeto.contraparte("r."),
        dc = ctx.dialeto.data_compra("r."),
    )
}

pub fn registros(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeRegistro>> {
    let sql = format!(
        "{base}{sel} ORDER BY r.data DESC, r.id DESC{limite}",
        base = sql_base_de_registros(ctx),
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeRegistro {
            id: l.get(0)?,
            data: l.get(1)?,
            ciclo: l.get(2)?,
            valor: l.get(3)?,
            tipo: l.get(4)?,
            fornecedor: l.get(5)?,
            contraparte: l.get(6)?,
            data_compra: l.get(7)?,
            descricao: l.get(8)?,
            categoria: l.get(9)?,
            conta: l.get(10)?,
            estado: l.get(11)?,
            fonte: l.get(12)?,
            decidido_por: l.get(13)?,
            content_hash: l.get(14)?,
            movimentacao_id: l.get(15)?,
        })
    })
}

/// `registros` com `:busca` e `:ciclo` do Datasette e `LIMIT 1000`.
pub fn registros_datasette(
    ctx: &Contexto<'_>,
    busca: &str,
    ciclo: &str,
) -> ResultadoDeConsulta<Vec<LinhaDeRegistro>> {
    let mut sel = Selecao::nova().com_limite(1000);
    if !busca.is_empty() {
        sel = sel.onde(
            "r.descricao_original LIKE '%' || ? || '%' OR r.supplier_name_raw LIKE '%' || ? || '%' OR r.fornecedor_normalizado LIKE '%' || ? || '%'",
            vec![texto(busca), texto(busca), texto(busca)],
        );
    }
    if !ciclo.is_empty() {
        sel = sel.onde("r.ciclo_id = ?", vec![texto(ciclo)]);
    }
    registros(ctx, &sel)
}

pub fn tabela_registros(linhas: &[LinhaDeRegistro]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Data"),
            coluna("Registro"),
            coluna_num("Valor"),
            coluna("Tipo"),
            coluna("Fornecedor"),
            coluna("Contraparte"),
            coluna("Data da compra"),
            coluna("Categoria"),
            coluna("Conta"),
            coluna("Ciclo"),
            coluna("Estado"),
            coluna("Fonte"),
        ],
        linhas: linhas
            .iter()
            .map(|l| Linha {
                alvo: Some(Ref::nova(Tipo::Registro, l.id.to_string())),
                celulas: vec![
                    Celula::Data(l.data.clone()),
                    c_link(
                        Tipo::Registro,
                        &l.id.to_string(),
                        &rotulo_do_registro(l.id, &l.descricao),
                    ),
                    c_valor(l.valor),
                    c_texto(&l.tipo),
                    c_link_opt(Tipo::Fornecedor, &l.fornecedor),
                    c_texto_opt(&l.contraparte),
                    c_data_opt(&l.data_compra),
                    c_texto_opt(&l.categoria),
                    c_link_opt(Tipo::Conta, &l.conta),
                    c_link_opt(Tipo::Ciclo, &l.ciclo),
                    c_estado_registro(&l.estado, &l.decidido_por),
                    c_link(Tipo::Conexao, &l.fonte, &l.fonte),
                ],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 4, 5 e A triar. Achados (documentos, duvidas, novos)
// ---------------------------------------------------------------------------

/// Uma linha de achado, com tudo o que `documentos`, `duvidas` e A triar mostram.
#[derive(Debug, Clone, PartialEq)]
pub struct LinhaDeAchado {
    pub hash: String,
    pub estado: String,
    pub tipo: Option<String>,
    pub extensao: Option<String>,
    pub data_chave: Option<String>,
    pub data_chave_tipo: Option<String>,
    pub ciclo: Option<String>,
    pub valor: Option<Centavos>,
    pub fornecedor: Option<String>,
    /// Nome do arquivo da primeira origem.
    pub nome: Option<String>,
    /// Caminho (ou assunto do e-mail) da primeira origem.
    pub onde: Option<String>,
    pub origens: i64,
    pub no_cofre: Option<String>,
    pub confianca: Option<f64>,
    pub notas: Option<String>,
    pub decidido_por: Option<String>,
}

/// O SELECT-base de achados (alias `a`).
pub const SQL_BASE_DE_ACHADOS: &str = "SELECT a.content_hash, a.estado, a.document_type, a.extensao, a.data_chave, a.data_chave_tipo,
       a.ciclo_id, a.valor_centavos, a.supplier_name_raw,
       (SELECT filename FROM origem o WHERE o.content_hash = a.content_hash ORDER BY o.id LIMIT 1) AS nome,
       (SELECT coalesce(local_filepath, gmail_subject) FROM origem o WHERE o.content_hash = a.content_hash ORDER BY o.id LIMIT 1) AS onde,
       (SELECT count(*) FROM origem o WHERE o.content_hash = a.content_hash) AS origens,
       a.caminho_acervo, a.confianca, a.notas, a.decidido_por
     FROM achado a WHERE 1 = 1";

fn mapear_achado(l: &Row<'_>) -> rusqlite::Result<LinhaDeAchado> {
    Ok(LinhaDeAchado {
        hash: l.get(0)?,
        estado: l.get(1)?,
        tipo: l.get(2)?,
        extensao: l.get(3)?,
        data_chave: l.get(4)?,
        data_chave_tipo: l.get(5)?,
        ciclo: l.get(6)?,
        valor: l.get(7)?,
        fornecedor: l.get(8)?,
        nome: l.get(9)?,
        onde: l.get(10)?,
        origens: l.get(11)?,
        no_cofre: l.get(12)?,
        confianca: l.get(13)?,
        notas: l.get(14)?,
        decidido_por: l.get(15)?,
    })
}

/// Achados de qualquer estado, mais novos primeiro.
pub fn achados(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeAchado>> {
    let sql = format!(
        "{SQL_BASE_DE_ACHADOS}{sel} ORDER BY a.data_chave DESC NULLS LAST, a.primeiro_visto_em DESC, a.content_hash{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), mapear_achado)
}

fn com_estado(sel: &Selecao, estado: &str) -> Selecao {
    Selecao::nova()
        .onde("a.estado = ?", vec![texto(estado)])
        .junta(sel.clone())
}

/// `documentos`: achados confirmados como documento.
pub fn documentos(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeAchado>> {
    achados(ctx, &com_estado(sel, "documento"))
}

/// `duvidas`: o que a triagem não conseguiu decidir sozinha.
pub fn duvidas(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeAchado>> {
    achados(ctx, &com_estado(sel, "duvida"))
}

/// A triar: achados com `estado = 'novo'`.
pub fn a_triar(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeAchado>> {
    achados(ctx, &com_estado(sel, "novo"))
}

/// `documentos` com `:tipo` e `:ciclo` do Datasette e `LIMIT 1000`.
pub fn documentos_datasette(
    ctx: &Contexto<'_>,
    tipo: &str,
    ciclo: &str,
) -> ResultadoDeConsulta<Vec<LinhaDeAchado>> {
    let mut sel = Selecao::nova().com_limite(1000);
    if !tipo.is_empty() {
        sel = sel.onde("a.document_type = ?", vec![texto(tipo)]);
    }
    if !ciclo.is_empty() {
        sel = sel.onde("a.ciclo_id = ?", vec![texto(ciclo)]);
    }
    documentos(ctx, &sel)
}

/// `duvidas` com `LIMIT 1000`.
pub fn duvidas_datasette(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Vec<LinhaDeAchado>> {
    duvidas(ctx, &Selecao::nova().com_limite(1000))
}

/// O nome de um achado: o arquivo da primeira origem, ou `Achado <hash curto>`.
pub fn rotulo_do_achado(nome: &Option<String>, hash: &str) -> String {
    nao_vazio(nome).map_or_else(|| format!("Achado {}", hash_curto(hash)), str::to_owned)
}

/// Qual `Tipo` usa o link de um achado: `Documento` quando a tabela é de documentos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LinkDoAchado {
    Achado,
    Documento,
}

impl LinkDoAchado {
    fn tipo(self) -> Tipo {
        match self {
            LinkDoAchado::Achado => Tipo::Achado,
            LinkDoAchado::Documento => Tipo::Documento,
        }
    }
}

/// A tabela de achados. Com `Documento`, mostra a confiança; com `Achado`, o motivo.
pub fn tabela_de_achados(linhas: &[LinhaDeAchado], link: LinkDoAchado) -> Tabela {
    let tipo_link = link.tipo();
    let com_documento = link == LinkDoAchado::Documento;
    let mut colunas = vec![
        coluna(if com_documento { "Documento" } else { "Achado" }),
        coluna("Estado"),
        coluna("Tipo"),
        coluna("Data"),
        coluna("Ciclo"),
        coluna_num("Valor"),
        coluna("Fornecedor"),
        coluna_num("Origens"),
        coluna("Onde"),
        coluna("No cofre"),
    ];
    colunas.push(if com_documento {
        coluna_num("Confiança")
    } else {
        coluna("Motivo")
    });
    Tabela {
        colunas,
        linhas: linhas
            .iter()
            .map(|l| {
                let de = Ref::nova(Tipo::Achado, &l.hash);
                let mut celulas = vec![
                    c_link(tipo_link, &l.hash, &rotulo_do_achado(&l.nome, &l.hash)),
                    c_estado_achado(&l.estado, &l.decidido_por),
                    c_texto_opt(&l.tipo),
                    c_data_opt(&l.data_chave),
                    c_link_opt(Tipo::Ciclo, &l.ciclo),
                    c_valor_opt(l.valor),
                    c_link_opt(Tipo::Fornecedor, &l.fornecedor),
                    c_contagem(l.origens, de, None, "origens"),
                    c_texto_opt(&l.onde),
                    c_texto_opt(&l.no_cofre),
                ];
                celulas.push(if com_documento {
                    l.confianca.map_or(Celula::Vazia, |c| {
                        Celula::Texto(format!("{:.0}%", c * 100.0))
                    })
                } else {
                    c_texto_opt(&l.notas)
                });
                Linha {
                    alvo: Some(Ref::nova(tipo_link, &l.hash)),
                    celulas,
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 6. emails_por_remetente
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeRemetente {
    pub dominio: Option<String>,
    pub emails: i64,
    pub ultimo: Option<String>,
    pub ultimo_assunto: Option<String>,
}

pub fn emails_por_remetente(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDeRemetente>> {
    let sql = format!(
        "SELECT lower(replace(substr(o.gmail_from, instr(o.gmail_from, '@') + 1), '>', '')) AS dominio,
           count(*) AS emails, max(o.gmail_date) AS ultimo,
           (SELECT gmail_subject FROM origem x WHERE x.gmail_from = o.gmail_from ORDER BY gmail_date DESC LIMIT 1) AS ultimo_assunto
         FROM origem o WHERE o.origin_type = 'gmail' AND o.mensagem_ref = 'eml'{sel}
         GROUP BY 1 ORDER BY 2 DESC, 1{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeRemetente {
            dominio: l.get(0)?,
            emails: l.get(1)?,
            ultimo: l.get(2)?,
            ultimo_assunto: l.get(3)?,
        })
    })
}

pub fn emails_por_remetente_datasette(
    ctx: &Contexto<'_>,
) -> ResultadoDeConsulta<Vec<LinhaDeRemetente>> {
    emails_por_remetente(ctx, &Selecao::nova().com_limite(300))
}

pub fn tabela_emails_por_remetente(linhas: &[LinhaDeRemetente]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Domínio"),
            coluna_num("E-mails"),
            coluna("Último"),
            coluna("Último assunto"),
        ],
        linhas: linhas
            .iter()
            .map(|l| Linha {
                alvo: None,
                celulas: vec![
                    c_texto_opt(&l.dominio),
                    c_contagem_solta(l.emails),
                    c_data_opt(&l.ultimo),
                    c_texto_opt(&l.ultimo_assunto),
                ],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 7. emails
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeEmail {
    pub origem_id: i64,
    pub hash: String,
    /// `substr(gmail_date, 1, 16)`.
    pub data: Option<String>,
    pub de: Option<String>,
    pub assunto: Option<String>,
    pub anexos: i64,
    pub estado: String,
    pub eml_no_cofre: Option<String>,
    pub decidido_por: Option<String>,
}

pub fn emails(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeEmail>> {
    let sql = format!(
        "SELECT o.id, a.content_hash, substr(o.gmail_date, 1, 16) AS data, o.gmail_from, o.gmail_subject,
           (SELECT count(*) FROM origem x WHERE x.gmail_message_id = o.gmail_message_id AND x.mensagem_ref = 'anexo') AS anexos,
           a.estado, a.caminho_acervo, a.decidido_por
         FROM origem o JOIN achado a ON a.content_hash = o.content_hash
         WHERE o.origin_type = 'gmail' AND o.mensagem_ref = 'eml'{sel}
         ORDER BY o.gmail_date DESC, o.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeEmail {
            origem_id: l.get(0)?,
            hash: l.get(1)?,
            data: l.get(2)?,
            de: l.get(3)?,
            assunto: l.get(4)?,
            anexos: l.get(5)?,
            estado: l.get(6)?,
            eml_no_cofre: l.get(7)?,
            decidido_por: l.get(8)?,
        })
    })
}

pub fn emails_datasette(ctx: &Contexto<'_>, busca: &str) -> ResultadoDeConsulta<Vec<LinhaDeEmail>> {
    let mut sel = Selecao::nova().com_limite(1000);
    if !busca.is_empty() {
        sel = sel.onde(
            "o.gmail_from LIKE '%' || ? || '%' OR o.gmail_subject LIKE '%' || ? || '%'",
            vec![texto(busca), texto(busca)],
        );
    }
    emails(ctx, &sel)
}

pub fn tabela_emails(linhas: &[LinhaDeEmail]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Data"),
            coluna("De"),
            coluna("Assunto"),
            coluna_num("Anexos"),
            coluna("Estado"),
            coluna("E-mail no cofre"),
        ],
        linhas: linhas
            .iter()
            .map(|l| Linha {
                alvo: Some(Ref::nova(Tipo::Origem, l.origem_id.to_string())),
                celulas: vec![
                    c_data_opt(&l.data),
                    c_texto_opt(&l.de),
                    c_link(
                        Tipo::Origem,
                        &l.origem_id.to_string(),
                        nao_vazio(&l.assunto).unwrap_or("(sem assunto)"),
                    ),
                    c_contagem_solta(l.anexos),
                    c_estado_achado(&l.estado, &l.decidido_por),
                    c_texto_opt(&l.eml_no_cofre),
                ],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 8. arquivos
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeArquivo {
    pub origem_id: i64,
    pub hash: Option<String>,
    pub nome: String,
    pub caminho: Option<String>,
    pub bytes: Option<i64>,
    pub data_arquivo: Option<String>,
    pub camada: Option<String>,
    pub estado: Option<String>,
    pub tipo: Option<String>,
    pub no_cofre: Option<String>,
    pub decidido_por: Option<String>,
}

pub fn arquivos(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeArquivo>> {
    let sql = format!(
        "SELECT o.id, o.content_hash, o.filename, o.local_filepath, o.file_size_bytes, substr(o.local_mtime, 1, 10),
           a.camada, a.estado, a.document_type, a.caminho_acervo, a.decidido_por
         FROM origem o LEFT JOIN achado a ON a.content_hash = o.content_hash
         WHERE o.origin_type IN ('local_file', 'onedrive', 'gdrive'){sel}
         ORDER BY o.local_mtime DESC, o.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeArquivo {
            origem_id: l.get(0)?,
            hash: l.get(1)?,
            nome: l.get(2)?,
            caminho: l.get(3)?,
            bytes: l.get(4)?,
            data_arquivo: l.get(5)?,
            camada: l.get(6)?,
            estado: l.get(7)?,
            tipo: l.get(8)?,
            no_cofre: l.get(9)?,
            decidido_por: l.get(10)?,
        })
    })
}

/// `arquivos` com `:busca` do Datasette e `LIMIT 1000`.
pub fn arquivos_datasette(
    ctx: &Contexto<'_>,
    busca: &str,
) -> ResultadoDeConsulta<Vec<LinhaDeArquivo>> {
    let sel = Selecao::nova()
        .onde(
            "o.filename LIKE '%' || ? || '%' OR o.local_filepath LIKE '%' || ? || '%'",
            vec![texto(busca), texto(busca)],
        )
        .com_limite(1000);
    arquivos(ctx, &sel)
}

pub fn tabela_arquivos(linhas: &[LinhaDeArquivo]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Nome"),
            coluna("Caminho"),
            coluna_num("Bytes"),
            coluna("Data do arquivo"),
            coluna("Camada"),
            coluna("Estado"),
            coluna("Tipo"),
            coluna("No cofre"),
        ],
        linhas: linhas
            .iter()
            .map(|l| Linha {
                alvo: Some(Ref::nova(Tipo::Origem, l.origem_id.to_string())),
                celulas: vec![
                    c_link(Tipo::Origem, &l.origem_id.to_string(), &l.nome),
                    c_texto_opt(&l.caminho),
                    l.bytes.map_or(Celula::Vazia, c_contagem_solta),
                    c_data_opt(&l.data_arquivo),
                    c_texto_opt(&l.camada),
                    l.estado
                        .as_deref()
                        .map_or(Celula::Vazia, |e| c_estado_achado(e, &l.decidido_por)),
                    c_texto_opt(&l.tipo),
                    c_texto_opt(&l.no_cofre),
                ],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 9. pastas_pessoais
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDePasta {
    pub pasta: Option<String>,
    pub achados: i64,
    pub documentos: i64,
    pub descartados: i64,
}

pub fn pastas_pessoais(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDePasta>> {
    let sql = format!(
        "SELECT substr(o.local_filepath, 1, length(o.local_filepath) - length(o.filename) - 1) AS pasta,
           count(DISTINCT o.content_hash) AS achados,
           coalesce(sum(a.estado = 'documento'), 0) AS documentos,
           coalesce(sum(a.estado = 'descartado'), 0) AS descartados
         FROM origem o JOIN achado a ON a.content_hash = o.content_hash
         WHERE a.camada = 'A' AND o.local_filepath IS NOT NULL{sel}
         GROUP BY 1 ORDER BY 2 DESC, 1{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDePasta {
            pasta: l.get(0)?,
            achados: l.get(1)?,
            documentos: l.get(2)?,
            descartados: l.get(3)?,
        })
    })
}

pub fn pastas_pessoais_datasette(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Vec<LinhaDePasta>> {
    pastas_pessoais(ctx, &Selecao::nova().com_limite(500))
}

pub fn tabela_pastas_pessoais(linhas: &[LinhaDePasta]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Pasta"),
            coluna_num("Achados"),
            coluna_num("Documentos"),
            coluna_num("Descartados"),
        ],
        linhas: linhas
            .iter()
            .map(|l| Linha {
                alvo: None,
                celulas: vec![
                    c_texto_opt(&l.pasta),
                    c_contagem_solta(l.achados),
                    c_contagem_solta(l.documentos),
                    c_contagem_solta(l.descartados),
                ],
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 10. varreduras (coletas)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeVarredura {
    pub id: i64,
    pub fonte_id: String,
    pub etapa: String,
    pub status: String,
    pub iniciada_em: String,
    pub concluida_em: Option<String>,
    pub vistos: i64,
    pub novos: i64,
    pub repetidos: i64,
    pub excluidos: i64,
    pub erros: i64,
    pub escopo: String,
    pub responsavel: String,
}

pub fn varreduras(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeVarredura>> {
    let sql = format!(
        "SELECT v.id, v.fonte_id, v.etapa, v.status, v.iniciada_em, v.concluida_em, v.vistos, v.novos,
           v.repetidos, v.excluidos, v.erros, v.escopo, v.responsavel
         FROM varredura v WHERE 1 = 1{sel} ORDER BY v.id DESC{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeVarredura {
            id: l.get(0)?,
            fonte_id: l.get(1)?,
            etapa: l.get(2)?,
            status: l.get(3)?,
            iniciada_em: l.get(4)?,
            concluida_em: l.get(5)?,
            vistos: l.get(6)?,
            novos: l.get(7)?,
            repetidos: l.get(8)?,
            excluidos: l.get(9)?,
            erros: l.get(10)?,
            escopo: l.get(11)?,
            responsavel: l.get(12)?,
        })
    })
}

pub fn varreduras_datasette(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Vec<LinhaDeVarredura>> {
    varreduras(ctx, &Selecao::nova())
}

/// O nome de uma coleta: `Coleta E1 · disco:e2 #1`.
pub fn rotulo_da_coleta(etapa: &str, fonte_id: &str, id: i64) -> String {
    format!("Coleta {etapa} · {fonte_id} #{id}")
}

pub fn tabela_varreduras(linhas: &[LinhaDeVarredura]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Coleta"),
            coluna("Conexão"),
            coluna("Etapa"),
            coluna("Situação"),
            coluna("Iniciada em"),
            coluna("Concluída em"),
            coluna_num("Vistos"),
            coluna_num("Novos"),
            coluna_num("Repetidos"),
            coluna_num("Excluídos"),
            coluna_num("Erros"),
            coluna("Escopo"),
            coluna("Responsável"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let de = Ref::nova(Tipo::Coleta, l.id.to_string());
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        c_link(
                            Tipo::Coleta,
                            &l.id.to_string(),
                            &rotulo_da_coleta(&l.etapa, &l.fonte_id, l.id),
                        ),
                        c_link(Tipo::Conexao, &l.fonte_id, &l.fonte_id),
                        c_texto(&l.etapa),
                        c_texto(&l.status),
                        Celula::Data(l.iniciada_em.clone()),
                        c_data_opt(&l.concluida_em),
                        c_contagem_solta(l.vistos),
                        c_contagem(l.novos, de, None, "origens"),
                        c_contagem_solta(l.repetidos),
                        c_contagem_solta(l.excluidos),
                        c_contagem_solta(l.erros),
                        c_texto(&l.escopo),
                        Celula::Texto(l.responsavel.clone()),
                    ],
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 11. faturas_conferencia
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeFatura {
    /// O id da fatura no Open Finance (`substr(external_ref, 13)`): a chave de `fatura:<id>`.
    pub fatura: String,
    pub conta: Option<String>,
    pub vencimento: String,
    /// Positivo: o total da fatura.
    pub total: Centavos,
    pub transacoes: i64,
    pub soma_transacoes: Centavos,
    pub diferenca: Centavos,
    /// `confere`, `quase` ou `não confere`.
    pub situacao: String,
}

pub fn faturas_conferencia(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDeFatura>> {
    let fatura_ref = ctx.dialeto.fatura_ref("");
    let sql = format!(
        "WITH f AS (SELECT substr(external_ref, 13) AS fatura, data AS vencimento, -valor_centavos AS total,
                            conta_provavel AS conta
                      FROM registro WHERE tipo = 'fatura_cartao'),
              t AS (SELECT {fatura_ref} AS fatura, count(*) AS n, -sum(valor_centavos) AS soma
                      FROM registro WHERE {fatura_ref} IS NOT NULL AND tipo NOT IN ('fatura_cartao', 'pagamento_fatura')
                      GROUP BY 1)
         SELECT f.fatura, f.conta, f.vencimento, f.total, coalesce(t.n, 0), coalesce(t.soma, 0),
           (f.total - coalesce(t.soma, 0)),
           CASE WHEN abs(f.total - coalesce(t.soma, 0)) <= 100 THEN 'confere'
                WHEN abs(f.total - coalesce(t.soma, 0)) <= f.total * 0.05 THEN 'quase'
                ELSE 'não confere' END AS situacao
         FROM f LEFT JOIN t ON t.fatura = f.fatura
         WHERE 1 = 1{sel} ORDER BY f.conta, f.vencimento, f.fatura{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        Ok(LinhaDeFatura {
            fatura: l.get(0)?,
            conta: l.get(1)?,
            vencimento: l.get(2)?,
            total: l.get(3)?,
            transacoes: l.get(4)?,
            soma_transacoes: l.get(5)?,
            diferenca: l.get(6)?,
            situacao: l.get(7)?,
        })
    })
}

pub fn faturas_conferencia_datasette(
    ctx: &Contexto<'_>,
) -> ResultadoDeConsulta<Vec<LinhaDeFatura>> {
    faturas_conferencia(ctx, &Selecao::nova())
}

/// O nome de uma fatura: `Cartão Beta · fatura de 2026-10-10`.
pub fn rotulo_da_fatura(conta: &Option<String>, vencimento: &str) -> String {
    format!(
        "{} · fatura de {vencimento}",
        nao_vazio(conta).unwrap_or("Cartão"),
    )
}

pub fn tabela_faturas(linhas: &[LinhaDeFatura]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Fatura"),
            coluna("Conta"),
            coluna("Vencimento"),
            coluna_num("Total da fatura"),
            coluna_num("Transações"),
            coluna_num("Soma das transações"),
            coluna_num("Diferença"),
            coluna("Situação"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let de = Ref::nova(Tipo::Fatura, &l.fatura);
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        c_link(
                            Tipo::Fatura,
                            &l.fatura,
                            &rotulo_da_fatura(&l.conta, &l.vencimento),
                        ),
                        c_link_opt(Tipo::Conta, &l.conta),
                        Celula::Data(l.vencimento.clone()),
                        c_valor_total(l.total, de.clone(), None, "fatura"),
                        c_contagem(l.transacoes, de.clone(), None, "transacoes"),
                        c_valor_total(l.soma_transacoes, de, None, "transacoes"),
                        c_valor(l.diferenca),
                        c_texto(&l.situacao),
                    ],
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// 12. contas_buracos (lacunas de conta)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeLacuna {
    pub conta: Option<String>,
    pub transacoes: i64,
    pub de: String,
    pub ate: String,
    /// Os meses `AAAA-MM`, dentro do período da conta, em que a Pluggy não trouxe nada.
    pub meses_sem_nada: Vec<String>,
}

pub fn contas_buracos(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDeLacuna>> {
    let sql = format!(
        "WITH m AS (SELECT DISTINCT substr(data, 1, 7) AS mes FROM registro
                      WHERE fonte_id = 'conexao:pluggy' AND data >= '2025-09'),
              ct AS (SELECT conta_provavel AS conta, count(*) AS n, min(substr(data, 1, 7)) AS de, max(substr(data, 1, 7)) AS ate
                       FROM registro WHERE fonte_id = 'conexao:pluggy' AND tipo <> 'fatura_cartao'
                       GROUP BY 1 HAVING n >= 20)
         SELECT ct.conta, ct.n, ct.de, ct.ate,
           (SELECT group_concat(m.mes, ' ') FROM m
             WHERE m.mes BETWEEN ct.de AND ct.ate
               AND NOT EXISTS (SELECT 1 FROM registro r WHERE r.conta_provavel = ct.conta AND substr(r.data, 1, 7) = m.mes)) AS meses_sem_nada
         FROM ct WHERE 1 = 1{sel} ORDER BY ct.n DESC, ct.conta{limite}",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), |l| {
        let meses: Option<String> = l.get(4)?;
        Ok(LinhaDeLacuna {
            conta: l.get(0)?,
            transacoes: l.get(1)?,
            de: l.get(2)?,
            ate: l.get(3)?,
            meses_sem_nada: meses
                .map(|m| m.split(' ').map(str::to_owned).collect())
                .unwrap_or_default(),
        })
    })
}

pub fn contas_buracos_datasette(ctx: &Contexto<'_>) -> ResultadoDeConsulta<Vec<LinhaDeLacuna>> {
    contas_buracos(ctx, &Selecao::nova())
}

pub fn tabela_contas_buracos(linhas: &[LinhaDeLacuna]) -> Tabela {
    Tabela {
        colunas: vec![
            coluna("Conta"),
            coluna_num("Transações"),
            coluna("De"),
            coluna("Até"),
            coluna_num("Meses sem nada"),
            coluna("Quais"),
        ],
        linhas: linhas
            .iter()
            .map(|l| {
                let alvo = nao_vazio(&l.conta).map(|c| Ref::nova(Tipo::Conta, c));
                let (conta, transacoes) = match &alvo {
                    Some(de) => (
                        c_link(Tipo::Conta, &de.chave, &de.chave),
                        c_contagem(l.transacoes, de.clone(), None, "registros"),
                    ),
                    None => (Celula::Vazia, c_contagem_solta(l.transacoes)),
                };
                let mut meses = l.meses_sem_nada.clone();
                meses.sort();
                Linha {
                    alvo,
                    celulas: vec![
                        conta,
                        transacoes,
                        c_texto(&l.de),
                        c_texto(&l.ate),
                        c_contagem_solta(meses.len() as i64),
                        Celula::Links(
                            meses
                                .iter()
                                .map(|m| (Ref::nova(Tipo::Ciclo, m), m.clone()))
                                .collect(),
                        ),
                    ],
                }
            })
            .collect(),
    }
}

// ---------------------------------------------------------------------------
// As áreas do painel de navegação
// ---------------------------------------------------------------------------

/// Monta a tabela de uma [`Area`] que é só uma vista. Pendências e Lotes são das
/// listagens de `relacoes`; a Visão geral e os Relatórios são painéis.
pub fn tabela_da_area(
    ctx: &Contexto<'_>,
    area: Area,
    filtro: &Filtro,
) -> ResultadoDeConsulta<Tabela> {
    let reg = || selecao_do_filtro(ctx, filtro, Alvo::Registro);
    let ach = || selecao_do_filtro(ctx, filtro, Alvo::Achado);
    match area {
        Area::ATriar => Ok(tabela_de_achados(
            &a_triar(ctx, &ach())?,
            LinkDoAchado::Achado,
        )),
        Area::EmDuvida => Ok(tabela_de_achados(
            &duvidas(ctx, &ach())?,
            LinkDoAchado::Achado,
        )),
        Area::Documentos => Ok(tabela_de_achados(
            &documentos(ctx, &ach())?,
            LinkDoAchado::Documento,
        )),
        Area::Registros => Ok(tabela_registros(&registros(ctx, &reg())?)),
        Area::Ciclos => Ok(tabela_por_ciclo(&por_ciclo(ctx, &reg())?)),
        Area::Fornecedores => Ok(tabela_fornecedores(&fornecedores(ctx, &reg())?)),
        Area::Faturas => {
            let mut sel = Selecao::nova();
            if let Some((de, ate)) = &filtro.ciclos {
                sel = sel.onde(
                    "f.vencimento >= ? AND f.vencimento <= ?",
                    vec![texto(inicio_do_ciclo(de)), texto(fim_do_ciclo(ate))],
                );
            }
            if let Some(t) = texto_do_filtro(filtro) {
                sel = sel.onde("f.conta LIKE ?", vec![texto(format!("%{t}%"))]);
            }
            Ok(tabela_faturas(&faturas_conferencia(ctx, &sel)?))
        }
        Area::ContasLacunas => {
            let mut sel = Selecao::nova();
            if let Some(t) = texto_do_filtro(filtro) {
                sel = sel.onde("ct.conta LIKE ?", vec![texto(format!("%{t}%"))]);
            }
            Ok(tabela_contas_buracos(&contas_buracos(ctx, &sel)?))
        }
        Area::Emails => Ok(tabela_emails(&emails(ctx, &selecao_de_emails(filtro))?)),
        Area::ArquivosEPastas => Ok(tabela_arquivos(&arquivos(
            ctx,
            &selecao_de_arquivos(filtro),
        )?)),
        Area::Coletas => {
            let mut sel = Selecao::nova();
            if let Some(t) = texto_do_filtro(filtro) {
                let p = texto(format!("%{t}%"));
                sel = sel.onde(
                    "v.fonte_id LIKE ? OR v.responsavel LIKE ? OR v.escopo LIKE ?",
                    vec![p.clone(), p.clone(), p],
                );
            }
            if !filtro.fontes.is_empty() {
                sel = sel.onde(
                    format!("v.fonte_id IN ({})", marcas(filtro.fontes.len())),
                    filtro.fontes.iter().map(|f| texto(f.as_str())).collect(),
                );
            }
            Ok(tabela_varreduras(&varreduras(ctx, &sel)?))
        }
        Area::VisaoGeral | Area::Relatorios | Area::Pendencias | Area::Lotes => Err(
            ErroDeConsulta::NaoSuportado(format!("a área {area:?} não é uma vista simples")),
        ),
    }
}

/// O filtro de E-mails: texto no remetente e no assunto, e o período (por data da mensagem).
pub fn selecao_de_emails(filtro: &Filtro) -> Selecao {
    let mut sel = Selecao::nova();
    if let Some(t) = texto_do_filtro(filtro) {
        let p = texto(format!("%{t}%"));
        sel = sel.onde(
            "o.gmail_from LIKE ? OR o.gmail_subject LIKE ?",
            vec![p.clone(), p],
        );
    }
    if let Some((de, ate)) = &filtro.ciclos {
        sel = sel.onde(
            "substr(o.gmail_date, 1, 10) >= ? AND substr(o.gmail_date, 1, 10) <= ?",
            vec![texto(inicio_do_ciclo(de)), texto(fim_do_ciclo(ate))],
        );
    }
    if !filtro.estados.is_empty() {
        sel = sel.onde(
            format!("a.estado IN ({})", marcas(filtro.estados.len())),
            filtro.estados.iter().map(|f| texto(f.as_str())).collect(),
        );
    }
    sel
}

/// O filtro de Arquivos e pastas: texto no nome e no caminho, fontes, estados e tipos.
pub fn selecao_de_arquivos(filtro: &Filtro) -> Selecao {
    let mut sel = Selecao::nova();
    if let Some(t) = texto_do_filtro(filtro) {
        let p = texto(format!("%{t}%"));
        sel = sel.onde(
            "o.filename LIKE ? OR o.local_filepath LIKE ?",
            vec![p.clone(), p],
        );
    }
    if !filtro.fontes.is_empty() {
        sel = sel.onde(
            format!("o.fonte_id IN ({})", marcas(filtro.fontes.len())),
            filtro.fontes.iter().map(|f| texto(f.as_str())).collect(),
        );
    }
    if !filtro.estados.is_empty() {
        sel = sel.onde(
            format!("a.estado IN ({})", marcas(filtro.estados.len())),
            filtro.estados.iter().map(|f| texto(f.as_str())).collect(),
        );
    }
    if !filtro.tipos.is_empty() {
        sel = sel.onde(
            format!("a.document_type IN ({})", marcas(filtro.tipos.len())),
            filtro.tipos.iter().map(|f| texto(f.as_str())).collect(),
        );
    }
    sel
}

/// A vista "E-mails por remetente" (a outra metade da área E-mails).
pub fn tabela_de_emails_por_remetente(
    ctx: &Contexto<'_>,
    filtro: &Filtro,
) -> ResultadoDeConsulta<Tabela> {
    let mut sel = Selecao::nova();
    if let Some(t) = texto_do_filtro(filtro) {
        let p = texto(format!("%{t}%"));
        sel = sel.onde(
            "o.gmail_from LIKE ? OR o.gmail_subject LIKE ?",
            vec![p.clone(), p],
        );
    }
    Ok(tabela_emails_por_remetente(&emails_por_remetente(
        ctx, &sel,
    )?))
}

/// A vista "Pastas pessoais" (a outra metade da área Arquivos e pastas).
pub fn tabela_de_pastas_pessoais(
    ctx: &Contexto<'_>,
    filtro: &Filtro,
) -> ResultadoDeConsulta<Tabela> {
    let mut sel = Selecao::nova();
    if let Some(t) = texto_do_filtro(filtro) {
        sel = sel.onde("o.local_filepath LIKE ?", vec![texto(format!("%{t}%"))]);
    }
    Ok(tabela_pastas_pessoais(&pastas_pessoais(ctx, &sel)?))
}
