//! O `testar` do roteiro escrito (spec 14, §5): o cenário `contracheque` roda a versão contra o
//! portal falso local (127.0.0.1). Com `SBF_CHROME_TESTE=1` o Chrome é o de verdade (sem janela,
//! perfil descartável); sem ele, o mesmo caminho roda com o navegador falso, que cobre o que não
//! depende de o portal responder (bloqueio, código que não chega, cenário incompatível, versão
//! que não é rascunho, Chrome ausente). Nada toca rede externa nem dado real.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_navegador::erro::Resultado;
use bancada_navegador::falso::{Elemento, NavegadorFalso};
use bancada_navegador::pagina::{Campo, Formulario, Pagina};
use bancada_navegador::politica::{Bloqueio, Candidato};
use comum::{clicavel, inventario};
use rusqlite::Connection;
use sbf_chaveiro::Segredo;
use sbf_roteiros::banco::{
    AutorEscrito, BancoDeRoteiros, Confirmacao, EstadoEscrito, ResultadoDoTeste,
};
use sbf_roteiros::chrome::OpcoesDoChrome;
use sbf_roteiros::executor::Opcoes;
use sbf_roteiros::manifesto::Manifesto;
use sbf_roteiros::navegador::Navegador;
use sbf_roteiros::servico::Ambiente;
use sbf_roteiros::teste_escrito::{Cenario, testar_em, testar_escrito};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MANIFESTO: &str = include_str!("../exemplos/contracheque-exemplo.json");
const FONTE: &str = include_str!("../exemplos/contracheque-exemplo.luau");

fn chrome_ligado() -> bool {
    if std::env::var("SBF_CHROME_TESTE").ok().as_deref() == Some("1") {
        return true;
    }
    eprintln!("pulado: defina SBF_CHROME_TESTE=1 para rodar com o Chrome");
    false
}

fn ambiente(d: &Path) -> (Ambiente, PathBuf) {
    let (cofre, _conn) = inventario(d);
    let mut a = Ambiente::novo(&d.join("dados"), &cofre);
    a.opcoes = Opcoes {
        espera: Duration::from_millis(60),
        download: Duration::from_secs(1),
        intervalo: Duration::from_millis(5),
    };
    a.prazo_da_pausa = Duration::from_secs(5);
    a.sem_janela = true;
    let inv = cofre.join("inventario.sqlite");
    (a, inv)
}

/// Um ambiente para o Chrome de verdade: prazos de uso real.
fn ambiente_chrome(d: &Path) -> (Ambiente, PathBuf) {
    let (mut a, inv) = ambiente(d);
    a.opcoes = Opcoes {
        espera: Duration::from_secs(15),
        download: Duration::from_secs(30),
        intervalo: Duration::from_millis(100),
    };
    (a, inv)
}

/// Importa um roteiro como rascunho (manifesto `json` e `fonte`) e devolve o número da versão.
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

