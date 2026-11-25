//! O roteiro escrito (Luau) sobre o navegador falso: política de hosts, bloqueio, segredo,
//! pausa, MFA por e-mail, download e agenda. Nada de rede, Chrome nem dado real
//! (`portal.exemplo.test`, "Fulano", `SenhaFicticia`).
//!
//! Fica fora do `src/` porque confere o valor de segredos com `Segredo::expor`, que a guarda
//! de `seguranca_fontes` só admite em arquivos de produção nomeados.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_navegador::falso::{Elemento, NavegadorFalso};
use bancada_navegador::luau::{self, Limites};
use bancada_navegador::pagina::{Campo, Formulario, Pagina, Tabela};
use bancada_navegador::politica::Bloqueio;
use comum::{PausaFalsa, clicavel, inventario};
use rusqlite::Connection;
use sbf_chaveiro::Segredo;
use sbf_roteiros::banco::{
    AutorEscrito, BancoDeRoteiros, Confirmacao, EstadoDaExecucao, EstadoEscrito,
};
use sbf_roteiros::banco::{ResultadoDoTeste, VersaoEscrita};
use sbf_roteiros::codigo_email::{CaixaDeEmail, ErroDeCodigo, MensagemCurta, Relogio};
use sbf_roteiros::erro::ErroDeRoteiro;
use sbf_roteiros::escrito::{Entradas, OpcoesEscrito, SbfVerbos};
use sbf_roteiros::executor::{Decisao, Fim, FonteDeSenhas, Opcoes};
use sbf_roteiros::manifesto::Manifesto;
use sbf_roteiros::servico::{
    Ambiente, RelatorioEscrito, Servicos, executar_escrito, executar_escrito_em,
    executar_escritos_devidos_com,
};
use std::cell::{Cell, RefCell};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tempfile::TempDir;
use zeroize::Zeroizing;

const SENHA: &str = "SenhaFicticia#4821";
const CPF: &str = "123.456.789-09";
const CPF_SO_DIGITOS: &str = "12345678909";
const CODIGO: &str = "482913";
const ROTULO: &str = "senha:portal-exemplo";
const REMETENTE: &str = "nao-responda@portal.exemplo.test";
const BASE: &str = "https://portal.exemplo.test";
/// 2026-11-25T01:00:00Z: 22h de 24/11 em UTC−3.
const T0: u64 = 1_795_568_400;

// ------------------------------------------------------------------------------------------
// Apoio
// ------------------------------------------------------------------------------------------

#[derive(Debug)]
struct Chaveiro;

impl FonteDeSenhas for Chaveiro {
    fn senha_de_site(&self, rotulo: &str) -> Result<Segredo, String> {
        if rotulo == ROTULO {
            Ok(Segredo::novo(SENHA))
        } else {
            Err("sem essa senha".into())
        }
    }
    fn cpf(&self) -> Result<Segredo, String> {
        Ok(Segredo::novo(CPF))
    }
}

struct RelogioFalso {
    agora: Cell<SystemTime>,
}

impl RelogioFalso {
    fn novo() -> Self {
        Self {
            agora: Cell::new(t(0)),
        }
    }
}

impl Relogio for RelogioFalso {
    fn agora(&self) -> SystemTime {
        self.agora.get()
    }
    fn esperar(&self, d: Duration) {
        self.agora.set(self.agora.get() + d);
    }
}

fn t(segundos: i64) -> SystemTime {
    if segundos >= 0 {
        UNIX_EPOCH + Duration::from_secs(T0 + u64::try_from(segundos).unwrap())
    } else {
        UNIX_EPOCH + Duration::from_secs(T0 - u64::try_from(-segundos).unwrap())
    }
}

/// A caixa de e-mail: cada mensagem tem remetente e instante (relativo a T0); só devolve as
/// do remetente pedido (como o Gmail com `from:`).
struct CaixaFalsa {
    mensagens: Vec<(&'static str, i64, String)>,
    consultas: Cell<u32>,
    remetentes: RefCell<Vec<String>>,
}

impl CaixaFalsa {
    fn com(mensagens: Vec<(&'static str, i64, String)>) -> Self {
        Self {
            mensagens,
            consultas: Cell::new(0),
            remetentes: RefCell::default(),
        }
    }
}

impl CaixaDeEmail for CaixaFalsa {
    fn mensagens_recentes(
        &self,
        remetente: &str,
        _desde: SystemTime,
    ) -> Result<Vec<MensagemCurta>, ErroDeCodigo> {
        self.consultas.set(self.consultas.get() + 1);
        self.remetentes.borrow_mut().push(remetente.to_owned());
        Ok(self
            .mensagens
            .iter()
            .filter(|(r, _, _)| *r == remetente)
            .map(|(_, quando, corpo)| MensagemCurta {
                recebida_em: t(*quando),
                corpo: Zeroizing::new(corpo.clone()),
            })
            .collect())
    }
}

fn email_com_codigo(quando: i64) -> (&'static str, i64, String) {
    (
        REMETENTE,
        quando,
        format!("Seu código de acesso\nO código é {CODIGO}. Não o compartilhe."),
    )
}

/// Tudo que uma execução precisa além do navegador.
struct Ctx {
    caixa: Option<CaixaFalsa>,
    relogio: RelogioFalso,
    pausa: PausaFalsa,
    opcoes: OpcoesEscrito,
    cancelar: AtomicBool,
}

impl Ctx {
    fn novo() -> Self {
        Self {
            caixa: None,
            relogio: RelogioFalso::novo(),
            pausa: PausaFalsa::com(&[]),
            opcoes: OpcoesEscrito::default(),
            cancelar: AtomicBool::new(false),
        }
    }
    fn com_caixa(mut self, c: CaixaFalsa) -> Self {
        self.caixa = Some(c);
        self
    }
    fn com_pausa(mut self, r: &[Decisao]) -> Self {
        self.pausa = PausaFalsa::com(r);
        self
    }
}

fn manifesto(mfa: &str, classe: &str) -> Manifesto {
    manifesto_nomeado("contracheque-exemplo", mfa, classe, false, Some(168))
}

fn manifesto_nomeado(
    nome: &str,
    mfa: &str,
    classe: &str,
    movimenta: bool,
    agenda: Option<u32>,
) -> Manifesto {
    let agenda = agenda.map_or("null".to_owned(), |h| h.to_string());
    Manifesto::de_json(&format!(
        r##"{{"nome":"{nome}","descricao":"Contracheque fictício","url_inicial":"{BASE}/entrar",
        "hosts":["portal.exemplo.test"],"rotulo_senha":"{ROTULO}","rotulo_cpf":"cpf:portal-exemplo",
        "mfa":{mfa},"movimenta_dinheiro":{movimenta},"classe":"{classe}","agenda_horas":{agenda}}}"##
    ))
    .unwrap()
}

const MFA_NENHUM: &str = r##"{"tipo":"nenhum"}"##;
const MFA_MANUAL: &str = r##"{"tipo":"manual"}"##;
const MFA_EMAIL: &str =
    r##"{"tipo":"email","remetente":"nao-responda@portal.exemplo.test","formato":"\\d{6}"}"##;

fn campo(seletor: &str, tipo: &str) -> Campo {
    Campo {
        seletor: seletor.into(),
        tipo: tipo.into(),
        ..Campo::default()
    }
}

fn elemento_de_campo(tipo: &str, senha: bool) -> Elemento {
    let mut e = Elemento::com_texto("");
    e.candidato.eh_campo = true;
    e.candidato.eh_senha = senha;
    e.candidato.tipo = tipo.into();
    e
}

