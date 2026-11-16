//! A fila contra o provedor falso: IA-03, IA-04, IA-05 (lado do registro), IA-06, mais lotes,
//! escalonamento, cancelamento, pausa e progresso.

mod apoio;

use apoio::{Linhas, Memoria, esquema, resposta, resposta_com};
use sbf_ia::{
    ConfiguracaoDaFila, Controle, Degrau, ErroDeIa, Evento, Falso, Fila, Interrupcao, Provedor,
    Relatorio, Resposta, ResultadoDeIa,
};
use serde_json::json;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

fn itens(n: usize) -> Vec<String> {
    (0..n).map(|i| format!("documento ficticio {i}")).collect()
}

fn config(reserva: bool) -> ConfiguracaoDaFila {
    let mut c = ConfiguracaoDaFila::nova("triagem", "v1", "classifique", esquema());
    c.usar_reserva = reserva;
    c
}

type Esperas = Arc<Mutex<Vec<Duration>>>;

fn fila(
    principal: &Arc<Falso>,
    reserva: Option<&Arc<Falso>>,
    registro: &Arc<Memoria>,
    usar_reserva: bool,
) -> (Fila, Esperas) {
    let esperas: Esperas = Arc::default();
    let gravar = Arc::clone(&esperas);
    let p: Arc<dyn Provedor> = principal.clone();
    let r: Option<Arc<dyn Provedor>> = reserva.map(|r| {
        let r: Arc<dyn Provedor> = r.clone();
        r
    });
    let f = Fila::nova(p, r, registro.clone(), config(usar_reserva))
        .com_espera(move |d, _| {
            gravar.lock().expect("trava").push(d);
            true
        })
        .com_relogio(|| "2026-11-14T00:00:00Z".to_owned());
    (f, esperas)
}

/// Um falso que lê a entrada e dá confiança 0,4 a "dificil" no degrau rápido e 0,9 ao resto.
fn esperto() -> Arc<Falso> {
    Arc::new(Falso::com_funcao(|pedido| {
        let itens: Vec<_> = pedido
            .entrada
            .lines()
            .filter_map(|l| l.split_once('|'))
            .map(|(k, texto)| {
                let c = if texto.contains("dificil") && pedido.degrau == Degrau::Rapido {
                    0.4
                } else {
                    0.9
                };
                json!({"i": k.parse::<u64>().unwrap_or(0), "confianca": c})
            })
            .collect();
        Ok(resposta(json!({"itens": itens})))
    }))
}

fn rodar(fila: &Fila, itens: &[String]) -> Relatorio {
    fila.executar(itens, &Linhas, &Controle::novo(), None)
}

#[test]
fn lotes_de_ate_25_itens() {
    let p = esperto();
    let reg = Arc::new(Memoria::default());
    let (f, _) = fila(&p, None, &reg, false);
    let r = rodar(&f, &itens(60));
    let tamanhos: Vec<usize> = p
        .pedidos()
        .iter()
        .map(|x| x.entrada.lines().count())
        .collect();
    assert_eq!(tamanhos, vec![25, 25, 10]);
    assert_eq!(r.chamadas, 3);
    assert!(r.nao_classificados.is_empty() && r.nao_processados.is_empty());
    assert!(r.atribuicoes.iter().all(Option::is_some));
    // A posição aponta para o item certo dentro do lote.
    let (lote, pos) = r.resposta_do_item(27).expect("resposta");
    assert_eq!(lote.indices[pos], 27);
}

#[test]
fn ia03_resposta_invalida_tenta_uma_vez_e_depois_nao_classificado() {
    // Inválida e depois certa: uma retentativa, classificado.
    let p = Arc::new(Falso::com_roteiro(vec![
        Err(ErroDeIa::RespostaInvalida("x".into())),
        Ok(resposta_com(2, 0.9)),
    ]));
    let reg = Arc::new(Memoria::default());
    let (f, _) = fila(&p, None, &reg, false);
    let r = rodar(&f, &itens(2));
    assert_eq!(p.chamadas(), 2);
    assert!(r.nao_classificados.is_empty());

    // Inválida duas vezes: não classificado, e a fila não insiste uma terceira.
    let p = Arc::new(Falso::com_roteiro(vec![
        Err(ErroDeIa::RespostaInvalida("x".into())),
        Err(ErroDeIa::RespostaInvalida("x".into())),
    ]));
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let r = rodar(&f, &itens(2));
    assert_eq!(p.chamadas(), 2);
    assert_eq!(r.nao_classificados, vec![0, 1]);
    assert!(r.atribuicoes.iter().all(Option::is_none));
    assert!(r.interrupcao.is_none());
}

