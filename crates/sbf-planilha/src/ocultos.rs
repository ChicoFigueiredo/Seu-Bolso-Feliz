//! Itens ocultos: saídas recorrentes que a Pluggy mostra e a planilha não traz (D5).
//!
//! Lê o inventário só para leitura. Um fornecedor presente em pelo menos 5 dos últimos 8
//! ciclos, fora de transferência, pagamento de fatura e descrição genérica, e que ainda não
//! casa com nenhum recorrente, vira um recorrente estimado (fonte `Pluggy`) no grupo
//! "Achados na Pluggy". Um recorrente casado cujo valor difere da mediana em mais de 10%
//! ganha uma versão nova a partir da competência seguinte.

use std::collections::{BTreeMap, HashSet};

use rusqlite::{Connection, params_from_iter};
use sbf_planejamento::{
    Alteracao, Autor, Centavos, Certeza, Competencia, Escopo, Fonte, Frequencia, Grupo,
    MudancaDeVersao, Natureza, NovoRecorrente, Planejamento, Prazo, Recorrente, Versao,
};

use crate::mapa::Mapa;
use crate::texto::normalizar;
use crate::{ErroPlanilha, Resultado};

pub const GRUPO_DOS_ACHADOS: &str = "Achados na Pluggy";

#[derive(Debug, Clone)]
pub struct ConfigOcultos {
    /// Quantos ciclos olhar para trás.
    pub ciclos: usize,
    /// Em quantos deles o fornecedor precisa aparecer.
    pub minimo_de_ciclos: usize,
    /// Os `tipo` do inventário que contam como saída recorrente.
    pub tipos: Vec<String>,
    /// Descrições genéricas de transferência (comparadas já normalizadas, por igualdade
    /// ou como prefixo de palavra).
    pub genericas: Vec<String>,
    /// Diferença, em %, a partir da qual o recorrente casado ganha versão nova.
    pub divergencia_pct: i64,
    /// Último ciclo (`AAAA-MM`) a considerar; `None` = o mais recente do inventário.
    pub ate_ciclo: Option<String>,
}

