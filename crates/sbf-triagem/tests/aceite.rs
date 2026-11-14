//! Aceitar e recusar em lote (IA-07), obsolescência (IA-08) e o registro do que saiu (IA-05).

mod ajuda;

use ajuda::*;
use sbf_ia::{Consumo, NivelDeConfianca, Registro, Saida};
use sbf_inventario::comando::{Acao, Comandos, PedidoDeDecisao};
use sbf_inventario::sugestao::RegistroDaIa;
use sbf_inventario::{Ref, Tipo};
use sbf_triagem::Alvo;
use sbf_triagem::aceite::{MOTIVO_DA_DECISAO, aceitar, recusar};
use sbf_triagem::resposta::SugestaoValida;
use sbf_triagem::sugestoes::{
    EstadoDaSugestao, FiltroDeSugestoes, NovaSugestao, agrupar, gravar, listar_abertas,
};
use serde_json::{Value, json};

fn sugerir(banco: &Banco, alvo: Alvo, campo: &str, valor: Value, confianca: f64) -> i64 {
    let c = banco.conexao();
    gravar(
        &c,
        &NovaSugestao::da_ia(
            &alvo,
            &SugestaoValida {
                campo: campo.to_owned(),
                valor,
                confianca,
                justificativa: "teste".to_owned(),
            },
            "falso-rapido",
            "triagem-v1",
            0,
            EstadoDaSugestao::Aberta,
        ),
    )
    .expect("gravar")
}

fn banco_com(n: i64) -> (Banco, Vec<i64>) {
    let banco = banco_limpo();
    let c = banco.conexao();
    let ids = (0..n)
        .map(|i| {
            inserir_registro(
                &c,
                &compra("2026-10-05", -1000 - i, &format!("COMPRA {i}"), None),
            )
        })
        .collect();
    c.execute(
        "INSERT INTO rodada_ia (id, provedor) VALUES (0, 'falso')",
        [],
    )
    .expect("rodada");
    drop(c);
    (banco, ids)
}

#[test]
fn ia07_aceitar_em_lote_grava_uma_decisao_do_usuario_por_item_e_desfazer_funciona() {
    let (banco, ids) = banco_com(3);
    let s: Vec<i64> = ids
        .iter()
        .map(|id| {
            sugerir(
                &banco,
                Alvo::Registro(*id),
                "categoria",
                json!("Mercado"),
                0.95,
            )
        })
        .collect();
    let mut inv = banco.inventario();
    let c = banco.conexao();

    let r = aceitar(&c, &mut inv, &s).expect("aceitar");

    assert_eq!(r.aceitas.len(), 3);
    assert_eq!(r.decisoes.len(), 3, "uma decisão por item");
    assert!(r.ignoradas.is_empty());
    for id in &ids {
        assert_eq!(
            valor_texto(
                &c,
                &format!(
                    "SELECT categoria_provavel || ':' || decidido_por FROM registro WHERE id = {id}"
                )
            ),
            "Mercado:usuario"
        );
    }
    assert_eq!(
        contar(
            &c,
            &format!(
                "SELECT COUNT(*) FROM decisao WHERE autor = 'usuario' AND campo = 'categoria' AND motivo = '{MOTIVO_DA_DECISAO}'"
            )
        ),
        3
    );
    assert_eq!(
        contar(
            &c,
            "SELECT COUNT(*) FROM sugestao_ia WHERE estado = 'aceita'"
        ),
        3
    );
    assert!(
        listar_abertas(&c, &FiltroDeSugestoes::default())
            .expect("l")
            .is_empty()
    );

    // Desfazer é o de sempre.
    inv.desfazer(r.decisoes[1]).expect("desfazer");
    assert_eq!(
        valor_texto(
            &c,
            &format!(
                "SELECT COALESCE(categoria_provavel, '(nada)') FROM registro WHERE id = {}",
                ids[1]
            )
        ),
        "(nada)"
    );
    // "Desfazer decisão" da barra desfaz a última.
    inv.desfazer_ultima().expect("desfazer última");
}

