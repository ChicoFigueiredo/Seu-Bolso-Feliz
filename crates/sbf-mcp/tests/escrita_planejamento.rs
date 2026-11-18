//! D15: as ferramentas de planejamento que APLICAM, contra o serviço real (SQLite em arquivo
//! temporário). Dados fictícios.
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

fn abrir(arquivo: &std::path::Path) -> PlanejamentoSqlite {
    PlanejamentoSqlite::novo(BancoDoPlano::abrir(arquivo).unwrap(), Box::new(SemSaldos))
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
                    nome: "Aluguel".into(),
                    grupo: "Essencial".into(),
                    subgrupo: None,
                    posicao: 1,
                    natureza: Natureza::Saida,
                    prazo: Prazo::SemFim,
                    primeira_versao: Versao {
                        id: String::new(),
                        desde: comp(2026, 10),
                        valor: Some(Centavos(150_000)),
                        certeza: Certeza::Confirmado,
                        fonte: Fonte::Usuario,
                        frequencia: Frequencia::Mensal { dia: 5 },
                        conta: Some("CX-0001".into()),
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
        .com_hoje(Data {
            ano: 2026,
            mes: 11,
            dia: 16,
        });
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

    fn historico(&self) -> Vec<sbf_planejamento::EntradaDoHistorico> {
        self.servico().historico(None, 100).unwrap()
    }

    fn previsto(&self, c: Competencia) -> Option<i64> {
        self.servico()
            .proximas(
                &self.rec,
                c,
                1,
                Data {
                    ano: 2026,
                    mes: 11,
                    dia: 16,
                },
            )
            .unwrap()
            .first()
            .and_then(|o| o.previsto.map(|v| v.0))
    }

    fn nome(&self) -> String {
        self.servico().recorrente(&self.rec).unwrap().unwrap().nome
    }

    fn aplicacao(&self) -> PedidoDeAplicacao {
        PedidoDeAplicacao {
            recorrente: self.rec.clone(),
            ..PedidoDeAplicacao::default()
        }
    }

    fn pasta_de_backups(&self) -> PathBuf {
        self.banco.caminho.parent().unwrap().join("backups/mcp")
    }
}

fn backups_de_planejamento(c: &Cenario) -> usize {
    std::fs::read_dir(c.pasta_de_backups())
        .map(|rd| {
            rd.filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().starts_with("planejamento-"))
                .count()
        })
        .unwrap_or(0)
}

fn id_do_recibo(r: &Value) -> String {
    r["alteracao_id"].as_str().unwrap().to_owned()
}

// ------------------------------------------------------------------------ aplicar_alteracao

#[test]
fn aplicar_alteracao_valor_daqui_em_diante_aplica_direto_com_autor_mcp_e_desfazer_volta() {
    let c = cenario();
    let r = c
        .plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            escopo: Some("daqui_em_diante".into()),
            competencia: Some("2026-12".into()),
            valor_centavos: Some(180_000),
            nota: Some("reajuste".into()),
            ..c.aplicacao()
        })
        .unwrap();
    assert_eq!(r["aplicado"], true);
    assert!(r["mensagem"].as_str().unwrap().contains("desfazer"));
    assert_eq!(c.previsto(comp(2026, 12)), Some(180_000));
    assert_eq!(c.previsto(comp(2026, 11)), Some(150_000));
    let h = c.historico();
    assert_eq!(h[0].autor, Autor::Mcp);
    assert_eq!(h[0].id, id_do_recibo(&r));

    let d = c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert_eq!(d["desfeito"], true);
    assert_eq!(d["autor"], "mcp");
    assert_eq!(c.previsto(comp(2026, 12)), Some(150_000));
    let h = c.historico();
    assert_eq!(h[0].autor, Autor::Mcp, "o Desfazer também tem autor mcp");
}

#[test]
fn aplicar_alteracao_so_este_mes_conta_centro_fornecedor_categoria_e_certeza() {
    let c = cenario();
    c.plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            escopo: Some("so_este_mes".into()),
            competencia: Some("2026-12".into()),
            valor_centavos: Some(99_900),
            ..c.aplicacao()
        })
        .unwrap();
    assert_eq!(c.previsto(comp(2026, 12)), Some(99_900));
    assert_eq!(c.previsto(comp(2027, 1)), Some(150_000));

    c.plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            escopo: Some("daqui_em_diante".into()),
            competencia: Some("2026-12".into()),
            conta: Some("CX-0002".into()),
            centro: Some("Casa".into()),
            fornecedor: Some("Imobiliária Exemplo".into()),
            categoria: Some("Moradia".into()),
            certeza: Some("estimado".into()),
            ..c.aplicacao()
        })
        .unwrap();
    let rec = c.servico().recorrente(&c.rec).unwrap().unwrap();
    let v = rec.versoes.last().unwrap();
    assert_eq!(v.conta.as_deref(), Some("CX-0002"));
    assert_eq!(v.centro.as_deref(), Some("Casa"));
    assert_eq!(v.fornecedor.as_deref(), Some("Imobiliária Exemplo"));
    assert_eq!(v.categoria.as_deref(), Some("Moradia"));
    assert_eq!(v.certeza, Certeza::Estimado);
}

