//! Erros do chaveiro. Nenhuma variante carrega segredo, senha, CPF, rótulo nem conteúdo de
//! arquivo: o texto pode ir para a tela e para um diagnóstico como está.

use bancada_segredos::ErroDeSegredo;
use std::fmt;

/// Por que o chaveiro não fez o que se pediu.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum ErroDoChaveiro {
    /// A senha-mestra está errada **ou** o `cofre.enc` foi alterado, cortado ou corrompido.
    /// As duas causas dão o mesmo erro, de propósito: nada é lido do arquivo.
    SenhaOuArquivoInvalido,
    /// Não há `cofre.enc` no caminho indicado.
    ArquivoAntigoAusente,
    /// O chaveiro já tem um arquivo de segredos (sem keyring do sistema) trancado com outra
    /// senha-mestra: destranque-o antes de importar.
    ChaveiroTrancado,
    /// A pasta do chaveiro fica dentro do cofre (o acervo de documentos). O chaveiro nunca
    /// mora junto dos documentos.
    PastaDentroDoCofre,
    /// O sistema não diz onde fica a pasta de dados do app (sem `HOME`, `XDG_DATA_HOME` nem
    /// `APPDATA`).
    SemPastaDeDados,
    /// O que está no chaveiro não confere com o `cofre.enc`: o arquivo antigo não pode ser
    /// apagado.
    NaoConfere,
    /// Um rótulo do `cofre.enc` não serve de rótulo no chaveiro (vazio, longo demais ou com
    /// caractere de controle).
    RotuloInvalido,
    /// Falha de disco ao ler ou apagar o `cofre.enc`.
    Disco { tipo: std::io::ErrorKind },
    /// O CPF digitado não confere (dígitos verificadores errados ou tamanho diferente de 11).
    /// A mensagem nunca repete o que foi digitado.
    CpfInvalido,
    /// A guarda de segredos (keyring do sistema ou arquivo de fallback) falhou.
    Guarda(ErroDeSegredo),
}

impl ErroDoChaveiro {
    pub(crate) fn disco(erro: &std::io::Error) -> Self {
        if erro.kind() == std::io::ErrorKind::NotFound {
            Self::ArquivoAntigoAusente
        } else {
            Self::Disco { tipo: erro.kind() }
        }
    }
}

impl From<ErroDeSegredo> for ErroDoChaveiro {
    fn from(erro: ErroDeSegredo) -> Self {
        Self::Guarda(erro)
    }
}

impl fmt::Display for ErroDoChaveiro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SenhaOuArquivoInvalido => f.write_str(
                "Não foi possível abrir o cofre.enc: a senha-mestra está errada ou o arquivo foi \
                 alterado. Nada foi lido dele.",
            ),
            Self::ArquivoAntigoAusente => f.write_str("Não há cofre.enc nesse caminho."),
            Self::ChaveiroTrancado => f.write_str(
                "O chaveiro já existe e está trancado com outra senha-mestra: destranque-o antes \
                 de importar.",
            ),
            Self::PastaDentroDoCofre => {
                f.write_str("A pasta do chaveiro não pode ficar dentro do cofre de documentos.")
            }
            Self::SemPastaDeDados => {
                f.write_str("Não foi possível descobrir a pasta de dados do app neste sistema.")
            }
            Self::NaoConfere => f.write_str(
                "O chaveiro não confere com o cofre.enc: o arquivo antigo fica onde está.",
            ),
            Self::RotuloInvalido => f.write_str(
                "Um rótulo do cofre.enc é inválido (vazio, longo demais ou com caractere de \
                 controle). Nada foi importado.",
            ),
            Self::CpfInvalido => {
                f.write_str("O CPF não confere: confira os 11 dígitos. Nada foi guardado.")
            }
            Self::Disco { tipo } => write!(f, "Falha de disco no cofre.enc: {tipo}."),
            Self::Guarda(erro) => write!(f, "{erro}"),
        }
    }
}

impl std::error::Error for ErroDoChaveiro {}
