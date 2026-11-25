//! O `testar` do roteiro escrito (spec 16, §5): roda a versão **ponta a ponta no site falso**
//! local (`127.0.0.1`), com um Chrome sem janela e um perfil descartável, e grava o resultado
//! com [`BancoDeRoteiros::marcar_testado`]. Só o teste verde leva o rascunho a `testado` (e só
//! um `testado` pode ser ativado, no terminal, pelo usuário).
//!
//! O que o teste **nunca** toca: o chaveiro real (as senhas são fictícias, em memória), o Gmail
//! (a caixa de e-mail é uma implementação falsa que lê o código do próprio portal), o perfil
//! dedicado do Chrome (o do teste nasce e morre numa pasta temporária), o cofre e o inventário
//! reais (o PDF fictício cai num cofre temporário) e qualquer rede além de `127.0.0.1`.
//!
//! Cenários desta fatia: só o [`Cenario::Contracheque`]. Ele sobe o
//! [`PortalDeContracheque`](bancada_navegador::site_falso::PortalDeContracheque) com CPF e senha
//! fictícios e MFA por e-mail; o host **lógico** do manifesto (`portal.exemplo.test`) é
//! mapeado para a base real do portal (modo teste do executor, [`OpcoesEscrito`]).
//!
//! O teste passa se, juntas: a execução concluiu; o login e o código foram aceitos; o
//! contracheque do mês corrente **ou** do anterior foi baixado; o portal não registrou nenhum
//! acesso proibido (pagar boleto, alterar cadastro); e o arquivo que entrou no inventário é o PDF
//! fictício esperado.

use crate::banco::{EstadoDaExecucao, EstadoEscrito, ResultadoDoTeste, VersaoEscrita};
use crate::chrome::{self, OpcoesDoChrome};
use crate::codigo_email::{CaixaDeEmail, ErroDeCodigo, MensagemCurta, RelogioReal};
use crate::erro::{ErroDeRoteiro, Resultado};
use crate::escrito::{OpcoesEscrito, data_de_hoje};
use crate::executor::{Decisao, Fim, FonteDeSenhas, Pausa};
use crate::manifesto::{Classe, Manifesto, Mfa};
use crate::navegador::Navegador;
use crate::servico::{Ambiente, RelatorioEscrito, Servicos, executar_escrito_em};
use bancada_navegador::site_falso::{Config, MfaDoPortal, PortalDeContracheque};
use bancada_navegador::url::host_de;
use regex::Regex;
use rusqlite::Connection;
use sbf_chaveiro::Segredo;
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};
use zeroize::Zeroizing;

/// O esquema do inventário (o mesmo que os testes do crate usam para um cofre de mentira).
const SQL_DO_INVENTARIO: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

/// O CPF fictício do portal falso.
const CPF_FICTICIO: &str = "123.456.789-09";
/// A senha fictícia do portal falso.
const SENHA_FICTICIA: &str = "SenhaFicticia#4821";
/// O teto de um teste inteiro: passou disto, o teste é cancelado e falha.
pub const TETO_DO_TESTE: Duration = Duration::from_secs(180);

/// O cenário do site falso contra o qual a versão é testada.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cenario {
    /// Portal de contracheque: login com CPF e senha, MFA por e-mail, lista de meses, download
    /// de PDF e os distratores "Pagar boleto" e "Alterar cadastro".
    Contracheque,
}

impl Cenario {
    /// Todos os cenários, na ordem em que o CLI os lista.
    pub const TODOS: &'static [Self] = &[Self::Contracheque];