#[test]
fn aplicar_alteracao_renomeia_e_desfazer_devolve_o_nome() {
    let c = cenario();
    let r = c
        .plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            nome: Some("Aluguel do apartamento".into()),
            ..c.aplicacao()
        })
        .unwrap();
    assert_eq!(c.nome(), "Aluguel do apartamento");
    assert_eq!(c.historico()[0].autor, Autor::Mcp);
    assert!(c.historico()[0].resumo.contains("Renomeou"));
    c.plano()
        .desfazer(&PedidoDeDesfazer {
            alteracao_id: Some(id_do_recibo(&r)),
        })
        .unwrap();
    assert_eq!(c.nome(), "Aluguel");
}

#[test]
fn aplicar_alteracao_com_nome_e_valor_faz_duas_alteracoes_e_desfaz_uma_a_uma() {
    let c = cenario();
    let r = c
        .plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            nome: Some("Aluguel novo".into()),
            escopo: Some("daqui_em_diante".into()),
            competencia: Some("2026-12".into()),
            valor_centavos: Some(160_000),
            ..c.aplicacao()
        })
        .unwrap();
    assert_eq!(r["alteracoes"].as_array().unwrap().len(), 2);
    assert_eq!(c.nome(), "Aluguel novo");
    assert_eq!(c.previsto(comp(2026, 12)), Some(160_000));
    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert_eq!(c.nome(), "Aluguel");
    assert_eq!(c.previsto(comp(2026, 12)), Some(160_000));
    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert_eq!(c.previsto(comp(2026, 12)), Some(150_000));
}

#[test]
fn aplicar_alteracao_valida_antes_de_gravar() {
    let c = cenario();
    // Pedido vazio.
    assert!(c.plano().aplicar_alteracao(&c.aplicacao()).is_err());
    // Muda a versão sem dizer escopo: nada é gravado, nem o nome.
    let e = c
        .plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            nome: Some("Não deve valer".into()),
            valor_centavos: Some(1),
            ..c.aplicacao()
        })
        .unwrap_err();
    assert!(e.contains("escopo"), "{e}");
    assert_eq!(c.nome(), "Aluguel");
    assert!(c.historico().iter().all(|h| h.autor != Autor::Mcp));
}

// ------------------------------------------------------------------------ criar, pausar...

fn novo_compromisso(nome: &str) -> PedidoNovoCompromisso {
    serde_json::from_value(json!({
        "nome": nome, "grupo": "Lazer", "natureza": "saida", "valor_centavos": 3_990,
        "desde": "2026-11", "dia": 10
    }))
    .unwrap()
}

#[test]
fn criar_compromisso_aplica_recusa_duplicado_e_desfazer_exclui() {
    let c = cenario();
    let r = c
        .plano()
        .aplicar(
            "criar_compromisso",
            &EntradaDeAlteracao::Compromisso(novo_compromisso("Streaming")),
        )
        .unwrap();
    let id = r["alvo"].as_str().unwrap().to_owned();
    assert_eq!(c.historico()[0].autor, Autor::Mcp);
    assert!(
        c.servico()
            .recorrente(&id)
            .unwrap()
            .unwrap()
            .excluido
            .is_none()
    );

    let e = c
        .plano()
        .aplicar(
            "criar_compromisso",
            &EntradaDeAlteracao::Compromisso(novo_compromisso("streaming")),
        )
        .unwrap_err();
    assert!(e.contains("já existe"), "{e}");

    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert!(
        c.servico()
            .recorrente(&id)
            .unwrap()
            .unwrap()
            .excluido
            .is_some()
    );
}

