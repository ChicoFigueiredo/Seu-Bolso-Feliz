//! Coleta de Google Drive e OneDrive via rclone: "listar tudo, baixar só o que falta".
//! Porte de `workers/inventario/src/coletar-nuvem.ts`, com as mesmas regras:
//!
//! 1. Lista o remoto inteiro (`rclone lsjson -R --hash`, nada é baixado) e guarda a listagem
//!    na entrada do cofre.
//! 2. Ignora a pasta `SBF/` (é do próprio app) e classifica cada item pelas regras de área,
//!    como se estivesse na cópia local (espelho): o que o disco descartaria, a nuvem também.
//! 3. Candidato que já existe numa cópia local (mesmo caminho e tamanho) só ganha a origem da
//!    nuvem ligada ao conteúdo já indexado.
//! 4. O que falta é baixado para `entrada/<pasta>/` (só os candidatos).
//!
//! O remoto só é lido: nada na nuvem é criado, movido ou apagado.
//!
//! # Incremental
//!
//! A chave da origem é `<origem>:<caminho>:<modtime>`, a mesma do Bun, e entra com
//! `INSERT OR IGNORE`. Um item cuja chave já está no banco **com conteúdo ligado** não é
//! classificado nem baixado de novo; mudou o `ModTime`, a chave é nova e ele volta a ser
//! candidato.
//!
//! # Para o `sbf worker`
//!
//! [`ColetaDeNuvem::coletar`] é síncrona, recebe um [`AtomicBool`] de cancelamento (checado
//! entre itens e entre lotes de download) e um callback de progresso. O que já foi gravado
//! fica; a varredura é marcada `interrompida` e uma nova rodada continua de onde parou, pela
//! idempotência das chaves. A indexação dos arquivos baixados (o `scan-discos.ts`) é a etapa
//! seguinte; depois dela, [`ColetaDeNuvem::ligar_hashes`] liga as origens ao conteúdo.

use crate::areas::{Tratamento, classe_do_caminho, tratamento_do_arquivo};
use crate::erro::{Erro, Resultado};
use crate::pasta_sbf::dentro_da_pasta_do_sbf;
use rusqlite::{Connection, OptionalExtension as _, params};
use sbf_estante::Rclone;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Quantos arquivos vão em cada `rclone copy` (o cancelamento vale entre lotes).
pub const TAMANHO_DO_LOTE: usize = 200;
/// Prazo da listagem completa de um remoto grande.
const PRAZO_DA_LISTAGEM: Duration = Duration::from_secs(60 * 60);
/// De quantos em quantos itens o progresso e o cancelamento são consultados.
const PASSO_DO_PROGRESSO: usize = 500;
const RESPONSAVEL: &str = "sbf-coleta@1";

/// Uma nuvem coletável.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Nuvem {
    /// Id da fonte no inventário (`nuvem:onedrive`, `nuvem:gdrive`).
    pub fonte: &'static str,
    /// Descrição da fonte, se ela ainda não existir.
    pub descricao: &'static str,
    /// `origin_type` e prefixo da chave de origem.
    pub origem: &'static str,
    /// Pasta dentro de `entrada/`.
    pub pasta: &'static str,
    /// Tipo do backend no rclone (`onedrive`, `drive`).
    pub tipo_do_rclone: &'static str,
    /// Cópias locais antigas, a mais completa primeiro (a primeira dá a classe).
    pub espelhos: Vec<PathBuf>,
}

impl Nuvem {
    /// OneDrive, com os espelhos do dono.
    #[must_use]
    pub fn onedrive() -> Self {
        Self {
            fonte: "nuvem:onedrive",
            descricao: "OneDrive ao vivo (rclone, só leitura)",
            origem: "onedrive",
            pasta: "onedrive",
            tipo_do_rclone: "onedrive",
            espelhos: [
                "/mnt/p/OneDrive",
                "/mnt/p2/$Nuvem/OneDrive",
                "/mnt/bak/$Nuvem/OneDrive",
                "/mnt/e2/Pessoal/Chico/OneDrive",
                "/mnt/e2/Pessoal/Chico/@OneDrive",
            ]
            .map(PathBuf::from)
            .to_vec(),
        }
    }