/// As linhas `origin_key` do inventário que são de roteiro.
fn origens(inventario: &Path) -> Vec<(String, String)> {
    let c = Connection::open(inventario).unwrap();
    let mut st = c
        .prepare(
            "SELECT origin_key, filename FROM origem WHERE origin_type = 'roteiro' ORDER BY id",
        )
        .unwrap();
    st.query_map([], |l| Ok((l.get(0)?, l.get(1)?)))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

// ------------------------------------------------------------------------------------------
// Chrome de verdade
// ------------------------------------------------------------------------------------------

#[test]
fn o_exemplo_passa_no_portal_falso_com_chrome_e_o_pdf_entra_no_cofre_temporario() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, inv) = ambiente_chrome(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, FONTE);
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);

    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let r = testar_em(&amb, &nome, n, Cenario::Contracheque, &mut chrome).unwrap();
    chrome.fechar();
    eprintln!("resumo: {}", r.resumo);
    assert!(r.passou, "{}", r.resumo);
    assert!(r.resumo.contains("baixado"), "{}", r.resumo);
    for segredo in ["SenhaFicticia", "123.456.789-09", "12345678909"] {
        assert!(!r.resumo.contains(segredo), "o resumo vazou segredo");
    }

    // a versão virou `testado`, com o resumo no histórico
    let v = amb.banco().unwrap().versao_escrita(&nome, n).unwrap();
    assert_eq!(v.estado, EstadoEscrito::Testado);
    assert!(v.resultado_do_teste.unwrap().starts_with("passou"));

    // o PDF fictício entrou no inventário do cofre temporário, com origem `roteiro:`
    let o = origens(&inv);
    assert_eq!(o.len(), 1, "{o:?}");
    assert!(
        o[0].0.starts_with("roteiro:contracheque-exemplo/"),
        "{}",
        o[0].0
    );
    assert!(o[0].1.starts_with("contracheque-") && o[0].1.ends_with(".pdf"));

    // o caminho de teste do banco aceita ativar uma versão testada (como a janela faz)
    let banco = amb.banco().unwrap();
    let digitado = Confirmacao::texto_de_ativar(&nome, n);
    let c = Confirmacao::do_terminal(true, &digitado, &digitado).unwrap();
    banco.ativar(&nome, n, &c).unwrap();
    assert_eq!(
        banco.versao_ativa_escrita(&nome).unwrap().unwrap().estado,
        EstadoEscrito::Ativo
    );
}

#[test]
fn testar_escrito_abre_chrome_proprio_e_nao_toca_cofre_nem_perfil_do_ambiente() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, inv) = ambiente_chrome(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, FONTE);

    let r = testar_escrito(&amb, &nome, n, Cenario::Contracheque).unwrap();
    eprintln!("resumo: {}", r.resumo);
    assert!(r.passou, "{}", r.resumo);
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Testado);

    // o cofre do ambiente (o "real") ficou intocado e o perfil dedicado nem foi criado
    assert!(origens(&inv).is_empty(), "o teste escreveu no cofre real");
    assert!(!amb.perfil().exists(), "o teste usou o perfil dedicado");
    // e a zona temporária sumiu
    let zona = amb.pasta_dados.join("roteiros").join("teste");
    let sobras = std::fs::read_dir(&zona).map_or(0, Iterator::count);
    assert_eq!(sobras, 0, "sobrou a pasta temporária do teste");
}

// ------------------------------------------------------------------------------------------
// O navegador falso: o mesmo caminho do `testar`, sem Chrome
// ------------------------------------------------------------------------------------------

/// O portal fictício em memória. O `testar` navega para `http://127.0.0.1:<porta>/...` (a porta
/// só se sabe lá dentro); este adaptador a traduz para as rotas lógicas do navegador falso.
struct PortalEmMemoria {
    dentro: NavegadorFalso,
    base: Option<String>,
}

const LOGICO: &str = "https://portal.exemplo.test";

fn campo(seletor: &str, tipo: &str) -> Campo {
    Campo {
        seletor: seletor.into(),
        tipo: tipo.into(),
        ..Campo::default()
    }
}

fn de_campo(tipo: &str, senha: bool) -> Elemento {
    let mut e = Elemento::com_texto("");
    e.candidato.eh_campo = true;
    e.candidato.eh_senha = senha;
    e.candidato.tipo = tipo.into();
    e
}

