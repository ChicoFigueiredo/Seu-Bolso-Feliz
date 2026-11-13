//! As Ferramentas do inventário real (aba Ferramentas, aba Achado e bastidores): PDF
//! protegido, regras de senha, estante, cópia de segurança, chaveiro e abrir o original.
//!
//! Tudo que é lento (rodada de PDF, rclone, Argon2id do chaveiro) roda numa thread com a
//! **própria** conexão: a janela só recebe o andamento e o resultado por canal e nunca
//! fica "não respondendo". Nenhum segredo passa por log, banco ou tela: a senha-mestra e a
//! senha da cópia vivem num `Segredo` e o campo de texto é limpo assim que a tarefa começa.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel};
use std::time::{Duration, Instant};

use egui::{Align, Context, Layout, RichText, Ui};
use sbf_chaveiro::{Chaveiro, OfertaDeApagar, ResumoDaImportacao, Segredo, Situacao};
use sbf_estante::{
    AVISO_DA_SENHA, CopiaDeSeguranca, Estante, Fase, Rclone, RelatorioDaCopia, RelatorioDaEstante,
};
use sbf_inventario::banco::Inventario;
use sbf_inventario::consulta::Tabela;
use sbf_inventario::leitor::LeitorSqlite;
use sbf_pdf::{OpcoesDaRodada, PropostaDeRegra, RelatorioDaRodada, Situacao as SituacaoDoPdf};

use crate::componentes;
use crate::navegacao::Pedido;
use crate::pagina::Vista;
use crate::tabela;
use crate::tema;

/// O rótulo da senha da cópia de segurança no chaveiro (`senha:copia-de-seguranca`).
pub const ROTULO_DA_SENHA_DA_COPIA: &str = "copia-de-seguranca";

/// A mensagem de um erro da estante ou da cópia, em português e com o que fazer.
pub fn mensagem_da_copia(e: &sbf_estante::Erro) -> String {
    use sbf_estante::Erro as E;
    match e {
        E::SenhaCurta { minimo } => format!(
            "A senha da cópia precisa de pelo menos {minimo} caracteres. Use uma mais longa (ou deixe o app gerar uma)."
        ),
        E::DestinoSemSal => "O destino da cópia já tem arquivos de outra origem e não tem o sal-da-copia.txt desta cópia. Escolha uma pasta vazia em Opções.".to_owned(),
        E::SalInvalido => "O sal-da-copia.txt no destino da cópia está adulterado. Nada foi copiado: confira a pasta do destino.".to_owned(),
        E::TempoEsgotado { comando } => format!(
            "O rclone ({comando}) passou do limite de tempo. Confira a conexão com o Drive e tente de novo."
        ),
        E::Aleatorio => "O sistema não entregou números aleatórios para a senha ou o sal. Tente de novo; se repetir, reinicie o computador.".to_owned(),
        outro => format!("A cópia não foi feita: {outro}"),
    }
}

/// O motivo da estante e da cópia sem destino.
pub const MOTIVO_SEM_DESTINO: &str = "Configure o destino em Opções";

enum Evento {
    Andamento(String),
    Pdf(Result<RelatorioDaRodada, String>),
    Estante(Result<RelatorioDaEstante, String>),
    Copia(Result<RelatorioDaCopia, String>),
    Importacao(Result<(ResumoDaImportacao, Option<OfertaDeApagar>, String), String>),
    Destrancar(Result<(), String>),
}

struct Tarefa {
    titulo: String,
    andamento: String,
    inicio: Instant,
    rx: Receiver<Evento>,
}

/// O que a janela de Ferramentas mostra quando nenhuma tarefa está rodando.
enum Resultado {
    Pdf {
        relatorio: RelatorioDaRodada,
        propostas: Vec<PropostaDeRegra>,
    },
    Regras(Arc<Tabela>),
    Estante(RelatorioDaEstante),
    Copia(RelatorioDaCopia),
    /// Pede a senha da cópia antes de começar (escolhida pelo usuário, sem chaveiro).
    PedirSenhaDaCopia,
    /// A senha nova da cópia, gerada e guardada no chaveiro, mostrada **uma vez**.
    SenhaNovaDaCopia(Segredo),
    Erro(String),
}