    /// Google Drive, com os espelhos do dono.
    #[must_use]
    pub fn gdrive() -> Self {
        Self {
            fonte: "nuvem:gdrive",
            descricao: "Google Drive ao vivo (rclone, só leitura)",
            origem: "gdrive",
            pasta: "gdrive",
            tipo_do_rclone: "drive",
            espelhos: [
                "/mnt/p2/$Nuvem/GoogleDrive",
                "/mnt/bak/$Nuvem/GoogleDrive",
                "/mnt/e2/Pessoal/Chico/@GoogleDrive",
            ]
            .map(PathBuf::from)
            .to_vec(),
        }
    }

    /// A nuvem de um id de fonte (`nuvem:onedrive` ou `nuvem:gdrive`).
    #[must_use]
    pub fn da_fonte(fonte: &str) -> Option<Self> {
        match fonte {
            "nuvem:onedrive" => Some(Self::onedrive()),
            "nuvem:gdrive" => Some(Self::gdrive()),
            _ => None,
        }
    }

    /// Troca as cópias locais (a primeira dá a classe das pastas).
    #[must_use]
    pub fn com_espelhos(mut self, espelhos: Vec<PathBuf>) -> Self {
        self.espelhos = espelhos;
        self
    }
}

/// Um item da listagem do `rclone lsjson`.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "PascalCase")]
pub struct Item {
    /// Caminho relativo à raiz do remoto.
    pub path: String,
    /// Nome do arquivo.
    pub name: String,
    /// Tamanho em bytes (`-1` em pastas).
    #[serde(default)]
    pub size: i64,
    /// Data de modificação, como o rclone devolve.
    #[serde(default)]
    pub mod_time: String,
    /// É pasta?
    #[serde(default)]
    pub is_dir: bool,
    /// Hashes que o provedor dá (`--hash`), por algoritmo.
    #[serde(default)]
    pub hashes: BTreeMap<String, String>,
}

/// Lê a saída do `lsjson` e tira pastas e a pasta `SBF/`.
///
/// # Errors
/// [`Erro::Listagem`] se não for uma lista de itens.
pub fn ler_listagem(json: &str) -> Resultado<Vec<Item>> {
    let itens: Vec<Item> = serde_json::from_str(json).map_err(|e| Erro::Listagem(e.to_string()))?;
    Ok(itens
        .into_iter()
        .filter(|i| !i.is_dir && !dentro_da_pasta_do_sbf(&i.path))
        .collect())
}

/// O primeiro remoto de um tipo na saída de `rclone listremotes --long`, com o `:` no fim.
#[must_use]
pub fn remoto_do_tipo(listagem: &str, tipo: &str) -> Option<String> {
    listagem.lines().find_map(|l| {
        let (nome, t) = l.split_once(':')?;
        (t.trim() == tipo && !nome.trim().is_empty()).then(|| format!("{}:", nome.trim()))
    })
}

/// Em que ponto está a coleta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    /// Listando o remoto (nada é baixado).
    Listando,
    /// Classificando e ligando ao que já existe.
    Classificando,
    /// Baixando os candidatos que faltam.
    Baixando,
    /// Fechando a varredura.
    Concluindo,
}

/// Progresso entregue ao chamador.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Andamento {
    /// A fase.
    pub fase: Fase,
    /// Quanto já foi (itens ou arquivos).
    pub feito: usize,
    /// Quanto há no total da fase (0 se ainda não se sabe).
    pub total: usize,
}

/// O que a coleta fez.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Relatorio {
    /// Id da varredura gravada.
    pub varredura_id: i64,
    /// Itens (arquivos) listados, fora `SBF/`.
    pub listados: usize,
    /// Candidatos: o que o disco não descartaria.
    pub candidatos: usize,
    /// Candidatos já ligados a conteúdo local (ou conhecidos de uma coleta anterior).
    pub ja_locais: usize,
    /// Candidatos a baixar.
    pub a_baixar: usize,
    /// Fora de escopo (área C, executáveis, fotos fora de área pessoal...).
    pub fora_de_escopo: usize,
    /// Arquivos baixados de fato.
    pub baixados: usize,
    /// Itens que não puderam ser baixados com segurança (caminho com `..` ou quebra de linha).
    pub recusados: usize,
    /// Candidatos ainda sem conteúdo ligado no fim.
    pub sem_hash_no_fim: usize,
    /// Onde os arquivos baixados ficam (`entrada/<pasta>`), para o scanner.
    pub destino: PathBuf,
    /// Cancelada antes do fim.
    pub interrompida: bool,
}

