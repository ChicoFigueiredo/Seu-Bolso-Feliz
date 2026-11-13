//! A estante: `SBF/Estante/` no Drive, só com documentos, organizada em
//! `<ciclo AAAA-MM>/<tipo>/<nome>` (ou `a-classificar/<tipo>/<nome>`), com os bytes
//! originais do acervo.
//!
//! # Como fica incremental (decisão: sem `sync --track-renames`)
//!
//! O nome de cada arquivo termina em `_<hash8>.<ext>` (os 8 primeiros caracteres do hash do
//! conteúdo, igual ao `vista-nome.ts`). Então o próprio **remoto é o manifesto**: a cada
//! execução listamos a estante (`rclone lsf -R`), lemos o `hash8` de cada nome e comparamos
//! com o que o banco diz que deve existir. O mesmo `hash8` em outro caminho é um rename
//! (`rclone moveto`, server-side no Drive); `hash8` novo é envio; `hash8` que sumiu do
//! banco (documento descartado ou em dúvida) é apagado. Por que não `sync --track-renames`:
//! - ele exigiria uma pasta de preparo com hardlinks/cópias dos originais renomeados, e
//!   ainda falharia em remotos sem hash comum (crypt, alguns provedores);
//! - `sync` apaga tudo que não reconhece, inclusive o que o usuário pôs na pasta à mão;
//!   aqui só mexemos em arquivos com o nosso padrão de nome (o resto vai em `estranhos`);
//! - sem manifesto local para perder, estragar ou dessincronizar: apagar o estado local
//!   não custa nada, o remoto é a verdade.
//!
//! Custo: uma listagem recursiva por execução e um processo `rclone` por arquivo mexido
//! (4 em paralelo).

use crate::erro::Resultado;
use crate::nome::{DadosDoNome, nome_na_vista, pasta_na_estante};
use crate::rclone::{Rclone, juntar, validar_destino};
use rusqlite::Connection;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Quantos processos `rclone` em paralelo ao enviar/renomear/apagar.
const PARALELISMO: usize = 4;
/// Código de saída do rclone para "diretório não encontrado".
const CODIGO_NAO_ENCONTRADO: i32 = 3;

/// Um documento que deve estar na estante.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ItemDaEstante {
    /// Hash do conteúdo.
    pub content_hash: String,
    /// Caminho relativo à base da estante (`2026-10/faturas/....pdf`).
    pub destino: String,
    /// Arquivo original no acervo.
    pub origem: PathBuf,
}

/// O que uma atualização faria.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Plano {
    /// Documentos que ainda não estão na estante.
    pub enviar: Vec<ItemDaEstante>,
    /// `(de, para)`: correção de fornecedor, data, valor ou tipo.
    pub renomear: Vec<(String, String)>,
    /// Arquivos da estante que não são mais documentos (ou duplicados).
    pub apagar: Vec<String>,
    /// Quantos já estavam certos.
    pub intactos: usize,
    /// Arquivos na estante que não seguem o nosso padrão de nome; nunca são tocados.
    pub estranhos: Vec<String>,
    /// Documentos sem original no acervo (sem `caminho_acervo` ou arquivo ausente).
    pub sem_copia: usize,
    /// Documentos cujo `hash8` colide com outro; o segundo fica de fora.
    pub colisoes: usize,
}

impl Plano {
    /// Nada a fazer.
    #[must_use]
    pub fn vazio(&self) -> bool {
        self.enviar.is_empty() && self.renomear.is_empty() && self.apagar.is_empty()
    }
}

/// Fase de uma atualização, para a barra de progresso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fase {
    /// Lendo o banco e a listagem do remoto.
    Planejando,
    /// Renomeando na estante.
    Renomeando,
    /// Enviando documentos.
    Enviando,
    /// Apagando o que não é mais documento.
    Apagando,
}

/// Andamento de uma atualização.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Andamento {
    /// Fase atual.
    pub fase: Fase,
    /// Operações concluídas na fase.
    pub feitos: usize,
    /// Operações da fase.
    pub total: usize,
}

