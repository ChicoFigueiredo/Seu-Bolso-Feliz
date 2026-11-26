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
//! Cenários: [`Cenario::Contracheque`], [`Cenario::Conexoes`] e [`Cenario::SouCaixa`].
//!
//! **Contracheque** sobe o
//! [`PortalDeContracheque`](bancada_navegador::site_falso::PortalDeContracheque) com CPF e senha
//! fictícios e MFA por e-mail; o host **lógico** do manifesto (`portal.exemplo.test`) é
//! mapeado para a base real do portal (modo teste do executor, [`OpcoesEscrito`]).
//! O teste passa se, juntas: a execução concluiu; o login e o código foram aceitos; o
//! contracheque do mês corrente **ou** do anterior foi baixado; o portal não registrou nenhum
//! acesso proibido (pagar boleto, alterar cadastro); e o arquivo que entrou no inventário é o PDF
//! fictício esperado.
//!
//! **Conexões** sobe o [`PortalDeConexoes`] (painel fictício do Meu Pluggy, sem login) e mapeia
//! o host do manifesto para ele. O teste passa se, juntas: a execução concluiu; cada conexão foi
//! atualizada exatamente uma vez; o painel não registrou nenhum acesso proibido (excluir); e
//! nada que pareça segredo ou URL com token chegou ao resumo. Se o roteiro pede a vez do
//! usuário (sessão expirada), o teste **falha** com o motivo da pausa: no teste não há usuário.
//!
//! **SouCaixa** sobe o [`PortalDeSouCaixa`] (CPF, validação por e-mail, código, senha, calendário
//! de meses e download do PDF) e mapeia os DOIS hosts do manifesto (`soucaixa.caixa.gov.br` e
//! `login.caixa.gov.br`) para a mesma origem local. O teste passa se, juntas: a execução
//! concluiu; o login e o código foram aceitos; o contracheque do mês corrente foi baixado e o
//! PDF entrou no inventário com origem `roteiro:`; o portal não registrou nenhum acesso
//! proibido; e nem a senha, nem o CPF, nem o código aparecem no resumo nem nos registros.

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
use bancada_navegador::site_falso_conexoes::{Config as ConfigDeConexoes, PortalDeConexoes};
use bancada_navegador::site_falso_soucaixa::{
    self, Config as ConfigDoSouCaixa, HOST_DO_LOGIN, HOST_DO_SITE, PortalDeSouCaixa,
    REMETENTE_DO_CODIGO,
};
use bancada_navegador::url::host_de;
use regex::Regex;
use rusqlite::Connection;
use sbf_chaveiro::Segredo;
use sha2::{Digest as _, Sha256};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, PoisonError};
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
    /// Painel de conexões do Meu Pluggy: abre cada conexão, clica em "Atualizar" e espera o
    /// widget (iframe) terminar; o vizinho "Excluir" nunca pode ser tocado.
    Conexoes,
    /// Portal do SouCaixa: CPF, escolha do e-mail, código, senha, calendário de meses e download
    /// do PDF do mês; os distratores (pedidos, auxílios, alterar dados) nunca podem ser tocados.
    SouCaixa,
}

impl Cenario {
    /// Todos os cenários, na ordem em que o CLI os lista.
    pub const TODOS: &'static [Self] = &[Self::Contracheque, Self::Conexoes, Self::SouCaixa];

