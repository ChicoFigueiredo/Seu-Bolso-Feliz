//! Cada ferramenta contra um banco fictício (nunca o inventário real).
#![allow(clippy::unwrap_used)]

#[path = "../../sbf-triagem/tests/ajuda/mod.rs"]
mod ajuda;

use ajuda::{Banco, Reg, banco_limpo, compra, inserir_achado, inserir_registro};
use rusqlite::params;
use sbf_mcp::Nucleo;
use sbf_mcp::ferramentas::*;
use serde_json::{Value, json};

struct Cenario {
    banco: Banco,
    nucleo: Nucleo,
    mercado: i64,
    decidido: i64,
    promovido: i64,
    hash_texto: String,
    hash_decidido: String,
}

fn cenario() -> Cenario {
    let banco = banco_limpo();
    let c = banco.conexao();
    let mercado = inserir_registro(
        &c,
        &Reg {
            fornecedor: Some("mercado delta"),
            ..compra("2026-10-03", -2340, "COMPRA MERCADO DELTA 03/10", None)
        },
    );
    inserir_registro(
        &c,
        &compra("2026-10-05", -990, "PADARIA SOL", Some("padaria sol")),
    );
    inserir_registro(
        &c,
        &Reg {
            categoria: Some("Mercado"),
            ..compra(
                "2026-09-02",
                -5000,
                "MERCADO DELTA 02/09",
                Some("mercado delta"),
            )
        },
    );
    let decidido = inserir_registro(
        &c,
        &Reg {
            categoria: Some("Saude"),
            decidido_por: Some("usuario"),
            ..compra("2026-09-10", -8000, "FARMACIA", Some("farmacia"))
        },
    );
    let promovido = inserir_registro(
        &c,
        &Reg {
            estado: "promovido",
            movimentacao_id: Some("mov-1"),
            ..compra("2026-08-10", -1000, "CINEMA", Some("cinema"))
        },
    );
    let longo = format!("{} FIM", "linha de texto do boleto\n".repeat(400));
    let hash_texto = inserir_achado(&c, 21, "novo", Some(&longo), "boleto-energia.pdf");
    inserir_achado(&c, 22, "novo", None, "foto.jpg");
    let hash_decidido = inserir_achado(&c, 23, "duvida", Some("texto"), "recibo.pdf");
    c.execute(
        "UPDATE achado SET decidido_por = 'usuario' WHERE content_hash = ?1",
        params![hash_decidido],
    )
    .unwrap();
    c.execute(
        "INSERT INTO decisao (alvo_tipo, alvo_id, campo, antes, depois, autor)
         VALUES ('registro', ?1, 'categoria', '{}', '{\"categoria_provavel\":\"Saude\"}', 'usuario')",
        params![decidido.to_string()],
    )
    .unwrap();
    drop(c);
    let nucleo = Nucleo::novo(&banco.caminho, None).unwrap();
    Cenario {
        banco,
        nucleo,
        mercado,
        decidido,
        promovido,
        hash_texto,
        hash_decidido,
    }
}

fn proposta(tipo: &str, id: &str, campo: &str, valor: Value) -> SugestaoProposta {
    SugestaoProposta {
        alvo_tipo: tipo.into(),
        alvo_id: id.into(),
        campo: campo.into(),
        valor,
        confianca: 0.9,
        justificativa: "mesmo fornecedor".into(),
    }
}

#[test]
fn resumo_conta_o_inventario() {
    let c = cenario();
    let r = c.nucleo.inventario_resumo().unwrap();
    assert_eq!(r["registros"]["total"], 5);
    assert_eq!(r["registros"]["sem_categoria"], 3);
    assert_eq!(r["registros"]["promovidos"], 1);
    assert_eq!(r["achados"]["total"], 3);
    assert_eq!(r["achados"]["decididos_pelo_usuario"], 1);
    assert_eq!(r["sugerir_disponivel"], true);
    assert_eq!(r["livro_caixa_disponivel"], false);
}