/// O portal fictício. `prefixo` é o que antecede o caminho nas URLs (o host lógico, ou a base
/// real no modo teste). `entrar_leva_a` diz para onde o botão Entrar leva.
fn portal(prefixo: &str, entrar_leva_a: &str) -> NavegadorFalso {
    let mut n = NavegadorFalso::default();
    for (caminho, pagina) in [
        ("/entrar", "login"),
        ("/codigo", "codigo"),
        ("/painel", "painel"),
        ("/redireciona", "fora"),
        ("/robo", "robo"),
        ("/token", "token"),
    ] {
        n.rotas.insert(format!("{prefixo}{caminho}"), pagina.into());
    }
    n.pagina_nomeada(
        "login",
        Pagina {
            url: format!("{prefixo}/entrar"),
            titulo: "Entrar".into(),
            texto: "Acesse sua conta".into(),
            elementos: vec![clicavel("#entrar", "Entrar")],
            formularios: vec![Formulario {
                seletor: "#f".into(),
                campos: vec![campo("#cpf", "text"), campo("#senha", "password")],
                ..Formulario::default()
            }],
            ..Pagina::default()
        },
    );
    n.pagina_nomeada(
        "codigo",
        Pagina {
            url: format!("{prefixo}/codigo"),
            titulo: "Verificação".into(),
            texto: "Enviamos um código de verificação para o seu e-mail".into(),
            elementos: vec![clicavel("#validar", "Validar")],
            formularios: vec![Formulario {
                seletor: "#g".into(),
                campos: vec![campo("#codigo", "text")],
                ..Formulario::default()
            }],
            ..Pagina::default()
        },
    );
    n.pagina_nomeada(
        "painel",
        Pagina {
            url: format!("{prefixo}/painel"),
            titulo: "Painel".into(),
            texto: "Seus contracheques".into(),
            elementos: vec![
                clicavel("#titulo", "Demonstrativo de novembro"),
                clicavel("#baixar", "Baixar contracheque"),
                clicavel("#pagar", "Pagar"),
                clicavel("#atualizar", "Atualizar"),
                clicavel("#sincronizar", "Sincronizar"),
                clicavel("#cadastro", "Atualizar dados cadastrais"),
                clicavel("#atualizar-e-pagar", "Atualizar e pagar"),
                clicavel("#atualizar-destino", "Atualizar"),
                clicavel("#sair", "Sair"),
                clicavel("#continuar", "Continuar"),
                clicavel("#eco", "eco"),
            ],
            tabelas: vec![Tabela {
                seletor: "#tabela".into(),
                linhas: 2,
                cabecalho: vec!["Mês".into(), "Líquido".into()],
            }],
            ..Pagina::default()
        },
    );
    n.pagina_nomeada(
        "fora",
        Pagina {
            url: "https://outro.exemplo.test/x".into(),
            titulo: "Outro".into(),
            texto: "Outro site".into(),
            elementos: vec![clicavel("#intruso", "Entrar")],
            formularios: vec![Formulario {
                seletor: "#h".into(),
                campos: vec![
                    campo("#cpf", "text"),
                    campo("#senha", "password"),
                    campo("#codigo", "text"),
                ],
                ..Formulario::default()
            }],
            ..Pagina::default()
        },
    );
    // A página que pede CAPTCHA e, quando o usuário continua, vira o painel.
    n.pagina_nomeada(
        "mfa",
        Pagina {
            url: format!("{prefixo}/robo"),
            titulo: "Verificação".into(),
            texto: "Marque: não sou um robô (captcha)".into(),
            elementos: vec![clicavel("#validar-robo", "Continuar")],
            ..Pagina::default()
        },
    );
    n.pagina_nomeada(
        "token",
        Pagina {
            url: format!("{prefixo}/token"),
            titulo: "Verificação".into(),
            texto: "Informe o token do seu aplicativo".into(),
            elementos: vec![clicavel("#validar-token", "Validar")],
            ..Pagina::default()
        },
    );
    let mut entrar = Elemento::com_texto("Entrar");
    entrar.leva_a = Some(entrar_leva_a.into());
    n.elementos.insert("#entrar".into(), entrar);
    n.elementos
        .insert("#cpf".into(), elemento_de_campo("text", false));
    n.elementos
        .insert("#senha".into(), elemento_de_campo("password", true));
    n.elementos
        .insert("#codigo".into(), elemento_de_campo("text", false));
    let mut validar = Elemento::com_texto("Validar");
    validar.leva_a = Some("painel".into());
    n.elementos.insert("#validar".into(), validar);
    n.elementos.insert(
        "#titulo".into(),
        Elemento::com_texto("Demonstrativo de novembro"),
    );
    let mut baixar = Elemento::com_texto("Baixar contracheque");
    baixar.baixa = Some((
        "contracheque.pdf".into(),
        b"%PDF-1.4 contracheque ficticio de novembro".to_vec(),
    ));
    n.elementos.insert("#baixar".into(), baixar);
    for (sel, texto) in [
        ("#pagar", "Pagar"),
        ("#atualizar", "Atualizar"),
        ("#sincronizar", "Sincronizar"),
        ("#cadastro", "Atualizar dados cadastrais"),
        ("#atualizar-e-pagar", "Atualizar e pagar"),
        ("#continuar", "Continuar"),
        ("#eco", "eco"),
    ] {
        n.elementos.insert(sel.into(), Elemento::com_texto(texto));
    }
    let mut destino = Elemento::com_texto("Atualizar");
    destino
        .candidato
        .campos
        .push(("destino".into(), "/pagamento/confirmar".into()));
    n.elementos.insert("#atualizar-destino".into(), destino);
    let mut sair = Elemento::com_texto("Sair");
    sair.leva_a = Some("fora".into());
    n.elementos.insert("#sair".into(), sair);
    n.tabelas.insert(
        "#tabela".into(),
        vec![
            vec!["Mês".into(), "Líquido".into()],
            vec!["Novembro".into(), "R$ 1.234,56".into()],
        ],
    );
    n.atual = "login".into();
    n
}

struct Cenario {
    dir: TempDir,
    amb: Ambiente,
    versao: VersaoEscrita,
    conn: Connection,
}

fn passou() -> ResultadoDoTeste {
    ResultadoDoTeste {
        passou: true,
        resumo: "verde".into(),
    }
}

fn ambiente(dir: &TempDir) -> (Ambiente, Connection) {
    let (cofre, conn) = inventario(dir.path());
    let mut a = Ambiente::novo(&dir.path().join("dados"), &cofre);
    a.opcoes = Opcoes {
        espera: Duration::from_millis(60),
        download: Duration::from_secs(1),
        intervalo: Duration::from_millis(5),
    };
    a.prazo_da_pausa = Duration::from_secs(5);
    a.sem_janela = true;
    (a, conn)
}

/// Importa, testa e ativa o roteiro no banco do ambiente.
fn ativar(amb: &Ambiente, m: &Manifesto, fonte: &str) -> VersaoEscrita {
    let banco = amb.banco().unwrap();
    let n = banco
        .importar_escrito(m, fonte, &AutorEscrito::Usuario, "teste")
        .unwrap()
        .numero;
    banco.marcar_testado(&m.nome, n, &passou()).unwrap();
    let digitado = Confirmacao::texto_de_ativar(&m.nome, n);
    let c = Confirmacao::do_terminal(true, &digitado, &digitado).unwrap();
    banco.ativar(&m.nome, n, &c).unwrap();
    banco.versao_ativa_escrita(&m.nome).unwrap().unwrap()
}

impl Cenario {
    fn novo(m: &Manifesto, fonte: &str) -> Self {
        let dir = TempDir::new().unwrap();
        let (amb, conn) = ambiente(&dir);
        let versao = ativar(&amb, m, fonte);
        Self {
            dir,
            amb,
            versao,
            conn,
        }
    }

    /// Uma execução nova, no navegador dado.
    fn executar(&self, n: &mut NavegadorFalso, ctx: &Ctx) -> (RelatorioEscrito, i64) {
        let banco = self.amb.banco().unwrap();
        let execucao = banco
            .iniciar_execucao_escrita(&self.versao.roteiro, self.versao.numero, "janela")
            .unwrap();
        let rel = executar_escrito_em(
            &self.amb,
            n,
            &self.versao,
            execucao,
            &Servicos {
                senhas: &Chaveiro,
                caixa: ctx.caixa.as_ref().map(|c| c as &dyn CaixaDeEmail),
                relogio: &ctx.relogio,
                pausa: Some(&ctx.pausa),
                opcoes: &ctx.opcoes,
            },
            &ctx.cancelar,
        )
        .unwrap();
        (rel, execucao)
    }

    fn banco(&self) -> BancoDeRoteiros {
        self.amb.banco().unwrap()
    }
}

/// Roda um roteiro de uma vez e devolve o relatório e o cenário.
fn rodar(
    mfa: &str,
    classe: &str,
    fonte: &str,
    n: &mut NavegadorFalso,
    ctx: &Ctx,
) -> (RelatorioEscrito, Cenario, i64) {
    let c = Cenario::novo(&manifesto(mfa, classe), fonte);
    let (rel, execucao) = c.executar(n, ctx);
    (rel, c, execucao)
}

fn falhou_com(fim: &Fim, trecho: &str) {
    match fim {
        Fim::Falhou { motivo, .. } => {
            assert!(motivo.contains(trecho), "motivo: {motivo}");
        }
        outro => panic!("esperava Falhou({trecho}), veio {outro:?}"),
    }
}

fn nenhum_segredo(texto: &str, onde: &str) {
    for s in [SENHA, CPF, CPF_SO_DIGITOS, CODIGO] {
        assert!(!texto.contains(s), "segredo `{s}` vazou em {onde}: {texto}");
    }
}

// ------------------------------------------------------------------------------------------
// Caminho feliz
// ------------------------------------------------------------------------------------------

const ROTEIRO_FELIZ: &str = r##"
navegador.registrar("começou")
navegador.abrir("https://portal.exemplo.test/entrar")
navegador.preencher_cpf("#cpf")
navegador.preencher_senha("#senha", "senha:portal-exemplo")
navegador.clicar("#entrar")
navegador.esperar("#codigo", 5)
navegador.preencher_codigo_do_email("#codigo", { janela_segundos = 120 })
navegador.clicar("#validar")
local titulo = navegador.texto("#titulo")
navegador.registrar("título: " .. titulo)
local linhas = navegador.tabela("#tabela")
navegador.registrar("líquido: " .. linhas[2][2])
local arquivo = navegador.baixar("#baixar", "contracheque-" .. util.hoje())
navegador.registrar("baixou " .. arquivo)
"##;

