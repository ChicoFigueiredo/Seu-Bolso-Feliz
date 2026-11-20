//! A aba **E-mail** da área Conexões: a lista das contas (cada uma, uma Conexão do tipo
//! E-mail), o detalhe da conta escolhida (rótulos seguidos, Entrar, Sair, Remover, credenciais
//! do aplicativo) e o botão **Adicionar conta**.
//!
//! A lógica (gravar `email.json`, o chaveiro, o login) fica em métodos sem `egui`, testados com
//! um chaveiro de mentira. O login e a lista de rótulos rodam numa thread: a interface só
//! espia o canal. Nenhum segredo é mostrado: o campo do secret é mascarado e **só substitui**.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{RichText, Ui};
use sbf_chaveiro::Chaveiro;
use sbf_gmail::config::{
    ARQUIVO_DA_CONFIG, Config, Conta, TipoDeEmail, arquivo_dos_rotulos, guardar_rotulos_da_conta,
    ler_rotulos_da_conta, sugestoes,
};
use sbf_gmail::contas::contas_efetivas;
use sbf_gmail::credenciais;
use sbf_gmail::{
    ClienteReal, EstadoDaConexao, FonteDeGmail as _, SituacaoDaConexao, estado_da_conta,
};
use zeroize::{Zeroize as _, Zeroizing};

use crate::conexoes_ui::{PontoDeEstado, data_curta, ponto};
use crate::{componentes, tema};

/// Quantas sugestões o combo mostra.
pub const SUGESTOES_NO_COMBO: usize = 8;

/// Uma conta e o que se sabe dela.
#[derive(Debug, Clone)]
pub struct LinhaDeConta {
    /// A conta (de `email.json`, ou a legada em memória).
    pub conta: Conta,
    /// O estado, lido do inventário e do chaveiro.
    pub estado: EstadoDaConexao,
    /// O aplicativo do tipo (client id e secret) já está no chaveiro?
    pub tem_app: bool,
    /// Os rótulos que a conta tem (a última lista guardada).
    pub rotulos_da_conta: Vec<String>,
}

enum Msg {
    Url(String),
    Fim(Result<String, String>),
}

struct NovaConta {
    tipo: TipoDeEmail,
    apelido: String,
}

/// O estado da aba.
#[derive(Default)]
pub struct EmailUi {
    cofre: Option<PathBuf>,
    dados: Option<PathBuf>,
    chaveiro: Option<Chaveiro>,
    contas: Vec<LinhaDeConta>,
    lido_em: Option<Instant>,
    /// A conta aberta no detalhe.
    pub selecionada: Option<String>,
    nova: Option<NovaConta>,
    campo_id: String,
    campo_segredo: Zeroizing<String>,
    so_desta_conta: bool,
    busca: String,
    escolhido: Option<String>,
    consulta_extra: String,
    consulta_da: Option<String>,
    login: Option<Receiver<Msg>>,
    /// Liga para cancelar o login em andamento (libera a porta do retorno).
    login_cancelar: std::sync::Arc<std::sync::atomic::AtomicBool>,
    url_do_login: Option<String>,
    rotulos_rx: Option<Receiver<Result<(), String>>>,
    confirmar_remocao: bool,
    /// O último aviso (texto, é erro?).
    pub aviso: Option<(String, bool)>,
    /// Não mostrar a conta legada em memória (credenciais antigas do ambiente): os testes.
    pub sem_legado: bool,
}

impl std::fmt::Debug for EmailUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailUi")
            .field("contas", &self.contas.len())
            .finish_non_exhaustive()
    }
}

/// Como obter o Client ID e o Client secret do aplicativo de cada tipo.
fn ajuda_do_app(tipo: TipoDeEmail) -> &'static str {
    match tipo {
        TipoDeEmail::Gmail => {
            "Cole o Client ID e o Client secret do aplicativo do Google (Google Cloud Console › APIs e serviços › Credenciais; redirecionamento http://127.0.0.1:8976/oauth2callback). Ficam só no chaveiro."
        }
        TipoDeEmail::Outlook => {
            "O Outlook ainda está em breve: ele vai pedir o Application (client) ID de um aplicativo registrado no Azure Portal › App registrations."
        }
    }
}

fn frase_da_situacao(l: &LinhaDeConta) -> String {
    let email = l.estado.email.as_deref();
    match l.estado.situacao {
        SituacaoDaConexao::EmBreve => "em breve".to_owned(),
        SituacaoDaConexao::NaoConectada => "não conectado".to_owned(),
        SituacaoDaConexao::AutorizacaoVencida => "autorização vencida".to_owned(),
        SituacaoDaConexao::Pronta | SituacaoDaConexao::Coletada => {
            format!("conectado como {}", email.unwrap_or("(conta)"))
        }
        SituacaoDaConexao::ComFalha => format!(
            "conectado como {}; a última coleta falhou",
            email.unwrap_or("(conta)")
        ),
    }
}

