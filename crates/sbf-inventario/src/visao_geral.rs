//! A Visão geral: o painel de contagens de `workers/inventario/src/relatorio.ts`
//! (resumo, etapas, fontes, os últimos 13 ciclos e avisos) como [`Painel`].
//!
//! INV-04: as contagens são as mesmas do `relatorio.ts` para o mesmo banco; o teste
//! `leitura_visao_geral` roda o `relatorio.ts` com `bun` sobre a fixture e compara.
//!
//! O que o `relatorio.ts` lê fora do banco (ferramentas instaladas, `cofre.enc`, pastas de
//! entrada, `painel/estado.json`) não é do banco: o estado manual (etapas abertas ou
//! fechadas, avisos) entra por [`EstadoManual`], lido do mesmo `estado.json`.
//!
//! Dono: agente de leitura.

use std::collections::HashMap;
use std::path::Path;

use serde::Deserialize;

use rusqlite::Connection;

use crate::consulta::{
    Celula, ErroDeConsulta, Linha, Painel, PedidoDeTabela, ResultadoDeConsulta, Secao, Tabela,
};
use crate::referencia::{Ref, Tipo};
use crate::vistas::{
    Contexto, c_contagem, c_contagem_solta, c_link, c_texto, c_texto_opt, coluna, coluna_num,
    consultar, pedido_total,
};

/// O ciclo (AAAA-MM do início) a que uma data `AAAA-MM-DD` pertence, com o dia de corte 20.
/// `None` se o texto não for uma data.
pub fn ciclo_da_data(data: &str) -> Option<String> {
    let ano: i32 = data.get(0..4)?.parse().ok()?;
    let mes: i32 = data.get(5..7)?.parse().ok()?;
    let dia: i32 = data.get(8..10)?.parse().ok()?;
    if !(1..=12).contains(&mes) || !(1..=31).contains(&dia) {
        return None;
    }
    let (a, m) = if dia < 20 {
        if mes == 1 {
            (ano - 1, 12)
        } else {
            (ano, mes - 1)
        }
    } else {
        (ano, mes)
    };
    Some(format!("{a:04}-{m:02}"))
}

/// A data de hoje `AAAA-MM-DD` em America/Sao_Paulo (UTC−3, sem horário de verão desde
/// 2019). O servidor roda em UTC e o usuário em UTC−3: a data ingênua erra a partir das 21h.
pub fn hoje_iso() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0));
    data_de_segundos_unix(segundos - 3 * 3600)
}

/// Converte segundos desde a época Unix em `AAAA-MM-DD` (algoritmo de Howard Hinnant).
fn data_de_segundos_unix(segundos: i64) -> String {
    let dias = segundos.div_euclid(86_400) + 719_468;
    let era = dias.div_euclid(146_097);
    let doe = dias.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let ano = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = if mes <= 2 { ano + 1 } else { ano };
    format!("{ano:04}-{mes:02}-{dia:02}")
}

// ---------------------------------------------------------------------------
// Estado manual (painel/estado.json)
// ---------------------------------------------------------------------------

/// O estado de uma etapa que só o usuário e o agente sabem.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct EtapaManual {
    pub status: String,
    #[serde(default)]
    pub detalhe: Option<String>,
}

/// Um aviso do estado manual.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct AvisoManual {
    /// `info`, `atencao` ou `problema`.
    pub nivel: String,
    pub aviso: String,
    pub desde: String,
}

/// O `<cofre>/painel/estado.json` do `relatorio.ts`.
#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct EstadoManual {
    #[serde(default)]
    pub etapas: HashMap<String, EtapaManual>,
    #[serde(default)]
    pub avisos: Vec<AvisoManual>,
}

impl EstadoManual {
    /// Lê o arquivo; arquivo ausente ou ilegível vale estado vazio.
    pub fn ler(caminho: &Path) -> EstadoManual {
        std::fs::read_to_string(caminho)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }
}

// ---------------------------------------------------------------------------
// As contagens
// ---------------------------------------------------------------------------

