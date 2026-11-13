//! O lado da **estação** do Seu Bolso Feliz desktop (spec 07, fatia 2; ADR-013): o que
//! pareia com o servidor, guarda a credencial, mantém o cache local do livro-caixa e
//! sincroniza. Sem egui.
//!
//! - [`Servidor`] e [`ClienteDoServidor`]: o contrato HTTP (`sbf-contrato`) sobre o portão
//!   da `bancada-rede` (só HTTPS, só o endereço do catálogo, credencial nunca em log).
//! - [`GuardaDeCredencial`]: a credencial no chaveiro (`estacao:<servidor>`), nunca em
//!   arquivo, log ou SQLite.
//! - [`CacheDoLivro`]: o `livro-caixa.sqlite` na pasta de dados do app (nunca no cofre nem
//!   no `inventario.sqlite`), coleções inteiras com `ETag` (Q5) trocadas numa transação só,
//!   histórico de trocas e fila de alterações.
//! - [`Sincronizador`]: uma troca; estação revogada apaga cache e credencial (Q11).
//! - [`EstacaoLocal`]: o que a janela usa, implementando os traits
//!   [`sbf_inventario::alteracao::Sincronizacao`] e [`sbf_inventario::alteracao::Alteracoes`].
//! - [`Pareador`]: login Google por loopback e pareamento.

mod alteracoes;
mod cache;
mod credencial;
mod estacao;
mod pareamento;
mod pasta;
mod servidor;
mod sincronizacao;
mod texto;

pub use alteracoes::{SEM_CATEGORIA, rotulo_da_movimentacao};
pub use cache::{
    ARQUIVO_DO_CACHE, CacheDoLivro, ErroDoCache, RegistroDeAlteracao, ResultadoDoCache,
    apagar_arquivos,
};
pub use credencial::{CredencialDaEstacao, GuardaDeCredencial, GuardaEmMemoria};
pub use estacao::{ConfiguracaoDaEstacao, EstacaoLocal, INTERVALO_PADRAO};
pub use pareamento::{CAMINHO_DO_RETORNO, ErroDePareamento, Pareador, sistema_atual};
pub use pasta::{
    SUBPASTA_DO_LIVRO_CAIXA, caminho_do_cache, garantir_fora_do_cofre, pasta_de_dados_padrao,
};
pub use servidor::{ClienteDoServidor, ColecaoNova, ErroDoServidor, Servidor};
pub use sincronizacao::{Desfecho, Sincronizador, comparar_versoes, revogar_localmente};
pub use texto::{data_br, dia_e_mes, dinheiro, instante_iso};