/// Onde o cofre guarda a entrada e os temporários.
#[derive(Debug, Clone)]
pub struct PastasDoCofre {
    /// `entrada/`: material trazido de fora antes do scan.
    pub entrada: PathBuf,
    /// `tmp/`: listas de download.
    pub tmp: PathBuf,
}

impl PastasDoCofre {
    /// As pastas padrão de um cofre (`<raiz>/entrada`, `<raiz>/tmp`).
    #[must_use]
    pub fn de(raiz: &Path) -> Self {
        Self {
            entrada: raiz.join("entrada"),
            tmp: raiz.join("tmp"),
        }
    }
}

/// Uma coleta de uma nuvem.
#[derive(Debug)]
pub struct ColetaDeNuvem<'a> {
    rclone: &'a Rclone,
    nuvem: Nuvem,
    remoto: String,
    cofre: PastasDoCofre,
    so_listar: bool,
}

impl<'a> ColetaDeNuvem<'a> {
    /// Coleta de `remoto` (por exemplo `onedrive:`), que já deve existir no rclone.
    #[must_use]
    pub fn nova(
        rclone: &'a Rclone,
        nuvem: Nuvem,
        remoto: impl Into<String>,
        cofre: PastasDoCofre,
    ) -> Self {
        Self {
            rclone,
            nuvem,
            remoto: remoto.into(),
            cofre,
            so_listar: false,
        }
    }

    /// Acha o remoto pelo **tipo** no rclone, não pelo nome (`OneDrive`, `onedrive` ou o que o
    /// usuário tenha dado).
    ///
    /// # Errors
    /// [`Erro::SemRemoto`] se não houver remoto desse tipo.
    pub fn detectando(rclone: &'a Rclone, nuvem: Nuvem, cofre: PastasDoCofre) -> Resultado<Self> {
        let lista = rclone.executar(["listremotes", "--long"], &[], None)?;
        let remoto = remoto_do_tipo(&lista, nuvem.tipo_do_rclone)
            .ok_or(Erro::SemRemoto(nuvem.tipo_do_rclone))?;
        Ok(Self::nova(rclone, nuvem, remoto, cofre))
    }

    /// Só lista e registra; não baixa (`--so-listar`).
    #[must_use]
    pub fn so_listando(mut self) -> Self {
        self.so_listar = true;
        self
    }

    /// O remoto em uso.
    #[must_use]
    pub fn remoto(&self) -> &str {
        &self.remoto
    }

