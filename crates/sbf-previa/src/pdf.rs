//! PDF: contar as páginas, decifrar o protegido em memória e desenhar uma página.
//!
//! - `pdfinfo -` conta as páginas com o PDF pela entrada padrão. Sem `pdfinfo`, ou quando ele
//!   não lê, o `lopdf` conta.
//! - Protegido: o `lopdf` confere cada [`Candidata`] e decifra em memória (como o `sbf-pdf`),
//!   num buffer zerável com a capacidade certa. Fallback: `qpdf --password-file=- --decrypt
//!   <original> -`, com a senha pela entrada padrão e o PDF decifrado pela saída padrão.
//! - `pdftoppm -f N -l N -singlefile -scale-to-x W -scale-to-y -1 -` desenha a página em PPM
//!   na saída padrão; nenhum arquivo é criado.

use crate::{Candidata, Conteudo, ErroDaPrevia, Imagem, Paginas};
use lopdf::encryption::DecryptionError;
use lopdf::{Document, LoadOptions};
use sbf_processo::{Chamada, localizar_ou_nome};
use std::io::Write;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::time::Duration;
use zeroize::Zeroizing;

/// O prazo de cada chamada ao poppler ou ao qpdf.
const PRAZO: Duration = Duration::from_secs(30);
/// O teto da saída do `pdftoppm` (PPM sem compressão: 4096 x 16000 x 3 cabe).
const SAIDA_MAXIMA_DA_PAGINA: usize = 192 * 1024 * 1024;

/// Roda o `lopdf` protegido contra pânico: um PDF hostil não derruba a janela.
fn protegido<T>(f: impl FnOnce() -> Result<T, lopdf::Error>) -> Option<Result<T, lopdf::Error>> {
    catch_unwind(AssertUnwindSafe(f)).ok()
}

