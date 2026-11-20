//! O Modelo de compromisso pelo MCP: as ferramentas aplicam direto (autor mcp, backup,
//! Desfazer), contra o serviço real com o Luau de verdade. Dados fictícios.
#![allow(clippy::unwrap_used)]

#[path = "../../sbf-triagem/tests/ajuda/mod.rs"]
#[allow(unused_imports)]
mod ajuda;

use std::path::PathBuf;
use std::sync::Arc;

use ajuda::{Banco, banco_limpo};
use sbf_mcp::Nucleo;
use sbf_mcp::plano::*;
use sbf_mcp::servidor::Servidor;
use sbf_planejamento::{
    Alteracao, Autor, Banco as BancoDoPlano, Centavos, Certeza, Competencia, Data, Fonte,
    Frequencia, Natureza, NovoRecorrente, Planejamento, PlanejamentoSqlite, Prazo, SemSaldos,
    Versao,
};
use sbf_script::ExecutorNoProcesso;
use serde_json::{Value, json};

struct Cenario {
    banco: Banco,
    arquivo: PathBuf,
    nucleo: Nucleo,
    rec: String,
}

fn comp(ano: i32, mes: u8) -> Competencia {
    Competencia { ano, mes }
}

const HOJE: Data = Data {
    ano: 2026,
    mes: 11,
    dia: 16,
};

fn abrir(arquivo: &std::path::Path) -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(BancoDoPlano::abrir(arquivo).unwrap(), Box::new(SemSaldos))
        .com_calculo(Arc::new(ExecutorNoProcesso::novo()))
}

fn cenario() -> Cenario {
    let banco = banco_limpo();
    let cofre = banco.caminho.parent().unwrap().to_path_buf();
    let arquivo = cofre.join("planejamento.sqlite");
    let mut s = abrir(&arquivo);
    let rec = s
        .aplicar(
            Alteracao::CriarRecorrente {
                recorrente: NovoRecorrente {
                    nome: "Diarista Exemplo".into(),
                    grupo: "Casa".into(),
                    subgrupo: None,
                    posicao: 1,
                    natureza: Natureza::Saida,
                    prazo: Prazo::SemFim,
                    primeira_versao: Versao {
                        id: String::new(),
                        desde: comp(2025, 1),
                        valor: Some(Centavos(100_000)),
                        certeza: Certeza::Estimado,
                        fonte: Fonte::Usuario,
                        frequencia: Frequencia::Mensal { dia: 5 },
                        conta: None,
                        centro: None,
                        fornecedor: None,
                        categoria: None,
                        nota: None,
                    },
                },
                origem: None,
            },
            Autor::Usuario,
        )
        .unwrap()
        .alvo;
    drop(s);
    let caminho = arquivo.clone();
    let fabrica: Arc<Fabrica> =
        Arc::new(move || Ok(Box::new(abrir(&caminho)) as Box<dyn Planejamento>));
    let nucleo = Nucleo::novo(&banco.caminho, None)
        .unwrap()
        .com_arquivo_do_planejamento(&arquivo)
        .com_planejamento(fabrica)
        .com_hoje(HOJE);
    Cenario {
        banco,
        arquivo,
        nucleo,
        rec,
    }
}

impl Cenario {
    fn plano(&self) -> &Plano {
        self.nucleo.plano().unwrap()
    }

    fn servico(&self) -> PlanejamentoSqlite {
        abrir(&self.arquivo)
    }

    fn previsto(&self, c: Competencia) -> Option<i64> {
        self.servico()
            .proximas(&self.rec, c, 1, HOJE)
            .unwrap()
            .first()
            .and_then(|o| o.previsto.map(|v| v.0))
    }

    /// Liga ao modelo de fábrica: R$ 190 até 2026-04, R$ 210 desde 2026-05, às sextas.
    fn ligar(&self) -> Value {
        self.plano()
            .aplicar_modelo(&PedidoDeAplicarModelo {
                recorrente: self.rec.clone(),
                modelo: "Diarista por dia da semana".into(),
                valores: vec![
                    ValorEntrada {
                        parametro: "diaria".into(),
                        desde: "2026-01".into(),
                        valor: json!(19_000),
                    },
                    ValorEntrada {
                        parametro: "diaria".into(),
                        desde: "2026-05".into(),
                        valor: json!(21_000),
                    },
                    ValorEntrada {
                        parametro: "dia_da_semana".into(),
                        desde: "2026-01".into(),
                        valor: json!("sexta"),
                    },
                ],
            })
            .unwrap()
    }
}