#[test]
fn caminho_feliz_login_mfa_por_email_e_contracheque() {
    let mut n = portal(BASE, "codigo");
    let ctx = Ctx::novo().com_caixa(CaixaFalsa::com(vec![
        // um código velho (anterior ao login) e o novo: vale o novo
        (REMETENTE, -3600, "Seu código é 111111".into()),
        email_com_codigo(5),
        // de outro remetente: ignorado
        ("golpe@exemplo.test", 6, "Seu código é 999999".into()),
    ]));
    let (rel, c, execucao) = rodar(MFA_EMAIL, "consulta", ROTEIRO_FELIZ, &mut n, &ctx);

    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!((rel.baixados, rel.novos), (1, 1));
    assert_eq!(
        rel.registros,
        vec![
            "começou".to_owned(),
            "título: Demonstrativo de novembro".to_owned(),
            "líquido: R$ 1.234,56".to_owned(),
            "baixou contracheque-2026-11-24.pdf".to_owned(),
        ],
        "util.hoje() às 22h de 24/11 em UTC−3 é 2026-11-24"
    );
    // O código certo (o novo, do remetente do manifesto) e só ele foi ao campo.
    let digitado = |sel: &str| -> Vec<String> {
        n.digitados
            .iter()
            .filter(|(s, _)| s == sel)
            .map(|(_, v)| v.clone())
            .collect()
    };
    assert_eq!(digitado("#codigo"), vec![CODIGO.to_owned()]);
    assert_eq!(digitado("#senha"), vec![SENHA.to_owned()]);
    assert_eq!(digitado("#cpf"), vec![CPF.to_owned()]);
    assert_eq!(
        ctx.caixa
            .as_ref()
            .unwrap()
            .remetentes
            .borrow()
            .first()
            .map(String::as_str),
        Some(REMETENTE),
        "a busca usa o remetente do manifesto"
    );

    // O download virou achado com a origem roteiro:<nome>/<execução>/<arquivo>.
    let origem: String = c
        .conn
        .query_row("SELECT origin_key FROM origem", [], |l| l.get(0))
        .unwrap();
    assert_eq!(
        origem,
        format!("roteiro:contracheque-exemplo/{execucao}/contracheque-2026-11-24.pdf")
    );
    let achados: i64 = c
        .conn
        .query_row("SELECT count(*) FROM achado", [], |l| l.get(0))
        .unwrap();
    assert_eq!(achados, 1);

    // O histórico fechou como concluída.
    let h = c
        .banco()
        .historico_escrito(Some("contracheque-exemplo"), 5)
        .unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].estado, EstadoDaExecucao::Concluida);
    assert_eq!((h[0].baixados, h[0].novos), (1, 1));
    assert_eq!(h[0].gatilho, "janela");

    // SENHA, CPF e código nunca aparecem no relatório, no histórico, nos eventos nem no banco.
    nenhum_segredo(&format!("{rel:?}"), "o relatório");
    nenhum_segredo(&format!("{h:?}"), "o histórico");
    nenhum_segredo(
        &format!("{:?}", c.banco().eventos_escritos(execucao).unwrap()),
        "os eventos",
    );
    for arquivo in std::fs::read_dir(c.dir.path().join("dados")).unwrap() {
        let p = arquivo.unwrap().path();
        let Ok(bytes) = std::fs::read(&p) else {
            continue; // pasta (o perfil do Chrome), não arquivo do banco
        };
        let texto = String::from_utf8_lossy(&bytes);
        nenhum_segredo(&texto, &p.display().to_string());
    }
}

#[test]
fn o_script_nunca_recebe_o_segredo_nem_o_codigo() {
    // O roteiro lê tudo o que pode ler depois de digitar: nada traz o segredo.
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.preencher_senha("#senha", "senha:portal-exemplo")
        navegador.registrar("depois da senha")
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, c, execucao) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida);
    assert_eq!(rel.registros, vec!["depois da senha".to_owned()]);
    nenhum_segredo(
        &format!("{:?}", c.banco().eventos_escritos(execucao).unwrap()),
        "eventos",
    );
}

#[test]
fn eco_do_segredo_na_pagina_sai_limpo_no_texto_no_registro_e_no_erro() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.preencher_senha("#senha", "senha:portal-exemplo")
        navegador.clicar("#entrar")
        local eco = navegador.texto("#eco")
        navegador.registrar("eco: " .. eco)
        error("deu ruim com " .. eco)
    "##;
    let mut n = portal(BASE, "painel");
    // A página repete a senha digitada (um eco do site).
    n.textos
        .insert("#eco".into(), format!("você digitou {SENHA}"));
    let (rel, c, execucao) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    falhou_com(&rel.fim, "deu ruim com você digitou ***");
    assert_eq!(rel.registros, vec!["eco: você digitou ***".to_owned()]);
    nenhum_segredo(&format!("{rel:?}"), "relatório");
    nenhum_segredo(
        &format!("{:?}", c.banco().eventos_escritos(execucao).unwrap()),
        "eventos",
    );
    let h = c.banco().historico_escrito(None, 5).unwrap();
    nenhum_segredo(&format!("{h:?}"), "histórico");
}

#[test]
fn o_debug_dos_verbos_nao_mostra_segredo() {
    let m = manifesto(MFA_NENHUM, "consulta");
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let mut n = portal(BASE, "painel");
    let mut destino = SoDestino;
    let cancelar = AtomicBool::new(false);
    let relogio = RelogioFalso::novo();
    let pausa = PausaFalsa::com(&[]);
    let mut v = SbfVerbos::novo(Entradas {
        manifesto: &m,
        execucao: 1,
        navegador: &mut n,
        senhas: &Chaveiro,
        caixa: None,
        relogio: &relogio,
        pausa: &pausa,
        destino: &mut destino,
        banco: &banco,
        pasta: dir.path().join("p"),
        opcoes: amb.opcoes,
        teste: OpcoesEscrito::default(),
        cancelar: &cancelar,
    });
    let _ = luau::Verbos::abrir(&mut v, "https://portal.exemplo.test/entrar");
    let _ = luau::Verbos::preencher_senha(&mut v, "#senha", ROTULO);
    nenhum_segredo(&format!("{v:?}"), "Debug de SbfVerbos");
}

/// Um destino que nunca recebe nada (os testes de Debug).
#[derive(Debug)]
struct SoDestino;

impl sbf_roteiros::executor::Destino for SoDestino {
    fn receber(
        &mut self,
        _arquivo: &std::path::Path,
        _nome: &str,
    ) -> Result<sbf_roteiros::executor::Recebido, ErroDeRoteiro> {
        Ok(sbf_roteiros::executor::Recebido::Novo)
    }
}

// ------------------------------------------------------------------------------------------
// Hosts (RE-03, RE-04)
// ------------------------------------------------------------------------------------------

#[test]
fn abrir_fora_da_allowlist_e_recusado_e_pcall_nao_engole() {
    let fonte = r##"
        local ok = pcall(navegador.abrir, "https://outro.exemplo.test/")
        navegador.registrar("seguiu")
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    falhou_com(&rel.fim, "hosts do manifesto");
    assert!(rel.registros.is_empty(), "o roteiro não seguiu: {rel:?}");
    assert!(
        !n.chamadas.iter().any(|c| c.starts_with("navegar")),
        "nada foi aberto: {:?}",
        n.chamadas
    );
}

#[test]
fn abrir_exige_https_e_recusa_usuario_na_url() {
    for url in [
        "http://portal.exemplo.test/entrar",
        "https://fulano@portal.exemplo.test/entrar",
        "https://portal.exemplo.test.mal.test/entrar",
        "ftp://portal.exemplo.test/",
    ] {
        let fonte = format!(r##"navegador.abrir("{url}")"##);
        let mut n = portal(BASE, "painel");
        let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", &fonte, &mut n, &Ctx::novo());
        falhou_com(&rel.fim, "hosts do manifesto");
        assert!(n.chamadas.is_empty(), "{url}: {:?}", n.chamadas);
    }
}

#[test]
fn redirecionamento_para_fora_da_allowlist_aborta() {
    // O host lógico é permitido, mas o navegador caiu em outro (redirecionamento do site).
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/redireciona")
        navegador.registrar("não devia chegar aqui")
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    falhou_com(&rel.fim, "hosts do manifesto");
    assert!(rel.registros.is_empty());
}

#[test]
fn clique_que_leva_para_fora_da_allowlist_aborta() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        navegador.clicar("#sair")
        navegador.registrar("não devia chegar aqui")
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    falhou_com(&rel.fim, "hosts do manifesto");
    assert!(rel.registros.is_empty());
}

#[test]
fn senha_cpf_e_codigo_recusam_pagina_de_host_fora_da_allowlist() {
    for (verbo, mfa) in [
        (
            r##"navegador.preencher_senha("#senha", "senha:portal-exemplo")"##,
            MFA_NENHUM,
        ),
        (r##"navegador.preencher_cpf("#cpf")"##, MFA_NENHUM),
        (
            r##"navegador.preencher_codigo_do_email("#codigo")"##,
            MFA_EMAIL,
        ),
    ] {
        let mut n = portal(BASE, "painel");
        // O navegador já está numa página de outro host (sem passar por `abrir`).
        n.atual = "fora".into();
        let ctx = Ctx::novo().com_caixa(CaixaFalsa::com(vec![email_com_codigo(5)]));
        let (rel, c, _) = rodar(
            mfa,
            "consulta",
            &format!("pcall(function() {verbo} end)\nnavegador.registrar('seguiu')"),
            &mut n,
            &ctx,
        );
        falhou_com(&rel.fim, "hosts do manifesto");
        assert!(n.digitados.is_empty(), "{verbo}: nada digitado");
        assert!(rel.registros.is_empty(), "{verbo}: parou no verbo");
        assert_eq!(ctx.caixa.as_ref().unwrap().consultas.get(), 0, "{verbo}");
        let h = c.banco().historico_escrito(None, 1).unwrap();
        assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
    }
}

#[test]
fn senha_so_vai_no_campo_de_senha_e_cpf_so_no_de_texto() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        local ok1 = pcall(navegador.preencher_senha, "#cpf", "senha:portal-exemplo")
        local ok2 = pcall(navegador.preencher_cpf, "#senha")
        navegador.registrar(tostring(ok1) .. " " .. tostring(ok2))
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida);
    assert_eq!(rel.registros, vec!["false false".to_owned()]);
    assert!(n.digitados.is_empty());
}