fn senha_errada(erro: &lopdf::Error) -> bool {
    matches!(
        erro,
        lopdf::Error::InvalidPassword
            | lopdf::Error::Decryption(DecryptionError::IncorrectPassword)
    )
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

/// O que o `pdfinfo` disse.
enum Info {
    Paginas(usize),
    /// Pede senha.
    Cifrado,
    /// Não leu (ou não há `pdfinfo`).
    NaoLeu,
}

fn pdfinfo(bytes: &Zeroizing<Vec<u8>>) -> Info {
    let saida = Chamada::nova(localizar_ou_nome("pdfinfo"))
        .argumentos(["-"])
        .com_entrada(bytes.clone())
        .com_prazo(PRAZO)
        .com_saida_maxima(256 * 1024)
        .rodar();
    let Ok(saida) = saida else {
        return Info::NaoLeu;
    };
    if !saida.status.success() {
        return if String::from_utf8_lossy(&saida.stderr).contains("Incorrect password") {
            Info::Cifrado
        } else {
            Info::NaoLeu
        };
    }
    String::from_utf8_lossy(&saida.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("Pages:"))
        .and_then(|n| n.trim().parse().ok())
        .map_or(Info::NaoLeu, Info::Paginas)
}

/// Abre um PDF já lido para a memória.
pub(crate) fn abrir(
    bytes: Zeroizing<Vec<u8>>,
    caminho: &Path,
    candidatas: &[Candidata],
) -> Result<Conteudo, ErroDaPrevia> {
    match pdfinfo(&bytes) {
        Info::Paginas(0) => Err(ErroDaPrevia::Ilegivel("PDF sem páginas".to_owned())),
        Info::Paginas(n) => Ok(Conteudo::Paginas(Paginas::novas(n, bytes))),
        Info::Cifrado | Info::NaoLeu => pelo_lopdf(bytes, caminho, candidatas),
    }
}

/// Abre pelo `lopdf`: conta as páginas do que abre sem senha e decifra o protegido.
fn pelo_lopdf(
    bytes: Zeroizing<Vec<u8>>,
    caminho: &Path,
    candidatas: &[Candidata],
) -> Result<Conteudo, ErroDaPrevia> {
    match protegido(|| Document::load_mem_with_options(&bytes, LoadOptions::default())) {
        Some(Ok(documento)) if !documento.is_encrypted() => {
            let total = documento.get_pages().len();
            if total == 0 {
                return Err(ErroDaPrevia::Ilegivel("PDF sem páginas".to_owned()));
            }
            Ok(Conteudo::Paginas(Paginas::novas(total, bytes)))
        }
        Some(Ok(documento)) => decifrar(&documento, &bytes, caminho, candidatas),
        // Nem o poppler nem o lopdf leem; o qpdf ainda pode decifrar uma cifra que o lopdf
        // não implementa.
        _ => {
            for candidata in candidatas {
                if let Some(conteudo) = pelo_qpdf(caminho, candidata)? {
                    return Ok(conteudo);
                }
            }
            Err(ErroDaPrevia::Ilegivel(
                "o PDF não abre (nem o poppler nem o lopdf leem)".to_owned(),
            ))
        }
    }
}

fn motivo_do_protegido(candidatas: &[Candidata]) -> String {
    if candidatas.is_empty() {
        "Protegido por senha: nenhuma regra de senha para tentar (ou o chaveiro está trancado)"
            .to_owned()
    } else {
        format!(
            "Protegido por senha: nenhuma das {} senhas candidatas abriu",
            candidatas.len()
        )
    }
}

/// Confere as candidatas em ordem e decifra com a primeira que abre.
fn decifrar(
    documento: &Document,
    bytes: &[u8],
    caminho: &Path,
    candidatas: &[Candidata],
) -> Result<Conteudo, ErroDaPrevia> {
    for candidata in candidatas {
        match protegido(|| documento.authenticate_password(candidata.expor())) {
            Some(Ok(())) => {
                let decifrado = protegido(|| {
                    let opcoes = LoadOptions::with_password(candidata.expor());
                    let mut d = Document::load_mem_with_options(bytes, opcoes)?;
                    if d.is_encrypted() {
                        return Err(lopdf::Error::InvalidPassword);
                    }
                    let total = d.get_pages().len();
                    Ok((serializar(&mut d)?, total))
                });
                match decifrado {
                    Some(Ok((pdf, total))) if total > 0 => {
                        return Ok(Conteudo::Paginas(Paginas::novas(total, pdf)));
                    }
                    // A senha confere, mas o lopdf não decifrou: o qpdf lê o original.
                    _ => {
                        if let Some(conteudo) = pelo_qpdf(caminho, candidata)? {
                            return Ok(conteudo);
                        }
                    }
                }
            }
            Some(Err(erro)) if senha_errada(&erro) => {}
            // Cifra que o lopdf não implementa (ou pânico): o qpdf tenta.
            _ => {
                if let Some(conteudo) = pelo_qpdf(caminho, candidata)? {
                    return Ok(conteudo);
                }
            }
        }
    }
    Ok(Conteudo::Protegido {
        motivo: motivo_do_protegido(candidatas),
    })
}

/// `qpdf --password-file=- --decrypt <original> -`: a senha pela entrada padrão, o PDF
/// decifrado pela saída padrão num buffer zerável. `None`: senha errada ou o qpdf não leu.
/// Só caminho absoluto (um relativo começando com `-` ou `@` viraria opção). O Ubuntu
/// confina o qpdf pelo AppArmor: ele só lê `*.pdf` fora de pastas ocultas.
fn pelo_qpdf(caminho: &Path, candidata: &Candidata) -> Result<Option<Conteudo>, ErroDaPrevia> {
    if !caminho.is_absolute() {
        return Ok(None);
    }
    let mut senha = Zeroizing::new(Vec::with_capacity(candidata.expor().len()));
    senha.extend_from_slice(candidata.expor().as_bytes());
    let tamanho = std::fs::metadata(caminho).map_or(0, |m| m.len());
    let capacidade = usize::try_from(tamanho)
        .unwrap_or(0)
        .saturating_mul(2)
        .saturating_add(64 * 1024);
    let saida = Chamada::nova(localizar_ou_nome("qpdf"))
        .argumentos([
            "--password-file=-".as_ref(),
            "--decrypt".as_ref(),
            caminho.as_os_str(),
            "-".as_ref(),
        ])
        .com_entrada(senha)
        .com_prazo(PRAZO)
        .com_saida_maxima(4 * crate::TAMANHO_MAXIMO as usize)
        .com_capacidade_da_saida(capacidade)
        .rodar();
    let saida = match saida {
        Ok(s) => s,
        // Sem qpdf é só um fallback a menos.
        Err(sbf_processo::FalhaDeProcesso::NaoEncontrado) => return Ok(None),
        Err(falha) => return Err(ErroDaPrevia::de_processo("qpdf", falha)),
    };
    // qpdf: 0 = certo, 3 = certo com avisos, 2 = erro (senha errada inclusive).
    if !matches!(saida.status.code(), Some(0 | 3)) || saida.stdout_cortado {
        return Ok(None);
    }
    let pdf = saida.stdout;
    let total = match pdfinfo(&pdf) {
        Info::Paginas(n) => n,
        _ => match protegido(|| Document::load_mem(&pdf)) {
            Some(Ok(d)) => d.get_pages().len(),
            _ => 0,
        },
    };
    Ok((total > 0).then(|| Conteudo::Paginas(Paginas::novas(total, pdf))))
}

/// Desenha a página `pagina` (já conferida) com `largura` px.
pub(crate) fn desenhar(
    pdf: &Zeroizing<Vec<u8>>,
    pagina: usize,
    largura: u32,
) -> Result<Imagem, ErroDaPrevia> {
    let pagina = pagina.to_string();
    let largura = largura.to_string();
    let saida = Chamada::nova(localizar_ou_nome("pdftoppm"))
        .argumentos([
            "-f",
            &pagina,
            "-l",
            &pagina,
            "-singlefile",
            "-scale-to-x",
            &largura,
            "-scale-to-y",
            "-1",
            "-",
        ])
        .com_entrada(pdf.clone())
        .com_prazo(PRAZO)
        .com_saida_maxima(SAIDA_MAXIMA_DA_PAGINA)
        .rodar()
        .map_err(|f| ErroDaPrevia::de_processo("pdftoppm", f))?;
    if saida.stdout_cortado {
        return Err(ErroDaPrevia::GrandeDemais);
    }
    if !saida.status.success() || saida.stdout.is_empty() {
        let erro = String::from_utf8_lossy(&saida.stderr);
        let ultima = erro.lines().last().unwrap_or("sem saída").trim();
        return Err(ErroDaPrevia::Ilegivel(format!("pdftoppm: {ultima}")));
    }
    crate::imagem::de_ppm(&saida.stdout)
}
