//! O chaveiro do Seu Bolso Feliz (spec 06, "Chaveiro"), sobre a `bancada-segredos`.
//!
//! - Guarda o CPF (chave `cpf`) e as senhas próprias de PDF (chaves `senha:<rotulo>`) sob o
//!   serviço "Seu Bolso Feliz": no keyring do sistema ou, sem ele (Linux sem Secret Service),
//!   no arquivo de fallback da bancada (Argon2id + XChaCha20-Poly1305) na pasta de dados do
//!   app, **nunca dentro do cofre** ([`pasta_padrao_do_chaveiro`]).
//! - Importa o `segredos/cofre.enc` antigo do `segredos.ts` (scrypt + AES-256-GCM) com a
//!   senha-mestra pedida uma vez ([`Chaveiro::importar`]); senha errada e arquivo adulterado
//!   dão o mesmo erro. Depois de conferir ([`Chaveiro::conferir_importacao`]), **oferece**
//!   apagar o arquivo antigo ([`OfertaDeApagar`]); nunca apaga sozinho.
//! - Nenhum valor vai para log (o crate não loga), SQLite ou arquivo temporário. O `Debug` de
//!   todo tipo daqui mostra só rótulos. Os bastidores leem só [`Chaveiro::rotulos_de_senha`] e
//!   [`Chaveiro::tem_cpf`].
//!
//! ```no_run
//! use bancada_segredos::Segredo;
//! use sbf_chaveiro::Chaveiro;
//! use std::path::Path;
//!
//! let chaveiro = Chaveiro::abrir_na_pasta_padrao()?;
//! let mestra = Segredo::novo("a senha-mestra digitada");
//! let cofre_enc = Path::new("/caminho/do/cofre/segredos/cofre.enc");
//! let resumo = chaveiro.importar(cofre_enc, &mestra)?;
//! let oferta = chaveiro.conferir_importacao(cofre_enc, &mestra)?;
//! // A janela pergunta; só se o usuário aceitar:
//! oferta.apagar()?;
//! # let _ = resumo;
//! # Ok::<(), sbf_chaveiro::ErroDoChaveiro>(())
//! ```

mod arquivo_antigo;
mod chaveiro;
mod cpf;
mod erro;
mod fonte;
mod pasta;

pub use chaveiro::{
    CHAVE_DO_CPF, Chaveiro, OfertaDeApagar, PREFIXO_DE_SENHA, ResumoDaImportacao, SERVICO,
};
pub use erro::ErroDoChaveiro;
pub use pasta::{
    SUBPASTA_DO_CHAVEIRO, Sistema, garantir_fora_do_cofre, pasta_de_dados_de,
    pasta_de_dados_do_app, pasta_padrao_do_chaveiro,
};

// Para a janela não precisar depender da bancada só por estes.
pub use bancada_segredos::{Segredo, Situacao};
