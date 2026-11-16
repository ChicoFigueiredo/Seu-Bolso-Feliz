//! Sugestões de conciliação: para cada ocorrência do período, procura no inventário a
//! transação de valor igual (ou bem próximo) na janela do vencimento e grava
//! `Conciliar { confirmada: false }` (D15). Uma transação serve a uma só ocorrência.

use std::collections::HashSet;

use rusqlite::Connection;
use sbf_planejamento::{
    Alteracao, Autor, Centavos, Conciliacao, Data, EstadoDaOcorrencia, Filtro, Natureza,
    Ocorrencia, Periodo, Planejamento, Sentido,
};

use crate::mapa::Mapa;
use crate::texto::normalizar;
use crate::{ErroPlanilha, Resultado};

#[derive(Debug, Clone)]
pub struct ConfigConciliacao {
    /// Dias antes e depois do vencimento.
    pub janela_dias: i64,
    /// Tolerância relativa, em %.
    pub tolerancia_pct: i64,
    /// Tolerância absoluta mínima. Vale a maior das duas.
    pub tolerancia_minima: Centavos,
    /// `tipo` do inventário que nunca é pagamento de compromisso.
    pub tipos_excluidos: Vec<String>,
}

impl Default for ConfigConciliacao {
    fn default() -> Self {
        ConfigConciliacao {
            janela_dias: 5,
            tolerancia_pct: 1,
            tolerancia_minima: Centavos(100),
            tipos_excluidos: ["transferencia_interna", "pagamento_fatura", "investimento", "estorno"]
                .map(str::to_owned)
                .to_vec(),
        }
    }
}

#[derive(Debug, Clone)]
struct Transacao {
    id: i64,
    referencia: String,
    data: Data,
    valor: i64,
    fornecedor: Option<String>,
}

fn erro(e: rusqlite::Error) -> ErroPlanilha {
    ErroPlanilha::Inventario(e.to_string())
}

fn conta_para_conciliar(o: &Ocorrencia) -> bool {
    !matches!(o.natureza, Natureza::DescontoEmFolha)
        && o.conciliado.is_none()
        && !o.sugestao_de_conciliacao
        && matches!(
            o.estado,
            EstadoDaOcorrencia::Prevista
                | EstadoDaOcorrencia::PagoInformado
                | EstadoDaOcorrencia::Parcial
                | EstadoDaOcorrencia::EmAtraso
                | EstadoDaOcorrencia::Adiada
        )
}

fn entra(o: &Ocorrencia) -> bool {
    matches!(o.natureza, Natureza::Entrada | Natureza::Passagem { sentido: Sentido::Entrada })
}

/// Calcula as sugestões, sem gravar.
pub fn sugerir(
    conn: &Connection,
    plan: &dyn Planejamento,
    periodo: &Periodo,
    mapa: &Mapa,
    cfg: &ConfigConciliacao,
    hoje: Data,
) -> Resultado<Vec<Conciliacao>> {
    let controle = plan.controle(periodo, Filtro::Todos, None, hoje)?;
    let ocorrencias: Vec<&Ocorrencia> = controle
        .linhas
        .iter()
        .chain(controle.atrasadas_de_antes.iter())
        .filter(|o| conta_para_conciliar(o) && o.previsto.or(o.pago_informado).is_some_and(|v| v.0 > 0))
        .collect();
    let (Some(menor), Some(maior)) = (
        ocorrencias.iter().map(|o| o.vencimento).min(),
        ocorrencias.iter().map(|o| o.vencimento).max(),
    ) else {
        return Ok(Vec::new());
    };
    let de = menor.somar_dias(-cfg.janela_dias);
    let ate = maior.somar_dias(cfg.janela_dias);
    let transacoes = transacoes_entre(conn, de, ate, cfg)?;

    // Todos os pares possíveis, do melhor ao pior; cada lado serve uma vez.
    let mut pares: Vec<(i64, i64, usize, usize)> = Vec::new(); // (diferença, dias, ocorrência, transação)
    for (i, o) in ocorrencias.iter().enumerate() {
        let alvo = o.previsto.or(o.pago_informado).map_or(0, |v| v.0);
        let tolerancia = (alvo * cfg.tolerancia_pct / 100).max(cfg.tolerancia_minima.0);
        let forn = mapa
            .item(&o.nome)
            .and_then(|m| m.fornecedor.clone())
            .or_else(|| o.fornecedor.clone())
            .map(|f| normalizar(&f));
        for (j, t) in transacoes.iter().enumerate() {
            if (t.valor > 0) != entra(o) {
                continue;
            }
            let dias = o.vencimento.dias_ate(t.data).abs();
            let diferenca = (t.valor.abs() - alvo).abs();
            if dias > cfg.janela_dias || diferenca > tolerancia {
                continue;
            }
            if let Some(f) = &forn
                && t.fornecedor.as_deref().map(normalizar).as_ref() != Some(f)
            {
                continue;
            }
            pares.push((diferenca, dias, i, j));
        }
    }
    pares.sort_by_key(|&(d, dias, i, j)| (d, dias, i, transacoes[j].id));
    // Transação disputada por duas ocorrências com a mesma pontuação é ambígua: não se
    // sugere (o usuário decide), em vez de sortear e repetir a sugestão a cada rodada.
    let ambiguas: HashSet<usize> = pares
        .iter()
        .filter(|a| {
            pares
                .iter()
                .any(|b| b.3 == a.3 && b.2 != a.2 && (b.0, b.1) == (a.0, a.1))
        })
        .map(|a| a.3)
        .collect();
    pares.retain(|p| !ambiguas.contains(&p.3));
    let mut ocorrencias_usadas = HashSet::new();
    let mut transacoes_usadas = HashSet::new();
    let mut sugestoes = Vec::new();
    for (_, _, i, j) in pares {
        if ocorrencias_usadas.contains(&i) || transacoes_usadas.contains(&j) {
            continue;
        }
        ocorrencias_usadas.insert(i);
        transacoes_usadas.insert(j);
        let t = &transacoes[j];
        sugestoes.push(Conciliacao {
            ocorrencia: ocorrencias[i].id.clone(),
            movimento: t.referencia.clone(),
            valor: Centavos(t.valor.abs()),
            data: t.data,
            confirmada: false,
        });
    }
    sugestoes.sort_by(|a, b| a.ocorrencia.cmp(&b.ocorrencia));
    Ok(sugestoes)
}

