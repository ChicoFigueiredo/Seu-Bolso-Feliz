//! O laço com coletor de nuvem e triador falsos, banco e pastas fictícios.

#![allow(clippy::unwrap_used)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rusqlite::Connection;
use sbf_worker::config::{Config, ConfigDaTriagem, PastaVigiada};
use sbf_worker::estado::{EstadoDoWorker, agora};
use sbf_worker::laco::{ColetorDeNuvem, Dados, ResumoDaNuvem, ResumoDaTriagem, Triador, Worker};
use sbf_worker::registro::Falha;

const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

struct Cenario {
    _d: tempfile::TempDir,
    dados: Dados,
    raiz: std::path::PathBuf,
}

fn cenario() -> Cenario {
    let d = tempfile::tempdir().unwrap();
    let cofre = d.path().join("cofre");
    let pasta = d.path().join("dados");
    let raiz = d.path().join("disco");
    for p in [&cofre, &pasta, &raiz] {
        std::fs::create_dir_all(p).unwrap();
    }
    let dados = Dados::novos(&cofre, &pasta);
    Connection::open(&dados.banco)
        .unwrap()
        .execute_batch(SQL_BASE)
        .unwrap();
    Cenario { _d: d, dados, raiz }
}

fn config(c: &Cenario) -> Config {
    Config {
        pastas: vec![PastaVigiada {
            fonte: "disco:teste".into(),
            raiz: c.raiz.clone(),
        }],
        reconciliar_minutos: 1,
        ..Config::default()
    }
}

fn origens(c: &Cenario) -> i64 {
    Connection::open(&c.dados.banco)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM origem", [], |l| l.get(0))
        .unwrap()
}

struct NuvemFalsa(Rc<Cell<u32>>, bool);
impl ColetorDeNuvem for NuvemFalsa {
    fn coletar(&mut self, _: &AtomicBool) -> Result<ResumoDaNuvem, Falha> {
        self.0.set(self.0.get() + 1);
        if self.1 {
            Err(Falha::Externo)
        } else {
            Ok(ResumoDaNuvem {
                listados: 4,
                baixados: 2,
            })
        }
    }
}

struct TriadorFalso(Rc<Cell<u32>>, usize);
impl Triador for TriadorFalso {
    fn ciclo(&mut self, lote: usize, _: &AtomicBool) -> Result<ResumoDaTriagem, Falha> {
        self.0.set(self.0.get() + 1);
        assert_eq!(lote, 25);
        Ok(ResumoDaTriagem {
            enviados: 3,
            sugestoes: 3,
            por_regra: 0,
            na_fila: self.1,
        })
    }
}

#[test]
fn reconciliacao_inicial_e_depois_o_vigia_indexam_e_o_estado_vai_para_o_arquivo() {
    let c = cenario();
    std::fs::write(c.raiz.join("boleto-um.pdf"), "boleto ficticio um").unwrap();
    let mut w = Worker::novo(c.dados.clone(), config(&c), None, None).unwrap();
    let nao = AtomicBool::new(false);
    // Primeiro passo: a reconciliação inicial acha o arquivo que já estava lá.
    w.passo(agora(), Duration::from_millis(10), &nao);
    assert_eq!(origens(&c), 1);
    assert!(w.estado.ultima_coleta.is_some());
    // Depois, um arquivo novo chega pelo vigia (antes da próxima reconciliação).
    std::fs::write(c.raiz.join("boleto-dois.pdf"), "boleto ficticio dois").unwrap();
    for _ in 0..40 {
        w.passo(agora(), Duration::from_millis(100), &nao);
        if origens(&c) == 2 {
            break;
        }
    }
    assert_eq!(origens(&c), 2, "o vigia indexou o arquivo novo");
    let lido = EstadoDoWorker::ler(&c.dados.estado()).expect("estado");
    assert!(lido.rodando(agora()));
    assert_eq!(lido.vigiando, 1);
    assert_eq!(
        lido.ultima_coleta.as_ref().map(|u| u.origem.as_str()),
        Some("pastas")
    );
    w.encerrar();
    assert!(
        !EstadoDoWorker::ler(&c.dados.estado())
            .unwrap()
            .rodando(agora()),
        "parado ao sair"
    );
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("iniciou") && log.contains("parou"), "{log}");
    assert!(
        !log.contains("boleto"),
        "nome de arquivo nunca vai ao log: {log}"
    );
    assert!(
        !log.contains(c.raiz.to_str().unwrap()),
        "nem caminho: {log}"
    );
}