    /// O nome do cenário na linha de comando (`--cenario contracheque`).
    #[must_use]
    pub fn nome(self) -> &'static str {
        match self {
            Self::Contracheque => "contracheque",
        }
    }

    /// Uma frase sobre o que o cenário exige do roteiro.
    #[must_use]
    pub fn descricao(self) -> &'static str {
        match self {
            Self::Contracheque => {
                "portal de contracheque fictício: CPF e senha, código por e-mail, download do PDF do mês"
            }
        }
    }

    /// O cenário de nome `texto`, se existe.
    #[must_use]
    pub fn de_texto(texto: &str) -> Option<Self> {
        Self::TODOS.iter().copied().find(|c| c.nome() == texto)
    }

    /// O manifesto serve a este cenário? Devolve o que o teste precisa dele.
    ///
    /// # Errors
    /// [`ErroDeRoteiro::Invalido`] com o motivo, em português claro e sem segredo.
    fn conferir(self, m: &Manifesto) -> Resultado<AlvoDoTeste> {
        let nao = |motivo: &str| {
            ErroDeRoteiro::Invalido(format!(
                "este roteiro não é do cenário {}: {motivo}",
                self.nome()
            ))
        };
        let Mfa::Email { remetente, formato } = &m.mfa else {
            return Err(nao("o manifesto tem de declarar MFA por e-mail"));
        };
        if m.movimenta_dinheiro {
            return Err(nao("o roteiro movimenta dinheiro"));
        }
        if m.classe != Classe::Consulta {
            return Err(nao("a classe tem de ser `consulta`"));
        }
        if m.rotulo_senha.is_none() {
            return Err(nao("o manifesto não declara `rotulo_senha`"));
        }
        let Some(host) = host_de(&m.url_inicial) else {
            return Err(nao("`url_inicial` não tem host"));
        };
        if !m.hosts.contains(&host) {
            return Err(nao("o host de `url_inicial` não está em `hosts`"));
        }
        // O portal falso manda um código de 6 dígitos: um formato que não o reconhece nunca
        // passaria, e o motivo certo é este, não "o código não chegou".
        let casa = Regex::new(&format!("^(?:{formato})$"))
            .map(|r| r.is_match("123456"))
            .unwrap_or(false);
        if !casa {
            return Err(nao(
                "o formato do código do manifesto não casa com o de 6 dígitos do portal falso",
            ));
        }
        Ok(AlvoDoTeste {
            host_logico: host,
            remetente: remetente.clone(),
            rotulo_senha: m.rotulo_senha.clone().unwrap_or_default(),
        })
    }
}

/// O que o teste tira do manifesto.
#[derive(Debug)]
struct AlvoDoTeste {
    /// O host que o roteiro abre (mapeado para o portal local).
    host_logico: String,
    /// De quem vem o e-mail com o código.
    remetente: String,
    /// O rótulo que a senha fictícia atende.
    rotulo_senha: String,
}

/// As senhas do teste: fictícias, em memória. Nunca o chaveiro real.
struct SenhasDoTeste {
    rotulo: String,
}

impl FonteDeSenhas for SenhasDoTeste {
    fn senha_de_site(&self, rotulo: &str) -> Result<Segredo, String> {
        if rotulo == self.rotulo {
            Ok(Segredo::novo(SENHA_FICTICIA))
        } else {
            Err("a senha do site não está no chaveiro do teste".to_owned())
        }
    }

    fn cpf(&self) -> Result<Segredo, String> {
        Ok(Segredo::novo(CPF_FICTICIO))
    }
}

/// A "caixa de e-mail" do teste: quando consultada, devolve uma mensagem do remetente do
/// manifesto com o código que o portal gerou, recebida **depois** do pedido (o instante em que
/// o portal gerou o código). Sem login no portal, a caixa está vazia.
struct CaixaDoPortal<'a> {
    portal: &'a PortalDeContracheque,
    remetente: String,
}

impl CaixaDeEmail for CaixaDoPortal<'_> {
    fn mensagens_recentes(
        &self,
        remetente: &str,
        desde: SystemTime,
    ) -> Result<Vec<MensagemCurta>, ErroDeCodigo> {
        if remetente != self.remetente {
            return Ok(Vec::new());
        }
        let (Some(codigo), Some(quando)) = (
            self.portal.codigo_gerado(),
            self.portal.instante_do_codigo(),
        ) else {
            return Ok(Vec::new());
        };
        if quando < desde {
            return Ok(Vec::new());
        }
        Ok(vec![MensagemCurta {
            recebida_em: quando,
            corpo: Zeroizing::new(format!(
                "Seu código de verificação\nO seu código de acesso é {codigo}. Não o compartilhe."
            )),
        }])
    }
}

/// Quem pausa não tem usuário do outro lado: no teste, a vez do usuário é uma falha.
struct SemUsuario;