impl EmailUi {
    /// Liga a aba ao inventário e ao chaveiro de verdade.
    pub fn ligar(&mut self, cofre: PathBuf, dados: PathBuf, chaveiro: Option<Chaveiro>) {
        self.cofre = Some(cofre);
        self.dados = Some(dados);
        self.chaveiro = chaveiro;
        self.lido_em = None;
    }

    /// A aba está ligada?
    #[must_use]
    pub fn ligada(&self) -> bool {
        self.dados.is_some()
    }

    fn config(&self) -> Result<Config, String> {
        if self.sem_legado {
            let Some(a) = self.arquivo() else {
                return Ok(Config::default());
            };
            Config::carregar(&a).map_err(|e| e.to_string())
        } else {
            // Com `email.json` recusado (id inválido...), nenhuma conta é usada e nada é
            // regravado por cima: o erro vai para o aviso da aba.
            contas_efetivas(self.dados.as_deref(), self.chaveiro.as_ref())
                .map_err(|e| e.to_string())
        }
    }

    fn arquivo(&self) -> Option<PathBuf> {
        self.dados.as_ref().map(|d| d.join(ARQUIVO_DA_CONFIG))
    }

    /// Relê as contas e o estado de cada uma (barato: arquivos e um SQLite só de leitura).
    pub fn recarregar(&mut self) {
        let (Some(cofre), Some(dados)) = (self.cofre.clone(), self.dados.clone()) else {
            return;
        };
        let config = match self.config() {
            Ok(c) => c,
            Err(e) => {
                self.contas.clear();
                self.aviso = Some((e, true));
                self.lido_em = Some(Instant::now());
                return;
            }
        };
        self.contas = config
            .contas
            .into_iter()
            .map(|conta| LinhaDeConta {
                estado: estado_da_conta(Some(&cofre), &conta, self.chaveiro.as_ref()),
                tem_app: credenciais::tem_app_no_chaveiro(self.chaveiro.as_ref(), conta.tipo),
                rotulos_da_conta: ler_rotulos_da_conta(&dados.join(arquivo_dos_rotulos(&conta.id))),
                conta,
            })
            .collect();
        self.lido_em = Some(Instant::now());
        if self
            .selecionada
            .as_ref()
            .is_some_and(|s| !self.contas.iter().any(|l| &l.conta.id == s))
        {
            self.selecionada = None;
        }
    }

    /// As contas lidas.
    #[must_use]
    pub fn contas(&self) -> &[LinhaDeConta] {
        &self.contas
    }

    /// O ponto do título da aba: verde se alguma conta coleta, âmbar se alguma precisa de
    /// atenção (vencida, falha, sem login), cinza se não há conta.
    #[must_use]
    pub fn ponto_da_aba(&self) -> PontoDeEstado {
        use SituacaoDaConexao as S;
        if self
            .contas
            .iter()
            .any(|l| matches!(l.estado.situacao, S::AutorizacaoVencida | S::ComFalha))
        {
            PontoDeEstado::Atencao
        } else if self
            .contas
            .iter()
            .any(|l| matches!(l.estado.situacao, S::Pronta | S::Coletada))
        {
            PontoDeEstado::Ok
        } else if self
            .contas
            .iter()
            .any(|l| l.estado.situacao == S::NaoConectada)
        {
            PontoDeEstado::Atencao
        } else {
            PontoDeEstado::Desligado
        }
    }

    fn mutar(&mut self, f: impl FnOnce(&mut Config) -> Result<(), String>) -> Result<(), String> {
        let arquivo = self.arquivo().ok_or("a aba não está ligada")?;
        let mut config = self.config()?;
        f(&mut config)?;
        config.salvar(&arquivo).map_err(|e| e.to_string())?;
        self.recarregar();
        Ok(())
    }

    /// Adiciona uma conta nova (e a seleciona). Devolve o id.
    ///
    /// # Errors
    /// O motivo (tipo em breve, disco).
    pub fn adicionar_conta(&mut self, tipo: TipoDeEmail, apelido: &str) -> Result<String, String> {
        let mut id = String::new();
        self.mutar(|c| {
            id = c.adicionar(tipo, apelido).map_err(|e| e.to_string())?;
            Ok(())
        })?;
        self.selecionada = Some(id.clone());
        Ok(id)
    }

    fn rotulos_de(&self, id: &str) -> Vec<String> {
        self.contas
            .iter()
            .find(|l| l.conta.id == id)
            .map(|l| l.rotulos_da_conta.clone())
            .unwrap_or_default()
    }

    /// Passa a seguir um rótulo da conta.
    ///
    /// # Errors
    /// O motivo.
    pub fn seguir(&mut self, id: &str, rotulo: &str) -> Result<(), String> {
        let da_conta = self.rotulos_de(id);
        self.mutar(|c| {
            c.conta_mut(id)
                .ok_or_else(|| format!("não existe a conta {id}"))?
                .seguir(rotulo, &da_conta);
            Ok(())
        })
    }

    /// Deixa de seguir um rótulo da conta.
    ///
    /// # Errors
    /// O motivo.
    pub fn deixar(&mut self, id: &str, rotulo: &str) -> Result<(), String> {
        let da_conta = self.rotulos_de(id);
        self.mutar(|c| {
            c.conta_mut(id)
                .ok_or_else(|| format!("não existe a conta {id}"))?
                .deixar(rotulo, &da_conta);
            Ok(())
        })
    }

