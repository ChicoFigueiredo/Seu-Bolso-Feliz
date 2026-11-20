//! O laço com coletor de nuvem e triador falsos, banco e pastas fictícios.

#![allow(clippy::unwrap_used)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use sbf_worker::config::{Config, ConfigDaTriagem, PastaVigiada};
use sbf_worker::estado::{EstadoDoWorker, agora};
use sbf_worker::laco::{
    Analista, ColetorDeGmail, ColetorDeMensageiros, ColetorDeNuvem, ColetorDeTelegram, Dados,
    Indexador, LeitorDeImagens, ResumoDaNuvem, ResumoDaTriagem, ResumoDeMensageiros, ResumoDoGmail,
    ResumoDoIndice, ResumoDoOcr, ResumoDoTelegram, ResumoDosPontos, Triador, Worker,
};
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

struct MensageirosFalsos(Rc<Cell<u32>>);
impl ColetorDeMensageiros for MensageirosFalsos {
    fn coletar(&mut self, _: &AtomicBool) -> Result<ResumoDeMensageiros, Falha> {
        self.0.set(self.0.get() + 1);
        Ok(ResumoDeMensageiros {
            mensagens: 5,
            novos: 3,
        })
    }
}

#[test]
fn mensageiros_so_rodam_ligados_e_no_intervalo_do_arquivo() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_mensageiros(Box::new(MensageirosFalsos(n.clone())));
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 0, "sem mensageiros.json: desligado");
    std::fs::write(
        c.dados.mensageiros(),
        r#"{"ligado": false, "intervalo_minutos": 5}"#,
    )
    .unwrap();
    w.passo(t0 + 1, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 0);
    std::fs::write(
        c.dados.mensageiros(),
        r#"{"ligado": true, "intervalo_minutos": 5}"#,
    )
    .unwrap();
    w.passo(t0 + 2, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1);
    assert!(w.estado.mensageiros_ligados);
    assert_eq!(
        w.estado.ultima_coleta.as_ref().unwrap().origem,
        "mensageiros"
    );
    w.passo(t0 + 60, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "antes do intervalo");
    w.passo(t0 + 5 * 60 + 5, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("mensageiros mensagens=5 novos=3"), "{log}");
}

struct GmailFalso(Rc<Cell<u32>>, bool);
impl ColetorDeGmail for GmailFalso {
    fn coletar(&mut self, _: &AtomicBool) -> Result<ResumoDoGmail, Falha> {
        self.0.set(self.0.get() + 1);
        if self.1 {
            Err(Falha::Externo)
        } else {
            Ok(ResumoDoGmail {
                mensagens: 4,
                novos: 2,
            })
        }
    }
}

#[test]
fn gmail_roda_a_cada_gmail_minutos_padrao_30_e_zero_desliga() {
    let c = cenario();
    assert_eq!(Config::default().gmail_minutos, 30);
    let n = Rc::new(Cell::new(0));
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_gmail(Box::new(GmailFalso(n.clone(), false)));
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "a primeira coleta é logo");
    assert_eq!(w.estado.ultima_coleta.as_ref().unwrap().origem, "gmail");
    assert_eq!(w.estado.ultima_coleta.as_ref().unwrap().novos, 2);
    w.passo(t0 + 29 * 60, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "antes do intervalo");
    w.passo(t0 + 30 * 60 + 1, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("gmail mensagens=4 novos=2"), "{log}");

    // gmail_minutos = 0 desliga; outro valor muda o intervalo.
    let n = Rc::new(Cell::new(0));
    let desligado = Config {
        gmail_minutos: 0,
        ..Config::default()
    };
    let mut w = Worker::novo(c.dados.clone(), desligado, None, None)
        .unwrap()
        .com_gmail(Box::new(GmailFalso(n.clone(), false)));
    w.passo(t0, Duration::from_millis(1), &nao);
    w.passo(t0 + 3600, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 0);
    let curto = Config {
        gmail_minutos: 5,
        ..Config::default()
    };
    let mut w = Worker::novo(c.dados.clone(), curto, None, None)
        .unwrap()
        .com_gmail(Box::new(GmailFalso(n.clone(), false)));
    w.passo(t0, Duration::from_millis(1), &nao);
    w.passo(t0 + 5 * 60 + 1, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2);
}

#[test]
fn falha_do_gmail_vira_falha_no_estado_sem_derrubar_o_laco() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_gmail(Box::new(GmailFalso(n.clone(), true)));
    w.passo(agora(), Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(n.get(), 1);
    assert_eq!(w.estado.falhas, 1);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("gmail"), "{log}");
}

struct RoteirosFalsos(Rc<Cell<u32>>);
impl sbf_worker::laco::ColetorDeRoteiros for RoteirosFalsos {
    fn executar_devidos(
        &mut self,
        _: u64,
        _: &AtomicBool,
    ) -> Result<sbf_worker::laco::ResumoDeRoteiros, Falha> {
        self.0.set(self.0.get() + 1);
        Ok(sbf_worker::laco::ResumoDeRoteiros {
            executados: 2,
            baixados: 3,
            novos: 1,
        })
    }
}