#[test]
fn nuvem_so_roda_com_a_coleta_ligada_e_no_intervalo() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let cfg = Config {
        nuvem_minutos: 10,
        ..Config::default()
    };
    let mut w = Worker::novo(
        c.dados.clone(),
        cfg,
        Some(Box::new(NuvemFalsa(n.clone(), false))),
        None,
    )
    .unwrap();
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 0, "coleta desligada em Opções › Nuvens");
    std::fs::write(c.dados.nuvens(), r#"{"coleta_ligada": true}"#).unwrap();
    w.passo(t0 + 1, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1);
    assert!(w.estado.nuvem_ligada);
    assert_eq!(w.estado.ultima_coleta.as_ref().unwrap().origem, "nuvem");
    w.passo(t0 + 60, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "antes do intervalo");
    w.passo(t0 + 10 * 60 + 5, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2);
}

#[test]
fn falha_da_nuvem_conta_e_vai_ao_log_so_como_categoria() {
    let c = cenario();
    std::fs::write(c.dados.nuvens(), r#"{"coleta_ligada": true}"#).unwrap();
    let n = Rc::new(Cell::new(0));
    let mut w = Worker::novo(
        c.dados.clone(),
        Config::default(),
        Some(Box::new(NuvemFalsa(n.clone(), true))),
        None,
    )
    .unwrap();
    w.passo(agora(), Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(w.estado.falhas, 1);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("falhou passo=nuvem causa=externo"), "{log}");
}

#[test]
fn triagem_so_na_janela_de_horario_ligada_e_com_lote_configurado() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    // Hora local fixa: 12:00 em UTC-3 = 15:00 UTC.
    let meio_dia = agora() - agora() % 86_400 + 86_400 + 15 * 3600; // amanhã, 15:00 UTC
    let cfg = |ligada, ini, fim| Config {
        triagem: ConfigDaTriagem {
            ligada,
            hora_inicial: ini,
            hora_final: fim,
            ..ConfigDaTriagem::default()
        },
        ..Config::default()
    };
    let mut desligada = Worker::novo(
        c.dados.clone(),
        cfg(false, 0, 24),
        None,
        Some(Box::new(TriadorFalso(n.clone(), 5))),
    )
    .unwrap();
    desligada.passo(meio_dia, Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(n.get(), 0);
    drop(desligada);
    let mut fora = Worker::novo(
        c.dados.clone(),
        cfg(true, 22, 6),
        None,
        Some(Box::new(TriadorFalso(n.clone(), 5))),
    )
    .unwrap();
    fora.passo(meio_dia, Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(n.get(), 0, "meio-dia fora da janela da noite");
    drop(fora);
    let mut dentro = Worker::novo(
        c.dados.clone(),
        cfg(true, 9, 18),
        None,
        Some(Box::new(TriadorFalso(n.clone(), 5))),
    )
    .unwrap();
    dentro.passo(meio_dia, Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(n.get(), 1);
    assert_eq!(dentro.estado.fila_triagem, 5);
    assert!(dentro.estado.triagem_ligada);
    // Há fila: o próximo lote vem em 1 min, e não em 15.
    dentro.passo(
        meio_dia + 30,
        Duration::from_millis(1),
        &AtomicBool::new(false),
    );
    assert_eq!(n.get(), 1);
    dentro.passo(
        meio_dia + 61,
        Duration::from_millis(1),
        &AtomicBool::new(false),
    );
    assert_eq!(n.get(), 2);
}

#[test]
fn segunda_instancia_e_recusada_pela_trava() {
    let c = cenario();
    let _a = sbf_worker::trava::Trava::tomar(&c.dados.lock()).unwrap();
    assert!(matches!(
        sbf_worker::trava::Trava::tomar(&c.dados.lock()),
        Err(sbf_worker::Erro::JaRodando)
    ));
}

#[test]
fn sinal_de_parada_encerra_o_laco_e_marca_parado() {
    let c = cenario();
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None).unwrap();
    let parar = std::sync::Arc::new(AtomicBool::new(false));
    let p2 = parar.clone();
    let t = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(1500));
        p2.store(true, std::sync::atomic::Ordering::Relaxed);
    });
    let inicio = std::time::Instant::now();
    w.rodar(&parar);
    t.join().unwrap();
    assert!(inicio.elapsed() < Duration::from_secs(5));
    assert!(
        !EstadoDoWorker::ler(&c.dados.estado())
            .unwrap()
            .rodando(agora())
    );
}
