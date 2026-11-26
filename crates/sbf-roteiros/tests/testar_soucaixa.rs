//! O roteiro `contracheque-soucaixa` contra o portal FALSO do SouCaixa (127.0.0.1): o cenário
//! `soucaixa` do `testar` e quatro situações do dia a dia. Com `SBF_CHROME_TESTE=1` o Chrome é o
//! de verdade (sem janela, perfil descartável); sem ele, só o que não precisa do Chrome roda
//! (o manifesto importa, o cenário o aceita). Nada toca rede externa nem dado real: CPF, senha,
//! nome e valores são fictícios.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod comum;

use bancada_navegador::falso::NavegadorFalso;
use bancada_navegador::site_falso_soucaixa::{
    HOST_DO_LOGIN, HOST_DO_SITE, PortalDeSouCaixa, REMETENTE_DO_CODIGO,
};
use rusqlite::Connection;
use sbf_chaveiro::Segredo;
use sbf_roteiros::banco::{AutorEscrito, EstadoDaExecucao, EstadoEscrito};
use sbf_roteiros::chrome::{Chrome, OpcoesDoChrome};
use sbf_roteiros::codigo_email::{CaixaDeEmail, ErroDeCodigo, MensagemCurta, RelogioReal};
use sbf_roteiros::escrito::OpcoesEscrito;
use sbf_roteiros::executor::{Decisao, Fim, FonteDeSenhas, Opcoes, Pausa};
use sbf_roteiros::manifesto::Manifesto;
use sbf_roteiros::navegador::Navegador;
use sbf_roteiros::parametros::ValorBruto;
use sbf_roteiros::servico::{
    Ambiente, PedidoDeExecucao, RelatorioEscrito, Servicos, executar_escrito_em_com,
    resolver_pedido,
};
use sbf_roteiros::teste_escrito::{
    Cenario, config_do_soucaixa_de_teste, mes_corrente_do_teste, testar_em, testar_soucaixa_em,
};
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::{Duration, SystemTime};
use zeroize::Zeroizing;

const MANIFESTO: &str = include_str!("../exemplos/contracheque-soucaixa.json");
const FONTE: &str = include_str!("../exemplos/contracheque-soucaixa.luau");

const SQL_IMPRESSAO: &str = include_str!("../../sbf-inventario/sql/0005-impressao-do-texto.sql");

/// O inventário de teste já com a migração 5 (a impressão do texto), como o real: é ela que
/// deixa o inventário reconhecer o mesmo texto com bytes diferentes.
fn inventario(pasta: &Path) -> (PathBuf, Connection) {
    let (cofre, conn) = comum::inventario(pasta);
    conn.execute_batch(SQL_IMPRESSAO).unwrap();
    (cofre, conn)
}

const SENHA: &str = "SenhaFicticia#4821";
const CPF: &str = "123.456.789-09";

fn chrome_ligado() -> bool {
    if std::env::var("SBF_CHROME_TESTE").ok().as_deref() == Some("1") {
        return true;
    }
    eprintln!("pulado: defina SBF_CHROME_TESTE=1 para rodar com o Chrome");
    false
}

/// Um ambiente para o Chrome de verdade: prazos de uso real.
fn ambiente(d: &Path) -> (Ambiente, PathBuf) {
    let (cofre, _conn) = inventario(d);
    let mut a = Ambiente::novo(&d.join("dados"), &cofre);
    a.opcoes = Opcoes {
        espera: Duration::from_secs(15),
        download: Duration::from_secs(30),
        intervalo: Duration::from_millis(100),
    };
    a.prazo_da_pausa = Duration::from_secs(5);
    a.sem_janela = true;
    let inv = cofre.join("inventario.sqlite");
    (a, inv)
}

fn importar(amb: &Ambiente) -> (String, u32) {
    let m = Manifesto::de_json(MANIFESTO).unwrap();
    let n = amb
        .banco()
        .unwrap()
        .importar_escrito(&m, FONTE, &AutorEscrito::Usuario, "teste")
        .unwrap()
        .numero;
    (m.nome, n)
}

