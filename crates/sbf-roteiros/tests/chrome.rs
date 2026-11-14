//! Chrome de verdade contra o site falso local (127.0.0.1). Só rodam com `SBF_CHROME_TESTE=1`
//! (e com o Chrome ou Chromium instalado; `SBF_CHROME` aponta outro). Por padrão sem janela;
//! `SBF_CHROME_VISIVEL=1` mostra a janela. Nada toca site real.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use comum::{CPF, SENHA, Senhas, inventario};
use sbf_roteiros::Navegador as _;
use sbf_roteiros::banco::{BancoDeRoteiros, OrigemDaVersao};
use sbf_roteiros::cdp::Cdp;
use sbf_roteiros::chrome::{Chrome, OpcoesDoChrome};
use sbf_roteiros::executor::{Contexto, Decisao, Fim, Opcoes, Pausa, executar};
use sbf_roteiros::inventario::NoInventario;
use sbf_roteiros::passo::Passo;
use sbf_roteiros::site_falso::{Configuracao, PDF_FICTICIO, SiteFalso};
use std::sync::Mutex;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn ligado() -> bool {
    if std::env::var("SBF_CHROME_TESTE").ok().as_deref() == Some("1") {
        return true;
    }
    eprintln!("pulado: defina SBF_CHROME_TESTE=1 para rodar com o Chrome");
    false
}

fn abrir(pasta: &std::path::Path) -> Chrome {
    Chrome::abrir(&OpcoesDoChrome {
        programa: None,
        perfil: pasta.join("perfil"),
        sem_janela: std::env::var("SBF_CHROME_VISIVEL").ok().as_deref() != Some("1"),
    })
    .unwrap()
}

/// O "usuário": age na página por uma segunda conexão CDP enquanto o executor espera.
struct UsuarioNoNavegador {
    alvo: String,
    script: &'static str,
    chamadas: Mutex<u32>,
}

impl Pausa for UsuarioNoNavegador {
    fn aguardar(&self, _execucao: i64, _motivo: &str) -> Decisao {
        *self.chamadas.lock().unwrap() += 1;
        let mut cdp = Cdp::conectar(&self.alvo).unwrap();
        cdp.avaliar(self.script).unwrap();
        std::thread::sleep(Duration::from_millis(900));
        Decisao::Continuar
    }
}

fn opcoes() -> Opcoes {
    Opcoes {
        espera: Duration::from_secs(8),
        download: Duration::from_secs(15),
        intervalo: Duration::from_millis(100),
    }
}

fn passos_de_login(site: &SiteFalso) -> Vec<Passo> {
    vec![
        Passo::Navegar {
            url: site.url("/login"),
        },
        Passo::PreencherCpf {
            seletor: "#cpf".into(),
            host: None,
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: None,
        },
        Passo::Clicar {
            seletor: "#entrar".into(),
        },
    ]
}

