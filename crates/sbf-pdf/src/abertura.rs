//! A abertura de um PDF protegido (spike `docs/specs/spikes/2026-11-12-pdf-protegido.md`,
//! opção C): o `lopdf` decifra o original **em memória**, o PDF decifrado vai pela entrada
//! padrão para `pdftotext -layout -l 3 - -`, e nem a senha nem uma cópia decifrada saem do
//! processo para o disco ou para um argumento.
//!
//! Fallback, para o que o `lopdf` não abre: `qpdf --password-file=- --decrypt <original> -`
//! em pipe para o `pdftotext` (opção B2). A senha vai pela entrada padrão do `qpdf`; a cópia
//! decifrada só existe no pipe.
//!
//! O original só é lido, nunca escrito.

use crate::estrategia::Candidata;
use crate::processo::{self, FalhaDeFerramenta, comando, rodar_com_entrada};
use crate::texto::{MINIMO_DE_LETRAS, decodificar_saida, espaco_do_js, letras};
use lopdf::encryption::DecryptionError;
use lopdf::{Document, LoadOptions};
use sbf_processo::{escrever_e_fechar, esperar, ler_com_limite};
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use zeroize::Zeroizing;

/// Os argumentos do `pdftotext`, iguais aos do `e3-extrair.ts`, com o PDF pela entrada padrão.
pub const ARGUMENTOS_DO_PDFTOTEXT: [&str; 5] = ["-layout", "-l", "3", "-", "-"];

/// O maior PDF que a abertura lê para a memória (achado A10 da revisão de segurança da fatia
/// 1). Fatura e boleto têm poucos MiB; acima disto é um anexo hostil ou não é documento.
pub const TAMANHO_MAXIMO_DO_PDF: u64 = 50 * 1024 * 1024;

/// As ferramentas externas e o limite de tempo de cada chamada.
///
/// O padrão resolve cada uma **uma vez**, por caminho absoluto nas pastas do sistema
/// ([`sbf_processo::localizar`]), nunca pelo `PATH`. Os campos são o lugar da
/// configuração explícita (Opções), e são o caminho a mostrar em Opções e Bastidores. Um
/// caminho relativo, ausente ou em pasta gravável por outros não roda: dá
/// [`FalhaDeFerramenta::NaoEncontrada`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ferramentas {
    /// O `pdftotext` (poppler). Padrão: o de `/usr/bin` ou `/usr/local/bin`.
    pub pdftotext: PathBuf,
    /// O `qpdf` do fallback, ou `None` para não usar. Padrão: o de `/usr/bin` ou
    /// `/usr/local/bin`.
    pub qpdf: Option<PathBuf>,
    /// O limite de cada processo (o `TIMEOUT_PDF_MS` do `e3-extrair.ts`: 30 s).
    pub limite_de_tempo: Duration,
}

impl Default for Ferramentas {
    fn default() -> Self {
        Self {
            pdftotext: sbf_processo::localizar_ou_nome("pdftotext"),
            qpdf: Some(sbf_processo::localizar_ou_nome("qpdf")),
            limite_de_tempo: Duration::from_secs(30),
        }
    }
}

/// Quem decifrou.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Via {
    /// O `lopdf`, em memória (caminho principal).
    Lopdf,
    /// O `qpdf` em pipe (fallback).
    Qpdf,
    /// O PDF abriu sem senha (só tem senha de dono, ou não está cifrado).
    SemSenha,
}

/// O texto das 3 primeiras páginas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TextoExtraido {
    /// A saída do `pdftotext -layout -l 3`, como o Bun a lê.
    pub texto: String,
    /// O `lopdf` precisou reconstruir a tabela de referências ("abriu com aviso").
    pub reparado: bool,
    pub via: Via,
}