impl Pausa for SemUsuario {
    fn aguardar(&self, _execucao: i64, _motivo: &str) -> Decisao {
        Decisao::Expirou
    }
}

/// A pasta temporária de um teste: o perfil do Chrome e o cofre de mentira. Some ao sair.
struct ZonaDeTeste {
    raiz: PathBuf,
}

impl ZonaDeTeste {
    /// Cria a pasta (permissão só do dono) dentro de `<pasta de dados>/roteiros/teste/`.
    fn criar(amb: &Ambiente) -> Resultado<Self> {
        let unico = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        let raiz = amb
            .pasta_dados
            .join("roteiros")
            .join("teste")
            .join(format!("{}-{unico}", std::process::id()));
        std::fs::create_dir_all(&raiz)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let _ = std::fs::set_permissions(&raiz, std::fs::Permissions::from_mode(0o700));
        }
        Ok(Self { raiz })
    }

    fn perfil(&self) -> PathBuf {
        self.raiz.join("perfil")
    }

    fn cofre(&self) -> PathBuf {
        self.raiz.join("cofre")
    }

    /// Cria o cofre de mentira com o esquema do inventário.
    fn criar_cofre(&self) -> Resultado<()> {
        std::fs::create_dir_all(self.cofre())?;
        let conn = Connection::open(self.cofre().join("inventario.sqlite"))?;
        conn.execute_batch(SQL_DO_INVENTARIO)?;
        Ok(())
    }
}

impl Drop for ZonaDeTeste {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.raiz);
    }
}

/// Testa a versão `numero` do roteiro `nome` no cenário, **abrindo o Chrome** (sem janela, com
/// perfil descartável) e um cofre temporário: nada do que é real é tocado. Grava o resultado
/// em `roteiros.sqlite` (`passou` leva o rascunho a `testado`; falha o mantém rascunho).
///
/// # Errors
/// Versão inexistente ou que não é rascunho, roteiro que não é do cenário, Chrome que não
/// abre, ou falha de infraestrutura (banco, disco). Um roteiro que **falha** o teste não é
/// erro: devolve `passou = false` com o motivo no resumo.
pub fn testar_escrito(
    amb: &Ambiente,
    nome: &str,
    numero: u32,
    cenario: Cenario,
) -> Resultado<ResultadoDoTeste> {
    // Tudo o que não precisa do Chrome vem antes: o erro certo aparece sem abrir nada.
    conferir_antes(amb, nome, numero, cenario)?;
    let zona = ZonaDeTeste::criar(amb)?;
    zona.criar_cofre()?;
    let mut chrome = chrome::abrir(&OpcoesDoChrome {
        programa: amb.programa_do_chrome.clone(),
        perfil: zona.perfil(),
        sem_janela: true,
    })
    .map_err(|e| match e {
        ErroDeRoteiro::Navegador(m) => ErroDeRoteiro::Navegador(format!(
            "o teste precisa do Chrome ou Chromium e ele não abriu ({m}); instale um \
             ou aponte SBF_CHROME para o executável"
        )),
        outro => outro,
    })?;
    // O cofre é o temporário; o banco dos roteiros é o do ambiente.
    let mut amb_teste = amb.clone();
    amb_teste.cofre = zona.cofre();
    amb_teste.inventario = zona.cofre().join("inventario.sqlite");
    amb_teste.sem_janela = true;
    let r = testar_em(&amb_teste, nome, numero, cenario, &mut chrome);
    chrome.fechar();
    r
}

/// A versão existe, é rascunho e o manifesto serve ao cenário.
fn conferir_antes(
    amb: &Ambiente,
    nome: &str,
    numero: u32,
    cenario: Cenario,
) -> Resultado<(VersaoEscrita, AlvoDoTeste)> {
    let versao = amb.banco()?.versao_escrita(nome, numero)?;
    if versao.estado != EstadoEscrito::Rascunho {
        return Err(ErroDeRoteiro::Invalido(format!(
            "só um rascunho pode ser testado; `{nome}` v{numero} está {}",
            versao.estado.texto()
        )));
    }
    let alvo = cenario.conferir(&versao.manifesto)?;
    Ok((versao, alvo))
}