/// O `resumo` do `relatorio.ts`.
#[derive(Debug, Clone, PartialEq)]
pub struct Resumo {
    pub achados: i64,
    pub origens: i64,
    pub copiados: i64,
    pub acervo_gb: f64,
    pub documentos: i64,
    pub registros: i64,
    pub pendencias: i64,
    pub fontes_concluidas: i64,
    pub fontes_dentro: i64,
}

/// Uma fonte, com a situação e as contagens da última varredura.
#[derive(Debug, Clone, PartialEq)]
pub struct FonteDaVisaoGeral {
    pub id: String,
    pub descricao: String,
    /// `fora`, o status da varredura (`rodando` primeiro, senão a última) ou `ausente`.
    pub status: String,
    pub vistos: i64,
    pub novos: i64,
    pub copiados: Option<i64>,
    pub erros: i64,
    pub atualizado: Option<String>,
    pub nota: Option<String>,
    // O período que a fonte cobre (`datas` do relatorio.ts).
    pub itens: i64,
    pub periodo_itens: Option<String>,
    pub documentos: i64,
    pub periodo_documentos: Option<String>,
}

/// Uma área deixada de fora de propósito (camada C).
#[derive(Debug, Clone, PartialEq)]
pub struct AreaDeixadaDeFora {
    pub motivo: String,
    pub areas: i64,
    pub arquivos: i64,
    pub bytes: i64,
    pub gb: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CicloDaVisaoGeral {
    pub ciclo: String,
    /// `dd/mm a dd/mm/aaaa`.
    pub periodo: String,
    pub documentos: i64,
    pub registros: i64,
    /// `atual` ou `completo`.
    pub situacao: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EtapaDaVisaoGeral {
    pub etapa: String,
    pub nome: String,
    pub status: String,
    pub detalhe: String,
}

/// Tudo o que a Visão geral mostra, antes de virar [`Painel`].
#[derive(Debug, Clone, PartialEq)]
pub struct VisaoGeral {
    pub resumo: Resumo,
    pub etapas: Vec<EtapaDaVisaoGeral>,
    pub fontes: Vec<FonteDaVisaoGeral>,
    pub areas: Vec<AreaDeixadaDeFora>,
    pub ciclos: Vec<CicloDaVisaoGeral>,
    pub avisos: Vec<AvisoManual>,
}

fn br(iso: &str) -> String {
    format!("{}/{}/{}", &iso[8..10], &iso[5..7], &iso[0..4])
}

fn periodo(de: &Option<String>, ate: &Option<String>) -> Option<String> {
    match (de, ate) {
        (Some(d), Some(a)) if d.len() >= 10 && a.len() >= 10 => Some(if d == a {
            br(d)
        } else {
            format!("{} a {}", br(d), br(a))
        }),
        _ => None,
    }
}

fn arredondar(valor: f64, casas: i32) -> f64 {
    let f = 10f64.powi(casas);
    (valor * f).round() / f
}

/// Fonte, quantas origens, e o período delas (`AAAA-MM-DD`).
type PeriodoPorFonte = Vec<(String, i64, Option<String>, Option<String>)>;
/// As contagens da tabela `achado`: total, copiados, bytes copiados, documentos,
/// documentos com ciclo e dúvidas.
type ContagemDeAchados = (i64, i64, i64, i64, i64, i64);

fn periodo_das_origens(conn: &Connection) -> ResultadoDeConsulta<PeriodoPorFonte> {
    consultar(
        conn,
        "SELECT fonte_id, count(*),
                substr(min(coalesce(nullif(local_mtime, ''), gmail_date)), 1, 10),
                substr(max(coalesce(nullif(local_mtime, ''), gmail_date)), 1, 10)
           FROM origem WHERE coalesce(local_mtime, gmail_date) >= '1995' GROUP BY fonte_id",
        &[],
        |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
    )
}

fn documentos_por_fonte(conn: &Connection) -> ResultadoDeConsulta<PeriodoPorFonte> {
    consultar(
        conn,
        "SELECT x.fonte_id, count(DISTINCT a.content_hash), min(a.data_chave), max(a.data_chave)
           FROM achado a CROSS JOIN origem x ON x.content_hash = a.content_hash
          WHERE a.estado = 'documento' AND a.data_chave >= '1995' GROUP BY x.fonte_id",
        &[],
        |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?, l.get(3)?)),
    )
}

