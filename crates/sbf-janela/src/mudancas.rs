//! A janela percebe escritas de fora: o MCP escreve direto em `planejamento.sqlite` e
//! `regras.sqlite`, e o worker em `inventario.sqlite`, mas a janela só leria na abertura.
//!
//! `PRAGMA data_version` muda numa conexão quando **outra** conexão confirma uma escrita; é
//! barato (não lê dados) e não precisa de relógio nem de inotify. Cada banco tem a sua
//! conexão de leitura aberta o tempo todo (o valor só se compara com o da mesma conexão).
//!
//! A sondagem roda numa thread própria, a cada [`INTERVALO`]: a UI nunca espera por um
//! banco ocupado, e a thread pede um quadro (`request_repaint`) quando algo mudou, porque o
//! egui não redesenha sozinho com a janela parada.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use rusqlite::{Connection, OpenFlags};

/// De quanto em quanto tempo os bancos são conferidos.
pub const INTERVALO: Duration = Duration::from_secs(2);

/// Os bancos que a janela vigia.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Banco {
    Planejamento,
    Regras,
    Inventario,
}

impl Banco {
    pub const TODOS: [Banco; 3] = [Banco::Planejamento, Banco::Regras, Banco::Inventario];

    /// O nome do arquivo dentro do cofre.
    pub fn arquivo(self) -> &'static str {
        match self {
            Banco::Planejamento => "planejamento.sqlite",
            Banco::Regras => "regras.sqlite",
            Banco::Inventario => "inventario.sqlite",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Visto {
    /// Ainda não olhou.
    Nada,
    /// O arquivo não existia (ou não abriu) da última vez.
    Ausente,
    Versao(i64),
}

/// A sonda de um arquivo: lembra o `data_version` da última conferência.
#[derive(Debug)]
pub struct Sonda {
    caminho: PathBuf,
    conexao: Option<Connection>,
    visto: Visto,
}

impl Sonda {
    pub fn nova(caminho: impl Into<PathBuf>) -> Self {
        Self {
            caminho: caminho.into(),
            conexao: None,
            visto: Visto::Nada,
        }
    }

    pub fn caminho(&self) -> &Path {
        &self.caminho
    }

    fn abrir(&self) -> Option<Connection> {
        let c = Connection::open_with_flags(
            &self.caminho,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .ok()?;
        // Banco ocupado por uma escrita longa: desiste desta rodada em vez de esperar.
        c.busy_timeout(Duration::from_millis(100)).ok()?;
        Some(c)
    }

    /// Outra conexão escreveu desde a última conferência? A primeira só fixa o valor
    /// (a janela acabou de ler tudo) e devolve `false`; um arquivo que passa a existir conta
    /// como mudança. Erro ou banco ocupado: `false`, e tenta de novo na próxima.
    pub fn mudou(&mut self) -> bool {
        if self.conexao.is_none() {
            self.conexao = self.abrir();
        }
        let Some(conexao) = self.conexao.as_ref() else {
            self.visto = Visto::Ausente;
            return false;
        };
        let Ok(versao) = conexao.query_row("PRAGMA data_version", [], |l| l.get::<_, i64>(0))
        else {
            // A conexão pode ter ficado ruim (arquivo trocado): reabre na próxima.
            self.conexao = None;
            return false;
        };
        let antes = std::mem::replace(&mut self.visto, Visto::Versao(versao));
        match antes {
            Visto::Nada => false,
            Visto::Ausente => true,
            Visto::Versao(v) => v != versao,
        }
    }
}

struct Compartilhado {
    geracoes: [AtomicU64; 3],
    parar: AtomicBool,
}

/// A thread que confere os bancos. Parar é largar o valor.
pub struct Vigia {
    compartilhado: Arc<Compartilhado>,
}

impl std::fmt::Debug for Vigia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vigia").finish_non_exhaustive()
    }
}

fn posicao(b: Banco) -> usize {
    match b {
        Banco::Planejamento => 0,
        Banco::Regras => 1,
        Banco::Inventario => 2,
    }
}

impl Vigia {
    /// Começa a conferir. `ao_mudar` roda na thread quando algum banco mudou (a janela pede
    /// um quadro ali). Cada banco ganha uma conexão de leitura própria, aberta na hora.
    pub fn iniciar(
        bancos: Vec<(Banco, PathBuf)>,
        intervalo: Duration,
        ao_mudar: impl Fn() + Send + 'static,
    ) -> Vigia {
        let compartilhado = Arc::new(Compartilhado {
            geracoes: [AtomicU64::new(0), AtomicU64::new(0), AtomicU64::new(0)],
            parar: AtomicBool::new(false),
        });
        let fraco: Weak<Compartilhado> = Arc::downgrade(&compartilhado);
        let mut sondas: Vec<(Banco, Sonda)> = bancos
            .into_iter()
            .map(|(b, c)| (b, Sonda::nova(c)))
            .collect();
        // O valor de partida é o de agora: quem abriu a janela acabou de ler.
        for (_, s) in &mut sondas {
            s.mudou();
        }
        let _ = std::thread::Builder::new()
            .name("sbf-vigia-dos-bancos".to_owned())
            .spawn(move || {
                loop {
                    std::thread::sleep(intervalo);
                    let Some(c) = fraco.upgrade() else { return };
                    if c.parar.load(Ordering::Relaxed) {
                        return;
                    }
                    let mut algum = false;
                    for (banco, sonda) in &mut sondas {
                        if sonda.mudou() {
                            c.geracoes[posicao(*banco)].fetch_add(1, Ordering::Relaxed);
                            algum = true;
                        }
                    }
                    if algum {
                        ao_mudar();
                    }
                }
            });
        Vigia { compartilhado }
    }

