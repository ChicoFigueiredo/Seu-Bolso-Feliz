//! dm-crypt/LUKS no Linux.
//!
//! 1. `/proc/self/mountinfo` diz qual montagem contém o caminho (a de ponto mais longo) e o
//!    dispositivo dela (`major:minor` e a fonte, como `/dev/mapper/cofre`).
//! 2. Acha o nome do bloco em `/sys/dev/block/<major>:<minor>` (ou pela fonte, que no btrfs
//!    tem `major` 0).
//! 3. Sobe pelos ancestrais (`/sys/class/block/<nome>/slaves/`): se algum tem
//!    `dm/uuid` começando por `CRYPT-`, o volume é cifrado (LUKS aparece assim; o LVM
//!    sobre LUKS e o LUKS sobre partição também, porque a cadeia passa por ele).
//!
//! Sem dispositivo de bloco (tmpfs, overlay, rede, fuse, 9p, drvfs do WSL): `Desconhecido`.
//! O código lê o sistema por [`Arvore`], para os testes usarem árvores de exemplo.

use crate::Cifra;
use std::collections::HashSet;
use std::path::Path;

/// Uma linha útil do `mountinfo`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Montagem {
    /// Ponto de montagem, já sem os escapes octais (`\040`).
    pub ponto: String,
    /// `major:minor` do dispositivo.
    pub maj_min: String,
    /// Tipo do sistema de arquivos.
    pub tipo: String,
    /// Fonte (`/dev/nvme0n1p2`, `/dev/mapper/x`, `tmpfs`, `server:/pasta`...).
    pub fonte: String,
}

fn desescapar(campo: &str) -> String {
    let bytes = campo.as_bytes();
    let mut saida = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\'
            && i + 3 < bytes.len()
            && let Some(valor) = campo
                .get(i + 1..i + 4)
                .and_then(|o| u8::from_str_radix(o, 8).ok())
        {
            saida.push(valor);
            i += 4;
            continue;
        }
        saida.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&saida).into_owned()
}

/// Lê o conteúdo de `/proc/self/mountinfo`. Linhas que não entendemos são ignoradas.
#[must_use]
pub fn ler_mountinfo(texto: &str) -> Vec<Montagem> {
    texto
        .lines()
        .filter_map(|linha| {
            let campos: Vec<&str> = linha.split(' ').collect();
            let separador = campos.iter().position(|c| *c == "-")?;
            if campos.len() < 6 || campos.len() < separador + 3 {
                return None;
            }
            Some(Montagem {
                ponto: desescapar(campos[4]),
                maj_min: campos[2].to_owned(),
                tipo: campos[separador + 1].to_owned(),
                fonte: desescapar(campos[separador + 2]),
            })
        })
        .collect()
}

/// A montagem que contém `caminho` (prefixo por componente, o ponto mais longo ganha; a
/// última da lista vence empates, como no kernel).
#[must_use]
pub fn montagem_do_caminho<'a>(montagens: &'a [Montagem], caminho: &str) -> Option<&'a Montagem> {
    let contem = |ponto: &str| {
        ponto == "/"
            || caminho == ponto
            || caminho
                .strip_prefix(ponto)
                .is_some_and(|resto| resto.starts_with('/'))
    };
    // O ponto mais longo ganha; entre iguais, a última da lista (a montada por cima).
    let mut melhor: Option<&Montagem> = None;
    for m in montagens.iter().filter(|m| contem(&m.ponto)) {
        if melhor.is_none_or(|b| m.ponto.len() >= b.ponto.len()) {
            melhor = Some(m);
        }
    }
    melhor
}

/// O que o analisador precisa do sistema.
pub trait Arvore {
    /// Conteúdo de um arquivo de texto, ou `None`.
    fn ler(&self, caminho: &str) -> Option<String>;
    /// Nomes das entradas de um diretório (vazio se não existe).
    fn listar(&self, caminho: &str) -> Vec<String>;
    /// Nome do bloco (`dm-0`, `nvme0n1p2`) de uma montagem.
    fn bloco_de(&self, montagem: &Montagem) -> Option<String>;
}

