//! O agente contínuo (spec 09): regras de 3 confirmações, exemplos no prompt v2, calibração
//! e painel. Provedor falso; o `claude` real nunca é chamado.

mod ajuda;

use std::sync::{Arc, Mutex};

use ajuda::*;
use sbf_ia::{Controle, Falso, Fila, Registro};
use sbf_inventario::comando::{Acao, Comandos, PedidoDeDecisao};
use sbf_inventario::sugestao::RegistroDaIa;
use sbf_inventario::{Ref, Tipo};
use sbf_triagem::Alvo;
use sbf_triagem::agente::{Agente, MINIMO_PARA_REGRA, rodar_ciclo};
use sbf_triagem::rodada::{Progresso, configuracao_da_fila_v2};
use sbf_triagem::sugestoes::{EstadoDaSugestao, NovaSugestao, gravar};
use serde_json::json;

fn reg(id: i64) -> Ref {
    Ref::nova(Tipo::Registro, id.to_string())
}

fn decidir(banco: &Banco, id: i64, categoria: &str, motivo: Option<&str>) {
    banco
        .inventario()
        .executar(&PedidoDeDecisao {
            acao: Acao::CorrigirCategoria(categoria.into()),
            alvos: vec![reg(id)],
            motivo: motivo.map(str::to_owned),
        })
        .expect("decidir");
}

fn mercado(banco: &Banco, n: u32) -> i64 {
    inserir_registro(
        &banco.conexao(),
        &compra(
            &format!("2026-09-{n:02}"),
            -1000 - i64::from(n),
            &format!("MERCADO DELTA {n}"),
            Some("Mercado Delta"),
        ),
    )
}

fn agente() -> (tempfile::TempDir, Agente) {
    let d = tempfile::tempdir().expect("dir");
    let a = Agente::abrir(d.path().join("agente.sqlite")).expect("agente");
    (d, a)
}

fn recusar_sugestao(banco: &Banco, id: i64, campo: &str, valor: &str) {
    gravar(
        &banco.conexao(),
        &NovaSugestao {
            alvo: Alvo::Registro(id),
            campo: campo.to_owned(),
            valor: json!(valor),
            confianca: 0.9,
            justificativa: "teste".into(),
            modelo: "regra:categoria-aprendida".into(),
            prompt_versao: "regras".into(),
            rodada_id: None,
            estado: EstadoDaSugestao::Recusada,
        },
    )
    .expect("gravar");
}

#[test]
fn regra_nasce_com_tres_confirmacoes_e_nao_com_duas() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    let ids: Vec<i64> = (1..=3).map(|n| mercado(&banco, n)).collect();
    decidir(&banco, ids[0], "Mercado", None);
    decidir(&banco, ids[1], "Mercado", None);
    let c = banco.conexao();
    assert!(
        a.atualizar_regras(&c).expect("regras").is_empty(),
        "duas não bastam"
    );
    decidir(&banco, ids[2], "Mercado", None);
    let regras = a.atualizar_regras(&c).expect("regras");
    assert_eq!(MINIMO_PARA_REGRA, 3);
    assert_eq!(regras.len(), 1, "{regras:?}");
    assert_eq!(regras[0].valor, "Mercado");
    assert_eq!(regras[0].exemplos, 3);
    assert!(
        regras[0].quando.contains("mercado delta"),
        "{}",
        regras[0].quando
    );
    // E a fila a usa: um quarto registro do mesmo fornecedor resolve sem IA.
    let quarto = mercado(&banco, 4);
    let mapa = a.categorias_para_a_fila().expect("mapa");
    let fila = sbf_triagem::fila::selecionar(
        &c,
        &sbf_triagem::fila::OpcoesDaFila {
            regras_categoria: Some(mapa),
            ..sbf_triagem::fila::OpcoesDaFila::padrao()
        },
    )
    .expect("fila");
    assert_eq!(fila.regras.len(), 1);
    assert_eq!(fila.regras[0].alvo, Alvo::Registro(quarto));
}

#[test]
fn aceite_de_sugestao_da_ia_nao_ensina_regra() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    for n in 1..=3 {
        let id = mercado(&banco, n);
        decidir(&banco, id, "Mercado", Some("sugestão da IA"));
    }
    assert!(
        a.atualizar_regras(&banco.conexao())
            .expect("regras")
            .is_empty()
    );
}

#[test]
fn uma_recusa_apaga_a_regra() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    let ids: Vec<i64> = (1..=4).map(|n| mercado(&banco, n)).collect();
    for id in &ids[..3] {
        decidir(&banco, *id, "Mercado", None);
    }
    let c = banco.conexao();
    assert_eq!(a.atualizar_regras(&c).expect("regras").len(), 1);
    // A regra sugeriu Mercado para o quarto e você recusou.
    recusar_sugestao(&banco, ids[3], "categoria", "Mercado");
    assert!(a.atualizar_regras(&c).expect("regras").is_empty());
    assert!(a.categorias_para_a_fila().expect("mapa").is_empty());
}

