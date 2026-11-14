//! RO-04 (e RO-01/RO-02 na IA): gravação guiada, execução sem IA e conserto só após aprovação.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use comum::{CPF, PausaFalsa, SENHA, Senhas, clicavel, inventario, site};
use sbf_chaveiro::Segredo;
use sbf_ia::{Consumo, Falso, NivelDeConfianca, Resposta};
use sbf_roteiros::banco::{BancoDeRoteiros, OrigemDaVersao};
use sbf_roteiros::executor::{Contexto, Fim, Opcoes};
use sbf_roteiros::gravacao::{
    Aprovacao, Aprovador, FimDaGravacao, Guia, Proposta, executar_com_conserto, gravar_guiado,
};
use sbf_roteiros::inventario::NoInventario;
use sbf_roteiros::passo::Passo;
use serde_json::json;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

fn resposta(j: serde_json::Value) -> sbf_ia::ResultadoDeIa<Resposta> {
    Ok(Resposta {
        json: j,
        consumo: Consumo::default(),
        modelo: "falso-1".into(),
    })
}

fn passo(p: serde_json::Value, explicacao: &str) -> sbf_ia::ResultadoDeIa<Resposta> {
    resposta(json!({"acao": "passo", "passo": p, "explicacao": explicacao}))
}

struct AceitaTudo(Vec<String>);
impl Aprovador for AceitaTudo {
    fn decidir(&mut self, p: &Proposta) -> Aprovacao {
        if let Proposta::Passo { passo, .. } = p {
            self.0.push(passo.descricao());
        }
        Aprovacao::Aceitar
    }
}

#[derive(Default)]
struct Memoria(std::sync::Mutex<Vec<sbf_ia::Saida>>);
impl sbf_ia::Registro for Memoria {
    fn registrar_saida(&self, s: &sbf_ia::Saida) -> sbf_ia::ResultadoDeIa<()> {
        self.0.lock().unwrap().push(s.clone());
        Ok(())
    }
    fn ler_cache(&self, _: &str) -> sbf_ia::ResultadoDeIa<Option<serde_json::Value>> {
        Ok(None)
    }
    fn gravar_cache(&self, _: &str, _: &serde_json::Value) -> sbf_ia::ResultadoDeIa<()> {
        Ok(())
    }
}

fn opcoes() -> Opcoes {
    Opcoes {
        espera: Duration::from_millis(30),
        download: Duration::from_secs(1),
        intervalo: Duration::from_millis(5),
    }
}