/// O desfecho de uma tentativa. Só [`Desfecho::SenhaErrada`] conta para o limite de falhas.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Desfecho {
    /// Abriu e tem texto.
    Aberto(TextoExtraido),
    /// Abriu sem reparo e veio sem texto (menos de 15 letras): candidato a OCR.
    SemTexto { via: Via },
    /// A senha não confere. Também é o que um `/U` danificado dá: indistinguível.
    SenhaErrada,
    /// Não é um PDF legível, ou abriu reparado e sem nenhuma letra.
    Corrompido,
    /// Cifra que o `lopdf` não implementa e sem `qpdf` para o fallback.
    CifraNaoSuportada,
    /// A senha conferiu, mas o `pdftotext` (ou o `qpdf`) falhou.
    FalhaNaExtracao(FalhaDeFerramenta),
}

/// O que o PDF é, visto sem senha.
#[derive(Debug)]
enum Estado {
    /// Cifrado: o documento carregado sem senha, com o `/Encrypt`, para conferir candidatas
    /// sem reler o arquivo.
    Cifrado(Box<Document>),
    /// Abre sem senha.
    SemSenha,
    /// O `lopdf` não lê, nem sem senha.
    Ilegivel { cifra_nao_suportada: bool },
}

/// Um PDF marcado como protegido, lido para a memória. O original não é tocado de novo,
/// a não ser pelo fallback do `qpdf`, que o lê pelo caminho.
#[derive(Debug)]
pub struct PdfProtegido {
    bytes: Vec<u8>,
    caminho: Option<PathBuf>,
    estado: Estado,
}

fn nao_suportada(erro: &lopdf::Error) -> bool {
    matches!(
        erro,
        lopdf::Error::Decryption(
            DecryptionError::UnsupportedEncryption
                | DecryptionError::UnsupportedVersion
                | DecryptionError::UnsupportedRevision
        ) | lopdf::Error::Unimplemented(_)
    )
}

fn senha_errada(erro: &lopdf::Error) -> bool {
    matches!(
        erro,
        lopdf::Error::InvalidPassword
            | lopdf::Error::Decryption(DecryptionError::IncorrectPassword)
    )
}

/// Roda o `lopdf` protegido contra pânico: um PDF hostil não derruba a janela.
fn protegido<T>(f: impl FnOnce() -> Result<T, lopdf::Error>) -> Option<Result<T, lopdf::Error>> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

/// Um `Write` que só conta os bytes, para reservar o buffer decifrado com a capacidade certa
/// (crescer o `Vec` deixaria cópias decifradas sem zerar para trás).
struct Contador(usize);