#[test]
fn roteiros_so_rodam_com_o_banco_e_de_cinco_em_cinco_minutos() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_roteiros(Box::new(RoteirosFalsos(n.clone())));
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 0, "sem roteiros.sqlite: nada a fazer");
    std::fs::write(c.dados.pasta.join("roteiros.sqlite"), b"").unwrap();
    w.passo(t0 + 1, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1);
    assert_eq!(w.estado.ultima_coleta.as_ref().unwrap().origem, "roteiros");
    w.passo(t0 + 60, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "antes dos 5 minutos");
    w.passo(t0 + 5 * 60 + 5, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("roteiros executados=2 baixados=3"), "{log}");
}

// -- índice local e pontos de atenção (spec 10) ----------------------------------------------

struct IndexadorFalso(Rc<Cell<u32>>, bool);
impl Indexador for IndexadorFalso {
    fn atualizar(&mut self, _: &AtomicBool) -> Result<Option<ResumoDoIndice>, Falha> {
        self.0.set(self.0.get() + 1);
        if self.1 {
            return Ok(None);
        }
        Ok(Some(ResumoDoIndice {
            embutidos: 3,
            mantidos: 7,
            removidos: 1,
        }))
    }
}

struct AnalistaFalso(Rc<Cell<u32>>);
impl Analista for AnalistaFalso {
    fn analisar(&mut self) -> Result<ResumoDosPontos, Falha> {
        self.0.set(self.0.get() + 1);
        Ok(ResumoDosPontos {
            novos: 2,
            reabertos: 0,
            fechados: 1,
        })
    }
}

#[test]
fn indice_de_meia_em_meia_hora_e_pontos_de_seis_em_seis_horas() {
    let c = cenario();
    let (i, p) = (Rc::new(Cell::new(0)), Rc::new(Cell::new(0)));
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_indice(Box::new(IndexadorFalso(i.clone(), false)))
        .com_pontos(Box::new(AnalistaFalso(p.clone())));
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!((i.get(), p.get()), (1, 1), "a primeira volta já roda");
    w.passo(t0 + 60, Duration::from_millis(1), &nao);
    assert_eq!((i.get(), p.get()), (1, 1));
    w.passo(t0 + 31 * 60, Duration::from_millis(1), &nao);
    assert_eq!((i.get(), p.get()), (2, 1));
    w.passo(t0 + 6 * 3600 + 5, Duration::from_millis(1), &nao);
    assert_eq!((i.get(), p.get()), (3, 2));
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(
        log.contains("indice embutidos=3 mantidos=7 removidos=1"),
        "{log}"
    );
    assert!(
        log.contains("pontos novos=2 reabertos=0 fechados=1"),
        "{log}"
    );
}

#[test]
fn indice_sem_modelo_baixado_nao_e_falha() {
    let c = cenario();
    let i = Rc::new(Cell::new(0));
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_indice(Box::new(IndexadorFalso(i.clone(), true)));
    w.passo(agora(), Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(i.get(), 1);
    assert_eq!(w.estado.falhas, 0);
}

// -- OCR das imagens novas ---------------------------------------------------------------------

/// Devolve os resumos na ordem, e o último para sempre.
struct LeitorFalso(Rc<Cell<u32>>, Vec<ResumoDoOcr>);
impl LeitorDeImagens for LeitorFalso {
    fn ler_novas(&mut self, _: &AtomicBool) -> Result<ResumoDoOcr, Falha> {
        let n = self.0.get() as usize;
        self.0.set(self.0.get() + 1);
        Ok(self.1[n.min(self.1.len() - 1)])
    }
}

struct TelegramFalso(Rc<Cell<u32>>);
impl ColetorDeTelegram for TelegramFalso {
    fn coletar(&mut self, _: &AtomicBool) -> Result<ResumoDoTelegram, Falha> {
        self.0.set(self.0.get() + 1);
        Ok(ResumoDoTelegram {
            conversas: 2,
            novos: 4,
        })
    }
}

#[test]
fn ocr_roda_a_cada_minuto_e_logo_de_novo_se_sobrou_imagem() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let resumos = vec![
        ResumoDoOcr {
            lidas: 2,
            sem_texto: 1,
            falhas: 0,
            pendentes: true,
            indisponivel: false,
            causa: None,
        },
        ResumoDoOcr {
            lidas: 1,
            ..ResumoDoOcr::default()
        },
    ];
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_ocr(Box::new(LeitorFalso(n.clone(), resumos)));
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "a primeira volta já roda");
    w.passo(t0 + 1, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2, "sobrou imagem: roda de novo na volta seguinte");
    w.passo(t0 + 2, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2, "não sobrou: espera o minuto");
    w.passo(t0 + 62, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 3);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("ocr lidas=2 sem_texto=1 falhas=0"), "{log}");
}

