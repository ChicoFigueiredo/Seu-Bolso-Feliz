//! RO-01, RO-02, RO-03 e RO-05 no executor, com o site falso em memória (sem Chrome).

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_navegador::falso::{Elemento, NavegadorFalso};
use comum::{CPF, PausaFalsa, SENHA, Senhas, clicavel, inventario, site};
use sbf_roteiros::Navegador as _;
use sbf_roteiros::banco::{BancoDeRoteiros, OrigemDaVersao};
use sbf_roteiros::executor::{Contexto, Decisao, Fim, Opcoes, Relatorio, executar};
use sbf_roteiros::inventario::NoInventario;
use sbf_roteiros::passo::{FORMATO, Passo, Roteiro};
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn login(com_senha: bool) -> Vec<Passo> {
    let mut p = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherCpf {
            seletor: "#cpf".into(),
            host: Some("127.0.0.1".into()),
        },
    ];
    if com_senha {
        p.push(Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("127.0.0.1".into()),
        });
    }
    p.push(Passo::Clicar {
        seletor: "#entrar".into(),
    });
    p
}

struct Resultado {
    rel: Relatorio,
    banco: BancoDeRoteiros,
    execucao: i64,
    roteiro: Roteiro,
}

fn rodar(
    nav: &mut NavegadorFalso,
    passos: Vec<Passo>,
    pausa: &PausaFalsa,
    pasta: &std::path::Path,
) -> Resultado {
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_roteiro(
            "extratos",
            "",
            "http://127.0.0.1:1/login",
            Some("banco-ficticio"),
            &passos,
            OrigemDaVersao::Gravacao,
        )
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
    let (cofre, conn) = inventario(pasta);
    let mut destino = NoInventario::novo(&conn, &cofre, "extratos", execucao);
    let nao = AtomicBool::new(false);
    let rel = {
        let mut c = Contexto {
            navegador: nav,
            senhas: &Senhas,
            pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: pasta.join("baixados"),
            opcoes: Opcoes {
                espera: Duration::from_millis(30),
                download: Duration::from_secs(1),
                intervalo: Duration::from_millis(5),
            },
            cancelar: &nao,
        };
        executar(&mut c, execucao, &roteiro).unwrap()
    };
    destino.fechar("concluida").unwrap();
    Resultado {
        rel,
        banco,
        execucao,
        roteiro,
    }
}

#[test]
fn ro_01_a_senha_e_o_cpf_chegam_ao_campo_e_a_nada_mais() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let mut passos = login(true);
    passos.push(Passo::EsperarSeletor {
        seletor: "#baixar".into(),
        segundos: Some(1),
    });
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: None,
    });
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    assert_eq!(r.rel.fim, Fim::Concluida);
    // chegou ao campo, no passo determinístico
    assert!(
        nav.digitados
            .contains(&("#senha".to_owned(), SENHA.to_owned()))
    );
    assert!(nav.digitados.contains(&("#cpf".to_owned(), CPF.to_owned())));

    // procura a senha em TUDO que sai: roteiro gravado, eventos, histórico, relatório, Debug,
    // a página que iria à IA e as chamadas do navegador
    let mut saidas = vec![
        serde_json::to_string(&r.roteiro).unwrap(),
        format!("{:?}", r.rel),
        format!("{:?}", r.roteiro),
        nav.chamadas.join("\n"),
        nav.pagina().map(|p| p.para_texto()).unwrap(),
    ];
    for e in r.banco.eventos(r.execucao).unwrap() {
        saidas.push(format!("{e:?}"));
    }
    for h in r.banco.historico(None, 10).unwrap() {
        saidas.push(format!("{h:?}"));
    }
    for l in r.banco.listar().unwrap() {
        saidas.push(format!("{l:?}"));
    }
    for s in &saidas {
        assert!(!s.contains(SENHA), "a senha vazou: {s}");
        assert!(!s.contains(CPF), "o CPF vazou: {s}");
    }
}