    /// Roda a coleta. `cancelar` vira `true` de outra thread para parar; `progresso` recebe
    /// o andamento. Grava varredura e origens em `conexao`.
    ///
    /// # Errors
    /// Falha do rclone, do banco ou do disco. A varredura fica marcada `falhou`.
    pub fn coletar(
        &self,
        conexao: &Connection,
        cancelar: &AtomicBool,
        progresso: &dyn Fn(Andamento),
    ) -> Resultado<Relatorio> {
        // O id da varredura só existe depois da listagem; falha antes dela não deixa rastro.
        progresso(Andamento {
            fase: Fase::Listando,
            feito: 0,
            total: 0,
        });
        let json = self.rclone.executar_com(
            [
                "lsjson",
                "-R",
                "--files-only",
                "--fast-list",
                "--hash",
                self.remoto.as_str(),
            ],
            &[],
            None,
            &[],
            Some(PRAZO_DA_LISTAGEM),
        )?;
        let itens = ler_listagem(&json)?;
        let destino = self.cofre.entrada.join(self.nuvem.pasta);
        criar_pasta_privada(&destino)?;
        let listagem =
            self.cofre
                .entrada
                .join(format!("{}-lista-{}.json", self.nuvem.pasta, hoje_iso()));
        std::fs::write(&listagem, json.as_bytes())?;

        conexao.execute(
            "INSERT OR IGNORE INTO fonte (id, tipo, descricao, escopo) VALUES (?1, 'nuvem', ?2, 'dentro')",
            params![self.nuvem.fonte, self.nuvem.descricao],
        )?;
        let varredura_id: i64 = conexao.query_row(
            "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel) VALUES (?1, 'E2', ?2, ?3) RETURNING id",
            params![self.nuvem.fonte, self.remoto, RESPONSAVEL],
            |l| l.get(0),
        )?;
        let mut relatorio = Relatorio {
            varredura_id,
            listados: itens.len(),
            destino: destino.clone(),
            ..Relatorio::default()
        };
        match self.rodar(
            conexao,
            &itens,
            &destino,
            cancelar,
            progresso,
            &mut relatorio,
        ) {
            Ok(()) => {
                progresso(Andamento {
                    fase: Fase::Concluindo,
                    feito: relatorio.listados,
                    total: relatorio.listados,
                });
                self.ligar_hashes(conexao)?;
                relatorio.sem_hash_no_fim = conexao.query_row(
                    "SELECT count(*) FROM origem WHERE varredura_id = ?1 AND content_hash IS NULL",
                    [varredura_id],
                    |l| l.get::<_, i64>(0),
                )? as usize;
                let status = if relatorio.interrompida || (self.so_listar && relatorio.a_baixar > 0)
                {
                    "interrompida"
                } else {
                    "concluida"
                };
                self.fechar(conexao, &relatorio, status, &listagem)?;
                Ok(relatorio)
            }
            Err(e) => {
                let _ = self.fechar(conexao, &relatorio, "falhou", &listagem);
                Err(e)
            }
        }
    }