#[test]
fn conflito_entre_decisoes_impede_a_regra() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    let ids: Vec<i64> = (1..=4).map(|n| mercado(&banco, n)).collect();
    for id in &ids[..3] {
        decidir(&banco, *id, "Mercado", None);
    }
    decidir(&banco, ids[3], "Padaria", None);
    assert!(
        a.atualizar_regras(&banco.conexao())
            .expect("regras")
            .is_empty()
    );
}

#[test]
fn regra_apagada_a_mao_so_volta_com_tres_confirmacoes_novas() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    let ids: Vec<i64> = (1..=6).map(|n| mercado(&banco, n)).collect();
    for id in &ids[..3] {
        decidir(&banco, *id, "Mercado", None);
    }
    let c = banco.conexao();
    let regras = a.atualizar_regras(&c).expect("regras");
    assert!(a.apagar_regra(regras[0].id).expect("apagar"));
    assert!(
        a.atualizar_regras(&c).expect("regras").is_empty(),
        "não ressuscita sozinha"
    );
    std::thread::sleep(std::time::Duration::from_millis(20));
    for id in &ids[3..5] {
        decidir(&banco, *id, "Mercado", None);
    }
    assert!(
        a.atualizar_regras(&c).expect("regras").is_empty(),
        "duas novas não bastam"
    );
    decidir(&banco, ids[5], "Mercado", None);
    assert_eq!(a.atualizar_regras(&c).expect("regras").len(), 1);
}

#[test]
fn regra_de_tipo_de_documento_pelo_nome_do_arquivo() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    let c = banco.conexao();
    let hs: Vec<String> = (1..=4)
        .map(|n| {
            inserir_achado(
                &c,
                n,
                "novo",
                Some("texto"),
                &format!("Fatura-Luz-2026-{n:02}.pdf"),
            )
        })
        .collect();
    for h in &hs[..3] {
        banco
            .inventario()
            .executar(&PedidoDeDecisao {
                acao: Acao::CorrigirTipo("fatura".into()),
                alvos: vec![Ref::nova(Tipo::Achado, h.clone())],
                motivo: None,
            })
            .expect("decidir");
    }
    let regras = a.atualizar_regras(&c).expect("regras");
    assert_eq!(regras.len(), 1, "{regras:?}");
    assert_eq!(regras[0].quando, "fatura luz #");
    let sug = a
        .sugestoes_de_tipo(&c, &[Alvo::Achado(hs[3].clone())])
        .expect("sugestoes");
    assert_eq!(sug.len(), 1);
    assert_eq!(sug[0].valor, "fatura");
}

#[test]
fn exemplos_vao_ate_oito_e_so_do_mesmo_fornecedor() {
    let banco = banco_limpo();
    for n in 1..=12 {
        let id = mercado(&banco, n);
        decidir(
            &banco,
            id,
            if n % 2 == 0 { "Mercado" } else { "Casa" },
            None,
        );
    }
    let outro = inserir_registro(
        &banco.conexao(),
        &compra("2026-09-20", -500, "PADARIA", Some("Padaria Beta")),
    );
    decidir(&banco, outro, "Padaria", None);
    let alvo = mercado(&banco, 13);
    let mapa = Agente::exemplos_para(&banco.conexao(), &[Alvo::Registro(alvo)]).expect("exemplos");
    let lista = &mapa[&format!("registro:{alvo}")];
    assert_eq!(lista.len(), 8, "{lista:?}");
    assert!(
        lista.iter().all(|l| l.contains("MERCADO DELTA")),
        "{lista:?}"
    );
    assert!(
        lista[0].contains("-> categoria Mercado"),
        "a mais recente primeiro: {lista:?}"
    );
}

fn motor(banco: &Banco, falso: &Arc<Falso>) -> (Arc<Fila>, Arc<RegistroDaIa>) {
    let registro = Arc::new(RegistroDaIa::abrir(&banco.caminho, None).expect("registro"));
    let fila = Arc::new(
        Fila::nova(
            Arc::clone(falso) as Arc<dyn sbf_ia::Provedor>,
            None,
            Arc::clone(&registro) as Arc<dyn Registro>,
            configuracao_da_fila_v2(false),
        )
        .com_espera(|_, _| true),
    );
    (fila, registro)
}