#[test]
fn aceitar_acoes_de_achado_cada_valor_vira_a_sua_acao() {
    let banco = banco_limpo();
    let c = banco.conexao();
    c.execute(
        "INSERT INTO rodada_ia (id, provedor) VALUES (0, 'falso')",
        [],
    )
    .expect("rodada");
    let a = inserir_achado(&c, 1, "novo", Some("t"), "a.pdf");
    let b = inserir_achado(&c, 2, "novo", Some("t"), "b.pdf");
    drop(c);
    let s = vec![
        sugerir(
            &banco,
            Alvo::Achado(a.clone()),
            "e_financeiro",
            json!("sim"),
            0.9,
        ),
        sugerir(
            &banco,
            Alvo::Achado(a.clone()),
            "tipo_documento",
            json!("fatura"),
            0.9,
        ),
        sugerir(&banco, Alvo::Achado(a.clone()), "valor", json!(18750), 0.9),
        sugerir(
            &banco,
            Alvo::Achado(a.clone()),
            "data_chave",
            json!("2026-11-12"),
            0.9,
        ),
        sugerir(
            &banco,
            Alvo::Achado(b.clone()),
            "e_financeiro",
            json!("nao"),
            0.9,
        ),
    ];
    let mut inv = banco.inventario();
    let c = banco.conexao();

    let r = aceitar(&c, &mut inv, &s).expect("aceitar");

    assert_eq!(r.aceitas.len(), 5, "{r:?}");
    let achado = |h: &str, col: &str| {
        valor_texto(
            &c,
            &format!(
                "SELECT CAST(COALESCE({col}, '') AS TEXT) FROM achado WHERE content_hash = '{h}'"
            ),
        )
    };
    assert_eq!(achado(&a, "estado"), "documento");
    assert_eq!(achado(&a, "document_type"), "fatura");
    assert_eq!(achado(&a, "valor_centavos"), "18750");
    assert_eq!(achado(&a, "data_chave"), "2026-11-12");
    assert_eq!(achado(&b, "estado"), "descartado");
    assert_eq!(achado(&b, "decidido_por"), "usuario");
}

#[test]
fn conta_e_duplicata_de_registro_ficam_so_para_leitura() {
    let (banco, ids) = banco_com(2);
    let conta = sugerir(
        &banco,
        Alvo::Registro(ids[0]),
        "conta",
        json!("Banco Alfa"),
        0.9,
    );
    let dup = sugerir(
        &banco,
        Alvo::Registro(ids[1]),
        "duplicata_de",
        json!(ids[0]),
        0.9,
    );
    let mut inv = banco.inventario();
    let c = banco.conexao();

    let r = aceitar(&c, &mut inv, &[conta, dup]).expect("aceitar");

    assert!(r.aceitas.is_empty());
    assert_eq!(r.ignoradas.len(), 2);
    assert!(r.ignoradas[0].1.contains("só leitura"));
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM decisao"), 0);
}

#[test]
fn registro_promovido_e_terminal_a_sugestao_vira_obsoleta() {
    let banco = banco_limpo();
    let c = banco.conexao();
    c.execute(
        "INSERT INTO rodada_ia (id, provedor) VALUES (0, 'falso')",
        [],
    )
    .expect("rodada");
    let id = inserir_registro(
        &c,
        &Reg {
            estado: "promovido",
            movimentacao_id: Some("m1"),
            ..compra("2026-10-05", -100, "X", None)
        },
    );
    drop(c);
    let s = sugerir(&banco, Alvo::Registro(id), "categoria", json!("Casa"), 0.9);
    let mut inv = banco.inventario();
    let c = banco.conexao();

    let r = aceitar(&c, &mut inv, &[s]).expect("aceitar");

    assert!(r.aceitas.is_empty());
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM decisao"), 0);
    assert_eq!(
        valor_texto(&c, "SELECT estado FROM sugestao_ia"),
        "obsoleta"
    );
}

#[test]
fn um_item_que_falha_nao_derruba_o_grupo() {
    let (banco, ids) = banco_com(2);
    let ok = sugerir(
        &banco,
        Alvo::Registro(ids[0]),
        "categoria",
        json!("Casa"),
        0.9,
    );
    let sumido = sugerir(
        &banco,
        Alvo::Registro(9999),
        "categoria",
        json!("Casa"),
        0.9,
    );
    let mut inv = banco.inventario();
    let c = banco.conexao();

    let r = aceitar(&c, &mut inv, &[ok, sumido]).expect("aceitar");

    assert_eq!(r.aceitas, vec![ok]);
    assert_eq!(r.ignoradas.len(), 1);
    assert_eq!(r.ignoradas[0].0, sumido);
}

#[test]
fn recusar_nao_grava_decisao() {
    let (banco, ids) = banco_com(2);
    let s: Vec<i64> = ids
        .iter()
        .map(|id| sugerir(&banco, Alvo::Registro(*id), "categoria", json!("Casa"), 0.9))
        .collect();
    let c = banco.conexao();
    assert_eq!(recusar(&c, &s[..1]).expect("recusar"), 1);
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM decisao"), 0);
    assert_eq!(
        contar(
            &c,
            "SELECT COUNT(*) FROM sugestao_ia WHERE estado = 'recusada'"
        ),
        1
    );
    assert_eq!(
        listar_abertas(&c, &FiltroDeSugestoes::default())
            .expect("l")
            .len(),
        1
    );
    // Recusar de novo a mesma não conta.
    assert_eq!(recusar(&c, &s[..1]).expect("recusar"), 0);
}