impl Default for ConfigOcultos {
    fn default() -> Self {
        ConfigOcultos {
            ciclos: 8,
            minimo_de_ciclos: 5,
            tipos: vec!["compra".to_owned(), "debito".to_owned()],
            genericas: [
                "transferencia",
                "transf",
                "pix",
                "ted",
                "doc",
                "saque",
                "deposito",
                "pagamento",
            ]
            .map(str::to_owned)
            .to_vec(),
            divergencia_pct: 10,
            ate_ciclo: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Achado {
    pub fornecedor: String,
    pub ciclos_presentes: usize,
    /// Mediana do total por ciclo.
    pub mediana: Centavos,
    /// Dia do mês mais comum (mediana dos dias).
    pub dia: u8,
    /// A maioria das cobranças é compra no cartão (tipo `compra`): o caixa sai pela fatura.
    pub no_cartao: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Divergencia {
    pub recorrente: String,
    pub nome: String,
    pub versao_base: i64,
    pub valor_atual: Centavos,
    pub valor_pluggy: Centavos,
    pub desde: Competencia,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviaDeOcultos {
    pub competencia: Competencia,
    pub grupo_novo: Option<Grupo>,
    pub novos: Vec<Achado>,
    pub divergentes: Vec<Divergencia>,
}

impl PreviaDeOcultos {
    pub fn contagens(&self) -> String {
        format!(
            "novos={} divergentes={}",
            self.novos.len(),
            self.divergentes.len()
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConfirmacaoDeOcultos {
    pub criados: u32,
    pub editados: u32,
}

fn erro(e: rusqlite::Error) -> ErroPlanilha {
    ErroPlanilha::Inventario(e.to_string())
}

fn mediana(mut v: Vec<i64>) -> i64 {
    v.sort_unstable();
    let n = v.len();
    if n % 2 == 1 {
        v[n / 2]
    } else {
        let soma = v[n / 2 - 1] + v[n / 2];
        // Meio para longe do zero.
        (soma + soma.signum()) / 2
    }
}

fn e_generica(fornecedor: &str, cfg: &ConfigOcultos) -> bool {
    let f = normalizar(fornecedor);
    cfg.genericas
        .iter()
        .map(|g| normalizar(g))
        .any(|g| f == g || f.starts_with(&format!("{g} ")))
}

/// Os fornecedores recorrentes do inventário, sem olhar o planejamento.
pub fn detectar(conn: &Connection, cfg: &ConfigOcultos) -> Resultado<Vec<Achado>> {
    let ciclos: Vec<String> = {
        let mut st = conn
            .prepare(
                "SELECT DISTINCT ciclo_id FROM registro
                 WHERE ciclo_id IS NOT NULL AND (?1 IS NULL OR ciclo_id <= ?1)
                 ORDER BY ciclo_id DESC LIMIT ?2",
            )
            .map_err(erro)?;
        st.query_map(rusqlite::params![cfg.ate_ciclo, cfg.ciclos as i64], |r| {
            r.get(0)
        })
        .map_err(erro)?
        .collect::<Result<_, _>>()
        .map_err(erro)?
    };
    if ciclos.is_empty() || cfg.tipos.is_empty() {
        return Ok(Vec::new());
    }
    let marcas = |n: usize, de: usize| {
        (0..n)
            .map(|i| format!("?{}", de + i + 1))
            .collect::<Vec<_>>()
            .join(",")
    };
    let sql = format!(
        "SELECT fornecedor_normalizado, ciclo_id, valor_centavos, data, tipo FROM registro
         WHERE valor_centavos < 0 AND fornecedor_normalizado IS NOT NULL
           AND fornecedor_normalizado <> '' AND tipo IN ({}) AND ciclo_id IN ({})",
        marcas(cfg.tipos.len(), 0),
        marcas(ciclos.len(), cfg.tipos.len())
    );
    let mut st = conn.prepare(&sql).map_err(erro)?;
    let parametros = cfg.tipos.iter().chain(ciclos.iter());
    let linhas = st
        .query_map(params_from_iter(parametros), |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
            ))
        })
        .map_err(erro)?;

    // fornecedor -> ciclo -> total; fornecedor -> dias
    let mut totais: BTreeMap<String, BTreeMap<String, i64>> = BTreeMap::new();
    let mut dias: BTreeMap<String, Vec<i64>> = BTreeMap::new();
    // fornecedor -> (compras no cartão, cobranças)
    let mut cartao: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for l in linhas {
        let (f, ciclo, valor, data, tipo) = l.map_err(erro)?;
        if e_generica(&f, cfg) {
            continue;
        }
        let c = cartao.entry(f.clone()).or_default();
        c.1 += 1;
        if tipo == "compra" {
            c.0 += 1;
        }
        *totais
            .entry(f.clone())
            .or_default()
            .entry(ciclo)
            .or_default() += -valor;
        if let Some(d) = data.get(8..10).and_then(|d| d.parse::<i64>().ok()) {
            dias.entry(f).or_default().push(d);
        }
    }
    Ok(totais
        .into_iter()
        .filter(|(_, por_ciclo)| por_ciclo.len() >= cfg.minimo_de_ciclos)
        .map(|(f, por_ciclo)| {
            let dia = dias.remove(&f).map_or(1, mediana).clamp(1, 31) as u8;
            let (compras, total) = cartao.get(&f).copied().unwrap_or((0, 0));
            Achado {
                no_cartao: compras * 2 > total,
                ciclos_presentes: por_ciclo.len(),
                mediana: Centavos(mediana(por_ciclo.into_values().collect())),
                dia,
                fornecedor: f,
            }
        })
        .collect())
}

fn parecidos(a: &str, b: &str) -> bool {
    let (a, b) = (normalizar(a), normalizar(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    a == b || (a.len().min(b.len()) >= 5 && (a.contains(&b) || b.contains(&a)))
}

/// Os recorrentes que casam com o fornecedor: pelo fornecedor da versão, pelo nome ou pelo
/// mapa.
fn casados<'a>(f: &str, existentes: &'a [Recorrente], mapa: &Mapa) -> Vec<&'a Recorrente> {
    let do_mapa: HashSet<String> = mapa
        .fornecedores()
        .filter(|(_, forn)| parecidos(forn, f))
        .map(|(d, _)| normalizar(d))
        .collect();
    existentes
        .iter()
        .filter(|r| {
            parecidos(&r.nome, f)
                || r.versoes
                    .iter()
                    .any(|v| v.fornecedor.as_deref().is_some_and(|x| parecidos(x, f)))
                || do_mapa.contains(&normalizar(&r.nome))
        })
        .collect()
}

fn vigente(r: &Recorrente, c: Competencia) -> Option<&Versao> {
    r.versoes
        .iter()
        .rev()
        .find(|v| v.desde <= c)
        .or_else(|| r.versoes.first())
}

/// Detecta e compara com o planejamento, sem gravar.
pub fn previa_de_ocultos(
    conn: &Connection,
    plan: &dyn Planejamento,
    mapa: &Mapa,
    cfg: &ConfigOcultos,
    competencia: Competencia,
) -> Resultado<PreviaDeOcultos> {
    let existentes = plan.recorrentes(false)?;
    let mut novos = Vec::new();
    let mut divergentes = Vec::new();
    for a in detectar(conn, cfg)? {
        if mapa.ignora(&a.fornecedor) {
            continue;
        }
        let m = casados(&a.fornecedor, &existentes, mapa);
        match m.as_slice() {
            // Citado no mapa (item ou extra): já coberto, mesmo sem recorrente criado ainda.
            [] if mapa
                .fornecedores()
                .any(|(_, f)| parecidos(f, &a.fornecedor)) => {}
            [] => novos.push(a),
            [r] if r.natureza == Natureza::Saida => {
                let Some(v) = vigente(r, competencia.seguinte()) else {
                    continue;
                };
                let Some(atual) = v.valor else { continue };
                let diferenca = (a.mediana.0 - atual.0).abs();
                if v.fonte != sbf_planejamento::Fonte::Usuario
                    && diferenca * 100 > atual.0.abs() * cfg.divergencia_pct
                {
                    divergentes.push(Divergencia {
                        recorrente: r.id.clone(),
                        nome: r.nome.clone(),
                        versao_base: r.versao_do_registro,
                        valor_atual: atual,
                        valor_pluggy: a.mediana,
                        desde: competencia.seguinte(),
                    });
                }
            }
            _ => {} // ambíguo ou de outra natureza: não mexe
        }
    }
    let grupo_novo = if novos.is_empty() {
        None
    } else {
        let grupos = plan.grupos()?;
        let existe = grupos
            .iter()
            .any(|g| normalizar(&g.nome) == normalizar(GRUPO_DOS_ACHADOS));
        (!existe).then(|| Grupo {
            nome: GRUPO_DOS_ACHADOS.to_owned(),
            posicao: grupos.iter().map(|g| g.posicao + 1).max().unwrap_or(0),
        })
    };
    Ok(PreviaDeOcultos {
        competencia,
        grupo_novo,
        novos,
        divergentes,
    })
}

pub fn confirmar_ocultos(
    previa: &PreviaDeOcultos,
    plan: &mut dyn Planejamento,
) -> Resultado<ConfirmacaoDeOcultos> {
    let mut c = ConfirmacaoDeOcultos::default();
    if let Some(g) = &previa.grupo_novo {
        plan.aplicar(
            Alteracao::CriarGrupo {
                nome: g.nome.clone(),
                posicao: g.posicao,
            },
            Autor::Importacao,
        )?;
    }
    for (i, a) in previa.novos.iter().enumerate() {
        plan.aplicar(
            Alteracao::CriarRecorrente {
                recorrente: NovoRecorrente {
                    nome: a.fornecedor.clone(),
                    grupo: GRUPO_DOS_ACHADOS.to_owned(),
                    subgrupo: Some(if a.no_cartao { "No cartão" } else { "Débito em conta" }.to_owned()),
                    posicao: i as i64 + 1,
                    natureza: if a.no_cartao { Natureza::DentroDaFatura } else { Natureza::Saida },
                    prazo: Prazo::Desconhecido,
                    primeira_versao: Versao {
                        id: String::new(),
                        desde: previa.competencia,
                        valor: Some(a.mediana),
                        certeza: Certeza::Estimado,
                        fonte: Fonte::Pluggy,
                        frequencia: Frequencia::Mensal { dia: a.dia },
                        conta: None,
                        centro: None,
                        fornecedor: Some(a.fornecedor.clone()),
                        categoria: None,
                        nota: Some(format!("achado na Pluggy em {} ciclos", a.ciclos_presentes)),
                    },
                },
                origem: None,
            },
            Autor::Importacao,
        )?;
        c.criados += 1;
    }
    for d in &previa.divergentes {
        plan.aplicar(
            Alteracao::Editar {
                recorrente: d.recorrente.clone(),
                escopo: Escopo::DaquiEmDiante(d.desde),
                mudanca: MudancaDeVersao {
                    valor: Some(Some(d.valor_pluggy)),
                    certeza: Some(Certeza::Estimado),
                    fonte: Some(Fonte::Pluggy),
                    ..MudancaDeVersao::default()
                },
                versao_base: d.versao_base,
            },
            Autor::Importacao,
        )?;
        c.editados += 1;
    }
    Ok(c)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::falso::Falso;
    use crate::fixtura::{inventario_ficticio, registro};

    const COMP: Competencia = Competencia { ano: 2026, mes: 11 };

    /// Oito ciclos (2026-02 a 2026-09) com os fornecedores do cenário.
    fn inventario() -> Connection {
        let c = inventario_ficticio();
        let mut n = 0;
        for mes in 2..=9 {
            let mut r = |tipo: &str, dia: u8, valor: i64, forn: &str| {
                n += 1;
                registro(
                    &c,
                    &format!("pluggy:txn_{n}"),
                    tipo,
                    &format!("2026-{mes:02}-{dia:02}"),
                    valor,
                    forn,
                );
            };
            r("compra", 25, -3990, "STREAMING X");
            r("compra", 22, -1000, "ACADEMIA Y");
            r("debito", 10, -140_000, "ALUGUEL IMOB");
            r("debito", 12, -18_500, "LUZ ENERGIA");
            r("transferencia_interna", 5, -50_000, "OUTRA CONTA");
            r("pagamento_fatura", 8, -90_000, "NUBANK");
            r("debito", 6, -2000, "PIX JOAO");
            // Duas no mesmo ciclo somam.
            r("compra", 21, -500, "CAFE W");
            r("compra", 23, -500, "CAFE W");
            if mes >= 6 {
                r("compra", 15, -777, "PADARIA Z"); // só 4 ciclos
            }
        }
        c
    }

    fn rec(nome: &str, valor: i64, fornecedor: Option<&str>) -> NovoRecorrente {
        NovoRecorrente {
            nome: nome.to_owned(),
            grupo: "Essencial".to_owned(),
            subgrupo: None,
            posicao: 1,
            natureza: Natureza::Saida,
            prazo: Prazo::Desconhecido,
            primeira_versao: Versao {
                id: String::new(),
                desde: Competencia { ano: 2026, mes: 1 },
                valor: Some(Centavos(valor)),
                certeza: Certeza::Confirmado,
                fonte: Fonte::Planilha,
                frequencia: Frequencia::Mensal { dia: 10 },
                conta: None,
                centro: None,
                fornecedor: fornecedor.map(str::to_owned),
                categoria: None,
                nota: None,
            },
        }
    }

    fn plano() -> Falso {
        let mut p = Falso::default();
        p.aplicar(
            Alteracao::CriarGrupo {
                nome: "Essencial".into(),
                posicao: 0,
            },
            Autor::Importacao,
        )
        .expect("g");
        for r in [
            rec("Aluguel Imob", 120_000, None),
            rec("Luz Exemplo", 18_050, None),
        ] {
            p.aplicar(
                Alteracao::CriarRecorrente {
                    recorrente: r,
                    origem: None,
                },
                Autor::Importacao,
            )
            .expect("r");
        }
        p
    }

    #[test]
    fn detecta_so_o_que_e_recorrente_e_nao_e_transferencia() {
        let a = detectar(&inventario(), &ConfigOcultos::default()).expect("detectar");
        let nomes: Vec<&str> = a.iter().map(|a| a.fornecedor.as_str()).collect();
        assert_eq!(
            nomes,
            [
                "ACADEMIA Y",
                "ALUGUEL IMOB",
                "CAFE W",
                "LUZ ENERGIA",
                "STREAMING X"
            ]
        );
        let cafe = a.iter().find(|a| a.fornecedor == "CAFE W").expect("cafe");
        assert_eq!(cafe.mediana, Centavos(1000));
        assert_eq!(cafe.ciclos_presentes, 8);
        assert_eq!(cafe.dia, 22);
    }

    #[test]
    fn mediana_de_quantidade_par() {
        assert_eq!(mediana(vec![100, 300]), 200);
        assert_eq!(mediana(vec![1, 2]), 2);
        assert_eq!(mediana(vec![5, 1, 9]), 5);
    }

    #[test]
    fn cria_achados_no_fim_e_edita_divergente_a_partir_do_ciclo_seguinte() {
        let mapa =
            Mapa::de_json(r#"{"itens":[{"descricao":"Luz Exemplo","fornecedor":"LUZ ENERGIA"}]}"#)
                .expect("mapa");
        let mut plan = plano();
        let p = previa_de_ocultos(&inventario(), &plan, &mapa, &ConfigOcultos::default(), COMP)
            .expect("previa");
        // Streaming, academia e café são novos; o aluguel casa pelo nome e a luz pelo mapa.
        let novos: Vec<&str> = p.novos.iter().map(|a| a.fornecedor.as_str()).collect();
        assert_eq!(novos, ["ACADEMIA Y", "CAFE W", "STREAMING X"]);
        assert_eq!(
            p.grupo_novo,
            Some(Grupo {
                nome: GRUPO_DOS_ACHADOS.to_owned(),
                posicao: 1
            })
        );
        // Aluguel: 1.200,00 contra 1.400,00 na Pluggy (16%): versão nova. Luz: 2,4%: nada.
        assert_eq!(p.divergentes.len(), 1);
        assert_eq!(p.divergentes[0].nome, "Aluguel Imob");
        assert_eq!(p.divergentes[0].desde, Competencia { ano: 2026, mes: 12 });
        assert_eq!(p.divergentes[0].valor_pluggy, Centavos(140_000));
        assert!(plan.aplicadas.len() == 3, "a prévia não grava");

        let c = confirmar_ocultos(&p, &mut plan).expect("confirmar");
        assert_eq!((c.criados, c.editados), (3, 1));
        let stream = plan
            .recorrentes
            .iter()
            .find(|r| r.nome == "STREAMING X")
            .expect("stream");
        let v = &stream.versoes[0];
        assert_eq!(
            (v.fonte, v.certeza, v.valor),
            (Fonte::Pluggy, Certeza::Estimado, Some(Centavos(3990)))
        );
        assert_eq!(v.frequencia, Frequencia::Mensal { dia: 25 });
        assert_eq!(stream.grupo, GRUPO_DOS_ACHADOS);
        assert_eq!(stream.prazo, Prazo::Desconhecido);
        let al = plan
            .recorrentes
            .iter()
            .find(|r| r.nome == "Aluguel Imob")
            .expect("al");
        assert_eq!(al.versoes.len(), 2);
        assert_eq!(al.versoes[1].desde, Competencia { ano: 2026, mes: 12 });
        assert_eq!(al.versoes[1].certeza, Certeza::Estimado);

        // Rodar de novo não cria nem edita nada.
        let p2 = previa_de_ocultos(&inventario(), &plan, &mapa, &ConfigOcultos::default(), COMP)
            .expect("previa 2");
        assert!(p2.novos.is_empty() && p2.divergentes.is_empty() && p2.grupo_novo.is_none());
    }

    #[test]
    fn mapa_ignora_e_cobre_fornecedores_de_itens_e_extras() {
        let mapa = Mapa::de_json(
            r#"{"itens":[{"descricao":"Ginastica","fornecedor":"ACADEMIA Y"}],
                "extras":[{"nome":"Cafezinho","grupo":"Extras","natureza":"saida","dia":22,"fornecedor":"CAFE W","certeza":"estimado"}],
                "ignorar_fornecedores":["streaming  x","luz energia"]}"#,
        )
        .expect("mapa");
        let plan = plano();
        let p = previa_de_ocultos(&inventario(), &plan, &mapa, &ConfigOcultos::default(), COMP)
            .expect("previa");
        assert!(p.novos.is_empty(), "{:?}", p.novos);
        assert!(p.grupo_novo.is_none());
    }

    #[test]
    fn nao_mexe_no_que_o_usuario_decidiu() {
        let mut plan = plano();
        plan.recorrentes[0].versoes[0].fonte = Fonte::Usuario;
        let p = previa_de_ocultos(
            &inventario(),
            &plan,
            &Mapa::vazio(),
            &ConfigOcultos::default(),
            COMP,
        )
        .expect("previa");
        assert!(p.divergentes.is_empty());
    }
}