#[test]
fn listar_modelos_mostra_o_de_fabrica_com_parametros_calculo_e_quem_usa() {
    let c = cenario();
    let v = c.plano().listar_modelos().unwrap();
    let m = &v["modelos"][0];
    assert_eq!(m["nome"], "Diarista por dia da semana");
    assert_eq!(m["de_fabrica"], true);
    assert_eq!(m["parametros"][0]["nome"], "diaria");
    assert_eq!(m["parametros"][0]["tipo"], "centavos");
    assert_eq!(m["parametros"][1]["tipo"], "dia_da_semana");
    assert!(m["calculo"].as_str().unwrap().contains("function calcular"));
    assert_eq!(m["usado_por"].as_array().unwrap().len(), 0);
    c.ligar();
    let v = c.plano().listar_modelos().unwrap();
    let usa = &v["modelos"][0]["usado_por"][0];
    assert_eq!(usa["recorrente"], c.rec.as_str());
    assert_eq!(usa["valores"].as_array().unwrap().len(), 3);
}

#[test]
fn aplicar_modelo_e_definir_parametro_aplicam_direto_com_autor_mcp_e_desfazer_volta() {
    let c = cenario();
    let r = c.ligar();
    assert_eq!(r["aplicado"], true);
    assert_eq!(r["autor"], "mcp");
    // 4 sextas × R$ 190 em abril; 5 sextas × R$ 210 em maio.
    assert_eq!(c.previsto(comp(2026, 4)), Some(76_000));
    assert_eq!(c.previsto(comp(2026, 5)), Some(105_000));
    // A resposta já traz a simulação (o que mudou) e o jeito de desfazer.
    assert!(!r["simulacao"]["mudancas"].as_array().unwrap().is_empty());
    assert_eq!(r["desfazer"]["ferramenta"], "desfazer");

    let d = c
        .plano()
        .definir_parametro(&PedidoDeDefinirParametro {
            recorrente: c.rec.clone(),
            parametro: "diaria".into(),
            desde: "2026-08".into(),
            valor: json!(25_000),
        })
        .unwrap();
    assert_eq!(d["aplicado"], true);
    assert_eq!(c.previsto(comp(2026, 7)), Some(105_000));
    // Agosto/2026 tem 4 sextas.
    assert_eq!(c.previsto(comp(2026, 8)), Some(100_000));
    let h = c.servico().historico(None, 10).unwrap();
    assert_eq!(h[0].autor, Autor::Mcp);

    // Desfazer volta o último; de novo, volta a ligação.
    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    // Voltou a diária de R$ 210 (4 sextas em agosto).
    assert_eq!(c.previsto(comp(2026, 8)), Some(84_000));
    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert_eq!(c.previsto(comp(2026, 5)), Some(100_000));
}

#[test]
fn ajustar_quantidade_vale_so_naquele_mes_e_desfazer_volta() {
    let c = cenario();
    c.ligar();
    assert_eq!(c.previsto(comp(2026, 1)), Some(95_000));
    let r = c
        .plano()
        .ajustar_quantidade(&PedidoDeAjustarQuantidade {
            id_ocorrencia: format!("{}:2026-01", c.rec),
            quantidade: 6,
        })
        .unwrap();
    assert_eq!(r["aplicado"], true);
    assert_eq!(c.previsto(comp(2026, 1)), Some(114_000));
    assert_eq!(c.previsto(comp(2026, 2)), Some(76_000));
    let exc = c.servico().excecoes_do_recorrente(&c.rec).unwrap();
    assert_eq!(exc.last().unwrap().autor, Autor::Mcp);
    c.plano()
        .desfazer(&PedidoDeDesfazer {
            alteracao_id: Some(r["alteracao_id"].as_str().unwrap().to_owned()),
        })
        .unwrap();
    assert_eq!(c.previsto(comp(2026, 1)), Some(95_000));
}

