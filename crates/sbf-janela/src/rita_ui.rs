//! O painel **Rita** (spec 15, D7 a D14): a assistente de padrões de reconhecimento, ao lado
//! do item selecionado. Está no dock e pode ser destacada numa janela flutuante; acompanha a
//! seleção (Caixa de entrada, A triar, Documentos, Padrões e Movimentações).
//!
//! - A conversa vive enquanto a janela está aberta, **uma por item** (D14).
//! - A chamada ao provedor roda numa **thread à parte** (nunca na da interface); o painel só
//!   espia o canal e tem o botão **Parar**.
//! - O rascunho que Rita cria aparece num **cartão** com o pré-teste e os botões Ativar,
//!   Abrir no editor e Descartar. Quem ativa é o usuário, pelo caminho que a janela já tem
//!   ([`AcaoDeRita::Ativar`]); se o pré-teste mostrou empate ou que o padrão derruba documentos de
//!   outro, o botão pede confirmação (D10). "Descartar" só tira o cartão do painel: uma versão
//!   salva nunca se apaga, e o rascunho nunca roda.
//! - Chaves e endereços ficam no Chaveiro ([`sbf_rita::GuardaDeProvedores`]); nada disso, nem
//!   texto de documento, vai a log.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{RichText, ScrollArea, Ui};
use sbf_ia::{Consumo, NaturezaDoCusto};
use sbf_regras::script::ExecutorDeScript;
use sbf_rita::{
    ChamadaFeita, Contexto, DadosDaTela, Evento, GuardaDeProvedores, Parada, PreTeste,
    ProvedorConfigurado, ProvedorDaRita, Relatorio, Rita, TipoDeProvedor,
};

use crate::{componentes, icones, tema};

/// O que o painel mostra de um item: a identidade e os dados que Rita recebe.
#[derive(Debug, Clone)]
pub struct ItemDaTela {
    /// A identidade estável do item (a conversa é uma por chave).
    pub chave: String,
    /// O nome para mostrar.
    pub rotulo: String,
    pub tela: DadosDaTela,
}

/// O que o painel pede à janela.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AcaoDeRita {
    /// O usuário clicou em Ativar (e confirmou, se pedido): a janela ativa como usuário.
    Ativar { regra: String, versao: u32 },
    /// Abrir a regra no editor da área Padrões.
    AbrirNoEditor { regra: String },
}

/// Tudo o que o painel precisa de fora do app.
#[derive(Clone)]
pub struct Ambiente {
    pub cofre: PathBuf,
    pub guarda: Arc<dyn GuardaDeProvedores>,
    pub executor: Arc<dyn ExecutorDeScript>,
    /// O próprio `sbf`, que o `claude -p` sobe como servidor MCP.
    pub programa: PathBuf,
}

