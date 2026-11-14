//! A rodada sobre a fila do motor (provedor falso do `sbf-ia`): IA-03, IA-04, IA-06, dois
//! degraus, pausa e o texto longo.

mod ajuda;

use std::sync::Mutex;

use ajuda::*;
use sbf_ia::{Controle, Degrau, ErroDeIa, Falso};
use sbf_triagem::Alvo;
use sbf_triagem::fila::{OpcoesDaFila, selecionar};
use sbf_triagem::rodada::{ConfigDaRodada, Progresso, SituacaoDaRodada, rodar};
use sbf_triagem::sugestoes::{FiltroDeSugestoes, listar_abertas};
use serde_json::json;

fn rodar_fila(banco: &Banco, m: &Montagem) -> Progresso {
    let c = banco.conexao();
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    rodar_alvos(banco, m, &fila.alvos, &Controle::novo())
}

fn rodar_alvos(banco: &Banco, m: &Montagem, alvos: &[Alvo], controle: &Controle) -> Progresso {
    let c = banco.conexao();
    let progresso = Mutex::new(Progresso::ocioso());
    rodar(
        &c,
        &m.fila,
        "falso",
        alvos,
        &ConfigDaRodada::default(),
        controle,
        &progresso,
        &|id| m.registro.definir_rodada(Some(id)),
    )
    .expect("rodar")
}

fn banco_com_registros(n: i64) -> (Banco, Vec<i64>) {
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
    let m = montar(
        &banco,
        Falso::com_funcao(|_| {
            resposta(json!({"itens": [
                {"n": 1, "sugestoes": [
                    s("e_financeiro", json!("sim"), 0.98),
                    s("tipo_documento", json!("fatura"), 0.97),
                    s("valor", json!(18750), 0.97)]},
                {"n": 2, "sugestoes": [s("categoria", json!("Padaria"), 0.85)]},
            ]}))
        }),
    );

    let resumo = rodar_fila(&banco, &m);

    assert_eq!(resumo.situacao, SituacaoDaRodada::Concluida);
    assert_eq!(
        (resumo.total, resumo.processados, resumo.chamadas),
        (2, 2, 1)
    );
    assert_eq!(resumo.sugestoes, 4);
    // Nível completo: o CPF vai como está.
    let pedido = m.pedido(0);
    assert!(pedido.entrada.contains("CPF 123.456.789-09"));
    assert_eq!(pedido.prompt_versao, "triagem-v1");
    assert_eq!(pedido.degrau, Degrau::Rapido);
    assert!(pedido.instrucao.contains("140 caracteres"));
    let c = banco.conexao();
    let abertas = listar_abertas(&c, &FiltroDeSugestoes::default()).expect("listar");
    assert_eq!(abertas.len(), 4);
    assert!(
        abertas
            .iter()
            .any(|s| s.alvo == Alvo::Achado(h.clone()) && s.campo == "valor")
    );
    assert!(
        abertas
            .iter()
            .any(|s| s.alvo == Alvo::Registro(r) && s.campo == "categoria")
    );
    // Nada virou decisão.
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM decisao"), 0);
    assert_eq!(
        valor_texto(
            &c,
            "SELECT situacao || ':' || itens || ':' || chamadas FROM rodada_ia"
        ),
        "concluida:2:1"
    );
    // IA-05: o motor registrou a saída, marcada com a rodada, sem o texto.
    assert_eq!(
        valor_texto(
            &c,
            "SELECT destinatario||'|'||nivel||'|'||rodada_id FROM saida_ia"
        ),
        "triagem|completo|1"
    );
}

#[test]
fn ia03_item_invalido_volta_uma_vez_e_depois_vira_nao_classificado() {
    let (banco, ids) = banco_com_registros(2);
    // 1ª chamada: só o item 1 vale. 2ª (só o item que faltou): continua inválido.
    let falso = Falso::com_roteiro(vec![
        resposta(json!({"itens": [{"n": 1, "sugestoes": [s("categoria", json!("Casa"), 0.9)]}]})),
        resposta(json!({"itens": []})),
    ]);
    let m = montar(&banco, falso);

    let resumo = rodar_fila(&banco, &m);

    assert_eq!(m.chamadas(), 2, "uma retentativa só");
    let reenvio = m.pedido(1).entrada;
    assert_eq!(
        reenvio.matches("=== ITEM").count(),
        1,
        "o reenvio leva só o inválido"
    );
    assert!(reenvio.contains(&format!("registro {}", ids[1])));
    assert_eq!(resumo.nao_classificados, 1);
    let c = banco.conexao();
    assert_eq!(
        contar(
            &c,
            &format!(
                "SELECT COUNT(*) FROM sugestao_ia WHERE campo = 'nao_classificado' AND alvo_id = '{}'",
                ids[1]
            )
        ),
        1
    );
    // Não aparece na lista e não volta à fila.
    assert_eq!(
        listar_abertas(&c, &FiltroDeSugestoes::default())
            .expect("l")
            .len(),
        1
    );
    assert!(
        selecionar(&c, &OpcoesDaFila::padrao())
            .expect("fila")
            .alvos
            .is_empty()
    );
}