/// As Ferramentas ligadas ao inventário real.
pub struct Ferramentas {
    /// O `inventario.sqlite`.
    pub banco: PathBuf,
    /// A pasta do cofre (o `caminho_acervo` dos achados é relativo a ela).
    pub cofre: PathBuf,
    pub chaveiro: Option<Chaveiro>,
    /// Por que o chaveiro não abriu (a barra mostra "—").
    pub erro_do_chaveiro: Option<String>,
    /// Destino do rclone para a estante (Opções). Vazio: Ação desabilitada.
    pub destino_da_estante: String,
    /// Destino do rclone para a cópia de segurança (Opções). Vazio: Ação desabilitada.
    pub destino_da_copia: String,
    /// "Tentar estratégias padrão" na rodada de PDF (desligada por padrão).
    pub tentar_estrategias_padrao: bool,
    /// Instante da última cópia feita nesta sessão.
    pub ultima_copia: Option<String>,
    tarefa: Option<Tarefa>,
    resultado: Option<Resultado>,
    vista: Vista,
    // Campos dos bastidores e da cópia: limpos assim que a tarefa começa.
    senha_mestra: String,
    caminho_do_cofre_enc: String,
    senha_da_copia: String,
    resumo_do_chaveiro: Option<String>,
    oferta: Option<OfertaDeApagar>,
}

impl std::fmt::Debug for Ferramentas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ferramentas")
            .field("banco", &self.banco)
            .field("cofre", &self.cofre)
            .finish_non_exhaustive()
    }
}

/// Abre o chaveiro da pasta padrão sem travar a janela: se o keyring do sistema não
/// responder em `espera`, desiste (a barra mostra "—").
pub fn abrir_chaveiro_com_espera(espera: Duration) -> Result<Chaveiro, String> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        let _ = tx.send(Chaveiro::abrir_na_pasta_padrao().map_err(|e| e.to_string()));
    });
    rx.recv_timeout(espera)
        .map_err(|_| "o keyring do sistema não respondeu".to_owned())?
}

/// O estado do chaveiro em palavras (nunca um valor).
pub fn texto_da_situacao(s: Situacao) -> &'static str {
    match s {
        Situacao::ChaveiroDoSistema => "no keyring do sistema",
        Situacao::ArquivoTrancado => "trancado",
        Situacao::ArquivoAberto => "aberto",
        Situacao::Indisponivel => "indisponível",
    }
}

impl Ferramentas {
    pub fn nova(banco: PathBuf, cofre: PathBuf) -> Self {
        Self {
            banco,
            cofre,
            chaveiro: None,
            erro_do_chaveiro: None,
            destino_da_estante: String::new(),
            destino_da_copia: String::new(),
            tentar_estrategias_padrao: false,
            ultima_copia: None,
            tarefa: None,
            resultado: None,
            vista: Vista::default(),
            senha_mestra: String::new(),
            caminho_do_cofre_enc: String::new(),
            senha_da_copia: String::new(),
            resumo_do_chaveiro: None,
            oferta: None,
        }
    }

    /// O texto do chaveiro para a barra de status.
    pub fn texto_do_chaveiro(&self) -> Option<String> {
        self.chaveiro
            .as_ref()
            .map(|c| texto_da_situacao(c.situacao()).to_owned())
    }

    /// O que a janela de Ferramentas mostra, em texto (testes e acessibilidade).
    pub fn resumo(&self) -> Option<String> {
        if let Some(t) = &self.tarefa {
            return Some(format!("{}: {}", t.titulo, t.andamento));
        }
        Some(match self.resultado.as_ref()? {
            Resultado::Pdf {
                relatorio,
                propostas,
            } => format!(
                "pdf: {} protegidos, {} propostas",
                relatorio.resultados.len(),
                propostas.len()
            ),
            Resultado::Regras(t) => format!("regras: {} linhas", t.linhas.len()),
            Resultado::Estante(r) => format!("estante: {} enviados", r.enviados),
            Resultado::Copia(r) => format!("cópia: {}", r.quando),
            Resultado::PedirSenhaDaCopia => "cópia: pedindo a senha".to_owned(),
            Resultado::SenhaNovaDaCopia(_) => "cópia: senha nova mostrada uma vez".to_owned(),
            Resultado::Erro(e) => format!("erro: {e}"),
        })
    }

    /// Espera a tarefa em andamento terminar (testes).
    pub fn esperar(&mut self, ctx: &Context, limite: Duration) -> bool {
        let inicio = Instant::now();
        while self.tarefa.is_some() && inicio.elapsed() < limite {
            self.sondar(ctx);
            std::thread::sleep(Duration::from_millis(20));
        }
        self.tarefa.is_none()
    }

    pub fn ocupada(&self) -> bool {
        self.tarefa.is_some()
    }