impl std::fmt::Debug for Ambiente {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Ambiente")
            .field("cofre", &self.cofre)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Balao {
    Usuario(String),
    Rita(String),
    Chamada(ChamadaFeita),
    Aviso(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoDoCartao {
    Aberto,
    Ativado,
    Descartado,
}

/// O cartão do rascunho (o melhor do último pedido).
#[derive(Debug, Clone, PartialEq)]
pub struct Cartao {
    pub regra: String,
    pub versao: u32,
    pub nome: String,
    pub pre_teste: PreTeste,
    pub estado: EstadoDoCartao,
    /// O botão Ativar está pedindo confirmação.
    pub confirmando: bool,
    /// O que deu errado ao ativar (exemplo que falha, por exemplo).
    pub erro: Option<String>,
}

struct Conversa {
    rita: Option<Rita>,
    baloes: Vec<Balao>,
    cartao: Option<Cartao>,
    /// Tokens e custo da resposta mais recente (ao vivo, durante o pedido) e a natureza dela.
    ultima: Option<(Consumo, NaturezaDoCusto)>,
    /// O total do item, somado por natureza do custo (real, nominal, local e não informado
    /// não se misturam numa conta só).
    totais: Vec<(NaturezaDoCusto, Consumo)>,
    /// Quantas respostas já entraram no total.
    respostas: usize,
    natureza: NaturezaDoCusto,
    modelo: String,
    duracao: Option<Duration>,
}

impl Conversa {
    fn somar_ao_total(&mut self, natureza: NaturezaDoCusto, consumo: Consumo) {
        self.respostas += 1;
        match self.totais.iter_mut().find(|(n, _)| *n == natureza) {
            Some((_, t)) => *t = soma(*t, consumo),
            None => self.totais.push((natureza, consumo)),
        }
    }

    fn nova() -> Self {
        Self {
            rita: None,
            baloes: Vec::new(),
            cartao: None,
            ultima: None,
            totais: Vec::new(),
            respostas: 0,
            natureza: NaturezaDoCusto::Zero,
            modelo: String::new(),
            duracao: None,
        }
    }
}

/// De onde vem o provedor do pedido: já montado (testes) ou a montar **na thread do pedido**
/// (achar o `claude` no disco e consultar o Chaveiro não rodam na thread da interface).
enum FonteDoProvedor {
    Pronto(ProvedorDaRita),
    Configurado {
        configurado: ProvedorConfigurado,
        guarda: Arc<dyn GuardaDeProvedores>,
        programa: PathBuf,
    },
}

impl FonteDoProvedor {
    fn natureza(&self) -> NaturezaDoCusto {
        match self {
            Self::Pronto(ProvedorDaRita::Conversa(p)) => p.natureza_do_custo(),
            Self::Pronto(ProvedorDaRita::Claude(_)) => NaturezaDoCusto::Nominal,
            Self::Configurado { configurado, .. } => match configurado.tipo {
                TipoDeProvedor::Local => NaturezaDoCusto::Zero,
                TipoDeProvedor::ClaudeCli => NaturezaDoCusto::Nominal,
                TipoDeProvedor::OpenRouter
                | TipoDeProvedor::Anthropic
                | TipoDeProvedor::Compativel => NaturezaDoCusto::Real,
            },
        }
    }

    fn montar(self) -> Result<ProvedorDaRita, String> {
        match self {
            Self::Pronto(p) => Ok(p),
            Self::Configurado {
                configurado,
                guarda,
                programa,
            } => sbf_rita::construir(&configurado, &guarda, &programa).map_err(|e| e.to_string()),
        }
    }
}

/// O pedido, na thread à parte: registro aberto (sem ele nada é enviado, ADR-015), provedor
/// montado e o laço.
fn executar_o_pedido(
    ambiente: &Ambiente,
    fonte: FonteDoProvedor,
    rita: &mut Rita,
    tela: &DadosDaTela,
    pedido: &str,
    parar: &AtomicBool,
    tx: &std::sync::mpsc::Sender<Msg>,
) -> Result<Relatorio, String> {
    let registro = sbf_rita::abrir_registro(&ambiente.cofre).map_err(|e| e.to_string())?;
    let executor = Arc::clone(&ambiente.executor);
    match fonte.montar()? {
        ProvedorDaRita::Claude(c) => c
            .pedir(tela, pedido, parar, &registro, executor)
            .map_err(|e| e.to_string()),
        ProvedorDaRita::Conversa(p) => rita
            .pedir(p.as_ref(), pedido, parar, &registro, &mut |e| match e {
                Evento::Chamada(c) => {
                    let _ = tx.send(Msg::Chamada(c.clone()));
                }
                Evento::Fala(t) => {
                    let _ = tx.send(Msg::Fala(t.to_owned()));
                }
                Evento::Consumo(c) => {
                    let _ = tx.send(Msg::Consumo(c));
                }
            })
            .map_err(|e| e.to_string()),
    }
}

enum Msg {
    Chamada(ChamadaFeita),
    Fala(String),
    Consumo(Consumo),
    Fim(Box<Result<Relatorio, String>>, Option<Box<Rita>>),
}

struct Andamento {
    chave: String,
    rx: Receiver<Msg>,
    parar: Arc<AtomicBool>,
    inicio: Instant,
}

/// O estado do painel.
#[derive(Default)]
pub struct RitaUi {
    ambiente: Option<Ambiente>,
    /// Destacada numa janela flutuante.
    pub flutuante: bool,
    pedido: String,
    escolhido: Option<String>,
    conversas: HashMap<String, Conversa>,
    andamento: Option<Andamento>,
    /// Rita gravou um rascunho desde a última vez que a janela perguntou.
    gravou: bool,
}

impl std::fmt::Debug for RitaUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RitaUi")
            .field("ligada", &self.ambiente.is_some())
            .field("trabalhando", &self.andamento.is_some())
            .finish_non_exhaustive()
    }
}

fn milhar(n: u64) -> String {
    let d = n.to_string();
    let mut s = String::new();
    for (i, c) in d.chars().enumerate() {
        if i > 0 && (d.len() - i).is_multiple_of(3) {
            s.push('.');
        }
        s.push(c);
    }
    s
}

/// O custo em palavras: real (US$), nominal (assinatura), zero (local) ou não informado (o
/// provedor cobra por token, mas não disse quanto: nunca "US$ 0").
pub fn texto_do_custo(c: &Consumo, natureza: NaturezaDoCusto) -> String {
    let usd = c.custo_dezmilesimos_usd;
    let valor = format!("US$ {},{:04}", usd / 10_000, usd % 10_000);
    match natureza {
        NaturezaDoCusto::Zero => "local, sem custo".to_owned(),
        NaturezaDoCusto::Nominal => format!("≈ {valor} (nominal, assinatura)"),
        NaturezaDoCusto::Real => valor,
        NaturezaDoCusto::NaoInformado => "custo não informado pelo provedor".to_owned(),
    }
}

fn texto_dos_tokens(c: &Consumo) -> String {
    format!(
        "{} tokens de entrada · {} de saída",
        milhar(c.tokens_entrada),
        milhar(c.tokens_saida)
    )
}

/// A linha de tokens e custo de uma resposta, com a natureza do custo dela.
pub fn texto_do_consumo(c: &Consumo, natureza: NaturezaDoCusto) -> String {
    format!("{} · {}", texto_dos_tokens(c), texto_do_custo(c, natureza))
}

/// O total do item: os tokens somados e, de cada natureza de custo, a parte dela.
fn texto_do_total(totais: &[(NaturezaDoCusto, Consumo)]) -> String {
    let todos = totais
        .iter()
        .fold(Consumo::default(), |a, (_, c)| soma(a, *c));
    let custos: Vec<String> = totais.iter().map(|(n, c)| texto_do_custo(c, *n)).collect();
    format!("{} · {}", texto_dos_tokens(&todos), custos.join(" + "))
}

fn soma(a: Consumo, b: Consumo) -> Consumo {
    Consumo {
        tokens_entrada: a.tokens_entrada + b.tokens_entrada,
        tokens_saida: a.tokens_saida + b.tokens_saida,
        custo_dezmilesimos_usd: a.custo_dezmilesimos_usd + b.custo_dezmilesimos_usd,
        nominal: a.nominal || b.nominal,
    }
}

impl RitaUi {
    /// Liga o painel ao cofre de verdade (a demonstração não liga: Rita avisa).
    pub fn ligar(&mut self, ambiente: Ambiente) {
        self.ambiente = Some(ambiente);
    }