    /// O nome do cenário na linha de comando (`--cenario contracheque`).
    #[must_use]
    pub fn nome(self) -> &'static str {
        match self {
            Self::Contracheque => "contracheque",
            Self::Conexoes => "conexoes",
            Self::SouCaixa => "soucaixa",
        }
    }

    /// Uma frase sobre o que o cenário exige do roteiro.
    #[must_use]
    pub fn descricao(self) -> &'static str {
        match self {
            Self::Contracheque => {
                "portal de contracheque fictício: CPF e senha, código por e-mail, download do PDF do mês"
            }
            Self::Conexoes => {
                "painel de conexões fictício: abre cada conexão, clica em Atualizar e espera o widget terminar"
            }
            Self::SouCaixa => {
                "SouCaixa fictício: CPF, código por e-mail e senha, mês no calendário, download do PDF"
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
        match self {
            Self::Contracheque => self.conferir_contracheque(m),
            Self::Conexoes => self.conferir_conexoes(m),
            Self::SouCaixa => self.conferir_soucaixa(m),
        }
    }

    /// O erro de "este roteiro não é deste cenário", com o motivo.
    fn nao(self, motivo: &str) -> ErroDeRoteiro {
        ErroDeRoteiro::Invalido(format!(
            "este roteiro não é do cenário {}: {motivo}",
            self.nome()
        ))
    }

    fn conferir_contracheque(self, m: &Manifesto) -> Resultado<AlvoDoTeste> {
        let nao = |motivo: &str| self.nao(motivo);
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
            hosts_logicos: vec![host.clone()],
            host_logico: host,
            remetente: remetente.clone(),
            rotulo_senha: m.rotulo_senha.clone().unwrap_or_default(),
        })
    }

    fn conferir_soucaixa(self, m: &Manifesto) -> Resultado<AlvoDoTeste> {
        let nao = |motivo: &str| self.nao(motivo);
        let Mfa::Email { remetente, formato } = &m.mfa else {
            return Err(nao("o manifesto tem de declarar MFA por e-mail"));
        };
        if remetente != REMETENTE_DO_CODIGO {
            return Err(nao(&format!(
                "o remetente do código tem de ser {REMETENTE_DO_CODIGO}"
            )));
        }
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
        for esperado in [HOST_DO_SITE, HOST_DO_LOGIN] {
            if !m.hosts.iter().any(|h| h == esperado) {
                return Err(nao(&format!("`hosts` não traz {esperado}")));
            }
        }
        if !m.hosts.contains(&host) {
            return Err(nao("o host de `url_inicial` não está em `hosts`"));
        }
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
            hosts_logicos: vec![HOST_DO_SITE.to_owned(), HOST_DO_LOGIN.to_owned()],
            remetente: remetente.clone(),
            rotulo_senha: m.rotulo_senha.clone().unwrap_or_default(),
        })
    }

    fn conferir_conexoes(self, m: &Manifesto) -> Resultado<AlvoDoTeste> {
        let nao = |motivo: &str| self.nao(motivo);
        if m.classe != Classe::AtualizarConexao {
            return Err(nao("a classe tem de ser `atualizar_conexao`"));
        }
        if m.movimenta_dinheiro {
            return Err(nao("o roteiro movimenta dinheiro"));
        }
        if matches!(m.mfa, Mfa::Email { .. }) {
            return Err(nao(
                "o painel de conexões não tem código por e-mail: o MFA tem de ser `manual` ou `nenhum`",
            ));
        }
        if m.rotulo_senha.is_some() {
            return Err(nao(
                "o painel de conexões não tem senha: o manifesto não pode declarar `rotulo_senha`",
            ));
        }
        let Some(host) = host_de(&m.url_inicial) else {
            return Err(nao("`url_inicial` não tem host"));
        };
        if !m.hosts.contains(&host) {
            return Err(nao("o host de `url_inicial` não está em `hosts`"));
        }
        Ok(AlvoDoTeste {
            hosts_logicos: vec![host.clone()],
            host_logico: host,
            remetente: String::new(),
            rotulo_senha: String::new(),
        })
    }
}

/// O que o teste tira do manifesto.
#[derive(Debug)]
struct AlvoDoTeste {
    /// O host que o roteiro abre (mapeado para o portal local).
    host_logico: String,
    /// Todos os hosts lógicos que o portal local imita (o SouCaixa tem dois: site e login).
    hosts_logicos: Vec<String>,
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

/// A "caixa de e-mail" do cenário `soucaixa`: o e-mail do portal (remetente fixo, "Código de
/// ativação: NNNNNN") recebido quando o portal gerou o código.
struct CaixaDoSouCaixa<'a> {
    portal: &'a PortalDeSouCaixa,
    remetente: String,
}