#[test]
fn ro_01_erro_do_navegador_que_repete_a_senha_e_limpo() {
    // Um navegador que erra e repete o valor na mensagem não faz a senha chegar ao histórico.
    use bancada_navegador::erro::{ErroDoNavegador, Resultado as R};
    use sbf_chaveiro::Segredo;
    use sbf_roteiros::bloqueio::{Bloqueio, Candidato};
    use sbf_roteiros::navegador::Navegador;
    use sbf_roteiros::pagina::Pagina;

    struct Vaza(NavegadorFalso);
    impl Navegador for Vaza {
        fn navegar(&mut self, u: &str) -> R<()> {
            self.0.navegar(u)
        }
        fn visivel(&mut self, s: &str) -> R<bool> {
            self.0.visivel(s)
        }
        fn descrever(&mut self, s: &str) -> R<Option<Candidato>> {
            self.0.descrever(s)
        }
        fn clicar(&mut self, s: &str) -> R<()> {
            self.0.clicar(s)
        }
        fn preencher(&mut self, s: &str, t: &str) -> R<()> {
            self.0.preencher(s, t)
        }
        fn preencher_segredo(&mut self, _s: &str, segredo: &Segredo) -> R<()> {
            Err(ErroDoNavegador::Navegador(format!(
                "campo recusou o valor {}",
                segredo.expor()
            )))
        }
        fn clicar_e_baixar(
            &mut self,
            s: &str,
            p: &std::path::Path,
            t: Duration,
        ) -> R<std::path::PathBuf> {
            self.0.clicar_e_baixar(s, p, t)
        }
        fn ler_tabela(&mut self, s: &str) -> R<Vec<Vec<String>>> {
            self.0.ler_tabela(s)
        }
        fn pagina(&mut self) -> R<Pagina> {
            self.0.pagina()
        }
        fn bloqueios(&mut self) -> R<Vec<Bloqueio>> {
            self.0.bloqueios()
        }
    }

    let d = tempfile::tempdir().unwrap();
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let passos = login(true);
    let id = banco
        .criar_roteiro("x", "", "", None, &passos, OrigemDaVersao::Manual)
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
    let (cofre, conn) = inventario(d.path());
    let mut destino = NoInventario::novo(&conn, &cofre, "x", execucao);
    let mut nav = Vaza(site(false));
    let nao = AtomicBool::new(false);
    let pausa = PausaFalsa::default();
    let mut c = Contexto {
        navegador: &mut nav,
        senhas: &Senhas,
        pausa: &pausa,
        destino: &mut destino,
        banco: &banco,
        pasta: d.path().join("b"),
        opcoes: Opcoes {
            espera: Duration::from_millis(30),
            ..Opcoes::default()
        },
        cancelar: &nao,
    };
    let rel = executar(&mut c, execucao, &roteiro).unwrap();
    let Fim::Falhou { motivo, .. } = &rel.fim else {
        panic!("{:?}", rel.fim)
    };
    assert!(!motivo.contains(SENHA));
    assert!(motivo.contains("***"));
    for e in banco.eventos(execucao).unwrap() {
        assert!(!format!("{e:?}").contains(SENHA));
    }
    let h = banco.historico(None, 5).unwrap();
    assert!(!format!("{h:?}").contains(SENHA));
}

#[test]
fn ro_01_campo_de_senha_nao_aceita_texto_fixo() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::Preencher {
            seletor: "#senha".into(),
            texto: "a-senha-no-roteiro".into(),
        },
    ];
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    assert!(
        matches!(r.rel.fim, Fim::Falhou { passo: 1, .. }),
        "{:?}",
        r.rel.fim
    );
    assert!(nav.digitados.is_empty());
}

fn bloqueado_ao_clicar(mutar: impl FnOnce(&mut NavegadorFalso)) -> (Fim, Vec<String>) {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    mutar(&mut nav);
    let mut passos = login(true);
    passos.push(Passo::Clicar {
        seletor: "#perigo".into(),
    });
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: None,
    });
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    (r.rel.fim, nav.chamadas)
}

