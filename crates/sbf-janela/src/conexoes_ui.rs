//! A área **Conexões** (grupo Fontes): o que alimenta o inventário por conta própria.
//!
//! - **Telegram** (a conta do usuário, só leitura): o estado (conectado ou precisa entrar), as
//!   conversas seguidas, a última coleta e quantos arquivos entraram, com **Coletar agora** e
//!   Seguir/Deixar por conversa. O login é no terminal (`sbf telegram entrar`): a janela só
//!   avisa. A coleta roda numa thread; a interface só espia o canal.
//! - **Pluggy** e **pastas do worker**: só o estado, lido do que já existe no inventário e em
//!   `worker.json` (nada é consultado na rede).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{RichText, ScrollArea, Ui};
use rusqlite::{Connection, OpenFlags};
use sbf_chaveiro::Chaveiro;
use sbf_telegram::config::{ARQUIVO_DA_CONFIG, Config};
use sbf_telegram::modelo::Conversa;
use sbf_telegram::painel::{Painel, Situacao};

use crate::{componentes, tema};

/// Quem coleta o Telegram de verdade. O `sbf` o monta (precisa do reconhecimento e do
/// chaveiro); a janela só o chama numa thread.
pub trait ColetaDoTelegram: Send + Sync {
    /// Coleta todas as conversas seguidas e devolve o resumo em texto (sem conteúdo).
    ///
    /// # Errors
    /// O motivo, em texto curto, para mostrar na janela.
    fn coletar(&self) -> Result<String, String>;
}

/// Uma conversa na lista: a conversa, se é seguida e (se é) o que já entrou dela.
type LinhaDaLista = (Conversa, bool, Option<(u64, Option<String>)>);

/// Uma linha de estado de uma fonte (Pluggy ou pasta).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeFonte {
    /// O nome.
    pub nome: String,
    /// O que a fonte é (descrição, ou a pasta).
    pub detalhe: String,
    /// A última varredura concluída (ISO 8601 UTC).
    pub ultima: Option<String>,
    /// Quantos itens o inventário guarda dela.
    pub itens: i64,
}

#[derive(Debug, Clone)]
struct Ambiente {
    cofre: PathBuf,
    dados: PathBuf,
}

/// O estado da área.
#[derive(Default)]
pub struct Conexoes {
    amb: Option<Ambiente>,
    chaveiro: Option<Chaveiro>,
    coleta: Option<Arc<dyn ColetaDoTelegram>>,
    painel: Option<Painel>,
    pluggy: Vec<LinhaDeFonte>,
    pastas: Vec<LinhaDeFonte>,
    lido_em: Option<Instant>,
    fontes_lidas_em: Option<Instant>,
    coletando: Option<Receiver<Result<String, String>>>,
    aviso: Option<(String, bool)>,
    filtro: String,
}

impl std::fmt::Debug for Conexoes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conexoes")
            .field("ligada", &self.amb.is_some())
            .finish_non_exhaustive()
    }
}

