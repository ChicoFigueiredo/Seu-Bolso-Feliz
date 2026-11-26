//! O `testar` do cenário `conexoes` (Meu Pluggy fictício) com o Chrome de verdade. Só roda com
//! `SBF_CHROME_TESTE=1` (Chrome sem janela, perfil descartável, tudo em `127.0.0.1`); sem isso,
//! cada teste avisa e passa. Nada toca rede externa nem dado real.
//!
//! - O exemplo `meu-pluggy-atualizar` passa: cada conexão é atualizada exatamente uma vez.
//! - Um roteiro tendencioso, que clica no vizinho "Excluir", é barrado pela lista de bloqueio.
//! - Com a sessão expirada o roteiro pede a vez do usuário e o teste FALHA (não há usuário).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_navegador::site_falso_conexoes::{Config, PortalDeConexoes};
use comum::inventario;
use sbf_roteiros::banco::{AutorEscrito, EstadoEscrito};
use sbf_roteiros::chrome::OpcoesDoChrome;
use sbf_roteiros::executor::Opcoes;
use sbf_roteiros::manifesto::Manifesto;
use sbf_roteiros::servico::Ambiente;
use sbf_roteiros::teste_escrito::{Cenario, testar_conexoes_em, testar_escrito};
use std::path::Path;
use std::time::Duration;

const MANIFESTO: &str = include_str!("../exemplos/meu-pluggy-atualizar.json");
const FONTE: &str = include_str!("../exemplos/meu-pluggy-atualizar.luau");

fn chrome_ligado() -> bool {
    if std::env::var("SBF_CHROME_TESTE").ok().as_deref() == Some("1") {
        return true;
    }
    eprintln!("pulado: defina SBF_CHROME_TESTE=1 para rodar com o Chrome");
    false
}

/// Um ambiente para o Chrome de verdade: prazos de uso real.
fn ambiente_chrome(d: &Path) -> Ambiente {
    let (cofre, _conn) = inventario(d);
    let mut a = Ambiente::novo(&d.join("dados"), &cofre);
    a.opcoes = Opcoes {
        espera: Duration::from_secs(15),
        download: Duration::from_secs(30),
        intervalo: Duration::from_millis(100),
    };
    a.prazo_da_pausa = Duration::from_secs(5);
    a.sem_janela = true;
    a
}

/// Importa um roteiro como rascunho e devolve `(nome, numero)`.
fn importar(amb: &Ambiente, json: &str, fonte: &str) -> (String, u32) {
    let m = Manifesto::de_json(json).unwrap();
    let n = amb
        .banco()
        .unwrap()
        .importar_escrito(&m, fonte, &AutorEscrito::Usuario, "teste")
        .unwrap()
        .numero;
    (m.nome, n)
}

fn estado(amb: &Ambiente, nome: &str, n: u32) -> EstadoEscrito {
    amb.banco().unwrap().versao_escrita(nome, n).unwrap().estado
}

fn abrir_chrome(perfil: &Path) -> sbf_roteiros::chrome::Chrome {
    sbf_roteiros::chrome::abrir(&OpcoesDoChrome {
        programa: None,
        perfil: perfil.to_path_buf(),
        sem_janela: std::env::var("SBF_CHROME_VISIVEL").ok().as_deref() != Some("1"),
    })
    .unwrap()
}

fn config_do_teste() -> Config {
    Config::default().com_duracao_widget(Duration::from_secs(2))
}

#[test]
fn o_exemplo_passa_no_painel_falso_e_cada_conexao_e_atualizada_uma_vez() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente_chrome(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, FONTE);
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);

    let portal = PortalDeConexoes::iniciar(config_do_teste()).unwrap();
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let r = testar_conexoes_em(&amb, &nome, n, &portal, &mut chrome).unwrap();
    chrome.fechar();
    eprintln!("resumo: {}", r.resumo);
    assert!(r.passou, "{}", r.resumo);

    assert_eq!(portal.total_de_atualizacoes(), 3);
    for id in portal.ids() {
        assert_eq!(portal.atualizacoes_de(&id), 1, "{id}");
    }
    assert_eq!(portal.exclusoes(), 0);
    assert!(portal.acessos_proibidos().is_empty());

    // a versão virou `testado`, com o resumo no histórico
    let v = amb.banco().unwrap().versao_escrita(&nome, n).unwrap();
    assert_eq!(v.estado, EstadoEscrito::Testado);
    assert!(v.resultado_do_teste.unwrap().starts_with("passou"));
}