/// Calcula e grava as sugestões. Devolve quantas gravou.
pub fn conciliar(
    conn: &Connection,
    plan: &mut dyn Planejamento,
    periodo: &Periodo,
    mapa: &Mapa,
    cfg: &ConfigConciliacao,
    hoje: Data,
) -> Resultado<usize> {
    let sugestoes = sugerir(conn, &*plan, periodo, mapa, cfg, hoje)?;
    let n = sugestoes.len();
    for s in sugestoes {
        plan.aplicar(Alteracao::Conciliar { conciliacao: s }, Autor::Importacao)?;
    }
    Ok(n)
}

fn transacoes_entre(
    conn: &Connection,
    de: Data,
    ate: Data,
    cfg: &ConfigConciliacao,
) -> Resultado<Vec<Transacao>> {
    let iso = |d: Data| format!("{:04}-{:02}-{:02}", d.ano, d.mes, d.dia);
    let mut st = conn
        .prepare(
            "SELECT id, external_ref, tipo, data, valor_centavos, fornecedor_normalizado
             FROM registro
             WHERE external_ref IS NOT NULL AND valor_centavos <> 0 AND data >= ?1 AND data <= ?2",
        )
        .map_err(erro)?;
    let linhas = st
        .query_map(rusqlite::params![iso(de), iso(ate)], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
                r.get::<_, Option<String>>(5)?,
            ))
        })
        .map_err(erro)?;
    let mut saida = Vec::new();
    for l in linhas {
        let (id, referencia, tipo, data, valor, fornecedor) = l.map_err(erro)?;
        if cfg.tipos_excluidos.contains(&tipo) {
            continue;
        }
        let Some(data) = Data::de_iso(&data) else { continue };
        saida.push(Transacao { id, referencia, data, valor, fornecedor });
    }
    Ok(saida)
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::falso::Falso;
    use crate::fixtura::{inventario_ficticio, registro};
    use sbf_planejamento::{Certeza, Competencia, Fonte, IdOcorrencia};

    fn ocorrencia(rec: &str, nome: &str, dia: u8, previsto: i64) -> Ocorrencia {
        Ocorrencia {
            id: IdOcorrencia {
                recorrente: rec.to_owned(),
                competencia: Competencia { ano: 2026, mes: 11 },
                parcela: None,
            },
            nome: nome.to_owned(),
            grupo: "Essencial".to_owned(),
            subgrupo: None,
            posicao: 1,
            natureza: Natureza::Saida,
            vencimento: Data::nova(2026, 11, dia).expect("data"),
            vencimento_original: Data::nova(2026, 11, dia).expect("data"),
            previsto: Some(Centavos(previsto)),
            certeza: Certeza::Confirmado,
            fonte: Fonte::Planilha,
            pago_informado: None,
            conciliado: None,
            estado: EstadoDaOcorrencia::Prevista,
            parcela: None,
            conta: None,
            centro: None,
            fornecedor: None,
            versao: "v".to_owned(),
            prazo_desconhecido: false,
            sugestao_de_conciliacao: false,
        }
    }

    fn periodo() -> Periodo {
        Periodo::MesCivil(Competencia { ano: 2026, mes: 11 })
    }

    const HOJE: Data = Data { ano: 2026, mes: 11, dia: 16 };

    fn cenario() -> (Connection, Falso) {
        let c = inventario_ficticio();
        // Aluguel: valor igual, 2 dias depois. Luz: 0,5% acima, na janela.
        registro(&c, "pluggy:txn_a", "debito", "2026-11-07", -150_000, "IMOBILIARIA");
        registro(&c, "pluggy:txn_l", "debito", "2026-11-10", -18_140, "ENERGIA");
        // Fora da janela (6 dias), valor errado e transferência interna do mesmo valor.
        registro(&c, "pluggy:txn_longe", "debito", "2026-11-18", -9_000, "NET");
        registro(&c, "pluggy:txn_errado", "debito", "2026-11-15", -9_500, "NET");
        registro(&c, "pluggy:txn_interna", "transferencia_interna", "2026-11-15", -9_000, "NET");
        // Duas ocorrências iguais disputam uma transação só.
        registro(&c, "pluggy:txn_s1", "compra", "2026-11-20", -3_990, "STREAMING");
        let mut p = Falso::default();
        p.ocorrencias = vec![
            ocorrencia("r1", "Aluguel", 5, 150_000),
            ocorrencia("r2", "Luz", 12, 18_050),
            ocorrencia("r3", "Internet", 12, 9_000),
            ocorrencia("r4", "Streaming A", 20, 3_990),
            ocorrencia("r5", "Streaming B", 20, 3_990),
        ];
        (c, p)
    }

    #[test]
    fn casa_por_valor_e_janela_uma_transacao_por_ocorrencia() {
        let (c, p) = cenario();
        let s = sugerir(&c, &p, &periodo(), &Mapa::vazio(), &ConfigConciliacao::default(), HOJE).expect("sugerir");
        let por_rec = |r: &str| s.iter().find(|x| x.ocorrencia.recorrente == r);
        assert_eq!(por_rec("r1").expect("aluguel").movimento, "pluggy:txn_a");
        assert_eq!(por_rec("r1").expect("aluguel").valor, Centavos(150_000));
        assert_eq!(por_rec("r2").expect("luz").movimento, "pluggy:txn_l");
        // Internet: nenhuma serve (longe, errada, interna).
        assert!(por_rec("r3").is_none());
        // Duas ocorrências iguais disputam a mesma transação: ambígua, nenhuma leva.
        assert_eq!(s.iter().filter(|x| x.movimento == "pluggy:txn_s1").count(), 0);
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| !x.confirmada));
    }

    #[test]
    fn mapa_exige_o_fornecedor() {
        let (c, p) = cenario();
        let mapa = Mapa::de_json(r#"{"itens":[{"descricao":"Aluguel","fornecedor":"OUTRA IMOBILIARIA"},{"descricao":"Luz","fornecedor":"energia"}]}"#).expect("mapa");
        let s = sugerir(&c, &p, &periodo(), &mapa, &ConfigConciliacao::default(), HOJE).expect("sugerir");
        assert!(s.iter().all(|x| x.ocorrencia.recorrente != "r1"));
        assert!(s.iter().any(|x| x.ocorrencia.recorrente == "r2"));
    }

    #[test]
    fn grava_como_sugestao_e_nao_repete_o_que_ja_tem_sugestao() {
        let (c, mut p) = cenario();
        let n = conciliar(&c, &mut p, &periodo(), &Mapa::vazio(), &ConfigConciliacao::default(), HOJE).expect("conciliar");
        assert_eq!(n, 2);
        assert_eq!(p.conciliacoes.len(), 2);
        assert!(p.conciliacoes.iter().all(|x| !x.confirmada));
        assert!(p.aplicadas.iter().all(|(_, a)| *a == Autor::Importacao));
        // O serviço marca as sugeridas; o que já tem sugestão ou conciliação é pulado.
        for o in &mut p.ocorrencias {
            if p.conciliacoes.iter().any(|x| x.ocorrencia == o.id) {
                o.sugestao_de_conciliacao = true;
            }
        }
        let de_novo = conciliar(&c, &mut p, &periodo(), &Mapa::vazio(), &ConfigConciliacao::default(), HOJE).expect("de novo");
        assert_eq!(de_novo, 0);
    }

    #[test]
    fn desconto_em_folha_e_entrada_com_sinal_certo() {
        let (c, mut p) = cenario();
        p.ocorrencias[0].natureza = Natureza::DescontoEmFolha;
        registro(&c, "pluggy:txn_entrada", "credito", "2026-11-05", 150_000, "EMPRESA");
        let s = sugerir(&c, &p, &periodo(), &Mapa::vazio(), &ConfigConciliacao::default(), HOJE).expect("sugerir");
        assert!(s.iter().all(|x| x.ocorrencia.recorrente != "r1"));
        p.ocorrencias[0].natureza = Natureza::Entrada;
        let s = sugerir(&c, &p, &periodo(), &Mapa::vazio(), &ConfigConciliacao::default(), HOJE).expect("sugerir");
        assert_eq!(s.iter().find(|x| x.ocorrencia.recorrente == "r1").expect("entrada").movimento, "pluggy:txn_entrada");
    }
}