    fn comecar<F>(&mut self, titulo: &str, trabalho: F)
    where
        F: FnOnce(&(dyn Fn(String) + Sync)) -> Evento + Send + 'static,
    {
        let (tx, rx) = channel();
        let tx2 = tx.clone();
        std::thread::spawn(move || {
            let andamento = move |t: String| {
                let _ = tx2.send(Evento::Andamento(t));
            };
            let fim = trabalho(&andamento);
            let _ = tx.send(fim);
        });
        self.resultado = None;
        self.tarefa = Some(Tarefa {
            titulo: titulo.to_owned(),
            andamento: "começando…".to_owned(),
            inicio: Instant::now(),
            rx,
        });
    }

    /// Ferramentas › Abrir PDF protegido: a rodada numa thread, com conexão própria.
    pub fn abrir_pdf_protegido(&mut self) {
        let Some(chaveiro) = self.chaveiro.clone() else {
            self.resultado = Some(Resultado::Erro(format!(
                "O chaveiro não abriu: {}",
                self.erro_do_chaveiro.clone().unwrap_or_default()
            )));
            return;
        };
        let (banco, cofre) = (self.banco.clone(), self.cofre.clone());
        let opcoes = OpcoesDaRodada {
            tentar_estrategias_padrao: self.tentar_estrategias_padrao,
            ..OpcoesDaRodada::default()
        };
        self.comecar("Abrir PDF protegido", move |andamento| {
            andamento("procurando os PDFs protegidos…".to_owned());
            let r = Inventario::abrir(&banco)
                .map_err(|e| e.to_string())
                .and_then(|inv| {
                    andamento("tentando as regras de senha…".to_owned());
                    sbf_pdf::abrir_protegidos(inv.conexao(), &cofre, &chaveiro, &opcoes)
                        .map_err(|e| e.to_string())
                });
            Evento::Pdf(r)
        });
    }

    /// Ferramentas › Regras de senha: só leitura, sem nenhuma senha.
    pub fn regras_de_senha(&mut self) {
        self.resultado = Some(
            match LeitorSqlite::abrir(&self.banco).and_then(|l| l.regras_de_senha()) {
                Ok(t) => Resultado::Regras(Arc::new(t)),
                Err(e) => Resultado::Erro(e.to_string()),
            },
        );
    }

    /// Ferramentas › Atualizar estante.
    pub fn atualizar_estante(&mut self) {
        let destino = self.destino_da_estante.trim().to_owned();
        if destino.is_empty() {
            self.resultado = Some(Resultado::Erro(MOTIVO_SEM_DESTINO.to_owned()));
            return;
        }
        let (banco, cofre) = (self.banco.clone(), self.cofre.clone());
        self.comecar("Atualizar estante", move |andamento| {
            let r = (|| -> Result<RelatorioDaEstante, String> {
                let inv = Inventario::abrir(&banco).map_err(|e| e.to_string())?;
                let estante =
                    Estante::nova(Rclone::novo(), destino).map_err(|e| mensagem_da_copia(&e))?;
                let progresso = |a: sbf_estante::Andamento| {
                    let fase = match a.fase {
                        Fase::Planejando => "planejando",
                        Fase::Renomeando => "renomeando",
                        Fase::Enviando => "enviando",
                        Fase::Apagando => "apagando",
                    };
                    andamento(format!("{fase}: {} de {}", a.feitos, a.total));
                };
                let resultado = estante.atualizar(inv.conexao(), &cofre, &progresso);
                resultado.map_err(|e| mensagem_da_copia(&e))
            })();
            Evento::Estante(r)
        });
    }

    /// Ferramentas › Fazer cópia de segurança.
    ///
    /// Com a senha da cópia no chaveiro (`senha:copia-de-seguranca`), copia direto. Sem
    /// ela, gera uma (150 bits), guarda no chaveiro e mostra **uma vez**, com o aviso; a
    /// cópia só começa depois do "Copiei a senha e guardei fora do computador". Sem
    /// chaveiro, pede uma senha escolhida (16 caracteres no mínimo).
    pub fn pedir_copia(&mut self) {
        if self.destino_da_copia.trim().is_empty() {
            self.resultado = Some(Resultado::Erro(MOTIVO_SEM_DESTINO.to_owned()));
            return;
        }
        let Some(chaveiro) = self.chaveiro.clone() else {
            self.resultado = Some(Resultado::PedirSenhaDaCopia);
            return;
        };
        if let Ok(senha) = chaveiro.senha(ROTULO_DA_SENHA_DA_COPIA) {
            self.fazer_copia(senha);
            return;
        }
        self.resultado = Some(
            match sbf_estante::copia::gerar_senha_da_copia()
                .map_err(|e| mensagem_da_copia(&e))
                .and_then(|senha| {
                    chaveiro
                        .guardar_senha(ROTULO_DA_SENHA_DA_COPIA, senha.clone())
                        .map(|()| senha)
                        .map_err(|e| {
                            format!("Não deu para guardar a senha da cópia no chaveiro: {e}")
                        })
                }) {
                Ok(senha) => Resultado::SenhaNovaDaCopia(senha),
                Err(e) => Resultado::Erro(e),
            },
        );
    }