fn com_perigo(texto: &str, campos: &[(&str, &str)]) -> impl FnOnce(&mut NavegadorFalso) {
    let texto = texto.to_owned();
    let campos: Vec<(String, String)> = campos
        .iter()
        .map(|(a, b)| ((*a).to_owned(), (*b).to_owned()))
        .collect();
    move |n: &mut NavegadorFalso| {
        let p = n.paginas.get_mut("painel").unwrap();
        p.elementos.push(clicavel("#perigo", &texto));
        let mut e = Elemento::com_texto(&texto);
        for (k, v) in campos {
            e.candidato.campos.push((k, v));
        }
        n.elementos.insert("#perigo".into(), e);
    }
}

#[test]
fn ro_02_botoes_perigosos_abortam_o_roteiro_sem_clicar() {
    for texto in [
        "Pagar",
        "Confirmar pagamento",
        "Transferir",
        "PIX",
        "Pix copia e cola",
        "Contratar",
        "Resgatar",
        "Investir agora",
        "Alterar cadastro",
        "Transferência",
    ] {
        // o elemento aparece no painel: o roteiro chega nele depois do login
        let (fim, chamadas) = bloqueado_ao_clicar(|n| {
            n.rotas.clear();
            n.rotas
                .insert("http://127.0.0.1:1/login".into(), "login".into());
            com_perigo(texto, &[])(n);
            // o login leva ao painel direto
        });
        // o clique do login só leva ao painel se `entrar` levar; o site(false) leva ao painel
        match fim {
            Fim::Bloqueada { bloqueio, .. } => {
                assert!(!bloqueio.termo.is_empty(), "{texto}");
            }
            outro => panic!("{texto}: esperava Bloqueada, veio {outro:?}"),
        }
        assert!(
            !chamadas.iter().any(|c| c == "clicar #perigo"),
            "{texto}: clicou"
        );
        assert!(
            !chamadas.iter().any(|c| c.starts_with("baixar")),
            "{texto}: seguiu adiante"
        );
    }
}

#[test]
fn ro_02_links_e_formularios_disfarcados() {
    // texto inocente, mas o destino, o id, o name, o value ou o aria-label denunciam
    for campos in [
        vec![("destino", "/pagamento/confirmar")],
        vec![("destino", "https://x.example/api/pix")],
        vec![("id", "btnPagarAgora")],
        vec![("name", "acao_transferir")],
        vec![("value", "Confirmar pagamento")],
        vec![("aria-label", "Efetuar transferência")],
        vec![("destino", "/transferencia")],
    ] {
        let (fim, chamadas) = bloqueado_ao_clicar(com_perigo("Continuar", &campos));
        assert!(matches!(fim, Fim::Bloqueada { .. }), "{campos:?}: {fim:?}");
        assert!(
            !chamadas.iter().any(|c| c == "clicar #perigo"),
            "{campos:?}"
        );
    }
}

#[test]
fn ro_02_o_que_so_o_script_do_navegador_pega_tambem_aborta() {
    // o executor não vê nada de errado, mas o navegador barrou o clique (captura em campo
    // que o executor não descreveu): o roteiro aborta no passo.
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let mut e = Elemento::com_texto("Continuar");
    e.barrado_no_navegador = Some(sbf_roteiros::Bloqueio {
        campo: "destino".into(),
        termo: "pagament".into(),
    });
    nav.elementos.insert("#ver".into(), e);
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::Clicar {
            seletor: "#entrar".into(),
        },
        Passo::Clicar {
            seletor: "#ver".into(),
        },
        Passo::Baixar {
            seletor: "#baixar".into(),
            nome: None,
        },
    ];
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    assert!(
        matches!(r.rel.fim, Fim::Bloqueada { passo: 2, .. }),
        "{:?}",
        r.rel.fim
    );
    assert!(!nav.chamadas.iter().any(|c| c.starts_with("baixar")));
    let h = r.banco.historico(None, 1).unwrap();
    assert_eq!(h[0].estado.texto(), "bloqueada");
    assert!(h[0].motivo.as_deref().unwrap().contains("consulta e baixa"));
}

#[test]
fn ro_02_navegar_para_url_de_pagamento_e_bloqueado() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let passos = vec![Passo::Navegar {
        url: "http://127.0.0.1:1/pagamento/novo".into(),
    }];
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    assert!(matches!(r.rel.fim, Fim::Bloqueada { passo: 0, .. }));
    assert!(nav.chamadas.is_empty());
}

