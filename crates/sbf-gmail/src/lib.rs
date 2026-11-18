//! O Gmail no desktop: os anexos de e-mail entram no cofre e passam pelas regras de
//! reconhecimento (spec 13), na mesma esteira dos arquivos soltos na janela.
//!
//! - [`fonte`]: o cliente do Gmail atrás de um trait ([`FonteDeGmail`]); o real reaproveita o
//!   OAuth2 do `sbf-coletor` e lê as credenciais de variáveis de ambiente;
//! - [`coletor`]: a coleta (`sbf gmail coletar`, o worker e o MCP): lista, baixa os anexos para
//!   `<cofre>/entrada/gmail/`, registra o achado com origem `gmail:<mensagem>:<parte>`, deduplica
//!   por sha256 e roda o reconhecimento em cada documento novo; o corpo de e-mail sem anexo vira
//!   uma entrada do tipo Mensagem;
//! - [`reconhecimento`]: o trait de reconhecimento e o real, sobre o `ReconhecedorDoCofre`;
//! - [`estado`]: o estado da conexão, para a área Conexões.
//!
//! Nada de conteúdo de e-mail vai para log nem para mensagem de erro.

pub mod coletor;
pub mod corpo;
pub mod erro;
pub mod estado;
pub mod fonte;
pub mod gravador;
pub mod reconhecimento;

pub use coletor::{CONSULTA_PADRAO, OpcoesDaColeta, ResumoDaColeta, coletar};
pub use erro::{Erro, Resultado};
pub use estado::{EstadoDaConexao, SituacaoDaConexao, estado_do_gmail};
pub use fonte::{ClienteReal, FonteDeGmail};
pub use reconhecimento::{Desfecho, Reconhecimento, ReconhecimentoDoCofre};