#[test]
fn ia03_retentativa_que_acerta_conta_como_sugestao() {
    let (banco, _) = banco_com_registros(1);
    let m = montar(
        &banco,
        Falso::com_roteiro(vec![
            Err(ErroDeIa::RespostaInvalida("não é JSON".into())),
            resposta(
                json!({"itens": [{"n": 1, "sugestoes": [s("categoria", json!("Casa"), 0.9)]}]}),
            ),
        ]),
    );
    let resumo = rodar_fila(&banco, &m);
    assert_eq!(
        (resumo.sugestoes, resumo.nao_classificados, resumo.chamadas),
        (1, 0, 2)
    );
}

#[test]
fn item_que_o_motor_aceitou_mas_o_tipo_nao_comporta_vira_nao_classificado() {
    // Achado respondendo só `categoria` (campo de registro): o motor vê o campo-chave, a
    // validação por tipo derruba o item.
    let banco = banco_limpo();
    let c = banco.conexao();
    inserir_achado(&c, 1, "novo", Some("texto"), "a.pdf");
    drop(c);
    let m = montar(
        &banco,
        Falso::com_funcao(|_| {
            resposta(
                json!({"itens": [{"n": 1, "sugestoes": [s("categoria", json!("Casa"), 0.9)]}]}),
            )
        }),
    );
    let resumo = rodar_fila(&banco, &m);
    assert_eq!((resumo.sugestoes, resumo.nao_classificados), (0, 1));
}

#[test]
fn dois_degraus_so_o_incerto_sobe_e_nunca_ha_terceiro() {
    let (banco, ids) = banco_com_registros(3);
    let m = montar(
        &banco,
        Falso::com_roteiro(vec![
            // Rápido: item 2 incerto.
            resposta(json!({"itens": [
                {"n": 1, "sugestoes": [s("categoria", json!("Casa"), 0.95)]},
                {"n": 2, "sugestoes": [s("categoria", json!("Talvez"), 0.4)]},
                {"n": 3, "sugestoes": [s("categoria", json!("Lazer"), 0.7)]},
            ]})),
            // Forte, só o item 2 (agora n=1 no reenvio).
            resposta(
                json!({"itens": [{"n": 1, "sugestoes": [s("categoria", json!("Saúde"), 0.6)]}]}),
            ),
        ]),
    );

    let resumo = rodar_fila(&banco, &m);

    assert_eq!(m.chamadas(), 2, "o forte não sobe de novo mesmo com 0.6");
    assert_eq!(m.pedido(0).degrau, Degrau::Rapido);
    assert_eq!(m.pedido(1).degrau, Degrau::Forte);
    assert!(
        m.pedido(1)
            .entrada
            .contains(&format!("registro {}", ids[1]))
    );
    assert_eq!(resumo.subiram_de_degrau, 1);
    let c = banco.conexao();
    assert_eq!(
        valor_texto(
            &c,
            &format!(
                "SELECT valor_json || ':' || modelo FROM sugestao_ia WHERE alvo_id = '{}'",
                ids[1]
            )
        ),
        "\"Saúde\":falso:forte"
    );
}

#[test]
fn ia04_repetir_a_rodada_nao_chama_de_novo() {
    let (banco, _) = banco_com_registros(4);
    let m = montar(
        &banco,
        Falso::com_funcao(|p| resposta(categoria_para_todos(p, "Casa", 0.9))),
    );
    let c = banco.conexao();
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    rodar_alvos(&banco, &m, &fila.alvos, &Controle::novo());
    assert_eq!(m.chamadas(), 1);

    // Pela fila: o que já tem sugestão nem entra.
    let de_novo = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    assert!(de_novo.alvos.is_empty());
    assert_eq!(de_novo.contagens.ja_sugeridos, 4);

    // Pelo cache do motor: os mesmos alvos, o mesmo texto, zero chamadas.
    let segunda = rodar_alvos(&banco, &m, &fila.alvos, &Controle::novo());
    assert_eq!(m.chamadas(), 1, "cache por conteúdo");
    assert_eq!(segunda.chamadas, 0);
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM cache_ia"), 1);
}

#[test]
fn lotes_de_25_itens() {
    let (banco, _) = banco_com_registros(60);
    let m = montar(
        &banco,
        Falso::com_funcao(|p| resposta(categoria_para_todos(p, "Casa", 0.9))),
    );
    let resumo = rodar_fila(&banco, &m);
    assert_eq!(m.chamadas(), 3);
    let tamanhos: Vec<usize> = (0..3)
        .map(|i| itens_da_entrada(&m.pedido(i).entrada).len())
        .collect();
    assert_eq!(tamanhos, [25, 25, 10]);
    assert_eq!(resumo.processados, 60);
}