    fn rodar(
        &self,
        conexao: &Connection,
        itens: &[Item],
        destino: &Path,
        cancelar: &AtomicBool,
        progresso: &dyn Fn(Andamento),
        r: &mut Relatorio,
    ) -> Resultado<()> {
        let mut baixar: Vec<&str> = Vec::new();
        let base = self.nuvem.espelhos.first().cloned().unwrap_or_default();
        {
            let tx = conexao.unchecked_transaction()?;
            {
                let mut conhecida = tx.prepare(
                    "SELECT content_hash FROM origem WHERE origin_key = ?1 AND content_hash IS NOT NULL",
                )?;
                let mut inserir = tx.prepare(
                    "INSERT OR IGNORE INTO origem (content_hash, fonte_id, varredura_id, origin_type, origin_key,
                       filename, file_size_bytes, local_mtime, mensagem_ref)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                )?;
                let mut hash_local = tx.prepare(
                    "SELECT content_hash FROM origem WHERE local_filepath = ?1 AND content_hash IS NOT NULL LIMIT 1",
                )?;
                for (i, item) in itens.iter().enumerate() {
                    if i % PASSO_DO_PROGRESSO == 0 {
                        progresso(Andamento {
                            fase: Fase::Classificando,
                            feito: i,
                            total: itens.len(),
                        });
                        if cancelar.load(Ordering::Relaxed) {
                            r.interrompida = true;
                            break;
                        }
                    }
                    let chave = format!("{}:{}:{}", self.nuvem.origem, item.path, item.mod_time);
                    // Incremental: o que uma coleta anterior já ligou não se repete.
                    if conhecida.exists([&chave])? {
                        r.candidatos += 1;
                        r.ja_locais += 1;
                        continue;
                    }
                    let virtual_ = base.join(&item.path);
                    let pasta = virtual_.parent().unwrap_or(Path::new("/"));
                    let classe = classe_do_caminho(&normalizar(pasta));
                    let tratamento = tratamento_do_arquivo(&item.name, &classe);
                    if classe.regra.ignorar
                        || !matches!(tratamento, Tratamento::Copiar | Tratamento::Hash)
                    {
                        r.fora_de_escopo += 1;
                        continue;
                    }
                    r.candidatos += 1;
                    let mut hash: Option<String> = None;
                    for espelho in &self.nuvem.espelhos {
                        let caminho = espelho.join(&item.path);
                        let tamanho_bate = std::fs::metadata(&caminho)
                            .is_ok_and(|m| i64::try_from(m.len()).is_ok_and(|t| t == item.size));
                        if tamanho_bate {
                            hash = hash_local
                                .query_row([caminho.to_string_lossy()], |l| l.get(0))
                                .optional()?;
                            if hash.is_some() {
                                break;
                            }
                        }
                    }
                    if hash.is_some() {
                        r.ja_locais += 1;
                    } else if caminho_seguro(&item.path) {
                        if !destino.join(&item.path).exists() {
                            baixar.push(&item.path);
                            r.a_baixar += 1;
                        }
                    } else {
                        r.recusados += 1;
                    }
                    inserir.execute(params![
                        hash,
                        self.nuvem.fonte,
                        r.varredura_id,
                        self.nuvem.origem,
                        chave,
                        item.name,
                        item.size,
                        item.mod_time,
                        format!("{}{}", self.remoto, item.path),
                    ])?;
                }
            }
            tx.commit()?;
        }
        if self.so_listar || baixar.is_empty() || r.interrompida {
            return Ok(());
        }
        for lote in baixar.chunks(TAMANHO_DO_LOTE) {
            if cancelar.load(Ordering::Relaxed) {
                r.interrompida = true;
                return Ok(());
            }
            progresso(Andamento {
                fase: Fase::Baixando,
                feito: r.baixados,
                total: baixar.len(),
            });
            self.baixar_lote(lote, destino, r.varredura_id)?;
            r.baixados += lote.len();
        }
        progresso(Andamento {
            fase: Fase::Baixando,
            feito: r.baixados,
            total: baixar.len(),
        });
        Ok(())
    }

    fn baixar_lote(&self, lote: &[&str], destino: &Path, varredura: i64) -> Resultado<()> {
        criar_pasta_privada(&self.cofre.tmp)?;
        let lista = self
            .cofre
            .tmp
            .join(format!("{}-baixar-{varredura}.txt", self.nuvem.pasta));
        escrever_privado(&lista, &format!("{}\n", lote.join("\n")))?;
        let resultado = self.rclone.executar(
            [
                std::ffi::OsString::from("copy"),
                self.remoto.clone().into(),
                destino.into(),
                "--files-from-raw".into(),
                lista.clone().into(),
                "--transfers".into(),
                "4".into(),
                "--checkers".into(),
                "8".into(),
            ],
            &[],
            None,
        );
        let _ = std::fs::remove_file(&lista);
        resultado.map(|_| ()).map_err(Erro::from)
    }

    /// Liga a origem da nuvem ao conteúdo do arquivo baixado e já indexado
    /// (`origem.local_filepath` do scan de `entrada/<pasta>`). Chame depois do scan.
    /// Devolve quantas origens ganharam conteúdo.
    ///
    /// # Errors
    /// Falha do banco.
    pub fn ligar_hashes(&self, conexao: &Connection) -> Resultado<usize> {
        let prefixo = format!("{}/", self.cofre.entrada.join(self.nuvem.pasta).display());
        let n = conexao.execute(
            "UPDATE origem SET content_hash = (
               SELECT l.content_hash FROM origem l
                WHERE l.local_filepath = ?1 || substr(origem.mensagem_ref, ?2)
                  AND l.content_hash IS NOT NULL LIMIT 1)
             WHERE fonte_id = ?3 AND content_hash IS NULL AND mensagem_ref LIKE ?4 ESCAPE '\\'
               AND EXISTS (
                SELECT 1 FROM origem l
                 WHERE l.local_filepath = ?1 || substr(origem.mensagem_ref, ?2)
                   AND l.content_hash IS NOT NULL)",
            params![
                prefixo,
                i64::try_from(self.remoto.chars().count() + 1).unwrap_or(i64::MAX),
                self.nuvem.fonte,
                format!("{}%", escapar_like(&self.remoto)),
            ],
        )?;
        Ok(n)
    }

    fn fechar(
        &self,
        conexao: &Connection,
        r: &Relatorio,
        status: &str,
        listagem: &Path,
    ) -> Resultado<()> {
        let resumo = serde_json::json!({
            "listados": r.listados,
            "candidatos": r.candidatos,
            "jaLocais": r.ja_locais,
            "aBaixar": r.a_baixar,
            "foraDeEscopo": r.fora_de_escopo,
            "baixados": r.baixados,
            "recusados": r.recusados,
            "semHashNoFim": r.sem_hash_no_fim,
            "listagem": listagem
                .strip_prefix(self.cofre.entrada.parent().unwrap_or(Path::new("")))
                .unwrap_or(listagem)
                .to_string_lossy(),
        });
        conexao.execute(
            "UPDATE varredura SET status = ?1, concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now'),
               vistos = ?2, novos = ?3, repetidos = ?4, excluidos = ?5, erros = ?6, resumo = ?7 WHERE id = ?8",
            params![
                status,
                r.listados as i64,
                r.a_baixar as i64,
                r.ja_locais as i64,
                r.fora_de_escopo as i64,
                r.recusados as i64,
                resumo.to_string(),
                r.varredura_id,
            ],
        )?;
        Ok(())
    }
}

