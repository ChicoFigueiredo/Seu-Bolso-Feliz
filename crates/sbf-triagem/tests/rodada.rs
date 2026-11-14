//! A rodada com o provedor falso: IA-03, IA-04, dois degraus, pausa e o texto longo.

mod ajuda;

use std::sync::Mutex;

use ajuda::*;
use sbf_ia::{Degrau, ErroDeIa};
use sbf_triagem::Alvo;
use sbf_triagem::fila::{OpcoesDaFila, selecionar};
use sbf_triagem::rodada::{ConfigDaRodada, Controle, Progresso, SituacaoDaRodada, rodar};
use sbf_triagem::sugestoes::{FiltroDeSugestoes, listar_abertas};
use serde_json::json;

fn rodar_fila(banco: &Banco, falso: &Falso) -> Progresso {
    let c = banco.conexao();
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    let progresso = Mutex::new(Progresso::ocioso());
    rodar(
        &c,
        falso,
        &fila.alvos,
        &ConfigDaRodada::default(),
        &Controle::default(),
        &progresso,
    )
    .expect("rodar")
}

fn banco_com_registros(n: i64) -> (Banco, Vec<i64>) {
    let banco = banco_limpo();
    let c = banco.conexao();
    let ids = (0..n)
        .map(|i| inserir_registro(&c, &compra("2026-10-05", -1000 - i, &format!("COMPRA {i}"), None)))
        .collect();
    drop(c);
    (banco, ids)
}

#[test]
fn lote_valido_grava_sugestoes_abertas_e_o_prompt_leva_o_texto_inteiro() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let h = inserir_achado(
        &c,
        1,
        "novo",
        Some("ENERGIA EXEMPLO S.A. Fatura. CPF 123.456.789-09 Total R$ 187,50"),
        "luz.pdf",
    );
    let r = inserir_registro(&c, &compra("2026-10-05", -2340, "PIX PADARIA BETA", None));
    drop(c);
    let falso = Falso::com(|p| {
        Ok(json!({"itens": [
            {"n": 1, "sugestoes": [
                s("e_financeiro", json!("sim"), 0.98),
                s("tipo_documento", json!("fatura"), 0.97),
                s("valor", json!(18750), 0.97)]},
            {"n": 2, "sugestoes": [s("categoria", json!("Padaria"), 0.85)]},
        ]}))
        .inspect(|_| assert_eq!(p.degrau, Degrau::Rapido))
    });

    let resumo = rodar_fila(&banco, &falso);

    assert_eq!(resumo.situacao, SituacaoDaRodada::Concluida);
    assert_eq!((resumo.total, resumo.processados, resumo.chamadas), (2, 2, 1));
    assert_eq!(resumo.sugestoes, 4);
    // Nível completo: o CPF vai como está, e o prompt não vai em argv (é só o campo `entrada`).
    let pedido = falso.pedido(0);
    assert!(pedido.entrada.contains("CPF 123.456.789-09"));
    assert_eq!(pedido.prompt_versao, "triagem-v1");
    assert!(pedido.instrucao.contains("140 caracteres"));
    let c = banco.conexao();
    let abertas = listar_abertas(&c, &FiltroDeSugestoes::default()).expect("listar");
    assert_eq!(abertas.len(), 4);
    assert!(abertas.iter().any(|s| s.alvo == Alvo::Achado(h.clone()) && s.campo == "valor"));
    assert!(abertas.iter().any(|s| s.alvo == Alvo::Registro(r) && s.campo == "categoria"));
    // Nada virou decisão.
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM decisao"), 0);
    // A rodada ficou registrada.
    assert_eq!(
        valor_texto(&c, "SELECT situacao || ':' || itens || ':' || chamadas FROM rodada_ia"),
        "concluida:2:1"
    );
}

#[test]
fn ia03_item_invalido_volta_uma_vez_e_depois_vira_nao_classificado() {
    let (banco, ids) = banco_com_registros(2);
    // 1ª chamada: só o item 1 vale. 2ª (só o item que faltou): continua inválido.
    let falso = Falso::em_fila(vec![
        Ok(json!({"itens": [{"n": 1, "sugestoes": [s("categoria", json!("Casa"), 0.9)]}]})),
        Ok(json!({"itens": []})),
    ]);

    let resumo = rodar_fila(&banco, &falso);

    assert_eq!(falso.chamadas(), 2, "uma retentativa só");
    let reenvio = falso.pedido(1).entrada;
    assert_eq!(reenvio.matches("=== ITEM").count(), 1, "o reenvio leva só o inválido");
    assert!(reenvio.contains(&format!("registro {}", ids[1])));
    assert_eq!(resumo.nao_classificados, 1);
    let c = banco.conexao();
    assert_eq!(
        contar(
            &c,
            &format!("SELECT COUNT(*) FROM sugestao_ia WHERE campo = 'nao_classificado' AND alvo_id = '{}'", ids[1])
        ),
        1
    );
    // Não aparece na lista e não volta à fila.
    assert_eq!(listar_abertas(&c, &FiltroDeSugestoes::default()).expect("l").len(), 1);
    assert!(selecionar(&c, &OpcoesDaFila::padrao()).expect("fila").alvos.is_empty());
}