#[test]
fn fluxo_completo_com_mfa_download_e_inventario() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao {
        com_mfa: true,
        com_captcha: false,
    })
    .unwrap();
    let mut chrome = abrir(d.path());
    let mut passos = passos_de_login(&site);
    passos.push(Passo::EsperarSeletor {
        seletor: "#extratos-link".into(),
        segundos: Some(10),
    });
    passos.push(Passo::Clicar {
        seletor: "#extratos-link".into(),
    });
    passos.push(Passo::LerTabela {
        seletor: "#extratos".into(),
        nome: "extratos".into(),
    });
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: Some("Extrato outubro".into()),
    });

    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_roteiro(
            "meu banco",
            "",
            &site.url("/login"),
            Some("banco-ficticio"),
            &passos,
            OrigemDaVersao::Gravacao,
        )
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let (cofre, conn) = inventario(d.path());
    let mut rel_final = None;
    for rodada in 0..2 {
        let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
        let mut destino = NoInventario::novo(&conn, &cofre, "meu banco", execucao);
        let pausa = UsuarioNoNavegador {
            alvo: chrome.alvo_da_pagina().to_owned(),
            script: "document.querySelector('#codigo').value='123456';document.querySelector('#fm').requestSubmit();true",
            chamadas: Mutex::new(0),
        };
        let nao = AtomicBool::new(false);
        let rel = {
            let mut c = Contexto {
                navegador: &mut chrome,
                senhas: &Senhas,
                pausa: &pausa,
                destino: &mut destino,
                banco: &banco,
                pasta: d.path().join(format!("baixados{rodada}")),
                opcoes: opcoes(),
                cancelar: &nao,
            };
            executar(&mut c, execucao, &roteiro).unwrap()
        };
        destino.fechar("concluida").unwrap();
        assert_eq!(rel.fim, Fim::Concluida, "rodada {rodada}");
        assert_eq!(
            *pausa.chamadas.lock().unwrap(),
            1,
            "uma pausa de MFA por rodada"
        );
        rel_final = Some(rel);
        // a segunda rodada começa sem sessão? o perfil guarda os cookies; o login refaz do mesmo jeito
    }
    let _ = rel_final;
    // o site aceitou a senha digitada pelo CDP
    assert!(site.logins() >= 2, "logins aceitos: {}", site.logins());
    assert_eq!(site.logins_recusados(), 0);
    assert_eq!(site.perigosas(), 0);
    assert!(site.downloads() >= 2);
    // inventário: PDF e CSV, sem duplicar na segunda rodada
    let achados: i64 = conn
        .query_row("SELECT count(*) FROM achado", [], |l| l.get(0))
        .unwrap();
    let origens: i64 = conn
        .query_row("SELECT count(*) FROM origem", [], |l| l.get(0))
        .unwrap();
    assert_eq!((achados, origens), (2, 2));
    let (hash, tamanho): (String, i64) = conn
        .query_row(
            "SELECT content_hash, file_size_bytes FROM achado WHERE extensao = 'pdf'",
            [],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .unwrap();
    assert_eq!(tamanho as usize, PDF_FICTICIO.len());
    assert_eq!(hash.len(), 64);
    // a senha não está em nenhum evento nem no roteiro gravado
    for e in banco.eventos(1).unwrap() {
        assert!(!format!("{e:?}").contains(SENHA) && !format!("{e:?}").contains(CPF));
    }
    chrome.fechar();
}

#[test]
fn a_pagina_simplificada_do_chrome_nao_tem_valor_de_campo() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    chrome.navegar(&site.url("/login")).unwrap();
    chrome.preencher("#cpf", CPF).unwrap();
    chrome
        .preencher_segredo("#senha", &sbf_chaveiro::Segredo::novo(SENHA))
        .unwrap();
    // o valor existe na página (o site vai aceitar), mas a página simplificada não o mostra
    let valor_no_dom = Cdp::conectar(chrome.alvo_da_pagina())
        .unwrap()
        .avaliar("document.querySelector('#senha').value")
        .unwrap();
    assert_eq!(valor_no_dom, SENHA, "a senha chegou ao campo");
    let p = chrome.pagina().unwrap();
    let saida = serde_json::to_string(&p).unwrap() + &p.para_texto();
    assert!(!saida.contains(SENHA), "senha na página simplificada");
    assert!(
        !saida.contains(CPF) && !saida.contains("12345678909"),
        "CPF na página simplificada"
    );
    let campos: Vec<_> = p.formularios.iter().flat_map(|f| &f.campos).collect();
    assert!(
        campos.iter().all(|c| c.preenchido),
        "os campos aparecem como preenchidos, sem valor"
    );
    assert!(campos.iter().any(|c| c.tipo == "password"));
    chrome.fechar();
}