    fn fazer_copia(&mut self, senha: Segredo) {
        let destino = self.destino_da_copia.trim().to_owned();
        let (banco, cofre) = (self.banco.clone(), self.cofre.clone());
        self.comecar("Fazer cópia de segurança", move |andamento| {
            andamento("copiando o banco e o acervo, cifrados…".to_owned());
            let r = (|| -> Result<RelatorioDaCopia, String> {
                let inv = Inventario::abrir(&banco).map_err(|e| e.to_string())?;
                let copia = CopiaDeSeguranca::nova(Rclone::novo(), destino)
                    .map_err(|e| mensagem_da_copia(&e))?;
                copia
                    .fazer(inv.conexao(), &cofre, &senha)
                    .map_err(|e| mensagem_da_copia(&e))
            })();
            Evento::Copia(r)
        });
    }

    fn importar_cofre_enc(&mut self) {
        let Some(chaveiro) = self.chaveiro.clone() else {
            return;
        };
        let senha = Segredo::novo(std::mem::take(&mut self.senha_mestra));
        let caminho = PathBuf::from(self.caminho_do_cofre_enc.trim());
        self.oferta = None;
        self.comecar("Importar cofre.enc", move |andamento| {
            andamento("lendo o cofre.enc (pode levar alguns segundos)…".to_owned());
            let r = chaveiro
                .importar(&caminho, &senha)
                .map_err(|e| e.to_string())
                .map(|resumo| {
                    andamento("conferindo o que foi guardado…".to_owned());
                    match chaveiro.conferir_importacao(&caminho, &senha) {
                        Ok(oferta) => (resumo, Some(oferta), String::new()),
                        Err(e) => (
                            resumo,
                            None,
                            format!("A conferência não passou ({e}); o arquivo antigo fica."),
                        ),
                    }
                });
            Evento::Importacao(r)
        });
    }

    fn destrancar(&mut self) {
        let Some(chaveiro) = self.chaveiro.clone() else {
            return;
        };
        let senha = Segredo::novo(std::mem::take(&mut self.senha_mestra));
        self.comecar("Destrancar o chaveiro", move |andamento| {
            andamento("conferindo a senha-mestra…".to_owned());
            Evento::Destrancar(chaveiro.destrancar(&senha).map_err(|e| e.to_string()))
        });
    }

    /// O caminho do original no cofre (`achado.caminho_acervo`), lido com uma conexão só
    /// de leitura. Sugestão de costura: expor o campo no `Item` do achado.
    pub fn caminho_do_original(&self, hash: &str) -> Result<PathBuf, String> {
        let conn = sbf_inventario::banco::abrir_leitura(&self.banco).map_err(|e| e.to_string())?;
        let relativo: Option<String> = conn
            .query_row(
                "SELECT caminho_acervo FROM achado WHERE content_hash = ?1",
                [hash],
                |l| l.get(0),
            )
            .map_err(|e| e.to_string())?;
        let relativo = relativo.ok_or("este achado ainda não tem cópia no acervo")?;
        let caminho = self.cofre.join(relativo);
        if caminho.is_file() {
            Ok(caminho)
        } else {
            Err(format!(
                "o arquivo não está no cofre: {}",
                caminho.display()
            ))
        }
    }

    /// Aba Achado › Abrir original.
    pub fn abrir_original(&self, hash: &str) -> Result<(), String> {
        let caminho = self.caminho_do_original(hash)?;
        abrir_no_sistema(&caminho, false)
    }

    /// Aba Achado › Mostrar no cofre.
    pub fn mostrar_no_cofre(&self, hash: &str) -> Result<(), String> {
        let caminho = self.caminho_do_original(hash)?;
        abrir_no_sistema(&caminho, true)
    }

