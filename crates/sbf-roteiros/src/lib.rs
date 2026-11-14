//! Roteiros do SBF (ADR-016, spec 10 C1): navegam em sites sem API, como o e-CAC, a CAIXA e o
//! internet banking, para **consultar e baixar** documentos. Nunca pagam, transferem,
//! contratam nem alteram cadastro.
//!
//! - [`bloqueio`]: a lista de bloqueio, no navegador e no executor (RO-02).
//! - [`pagina`]: a página simplificada, sem valores de campo, e a detecção de MFA e CAPTCHA
//!   (RO-01, RO-03).
//! - [`executor`]: roda os passos sem IA, com pausa para o usuário (RO-03).
//! - [`gravacao`]: a primeira coleta guiada pela IA e o conserto proposto (RO-04).
//! - [`inventario`]: o download vira achado com origem `roteiro:<nome>/<execução>/<arquivo>`
//!   (RO-05).
//! - [`chrome`] e [`cdp`]: o Chrome visível controlado por CDP, num perfil dedicado.
//! - [`site_falso`]: o site de mentira local (127.0.0.1) dos testes e da demonstração.
//!
//! **A senha** vive só em memória (`Segredo`/`Zeroizing`), entre o chaveiro e o
//! `Input.insertText` do CDP. O roteiro gravado guarda só o rótulo do chaveiro; a IA recebe a
//! página sem valor de campo nenhum; nada disso vai a log, `Debug` ou erro.

pub mod banco;
pub mod bloqueio;
pub mod erro;
pub mod executor;
pub mod navegador;
pub mod pagina;
pub mod passo;

pub use banco::{BancoDeRoteiros, EstadoDaExecucao, LinhaDeExecucao, LinhaDeRoteiro, OrigemDaVersao, Proposta};
pub use bloqueio::{Bloqueio, Candidato};
pub use erro::{ErroDeRoteiro, Resultado};
pub use navegador::Navegador;
pub use pagina::{MotivoDePausa, Pagina};
pub use passo::{Passo, Roteiro};
