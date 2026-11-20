//! O log do worker: um arquivo na pasta de dados, rotacionado por tamanho (`worker.log`,
//! `worker.log.1`, ...).
//!
//! **O que nunca entra:** conteúdo de documento, nome de arquivo, caminho, texto de erro de
//! programa externo, segredo. Por construção: o registro só aceita um [`Evento`], que tem
//! campos numéricos e rótulos fixos (o id da fonte, como `disco:p`, é o único texto livre, e
//! é um identificador que o usuário escolheu, não um caminho). Falhas entram por categoria
//! ([`Falha`]), nunca pela mensagem.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::Resultado;

/// Tamanho a partir do qual o arquivo é rotacionado.
pub const LIMITE_DO_ARQUIVO: u64 = 1024 * 1024;
/// Quantos arquivos antigos se guardam.
pub const ARQUIVOS_ANTIGOS: u32 = 3;

/// Por que um passo falhou, sem a mensagem original.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Falha {
    /// Banco ocupado ou com erro.
    Banco,
    /// Disco ou permissão.
    Disco,
    /// Programa externo (rclone, claude) indisponível ou com erro.
    Externo,
    /// O provedor de IA não respondeu ou parou.
    Provedor,
    /// A autorização de uma conta venceu ou foi revogada (Gmail `invalid_grant`): não adianta
    /// tentar de novo até a pessoa entrar outra vez.
    Autorizacao,
    /// Outro.
    Outra,
}

impl fmt::Display for Falha {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Banco => "banco",
            Self::Disco => "disco",
            Self::Externo => "externo",
            Self::Provedor => "provedor",
            Self::Autorizacao => "autorizacao",
            Self::Outra => "outra",
        })
    }
}

/// O que o worker conta no log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Evento<'a> {
    /// O worker subiu.
    Iniciou {
        /// Pastas vigiadas de fato.
        vigiadas: usize,
    },
    /// O worker parou (sinal).
    Parou,
    /// O vigia indexou arquivos.
    Vigia {
        /// A fonte.
        fonte: &'a str,
        /// Origens novas.
        novos: u64,
        /// Arquivos avisados.
        avisados: usize,
    },
    /// Varredura de reconciliação.
    Reconciliou {
        /// A fonte.
        fonte: &'a str,
        /// Origens novas.
        novos: u64,
        /// Erros de leitura.
        erros: u64,
    },
    /// Coleta de nuvem.
    Nuvem {
        /// Itens listados.
        listados: usize,
        /// Itens baixados.
        baixados: usize,
    },
    /// Coleta do Gmail.
    Gmail {
        /// Mensagens lidas.
        mensagens: usize,
        /// Conteúdos novos no inventário.
        novos: usize,
    },
    /// Coleta de mensageiros.
    Mensageiros {
        /// Mensagens vistas.
        mensagens: usize,
        /// Origens novas.
        novos: usize,
    },
    /// Coleta do Telegram (conversas seguidas).
    Telegram {
        /// Conversas consultadas.
        conversas: usize,
        /// Arquivos novos.
        novos: usize,
    },
    /// Roteiros agendados executados.
    Roteiros {
        /// Roteiros executados.
        executados: usize,
        /// Arquivos baixados.
        baixados: usize,
    },
    /// Ciclo da triagem.
    Triagem {
        /// Itens enviados à IA.
        enviados: usize,
        /// Sugestões gravadas pela IA.
        sugestoes: usize,
        /// Sugestões de regra.
        por_regra: usize,
        /// Itens que ainda esperam.
        na_fila: usize,
    },
    /// Atualização do índice local (spec 10).
    Indice {
        /// Trechos novos ou mudados (passaram pelo modelo).
        embutidos: usize,
        /// Trechos iguais (sem recalcular).
        mantidos: usize,
        /// Trechos que saíram.
        removidos: usize,
    },
    /// OCR das imagens novas (só contagens: nem nome nem texto da imagem).
    Ocr {
        /// Imagens com texto lido.
        lidas: usize,
        /// Imagens sem texto.
        sem_texto: usize,
        /// Imagens que não puderam ser lidas.
        falhas: usize,
        /// A causa mais comum das falhas (uma palavra: sem caminho nem conteúdo).
        causa: Option<&'static str>,
    },
    /// Análise dos pontos de atenção (spec 10).
    Pontos {
        /// Pontos novos.
        novos: usize,
        /// Pontos que voltaram por mudança da evidência.
        reabertos: usize,
        /// Pontos fechados porque a evidência sumiu.
        fechados: usize,
    },
    /// Um passo falhou.
    Falhou {
        /// O passo (`vigia`, `reconciliacao`, `nuvem`, `mensageiros`, `telegram`, `triagem`, `estado`).
        passo: &'static str,
        /// A categoria.
        falha: Falha,
    },
    /// Um passo falhou numa conta específica (e-mail: uma conta com autorização vencida não
    /// para as outras).
    FalhouConta {
        /// O passo.
        passo: &'static str,
        /// A categoria.
        falha: Falha,
        /// O id da conta (`gmail-2`), nunca o e-mail.
        conta: &'a str,
    },
}