    pub fn ligada(&self) -> bool {
        self.ambiente.is_some()
    }

    /// A pasta do cofre, quando ligada.
    pub fn cofre(&self) -> Option<PathBuf> {
        self.ambiente.as_ref().map(|a| a.cofre.clone())
    }

    /// Rita gravou um rascunho desde a última pergunta (a lista de padrões precisa reler).
    pub fn tomar_gravou(&mut self) -> bool {
        std::mem::take(&mut self.gravou)
    }

    pub fn trabalhando(&self) -> bool {
        self.andamento.is_some()
    }

    /// O provedor escolhido (o principal da configuração, se o usuário não trocou).
    pub fn provedor_escolhido(&self) -> Option<String> {
        let a = self.ambiente.as_ref()?;
        self.escolhido
            .clone()
            .or_else(|| a.guarda.ler().o_padrao().map(|p| p.id.clone()))
    }

    pub fn escolher_provedor(&mut self, id: &str) {
        self.escolhido = Some(id.to_owned());
    }

    /// O cartão da conversa do item, se houver.
    pub fn cartao(&self, chave: &str) -> Option<&Cartao> {
        self.conversas.get(chave)?.cartao.as_ref()
    }

    /// Quantas mensagens a conversa do item mostra (para os testes).
    pub fn baloes(&self, chave: &str) -> usize {
        self.conversas.get(chave).map_or(0, |c| c.baloes.len())
    }

    /// A linha de tokens e custo da **última resposta** do item, com a natureza dela.
    pub fn linha_de_consumo(&self, chave: &str) -> Option<String> {
        let (consumo, natureza) = self.conversas.get(chave)?.ultima?;
        Some(format!(
            "Última resposta: {}",
            texto_do_consumo(&consumo, natureza)
        ))
    }

    /// A linha do **total do item** (as respostas de todos os pedidos da conversa), só quando
    /// há mais de uma resposta: com uma só, o total é a última resposta.
    pub fn linha_do_total(&self, chave: &str) -> Option<String> {
        let c = self.conversas.get(chave)?;
        (c.respostas > 1).then(|| format!("Total do item: {}", texto_do_total(&c.totais)))
    }

    /// Manda o pedido. A chamada ao provedor roda numa thread à parte.
    ///
    /// # Errors
    /// Texto para a tela: sem cofre, já trabalhando, pedido vazio, provedor que não monta.
    pub fn pedir(&mut self, item: &ItemDaTela, pedido: &str) -> Result<(), String> {
        let a = self
            .ambiente
            .clone()
            .ok_or("Rita só funciona com o cofre aberto.")?;
        let pedido = pedido.trim();
        if pedido.is_empty() {
            return Err("Escreva o que você quer.".to_owned());
        }
        if self.andamento.is_some() {
            return Err("Rita já está trabalhando num pedido.".to_owned());
        }
        let config = a.guarda.ler();
        let id = self
            .escolhido
            .clone()
            .or_else(|| config.o_padrao().map(|p| p.id.clone()))
            .ok_or("Nenhum provedor configurado (Conexões, aba IA).")?;
        let configurado = config
            .por_id(&id)
            .cloned()
            .ok_or("O provedor escolhido não existe mais (Conexões, aba IA).")?;
        // O provedor é montado na thread do pedido, não aqui.
        self.iniciar(
            item,
            pedido,
            FonteDoProvedor::Configurado {
                configurado,
                guarda: Arc::clone(&a.guarda),
                programa: a.programa.clone(),
            },
        );
        Ok(())
    }

    /// Começa o pedido com um provedor já montado (a thread à parte).
    pub fn iniciar_com(&mut self, item: &ItemDaTela, pedido: &str, provedor: ProvedorDaRita) {
        self.iniciar(item, pedido, FonteDoProvedor::Pronto(provedor));
    }

    fn iniciar(&mut self, item: &ItemDaTela, pedido: &str, fonte: FonteDoProvedor) {
        let Some(a) = self.ambiente.clone() else {
            return;
        };
        let conversa = self
            .conversas
            .entry(item.chave.clone())
            .or_insert_with(Conversa::nova);
        let mut rita = conversa.rita.take().unwrap_or_else(|| {
            Rita::nova(Contexto::novo(item.tela.clone(), Arc::clone(&a.executor)))
        });
        rita.trocar_tela(item.tela.clone());
        conversa.baloes.push(Balao::Usuario(pedido.to_owned()));
        conversa.natureza = fonte.natureza();
        conversa.ultima = None;
        let (tx, rx) = channel();
        let parar = Arc::new(AtomicBool::new(false));
        let parar_da_thread = Arc::clone(&parar);
        let tela = item.tela.clone();
        let pedido_da_thread = pedido.to_owned();
        std::thread::spawn(move || {
            let r = executar_o_pedido(
                &a,
                fonte,
                &mut rita,
                &tela,
                &pedido_da_thread,
                &parar_da_thread,
                &tx,
            );
            let _ = tx.send(Msg::Fim(Box::new(r), Some(Box::new(rita))));
        });
        self.andamento = Some(Andamento {
            chave: item.chave.clone(),
            rx,
            parar,
            inicio: Instant::now(),
        });
    }

    /// Manda parar (o botão Parar).
    pub fn parar(&mut self) {
        if let Some(a) = &self.andamento {
            a.parar.store(true, Ordering::Release);
        }
    }