#[test]
fn ia03_retentativa_que_acerta_conta_como_sugestao() {
    let (banco, _) = banco_com_registros(1);
    let falso = Falso::em_fila(vec![
        Err(ErroDeIa::RespostaInvalida("não é JSON".into())),
        Ok(json!({"itens": [{"n": 1, "sugestoes": [s("categoria", json!("Casa"), 0.9)]}]})),
    ]);
    let resumo = rodar_fila(&banco, &falso);
    assert_eq!((resumo.sugestoes, resumo.nao_classificados, resumo.chamadas), (1, 0, 2));
}

#[test]
fn dois_degraus_so_o_incerto_sobe_e_nunca_ha_terceiro() {
    let (banco, ids) = banco_com_registros(3);
    let falso = Falso::em_fila(vec![
        // Rápido: item 2 incerto.
        Ok(json!({"itens": [
            {"n": 1, "sugestoes": [s("categoria", json!("Casa"), 0.95)]},
            {"n": 2, "sugestoes": [s("categoria", json!("Talvez"), 0.4)]},
            {"n": 3, "sugestoes": [s("categoria", json!("Lazer"), 0.7)]},
        ]})),
        // Forte, só o item 2 (agora n=1 no reenvio).
        Ok(json!({"itens": [{"n": 1, "sugestoes": [s("categoria", json!("Saúde"), 0.6)]}]})),
    ]);

    let resumo = rodar_fila(&banco, &falso);

    assert_eq!(falso.chamadas(), 2, "o forte não sobe de novo mesmo com 0.6");
    assert_eq!(falso.pedido(0).degrau, Degrau::Rapido);
    assert_eq!(falso.pedido(1).degrau, Degrau::Forte);
    assert!(falso.pedido(1).entrada.contains(&format!("registro {}", ids[1])));
    assert_eq!(resumo.subiram_de_degrau, 1);
    let c = banco.conexao();
    assert_eq!(
        valor_texto(
            &c,
            &format!("SELECT valor_json || ':' || modelo FROM sugestao_ia WHERE alvo_id = '{}'", ids[1])
        ),
        "\"Saúde\":falso-forte"
    );
}

#[test]
fn ia04_repetir_a_rodada_nao_chama_de_novo() {
    let (banco, _) = banco_com_registros(4);
    let falso = Falso::com(|p| Ok(categoria_para_todos(p, "Casa", 0.9)));
    rodar_fila(&banco, &falso);
    assert_eq!(falso.chamadas(), 1);
    let c = banco.conexao();
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    assert!(fila.alvos.is_empty());
    assert_eq!(fila.contagens.ja_sugeridos, 4);
}

#[test]
fn lotes_de_25_itens() {
    let (banco, _) = banco_com_registros(60);
    let falso = Falso::com(|p| Ok(categoria_para_todos(p, "Casa", 0.9)));
    let resumo = rodar_fila(&banco, &falso);
    assert_eq!(falso.chamadas(), 3);
    let tamanhos: Vec<usize> = (0..3)
        .map(|i| itens_da_entrada(&falso.pedido(i).entrada).len())
        .collect();
    assert_eq!(tamanhos, [25, 25, 10]);
    assert_eq!(resumo.processados, 60);
}

#[test]
fn limite_pausa_com_o_que_ja_foi_gravado_e_retomar_termina() {
    let (banco, _) = banco_com_registros(30);
    let falso = Falso::em_fila(vec![
        Ok(json!({"itens": (1..=25).map(|n| json!({"n": n, "sugestoes": [s("categoria", json!("Casa"), 0.9)]})).collect::<Vec<_>>()})),
        Err(ErroDeIa::Limite { espera_segundos: 60 }),
    ]);

    let resumo = rodar_fila(&banco, &falso);

    assert_eq!(resumo.situacao, SituacaoDaRodada::Pausada);
    assert_eq!(resumo.processados, 25);
    assert!(resumo.aviso.as_deref().unwrap_or("").contains("60s"));
    let c = banco.conexao();
    assert_eq!(valor_texto(&c, "SELECT situacao FROM rodada_ia"), "pausada");

    // Retomar: a fila recalculada só tem os 5 que faltam.
    let falso2 = Falso::com(|p| Ok(categoria_para_todos(p, "Casa", 0.9)));
    let resumo = rodar_fila(&banco, &falso2);
    assert_eq!((resumo.total, resumo.situacao), (5, SituacaoDaRodada::Concluida));
}