impl fmt::Display for Evento<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Iniciou { vigiadas } => write!(f, "iniciou vigiadas={vigiadas}"),
            Self::Parou => f.write_str("parou"),
            Self::Vigia {
                fonte,
                novos,
                avisados,
            } => write!(f, "vigia fonte={fonte} avisados={avisados} novos={novos}"),
            Self::Reconciliou {
                fonte,
                novos,
                erros,
            } => write!(f, "reconciliou fonte={fonte} novos={novos} erros={erros}"),
            Self::Nuvem { listados, baixados } => {
                write!(f, "nuvem listados={listados} baixados={baixados}")
            }
            Self::Gmail { mensagens, novos } => {
                write!(f, "gmail mensagens={mensagens} novos={novos}")
            }
            Self::Mensageiros { mensagens, novos } => {
                write!(f, "mensageiros mensagens={mensagens} novos={novos}")
            }
            Self::Telegram { conversas, novos } => {
                write!(f, "telegram conversas={conversas} novos={novos}")
            }
            Self::Roteiros {
                executados,
                baixados,
            } => write!(f, "roteiros executados={executados} baixados={baixados}"),
            Self::Triagem {
                enviados,
                sugestoes,
                por_regra,
                na_fila,
            } => write!(
                f,
                "triagem enviados={enviados} sugestoes={sugestoes} por_regra={por_regra} na_fila={na_fila}"
            ),
            Self::Indice {
                embutidos,
                mantidos,
                removidos,
            } => write!(
                f,
                "indice embutidos={embutidos} mantidos={mantidos} removidos={removidos}"
            ),
            Self::Ocr {
                lidas,
                sem_texto,
                falhas,
                causa,
            } => {
                write!(f, "ocr lidas={lidas} sem_texto={sem_texto} falhas={falhas}")?;
                match causa {
                    Some(c) => write!(f, " causa={c}"),
                    None => Ok(()),
                }
            }
            Self::Pontos {
                novos,
                reabertos,
                fechados,
            } => write!(
                f,
                "pontos novos={novos} reabertos={reabertos} fechados={fechados}"
            ),
            Self::Falhou { passo, falha } => write!(f, "falhou passo={passo} causa={falha}"),
            Self::FalhouConta {
                passo,
                falha,
                conta,
            } => write!(f, "falhou passo={passo} causa={falha} conta={conta}"),
        }
    }
}

/// O log em arquivo.
#[derive(Debug)]
pub struct Registro {
    caminho: PathBuf,
    arquivo: Option<File>,
    limite: u64,
}

impl Registro {
    /// Abre (ou cria) o log em `caminho`.
    ///
    /// # Errors
    /// Falha de disco.
    pub fn abrir(caminho: &Path) -> Resultado<Self> {
        Self::com_limite(caminho, LIMITE_DO_ARQUIVO)
    }