#[test]
fn ro_04_grava_com_ia_executa_sem_ia_e_conserta_so_apos_aprovacao() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_rascunho(
            "meu banco",
            "",
            "http://127.0.0.1:1/login",
            Some("banco-ficticio"),
        )
        .unwrap();
    let (vid, _, _) = banco.versao_ativa(id).unwrap();
    let nao = AtomicBool::new(false);
    let pausa = PausaFalsa::default();

    // ---------- 1. gravação guiada ----------
    let ia = Falso::com_roteiro(vec![
        passo(json!({"tipo": "preencher_cpf", "seletor": "#cpf"}), "CPF"),
        // a IA tenta clicar em Pagar: os filtros recusam e ela recebe o motivo
        passo(
            json!({"tipo": "clicar", "seletor": "#pagar"}),
            "tentativa errada",
        ),
        // e inventa um rótulo de senha: recusado também
        passo(
            json!({"tipo": "preencher_senha", "seletor": "#senha", "rotulo": "outro-banco"}),
            "senha",
        ),
        passo(
            json!({"tipo": "preencher_senha", "seletor": "#senha", "rotulo": "banco-ficticio"}),
            "senha",
        ),
        passo(json!({"tipo": "clicar", "seletor": "#entrar"}), "entrar"),
        passo(
            json!({"tipo": "baixar", "seletor": "#baixar", "nome": "Extrato"}),
            "baixar",
        ),
        resposta(json!({"acao": "fim"})),
    ]);
    // a página do painel traz o botão Pagar; a do login não: para testar o filtro de bloqueio
    // o "#pagar" precisa estar na página em que a IA o propõe (login com botão escondido)
    let mut nav = site(false);
    nav.paginas
        .get_mut("login")
        .unwrap()
        .elementos
        .push(clicavel("#pagar", "Pagar"));
    let registro = Memoria::default();
    let guia = Guia {
        provedor: &ia,
        registro: Some(&registro),
        destinatario: "roteiros".into(),
        nivel: NivelDeConfianca::Mascarado,
        rotulos_de_senha: vec!["banco-ficticio".into()],
        tem_cpf: true,
        // a senha está em memória; se a página a repetisse, sairia como ***
        segredos: vec![Segredo::novo(SENHA), Segredo::novo(CPF)],
    };
    nav.paginas.get_mut("login").unwrap().texto = format!("Acesse. Seu CPF {CPF} e senha {SENHA}");
    let exec_g = banco.iniciar_execucao(id, vid, "gravacao").unwrap();
    let mut destino = NoInventario::novo(&conn, &cofre, "meu banco", exec_g);
    let gravado = {
        let mut c = Contexto {
            navegador: &mut nav,
            senhas: &Senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: d.path().join("g"),
            opcoes: opcoes(),
            cancelar: &nao,
        };
        let mut ap = AceitaTudo(Vec::new());
        gravar_guiado(
            &guia,
            &mut c,
            exec_g,
            "baixar o extrato do mês",
            "http://127.0.0.1:1/login",
            &mut ap,
            20,
        )
        .unwrap()
    };
    assert_eq!(gravado.fim, FimDaGravacao::Cumprido);
    assert_eq!(
        gravado.passos,
        vec![
            Passo::Navegar {
                url: "http://127.0.0.1:1/login".into()
            },
            Passo::PreencherCpf {
                seletor: "#cpf".into(),
                host: Some("127.0.0.1".into())
            },
            Passo::PreencherSenha {
                seletor: "#senha".into(),
                rotulo: "banco-ficticio".into(),
                host: Some("127.0.0.1".into())
            },
            Passo::Clicar {
                seletor: "#entrar".into()
            },
            Passo::Baixar {
                seletor: "#baixar".into(),
                nome: Some("Extrato".into())
            },
        ]
    );
    assert!(
        !nav.chamadas.iter().any(|c| c.contains("#pagar")),
        "o filtro barrou Pagar antes de executar"
    );
    // RO-01: nada do que foi à IA tem a senha nem o CPF; só o rótulo
    for p in ia.pedidos() {
        let tudo = format!("{}\n{}", p.instrucao, p.entrada);
        assert!(
            !tudo.contains(SENHA) && !tudo.contains(CPF),
            "vazou para a IA"
        );
        assert_eq!(p.prompt_versao, "roteiro-gravar-v1");
    }
    let recusas = ia
        .pedidos()
        .iter()
        .filter(|p| p.entrada.contains("proposta anterior foi recusada"))
        .count();
    assert_eq!(
        recusas, 2,
        "o Pagar e o rótulo inventado voltaram à IA com o motivo"
    );
    assert!(
        ia.pedidos().iter().any(|p| p.entrada.contains("***")),
        "a página repetida foi limpa"
    );
    // o registro do que saiu guarda tamanho, nunca conteúdo
    assert!(
        registro
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|s| s.destinatario == "roteiros" && s.bytes_enviados > 0)
    );
    banco.concluir_gravacao(id, &gravado.passos).unwrap();
    banco
        .finalizar(
            exec_g,
            sbf_roteiros::EstadoDaExecucao::Concluida,
            None,
            0,
            0,
        )
        .unwrap();
    destino.fechar("concluida").unwrap();
    let json_gravado = serde_json::to_string(&banco.versao_ativa(id).unwrap().2).unwrap();
    assert!(!json_gravado.contains(SENHA) && !json_gravado.contains(CPF));

    // ---------- 2. execução sem IA ----------
    let chamadas_antes = ia.chamadas();
    let roteiro = banco.versao_ativa(id).unwrap().2;
    let mut nav = site(false);
    let execucao = banco.iniciar_execucao(id, vid, "janela").unwrap();
    let mut destino = NoInventario::novo(&conn, &cofre, "meu banco", execucao);
    let rel = {
        let mut c = Contexto {
            navegador: &mut nav,
            senhas: &Senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: d.path().join("e1"),
            opcoes: opcoes(),
            cancelar: &nao,
        };
        executar_com_conserto(&mut c, id, execucao, &roteiro, None).unwrap()
    };
    assert_eq!(rel.fim, Fim::Concluida);
    assert_eq!(
        ia.chamadas(),
        chamadas_antes,
        "rodar o roteiro gravado não chama a IA"
    );

    // ---------- 3. o site muda: a IA propõe, mas só vale depois da aprovação ----------
    let mut nav = site(false);
    // o botão passou a se chamar #baixar-v2
    let el = nav.elementos.remove("#baixar").unwrap();
    nav.elementos.insert("#baixar-v2".into(), el);
    let p = nav.paginas.get_mut("painel").unwrap();
    p.elementos
        .iter_mut()
        .find(|e| e.seletor == "#baixar")
        .unwrap()
        .seletor = "#baixar-v2".into();
    let conserto = Falso::com_roteiro(vec![resposta(json!({
        "acao": "conserto",
        "explicacao": "o botão agora é #baixar-v2",
        "passos": [{"tipo": "baixar", "seletor": "#baixar-v2", "nome": "Extrato"}]
    }))]);
    let guia2 = Guia {
        provedor: &conserto,
        registro: None,
        destinatario: "roteiros".into(),
        nivel: NivelDeConfianca::Mascarado,
        rotulos_de_senha: vec!["banco-ficticio".into()],
        tem_cpf: true,
        segredos: vec![],
    };
    let execucao = banco.iniciar_execucao(id, vid, "agenda").unwrap();
    let mut destino = NoInventario::novo(&conn, &cofre, "meu banco", execucao);
    let rel = {
        let mut c = Contexto {
            navegador: &mut nav,
            senhas: &Senhas,
            pausa: &pausa,
            destino: &mut destino,
            banco: &banco,
            pasta: d.path().join("e2"),
            opcoes: opcoes(),
            cancelar: &nao,
        };
        executar_com_conserto(&mut c, id, execucao, &roteiro, Some(&guia2)).unwrap()
    };
    assert!(
        matches!(rel.fim, Fim::Falhou { passo: 4, .. }),
        "{:?}",
        rel.fim
    );
    assert_eq!(conserto.chamadas(), 1);
    assert_eq!(conserto.pedidos()[0].prompt_versao, "roteiro-consertar-v1");
    // a proposta existe, mas a versão ativa continua a antiga
    let proposta = banco.proposta(id).unwrap().expect("há proposta");
    assert_eq!(proposta.passos.len(), 5);
    assert_eq!(banco.versao_ativa(id).unwrap().1, 1);
    assert_eq!(banco.versao_ativa(id).unwrap().2.passos, roteiro.passos);
    // sem aprovação, o roteiro continua falhando no site novo
    let mut nav2 = site(false);
    let el = nav2.elementos.remove("#baixar").unwrap();
    nav2.elementos.insert("#baixar-v2".into(), el);
    nav2.paginas
        .get_mut("painel")
        .unwrap()
        .elementos
        .iter_mut()
        .find(|e| e.seletor == "#baixar")
        .unwrap()
        .seletor = "#baixar-v2".into();
    let rodar =
        |nav: &mut sbf_roteiros::falso::NavegadorFalso, r: &sbf_roteiros::Roteiro, pasta: &str| {
            let e = banco.iniciar_execucao(id, vid, "janela").unwrap();
            let mut destino = NoInventario::novo(&conn, &cofre, "meu banco", e);
            let mut c = Contexto {
                navegador: nav,
                senhas: &Senhas,
                pausa: &pausa,
                destino: &mut destino,
                banco: &banco,
                pasta: d.path().join(pasta),
                opcoes: opcoes(),
                cancelar: &nao,
            };
            executar_com_conserto(&mut c, id, e, r, None).unwrap().fim
        };
    assert!(matches!(
        rodar(&mut nav2, &banco.versao_ativa(id).unwrap().2, "e3"),
        Fim::Falhou { .. }
    ));
    // o usuário aprova: o roteiro novo roda no site novo
    banco.aprovar_proposta(id).unwrap();
    assert_eq!(banco.versao_ativa(id).unwrap().1, 2);
    let mut nav3 = site(false);
    let el = nav3.elementos.remove("#baixar").unwrap();
    nav3.elementos.insert("#baixar-v2".into(), el);
    nav3.paginas
        .get_mut("painel")
        .unwrap()
        .elementos
        .iter_mut()
        .find(|e| e.seletor == "#baixar")
        .unwrap()
        .seletor = "#baixar-v2".into();
    assert_eq!(
        rodar(&mut nav3, &banco.versao_ativa(id).unwrap().2, "e4"),
        Fim::Concluida
    );
    let _ = OrigemDaVersao::Conserto;
}