// ------------------------------------------------------------------------------------------
// Rótulos (RE-02)
// ------------------------------------------------------------------------------------------

#[test]
fn rotulo_diferente_do_manifesto_e_erro_sem_revelar_nada() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        local ok, erro = pcall(navegador.preencher_senha, "#senha", "senha:banco-do-vizinho")
        navegador.registrar(tostring(ok) .. ": " .. tostring(erro))
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida);
    assert!(
        rel.registros[0].starts_with("false: "),
        "{:?}",
        rel.registros
    );
    assert!(
        rel.registros[0].contains("rótulo da senha não é o do manifesto"),
        "{:?}",
        rel.registros
    );
    assert!(!rel.registros[0].contains("banco-do-vizinho"));
    assert!(!rel.registros[0].contains(ROTULO));
    assert!(n.digitados.is_empty(), "o chaveiro nem foi consultado");
}

/// O CPF é o único do chaveiro: o manifesto não precisa de `rotulo_cpf` para ele ser digitado,
/// mas só no host aprovado.
#[test]
fn cpf_e_digitado_no_host_aprovado_sem_rotulo_e_recusado_em_host_de_fora() {
    let m = Manifesto::de_json(
        r##"{"nome":"sem-cpf","url_inicial":"https://portal.exemplo.test/entrar",
        "hosts":["portal.exemplo.test"],"rotulo_senha":"senha:portal-exemplo",
        "mfa":{"tipo":"nenhum"},"movimenta_dinheiro":false}"##,
    )
    .unwrap();
    assert!(m.rotulo_cpf.is_none());
    // No host aprovado: digita.
    let c = Cenario::novo(
        &m,
        r##"navegador.abrir("https://portal.exemplo.test/entrar") navegador.preencher_cpf("#cpf")"##,
    );
    let mut n = portal(BASE, "painel");
    let (rel, _) = c.executar(&mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(n.digitados, vec![("#cpf".to_owned(), CPF.to_owned())]);
    // Em página de host fora da lista: recusa, nada digitado.
    let c = Cenario::novo(&m, r##"navegador.preencher_cpf("#cpf")"##);
    let mut n = portal(BASE, "painel");
    n.atual = "fora".into();
    let (rel, _) = c.executar(&mut n, &Ctx::novo());
    falhou_com(&rel.fim, "hosts do manifesto");
    assert!(n.digitados.is_empty());
}

// ------------------------------------------------------------------------------------------
// TOCTOU da digitação do segredo
// ------------------------------------------------------------------------------------------

#[test]
fn pagina_que_muda_de_host_entre_a_checagem_e_a_digitacao_nao_recebe_segredo() {
    for (pagina, verbo, mfa) in [
        (
            "entrar",
            r##"navegador.preencher_senha("#senha", "senha:portal-exemplo")"##,
            MFA_NENHUM,
        ),
        ("entrar", r##"navegador.preencher_cpf("#cpf")"##, MFA_NENHUM),
        (
            "codigo",
            r##"navegador.preencher_codigo_do_email("#codigo", { janela_segundos = 30 })"##,
            MFA_EMAIL,
        ),
    ] {
        let fonte = format!(
            "navegador.abrir('https://portal.exemplo.test/{pagina}')\npcall(function() {verbo} end)\nnavegador.registrar('seguiu')"
        );
        let mut n = portal(BASE, "painel");
        // O site "redireciona por timer" para outro host depois que o SBF conferiu a página e
        // antes de a digitação chegar ao navegador.
        n.redireciona_antes_de_digitar = Some("fora".into());
        let ctx = Ctx::novo().com_caixa(CaixaFalsa::com(vec![email_com_codigo(5)]));
        let (rel, c, execucao) = rodar(mfa, "consulta", &fonte, &mut n, &ctx);
        // Nada foi digitado, o roteiro termina (o `pcall` não o salva) e sem eco de segredo.
        assert!(n.digitados.is_empty(), "{verbo}: nada digitado");
        falhou_com(&rel.fim, "hosts do manifesto");
        assert!(rel.registros.is_empty(), "{verbo}: parou no verbo");
        nenhum_segredo(&format!("{rel:?}"), "relatório");
        nenhum_segredo(
            &format!("{:?}", c.banco().eventos_escritos(execucao).unwrap()),
            "eventos",
        );
        let h = c.banco().historico_escrito(None, 1).unwrap();
        assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
        nenhum_segredo(&format!("{h:?}"), "histórico");
    }
}

// ------------------------------------------------------------------------------------------
// Guarda de rascunho (RE-16)
// ------------------------------------------------------------------------------------------

#[test]
fn rascunho_e_testado_so_rodam_no_modo_teste_nunca_com_opcoes_de_producao() {
    let m = manifesto(MFA_NENHUM, "consulta");
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let numero = banco
        .importar_escrito(
            &m,
            "navegador.registrar('rodou')",
            &AutorEscrito::Usuario,
            "teste",
        )
        .unwrap()
        .numero;
    let ctx = Ctx::novo();
    let rodar_com = |versao: &VersaoEscrita, opcoes: &OpcoesEscrito, prefixo: &str| {
        let execucao = banco
            .iniciar_execucao_escrita(&versao.roteiro, versao.numero, "janela")
            .unwrap();
        let mut n = portal(prefixo, "painel");
        let r = executar_escrito_em(
            &amb,
            &mut n,
            versao,
            execucao,
            &Servicos {
                senhas: &Chaveiro,
                caixa: None,
                relogio: &ctx.relogio,
                pausa: Some(&ctx.pausa),
                opcoes,
            },
            &ctx.cancelar,
        );
        // libera a execução (o banco só admite uma por roteiro)
        let _ = banco.finalizar_escrita(execucao, EstadoDaExecucao::Falhou, Some("teste"), 0, 0);
        r
    };
    let modo_teste = OpcoesEscrito {
        mapa_de_hosts: vec![("portal.exemplo.test".into(), "http://127.0.0.1:4101".into())],
        aceitar_http_loopback: true,
    };

    // Rascunho: produção recusa; modo teste (o caminho do `testar`) roda.
    let rascunho = banco.versao_escrita(&m.nome, numero).unwrap();
    assert_eq!(rascunho.estado, EstadoEscrito::Rascunho);
    let e = rodar_com(&rascunho, &OpcoesEscrito::default(), BASE).unwrap_err();
    assert!(e.to_string().contains("não está ativa"), "{e}");
    // Só uma das duas opções de teste não basta.
    let so_mapa = OpcoesEscrito {
        aceitar_http_loopback: false,
        ..modo_teste.clone()
    };
    assert!(rodar_com(&rascunho, &so_mapa, BASE).is_err());
    let rel = rodar_com(&rascunho, &modo_teste, "http://127.0.0.1:4101").unwrap();
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");

    // O estado vem do banco: uma struct que se declara ativa não engana a guarda.
    let mut mentirosa = rascunho.clone();
    mentirosa.estado = EstadoEscrito::Ativo;
    assert!(rodar_com(&mentirosa, &OpcoesEscrito::default(), BASE).is_err());

    // Testado: continua recusado em produção.
    banco.marcar_testado(&m.nome, numero, &passou()).unwrap();
    let testado = banco.versao_escrita(&m.nome, numero).unwrap();
    assert_eq!(testado.estado, EstadoEscrito::Testado);
    assert!(rodar_com(&testado, &OpcoesEscrito::default(), BASE).is_err());

    // Ativa: roda com as opções de produção. O estado vem do banco, não da struct.
    let digitado = Confirmacao::texto_de_ativar(&m.nome, numero);
    let conf = Confirmacao::do_terminal(true, &digitado, &digitado).unwrap();
    banco.ativar(&m.nome, numero, &conf).unwrap();
    let ativa = banco.versao_ativa_escrita(&m.nome).unwrap().unwrap();
    let rel = rodar_com(&ativa, &OpcoesEscrito::default(), BASE).unwrap();
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
}

// ------------------------------------------------------------------------------------------
// Bloqueio e classe (RE-05, RE-06, RE-15)
// ------------------------------------------------------------------------------------------

#[test]
fn clicar_em_pagar_aborta_com_bloqueio_antes_de_clicar() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        navegador.clicar("#pagar")
        navegador.registrar("não devia chegar aqui")
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, c, execucao) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    match &rel.fim {
        Fim::Bloqueada { bloqueio, .. } => assert_eq!(bloqueio.termo, "pagar"),
        outro => panic!("esperava Bloqueada, veio {outro:?}"),
    }
    assert!(rel.registros.is_empty());
    assert!(
        !n.chamadas.iter().any(|c| c.starts_with("clicar")),
        "o clique nem saiu: {:?}",
        n.chamadas
    );
    let h = c.banco().historico_escrito(None, 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Bloqueada);
    let eventos = c.banco().eventos_escritos(execucao).unwrap();
    assert!(eventos.iter().any(|(_, tipo, _)| tipo == "bloqueio"));
}

