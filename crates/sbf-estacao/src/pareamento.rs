//! O pareamento (spec 07, "Autenticação da estação"):
//!
//! 1. login Google pelo navegador do sistema, com retorno por loopback (`bancada-rede`);
//! 2. o `id_token` e o `nonce` vão em `POST /estacoes/pareamento`;
//! 3. a credencial que volta vai **direto** para o chaveiro (`estacao:<servidor>`), nunca
//!    para arquivo, log ou SQLite.
//!
//! Tudo aqui bloqueia: a janela roda num thread e acompanha.

use crate::credencial::GuardaDeCredencial;
use crate::servidor::{ClienteDoServidor, ErroDoServidor};
use bancada_rede::{ErroOAuth, FluxoOAuth, Instante, ProvedorOAuth, Relogio, Segredo, TokensOAuth};
use sbf_contrato::{Estacao, PedidoDePareamento};
use std::fmt;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use zeroize::Zeroize;

/// O caminho do retorno no loopback.
pub const CAMINHO_DO_RETORNO: &str = "/seu-bolso-feliz/retorno";
const NOME_DO_APP: &str = "Seu Bolso Feliz";

/// Por que o pareamento não deu certo. Nenhuma variante carrega token ou credencial.
#[derive(Debug, Clone, PartialEq)]
pub enum ErroDePareamento {
    /// Não há cliente Google configurado.
    SemGoogle,
    /// O login no navegador não terminou.
    Login(ErroOAuth),
    /// O Google não devolveu `id_token`.
    SemIdToken,
    /// O servidor recusou (login de usuário desconhecido, token inválido…) ou falhou.
    Servidor(ErroDoServidor),
    /// A credencial que veio não tem forma de token.
    CredencialInvalida,
    /// O chaveiro não guardou a credencial.
    Chaveiro(String),
}

impl fmt::Display for ErroDePareamento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SemGoogle => f.write_str("o login Google não está configurado neste app"),
            Self::Login(e) => write!(f, "login Google: {e}"),
            Self::SemIdToken => f.write_str("o Google não devolveu a prova do login"),
            Self::Servidor(e) => write!(f, "pareamento: {e}"),
            Self::CredencialInvalida => f.write_str("o servidor devolveu uma credencial inválida"),
            Self::Chaveiro(e) => write!(f, "o chaveiro não guardou a credencial: {e}"),
        }
    }
}

impl std::error::Error for ErroDePareamento {}

/// O pareamento de uma estação. Barato de clonar; vai para o thread do pareamento.
#[derive(Debug, Clone)]
pub struct Pareador {
    cliente: ClienteDoServidor,
    guarda: Arc<dyn GuardaDeCredencial>,
    google: Option<ProvedorOAuth>,
    relogio: Arc<dyn Relogio>,
}

impl Pareador {
    pub(crate) fn novo(
        cliente: ClienteDoServidor,
        guarda: Arc<dyn GuardaDeCredencial>,
        google: Option<ProvedorOAuth>,
        relogio: Arc<dyn Relogio>,
    ) -> Self {
        Self {
            cliente,
            guarda,
            google,
            relogio,
        }
    }

    /// Abre a porta de loopback e monta a página do Google. A janela abre o navegador
    /// ([`FluxoOAuth::abrir_no_navegador`]) e chama [`Pareador::concluir_login`] num thread.
    ///
    /// # Errors
    /// Sem cliente Google, ou porta indisponível.
    pub fn iniciar_login(&self) -> Result<FluxoOAuth, ErroDePareamento> {
        let google = self.google.clone().ok_or(ErroDePareamento::SemGoogle)?;
        FluxoOAuth::iniciar(google, CAMINHO_DO_RETORNO, NOME_DO_APP)
            .map_err(ErroDePareamento::Login)
    }

    /// Espera o retorno do navegador e troca o código pelos tokens. Bloqueia.
    ///
    /// # Errors
    /// Prazo, cancelamento, login negado ou falha da troca.
    pub fn concluir_login(
        &self,
        fluxo: FluxoOAuth,
        prazo: Instante,
        cancelar: &AtomicBool,
    ) -> Result<TokensOAuth, ErroDePareamento> {
        let codigo = fluxo
            .esperar(self.relogio.as_ref(), prazo, cancelar)
            .map_err(ErroDePareamento::Login)?;
        codigo
            .trocar(self.cliente.rede())
            .map_err(ErroDePareamento::Login)
    }

    /// Troca a prova do login por uma credencial de estação e a guarda no chaveiro.
    /// Bloqueia.
    ///
    /// # Errors
    /// Login recusado pelo servidor, falha de rede ou do chaveiro.
    pub fn parear(
        &self,
        tokens: &TokensOAuth,
        nome: &str,
        sistema: &str,
    ) -> Result<Estacao, ErroDePareamento> {
        let id_token = tokens
            .id_token
            .as_ref()
            .ok_or(ErroDePareamento::SemIdToken)?;
        self.parear_com_prova(id_token, &tokens.nonce, nome, sistema)
    }

    /// O mesmo, com a prova já em mãos (testes e servidor local de teste).
    ///
    /// # Errors
    /// Login recusado pelo servidor, falha de rede ou do chaveiro.
    pub fn parear_com_prova(
        &self,
        id_token: &Segredo,
        nonce: &Segredo,
        nome: &str,
        sistema: &str,
    ) -> Result<Estacao, ErroDePareamento> {
        let mut pedido = PedidoDePareamento {
            id_token: id_token.expor().to_owned(),
            nonce: nonce.expor().to_owned(),
            nome: nome.to_owned(),
            sistema: sistema.to_owned(),
        };
        let resultado = self.cliente.parear(&pedido);
        pedido.id_token.zeroize();
        pedido.nonce.zeroize();
        let pareamento = resultado.map_err(ErroDePareamento::Servidor)?;
        let credencial = Segredo::novo(pareamento.credencial.clone());
        let mut resto = pareamento;
        resto.credencial.zeroize();
        if bancada_rede::Autorizacao::portador(&credencial).is_none() {
            return Err(ErroDePareamento::CredencialInvalida);
        }
        self.guarda
            .gravar(self.cliente.servidor().host(), credencial)
            .map_err(ErroDePareamento::Chaveiro)?;
        Ok(resto.estacao)
    }
}

/// O sistema desta máquina, como vai no pareamento.
pub fn sistema_atual() -> &'static str {
    if cfg!(target_os = "windows") {
        "windows"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else {
        "linux"
    }
}