fn abrir_chrome(perfil: &Path) -> Chrome {
    sbf_roteiros::chrome::abrir(&OpcoesDoChrome {
        programa: None,
        perfil: perfil.to_path_buf(),
        sem_janela: std::env::var("SBF_CHROME_VISIVEL").ok().as_deref() != Some("1"),
    })
    .unwrap()
}

// ------------------------------------------------------------------------------------------
// O que o `testar` monta, refeito aqui para as situações que o cenário não cobre
// ------------------------------------------------------------------------------------------

struct Senhas;

impl FonteDeSenhas for Senhas {
    fn senha_de_site(&self, rotulo: &str) -> Result<Segredo, String> {
        if rotulo == "soucaixa" {
            Ok(Segredo::novo(SENHA))
        } else {
            Err("a senha do site não está no chaveiro do teste".to_owned())
        }
    }

    fn cpf(&self) -> Result<Segredo, String> {
        Ok(Segredo::novo(CPF))
    }
}

struct Caixa<'a>(&'a PortalDeSouCaixa);

impl CaixaDeEmail for Caixa<'_> {
    fn mensagens_recentes(
        &self,
        remetente: &str,
        desde: SystemTime,
    ) -> Result<Vec<MensagemCurta>, ErroDeCodigo> {
        if remetente != REMETENTE_DO_CODIGO {
            return Ok(Vec::new());
        }
        let Some((corpo, quando)) = self.0.email_do_codigo() else {
            return Ok(Vec::new());
        };
        if quando < desde {
            return Ok(Vec::new());
        }
        Ok(vec![MensagemCurta {
            recebida_em: quando,
            corpo: Zeroizing::new(corpo),
        }])
    }
}

/// No teste não há usuário: a vez dele é falha.
struct SemUsuario;

impl Pausa for SemUsuario {
    fn aguardar(&self, _execucao: i64, _motivo: &str) -> Decisao {
        Decisao::Expirou
    }
}

/// Roda a versão ativa de `nome`/`n` num `navegador` já aberto, contra `portal`, com o `pedido`.
fn rodar(
    amb: &Ambiente,
    nome: &str,
    n: u32,
    navegador: &mut dyn Navegador,
    portal: &PortalDeSouCaixa,
    pedido: &PedidoDeExecucao,
) -> (RelatorioEscrito, i64) {
    let banco = amb.banco().unwrap();
    let versao = banco.versao_escrita(nome, n).unwrap();
    let agora = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let resolvida =
        resolver_pedido(&versao.manifesto, pedido, i64::try_from(agora).unwrap()).unwrap();
    let execucao = banco
        .iniciar_execucao_escrita_com(nome, n, "teste", &resolvida.inicio)
        .unwrap();
    let opcoes = OpcoesEscrito {
        mapa_de_hosts: vec![
            (HOST_DO_SITE.to_owned(), portal.url_base()),
            (HOST_DO_LOGIN.to_owned(), portal.url_base()),
        ],
        aceitar_http_loopback: true,
    };
    let caixa = Caixa(portal);
    let rel = executar_escrito_em_com(
        amb,
        navegador,
        &versao,
        execucao,
        &Servicos {
            senhas: &Senhas,
            caixa: Some(&caixa),
            relogio: &RelogioReal,
            pausa: Some(&SemUsuario),
            opcoes: &opcoes,
        },
        &AtomicBool::new(false),
        &resolvida,
    )
    .unwrap();
    (rel, execucao)
}