#[test]
fn ro_03_mfa_detectado_pausa_e_continuar_retoma_do_passo_certo() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(true);
    let sinal = std::sync::Arc::new(AtomicBool::new(false));
    nav.liberacao = Some(sinal.clone());
    let mut passos = login(true);
    // sem passo explícito: a detecção é que para, antes de esperar o painel (passo 4)
    passos.push(Passo::EsperarSeletor {
        seletor: "#baixar".into(),
        segundos: Some(1),
    });
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: None,
    });
    let mut pausa = PausaFalsa::com(&[Decisao::Continuar]);
    pausa.sinal = Some(sinal);
    let r = rodar(&mut nav, passos, &pausa, d.path());
    assert_eq!(r.rel.fim, Fim::Concluida);
    assert_eq!(r.rel.pausas, 1);
    assert!(pausa.motivos.lock().unwrap()[0].contains("MFA"));
    let eventos = r.banco.eventos(r.execucao).unwrap();
    let vistos: Vec<i64> = eventos
        .iter()
        .filter(|e| e.2 == "passo")
        .filter_map(|e| e.1)
        .collect();
    // cada passo executado uma só vez: o login não se repete depois do Continuar
    assert_eq!(vistos, vec![0, 1, 2, 3, 4, 5]);
    let pausa_ev = eventos.iter().position(|e| e.2 == "pausa").unwrap();
    let retomada = eventos.iter().position(|e| e.2 == "retomada").unwrap();
    assert!(pausa_ev < retomada);
    // a pausa foi registrada no passo 4 e o passo 4 é o primeiro depois dela
    assert_eq!(eventos[pausa_ev].1, Some(4));
    assert_eq!(eventos[retomada + 1].1, Some(4));
    assert_eq!(
        nav.digitados.iter().filter(|(s, _)| s == "#senha").count(),
        1
    );
    assert_eq!(
        r.banco.historico(None, 1).unwrap()[0].estado.texto(),
        "concluida"
    );
}

#[test]
fn ro_03_passo_explicito_pausa_e_continua_no_proximo() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let mut passos = login(true);
    passos.push(Passo::PausaParaUsuario {
        motivo: "confirme no aplicativo do banco".into(),
    });
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: None,
    });
    let pausa = PausaFalsa::com(&[Decisao::Continuar]);
    let r = rodar(&mut nav, passos, &pausa, d.path());
    assert_eq!(r.rel.fim, Fim::Concluida);
    assert_eq!(r.rel.pausas, 1);
    assert_eq!(
        pausa.motivos.lock().unwrap()[0],
        "confirme no aplicativo do banco"
    );
    // depois da pausa só o download correu: o passo da pausa não foi refeito
    let ultimas: Vec<&String> = nav.chamadas.iter().rev().take(1).collect();
    assert!(ultimas[0].starts_with("baixar"));
    assert_eq!(r.rel.baixados, 1);
}

#[test]
fn ro_03_cancelar_e_expirar_encerram_sem_baixar() {
    for (decisao, esperado) in [
        (Decisao::Cancelar, "cancelada"),
        (Decisao::Expirou, "falhou"),
    ] {
        let d = tempfile::tempdir().unwrap();
        let mut nav = site(false);
        let passos = vec![
            Passo::Navegar {
                url: "http://127.0.0.1:1/login".into(),
            },
            Passo::PausaParaUsuario {
                motivo: "CAPTCHA".into(),
            },
            Passo::Clicar {
                seletor: "#entrar".into(),
            },
        ];
        let r = rodar(&mut nav, passos, &PausaFalsa::com(&[decisao]), d.path());
        assert_eq!(
            r.banco.historico(None, 1).unwrap()[0].estado.texto(),
            esperado
        );
        assert!(!nav.chamadas.iter().any(|c| c.starts_with("clicar")));
    }
}