const PROFUNDIDADE_MAXIMA: usize = 16;

fn cifrado_em<A: Arvore>(
    arvore: &A,
    bloco: &str,
    vistos: &mut HashSet<String>,
    nivel: usize,
) -> bool {
    if nivel > PROFUNDIDADE_MAXIMA || !vistos.insert(bloco.to_owned()) {
        return false;
    }
    if arvore
        .ler(&format!("/sys/class/block/{bloco}/dm/uuid"))
        .is_some_and(|uuid| uuid.trim().starts_with("CRYPT-"))
    {
        return true;
    }
    arvore
        .listar(&format!("/sys/class/block/{bloco}/slaves"))
        .iter()
        .any(|pai| cifrado_em(arvore, pai, vistos, nivel + 1))
}

/// Análise pura: `caminho` precisa estar canonizado (absoluto, sem `..` nem links).
#[must_use]
pub fn analisar<A: Arvore>(arvore: &A, mountinfo: &str, caminho: &str) -> Cifra {
    let montagens = ler_mountinfo(mountinfo);
    let Some(montagem) = montagem_do_caminho(&montagens, caminho) else {
        return Cifra::Desconhecido;
    };
    let Some(bloco) = arvore.bloco_de(montagem) else {
        return Cifra::Desconhecido;
    };
    if arvore
        .ler(&format!("/sys/class/block/{bloco}/dev"))
        .is_none()
    {
        return Cifra::Desconhecido;
    }
    if cifrado_em(arvore, &bloco, &mut HashSet::new(), 0) {
        Cifra::Cifrado
    } else {
        Cifra::NaoCifrado
    }
}

/// O sistema real.
#[derive(Debug, Default)]
pub struct ArvoreReal;

impl Arvore for ArvoreReal {
    fn ler(&self, caminho: &str) -> Option<String> {
        std::fs::read_to_string(caminho).ok()
    }