fn escapar_like(texto: &str) -> String {
    texto
        .replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Caminho relativo sem `..`, sem raiz e sem quebra de linha (a lista de download é por linha).
fn caminho_seguro(caminho: &str) -> bool {
    !caminho.is_empty()
        && !caminho.contains(['\n', '\r', '\0'])
        && Path::new(caminho)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// Como o `join` do Node normaliza: tira `.` e barras repetidas.
fn normalizar(caminho: &Path) -> String {
    let mut s = String::new();
    for c in caminho.components() {
        match c {
            Component::RootDir => s.push('/'),
            Component::Normal(p) => {
                if !s.ends_with('/') && !s.is_empty() {
                    s.push('/');
                }
                s.push_str(&p.to_string_lossy());
            }
            _ => {}
        }
    }
    s
}

fn criar_pasta_privada(pasta: &Path) -> Resultado<()> {
    std::fs::create_dir_all(pasta)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::set_permissions(pasta, std::fs::Permissions::from_mode(0o700))?;
    }
    Ok(())
}

fn escrever_privado(arquivo: &Path, conteudo: &str) -> Resultado<()> {
    #[cfg(unix)]
    {
        use std::io::Write as _;
        use std::os::unix::fs::OpenOptionsExt as _;
        let mut f = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(arquivo)?;
        f.write_all(conteudo.as_bytes())?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        std::fs::write(arquivo, conteudo)?;
        Ok(())
    }
}

/// Data de hoje no fuso do usuário (América/São Paulo, UTC-3 sem horário de verão desde 2019).
#[must_use]
pub fn hoje_iso() -> String {
    let segundos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
        - 3 * 3600;
    let dias = segundos.div_euclid(86_400);
    // Dias desde 1970-01-01 para data civil (algoritmo de Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!("{a:04}-{m:02}-{d:02}")
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn caminhos_perigosos_nao_baixam() {
        assert!(caminho_seguro("Docs/a.pdf"));
        assert!(!caminho_seguro("../a.pdf"));
        assert!(!caminho_seguro("/etc/a"));
        assert!(!caminho_seguro("a\nb.pdf"));
        assert!(!caminho_seguro("a/../../b"));
    }

    #[test]
    fn remoto_pelo_tipo() {
        let l = "gd: drive\nMeuOne: onedrive\n";
        assert_eq!(remoto_do_tipo(l, "onedrive").as_deref(), Some("MeuOne:"));
        assert_eq!(remoto_do_tipo(l, "dropbox"), None);
    }

    #[test]
    fn listagem_tira_pastas_e_sbf() {
        let j = r#"[{"Path":"SBF/x.pdf","Name":"x.pdf","Size":1,"ModTime":"t","IsDir":false},
                    {"Path":"A","Name":"A","Size":-1,"ModTime":"t","IsDir":true},
                    {"Path":"A/b.pdf","Name":"b.pdf","Size":2,"ModTime":"t","IsDir":false,"Hashes":{"md5":"abc"}}]"#;
        let itens = ler_listagem(j).expect("ok");
        assert_eq!(itens.len(), 1);
        assert_eq!(itens[0].hashes["md5"], "abc");
        assert!(ler_listagem("{").is_err());
    }

    #[test]
    fn data_civil() {
        let h = hoje_iso();
        assert_eq!(h.len(), 10);
        assert!(h.starts_with("20"));
    }
}
