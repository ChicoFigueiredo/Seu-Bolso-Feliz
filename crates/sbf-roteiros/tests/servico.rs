//! O serviço sobre o banco: a pausa passa pelo `roteiros.sqlite` (a janela grava Continuar), o
//! agendamento dispara e o roteiro não roda duas vezes ao mesmo tempo.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use comum::{Senhas, inventario, site};
use sbf_roteiros::banco::{BancoDeRoteiros, EstadoDaExecucao, OrigemDaVersao};
use sbf_roteiros::executor::{Fim, Opcoes};
use sbf_roteiros::passo::Passo;
use sbf_roteiros::servico::{Ambiente, executar_roteiro, executar_roteiro_em};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn ambiente(d: &std::path::Path) -> Ambiente {
    let (cofre, _conn) = inventario(d);
    let mut a = Ambiente::novo(&d.join("dados"), &cofre);
    a.opcoes = Opcoes {
        espera: Duration::from_millis(50),
        download: Duration::from_secs(1),
        intervalo: Duration::from_millis(5),
    };
    a.prazo_da_pausa = Duration::from_secs(10);
    a.sem_janela = true;
    a
}

fn criar(banco: &BancoDeRoteiros) -> i64 {
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherCpf {
            seletor: "#cpf".into(),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
        },
        Passo::Clicar {
            seletor: "#entrar".into(),
        },
        Passo::EsperarSeletor {
            seletor: "#baixar".into(),
            segundos: Some(1),
        },
        Passo::Baixar {
            seletor: "#baixar".into(),
            nome: None,
        },
    ];
    banco
        .criar_roteiro(
            "meu banco",
            "",
            "http://127.0.0.1:1/login",
            Some("banco-ficticio"),
            &passos,
            OrigemDaVersao::Gravacao,
        )
        .unwrap()
}

#[test]
fn a_janela_continua_pelo_banco_e_o_roteiro_retoma() {
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente(d.path());
    let banco = amb.banco().unwrap();
    let id = criar(&banco);
    let (versao, _, _) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, versao, "janela").unwrap();

    let mut nav = site(true);
    let sinal = std::sync::Arc::new(AtomicBool::new(false));
    nav.liberacao = Some(sinal.clone());
    let arquivo = amb.arquivo_do_banco();
    // a "janela": outro processo, outra conexão, vê "Sua vez no navegador" e clica Continuar
    let janela = std::thread::spawn(move || {
        let b = BancoDeRoteiros::abrir(&arquivo).unwrap();
        for _ in 0..400 {
            let esperando = b.aguardando().unwrap();
            if let Some(e) = esperando.first() {
                assert_eq!(e.estado.rotulo(), "Sua vez no navegador");
                assert!(e.motivo.as_deref().unwrap().contains("MFA"));
                sinal.store(true, std::sync::atomic::Ordering::SeqCst);
                b.pedir_continuar(e.id, true).unwrap();
                return true;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        false
    });
    let nao = AtomicBool::new(false);
    let rel = executar_roteiro_em(&amb, &mut nav, id, execucao, &Senhas, None, &nao).unwrap();
    assert!(janela.join().unwrap(), "a janela viu a execução aguardando");
    assert_eq!(rel.fim, Fim::Concluida);
    assert_eq!(rel.pausas, 1);
    let h = banco.historico(Some(id), 5).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Concluida);
    assert_eq!((h[0].baixados, h[0].novos), (1, 1));
}

#[test]
fn cancelar_na_janela_cancela_a_execucao() {
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente(d.path());
    let banco = amb.banco().unwrap();
    let id = criar(&banco);
    let (versao, _, _) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, versao, "janela").unwrap();
    let mut nav = site(true);
    let arquivo = amb.arquivo_do_banco();
    let janela = std::thread::spawn(move || {
        let b = BancoDeRoteiros::abrir(&arquivo).unwrap();
        for _ in 0..400 {
            if let Some(e) = b.aguardando().unwrap().first() {
                b.pedir_continuar(e.id, false).unwrap();
                return;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });
    let nao = AtomicBool::new(false);
    let rel = executar_roteiro_em(&amb, &mut nav, id, execucao, &Senhas, None, &nao).unwrap();
    janela.join().unwrap();
    assert_eq!(rel.fim, Fim::Cancelada);
    assert_eq!(
        banco.historico(Some(id), 1).unwrap()[0].estado,
        EstadoDaExecucao::Cancelada
    );
}

#[test]
fn a_pausa_que_ninguem_resolve_expira() {
    let d = tempfile::tempdir().unwrap();
    let mut amb = ambiente(d.path());
    amb.prazo_da_pausa = Duration::from_millis(300);
    let banco = amb.banco().unwrap();
    let id = criar(&banco);
    let (versao, _, _) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, versao, "agenda").unwrap();
    let mut nav = site(true);
    let nao = AtomicBool::new(false);
    let rel = executar_roteiro_em(&amb, &mut nav, id, execucao, &Senhas, None, &nao).unwrap();
    assert!(matches!(rel.fim, Fim::PausaExpirou { .. }), "{:?}", rel.fim);
    let h = banco.historico(Some(id), 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
    assert!(h[0].motivo.as_deref().unwrap().contains("não voltou"));
}

#[test]
fn roteiro_rodando_nao_roda_de_novo() {
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente(d.path());
    let banco = amb.banco().unwrap();
    let id = criar(&banco);
    let (versao, _, _) = banco.versao_ativa(id).unwrap();
    banco.iniciar_execucao(id, versao, "janela").unwrap();
    let nao = AtomicBool::new(false);
    let r = executar_roteiro(&amb, id, "agenda", &Senhas, None, &nao);
    assert!(
        matches!(r, Err(sbf_roteiros::ErroDeRoteiro::Invalido(m)) if m.contains("já está rodando"))
    );
}

#[test]
fn execucao_de_processo_morto_e_encerrada() {
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente(d.path());
    let banco = amb.banco().unwrap();
    let id = criar(&banco);
    let (versao, _, _) = banco.versao_ativa(id).unwrap();
    let viva = banco.iniciar_execucao(id, versao, "janela").unwrap();
    assert_eq!(banco.encerrar_orfas().unwrap(), 0, "este processo existe");
    assert!(banco.em_execucao(id).unwrap());
    // o processo dono morreu (pid que não existe): a execução vira falha
    let c = rusqlite::Connection::open(amb.arquivo_do_banco()).unwrap();
    c.execute("UPDATE execucao SET pid = 2000000000 WHERE id = ?1", [viva])
        .unwrap();
    assert_eq!(banco.encerrar_orfas().unwrap(), 1);
    assert!(!banco.em_execucao(id).unwrap());
    let h = banco.historico(Some(id), 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
}