#[test]
fn o_navegador_barra_de_novo_mesmo_se_o_executor_deixa_passar() {
    // "Continuar" passa pela lista do executor, mas o script do navegador barra o clique.
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        navegador.clicar("#continuar")
        navegador.registrar("não devia chegar aqui")
    "##;
    let mut n = portal(BASE, "painel");
    n.elementos
        .get_mut("#continuar")
        .unwrap()
        .barrado_no_navegador = Some(Bloqueio {
        campo: "destino".into(),
        termo: "transfer".into(),
    });
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    match &rel.fim {
        Fim::Bloqueada { bloqueio, .. } => assert_eq!(bloqueio.termo, "transfer"),
        outro => panic!("esperava Bloqueada, veio {outro:?}"),
    }
    assert!(rel.registros.is_empty());
}

#[test]
fn download_de_elemento_de_pagar_tambem_e_bloqueado() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        navegador.baixar("#pagar")
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert!(matches!(rel.fim, Fim::Bloqueada { .. }), "{:?}", rel.fim);
    assert!(!n.chamadas.iter().any(|c| c.starts_with("baixar")));
    let achados: i64 = c
        .conn
        .query_row("SELECT count(*) FROM achado", [], |l| l.get(0))
        .unwrap();
    assert_eq!(achados, 0);
}

fn clicar_em(seletor: &str, classe: &str) -> Fim {
    let fonte = format!(
        r##"navegador.abrir("https://portal.exemplo.test/painel")
        navegador.clicar("{seletor}")"##
    );
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, classe, &fonte, &mut n, &Ctx::novo());
    rel.fim
}

#[test]
fn classe_consulta_nao_libera_atualizar_nem_sincronizar() {
    for sel in ["#atualizar", "#sincronizar"] {
        assert!(
            matches!(clicar_em(sel, "consulta"), Fim::Bloqueada { .. }),
            "{sel}"
        );
    }
    // E um botão comum continua clicável.
    assert_eq!(clicar_em("#continuar", "consulta"), Fim::Concluida);
}

#[test]
fn classe_atualizar_conexao_libera_so_o_atualizar_exato_e_continua_barrando_o_resto() {
    assert_eq!(clicar_em("#atualizar", "atualizar_conexao"), Fim::Concluida);
    assert_eq!(
        clicar_em("#sincronizar", "atualizar_conexao"),
        Fim::Concluida
    );
    for sel in [
        "#pagar",
        "#cadastro",          // "Atualizar dados cadastrais" não é o texto exato
        "#atualizar-e-pagar", // "Atualizar e pagar" também não
        "#atualizar-destino", // texto exato, mas o destino é de pagamento
    ] {
        assert!(
            matches!(clicar_em(sel, "atualizar_conexao"), Fim::Bloqueada { .. }),
            "{sel}"
        );
    }
}

// ------------------------------------------------------------------------------------------
// Campos (RE-07)
// ------------------------------------------------------------------------------------------

#[test]
fn texto_nunca_le_campo_e_preencher_recusa_senha() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.preencher_senha("#senha", "senha:portal-exemplo")
        local ok1, e1 = pcall(navegador.texto, "#senha")
        local ok2, e2 = pcall(navegador.preencher, "#senha", "x")
        local ok3, e3 = pcall(navegador.texto, "#cpf")
        navegador.registrar(tostring(ok1) .. "|" .. tostring(e1))
        navegador.registrar(tostring(ok2) .. "|" .. tostring(e2))
        navegador.registrar(tostring(ok3))
        navegador.preencher("#cpf", "texto livre")
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert!(
        rel.registros[0].starts_with("false|") && rel.registros[0].contains("campo de formulário")
    );
    assert!(rel.registros[1].starts_with("false|") && rel.registros[1].contains("preencher_senha"));
    assert_eq!(rel.registros[2], "false");
    assert_eq!(
        n.digitados,
        vec![
            ("#senha".to_owned(), SENHA.to_owned()),
            ("#cpf".to_owned(), "texto livre".to_owned())
        ],
        "o `x` não foi para o campo de senha"
    );
    nenhum_segredo(&rel.registros.join("\n"), "registros");
}

// ------------------------------------------------------------------------------------------
// MFA por e-mail (RE-09 a RE-13)
// ------------------------------------------------------------------------------------------

const SO_CODIGO: &str = r##"
    navegador.abrir("https://portal.exemplo.test/entrar")
    navegador.clicar("#entrar")
    navegador.preencher_codigo_do_email("#codigo")
    navegador.registrar("digitado")
"##;

#[test]
fn mfa_por_email_em_manifesto_sem_email_e_erro() {
    for mfa in [MFA_NENHUM, MFA_MANUAL] {
        // A página de código pausa (MFA de token, aqui "manual" ou nem declarado); o usuário
        // continua e o verbo do e-mail, que o manifesto não autoriza, falha.
        let ctx = Ctx::novo()
            .com_caixa(CaixaFalsa::com(vec![email_com_codigo(5)]))
            .com_pausa(&[Decisao::Continuar]);
        let mut n = portal(BASE, "codigo");
        let (rel, _c, _) = rodar(mfa, "consulta", SO_CODIGO, &mut n, &ctx);
        falhou_com(&rel.fim, "MFA por e-mail");
        assert!(!n.digitados.iter().any(|(s, _)| s == "#codigo"));
        assert_eq!(ctx.caixa.as_ref().unwrap().consultas.get(), 0);
    }
}

#[test]
fn codigo_so_de_antes_do_login_e_ignorado_e_o_roteiro_falha_sem_segredo() {
    let ctx = Ctx::novo().com_caixa(CaixaFalsa::com(vec![(
        REMETENTE,
        -60,
        format!("Seu código é {CODIGO}"),
    )]));
    let mut n = portal(BASE, "codigo");
    let (rel, c, execucao) = rodar(MFA_EMAIL, "consulta", SO_CODIGO, &mut n, &ctx);
    falhou_com(&rel.fim, "prazo");
    assert!(!n.digitados.iter().any(|(s, _)| s == "#codigo"));
    assert!(
        ctx.caixa.as_ref().unwrap().consultas.get() > 1,
        "esperou a janela inteira"
    );
    nenhum_segredo(&format!("{rel:?}"), "relatório");
    nenhum_segredo(
        &format!("{:?}", c.banco().eventos_escritos(execucao).unwrap()),
        "eventos",
    );
}

#[test]
fn codigo_de_outro_remetente_e_ignorado() {
    let ctx = Ctx::novo().com_caixa(CaixaFalsa::com(vec![(
        "golpe@exemplo.test",
        5,
        format!("Seu código é {CODIGO}"),
    )]));
    let mut n = portal(BASE, "codigo");
    let (rel, _c, _) = rodar(MFA_EMAIL, "consulta", SO_CODIGO, &mut n, &ctx);
    falhou_com(&rel.fim, "prazo");
    assert!(n.digitados.is_empty());
}

#[test]
fn a_janela_do_script_vale_e_o_teto_e_de_300_segundos() {
    // janela_segundos = 9999 é limitada pelo host a 300 s: a caixa vazia é consultada até o
    // relógio falso passar disso (e não além).
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.clicar("#entrar")
        navegador.preencher_codigo_do_email("#codigo", { janela_segundos = 9999 })
    "##;
    let ctx = Ctx::novo().com_caixa(CaixaFalsa::com(vec![]));
    let mut n = portal(BASE, "codigo");
    let (rel, _c, _) = rodar(MFA_EMAIL, "consulta", fonte, &mut n, &ctx);
    falhou_com(&rel.fim, "prazo");
    let gasto = ctx.relogio.agora().duration_since(t(0)).unwrap();
    assert!(
        gasto >= Duration::from_secs(295) && gasto <= Duration::from_secs(305),
        "{gasto:?}"
    );
}

#[test]
fn sem_caixa_de_email_o_verbo_falha_com_mensagem_clara() {
    let mut n = portal(BASE, "codigo");
    let (rel, _c, _) = rodar(MFA_EMAIL, "consulta", SO_CODIGO, &mut n, &Ctx::novo());
    falhou_com(&rel.fim, "caixa de e-mail");
}

#[test]
fn o_verbo_revalida_que_o_roteiro_nao_movimenta_dinheiro() {
    // Um manifesto que o importador nunca aceitaria (MFA por e-mail + movimenta dinheiro),
    // montado na mão: o verbo se recusa mesmo assim.
    let mut m = manifesto(MFA_EMAIL, "consulta");
    m.movimenta_dinheiro = true;
    assert!(m.validar().is_err());
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let caixa = CaixaFalsa::com(vec![email_com_codigo(5)]);
    let mut n = portal(BASE, "codigo");
    let mut destino = SoDestino;
    let cancelar = AtomicBool::new(false);
    let relogio = RelogioFalso::novo();
    let pausa = PausaFalsa::com(&[]);
    let mut v = SbfVerbos::novo(Entradas {
        manifesto: &m,
        execucao: 1,
        navegador: &mut n,
        senhas: &Chaveiro,
        caixa: Some(&caixa),
        relogio: &relogio,
        pausa: &pausa,
        destino: &mut destino,
        banco: &banco,
        pasta: dir.path().join("p"),
        opcoes: amb.opcoes,
        teste: OpcoesEscrito::default(),
        cancelar: &cancelar,
    });
    let r = luau::executar(SO_CODIGO, &mut v, &Limites::default());
    assert!(r.is_err(), "{r:?}");
    assert_eq!(caixa.consultas.get(), 0, "o e-mail nem foi consultado");
    assert!(n.digitados.is_empty());
}