#[test]
fn ro_05_o_download_vira_achado_com_origem_roteiro_e_nao_duplica() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let mut passos = login(true);
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: Some("Extrato Outubro".into()),
    });
    passos.push(Passo::LerTabela {
        seletor: "#extratos".into(),
        nome: "extratos".into(),
    });
    let id = banco
        .criar_roteiro("meu banco", "", "", None, &passos, OrigemDaVersao::Gravacao)
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let nao = AtomicBool::new(false);
    let pausa = PausaFalsa::default();
    let mut totais = Vec::new();
    for _ in 0..2 {
        let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
        let mut destino = NoInventario::novo(&conn, &cofre, "meu banco", execucao);
        let mut nav = site(false);
        let rel = {
            let mut c = Contexto {
                navegador: &mut nav,
                senhas: &Senhas,
                pausa: &pausa,
                destino: &mut destino,
                banco: &banco,
                pasta: d.path().join(format!("b{execucao}")),
                opcoes: Opcoes {
                    espera: Duration::from_millis(30),
                    ..Opcoes::default()
                },
                cancelar: &nao,
            };
            executar(&mut c, execucao, &roteiro).unwrap()
        };
        destino.fechar("concluida").unwrap();
        assert_eq!(rel.fim, Fim::Concluida);
        totais.push((rel.baixados, rel.novos, execucao));
    }
    // 1ª vez: PDF e CSV novos; 2ª vez: os mesmos conteúdos, nada novo
    assert_eq!((totais[0].0, totais[0].1), (2, 2));
    assert_eq!((totais[1].0, totais[1].1), (2, 0));
    let achados: i64 = conn
        .query_row("SELECT count(*) FROM achado", [], |l| l.get(0))
        .unwrap();
    let origens: i64 = conn
        .query_row("SELECT count(*) FROM origem", [], |l| l.get(0))
        .unwrap();
    assert_eq!((achados, origens), (2, 2));
    let chave: String = conn
        .query_row(
            "SELECT origin_key FROM origem WHERE filename LIKE 'Extrato%'",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert_eq!(
        chave,
        format!("roteiro:meu banco/{}/Extrato Outubro.pdf", totais[0].2)
    );
    let (tipo, fonte_tipo): (String, String) = conn
        .query_row(
            "SELECT o.origin_type, f.tipo FROM origem o JOIN fonte f ON f.id = o.fonte_id LIMIT 1",
            [],
            |l| Ok((l.get(0)?, l.get(1)?)),
        )
        .unwrap();
    assert_eq!((tipo.as_str(), fonte_tipo.as_str()), ("roteiro", "conexao"));
    // o arquivo foi para o cofre
    let caminho: String = conn
        .query_row(
            "SELECT caminho_acervo FROM achado WHERE extensao = 'pdf'",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert!(cofre.join(caminho).exists());
    let _ = FORMATO;
}

#[test]
fn ro_01_a_senha_so_vai_para_o_host_em_que_foi_aprovada() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("outro-site.example".into()),
        },
    ];
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    let Fim::Falhou { passo: 1, motivo } = &r.rel.fim else {
        panic!("{:?}", r.rel.fim)
    };
    assert!(motivo.contains("outro-site.example"));
    assert!(nav.digitados.is_empty(), "nada foi digitado");
    // o mesmo passo com o host certo digita
    let mut nav = site(false);
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("127.0.0.1".into()),
        },
    ];
    let d2 = tempfile::tempdir().unwrap();
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d2.path());
    assert_eq!(r.rel.fim, Fim::Concluida);
    assert_eq!(nav.digitados.len(), 1);
}

#[test]
fn ro_02_cair_numa_pagina_de_pagamento_por_redirecionamento_aborta() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    // um link inocente cujo servidor redireciona para /pagamento/…
    nav.pagina_nomeada(
        "destino",
        sbf_roteiros::Pagina {
            url: "http://127.0.0.1:1/pagamento/confirmar".into(),
            ..sbf_roteiros::Pagina::default()
        },
    );
    nav.elementos.get_mut("#ver").unwrap().leva_a = Some("destino".into());
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::Clicar {
            seletor: "#entrar".into(),
        },
        Passo::Clicar {
            seletor: "#ver".into(),
        },
        Passo::Baixar {
            seletor: "#baixar".into(),
            nome: None,
        },
    ];
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    assert!(
        matches!(r.rel.fim, Fim::Bloqueada { passo: 2, .. }),
        "{:?}",
        r.rel.fim
    );
    assert!(!nav.chamadas.iter().any(|c| c.starts_with("baixar")));
}