#[test]
fn ia03_item_que_faltou_na_resposta_volta_uma_vez() {
    // O lote tem 3 itens; a resposta traz só 0 e 2. O item 1 volta num lote próprio.
    let p = Arc::new(Falso::com_roteiro(vec![
        Ok(resposta(json!({"itens": [
            {"i": 0, "confianca": 0.9}, {"i": 2, "confianca": 0.9}
        ]}))),
        // Na volta, o item 1 vira o item 0 do novo lote, e falta de novo.
        Ok(resposta(json!({"itens": []}))),
    ]));
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let r = rodar(&f, &itens(3));
    assert_eq!(p.chamadas(), 2);
    assert_eq!(p.pedidos()[1].entrada.lines().count(), 1);
    assert_eq!(r.nao_classificados, vec![1]);
    assert!(r.atribuicoes[0].is_some() && r.atribuicoes[2].is_some());
}

#[test]
fn ia04_repetir_a_rodada_faz_zero_chamadas() {
    let p = esperto();
    let reg = Arc::new(Memoria::default());
    let lista = itens(30);
    let (f, _) = fila(&p, None, &reg, false);
    let primeira = rodar(&f, &lista);
    let chamadas = p.chamadas();
    assert!(chamadas >= 2);
    let saidas = reg.saidas.lock().expect("trava").len();

    let segunda = rodar(&f, &lista);
    assert_eq!(p.chamadas(), chamadas, "nenhuma chamada nova");
    assert_eq!(segunda.chamadas, 0);
    assert!(segunda.acertos_de_cache > 0);
    assert_eq!(reg.saidas.lock().expect("trava").len(), saidas, "nada saiu");
    assert_eq!(segunda.atribuicoes.iter().flatten().count(), 30);
    assert_eq!(primeira.confiancas, segunda.confiancas);
}

#[test]
fn ia04_o_cache_tambem_cobre_o_degrau_forte() {
    let p = esperto();
    let reg = Arc::new(Memoria::default());
    let lista = vec!["documento dificil".to_owned(), "documento facil".to_owned()];
    let (f, _) = fila(&p, None, &reg, false);
    rodar(&f, &lista);
    let antes = p.chamadas();
    assert_eq!(antes, 2, "rápido e depois forte");
    rodar(&f, &lista);
    assert_eq!(p.chamadas(), antes);
}

#[test]
fn ia05_a_saida_grava_tamanho_tokens_e_custo_sem_texto() {
    let p = esperto();
    let reg = Arc::new(Memoria::default());
    let lista = vec!["CPF 123.456.789-00 segredo".to_owned()];
    let (f, _) = fila(&p, None, &reg, false);
    rodar(&f, &lista);
    let saidas = reg.saidas.lock().expect("trava");
    assert_eq!(saidas.len(), 1);
    let s = &saidas[0];
    assert_eq!(s.destinatario, "triagem");
    assert_eq!(s.provedor, "falso");
    assert_eq!(s.modelo, "modelo-falso");
    assert!(s.bytes_enviados > 0);
    assert_eq!(s.consumo.tokens_entrada, 10);
    assert_eq!(s.quando, "2026-11-14T00:00:00Z");
    let texto = serde_json::to_string(&*saidas).expect("json");
    assert!(
        !texto.contains("123.456") && !texto.contains("segredo"),
        "{texto}"
    );
}

#[test]
fn escalonamento_so_do_que_ficou_abaixo_de_0_7() {
    let p = esperto();
    let reg = Arc::new(Memoria::default());
    let lista = vec![
        "a facil".to_owned(),
        "b dificil".to_owned(),
        "c facil".to_owned(),
    ];
    let (f, _) = fila(&p, None, &reg, false);
    let r = rodar(&f, &lista);
    let pedidos = p.pedidos();
    assert_eq!(pedidos.len(), 2);
    assert_eq!(pedidos[0].degrau, Degrau::Rapido);
    assert_eq!(pedidos[1].degrau, Degrau::Forte);
    assert_eq!(
        pedidos[1].entrada.lines().count(),
        1,
        "só o item difícil sobe"
    );
    assert_eq!(r.escalonados, 1);
    let (lote, pos) = r.resposta_do_item(1).expect("resposta");
    assert_eq!((lote.degrau, lote.indices[pos]), (Degrau::Forte, 1));
    assert_eq!(
        r.resposta_do_item(0).expect("resposta").0.degrau,
        Degrau::Rapido
    );
    assert_eq!(r.confiancas[1], Some(0.9));
}

#[test]
fn sem_escalonar_nao_sobe() {
    let p = esperto();
    let mut c = config(false);
    c.escalonar = false;
    let pr: Arc<dyn Provedor> = p.clone();
    let f = Fila::nova(pr, None, Arc::new(Memoria::default()), c);
    let r = rodar(&f, &["muito dificil".to_owned()]);
    assert_eq!(p.chamadas(), 1);
    assert_eq!(r.confiancas[0], Some(0.4));
}