    /// Recolhe o que a thread mandou. Chame a cada quadro; devolve `true` se algo mudou.
    pub fn sondar(&mut self) -> bool {
        let Some(andamento) = self.andamento.take() else {
            return false;
        };
        let mut mudou = false;
        let mut terminou = false;
        loop {
            match andamento.rx.try_recv() {
                Ok(m) => {
                    mudou = true;
                    if self.receber(&andamento, m) {
                        terminou = true;
                        break;
                    }
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    // A thread caiu sem avisar: a conversa perde o estado, mas o painel volta.
                    if let Some(c) = self.conversas.get_mut(&andamento.chave) {
                        c.baloes
                            .push(Balao::Aviso("Rita parou de repente.".to_owned()));
                    }
                    mudou = true;
                    terminou = true;
                    break;
                }
            }
        }
        if !terminou {
            self.andamento = Some(andamento);
        }
        mudou
    }

    /// `true` quando a mensagem encerra o pedido.
    fn receber(&mut self, a: &Andamento, m: Msg) -> bool {
        let Some(c) = self.conversas.get_mut(&a.chave) else {
            return matches!(m, Msg::Fim(..));
        };
        match m {
            Msg::Chamada(ch) => c.baloes.push(Balao::Chamada(ch)),
            Msg::Fala(t) => c.baloes.push(Balao::Rita(t)),
            Msg::Consumo(k) => c.ultima = Some((k, c.natureza)),
            Msg::Fim(resultado, rita) => {
                c.rita = rita.map(|r| *r);
                c.duracao = Some(a.inicio.elapsed());
                match *resultado {
                    Err(e) => {
                        // O que já saiu antes da falha conta no total.
                        if let Some((k, n)) = c.ultima {
                            c.somar_ao_total(n, k);
                        }
                        c.baloes.push(Balao::Aviso(e));
                    }
                    Ok(r) => {
                        c.natureza = r.natureza;
                        c.ultima = Some((r.consumo, r.natureza));
                        c.somar_ao_total(r.natureza, r.consumo);
                        c.modelo.clone_from(&r.modelo);
                        // O `claude` faz as chamadas em outro processo: elas só chegam agora.
                        if r.natureza == NaturezaDoCusto::Nominal {
                            c.baloes
                                .extend(r.chamadas.iter().cloned().map(Balao::Chamada));
                        }
                        let ja_disse =
                            matches!(c.baloes.last(), Some(Balao::Rita(t)) if *t == r.resposta);
                        if r.parada != Parada::Respondeu || !ja_disse {
                            c.baloes.push(Balao::Rita(r.resposta.clone()));
                        }
                        self.gravou |= !r.rascunhos.is_empty();
                        if let Some(m) = r.melhor() {
                            c.cartao = Some(Cartao {
                                regra: m.regra.clone(),
                                versao: m.versao,
                                nome: m.nome.clone(),
                                pre_teste: m.pre_teste.clone(),
                                estado: EstadoDoCartao::Aberto,
                                confirmando: false,
                                erro: None,
                            });
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    /// A janela ativou (ou falhou ao ativar) o rascunho do cartão.
    pub fn ativacao_feita(&mut self, regra: &str, versao: u32, resultado: Result<(), String>) {
        for c in self.conversas.values_mut() {
            if let Some(k) = c
                .cartao
                .as_mut()
                .filter(|k| k.regra == regra && k.versao == versao)
            {
                k.confirmando = false;
                match &resultado {
                    Ok(()) => {
                        k.estado = EstadoDoCartao::Ativado;
                        k.erro = None;
                    }
                    Err(e) => k.erro = Some(e.clone()),
                }
            }
        }
    }

    /// Descarta o cartão do item (só some do painel).
    pub fn descartar(&mut self, chave: &str) {
        if let Some(k) = self
            .conversas
            .get_mut(chave)
            .and_then(|c| c.cartao.as_mut())
        {
            k.estado = EstadoDoCartao::Descartado;
            k.confirmando = false;
        }
    }

    // ---- desenho ----

    /// O conteúdo do painel (dock ou janela flutuante).
    pub fn desenhar(&mut self, ui: &mut Ui, item: Option<&ItemDaTela>) -> Vec<AcaoDeRita> {
        let p = tema::paleta(ui.ctx());
        let mut acoes = Vec::new();
        if self.sondar() || self.andamento.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(120));
        }
        self.cabecalho(ui);
        ui.separator();
        if self.ambiente.is_none() {
            ui.label(
                RichText::new("Rita só funciona com o cofre aberto (fora da demonstração).")
                    .color(p.tinta_suave),
            );
            return acoes;
        }
        let Some(item) = item else {
            self.sem_item(ui);
            return acoes;
        };
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new("Item:").color(p.tinta_suave));
            ui.label(RichText::new(&item.rotulo).strong());
            ui.label(RichText::new(format!("· {}", item.tela.area)).color(p.tinta_suave));
        });
        let altura_rodape = 120.0;
        let altura = (ui.available_height() - altura_rodape).max(80.0);
        ScrollArea::vertical()
            .id_salt("rita-conversa")
            .max_height(altura)
            .stick_to_bottom(true)
            .auto_shrink([false, false])
            .show(ui, |ui| self.conversa_ui(ui, item, &mut acoes));
        ui.separator();
        self.rodape(ui, item);
        acoes
    }

    fn cabecalho(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        ui.horizontal(|ui| {
            ui.label(icones::icone_cor("smart_toy", 18.0, p.destaque));
            ui.label(RichText::new("Rita").strong().size(15.0));
            if let Some(a) = self.ambiente.clone() {
                let config = a.guarda.ler();
                let atual = self.provedor_escolhido();
                let nome = atual
                    .as_deref()
                    .and_then(|id| config.por_id(id))
                    .map_or("sem provedor", |p| p.nome.as_str())
                    .to_owned();
                egui::ComboBox::from_id_salt("rita-provedor")
                    .selected_text(nome)
                    .show_ui(ui, |ui| {
                        for pr in &config.provedores {
                            let marcado = atual.as_deref() == Some(pr.id.as_str());
                            let rotulo = format!("{} · {}", pr.nome, pr.modelo);
                            if ui.selectable_label(marcado, rotulo).clicked() {
                                self.escolhido = Some(pr.id.clone());
                            }
                        }
                    });
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let (icone, dica) = if self.flutuante {
                    ("close", "Voltar para o painel do dock")
                } else {
                    ("open_in_new", "Destacar numa janela flutuante")
                };
                if componentes::botao_icone(ui, icone, dica, true).clicked() {
                    self.flutuante = !self.flutuante;
                }
            });
        });
    }