#[test]
fn ro_02_o_executor_marca_a_fase_de_login_ao_digitar_e_a_encerra_no_clique() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
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
    ];
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    assert_eq!(r.rel.fim, Fim::Concluida);
    assert_eq!(
        nav.fases_de_login,
        vec![
            None, // navegar
            Some("127.0.0.1".to_owned()),
            Some("127.0.0.1".to_owned()),
            None, // o clique que envia o login
        ]
    );
}

/// Roda um roteiro **sem passar pela validação do banco** (defesa em profundidade: o executor
/// recusa sozinho o que o banco já recusaria, caso um JSON antigo ou editado à mão chegue).
fn rodar_cru(nav: &mut NavegadorFalso, passos: Vec<Passo>, pasta: &std::path::Path) -> Resultado {
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_roteiro("cru", "", "", None, &[], OrigemDaVersao::Manual)
        .unwrap();
    let (vid, _, _) = banco.versao_ativa(id).unwrap();
    let execucao = banco.iniciar_execucao(id, vid, "teste").unwrap();
    let roteiro = Roteiro {
        formato: FORMATO,
        nome: "cru".into(),
        descricao: String::new(),
        passos,
    };
    let (cofre, conn) = inventario(pasta);
    let mut destino = NoInventario::novo(&conn, &cofre, "cru", execucao);
    let nao = AtomicBool::new(false);
    let pausa = PausaFalsa::default();
    let rel = {
        let mut c = Contexto {
            navegador: nav,
            senhas: &Senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: pasta.join("baixados"),
            opcoes: Opcoes {
                espera: Duration::from_millis(30),
                download: Duration::from_secs(1),
                intervalo: Duration::from_millis(5),
            },
            cancelar: &nao,
        };
        executar(&mut c, execucao, &roteiro).unwrap()
    };
    Resultado {
        rel,
        banco,
        execucao,
        roteiro,
    }
}

fn senha_em(seletor: &str, host: Option<&str>) -> Vec<Passo> {
    vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherSenha {
            seletor: seletor.into(),
            rotulo: "banco-ficticio".into(),
            host: host.map(str::to_owned),
        },
    ]
}

#[test]
fn ro_01_passo_de_senha_ou_cpf_sem_site_aprovado_nao_digita_nada() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let r = rodar_cru(&mut nav, senha_em("#senha", None), d.path());
    assert!(
        matches!(r.rel.fim, Fim::Falhou { passo: 1, .. }),
        "{:?}",
        r.rel.fim
    );
    let mut nav = site(false);
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherCpf {
            seletor: "#cpf".into(),
            host: None,
        },
    ];
    let d2 = tempfile::tempdir().unwrap();
    let r = rodar_cru(&mut nav, passos, d2.path());
    assert!(matches!(r.rel.fim, Fim::Falhou { passo: 1, .. }));
    assert!(nav.digitados.is_empty(), "nada foi digitado");
}

#[test]
fn ro_01_a_senha_so_vai_num_campo_de_senha_e_o_cpf_so_num_campo_de_texto() {
    let d = tempfile::tempdir().unwrap();
    // senha num campo de texto: recusado
    let mut nav = site(false);
    let r = rodar_cru(&mut nav, senha_em("#cpf", Some("127.0.0.1")), d.path());
    assert!(
        matches!(r.rel.fim, Fim::Falhou { passo: 1, .. }),
        "{:?}",
        r.rel.fim
    );
    assert!(nav.digitados.is_empty());
    // CPF num campo de senha: recusado
    let mut nav = site(false);
    let d2 = tempfile::tempdir().unwrap();
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherCpf {
            seletor: "#senha".into(),
            host: Some("127.0.0.1".into()),
        },
    ];
    let r = rodar_cru(&mut nav, passos, d2.path());
    assert!(matches!(r.rel.fim, Fim::Falhou { passo: 1, .. }));
    assert!(nav.digitados.is_empty());
    // CPF num botão: recusado
    let mut nav = site(false);
    let d3 = tempfile::tempdir().unwrap();
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherCpf {
            seletor: "#entrar".into(),
            host: Some("127.0.0.1".into()),
        },
    ];
    let r = rodar_cru(&mut nav, passos, d3.path());
    assert!(matches!(r.rel.fim, Fim::Falhou { passo: 1, .. }));
    assert!(nav.digitados.is_empty());
}