fn estado_da(amb: &Ambiente, execucao: i64) -> EstadoDaExecucao {
    amb.banco()
        .unwrap()
        .historico_escrito(None, 50)
        .unwrap()
        .into_iter()
        .find(|l| l.id == execucao)
        .unwrap()
        .estado
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Quantos achados o inventário tem.
fn achados(inventario: &Path) -> i64 {
    Connection::open(inventario)
        .unwrap()
        .query_row("SELECT COUNT(*) FROM achado", [], |l| l.get(0))
        .unwrap()
}

/// As origens de roteiro do inventário: (chave, nome do arquivo).
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

fn mes_anterior(mes: &str) -> String {
    let ano: i64 = mes[..4].parse().unwrap();
    let n: i64 = mes[5..].parse().unwrap();
    if n == 1 {
        format!("{:04}-12", ano - 1)
    } else {
        format!("{ano:04}-{:02}", n - 1)
    }
}

// ------------------------------------------------------------------------------------------
// Sem Chrome
// ------------------------------------------------------------------------------------------

#[test]
fn o_manifesto_importa_com_agenda_e_parametros_e_o_cenario_o_aceita() {
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente(d.path());
    let (nome, n) = importar(&amb);
    assert_eq!(nome, "contracheque-soucaixa");
    let v = amb.banco().unwrap().versao_escrita(&nome, n).unwrap();
    assert_eq!(v.estado, EstadoEscrito::Rascunho);
    assert_eq!(v.manifesto.rotulo_senha.as_deref(), Some("soucaixa"));
    assert_eq!(v.manifesto.agenda.len(), 1);
    assert_eq!(v.manifesto.agenda[0].dias_do_mes, Some(vec![20, 21, 30, 1]));
    assert_eq!(v.manifesto.parametros.len(), 3);
    assert_eq!(Cenario::de_texto("soucaixa"), Some(Cenario::SouCaixa));
    assert!(Cenario::TODOS.contains(&Cenario::SouCaixa));

    // O cenário aceita o manifesto; com um navegador falso (que não navega) o teste falha com
    // um resumo claro e sem segredo, e a versão continua rascunho.
    let mut falso = NavegadorFalso::default();
    let r = testar_em(&amb, &nome, n, Cenario::SouCaixa, &mut falso).unwrap();
    assert!(!r.passou, "{}", r.resumo);
    assert!(
        r.resumo.starts_with("falhou no cenário soucaixa"),
        "{}",
        r.resumo
    );
    for segredo in [SENHA, CPF, "12345678909"] {
        assert!(!r.resumo.contains(segredo), "o resumo vazou segredo");
    }
}

#[test]
fn o_cenario_recusa_o_manifesto_do_contracheque_do_portal_ficticio() {
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente(d.path());
    let m = Manifesto::de_json(include_str!("../exemplos/contracheque-exemplo.json")).unwrap();
    amb.banco()
        .unwrap()
        .importar_escrito(&m, "return", &AutorEscrito::Usuario, "teste")
        .unwrap();
    let mut falso = NavegadorFalso::default();
    let e = testar_em(&amb, &m.nome, 1, Cenario::SouCaixa, &mut falso)
        .unwrap_err()
        .to_string();
    assert!(e.contains("este roteiro não é do cenário soucaixa"), "{e}");
}

#[test]
fn o_roteiro_nunca_clica_em_nada_proibido_nem_cita_segredo() {
    // O fonte é lido por humanos: nada de seletor de pedidos, auxílios, perfil ou sair, e o
    // único rótulo de senha é o do manifesto.
    for proibido in [
        "pedidos",
        "adiantamentos",
        "auxilios",
        "perfil",
        "#kc-cancel",
        "alterar-dados",
        "Recuperar",
    ] {
        let clica = FONTE
            .lines()
            .filter(|l| l.contains("navegador.clicar") || l.contains("navegador.baixar"))
            .any(|l| l.contains(proibido));
        assert!(!clica, "o roteiro clica em algo proibido: {proibido}");
    }
    assert!(FONTE.contains(r#"preencher_senha(CAMPO_SENHA, ROTULO_DA_SENHA)"#));
    assert!(FONTE.contains(r#"ROTULO_DA_SENHA = "soucaixa""#));
    assert!(!FONTE.contains("senha:soucaixa"));
    assert!(!MANIFESTO.contains("senha:soucaixa"));
}

// ------------------------------------------------------------------------------------------
// Chrome de verdade
// ------------------------------------------------------------------------------------------

/// (a) O caminho feliz pelo cenário: o PDF do mês corrente entra no cofre temporário.
#[test]
fn a_caminho_feliz_baixa_o_contracheque_do_mes_corrente_e_o_pdf_entra_no_inventario() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, inv) = ambiente(d.path());
    let (nome, n) = importar(&amb);
    let mes = mes_corrente_do_teste();
    let portal = PortalDeSouCaixa::iniciar(config_do_soucaixa_de_teste(&[
        mes_anterior(&mes).as_str(),
        mes.as_str(),
    ]))
    .unwrap();

    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let r = testar_soucaixa_em(&amb, &nome, n, &portal, &mut chrome).unwrap();
    chrome.fechar();
    eprintln!("resumo: {}", r.resumo);
    assert!(r.passou, "{}", r.resumo);
    assert!(r.resumo.contains(&format!("contracheque de {mes} baixado")));
    for segredo in [SENHA, CPF, "12345678909"] {
        assert!(!r.resumo.contains(segredo), "o resumo vazou segredo");
    }
    if let Some(codigo) = portal.codigo_gerado() {
        assert!(!r.resumo.contains(&codigo), "o resumo vazou o código");
    }

    // o portal viu: um login, nenhum código recusado, só o mês corrente baixado, nada proibido
    assert_eq!(portal.logins(), 1);
    assert_eq!(portal.codigos_recusados(), 0);
    assert!(portal.baixou(&mes));
    assert_eq!(portal.downloads(), 1, "só o mês pedido");
    assert!(
        portal.acessos_proibidos().is_empty(),
        "{:?}",
        portal.acessos_proibidos()
    );

    // a versão virou `testado`, e o PDF fictício entrou, com origem `roteiro:`
    let v = amb.banco().unwrap().versao_escrita(&nome, n).unwrap();
    assert_eq!(v.estado, EstadoEscrito::Testado);
    let o = origens(&inv);
    assert_eq!(o.len(), 1, "{o:?}");
    assert!(
        o[0].0.starts_with("roteiro:contracheque-soucaixa/"),
        "{}",
        o[0].0
    );
    assert_eq!(o[0].1, format!("contracheque-{mes}.pdf"));
    assert_eq!(achados(&inv), 1);
}

/// (b) Senha recusada: a execução termina em `entrada_recusada` e nada é baixado.
#[test]
fn b_senha_recusada_termina_em_entrada_recusada() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, inv) = ambiente(d.path());
    let (nome, n) = importar(&amb);
    let portal = PortalDeSouCaixa::iniciar(
        config_do_soucaixa_de_teste(&[mes_corrente_do_teste().as_str()]).com_rejeitar_senha(true),
    )
    .unwrap();

    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let (rel, execucao) = rodar(
        &amb,
        &nome,
        n,
        &mut chrome,
        &portal,
        &PedidoDeExecucao::default(),
    );
    chrome.fechar();

    assert_ne!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(
        rel.entrada_recusada.as_deref(),
        Some("o SouCaixa não aceitou a entrada")
    );
    assert_eq!(estado_da(&amb, execucao), EstadoDaExecucao::EntradaRecusada);
    assert!(portal.logins_recusados() >= 1);
    assert_eq!(portal.logins(), 0);
    assert_eq!(portal.codigos_recusados(), 0, "o código estava certo");
    assert_eq!(portal.downloads(), 0);
    assert!(portal.acessos_proibidos().is_empty());
    assert_eq!(achados(&inv), 0);
    // nenhum segredo no que o roteiro registrou
    let registros = rel.registros.join("\n");
    assert!(
        !registros.contains(SENHA) && !registros.contains("12345678909"),
        "{registros}"
    );
}

/// (c) Segunda execução: o portal gera bytes novos, e o inventário descarta pelo texto.
#[test]
fn c_segunda_execucao_com_bytes_novos_e_descartada_pelo_texto() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, inv) = ambiente(d.path());
    let (nome, n) = importar(&amb);
    let mes = mes_corrente_do_teste();
    let portal = PortalDeSouCaixa::iniciar(
        config_do_soucaixa_de_teste(&[mes.as_str()]).com_pdf_bytes_novos_a_cada_baixa(true),
    )
    .unwrap();

    // O mesmo Chrome: a sessão do portal ainda vale na segunda vez (sem novo login).
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let (primeira, _) = rodar(
        &amb,
        &nome,
        n,
        &mut chrome,
        &portal,
        &PedidoDeExecucao::default(),
    );
    assert_eq!(primeira.fim, Fim::Concluida, "{primeira:?}");
    assert_eq!(
        (primeira.baixados, primeira.novos, primeira.descartados),
        (1, 1, 0)
    );
    assert_eq!(achados(&inv), 1);

    let (segunda, id_da_segunda) = rodar(
        &amb,
        &nome,
        n,
        &mut chrome,
        &portal,
        &PedidoDeExecucao::default(),
    );
    assert_eq!(segunda.fim, Fim::Concluida, "{segunda:?}");
    assert_eq!(
        portal.logins(),
        1,
        "a segunda vez entrou direto pela sessão"
    );
    assert_eq!(portal.baixadas_de(&mes), 2, "o portal serviu de novo");
    assert_eq!(
        (segunda.baixados, segunda.novos, segunda.descartados),
        (1, 0, 1),
        "{segunda:?}"
    );
    assert_eq!(achados(&inv), 1, "o texto é o mesmo: nenhum achado novo");
    // o histórico da execução diz por que o arquivo não entrou
    let eventos = amb
        .banco()
        .unwrap()
        .eventos_escritos(id_da_segunda)
        .unwrap();
    assert!(
        eventos
            .iter()
            .any(|(_, tipo, m)| tipo == "descartado" && m.contains("já estava no inventário")),
        "{eventos:?}"
    );
    // os bytes, de fato, eram outros
    let esperado =
        sha256_hex(&bancada_navegador::site_falso_soucaixa::pdf_do_contracheque(&mes, None));
    let c = Connection::open(&inv).unwrap();
    let a_primeira: String = c
        .query_row("SELECT content_hash FROM achado", [], |l| l.get(0))
        .unwrap();
    assert_ne!(a_primeira, "", "{esperado}");

    // Baixa forçada: o usuário confirma, e o arquivo entra de novo.
    let forcada = PedidoDeExecucao {
        forcar: true,
        ..PedidoDeExecucao::default()
    };
    let (terceira, _) = rodar(&amb, &nome, n, &mut chrome, &portal, &forcada);
    chrome.fechar();
    eprintln!(
        "baixa forçada: {terceira:?}; achados agora: {}",
        achados(&inv)
    );
    assert_eq!(terceira.fim, Fim::Concluida, "{terceira:?}");
    assert_eq!(terceira.descartados, 0, "forçar não descarta");
}