#[test]
fn ia06_limite_pausa_espera_e_retoma() {
    let p = Arc::new(Falso::com_roteiro(vec![
        Err(ErroDeIa::Limite { espera_segundos: 7 }),
        Ok(resposta_com(2, 0.9)),
    ]));
    let (f, esperas) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let (tx, rx) = mpsc::channel();
    let r = f.executar(&itens(2), &Linhas, &Controle::novo(), Some(&tx));
    assert_eq!(
        *esperas.lock().expect("trava"),
        vec![Duration::from_secs(7)]
    );
    assert!(r.interrupcao.is_none());
    assert_eq!(r.atribuicoes.iter().flatten().count(), 2);
    assert_eq!(r.chamadas, 1, "a chamada barrada pelo limite não conta");
    let eventos: Vec<Evento> = rx.try_iter().collect();
    assert!(eventos.contains(&Evento::EsperandoLimite { espera_segundos: 7 }));
    assert!(matches!(
        eventos.last(),
        Some(Evento::Terminada { interrupcao: None })
    ));
}

#[test]
fn ia06_a_reserva_so_entra_se_ligada_e_abaixo_do_teto() {
    let limite = || {
        Err(ErroDeIa::Limite {
            espera_segundos: 3600,
        })
    };

    // Ligada e abaixo do teto: a reserva responde, sem esperar.
    let p = Arc::new(Falso::com_roteiro(vec![limite()]));
    let reserva = Arc::new(Falso::com_funcao(|_| Ok(resposta_com(2, 0.9))).chamado("reserva"));
    let reg = Arc::new(Memoria::default());
    let (f, esperas) = fila(&p, Some(&reserva), &reg, true);
    let r = rodar(&f, &itens(2));
    assert_eq!(reserva.chamadas(), 1);
    assert!(esperas.lock().expect("trava").is_empty());
    assert_eq!(r.lotes[0].provedor, "reserva");
    assert_eq!(reg.saidas.lock().expect("trava")[0].provedor, "reserva");

    // Ligada, mas no teto: espera o principal.
    let p = Arc::new(Falso::com_roteiro(vec![limite(), Ok(resposta_com(2, 0.9))]));
    let reserva = Arc::new(Falso::com_funcao(|_| Ok(resposta_com(2, 0.9))).chamado("reserva"));
    reserva.definir_dentro_do_teto(false);
    let (f, esperas) = fila(&p, Some(&reserva), &Arc::new(Memoria::default()), true);
    let r = rodar(&f, &itens(2));
    assert_eq!(reserva.chamadas(), 0);
    assert_eq!(esperas.lock().expect("trava").len(), 1);
    assert_eq!(r.lotes[0].provedor, "falso");

    // Desligada: espera o principal, mesmo com a reserva livre.
    let p = Arc::new(Falso::com_roteiro(vec![limite(), Ok(resposta_com(2, 0.9))]));
    let reserva = Arc::new(Falso::com_funcao(|_| Ok(resposta_com(2, 0.9))).chamado("reserva"));
    let (f, esperas) = fila(&p, Some(&reserva), &Arc::new(Memoria::default()), false);
    rodar(&f, &itens(2));
    assert_eq!(reserva.chamadas(), 0);
    assert_eq!(esperas.lock().expect("trava").len(), 1);
}

#[test]
fn ia06_o_teto_da_reserva_no_meio_volta_a_esperar_o_principal() {
    let p = Arc::new(Falso::com_roteiro(vec![
        Err(ErroDeIa::Limite {
            espera_segundos: 60,
        }),
        Err(ErroDeIa::Limite {
            espera_segundos: 60,
        }),
        Ok(resposta_com(2, 0.9)),
    ]));
    let reserva = Arc::new(Falso::com_roteiro(vec![Err(ErroDeIa::Teto)]).chamado("reserva"));
    let (f, esperas) = fila(&p, Some(&reserva), &Arc::new(Memoria::default()), true);
    let r = rodar(&f, &itens(2));
    assert!(r.interrupcao.is_none(), "{:?}", r.interrupcao);
    assert_eq!(reserva.chamadas(), 1, "a reserva só é tentada uma vez");
    assert_eq!(esperas.lock().expect("trava").len(), 1);
    assert_eq!(r.lotes[0].provedor, "falso");
}

#[test]
fn teto_do_principal_interrompe_a_rodada() {
    let p = Arc::new(Falso::com_roteiro(vec![Err(ErroDeIa::Teto)]));
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let r = rodar(&f, &itens(60));
    assert_eq!(r.interrupcao, Some(Interrupcao::Teto));
    assert_eq!(r.nao_processados.len(), 60);
    assert!(r.nao_classificados.is_empty());
}