    /// Lê os eventos da tarefa. Devolve `true` quando ela terminou e o banco pode ter
    /// mudado (a janela relê).
    pub fn sondar(&mut self, ctx: &Context) -> bool {
        let Some(tarefa) = &mut self.tarefa else {
            return false;
        };
        ctx.request_repaint_after(Duration::from_millis(150));
        let mut fim = None;
        while let Ok(e) = tarefa.rx.try_recv() {
            match e {
                Evento::Andamento(t) => tarefa.andamento = t,
                outro => fim = Some(outro),
            }
        }
        let Some(fim) = fim else { return false };
        self.tarefa = None;
        self.resultado = match fim {
            Evento::Pdf(Ok(relatorio)) => {
                let propostas = relatorio.propostas().cloned().collect();
                Some(Resultado::Pdf {
                    relatorio,
                    propostas,
                })
            }
            Evento::Estante(Ok(r)) => Some(Resultado::Estante(r)),
            Evento::Copia(Ok(r)) => {
                self.ultima_copia = Some(r.quando.clone());
                Some(Resultado::Copia(r))
            }
            Evento::Importacao(Ok((resumo, oferta, aviso))) => {
                let mut t = format!(
                    "Importado: CPF {}, {} senha{} ({}), guardado {}.",
                    if resumo.cpf_valido {
                        "válido"
                    } else {
                        "não conferiu os dígitos"
                    },
                    resumo.rotulos.len(),
                    if resumo.rotulos.len() == 1 { "" } else { "s" },
                    resumo.rotulos.join(", "),
                    texto_da_situacao(resumo.onde)
                );
                if !aviso.is_empty() {
                    t.push(' ');
                    t.push_str(&aviso);
                }
                self.resumo_do_chaveiro = Some(t);
                self.oferta = oferta;
                None
            }
            Evento::Destrancar(Ok(())) => {
                self.resumo_do_chaveiro = Some("Chaveiro destrancado nesta sessão.".to_owned());
                None
            }
            Evento::Pdf(Err(e)) | Evento::Estante(Err(e)) | Evento::Copia(Err(e)) => {
                Some(Resultado::Erro(e))
            }
            Evento::Importacao(Err(e)) | Evento::Destrancar(Err(e)) => {
                self.resumo_do_chaveiro = Some(format!("Não deu: {e}"));
                None
            }
            Evento::Andamento(_) => None,
        };
        true
    }