#[test]
fn pausar_retomar_cancelar_excecao_e_exclusao_aplicam_com_autor_mcp_e_desfazem() {
    let c = cenario();
    let p = c.plano();
    let rec = c.rec.clone();

    p.aplicar(
        "pausar",
        &EntradaDeAlteracao::Pausa(PedidoDePausa {
            recorrente: rec.clone(),
            desde: "2027-01".into(),
            ate: None,
            motivo: "viagem".into(),
        }),
    )
    .unwrap();
    assert_eq!(c.historico()[0].autor, Autor::Mcp);
    assert_eq!(
        c.servico().recorrente(&rec).unwrap().unwrap().pausas.len(),
        1
    );

    p.aplicar(
        "retomar",
        &EntradaDeAlteracao::Retomada(PedidoDeRetomada {
            recorrente: rec.clone(),
            desde: "2027-03".into(),
        }),
    )
    .unwrap();
    assert_eq!(c.historico()[0].autor, Autor::Mcp);
    p.desfazer(&PedidoDeDesfazer::default()).unwrap(); // desfaz a retomada
    assert!(
        c.servico().recorrente(&rec).unwrap().unwrap().pausas[0]
            .ate
            .is_none()
    );
    p.desfazer(&PedidoDeDesfazer::default()).unwrap(); // desfaz a pausa

    p.aplicar(
        "cancelar",
        &EntradaDeAlteracao::Cancelamento(PedidoDeCancelamento {
            recorrente: rec.clone(),
            desde: "2027-06".into(),
            motivo: "mudança".into(),
            comprovado: None,
        }),
    )
    .unwrap();
    assert!(
        c.servico()
            .recorrente(&rec)
            .unwrap()
            .unwrap()
            .cancelamento
            .is_some()
    );
    p.desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert!(
        c.servico()
            .recorrente(&rec)
            .unwrap()
            .unwrap()
            .cancelamento
            .is_none()
    );

    p.aplicar(
        "excecao",
        &EntradaDeAlteracao::Excecao(PedidoDeExcecao {
            id_ocorrencia: format!("{rec}:2026-12"),
            acao: "valor_so_deste_mes".into(),
            nova_data: None,
            motivo: None,
            valor_centavos: Some(100_000),
            data: None,
        }),
    )
    .unwrap();
    assert_eq!(c.previsto(comp(2026, 12)), Some(100_000));
    p.desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert_eq!(c.previsto(comp(2026, 12)), Some(150_000));

    p.aplicar(
        "excluir_logicamente",
        &EntradaDeAlteracao::ExclusaoLogica(PedidoDeExclusao {
            recorrente: rec.clone(),
            motivo: "engano".into(),
        }),
    )
    .unwrap();
    assert!(
        c.servico()
            .recorrente(&rec)
            .unwrap()
            .unwrap()
            .excluido
            .is_some()
    );
    p.desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert!(
        c.servico()
            .recorrente(&rec)
            .unwrap()
            .unwrap()
            .excluido
            .is_none()
    );

    // Tudo o que o MCP fez tem autor mcp; só o que o usuário criou não tem.
    let h = c.historico();
    assert!(h.iter().filter(|e| e.autor != Autor::Mcp).count() == 1);
}

// ------------------------------------------------------------------------ conciliar

#[test]
fn conciliar_com_dois_movimentos_aplica_e_desfaz_um_a_um() {
    let c = cenario();
    let ocorrencia = format!("{}:2026-12", c.rec);
    let r = c
        .plano()
        .conciliar(&PedidoDeConciliar {
            id_ocorrencia: ocorrencia.clone(),
            movimentos: vec![
                MovimentoParaConciliar {
                    movimento: "pluggy:txn_a".into(),
                    valor_centavos: 100_000,
                    data: "2026-12-05".into(),
                },
                MovimentoParaConciliar {
                    movimento: "pluggy:txn_b".into(),
                    valor_centavos: 50_000,
                    data: "2026-12-06".into(),
                },
            ],
        })
        .unwrap();
    assert_eq!(r["conciliacoes"].as_array().unwrap().len(), 2);
    let id = sbf_planejamento::IdOcorrencia::de_texto(&ocorrencia).unwrap();
    let ligadas = c.servico().conciliacoes_da_ocorrencia(&id).unwrap();
    assert_eq!(ligadas.len(), 2);
    assert!(ligadas.iter().all(|l| l.conciliacao.confirmada));
    let h = c.historico();
    assert_eq!(h[0].autor, Autor::Mcp);
    assert_eq!(h[1].autor, Autor::Mcp);

    // Desfazer volta o último movimento; desconciliar solta o outro.
    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert_eq!(
        c.servico().conciliacoes_da_ocorrencia(&id).unwrap().len(),
        1
    );
    c.plano()
        .desconciliar(&PedidoDeDesconciliar {
            id_ocorrencia: ocorrencia,
            movimento: "pluggy:txn_a".into(),
        })
        .unwrap();
    assert!(
        c.servico()
            .conciliacoes_da_ocorrencia(&id)
            .unwrap()
            .is_empty()
    );
    assert_eq!(c.historico()[0].autor, Autor::Mcp);
}

