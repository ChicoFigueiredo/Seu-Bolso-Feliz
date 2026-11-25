//! Chrome de verdade contra o site falso local (127.0.0.1). Só rodam com `SBF_CHROME_TESTE=1`
//! (e com o Chrome ou Chromium instalado; `SBF_CHROME` aponta outro). Por padrão sem janela;
//! `SBF_CHROME_VISIVEL=1` mostra a janela. Nada toca site real.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_navegador::site_falso::{Configuracao, PDF_FICTICIO, SiteFalso};
use comum::{CPF, SENHA, Senhas, inventario};
use sbf_roteiros::Navegador as _;
use sbf_roteiros::banco::{BancoDeRoteiros, OrigemDaVersao};
use sbf_roteiros::cdp::Cdp;
use sbf_roteiros::chrome::{Chrome, OpcoesDoChrome};
use sbf_roteiros::executor::{Contexto, Decisao, Fim, Opcoes, Pausa, executar};
use sbf_roteiros::inventario::NoInventario;
use sbf_roteiros::passo::Passo;
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
    sbf_roteiros::chrome::abrir(&OpcoesDoChrome {
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
            host: Some("127.0.0.1".into()),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("127.0.0.1".into()),
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
    assert_eq!(
        (achados, origens),
        (2, 2),
        "achados e origens: {:?}",
        conn.prepare("SELECT origin_key FROM origem")
            .unwrap()
            .query_map([], |l| l.get::<_, String>(0))
            .unwrap()
            .collect::<Vec<_>>()
    );
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

// ---------------------------------------------------------------------------------------------
// Revisão de segurança (Opus): a página é hostil e o SBF não confia em nada que ela diga.
// ---------------------------------------------------------------------------------------------

/// Roda `passos` no Chrome e devolve o relatório e o banco.
fn rodar_no_chrome(
    chrome: &mut Chrome,
    pasta: &std::path::Path,
    passos: &[Passo],
) -> (
    sbf_roteiros::executor::Relatorio,
    BancoDeRoteiros,
    std::path::PathBuf,
) {
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_roteiro(
            "x",
            "",
            "",
            Some("banco-ficticio"),
            passos,
            OrigemDaVersao::Manual,
        )
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
    let (cofre, conn) = inventario(pasta);
    let mut destino = NoInventario::novo(&conn, &cofre, "x", execucao);
    let pausa = comum::PausaFalsa::default();
    let nao = AtomicBool::new(false);
    let baixados = pasta.join("b");
    let rel = {
        let mut c = Contexto {
            navegador: chrome,
            senhas: &Senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: baixados.clone(),
            opcoes: opcoes(),
            cancelar: &nao,
        };
        executar(&mut c, execucao, &roteiro).unwrap()
    };
    (rel, banco, baixados)
}

/// O mesmo servidor, visto pelo outro nome: o host muda (`localhost`), o servidor não.
fn como_localhost(url: &str) -> String {
    url.replace("127.0.0.1", "localhost")
}

#[test]
fn ro_01_pagina_hostil_que_troca_a_ponte_e_mente_a_url_nao_recebe_a_senha() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    // O usuário aprovou a senha em 127.0.0.1; a página hostil está em localhost e jura, pela
    // ponte que ela mesma trocou, que está em 127.0.0.1.
    let passos = vec![
        Passo::Navegar {
            url: como_localhost(&site.url("/hostil")),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("127.0.0.1".into()),
        },
    ];
    let (rel, _banco, _) = rodar_no_chrome(&mut chrome, d.path(), &passos);
    assert!(
        matches!(rel.fim, Fim::Falhou { passo: 1, .. }),
        "{:?}",
        rel.fim
    );
    let valor = Cdp::conectar(chrome.alvo_da_pagina())
        .unwrap()
        .avaliar("document.querySelector('#senha').value")
        .unwrap();
    assert_eq!(
        valor, "",
        "a senha não pode ter sido digitada na página hostil"
    );
    chrome.fechar();
}

#[test]
fn ro_02_clique_inocente_que_paga_e_transfere_por_script_e_barrado_na_rede() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    let passos = vec![
        Passo::Navegar {
            url: site.url("/continuar"),
        },
        Passo::Clicar {
            seletor: "#continuar".into(),
        },
        Passo::Baixar {
            seletor: "#baixar".into(),
            nome: None,
        },
    ];
    let (rel, banco, _) = rodar_no_chrome(&mut chrome, d.path(), &passos);
    assert!(
        matches!(rel.fim, Fim::Bloqueada { passo: 1, .. }),
        "{:?}",
        rel.fim
    );
    assert_eq!(
        site.perigosas(),
        0,
        "o fetch POST /pagar e o location=/transferir não podem chegar ao servidor"
    );
    assert_eq!(site.downloads(), 0, "o roteiro abortou antes do download");
    assert_eq!(
        banco.historico(None, 1).unwrap()[0].estado.texto(),
        "bloqueada"
    );
    chrome.fechar();
}

