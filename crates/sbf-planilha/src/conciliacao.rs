//! Sugestões de conciliação para o CLI (`sbf planejamento conciliar`): um adaptador fino sobre
//! o módulo puro [`sbf_planejamento::conciliacao`]. Para cada ocorrência do período, procura no
//! inventário o movimento de valor igual (ou bem próximo) na janela do vencimento, na conta
//! certa quando há par confirmado, ou pela assinatura, e grava
//! `Conciliar { confirmada: false }` (D15). Um movimento serve a uma só ocorrência.
//!
//! O serviço (`Planejamento::buscar_conciliacoes`) faz o mesmo sem passar por aqui; este
//! adaptador existe porque o CLI ainda aceita o mapa de fornecedores da planilha, que corta
//! os candidatos por fornecedor.

use std::collections::{HashMap, HashSet};

use rusqlite::Connection;
use sbf_planejamento::conciliacao::{self, Alvo, pode_conciliar, tem_como_casar, valor_esperado};
use sbf_planejamento::{
    Alteracao, Autor, Conciliacao, Data, Filtro, MovimentoDoExtrato, Ocorrencia, Periodo,
    Planejamento,
};

use crate::Resultado;
use crate::mapa::Mapa;
use crate::movimentos::registros_entre;
use crate::texto::normalizar;

/// Os parâmetros da conciliação (os do módulo puro).
pub type ConfigConciliacao = conciliacao::Config;

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
    let pares: HashMap<String, String> = plan
        .contas()?
        .into_iter()
        .filter_map(|c| Some((c.apelido, c.conta_do_extrato?)))
        .collect();
    let mut alvos = Vec::new();
    for o in controle
        .linhas
        .iter()
        .chain(controle.atrasadas_de_antes.iter())
        .filter(|o| pode_conciliar(o))
    {
        let composicao = plan.composicao(&o.id.recorrente, o.id.competencia)?;
        let alvo = Alvo {
            esperado: valor_esperado(o, composicao.as_ref()),
            conta_exigida: o
                .conta
                .as_deref()
                .and_then(|a| pares.get(a.trim()))
                .cloned(),
            assinatura: plan.assinatura(&o.id.recorrente)?,
            ocorrencia: o.clone(),
        };
        if tem_como_casar(&alvo) {
            alvos.push(alvo);
        }
    }
    let (Some(menor), Some(maior)) = (
        alvos.iter().map(|a| a.ocorrencia.vencimento).min(),
        alvos.iter().map(|a| a.ocorrencia.vencimento).max(),
    ) else {
        return Ok(Vec::new());
    };
    let janela = alvos
        .iter()
        .filter_map(|a| a.assinatura.as_ref())
        .map(|a| i64::from(a.janela_dias))
        .max()
        .unwrap_or(0)
        .max(cfg.janela_dias);
    let registros = registros_entre(conn, menor.somar_dias(-janela), maior.somar_dias(janela))?;
    let fornecedores: HashMap<&str, Option<&str>> = registros
        .iter()
        .map(|r| (r.movimento.movimento.as_str(), r.fornecedor.as_deref()))
        .collect();
    let movimentos: Vec<MovimentoDoExtrato> =
        registros.iter().map(|r| r.movimento.clone()).collect();
    // O mapa da planilha, quando diz o fornecedor do item, exige o mesmo no movimento.
    let filtro = |o: &Ocorrencia, m: &MovimentoDoExtrato| {
        let Some(f) = mapa
            .item(&o.nome)
            .and_then(|i| i.fornecedor.clone())
            .or_else(|| o.fornecedor.clone())
            .map(|f| normalizar(&f))
        else {
            return true;
        };
        fornecedores
            .get(m.movimento.as_str())
            .copied()
            .flatten()
            .map(normalizar)
            .as_ref()
            == Some(&f)
    };
    // O adaptador não enxerga as conciliações já gravadas (o trait não as lista); o serviço
    // (`buscar_conciliacoes`) sim. Aqui só valem o `pode_conciliar` e as ocorrências marcadas.
    Ok(conciliacao::sugerir(
        &alvos,
        &movimentos,
        &HashSet::new(),
        cfg,
        Some(&filtro),
    ))
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

#[cfg(test)]
mod testes {
    use super::*;
    use crate::falso::Falso;
    use crate::fixtura::{inventario_ficticio, registro, registro_completo};
    use sbf_planejamento::Centavos;
    use sbf_planejamento::{
        Assinatura, Certeza, Competencia, Composicao, ContaDoPlanejamento, EstadoDaOcorrencia,
        Fonte, IdOcorrencia, Natureza, Rubrica, TipoDeRubrica,
    };

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

    const HOJE: Data = Data {
        ano: 2026,
        mes: 11,
        dia: 16,
    };