#[test]
fn conciliar_e_tudo_ou_nada() {
    let c = cenario();
    let mov = |m: &str| MovimentoParaConciliar {
        movimento: m.into(),
        valor_centavos: 1_000,
        data: "2026-12-05".into(),
    };
    // Movimento repetido.
    assert!(
        c.plano()
            .conciliar(&PedidoDeConciliar {
                id_ocorrencia: format!("{}:2026-12", c.rec),
                movimentos: vec![mov("x"), mov("x")],
            })
            .is_err()
    );
    // Lista vazia.
    assert!(
        c.plano()
            .conciliar(&PedidoDeConciliar {
                id_ocorrencia: format!("{}:2026-12", c.rec),
                movimentos: vec![],
            })
            .is_err()
    );
    // Recorrente que não existe: nada ligado.
    assert!(
        c.plano()
            .conciliar(&PedidoDeConciliar {
                id_ocorrencia: "rec_fantasma:2026-12".into(),
                movimentos: vec![mov("y")],
            })
            .is_err()
    );
    assert!(c.historico().iter().all(|h| h.autor != Autor::Mcp));
}

// ------------------------------------------------------------------------ propostas

#[test]
fn aplicar_proposta_aplica_como_mcp_e_recusar_proposta_recusa() {
    let c = cenario();
    let proposta = |chave: &str, valor: i64| {
        c.plano()
            .propor(
                "propor_alteracao",
                chave,
                &EntradaDeAlteracao::Alteracao(PedidoDeAlteracao {
                    recorrente: c.rec.clone(),
                    escopo: "daqui_em_diante".into(),
                    competencia: "2026-12".into(),
                    valor_centavos: Some(valor),
                    sem_valor: None,
                    certeza: None,
                    conta: None,
                    centro: None,
                    fornecedor: None,
                    categoria: None,
                    nota: None,
                    limpar: None,
                }),
            )
            .unwrap()["proposta"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let a = proposta("k-a", 170_000);
    let r = c
        .plano()
        .aplicar_proposta(&PedidoDeAplicarProposta { id: a.clone() })
        .unwrap();
    assert_eq!(r["proposta"]["estado"], "confirmada");
    assert_eq!(c.previsto(comp(2026, 12)), Some(170_000));
    assert_eq!(c.historico()[0].autor, Autor::Mcp, "não é Usuario");
    // Já decidida: recusa.
    assert!(
        c.plano()
            .aplicar_proposta(&PedidoDeAplicarProposta { id: a })
            .is_err()
    );
    // E dá para desfazer.
    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    assert_eq!(c.previsto(comp(2026, 12)), Some(150_000));

    let b = proposta("k-b", 190_000);
    assert!(
        c.plano()
            .recusar_proposta(&PedidoDeRecusaDeProposta {
                id: b.clone(),
                motivo: "  ".into()
            })
            .is_err(),
        "motivo obrigatório"
    );
    c.plano()
        .recusar_proposta(&PedidoDeRecusaDeProposta {
            id: b,
            motivo: "não vale".into(),
        })
        .unwrap();
    assert_eq!(c.previsto(comp(2026, 12)), Some(150_000));
}

// ------------------------------------------------------------------------ desfazer

#[test]
fn desfazer_nao_mexe_no_que_o_usuario_fez_nem_repete() {
    let c = cenario();
    // Nada do MCP ainda.
    let e = c
        .plano()
        .desfazer(&PedidoDeDesfazer::default())
        .unwrap_err();
    assert!(e.contains("nada do MCP"), "{e}");
    // Pelo id da criação do usuário: recusado.
    let do_usuario = c.historico()[0].id.clone();
    let e = c
        .plano()
        .desfazer(&PedidoDeDesfazer {
            alteracao_id: Some(do_usuario),
        })
        .unwrap_err();
    assert!(e.contains("só se desfaz o que o MCP fez"), "{e}");
    assert!(
        c.servico()
            .recorrente(&c.rec)
            .unwrap()
            .unwrap()
            .excluido
            .is_none()
    );

    let r = c
        .plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            nome: Some("Outro".into()),
            ..c.aplicacao()
        })
        .unwrap();
    c.plano().desfazer(&PedidoDeDesfazer::default()).unwrap();
    // Duas vezes a mesma: recusa; e o "último" já não existe.
    assert!(
        c.plano()
            .desfazer(&PedidoDeDesfazer {
                alteracao_id: Some(id_do_recibo(&r))
            })
            .is_err()
    );
    assert!(c.plano().desfazer(&PedidoDeDesfazer::default()).is_err());
}

