//! O piloto de 200 itens (IA-09) e o serviço sobre o inventário.

mod ajuda;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ajuda::*;
use sbf_ia::{Controle, Falso};
use sbf_inventario::comando::{Acao, Comandos, PedidoDeDecisao};
use sbf_inventario::{Ref, Tipo};
use sbf_triagem::piloto::{FAIXA_ALTA, FAIXA_BAIXA, amostra, calcular, rodar_piloto};
use sbf_triagem::rodada::{Progresso, SituacaoDaRodada};
use sbf_triagem::servico::{RodadaDeTriagem, ServicoDeTriagem, TriagemDoInventario};
use sbf_triagem::sugestoes::{FiltroDeSugestoes, listar_abertas};
use serde_json::json;

const CATEGORIAS: [&str; 4] = ["Mercado", "Energia", "Saúde", "Transporte"];

/// 200 registros que o usuário já categorizou, mais 30 que ninguém tocou.
fn banco_do_piloto() -> (Banco, HashMap<i64, &'static str>) {
    let banco = banco_limpo();
    let c = banco.conexao();
    let mut verdade = HashMap::new();
    let mut ids = Vec::new();
    for i in 0..200 {
        let id = inserir_registro(
            &c,
            &compra("2026-10-05", -1000 - i, &format!("COMPRA {i}"), None),
        );
        let categoria = CATEGORIAS[(i % 4) as usize];
        verdade.insert(id, categoria);
        ids.push((id, categoria));
    }
    for i in 0..30 {
        inserir_registro(&c, &compra("2026-10-06", -9000 - i, "LIVRE", None));
    }
    drop(c);
    let mut inv = banco.inventario();
    for (id, categoria) in ids {
        inv.executar(&PedidoDeDecisao {
            acao: Acao::CorrigirCategoria(categoria.into()),
            alvos: vec![Ref::nova(Tipo::Registro, id.to_string())],
            motivo: None,
        })
        .expect("decidir");
    }
    (banco, verdade)
}

/// Acerta 4 de cada 5 itens; a confiança do acerto é alta e a do erro é baixa.
fn falso_que_acerta_80(verdade: HashMap<i64, &'static str>) -> Falso {
    Falso::com_funcao(move |p| {
        let itens: Vec<_> = itens_da_entrada(&p.entrada)
            .into_iter()
            .map(|(n, id)| {
                let id = id.expect("registro");
                let certa = verdade.get(&id).copied().unwrap_or("Mercado");
                let (valor, conf) = if id % 5 == 0 {
                    ("Outros", 0.9)
                } else {
                    (certa, 0.95)
                };
                json!({"n": n, "sugestoes": [s("categoria", json!(valor), conf)]})
            })
            .collect();
        resposta(json!({"itens": itens}))
    })
}

#[test]
fn ia09_o_piloto_mede_acerto_contra_as_decisoes_do_usuario_e_o_consumo() {
    let (banco, verdade) = banco_do_piloto();
    let m = montar(&banco, falso_que_acerta_80(verdade));
    let c = banco.conexao();
    assert_eq!(amostra(&c, 200).expect("amostra").len(), 200);
    let progresso = Mutex::new(Progresso::ocioso());

    let r = rodar_piloto(
        &c,
        &m.fila,
        "falso",
        200,
        &Controle::novo(),
        &progresso,
        &|id| m.registro.definir_rodada(Some(id)),
    )
    .expect("piloto");

    assert!(r.rodada.piloto);
    assert_eq!(r.itens, 200);
    assert_eq!(r.geral.comparadas, 200);
    assert_eq!(r.geral.acertos, 160, "80% de acerto");
    assert!((r.geral.taxa().expect("taxa") - 0.8).abs() < 1e-9);
    assert_eq!(r.por_campo["categoria"].comparadas, 200);
    // Calibração: o erro foi dito com 0.9 e o acerto com 0.95.
    assert_eq!(r.por_faixa[FAIXA_ALTA].comparadas, 200);
    assert!(!r.por_faixa.contains_key(FAIXA_BAIXA));
    // Consumo: 8 chamadas de 25, 150 dezmilésimos cada, tokens somados do `saida_ia`.
    assert_eq!(r.rodada.chamadas, 8);
    assert_eq!(r.tokens_entrada, 8 * 1000);
    assert_eq!(r.tokens_saida, 8 * 200);
    assert_eq!(r.custo_dezmilesimos_usd, 8 * 150);
    assert_eq!(r.tokens_por_item, Some((8000 + 1600) / 200));
    // Nada do piloto vira sugestão aberta nem decisão nova, e a IA não viu a resposta.
    assert!(
        listar_abertas(&c, &FiltroDeSugestoes::default())
            .expect("l")
            .is_empty()
    );
    assert_eq!(contar(&c, "SELECT COUNT(*) FROM decisao"), 200);
    assert!(m.pedido(0).entrada.contains("COMPRA"));
    assert!(!m.pedido(0).entrada.contains("categoria atual"));
    // Os 30 livres não entraram no piloto.
    assert_eq!(
        contar(&c, "SELECT COUNT(*) FROM sugestao_ia WHERE rodada_id = 1"),
        200
    );

    // Recalcular dá o mesmo.
    assert_eq!(calcular(&c, r.rodada.id).expect("calcular").geral, r.geral);
}