impl CaixaDeEmail for CaixaDoSouCaixa<'_> {
    fn mensagens_recentes(
        &self,
        remetente: &str,
        desde: SystemTime,
    ) -> Result<Vec<MensagemCurta>, ErroDeCodigo> {
        if remetente != self.remetente {
            return Ok(Vec::new());
        }
        let Some((corpo, quando)) = self.portal.email_do_codigo() else {
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

/// Quem pausa não tem usuário do outro lado: no teste, a vez do usuário é uma falha.
struct SemUsuario;

impl Pausa for SemUsuario {
    fn aguardar(&self, _execucao: i64, _motivo: &str) -> Decisao {
        Decisao::Expirou
    }
}

/// Como [`SemUsuario`], mas guarda o motivo de cada pausa: o teste do cenário `conexoes` o cita
/// no resumo (sessão expirada).
#[derive(Default)]
struct PausaAnotada {
    motivos: Mutex<Vec<String>>,
}

impl PausaAnotada {
    fn motivos(&self) -> Vec<String> {
        self.motivos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Pausa for PausaAnotada {
    fn aguardar(&self, _execucao: i64, motivo: &str) -> Decisao {
        self.motivos
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(motivo.to_owned());
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
    match cenario {
        Cenario::Contracheque => testar_contracheque(amb, &versao, &alvo, navegador),
        Cenario::Conexoes => {
            let portal = PortalDeConexoes::iniciar(
                ConfigDeConexoes::default().com_duracao_widget(DURACAO_DO_WIDGET_NO_TESTE),
            )?;
            testar_conexoes(amb, &versao, &alvo, &portal, navegador)
        }
        Cenario::SouCaixa => {
            let (_, mes_atual, mes_anterior) = meses_do_teste();
            let portal = PortalDeSouCaixa::iniciar(
                ConfigDoSouCaixa::nova(CPF_FICTICIO, SENHA_FICTICIA, &mes_atual)
                    .com_meses_publicados(&[mes_anterior.as_str(), mes_atual.as_str()]),
            )?;
            testar_soucaixa(amb, &versao, &alvo, &portal, navegador)
        }
    }
}

/// O cenário `soucaixa` contra um `portal` que o chamador já subiu (para os testes olharem os
/// contadores do portal depois). Mesmo caminho de [`testar_em`]: grava o resultado com
/// `marcar_testado`. O portal tem de aceitar o CPF e a senha fictícios do teste
/// ([`CPF_FICTICIO`], [`SENHA_FICTICIA`]) e ter o mês corrente do teste como mês corrente.
///
/// # Errors
/// Os de [`testar_em`].
pub fn testar_soucaixa_em(
    amb: &Ambiente,
    nome: &str,
    numero: u32,
    portal: &PortalDeSouCaixa,
    navegador: &mut dyn Navegador,
) -> Resultado<ResultadoDoTeste> {
    let (versao, alvo) = conferir_antes(amb, nome, numero, Cenario::SouCaixa)?;
    testar_soucaixa(amb, &versao, &alvo, portal, navegador)
}

/// O CPF, a senha e o mês corrente que um portal do SouCaixa de teste tem de aceitar para o
/// cenário `soucaixa`: o chamador de [`testar_soucaixa_em`] monta a [`ConfigDoSouCaixa`] com isto.
#[must_use]
pub fn config_do_soucaixa_de_teste(meses_publicados: &[&str]) -> ConfigDoSouCaixa {
    let (_, mes_atual, _) = meses_do_teste();
    ConfigDoSouCaixa::nova(CPF_FICTICIO, SENHA_FICTICIA, &mes_atual)
        .com_meses_publicados(meses_publicados)
}

/// O mês corrente do teste (`AAAA-MM`, UTC−3), o que o roteiro vê como "hoje".
#[must_use]
pub fn mes_corrente_do_teste() -> String {
    meses_do_teste().1
}

/// O cenário `conexoes` contra um `portal` que o chamador já subiu (para os testes olharem os
/// contadores do painel depois, ou o subirem com sessão expirada). Mesmo caminho de
/// [`testar_em`]: grava o resultado com `marcar_testado`.
///
/// # Errors
/// Os de [`testar_em`].
pub fn testar_conexoes_em(
    amb: &Ambiente,
    nome: &str,
    numero: u32,
    portal: &PortalDeConexoes,
    navegador: &mut dyn Navegador,
) -> Resultado<ResultadoDoTeste> {
    let (versao, alvo) = conferir_antes(amb, nome, numero, Cenario::Conexoes)?;
    testar_conexoes(amb, &versao, &alvo, portal, navegador)
}

/// Roda a versão com o vigia de teto e fecha a execução com falha se a infraestrutura quebra.
fn rodar_com_vigia(
    amb: &Ambiente,
    versao: &VersaoEscrita,
    navegador: &mut dyn Navegador,
    servicos: &Servicos<'_>,
) -> Resultado<RelatorioEscrito> {
    let banco = amb.banco()?;
    let execucao = banco.iniciar_execucao_escrita(&versao.roteiro, versao.numero, "teste")?;
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
        let r = executar_escrito_em(amb, navegador, versao, execucao, servicos, &cancelar);
        pronto.store(true, Ordering::SeqCst);
        r
    });
    match rodou {
        Ok(r) => Ok(r),
        Err(e) => {
            // Falha de infraestrutura no meio: a execução não pode ficar "rodando".
            let _ = banco.finalizar_escrita(
                execucao,
                EstadoDaExecucao::Falhou,
                Some(&e.to_string()),
                0,
                0,
            );
            Err(e)
        }
    }
}

fn testar_contracheque(
    amb: &Ambiente,
    versao: &VersaoEscrita,
    alvo: &AlvoDoTeste,
    navegador: &mut dyn Navegador,
) -> Resultado<ResultadoDoTeste> {
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
    let relatorio = rodar_com_vigia(
        amb,
        versao,
        navegador,
        &Servicos {
            senhas: &senhas,
            caixa: Some(&caixa),
            relogio: &RelogioReal,
            pausa: Some(&SemUsuario),
            opcoes: &opcoes,
        },
    )?;
    let resultado = avaliar(
        &relatorio,
        &portal,
        &[mes_atual.as_str(), mes_anterior.as_str()],
        &amb.inventario,
        &codigos_a_esconder(&portal),
    );
    banco.marcar_testado(&versao.roteiro, versao.numero, &resultado)?;
    Ok(resultado)
}

fn testar_soucaixa(
    amb: &Ambiente,
    versao: &VersaoEscrita,
    alvo: &AlvoDoTeste,
    portal: &PortalDeSouCaixa,
    navegador: &mut dyn Navegador,
) -> Resultado<ResultadoDoTeste> {
    let banco = amb.banco()?;
    let (_, mes_atual, _) = meses_do_teste();
    let senhas = SenhasDoTeste {
        rotulo: alvo.rotulo_senha.clone(),
    };
    let caixa = CaixaDoSouCaixa {
        portal,
        remetente: alvo.remetente.clone(),
    };
    // Os dois hosts do manifesto (site e login) são a mesma origem local.
    let opcoes = OpcoesEscrito {
        mapa_de_hosts: alvo
            .hosts_logicos
            .iter()
            .map(|h| (h.clone(), portal.url_base()))
            .collect(),
        aceitar_http_loopback: true,
    };
    let relatorio = rodar_com_vigia(
        amb,
        versao,
        navegador,
        &Servicos {
            senhas: &senhas,
            caixa: Some(&caixa),
            relogio: &RelogioReal,
            pausa: Some(&SemUsuario),
            opcoes: &opcoes,
        },
    )?;
    let mut esconder = vec![
        SENHA_FICTICIA.to_owned(),
        CPF_FICTICIO.to_owned(),
        CPF_FICTICIO.chars().filter(char::is_ascii_digit).collect(),
    ];
    esconder.extend(portal.codigo_gerado());
    let resultado = avaliar_soucaixa(&relatorio, portal, &mes_atual, &amb.inventario, &esconder);
    banco.marcar_testado(&versao.roteiro, versao.numero, &resultado)?;
    Ok(resultado)
}

/// A duração do widget no teste: curta, mas o bastante para o roteiro vê-lo abrir.
const DURACAO_DO_WIDGET_NO_TESTE: Duration = Duration::from_secs(2);

fn testar_conexoes(
    amb: &Ambiente,
    versao: &VersaoEscrita,
    alvo: &AlvoDoTeste,
    portal: &PortalDeConexoes,
    navegador: &mut dyn Navegador,
) -> Resultado<ResultadoDoTeste> {
    let banco = amb.banco()?;
    // O painel não tem senha nem CPF: o chaveiro do teste não atende rótulo nenhum.
    let senhas = SenhasDoTeste {
        rotulo: String::new(),
    };
    let pausa = PausaAnotada::default();
    let opcoes = OpcoesEscrito {
        mapa_de_hosts: vec![(alvo.host_logico.clone(), portal.url_base())],
        aceitar_http_loopback: true,
    };
    let relatorio = rodar_com_vigia(
        amb,
        versao,
        navegador,
        &Servicos {
            senhas: &senhas,
            caixa: None,
            relogio: &RelogioReal,
            pausa: Some(&pausa),
            opcoes: &opcoes,
        },
    )?;
    let resultado = avaliar_conexoes(&relatorio, portal, &pausa.motivos(), &[]);
    banco.marcar_testado(&versao.roteiro, versao.numero, &resultado)?;
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

/// Existe alguma origem de roteiro (`roteiro:...`) no inventário?
fn origem_de_roteiro_existe(inventario: &Path) -> bool {
    let Ok(conn) = Connection::open(inventario) else {
        return false;
    };
    conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM origem WHERE origin_type = 'roteiro' AND origin_key LIKE 'roteiro:%')",
        [],
        |l| l.get::<_, bool>(0),
    )
    .unwrap_or(false)
}

/// Confere os critérios do cenário `soucaixa` e monta o resumo (sem segredo).
fn avaliar_soucaixa(
    rel: &RelatorioEscrito,
    portal: &PortalDeSouCaixa,
    mes: &str,
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
    if let Some(motivo) = &rel.entrada_recusada {
        falhas.push(format!("o portal recusou a entrada ({motivo})"));
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
    if portal.codigos_recusados() == 0 && portal.logins() >= 1 {
        fatos.push("o código foi aceito".to_owned());
    } else if portal.codigos_recusados() > 0 {
        falhas.push(format!(
            "o portal recusou {} código(s)",
            portal.codigos_recusados()
        ));
    }

    if portal.baixou(mes) {
        fatos.push(format!("contracheque de {mes} baixado"));
        let esperado = sha256_hex(&site_falso_soucaixa::pdf_do_contracheque(mes, None));
        if rel.baixados >= 1 && achado_existe(inventario, &esperado) {
            fatos.push("o arquivo no inventário é o PDF fictício esperado".to_owned());
        } else {
            falhas.push(
                "o arquivo baixado não está no inventário ou não é o PDF fictício esperado"
                    .to_owned(),
            );
        }
        if !origem_de_roteiro_existe(inventario) {
            falhas.push("o inventário não registrou a origem `roteiro:` do arquivo".to_owned());
        }
    } else {
        falhas.push(format!(
            "o contracheque do mês corrente ({mes}) não foi baixado"
        ));
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

    // Nenhum segredo (nem o código do e-mail) pode ter chegado ao histórico do roteiro.
    let historico = rel.registros.join("\n");
    if esconder
        .iter()
        .filter(|v| !v.is_empty())
        .any(|v| historico.contains(v.as_str()))
    {
        falhas.push("um segredo apareceu nos registros do roteiro".to_owned());
    } else {
        fatos.push("nenhum segredo nos registros".to_owned());
    }

    let passou = falhas.is_empty();
    let mut resumo = if passou {
        format!("passou no cenário soucaixa: {}", fatos.join("; "))
    } else {
        format!("falhou no cenário soucaixa: {}", falhas.join("; "))
    };
    if !passou && let Some(ultimo) = rel.registros.last() {
        resumo.push_str(&format!(" (último registro do roteiro: {ultimo})"));
    }
    let mut resumo = sem_consulta_de_url(&resumo);
    for valor in esconder.iter().filter(|v| !v.is_empty()) {
        resumo = resumo.replace(valor.as_str(), "***");
    }
    ResultadoDoTeste { passou, resumo }
}

/// Tira de um texto o que vem depois do `?` ou do `#` de qualquer URL (onde ficam os tokens).
fn sem_consulta_de_url(texto: &str) -> String {
    match Regex::new(r"(https?://[^\s?#]+)[?#]\S*") {
        Ok(r) => r.replace_all(texto, "$1").into_owned(),
        Err(_) => texto.to_owned(),
    }
}

/// Confere os critérios do cenário `conexoes` e monta o resumo (sem segredo nem token de URL).
///
/// `pausas` são os motivos das pausas que o roteiro pediu (no teste, ninguém responde).
fn avaliar_conexoes(
    rel: &RelatorioEscrito,
    portal: &PortalDeConexoes,
    pausas: &[String],
    esconder: &[String],
) -> ResultadoDoTeste {
    let mut falhas: Vec<String> = Vec::new();
    let mut fatos: Vec<String> = Vec::new();

    match &rel.fim {
        Fim::Concluida => fatos.push("a execução concluiu".to_owned()),
        Fim::PausaExpirou { .. } => {
            let motivo = pausas
                .last()
                .map_or_else(|| "sem motivo".to_owned(), Clone::clone);
            falhas.push(format!(
                "o roteiro pediu a vez do usuário (pausa: {motivo}) e no teste não há usuário"
            ));
        }
        outro => falhas.push(format!("a execução terminou {}", fim_em_texto(outro))),
    }

    let ids = portal.ids();
    let certas = ids
        .iter()
        .filter(|id| portal.atualizacoes_de(id) == 1)
        .count();
    if certas == ids.len() && portal.total_de_atualizacoes() == ids.len() {
        fatos.push(format!(
            "as {} conexões foram atualizadas uma vez cada",
            ids.len()
        ));
    } else {
        for id in &ids {
            let n = portal.atualizacoes_de(id);
            if n != 1 {
                let nome = portal.nome_de(id).unwrap_or("conexão");
                falhas.push(format!(
                    "{nome} foi atualizada {n} vez(es) (esperado: exatamente 1)"
                ));
            }
        }
    }

    let proibidos = portal.acessos_proibidos();
    if portal.exclusoes() == 0 && proibidos.is_empty() {
        fatos.push("nenhum acesso proibido (Excluir não foi tocado)".to_owned());
    } else {
        falhas.push(format!(
            "o painel registrou acesso proibido: {}",
            proibidos.join(", ")
        ));
    }

    let passou = falhas.is_empty();
    let mut resumo = if passou {
        format!("passou no cenário conexoes: {}", fatos.join("; "))
    } else {
        format!("falhou no cenário conexoes: {}", falhas.join("; "))
    };
    if !passou && let Some(ultimo) = rel.registros.last() {
        resumo.push_str(&format!(" (último registro do roteiro: {ultimo})"));
    }
    let mut resumo = sem_consulta_de_url(&resumo);
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
            descartados: 0,
            registros: vec![format!("viu {CPF_FICTICIO}")],
            entrada_recusada: None,
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

    // ------------------------------------------------------------------ cenário conexoes

    fn manifesto_conexoes(mfa: &str, classe: &str, movimenta: bool, rotulo: &str) -> Manifesto {
        Manifesto::de_json(&format!(
            r#"{{"nome":"meu-pluggy-atualizar","url_inicial":"https://meu.pluggy.ai/connections",
            "hosts":["meu.pluggy.ai"],"rotulo_senha":{rotulo},"rotulo_cpf":null,"mfa":{mfa},
            "movimenta_dinheiro":{movimenta},"classe":"{classe}"}}"#
        ))
        .unwrap()
    }

    #[test]
    fn o_cenario_conexoes_tem_nome_e_descricao_e_volta_do_texto() {
        assert_eq!(Cenario::de_texto("conexoes"), Some(Cenario::Conexoes));
        assert_eq!(Cenario::Conexoes.nome(), "conexoes");
        assert!(Cenario::Conexoes.descricao().contains("widget"));
        assert!(Cenario::TODOS.contains(&Cenario::Conexoes));
        assert!(Cenario::TODOS.contains(&Cenario::Contracheque));
    }

    #[test]
    fn manifesto_de_conexoes_com_mfa_manual_ou_nenhum_e_aceito() {
        for mfa in [r#"{"tipo":"manual"}"#, r#"{"tipo":"nenhum"}"#] {
            let alvo = Cenario::Conexoes
                .conferir(&manifesto_conexoes(mfa, "atualizar_conexao", false, "null"))
                .unwrap();
            assert_eq!(alvo.host_logico, "meu.pluggy.ai");
        }
    }

    #[test]
    fn manifesto_que_nao_e_de_conexoes_e_recusado_com_motivo_claro() {
        let manual = r#"{"tipo":"manual"}"#;
        let casos = [
            (
                manifesto_conexoes(manual, "consulta", false, "null"),
                "atualizar_conexao",
            ),
            (
                manifesto_conexoes(manual, "atualizar_conexao", true, "null"),
                "movimenta dinheiro",
            ),
            (
                manifesto_conexoes(EMAIL, "atualizar_conexao", false, "null"),
                "`manual` ou `nenhum`",
            ),
            (
                manifesto_conexoes(manual, "atualizar_conexao", false, "\"senha:x\""),
                "rotulo_senha",
            ),
        ];
        for (m, trecho) in casos {
            let e = Cenario::Conexoes.conferir(&m).unwrap_err().to_string();
            assert!(e.contains("este roteiro não é do cenário conexoes"), "{e}");
            assert!(e.contains(trecho), "{e}");
        }
        // o manifesto do contracheque também não serve ao cenário conexoes
        let e = Cenario::Conexoes
            .conferir(&manifesto(EMAIL, "consulta", false))
            .unwrap_err()
            .to_string();
        assert!(e.contains("conexoes"), "{e}");
    }

    /// Faz `POST` num caminho do painel (o que o JS da página faria).
    fn post(portal: &PortalDeConexoes, caminho: &str) {
        use std::io::{Read as _, Write as _};
        let mut c = std::net::TcpStream::connect(("127.0.0.1", portal.porta())).unwrap();
        write!(
            c,
            "POST {caminho} HTTP/1.1\r\nHost: meu.pluggy.ai\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
        )
        .unwrap();
        let mut _resp = Vec::new();
        let _ = c.read_to_end(&mut _resp);
    }

    fn relatorio(fim: Fim, registros: &[&str]) -> RelatorioEscrito {
        RelatorioEscrito {
            fim,
            baixados: 0,
            novos: 0,
            descartados: 0,
            registros: registros.iter().map(|r| (*r).to_owned()).collect(),
            entrada_recusada: None,
        }
    }

    #[test]
    fn conexoes_passa_quando_cada_conexao_e_atualizada_uma_vez() {
        let p = PortalDeConexoes::iniciar(ConfigDeConexoes::default()).unwrap();
        for id in p.ids() {
            post(&p, &format!("/api/atualizou/{id}"));
        }
        let r = avaliar_conexoes(&relatorio(Fim::Concluida, &[]), &p, &[], &[]);
        assert!(r.passou, "{}", r.resumo);
        assert!(
            r.resumo.starts_with("passou no cenário conexoes"),
            "{}",
            r.resumo
        );
    }

    #[test]
    fn conexoes_falha_se_alguma_nao_foi_atualizada_ou_foi_duas_vezes() {
        let p = PortalDeConexoes::iniciar(ConfigDeConexoes::default()).unwrap();
        let ids = p.ids();
        post(&p, &format!("/api/atualizou/{}", ids[0]));
        post(&p, &format!("/api/atualizou/{}", ids[0]));
        post(&p, &format!("/api/atualizou/{}", ids[1]));
        // a terceira ficou sem atualizar
        let r = avaliar_conexoes(&relatorio(Fim::Concluida, &[]), &p, &[], &[]);
        assert!(!r.passou);
        assert!(
            r.resumo.contains("Banco Um foi atualizada 2 vez(es)"),
            "{}",
            r.resumo
        );
        assert!(
            r.resumo.contains("Banco Tres foi atualizada 0 vez(es)"),
            "{}",
            r.resumo
        );
        assert!(!r.resumo.contains("Banco Dois foi"), "{}", r.resumo);
    }

    #[test]
    fn conexoes_falha_se_excluir_foi_tocado_mesmo_com_tudo_atualizado() {
        let p = PortalDeConexoes::iniciar(ConfigDeConexoes::default()).unwrap();
        let ids = p.ids();
        for id in &ids {
            post(&p, &format!("/api/atualizou/{id}"));
        }
        post(&p, &format!("/api/excluiu/{}", ids[0]));
        let r = avaliar_conexoes(&relatorio(Fim::Concluida, &[]), &p, &[], &[]);
        assert!(!r.passou);
        assert!(r.resumo.contains("acesso proibido"), "{}", r.resumo);
    }

    #[test]
    fn conexoes_com_pausa_falha_citando_o_motivo_da_pausa() {
        let p = PortalDeConexoes::iniciar(ConfigDeConexoes::default().com_sessao_expirada(true))
            .unwrap();
        let rel = relatorio(Fim::PausaExpirou { passo: 0 }, &[]);
        let r = avaliar_conexoes(
            &rel,
            &p,
            &["sessão expirada? entre na conta".to_owned()],
            &[],
        );
        assert!(!r.passou);
        assert!(
            r.resumo
                .contains("pediu a vez do usuário (pausa: sessão expirada? entre na conta)"),
            "{}",
            r.resumo
        );
    }

    #[test]
    fn o_resumo_de_conexoes_nunca_mostra_token_de_url_nem_valor_escondido() {
        let p = PortalDeConexoes::iniciar(ConfigDeConexoes::default()).unwrap();
        let rel = relatorio(
            Fim::Falhou {
                passo: 0,
                motivo: "x".to_owned(),
            },
            &["abriu https://meu.pluggy.ai/connections?token=SEGREDO123#frag e SegredoFicticio"],
        );
        let r = avaliar_conexoes(&rel, &p, &[], &["SegredoFicticio".to_owned()]);
        assert!(!r.passou);
        assert!(!r.resumo.contains("SEGREDO123"), "{}", r.resumo);
        assert!(!r.resumo.contains("token="), "{}", r.resumo);
        assert!(!r.resumo.contains("SegredoFicticio"), "{}", r.resumo);
        assert!(
            r.resumo.contains("https://meu.pluggy.ai/connections"),
            "{}",
            r.resumo
        );
    }

    #[test]
    fn a_pausa_anotada_guarda_o_motivo_e_expira() {
        let pausa = PausaAnotada::default();
        assert_eq!(pausa.aguardar(1, "faça login"), Decisao::Expirou);
        assert_eq!(pausa.motivos(), vec!["faça login".to_owned()]);
    }

    // ------------------------------------------------------------------ cenário soucaixa

    fn manifesto_soucaixa(mfa: &str, classe: &str, movimenta: bool, hosts: &str) -> Manifesto {
        Manifesto::de_json(&format!(
            r#"{{"nome":"contracheque-soucaixa","url_inicial":"https://soucaixa.caixa.gov.br/",
            "hosts":{hosts},"rotulo_senha":"soucaixa","rotulo_cpf":null,"mfa":{mfa},
            "movimenta_dinheiro":{movimenta},"classe":"{classe}"}}"#
        ))
        .unwrap()
    }

    const EMAIL_CAIXA: &str =
        r#"{"tipo":"email","remetente":"logincaixa@caixa.gov.br","formato":"\\d{6}"}"#;
    const DOIS_HOSTS: &str = r#"["soucaixa.caixa.gov.br","login.caixa.gov.br"]"#;

    #[test]
    fn o_cenario_soucaixa_aceita_o_manifesto_com_os_dois_hosts() {
        assert_eq!(Cenario::de_texto("soucaixa"), Some(Cenario::SouCaixa));
        assert!(Cenario::SouCaixa.descricao().contains("SouCaixa"));
        let alvo = Cenario::SouCaixa
            .conferir(&manifesto_soucaixa(
                EMAIL_CAIXA,
                "consulta",
                false,
                DOIS_HOSTS,
            ))
            .unwrap();
        assert_eq!(alvo.hosts_logicos.len(), 2);
        assert_eq!(alvo.remetente, "logincaixa@caixa.gov.br");
        assert_eq!(alvo.rotulo_senha, "soucaixa");
    }

    #[test]
    fn o_cenario_soucaixa_recusa_o_que_nao_e_dele_com_motivo_claro() {
        let outro_remetente =
            r#"{"tipo":"email","remetente":"outro@exemplo.test","formato":"\\d{6}"}"#;
        let casos = [
            (
                manifesto_soucaixa(r#"{"tipo":"manual"}"#, "consulta", false, DOIS_HOSTS),
                "MFA por e-mail",
            ),
            (
                manifesto_soucaixa(outro_remetente, "consulta", false, DOIS_HOSTS),
                "remetente",
            ),
            (
                manifesto_soucaixa(EMAIL_CAIXA, "atualizar_conexao", false, DOIS_HOSTS),
                "consulta",
            ),
            (
                manifesto_soucaixa(
                    EMAIL_CAIXA,
                    "consulta",
                    false,
                    r#"["soucaixa.caixa.gov.br"]"#,
                ),
                "login.caixa.gov.br",
            ),
        ];
        for (m, trecho) in casos {
            let e = Cenario::SouCaixa.conferir(&m).unwrap_err().to_string();
            assert!(e.contains("este roteiro não é do cenário soucaixa"), "{e}");
            assert!(e.contains(trecho), "{e}");
        }
    }

    #[test]
    fn a_caixa_do_soucaixa_so_responde_ao_remetente_e_so_depois_do_codigo() {
        let (_, mes, _) = meses_do_teste();
        let portal =
            PortalDeSouCaixa::iniciar(ConfigDoSouCaixa::nova(CPF_FICTICIO, SENHA_FICTICIA, &mes))
                .unwrap();
        let caixa = CaixaDoSouCaixa {
            portal: &portal,
            remetente: REMETENTE_DO_CODIGO.to_owned(),
        };
        let antes = SystemTime::now() - Duration::from_secs(60);
        assert!(
            caixa
                .mensagens_recentes(REMETENTE_DO_CODIGO, antes)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn soucaixa_falha_sem_login_sem_download_e_nunca_mostra_segredo() {
        let (_, mes, _) = meses_do_teste();
        let portal =
            PortalDeSouCaixa::iniciar(ConfigDoSouCaixa::nova(CPF_FICTICIO, SENHA_FICTICIA, &mes))
                .unwrap();
        let rel = RelatorioEscrito {
            fim: Fim::Falhou {
                passo: 0,
                motivo: format!("erro com {SENHA_FICTICIA}"),
            },
            baixados: 0,
            novos: 0,
            descartados: 0,
            registros: vec![format!("viu {CPF_FICTICIO}")],
            entrada_recusada: Some("o SouCaixa não aceitou a entrada".to_owned()),
        };
        let esconder = vec![SENHA_FICTICIA.to_owned(), CPF_FICTICIO.to_owned()];
        let r = avaliar_soucaixa(&rel, &portal, &mes, Path::new("/nao/existe"), &esconder);
        assert!(!r.passou);
        assert!(
            r.resumo.starts_with("falhou no cenário soucaixa"),
            "{}",
            r.resumo
        );
        assert!(r.resumo.contains("recusou a entrada"), "{}", r.resumo);
        assert!(r.resumo.contains("nenhum login aceito"), "{}", r.resumo);
        assert!(r.resumo.contains("não foi baixado"), "{}", r.resumo);
        assert!(
            r.resumo.contains("segredo apareceu nos registros"),
            "{}",
            r.resumo
        );
        assert!(!r.resumo.contains(SENHA_FICTICIA), "{}", r.resumo);
        assert!(!r.resumo.contains(CPF_FICTICIO), "{}", r.resumo);
    }
}