#[test]
fn ro_02_no_navegador_clique_e_envio_perigosos_sao_barrados() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    // entra
    chrome.navegar(&site.url("/login")).unwrap();
    chrome.preencher("#cpf", CPF).unwrap();
    chrome
        .preencher_segredo("#senha", &sbf_chaveiro::Segredo::novo(SENHA))
        .unwrap();
    chrome.clicar("#entrar").unwrap();
    assert!(chrome.visivel("#pagar").unwrap(), "chegou ao painel");

    // 1. botão Pagar (submit de formulário para /pagar)
    // 2. link inocente "Continuar" que leva a /pagamento/confirmar
    // 3. botão "Enviar" cujo formulário tem action /transferir
    // 4. botão PIX com onclick de navegação
    for seletor in ["#pagar", "#seguir", "#enviar", "#pix"] {
        chrome.clicar(seletor).unwrap();
        let b = chrome.bloqueios().unwrap();
        assert!(!b.is_empty(), "o navegador deveria ter barrado {seletor}");
        let url = chrome.pagina().unwrap().url;
        assert!(
            url.ends_with("/painel"),
            "{seletor}: saiu do painel para {url}"
        );
    }
    // formulário disfarçado enviado por script
    let mut cdp = Cdp::conectar(chrome.alvo_da_pagina()).unwrap();
    cdp.avaliar("document.querySelector('form[action=\"/transferir\"]').submit();true")
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert!(chrome.pagina().unwrap().url.ends_with("/painel"));
    cdp.avaliar("document.querySelector('form[action=\"/pagar\"]').requestSubmit();true")
        .unwrap();
    std::thread::sleep(Duration::from_millis(400));
    assert!(chrome.pagina().unwrap().url.ends_with("/painel"));
    assert_eq!(site.perigosas(), 0, "nenhuma rota perigosa foi alcançada");
    chrome.fechar();
}

#[test]
fn ro_02_executor_com_chrome_aborta_no_botao_pagar() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    let mut passos = passos_de_login(&site);
    passos.push(Passo::Clicar {
        seletor: "#pagar".into(),
    });
    passos.push(Passo::Clicar {
        seletor: "#extratos-link".into(),
    });
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_roteiro(
            "x",
            "",
            "",
            Some("banco-ficticio"),
            &passos,
            OrigemDaVersao::Manual,
        )
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
    let (cofre, conn) = inventario(d.path());
    let mut destino = NoInventario::novo(&conn, &cofre, "x", execucao);
    let pausa = comum::PausaFalsa::default();
    let nao = AtomicBool::new(false);
    let rel = {
        let mut c = Contexto {
            navegador: &mut chrome,
            senhas: &Senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: d.path().join("b"),
            opcoes: opcoes(),
            cancelar: &nao,
        };
        executar(&mut c, execucao, &roteiro).unwrap()
    };
    assert!(
        matches!(rel.fim, Fim::Bloqueada { passo: 4, .. }),
        "{:?}",
        rel.fim
    );
    assert_eq!(site.perigosas(), 0);
    let h = banco.historico(None, 1).unwrap();
    assert_eq!(h[0].estado.texto(), "bloqueada");
    chrome.fechar();
}

#[test]
fn ro_03_captcha_pausa_e_o_site_mudado_falha_no_passo() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao {
        com_mfa: false,
        com_captcha: true,
    })
    .unwrap();
    let mut chrome = abrir(d.path());
    let mut passos = passos_de_login(&site);
    passos.push(Passo::Clicar {
        seletor: "#extratos-link".into(),
    });
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: None,
    });
    site.mudar_layout(); // o botão deixou de ser #baixar
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_roteiro(
            "x",
            "",
            "",
            Some("banco-ficticio"),
            &passos,
            OrigemDaVersao::Manual,
        )
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
    let (cofre, conn) = inventario(d.path());
    let mut destino = NoInventario::novo(&conn, &cofre, "x", execucao);
    let pausa = UsuarioNoNavegador {
        alvo: chrome.alvo_da_pagina().to_owned(),
        script: "document.querySelector('#humano').click();true",
        chamadas: Mutex::new(0),
    };
    let nao = AtomicBool::new(false);
    let mut op = opcoes();
    op.espera = Duration::from_secs(2);
    let rel = {
        let mut c = Contexto {
            navegador: &mut chrome,
            senhas: &Senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: d.path().join("b"),
            opcoes: op,
            cancelar: &nao,
        };
        executar(&mut c, execucao, &roteiro).unwrap()
    };
    assert_eq!(
        *pausa.chamadas.lock().unwrap(),
        1,
        "o CAPTCHA pausou uma vez"
    );
    assert_eq!(rel.pausas, 1);
    assert!(
        matches!(rel.fim, Fim::Falhou { passo: 5, .. }),
        "{:?}",
        rel.fim
    );
    assert!(site.logins() == 1 && site.downloads() == 0);
    chrome.fechar();
}