#[test]
fn pedido_de_pausa_vale_entre_lotes() {
    let (banco, _) = banco_com_registros(30);
    let c = banco.conexao();
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    let controle = std::sync::Arc::new(Controle::default());
    let c2 = std::sync::Arc::clone(&controle);
    let falso = Falso::com(move |p| {
        c2.pedir_pausa();
        Ok(categoria_para_todos(p, "Casa", 0.9))
    });
    let progresso = Mutex::new(Progresso::ocioso());
    let resumo = rodar(&c, &falso, &fila.alvos, &ConfigDaRodada::default(), &controle, &progresso)
        .expect("rodar");
    assert_eq!(resumo.situacao, SituacaoDaRodada::Pausada);
    assert_eq!((falso.chamadas(), resumo.processados), (1, 25));
}

#[test]
fn erro_indisponivel_falha_a_rodada_e_erros_seguidos_tambem() {
    let (banco, _) = banco_com_registros(80);
    let falso = Falso::em_fila(vec![Err(ErroDeIa::Indisponivel("claude não instalado".into()))]);
    let r = rodar_fila(&banco, &falso);
    assert_eq!(r.situacao, SituacaoDaRodada::Falhou);
    assert!(r.aviso.unwrap_or_default().contains("claude não instalado"));

    let falso = Falso::com(|_| Err(ErroDeIa::Outro("rede caiu".into())));
    let r = rodar_fila(&banco, &falso);
    assert_eq!(r.situacao, SituacaoDaRodada::Falhou);
    assert_eq!(falso.chamadas(), 3, "três falhas seguidas encerram");
    assert_eq!(r.processados, 0, "nada foi marcado como feito: volta na próxima rodada");
}

#[test]
fn texto_longo_e_truncado_no_inicio_e_no_fim() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let texto = format!("INICIO-{} {} FIM-DO-DOCUMENTO", "a".repeat(5000), "b".repeat(5000));
    inserir_achado(&c, 1, "novo", Some(&texto), "longo.pdf");
    drop(c);
    let falso = Falso::com(|_| Ok(json!({"itens": [{"n": 1, "sugestoes": [s("e_financeiro", json!("nao"), 0.9)]}]})));
    rodar_fila(&banco, &falso);
    let entrada = falso.pedido(0).entrada;
    assert!(entrada.contains("INICIO-"));
    assert!(entrada.contains("FIM-DO-DOCUMENTO"));
    assert!(entrada.contains("caracteres omitidos"));
    assert!(entrada.len() < 5000, "{} bytes", entrada.len());
}

#[test]
fn sugestao_igual_ao_valor_atual_nao_aparece() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let r = inserir_registro(
        &c,
        &Reg {
            categoria: Some("Mercado"),
            ..compra("2026-10-05", -100, "COMPRA", None)
        },
    );
    drop(c);
    let falso = Falso::com(|_| {
        Ok(json!({"itens": [{"n": 1, "sugestoes": [
            s("categoria", json!("mercado"), 0.9), s("tipo", json!("debito"), 0.8)]}]}))
    });
    rodar_fila(&banco, &falso);
    let c = banco.conexao();
    let abertas = listar_abertas(&c, &FiltroDeSugestoes::default()).expect("l");
    assert_eq!(abertas.len(), 1, "só o tipo difere do que já está: {r}");
    assert_eq!(abertas[0].campo, "tipo");
}

#[test]
fn vocabulario_do_usuario_vai_no_cabecalho_do_lote() {
    let banco = banco_limpo();
    let c = banco.conexao();
    inserir_registro(&c, &Reg { categoria: Some("Mercado"), conta: Some("Banco Alfa"), ..compra("2026-09-01", -1, "A", Some("X")) });
    inserir_registro(&c, &compra("2026-10-05", -100, "SEM CATEGORIA", None));
    drop(c);
    let falso = Falso::com(|p| Ok(categoria_para_todos(p, "Casa", 0.9)));
    rodar_fila(&banco, &falso);
    let entrada = falso.pedido(0).entrada;
    assert!(entrada.starts_with("categorias já usadas: Mercado"));
    assert!(entrada.contains("contas já usadas: Banco Alfa"));
}