fn so_leitura(cofre: &Path) -> Option<Connection> {
    let c = Connection::open_with_flags(
        cofre.join("inventario.sqlite"),
        OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()?;
    c.busy_timeout(Duration::from_secs(2)).ok()?;
    Some(c)
}

fn ultima_varredura(c: &Connection, fonte: &str) -> Option<String> {
    c.query_row(
        "SELECT max(concluida_em) FROM varredura WHERE fonte_id = ?1 AND status = 'concluida'",
        [fonte],
        |l| l.get::<_, Option<String>>(0),
    )
    .ok()
    .flatten()
}

/// As conexões do inventário (Pluggy e outras de banco): fonte, última varredura e registros.
#[must_use]
pub fn ler_conexoes(cofre: &Path) -> Vec<LinhaDeFonte> {
    let Some(c) = so_leitura(cofre) else {
        return Vec::new();
    };
    let Ok(mut q) =
        c.prepare("SELECT id, descricao FROM fonte WHERE tipo IN ('conexao', 'banco') ORDER BY id")
    else {
        return Vec::new();
    };
    let fontes: Vec<(String, String)> = q
        .query_map([], |l| Ok((l.get(0)?, l.get(1)?)))
        .map(|r| r.flatten().collect())
        .unwrap_or_default();
    fontes
        .into_iter()
        .map(|(id, descricao)| {
            let itens = c
                .query_row(
                    "SELECT count(*) FROM registro WHERE fonte_id = ?1",
                    [&id],
                    |l| l.get(0),
                )
                .unwrap_or(0);
            LinhaDeFonte {
                nome: id.clone(),
                detalhe: descricao,
                ultima: ultima_varredura(&c, &id),
                itens,
            }
        })
        .collect()
}

/// As pastas que o worker vigia (`worker.json`), com a última varredura e as origens.
#[must_use]
pub fn ler_pastas(dados: &Path, cofre: &Path) -> Vec<LinhaDeFonte> {
    let Ok(config) = sbf_worker::config::Config::carregar(&dados.join("worker.json")) else {
        return Vec::new();
    };
    let conexao = so_leitura(cofre);
    config
        .pastas
        .into_iter()
        .map(|p| {
            let (ultima, itens) = conexao.as_ref().map_or((None, 0), |c| {
                (
                    ultima_varredura(c, &p.fonte),
                    c.query_row(
                        "SELECT count(*) FROM origem WHERE fonte_id = ?1",
                        [&p.fonte],
                        |l| l.get(0),
                    )
                    .unwrap_or(0),
                )
            });
            LinhaDeFonte {
                nome: p.fonte,
                detalhe: p.raiz.to_string_lossy().into_owned(),
                ultima,
                itens,
            }
        })
        .collect()
}

/// `2026-11-18T10:03:55.123Z` vira `2026-11-18 10:03` (UTC).
fn data_curta(iso: &str) -> String {
    iso.get(..16)
        .map_or_else(|| iso.to_owned(), |s| s.replace('T', " "))
}

impl Conexoes {
    /// Liga a área ao inventário de verdade. `coleta` é o coletor do Telegram.
    pub fn ligar(
        &mut self,
        cofre: PathBuf,
        dados: PathBuf,
        chaveiro: Option<Chaveiro>,
        coleta: Arc<dyn ColetaDoTelegram>,
    ) {
        self.amb = Some(Ambiente { cofre, dados });
        self.chaveiro = chaveiro;
        self.coleta = Some(coleta);
    }

    fn reler(&mut self) {
        let Some(a) = self.amb.clone() else {
            return;
        };
        self.painel = Some(Painel::ler(&a.dados, self.chaveiro.as_ref()));
        self.lido_em = Some(Instant::now());
        if self
            .fontes_lidas_em
            .is_none_or(|t| t.elapsed() > Duration::from_secs(5))
        {
            self.pluggy = ler_conexoes(&a.cofre);
            self.pastas = ler_pastas(&a.dados, &a.cofre);
            self.fontes_lidas_em = Some(Instant::now());
        }
    }

    /// Segue ou deixa uma conversa (escreve `telegram.json`).
    pub fn alternar_seguida(&mut self, conversa: &Conversa, seguir: bool) {
        let Some(a) = &self.amb else {
            return;
        };
        let arquivo = a.dados.join(ARQUIVO_DA_CONFIG);
        let r = Config::carregar(&arquivo).and_then(|mut c| {
            if seguir {
                c.seguir(conversa);
            } else {
                c.deixar(&conversa.id);
            }
            c.salvar(&arquivo)
        });
        match r {
            Ok(()) => self.lido_em = None,
            Err(e) => self.aviso = Some((format!("Não deu para guardar a escolha: {e}"), true)),
        }
    }

    fn coletar_agora(&mut self) {
        let Some(c) = self.coleta.clone() else {
            return;
        };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let _ = tx.send(c.coletar());
        });
        self.coletando = Some(rx);
        self.aviso = None;
    }

    fn espiar(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.coletando {
            match rx.try_recv() {
                Ok(r) => {
                    self.coletando = None;
                    self.aviso = Some(match r {
                        Ok(t) => (format!("Coleta do Telegram: {t}"), false),
                        Err(e) => (format!("A coleta do Telegram não terminou: {e}"), true),
                    });
                    self.lido_em = None;
                    self.fontes_lidas_em = None;
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(300)),
                Err(TryRecvError::Disconnected) => self.coletando = None,
            }
        }
    }

    /// A área Conexões.
    pub fn area(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        componentes::titulo(ui, "Conexões", 20.0);
        if self.amb.is_none() {
            ui.add_space(6.0);
            ui.label(
                RichText::new(
                    "As conexões só existem no inventário de verdade (fora da demonstração).",
                )
                .color(p.tinta_suave),
            );
            return;
        }
        self.espiar(ui.ctx());
        if self
            .lido_em
            .is_none_or(|t| t.elapsed() > Duration::from_secs(1))
        {
            self.reler();
        }
        ui.ctx().request_repaint_after(Duration::from_millis(1000));
        ui.label(
            RichText::new("O que alimenta o inventário por conta própria. Aqui só o estado e as escolhas; os logins e as senhas ficam no terminal e no chaveiro.")
                .size(12.5)
                .color(p.tinta_suave),
        );
        ui.add_space(8.0);
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                if let Some((t, erro)) = &self.aviso {
                    if *erro {
                        componentes::aviso(ui, "error", t, p.perigo_sutil, p.borda, p.perigo);
                    } else {
                        componentes::aviso(ui, "help", t, p.info_sutil, p.info_borda, p.info);
                    }
                    ui.add_space(6.0);
                }
                self.telegram_ui(ui);
                ui.add_space(10.0);
                fontes_ui(
                    ui,
                    "Pluggy e bancos",
                    "account_balance",
                    &self.pluggy,
                    "Nenhuma conexão no inventário ainda.",
                );
                ui.add_space(10.0);
                fontes_ui(
                    ui,
                    "Pastas do worker",
                    "folder",
                    &self.pastas,
                    "O worker não vigia nenhuma pasta (Opções › Worker).",
                );
            });
    }

    fn telegram_ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        let Some(painel) = self.painel.clone() else {
            return;
        };
        let mut seguir_ou_deixar: Option<(Conversa, bool)> = None;
        let mut coletar = false;
        componentes::cartao(ui, |ui| {
            ui.horizontal(|ui| {
                componentes::titulo(ui, "Telegram", 16.0);
                let (fundo, cor) = match painel.situacao {
                    Situacao::Conectado => (p.sucesso_sutil, p.sucesso),
                    Situacao::PrecisaEntrar => (p.aviso_sutil, p.aviso),
                    Situacao::Desconhecida => (p.superficie_3, p.tinta_suave),
                };
                componentes::selo(ui, painel.situacao.rotulo(), fundo, cor);
                componentes::selo(ui, "só leitura", p.superficie_3, p.tinta_suave);
            });
            ui.label(
                RichText::new("A sua conta, como um aplicativo conectado. Só lê: baixa fotos, documentos e PDFs das conversas que você seguir para `entrada/telegram/` e os reconhece. Nunca envia, apaga nem marca como lido.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
            if painel.situacao == Situacao::PrecisaEntrar {
                ui.add_space(4.0);
                componentes::aviso(
                    ui,
                    "terminal",
                    "Rode `sbf telegram entrar` no terminal (número, código do app e, se houver, a senha de duas etapas).",
                    p.aviso_sutil,
                    p.aviso_borda,
                    p.aviso,
                );
            }
            ui.add_space(4.0);
            egui::Grid::new("telegram-resumo")
                .num_columns(2)
                .spacing([16.0, 4.0])
                .show(ui, |ui| {
                    let linha = |ui: &mut Ui, a: &str, b: String| {
                        ui.label(RichText::new(a).color(p.tinta_suave));
                        ui.label(b);
                        ui.end_row();
                    };
                    linha(ui, "Conversas seguidas", painel.seguidas.len().to_string());
                    linha(
                        ui,
                        "Última coleta",
                        painel
                            .ultima_coleta
                            .as_deref()
                            .map_or_else(|| "nunca".to_owned(), data_curta),
                    );
                    linha(ui, "Arquivos que entraram", painel.arquivos.to_string());
                    if let Some(e) = &painel.ultimo_erro {
                        linha(ui, "Último problema", e.clone());
                    }
                });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let pode = self.coletando.is_none()
                    && !painel.seguidas.is_empty()
                    && painel.situacao != Situacao::PrecisaEntrar;
                if ui
                    .add_enabled(pode, egui::Button::new("Coletar agora"))
                    .on_disabled_hover_text(
                        "Precisa de uma conversa seguida e da sessão (sbf telegram entrar).",
                    )
                    .clicked()
                {
                    coletar = true;
                }
                if self.coletando.is_some() {
                    ui.spinner();
                    ui.label(RichText::new("coletando…").color(p.tinta_suave));
                }
            });
            ui.add_space(6.0);
            componentes::rotulo_de_secao(ui, "Conversas");
            if painel.conhecidas.is_empty() && painel.seguidas.is_empty() {
                ui.label(
                    RichText::new("Rode `sbf telegram conversas` no terminal para listar as conversas da conta; elas aparecem aqui para você escolher.")
                        .color(p.tinta_suave),
                );
                return;
            }
            ui.add(
                egui::TextEdit::singleline(&mut self.filtro)
                    .hint_text("Filtrar pelo nome")
                    .desired_width(260.0),
            );
            let filtro = self.filtro.trim().to_lowercase();
            // As seguidas primeiro (mesmo as que a lista guardada não tem mais).
            let mut linhas: Vec<LinhaDaLista> = painel
                .seguidas
                .iter()
                .map(|s| {
                    (
                        s.conversa.clone(),
                        true,
                        Some((s.arquivos, s.ultima_coleta.clone())),
                    )
                })
                .collect();
            for c in &painel.conhecidas {
                if !linhas.iter().any(|(l, _, _)| l.id == c.id) {
                    linhas.push((c.clone(), false, None));
                }
            }
            for (c, seguida, uso) in linhas
                .into_iter()
                .filter(|(c, _, _)| filtro.is_empty() || c.nome.to_lowercase().contains(&filtro))
                .take(200)
            {
                ui.horizontal(|ui| {
                    let mut marcada = seguida;
                    if ui.checkbox(&mut marcada, "").changed() {
                        seguir_ou_deixar = Some((c.clone(), marcada));
                    }
                    ui.label(c.nome.clone());
                    componentes::selo(ui, c.tipo.rotulo(), p.superficie_3, p.tinta_suave);
                    if let Some((arquivos, ultima)) = uso {
                        ui.label(
                            RichText::new(format!(
                                "{arquivos} arquivo(s){}",
                                ultima
                                    .map(|u| format!(" · última coleta {}", data_curta(&u)))
                                    .unwrap_or_default()
                            ))
                            .size(12.0)
                            .color(p.tinta_suave),
                        );
                    }
                });
            }
        });
        if let Some((c, seguir)) = seguir_ou_deixar {
            self.alternar_seguida(&c, seguir);
        }
        if coletar {
            self.coletar_agora();
        }
    }
}