/// Uma só passada pelos achados: todas as contagens do resumo e da etapa E3.
fn contar_achados(conn: &Connection) -> ResultadoDeConsulta<ContagemDeAchados> {
    Ok(consultar(
        conn,
        "SELECT count(*),
                coalesce(sum(caminho_acervo IS NOT NULL), 0),
                coalesce(sum(CASE WHEN caminho_acervo IS NOT NULL THEN file_size_bytes END), 0),
                coalesce(sum(estado = 'documento'), 0),
                coalesce(sum(estado = 'documento' AND ciclo_id IS NOT NULL), 0),
                coalesce(sum(estado = 'duvida'), 0)
           FROM achado",
        &[],
        |l| {
            Ok((
                l.get(0)?,
                l.get(1)?,
                l.get(2)?,
                l.get(3)?,
                l.get(4)?,
                l.get(5)?,
            ))
        },
    )?
    .into_iter()
    .next()
    .unwrap_or_default())
}

/// As três varreduras grandes da Visão geral. Com banco em arquivo rodam em paralelo, cada
/// uma numa conexão de leitura própria (o SQLite lê em paralelo); em memória, em sequência.
fn varreduras_grandes(
    conn: &Connection,
) -> ResultadoDeConsulta<(PeriodoPorFonte, PeriodoPorFonte, ContagemDeAchados)> {
    let Some(caminho) = conn
        .path()
        .filter(|p| !p.is_empty() && !p.contains(":memory:"))
        .map(std::path::PathBuf::from)
    else {
        return Ok((
            periodo_das_origens(conn)?,
            documentos_por_fonte(conn)?,
            contar_achados(conn)?,
        ));
    };
    std::thread::scope(|escopo| {
        let origens = escopo.spawn(|| periodo_das_origens(&crate::banco::abrir_leitura(&caminho)?));
        let documentos =
            escopo.spawn(|| documentos_por_fonte(&crate::banco::abrir_leitura(&caminho)?));
        let achados = contar_achados(conn)?;
        let falha = || ErroDeConsulta::Banco("a leitura em paralelo falhou".to_owned());
        Ok((
            origens.join().map_err(|_| falha())??,
            documentos.join().map_err(|_| falha())??,
            achados,
        ))
    })
}