#[test]
fn executar_escrito_em_revalida_o_manifesto_e_o_modo_teste() {
    let c = Cenario::novo(
        &manifesto(MFA_NENHUM, "consulta"),
        "navegador.registrar('a')",
    );
    let mut n = portal(BASE, "painel");
    let execucao = c
        .banco()
        .iniciar_execucao_escrita(&c.versao.roteiro, c.versao.numero, "janela")
        .unwrap();
    let mut ruim = c.versao.clone();
    ruim.manifesto = {
        let mut m = manifesto(MFA_EMAIL, "consulta");
        m.movimenta_dinheiro = true;
        m
    };
    let ctx = Ctx::novo();
    let servicos = Servicos {
        senhas: &Chaveiro,
        caixa: None,
        relogio: &ctx.relogio,
        pausa: None,
        opcoes: &ctx.opcoes,
    };
    let e = executar_escrito_em(&c.amb, &mut n, &ruim, execucao, &servicos, &ctx.cancelar);
    assert!(e.is_err());

    // Base real que não é o próprio computador: recusada.
    let teste = OpcoesEscrito {
        mapa_de_hosts: vec![(
            "portal.exemplo.test".into(),
            "http://exemplo.test:80".into(),
        )],
        aceitar_http_loopback: true,
    };
    assert!(teste.validar(&c.versao.manifesto).is_err());
    // Host lógico que não é do manifesto: recusado.
    let teste = OpcoesEscrito {
        mapa_de_hosts: vec![("outro.exemplo.test".into(), "http://127.0.0.1:4101".into())],
        aceitar_http_loopback: true,
    };
    assert!(teste.validar(&c.versao.manifesto).is_err());
    let ok = OpcoesEscrito {
        mapa_de_hosts: vec![("portal.exemplo.test".into(), "http://127.0.0.1:4101".into())],
        aceitar_http_loopback: true,
    };
    assert!(ok.validar(&c.versao.manifesto).is_ok());
}

// ------------------------------------------------------------------------------------------
// Pausa (RE-14)
// ------------------------------------------------------------------------------------------

const ENTRAR_E_BAIXAR: &str = r##"
    navegador.abrir("https://portal.exemplo.test/entrar")
    navegador.clicar("#entrar")
    navegador.esperar("#baixar", 2)
    navegador.registrar("no painel")
"##;

#[test]
fn captcha_pausa_espera_o_usuario_e_continua() {
    let sinal = Arc::new(AtomicBool::new(false));
    let mut n = portal(BASE, "mfa");
    n.liberacao = Some(Arc::clone(&sinal));
    let mut ctx = Ctx::novo().com_pausa(&[Decisao::Continuar]);
    ctx.pausa.sinal = Some(sinal);
    let (rel, c, execucao) = rodar(MFA_NENHUM, "consulta", ENTRAR_E_BAIXAR, &mut n, &ctx);
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(rel.registros, vec!["no painel".to_owned()]);
    let motivos = ctx.pausa.motivos.lock().unwrap().clone();
    assert_eq!(motivos.len(), 1, "{motivos:?}");
    assert!(motivos[0].contains("CAPTCHA"), "{motivos:?}");
    let eventos = c.banco().eventos_escritos(execucao).unwrap();
    assert!(eventos.iter().any(|(_, t, _)| t == "pausa"));
    assert!(eventos.iter().any(|(_, t, _)| t == "retomada"));
}

/// O usuário que demora: espera `0` ms e continua (e acende o sinal que libera o painel).
struct PausaLenta {
    espera: Duration,
    sinal: Arc<AtomicBool>,
}

impl sbf_roteiros::executor::Pausa for PausaLenta {
    fn aguardar(&self, _execucao: i64, _motivo: &str) -> Decisao {
        std::thread::sleep(self.espera);
        self.sinal.store(true, Ordering::SeqCst);
        Decisao::Continuar
    }
}

#[test]
fn o_tempo_da_pausa_automatica_e_acumulado_e_zera_ao_ler_mas_a_pausa_explicita_nao_conta() {
    let m = manifesto(MFA_NENHUM, "consulta");
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    let sinal = Arc::new(AtomicBool::new(false));
    let mut n = portal(BASE, "mfa");
    n.liberacao = Some(Arc::clone(&sinal));
    let mut destino = SoDestino;
    let cancelar = AtomicBool::new(false);
    let relogio = RelogioFalso::novo();
    let pausa = PausaLenta {
        espera: Duration::from_millis(150),
        sinal,
    };
    let mut v = SbfVerbos::novo(Entradas {
        manifesto: &m,
        execucao: 1,
        navegador: &mut n,
        senhas: &Chaveiro,
        caixa: None,
        relogio: &relogio,
        pausa: &pausa,
        destino: &mut destino,
        banco: &banco,
        pasta: dir.path().join("p"),
        opcoes: amb.opcoes,
        teste: OpcoesEscrito::default(),
        cancelar: &cancelar,
    });
    assert_eq!(v.tempo_pausado_automatico(), Duration::ZERO);
    luau::Verbos::abrir(&mut v, "https://portal.exemplo.test/entrar").unwrap();
    // O clique cai na página do CAPTCHA: a pausa automática espera o usuário.
    luau::Verbos::clicar(&mut v, "#entrar").unwrap();
    assert_eq!(v.pausas(), 1);
    let gasto = v.tempo_pausado_automatico();
    assert!(gasto >= Duration::from_millis(150), "{gasto:?}");
    assert!(gasto < Duration::from_secs(5), "{gasto:?}");
    assert_eq!(v.tempo_pausado_automatico(), Duration::ZERO, "zera ao ler");
    // A pausa que o próprio roteiro pede NÃO é automática: o host já a conta à parte.
    luau::Verbos::pausa(&mut v, "confirme no app").unwrap();
    assert_eq!(v.pausas(), 2);
    assert_eq!(v.tempo_pausado_automatico(), Duration::ZERO);
}

#[test]
fn sem_autorizacao_do_gmail_o_verbo_do_email_falha_na_hora_com_a_mensagem_certa() {
    let mut n = portal(BASE, "codigo");
    let ctx = Ctx::novo();
    let c = Cenario::novo(&manifesto(MFA_EMAIL, "consulta"), SO_CODIGO);
    let caixa = sbf_roteiros::executor::CaixaSemAutorizacao;
    let execucao = c
        .banco()
        .iniciar_execucao_escrita(&c.versao.roteiro, c.versao.numero, "janela")
        .unwrap();
    let rel = executar_escrito_em(
        &c.amb,
        &mut n,
        &c.versao,
        execucao,
        &Servicos {
            senhas: &Chaveiro,
            caixa: Some(&caixa),
            relogio: &ctx.relogio,
            pausa: Some(&ctx.pausa),
            opcoes: &ctx.opcoes,
        },
        &ctx.cancelar,
    )
    .unwrap();
    falhou_com(
        &rel.fim,
        "Autorização do Gmail ausente: entre de novo em Conexões › E-mail",
    );
    assert!(n.digitados.is_empty());
    // Não esperou a janela inteira: o relógio de teste só avança quando alguém espera.
    assert_eq!(ctx.relogio.agora.get(), t(0));
}

#[test]
fn mfa_de_token_manual_pausa_e_continua() {
    let sinal = Arc::new(AtomicBool::new(false));
    // A página do token vira o painel quando o usuário resolve (reaproveita a "mfa" do falso).
    let mut n = portal(BASE, "mfa");
    let token = n.paginas["token"].clone();
    n.paginas.insert("mfa".into(), token);
    n.liberacao = Some(Arc::clone(&sinal));
    let mut ctx = Ctx::novo().com_pausa(&[Decisao::Continuar]);
    ctx.pausa.sinal = Some(sinal);
    let (rel, _c, _) = rodar(MFA_MANUAL, "consulta", ENTRAR_E_BAIXAR, &mut n, &ctx);
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    let motivos = ctx.pausa.motivos.lock().unwrap().clone();
    assert_eq!(motivos.len(), 1, "{motivos:?}");
    assert!(motivos[0].contains("MFA"), "{motivos:?}");
}

#[test]
fn abortar_a_pausa_encerra_como_cancelada_e_expirar_como_nao_concluida() {
    let mut n = portal(BASE, "mfa");
    let ctx = Ctx::novo().com_pausa(&[Decisao::Cancelar]);
    let (rel, c, _) = rodar(MFA_NENHUM, "consulta", ENTRAR_E_BAIXAR, &mut n, &ctx);
    assert_eq!(rel.fim, Fim::Cancelada);
    assert!(rel.registros.is_empty());
    let h = c.banco().historico_escrito(None, 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Cancelada);

    let mut n = portal(BASE, "mfa");
    let ctx = Ctx::novo().com_pausa(&[Decisao::Expirou]);
    let (rel, c, _) = rodar(MFA_NENHUM, "consulta", ENTRAR_E_BAIXAR, &mut n, &ctx);
    assert!(matches!(rel.fim, Fim::PausaExpirou { .. }), "{:?}", rel.fim);
    let h = c.banco().historico_escrito(None, 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
    assert!(
        h[0].motivo
            .as_deref()
            .unwrap()
            .contains("não voltou a tempo")
    );
}

#[test]
fn o_verbo_pausa_do_script_espera_o_usuario() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.pausa("resolva o desafio do portal")
        navegador.registrar("voltou")
    "##;
    let mut n = portal(BASE, "painel");
    let ctx = Ctx::novo().com_pausa(&[Decisao::Continuar]);
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &ctx);
    assert_eq!(rel.fim, Fim::Concluida);
    assert_eq!(rel.registros, vec!["voltou".to_owned()]);
    assert_eq!(
        ctx.pausa.motivos.lock().unwrap().clone(),
        vec!["resolva o desafio do portal".to_owned()]
    );

    // Se o usuário aborta, o roteiro termina (e pcall não engole).
    let fonte = r##"
        pcall(navegador.pausa, "x")
        navegador.registrar("seguiu")
    "##;
    let mut n = portal(BASE, "painel");
    let ctx = Ctx::novo().com_pausa(&[Decisao::Cancelar]);
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &ctx);
    assert_eq!(rel.fim, Fim::Cancelada);
    assert!(rel.registros.is_empty());
}

