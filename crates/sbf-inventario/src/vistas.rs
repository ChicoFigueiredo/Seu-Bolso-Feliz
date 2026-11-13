//! As 12 consultas do Datasette (congeladas em `tests/fixtures/datasette-metadata.json`) como
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
    /// Quantas linhas pular antes do `limite` (páginas da tabela).
    pub deslocamento: usize,
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

    /// `" LIMIT n OFFSET m"`, ou vazio.
    pub fn sql_limite(&self) -> String {
        match (self.limite, self.deslocamento) {
            (None, 0) => String::new(),
            (None, m) => format!(" LIMIT -1 OFFSET {m}"),
            (Some(n), 0) => format!(" LIMIT {n}"),
            (Some(n), m) => format!(" LIMIT {n} OFFSET {m}"),
        }
    }

    /// Pede a página de `limite` linhas que começa em `inicio`.
    #[must_use]
    pub fn com_pagina(mut self, inicio: usize, limite: usize) -> Self {
        self.deslocamento = inicio;
        self.limite = Some(limite);
        self
    }

    /// Alguma condição menciona `texto` (por exemplo, o alias de uma tabela ligada)?
    pub fn menciona(&self, texto: &str) -> bool {
        self.clausulas.iter().any(|c| c.contains(texto))
    }

    /// A mesma seleção, sem limite nem deslocamento (para contar).
    #[must_use]
    pub fn sem_pagina(&self) -> Self {
        Self {
            limite: None,
            deslocamento: 0,
            ..self.clone()
        }
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
            self.deslocamento = outra.deslocamento;
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
                // Subconsulta não correlacionada: o plano correlacionado varria `origem_fonte` por achado.
                format!("a.content_hash IN (SELECT o.content_hash FROM origem o WHERE o.fonte_id IN ({m}))"),
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

pub(crate) fn celula_texto(valor: &str) -> Celula {
    if valor.is_empty() {
        Celula::Vazia
    } else {
        Celula::Texto(valor.to_owned())
    }
}

pub(crate) fn celula_texto_opt(valor: &Option<String>) -> Celula {
    nao_vazio(valor).map_or(Celula::Vazia, celula_texto)
}

pub(crate) fn celula_data_opt(valor: &Option<String>) -> Celula {
    nao_vazio(valor).map_or(Celula::Vazia, |d| Celula::Data(d.to_owned()))
}

pub(crate) fn celula_link(tipo: Tipo, chave: &str, texto: &str) -> Celula {
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
pub(crate) fn celula_link_opt(tipo: Tipo, nome: &Option<String>) -> Celula {
    nao_vazio(nome).map_or(Celula::Vazia, |n| celula_link(tipo, n, n))
}

pub(crate) fn celula_valor(centavos: Centavos) -> Celula {
    Celula::Valor {
        centavos,
        fonte: Fonte::Inventario,
        itens: None,
    }
}

pub(crate) fn celula_valor_opt(centavos: Option<Centavos>) -> Celula {
    centavos.map_or(Celula::Vazia, celula_valor)
}

/// `base` com o filtro da aba Início, para que o número de uma tabela filtrada abra os
/// itens do mesmo filtro. Filtro vazio não muda o recorte.
pub fn recorte_com_filtro(base: &str, filtro: &Filtro) -> String {
    if *filtro == Filtro::default() {
        return base.to_owned();
    }
    match serde_json::to_string(filtro) {
        Ok(json) => format!("{base}|filtro={json}"),
        Err(_) => base.to_owned(),
    }
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
pub(crate) fn celula_valor_total(
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
pub(crate) fn celula_contagem(n: i64, de: Ref, ciclo: Option<&str>, recorte: &str) -> Celula {
    Celula::Contagem {
        n,
        itens: Some(pedido_total(de, ciclo, recorte)),
    }
}

pub(crate) fn celula_contagem_solta(n: i64) -> Celula {
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

pub(crate) fn celula_estado_achado(estado: &str, decidido_por: &Option<String>) -> Celula {
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

pub(crate) fn celula_estado_registro(estado: &str, decidido_por: &Option<String>) -> Celula {
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

pub fn tabela_por_ciclo(linhas: &[LinhaDeCiclo], filtro: &Filtro) -> Tabela {
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
                        celula_link(Tipo::Ciclo, &l.ciclo, &l.ciclo),
                        celula_valor_total(
                            l.entrou,
                            de.clone(),
                            Some(&l.ciclo),
                            &recorte_com_filtro("entradas", filtro),
                        ),
                        celula_valor_total(
                            l.saiu,
                            de.clone(),
                            Some(&l.ciclo),
                            &recorte_com_filtro("saidas", filtro),
                        ),
                        celula_valor_total(
                            l.movido_entre_contas,
                            de.clone(),
                            Some(&l.ciclo),
                            &recorte_com_filtro("movido", filtro),
                        ),
                        celula_contagem(
                            l.registros,
                            de,
                            Some(&l.ciclo),
                            &recorte_com_filtro("registros", filtro),
                        ),
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

pub fn tabela_fornecedores(linhas: &[LinhaDeFornecedor], filtro: &Filtro) -> Tabela {
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
                let recorte = recorte_com_filtro(&format!("saidas:{}", l.tipo), filtro);
                let (nome, vezes, saiu) = match &alvo {
                    Some(de) => (
                        celula_link(Tipo::Fornecedor, &de.chave, &de.chave),
                        celula_contagem(l.vezes, de.clone(), None, &recorte),
                        celula_valor_total(l.saiu, de.clone(), None, &recorte),
                    ),
                    None => (
                        Celula::Vazia,
                        celula_contagem_solta(l.vezes),
                        celula_valor(l.saiu),
                    ),
                };
                let mut ciclos = l.ciclos.clone();
                ciclos.sort();
                Linha {
                    alvo,
                    celulas: vec![
                        nome,
                        celula_texto(&l.tipo),
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
                    celula_link(
                        Tipo::Registro,
                        &l.id.to_string(),
                        &rotulo_do_registro(l.id, &l.descricao),
                    ),
                    celula_valor(l.valor),
                    celula_texto(&l.tipo),
                    celula_link_opt(Tipo::Fornecedor, &l.fornecedor),
                    celula_texto_opt(&l.contraparte),
                    celula_data_opt(&l.data_compra),
                    celula_texto_opt(&l.categoria),
                    celula_link_opt(Tipo::Conta, &l.conta),
                    celula_link_opt(Tipo::Ciclo, &l.ciclo),
                    celula_estado_registro(&l.estado, &l.decidido_por),
                    celula_link(Tipo::Conexao, &l.fonte, &l.fonte),
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
///
/// Em duas fases: a de dentro escolhe e ordena só as chaves (e aplica `LIMIT`/`OFFSET`); a de
/// fora monta as colunas, com as subconsultas de origem, só para as linhas da página. Num só
/// SELECT o SQLite calcularia as subconsultas de todos os achados antes de ordenar.
pub fn achados(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<Vec<LinhaDeAchado>> {
    let sql = format!(
        "SELECT ax.content_hash, ax.estado, ax.document_type, ax.extensao, ax.data_chave, ax.data_chave_tipo,
           ax.ciclo_id, ax.valor_centavos, ax.supplier_name_raw,
           (SELECT filename FROM origem o WHERE o.content_hash = ax.content_hash ORDER BY o.id LIMIT 1) AS nome,
           (SELECT coalesce(local_filepath, gmail_subject) FROM origem o WHERE o.content_hash = ax.content_hash ORDER BY o.id LIMIT 1) AS onde,
           (SELECT count(*) FROM origem o WHERE o.content_hash = ax.content_hash) AS origens,
           ax.caminho_acervo, ax.confianca, ax.notas, ax.decidido_por
         FROM (SELECT a.content_hash AS h, a.data_chave AS dc, a.primeiro_visto_em AS pv
                 FROM achado a WHERE 1 = 1{sel}
                ORDER BY a.data_chave DESC NULLS LAST, a.primeiro_visto_em DESC, a.content_hash{limite}) k
           JOIN achado ax ON ax.content_hash = k.h
         ORDER BY k.dc DESC NULLS LAST, k.pv DESC, k.h",
        sel = sel.sql_e(),
        limite = sel.sql_limite(),
    );
    consultar(ctx.conn, &sql, sel.params(), mapear_achado)
}

/// Quantos achados a seleção tem (ignora limite e deslocamento).
pub fn contar_achados(ctx: &Contexto<'_>, sel: &Selecao) -> ResultadoDeConsulta<usize> {
    let sel = sel.sem_pagina();
    contar_linhas(
        ctx.conn,
        &format!("SELECT count(*) FROM achado a WHERE 1 = 1{}", sel.sql_e()),
        sel.params(),
    )
}

/// Roda um `SELECT count(*)`.
pub fn contar_linhas(conn: &Connection, sql: &str, params: &[Value]) -> ResultadoDeConsulta<usize> {
    Ok(usize::try_from(escalar(conn, sql, params)?).unwrap_or(0))
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
                    celula_link(tipo_link, &l.hash, &rotulo_do_achado(&l.nome, &l.hash)),
                    celula_estado_achado(&l.estado, &l.decidido_por),
                    celula_texto_opt(&l.tipo),
                    celula_data_opt(&l.data_chave),
                    celula_link_opt(Tipo::Ciclo, &l.ciclo),
                    celula_valor_opt(l.valor),
                    celula_link_opt(Tipo::Fornecedor, &l.fornecedor),
                    celula_contagem(l.origens, de, None, "origens"),
                    celula_texto_opt(&l.onde),
                    celula_texto_opt(&l.no_cofre),
                ];
                celulas.push(if com_documento {
                    l.confianca.map_or(Celula::Vazia, |c| {
                        Celula::Texto(format!("{:.0}%", c * 100.0))
                    })
                } else {
                    celula_texto_opt(&l.notas)
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
    // O último assunto vem da linha de `max(gmail_date)` (coluna "nua" do SQLite): a consulta
    // original do Datasette achava o assunto com uma subconsulta correlacionada por remetente,
    // que varria `origem` inteira por grupo (130 s com 600 mil origens e 1.000 remetentes).
    let sql = format!(
        "SELECT lower(replace(substr(o.gmail_from, instr(o.gmail_from, '@') + 1), '>', '')) AS dominio,
           count(*) AS emails, max(o.gmail_date) AS ultimo, o.gmail_subject AS ultimo_assunto
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
                    celula_texto_opt(&l.dominio),
                    celula_contagem_solta(l.emails),
                    celula_data_opt(&l.ultimo),
                    celula_texto_opt(&l.ultimo_assunto),
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
    // Em duas fases, como `arquivos`: a de dentro ordena e corta só as chaves, pelo índice
    // parcial `origem_gmail_date` (que já tem `content_hash`, então não lê a linha nem junta
    // `achado` ao pular páginas); a de fora monta as colunas só para a página. A junção com
    // `achado` na de dentro só entra quando o filtro olha para o achado.
    let juncao = if sel.menciona("a.") {
        " JOIN achado a ON a.content_hash = o.content_hash"
    } else {
        ""
    };
    let sql = format!(
        "SELECT o.id, a.content_hash, substr(o.gmail_date, 1, 16) AS data, o.gmail_from, o.gmail_subject,
           (SELECT count(*) FROM origem x WHERE x.gmail_message_id = o.gmail_message_id AND x.mensagem_ref = 'anexo') AS anexos,
           a.estado, a.caminho_acervo, a.decidido_por
         FROM (SELECT o.id AS oid, o.gmail_date AS gd FROM origem o{juncao}
                WHERE o.origin_type = 'gmail' AND o.mensagem_ref = 'eml' AND o.content_hash IS NOT NULL{sel}
                ORDER BY o.gmail_date DESC, o.id DESC{limite}) k
           JOIN origem o ON o.id = k.oid
           JOIN achado a ON a.content_hash = o.content_hash
         ORDER BY k.gd DESC, k.oid DESC",
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
                    celula_data_opt(&l.data),
                    celula_texto_opt(&l.de),
                    celula_link(
                        Tipo::Origem,
                        &l.origem_id.to_string(),
                        nao_vazio(&l.assunto).unwrap_or("(sem assunto)"),
                    ),
                    celula_contagem_solta(l.anexos),
                    celula_estado_achado(&l.estado, &l.decidido_por),
                    celula_texto_opt(&l.eml_no_cofre),
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
    // Em duas fases, como `achados`: a de dentro ordena e corta só as chaves, pelo índice
    // `origem_local_mtime` e sem juntar `achado` (a menos que o filtro peça); a de fora monta
    // as colunas para as linhas da página. Pular 500 mil linhas não custa uma junção cada.
    let juncao = if sel.menciona("a.") {
        " LEFT JOIN achado a ON a.content_hash = o.content_hash"
    } else {
        ""
    };
    let sql = format!(
        "SELECT o.id, o.content_hash, o.filename, o.local_filepath, o.file_size_bytes, substr(o.local_mtime, 1, 10),
           a.camada, a.estado, a.document_type, a.caminho_acervo, a.decidido_por
         FROM (SELECT o.id AS oid, o.local_mtime AS lm FROM origem o{juncao}
                WHERE o.origin_type IN ('local_file', 'onedrive', 'gdrive'){sel}
                ORDER BY o.local_mtime DESC, o.id DESC{limite}) k
           JOIN origem o ON o.id = k.oid
           LEFT JOIN achado a ON a.content_hash = o.content_hash
         ORDER BY k.lm DESC, k.oid DESC",
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
                    celula_link(Tipo::Origem, &l.origem_id.to_string(), &l.nome),
                    celula_texto_opt(&l.caminho),
                    l.bytes.map_or(Celula::Vazia, celula_contagem_solta),
                    celula_data_opt(&l.data_arquivo),
                    celula_texto_opt(&l.camada),
                    l.estado
                        .as_deref()
                        .map_or(Celula::Vazia, |e| celula_estado_achado(e, &l.decidido_por)),
                    celula_texto_opt(&l.tipo),
                    celula_texto_opt(&l.no_cofre),
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

/// O que se sabe de uma pasta enquanto as linhas passam.
#[derive(Default)]
struct DaPasta {
    numero: u32,
    achados: i64,
    documentos: i64,
    descartados: i64,
}

/// Os 128 primeiros bits do hash (hexadecimal) como número; texto que não seja hexadecimal
/// cai num hash comum do Rust.
fn chave_do_hash(hash: &str) -> u128 {
    hash.get(..32)
        .and_then(|h| u128::from_str_radix(h, 16).ok())
        .unwrap_or_else(|| {
            use std::hash::{BuildHasher, RandomState};
            u128::from(RandomState::new().hash_one(hash))
        })
}

/// `pastas_pessoais`: achados da camada A por pasta.
///
/// O agrupamento é feito aqui, e não no `GROUP BY` do SQLite: as pastas de um disco pessoal
/// têm caminhos longos que começam igual, e o SQLite ordena e compara a chave inteira a cada
/// linha (2,4 s com 81 mil pastas no inventário real). Uma tabela de dispersão resolve em
/// uma passada. A ordem e as contagens são as da SQL original (`count(DISTINCT hash)` e a
/// soma de estados por origem).
pub fn pastas_pessoais(
    ctx: &Contexto<'_>,
    sel: &Selecao,
) -> ResultadoDeConsulta<Vec<LinhaDePasta>> {
    use std::collections::HashMap;
    let sel_completa = sel.sem_pagina();
    let sql = format!(
        "SELECT substr(o.local_filepath, 1, length(o.local_filepath) - length(o.filename) - 1) AS pasta,
           o.content_hash, a.estado
         FROM achado a CROSS JOIN origem o ON o.content_hash = a.content_hash
         WHERE a.camada = 'A' AND o.local_filepath IS NOT NULL{sel}",
        sel = sel_completa.sql_e(),
    );
    let mut instrucao = ctx.conn.prepare_cached(&sql)?;
    let mut linhas = instrucao.query(rusqlite::params_from_iter(sel_completa.params().iter()))?;
    let mut pastas: HashMap<String, DaPasta> = HashMap::new();
    let mut sem_pasta = DaPasta::default();
    // Um conjunto só para `count(DISTINCT hash)` de todas as pastas: (pasta, hash).
    let mut vistos: std::collections::HashSet<(u32, u128)> = std::collections::HashSet::new();
    let erro = |e: rusqlite::types::FromSqlError| ErroDeConsulta::Banco(e.to_string());
    while let Some(l) = linhas.next()? {
        let pasta = l.get_ref(0)?.as_str_or_null().map_err(erro)?;
        let hash = l
            .get_ref(1)?
            .as_str_or_null()
            .map_err(erro)?
            .unwrap_or_default();
        let estado = l
            .get_ref(2)?
            .as_str_or_null()
            .map_err(erro)?
            .unwrap_or_default();
        // Só aloca a chave na primeira vez que a pasta aparece.
        let proximo = u32::try_from(pastas.len() + 1).unwrap_or(u32::MAX);
        let da_pasta = match pasta {
            Some(p) => match pastas.get_mut(p) {
                Some(d) => d,
                None => pastas.entry(p.to_owned()).or_insert(DaPasta {
                    numero: proximo,
                    ..DaPasta::default()
                }),
            },
            None => &mut sem_pasta,
        };
        if vistos.insert((da_pasta.numero, chave_do_hash(hash))) {
            da_pasta.achados += 1;
        }
        match estado {
            "documento" => da_pasta.documentos += 1,
            "descartado" => da_pasta.descartados += 1,
            _ => {}
        }
    }
    let nula = (sem_pasta.achados > 0).then_some((None, sem_pasta));
    let mut saida: Vec<LinhaDePasta> = pastas
        .into_iter()
        .map(|(pasta, p)| (Some(pasta), p))
        .chain(nula)
        .map(|(pasta, p)| LinhaDePasta {
            pasta,
            achados: p.achados,
            documentos: p.documentos,
            descartados: p.descartados,
        })
        .collect();
    // ORDER BY achados DESC, pasta ASC (NULL primeiro, comparação por bytes).
    saida.sort_by(|a, b| {
        b.achados
            .cmp(&a.achados)
            .then_with(|| a.pasta.cmp(&b.pasta))
    });
    Ok(saida
        .into_iter()
        .skip(sel.deslocamento)
        .take(sel.limite.unwrap_or(usize::MAX))
        .collect())
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
                    celula_texto_opt(&l.pasta),
                    celula_contagem_solta(l.achados),
                    celula_contagem_solta(l.documentos),
                    celula_contagem_solta(l.descartados),
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
                        celula_link(
                            Tipo::Coleta,
                            &l.id.to_string(),
                            &rotulo_da_coleta(&l.etapa, &l.fonte_id, l.id),
                        ),
                        celula_link(Tipo::Conexao, &l.fonte_id, &l.fonte_id),
                        celula_texto(&l.etapa),
                        celula_texto(&l.status),
                        Celula::Data(l.iniciada_em.clone()),
                        celula_data_opt(&l.concluida_em),
                        celula_contagem_solta(l.vistos),
                        celula_contagem_solta(l.novos),
                        celula_contagem_solta(l.repetidos),
                        celula_contagem_solta(l.excluidos),
                        celula_contagem_solta(l.erros),
                        celula_texto(&l.escopo),
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
                        celula_link(
                            Tipo::Fatura,
                            &l.fatura,
                            &rotulo_da_fatura(&l.conta, &l.vencimento),
                        ),
                        celula_link_opt(Tipo::Conta, &l.conta),
                        Celula::Data(l.vencimento.clone()),
                        celula_valor_total(l.total, de.clone(), None, "fatura"),
                        celula_contagem(l.transacoes, de.clone(), None, "transacoes"),
                        celula_valor_total(l.soma_transacoes, de, None, "transacoes"),
                        celula_valor(l.diferenca),
                        celula_texto(&l.situacao),
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
                        celula_link(Tipo::Conta, &de.chave, &de.chave),
                        celula_contagem(l.transacoes, de.clone(), None, "registros"),
                    ),
                    None => (Celula::Vazia, celula_contagem_solta(l.transacoes)),
                };
                let mut meses = l.meses_sem_nada.clone();
                meses.sort();
                Linha {
                    alvo,
                    celulas: vec![
                        conta,
                        transacoes,
                        celula_texto(&l.de),
                        celula_texto(&l.ate),
                        celula_contagem_solta(meses.len() as i64),
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
        Area::Ciclos => Ok(tabela_por_ciclo(&por_ciclo(ctx, &reg())?, filtro)),
        Area::Fornecedores => Ok(tabela_fornecedores(&fornecedores(ctx, &reg())?, filtro)),
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
        Area::EmailsPorRemetente => tabela_de_emails_por_remetente(ctx, filtro),
        Area::PastasPessoais => tabela_de_pastas_pessoais(ctx, filtro),
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

// ---------------------------------------------------------------------------
// Páginas das áreas grandes
// ---------------------------------------------------------------------------

/// A área devolve páginas pelo SQL (`LIMIT`/`OFFSET`)? As outras são agregações pequenas (ou
/// precisam do conjunto inteiro) e a página é um corte da tabela completa.
pub fn area_e_paginavel(area: Area) -> bool {
    matches!(
        area,
        Area::ATriar
            | Area::EmDuvida
            | Area::Documentos
            | Area::Registros
            | Area::Emails
            | Area::ArquivosEPastas
    )
}

/// Quantas linhas a área tem com o filtro, sem montar nenhuma. `None` para as áreas que
/// precisam ser calculadas inteiras.
pub fn contar_da_area(
    ctx: &Contexto<'_>,
    area: Area,
    filtro: &Filtro,
) -> ResultadoDeConsulta<Option<usize>> {
    Ok(Some(match area {
        Area::ATriar | Area::EmDuvida | Area::Documentos => {
            let estado = estado_da_area(area);
            contar_achados(
                ctx,
                &com_estado(&selecao_do_filtro(ctx, filtro, Alvo::Achado), estado),
            )?
        }
        Area::Registros => {
            let sel = selecao_do_filtro(ctx, filtro, Alvo::Registro);
            contar_linhas(
                ctx.conn,
                &format!("SELECT count(*) FROM registro r WHERE 1 = 1{}", sel.sql_e()),
                sel.params(),
            )?
        }
        Area::Emails => {
            let sel = selecao_de_emails(filtro);
            contar_linhas(
                ctx.conn,
                &format!(
                    "SELECT count(*) FROM origem o JOIN achado a ON a.content_hash = o.content_hash
                     WHERE o.origin_type = 'gmail' AND o.mensagem_ref = 'eml'{}",
                    sel.sql_e()
                ),
                sel.params(),
            )?
        }
        Area::ArquivosEPastas => {
            let sel = selecao_de_arquivos(filtro);
            let juncao = if sel.menciona("a.") {
                " LEFT JOIN achado a ON a.content_hash = o.content_hash"
            } else {
                ""
            };
            contar_linhas(
                ctx.conn,
                &format!(
                    "SELECT count(*) FROM origem o{juncao}
                     WHERE o.origin_type IN ('local_file', 'onedrive', 'gdrive'){}",
                    sel.sql_e()
                ),
                sel.params(),
            )?
        }
        _ => return Ok(None),
    }))
}

fn estado_da_area(area: Area) -> &'static str {
    match area {
        Area::ATriar => "novo",
        Area::EmDuvida => "duvida",
        _ => "documento",
    }
}

/// A página de `limite` linhas que começa na linha `inicio` de uma área paginável, com as
/// colunas. Devolve `None` quando a área não é paginável (veja [`area_e_paginavel`]).
pub fn pagina_da_area(
    ctx: &Contexto<'_>,
    area: Area,
    filtro: &Filtro,
    inicio: usize,
    limite: usize,
) -> ResultadoDeConsulta<Option<Tabela>> {
    Ok(Some(match area {
        Area::ATriar | Area::EmDuvida | Area::Documentos => {
            let sel = com_estado(
                &selecao_do_filtro(ctx, filtro, Alvo::Achado),
                estado_da_area(area),
            )
            .com_pagina(inicio, limite);
            let link = if area == Area::Documentos {
                LinkDoAchado::Documento
            } else {
                LinkDoAchado::Achado
            };
            tabela_de_achados(&achados(ctx, &sel)?, link)
        }
        Area::Registros => {
            let sel = selecao_do_filtro(ctx, filtro, Alvo::Registro).com_pagina(inicio, limite);
            tabela_registros(&registros(ctx, &sel)?)
        }
        Area::Emails => {
            let sel = selecao_de_emails(filtro).com_pagina(inicio, limite);
            tabela_emails(&emails(ctx, &sel)?)
        }
        Area::ArquivosEPastas => {
            let sel = selecao_de_arquivos(filtro).com_pagina(inicio, limite);
            tabela_arquivos(&arquivos(ctx, &sel)?)
        }
        _ => return Ok(None),
    }))
}