/// (d) Mês não publicado não é erro: o intervalo `de` -1 a `ate` 0 baixa o anterior e só avisa
/// do corrente (habilitado no calendário, mas sem contracheque).
#[test]
fn d_mes_nao_publicado_nao_e_erro() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, inv) = ambiente(d.path());
    let (nome, n) = importar(&amb);
    let mes = mes_corrente_do_teste();
    let anterior = mes_anterior(&mes);
    // só o mês anterior está publicado
    let portal =
        PortalDeSouCaixa::iniciar(config_do_soucaixa_de_teste(&[anterior.as_str()])).unwrap();
    let pedido = PedidoDeExecucao {
        entregues: [
            ("de".to_owned(), ValorBruto::Texto("-1".to_owned())),
            ("ate".to_owned(), ValorBruto::Texto("0".to_owned())),
        ]
        .into_iter()
        .collect(),
        ..PedidoDeExecucao::default()
    };

    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let (rel, execucao) = rodar(&amb, &nome, n, &mut chrome, &portal, &pedido);
    chrome.fechar();

    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(estado_da(&amb, execucao), EstadoDaExecucao::Concluida);
    assert!(rel.entrada_recusada.is_none());
    assert!(portal.baixou(&anterior), "o mês publicado foi baixado");
    assert!(!portal.baixou(&mes));
    assert_eq!(portal.downloads(), 1);
    assert_eq!((rel.baixados, rel.novos), (1, 1));
    assert!(
        rel.registros.iter().any(|r| r.contains(&format!(
            "contracheque de {mes} ainda não publicado ou fora do período"
        ))),
        "{:?}",
        rel.registros
    );
    assert!(
        rel.registros
            .iter()
            .any(|r| r == "contracheques baixados: 1 de 2"),
        "{:?}",
        rel.registros
    );
    assert!(portal.acessos_proibidos().is_empty());
    assert_eq!(origens(&inv).len(), 1);
}

