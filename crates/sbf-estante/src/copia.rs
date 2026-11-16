//! Cópia de segurança cifrada do cofre: o acervo inteiro e um instantâneo do
//! `inventario.sqlite`, num remoto **rclone crypt** (`SBF/Copia-de-seguranca/`).
//!
//! # A senha
//!
//! Vem do chaveiro como [`Segredo`] e chega ao rclone **sem argumento de linha de comando**:
//! 1. `rclone obscure -` recebe a senha pela entrada padrão;
//! 2. o valor ofuscado vai só no ambiente do processo filho, em
//!    `RCLONE_CONFIG_SBFCOPIA_PASSWORD`, junto de `..._TYPE=crypt` e `..._REMOTE=<destino>`
//!    (a v1.60 aceita remoto definido por variáveis de ambiente; nenhum arquivo de
//!    configuração é escrito).
//!
//! O ambiente de um processo só é legível pelo mesmo usuário (`/proc/<pid>/environ`), ao
//! contrário do `cmdline`, que qualquer um lê. O valor ofuscado equivale à senha para quem o
//! ler, então vale o mesmo cuidado que ela.
//!
//! # A senha forte e o sal (achado A2 da revisão de segurança da fatia 1)
//!
//! - [`gerar_senha_da_copia`] gera a senha recomendada: 30 símbolos de um alfabeto de 32
//!   (150 bits), em grupos de 5, sem `l`, `o`, `0` e `1`, para o chaveiro guardar e o
//!   usuário anotar fora do computador ([`AVISO_DA_SENHA`]).
//! - Uma senha escolhida pelo usuário precisa de [`MINIMO_DA_SENHA_ESCOLHIDA`] caracteres.
//! - O rclone crypt deriva a chave por scrypt com a senha e o `PASSWORD2` (o sal). Sem
//!   `PASSWORD2`, ele usa um sal fixo, igual para todo usuário do rclone, e uma tabela
//!   pré-calculada serve para todos. Aqui cada destino tem um sal próprio: 32 bytes
//!   aleatórios em hexadecimal, gravados **em claro** em [`NOME_DO_SAL`], ao lado da pasta
//!   cifrada, na primeira cópia, e passados ofuscados em `RCLONE_CONFIG_SBFCOPIA_PASSWORD2`.
//!   O sal não é segredo (só impede a pré-computação); guardá-lo junto da cópia faz a
//!   restauração precisar só da senha, como o aviso promete. Um destino com arquivos e sem
//!   sal é recusado ([`Erro::DestinoSemSal`]) em vez de misturado.
//!
//! # O banco
//!
//! O instantâneo sai da API de backup do SQLite (`bancada_armazem::copiar_banco`): consistente
//! com escritores ativos e sem `-wal`/`-shm`. Fica numa pasta temporária 0700 dentro do cofre
//! (`tmp/`), é conferido (`integrity_check`), enviado e apagado. O nome carrega o hash:
//! `banco/inventario-<AAAA-MM-DDTHHMMSSZ>-<sha256 16>.sqlite`, e a restauração confere.

use crate::erro::{Erro, Resultado};
use crate::rclone::{Rclone, VariavelDeAmbiente, juntar, validar_destino};
use bancada_armazem::{copiar_banco, verificar_integridade_de_arquivo};
use bancada_segredos::Segredo;
use rusqlite::Connection;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use zeroize::Zeroizing;

/// Texto para a tela (Ferramentas > Fazer cópia de segurança, e Opções).
pub const AVISO_DA_SENHA: &str = "Sem a senha da cópia de segurança, a cópia é ilegível: \
nem você nem ninguém consegue abrir os arquivos. Guarde essa senha também fora deste \
computador (num gerenciador de senhas ou num papel em lugar seguro). Se o computador e a \
senha se perderem juntos, a cópia se perde com eles.";

/// Idade a partir da qual uma pasta `<cofre>/tmp/copia-*` é sobra (achado A8 da revisão de
/// segurança da fatia 1). Mais nova que isto pode ser de uma cópia rodando em outra janela.
pub const IDADE_DA_SOBRA: Duration = Duration::from_secs(24 * 60 * 60);
const PREFIXO_DA_PASTA_TEMPORARIA: &str = "copia-";