/// Mede a Visão geral no banco. `hoje` é `AAAA-MM-DD`; `manual` traz o que o banco não sabe.
pub fn medir(
    ctx: &Contexto<'_>,
    hoje: &str,
    manual: &EstadoManual,
) -> ResultadoDeConsulta<VisaoGeral> {
    let um = |sql: &str| crate::vistas::escalar(ctx.conn, sql, &[]);

    // Fontes: status, varreduras e período (as duas consultas do relatorio.ts).
    let mut fontes = consultar(
        ctx.conn,
        "SELECT f.id, f.descricao, f.escopo, f.motivo,
                (SELECT status FROM varredura WHERE fonte_id = f.id
                  ORDER BY (status = 'rodando') DESC, id DESC LIMIT 1) AS status,
                (SELECT max(vistos) FROM varredura WHERE fonte_id = f.id) AS vistos,
                (SELECT sum(novos) FROM varredura WHERE fonte_id = f.id) AS novos,
                (SELECT sum(json_extract(resumo, '$.copiados')) FROM varredura WHERE fonte_id = f.id) AS copiados,
                (SELECT erros FROM varredura WHERE fonte_id = f.id ORDER BY vistos DESC LIMIT 1) AS erros,
                (SELECT max(coalesce(concluida_em, iniciada_em)) FROM varredura WHERE fonte_id = f.id) AS quando
           FROM fonte f
          ORDER BY f.escopo, f.id",
        &[],
        |l| {
            let escopo: String = l.get(2)?;
            let status: Option<String> = l.get(4)?;
            Ok(FonteDaVisaoGeral {
                id: l.get(0)?,
                descricao: l.get(1)?,
                status: if escopo == "fora" {
                    "fora".to_owned()
                } else {
                    status.unwrap_or_else(|| "ausente".to_owned())
                },
                vistos: l.get::<_, Option<i64>>(5)?.unwrap_or(0),
                novos: l.get::<_, Option<i64>>(6)?.unwrap_or(0),
                copiados: l.get(7)?,
                erros: l.get::<_, Option<i64>>(8)?.unwrap_or(0),
                atualizado: l.get(9)?,
                nota: l.get(3)?,
                itens: 0,
                periodo_itens: None,
                documentos: 0,
                periodo_documentos: None,
            })
        },
    )?;

    // As três varreduras grandes (origens, documentos por fonte e achados) não dependem uma da
    // outra: rodam juntas, cada uma na sua conexão de leitura.
    let (por_origem, por_documento, achados_contados) = varreduras_grandes(ctx.conn)?;
    let por_registro = consultar(
        ctx.conn,
        "SELECT fonte_id, count(*), min(data), max(data) FROM registro GROUP BY fonte_id",
        &[],
        |l| {
            Ok((
                l.get::<_, String>(0)?,
                l.get::<_, i64>(1)?,
                l.get::<_, String>(2)?,
                l.get::<_, String>(3)?,
            ))
        },
    )?;
    for f in &mut fontes {
        let o = por_origem.iter().find(|x| x.0 == f.id);
        let d = por_documento.iter().find(|x| x.0 == f.id);
        let r = por_registro.iter().find(|x| x.0 == f.id);
        // `origens ?? registros`, e o período dos itens vem do mesmo lado (`arq_de ?? reg_de`).
        f.itens = o.map(|x| x.1).or(r.map(|x| x.1)).unwrap_or(0);
        let de = o.and_then(|x| x.2.clone()).or(r.map(|x| x.2.clone()));
        let ate = o.and_then(|x| x.3.clone()).or(r.map(|x| x.3.clone()));
        f.periodo_itens = periodo(&de, &ate);
        f.documentos = d.map_or(0, |x| x.1);
        f.periodo_documentos = d.and_then(|x| periodo(&x.2, &x.3));
    }

    let areas = consultar(
        ctx.conn,
        "SELECT motivo, count(*) AS areas, sum(arquivos) AS arquivos, sum(bytes) AS bytes
           FROM area_excluida a
          WHERE a.varredura_id = (SELECT id FROM varredura WHERE fonte_id = a.fonte_id AND etapa = 'E1'
                                   ORDER BY vistos DESC LIMIT 1)
          GROUP BY motivo ORDER BY arquivos DESC",
        &[],
        |l| {
            let bytes: i64 = l.get(3)?;
            Ok(AreaDeixadaDeFora {
                motivo: l.get(0)?,
                areas: l.get(1)?,
                arquivos: l.get(2)?,
                bytes,
                #[allow(clippy::cast_precision_loss)]
                gb: arredondar(bytes as f64 / 1024f64.powi(3), 1),
            })
        },
    )?;

    let ciclo_atual = ciclo_da_data(hoje).unwrap_or_default();
    let ciclos = consultar(
        ctx.conn,
        "SELECT c.id, c.inicio, c.fim,
                (SELECT count(*) FROM achado WHERE ciclo_id = c.id AND estado = 'documento'),
                (SELECT count(*) FROM registro WHERE ciclo_id = c.id)
           FROM ciclo c WHERE c.id <= ? ORDER BY c.id DESC LIMIT 13",
        &[crate::vistas::texto(ciclo_atual.as_str())],
        |l| {
            let id: String = l.get(0)?;
            let inicio: String = l.get(1)?;
            let fim: String = l.get(2)?;
            Ok(CicloDaVisaoGeral {
                periodo: format!(
                    "{}/{} a {}/{}/{}",
                    &inicio[8..],
                    &inicio[5..7],
                    &fim[8..],
                    &fim[5..7],
                    &fim[0..4]
                ),
                situacao: if id == ciclo_atual {
                    "atual"
                } else {
                    "completo"
                }
                .to_owned(),
                ciclo: id,
                documentos: l.get(3)?,
                registros: l.get(4)?,
            })
        },
    )?;

    // Etapas.
    let discos: Vec<&FonteDaVisaoGeral> = fontes
        .iter()
        .filter(|f| f.id.starts_with("disco:"))
        .collect();
    let discos_concluidos = discos.iter().filter(|f| f.status == "concluida").count();
    let (n_achados, copiados, acervo_bytes, docs, docs_com_ciclo, duvidas) = achados_contados;
    let etapas_base = [
        EtapaDaVisaoGeral {
            etapa: "E0".into(),
            nome: "Preparar".into(),
            status: "aberta".into(),
            detalhe: String::new(),
        },
        EtapaDaVisaoGeral {
            etapa: "E1".into(),
            nome: "Scan dos discos".into(),
            status: if discos_concluidos == discos.len() {
                "concluida"
            } else if discos.iter().any(|d| d.status == "rodando") {
                "rodando"
            } else {
                "aberta"
            }
            .into(),
            detalhe: format!("{discos_concluidos} de {} discos concluídos", discos.len()),
        },
        EtapaDaVisaoGeral {
            etapa: "E2".into(),
            nome: "Fontes vivas".into(),
            status: "aberta".into(),
            detalhe: String::new(),
        },
        EtapaDaVisaoGeral {
            etapa: "E3".into(),
            nome: "Organizar".into(),
            status: "aberta".into(),
            detalhe: format!(
                "{} documentos, {} já por ciclo; {} dúvidas na fila de texto/OCR",
                milhar(docs),
                milhar(docs_com_ciclo),
                milhar(duvidas)
            ),
        },
        EtapaDaVisaoGeral {
            etapa: "E4".into(),
            nome: "Fragmentar".into(),
            status: "aguardando".into(),
            detalhe: String::new(),
        },
        EtapaDaVisaoGeral {
            etapa: "E5".into(),
            nome: "Auditoria e cobertura".into(),
            status: "aguardando".into(),
            detalhe: "Relatório às 06:00".into(),
        },
    ];
    let etapas = etapas_base
        .into_iter()
        .map(|mut e| {
            if let Some(m) = manual.etapas.get(&e.etapa) {
                e.status = m.status.clone();
                if let Some(d) = &m.detalhe {
                    e.detalhe = d.clone();
                }
            }
            e
        })
        .collect();

    // Avisos: os do estado manual e os erros de leitura.
    let mut avisos = manual.avisos.clone();
    let com_erro: Vec<&str> = fontes
        .iter()
        .filter(|f| f.erros > 0)
        .map(|f| f.id.as_str())
        .collect();
    if !com_erro.is_empty() {
        avisos.push(AvisoManual {
            nivel: "atencao".to_owned(),
            aviso: format!(
                "Erros de leitura em {} (detalhes nos logs do cofre)",
                com_erro.join(", ")
            ),
            desde: hoje.to_owned(),
        });
    }

    let resumo = Resumo {
        achados: n_achados,
        origens: um("SELECT count(*) FROM origem")?,
        copiados,
        #[allow(clippy::cast_precision_loss)]
        acervo_gb: arredondar(acervo_bytes as f64 / 1024f64.powi(3), 2),
        documentos: docs,
        registros: um("SELECT count(*) FROM registro")?,
        pendencias: um("SELECT count(*) FROM pendencia WHERE resolvida_em IS NULL")?,
        fontes_concluidas: i64::try_from(fontes.iter().filter(|f| f.status == "concluida").count())
            .unwrap_or(0),
        fontes_dentro: i64::try_from(fontes.iter().filter(|f| f.status != "fora").count())
            .unwrap_or(0),
    };

    Ok(VisaoGeral {
        resumo,
        etapas,
        fontes,
        areas,
        ciclos,
        avisos,
    })
}