    /// Guarda os termos extras da consulta da conta.
    ///
    /// # Errors
    /// O motivo.
    pub fn guardar_consulta_extra(&mut self, id: &str, texto: &str) -> Result<(), String> {
        let texto = texto.trim().to_owned();
        self.mutar(|c| {
            c.conta_mut(id)
                .ok_or_else(|| format!("não existe a conta {id}"))?
                .consulta_extra = texto;
            Ok(())
        })
    }

    /// As sugestões do combo: os rótulos da conta que casam com o digitado e ainda não são
    /// seguidos (no máximo [`SUGESTOES_NO_COMBO`]).
    #[must_use]
    pub fn sugestoes_para(&self, id: &str, digitado: &str) -> Vec<String> {
        let Some(l) = self.contas.iter().find(|l| l.conta.id == id) else {
            return Vec::new();
        };
        let seguidos = l.conta.seguidos_para_mostrar(&l.rotulos_da_conta);
        sugestoes(&l.rotulos_da_conta, digitado, &seguidos, SUGESTOES_NO_COMBO)
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    /// Guarda o Client ID e/ou o Client secret (só os preenchidos). `so_desta_conta` grava na
    /// conta; senão, no aplicativo do tipo (as contas do tipo compartilham).
    ///
    /// # Errors
    /// O motivo.
    pub fn guardar_credenciais(
        &mut self,
        id: &str,
        so_desta_conta: bool,
        client_id: &str,
        client_secret: &str,
    ) -> Result<(), String> {
        let c = self.chaveiro.as_ref().ok_or("o chaveiro não abriu")?;
        let tipo = self
            .contas
            .iter()
            .find(|l| l.conta.id == id)
            .map(|l| l.conta.tipo)
            .ok_or_else(|| format!("não existe a conta {id}"))?;
        let (i, s) = (Some(client_id), Some(client_secret));
        if so_desta_conta {
            credenciais::guardar_cliente_da_conta(c, id, i, s)
        } else {
            credenciais::guardar_app(c, tipo, i, s)
        }
        .map_err(|e| e.to_string())?;
        self.recarregar();
        Ok(())
    }

    /// Apaga as credenciais da conta do chaveiro (a conta continua na lista).
    ///
    /// # Errors
    /// O motivo.
    pub fn sair(&mut self, id: &str) -> Result<(), String> {
        let c = self.chaveiro.as_ref().ok_or("o chaveiro não abriu")?;
        credenciais::apagar(c, id).map_err(|e| e.to_string())?;
        self.mutar(|c| {
            if let Some(conta) = c.conta_mut(id) {
                conta.conta = None;
            }
            Ok(())
        })
    }

    /// Sai e tira a conta da lista (o que já entrou no inventário continua lá).
    ///
    /// # Errors
    /// O motivo.
    pub fn remover(&mut self, id: &str) -> Result<(), String> {
        let c = self.chaveiro.as_ref().ok_or("o chaveiro não abriu")?;
        credenciais::apagar(c, id).map_err(|e| e.to_string())?;
        self.mutar(|c| {
            c.remover(id);
            Ok(())
        })?;
        if self.selecionada.as_deref() == Some(id) {
            self.selecionada = None;
        }
        Ok(())
    }

    /// Começa o login numa thread (o navegador abre; a interface espia o canal).
    pub fn iniciar_login(&mut self, id: &str) {
        let (Some(chaveiro), Some(arquivo)) = (self.chaveiro.clone(), self.arquivo()) else {
            self.aviso = Some(("O chaveiro não abriu.".to_owned(), true));
            return;
        };
        // A conta legada só existe em memória até o primeiro login: grava `email.json` antes.
        if let Err(e) = self.mutar(|_| Ok(())) {
            self.aviso = Some((e, true));
            return;
        }
        let (tx, rx) = channel();
        let id = id.to_owned();
        let cancelar = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        self.login_cancelar = std::sync::Arc::clone(&cancelar);
        std::thread::spawn(move || {
            let avisar = tx.clone();
            let r = sbf_gmail::oauth::entrar_no_navegador(
                &chaveiro,
                &arquivo,
                &id,
                &move |url| {
                    let _ = avisar.send(Msg::Url(url.to_owned()));
                },
                cancelar,
            );
            let _ = tx.send(Msg::Fim(r.map_err(|e| e.to_string())));
        });
        self.login = Some(rx);
        self.url_do_login = None;
        self.aviso = None;
    }

    /// Atualiza a lista de rótulos da conta (uma chamada ao Gmail, numa thread).
    pub fn atualizar_rotulos(&mut self, id: &str) {
        let (Some(dados), Some(l)) = (
            self.dados.clone(),
            self.contas.iter().find(|l| l.conta.id == id).cloned(),
        ) else {
            return;
        };
        let chaveiro = self.chaveiro.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = ClienteReal::resolver(chaveiro.as_ref(), l.conta.alvo())
                .and_then(|c| c.rotulos())
                .and_then(|r| {
                    guardar_rotulos_da_conta(&dados.join(arquivo_dos_rotulos(&l.conta.id)), &r)
                })
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.rotulos_rx = Some(rx);
    }

    fn espiar(&mut self, ctx: &egui::Context) {
        if let Some(rx) = &self.login {
            loop {
                match rx.try_recv() {
                    Ok(Msg::Url(u)) => self.url_do_login = Some(u),
                    Ok(Msg::Fim(r)) => {
                        self.login = None;
                        self.url_do_login = None;
                        self.aviso = Some(match r {
                            Ok(email) => (format!("Conectado como {email}."), false),
                            Err(e) => (e, true),
                        });
                        self.recarregar();
                        break;
                    }
                    Err(TryRecvError::Empty) => {
                        ctx.request_repaint_after(Duration::from_millis(300));
                        break;
                    }
                    Err(TryRecvError::Disconnected) => {
                        self.login = None;
                        break;
                    }
                }
            }
        }
        if let Some(rx) = &self.rotulos_rx {
            match rx.try_recv() {
                Ok(r) => {
                    self.rotulos_rx = None;
                    if let Err(e) = r {
                        self.aviso = Some((e, true));
                    }
                    self.recarregar();
                }
                Err(TryRecvError::Empty) => ctx.request_repaint_after(Duration::from_millis(300)),
                Err(TryRecvError::Disconnected) => self.rotulos_rx = None,
            }
        }
    }

    /// Desenha a aba.
    pub fn desenhar(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        self.espiar(ui.ctx());
        if self
            .lido_em
            .is_none_or(|t| t.elapsed() > Duration::from_secs(5))
        {
            self.recarregar();
        }
        if let Some((t, erro)) = self.aviso.clone() {
            if erro {
                componentes::aviso(ui, "error", &t, p.perigo_sutil, p.borda, p.perigo);
            } else {
                componentes::aviso(ui, "help", &t, p.info_sutil, p.info_borda, p.info);
            }
            ui.add_space(6.0);
        }
        componentes::cartao(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(crate::icones::icone_cor("mail", 18.0, p.tinta));
                componentes::titulo(ui, "Contas de e-mail", 16.0);
                componentes::selo(ui, "só leitura", p.superficie_3, p.tinta_suave);
            });
            ui.label(
                RichText::new("Anexos (PDF, imagem, XML) e avisos por e-mail dos rótulos que você seguir vão para `entrada/` e passam pelas regras de reconhecimento. A coleta roda no worker, em `sbf email coletar` e na ferramenta `gmail_coletar` do MCP. Só lê: nunca envia, apaga nem marca como lido.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
            ui.add_space(6.0);
            if self.contas.is_empty() {
                ui.label(
                    RichText::new("Nenhuma conta ainda. Use Adicionar conta.").color(p.tinta_suave),
                );
            }
            let mut escolher: Option<String> = None;
            for l in &self.contas {
                let marcada = self.selecionada.as_deref() == Some(l.conta.id.as_str());
                ui.horizontal(|ui| {
                    let ponto_ = match l.estado.situacao {
                        SituacaoDaConexao::Pronta | SituacaoDaConexao::Coletada => {
                            PontoDeEstado::Ok
                        }
                        SituacaoDaConexao::EmBreve => PontoDeEstado::Desligado,
                        _ => PontoDeEstado::Atencao,
                    };
                    ponto(ui, ponto_);
                    if ui
                        .selectable_label(marcada, RichText::new(l.conta.nome()).strong())
                        .clicked()
                    {
                        escolher = Some(l.conta.id.clone());
                    }
                    componentes::selo(ui, l.conta.tipo.rotulo(), p.superficie_3, p.tinta_suave);
                    ui.label(
                        RichText::new(format!(
                            "{} · última coleta {} · {} arquivo(s)",
                            frase_da_situacao(l),
                            l.estado
                                .ultima_coleta
                                .as_deref()
                                .map_or_else(|| "nunca".to_owned(), data_curta),
                            l.estado.documentos
                        ))
                        .size(12.0)
                        .color(p.tinta_suave),
                    );
                });
            }
            if let Some(id) = escolher {
                self.selecionada = Some(id);
                self.confirmar_remocao = false;
                self.busca.clear();
                self.escolhido = None;
                self.consulta_da = None;
            }
            ui.add_space(6.0);
            self.adicionar_ui(ui);
        });
        if let Some(l) = self
            .selecionada
            .as_ref()
            .and_then(|s| self.contas.iter().find(|l| &l.conta.id == s))
            .cloned()
        {
            ui.add_space(10.0);
            self.detalhe_ui(ui, &l);
        }
    }