/// Apaga as pastas `<cofre>/tmp/copia-*` mais velhas que `idade_minima`: o instantâneo em
/// claro que uma cópia interrompida (SIGKILL, queda de energia) deixou. [`CopiaDeSeguranca::fazer`]
/// chama com [`IDADE_DA_SOBRA`]; a abertura do app também deve chamar. Não segue link
/// simbólico e não mexe no que não é da cópia. Devolve quantas apagou.
///
/// # Errors
/// Falha de disco ao listar ou apagar.
pub fn faxinar_sobras(cofre: &Path, idade_minima: Duration) -> Resultado<usize> {
    let tmp = cofre.join("tmp");
    let entradas = match std::fs::read_dir(&tmp) {
        Ok(entradas) => entradas,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(e.into()),
    };
    let agora = SystemTime::now();
    let mut apagadas = 0;
    for entrada in entradas {
        let entrada = entrada?;
        let e_da_copia = entrada
            .file_name()
            .to_str()
            .is_some_and(|n| n.starts_with(PREFIXO_DA_PASTA_TEMPORARIA));
        // `file_type` da entrada não segue link: um link `copia-x` não é apagado.
        if !e_da_copia || !entrada.file_type()?.is_dir() {
            continue;
        }
        let idade = entrada
            .metadata()?
            .modified()
            .ok()
            .and_then(|m| agora.duration_since(m).ok())
            .unwrap_or(Duration::ZERO);
        if idade >= idade_minima {
            std::fs::remove_dir_all(entrada.path())?;
            apagadas += 1;
        }
    }
    Ok(apagadas)
}

/// O mínimo, em caracteres, de uma senha escolhida pelo usuário. A gerada tem 35.
pub const MINIMO_DA_SENHA_ESCOLHIDA: usize = 16;

/// O arquivo do sal, em claro, na raiz do destino (fora da pasta cifrada).
pub const NOME_DO_SAL: &str = "sal-da-copia.txt";
/// Bytes aleatórios do sal (64 hexadecimais).
const BYTES_DO_SAL: usize = 32;
/// Alfabeto da senha gerada: minúsculas sem `l` e `o`, dígitos sem `0` e `1`. São 32
/// símbolos, então `byte % 32` é uniforme.
const ALFABETO_DA_SENHA: &[u8; 32] = b"abcdefghijkmnpqrstuvwxyz23456789";
const GRUPOS_DA_SENHA: usize = 6;
const SIMBOLOS_POR_GRUPO: usize = 5;

/// Gera a senha da cópia de segurança: `xxxxx-xxxxx-xxxxx-xxxxx-xxxxx-xxxxx`, 150 bits.
/// Quem chama guarda no chaveiro e mostra uma vez, com [`AVISO_DA_SENHA`].
///
/// # Errors
/// [`Erro::Aleatorio`] se o sistema não entregar bytes aleatórios.
pub fn gerar_senha_da_copia() -> Resultado<Segredo> {
    let simbolos = GRUPOS_DA_SENHA * SIMBOLOS_POR_GRUPO;
    let mut aleatorio = Zeroizing::new([0u8; GRUPOS_DA_SENHA * SIMBOLOS_POR_GRUPO]);
    getrandom::fill(aleatorio.as_mut_slice()).map_err(|_| Erro::Aleatorio)?;
    let mut senha = String::with_capacity(simbolos + GRUPOS_DA_SENHA - 1);
    for (i, byte) in aleatorio.iter().enumerate() {
        if i > 0 && i % SIMBOLOS_POR_GRUPO == 0 {
            senha.push('-');
        }
        senha.push(char::from(ALFABETO_DA_SENHA[usize::from(*byte) % 32]));
    }
    Ok(Segredo::novo(senha))
}