#[test]
fn servico_sobre_o_inventario_roda_lista_aceita_e_desfaz() {
    let (banco, verdade) = banco_do_piloto();
    let m = montar(&banco, falso_que_acerta_80(verdade));
    let mut servico = TriagemDoInventario::nova(
        &banco.caminho,
        Arc::clone(&m.fila),
        Arc::clone(&m.registro),
        Arc::clone(&m.falso) as Arc<dyn sbf_ia::Provedor>,
    )
    .expect("serviço");

    // A rodada normal só pega os 30 livres (os 200 já têm decisão do usuário).
    servico.iniciar(false).expect("iniciar");
    servico.esperar();
    let p = servico.progresso();
    assert_eq!(p.situacao, SituacaoDaRodada::Concluida);
    assert_eq!(p.total, 30);
    let grupos = servico
        .grupos(&FiltroDeSugestoes::default())
        .expect("grupos");
    let total: usize = grupos.iter().map(|g| g.sugestoes.len()).sum();
    assert!(total > 0 && total <= 30, "{total}");

    let ids: Vec<i64> = grupos.iter().flat_map(|g| g.ids()).collect();
    let mut inv = banco.inventario();
    let r = servico.aceitar(&ids, &mut inv).expect("aceitar");
    assert_eq!(r.decisoes.len(), r.aceitas.len());
    assert!(
        servico
            .grupos(&FiltroDeSugestoes::default())
            .expect("g")
            .is_empty()
    );
    if let Some(d) = r.decisoes.first() {
        inv.desfazer(*d).expect("desfazer");
    }

    servico
        .salvar_configuracao(Default::default())
        .expect("salvar");
    assert_eq!(servico.configuracao(), Default::default());
    assert!(servico.testar_conexao().expect("teste").contains("falso"));
}

#[test]
fn teste_e_salvamento_nao_bloqueiam_e_chegam_por_sondar() {
    use sbf_triagem::servico::Conclusao;
    let (banco, verdade) = banco_do_piloto();
    let m = montar(&banco, falso_que_acerta_80(verdade));
    let mut servico = TriagemDoInventario::nova(
        &banco.caminho,
        Arc::clone(&m.fila),
        Arc::clone(&m.registro),
        Arc::clone(&m.falso) as Arc<dyn sbf_ia::Provedor>,
    )
    .expect("serviço");

    let mut nova = servico.configuracao();
    nova.teto_do_mes_centavos_usd = 777;
    servico.iniciar_teste_de_conexao();
    servico.iniciar_salvamento(nova.clone());
    // A configuração já vale na memória, sem esperar a gravação.
    assert_eq!(servico.configuracao(), nova);

    let mut chegaram = Vec::new();
    for _ in 0..200 {
        chegaram.extend(servico.sondar());
        if chegaram.len() == 2 && !servico.ocupado() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    assert!(!servico.ocupado());
    assert!(
        chegaram
            .iter()
            .any(|c| matches!(c, Conclusao::Teste(Ok(t)) if t.contains("falso")))
    );
    assert!(chegaram.contains(&Conclusao::Configuracao(Ok(()))));
    // Gravou de verdade: a configuração salva é a que o próximo app abre.
    assert_eq!(
        sbf_triagem::servico::configuracao_salva(&banco.caminho),
        nova
    );
}