    fn adicionar_ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        let mut criar: Option<(TipoDeEmail, String)> = None;
        let mut cancelar = false;
        let mut abrir = false;
        match &mut self.nova {
            None => {
                abrir = ui.button("Adicionar conta").clicked();
            }
            Some(n) => {
                ui.horizontal(|ui| {
                    ui.label("Tipo");
                    for t in [TipoDeEmail::Gmail, TipoDeEmail::Outlook] {
                        let texto = if t.disponivel() {
                            t.rotulo().to_owned()
                        } else {
                            format!("{} (em breve)", t.rotulo())
                        };
                        ui.add_enabled_ui(t.disponivel(), |ui| {
                            ui.selectable_value(&mut n.tipo, t, texto);
                        });
                    }
                });
                ui.horizontal(|ui| {
                    ui.label("Apelido");
                    ui.add(
                        egui::TextEdit::singleline(&mut n.apelido)
                            .hint_text("ex.: Casa (opcional)")
                            .desired_width(200.0),
                    );
                    if ui.button("Criar").clicked() {
                        criar = Some((n.tipo, n.apelido.clone()));
                    }
                    if ui.button("Cancelar").clicked() {
                        cancelar = true;
                    }
                });
                ui.label(
                    RichText::new("Depois de criar, informe o Client ID e o Client secret do aplicativo (se ainda não houver) e use Entrar.")
                        .size(12.0)
                        .color(p.tinta_suave),
                );
            }
        }
        if abrir {
            self.nova = Some(NovaConta {
                tipo: TipoDeEmail::Gmail,
                apelido: String::new(),
            });
        }
        if cancelar {
            self.nova = None;
        }
        if let Some((tipo, apelido)) = criar {
            self.nova = None;
            match self.adicionar_conta(tipo, &apelido) {
                Ok(_) => self.aviso = None,
                Err(e) => self.aviso = Some((e, true)),
            }
        }
    }

    fn detalhe_ui(&mut self, ui: &mut Ui, l: &LinhaDeConta) {
        let p = tema::paleta(ui.ctx());
        let id = l.conta.id.clone();
        let entrando = self.login.is_some();
        let mut entrar = false;
        let mut sair = false;
        let mut remover = false;
        let mut guardar = false;
        let mut cancelar_login = false;
        let mut atualizar = false;
        let mut seguir: Option<String> = None;
        let mut deixar: Option<String> = None;
        let mut extra: Option<String> = None;
        componentes::cartao(ui, |ui| {
            ui.horizontal(|ui| {
                componentes::titulo(ui, l.conta.nome(), 16.0);
                let (rotulo, fundo, cor) = match l.estado.situacao {
                    SituacaoDaConexao::EmBreve => {
                        ("em breve".to_owned(), p.superficie_3, p.tinta_suave)
                    }
                    SituacaoDaConexao::NaoConectada => {
                        ("não conectado".to_owned(), p.aviso_sutil, p.aviso)
                    }
                    SituacaoDaConexao::AutorizacaoVencida => {
                        ("autorização vencida".to_owned(), p.perigo_sutil, p.perigo)
                    }
                    SituacaoDaConexao::Pronta => (frase_da_situacao(l), p.info_sutil, p.info),
                    SituacaoDaConexao::Coletada => {
                        (frase_da_situacao(l), p.sucesso_sutil, p.sucesso)
                    }
                    SituacaoDaConexao::ComFalha => (frase_da_situacao(l), p.perigo_sutil, p.perigo),
                };
                componentes::selo(ui, &rotulo, fundo, cor);
            });
            let precisa_entrar = matches!(
                l.estado.situacao,
                SituacaoDaConexao::NaoConectada | SituacaoDaConexao::AutorizacaoVencida
            );
            if l.estado.situacao == SituacaoDaConexao::AutorizacaoVencida {
                componentes::aviso(
                    ui,
                    "error",
                    &format!(
                        "A autorização do {} venceu ou foi revogada. Entre de novo para voltar a coletar.",
                        l.conta.tipo.rotulo()
                    ),
                    p.perigo_sutil,
                    p.borda,
                    p.perigo,
                );
            } else if l.estado.situacao == SituacaoDaConexao::NaoConectada {
                let entrar_no = format!("Entrar no {}", l.conta.tipo.rotulo());
                let texto = if l.tem_app {
                    format!("Esta conta ainda não fez o login. Use {entrar_no}.")
                } else {
                    format!(
                        "Informe o Client ID e o Client secret do aplicativo (abaixo) e use {entrar_no}."
                    )
                };
                let texto = texto.as_str();
                componentes::aviso(ui, "help", texto, p.info_sutil, p.info_borda, p.info);
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                let rotulo = if l.estado.situacao == SituacaoDaConexao::NaoConectada {
                    format!("Entrar no {}", l.conta.tipo.rotulo())
                } else {
                    "Entrar de novo".to_owned()
                };
                if ui
                    .add_enabled(
                        !entrando && l.conta.tipo.disponivel(),
                        egui::Button::new(rotulo),
                    )
                    .on_hover_text(format!(
                        "Abre o navegador para você autorizar a leitura do {}.",
                        l.conta.tipo.rotulo()
                    ))
                    .clicked()
                {
                    entrar = true;
                }
                if ui
                    .button("Sair")
                    .on_hover_text("Apaga as credenciais desta conta do chaveiro.")
                    .clicked()
                {
                    sair = true;
                }
                if self.confirmar_remocao {
                    if ui
                        .button(RichText::new("Confirmar remover").color(p.perigo))
                        .clicked()
                    {
                        remover = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        self.confirmar_remocao = false;
                    }
                } else if ui
                    .button("Remover conexão")
                    .on_hover_text("Tira a conta da lista. O que já entrou no inventário fica.")
                    .clicked()
                {
                    self.confirmar_remocao = true;
                }
                if entrando {
                    if ui
                        .button("Cancelar login")
                        .on_hover_text(
                            "Desiste de esperar o navegador e libera a porta do retorno.",
                        )
                        .clicked()
                    {
                        cancelar_login = true;
                    }
                    ui.spinner();
                    ui.label(
                        RichText::new("esperando você autorizar no navegador…")
                            .color(p.tinta_suave),
                    );
                }
            });
            if let Some(u) = &self.url_do_login {
                ui.label(
                    RichText::new("Se o navegador não abriu, copie este endereço:")
                        .size(12.0)
                        .color(p.tinta_suave),
                );
                let mut copia = u.clone();
                ui.add(egui::TextEdit::singleline(&mut copia).desired_width(f32::INFINITY));
            }
            ui.add_space(6.0);
            componentes::rotulo_de_secao(ui, "Rótulos seguidos");
            let seguidos = l.conta.seguidos_para_mostrar(&l.rotulos_da_conta);
            if seguidos.is_empty() {
                ui.label(
                    RichText::new(if l.conta.rotulos.is_some() {
                        "Nenhum rótulo seguido: nada é coletado."
                    } else {
                        "Nenhum rótulo escolhido. Sem escolha, vale o rótulo Comprovantes (se a conta o tem); senão, os anexos dos últimos 30 dias da caixa inteira."
                    })
                    .color(p.tinta_suave),
                );
            }
            for n in &seguidos {
                ui.horizontal(|ui| {
                    if ui
                        .small_button("×")
                        .on_hover_text("Deixar de seguir")
                        .clicked()
                    {
                        deixar = Some(n.clone());
                    }
                    ui.label(n);
                });
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add(
                    egui::TextEdit::singleline(&mut self.busca)
                        .hint_text("Digite para achar um rótulo da conta")
                        .desired_width(260.0),
                );
                let pode = self
                    .escolhido
                    .as_ref()
                    .is_some_and(|e| !seguidos.iter().any(|s| s.eq_ignore_ascii_case(e)));
                if ui
                    .add_enabled(pode, egui::Button::new("Adicionar"))
                    .clicked()
                {
                    seguir = self.escolhido.clone();
                }
                if ui
                    .add_enabled(
                        self.rotulos_rx.is_none() && !precisa_entrar,
                        egui::Button::new("Atualizar rótulos"),
                    )
                    .on_hover_text(format!(
                        "Pergunta ao {} quais rótulos a conta tem agora.",
                        l.conta.tipo.rotulo()
                    ))
                    .clicked()
                {
                    atualizar = true;
                }
                if self.rotulos_rx.is_some() {
                    ui.spinner();
                }
            });
            if l.rotulos_da_conta.is_empty() {
                ui.label(
                    RichText::new("A lista de rótulos da conta ainda não foi baixada: use Atualizar rótulos depois de entrar.")
                        .size(12.0)
                        .color(p.tinta_suave),
                );
            }
            let sugeridos = self.sugestoes_para(&id, &self.busca.clone());
            if !self.busca.trim().is_empty() || self.escolhido.is_some() {
                for s in sugeridos {
                    let marcado = self.escolhido.as_deref() == Some(s.as_str());
                    if ui.selectable_label(marcado, &s).clicked() {
                        self.busca.clone_from(&s);
                        self.escolhido = Some(s);
                    }
                }
            }
            ui.add_space(6.0);
            if self.consulta_da.as_deref() != Some(id.as_str()) {
                self.consulta_extra.clone_from(&l.conta.consulta_extra);
                self.consulta_da = Some(id.clone());
            }
            ui.horizontal(|ui| {
                ui.label(RichText::new("Consulta extra").color(p.tinta_suave));
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.consulta_extra)
                        .hint_text("opcional, ex.: from:banco (soma a has:attachment)")
                        .desired_width(320.0),
                );
                if r.lost_focus() && self.consulta_extra.trim() != l.conta.consulta_extra {
                    extra = Some(self.consulta_extra.clone());
                }
            });
            ui.add_space(6.0);
            egui::CollapsingHeader::new("Credenciais do aplicativo")
                .id_salt(("credenciais", &id))
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(if l.tem_app {
                            "O Client ID e o Client secret do aplicativo já estão no chaveiro (valores não são mostrados). Preencha só para substituir."
                        } else {
                            ajuda_do_app(l.conta.tipo)
                        })
                        .size(12.0)
                        .color(p.tinta_suave),
                    );
                    ui.horizontal(|ui| {
                        ui.label("Client ID");
                        ui.add(
                            egui::TextEdit::singleline(&mut self.campo_id)
                                .hint_text(if l.tem_app { "guardado" } else { "cole aqui" })
                                .desired_width(320.0),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label("Client secret");
                        ui.add(
                            egui::TextEdit::singleline(&mut *self.campo_segredo)
                                .password(true)
                                .hint_text(if l.tem_app { "••••••••" } else { "cole aqui" })
                                .desired_width(320.0),
                        );
                    });
                    ui.checkbox(&mut self.so_desta_conta, "Só para esta conta (as outras contas do mesmo tipo usam o do aplicativo)");
                    if ui
                        .add_enabled(
                            !self.campo_id.trim().is_empty() || !self.campo_segredo.trim().is_empty(),
                            egui::Button::new("Guardar no chaveiro"),
                        )
                        .clicked()
                    {
                        guardar = true;
                    }
                    if !l.estado.faltando.is_empty() {
                        egui::CollapsingHeader::new("Detalhe técnico")
                            .id_salt(("tecnico", &id))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "Faltam em todas as fontes (chaveiro, ambiente e .env): {}.",
                                        l.estado.faltando.join(", ")
                                    ))
                                    .size(12.0)
                                    .color(p.tinta_suave),
                                );
                            });
                    }
                });
        });
        if entrar {
            self.iniciar_login(&id);
        }
        if cancelar_login {
            self.login_cancelar
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if sair {
            self.aviso = Some(match self.sair(&id) {
                Ok(()) => ("Credenciais apagadas do chaveiro.".to_owned(), false),
                Err(e) => (e, true),
            });
        }
        if remover {
            self.confirmar_remocao = false;
            self.aviso = Some(match self.remover(&id) {
                Ok(()) => ("Conta removida da lista.".to_owned(), false),
                Err(e) => (e, true),
            });
        }
        if guardar {
            let i = self.campo_id.clone();
            let s = self.campo_segredo.clone();
            let r = self.guardar_credenciais(&id, self.so_desta_conta, &i, &s);
            if r.is_ok() {
                self.campo_id.clear();
                self.campo_segredo.zeroize();
            }
            self.aviso = Some(match r {
                Ok(()) => ("Credenciais guardadas no chaveiro.".to_owned(), false),
                Err(e) => (e, true),
            });
        }
        if atualizar {
            self.atualizar_rotulos(&id);
        }
        if let Some(n) = seguir {
            if let Err(e) = self.seguir(&id, &n) {
                self.aviso = Some((e, true));
            }
            self.busca.clear();
            self.escolhido = None;
        }
        if let Some(n) = deixar
            && let Err(e) = self.deixar(&id, &n)
        {
            self.aviso = Some((e, true));
        }
        if let Some(t) = extra
            && let Err(e) = self.guardar_consulta_extra(&id, &t)
        {
            self.aviso = Some((e, true));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod testes {
    use super::*;
    use sbf_gmail::credenciais::Alvo;
    use std::sync::Arc;

    use bancada_segredos::ParametrosDeDerivacao;
    use bancada_segredos::testes::ChaveiroEmMemoria;

    fn ligada() -> (tempfile::TempDir, EmailUi) {
        let d = tempfile::tempdir().unwrap();
        let chaveiro = Chaveiro::abrir_com_chaveiro(
            &d.path().join("chaveiro"),
            Arc::new(ChaveiroEmMemoria::novo()),
            ParametrosDeDerivacao::rapidos_para_testes(),
        )
        .unwrap();
        let dados = d.path().join("dados");
        std::fs::create_dir_all(&dados).unwrap();
        let mut ui = EmailUi {
            sem_legado: true,
            ..EmailUi::default()
        };
        ui.ligar(d.path().join("cofre"), dados, Some(chaveiro));
        (d, ui)
    }

    #[test]
    fn adicionar_varias_contas_e_outlook_fica_em_breve() {
        let (_d, mut e) = ligada();
        let a = e.adicionar_conta(TipoDeEmail::Gmail, "Casa").unwrap();
        let b = e.adicionar_conta(TipoDeEmail::Gmail, "Trabalho").unwrap();
        assert_eq!((a.as_str(), b.as_str()), ("gmail", "gmail-2"));
        assert_eq!(e.contas().len(), 2);
        assert_eq!(e.selecionada.as_deref(), Some("gmail-2"));
        assert!(e.adicionar_conta(TipoDeEmail::Outlook, "").is_err());
        assert_eq!(e.contas().len(), 2);
    }

    #[test]
    fn email_json_com_id_invalido_e_recusado_e_nada_e_regravado() {
        let (d, mut e) = ligada();
        let arquivo = d.path().join("dados").join(ARQUIVO_DA_CONFIG);
        let ruim = r#"{"contas":[{"id":"../..","tipo":"gmail"}]}"#;
        std::fs::write(&arquivo, ruim).unwrap();
        e.recarregar();
        assert!(e.contas().is_empty());
        let (msg, erro) = e.aviso.clone().unwrap();
        assert!(erro && msg.contains("id inválido"), "{msg}");
        assert!(e.adicionar_conta(TipoDeEmail::Gmail, "").is_err());
        assert_eq!(std::fs::read_to_string(&arquivo).unwrap(), ruim);
    }

    #[test]
    fn seguir_deixar_e_o_combo_de_rotulos() {
        let (d, mut e) = ligada();
        e.adicionar_conta(TipoDeEmail::Gmail, "").unwrap();
        // A conta tem estes rótulos (a lista guardada pela última atualização).
        let rotulos: Vec<sbf_gmail::Rotulo> = ["Comprovantes", "Viagens", "Contas", "Cartões"]
            .iter()
            .enumerate()
            .map(|(i, n)| sbf_gmail::Rotulo {
                id: format!("Label_{i}"),
                nome: (*n).to_owned(),
            })
            .collect();
        guardar_rotulos_da_conta(
            &d.path().join("dados").join(arquivo_dos_rotulos("gmail")),
            &rotulos,
        )
        .unwrap();
        e.recarregar();
        // Padrão: Comprovantes (a conta o tem); o combo não o oferece de novo.
        assert_eq!(e.sugestoes_para("gmail", "co"), ["Contas"]);
        assert_eq!(e.sugestoes_para("gmail", "").len(), 3);
        e.seguir("gmail", "Viagens").unwrap();
        let c = e.contas()[0].conta.clone();
        assert_eq!(
            c.rotulos,
            Some(vec!["Comprovantes".into(), "Viagens".into()])
        );
        assert!(e.sugestoes_para("gmail", "via").is_empty());
        e.deixar("gmail", "Comprovantes").unwrap();
        assert_eq!(
            e.contas()[0].conta.rotulos,
            Some(vec!["Viagens".to_owned()])
        );
        assert_eq!(e.sugestoes_para("gmail", "comp"), ["Comprovantes"]);
        e.guardar_consulta_extra("gmail", "  from:banco ").unwrap();
        assert_eq!(e.contas()[0].conta.consulta_extra, "from:banco");
        // O combo limita a 8.
        let muitos: Vec<sbf_gmail::Rotulo> = (0..30)
            .map(|i| sbf_gmail::Rotulo {
                id: format!("Label_{i}"),
                nome: format!("Pasta {i}"),
            })
            .collect();
        guardar_rotulos_da_conta(
            &d.path().join("dados").join(arquivo_dos_rotulos("gmail")),
            &muitos,
        )
        .unwrap();
        e.recarregar();
        assert_eq!(e.sugestoes_para("gmail", "pasta").len(), SUGESTOES_NO_COMBO);
    }

    #[test]
    fn credenciais_sair_e_remover_so_mexem_no_chaveiro_da_conta() {
        let (_d, mut e) = ligada();
        e.adicionar_conta(TipoDeEmail::Gmail, "").unwrap();
        e.adicionar_conta(TipoDeEmail::Gmail, "").unwrap();
        e.guardar_credenciais("gmail", false, "id-do-app", "segredo-do-app")
            .unwrap();
        assert!(e.contas().iter().all(|l| l.tem_app), "o app é do tipo");
        e.guardar_credenciais("gmail-2", true, "id-proprio", "")
            .unwrap();
        let c = e.chaveiro.clone();
        credenciais::guardar_login(c.as_ref().unwrap(), "gmail", "token-um").unwrap();
        credenciais::guardar_login(c.as_ref().unwrap(), "gmail-2", "token-dois").unwrap();
        e.sair("gmail").unwrap();
        assert!(!credenciais::tem_token_no_chaveiro(c.as_ref(), "gmail"));
        assert!(credenciais::tem_token_no_chaveiro(c.as_ref(), "gmail-2"));
        assert!(credenciais::tem_app_no_chaveiro(
            c.as_ref(),
            TipoDeEmail::Gmail
        ));
        assert_eq!(e.contas().len(), 2, "sair não remove da lista");
        e.remover("gmail-2").unwrap();
        assert_eq!(e.contas().len(), 1);
        assert!(!credenciais::tem_token_no_chaveiro(c.as_ref(), "gmail-2"));
        assert!(!credenciais::tem_cliente_proprio(c.as_ref(), "gmail-2"));
    }

    #[test]
    fn autorizacao_vencida_aparece_no_estado_e_no_ponto_da_aba() {
        let (_d, mut e) = ligada();
        e.adicionar_conta(TipoDeEmail::Gmail, "").unwrap();
        e.guardar_credenciais("gmail", false, "id", "segredo")
            .unwrap();
        let c = e.chaveiro.clone().unwrap();
        credenciais::guardar_login(&c, "gmail", "token-velho").unwrap();
        e.recarregar();
        assert_eq!(e.contas()[0].estado.situacao, SituacaoDaConexao::Pronta);
        assert_eq!(e.ponto_da_aba(), PontoDeEstado::Ok);
        let cred = sbf_gmail::Credenciais::resolver(
            &|_| None,
            &[],
            Some(&c),
            Alvo {
                id: "gmail",
                tipo: TipoDeEmail::Gmail,
            },
        )
        .unwrap();
        credenciais::marcar_vencida(&c, "gmail", &cred).unwrap();
        e.recarregar();
        assert_eq!(
            e.contas()[0].estado.situacao,
            SituacaoDaConexao::AutorizacaoVencida
        );
        assert_eq!(e.ponto_da_aba(), PontoDeEstado::Atencao);
        assert_eq!(frase_da_situacao(&e.contas()[0]), "autorização vencida");
    }
}