#[test]
fn ro_02_iframe_de_outro_site_tambem_e_vigiado_na_rede() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    chrome.navegar(&site.url("/quadro-de-fora")).unwrap();
    let limite = std::time::Instant::now() + Duration::from_secs(6);
    let mut barrados = Vec::new();
    while barrados.is_empty() && std::time::Instant::now() < limite {
        barrados = chrome.bloqueios().unwrap();
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !barrados.is_empty(),
        "o fetch do iframe deveria ser barrado"
    );
    assert_eq!(
        site.perigosas(),
        0,
        "a requisição do iframe chegou ao servidor"
    );
    chrome.fechar();
}

#[test]
fn ro_02_na_fase_de_login_envio_para_outro_site_e_barrado() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    chrome.navegar(&site.url("/login")).unwrap();
    chrome.fase_de_login(Some("127.0.0.1"));
    // um envio (POST) para o outro nome do servidor, como um formulário hostil faria
    Cdp::conectar(chrome.alvo_da_pagina())
        .unwrap()
        .avaliar(&format!(
            "fetch('http://localhost:{}/captcha-ok',{{method:'POST',mode:'no-cors'}});true",
            site.porta()
        ))
        .unwrap();
    let limite = std::time::Instant::now() + Duration::from_secs(5);
    let mut barrados = Vec::new();
    while barrados.is_empty() && std::time::Instant::now() < limite {
        barrados = chrome.bloqueios().unwrap();
        std::thread::sleep(Duration::from_millis(100));
    }
    assert!(
        !barrados.is_empty(),
        "POST para outro host no login deveria cair"
    );
    // fora da fase de login o mesmo POST é só uma requisição comum
    chrome.fase_de_login(None);
    chrome.fechar();
}

#[test]
fn ro_05_clique_coberto_por_capa_ou_quadro_e_recusado() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    for rota in ["/coberto", "/coberto-por-quadro"] {
        chrome.navegar(&site.url(rota)).unwrap();
        let r = chrome.clicar("#alvo");
        assert!(
            r.is_err(),
            "{rota}: o clique no ponto coberto devia ser recusado"
        );
        let url = chrome.pagina().unwrap().url;
        assert!(url.ends_with(rota), "{rota}: foi levado para {url}");
    }
    chrome.fechar();
}

#[test]
fn ro_05_botao_dentro_de_shadow_dom_e_conferido_pelo_alvo_de_verdade() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    chrome.navegar(&site.url("/sombra")).unwrap();
    chrome.clicar("#hospedeiro").unwrap();
    let b = chrome.bloqueios().unwrap();
    assert!(
        !b.is_empty(),
        "o botão Pagar do shadow DOM devia ser barrado"
    );
    assert!(chrome.pagina().unwrap().url.ends_with("/sombra"));
    chrome.fechar();
}

#[test]
fn ro_02_janela_nova_aberta_por_script_nao_escapa_da_vigilancia() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    chrome.navegar(&site.url("/abre-janela")).unwrap();
    let _ = chrome.clicar("#abre");
    std::thread::sleep(Duration::from_millis(1200));
    assert_eq!(
        site.perigosas(),
        0,
        "a janela nova chegou a /transferir sem passar pela lista de bloqueio"
    );
    chrome.fechar();
}

#[test]
fn ro_02_no_navegador_a_excecao_do_comprovante_vale_e_o_resto_continua_barrado() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    chrome.navegar(&site.url("/comprovante")).unwrap();
    chrome.clicar("#ok").unwrap();
    assert!(chrome.bloqueios().unwrap().is_empty(), "consulta passa");
    let titulo = Cdp::conectar(chrome.alvo_da_pagina())
        .unwrap()
        .avaliar("document.title")
        .unwrap();
    assert_eq!(titulo, "clicou");
    chrome.clicar("#mau").unwrap();
    assert!(
        !chrome.bloqueios().unwrap().is_empty(),
        "o resto do campo vale"
    );
    chrome.fechar();
}

#[test]
fn texto_no_chrome_le_o_que_se_ve_e_nunca_o_valor_de_campo() {
    if !ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let site = SiteFalso::iniciar(Configuracao::default()).unwrap();
    let mut chrome = abrir(d.path());
    chrome.navegar(&site.url("/comprovante")).unwrap();
    assert_eq!(chrome.texto("#ok").unwrap(), "Comprovante de pagamento");
    assert!(chrome.texto("#nao-existe").is_err());

    chrome.navegar(&site.url("/login")).unwrap();
    chrome.preencher("#cpf", CPF).unwrap();
    chrome
        .preencher_segredo("#senha", &sbf_chaveiro::Segredo::novo(SENHA))
        .unwrap();
    // O valor existe no campo, mas `texto` recusa o campo e a página inteira não o repete.
    assert!(chrome.texto("#senha").is_err());
    assert!(chrome.texto("#cpf").is_err());
    let corpo = chrome.texto("body").unwrap();
    assert!(
        !corpo.contains(SENHA) && !corpo.contains(CPF),
        "o texto visível da página não traz valor de campo"
    );
    chrome.fechar();
}