#[test]
fn ocr_que_so_falha_nao_gira_a_cada_tique_espera_e_registra_a_causa() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let so_falhas = ResumoDoOcr {
        falhas: 10,
        pendentes: true,
        causa: Some("formato"),
        ..ResumoDoOcr::default()
    };
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_ocr(Box::new(LeitorFalso(n.clone(), vec![so_falhas])));
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1);
    for s in 1..=4 {
        w.passo(t0 + s * 2, Duration::from_millis(1), &nao);
    }
    assert_eq!(n.get(), 1, "o lote só falhou: não repete a cada tique");
    w.passo(t0 + 299, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "ainda no recuo de 5 min");
    w.passo(t0 + 300, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2, "passado o recuo, tenta de novo");
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(
        log.contains("ocr lidas=0 sem_texto=0 falhas=10 causa=formato"),
        "{log}"
    );
    assert_eq!(w.estado.falhas, 0, "falha de imagem não é falha do worker");
}

#[test]
fn ocr_indisponivel_ou_sem_nada_novo_nao_escreve_no_log_nem_conta_falha() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let indisponivel = ResumoDoOcr {
        indisponivel: true,
        ..ResumoDoOcr::default()
    };
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_ocr(Box::new(LeitorFalso(n.clone(), vec![indisponivel])));
    w.passo(agora(), Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(n.get(), 1);
    assert_eq!(w.estado.falhas, 0);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(!log.contains("ocr "), "{log}");
}

#[test]
fn telegram_coleta_no_intervalo_do_worker_json_e_registra_sem_conteudo() {
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let cfg = Config {
        telegram_minutos: 15,
        ..Config::default()
    };
    let mut w = Worker::novo(c.dados.clone(), cfg, None, None)
        .unwrap()
        .com_telegram(Box::new(TelegramFalso(n.clone())));
    let nao = AtomicBool::new(false);
    let t0 = agora();
    w.passo(t0, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "a primeira coleta é logo");
    assert_eq!(w.estado.ultima_coleta.as_ref().unwrap().origem, "telegram");
    assert_eq!(w.estado.ultima_coleta.as_ref().unwrap().novos, 4);
    w.passo(t0 + 14 * 60, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 1, "antes do intervalo");
    w.passo(t0 + 15 * 60 + 1, Duration::from_millis(1), &nao);
    assert_eq!(n.get(), 2);
    let log = std::fs::read_to_string(c.dados.log()).unwrap();
    assert!(log.contains("telegram conversas=2 novos=4"), "{log}");
}

#[test]
fn telegram_minutos_tem_padrao_15_e_zero_desliga() {
    assert_eq!(Config::default().telegram_minutos, 15);
    let c = cenario();
    let n = Rc::new(Cell::new(0));
    let cfg = Config {
        telegram_minutos: 0,
        ..Config::default()
    };
    let mut w = Worker::novo(c.dados.clone(), cfg, None, None)
        .unwrap()
        .com_telegram(Box::new(TelegramFalso(n.clone())));
    w.passo(agora(), Duration::from_millis(1), &AtomicBool::new(false));
    assert_eq!(n.get(), 0);
}

// -- O estado aparece logo e não envelhece num passo longo --------------------------------------

/// Um OCR que demora (faz de varredura inicial longa) e avisa quando terminou.
struct LeitorLento(Arc<AtomicBool>, Duration);
impl LeitorDeImagens for LeitorLento {
    fn ler_novas(&mut self, _: &AtomicBool) -> Result<ResumoDoOcr, Falha> {
        std::thread::sleep(self.1);
        self.0.store(true, Ordering::Relaxed);
        Ok(ResumoDoOcr::default())
    }
}

#[test]
fn rodar_grava_o_estado_logo_e_o_renova_durante_um_passo_longo() {
    let c = cenario();
    // Um estado velho de outro processo, como o que ficou depois do reinício do serviço.
    let velho = EstadoDoWorker {
        pid: 1,
        ..EstadoDoWorker::default()
    };
    std::fs::write(c.dados.estado(), serde_json::to_vec(&velho).unwrap()).unwrap();

    let terminou = Arc::new(AtomicBool::new(false));
    let mut w = Worker::novo(c.dados.clone(), Config::default(), None, None)
        .unwrap()
        .com_ocr(Box::new(LeitorLento(
            terminou.clone(),
            Duration::from_millis(3500),
        )))
        .com_batimento(1);
    let cancelar = AtomicBool::new(false);
    let estado = c.dados.estado();
    let mut vistos = std::collections::BTreeSet::new();
    let mut primeiro_pid = None;
    std::thread::scope(|s| {
        let observador = s.spawn(|| {
            let t0 = std::time::Instant::now();
            while !terminou.load(Ordering::Relaxed) && t0.elapsed() < Duration::from_secs(15) {
                if let Some(e) = EstadoDoWorker::ler(&estado)
                    && e.pid == std::process::id()
                {
                    primeiro_pid.get_or_insert(e.pid);
                    vistos.insert(e.atualizado_em);
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            cancelar.store(true, Ordering::Relaxed);
        });
        w.rodar(&cancelar);
        observador.join().unwrap();
    });
    assert_eq!(primeiro_pid, Some(std::process::id()), "pid novo logo");
    assert!(
        vistos.len() >= 3,
        "o estado renovou durante o passo longo: {vistos:?}"
    );
}