    /// A janela de Ferramentas (andamento e resultado). Propostas aceitas viram
    /// `Pedido::Decidir`, gravadas pelos `Comandos` como qualquer decisão.
    pub fn janela_ui(&mut self, ctx: &Context, pedidos: &mut Vec<Pedido>) {
        if self.tarefa.is_none() && self.resultado.is_none() {
            return;
        }
        let p = tema::paleta(ctx);
        let mut fechar = false;
        let mut copiar = false;
        let mut confirmou_senha: Option<Segredo> = None;
        let titulo = self
            .tarefa
            .as_ref()
            .map(|t| t.titulo.clone())
            .unwrap_or_else(|| match &self.resultado {
                Some(Resultado::Pdf { .. }) => "Abrir PDF protegido".to_owned(),
                Some(Resultado::Regras(_)) => "Regras de senha".to_owned(),
                Some(Resultado::Estante(_)) => "Atualizar estante".to_owned(),
                Some(
                    Resultado::Copia(_)
                    | Resultado::PedirSenhaDaCopia
                    | Resultado::SenhaNovaDaCopia(_),
                ) => "Fazer cópia de segurança".to_owned(),
                _ => "Ferramentas".to_owned(),
            });
        egui::Window::new(RichText::new(titulo).strong())
            .id(egui::Id::new("sbf-ferramentas"))
            .default_size([620.0, 420.0])
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                if let Some(t) = &self.tarefa {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        ui.label(&t.andamento);
                    });
                    ui.label(
                        RichText::new(format!("{} s", t.inicio.elapsed().as_secs()))
                            .size(12.0)
                            .color(p.tinta_suave),
                    );
                    return;
                }
                match &mut self.resultado {
                    Some(Resultado::Pdf { relatorio, propostas }) => {
                        resumo_do_pdf(ui, relatorio);
                        let mut aceitas = Vec::new();
                        let mut recusadas = Vec::new();
                        for (i, proposta) in propostas.iter().enumerate() {
                            ui.separator();
                            ui.label(RichText::new(&proposta.frase).strong());
                            ui.horizontal(|ui| {
                                if ui.button("Guardar como regra").clicked() {
                                    aceitas.push(i);
                                }
                                if ui.button("Agora não").clicked() {
                                    recusadas.push(i);
                                }
                            });
                        }
                        for i in aceitas.iter().rev() {
                            pedidos.push(Pedido::Decidir(propostas[*i].pedido()));
                            propostas.remove(*i);
                        }
                        for i in recusadas.iter().rev() {
                            propostas.remove(*i);
                        }
                    }
                    Some(Resultado::Regras(t)) => {
                        if ui
                            .button((crate::icones::icone_cor("add", 15.0, p.tinta), "Nova regra"))
                            .clicked()
                        {
                            pedidos.push(Pedido::Executar(crate::acoes::IdAcao::NovaRegraDeSenha));
                        }
                        ui.label(
                            RichText::new("As regras dizem como montar a senha; a senha mesmo fica no chaveiro e nunca aparece aqui.")
                                .color(p.tinta_suave),
                        );
                        let mut estado = self.vista.tabelas.remove("regras").unwrap_or_default();
                        tabela::desenhar(ui, "regras-de-senha", t, &mut estado, &mut self.vista.selecao, pedidos, Some(300.0));
                        self.vista.tabelas.insert("regras".to_owned(), estado);
                    }
                    Some(Resultado::Estante(r)) => {
                        ui.label(format!(
                            "Enviados {}, renomeados {}, apagados {}, já certos {}, sem original no acervo {}.",
                            r.enviados, r.renomeados, r.apagados, r.intactos, r.sem_copia
                        ));
                        for f in &r.falhas {
                            ui.colored_label(p.perigo, f);
                        }
                    }
                    Some(Resultado::Copia(r)) => {
                        ui.label(format!(
                            "Cópia feita em {} ({} arquivos do acervo). Banco: {}.",
                            r.quando, r.arquivos_do_acervo, r.banco
                        ));
                    }
                    Some(Resultado::SenhaNovaDaCopia(senha)) => {
                        componentes::aviso(ui, "warning", AVISO_DA_SENHA, p.aviso_sutil, p.aviso_borda, p.aviso);
                        ui.label("A senha da cópia de segurança (aparece só agora):");
                        let mut texto = senha.expor().to_owned();
                        ui.add(
                            egui::TextEdit::singleline(&mut texto)
                                .font(egui::TextStyle::Monospace)
                                .desired_width(f32::INFINITY),
                        );
                        texto.clear();
                        ui.label(
                            RichText::new("Ela também ficou guardada no chaveiro, para as próximas cópias.")
                                .size(12.0)
                                .color(p.tinta_suave),
                        );
                        if ui.button("Copiei a senha e guardei fora do computador").clicked() {
                            confirmou_senha = Some(senha.clone());
                        }
                    }
                    Some(Resultado::PedirSenhaDaCopia) => {
                        componentes::aviso(ui, "warning", AVISO_DA_SENHA, p.aviso_sutil, p.aviso_borda, p.aviso);
                        ui.label("Senha da cópia de segurança");
                        ui.add(egui::TextEdit::singleline(&mut self.senha_da_copia).password(true));
                        if ui
                            .add_enabled(!self.senha_da_copia.is_empty(), egui::Button::new("Fazer a cópia"))
                            .clicked()
                        {
                            copiar = true;
                        }
                    }
                    Some(Resultado::Erro(e)) => {
                        componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
                    }
                    None => {}
                }
                ui.add_space(8.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Fechar").clicked() {
                        fechar = true;
                    }
                });
            });
        if copiar {
            let senha = Segredo::novo(std::mem::take(&mut self.senha_da_copia));
            self.fazer_copia(senha);
        }
        if let Some(senha) = confirmou_senha {
            self.fazer_copia(senha);
        }
        if fechar {
            self.resultado = None;
            self.senha_da_copia.clear();
        }
    }

    /// Bastidores › Chaveiro.
    pub fn chaveiro_ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        let Some(chaveiro) = self.chaveiro.clone() else {
            ui.label(format!(
                "O chaveiro não abriu: {}",
                self.erro_do_chaveiro.clone().unwrap_or_else(|| "—".into())
            ));
            return;
        };
        let situacao = chaveiro.situacao();
        ui.label(format!("Estado: {}", texto_da_situacao(situacao)));
        ui.label(format!(
            "CPF guardado: {}",
            if chaveiro.tem_cpf() { "sim" } else { "não" }
        ));
        let rotulos = chaveiro.rotulos_de_senha();
        ui.label(format!(
            "Senhas próprias ({}): {}",
            rotulos.len(),
            if rotulos.is_empty() {
                "nenhuma".to_owned()
            } else {
                rotulos.join(", ")
            }
        ));
        ui.label(
            RichText::new("Só os rótulos aparecem: os valores nunca.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        ui.add_space(8.0);
        let ocupada = self.ocupada();
        if situacao == Situacao::ArquivoTrancado {
            ui.label("Senha-mestra");
            ui.add(egui::TextEdit::singleline(&mut self.senha_mestra).password(true));
            if ui
                .add_enabled(
                    !ocupada && !self.senha_mestra.is_empty(),
                    egui::Button::new("Destrancar"),
                )
                .clicked()
            {
                self.destrancar();
            }
            ui.add_space(8.0);
        }
        componentes::titulo(ui, "Importar o cofre.enc antigo", 15.0);
        ui.label("Caminho do cofre.enc");
        ui.add(
            egui::TextEdit::singleline(&mut self.caminho_do_cofre_enc).desired_width(f32::INFINITY),
        );
        ui.label("Senha-mestra do cofre.enc");
        ui.add(egui::TextEdit::singleline(&mut self.senha_mestra).password(true));
        if ui
            .add_enabled(
                !ocupada
                    && !self.senha_mestra.is_empty()
                    && !self.caminho_do_cofre_enc.trim().is_empty(),
                egui::Button::new("Importar"),
            )
            .clicked()
        {
            self.importar_cofre_enc();
        }
        if let Some(t) = &self.tarefa {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new());
                ui.label(&t.andamento);
            });
        }
        if let Some(r) = &self.resumo_do_chaveiro {
            ui.label(r);
        }
        let mut apagar = false;
        let mut manter = false;
        if let Some(o) = &self.oferta {
            componentes::aviso(
                ui,
                "help",
                &format!(
                    "O chaveiro guarda o mesmo que {}. Apagar o arquivo antigo?",
                    o.caminho().display()
                ),
                p.info_sutil,
                p.info_borda,
                p.info,
            );
            ui.horizontal(|ui| {
                apagar = ui.button("Apagar o cofre.enc antigo").clicked();
                manter = ui.button("Manter").clicked();
            });
        }
        if apagar && let Some(o) = self.oferta.take() {
            self.resumo_do_chaveiro = Some(match o.apagar() {
                Ok(()) => "O cofre.enc antigo foi apagado.".to_owned(),
                Err(e) => format!("Não deu para apagar: {e}"),
            });
        }
        if manter {
            self.oferta = None;
        }
    }

    /// Bastidores › Estante e cópia de segurança: os destinos (Opções).
    pub fn destinos_ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        ui.label("Destino da estante (remoto do rclone, por exemplo gdrive:SBF/Estante)");
        let a = ui.add(
            egui::TextEdit::singleline(&mut self.destino_da_estante).desired_width(f32::INFINITY),
        );
        ui.label("Destino da cópia de segurança (por exemplo gdrive:SBF/Copia-de-seguranca)");
        let b = ui.add(
            egui::TextEdit::singleline(&mut self.destino_da_copia).desired_width(f32::INFINITY),
        );
        ui.label(
            RichText::new(
                "Sem destino, Atualizar estante e Fazer cópia de segurança ficam desabilitadas.",
            )
            .size(12.0)
            .color(p.tinta_suave),
        );
        let c = ui.checkbox(
            &mut self.tentar_estrategias_padrao,
            "PDF protegido: tentar estratégias padrão",
        );
        if (a.lost_focus() || b.lost_focus() || c.changed())
            && let Err(e) = self.salvar_preferencias()
        {
            ui.colored_label(p.perigo, format!("Não deu para guardar as opções: {e}"));
        }
        if let Some(arquivo) = self.arquivo_de_preferencias() {
            ui.label(
                RichText::new(format!("Guardadas em {}", arquivo.display()))
                    .size(12.0)
                    .color(p.tinta_suave),
            );
        }
    }

    /// O arquivo das opções das Ferramentas: na pasta de dados do app, nunca no cofre.
    pub fn arquivo_de_preferencias(&self) -> Option<PathBuf> {
        let pasta = sbf_chaveiro::pasta_de_dados_do_app().ok()?;
        sbf_chaveiro::garantir_fora_do_cofre(&pasta, &self.cofre).ok()?;
        Some(pasta.join("preferencias-das-ferramentas.json"))
    }

    /// Lê os destinos e a opção do PDF guardados (sem arquivo, fica tudo vazio).
    pub fn carregar_preferencias(&mut self) {
        let Some(arquivo) = self.arquivo_de_preferencias() else {
            return;
        };
        let Ok(texto) = std::fs::read_to_string(arquivo) else {
            return;
        };
        let Ok(p) = serde_json::from_str::<PreferenciasDasFerramentas>(&texto) else {
            return;
        };
        self.destino_da_estante = p.destino_da_estante;
        self.destino_da_copia = p.destino_da_copia;
        self.tentar_estrategias_padrao = p.tentar_estrategias_padrao;
    }

    /// Grava os destinos e a opção do PDF.
    pub fn salvar_preferencias(&self) -> Result<(), String> {
        let arquivo = self
            .arquivo_de_preferencias()
            .ok_or("sem pasta de dados do app (ou ela fica dentro do cofre)")?;
        if let Some(pasta) = arquivo.parent() {
            std::fs::create_dir_all(pasta).map_err(|e| e.to_string())?;
        }
        let p = PreferenciasDasFerramentas {
            destino_da_estante: self.destino_da_estante.trim().to_owned(),
            destino_da_copia: self.destino_da_copia.trim().to_owned(),
            tentar_estrategias_padrao: self.tentar_estrategias_padrao,
        };
        let texto = serde_json::to_string_pretty(&p).map_err(|e| e.to_string())?;
        std::fs::write(arquivo, texto).map_err(|e| e.to_string())
    }

    /// Ao abrir: apaga as sobras de cópias interrompidas (`<cofre>/tmp/copia-*` com mais de
    /// um dia), numa thread.
    pub fn faxinar_sobras_em_segundo_plano(&self) {
        let cofre = self.cofre.clone();
        std::thread::spawn(move || {
            let _ = sbf_estante::copia::faxinar_sobras(&cofre, sbf_estante::copia::IDADE_DA_SOBRA);
        });
    }
}