/// Resultado de uma atualização.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RelatorioDaEstante {
    /// Enviados.
    pub enviados: usize,
    /// Renomeados.
    pub renomeados: usize,
    /// Apagados.
    pub apagados: usize,
    /// Já estavam certos.
    pub intactos: usize,
    /// Arquivos alheios deixados em paz.
    pub estranhos: usize,
    /// Documentos sem original no acervo.
    pub sem_copia: usize,
    /// Operações que falharam (mensagem curta, sem segredo). A próxima execução tenta de novo.
    pub falhas: Vec<String>,
}

/// Os 8 caracteres hexadecimais do fim do nome (`..._ab12cd34.pdf` ou `ab12cd34.pdf`).
#[must_use]
pub fn hash8_do_nome(caminho: &str) -> Option<String> {
    let nome = caminho.rsplit('/').next()?;
    let sem_ext = nome.split('.').next()?;
    let candidato = sem_ext.rsplit('_').next()?;
    (candidato.len() == 8
        && candidato
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')))
    .then(|| candidato.to_owned())
}

fn caminho_seguro(relativo: &str) -> bool {
    let caminho = Path::new(relativo);
    !relativo.is_empty()
        && caminho.is_relative()
        && caminho
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
}

/// Lê do banco os documentos que devem estar na estante. Devolve também quantos não têm
/// original no acervo.
///
/// # Errors
/// Falha do SQLite.
pub fn documentos_desejados(
    conexao: &Connection,
    cofre: &Path,
) -> Resultado<(Vec<ItemDaEstante>, usize)> {
    let mut consulta = conexao.prepare(
        "SELECT a.content_hash, a.extensao, a.document_type, a.data_chave, a.ciclo_id,
                a.supplier_name_raw, a.valor_centavos, a.caminho_acervo,
                (SELECT o.filename FROM origem o WHERE o.content_hash = a.content_hash
                  ORDER BY o.id LIMIT 1) AS nome_original
           FROM achado a
          WHERE a.estado = 'documento'
          ORDER BY a.content_hash",
    )?;
    let linhas = consulta.query_map([], |l| {
        Ok((
            l.get::<_, String>(0)?,
            l.get::<_, Option<String>>(1)?,
            l.get::<_, Option<String>>(2)?,
            l.get::<_, Option<String>>(3)?,
            l.get::<_, Option<String>>(4)?,
            l.get::<_, Option<String>>(5)?,
            l.get::<_, Option<i64>>(6)?,
            l.get::<_, Option<String>>(7)?,
            l.get::<_, Option<String>>(8)?,
        ))
    })?;
    let mut itens = Vec::new();
    let mut sem_copia = 0;
    for linha in linhas {
        let (hash, ext, tipo, data, ciclo, fornecedor, valor, acervo, original) = linha?;
        let Some(acervo) = acervo.filter(|a| caminho_seguro(a)) else {
            sem_copia += 1;
            continue;
        };
        let origem = cofre.join(&acervo);
        if !origem.is_file() {
            sem_copia += 1;
            continue;
        }
        let nome = nome_na_vista(&DadosDoNome {
            data_chave: data.as_deref(),
            fornecedor: fornecedor.as_deref(),
            valor_centavos: valor,
            content_hash: &hash,
            extensao: ext.as_deref(),
            nome_original: original.as_deref(),
        });
        let pasta = pasta_na_estante(ciclo.as_deref(), tipo.as_deref());
        itens.push(ItemDaEstante {
            content_hash: hash,
            destino: format!("{pasta}/{nome}"),
            origem,
        });
    }
    Ok((itens, sem_copia))
}

/// Compara o desejado com a listagem do remoto. Puro.
#[must_use]
pub fn planejar(desejados: &[ItemDaEstante], sem_copia: usize, remoto: &[String]) -> Plano {
    let mut plano = Plano {
        sem_copia,
        ..Plano::default()
    };
    // hash8 -> item (o primeiro vence; colisão conta e fica de fora).
    let mut por_hash8: HashMap<String, &ItemDaEstante> = HashMap::new();
    for item in desejados {
        match hash8_do_nome(&item.destino) {
            Some(h) if por_hash8.contains_key(&h) => plano.colisoes += 1,
            Some(h) => {
                por_hash8.insert(h, item);
            }
            None => plano.colisoes += 1,
        }
    }
    // hash8 -> caminhos que existem no remoto.
    let mut no_remoto: HashMap<String, Vec<&String>> = HashMap::new();
    for caminho in remoto {
        match hash8_do_nome(caminho) {
            Some(h) => no_remoto.entry(h).or_default().push(caminho),
            None => plano.estranhos.push(caminho.clone()),
        }
    }
    let mut chaves: Vec<&String> = no_remoto.keys().collect();
    chaves.sort();
    for h in chaves {
        let caminhos = &no_remoto[h];
        match por_hash8.get(h) {
            None => plano.apagar.extend(caminhos.iter().map(|c| (*c).clone())),
            Some(item) => {
                if caminhos.iter().any(|c| **c == item.destino) {
                    plano.intactos += 1;
                    plano.apagar.extend(
                        caminhos
                            .iter()
                            .filter(|c| ***c != item.destino)
                            .map(|c| (*c).clone()),
                    );
                } else {
                    plano
                        .renomear
                        .push((caminhos[0].clone(), item.destino.clone()));
                    plano
                        .apagar
                        .extend(caminhos[1..].iter().map(|c| (*c).clone()));
                }
            }
        }
    }
    let mut itens: Vec<&ItemDaEstante> = por_hash8.values().copied().collect();
    itens.sort_by(|a, b| a.destino.cmp(&b.destino));
    for item in itens {
        let h = hash8_do_nome(&item.destino).unwrap_or_default();
        if !no_remoto.contains_key(&h) {
            plano.enviar.push(item.clone());
        }
    }
    plano.renomear.sort();
    plano.apagar.sort();
    plano
}

/// A estante num remoto rclone (`gdrive:SBF/Estante`, ou uma pasta local em teste).
#[derive(Debug, Clone)]
pub struct Estante {
    rclone: Rclone,
    base: String,
}

impl Estante {
    /// `base` é o remoto e a pasta, por exemplo `gdrive:SBF/Estante`.
    ///
    /// # Errors
    /// [`crate::Erro::DestinoInvalido`] se vazio ou começando com `-`.
    pub fn nova(rclone: Rclone, base: impl Into<String>) -> Resultado<Self> {
        let base = base.into();
        validar_destino(&base)?;
        Ok(Self { rclone, base })
    }

    /// Lista os arquivos da estante (caminhos relativos). Estante ainda inexistente = vazia.
    ///
    /// # Errors
    /// Falha do rclone diferente de "pasta não encontrada".
    pub fn listar(&self) -> Resultado<Vec<String>> {
        match self
            .rclone
            .executar(["lsf", "-R", "--files-only", self.base.as_str()], &[], None)
        {
            Ok(saida) => Ok(saida
                .lines()
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect()),
            Err(crate::Erro::Rclone {
                codigo: Some(CODIGO_NAO_ENCONTRADO),
                ..
            }) => Ok(Vec::new()),
            Err(outro) => Err(outro),
        }
    }

    /// O que [`Self::atualizar`] faria, sem mexer em nada ("N a enviar" na janela).
    ///
    /// # Errors
    /// Falha do SQLite ou do rclone.
    pub fn planejar(&self, conexao: &Connection, cofre: &Path) -> Resultado<Plano> {
        let (desejados, sem_copia) = documentos_desejados(conexao, cofre)?;
        Ok(planejar(&desejados, sem_copia, &self.listar()?))
    }

    /// Atualiza a estante: renomeia, envia e apaga o necessário. Falha de um arquivo não
    /// para os outros; vai em `falhas` e a próxima execução tenta de novo.
    ///
    /// # Errors
    /// Falha do SQLite ou da listagem. Falhas de arquivo vão no relatório.
    pub fn atualizar(
        &self,
        conexao: &Connection,
        cofre: &Path,
        progresso: &(dyn Fn(Andamento) + Sync),
    ) -> Resultado<RelatorioDaEstante> {
        progresso(Andamento {
            fase: Fase::Planejando,
            feitos: 0,
            total: 1,
        });
        let plano = self.planejar(conexao, cofre)?;
        let falhas = Mutex::new(Vec::new());
        let mut relatorio = RelatorioDaEstante {
            intactos: plano.intactos,
            estranhos: plano.estranhos.len(),
            sem_copia: plano.sem_copia,
            ..RelatorioDaEstante::default()
        };

        let renomear: Vec<_> = plano.renomear.iter().collect();
        relatorio.renomeados = self.em_paralelo(
            Fase::Renomeando,
            &renomear,
            progresso,
            &falhas,
            |(de, para)| {
                self.rclone
                    .executar(
                        [
                            "moveto".to_owned(),
                            juntar(&self.base, de),
                            juntar(&self.base, para),
                        ],
                        &[],
                        None,
                    )
                    .map(|_| ())
            },
        );
        let enviar: Vec<_> = plano.enviar.iter().collect();
        relatorio.enviados =
            self.em_paralelo(Fase::Enviando, &enviar, progresso, &falhas, |item| {
                self.rclone
                    .executar(
                        [
                            std::ffi::OsString::from("copyto"),
                            item.origem.clone().into_os_string(),
                            juntar(&self.base, &item.destino).into(),
                        ],
                        &[],
                        None,
                    )
                    .map(|_| ())
            });
        let apagar: Vec<_> = plano.apagar.iter().collect();
        relatorio.apagados =
            self.em_paralelo(Fase::Apagando, &apagar, progresso, &falhas, |caminho| {
                self.rclone
                    .executar(
                        ["deletefile".to_owned(), juntar(&self.base, caminho)],
                        &[],
                        None,
                    )
                    .map(|_| ())
            });

        // Pastas que ficaram vazias depois de renomear ou apagar.
        if relatorio.renomeados + relatorio.apagados > 0
            && let Err(e) =
                self.rclone
                    .executar(["rmdirs", "--leave-root", self.base.as_str()], &[], None)
            && let Ok(mut f) = falhas.lock()
        {
            f.push(format!("limpar pastas vazias: {e}"));
        }
        relatorio.falhas = falhas.into_inner().unwrap_or_default();
        Ok(relatorio)
    }

    /// Roda `operacao` sobre `itens` com [`PARALELISMO`] threads. Devolve quantas deram certo.
    fn em_paralelo<T: Sync>(
        &self,
        fase: Fase,
        itens: &[&T],
        progresso: &(dyn Fn(Andamento) + Sync),
        falhas: &Mutex<Vec<String>>,
        operacao: impl Fn(&T) -> Resultado<()> + Sync,
    ) -> usize {
        let total = itens.len();
        if total == 0 {
            return 0;
        }
        let proximo = AtomicUsize::new(0);
        let feitos = AtomicUsize::new(0);
        let certos = AtomicUsize::new(0);
        std::thread::scope(|escopo| {
            for _ in 0..PARALELISMO.min(total) {
                escopo.spawn(|| {
                    loop {
                        let i = proximo.fetch_add(1, Ordering::Relaxed);
                        let Some(item) = itens.get(i) else { break };
                        match operacao(item) {
                            Ok(()) => {
                                certos.fetch_add(1, Ordering::Relaxed);
                            }
                            Err(e) => {
                                if let Ok(mut f) = falhas.lock() {
                                    f.push(e.to_string());
                                }
                            }
                        }
                        let n = feitos.fetch_add(1, Ordering::Relaxed) + 1;
                        progresso(Andamento {
                            fase,
                            feitos: n,
                            total,
                        });
                    }
                });
            }
        });
        certos.into_inner()
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn item(destino: &str) -> ItemDaEstante {
        ItemDaEstante {
            content_hash: "x".into(),
            destino: destino.into(),
            origem: PathBuf::from("/x"),
        }
    }

    #[test]
    fn hash8_do_nome_de_arquivo() {
        assert_eq!(
            hash8_do_nome("2026-10/faturas/2026-10-25_nu_1234,56_ab12cd34.pdf").as_deref(),
            Some("ab12cd34")
        );
        assert_eq!(
            hash8_do_nome("a-classificar/outros/ab12cd34.pdf").as_deref(),
            Some("ab12cd34")
        );
        assert_eq!(hash8_do_nome("a/b/ab12cd34").as_deref(), Some("ab12cd34"));
        assert_eq!(hash8_do_nome("a/b/anotacoes.txt"), None);
        assert_eq!(hash8_do_nome("a/b/x_AB12CD34.pdf"), None);
        assert_eq!(hash8_do_nome("a/b/x_ab12cd3.pdf"), None);
    }

    #[test]
    fn plano_envia_renomeia_apaga_e_preserva() {
        let desejados = vec![
            item("2026-10/faturas/2026-10-25_nu_10,00_aaaaaaaa.pdf"),
            item("2026-11/boletos/2026-11-02_luz_5,00_bbbbbbbb.pdf"),
            item("a-classificar/outros/cccccccc.pdf"),
        ];
        let remoto = vec![
            // intacto
            "2026-10/faturas/2026-10-25_nu_10,00_aaaaaaaa.pdf".to_owned(),
            // fornecedor corrigido: rename
            "2026-11/boletos/2026-11-02_cemig_5,00_bbbbbbbb.pdf".to_owned(),
            // descartado: apagar
            "2026-09/faturas/2026-09-01_x_1,00_dddddddd.pdf".to_owned(),
            // alheio
            "2026-09/anotacao.txt".to_owned(),
        ];
        let p = planejar(&desejados, 2, &remoto);
        assert_eq!(p.intactos, 1);
        assert_eq!(
            p.renomear,
            vec![(
                "2026-11/boletos/2026-11-02_cemig_5,00_bbbbbbbb.pdf".to_owned(),
                "2026-11/boletos/2026-11-02_luz_5,00_bbbbbbbb.pdf".to_owned()
            )]
        );
        assert_eq!(
            p.apagar,
            vec!["2026-09/faturas/2026-09-01_x_1,00_dddddddd.pdf".to_owned()]
        );
        assert_eq!(p.enviar.len(), 1);
        assert_eq!(p.enviar[0].destino, "a-classificar/outros/cccccccc.pdf");
        assert_eq!(p.estranhos, vec!["2026-09/anotacao.txt".to_owned()]);
        assert_eq!(p.sem_copia, 2);
        assert!(!p.vazio());
    }

    #[test]
    fn copia_repetida_no_remoto_e_apagada() {
        let desejados = vec![item("2026-10/faturas/a_aaaaaaaa.pdf")];
        let remoto = vec![
            "2026-10/faturas/a_aaaaaaaa.pdf".to_owned(),
            "a-classificar/faturas/a_aaaaaaaa.pdf".to_owned(),
        ];
        let p = planejar(&desejados, 0, &remoto);
        assert_eq!(
            p.apagar,
            vec!["a-classificar/faturas/a_aaaaaaaa.pdf".to_owned()]
        );
        assert!(p.enviar.is_empty() && p.renomear.is_empty());
    }

    #[test]
    fn sem_mudanca_o_plano_e_vazio() {
        let desejados = vec![item("2026-10/faturas/a_aaaaaaaa.pdf")];
        let p = planejar(
            &desejados,
            0,
            &["2026-10/faturas/a_aaaaaaaa.pdf".to_owned()],
        );
        assert!(p.vazio());
        assert_eq!(p.intactos, 1);
    }

    #[test]
    fn caminhos_do_acervo_perigosos_sao_recusados() {
        assert!(caminho_seguro("acervo/ab/abcdef.pdf"));
        assert!(!caminho_seguro("../fora.pdf"));
        assert!(!caminho_seguro("/etc/passwd"));
        assert!(!caminho_seguro(""));
    }
}