#[test]
fn ro_04_conserto_que_propoe_acao_perigosa_e_recusado() {
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let passos = vec![Passo::Clicar {
        seletor: "#baixar".into(),
    }];
    let id = banco
        .criar_roteiro("r", "", "", None, &passos, OrigemDaVersao::Gravacao)
        .unwrap();
    let roteiro = banco.versao_ativa(id).unwrap().2;
    let mut nav = site(false);
    nav.atual = "painel".into();
    let pagina = sbf_roteiros::Navegador::pagina(&mut nav).unwrap();
    let ia = Falso::com_roteiro(vec![resposta(json!({
        "acao": "conserto", "explicacao": "x",
        "passos": [{"tipo": "clicar", "seletor": "#pagar"}]
    }))]);
    let g = Guia {
        provedor: &ia,
        registro: None,
        destinatario: "roteiros".into(),
        nivel: NivelDeConfianca::Mascarado,
        rotulos_de_senha: vec![],
        tem_cpf: false,
        segredos: vec![],
    };
    let r = g.propor_conserto(&roteiro, 0, "sumiu", &pagina);
    assert!(
        matches!(r, Err(sbf_roteiros::ErroDeRoteiro::Bloqueado(_))),
        "{r:?}"
    );
}

#[test]
fn ro_04_resposta_fora_do_esquema_e_erro() {
    let ia = Falso::com_roteiro(vec![resposta(json!({"acao": "voar"}))]);
    let g = Guia {
        provedor: &ia,
        registro: None,
        destinatario: "roteiros".into(),
        nivel: NivelDeConfianca::Mascarado,
        rotulos_de_senha: vec![],
        tem_cpf: false,
        segredos: vec![],
    };
    let r = g.propor_proximo("x", &[], &sbf_roteiros::Pagina::default(), None);
    assert!(r.is_err());
}

