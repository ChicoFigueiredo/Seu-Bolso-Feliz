//! Estante e cópia de segurança do cofre (spec 06, INV-11 e INV-12). Sem egui.
//!
//! - [`estante::Estante`]: "Atualizar estante". Só documentos, com o nome de `vista-nome.ts`,
//!   incremental (renomeia em vez de reenviar). Rode sob demanda.
//! - [`copia::CopiaDeSeguranca`]: "Fazer cópia de segurança". Acervo e instantâneo do banco
//!   num remoto rclone crypt, com a senha do chaveiro fora da linha de comando.
//!   [`copia::AVISO_DA_SENHA`] é o texto que a tela mostra.
//!
//! Ambos recebem a conexão do inventário (`&rusqlite::Connection`, aberta para leitura serve)
//! e a raiz do cofre; nenhum depende de `sbf-inventario` nem de `sbf-chaveiro`.

pub mod copia;
pub mod destinos;
pub mod erro;
pub mod estante;
pub mod nome;
pub mod nuvens;
mod rclone;

pub use bancada_segredos::Segredo;
pub use copia::{AVISO_DA_SENHA, CopiaDeSeguranca, RelatorioDaCopia, RelatorioDaRestauracao};
pub use destinos::{
    Destino, ListaDeDestinos, RelatorioDoDestino, TipoDeDestino, atualizar_destinos,
    detectar_remotos, planejar_destinos,
};
pub use erro::{Erro, Resultado};
pub use estante::{Andamento, Estante, Fase, Plano, RelatorioDaEstante};
pub use nuvens::{Conexao, Conexoes, Nuvem};
pub use rclone::{Rclone, SAIDA_MAXIMA_DO_RCLONE, VARIAVEIS_DO_RCLONE, localizar_rclone};
