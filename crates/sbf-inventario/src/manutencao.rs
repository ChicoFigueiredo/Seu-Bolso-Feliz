//! Comandos de manutenção que o binário expõe na linha de comando, fora da janela.
//!
//! - `sbf reprojetar --inventario <pasta-do-cofre>` chama [`reprojetar_a_pasta`] e imprime
//!   [`ResultadoDaManutencao::resumo`]: refaz a projeção a partir do log de decisões, numa
//!   transação só (INV-05). Não migra: um banco que ainda não foi migrado é recusado, porque a
//!   migração pede o aviso e a cópia antes (INV-01), e isso é da janela.

use std::path::{Path, PathBuf};

use crate::banco::{ErroDoBanco, Inventario};
use crate::comando::ErroDeComando;
use crate::projecao::ResumoDaReprojecao;

/// O nome do banco dentro da pasta do cofre.
pub const NOME_DO_BANCO: &str = "inventario.sqlite";

/// Por que a manutenção não rodou. Nada mudou no banco.
#[derive(Debug, thiserror::Error)]
pub enum ErroDeManutencao {
    #[error("não há {NOME_DO_BANCO} em {0}")]
    SemBanco(PathBuf),
    #[error("o inventário não abriu: {0}")]
    Abrir(#[from] ErroDoBanco),
    #[error(
        "o inventário ainda não foi migrado: abra-o uma vez no app (que avisa e faz a cópia antes)"
    )]
    NaoMigrado,
    #[error("a reprojeção falhou e nada mudou: {0}")]
    Reprojetar(#[from] ErroDeComando),
}

/// O que a manutenção fez, para a linha de comando.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultadoDaManutencao {
    pub banco: PathBuf,
    pub reprojecao: ResumoDaReprojecao,
}

impl ResultadoDaManutencao {
    /// A saída resumida: uma linha, sem nenhum dado do inventário além das contagens.
    pub fn resumo(&self) -> String {
        format!(
            "reprojetado {}: {} decisão(ões) reaplicada(s), {} achado(s) ou registro(s) com \
             o decisor recalculado.",
            self.banco.display(),
            self.reprojecao.decisoes,
            self.reprojecao.alvos
        )
    }
}

/// `sbf reprojetar --inventario <pasta>`: abre `<pasta>/inventario.sqlite` (sem criar), exige
/// o banco migrado e refaz a projeção a partir do log de decisões.
///
/// # Errors
/// [`ErroDeManutencao`]: sem banco, banco que não abre, não migrado, ou falha da
/// reprojeção (a transação é desfeita inteira).
pub fn reprojetar_a_pasta(pasta: &Path) -> Result<ResultadoDaManutencao, ErroDeManutencao> {
    let banco = pasta.join(NOME_DO_BANCO);
    if !banco.is_file() {
        return Err(ErroDeManutencao::SemBanco(pasta.to_path_buf()));
    }
    let mut inventario = Inventario::abrir(&banco)?;
    if inventario.plano()?.precisa_migrar() {
        return Err(ErroDeManutencao::NaoMigrado);
    }
    let reprojecao = inventario.reprojetar()?;
    Ok(ResultadoDaManutencao { banco, reprojecao })
}