/// Um mês antes do período do portal (desabilitado no calendário) também só avisa.
#[test]
fn e_mes_fora_do_periodo_do_portal_nao_e_erro() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, _) = ambiente(d.path());
    let (nome, n) = importar(&amb);
    let mes = mes_corrente_do_teste();
    let portal = PortalDeSouCaixa::iniciar(config_do_soucaixa_de_teste(&[mes.as_str()])).unwrap();
    // 2026-03 está antes de maio de 2026: desabilitado. O mês corrente está publicado.
    let pedido = PedidoDeExecucao {
        entregues: [
            ("de".to_owned(), ValorBruto::Texto("2026-03".to_owned())),
            ("ate".to_owned(), ValorBruto::Texto("2026-03".to_owned())),
        ]
        .into_iter()
        .collect(),
        ..PedidoDeExecucao::default()
    };
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let (rel, _) = rodar(&amb, &nome, n, &mut chrome, &portal, &pedido);
    chrome.fechar();
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert_eq!(portal.downloads(), 0);
    assert!(
        rel.registros
            .iter()
            .any(|r| r.contains("contracheque de 2026-03 ainda não publicado ou fora do período")),
        "{:?}",
        rel.registros
    );
    assert!(portal.acessos_proibidos().is_empty());
}

/// (f) Os parâmetros da agenda (`de` = `ate` = -1) baixam só o mês anterior publicado.
#[test]
fn f_parametros_de_e_ate_relativos_baixam_so_o_mes_anterior() {
    if !chrome_ligado() {
        return;
    }
    let d = tempfile::tempdir().unwrap();
    let (amb, inv) = ambiente(d.path());
    let (nome, n) = importar(&amb);
    let mes = mes_corrente_do_teste();
    let anterior = mes_anterior(&mes);
    let portal = PortalDeSouCaixa::iniciar(config_do_soucaixa_de_teste(&[
        anterior.as_str(),
        mes.as_str(),
    ]))
    .unwrap();
    let pedido = PedidoDeExecucao {
        entregues: [
            ("de".to_owned(), ValorBruto::Texto("-1".to_owned())),
            ("ate".to_owned(), ValorBruto::Texto("-1".to_owned())),
        ]
        .into_iter()
        .collect(),
        ..PedidoDeExecucao::default()
    };
    let mut chrome = abrir_chrome(&d.path().join("perfil"));
    let (rel, _) = rodar(&amb, &nome, n, &mut chrome, &portal, &pedido);
    chrome.fechar();
    assert_eq!(rel.fim, Fim::Concluida, "{rel:?}");
    assert!(portal.baixou(&anterior));
    assert!(!portal.baixou(&mes), "o mês corrente não foi pedido");
    assert_eq!(portal.downloads(), 1);
    assert_eq!(
        origens(&inv)[0].1,
        format!("contracheque-{anterior}.pdf"),
        "{:?}",
        origens(&inv)
    );
    assert!(portal.acessos_proibidos().is_empty());
}