fn guia_de<'a>(ia: &'a Falso) -> Guia<'a> {
    Guia {
        provedor: ia,
        registro: None,
        destinatario: "roteiros".into(),
        nivel: NivelDeConfianca::Mascarado,
        rotulos_de_senha: vec!["banco-ficticio".into()],
        tem_cpf: true,
        // como no `sbf` e na janela: o chamador não conhece nenhum segredo de antemão
        segredos: vec![],
    }
}

fn gravar(
    nav: &mut sbf_roteiros::falso::NavegadorFalso,
    guia: &Guia<'_>,
    pasta: &std::path::Path,
) -> sbf_roteiros::erro::Resultado<sbf_roteiros::gravacao::Gravado> {
    let (cofre, conn) = inventario(pasta);
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let id = banco
        .criar_rascunho("g", "", "http://127.0.0.1:1/login", Some("banco-ficticio"))
        .unwrap();
    let (vid, _, _) = banco.versao_ativa(id).unwrap();
    let exec = banco.iniciar_execucao(id, vid, "gravacao").unwrap();
    let mut destino = NoInventario::novo(&conn, &cofre, "g", exec);
    let nao = AtomicBool::new(false);
    let pausa = PausaFalsa::default();
    let mut c = Contexto {
        navegador: nav,
        senhas: &Senhas,
        pausa: &pausa,
        destino: &mut destino,
        banco: &banco,
        pasta: pasta.join("g"),
        opcoes: opcoes(),
        cancelar: &nao,
    };
    let mut ap = AceitaTudo(Vec::new());
    gravar_guiado(
        guia,
        &mut c,
        exec,
        "baixar",
        "http://127.0.0.1:1/login",
        &mut ap,
        20,
    )
}

#[test]
fn ro_01_a_ia_nao_leva_a_senha_em_campo_que_nao_e_de_senha() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    // um campo de busca na página de login
    nav.paginas.get_mut("login").unwrap().formularios[0]
        .campos
        .push(sbf_roteiros::pagina::Campo {
            seletor: "#busca".into(),
            tipo: "text".into(),
            nome: "busca".into(),
            ..Default::default()
        });
    let mut e = sbf_roteiros::falso::Elemento::com_texto("");
    e.candidato.eh_campo = true;
    e.candidato.tipo = "text".into();
    nav.elementos.insert("#busca".into(), e);
    let ia = Falso::com_roteiro(vec![
        // a IA propõe a senha do chaveiro num campo de busca
        passo(
            json!({"tipo": "preencher_senha", "seletor": "#busca", "rotulo": "banco-ficticio"}),
            "senha em #busca",
        ),
        // e o CPF num campo de senha
        passo(json!({"tipo": "preencher_cpf", "seletor": "#senha"}), "cpf"),
        resposta(json!({"acao": "fim"})),
    ]);
    let g = gravar(&mut nav, &guia_de(&ia), d.path()).unwrap();
    assert_eq!(g.passos.len(), 1, "só o Navegar inicial: {:?}", g.passos);
    assert!(nav.digitados.is_empty(), "nada foi digitado");
    let pedidos = ia.pedidos();
    assert!(
        pedidos[1]
            .entrada
            .contains("proposta anterior foi recusada"),
        "a recusa volta para a IA"
    );
}