#[derive(serde::Serialize, serde::Deserialize, Default)]
struct PreferenciasDasFerramentas {
    #[serde(default)]
    destino_da_estante: String,
    #[serde(default)]
    destino_da_copia: String,
    #[serde(default)]
    tentar_estrategias_padrao: bool,
}

fn resumo_do_pdf(ui: &mut Ui, r: &RelatorioDaRodada) {
    let p = tema::paleta(ui.ctx());
    let mut abertos = 0;
    let mut sem_texto = 0;
    let mut pendencias = 0;
    let mut nao_abriu = 0;
    let mut grandes = 0;
    let mut outros = 0;
    for x in &r.resultados {
        match x.situacao {
            SituacaoDoPdf::Aberto { .. } => abertos += 1,
            SituacaoDoPdf::SemTexto { .. } => sem_texto += 1,
            SituacaoDoPdf::Pendencia { .. } => pendencias += 1,
            SituacaoDoPdf::NaoAbriu => nao_abriu += 1,
            SituacaoDoPdf::GrandeDemais => grandes += 1,
            _ => outros += 1,
        }
    }
    ui.label(format!(
        "{} PDFs protegidos: {abertos} abertos, {sem_texto} sem texto (OCR numa fatia futura), {pendencias} viraram pendência, {nao_abriu} ainda não abriram, {grandes} grandes demais (acima de 50 MiB), {outros} com falha ou sem arquivo.",
        r.resultados.len()
    ));
    if r.resultados.is_empty() {
        ui.label(RichText::new("Nenhum PDF protegido esperando.").color(p.tinta_suave));
    }
    if !r.estrategias_desconhecidas.is_empty() {
        ui.label(
            RichText::new(format!(
                "Estratégias desconhecidas ignoradas: {}",
                r.estrategias_desconhecidas.join(", ")
            ))
            .color(p.aviso),
        );
    }
}