    /// Quantas vezes o banco mudou por fora desde que a vigia começou.
    pub fn geracao(&self, banco: Banco) -> u64 {
        self.compartilhado.geracoes[posicao(banco)].load(Ordering::Relaxed)
    }
}

impl Drop for Vigia {
    fn drop(&mut self) {
        self.compartilhado.parar.store(true, Ordering::Relaxed);
    }
}

/// O que a janela guarda: a vigia (quando já começou) e o que ela já viu de cada banco.
#[derive(Debug)]
pub struct Mudancas {
    a_vigiar: Vec<(Banco, PathBuf)>,
    intervalo: Duration,
    vigia: Option<Vigia>,
    vistas: [u64; 3],
}

impl Default for Mudancas {
    fn default() -> Self {
        Self {
            a_vigiar: Vec::new(),
            intervalo: INTERVALO,
            vigia: None,
            vistas: [0; 3],
        }
    }
}

impl Mudancas {
    /// Outro intervalo (os testes usam um curto).
    #[must_use]
    pub fn com_intervalo(mut self, intervalo: Duration) -> Self {
        self.intervalo = intervalo;
        self
    }

    /// Registra o que vigiar (depois de ligar o cofre). A thread só começa no primeiro
    /// quadro, quando já há contexto para pedir o redesenho.
    pub fn vigiar(&mut self, bancos: Vec<(Banco, PathBuf)>) {
        self.a_vigiar = bancos;
        self.vigia = None;
        self.vistas = [0; 3];
    }

    /// Os bancos de um cofre que existem (um que ainda não existe é criado depois e não é
    /// vigiado até a próxima abertura).
    pub fn bancos_do_cofre(cofre: &Path) -> Vec<(Banco, PathBuf)> {
        Banco::TODOS
            .into_iter()
            .map(|b| (b, cofre.join(b.arquivo())))
            .filter(|(_, c)| c.is_file())
            .collect()
    }