    fn cenario() -> (Connection, Falso) {
        let c = inventario_ficticio();
        // Aluguel: valor igual, 2 dias depois. Luz: 0,5% acima, na janela.
        registro(
            &c,
            "pluggy:txn_a",
            "debito",
            "2026-11-07",
            -150_000,
            "IMOBILIARIA",
        );
        registro(
            &c,
            "pluggy:txn_l",
            "debito",
            "2026-11-10",
            -18_140,
            "ENERGIA",
        );
        // Fora da janela (6 dias), valor errado e transferência interna do mesmo valor.
        registro(
            &c,
            "pluggy:txn_longe",
            "debito",
            "2026-11-18",
            -9_000,
            "NET",
        );
        registro(
            &c,
            "pluggy:txn_errado",
            "debito",
            "2026-11-15",
            -9_500,
            "NET",
        );
        registro(
            &c,
            "pluggy:txn_interna",
            "transferencia_interna",
            "2026-11-15",
            -9_000,
            "NET",
        );
        // Duas ocorrências iguais disputam uma transação só.
        registro(
            &c,
            "pluggy:txn_s1",
            "compra",
            "2026-11-20",
            -3_990,
            "STREAMING",
        );
        let p = Falso {
            ocorrencias: vec![
                ocorrencia("r1", "Aluguel", 5, 150_000),
                ocorrencia("r2", "Luz", 12, 18_050),
                ocorrencia("r3", "Internet", 12, 9_000),
                ocorrencia("r4", "Streaming A", 20, 3_990),
                ocorrencia("r5", "Streaming B", 20, 3_990),
            ],
            ..Falso::default()
        };
        (c, p)
    }

    #[test]
    fn casa_por_valor_e_janela_uma_transacao_por_ocorrencia() {
        let (c, p) = cenario();
        let s = sugerir(
            &c,
            &p,
            &periodo(),
            &Mapa::vazio(),
            &ConfigConciliacao::default(),
            HOJE,
        )
        .expect("sugerir");
        let por_rec = |r: &str| s.iter().find(|x| x.ocorrencia.recorrente == r);
        assert_eq!(por_rec("r1").expect("aluguel").movimento, "pluggy:txn_a");
        assert_eq!(por_rec("r1").expect("aluguel").valor, Centavos(150_000));
        assert_eq!(por_rec("r2").expect("luz").movimento, "pluggy:txn_l");
        // Internet: nenhuma serve (longe, errada, interna).
        assert!(por_rec("r3").is_none());
        // Duas ocorrências iguais disputam a mesma transação: ambígua, nenhuma leva.
        assert_eq!(
            s.iter().filter(|x| x.movimento == "pluggy:txn_s1").count(),
            0
        );
        assert_eq!(s.len(), 2);
        assert!(s.iter().all(|x| !x.confirmada));
    }