    fn sem_item(&self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Selecione um documento (ou um registro do banco) e peça: Rita lê o que está na tela, cria o rascunho de um padrão de reconhecimento e faz o pré-teste. Quem ativa é você.",
            )
            .color(p.tinta_suave),
        );
        ui.add_space(4.0);
        ui.label(
            RichText::new(
                "Exemplo: \"o título e o número do contrato determinam o padrão; a data determina a competência; cria\".",
            )
            .italics()
            .color(p.tinta_suave),
        );
    }

    fn conversa_ui(&mut self, ui: &mut Ui, item: &ItemDaTela, acoes: &mut Vec<AcaoDeRita>) {
        let p = tema::paleta(ui.ctx());
        let trabalhando = self
            .andamento
            .as_ref()
            .is_some_and(|a| a.chave == item.chave);
        let Some(c) = self.conversas.get_mut(&item.chave) else {
            self.sem_item(ui);
            return;
        };
        for b in &c.baloes {
            match b {
                Balao::Usuario(t) => {
                    componentes::cartao(ui, |ui| {
                        ui.label(RichText::new("Você").size(11.5).color(p.tinta_suave));
                        ui.label(t);
                    });
                }
                Balao::Rita(t) => {
                    ui.label(RichText::new("Rita").size(11.5).color(p.destaque));
                    ui.label(t);
                }
                Balao::Chamada(ch) => {
                    let cor = if ch.recusada { p.perigo } else { p.tinta_suave };
                    ui.label(
                        RichText::new(format!(
                            "{} {}{}",
                            ch.numero,
                            ch.nome,
                            if ch.recusada { " (recusada)" } else { "" }
                        ))
                        .size(11.5)
                        .color(cor),
                    )
                    .on_hover_text(&ch.resultado);
                }
                Balao::Aviso(t) => {
                    componentes::aviso(ui, "warning", t, p.aviso_sutil, p.aviso_borda, p.aviso);
                }
            }
            ui.add_space(4.0);
        }
        if trabalhando {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(RichText::new("Rita está trabalhando…").color(p.tinta_suave));
            });
        }
        if let Some(k) = c
            .cartao
            .as_mut()
            .filter(|k| k.estado != EstadoDoCartao::Descartado)
        {
            cartao_do_rascunho(ui, k, acoes);
        }
    }

    fn rodape(&mut self, ui: &mut Ui, item: &ItemDaTela) {
        let p = tema::paleta(ui.ctx());
        let trabalhando = self.andamento.is_some();
        let resposta = ui.add_enabled(
            !trabalhando,
            egui::TextEdit::multiline(&mut self.pedido)
                .desired_rows(2)
                .desired_width(f32::INFINITY)
                .hint_text("O que você quer que Rita faça com este item?"),
        );
        let enviar_com_atalho = resposta.has_focus()
            && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::Enter));
        ui.horizontal(|ui| {
            if trabalhando {
                if ui
                    .button((icones::icone_cor("cancel", 15.0, p.perigo), "Parar"))
                    .clicked()
                {
                    self.parar();
                }
            } else {
                let vazio = self.pedido.trim().is_empty();
                let clique = ui
                    .add_enabled(
                        !vazio,
                        egui::Button::new((
                            icones::icone_cor("send", 15.0, p.sobre_destaque),
                            "Enviar",
                        ))
                        .fill(p.destaque),
                    )
                    .on_hover_text("Ctrl+Enter também envia")
                    .clicked();
                if (clique || enviar_com_atalho) && !vazio {
                    let pedido = std::mem::take(&mut self.pedido);
                    if let Err(e) = self.pedir(item, &pedido) {
                        if let Some(c) = self.conversas.get_mut(&item.chave) {
                            c.baloes.push(Balao::Aviso(e));
                        } else {
                            let mut c = Conversa::nova();
                            c.baloes.push(Balao::Aviso(e));
                            self.conversas.insert(item.chave.clone(), c);
                        }
                        self.pedido = pedido;
                    }
                }
            }
        });
        for linha in [
            self.linha_de_consumo(&item.chave),
            self.linha_do_total(&item.chave),
        ]
        .into_iter()
        .flatten()
        {
            ui.label(RichText::new(linha).size(11.5).color(p.tinta_suave));
        }
        if let Some(a) = self.andamento.as_ref().filter(|a| a.chave != item.chave) {
            let _ = a;
            ui.label(
                RichText::new("Rita está trabalhando em outro item.")
                    .size(11.5)
                    .color(p.tinta_suave),
            );
        }
    }
}