/// Recusa senha vazia ou curta, antes de qualquer processo.
fn conferir_senha(senha: &Segredo) -> Resultado<()> {
    if senha.esta_vazio() {
        return Err(Erro::SenhaVazia);
    }
    if senha.expor().chars().count() < MINIMO_DA_SENHA_ESCOLHIDA {
        return Err(Erro::SenhaCurta {
            minimo: MINIMO_DA_SENHA_ESCOLHIDA,
        });
    }
    Ok(())
}

/// Um sal novo: 32 bytes aleatórios em hexadecimal.
fn sal_novo() -> Resultado<Zeroizing<String>> {
    let mut bytes = [0u8; BYTES_DO_SAL];
    getrandom::fill(&mut bytes).map_err(|_| Erro::Aleatorio)?;
    Ok(Zeroizing::new(
        bytes.iter().map(|b| format!("{b:02x}")).collect(),
    ))
}

fn sal_no_formato(sal: &str) -> bool {
    sal.len() == BYTES_DO_SAL * 2 && sal.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Nome do remoto crypt montado por variáveis de ambiente.
const REMOTO: &str = "SBFCOPIA";
const PASTA_ACERVO: &str = "acervo";
const PASTA_BANCO: &str = "banco";
const PREFIXO_DO_BANCO: &str = "inventario-";
/// Quantos caracteres do SHA-256 vão no nome do instantâneo.
const HASH_NO_NOME: usize = 16;

/// Resultado de uma cópia de segurança.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioDaCopia {
    /// Instante UTC da cópia (`AAAA-MM-DDTHHMMSSZ`), para "última cópia" na barra de status.
    pub quando: String,
    /// Nome do instantâneo do banco no remoto (dentro de `banco/`).
    pub banco: String,
    /// SHA-256 completo do instantâneo do banco.
    pub banco_sha256: String,
    /// Arquivos que o acervo local tinha na hora.
    pub arquivos_do_acervo: usize,
    /// Bytes do acervo local.
    pub bytes_do_acervo: u64,
}

/// Resultado de uma restauração.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelatorioDaRestauracao {
    /// Instantâneo do banco usado (o mais recente).
    pub banco: String,
    /// SHA-256 do banco restaurado (confere com o do nome).
    pub banco_sha256: String,
    /// Arquivos do acervo restaurado.
    pub arquivos_do_acervo: usize,
}

/// A cópia de segurança num destino rclone (`gdrive:SBF/Copia-de-seguranca`, ou uma pasta
/// local em teste). O destino é a pasta **por baixo** da cripto: nomes e conteúdo ali são
/// cifrados.
#[derive(Debug, Clone)]
pub struct CopiaDeSeguranca {
    rclone: Rclone,
    destino: String,
}

impl CopiaDeSeguranca {
    /// # Errors
    /// [`Erro::DestinoInvalido`] se vazio ou começando com `-`.
    pub fn nova(rclone: Rclone, destino: impl Into<String>) -> Resultado<Self> {
        let destino = destino.into();
        validar_destino(&destino)?;
        Ok(Self { rclone, destino })
    }

    /// O sal guardado no destino, se há. Lê no máximo um pouco além do tamanho certo.
    fn sal_guardado(&self) -> Resultado<Option<Zeroizing<String>>> {
        match self.rclone.executar(
            [
                "cat",
                "--head",
                "200",
                juntar(&self.destino, NOME_DO_SAL).as_str(),
            ],
            &[],
            None,
        ) {
            Ok(texto) => {
                let sal = texto.trim();
                if sal_no_formato(sal) {
                    Ok(Some(Zeroizing::new(sal.to_owned())))
                } else {
                    Err(Erro::SalInvalido)
                }
            }
            Err(Erro::Rclone {
                codigo: Some(3 | 4),
                ..
            }) => Ok(None),
            Err(outro) => Err(outro),
        }
    }