#[test]
fn ia06_limite_espera_e_continua_sozinho() {
    let (banco, _) = banco_com_registros(30);
    let todos: Vec<_> = (1..=25)
        .map(|n| json!({"n": n, "sugestoes": [s("categoria", json!("Casa"), 0.9)]}))
        .collect();
    let falso = Falso::com_funcao(|p| resposta(categoria_para_todos(p, "Casa", 0.9)));
    falso.empilhar(resposta(json!({"itens": todos})));
    falso.empilhar(Err(ErroDeIa::Limite {
        espera_segundos: 60,
    }));
    let m = montar(&banco, falso);

    let resumo = rodar_fila(&banco, &m);

    assert_eq!(resumo.situacao, SituacaoDaRodada::Concluida);
    assert_eq!(resumo.processados, 30);
    assert_eq!(
        m.chamadas(),
        3,
        "a chamada que bateu no limite foi repetida"
    );
}

#[test]
fn pausa_do_motor_segura_a_rodada_ate_retomar() {
    let (banco, _) = banco_com_registros(30);
    let m = montar(
        &banco,
        Falso::com_funcao(|p| resposta(categoria_para_todos(p, "Casa", 0.9))),
    );
    let c = banco.conexao();
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    let controle = Controle::novo();
    controle.pausar();
    let progresso = Mutex::new(Progresso::ocioso());
    std::thread::scope(|escopo| {
        let fio = escopo.spawn(|| {
            let c = banco.conexao();
            rodar(
                &c,
                &m.fila,
                "falso",
                &fila.alvos,
                &ConfigDaRodada::default(),
                &controle,
                &progresso,
                &|_| {},
            )
            .expect("rodar")
        });
        let inicio = std::time::Instant::now();
        while progresso.lock().expect("p").situacao != SituacaoDaRodada::Pausada {
            assert!(inicio.elapsed().as_secs() < 5, "a pausa não apareceu");
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert_eq!(m.chamadas(), 0, "pausada antes de qualquer chamada");
        controle.retomar();
        let resumo = fio.join().expect("fio");
        assert_eq!(resumo.situacao, SituacaoDaRodada::Concluida);
        assert_eq!(resumo.processados, 30);
    });
}

#[test]
fn teto_pausa_a_rodada_e_indisponivel_a_falha() {
    let (banco, _) = banco_com_registros(30);
    let m = montar(&banco, Falso::com_roteiro(vec![Err(ErroDeIa::Teto)]));
    let r = rodar_fila(&banco, &m);
    assert_eq!(r.situacao, SituacaoDaRodada::Pausada);
    assert!(r.aviso.unwrap_or_default().contains("teto"));
    assert_eq!(
        valor_texto(&banco.conexao(), "SELECT situacao FROM rodada_ia"),
        "pausada"
    );

    let m = montar(
        &banco,
        Falso::com_roteiro(vec![Err(ErroDeIa::Indisponivel(
            "claude não instalado".into(),
        ))]),
    );
    let r = rodar_fila(&banco, &m);
    assert_eq!(r.situacao, SituacaoDaRodada::Falhou);
    assert!(r.aviso.unwrap_or_default().contains("claude não instalado"));
    assert_eq!(
        r.processados, 0,
        "nada foi marcado como feito: volta na próxima rodada"
    );
}

#[test]
fn texto_longo_e_truncado_no_inicio_e_no_fim() {
    let banco = banco_limpo();
    let c = banco.conexao();
    let texto = format!(
        "INICIO-{} {} FIM-DO-DOCUMENTO",
        "a".repeat(5000),
        "b".repeat(5000)
    );
    inserir_achado(&c, 1, "novo", Some(&texto), "longo.pdf");
    drop(c);
    let m = montar(
        &banco,
        Falso::com_funcao(|_| {
            resposta(
                json!({"itens": [{"n": 1, "sugestoes": [s("e_financeiro", json!("nao"), 0.9)]}]}),
            )
        }),
    );
    rodar_fila(&banco, &m);
    let entrada = m.pedido(0).entrada;
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
    let m = montar(
        &banco,
        Falso::com_funcao(|_| {
            resposta(json!({"itens": [{"n": 1, "sugestoes": [
                s("categoria", json!("mercado"), 0.9), s("tipo", json!("debito"), 0.8)]}]}))
        }),
    );
    rodar_fila(&banco, &m);
    let c = banco.conexao();
    let abertas = listar_abertas(&c, &FiltroDeSugestoes::default()).expect("l");
    assert_eq!(abertas.len(), 1, "só o tipo difere do que já está: {r}");
    assert_eq!(abertas[0].campo, "tipo");
}

#[test]
fn vocabulario_do_usuario_vai_no_cabecalho_do_lote() {
    let banco = banco_limpo();
    let c = banco.conexao();
    inserir_registro(
        &c,
        &Reg {
            categoria: Some("Mercado"),
            conta: Some("Banco Alfa"),
            ..compra("2026-09-01", -1, "A", Some("X"))
        },
    );
    inserir_registro(&c, &compra("2026-10-05", -100, "SEM CATEGORIA", None));
    drop(c);
    let m = montar(
        &banco,
        Falso::com_funcao(|p| resposta(categoria_para_todos(p, "Casa", 0.9))),
    );
    rodar_fila(&banco, &m);
    let entrada = m.pedido(0).entrada;
    assert!(entrada.starts_with("categorias já usadas: Mercado"));
    assert!(entrada.contains("contas já usadas: Banco Alfa"));
}

#[test]
fn falha_transitoria_do_provedor_adia_os_itens_e_resposta_invalida_repetida_nao_classifica() {
    use sbf_triagem::rodada::RastroDeFalhas;
    let (banco, ids) = banco_com_registros(3);
    let c = banco.conexao();
    let rastro = RastroDeFalhas::default();
    let progresso = Mutex::new(Progresso::ocioso());
    let config = ConfigDaRodada {
        rastro: Some(rastro.clone()),
        ..ConfigDaRodada::default()
    };

    // 1) Erro transitório: nada é gravado e os itens voltam à fila.
    let falso = std::sync::Arc::new(Falso::com_funcao(|_| {
        Err(ErroDeIa::Outro("rede caiu".into()))
    }));
    let m = montar_com(&banco, rastro.embrulhar(falso.clone()));
    let fila = selecionar(&c, &OpcoesDaFila::padrao()).expect("fila");
    let r = rodar(
        &c,
        &m.fila,
        "falso",
        &fila.alvos,
        &config,
        &Controle::novo(),
        &progresso,
        &|_| {},
    )
    .expect("rodar");
    assert_eq!(r.nao_classificados, 0);
    assert_eq!(r.processados, 0);
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM sugestao_ia"), 0);
    assert_eq!(
        selecionar(&c, &OpcoesDaFila::padrao())
            .expect("fila")
            .alvos
            .len(),
        3
    );

    // 2) Resposta inválida duas vezes: "não classificado" de verdade.
    let falso = std::sync::Arc::new(Falso::com_funcao(|_| {
        Err(ErroDeIa::RespostaInvalida("lixo".into()))
    }));
    let m = montar_com(&banco, rastro.embrulhar(falso));
    let r = rodar(
        &c,
        &m.fila,
        "falso",
        &fila.alvos,
        &config,
        &Controle::novo(),
        &progresso,
        &|_| {},
    )
    .expect("rodar");
    assert_eq!(r.nao_classificados, 3);
    assert_eq!(
        contar(
            &c,
            "SELECT COUNT(*) FROM sugestao_ia WHERE campo = 'nao_classificado'"
        ),
        3
    );
    assert!(
        selecionar(&c, &OpcoesDaFila::padrao())
            .expect("fila")
            .alvos
            .is_empty()
    );
    let _ = ids;
}

#[test]
fn usuario_que_decidiu_durante_a_rodada_nao_recebe_sugestao_naquele_campo() {
    let (banco, ids) = banco_com_registros(2);
    let caminho = banco.caminho.clone();
    let (decidido, promovido) = (ids[0], ids[1]);
    let m = montar(
        &banco,
        Falso::com_funcao(move |_| {
            // Enquanto a IA pensa, o usuário decide a categoria de um registro e o outro é
            // promovido.
            let c = rusqlite::Connection::open(&caminho).expect("abrir");
            c.execute(
                "INSERT INTO decisao (alvo_tipo, alvo_id, campo, antes, depois, autor)
                 VALUES ('registro', ?1, 'categoria', '{}', '{\"categoria_provavel\":\"Feita\"}', 'usuario')",
                [decidido.to_string()],
            )
            .expect("decisao");
            c.execute(
                "UPDATE registro SET estado = 'promovido' WHERE id = ?1",
                [promovido],
            )
            .expect("promover");
            resposta(json!({"itens": [
                {"n": 1, "sugestoes": [s("categoria", json!("Mercado"), 0.9)]},
                {"n": 2, "sugestoes": [s("categoria", json!("Mercado"), 0.9)]},
            ]}))
        }),
    );
    rodar_fila(&banco, &m);
    let c = banco.conexao();
    assert_eq!(
        contar(
            &c,
            "SELECT COUNT(*) FROM sugestao_ia WHERE campo = 'categoria' AND estado = 'aberta'"
        ),
        0,
        "nada sugerido a quem já decidiu ou já foi promovido"
    );
}