#[test]
fn ciclo_manda_o_prompt_v2_com_exemplos_em_lote_pequeno_e_so_sugere() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    // Três decisões suas sobre o Mercado Delta (viram regra) e duas sobre a Farmácia Gama
    // (viram só exemplos).
    for n in 1..=3 {
        let id = mercado(&banco, n);
        decidir(&banco, id, "Mercado", None);
    }
    for n in 1..=2 {
        let id = inserir_registro(
            &banco.conexao(),
            &compra(
                &format!("2026-09-{n:02}"),
                -900,
                &format!("FARMACIA GAMA {n}"),
                Some("Farmácia Gama"),
            ),
        );
        decidir(&banco, id, "Saúde", None);
    }
    // Chegam: um do Mercado (resolvido pela regra) e cinco da Farmácia (vão à IA, lote de 3).
    let mercado_novo = mercado(&banco, 10);
    let novos: Vec<i64> = (10..15)
        .map(|n| {
            inserir_registro(
                &banco.conexao(),
                &compra(
                    &format!("2026-10-{n:02}"),
                    -1900,
                    &format!("FARMACIA GAMA {n}"),
                    Some("Farmácia Gama"),
                ),
            )
        })
        .collect();
    // Na primeira rodada nada é "chegada": a base conta como inicial.
    let falso = Arc::new(Falso::com_funcao(|p| {
        resposta(categoria_para_todos(p, "Saúde", 0.9))
    }));
    let (fila, registro) = motor(&banco, &falso);
    let inv = banco.conexao();
    let progresso = Mutex::new(Progresso::ocioso());
    let resumo = rodar_ciclo(
        &inv,
        &a,
        &fila,
        "falso",
        3,
        &Controle::novo(),
        &progresso,
        &|id| registro.definir_rodada(Some(id)),
        None,
    )
    .expect("ciclo");
    assert_eq!(resumo.chegaram, 0, "a primeira vez é a base");
    assert_eq!(resumo.por_regra, 1, "o Mercado Delta saiu pela regra");
    assert_eq!(resumo.enviados, 3);
    assert_eq!(resumo.na_fila, 2, "o resto espera o próximo lote");
    assert_eq!(falso.chamadas(), 1);
    let pedido = &falso.pedidos()[0];
    assert_eq!(pedido.prompt_versao, "triagem-v2");
    assert!(pedido.instrucao.contains("EXEMPLOS DO USUÁRIO"));
    assert!(
        pedido.entrada.contains("decisões suas parecidas:"),
        "{}",
        pedido.entrada
    );
    assert!(pedido.entrada.contains("-> categoria Saúde"));
    assert!(
        !pedido.entrada.contains("MERCADO DELTA"),
        "só o fornecedor do item"
    );
    // Só sugeriu: nenhum registro foi decidido pelo agente.
    let decididos: i64 = contar(
        &inv,
        "SELECT COUNT(*) FROM registro WHERE decidido_por = 'usuario'",
    );
    assert_eq!(decididos, 5);
    assert_eq!(
        contar(
            &inv,
            &format!(
                "SELECT COUNT(*) FROM sugestao_ia WHERE alvo_id = '{mercado_novo}' AND modelo LIKE 'regra:%'"
            )
        ),
        1
    );
    // O segundo ciclo pega os 2 que faltam; chegaram vira 0 (nada novo) e as abertas não repetem.
    let r2 = rodar_ciclo(
        &inv,
        &a,
        &fila,
        "falso",
        3,
        &Controle::novo(),
        &progresso,
        &|id| registro.definir_rodada(Some(id)),
        None,
    )
    .expect("ciclo 2");
    assert_eq!((r2.enviados, r2.na_fila), (2, 0), "{r2:?}");
    assert_eq!(
        contar(
            &inv,
            "SELECT COUNT(DISTINCT alvo_id) FROM sugestao_ia WHERE modelo NOT LIKE 'regra:%' AND campo = 'categoria'"
        ),
        novos.len() as i64
    );
    // Chega um item novo depois da base: conta como chegada.
    let _ = mercado(&banco, 20);
    let r3 = rodar_ciclo(
        &inv,
        &a,
        &fila,
        "falso",
        3,
        &Controle::novo(),
        &progresso,
        &|id| registro.definir_rodada(Some(id)),
        None,
    )
    .expect("ciclo 3");
    assert_eq!(r3.chegaram, 1);
}

#[test]
fn calibracao_faixa_que_erra_mais_do_que_diz_vale_menos() {
    let banco = banco_limpo();
    let (_d, a) = agente();
    let c = banco.conexao();
    // 12 sugestões de 0,9: 4 aceitas e 8 recusadas.
    for i in 0..12 {
        let id = mercado(&banco, 1 + i);
        gravar(
            &c,
            &NovaSugestao {
                alvo: Alvo::Registro(id),
                campo: "categoria".into(),
                valor: json!("Mercado"),
                confianca: 0.9,
                justificativa: "teste".into(),
                modelo: "claude-cli:rapido".into(),
                prompt_versao: "triagem-v2".into(),
                rodada_id: None,
                estado: if i < 4 {
                    EstadoDaSugestao::Aceita
                } else {
                    EstadoDaSugestao::Recusada
                },
            },
        )
        .expect("gravar");
    }
    // Sem calibrar, vale o que a IA disse.
    assert!((a.confianca_calibrada(0.9).expect("c") - 0.9).abs() < 1e-9);
    let cal = a.atualizar_calibracao(&c).expect("calibrar");
    assert_eq!((cal[2].aceitas, cal[2].recusadas), (4, 8));
    let v = a.confianca_calibrada(0.9).expect("c");
    assert!((v - 4.0 / 12.0).abs() < 1e-9, "{v}");
    // Uma faixa sem amostra não muda nada.
    assert!((a.confianca_calibrada(0.99).expect("c") - 0.99).abs() < 1e-9);
    // As sugestões de regra não entram na calibração.
    assert_eq!(cal[0].aceitas + cal[0].recusadas, 0);
}