/// 12345 → `12.345` (pt-BR).
fn milhar(n: i64) -> String {
    let texto = n.abs().to_string();
    let mut saida = String::new();
    for (i, c) in texto.chars().enumerate() {
        if i > 0 && (texto.len() - i).is_multiple_of(3) {
            saida.push('.');
        }
        saida.push(c);
    }
    if n < 0 { format!("-{saida}") } else { saida }
}

/// Decimal com vírgula: `1.5` → `1,5`.
fn decimal(valor: f64, casas: usize) -> String {
    format!("{valor:.casas$}").replace('.', ",")
}

fn pedido_de_area(area: crate::consulta::Area) -> Box<PedidoDeTabela> {
    Box::new(PedidoDeTabela::Area {
        area,
        filtro: crate::consulta::Filtro::default(),
    })
}

/// Monta o [`Painel`] da Visão geral.
pub fn painel(visao: &VisaoGeral) -> Painel {
    use crate::consulta::Area;
    let todos_achados = Ref::nova(Tipo::Achado, "*");
    let contagem_de_area = |n: i64, area| Celula::Contagem {
        n,
        itens: Some(pedido_de_area(area)),
    };
    let r = &visao.resumo;

    let resumo = Tabela {
        colunas: vec![coluna("Indicador"), coluna_num("Quantidade")],
        linhas: vec![
            linha_de_resumo(
                "Achados",
                c_contagem(r.achados, todos_achados.clone(), None, "achados"),
            ),
            linha_de_resumo(
                "Origens",
                c_contagem(r.origens, Ref::nova(Tipo::Origem, "*"), None, "origens"),
            ),
            linha_de_resumo(
                "Copiados para o acervo",
                c_contagem(r.copiados, todos_achados, None, "copiados"),
            ),
            linha_de_resumo(
                "Acervo",
                Celula::Texto(format!("{} GB", decimal(r.acervo_gb, 2))),
            ),
            linha_de_resumo(
                "Documentos",
                contagem_de_area(r.documentos, Area::Documentos),
            ),
            linha_de_resumo(
                "Registros · a conferir",
                contagem_de_area(r.registros, Area::Registros),
            ),
            linha_de_resumo(
                "Pendências abertas",
                Celula::Contagem {
                    n: r.pendencias,
                    itens: Some(pedido_total(
                        Ref::nova(Tipo::Pendencia, "*"),
                        None,
                        "abertas",
                    )),
                },
            ),
            linha_de_resumo(
                "Fontes concluídas",
                Celula::Texto(format!("{} de {}", r.fontes_concluidas, r.fontes_dentro)),
            ),
        ],
    };

    let etapas = Tabela {
        colunas: vec![
            coluna("Etapa"),
            coluna("Nome"),
            coluna("Situação"),
            coluna("Detalhe"),
        ],
        linhas: visao
            .etapas
            .iter()
            .map(|e| Linha {
                alvo: None,
                celulas: vec![
                    c_texto(&e.etapa),
                    c_texto(&e.nome),
                    c_texto(&e.status),
                    c_texto(&e.detalhe),
                ],
            })
            .collect(),
    };

    let fontes = Tabela {
        colunas: vec![
            coluna("Conexão"),
            coluna("Situação"),
            coluna_num("Vistos"),
            coluna_num("Novos"),
            coluna_num("Copiados"),
            coluna_num("Erros"),
            coluna("Atualizado"),
            coluna_num("Itens"),
            coluna("Período dos itens"),
            coluna_num("Documentos"),
            coluna("Período dos documentos"),
            coluna("Nota"),
        ],
        linhas: visao
            .fontes
            .iter()
            .map(|f| {
                let de = Ref::nova(Tipo::Conexao, f.id.as_str());
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        c_link(Tipo::Conexao, &f.id, &f.descricao),
                        c_texto(&f.status),
                        c_contagem_solta(f.vistos),
                        c_contagem_solta(f.novos),
                        f.copiados.map_or(Celula::Vazia, c_contagem_solta),
                        c_contagem_solta(f.erros),
                        f.atualizado
                            .as_ref()
                            .map_or(Celula::Vazia, |q| Celula::Data(q.clone())),
                        c_contagem_solta(f.itens),
                        c_texto_opt(&f.periodo_itens),
                        c_contagem_solta(f.documentos),
                        c_texto_opt(&f.periodo_documentos),
                        c_texto_opt(&f.nota),
                    ],
                }
            })
            .collect(),
    };

    let areas = Tabela {
        colunas: vec![
            coluna("Motivo"),
            coluna_num("Áreas"),
            coluna_num("Arquivos"),
            coluna_num("GB"),
        ],
        linhas: visao
            .areas
            .iter()
            .map(|a| Linha {
                alvo: None,
                celulas: vec![
                    c_texto(&a.motivo),
                    c_contagem_solta(a.areas),
                    c_contagem_solta(a.arquivos),
                    Celula::Texto(decimal(a.gb, 1)),
                ],
            })
            .collect(),
    };

    let ciclos = Tabela {
        colunas: vec![
            coluna("Ciclo"),
            coluna("Período"),
            coluna_num("Documentos"),
            coluna_num("Registros · a conferir"),
            coluna("Situação"),
        ],
        linhas: visao
            .ciclos
            .iter()
            .map(|c| {
                let de = Ref::nova(Tipo::Ciclo, c.ciclo.as_str());
                Linha {
                    alvo: Some(de.clone()),
                    celulas: vec![
                        c_link(Tipo::Ciclo, &c.ciclo, &c.ciclo),
                        c_texto(&c.periodo),
                        c_contagem(c.documentos, de.clone(), Some(&c.ciclo), "documentos"),
                        c_contagem(c.registros, de, Some(&c.ciclo), "registros"),
                        c_texto(&c.situacao),
                    ],
                }
            })
            .collect(),
    };

    Painel {
        secoes: vec![
            Secao {
                titulo: "Resumo".to_owned(),
                nota: None,
                tabela: resumo,
            },
            Secao {
                titulo: "Etapas".to_owned(),
                nota: None,
                tabela: etapas,
            },
            Secao {
                titulo: "Fontes".to_owned(),
                nota: Some("Situação e período de cada conexão do inventário.".to_owned()),
                tabela: fontes,
            },
            Secao {
                titulo: "Áreas deixadas de fora".to_owned(),
                nota: Some("Vistas e excluídas de propósito, com contagem.".to_owned()),
                tabela: areas,
            },
            Secao {
                titulo: "Últimos 13 ciclos".to_owned(),
                nota: None,
                tabela: ciclos,
            },
        ],
        avisos: visao
            .avisos
            .iter()
            .map(|a| format!("{}: {}", rotulo_do_nivel(&a.nivel), a.aviso))
            .collect(),
    }
}