#[test]
fn ro_01_segredo_so_vai_em_https_salvo_no_proprio_computador() {
    // http num site de verdade: recusado, mesmo com o host certo
    let mut nav = site(false);
    nav.rotas
        .insert("http://banco.exemplo/login".into(), "login".into());
    nav.paginas.get_mut("login").unwrap().url = "http://banco.exemplo/login".into();
    let passos = vec![
        Passo::Navegar {
            url: "http://banco.exemplo/login".into(),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("banco.exemplo".into()),
        },
    ];
    let d = tempfile::tempdir().unwrap();
    let r = rodar_cru(&mut nav, passos, d.path());
    let Fim::Falhou { passo: 1, motivo } = &r.rel.fim else {
        panic!("{:?}", r.rel.fim)
    };
    assert!(motivo.contains("https"), "{motivo}");
    assert!(nav.digitados.is_empty());
    // https: digita
    let mut nav = site(false);
    nav.rotas
        .insert("https://banco.exemplo/login".into(), "login".into());
    nav.paginas.get_mut("login").unwrap().url = "https://banco.exemplo/login".into();
    let passos = vec![
        Passo::Navegar {
            url: "https://banco.exemplo/login".into(),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("banco.exemplo".into()),
        },
    ];
    let d = tempfile::tempdir().unwrap();
    let r = rodar_cru(&mut nav, passos, d.path());
    assert_eq!(r.rel.fim, Fim::Concluida);
    assert_eq!(nav.digitados.len(), 1);
}

#[test]
fn limpar_texto_tira_o_segredo_e_a_forma_percent_encoded() {
    use sbf_chaveiro::Segredo;
    let s = [Segredo::novo(SENHA), Segredo::novo(CPF)];
    let t = format!(
        "a={}&b=SenhaFicticia%234821 e o cpf {CPF} ou 12345678909",
        SENHA
    );
    let limpo = sbf_roteiros::executor::limpar_texto(&t, &s);
    assert!(!limpo.contains(SENHA) && !limpo.contains("%234821"));
    assert!(!limpo.contains(CPF) && !limpo.contains("12345678909"));
}

#[test]
fn ro_02_o_executor_so_navega_para_http_ou_https() {
    for url in [
        "file:///etc/hosts",
        "javascript:alert(1)",
        "data:text/html,oi",
        "chrome://settings",
    ] {
        let d = tempfile::tempdir().unwrap();
        let mut nav = site(false);
        let r = rodar_cru(&mut nav, vec![Passo::Navegar { url: url.into() }], d.path());
        assert!(
            matches!(r.rel.fim, Fim::Falhou { passo: 0, .. }),
            "{url}: {:?}",
            r.rel.fim
        );
        assert!(nav.chamadas.is_empty(), "{url}: o navegador foi chamado");
    }
}

#[test]
fn o_download_e_o_csv_temporarios_somem_depois_de_ir_ao_cofre() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let mut passos = login(true);
    passos.push(Passo::Baixar {
        seletor: "#baixar".into(),
        nome: Some("Extrato".into()),
    });
    passos.push(Passo::LerTabela {
        seletor: "#extratos".into(),
        nome: "extratos".into(),
    });
    let r = rodar(&mut nav, passos, &PausaFalsa::default(), d.path());
    assert_eq!((r.rel.fim.clone(), r.rel.baixados), (Fim::Concluida, 2));
    let sobras: Vec<_> = std::fs::read_dir(d.path().join("baixados"))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    assert!(sobras.is_empty(), "sobrou: {sobras:?}");
    // e foram para o cofre
    let achados: i64 = rusqlite::Connection::open(d.path().join("cofre/inventario.sqlite"))
        .unwrap()
        .query_row(
            "SELECT count(*) FROM achado WHERE caminho_acervo IS NOT NULL",
            [],
            |l| l.get(0),
        )
        .unwrap();
    assert_eq!(achados, 2);
}