fn cartao_do_rascunho(ui: &mut Ui, k: &mut Cartao, acoes: &mut Vec<AcaoDeRita>) {
    let p = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(icones::icone_cor("edit", 16.0, p.destaque));
            ui.label(RichText::new(&k.nome).strong());
            ui.label(
                RichText::new(format!("{} · versão {} · rascunho", k.regra, k.versao))
                    .size(11.5)
                    .color(p.tinta_suave),
            );
        });
        let (rotulo, cor) = if k.pre_teste.passou {
            ("Passou no pré-teste", p.sucesso)
        } else {
            ("Não passou no pré-teste", p.aviso)
        };
        ui.label(RichText::new(rotulo).color(cor).strong());
        for l in k.pre_teste.linhas() {
            ui.label(RichText::new(l).size(12.5));
        }
        if !k.pre_teste.passou && !k.pre_teste.motivo.is_empty() {
            ui.label(RichText::new(&k.pre_teste.motivo).size(12.5).color(p.aviso));
        }
        if let Some(e) = &k.erro {
            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
        }
        ui.add_space(4.0);
        match k.estado {
            EstadoDoCartao::Ativado => {
                ui.label(
                    RichText::new("Ativado por você: o padrão passou a propor.").color(p.sucesso),
                );
            }
            EstadoDoCartao::Aberto => botoes_do_cartao(ui, k, acoes),
            EstadoDoCartao::Descartado => {}
        }
    });
}