#[test]
fn limite_que_nao_passa_desiste_depois_do_maximo() {
    let p = Arc::new(Falso::com_funcao(|_| {
        Err(ErroDeIa::Limite { espera_segundos: 1 })
    }));
    let reg = Arc::new(Memoria::default());
    let mut c = config(false);
    c.esperas_maximas_por_lote = 3;
    let pr: Arc<dyn Provedor> = p.clone();
    let f = Fila::nova(pr, None, reg, c).com_espera(|_, _| true);
    let r = rodar(&f, &itens(2));
    assert_eq!(r.interrupcao, Some(Interrupcao::LimiteSemFim));
    assert_eq!(r.nao_processados, vec![0, 1]);
}

#[test]
fn indisponivel_interrompe_e_outro_so_perde_o_lote() {
    let p = Arc::new(Falso::com_roteiro(vec![Err(ErroDeIa::Indisponivel(
        "sem claude".into(),
    ))]));
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let r = rodar(&f, &itens(2));
    assert!(matches!(r.interrupcao, Some(Interrupcao::Indisponivel(_))));

    let p = Arc::new(Falso::com_roteiro(vec![
        Err(ErroDeIa::Outro("falha".into())),
        Ok(resposta_com(5, 0.9)),
    ]));
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let r = rodar(&f, &itens(30));
    assert!(r.interrupcao.is_none());
    assert_eq!(r.nao_classificados.len(), 25);
    assert_eq!(r.atribuicoes.iter().flatten().count(), 5);
}

#[test]
fn cancelar_antes_de_comecar_nao_chama_ninguem() {
    let p = esperto();
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let controle = Controle::novo();
    controle.cancelar();
    let r = f.executar(&itens(10), &Linhas, &controle, None);
    assert_eq!(p.chamadas(), 0);
    assert_eq!(r.interrupcao, Some(Interrupcao::Cancelada));
    assert_eq!(r.nao_processados.len(), 10);
}

#[test]
fn cancelar_no_meio_guarda_o_que_ja_veio() {
    let controle = Controle::novo();
    let c2 = controle.clone();
    let p = Arc::new(Falso::com_funcao(move |pedido| {
        c2.cancelar(); // cancela durante a primeira chamada
        Ok(resposta_com(pedido.entrada.lines().count(), 0.9))
    }));
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let r = f.executar(&itens(60), &Linhas, &controle, None);
    assert_eq!(p.chamadas(), 1);
    assert_eq!(r.interrupcao, Some(Interrupcao::Cancelada));
    assert_eq!(r.atribuicoes.iter().flatten().count(), 25);
    assert_eq!(r.nao_processados.len(), 35);
}

#[test]
fn pausar_segura_a_fila_ate_retomar() {
    let p = esperto();
    let reg = Arc::new(Memoria::default());
    let (f, _) = fila(&p, None, &reg, false);
    let f = Arc::new(f);
    let controle = Controle::novo();
    controle.pausar();
    let (rx, fio) = f.iniciar(itens(40), Arc::new(Linhas), controle.clone());
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Evento::Iniciada {
            itens: 40,
            lotes: 2
        })
    );
    assert_eq!(rx.recv_timeout(Duration::from_secs(5)), Ok(Evento::Pausada));
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(p.chamadas(), 0, "pausada não chama");
    controle.retomar();
    let r = fio.join().expect("fio");
    assert_eq!(
        rx.recv_timeout(Duration::from_secs(5)),
        Ok(Evento::Retomada)
    );
    assert_eq!(r.atribuicoes.iter().flatten().count(), 40);
    let resto: Vec<Evento> = rx.try_iter().collect();
    let conclusoes = resto
        .iter()
        .filter(|e| matches!(e, Evento::LoteConcluido { .. }))
        .count();
    assert_eq!(conclusoes, 2);
}

#[test]
fn o_progresso_conta_itens_chamadas_e_consumo() {
    let p = esperto();
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    let (tx, rx) = mpsc::channel();
    f.executar(&itens(30), &Linhas, &Controle::novo(), Some(&tx));
    let ultimo = rx
        .try_iter()
        .filter_map(|e| match e {
            Evento::LoteConcluido {
                itens_feitos,
                itens_total,
                chamadas,
                consumo,
                ..
            } => Some((itens_feitos, itens_total, chamadas, consumo.tokens_entrada)),
            _ => None,
        })
        .last();
    assert_eq!(ultimo, Some((30, 30, 2, 20)));
}

#[test]
fn o_provedor_recebe_nova_rodada_a_cada_execucao() {
    let p = esperto();
    let (f, _) = fila(&p, None, &Arc::new(Memoria::default()), false);
    rodar(&f, &itens(1));
    rodar(&f, &itens(1));
    assert_eq!(p.rodadas(), 2);
}

#[allow(dead_code)]
fn _usa(_: ResultadoDeIa<Resposta>) {}