    /// O sal para fazer uma cópia: o guardado, ou um novo se o destino está vazio.
    fn sal_para_fazer(&self) -> Resultado<Zeroizing<String>> {
        if let Some(sal) = self.sal_guardado()? {
            return Ok(sal);
        }
        let ocupado = match self.rclone.executar(
            ["lsf", "--max-depth", "1", self.destino.as_str()],
            &[],
            None,
        ) {
            Ok(lista) => lista.lines().any(|l| !l.trim().is_empty()),
            Err(Erro::Rclone {
                codigo: Some(3), ..
            }) => false,
            Err(outro) => return Err(outro),
        };
        if ocupado {
            return Err(Erro::DestinoSemSal);
        }
        let sal = sal_novo()?;
        self.rclone.executar(
            ["rcat", juntar(&self.destino, NOME_DO_SAL).as_str()],
            &[],
            Some(sal.as_str()),
        )?;
        Ok(sal)
    }

    fn ambiente(&self, senha: &Segredo, sal: &str) -> Resultado<Vec<VariavelDeAmbiente>> {
        conferir_senha(senha)?;
        let ofuscada = self.rclone.ofuscar(senha.expor())?;
        let sal_ofuscado = self.rclone.ofuscar(sal)?;
        let var = |nome: &str, valor: &str| -> VariavelDeAmbiente {
            (
                format!("RCLONE_CONFIG_{REMOTO}_{nome}"),
                Zeroizing::new(valor.to_owned()),
            )
        };
        Ok(vec![
            var("TYPE", "crypt"),
            var("REMOTE", &self.destino),
            var("FILENAME_ENCRYPTION", "standard"),
            var("DIRECTORY_NAME_ENCRYPTION", "true"),
            var("PASSWORD", &ofuscada),
            var("PASSWORD2", &sal_ofuscado),
        ])
    }

    /// O ambiente de uma cópia que já existe; `None` se o destino não tem sal (sem cópia).
    fn ambiente_existente(&self, senha: &Segredo) -> Resultado<Option<Vec<VariavelDeAmbiente>>> {
        conferir_senha(senha)?;
        match self.sal_guardado()? {
            Some(sal) => Ok(Some(self.ambiente(senha, &sal)?)),
            None => Ok(None),
        }
    }

    fn cripto(caminho: &str) -> String {
        format!("{REMOTO}:{caminho}")
    }

    /// Faz a cópia: acervo (só acrescenta, nunca apaga no destino) e instantâneo do banco.
    ///
    /// # Errors
    /// Senha vazia ou curta, destino ocupado sem sal, sal adulterado, falha do banco, do
    /// disco ou do rclone.
    pub fn fazer(
        &self,
        conexao: &Connection,
        cofre: &Path,
        senha: &Segredo,
    ) -> Resultado<RelatorioDaCopia> {
        conferir_senha(senha)?;
        let sal = self.sal_para_fazer()?;
        let ambiente = self.ambiente(senha, &sal)?;
        let quando = instante_utc(SystemTime::now());

        // 1. Instantâneo do banco, numa pasta temporária 0700 do cofre.
        let tmp = cofre.join("tmp");
        std::fs::create_dir_all(&tmp)?;
        // A faxina é melhor-esforço: uma sobra que não sai não impede a cópia.
        let _ = faxinar_sobras(cofre, IDADE_DA_SOBRA);
        let pasta = tempfile::Builder::new()
            .prefix(PREFIXO_DA_PASTA_TEMPORARIA)
            .tempdir_in(&tmp)?;
        let instantaneo = pasta.path().join("inventario.sqlite");
        copiar_banco(conexao, &instantaneo)?;
        verificar_integridade_de_arquivo(&instantaneo)?;
        let sha = sha256_do_arquivo(&instantaneo)?;
        let nome_do_banco = format!("{PREFIXO_DO_BANCO}{quando}-{}.sqlite", &sha[..HASH_NO_NOME]);

        // 2. Acervo: `copy` só acrescenta.
        let acervo = cofre.join(PASTA_ACERVO);
        let (arquivos, bytes) = if acervo.is_dir() {
            self.rclone.executar(
                [
                    std::ffi::OsString::from("copy"),
                    acervo.clone().into_os_string(),
                    Self::cripto(PASTA_ACERVO).into(),
                    "--transfers".into(),
                    "4".into(),
                    "--checkers".into(),
                    "8".into(),
                ],
                &ambiente,
                None,
            )?;
            contar(&acervo)?
        } else {
            (0, 0)
        };

        // 3. O banco por último: o instantâneo só aparece depois do acervo que ele descreve.
        self.rclone.executar(
            [
                std::ffi::OsString::from("copyto"),
                instantaneo.into_os_string(),
                Self::cripto(&format!("{PASTA_BANCO}/{nome_do_banco}")).into(),
            ],
            &ambiente,
            None,
        )?;
        Ok(RelatorioDaCopia {
            quando,
            banco: nome_do_banco,
            banco_sha256: sha,
            arquivos_do_acervo: arquivos,
            bytes_do_acervo: bytes,
        })
    }