// ------------------------------------------------------------------------ backup

#[test]
fn primeira_escrita_da_sessao_faz_backup_do_planejamento_e_so_uma_vez() {
    let c = cenario();
    c.plano()
        .listar_compromissos(&PedidoDeLista::default())
        .unwrap();
    assert_eq!(backups_de_planejamento(&c), 0, "leitura não copia");
    c.plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            nome: Some("Um".into()),
            ..c.aplicacao()
        })
        .unwrap();
    c.plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            nome: Some("Dois".into()),
            ..c.aplicacao()
        })
        .unwrap();
    assert_eq!(backups_de_planejamento(&c), 1);
    // A cópia é de antes da primeira escrita: o nome ainda era "Aluguel".
    let copia = std::fs::read_dir(c.pasta_de_backups())
        .unwrap()
        .filter_map(Result::ok)
        .find(|e| e.file_name().to_string_lossy().starts_with("planejamento-"))
        .unwrap()
        .path();
    let s = abrir(&copia);
    assert_eq!(s.recorrente(&c.rec).unwrap().unwrap().nome, "Aluguel");
}

#[test]
fn propor_tambem_conta_como_escrita_e_faz_backup() {
    let c = cenario();
    c.plano()
        .propor(
            "propor_pausa",
            "k",
            &EntradaDeAlteracao::Pausa(PedidoDePausa {
                recorrente: c.rec.clone(),
                desde: "2027-01".into(),
                ate: None,
                motivo: "x".into(),
            }),
        )
        .unwrap();
    assert_eq!(backups_de_planejamento(&c), 1);
}

#[test]
fn se_o_backup_falha_nada_e_escrito() {
    let c = cenario();
    let cofre = c.banco.caminho.parent().unwrap().to_path_buf();
    std::fs::write(cofre.join("backups"), "obstáculo").unwrap();
    let e = c
        .plano()
        .aplicar_alteracao(&PedidoDeAplicacao {
            nome: Some("Não".into()),
            ..c.aplicacao()
        })
        .unwrap_err();
    assert!(
        e.contains("backup") && e.contains("escrita foi recusada"),
        "{e}"
    );
    assert_eq!(c.nome(), "Aluguel");
    assert!(c.historico().iter().all(|h| h.autor != Autor::Mcp));
}

#[test]
fn fazer_backup_copia_o_planejamento_e_o_inventario() {
    let c = cenario();
    let r = c.nucleo.fazer_backup().unwrap();
    let bancos: Vec<&str> = r["feitos"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["banco"].as_str().unwrap())
        .collect();
    assert!(bancos.contains(&"planejamento"));
    assert!(bancos.contains(&"inventario"));
}

// ------------------------------------------------------------------------ catálogo

#[test]
fn as_ferramentas_novas_aparecem_e_a_instrucao_nao_diz_mais_somente_leitura() {
    let c = cenario();
    let servidor = Servidor::novo(c.nucleo.clone());
    let instrucao = rmcp::ServerHandler::get_info(&servidor)
        .instructions
        .unwrap_or_default();
    assert!(instrucao.contains("CONTROLE TOTAL"), "{instrucao}");
    assert!(!instrucao.contains("somente leitura"), "{instrucao}");
    assert!(instrucao.contains("backup"));
    let nomes = servidor.nomes_das_ferramentas();
    for n in FERRAMENTAS_DE_ESCRITA.iter().chain(&["fazer_backup"]) {
        assert!(nomes.iter().any(|x| x == n), "falta {n}");
    }
    for n in [
        "ativar_regra",
        "desativar_regra",
        "reverter_regra",
        "consolidar_regra",
    ] {
        assert!(
            !nomes.iter().any(|x| x == n),
            "sem regras ligadas, {n} não aparece"
        );
    }
    // O `fazer_backup` existe mesmo sem planejamento.
    let sem = Servidor::novo(Nucleo::novo(&c.banco.caminho, None).unwrap());
    assert!(
        sem.nomes_das_ferramentas()
            .iter()
            .any(|x| x == "fazer_backup")
    );
    assert!(!sem.nomes_das_ferramentas().iter().any(|x| x == "desfazer"));
}