// ------------------------------------------------------------------------------------------
// Cancelar, limites
// ------------------------------------------------------------------------------------------

#[test]
fn cancelar_interrompe_o_verbo_seguinte() {
    let ctx = Ctx::novo();
    ctx.cancelar.store(true, Ordering::SeqCst);
    let mut n = portal(BASE, "painel");
    let (rel, c, _) = rodar(
        MFA_NENHUM,
        "consulta",
        r##"navegador.registrar("a") navegador.abrir("https://portal.exemplo.test/entrar")"##,
        &mut n,
        &ctx,
    );
    assert_eq!(rel.fim, Fim::Cancelada);
    assert!(rel.registros.is_empty());
    assert!(n.chamadas.is_empty());
    let h = c.banco().historico_escrito(None, 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Cancelada);
}

#[test]
fn cancelar_no_meio_de_uma_espera_longa_termina_logo() {
    let ctx = Ctx::novo();
    let mut n = portal(BASE, "painel");
    let c = Cenario::novo(
        &manifesto(MFA_NENHUM, "consulta"),
        r##"navegador.abrir("https://portal.exemplo.test/entrar")
           navegador.esperar("#nunca-aparece", 120)
           navegador.registrar("não devia chegar aqui")"##,
    );
    let inicio = std::time::Instant::now();
    let (rel, _) = std::thread::scope(|s| {
        let cancelar = &ctx.cancelar;
        s.spawn(move || {
            std::thread::sleep(Duration::from_millis(150));
            cancelar.store(true, Ordering::SeqCst);
        });
        c.executar(&mut n, &ctx)
    });
    assert_eq!(rel.fim, Fim::Cancelada);
    assert!(inicio.elapsed() < Duration::from_secs(10));
    assert!(rel.registros.is_empty());
}

#[test]
fn laco_infinito_termina_com_erro_claro_e_o_historico_fecha() {
    let mut n = portal(BASE, "painel");
    let (rel, c, _) = rodar(
        MFA_NENHUM,
        "consulta",
        "while true do end",
        &mut n,
        &Ctx::novo(),
    );
    falhou_com(&rel.fim, "instruções");
    let h = c.banco().historico_escrito(None, 1).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
    // E dá para executar de novo: nada ficou rodando.
    assert!(
        !c.banco()
            .em_execucao_escrita("contracheque-exemplo")
            .unwrap()
    );
}

// ------------------------------------------------------------------------------------------
// Download e inventário (RE-27, RE-28)
// ------------------------------------------------------------------------------------------

#[test]
fn download_devolve_so_o_nome_e_repetir_o_arquivo_nao_duplica_o_achado() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        local nome = navegador.baixar("#baixar")
        navegador.registrar("nome=" .. nome .. " tipo=" .. type(nome))
        navegador.baixar("#baixar", "outro-nome")
    "##;
    let c = Cenario::novo(&manifesto(MFA_NENHUM, "consulta"), fonte);
    let mut n = portal(BASE, "painel");
    let (rel, e1) = c.executar(&mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(
        rel.registros,
        vec!["nome=contracheque.pdf tipo=string".to_owned()]
    );
    // O mesmo conteúdo duas vezes na mesma execução: dois baixados, um achado novo.
    assert_eq!((rel.baixados, rel.novos), (2, 1));

    // Uma segunda execução com o mesmo arquivo: nenhum achado novo.
    let mut n = portal(BASE, "painel");
    let (rel2, e2) = c.executar(&mut n, &Ctx::novo());
    assert_eq!((rel2.baixados, rel2.novos), (2, 0));
    assert_ne!(e1, e2);
    let achados: i64 = c
        .conn
        .query_row("SELECT count(*) FROM achado", [], |l| l.get(0))
        .unwrap();
    assert_eq!(achados, 1, "o mesmo conteúdo não vira outro achado");
    // O arquivo temporário não ficou na pasta de dados.
    assert!(!c.amb.pasta_da_execucao(e1).exists());
    assert!(!c.amb.pasta_da_execucao(e2).exists());
}

// ------------------------------------------------------------------------------------------
// Datas (RE-29)
// ------------------------------------------------------------------------------------------

#[test]
fn util_hoje_e_somar_meses_usam_o_fuso_do_usuario() {
    let fonte = r##"
        local hoje = util.hoje()
        navegador.registrar(hoje)
        navegador.registrar(util.somar_meses(hoje, 3))
        navegador.registrar(util.somar_meses("2026-01-31", 1))
        local ok, erro = pcall(util.somar_meses, "ontem", 1)
        navegador.registrar(tostring(ok))
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(
        rel.registros,
        vec![
            "2026-11-24".to_owned(),
            "2027-02-24".to_owned(),
            "2026-02-28".to_owned(),
            "false".to_owned()
        ]
    );
}

// ------------------------------------------------------------------------------------------
// Modo teste (host lógico) e produção
// ------------------------------------------------------------------------------------------

#[test]
fn modo_teste_navega_para_a_base_real_mas_confere_o_host_logico() {
    let real = "http://127.0.0.1:4101";
    let mut n = portal(real, "painel");
    let mut ctx = Ctx::novo();
    ctx.opcoes = OpcoesEscrito {
        mapa_de_hosts: vec![("portal.exemplo.test".into(), real.into())],
        aceitar_http_loopback: true,
    };
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.preencher_senha("#senha", "senha:portal-exemplo")
        navegador.clicar("#entrar")
        navegador.registrar(navegador.texto("#titulo"))
    "##;
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &ctx);
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert!(
        n.chamadas
            .contains(&"navegar http://127.0.0.1:4101/entrar".to_owned()),
        "{:?}",
        n.chamadas
    );
    assert_eq!(n.digitados, vec![("#senha".to_owned(), SENHA.to_owned())]);
    // O host da fase de login que o navegador recebe é o REAL (é o que ele compara).
    assert!(n.fases_de_login.contains(&Some("127.0.0.1".to_owned())));
}

#[test]
fn modo_teste_continua_barrando_pagina_que_nao_e_a_base_mapeada() {
    let real = "http://127.0.0.1:4101";
    let mut n = portal(real, "painel");
    // A página "fora" fica no site de outra porta: no host lógico, é de fora.
    n.pagina_nomeada(
        "fora",
        Pagina {
            url: "http://127.0.0.1:9999/x".into(),
            ..Pagina::default()
        },
    );
    let mut ctx = Ctx::novo();
    ctx.opcoes = OpcoesEscrito {
        mapa_de_hosts: vec![("portal.exemplo.test".into(), real.into())],
        aceitar_http_loopback: true,
    };
    let (rel, _c, _) = rodar(
        MFA_NENHUM,
        "consulta",
        r##"navegador.abrir("https://portal.exemplo.test/redireciona")"##,
        &mut n,
        &ctx,
    );
    falhou_com(&rel.fim, "hosts do manifesto");
}

#[test]
fn em_producao_http_para_o_proprio_computador_nao_vale() {
    let mut n = portal("http://127.0.0.1:4101", "painel");
    let (rel, _c, _) = rodar(
        MFA_NENHUM,
        "consulta",
        r##"navegador.abrir("http://127.0.0.1:4101/entrar")"##,
        &mut n,
        &Ctx::novo(),
    );
    falhou_com(&rel.fim, "hosts do manifesto");
    assert!(n.chamadas.is_empty());
}

// ------------------------------------------------------------------------------------------
// Serviço: versão ativa, uma por vez, Chrome que não abre, agenda
// ------------------------------------------------------------------------------------------

#[test]
fn executar_escrito_exige_versao_ativa_e_recusa_se_ja_roda() {
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let cancelar = AtomicBool::new(false);

    // Nada importado.
    let e = executar_escrito(&amb, "contracheque-exemplo", "janela", &Chaveiro, &cancelar);
    assert!(e.unwrap_err().to_string().contains("não tem versão ativa"));

    // Só rascunho: ainda sem versão ativa (rascunho nunca roda).
    let m = manifesto(MFA_NENHUM, "consulta");
    amb.banco()
        .unwrap()
        .importar_escrito(&m, "navegador.registrar('a')", &AutorEscrito::Usuario, "")
        .unwrap();
    let e = executar_escrito(&amb, "contracheque-exemplo", "janela", &Chaveiro, &cancelar);
    assert!(e.unwrap_err().to_string().contains("não tem versão ativa"));

    // Ativa, mas já tem uma execução rodando: recusa.
    let v = ativar(&amb, &m, "navegador.registrar('a')");
    let banco = amb.banco().unwrap();
    banco
        .iniciar_execucao_escrita(&v.roteiro, v.numero, "agenda")
        .unwrap();
    let e = executar_escrito(&amb, "contracheque-exemplo", "janela", &Chaveiro, &cancelar);
    assert!(e.unwrap_err().to_string().contains("já está rodando"));
}