/// O miolo de [`testar_escrito`]: roda a versão num `navegador` já aberto e usa o cofre e o
/// inventário **do `amb` dado** (quem chama garante que são de teste). Os testes trocam o
/// Chrome por um navegador falso.
///
/// # Errors
/// Os de [`testar_escrito`], menos o do Chrome.
pub fn testar_em(
    amb: &Ambiente,
    nome: &str,
    numero: u32,
    cenario: Cenario,
    navegador: &mut dyn Navegador,
) -> Resultado<ResultadoDoTeste> {
    let (versao, alvo) = conferir_antes(amb, nome, numero, cenario)?;
    let banco = amb.banco()?;

    let (_, mes_atual, mes_anterior) = meses_do_teste();
    let portal = PortalDeContracheque::iniciar(
        Config::nova(CPF_FICTICIO, SENHA_FICTICIA)
            .com_mfa(MfaDoPortal::Email)
            .com_meses(&[mes_anterior.as_str(), mes_atual.as_str()]),
    )?;
    let senhas = SenhasDoTeste {
        rotulo: alvo.rotulo_senha.clone(),
    };
    let caixa = CaixaDoPortal {
        portal: &portal,
        remetente: alvo.remetente.clone(),
    };
    let opcoes = OpcoesEscrito {
        mapa_de_hosts: vec![(alvo.host_logico.clone(), portal.url_base())],
        aceitar_http_loopback: true,
    };

    let execucao = banco.iniciar_execucao_escrita(nome, numero, "teste")?;
    let cancelar = AtomicBool::new(false);
    let pronto = AtomicBool::new(false);
    let rodou = std::thread::scope(|s| {
        // O vigia: passado o teto, cancela (o teste não pode travar a linha de comando).
        s.spawn(|| {
            let inicio = std::time::Instant::now();
            while !pronto.load(Ordering::SeqCst) {
                if inicio.elapsed() >= TETO_DO_TESTE {
                    cancelar.store(true, Ordering::SeqCst);
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
        let r = executar_escrito_em(
            amb,
            navegador,
            &versao,
            execucao,
            &Servicos {
                senhas: &senhas,
                caixa: Some(&caixa),
                relogio: &RelogioReal,
                pausa: Some(&SemUsuario),
                opcoes: &opcoes,
            },
            &cancelar,
        );
        pronto.store(true, Ordering::SeqCst);
        r
    });
    let relatorio = match rodou {
        Ok(r) => r,
        Err(e) => {
            // Falha de infraestrutura no meio: a execução não pode ficar "rodando".
            let _ = banco.finalizar_escrita(
                execucao,
                EstadoDaExecucao::Falhou,
                Some(&e.to_string()),
                0,
                0,
            );
            return Err(e);
        }
    };
    let resultado = avaliar(
        &relatorio,
        &portal,
        &[mes_atual.as_str(), mes_anterior.as_str()],
        &amb.inventario,
        &codigos_a_esconder(&portal),
    );
    banco.marcar_testado(nome, numero, &resultado)?;
    Ok(resultado)
}

/// O mês corrente e o anterior (`AAAA-MM`), no fuso do usuário (UTC−3), como o roteiro os vê.
fn meses_do_teste() -> (String, String, String) {
    let hoje = data_de_hoje(SystemTime::now());
    let ano: i64 = hoje.get(..4).and_then(|a| a.parse().ok()).unwrap_or(2026);
    let mes: i64 = hoje.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(1);
    let (ano_ant, mes_ant) = if mes == 1 {
        (ano - 1, 12)
    } else {
        (ano, mes - 1)
    };
    (
        hoje.clone(),
        format!("{ano:04}-{mes:02}"),
        format!("{ano_ant:04}-{mes_ant:02}"),
    )
}

/// Os valores que não podem aparecer no resumo (mesmo sendo fictícios, o hábito é o mesmo).
fn codigos_a_esconder(portal: &PortalDeContracheque) -> Vec<String> {
    let mut v = vec![
        SENHA_FICTICIA.to_owned(),
        CPF_FICTICIO.to_owned(),
        CPF_FICTICIO.chars().filter(char::is_ascii_digit).collect(),
    ];
    if let Some(c) = portal.codigo_gerado() {
        v.push(c);
    }
    v
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// O achado com este `content_hash` existe no inventário?
fn achado_existe(inventario: &Path, hash: &str) -> bool {
    let Ok(conn) = Connection::open(inventario) else {
        return false;
    };
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM achado WHERE content_hash = ?1)",
        [hash],
        |l| l.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

fn fim_em_texto(fim: &Fim) -> String {
    match fim {
        Fim::Concluida => "concluída".to_owned(),
        Fim::Falhou { motivo, .. } => format!("falhou ({motivo})"),
        Fim::Bloqueada { bloqueio, .. } => format!("barrada pela lista de bloqueio ({bloqueio})"),
        Fim::Cancelada => "cancelada (o teste passou do prazo ou foi interrompido)".to_owned(),
        Fim::PausaExpirou { .. } => {
            "pediu a vez do usuário (pausa), o que o site falso não deveria exigir".to_owned()
        }
    }
}

/// Confere os critérios do teste e monta o resumo (sem segredo).
fn avaliar(
    rel: &RelatorioEscrito,
    portal: &PortalDeContracheque,
    meses: &[&str],
    inventario: &Path,
    esconder: &[String],
) -> ResultadoDoTeste {
    let mut falhas: Vec<String> = Vec::new();
    let mut fatos: Vec<String> = Vec::new();

    if rel.fim == Fim::Concluida {
        fatos.push("a execução concluiu".to_owned());
    } else {
        falhas.push(format!("a execução terminou {}", fim_em_texto(&rel.fim)));
    }

    if portal.logins() >= 1 && portal.logins_recusados() == 0 {
        fatos.push("o login foi aceito".to_owned());
    } else if portal.logins() == 0 {
        falhas.push("o portal não registrou nenhum login aceito".to_owned());
    } else {
        falhas.push(format!(
            "o portal recusou {} login(s)",
            portal.logins_recusados()
        ));
    }

    if portal.logins() >= 1 && portal.codigos_recusados() == 0 && portal.downloads() >= 1 {
        fatos.push("o código foi aceito".to_owned());
    } else if portal.codigos_recusados() > 0 {
        falhas.push(format!(
            "o portal recusou {} código(s)",
            portal.codigos_recusados()
        ));
    }

    let baixado = meses.iter().find(|m| portal.baixou(m));
    match baixado {
        Some(mes) => {
            fatos.push(format!("contracheque de {mes} baixado"));
            let esperado = sha256_hex(&bancada_navegador::site_falso::pdf_do_contracheque(mes));
            if rel.baixados >= 1 && achado_existe(inventario, &esperado) {
                fatos.push("o arquivo no inventário é o PDF fictício esperado".to_owned());
            } else {
                falhas.push(
                    "o arquivo baixado não está no inventário ou não é o PDF fictício esperado"
                        .to_owned(),
                );
            }
        }
        None => falhas
            .push("nenhum contracheque do mês corrente nem do anterior foi baixado".to_owned()),
    }

    let proibidos = portal.acessos_proibidos();
    if proibidos.is_empty() {
        fatos.push("nenhum acesso proibido".to_owned());
    } else {
        falhas.push(format!(
            "o portal registrou acesso proibido: {}",
            proibidos.join(", ")
        ));
    }

    let passou = falhas.is_empty();
    let mut resumo = if passou {
        format!("passou no cenário contracheque: {}", fatos.join("; "))
    } else {
        format!("falhou no cenário contracheque: {}", falhas.join("; "))
    };
    if !passou && let Some(ultimo) = rel.registros.last() {
        resumo.push_str(&format!(" (último registro do roteiro: {ultimo})"));
    }
    for valor in esconder.iter().filter(|v| !v.is_empty()) {
        resumo = resumo.replace(valor.as_str(), "***");
    }
    ResultadoDoTeste { passou, resumo }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn manifesto(mfa: &str, classe: &str, movimenta: bool) -> Manifesto {
        Manifesto::de_json(&format!(
            r#"{{"nome":"contracheque-exemplo","url_inicial":"https://portal.exemplo.test/entrar",
            "hosts":["portal.exemplo.test"],"rotulo_senha":"senha:portal-exemplo",
            "rotulo_cpf":"cpf:portal-exemplo","mfa":{mfa},"movimenta_dinheiro":{movimenta},
            "classe":"{classe}"}}"#
        ))
        .unwrap()
    }

    const EMAIL: &str =
        r#"{"tipo":"email","remetente":"nao-responda@portal.exemplo.test","formato":"\\d{6}"}"#;

    #[test]
    fn cenarios_tem_nome_e_voltam_do_texto() {
        assert_eq!(
            Cenario::de_texto("contracheque"),
            Some(Cenario::Contracheque)
        );
        assert_eq!(Cenario::de_texto("outro"), None);
        assert_eq!(Cenario::Contracheque.nome(), "contracheque");
        assert!(!Cenario::Contracheque.descricao().is_empty());
    }

    #[test]
    fn manifesto_do_cenario_e_aceito() {
        let alvo = Cenario::Contracheque
            .conferir(&manifesto(EMAIL, "consulta", false))
            .unwrap();
        assert_eq!(alvo.host_logico, "portal.exemplo.test");
        assert_eq!(alvo.remetente, "nao-responda@portal.exemplo.test");
        assert_eq!(alvo.rotulo_senha, "senha:portal-exemplo");
    }

    #[test]
    fn manifesto_de_outro_cenario_e_recusado_com_motivo_claro() {
        for (mfa, classe) in [
            (r#"{"tipo":"nenhum"}"#, "consulta"),
            (r#"{"tipo":"manual"}"#, "consulta"),
            (EMAIL, "atualizar_conexao"),
        ] {
            let e = Cenario::Contracheque
                .conferir(&manifesto(mfa, classe, false))
                .unwrap_err()
                .to_string();
            assert!(
                e.contains("este roteiro não é do cenário contracheque"),
                "{e}"
            );
        }
    }

    #[test]
    fn formato_de_codigo_que_o_portal_nao_gera_e_recusado() {
        let oito =
            r#"{"tipo":"email","remetente":"nao-responda@portal.exemplo.test","formato":"\\d{8}"}"#;
        let e = Cenario::Contracheque
            .conferir(&manifesto(oito, "consulta", false))
            .unwrap_err()
            .to_string();
        assert!(e.contains("formato do código"), "{e}");
    }

    #[test]
    fn a_caixa_so_responde_ao_remetente_e_so_depois_do_login() {
        let portal =
            PortalDeContracheque::iniciar(Config::nova(CPF_FICTICIO, SENHA_FICTICIA)).unwrap();
        let caixa = CaixaDoPortal {
            portal: &portal,
            remetente: "nao-responda@portal.exemplo.test".to_owned(),
        };
        let antes = SystemTime::now() - Duration::from_secs(60);
        // sem login no portal, a caixa está vazia
        assert!(
            caixa
                .mensagens_recentes("nao-responda@portal.exemplo.test", antes)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn meses_do_teste_sao_corrente_e_anterior() {
        let (hoje, atual, anterior) = meses_do_teste();
        assert_eq!(&hoje[..7], atual);
        assert!(anterior < atual);
        assert_eq!(anterior.len(), 7);
    }

    #[test]
    fn o_resumo_nunca_mostra_o_valor_ficticio_do_segredo() {
        let portal =
            PortalDeContracheque::iniciar(Config::nova(CPF_FICTICIO, SENHA_FICTICIA)).unwrap();
        let rel = RelatorioEscrito {
            fim: Fim::Falhou {
                passo: 0,
                motivo: format!("erro com {SENHA_FICTICIA}"),
            },
            baixados: 0,
            novos: 0,
            registros: vec![format!("viu {CPF_FICTICIO}")],
        };
        let r = avaliar(
            &rel,
            &portal,
            &["2026-11"],
            Path::new("/nao/existe"),
            &codigos_a_esconder(&portal),
        );
        assert!(!r.passou);
        assert!(!r.resumo.contains(SENHA_FICTICIA), "{}", r.resumo);
        assert!(!r.resumo.contains(CPF_FICTICIO), "{}", r.resumo);
    }
}