    /// Chamado a cada quadro: barato (lê contadores). Devolve os bancos que mudaram desde
    /// a última chamada.
    pub fn mudaram(&mut self, ctx: &egui::Context) -> Vec<Banco> {
        if self.vigia.is_none() && !self.a_vigiar.is_empty() {
            let ctx = ctx.clone();
            let bancos = std::mem::take(&mut self.a_vigiar);
            self.vigia = Some(Vigia::iniciar(bancos, self.intervalo, move || {
                ctx.request_repaint();
            }));
        }
        let Some(v) = &self.vigia else {
            return Vec::new();
        };
        let mut mudou = Vec::new();
        for b in Banco::TODOS {
            let g = v.geracao(b);
            let i = posicao(b);
            if g != self.vistas[i] {
                self.vistas[i] = g;
                mudou.push(b);
            }
        }
        mudou
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn banco_de_teste() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().expect("pasta");
        let caminho = dir.path().join("planejamento.sqlite");
        let c = Connection::open(&caminho).expect("banco");
        c.execute_batch("CREATE TABLE t (n INTEGER); INSERT INTO t VALUES (1);")
            .expect("esquema");
        (dir, caminho)
    }

    #[test]
    fn a_sonda_relata_mudou_quando_outra_conexao_escreve() {
        let (_dir, caminho) = banco_de_teste();
        let mut sonda = Sonda::nova(&caminho);
        // A primeira conferência só fixa o valor.
        assert!(!sonda.mudou());
        assert!(!sonda.mudou());
        let outra = Connection::open(&caminho).expect("outra conexão");
        outra
            .execute("INSERT INTO t VALUES (2)", [])
            .expect("escrita de fora");
        assert!(sonda.mudou());
        // E só uma vez por escrita.
        assert!(!sonda.mudou());
    }

    #[test]
    fn um_banco_que_passa_a_existir_conta_como_mudanca() {
        let dir = tempfile::tempdir().expect("pasta");
        let caminho = dir.path().join("regras.sqlite");
        let mut sonda = Sonda::nova(&caminho);
        assert!(!sonda.mudou());
        assert!(!sonda.mudou());
        Connection::open(&caminho)
            .expect("criar")
            .execute_batch("CREATE TABLE t (n INTEGER);")
            .expect("esquema");
        assert!(sonda.mudou());
        assert!(!sonda.mudou());
    }

    #[test]
    fn a_vigia_em_segundo_plano_conta_a_escrita_de_fora_e_pede_quadro() {
        let (_dir, caminho) = banco_de_teste();
        let pedidos = Arc::new(AtomicU64::new(0));
        let contagem = pedidos.clone();
        let vigia = Vigia::iniciar(
            vec![(Banco::Planejamento, caminho.clone())],
            Duration::from_millis(20),
            move || {
                contagem.fetch_add(1, Ordering::Relaxed);
            },
        );
        assert_eq!(vigia.geracao(Banco::Planejamento), 0);
        Connection::open(&caminho)
            .expect("outra conexão")
            .execute("INSERT INTO t VALUES (2)", [])
            .expect("escrita de fora");
        let limite = std::time::Instant::now() + Duration::from_secs(5);
        while vigia.geracao(Banco::Planejamento) == 0 && std::time::Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(vigia.geracao(Banco::Planejamento), 1);
        assert_eq!(vigia.geracao(Banco::Regras), 0);
        assert!(pedidos.load(Ordering::Relaxed) >= 1);
    }

    #[test]
    fn mudancas_devolve_so_o_que_mudou_e_so_uma_vez() {
        let (dir, caminho) = banco_de_teste();
        let regras = dir.path().join("regras.sqlite");
        Connection::open(&regras)
            .expect("regras")
            .execute_batch("CREATE TABLE t (n INTEGER);")
            .expect("esquema");
        let ctx = egui::Context::default();
        let mut m = Mudancas::default().com_intervalo(Duration::from_millis(20));
        m.vigiar(vec![
            (Banco::Planejamento, caminho.clone()),
            (Banco::Regras, regras),
        ]);
        assert!(m.mudaram(&ctx).is_empty());
        Connection::open(&caminho)
            .expect("outra conexão")
            .execute("INSERT INTO t VALUES (3)", [])
            .expect("escrita de fora");
        let limite = std::time::Instant::now() + Duration::from_secs(5);
        let mut vistos = Vec::new();
        while vistos.is_empty() && std::time::Instant::now() < limite {
            std::thread::sleep(Duration::from_millis(20));
            vistos = m.mudaram(&ctx);
        }
        assert_eq!(vistos, vec![Banco::Planejamento]);
        assert!(m.mudaram(&ctx).is_empty());
    }
}
