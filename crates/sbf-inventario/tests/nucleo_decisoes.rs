//! Cada Ação do §10 grava a decisão e a projeção na mesma transação; desfazer restaura
//! (INV-05). A decisão do usuário fica marcada em `decidido_por` (INV-06, lado Rust).
#![allow(clippy::unwrap_used)]

mod nucleo_comum;

use nucleo_comum::*;
use rusqlite::Connection;
use sbf_inventario::banco::Inventario;
use sbf_inventario::{Acao, Comandos, ErroDeComando, PedidoDeDecisao, Ref, Tipo};

fn pedido(acao: Acao, alvos: Vec<Ref>) -> PedidoDeDecisao {
    PedidoDeDecisao {
        acao,
        alvos,
        motivo: None,
    }
}

fn achado_col(c: &Connection, n: u8, coluna: &str) -> String {
    valor(
        c,
        &format!(
            "SELECT {coluna} FROM achado WHERE content_hash = '{}'",
            hash(n)
        ),
    )
}

fn registro_col(c: &Connection, id: i64, coluna: &str) -> String {
    valor(c, &format!("SELECT {coluna} FROM registro WHERE id = {id}"))
}

/// (alvo_tipo, alvo_id, campo, antes, depois, autor, motivo, desfaz) da decisão.
fn decisao(c: &Connection, id: i64) -> Vec<String> {
    [
        "alvo_tipo",
        "alvo_id",
        "campo",
        "antes",
        "depois",
        "autor",
        "motivo",
        "desfaz",
    ]
    .iter()
    .map(|col| valor(c, &format!("SELECT {col} FROM decisao WHERE id = {id}")))
    .collect()
}

fn novo() -> (tempfile::TempDir, Inventario) {
    let pasta = tempfile::tempdir().unwrap();
    let inventario = inventario_migrado(pasta.path());
    (pasta, inventario)
}