#[test]
fn criar_modelo_confere_o_script_e_aplica_direto() {
    let c = cenario();
    let parametros = || {
        vec![ParametroDeclarado {
            nome: "valor".into(),
            tipo: "centavos".into(),
        }]
    };
    // Sem `calcular`, com sintaxe errada, tipo desconhecido.
    for (calculo, tipo) in [
        ("x = 1", "centavos"),
        ("function calcular(", "centavos"),
        (
            "function calcular(e, a) return {valor_centavos = 1, explicacao = 'x'} end",
            "real",
        ),
    ] {
        let r = c.plano().criar_modelo(&PedidoDeCriarModelo {
            nome: "Meu modelo".into(),
            parametros: vec![ParametroDeclarado {
                nome: "valor".into(),
                tipo: tipo.into(),
            }],
            calculo: calculo.into(),
        });
        assert!(r.is_err(), "{calculo}");
    }
    let r = c
        .plano()
        .criar_modelo(&PedidoDeCriarModelo {
            nome: "Meu modelo".into(),
            parametros: parametros(),
            calculo: "function calcular(e, a) return {valor_centavos = e.parametros.valor, explicacao = 'fixo'} end".into(),
        })
        .unwrap();
    assert_eq!(r["aplicado"], true);
    let id = r["modelo"]["id"].as_str().unwrap().to_owned();
    // Ligar pelo nome (e não pelo id) também vale.
    c.plano()
        .aplicar_modelo(&PedidoDeAplicarModelo {
            recorrente: c.rec.clone(),
            modelo: "meu modelo".into(),
            valores: vec![ValorEntrada {
                parametro: "valor".into(),
                desde: "2026-01".into(),
                valor: json!(5_500),
            }],
        })
        .unwrap();
    assert_eq!(c.previsto(comp(2026, 3)), Some(5_500));
    assert!(c.servico().modelos().unwrap().iter().any(|m| m.id == id));
}

#[test]
fn argumentos_invalidos_sao_recusados_sem_gravar() {
    let c = cenario();
    // Sem ligação, definir_parametro e ajustar_quantidade recusam.
    assert!(
        c.plano()
            .definir_parametro(&PedidoDeDefinirParametro {
                recorrente: c.rec.clone(),
                parametro: "diaria".into(),
                desde: "2026-01".into(),
                valor: json!(1),
            })
            .is_err()
    );
    assert!(
        c.plano()
            .ajustar_quantidade(&PedidoDeAjustarQuantidade {
                id_ocorrencia: format!("{}:2026-01", c.rec),
                quantidade: 3,
            })
            .is_err()
    );
    // Modelo inexistente, parâmetro desconhecido, dia da semana desconhecido, tipo errado.
    let pedido = |modelo: &str, parametro: &str, valor: Value| PedidoDeAplicarModelo {
        recorrente: c.rec.clone(),
        modelo: modelo.into(),
        valores: vec![ValorEntrada {
            parametro: parametro.into(),
            desde: "2026-01".into(),
            valor,
        }],
    };
    let d = "Diarista por dia da semana";
    for ruim in [
        pedido("nenhum", "diaria", json!(1)),
        pedido(d, "naoexiste", json!(1)),
        pedido(d, "dia_da_semana", json!("sextilha")),
        pedido(d, "dia_da_semana", json!(9)),
        pedido(d, "diaria", json!("cem")),
        pedido(d, "diaria", json!(1.5)),
        pedido(d, "diaria", json!(-5)),
    ] {
        assert!(c.plano().aplicar_modelo(&ruim).is_err());
    }
    assert_eq!(c.servico().ligacao_de_modelo(&c.rec).unwrap(), None);
    assert!(
        c.plano()
            .ajustar_quantidade(&PedidoDeAjustarQuantidade {
                id_ocorrencia: "lixo".into(),
                quantidade: 3,
            })
            .is_err()
    );
}

#[test]
fn escrever_faz_o_backup_da_sessao_e_as_ferramentas_aparecem_so_com_planejamento() {
    let c = cenario();
    c.ligar();
    let pasta = c.banco.caminho.parent().unwrap().join("backups/mcp");
    let n = std::fs::read_dir(pasta)
        .unwrap()
        .filter_map(Result::ok)
        .filter(|e| e.file_name().to_string_lossy().starts_with("planejamento-"))
        .count();
    assert_eq!(n, 1);

    let com = Servidor::novo(cenario().nucleo).nomes_das_ferramentas();
    for nome in [
        "listar_modelos",
        "criar_modelo",
        "aplicar_modelo",
        "definir_parametro",
        "ajustar_quantidade",
        "desfazer",
    ] {
        assert!(com.iter().any(|x| x == nome), "{nome}");
    }
    let sem =
        Servidor::novo(Nucleo::novo(&banco_limpo().caminho, None).unwrap()).nomes_das_ferramentas();
    for nome in [
        "listar_modelos",
        "criar_modelo",
        "aplicar_modelo",
        "definir_parametro",
        "ajustar_quantidade",
    ] {
        assert!(!sem.iter().any(|x| x == nome), "{nome}");
    }
}