#[test]
fn ro_01_senha_usada_na_gravacao_nao_aparece_no_pedido_seguinte_nem_na_url() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    // depois do login, o site "repete" a senha no texto e na URL (GET com a senha)
    {
        let p = nav.paginas.get_mut("painel").unwrap();
        p.url = format!("http://127.0.0.1:1/painel?s=SenhaFicticia%234821&k={SENHA}");
        p.texto = format!("Olá {CPF}, sua senha {SENHA} está certa");
    }
    let ia = Falso::com_roteiro(vec![
        passo(json!({"tipo": "preencher_cpf", "seletor": "#cpf"}), "cpf"),
        passo(
            json!({"tipo": "preencher_senha", "seletor": "#senha", "rotulo": "banco-ficticio"}),
            "senha",
        ),
        passo(json!({"tipo": "clicar", "seletor": "#entrar"}), "entrar"),
        resposta(json!({"acao": "fim"})),
    ]);
    let g = gravar(&mut nav, &guia_de(&ia), d.path()).unwrap();
    assert_eq!(g.fim, FimDaGravacao::Cumprido);
    let pedidos = ia.pedidos();
    assert_eq!(pedidos.len(), 4);
    for p in &pedidos {
        let tudo = format!("{}\n{}", p.instrucao, p.entrada);
        assert!(!tudo.contains(SENHA), "senha no pedido: {tudo}");
        assert!(!tudo.contains("%234821"), "senha codificada no pedido");
        assert!(!tudo.contains(CPF), "CPF no pedido");
    }
    assert!(pedidos[3].entrada.contains("***"), "a página foi limpa");
}

#[test]
fn ro_01_conserto_nao_leva_a_senha_que_a_execucao_usou() {
    let d = tempfile::tempdir().unwrap();
    let (cofre, conn) = inventario(d.path());
    let banco = BancoDeRoteiros::em_memoria().unwrap();
    let passos = vec![
        Passo::Navegar {
            url: "http://127.0.0.1:1/login".into(),
        },
        Passo::PreencherSenha {
            seletor: "#senha".into(),
            rotulo: "banco-ficticio".into(),
            host: Some("127.0.0.1".into()),
        },
        Passo::Clicar {
            seletor: "#entrar".into(),
        },
        Passo::Baixar {
            seletor: "#some".into(),
            nome: None,
        },
    ];
    let id = banco
        .criar_roteiro("r", "", "", None, &passos, OrigemDaVersao::Manual)
        .unwrap();
    let (vid, _, roteiro) = banco.versao_ativa(id).unwrap();
    let mut nav = site(false);
    {
        let p = nav.paginas.get_mut("painel").unwrap();
        p.url = format!("http://127.0.0.1:1/painel?s={SENHA}");
        p.texto = format!("sua senha {SENHA}");
    }
    let ia = Falso::com_roteiro(vec![resposta(json!({"acao": "desistir"}))]);
    let execucao = banco.iniciar_execucao(id, vid, "agenda").unwrap();
    let mut destino = NoInventario::novo(&conn, &cofre, "r", execucao);
    let nao = AtomicBool::new(false);
    let pausa = PausaFalsa::default();
    let mut c = Contexto {
        navegador: &mut nav,
        senhas: &Senhas,
        pausa: &pausa,
        destino: &mut destino,
        banco: &banco,
        pasta: d.path().join("e"),
        opcoes: opcoes(),
        cancelar: &nao,
    };
    let g = guia_de(&ia);
    let rel = executar_com_conserto(&mut c, id, execucao, &roteiro, Some(&g)).unwrap();
    assert!(
        matches!(rel.fim, Fim::Falhou { passo: 3, .. }),
        "{:?}",
        rel.fim
    );
    let pedidos = ia.pedidos();
    assert_eq!(pedidos.len(), 1);
    let tudo = format!("{}\n{}", pedidos[0].instrucao, pedidos[0].entrada);
    assert!(!tudo.contains(SENHA), "senha no pedido de conserto: {tudo}");
    assert!(tudo.contains("***"));
}