    fn listar(&self, caminho: &str) -> Vec<String> {
        std::fs::read_dir(caminho)
            .map(|entradas| {
                entradas
                    .filter_map(Result::ok)
                    .filter_map(|e| e.file_name().into_string().ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    fn bloco_de(&self, montagem: &Montagem) -> Option<String> {
        let nome_do_link = |p: &Path| p.file_name()?.to_str().map(str::to_owned);
        if montagem.maj_min.split(':').next() != Some("0")
            && let Ok(destino) = std::fs::read_link(format!("/sys/dev/block/{}", montagem.maj_min))
            && let Some(nome) = nome_do_link(&destino)
        {
            return Some(nome);
        }
        // btrfs e afins reportam major 0: a fonte (`/dev/mapper/x`) aponta o bloco.
        if montagem.fonte.starts_with("/dev/") {
            let real = std::fs::canonicalize(&montagem.fonte).ok()?;
            return nome_do_link(&real);
        }
        None
    }
}

/// Cifra do volume de `caminho` neste Linux. Nunca falha.
#[must_use]
pub fn cifra_do_volume(caminho: &Path) -> Cifra {
    let Ok(canonico) = std::fs::canonicalize(caminho) else {
        return Cifra::Desconhecido;
    };
    let Some(texto) = canonico.to_str() else {
        return Cifra::Desconhecido;
    };
    let Ok(mountinfo) = std::fs::read_to_string("/proc/self/mountinfo") else {
        return Cifra::Desconhecido;
    };
    analisar(&ArvoreReal, &mountinfo, texto)
}

#[cfg(test)]
mod testes {
    use super::*;
    use std::collections::HashMap;

    /// Árvore falsa: arquivos, diretórios e o bloco de cada `major:minor`.
    #[derive(Default)]
    struct Falsa {
        arquivos: HashMap<String, String>,
        pastas: HashMap<String, Vec<String>>,
        blocos: HashMap<String, String>,
    }

    impl Falsa {
        fn bloco(&mut self, nome: &str, maj_min: &str, dm_uuid: Option<&str>, pais: &[&str]) {
            self.arquivos.insert(
                format!("/sys/class/block/{nome}/dev"),
                format!("{maj_min}\n"),
            );
            if let Some(uuid) = dm_uuid {
                self.arquivos.insert(
                    format!("/sys/class/block/{nome}/dm/uuid"),
                    format!("{uuid}\n"),
                );
            }
            self.pastas.insert(
                format!("/sys/class/block/{nome}/slaves"),
                pais.iter().map(|p| (*p).to_owned()).collect(),
            );
            self.blocos.insert(maj_min.to_owned(), nome.to_owned());
        }
    }

    impl Arvore for Falsa {
        fn ler(&self, caminho: &str) -> Option<String> {
            self.arquivos.get(caminho).cloned()
        }
        fn listar(&self, caminho: &str) -> Vec<String> {
            self.pastas.get(caminho).cloned().unwrap_or_default()
        }
        fn bloco_de(&self, m: &Montagem) -> Option<String> {
            self.blocos.get(&m.maj_min).cloned()
        }
    }

    const MOUNTINFO_CIFRADO: &str = "\
26 1 253:1 / / rw,relatime shared:1 - ext4 /dev/mapper/vg-raiz rw
27 26 0:6 / /dev rw,nosuid shared:2 - devtmpfs udev rw
30 26 259:1 / /boot rw,relatime shared:9 - ext4 /dev/nvme0n1p1 rw
31 26 0:50 / /run/user/1000 rw,nosuid shared:3 - tmpfs tmpfs rw
";

    /// nvme0n1p3 (partição) -> dm-0 (LUKS) -> dm-1 (LVM vg-raiz) montado em `/`.
    fn arvore_cifrada() -> Falsa {
        let mut a = Falsa::default();
        a.bloco("nvme0n1p3", "259:3", None, &[]);
        a.bloco(
            "dm-0",
            "253:0",
            Some("CRYPT-LUKS2-abcd-nvme"),
            &["nvme0n1p3"],
        );
        a.bloco("dm-1", "253:1", Some("LVM-xyz"), &["dm-0"]);
        a.bloco("nvme0n1p1", "259:1", None, &[]);
        a
    }

    #[test]
    fn lvm_sobre_luks_e_cifrado() {
        let a = arvore_cifrada();
        assert_eq!(
            analisar(&a, MOUNTINFO_CIFRADO, "/home/ana/financas"),
            Cifra::Cifrado
        );
    }

    #[test]
    fn particao_comum_nao_e_cifrada() {
        let a = arvore_cifrada();
        // /boot fica numa partição sem cifra nenhuma na cadeia.
        assert_eq!(
            analisar(&a, MOUNTINFO_CIFRADO, "/boot/efi/x"),
            Cifra::NaoCifrado
        );
    }

    #[test]
    fn ext4_direto_no_disco_nao_e_cifrado() {
        let mut a = Falsa::default();
        a.bloco("nvme1n1p2", "259:6", None, &[]);
        let mi = "33 2 259:6 / / rw,relatime shared:1 - ext4 /dev/nvme1n1p2 rw\n";
        assert_eq!(analisar(&a, mi, "/tmp/x"), Cifra::NaoCifrado);
    }

    #[test]
    fn dm_comum_sem_crypt_nao_e_cifrado() {
        let mut a = Falsa::default();
        a.bloco("sda2", "8:2", None, &[]);
        a.bloco("dm-0", "253:0", Some("LVM-abc"), &["sda2"]);
        let mi = "1 0 253:0 / / rw - ext4 /dev/mapper/vg-raiz rw\n";
        assert_eq!(analisar(&a, mi, "/x"), Cifra::NaoCifrado);
    }

    #[test]
    fn tmpfs_e_rede_sao_desconhecidos() {
        let a = arvore_cifrada();
        assert_eq!(
            analisar(&a, MOUNTINFO_CIFRADO, "/run/user/1000/x"),
            Cifra::Desconhecido
        );
        let rede = "40 26 0:60 / /mnt/nas rw - nfs4 servidor:/pasta rw\n";
        assert_eq!(analisar(&a, rede, "/mnt/nas/a"), Cifra::Desconhecido);
    }

    #[test]
    fn sem_montagem_ou_com_lixo_e_desconhecido() {
        let a = arvore_cifrada();
        assert_eq!(analisar(&a, "", "/x"), Cifra::Desconhecido);
        assert_eq!(
            analisar(&a, "lixo sem formato\n- - -\n", "/x"),
            Cifra::Desconhecido
        );
    }

    #[test]
    fn ciclo_nos_slaves_nao_trava() {
        let mut a = Falsa::default();
        a.bloco("dm-0", "253:0", None, &["dm-1"]);
        a.bloco("dm-1", "253:1", None, &["dm-0"]);
        let mi = "1 0 253:0 / / rw - ext4 /dev/dm-0 rw\n";
        assert_eq!(analisar(&a, mi, "/x"), Cifra::NaoCifrado);
    }

    #[test]
    fn ponto_mais_longo_ganha_e_prefixo_e_por_componente() {
        let m = ler_mountinfo(
            "1 0 8:1 / / rw - ext4 /dev/sda1 rw\n2 1 8:2 / /mnt/d rw - ext4 /dev/sda2 rw\n",
        );
        assert_eq!(
            montagem_do_caminho(&m, "/mnt/d/financas").map(|x| x.maj_min.as_str()),
            Some("8:2")
        );
        // "/mnt/dados" não está em "/mnt/d".
        assert_eq!(
            montagem_do_caminho(&m, "/mnt/dados/x").map(|x| x.maj_min.as_str()),
            Some("8:1")
        );
    }

    #[test]
    fn escapes_octais_do_ponto_de_montagem() {
        let m = ler_mountinfo("5 1 8:3 / /mnt/Disco\\040Externo rw - ext4 /dev/sdb1 rw\n");
        assert_eq!(m[0].ponto, "/mnt/Disco Externo");
    }

    #[test]
    fn btrfs_com_major_zero_usa_a_fonte() {
        // O teste da fonte real fica na ArvoreReal; aqui a árvore falsa casa por maj:min.
        let mut a = Falsa::default();
        a.bloco("dm-0", "0:45", Some("CRYPT-LUKS2-x"), &[]);
        let mi = "9 1 0:45 /@ / rw - btrfs /dev/mapper/cryptroot rw\n";
        assert_eq!(analisar(&a, mi, "/home/x"), Cifra::Cifrado);
    }

    /// INV-13 na máquina real: nunca falha, e o resultado é um dos três.
    #[test]
    fn pasta_temporaria_real_devolve_um_valor() {
        let pasta = tempfile::tempdir().expect("pasta temporária");
        let cifra = cifra_do_volume(pasta.path());
        eprintln!(
            "INV-13 real: cifra_do_volume({}) = {cifra:?}",
            pasta.path().display()
        );
        assert!(matches!(
            cifra,
            Cifra::Cifrado | Cifra::NaoCifrado | Cifra::Desconhecido
        ));
    }

    /// Registra o resultado em pastas reais da máquina (só informativo, não afirma o valor).
    #[test]
    fn pastas_reais_da_maquina() {
        for caminho in ["/", "/home", "/mnt/d", "/mnt/p", "/tmp", "/proc"] {
            let cifra = cifra_do_volume(Path::new(caminho));
            eprintln!("INV-13 real: {caminho} = {cifra:?}");
        }
    }

    #[test]
    fn caminho_que_nao_existe_e_desconhecido() {
        assert_eq!(
            cifra_do_volume(Path::new("/nao/existe/mesmo")),
            Cifra::Desconhecido
        );
    }
}