#[test]
fn testar_escrito_sobe_o_proprio_painel_e_chrome_e_leva_a_testado() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente_chrome(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, FONTE);

    let r = testar_escrito(&amb, &nome, n, Cenario::Conexoes).unwrap();
    eprintln!("resumo: {}", r.resumo);
    assert!(r.passou, "{}", r.resumo);
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Testado);
    // o perfil dedicado do ambiente nem foi criado
    assert!(!amb.perfil().exists(), "o teste usou o perfil dedicado");
}

#[test]
fn roteiro_que_clica_em_excluir_e_barrado_pela_lista_de_bloqueio_e_o_teste_falha() {
    if !chrome_ligado() {
        return;
    }
    // O tendencioso: o "Atualizar" dele é o 2º botão (o Excluir) e a conferência de texto
    // concorda, para que só a lista de bloqueio o impeça.
    let alvo_antigo = "div:nth-of-type(2) > button:nth-of-type(1)\"\n\n-- A primeira conta";
    assert!(
        FONTE.contains(alvo_antigo),
        "o exemplo mudou: ajuste o teste tendencioso"
    );
    let fonte = FONTE
        .replace(
            alvo_antigo,
            "div:nth-of-type(2) > button:nth-of-type(2)\"\n\n-- A primeira conta",
        )
        .replace("~= \"Atualizar\"", "~= \"Excluir\"");
    assert_ne!(fonte, FONTE);

    let d = tempfile::tempdir().unwrap();
    let amb = ambiente_chrome(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, &fonte);

    let portal = PortalDeConexoes::iniciar(config_do_teste()).unwrap();
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let r = testar_conexoes_em(&amb, &nome, n, &portal, &mut chrome).unwrap();
    chrome.fechar();
    eprintln!("resumo: {}", r.resumo);

    assert!(!r.passou, "{}", r.resumo);
    assert!(
        r.resumo.contains("barrada pela lista de bloqueio"),
        "{}",
        r.resumo
    );
    assert_eq!(portal.exclusoes(), 0, "o Excluir chegou ao painel");
    assert!(portal.acessos_proibidos().is_empty());
    assert_eq!(portal.total_de_atualizacoes(), 0);
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);
}

#[test]
fn sessao_expirada_faz_o_roteiro_pausar_e_o_teste_falhar() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente_chrome(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, FONTE);

    let portal = PortalDeConexoes::iniciar(config_do_teste().com_sessao_expirada(true)).unwrap();
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let r = testar_conexoes_em(&amb, &nome, n, &portal, &mut chrome).unwrap();
    chrome.fechar();
    eprintln!("resumo: {}", r.resumo);

    assert!(!r.passou, "{}", r.resumo);
    assert!(r.resumo.contains("a vez do usuário"), "{}", r.resumo);
    assert!(r.resumo.contains("sessão expirada"), "{}", r.resumo);
    assert_eq!(portal.total_de_atualizacoes(), 0);
    assert_eq!(portal.exclusoes(), 0);
    // falhou: a versão continua rascunho
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);
}

// ------------------------------------------------------------------------------------------
// Sem Chrome
// ------------------------------------------------------------------------------------------

#[test]
fn manifesto_do_contracheque_nao_serve_ao_cenario_conexoes() {
    let d = tempfile::tempdir().unwrap();
    let amb = ambiente_chrome(d.path());
    let (nome, n) = importar(
        &amb,
        include_str!("../exemplos/contracheque-exemplo.json"),
        include_str!("../exemplos/contracheque-exemplo.luau"),
    );
    let e = testar_escrito(&amb, &nome, n, Cenario::Conexoes)
        .unwrap_err()
        .to_string();
    assert!(e.contains("este roteiro não é do cenário conexoes"), "{e}");
}