impl PortalEmMemoria {
    fn novo() -> Self {
        let mut n = NavegadorFalso::default();
        n.rotas.insert(format!("{LOGICO}/entrar"), "entrar".into());
        n.pagina_nomeada(
            "entrar",
            Pagina {
                url: format!("{LOGICO}/entrar"),
                titulo: "Entrar".into(),
                texto: "Acesse seu contracheque".into(),
                elementos: vec![
                    clicavel("#entrar", "Entrar"),
                    clicavel("#pagar-boleto", "Pagar boleto"),
                ],
                formularios: vec![Formulario {
                    seletor: "#f".into(),
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
        n.elementos.insert("#cpf".into(), de_campo("text", false));
        n.elementos
            .insert("#senha".into(), de_campo("password", true));
        n.elementos
            .insert("#codigo".into(), de_campo("text", false));
        n.elementos
            .insert("#entrar".into(), Elemento::com_texto("Entrar"));
        n.elementos
            .insert("#pagar-boleto".into(), Elemento::com_texto("Pagar boleto"));
        n.atual = "entrar".into();
        Self {
            dentro: n,
            base: None,
        }
    }
}

impl Navegador for PortalEmMemoria {
    fn navegar(&mut self, url: &str) -> Resultado<()> {
        // `http://127.0.0.1:<porta>/x` -> `https://portal.exemplo.test/x`
        let logico = match url.strip_prefix("http://127.0.0.1:") {
            Some(resto) => {
                let (porta, caminho) = resto.split_once('/').unwrap_or((resto, ""));
                self.base = Some(format!("http://127.0.0.1:{porta}"));
                format!("{LOGICO}/{caminho}")
            }
            None => url.to_owned(),
        };
        self.dentro.navegar(&logico)
    }
    fn visivel(&mut self, seletor: &str) -> Resultado<bool> {
        self.dentro.visivel(seletor)
    }
    fn descrever(&mut self, seletor: &str) -> Resultado<Option<Candidato>> {
        self.dentro.descrever(seletor)
    }
    fn clicar(&mut self, seletor: &str) -> Resultado<()> {
        self.dentro.clicar(seletor)
    }
    fn preencher(&mut self, seletor: &str, texto: &str) -> Resultado<()> {
        self.dentro.preencher(seletor, texto)
    }
    fn preencher_segredo(&mut self, seletor: &str, segredo: &Segredo) -> Resultado<()> {
        self.dentro.preencher_segredo(seletor, segredo)
    }
    fn clicar_e_baixar(
        &mut self,
        seletor: &str,
        pasta: &Path,
        prazo: Duration,
    ) -> Resultado<PathBuf> {
        self.dentro.clicar_e_baixar(seletor, pasta, prazo)
    }
    fn texto(&mut self, seletor: &str) -> Resultado<String> {
        self.dentro.texto(seletor)
    }
    fn ler_tabela(&mut self, seletor: &str) -> Resultado<Vec<Vec<String>>> {
        self.dentro.ler_tabela(seletor)
    }
    fn pagina(&mut self) -> Resultado<Pagina> {
        let mut p = self.dentro.pagina()?;
        if let Some(base) = &self.base {
            p.url = p.url.replacen(LOGICO, base, 1);
        }
        Ok(p)
    }
    fn bloqueios(&mut self) -> Resultado<Vec<Bloqueio>> {
        self.dentro.bloqueios()
    }
    fn fase_de_login(&mut self, host: Option<&str>) {
        self.dentro.fase_de_login(host);
    }
}

/// Um roteiro que tenta clicar em "Pagar boleto". O navegador falso não conversa com o portal
/// (o código nunca sairia), então ele vai direto ao botão; o caminho completo (login, código,
/// lista e só então o clique) roda no teste com Chrome.
const FONTE_QUE_PAGA: &str = r##"
navegador.abrir("https://portal.exemplo.test/entrar")
navegador.esperar("#pagar-boleto", 5)
navegador.clicar("#pagar-boleto")
"##;

/// Um roteiro que espera um código de e-mail que nunca foi pedido: a caixa está vazia.
const FONTE_SEM_CODIGO: &str = r##"
navegador.abrir("https://portal.exemplo.test/entrar")
navegador.esperar("#codigo", 5)
navegador.preencher_codigo_do_email("#codigo", { janela_segundos = 2 })
navegador.registrar("não devia chegar aqui")
"##;

fn rodar_falso(fonte: &str) -> (Ambiente, String, u32, ResultadoDoTeste, tempfile::TempDir) {
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, fonte);
    let mut nav = PortalEmMemoria::novo();
    let r = testar_em(&amb, &nome, n, Cenario::Contracheque, &mut nav).unwrap();
    (amb, nome, n, r, d)
}

#[test]
fn clicar_em_pagar_boleto_e_barrado_e_o_teste_falha_mantendo_rascunho() {
    let (amb, nome, n, r, _d) = rodar_falso(FONTE_QUE_PAGA);
    assert!(!r.passou, "{}", r.resumo);
    assert!(
        r.resumo.contains("lista de bloqueio"),
        "o resumo tem de dizer que o bloqueio barrou: {}",
        r.resumo
    );
    assert!(r.resumo.starts_with("falhou"), "{}", r.resumo);
    // continua rascunho, com o motivo no histórico: não pode ser ativado
    let v = amb.banco().unwrap().versao_escrita(&nome, n).unwrap();
    assert_eq!(v.estado, EstadoEscrito::Rascunho);
    assert!(v.resultado_do_teste.unwrap().starts_with("falhou"));
    let digitado = Confirmacao::texto_de_ativar(&nome, n);
    let c = Confirmacao::do_terminal(true, &digitado, &digitado).unwrap();
    assert!(amb.banco().unwrap().ativar(&nome, n, &c).is_err());
}

#[test]
fn codigo_que_nao_chega_expira_e_o_teste_falha_sem_travar() {
    let inicio = std::time::Instant::now();
    let (amb, nome, n, r, _d) = rodar_falso(FONTE_SEM_CODIGO);
    assert!(!r.passou, "{}", r.resumo);
    assert!(r.resumo.contains("código"), "{}", r.resumo);
    assert!(
        inicio.elapsed() < Duration::from_secs(20),
        "a janela de 2 s não pode travar o teste: {:?}",
        inicio.elapsed()
    );
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);
}

#[test]
fn o_resumo_do_teste_nao_leva_segredo() {
    let (_amb, _nome, _n, r, _d) = rodar_falso(FONTE_QUE_PAGA);
    for segredo in ["SenhaFicticia", "123.456.789-09", "12345678909"] {
        assert!(!r.resumo.contains(segredo), "{}", r.resumo);
    }
}

#[test]
fn roteiro_de_outro_cenario_e_recusado_com_erro_claro_e_sem_rodar() {
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente(d.path());
    // MFA `nenhum`: não é o contracheque do portal falso (que exige código por e-mail)
    let json = MANIFESTO.replace(
        r#""mfa": {
    "tipo": "email",
    "remetente": "nao-responda@portal.exemplo.test",
    "formato": "\\d{6}"
  }"#,
        r#""mfa": { "tipo": "nenhum" }"#,
    );
    assert!(json.contains(r#""tipo": "nenhum""#), "a troca não valeu");
    let (nome, n) = importar(&amb, &json, FONTE);
    let mut nav = PortalEmMemoria::novo();
    let e = testar_em(&amb, &nome, n, Cenario::Contracheque, &mut nav)
        .unwrap_err()
        .to_string();
    assert!(
        e.contains("este roteiro não é do cenário contracheque"),
        "{e}"
    );
    assert!(nav.dentro.chamadas.is_empty(), "o teste nem devia navegar");
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);
    assert!(
        amb.banco()
            .unwrap()
            .versao_escrita(&nome, n)
            .unwrap()
            .resultado_do_teste
            .is_none(),
        "um erro de cenário não é resultado de teste"
    );
}

#[test]
fn so_rascunho_pode_ser_testado() {
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente(d.path());
    let (nome, n) = importar(&amb, MANIFESTO, FONTE);
    amb.banco()
        .unwrap()
        .marcar_testado(
            &nome,
            n,
            &ResultadoDoTeste {
                passou: true,
                resumo: "ok".into(),
            },
        )
        .unwrap();
    let mut nav = PortalEmMemoria::novo();
    let e = testar_em(&amb, &nome, n, Cenario::Contracheque, &mut nav)
        .unwrap_err()
        .to_string();
    assert!(e.contains("só um rascunho pode ser testado"), "{e}");
}

#[test]
fn versao_inexistente_e_erro() {
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente(d.path());
    let mut nav = PortalEmMemoria::novo();
    assert!(testar_em(&amb, "nao-existe", 1, Cenario::Contracheque, &mut nav).is_err());
}

#[test]
fn chrome_ausente_e_erro_claro_a_versao_continua_rascunho_e_nada_sobra() {
    let d = tempfile::tempdir().unwrap();
    let (mut amb, inv) = ambiente(d.path());
    amb.programa_do_chrome = Some(d.path().join("nao-existe").join("chrome"));
    let (nome, n) = importar(&amb, MANIFESTO, FONTE);
    let e = testar_escrito(&amb, &nome, n, Cenario::Contracheque)
        .unwrap_err()
        .to_string();
    assert!(e.contains("Chrome"), "{e}");
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);
    assert!(origens(&inv).is_empty());
    let zona = amb.pasta_dados.join("roteiros").join("teste");
    assert_eq!(std::fs::read_dir(&zona).map_or(0, Iterator::count), 0);
    // o banco sobrevive (o `BancoDeRoteiros` abre de novo)
    BancoDeRoteiros::abrir(&amb.arquivo_do_banco()).unwrap();
}

// ------------------------------------------------------------------------------------------
// Os caminhos negativos também com o Chrome de verdade
// ------------------------------------------------------------------------------------------

#[test]
fn com_chrome_pagar_boleto_e_barrado_e_o_portal_nao_e_alcancado() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente_chrome(d.path());
    let fonte = r##"
navegador.abrir("https://portal.exemplo.test/entrar")
navegador.esperar("#cpf", 20)
navegador.preencher_cpf("#cpf")
navegador.preencher_senha("#senha", "senha:portal-exemplo")
navegador.clicar("#entrar")
navegador.esperar("#codigo", 20)
navegador.preencher_codigo_do_email("#codigo", { janela_segundos = 30 })
navegador.clicar("#confirmar")
navegador.esperar("#lista", 30)
navegador.clicar("#pagar-boleto")
"##;
    let (nome, n) = importar(&amb, MANIFESTO, fonte);
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let r = testar_em(&amb, &nome, n, Cenario::Contracheque, &mut chrome).unwrap();
    chrome.fechar();
    eprintln!("resumo: {}", r.resumo);
    assert!(!r.passou, "{}", r.resumo);
    assert!(r.resumo.contains("lista de bloqueio"), "{}", r.resumo);
    // o portal não registrou acesso proibido: o bloqueio barrou antes
    assert!(
        !r.resumo.contains("acesso proibido"),
        "o clique chegou ao portal: {}",
        r.resumo
    );
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);
}

#[test]
fn com_chrome_codigo_que_nao_chega_expira_sem_travar() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente_chrome(d.path());
    // A caixa só tem código depois de um login; sem login, nunca chega. O campo de CPF serve
    // de alvo: o verbo espera o e-mail antes de digitar.
    let fonte = r##"
navegador.abrir("https://portal.exemplo.test/entrar")
navegador.esperar("#cpf", 20)
navegador.preencher_codigo_do_email("#cpf", { janela_segundos = 2 })
"##;
    let (nome, n) = importar(&amb, MANIFESTO, fonte);
    let inicio = std::time::Instant::now();
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let r = testar_em(&amb, &nome, n, Cenario::Contracheque, &mut chrome).unwrap();
    chrome.fechar();
    eprintln!("resumo: {}", r.resumo);
    assert!(!r.passou, "{}", r.resumo);
    assert!(r.resumo.contains("código"), "{}", r.resumo);
    assert!(inicio.elapsed() < Duration::from_secs(60));
    assert_eq!(estado(&amb, &nome, n), EstadoEscrito::Rascunho);
}