#[test]
fn ia08_decisao_do_usuario_depois_da_sugestao_a_torna_obsoleta() {
    let (banco, ids) = banco_com(1);
    let categoria = sugerir(
        &banco,
        Alvo::Registro(ids[0]),
        "categoria",
        json!("Mercado"),
        0.9,
    );
    let tipo = sugerir(&banco, Alvo::Registro(ids[0]), "tipo", json!("debito"), 0.9);
    let mut inv = banco.inventario();

    inv.executar(&PedidoDeDecisao {
        acao: Acao::CorrigirCategoria("Casa".into()),
        alvos: vec![Ref::nova(Tipo::Registro, ids[0].to_string())],
        motivo: None,
    })
    .expect("decidir");

    let c = banco.conexao();
    let estado = |id: i64| {
        valor_texto(
            &c,
            &format!("SELECT estado FROM sugestao_ia WHERE id = {id}"),
        )
    };
    assert_eq!(
        estado(categoria),
        "obsoleta",
        "o campo decidido por outro caminho"
    );
    assert_eq!(estado(tipo), "aberta", "outro campo não é afetado");
    assert!(
        listar_abertas(&c, &FiltroDeSugestoes::default())
            .expect("l")
            .iter()
            .all(|s| s.id != categoria)
    );
}

#[test]
fn ia08_aceitar_data_nao_obsoleta_a_sugestao_de_valor() {
    let banco = banco_limpo();
    let c = banco.conexao();
    c.execute(
        "INSERT INTO rodada_ia (id, provedor) VALUES (0, 'falso')",
        [],
    )
    .expect("rodada");
    let a = inserir_achado(&c, 1, "novo", Some("t"), "a.pdf");
    drop(c);
    let data = sugerir(
        &banco,
        Alvo::Achado(a.clone()),
        "data_chave",
        json!("2026-11-12"),
        0.9,
    );
    let valor = sugerir(&banco, Alvo::Achado(a), "valor", json!(100), 0.9);
    let mut inv = banco.inventario();
    let c = banco.conexao();

    aceitar(&c, &mut inv, &[data]).expect("aceitar");

    assert_eq!(
        valor_texto(
            &c,
            &format!("SELECT estado FROM sugestao_ia WHERE id = {data}")
        ),
        "aceita"
    );
    assert_eq!(
        valor_texto(
            &c,
            &format!("SELECT estado FROM sugestao_ia WHERE id = {valor}")
        ),
        "aberta"
    );
}

#[test]
fn agrupar_por_campo_e_valor_com_o_maior_grupo_primeiro() {
    let (banco, ids) = banco_com(5);
    for (i, id) in ids.iter().enumerate() {
        let valor = if i < 3 { "Mercado" } else { "Casa" };
        sugerir(
            &banco,
            Alvo::Registro(*id),
            "categoria",
            json!(valor),
            0.9 - i as f64 * 0.01,
        );
    }
    let c = banco.conexao();
    let grupos = agrupar(listar_abertas(&c, &FiltroDeSugestoes::default()).expect("l"));
    assert_eq!(grupos.len(), 2);
    assert_eq!(
        (grupos[0].valor_texto.as_str(), grupos[0].sugestoes.len()),
        ("Mercado", 3)
    );
    assert_eq!(grupos[1].ids().len(), 2);
    // O filtro de confiança corta antes de agrupar.
    let filtro = FiltroDeSugestoes {
        confianca_minima: Some(0.895),
        ..FiltroDeSugestoes::default()
    };
    assert_eq!(listar_abertas(&c, &filtro).expect("l").len(), 1);
}

#[test]
fn ia05_registro_da_ia_guarda_tamanho_tokens_e_custo_nunca_o_texto_e_faz_cache() {
    let (banco, _) = banco_com(1);
    let registro = RegistroDaIa::abrir(&banco.caminho, Some(0)).expect("abrir");
    registro
        .registrar_saida(&Saida {
            destinatario: "triagem".into(),
            provedor: "falso".into(),
            modelo: "falso-rapido".into(),
            nivel: NivelDeConfianca::Completo,
            bytes_enviados: 4321,
            consumo: Consumo {
                tokens_entrada: 900,
                tokens_saida: 120,
                custo_dezmilesimos_usd: 77,
                nominal: true,
            },
            quando: "2026-11-14T10:00:00Z".into(),
        })
        .expect("saída");
    let c = banco.conexao();
    assert_eq!(
        valor_texto(
            &c,
            "SELECT destinatario||'|'||nivel||'|'||bytes_enviados||'|'||tokens_entrada||'|'||custo_dezmilesimos_usd||'|'||nominal||'|'||rodada_id FROM saida_ia"
        ),
        "triagem|completo|4321|900|77|1|0"
    );
    // Nenhuma coluna de saida_ia pode guardar o texto enviado.
    let colunas = valor_texto(
        &c,
        "SELECT group_concat(name) FROM pragma_table_info('saida_ia')",
    );
    assert!(
        !colunas.contains("texto")
            && !colunas.contains("entrada_json")
            && !colunas.contains("prompt"),
        "{colunas}"
    );

    assert_eq!(registro.ler_cache("abc").expect("ler"), None);
    registro
        .gravar_cache("abc", &json!({"itens": []}))
        .expect("gravar");
    assert_eq!(
        registro.ler_cache("abc").expect("ler"),
        Some(json!({"itens": []}))
    );
}