#[test]
fn buscar_registros_filtra_e_pagina() {
    let c = cenario();
    let todos = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros {
            limite: Some(2),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(todos["total"], 5);
    assert_eq!(todos["itens"].as_array().unwrap().len(), 2);
    assert_eq!(todos["proximo_deslocamento"], 2);
    assert_eq!(todos["itens"][0]["data"], "2026-10-05");

    let texto = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros {
            texto: Some("mercado".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(texto["total"], 2);
    let sem = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros {
            fornecedor: Some("mercado".into()),
            sem_categoria: Some(true),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(sem["total"], 1);
    assert_eq!(sem["itens"][0]["id"], c.mercado);
    let cat = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros {
            categoria: Some("mercado".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(cat["total"], 1);
    let periodo = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros {
            de: Some("2026-10-01".into()),
            ate: Some("2026-10-31".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(periodo["total"], 2);
    let estado = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros {
            estado: Some("promovido".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(estado["itens"][0]["id"], c.promovido);
    // O `%` do texto não é curinga.
    let curinga = c
        .nucleo
        .buscar_registros(&ParamsDeRegistros {
            texto: Some("%".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(curinga["total"], 0);
}

#[test]
fn ver_registro_traz_decisoes_e_recusa_id_inexistente() {
    let c = cenario();
    let v = c.nucleo.ver_registro(c.decidido).unwrap();
    assert_eq!(v["decidido_por"], "usuario");
    assert_eq!(v["decisoes"][0]["campo"], "categoria");
    assert_eq!(
        c.nucleo.ver_registro(c.promovido).unwrap()["promovido"],
        true
    );
    assert!(
        c.nucleo
            .ver_registro(99999)
            .unwrap_err()
            .contains("não existe")
    );
}

#[test]
fn buscar_achados_por_estado_tipo_e_texto() {
    let c = cenario();
    let novos = c
        .nucleo
        .buscar_achados(&ParamsDeAchados {
            estado: Some("novo".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(novos["total"], 2);
    let por_nome = c
        .nucleo
        .buscar_achados(&ParamsDeAchados {
            texto: Some("energia".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(por_nome["total"], 1);
    assert_eq!(por_nome["itens"][0]["content_hash"], c.hash_texto);
    assert_eq!(por_nome["itens"][0]["tem_texto"], true);
    let tipo = c
        .nucleo
        .buscar_achados(&ParamsDeAchados {
            tipo: Some("boleto".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(tipo["total"], 0);
}

#[test]
fn texto_do_achado_vem_truncado_como_na_triagem() {
    let c = cenario();
    let v = c.nucleo.ver_texto_do_achado(&c.hash_texto).unwrap();
    assert_eq!(v["truncado"], true);
    let texto = v["texto"].as_str().unwrap();
    assert!(texto.contains("caracteres omitidos"));
    assert!(texto.ends_with("FIM"));
    // 3000 + 1000 + o marcador.
    assert!(texto.chars().count() < 4100);
    let curto = c.nucleo.ver_texto_do_achado(&c.hash_decidido).unwrap();
    assert_eq!(curto["truncado"], false);
    assert!(c.nucleo.ver_texto_do_achado(&ajuda::hash(250)).is_err());
    // O achado sem texto explica o motivo.
    let sem = c
        .nucleo
        .buscar_achados(&ParamsDeAchados {
            texto: Some("foto".into()),
            ..Default::default()
        })
        .unwrap();
    let h = sem["itens"][0]["content_hash"].as_str().unwrap();
    assert!(
        c.nucleo
            .ver_texto_do_achado(h)
            .unwrap_err()
            .contains("sem texto")
            || c.nucleo
                .ver_texto_do_achado(h)
                .unwrap_err()
                .contains("não tem texto")
    );
}

#[test]
fn fornecedores_contam_e_mostram_a_categoria_aprendida() {
    let c = cenario();
    let v = c
        .nucleo
        .fornecedores(&ParamsDeFornecedores::default())
        .unwrap();
    assert_eq!(v["total"], 4);
    assert_eq!(v["itens"][0]["fornecedor"], "mercado delta");
    assert_eq!(v["itens"][0]["registros"], 2);
    assert_eq!(v["itens"][0]["sem_categoria"], 1);
    let f = c
        .nucleo
        .fornecedores(&ParamsDeFornecedores {
            texto: Some("farm".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(f["total"], 1);
    // Aprendida: a farmácia tem uma decisão do usuário em vigor.
    let f = c
        .nucleo
        .fornecedores(&ParamsDeFornecedores {
            texto: Some("farm".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        f["itens"][0]["categoria_aprendida_por_tipo"]["compra"]["categoria"],
        "Saude"
    );
}

#[test]
fn decisoes_recentes_listam_o_log() {
    let c = cenario();
    let v = c
        .nucleo
        .decisoes_recentes(&ParamsDeDecisoes::default())
        .unwrap();
    assert_eq!(v["total"], 1);
    assert_eq!(v["itens"][0]["autor"], "usuario");
    let nenhuma = c
        .nucleo
        .decisoes_recentes(&ParamsDeDecisoes {
            alvo_tipo: Some("lote".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(nenhuma["total"], 0);
}

#[test]
fn livro_sem_cache_diz_o_motivo_e_com_cache_filtra() {
    let c = cenario();
    assert!(
        c.nucleo
            .livro_movimentacoes(&ParamsDoLivro::default())
            .unwrap_err()
            .contains("cache")
    );

    let dir = tempfile::tempdir().unwrap();
    let caminho = dir.path().join("livro-caixa.sqlite");
    let conn = rusqlite::Connection::open(&caminho).unwrap();
    conn.execute_batch("CREATE TABLE linha (colecao TEXT NOT NULL, id TEXT NOT NULL, versao TEXT, dados TEXT NOT NULL, PRIMARY KEY (colecao, id));").unwrap();
    let grava = |colecao: &str, id: &str, v: Value| {
        conn.execute(
            "INSERT INTO linha (colecao, id, dados) VALUES (?1, ?2, ?3)",
            params![colecao, id, v.to_string()],
        )
        .unwrap();
    };
    grava("categorias", "c1", json!({"id": "c1", "nome": "Mercado"}));
    grava("contas", "k1", json!({"id": "k1", "nome": "Corrente"}));
    grava(
        "movimentacoes",
        "m1",
        json!({"id": "m1", "conta": "k1", "tipo": "saida", "valor": 2340, "descricao": "Mercado Delta", "data": "2026-10-03", "categoria": "c1", "confirmada": true}),
    );
    grava(
        "movimentacoes",
        "m2",
        json!({"id": "m2", "conta": "k1", "tipo": "saida", "valor": 990, "descricao": "Padaria", "data": "2026-10-05", "categoria": null, "confirmada": false}),
    );
    drop(conn);
    let n = Nucleo::novo(&c.banco.caminho, Some(&caminho)).unwrap();
    let todas = n.livro_movimentacoes(&ParamsDoLivro::default()).unwrap();
    assert_eq!(todas["total"], 2);
    assert_eq!(todas["itens"][0]["id"], "m2");
    assert_eq!(todas["itens"][1]["categoria"], "Mercado");
    assert_eq!(todas["itens"][1]["conta"], "Corrente");
    let sem = n
        .livro_movimentacoes(&ParamsDoLivro {
            sem_categoria: Some(true),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(sem["total"], 1);
    let txt = n
        .livro_movimentacoes(&ParamsDoLivro {
            texto: Some("delta".into()),
            categoria: Some("mercado".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(txt["total"], 1);
}

#[test]
fn sugerir_grava_sugestao_do_mcp_numa_rodada_propria_e_nenhuma_decisao() {
    let c = cenario();
    let antes: i64 = ajuda::contar(&c.banco.conexao(), "SELECT COUNT(*) FROM decisao");
    let r = c
        .nucleo
        .sugerir(&[proposta(
            "registro",
            &c.mercado.to_string(),
            "categoria",
            json!("Mercado"),
        )])
        .unwrap();
    assert_eq!(r["gravadas"], 1);
    assert!(r["recusadas"].as_array().unwrap().is_empty());
    let conn = c.banco.conexao();
    let (modelo, estado, rodada): (String, String, i64) = conn
        .query_row(
            "SELECT modelo, estado, rodada_id FROM sugestao_ia",
            [],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .unwrap();
    assert_eq!((modelo.as_str(), estado.as_str()), ("mcp", "aberta"));
    let (prov, sit, itens): (String, String, i64) = conn
        .query_row(
            "SELECT provedor, situacao, itens FROM rodada_ia WHERE id = ?1",
            [rodada],
            |l| Ok((l.get(0)?, l.get(1)?, l.get(2)?)),
        )
        .unwrap();
    assert_eq!(
        (prov.as_str(), sit.as_str(), itens),
        ("mcp", "concluida", 1)
    );
    assert_eq!(ajuda::contar(&conn, "SELECT COUNT(*) FROM decisao"), antes);
    // E o registro continua como estava.
    assert_eq!(
        ajuda::valor_texto(
            &conn,
            &format!(
                "SELECT COALESCE(categoria_provavel, '') FROM registro WHERE id = {}",
                c.mercado
            )
        ),
        ""
    );

    let abertas = c
        .nucleo
        .sugestoes_abertas(&ParamsDeSugestoesAbertas::default())
        .unwrap();
    assert_eq!(abertas["total"], 1);
    assert_eq!(abertas["itens"][0]["valor"], "Mercado");
    assert_eq!(
        c.nucleo.ver_registro(c.mercado).unwrap()["sugestoes_abertas"][0]["campo"],
        "categoria"
    );
    // Sugerir de novo o mesmo campo substitui a anterior.
    c.nucleo
        .sugerir(&[proposta(
            "registro",
            &c.mercado.to_string(),
            "categoria",
            json!("Supermercado"),
        )])
        .unwrap();
    let abertas = c
        .nucleo
        .sugestoes_abertas(&ParamsDeSugestoesAbertas {
            modelo: Some("mcp".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(abertas["total"], 1);
    assert_eq!(abertas["itens"][0]["valor"], "Supermercado");
}

fn motivo(r: &Value, i: usize) -> String {
    r["recusadas"][i]["motivo"]
        .as_str()
        .unwrap_or_default()
        .to_owned()
}

#[test]
fn sugerir_recusa_decidido_promovido_inexistente_e_campo_invalido() {
    let c = cenario();
    let r = c
        .nucleo
        .sugerir(&[
            proposta(
                "registro",
                &c.decidido.to_string(),
                "categoria",
                json!("Saude"),
            ),
            proposta(
                "registro",
                &c.promovido.to_string(),
                "categoria",
                json!("Lazer"),
            ),
            proposta("registro", "99999", "categoria", json!("X")),
            proposta(
                "registro",
                &c.mercado.to_string(),
                "e_financeiro",
                json!("sim"),
            ),
            proposta(
                "registro",
                &c.mercado.to_string(),
                "tipo",
                json!("viagem-espacial"),
            ),
            proposta("achado", &c.hash_decidido, "e_financeiro", json!("sim")),
            proposta("achado", &c.hash_texto, "e_financeiro", json!("talvez")),
            proposta("fornecedor", "1", "categoria", json!("X")),
            SugestaoProposta {
                confianca: 1.5,
                ..proposta("registro", &c.mercado.to_string(), "categoria", json!("X"))
            },
            SugestaoProposta {
                justificativa: "x".repeat(141),
                ..proposta("registro", &c.mercado.to_string(), "categoria", json!("X"))
            },
        ])
        .unwrap();
    assert_eq!(r["gravadas"], 0);
    assert_eq!(r["rodada"], Value::Null);
    assert_eq!(r["recusadas"].as_array().unwrap().len(), 10);
    assert!(motivo(&r, 0).contains("já decidiu"));
    assert!(motivo(&r, 1).contains("promovido"));
    assert!(motivo(&r, 2).contains("não existe"));
    assert!(motivo(&r, 3).contains("não é permitido"));
    assert!(motivo(&r, 4).contains("inválido"));
    assert!(motivo(&r, 5).contains("já decidiu"));
    assert!(motivo(&r, 6).contains("inválido"));
    assert!(motivo(&r, 7).contains("alvo_tipo"));
    assert!(motivo(&r, 8).contains("confiança"));
    assert!(motivo(&r, 9).contains("justificativa"));
    let conn = c.banco.conexao();
    assert_eq!(ajuda::contar(&conn, "SELECT COUNT(*) FROM sugestao_ia"), 0);
    assert_eq!(ajuda::contar(&conn, "SELECT COUNT(*) FROM rodada_ia"), 0);
}

#[test]
fn sugerir_aceita_as_validas_e_recusa_so_as_ruins() {
    let c = cenario();
    let r = c
        .nucleo
        .sugerir(&[
            proposta("achado", &c.hash_texto, "e_financeiro", json!("sim")),
            proposta("registro", &c.decidido.to_string(), "categoria", json!("X")),
            proposta("achado", &c.hash_texto, "valor", json!(23400)),
        ])
        .unwrap();
    assert_eq!(r["gravadas"], 2);
    assert_eq!(r["recusadas"].as_array().unwrap().len(), 1);
    assert_eq!(r["recusadas"][0]["posicao"], 1);
    assert!(c.nucleo.sugerir(&[]).is_err());
}

#[test]
fn abrir_so_com_o_caminho_dado_e_sem_criar_nada() {
    let dir = tempfile::tempdir().unwrap();
    let falta = dir.path().join("inventario.sqlite");
    assert!(Nucleo::novo(&falta, None).is_err());
    assert!(!falta.exists());
}