fn rotulo_do_nivel(nivel: &str) -> &'static str {
    match nivel {
        "problema" => "Problema",
        "atencao" => "Atenção",
        _ => "Aviso",
    }
}

fn linha_de_resumo(rotulo: &str, valor: Celula) -> Linha {
    Linha {
        alvo: None,
        celulas: vec![Celula::Texto(rotulo.to_owned()), valor],
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn ciclo_corta_no_dia_20() {
        assert_eq!(ciclo_da_data("2026-10-19").as_deref(), Some("2026-09"));
        assert_eq!(ciclo_da_data("2026-10-20").as_deref(), Some("2026-10"));
        assert_eq!(ciclo_da_data("2026-01-05").as_deref(), Some("2025-12"));
        assert_eq!(ciclo_da_data("2026-11-13").as_deref(), Some("2026-10"));
        assert_eq!(ciclo_da_data("lixo"), None);
    }

    #[test]
    fn data_de_segundos_unix_conhecida() {
        assert_eq!(data_de_segundos_unix(0), "1970-01-01");
        assert_eq!(data_de_segundos_unix(1_784_000_000), "2026-07-14");
        // Um segundo antes da meia-noite UTC ainda é o dia anterior.
        assert_eq!(data_de_segundos_unix(86_399), "1970-01-01");
        assert_eq!(data_de_segundos_unix(86_400), "1970-01-02");
    }

    #[test]
    fn milhar_com_ponto() {
        assert_eq!(milhar(0), "0");
        assert_eq!(milhar(999), "999");
        assert_eq!(milhar(12_345), "12.345");
        assert_eq!(milhar(1_234_567), "1.234.567");
    }
}