#[test]
fn ro_02_a_ia_so_navega_para_http_ou_https() {
    let mut nav = site(false);
    let pagina = sbf_roteiros::Navegador::pagina(&mut nav).unwrap();
    for url in [
        "javascript:alert(1)",
        "file:///etc/passwd",
        "data:text/html,<script>1</script>",
        "chrome://settings",
        "about:blank",
        "view-source:http://127.0.0.1:1/",
        "ftp://x.example/a",
    ] {
        let ia = Falso::com_roteiro(vec![passo(json!({"tipo": "navegar", "url": url}), "ir")]);
        let r = guia_de(&ia).propor_proximo("x", &[], &pagina, None);
        assert!(
            matches!(r, Err(sbf_roteiros::ErroDeRoteiro::Invalido(_))),
            "{url}: {r:?}"
        );
    }
    let ia = Falso::com_roteiro(vec![passo(
        json!({"tipo": "navegar", "url": "https://banco.example/extratos?mes=10"}),
        "ir",
    )]);
    let r = guia_de(&ia)
        .propor_proximo("x", &[], &pagina, None)
        .unwrap();
    assert!(matches!(r, Proposta::Passo { .. }));
}

#[test]
fn ro_04_a_proposta_de_clicar_mostra_o_que_ha_no_alvo() {
    let mut nav = site(false);
    nav.atual = "login".into();
    let pagina = sbf_roteiros::Navegador::pagina(&mut nav).unwrap();
    let ia = Falso::com_roteiro(vec![passo(
        json!({"tipo": "clicar", "seletor": "#entrar"}),
        "entrar",
    )]);
    let Proposta::Passo { alvo, .. } = guia_de(&ia)
        .propor_proximo("x", &[], &pagina, None)
        .unwrap()
    else {
        panic!("esperava um passo")
    };
    assert!(
        alvo.contains("Entrar"),
        "o texto do elemento aparece: {alvo}"
    );
    assert!(alvo.contains("button"), "e a tag: {alvo}");
}

#[test]
fn ro_01_numero_longo_no_seletor_nao_vai_a_ia_e_a_resposta_volta_ao_seletor_de_verdade() {
    let d = tempfile::tempdir().unwrap();
    let mut nav = site(false);
    let p = nav.paginas.get_mut("login").unwrap();
    p.elementos.push(clicavel("#conta-12345678901", "Baixar"));
    let mut e = sbf_roteiros::falso::Elemento::com_texto("Baixar");
    e.candidato.tipo = "button".into();
    nav.elementos.insert("#conta-12345678901".into(), e);
    let ia = Falso::com_roteiro(vec![
        // a IA só conhece o seletor mascarado
        passo(json!({"tipo": "clicar", "seletor": "#conta-•••"}), "baixar"),
        resposta(json!({"acao": "fim"})),
    ]);
    let g = gravar(&mut nav, &guia_de(&ia), d.path()).unwrap();
    for p in ia.pedidos() {
        assert!(
            !p.entrada.contains("12345678901"),
            "número longo na entrada"
        );
    }
    assert_eq!(
        g.passos.last(),
        Some(&Passo::Clicar {
            seletor: "#conta-12345678901".into()
        })
    );
}

#[test]
fn ro_01_provedor_que_falha_depois_de_receber_o_pedido_deixa_a_saida_registrada() {
    use sbf_ia::ErroDeIa;
    for (erro, saidas) in [
        (ErroDeIa::Outro("tempo esgotado".into()), 1_usize),
        (ErroDeIa::Limite { espera_segundos: 5 }, 1),
        (ErroDeIa::Indisponivel("sem claude".into()), 0),
        (ErroDeIa::Teto, 0),
    ] {
        let ia = Falso::com_roteiro(vec![Err(erro)]);
        let registro = Memoria::default();
        let g = Guia {
            registro: Some(&registro),
            ..guia_de(&ia)
        };
        let r = g.propor_proximo("x", &[], &sbf_roteiros::Pagina::default(), None);
        assert!(r.is_err());
        assert_eq!(registro.0.lock().unwrap().len(), saidas);
        if saidas == 1 {
            assert!(registro.0.lock().unwrap()[0].bytes_enviados > 0);
        }
    }
}