    /// Nomes dos instantâneos do banco, do mais antigo ao mais novo. Senha errada ou cópia
    /// vazia dão lista vazia.
    ///
    /// # Errors
    /// Senha vazia ou falha do rclone.
    pub fn instantaneos(&self, senha: &Segredo) -> Resultado<Vec<String>> {
        match self.ambiente_existente(senha)? {
            Some(ambiente) => self.listar_bancos(&ambiente),
            None => Ok(Vec::new()),
        }
    }

    fn listar_bancos(&self, ambiente: &[VariavelDeAmbiente]) -> Resultado<Vec<String>> {
        let saida = match self.rclone.executar(
            ["lsf", "--files-only", Self::cripto(PASTA_BANCO).as_str()],
            ambiente,
            None,
        ) {
            Ok(saida) => saida,
            Err(Erro::Rclone {
                codigo: Some(3), ..
            }) => return Ok(Vec::new()),
            Err(outro) => return Err(outro),
        };
        let mut nomes: Vec<String> = saida
            .lines()
            .filter(|n| n.starts_with(PREFIXO_DO_BANCO) && n.ends_with(".sqlite"))
            .map(str::to_owned)
            .collect();
        nomes.sort();
        Ok(nomes)
    }

    /// Restaura o acervo e o banco mais recente em `para` (`para/acervo/`,
    /// `para/inventario.sqlite`). Nunca sobrescreve: se `para/inventario.sqlite` existe, recusa;
    /// um arquivo do acervo que já esteja em `para/acervo/` fica como está (`--ignore-existing`).
    ///
    /// # Errors
    /// [`Erro::JaExiste`], [`Erro::SemInstantaneo`] (senha errada ou cópia vazia),
    /// [`Erro::HashDiferente`] ou falha do rclone/disco.
    pub fn restaurar(&self, senha: &Segredo, para: &Path) -> Resultado<RelatorioDaRestauracao> {
        let ambiente = self
            .ambiente_existente(senha)?
            .ok_or(Erro::SemInstantaneo)?;
        let banco_destino = para.join("inventario.sqlite");
        if banco_destino.exists() {
            return Err(Erro::JaExiste(banco_destino));
        }
        let nome = self
            .listar_bancos(&ambiente)?
            .pop()
            .ok_or(Erro::SemInstantaneo)?;
        std::fs::create_dir_all(para)?;
        let acervo = para.join(PASTA_ACERVO);
        self.rclone.executar(
            [
                std::ffi::OsString::from("copy"),
                Self::cripto(PASTA_ACERVO).into(),
                acervo.clone().into_os_string(),
                "--transfers".into(),
                "4".into(),
                // Nunca sobrescreve um arquivo que já esteja no destino (achado A9).
                "--ignore-existing".into(),
            ],
            &ambiente,
            None,
        )?;
        self.rclone.executar(
            [
                std::ffi::OsString::from("copyto"),
                Self::cripto(&format!("{PASTA_BANCO}/{nome}")).into(),
                banco_destino.clone().into_os_string(),
            ],
            &ambiente,
            None,
        )?;
        let sha = sha256_do_arquivo(&banco_destino)?;
        let esperado = hash_do_nome(&nome).unwrap_or_default();
        if !sha.starts_with(&esperado) || esperado.is_empty() {
            let _ = std::fs::remove_file(&banco_destino);
            return Err(Erro::HashDiferente {
                esperado,
                obtido: sha[..HASH_NO_NOME].to_owned(),
            });
        }
        verificar_integridade_de_arquivo(&banco_destino)?;
        let (arquivos, _) = if acervo.is_dir() {
            contar(&acervo)?
        } else {
            (0, 0)
        };
        Ok(RelatorioDaRestauracao {
            banco: nome,
            banco_sha256: sha,
            arquivos_do_acervo: arquivos,
        })
    }