/// Abre o arquivo (ou a pasta dele) com o programa padrão do sistema, sem esperar.
fn abrir_no_sistema(caminho: &Path, so_a_pasta: bool) -> Result<(), String> {
    let alvo = if so_a_pasta {
        caminho.parent().unwrap_or(caminho).to_path_buf()
    } else {
        caminho.to_path_buf()
    };
    let mut cmd = if cfg!(target_os = "windows") {
        let mut c = std::process::Command::new("explorer");
        if so_a_pasta {
            c.arg(format!("/select,{}", caminho.display()));
        } else {
            c.arg(&alvo);
        }
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        if so_a_pasta {
            c.arg("-R").arg(caminho);
        } else {
            c.arg(&alvo);
        }
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(&alvo);
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("não deu para abrir: {e}"))
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn erros_da_copia_tem_mensagem_clara() {
        use sbf_estante::Erro as E;
        let m = mensagem_da_copia(&E::SenhaCurta { minimo: 16 });
        assert!(m.contains("16 caracteres"), "{m}");
        assert!(mensagem_da_copia(&E::DestinoSemSal).contains("sal-da-copia.txt"));
        assert!(mensagem_da_copia(&E::SalInvalido).contains("adulterado"));
        assert!(
            mensagem_da_copia(&E::TempoEsgotado {
                comando: "lsf".into()
            })
            .contains("limite de tempo")
        );
        assert!(mensagem_da_copia(&E::Aleatorio).contains("aleatórios"));
    }

    #[test]
    fn situacao_do_chaveiro_em_palavras() {
        assert_eq!(texto_da_situacao(Situacao::ArquivoTrancado), "trancado");
        assert_eq!(
            texto_da_situacao(Situacao::ChaveiroDoSistema),
            "no keyring do sistema"
        );
    }
}