fn fontes_ui(ui: &mut Ui, titulo: &str, icone: &str, linhas: &[LinhaDeFonte], vazio: &str) {
    let p = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(crate::icones::icone_cor(icone, 18.0, p.tinta));
            componentes::titulo(ui, titulo, 16.0);
            componentes::selo(ui, "só estado", p.superficie_3, p.tinta_suave);
        });
        if linhas.is_empty() {
            ui.label(RichText::new(vazio).color(p.tinta_suave));
            return;
        }
        for l in linhas {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(&l.nome).strong());
                ui.label(RichText::new(&l.detalhe).color(p.tinta_suave));
                ui.label(
                    RichText::new(format!(
                        "· {} item(ns) · última varredura {}",
                        l.itens,
                        l.ultima
                            .as_deref()
                            .map_or_else(|| "nunca".to_owned(), data_curta)
                    ))
                    .size(12.0)
                    .color(p.tinta_suave),
                );
            });
        }
    });
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;
    use sbf_telegram::modelo::Tipo;

    const SQL_BASE: &str = include_str!("../../sbf-inventario/sql/0001-base-bun-v2.sql");

    fn conversa(id: &str) -> Conversa {
        Conversa {
            id: id.into(),
            nome: format!("Conversa {id}"),
            tipo: Tipo::Grupo,
        }
    }

    struct Falsa;
    impl ColetaDoTelegram for Falsa {
        fn coletar(&self) -> Result<String, String> {
            Ok("ok".into())
        }
    }

    #[test]
    fn data_curta_corta_em_minutos() {
        assert_eq!(data_curta("2026-11-18T10:03:55.123Z"), "2026-11-18 10:03");
        assert_eq!(data_curta("curta"), "curta");
    }

    #[test]
    fn seguir_e_deixar_escrevem_o_telegram_json() {
        let d = tempfile::tempdir().unwrap();
        let mut c = Conexoes::default();
        c.ligar(
            d.path().join("cofre"),
            d.path().to_path_buf(),
            None,
            Arc::new(Falsa),
        );
        c.alternar_seguida(&conversa("-1"), true);
        c.alternar_seguida(&conversa("-2"), true);
        c.alternar_seguida(&conversa("-1"), false);
        let cfg = Config::carregar(&d.path().join(ARQUIVO_DA_CONFIG)).unwrap();
        assert!(!cfg.segue("-1") && cfg.segue("-2"));
        assert!(c.aviso.is_none());
    }

    #[test]
    fn le_pluggy_e_pastas_so_do_que_existe() {
        let d = tempfile::tempdir().unwrap();
        let cofre = d.path().join("cofre");
        std::fs::create_dir_all(&cofre).unwrap();
        // Sem inventário nem worker.json: nada, sem erro.
        assert!(ler_conexoes(&cofre).is_empty());
        assert!(ler_pastas(d.path(), &cofre).is_empty());

        let conn = Connection::open(cofre.join("inventario.sqlite")).unwrap();
        conn.execute_batch(SQL_BASE).unwrap();
        conn.execute(
            "INSERT INTO fonte (id, tipo, descricao) VALUES ('conexao:pluggy', 'conexao', 'Pluggy (Open Finance)')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO fonte (id, tipo, descricao) VALUES ('disco:docs', 'disco', 'Documentos')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO varredura (fonte_id, etapa, escopo, responsavel, status, concluida_em)
             VALUES ('conexao:pluggy', 'E2', 'x', 'teste', 'concluida', '2026-11-18T09:00:00.000Z')",
            [],
        )
        .unwrap();
        drop(conn);
        let l = ler_conexoes(&cofre);
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].nome, "conexao:pluggy");
        assert_eq!(l[0].ultima.as_deref(), Some("2026-11-18T09:00:00.000Z"));
        assert_eq!(l[0].itens, 0);

        std::fs::write(
            d.path().join("worker.json"),
            r#"{"pastas": [{"fonte": "disco:docs", "raiz": "/tmp/docs"}]}"#,
        )
        .unwrap();
        let p = ler_pastas(d.path(), &cofre);
        assert_eq!(p.len(), 1);
        assert_eq!(p[0].detalhe, "/tmp/docs");
        assert_eq!(p[0].ultima, None);
    }
}