#[test]
fn chrome_que_nao_abre_deixa_a_execucao_falhada_e_nao_rodando() {
    let dir = TempDir::new().unwrap();
    let (mut amb, _conn) = ambiente(&dir);
    amb.programa_do_chrome = Some(dir.path().join("nao-existe").join("chrome"));
    let m = manifesto(MFA_NENHUM, "consulta");
    ativar(&amb, &m, "navegador.registrar('a')");
    let cancelar = AtomicBool::new(false);
    let e = executar_escrito(&amb, &m.nome, "janela", &Chaveiro, &cancelar);
    assert!(e.is_err());
    let banco = amb.banco().unwrap();
    assert!(!banco.em_execucao_escrita(&m.nome).unwrap());
    let h = banco.historico_escrito(Some(&m.nome), 5).unwrap();
    assert_eq!(h.len(), 1);
    assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
    assert_eq!(h[0].versao, 1);
}

#[test]
fn a_execucao_em_andamento_trava_a_segunda_e_a_pausa_fala_pelo_banco() {
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let m = manifesto(MFA_NENHUM, "consulta");
    let v = ativar(&amb, &m, "navegador.registrar('a')");
    let banco = amb.banco().unwrap();
    let e1 = banco
        .iniciar_execucao_escrita(&v.roteiro, v.numero, "janela")
        .unwrap();
    assert!(banco.em_execucao_escrita(&m.nome).unwrap());
    assert!(
        banco
            .iniciar_execucao_escrita(&v.roteiro, v.numero, "agenda")
            .is_err()
    );
    // A janela pede "continuar" só enquanto a execução espera o usuário.
    banco.pedir_continuar_escrita(e1, true).unwrap();
    assert_eq!(banco.pedido_do_usuario_escrita(e1).unwrap(), 0);
    banco
        .marcar_aguardando_escrita(e1, "o site mostrou um CAPTCHA")
        .unwrap();
    assert_eq!(banco.aguardando_escritas().unwrap().len(), 1);
    banco.pedir_continuar_escrita(e1, true).unwrap();
    assert_eq!(banco.pedido_do_usuario_escrita(e1).unwrap(), 1);
    banco.marcar_rodando_escrita(e1).unwrap();
    assert!(banco.aguardando_escritas().unwrap().is_empty());
    banco
        .finalizar_escrita(e1, EstadoDaExecucao::Concluida, None, 0, 0)
        .unwrap();
    assert!(
        banco
            .iniciar_execucao_escrita(&v.roteiro, v.numero, "agenda")
            .is_ok()
    );
}

#[test]
fn agenda_dispara_os_devidos_um_a_um_e_a_falha_de_um_nao_impede_outro() {
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let banco = amb.banco().unwrap();
    // a e b: ativos com agenda; c: sem agenda; d: só rascunho; e: ativo e já rodando.
    for nome in ["rot-a", "rot-b", "rot-c", "rot-e"] {
        let agenda = (nome != "rot-c").then_some(24);
        let m = manifesto_nomeado(nome, MFA_NENHUM, "consulta", false, agenda);
        ativar(&amb, &m, "navegador.registrar('a')");
    }
    banco
        .importar_escrito(
            &manifesto_nomeado("rot-d", MFA_NENHUM, "consulta", false, Some(24)),
            "navegador.registrar('a')",
            &AutorEscrito::Usuario,
            "",
        )
        .unwrap();
    banco
        .iniciar_execucao_escrita("rot-e", 1, "janela")
        .unwrap();

    let agora = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap()
        + 1000;
    let cancelar = AtomicBool::new(false);
    let mut chamados: Vec<String> = Vec::new();
    let r = executar_escritos_devidos_com(&amb, agora, &cancelar, &mut |nome| {
        chamados.push(nome.to_owned());
        if nome == "rot-a" {
            return Err(ErroDeRoteiro::Invalido("o Chrome não abriu".into()));
        }
        Ok(RelatorioEscrito {
            fim: Fim::Concluida,
            baixados: 2,
            novos: 1,
            registros: vec![],
        })
    })
    .unwrap();
    assert_eq!(
        chamados,
        vec!["rot-a", "rot-b"],
        "só os ativos, com agenda e livres"
    );
    assert_eq!(r.executados, 2);
    assert_eq!(r.concluidos, 1);
    assert_eq!((r.baixados, r.novos), (2, 1));

    // Cancelado: não roda ninguém.
    cancelar.store(true, Ordering::SeqCst);
    let mut n = 0;
    let r = executar_escritos_devidos_com(&amb, agora, &cancelar, &mut |_| {
        n += 1;
        Err(ErroDeRoteiro::Cancelado)
    })
    .unwrap();
    assert_eq!((r.executados, n), (0, 0));
}

#[test]
fn agenda_respeita_o_intervalo_desde_a_ultima_execucao_e_encerra_orfas() {
    let dir = TempDir::new().unwrap();
    let (amb, _conn) = ambiente(&dir);
    let m = manifesto_nomeado("rot-a", MFA_NENHUM, "consulta", false, Some(2));
    let v = ativar(&amb, &m, "navegador.registrar('a')");
    let banco = amb.banco().unwrap();
    let agora = i64::try_from(
        std::time::SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs(),
    )
    .unwrap();
    assert_eq!(
        banco.escritos_devidos(agora).unwrap(),
        vec!["rot-a".to_owned()]
    );

    // Uma execução órfã (o processo que a tocava não existe mais).
    let e = banco
        .iniciar_execucao_escrita(&v.roteiro, v.numero, "agenda")
        .unwrap();
    Connection::open(amb.arquivo_do_banco())
        .unwrap()
        .execute(
            "UPDATE execucao_escrita SET pid = 999999999 WHERE id = ?1",
            [e],
        )
        .unwrap();
    let cancelar = AtomicBool::new(false);
    let r = executar_escritos_devidos_com(&amb, agora + 3 * 3600, &cancelar, &mut |_| {
        Ok(RelatorioEscrito {
            fim: Fim::Concluida,
            baixados: 0,
            novos: 0,
            registros: vec![],
        })
    })
    .unwrap();
    assert_eq!(r.executados, 1, "a órfã foi encerrada e o roteiro rodou");
    let h = banco.historico_escrito(Some("rot-a"), 5).unwrap();
    assert_eq!(h[0].estado, EstadoDaExecucao::Falhou);
    assert!(h[0].motivo.as_deref().unwrap().contains("interrompida"));

    // Dentro do intervalo de 2 horas desde a última execução: não é devido; passando, é.
    assert!(banco.escritos_devidos(agora + 3600).unwrap().is_empty());
    assert_eq!(
        banco.escritos_devidos(agora + 3 * 3600).unwrap(),
        vec!["rot-a".to_owned()]
    );
}

// ------------------------------------------------------------------------------------------
// `navegador.esperar_sumir` no host do SBF (implementação padrão do trait, sobre `existe`)
// ------------------------------------------------------------------------------------------

#[test]
fn esperar_sumir_devolve_true_quando_o_elemento_sai_da_tela() {
    // O clique em #entrar leva ao painel: o #entrar do login deixa de estar visível.
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/entrar")
        navegador.clicar("#entrar")
        navegador.registrar("sumiu=" .. tostring(navegador.esperar_sumir("#entrar", 2)))
        navegador.registrar("ja_ausente=" .. tostring(navegador.esperar_sumir("#nao-existe", 2)))
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(rel.registros, vec!["sumiu=true", "ja_ausente=true"]);
}

#[test]
fn esperar_sumir_devolve_false_quando_o_prazo_acaba_com_o_elemento_na_tela() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        navegador.registrar("sumiu=" .. tostring(navegador.esperar_sumir("#titulo", 1)))
    "##;
    let mut n = portal(BASE, "painel");
    let (rel, _c, _) = rodar(MFA_NENHUM, "consulta", fonte, &mut n, &Ctx::novo());
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(rel.registros, vec!["sumiu=false"]);
}

#[test]
fn esperar_sumir_respeita_o_cancelamento_no_meio_da_espera() {
    let fonte = r##"
        navegador.abrir("https://portal.exemplo.test/painel")
        navegador.esperar_sumir("#titulo", 100)
        navegador.registrar("nao devia chegar aqui")
    "##;
    let mut n = portal(BASE, "painel");
    let ctx = Ctx::novo();
    let c = Cenario::novo(&manifesto(MFA_NENHUM, "consulta"), fonte);
    let inicio = std::time::Instant::now();
    let (rel, _) = std::thread::scope(|s| {
        s.spawn(|| {
            std::thread::sleep(Duration::from_millis(400));
            ctx.cancelar.store(true, Ordering::SeqCst);
        });
        c.executar(&mut n, &ctx)
    });
    assert_eq!(rel.fim, Fim::Cancelada, "{rel:?}");
    assert!(rel.registros.is_empty(), "{:?}", rel.registros);
    assert!(
        inicio.elapsed() < Duration::from_secs(20),
        "a espera de 100 s tem de acabar com o cancelamento"
    );
}