    #[test]
    fn mapa_exige_o_fornecedor() {
        let (c, p) = cenario();
        let mapa = Mapa::de_json(r#"{"itens":[{"descricao":"Aluguel","fornecedor":"OUTRA IMOBILIARIA"},{"descricao":"Luz","fornecedor":"energia"}]}"#).expect("mapa");
        let s = sugerir(
            &c,
            &p,
            &periodo(),
            &mapa,
            &ConfigConciliacao::default(),
            HOJE,
        )
        .expect("sugerir");
        assert!(s.iter().all(|x| x.ocorrencia.recorrente != "r1"));
        assert!(s.iter().any(|x| x.ocorrencia.recorrente == "r2"));
    }

    #[test]
    fn grava_como_sugestao_e_nao_repete_o_que_ja_tem_sugestao() {
        let (c, mut p) = cenario();
        let n = conciliar(
            &c,
            &mut p,
            &periodo(),
            &Mapa::vazio(),
            &ConfigConciliacao::default(),
            HOJE,
        )
        .expect("conciliar");
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
        let de_novo = conciliar(
            &c,
            &mut p,
            &periodo(),
            &Mapa::vazio(),
            &ConfigConciliacao::default(),
            HOJE,
        )
        .expect("de novo");
        assert_eq!(de_novo, 0);
    }

    #[test]
    fn desconto_em_folha_e_entrada_com_sinal_certo() {
        let (c, mut p) = cenario();
        p.ocorrencias[0].natureza = Natureza::DescontoEmFolha;
        registro(
            &c,
            "pluggy:txn_entrada",
            "credito",
            "2026-11-05",
            150_000,
            "EMPRESA",
        );
        let s = sugerir(
            &c,
            &p,
            &periodo(),
            &Mapa::vazio(),
            &ConfigConciliacao::default(),
            HOJE,
        )
        .expect("sugerir");
        assert!(s.iter().all(|x| x.ocorrencia.recorrente != "r1"));
        p.ocorrencias[0].natureza = Natureza::Entrada;
        let s = sugerir(
            &c,
            &p,
            &periodo(),
            &Mapa::vazio(),
            &ConfigConciliacao::default(),
            HOJE,
        )
        .expect("sugerir");
        assert_eq!(
            s.iter()
                .find(|x| x.ocorrencia.recorrente == "r1")
                .expect("entrada")
                .movimento,
            "pluggy:txn_entrada"
        );
    }

    fn entrada(rec: &str, dia: u8, previsto: i64, conta: &str) -> Ocorrencia {
        Ocorrencia {
            natureza: Natureza::Entrada,
            conta: Some(conta.to_owned()),
            ..ocorrencia(rec, "Salário", dia, previsto)
        }
    }

    #[test]
    fn conta_e_exigida_com_par_confirmado_e_ignorada_sem_par() {
        let c = inventario_ficticio();
        registro_completo(
            &c,
            "pluggy:txn_o",
            "credito",
            "2026-11-05",
            150_000,
            Some("BANCO · 2"),
            "EMPRESA",
        );
        registro_completo(
            &c,
            "pluggy:txn_x",
            "credito",
            "2026-11-07",
            150_000,
            Some("BANCO · 1"),
            "EMPRESA",
        );
        let mut p = Falso {
            ocorrencias: vec![entrada("r1", 5, 150_000, "CX-0001")],
            ..Falso::default()
        };
        let cfg = ConfigConciliacao::default();
        let sem = sugerir(&c, &p, &periodo(), &Mapa::vazio(), &cfg, HOJE).expect("sem par");
        assert_eq!(sem[0].movimento, "pluggy:txn_o");
        p.contas = vec![ContaDoPlanejamento {
            apelido: "CX-0001".to_owned(),
            conta_do_extrato: Some("BANCO · 1".to_owned()),
        }];
        let com = sugerir(&c, &p, &periodo(), &Mapa::vazio(), &cfg, HOJE).expect("com par");
        assert_eq!(com[0].movimento, "pluggy:txn_x");
    }

    #[test]
    fn assinatura_acha_o_credito_com_valor_diferente() {
        let c = inventario_ficticio();
        registro_completo(
            &c,
            "pluggy:txn_plr",
            "credito",
            "2026-11-06",
            912_345,
            Some("BANCO · 1"),
            "SALARIO EMPRESA",
        );
        let mut p = Falso {
            ocorrencias: vec![entrada("r1", 5, 500_000, "CX-0001")],
            ..Falso::default()
        };
        let cfg = ConfigConciliacao::default();
        assert!(
            sugerir(&c, &p, &periodo(), &Mapa::vazio(), &cfg, HOJE)
                .expect("sem")
                .is_empty()
        );
        p.assinaturas.insert(
            "r1".to_owned(),
            Assinatura {
                texto: "salário empresa".to_owned(),
                janela_dias: 4,
            },
        );
        let s = sugerir(&c, &p, &periodo(), &Mapa::vazio(), &cfg, HOJE).expect("com");
        assert_eq!(s[0].movimento, "pluggy:txn_plr");
        assert_eq!(s[0].valor, Centavos(912_345));
    }

    #[test]
    fn liquido_da_composicao_e_o_alvo() {
        let c = inventario_ficticio();
        registro_completo(
            &c,
            "pluggy:txn_digitado",
            "credito",
            "2026-11-05",
            700_000,
            None,
            "X",
        );
        registro_completo(
            &c,
            "pluggy:txn_liquido",
            "credito",
            "2026-11-06",
            480_000,
            None,
            "X",
        );
        let rubrica = |tipo, valor| Rubrica {
            codigo: None,
            nome: "r".to_owned(),
            tipo,
            valor: Centavos(valor),
            competencia: None,
            prazo: None,
            recorrente: None,
        };
        let p = Falso {
            ocorrencias: vec![entrada("r1", 5, 700_000, "CX-0001")],
            composicoes: vec![Composicao {
                recorrente: "r1".to_owned(),
                desde: Competencia { ano: 2026, mes: 11 },
                rubricas: vec![
                    rubrica(TipoDeRubrica::Provento, 600_000),
                    rubrica(TipoDeRubrica::Desconto, 120_000),
                ],
                documento: None,
                origem: "teste".to_owned(),
            }],
            ..Falso::default()
        };
        let s = sugerir(
            &c,
            &p,
            &periodo(),
            &Mapa::vazio(),
            &ConfigConciliacao::default(),
            HOJE,
        )
        .expect("sugerir");
        assert_eq!(s[0].movimento, "pluggy:txn_liquido");
    }
}