fn botoes_do_cartao(ui: &mut Ui, k: &mut Cartao, acoes: &mut Vec<AcaoDeRita>) {
    let p = tema::paleta(ui.ctx());
    if k.confirmando {
        let mut motivos = Vec::new();
        for a in &k.pre_teste.empates {
            motivos.push(format!(
                "empata com `{}` em {} documentos",
                a.regra, a.quantos
            ));
        }
        for a in &k.pre_teste.derrubadas {
            motivos.push(format!("tira documentos de `{}` ({})", a.regra, a.quantos));
        }
        if k.pre_teste.perdem > 0 {
            motivos.push(format!(
                "deixa de reconhecer {} que a versão em vigor reconhece",
                k.pre_teste.perdem
            ));
        }
        if motivos.is_empty() {
            motivos.push("empata com outro padrão neste item".to_owned());
        }
        componentes::aviso(
            ui,
            "warning",
            &format!(
                "O pré-teste mostrou que este padrão {}. Ativar mesmo assim?",
                motivos.join("; ")
            ),
            p.aviso_sutil,
            p.aviso_borda,
            p.aviso,
        );
        ui.horizontal(|ui| {
            if ui.button("Ativar mesmo assim").clicked() {
                acoes.push(AcaoDeRita::Ativar {
                    regra: k.regra.clone(),
                    versao: k.versao,
                });
            }
            if ui.button("Cancelar").clicked() {
                k.confirmando = false;
            }
        });
        return;
    }
    ui.horizontal_wrapped(|ui| {
        if ui
            .add(
                egui::Button::new((icones::icone_cor("check_circle", 15.0, p.sobre_destaque), "Ativar"))
                    .fill(p.destaque),
            )
            .on_hover_text("Ativa como você: o padrão passa a reconhecer e propor. Os exemplos precisam passar.")
            .clicked()
        {
            if k.pre_teste.pede_confirmacao() {
                k.confirmando = true;
            } else {
                acoes.push(AcaoDeRita::Ativar {
                    regra: k.regra.clone(),
                    versao: k.versao,
                });
            }
        }
        if ui
            .button((icones::icone_cor("edit", 15.0, p.tinta), "Abrir no editor"))
            .clicked()
        {
            acoes.push(AcaoDeRita::AbrirNoEditor {
                regra: k.regra.clone(),
            });
        }
        if ui
            .button((icones::icone_cor("delete", 15.0, p.tinta), "Descartar"))
            .on_hover_text("Tira o cartão do painel. O rascunho fica no histórico do padrão e nunca roda.")
            .clicked()
        {
            k.estado = EstadoDoCartao::Descartado;
        }
    });
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_ia::{ChamadaDeFerramenta, FalsoDeConversa, RespostaDeConversa};
    use sbf_regras::catalogo::Catalogo;
    use sbf_rita::{GuardaEmMemoria, entrada_de_documento};
    use serde_json::json;

    const TEXTO: &str = "Comprovante Pix\nBanco Exemplo\nValor: R$ 150,00\nData: 10/11/2026\n";

    fn ambiente(cofre: &std::path::Path) -> Ambiente {
        Catalogo::abrir(&cofre.join("regras.sqlite")).expect("catálogo");
        sbf_rita::registro::inventario_minimo_para_testes(cofre);
        Ambiente {
            cofre: cofre.to_path_buf(),
            guarda: Arc::new(GuardaEmMemoria::default()),
            executor: Arc::new(sbf_script_falso::Falso),
            programa: PathBuf::from("/usr/bin/sbf"),
        }
    }

    /// Um executor de scripts que não roda nada (estes testes não usam script).
    mod sbf_script_falso {
        use sbf_regras::script::{
            ErroDoScript, ExecutorDeScript, PedidoDeScript, RespostaDoScript,
        };

        #[derive(Debug)]
        pub struct Falso;

        impl ExecutorDeScript for Falso {
            fn executar(&self, _: &PedidoDeScript) -> Result<RespostaDoScript, ErroDoScript> {
                Err(ErroDoScript::Processo("sem scripts".to_owned()))
            }
            fn verificar(&self, _: &str) -> Result<(), ErroDoScript> {
                Ok(())
            }
        }
    }

    fn item(cofre: &std::path::Path) -> ItemDaTela {
        let e = entrada_de_documento(
            &"ab".repeat(32),
            Some("c.txt"),
            Some("text/plain"),
            Some(TEXTO),
        );
        ItemDaTela {
            chave: "ab".repeat(32),
            rotulo: "c.txt".to_owned(),
            tela: DadosDaTela::de_documento(cofre, e),
        }
    }

    fn esperar(r: &mut RitaUi) {
        let t0 = Instant::now();
        while r.trabalhando() {
            r.sondar();
            assert!(t0.elapsed() < Duration::from_secs(20), "demorou");
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    #[test]
    fn texto_do_consumo_diz_real_nominal_ou_local() {
        let c = Consumo {
            tokens_entrada: 12_345,
            tokens_saida: 67,
            custo_dezmilesimos_usd: 123,
            nominal: false,
        };
        assert_eq!(
            texto_do_consumo(&c, NaturezaDoCusto::Real),
            "12.345 tokens de entrada · 67 de saída · US$ 0,0123"
        );
        assert!(texto_do_consumo(&c, NaturezaDoCusto::Nominal).contains("nominal"));
        assert!(texto_do_consumo(&c, NaturezaDoCusto::Zero).ends_with("local, sem custo"));
        let nao = texto_do_consumo(&c, NaturezaDoCusto::NaoInformado);
        assert!(nao.ends_with("custo não informado pelo provedor"), "{nao}");
        assert!(!nao.contains("US$"), "nunca US$ 0 quando o custo não veio");
    }

    #[test]
    fn o_total_do_item_soma_por_natureza_e_a_ultima_resposta_fica_separada() {
        let mut c = Conversa::nova();
        let k = |e, s, custo| Consumo {
            tokens_entrada: e,
            tokens_saida: s,
            custo_dezmilesimos_usd: custo,
            nominal: false,
        };
        c.somar_ao_total(NaturezaDoCusto::Real, k(1000, 100, 120));
        c.somar_ao_total(NaturezaDoCusto::NaoInformado, k(500, 50, 0));
        c.somar_ao_total(NaturezaDoCusto::Real, k(200, 20, 30));
        c.ultima = Some((k(200, 20, 30), NaturezaDoCusto::Real));
        let mut r = RitaUi::default();
        r.conversas.insert("i".into(), c);
        assert_eq!(
            r.linha_de_consumo("i").as_deref(),
            Some("Última resposta: 200 tokens de entrada · 20 de saída · US$ 0,0030")
        );
        assert_eq!(
            r.linha_do_total("i").as_deref(),
            Some(
                "Total do item: 1.700 tokens de entrada · 170 de saída · US$ 0,0150 + custo não informado pelo provedor"
            )
        );
    }

    #[test]
    fn sem_cofre_ligado_o_pedido_e_recusado_com_texto() {
        let mut r = RitaUi::default();
        let d = tempfile::tempdir().expect("pasta");
        let e = r.pedir(&item(d.path()), "cria").expect_err("sem cofre");
        assert!(e.contains("cofre"));
    }

    #[test]
    fn pedido_vazio_nao_inicia_thread() {
        let d = tempfile::tempdir().expect("pasta");
        let mut r = RitaUi::default();
        r.ligar(ambiente(d.path()));
        assert!(r.pedir(&item(d.path()), "   ").is_err());
        assert!(!r.trabalhando());
    }

    #[test]
    fn o_laco_roda_numa_thread_e_o_cartao_nasce_com_o_pre_teste() {
        let d = tempfile::tempdir().expect("pasta");
        let mut r = RitaUi::default();
        r.ligar(ambiente(d.path()));
        // O provedor local de verdade apontaria para o Ollama: aqui injetamos o falso pela
        // mesma porta que a thread usa, montando a conversa à mão.
        let definicao = json!({
            "tipo_documental": "pix-banco-exemplo",
            "detectar": {"todos": [{"contem": "comprovante pix"}]},
            "extrair": {"valor_centavos": {"regex": "Valor:\\s*R\\$\\s*([\\d.,]+)", "formato": "centavos"}},
            "obrigatorios": ["valor_centavos"]
        });
        let falso = Arc::new(FalsoDeConversa::com_roteiro(vec![Ok(RespostaDeConversa {
            custo_ausente: false,
            texto: None,
            chamadas: vec![ChamadaDeFerramenta {
                id: "1".into(),
                nome: "propor_padrao".into(),
                argumentos: json!({"regra": "pix-banco-exemplo", "nome": "Pix Banco Exemplo", "definicao": definicao}),
            }],
            consumo: Consumo {
                tokens_entrada: 900,
                tokens_saida: 40,
                ..Consumo::default()
            },
            modelo: "falso-1".into(),
        })]));
        r.iniciar_com(
            &item(d.path()),
            "cria o padrão",
            ProvedorDaRita::Conversa(falso),
        );
        assert!(r.trabalhando());
        esperar(&mut r);
        let chave = "ab".repeat(32);
        let k = r.cartao(&chave).expect("cartão");
        assert_eq!(k.regra, "pix-banco-exemplo");
        assert!(k.pre_teste.passou);
        assert_eq!(k.estado, EstadoDoCartao::Aberto);
        assert!(!k.pre_teste.pede_confirmacao());
        assert!(r.baloes(&chave) >= 3);
        assert_eq!(
            r.linha_de_consumo(&chave).as_deref(),
            Some("Última resposta: 900 tokens de entrada · 40 de saída · local, sem custo")
        );
        // A janela ativa e avisa o painel.
        r.ativacao_feita("pix-banco-exemplo", 1, Ok(()));
        assert_eq!(
            r.cartao(&chave).map(|k| k.estado),
            Some(EstadoDoCartao::Ativado)
        );
    }

    #[test]
    fn ativar_com_empate_pede_confirmacao_e_descartar_some_do_painel() {
        let mut k = Cartao {
            regra: "x".into(),
            versao: 1,
            nome: "X".into(),
            pre_teste: PreTeste {
                regra: "x".into(),
                versao: 1,
                item: sbf_rita::pre_teste::NoItem {
                    casa: true,
                    decisao: sbf_rita::pre_teste::DecisaoNoItem::Esta,
                    campos: vec![],
                    diagnostico: vec![],
                    condicoes_falhando: 0,
                    faltam: vec![],
                },
                exemplos: sbf_rita::pre_teste::DosExemplos {
                    total: 0,
                    passaram: 0,
                    falharam: vec![],
                },
                amostra: sbf_rita::pre_teste::DaAmostra {
                    examinados: 0,
                    casam: 0,
                    titulos: vec![],
                    repetidas: 0,
                },
                empates: vec![sbf_rita::pre_teste::Afetado {
                    regra: "y".into(),
                    quantos: 2,
                }],
                derrubadas: vec![],
                novos: 0,
                perdem: 0,
                passou: true,
                motivo: String::new(),
            },
            estado: EstadoDoCartao::Aberto,
            confirmando: false,
            erro: None,
        };
        assert!(k.pre_teste.pede_confirmacao());
        // Sem empate, o botão ativa direto.
        k.pre_teste.empates.clear();
        assert!(!k.pre_teste.pede_confirmacao());
        let mut r = RitaUi::default();
        let mut c = Conversa::nova();
        c.cartao = Some(k);
        r.conversas.insert("i".into(), c);
        r.descartar("i");
        assert_eq!(
            r.cartao("i").map(|k| k.estado),
            Some(EstadoDoCartao::Descartado)
        );
    }

    #[test]
    fn sem_o_registro_do_que_sai_nada_e_enviado_e_o_painel_diz_o_porque() {
        let d = tempfile::tempdir().expect("pasta");
        let mut r = RitaUi::default();
        let mut a = ambiente(d.path());
        // O inventário some: o registro não abre.
        std::fs::remove_file(d.path().join("inventario.sqlite")).expect("remover");
        a.cofre = d.path().to_path_buf();
        r.ligar(a);
        let falso = Arc::new(FalsoDeConversa::com_roteiro(vec![]));
        r.iniciar_com(
            &item(d.path()),
            "cria",
            ProvedorDaRita::Conversa(falso.clone()),
        );
        esperar(&mut r);
        assert_eq!(falso.chamadas(), 0, "o provedor não foi chamado");
        let chave = "ab".repeat(32);
        let avisos: Vec<_> = r
            .conversas
            .get(&chave)
            .expect("conversa")
            .baloes
            .iter()
            .filter_map(|b| match b {
                Balao::Aviso(t) => Some(t.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(avisos.len(), 1);
        assert!(avisos[0].contains("nada foi enviado"), "{avisos:?}");
    }

    /// Uma guarda cujo provedor local aponta para um endereço que o provedor recusa.
    struct GuardaRuim;

    impl GuardaDeProvedores for GuardaRuim {
        fn ler(&self) -> sbf_rita::ConfiguracaoDeRita {
            let mut c = sbf_rita::ConfiguracaoDeRita::inicial();
            c.provedores[0].endereco = Some("http://localhost.exemplo.com/v1".into());
            c
        }
        fn gravar(&self, _: &sbf_rita::ConfiguracaoDeRita) -> Result<(), String> {
            Ok(())
        }
        fn chave(&self, _: &str) -> Option<sbf_chaveiro::Segredo> {
            None
        }
        fn guardar_chave(&self, _: &str, _: sbf_chaveiro::Segredo) -> Result<(), String> {
            Ok(())
        }
        fn apagar_chave(&self, _: &str) -> Result<(), String> {
            Ok(())
        }
    }

    #[test]
    fn o_provedor_so_e_montado_na_thread_do_pedido() {
        let d = tempfile::tempdir().expect("pasta");
        let mut r = RitaUi::default();
        let mut a = ambiente(d.path());
        a.guarda = Arc::new(GuardaRuim);
        r.ligar(a);
        // `pedir` volta na hora, sem montar nada na thread da interface; a falha de montar
        // (endereço recusado) chega depois, como aviso da conversa.
        assert!(r.pedir(&item(d.path()), "cria").is_ok());
        assert!(r.trabalhando() || r.baloes(&"ab".repeat(32)) >= 1);
        esperar(&mut r);
        let aviso = r
            .conversas
            .get(&"ab".repeat(32))
            .expect("conversa")
            .baloes
            .iter()
            .find_map(|b| match b {
                Balao::Aviso(t) => Some(t.clone()),
                _ => None,
            })
            .expect("aviso");
        assert!(aviso.contains("https"), "{aviso}");
    }

    #[test]
    fn parar_interrompe_o_pedido() {
        let d = tempfile::tempdir().expect("pasta");
        let mut r = RitaUi::default();
        r.ligar(ambiente(d.path()));
        let lento = Arc::new(FalsoDeConversa::com_funcao(|_| {
            std::thread::sleep(Duration::from_millis(300));
            Ok(RespostaDeConversa {
                custo_ausente: false,
                texto: None,
                chamadas: vec![ChamadaDeFerramenta {
                    id: "1".into(),
                    nome: "titulos_da_lista".into(),
                    argumentos: json!({}),
                }],
                consumo: Consumo::default(),
                modelo: "falso-1".into(),
            })
        }));
        r.iniciar_com(&item(d.path()), "olhe", ProvedorDaRita::Conversa(lento));
        r.parar();
        esperar(&mut r);
        let chave = "ab".repeat(32);
        assert!(r.baloes(&chave) >= 2);
        assert!(r.cartao(&chave).is_none());
    }
}