#[test]
fn inv05_marcar_documento_grava_decisao_e_projecao() {
    let (_p, mut inv) = novo();
    let ids = inv
        .executar(&pedido(Acao::MarcarDocumento, vec![achado(1)]))
        .unwrap();
    let c = inv.conexao();
    assert_eq!(achado_col(c, 1, "estado"), "documento");
    assert_eq!(achado_col(c, 1, "decidido_por"), "usuario");
    let d = decisao(c, ids[0]);
    assert_eq!(d[0..3], ["achado", &hash(1), "estado"]);
    assert_eq!(d[3], r#"{"estado":"novo"}"#);
    assert_eq!(d[4], r#"{"estado":"documento"}"#);
    assert_eq!(d[5], "usuario");
    assert_eq!(d[7], "∅");
}

#[test]
fn inv05_descartar_e_em_duvida_em_varios_alvos() {
    let (_p, mut inv) = novo();
    let ids = inv
        .executar(&pedido(Acao::Descartar, vec![achado(1), achado(3)]))
        .unwrap();
    assert_eq!(ids.len(), 2);
    inv.executar(&pedido(Acao::EmDuvida, vec![achado(2)]))
        .unwrap();
    let c = inv.conexao();
    assert_eq!(achado_col(c, 1, "estado"), "descartado");
    assert_eq!(achado_col(c, 3, "estado"), "descartado");
    assert_eq!(achado_col(c, 2, "estado"), "duvida");
}

#[test]
fn inv05_documento_e_o_mesmo_conteudo_do_achado() {
    let (_p, mut inv) = novo();
    inv.executar(&pedido(
        Acao::Descartar,
        vec![Ref::nova(Tipo::Documento, hash(2))],
    ))
    .unwrap();
    assert_eq!(achado_col(inv.conexao(), 2, "estado"), "descartado");
}

#[test]
fn inv05_corrigir_fornecedor_categoria_e_tipo() {
    let (_p, mut inv) = novo();
    inv.executar(&pedido(
        Acao::CorrigirFornecedor("Água Exemplo".into()),
        vec![achado(2), registro(1)],
    ))
    .unwrap();
    inv.executar(&pedido(
        Acao::CorrigirCategoria("moradia".into()),
        vec![registro(1)],
    ))
    .unwrap();
    inv.executar(&pedido(
        Acao::CorrigirTipo("conta de consumo".into()),
        vec![achado(2)],
    ))
    .unwrap();
    inv.executar(&pedido(
        Acao::CorrigirTipo("tarifa".into()),
        vec![registro(2)],
    ))
    .unwrap();
    let c = inv.conexao();
    assert_eq!(achado_col(c, 2, "supplier_name_raw"), "Água Exemplo");
    assert_eq!(registro_col(c, 1, "fornecedor_normalizado"), "Água Exemplo");
    assert_eq!(registro_col(c, 1, "categoria_provavel"), "moradia");
    assert_eq!(achado_col(c, 2, "document_type"), "conta de consumo");
    assert_eq!(registro_col(c, 2, "tipo"), "tarifa");
    assert_eq!(registro_col(c, 2, "decidido_por"), "usuario");
}

#[test]
fn inv05_corrigir_data_recalcula_o_ciclo_e_cria_a_linha_do_ciclo() {
    let (_p, mut inv) = novo();
    inv.executar(&pedido(
        Acao::CorrigirDataEValor {
            data: Some("2026-12-19".into()),
            valor_centavos: Some(777),
        },
        vec![achado(2)],
    ))
    .unwrap();
    inv.executar(&pedido(
        Acao::CorrigirDataEValor {
            data: Some("2027-01-20".into()),
            valor_centavos: None,
        },
        vec![registro(3)],
    ))
    .unwrap();
    let c = inv.conexao();
    assert_eq!(achado_col(c, 2, "data_chave"), "2026-12-19");
    assert_eq!(achado_col(c, 2, "ciclo_id"), "2026-11");
    assert_eq!(achado_col(c, 2, "valor_centavos"), "777");
    assert_eq!(registro_col(c, 3, "data"), "2027-01-20");
    assert_eq!(registro_col(c, 3, "ciclo_id"), "2027-01");
    assert_eq!(registro_col(c, 3, "valor_centavos"), "100000");
    assert_eq!(
        valor(
            c,
            "SELECT inicio || ' a ' || fim FROM ciclo WHERE id = '2026-11'"
        ),
        "2026-11-20 a 2026-12-19"
    );
    assert_eq!(
        valor(
            c,
            "SELECT inicio || ' a ' || fim FROM ciclo WHERE id = '2027-01'"
        ),
        "2027-01-20 a 2027-02-19"
    );
}

#[test]
fn inv05_data_inexistente_nao_se_aplica() {
    let (_p, mut inv) = novo();
    let erro = inv
        .executar(&pedido(
            Acao::CorrigirDataEValor {
                data: Some("2026-02-30".into()),
                valor_centavos: None,
            },
            vec![achado(2)],
        ))
        .unwrap_err();
    assert!(matches!(erro, ErroDeComando::NaoSeAplica { .. }), "{erro}");
}

#[test]
fn inv05_marcar_duplicidade_e_unir_conciliados() {
    let (_p, mut inv) = novo();
    inv.executar(&pedido(
        Acao::MarcarDuplicidade { com: achado(2) },
        vec![achado(4)],
    ))
    .unwrap();
    inv.executar(&pedido(
        Acao::UnirConciliados { com: registro(1) },
        vec![registro(2)],
    ))
    .unwrap();
    let c = inv.conexao();
    assert_eq!(
        valor(
            c,
            "SELECT a || '|' || b FROM relacao WHERE tipo = 'duplicidade'"
        ),
        format!("{}|{}", hash(2), hash(4))
    );
    assert_eq!(
        valor(
            c,
            "SELECT a || '|' || b FROM relacao_registro WHERE tipo = 'conciliacao'"
        ),
        "1|2"
    );
    // Consigo mesmo não se aplica.
    let erro = inv
        .executar(&pedido(
            Acao::MarcarDuplicidade { com: achado(4) },
            vec![achado(4)],
        ))
        .unwrap_err();
    assert!(matches!(erro, ErroDeComando::NaoSeAplica { .. }));
}

#[test]
fn inv05_confirmar_recorrencia_e_so_a_decisao() {
    let (_p, mut inv) = novo();
    let antes = retrato(inv.conexao());
    let ids = inv
        .executar(&pedido(
            Acao::ConfirmarRecorrencia,
            vec![fornecedor("Energia Exemplo")],
        ))
        .unwrap();
    let c = inv.conexao();
    assert_eq!(retrato(c), antes);
    let d = decisao(c, ids[0]);
    assert_eq!(d[0..3], ["fornecedor", "Energia Exemplo", "recorrente"]);
    assert_eq!(d[3], r#"{"recorrente":false}"#);
    assert_eq!(d[4], r#"{"recorrente":true}"#);
}

#[test]
fn inv05_aprovar_e_rejeitar_com_motivo() {
    let (_p, mut inv) = novo();
    inv.executar(&pedido(Acao::Aprovar, vec![registro(1)]))
        .unwrap();
    let sem_motivo = inv
        .executar(&pedido(Acao::Rejeitar, vec![registro(2)]))
        .unwrap_err();
    assert!(matches!(sem_motivo, ErroDeComando::MotivoObrigatorio));
    let ids = inv
        .executar(&PedidoDeDecisao {
            acao: Acao::Rejeitar,
            alvos: vec![registro(2)],
            motivo: Some("valor não confere com a fatura".into()),
        })
        .unwrap();
    let c = inv.conexao();
    assert_eq!(registro_col(c, 1, "estado"), "aprovado");
    assert_eq!(registro_col(c, 2, "estado"), "divergente");
    assert_eq!(decisao(c, ids[0])[6], "valor não confere com a fatura");
    // Aprovar não se aplica a achado.
    let erro = inv
        .executar(&pedido(Acao::Aprovar, vec![achado(1)]))
        .unwrap_err();
    assert!(matches!(erro, ErroDeComando::NaoSeAplica { .. }));
}

#[test]
fn inv05_sinalizar_e_responder_pendencia() {
    let (_p, mut inv) = novo();
    let ids = inv
        .executar(&pedido(
            Acao::Sinalizar {
                pergunta: "Este boleto é meu?".into(),
            },
            vec![achado(2)],
        ))
        .unwrap();
    let c = inv.conexao();
    let id_pendencia = decisao(c, ids[0])[1].clone();
    assert_eq!(id_pendencia, "2", "a pendência 1 é do Bun");
    assert_eq!(
        valor(
            c,
            "SELECT referencia || '|' || pergunta FROM pendencia WHERE id = 2"
        ),
        format!("achado:{}|Este boleto é meu?", hash(2))
    );
    assert_eq!(
        valor(c, "SELECT resolvida_em FROM pendencia WHERE id = 2"),
        "∅"
    );
    inv.executar(&pedido(
        Acao::Responder {
            resposta: "sim".into(),
        },
        vec![
            Ref::nova(Tipo::Pendencia, "2"),
            Ref::nova(Tipo::Pendencia, "1"),
        ],
    ))
    .unwrap();
    let c = inv.conexao();
    assert_eq!(
        valor(c, "SELECT resposta FROM pendencia WHERE id = 2"),
        "sim"
    );
    assert_ne!(
        valor(c, "SELECT resolvida_em FROM pendencia WHERE id = 2"),
        "∅"
    );
    assert_eq!(
        valor(c, "SELECT resposta FROM pendencia WHERE id = 1"),
        "sim"
    );
}

#[test]
fn inv05_guardar_regra_de_senha_grava_so_a_estrategia() {
    let (_p, mut inv) = novo();
    let regra = Acao::GuardarRegraDeSenha {
        document_type: "fatura".into(),
        estrategia: "cpf_5_primeiros".into(),
    };
    inv.executar(&pedido(regra.clone(), vec![fornecedor("Banco Fictício")]))
        .unwrap();
    // De novo: atualiza, não duplica.
    inv.executar(&pedido(regra, vec![fornecedor("Banco Fictício")]))
        .unwrap();
    let c = inv.conexao();
    assert_eq!(
        valor(
            c,
            "SELECT COUNT(*) || '|' || fornecedor || '|' || estrategia || '|' || ativa FROM regra_senha"
        ),
        "1|Banco Fictício|cpf_5_primeiros|1"
    );
}

#[test]
fn inv05_falha_num_alvo_nao_grava_nenhum() {
    let (_p, mut inv) = novo();
    let antes = retrato(inv.conexao());
    let erro = inv
        .executar(&pedido(Acao::MarcarDocumento, vec![achado(1), achado(99)]))
        .unwrap_err();
    assert!(matches!(erro, ErroDeComando::NaoEncontrado(_)));
    let c = inv.conexao();
    assert_eq!(retrato(c), antes);
    assert_eq!(valor(c, "SELECT COUNT(*) FROM decisao"), "0");
}

#[test]
fn inv05_desfazer_restaura_o_antes_e_quem_decidiu() {
    let (_p, mut inv) = novo();
    let antes = retrato(inv.conexao());
    let ids = inv
        .executar(&pedido(Acao::Descartar, vec![achado(2)]))
        .unwrap();
    let desfeita = inv.desfazer(ids[0]).unwrap();
    let c = inv.conexao();
    assert_eq!(retrato(c), antes);
    assert_eq!(achado_col(c, 2, "decidido_por"), "regras@2");
    let d = decisao(c, desfeita);
    assert_eq!(d[3], r#"{"estado":"descartado"}"#);
    assert_eq!(d[4], r#"{"estado":"documento"}"#);
    assert_eq!(d[7], ids[0].to_string());
    // Não se desfaz duas vezes.
    assert!(matches!(
        inv.desfazer(ids[0]),
        Err(ErroDeComando::NadaADesfazer(_))
    ));
    // Desfazer o desfazer refaz.
    inv.desfazer(desfeita).unwrap();
    let c = inv.conexao();
    assert_eq!(achado_col(c, 2, "estado"), "descartado");
    assert_eq!(achado_col(c, 2, "decidido_por"), "usuario");
}

#[test]
fn inv05_desfazer_uma_de_duas_decisoes_mantem_a_marca_do_usuario() {
    let (_p, mut inv) = novo();
    let estado = inv
        .executar(&pedido(Acao::MarcarDocumento, vec![achado(1)]))
        .unwrap();
    inv.executar(&pedido(
        Acao::CorrigirTipo("recibo".into()),
        vec![achado(1)],
    ))
    .unwrap();
    inv.desfazer(estado[0]).unwrap();
    let c = inv.conexao();
    assert_eq!(achado_col(c, 1, "estado"), "novo");
    assert_eq!(achado_col(c, 1, "document_type"), "recibo");
    assert_eq!(achado_col(c, 1, "decidido_por"), "usuario");
}

#[test]
fn inv05_desfazer_ultima_volta_no_tempo() {
    let (_p, mut inv) = novo();
    let antes = retrato(inv.conexao());
    inv.executar(&pedido(Acao::MarcarDocumento, vec![achado(1)]))
        .unwrap();
    inv.executar(&pedido(
        Acao::MarcarDuplicidade { com: achado(1) },
        vec![achado(3)],
    ))
    .unwrap();
    inv.executar(&pedido(
        Acao::Sinalizar {
            pergunta: "?".into(),
        },
        vec![registro(3)],
    ))
    .unwrap();
    assert!(inv.desfazer_ultima().unwrap().is_some());
    // A pendência sinalizada fecha, não some.
    assert_eq!(
        valor(inv.conexao(), "SELECT resposta FROM pendencia WHERE id = 2"),
        "(sinalização desfeita)"
    );
    assert!(inv.desfazer_ultima().unwrap().is_some());
    assert!(inv.desfazer_ultima().unwrap().is_some());
    assert_eq!(inv.desfazer_ultima().unwrap(), None);
    let depois = retrato(inv.conexao());
    // Tudo volta, menos a pendência fechada.
    let sem_pendencia = |r: &str| {
        r.lines()
            .filter(|l| !l.contains("sinalizada"))
            .collect::<Vec<_>>()
            .join("\n")
    };
    assert_eq!(sem_pendencia(&depois), sem_pendencia(&antes));
}

#[test]
fn inv05_desfazer_decisao_inexistente() {
    let (_p, mut inv) = novo();
    assert!(matches!(
        inv.desfazer(42),
        Err(ErroDeComando::NadaADesfazer(42))
    ));
}

#[test]
fn inv05_sinalizar_vale_para_qualquer_item_com_pagina() {
    let (_p, mut inv) = novo();
    inv.executar(&pedido(
        Acao::Sinalizar {
            pergunta: "Esta conexão ainda vale?".into(),
        },
        vec![
            Ref::nova(Tipo::Conexao, "disco:teste"),
            fornecedor("Energia Exemplo"),
        ],
    ))
    .unwrap();
    let c = inv.conexao();
    assert_eq!(
        valor(
            c,
            "SELECT group_concat(referencia, ',') FROM pendencia WHERE tipo = 'sinalizada'"
        ),
        "conexao:disco:teste,fornecedor:Energia Exemplo"
    );
    // Um achado que não existe não ganha pendência.
    assert!(matches!(
        inv.executar(&pedido(
            Acao::Sinalizar {
                pergunta: "?".into()
            },
            vec![achado(77)]
        )),
        Err(ErroDeComando::NaoEncontrado(_))
    ));
}

/// O retrato sem a tabela `ciclo`: a linha de um ciclo é calendário, não decisão, e fica
/// depois que a data que a criou é desfeita.
fn sem_ciclo(retrato: String) -> String {
    retrato
        .split("## ciclo")
        .next()
        .unwrap_or_default()
        .to_owned()
}

/// Cada Ação do §10, executada e desfeita, devolve a projeção ao que era. Sinalizar fica de
/// fora (a pendência fecha em vez de sumir: os scripts também criam pendências; ver
/// `inv05_desfazer_ultima_volta_no_tempo`).
#[test]
fn inv05_desfazer_restaura_cada_acao_do_paragrafo_10() {
    let regra = Acao::GuardarRegraDeSenha {
        document_type: "fatura".into(),
        estrategia: "cpf_completo".into(),
    };
    let casos: Vec<(Acao, Ref)> = vec![
        (Acao::MarcarDocumento, achado(1)),
        (Acao::Descartar, achado(2)),
        (Acao::EmDuvida, achado(4)),
        (Acao::CorrigirFornecedor("Outro".into()), achado(2)),
        (Acao::CorrigirFornecedor("Outro".into()), registro(1)),
        (Acao::CorrigirCategoria("lazer".into()), registro(2)),
        (Acao::CorrigirTipo("extrato".into()), achado(4)),
        (Acao::CorrigirTipo("estorno".into()), registro(3)),
        (
            Acao::CorrigirDataEValor {
                data: Some("2025-03-02".into()),
                valor_centavos: Some(1),
            },
            achado(2),
        ),
        (
            Acao::CorrigirDataEValor {
                data: Some("2025-03-02".into()),
                valor_centavos: Some(1),
            },
            registro(1),
        ),
        (Acao::MarcarDuplicidade { com: achado(2) }, achado(3)),
        (Acao::UnirConciliados { com: registro(3) }, registro(2)),
        (Acao::ConfirmarRecorrencia, fornecedor("Energia Exemplo")),
        (Acao::Aprovar, registro(3)),
        (
            Acao::Responder {
                resposta: "não sei".into(),
            },
            Ref::nova(Tipo::Pendencia, "1"),
        ),
        (regra, fornecedor("Banco Fictício")),
    ];
    for (acao, alvo) in casos {
        let (_p, mut inv) = novo();
        let antes = sem_ciclo(retrato(inv.conexao()));
        let ids = inv
            .executar(&pedido(acao.clone(), vec![alvo.clone()]))
            .unwrap();
        inv.desfazer(ids[0]).unwrap();
        assert_eq!(
            sem_ciclo(retrato(inv.conexao())),
            antes,
            "{acao:?} em {alvo} não foi restaurada"
        );
    }
    // Rejeitar precisa de motivo.
    let (_p, mut inv) = novo();
    let antes = retrato(inv.conexao());
    let ids = inv
        .executar(&PedidoDeDecisao {
            acao: Acao::Rejeitar,
            alvos: vec![registro(1)],
            motivo: Some("não reconheço".into()),
        })
        .unwrap();
    inv.desfazer(ids[0]).unwrap();
    assert_eq!(retrato(inv.conexao()), antes);
}