impl Write for Contador {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0 += buf.len();
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// O documento decifrado, gravado num buffer zerável.
fn serializar(documento: &mut Document) -> Result<Zeroizing<Vec<u8>>, lopdf::Error> {
    let mut contador = Contador(0);
    documento.save_to(&mut contador)?;
    let mut saida = Zeroizing::new(Vec::with_capacity(contador.0 + 4096));
    documento.save_to(&mut *saida)?;
    Ok(saida)
}

/// Só espaços (no sentido do JS).
fn vazio(texto: &[u8]) -> bool {
    String::from_utf8_lossy(texto).chars().all(espaco_do_js)
}

/// Texto, sem texto ou corrompido, pela regra do spike: reparado e sem nenhuma letra conta
/// como corrompido.
fn classificar(texto: String, reparado: bool, via: Via) -> Desfecho {
    let n = letras(&texto);
    if reparado && n == 0 {
        Desfecho::Corrompido
    } else if n < MINIMO_DE_LETRAS {
        Desfecho::SemTexto { via }
    } else {
        Desfecho::Aberto(TextoExtraido {
            texto,
            reparado,
            via,
        })
    }
}

/// `pdftotext -layout -l 3 - -` com o PDF (já decifrado) pela entrada padrão.
fn pdftotext(
    ferramentas: &Ferramentas,
    pdf: Zeroizing<Vec<u8>>,
) -> Result<String, FalhaDeFerramenta> {
    let saida = rodar_com_entrada(
        &ferramentas.pdftotext,
        &ARGUMENTOS_DO_PDFTOTEXT,
        pdf,
        ferramentas.limite_de_tempo,
    )?;
    if !saida.status.success() && vazio(&saida.stdout) {
        return Err(FalhaDeFerramenta::SemSaida);
    }
    Ok(decodificar_saida(&saida.stdout))
}

impl PdfProtegido {
    /// Lê o original para a memória (e guarda o caminho para o fallback do `qpdf`).
    ///
    /// # Errors
    /// Falha de leitura, ou [`std::io::ErrorKind::FileTooLarge`] acima de
    /// [`TAMANHO_MAXIMO_DO_PDF`] (o arquivo nem é lido).
    pub fn ler(caminho: &Path) -> std::io::Result<Self> {
        use std::io::Read as _;
        let grande_demais = || std::io::Error::from(std::io::ErrorKind::FileTooLarge);
        let arquivo = std::fs::File::open(caminho)?;
        if arquivo.metadata()?.len() > TAMANHO_MAXIMO_DO_PDF {
            return Err(grande_demais());
        }
        // O tamanho pode mudar entre o `metadata` e a leitura: o `take` é o teto de fato.
        let mut bytes = Vec::new();
        arquivo
            .take(TAMANHO_MAXIMO_DO_PDF + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > TAMANHO_MAXIMO_DO_PDF {
            return Err(grande_demais());
        }
        Ok(Self::novo(bytes, Some(caminho.to_path_buf())))
    }

    /// Um PDF já em memória (sem fallback do `qpdf`, que precisa do caminho).
    pub fn de_bytes(bytes: Vec<u8>) -> Self {
        Self::novo(bytes, None)
    }

    fn novo(bytes: Vec<u8>, caminho: Option<PathBuf>) -> Self {
        let estado = match protegido(|| {
            Document::load_mem_with_options(&bytes, LoadOptions::default())
        }) {
            Some(Ok(documento)) if documento.is_encrypted() => Estado::Cifrado(Box::new(documento)),
            Some(Ok(_)) => Estado::SemSenha,
            Some(Err(erro)) => Estado::Ilegivel {
                cifra_nao_suportada: nao_suportada(&erro),
            },
            None => Estado::Ilegivel {
                cifra_nao_suportada: false,
            },
        };
        Self {
            bytes,
            caminho,
            estado,
        }
    }

    /// Precisa de senha: está cifrado e a senha vazia não abre.
    pub fn precisa_de_senha(&self) -> bool {
        matches!(self.estado, Estado::Cifrado(_))
    }

    /// O PDF não é legível nem sem senha (lixo, cortado antes do `/Encrypt`).
    pub fn ilegivel(&self) -> bool {
        matches!(self.estado, Estado::Ilegivel { .. })
    }

    /// Abre o que não precisa de senha (o Bun marcou como protegido, mas a senha vazia abre).
    pub fn abrir_sem_senha(&self, ferramentas: &Ferramentas) -> Desfecho {
        match &self.estado {
            Estado::SemSenha => self.decifrar_e_extrair(None, ferramentas, Via::SemSenha),
            Estado::Ilegivel {
                cifra_nao_suportada: true,
            } => Desfecho::CifraNaoSuportada,
            Estado::Ilegivel { .. } => Desfecho::Corrompido,
            Estado::Cifrado(_) => Desfecho::SenhaErrada,
        }
    }

    /// Tenta a candidata. A senha só vive na memória deste processo (e, no fallback, na
    /// entrada padrão do `qpdf`).
    pub fn tentar(&self, candidata: &Candidata, ferramentas: &Ferramentas) -> Desfecho {
        match &self.estado {
            Estado::SemSenha => self.abrir_sem_senha(ferramentas),
            Estado::Ilegivel {
                cifra_nao_suportada,
            } => self.pelo_qpdf_ou(candidata, ferramentas, *cifra_nao_suportada),
            Estado::Cifrado(documento) => {
                match protegido(|| documento.authenticate_password(candidata.expor())) {
                    Some(Ok(())) => {
                        match self.decifrar_e_extrair(Some(candidata), ferramentas, Via::Lopdf) {
                            // O lopdf autenticou mas não decifrou, ou o pdftotext não leu a
                            // reserialização: o qpdf lê o original.
                            Desfecho::Corrompido | Desfecho::FalhaNaExtracao(_)
                                if self.tem_qpdf(ferramentas) =>
                            {
                                extrair_pelo_qpdf(self.caminho_do_qpdf(), candidata, ferramentas)
                            }
                            outro => outro,
                        }
                    }
                    Some(Err(erro)) if senha_errada(&erro) => Desfecho::SenhaErrada,
                    Some(Err(erro)) => {
                        self.pelo_qpdf_ou(candidata, ferramentas, nao_suportada(&erro))
                    }
                    None => self.pelo_qpdf_ou(candidata, ferramentas, false),
                }
            }
        }
    }

    fn tem_qpdf(&self, ferramentas: &Ferramentas) -> bool {
        ferramentas.qpdf.is_some() && self.caminho.is_some()
    }

    fn caminho_do_qpdf(&self) -> &Path {
        self.caminho.as_deref().unwrap_or(Path::new(""))
    }

    fn pelo_qpdf_ou(
        &self,
        candidata: &Candidata,
        ferramentas: &Ferramentas,
        nao_suportada: bool,
    ) -> Desfecho {
        if self.tem_qpdf(ferramentas) {
            extrair_pelo_qpdf(self.caminho_do_qpdf(), candidata, ferramentas)
        } else if nao_suportada {
            Desfecho::CifraNaoSuportada
        } else {
            Desfecho::Corrompido
        }
    }

    /// Carrega com a senha, grava o decifrado num buffer zerável e passa ao `pdftotext`.
    fn decifrar_e_extrair(
        &self,
        candidata: Option<&Candidata>,
        ferramentas: &Ferramentas,
        via: Via,
    ) -> Desfecho {
        let opcoes = match candidata {
            Some(c) => LoadOptions::with_password(c.expor()),
            None => LoadOptions::default(),
        };
        let decifrado = protegido(|| {
            let mut documento = Document::load_mem_with_options(&self.bytes, opcoes)?;
            if documento.is_encrypted() {
                return Err(lopdf::Error::InvalidPassword);
            }
            let reparado = documento.xref_recovery.is_some();
            Ok((serializar(&mut documento)?, reparado))
        });
        let (pdf, reparado) = match decifrado {
            Some(Ok(par)) => par,
            Some(Err(erro)) if senha_errada(&erro) => return Desfecho::SenhaErrada,
            Some(Err(erro)) if nao_suportada(&erro) => return Desfecho::CifraNaoSuportada,
            _ => return Desfecho::Corrompido,
        };
        match pdftotext(ferramentas, pdf) {
            Ok(texto) => classificar(texto, reparado, via),
            Err(falha) => Desfecho::FalhaNaExtracao(falha),
        }
    }
}

/// O fallback B2: `qpdf --password-file=- --decrypt <original> -` em pipe direto para
/// `pdftotext -layout -l 3 - -`. A senha entra só pela entrada padrão do `qpdf`; o argv tem
/// só o caminho do original. O Ubuntu confina o `qpdf` pelo AppArmor: ele só lê `*.pdf` fora
/// de pastas ocultas (ver o spike).
pub fn extrair_pelo_qpdf(
    original: &Path,
    candidata: &Candidata,
    ferramentas: &Ferramentas,
) -> Desfecho {
    let Some(qpdf) = ferramentas.qpdf.as_deref() else {
        return Desfecho::CifraNaoSuportada;
    };
    match pipeline_do_qpdf(qpdf, original, candidata, ferramentas) {
        Ok(desfecho) => desfecho,
        Err(falha) => Desfecho::FalhaNaExtracao(falha),
    }
}

fn pipeline_do_qpdf(
    qpdf: &Path,
    original: &Path,
    candidata: &Candidata,
    ferramentas: &Ferramentas,
) -> Result<Desfecho, FalhaDeFerramenta> {
    let prazo = Instant::now() + ferramentas.limite_de_tempo;
    // Só caminho absoluto: um relativo começando com `-` ou `@` o qpdf leria como opção ou
    // arquivo de argumentos (achado A14 da revisão de segurança da fatia 1).
    if !original.is_absolute() {
        return Err(FalhaDeFerramenta::NaoEncontrada);
    }
    let caminho = original.to_str().ok_or(FalhaDeFerramenta::NaoEncontrada)?;
    let mut decifrador =
        comando(qpdf, &["--password-file=-", "--decrypt", caminho, "-"])?.spawn()?;
    let pipe = decifrador
        .stdout
        .take()
        .map(Stdio::from)
        .unwrap_or_else(Stdio::null);
    let mut extrator = match comando(&ferramentas.pdftotext, &ARGUMENTOS_DO_PDFTOTEXT)
        .and_then(|mut c| c.stdin(pipe).spawn().map_err(FalhaDeFerramenta::from))
    {
        Ok(filho) => filho,
        Err(erro) => {
            let _ = decifrador.kill();
            let _ = decifrador.wait();
            return Err(erro);
        }
    };
    // A senha vai numa cópia zerável, pela entrada padrão do qpdf, e a entrada fecha (EOF).
    let mut senha = Zeroizing::new(Vec::with_capacity(candidata.expor().len()));
    senha.extend_from_slice(candidata.expor().as_bytes());
    let escrita = decifrador.stdin.take().map(|e| escrever_e_fechar(e, senha));
    let erro_do_qpdf = ler_com_limite(decifrador.stderr.take(), processo::ERRO_MAXIMO_DO_FILHO, 0);
    let texto = ler_com_limite(extrator.stdout.take(), processo::SAIDA_MAXIMA_DO_FILHO, 0);
    let erro_do_pdftotext =
        ler_com_limite(extrator.stderr.take(), processo::ERRO_MAXIMO_DO_FILHO, 0);

    let status_do_qpdf = esperar(&mut decifrador, Some(prazo)).map_err(FalhaDeFerramenta::from);
    let status_do_pdftotext = match status_do_qpdf {
        Ok(_) => esperar(&mut extrator, Some(prazo)).map_err(FalhaDeFerramenta::from),
        Err(_) => {
            let _ = extrator.kill();
            let _ = extrator.wait();
            Err(FalhaDeFerramenta::TempoEsgotado)
        }
    };
    let _ = escrita.map(|t| t.join());
    let erro_do_qpdf = erro_do_qpdf.join().unwrap_or_default().bytes;
    let texto = texto.join().unwrap_or_default().bytes;
    let _ = erro_do_pdftotext.join();
    let status_do_qpdf = status_do_qpdf?;

    // qpdf: 0 = certo, 3 = certo com avisos (reparou), 2 = erro.
    match status_do_qpdf.code() {
        Some(0) | Some(3) => {}
        Some(2) if String::from_utf8_lossy(&erro_do_qpdf).contains("invalid password") => {
            return Ok(Desfecho::SenhaErrada);
        }
        _ => return Ok(Desfecho::Corrompido),
    }
    let reparado = status_do_qpdf.code() == Some(3);
    let status_do_pdftotext = status_do_pdftotext?;
    if !status_do_pdftotext.success() && vazio(&texto) {
        return Err(processo::FalhaDeFerramenta::SemSaida);
    }
    Ok(classificar(decodificar_saida(&texto), reparado, Via::Qpdf))
}