    /// Como [`Registro::abrir`], com o limite de rotação dado (testes).
    ///
    /// # Errors
    /// Falha de disco.
    pub fn com_limite(caminho: &Path, limite: u64) -> Resultado<Self> {
        let mut r = Self {
            caminho: caminho.to_path_buf(),
            arquivo: None,
            limite,
        };
        r.garantir_aberto()?;
        Ok(r)
    }

    fn garantir_aberto(&mut self) -> Resultado<()> {
        if self.arquivo.is_none() {
            let mut o = OpenOptions::new();
            o.create(true).append(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt as _;
                o.mode(0o600);
            }
            self.arquivo = Some(o.open(&self.caminho)?);
        }
        Ok(())
    }

    fn antigo(&self, n: u32) -> PathBuf {
        let mut s = self.caminho.clone().into_os_string();
        s.push(format!(".{n}"));
        PathBuf::from(s)
    }

    fn rotacionar(&mut self) -> Resultado<()> {
        self.arquivo = None;
        let _ = std::fs::remove_file(self.antigo(ARQUIVOS_ANTIGOS));
        for n in (1..ARQUIVOS_ANTIGOS).rev() {
            let _ = std::fs::rename(self.antigo(n), self.antigo(n + 1));
        }
        std::fs::rename(&self.caminho, self.antigo(1))?;
        self.garantir_aberto()
    }

    /// Escreve um evento com a hora (UTC, segundos unix).
    ///
    /// # Errors
    /// Falha de disco.
    pub fn registrar(&mut self, evento: &Evento<'_>) -> Resultado<()> {
        self.garantir_aberto()?;
        let tamanho = self
            .arquivo
            .as_ref()
            .map_or(0, |f| f.metadata().map_or(0, |m| m.len()));
        if tamanho >= self.limite {
            self.rotacionar()?;
        }
        let agora = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        if let Some(f) = self.arquivo.as_mut() {
            writeln!(f, "{agora} {evento}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn rotaciona_por_tamanho_e_guarda_so_tres_antigos() {
        let d = tempfile::tempdir().unwrap();
        let caminho = d.path().join("worker.log");
        let mut r = Registro::com_limite(&caminho, 200).unwrap();
        for _ in 0..200 {
            r.registrar(&Evento::Nuvem {
                listados: 10,
                baixados: 2,
            })
            .unwrap();
        }
        assert!(caminho.exists());
        for n in 1..=ARQUIVOS_ANTIGOS {
            assert!(d.path().join(format!("worker.log.{n}")).exists(), "{n}");
        }
        assert!(!d.path().join("worker.log.4").exists());
        assert!(std::fs::metadata(&caminho).unwrap().len() < 400);
    }

    #[test]
    fn so_aceita_eventos_tipados_sem_caminho_nem_mensagem() {
        let d = tempfile::tempdir().unwrap();
        let caminho = d.path().join("worker.log");
        let mut r = Registro::abrir(&caminho).unwrap();
        r.registrar(&Evento::Vigia {
            fonte: "disco:p",
            novos: 2,
            avisados: 3,
        })
        .unwrap();
        r.registrar(&Evento::Falhou {
            passo: "nuvem",
            falha: Falha::Externo,
        })
        .unwrap();
        let t = std::fs::read_to_string(&caminho).unwrap();
        assert!(t.contains("vigia fonte=disco:p avisados=3 novos=2"), "{t}");
        assert!(t.contains("falhou passo=nuvem causa=externo"), "{t}");
        r.registrar(&Evento::FalhouConta {
            passo: "gmail",
            falha: Falha::Autorizacao,
            conta: "gmail-2",
        })
        .unwrap();
        let t = std::fs::read_to_string(&caminho).unwrap();
        assert!(
            t.contains("falhou passo=gmail causa=autorizacao conta=gmail-2"),
            "{t}"
        );
        assert!(!t.contains('/'), "nenhum caminho: {t}");
    }
}
