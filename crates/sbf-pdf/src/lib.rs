//! A abertura de PDF protegido do Seu Bolso Feliz (spec 06, "PDF protegido"; Apêndice V6).
//!
//! - [`Estrategia`]: como montar a senha candidata (`cpf_completo`, `cpf_pontuado`,
//!   `cpf_primeiros:N`, `cpf_ultimos:N`, `senha_propria:<rotulo>`), a partir de uma
//!   [`FonteDeSegredos`] (o `sbf-chaveiro` implementa). A candidata vive só em memória,
//!   zerada ao descartar ([`Candidata`]).
//! - [`PdfProtegido`]: o `lopdf` decifra em memória e o PDF decifrado vai pela entrada padrão
//!   para `pdftotext -layout -l 3 - -`; fallback `qpdf --password-file=-` em pipe. **A senha
//!   nunca vai em argumento de linha de comando, e nenhuma cópia decifrada vai para o
//!   disco.** O original nunca muda.
//! - [`rodada`]: a rodada sobre o inventário (alvos, regras em ordem, tentativas, limite de
//!   falhas, pendência e a proposta de regra), gravada por `sbf_inventario::protegidos`.

pub mod abertura;
pub mod estrategia;
pub mod ferramenta;
pub mod fonte;
pub mod processo;
pub mod rodada;
pub mod texto;

pub use abertura::{
    ARGUMENTOS_DO_PDFTOTEXT, Desfecho, Ferramentas, PdfProtegido, TextoExtraido, Via,
    extrair_pelo_qpdf,
};
pub use estrategia::{Candidata, Estrategia, EstrategiaDesconhecida};
pub use fonte::{ErroDeFonte, FonteDeSegredos};
pub use processo::FalhaDeFerramenta;
pub use rodada::{
    OpcoesDaRodada, PropostaDeRegra, RelatorioDaRodada, ResultadoDoAlvo, Situacao, abrir_protegidos,
};
pub use texto::impressao_canonica;