    /// Destino sem a cripto, útil para mostrar em Opções.
    #[must_use]
    pub fn destino(&self) -> &str {
        &self.destino
    }
}

/// O prefixo de hash que está no nome do instantâneo.
fn hash_do_nome(nome: &str) -> Option<String> {
    let miolo = nome.strip_suffix(".sqlite")?;
    let hash = miolo.rsplit('-').next()?;
    (hash.len() == HASH_NO_NOME && hash.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hash.to_owned())
}

/// SHA-256 (hex minúsculo) de um arquivo, lido em blocos.
///
/// # Errors
/// Falha de leitura.
pub fn sha256_do_arquivo(caminho: &Path) -> Resultado<String> {
    let mut arquivo = std::fs::File::open(caminho)?;
    let mut resumo = Sha256::new();
    let mut bloco = vec![0u8; 1 << 16];
    loop {
        let n = arquivo.read(&mut bloco)?;
        if n == 0 {
            break;
        }
        resumo.update(&bloco[..n]);
    }
    Ok(resumo
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn contar(pasta: &Path) -> Resultado<(usize, u64)> {
    let mut pilha: Vec<PathBuf> = vec![pasta.to_path_buf()];
    let (mut arquivos, mut bytes) = (0usize, 0u64);
    while let Some(atual) = pilha.pop() {
        for entrada in std::fs::read_dir(&atual)? {
            let entrada = entrada?;
            let tipo = entrada.file_type()?;
            if tipo.is_dir() {
                pilha.push(entrada.path());
            } else if tipo.is_file() {
                arquivos += 1;
                bytes += entrada.metadata()?.len();
            }
        }
    }
    Ok((arquivos, bytes))
}

/// `AAAA-MM-DDTHHMMSSZ` em UTC (sem relógio externo).
#[must_use]
pub fn instante_utc(instante: SystemTime) -> String {
    let segundos = instante
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let dias = i64::try_from(segundos / 86_400).unwrap_or(0);
    let resto = segundos % 86_400;
    // Dias desde 1970-01-01 para data civil (algoritmo de Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let dia = doy - (153 * mp + 2) / 5 + 1;
    let mes = if mp < 10 { mp + 3 } else { mp - 9 };
    let ano = yoe + era * 400 + i64::from(mes <= 2);
    format!(
        "{ano:04}-{mes:02}-{dia:02}T{:02}{:02}{:02}Z",
        resto / 3600,
        resto % 3600 / 60,
        resto % 60
    )
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::time::Duration;

    #[test]
    fn instante_utc_em_datas_conhecidas() {
        assert_eq!(instante_utc(UNIX_EPOCH), "1970-01-01T000000Z");
        // 2026-11-13 21:30:05 UTC
        assert_eq!(
            instante_utc(UNIX_EPOCH + Duration::from_secs(1_794_605_405)),
            "2026-11-13T213005Z"
        );
        // 29 de fevereiro de 2024, 23:59:59
        assert_eq!(
            instante_utc(UNIX_EPOCH + Duration::from_secs(1_709_251_199)),
            "2024-02-29T235959Z"
        );
    }

    #[test]
    fn hash_do_nome_do_instantaneo() {
        assert_eq!(
            hash_do_nome("inventario-2026-11-13T213005Z-0123456789abcdef.sqlite").as_deref(),
            Some("0123456789abcdef")
        );
        assert_eq!(hash_do_nome("inventario.sqlite"), None);
    }

    #[test]
    fn aviso_da_senha_menciona_o_essencial() {
        assert!(AVISO_DA_SENHA.contains("ilegível"));
        assert!(AVISO_DA_SENHA.contains("fora deste computador"));
    }
}
