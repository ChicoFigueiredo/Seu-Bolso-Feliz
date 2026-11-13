//! A Prévia do arquivo na janela (spec 06, "Prévia do arquivo"; glossário: Prévia).
//!
//! O mesmo conteúdo em quatro lugares, cada um um [`Posto`]:
//!
//! - **no canto:** o painel Prévia do dock, com a miniatura do item em foco;
//! - **na aba Arquivo** da página do item, grande, com páginas e zoom;
//! - **destacada:** numa janela própria do sistema (um viewport do egui);
//! - **na dica:** meio segundo com o mouse parado sobre o nome de um achado ou documento.
//!
//! Abrir e desenhar são lentos (PDF ~0,1 a 0,3 s por página, Office segundos): tudo roda
//! numa **thread de fundo** com uma fila de um pedido por posto. Um pedido novo do mesmo
//! posto substitui o que ainda não começou (mudar a seleção depressa só desenha o último).
//! O conteúdo aberto fica no [`sbf_previa::Cache`] (só em memória); as texturas do egui,
//! só das prévias desenhadas no quadro.
//!
//! O arquivo vem da cópia do acervo (`achado.caminho_acervo`, campo "No cofre" do item,
//! relativo à pasta do cofre). PDF protegido só abre com as candidatas das regras de senha,
//! montadas em memória a partir do chaveiro aberto; sem regra, mostra "Protegido por senha"
//! e o atalho Nova regra.

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use egui::{
    Align, Color32, CornerRadius, Frame, Id, Layout, Margin, Order, Rect, RichText, Sense, Shadow,
    Stroke, TextureHandle, TextureOptions, Ui, pos2, vec2,
};
use sbf_inventario::consulta::{Celula, PedidoDeTabela};
use sbf_inventario::{Ref, Tipo};
use sbf_previa::{Candidata, Conteudo, Email, EntradaDoZip, ErroDaPrevia, Familia, Imagem};

use crate::componentes;
use crate::formato;
use crate::icones;
use crate::leitura::Leitor;
use crate::navegacao::Pedido;
use crate::tema;

/// Quanto o mouse fica parado sobre um nome antes da dica com a miniatura.
pub const ESPERA_DA_DICA: f64 = 0.5;
/// A largura da miniatura da dica, em pontos.
pub const LARGURA_DA_DICA: f32 = 220.0;
/// A largura de uma página A4 a 96 dpi: o 100% do zoom.
const LARGURA_A4: f32 = 794.0;
/// A proporção de uma folha A4 (altura / largura), para o lugar da página que carrega.
const PROPORCAO_A4: f32 = 297.0 / 210.0;
/// O papel continua claro no tema escuro (é um documento); a moldura segue o tema.
const PAPEL: Color32 = Color32::from_rgb(0xfd, 0xfd, 0xfb);
const TINTA_DO_PAPEL: Color32 = Color32::from_rgb(0x2b, 0x34, 0x30);
const LINHA_DO_PAPEL: Color32 = Color32::from_rgb(0xe3, 0xe8, 0xe5);
/// Texto longo demais para a vista: corta aqui (o original fica intacto).
const TETO_DO_TEXTO: usize = 200_000;

/// Abre o arquivo e devolve o conteúdo (o [`sbf_previa::abrir`], ou um falso nos testes).
pub type Abridor =
    Arc<dyn Fn(sbf_previa::Pedido<'_>) -> Result<Conteudo, ErroDaPrevia> + Send + Sync>;

/// Monta as senhas candidatas de um PDF protegido (pelo `content_hash`), em memória. `Err`
/// traz o motivo para mostrar ("sem regra", "chaveiro trancado").
pub type GeradorDeCandidatas = Arc<dyn Fn(&str) -> Result<Vec<Candidata>, String> + Send + Sync>;

/// O motivo de um PDF protegido sem regra de senha.
pub const MOTIVO_SEM_REGRA: &str =
    "Nenhuma regra de senha abre este PDF. Crie uma regra para o fornecedor e o tipo dele.";

/// Onde a prévia está sendo mostrada. A fila guarda um pedido por posto.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Posto {
    /// O painel Prévia do dock.
    Painel,
    /// A aba Arquivo da página do achado (pelo `content_hash`).
    Aba(String),
    /// Uma janela destacada, pelo id.
    Destacada(u64),
    /// A dica sobre o nome numa tabela.
    Dica,
}

/// O arquivo de um achado, com o que a prévia precisa saber dele (sem ler o conteúdo).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArquivoDaPrevia {
    /// O achado (ou documento) dono do arquivo.
    pub achado: Ref,
    pub hash: String,
    pub nome: String,
    /// Sem ponto, minúscula.
    pub extensao: String,
    /// O tipo do documento, como o item mostra.
    pub tipo: Option<String>,
    pub fornecedor: Option<String>,
    pub data_chave: Option<String>,
    /// O caminho absoluto no acervo, ou por que não há.
    pub caminho: Result<PathBuf, String>,
    pub tamanho: Option<u64>,
    /// O item diz que é protegido por senha.
    pub protegido: bool,
}

impl ArquivoDaPrevia {
    pub fn familia(&self) -> Familia {
        sbf_previa::familia(&self.extensao)
    }
}

/// O que se sabe do conteúdo aberto de um arquivo (sem os bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Resumo {
    Paginas {
        total: usize,
    },
    Imagem,
    Email(Email),
    Texto(String),
    ListaDoZip(Vec<EntradaDoZip>),
    Protegido {
        motivo: String,
    },
    SemPrevia {
        motivo: String,
    },
    /// Não abriu: o motivo em palavras.
    Erro {
        motivo: String,
    },
}

/// Onde uma vista grande está: página, zoom e "caber na largura".
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EstadoDaVista {
    pub pagina: usize,
    /// 1.0 = 100% (a página A4 no tamanho real, a imagem pixel a pixel).
    pub zoom: f32,
    pub caber: bool,
}

impl Default for EstadoDaVista {
    fn default() -> Self {
        Self {
            pagina: 1,
            zoom: 1.0,
            caber: true,
        }
    }
}

/// Uma prévia destacada numa janela própria.
#[derive(Debug, Clone)]
pub struct Destacada {
    pub id: u64,
    pub achado: Ref,
    pub estado: EstadoDaVista,
}

type ChaveDaImagem = (String, usize, u32);

struct Trabalho {
    posto: Posto,
    arquivo: ArquivoDaPrevia,
    pagina: usize,
    largura: u32,
}

enum Resposta {
    Conteudo {
        hash: String,
        resumo: Resumo,
    },
    Imagem {
        chave: ChaveDaImagem,
        imagem: Result<Arc<Imagem>, String>,
    },
    /// O trabalhador pegou um pedido (para os testes saberem o que foi aberto).
    Comecou(String),
}

#[derive(Default)]
struct EstadoDaFila {
    trabalhos: VecDeque<Trabalho>,
    parar: bool,
    ocupado: bool,
}

#[derive(Default)]
struct Fila {
    estado: Mutex<EstadoDaFila>,
    sinal: Condvar,
}

struct Dica {
    alvo: Ref,
    retangulo: Rect,
    desde: f64,
}

/// A prévia do arquivo: a fila de fundo, o que já chegou e as vistas.
pub struct Previas {
    pasta: Option<PathBuf>,
    /// A pasta é temporária (a demonstração): apagada ao sair.
    temporaria: bool,
    abridor: Abridor,
    candidatas: Option<GeradorDeCandidatas>,
    cache: Arc<Mutex<sbf_previa::Cache>>,
    limites: (usize, usize),
    fila: Arc<Fila>,
    tx: Sender<Resposta>,
    rx: Receiver<Resposta>,
    trabalhador: bool,
    em_voo: HashMap<Posto, ChaveDaImagem>,
    resumos: HashMap<String, Resumo>,
    texturas: HashMap<ChaveDaImagem, (TextureHandle, u64)>,
    falhas: HashMap<ChaveDaImagem, String>,
    resolvidos: HashMap<Ref, Result<ArquivoDaPrevia, String>>,
    vistas: HashMap<String, EstadoDaVista>,
    destacadas: Vec<Destacada>,
    proximo_id: u64,
    dica: Option<Dica>,
    dica_visivel: Option<Ref>,
    aviso: Option<String>,
    quadro: u64,
    /// Os `content_hash` que o trabalhador começou a abrir, em ordem (testes).
    pub abertos: Vec<String>,
}

impl std::fmt::Debug for Previas {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Previas")
            .field("pasta", &self.pasta)
            .field("limites", &self.limites)
            .field("destacadas", &self.destacadas.len())
            .finish_non_exhaustive()
    }
}

impl Default for Previas {
    fn default() -> Self {
        Self::nova()
    }
}

impl Drop for Previas {
    fn drop(&mut self) {
        if let Ok(mut f) = self.fila.estado.lock() {
            f.parar = true;
            f.trabalhos.clear();
        }
        self.fila.sinal.notify_all();
        if self.temporaria
            && let Some(p) = &self.pasta
        {
            let _ = std::fs::remove_dir_all(p);
        }
    }
}

fn abrir_de_verdade() -> Abridor {
    Arc::new(sbf_previa::abrir)
}

impl Previas {
    /// Sem pasta do cofre: toda prévia mostra os metadados e o motivo.
    pub fn nova() -> Self {
        Self::com_abridor(abrir_de_verdade())
    }

    /// Com outro abridor (testes).
    pub fn com_abridor(abridor: Abridor) -> Self {
        let (tx, rx) = channel();
        let limites = sbf_previa::Cache::default().limites();
        Self {
            pasta: None,
            temporaria: false,
            abridor,
            candidatas: None,
            cache: Arc::new(Mutex::new(sbf_previa::Cache::default())),
            limites,
            fila: Arc::new(Fila::default()),
            tx,
            rx,
            trabalhador: false,
            em_voo: HashMap::new(),
            resumos: HashMap::new(),
            texturas: HashMap::new(),
            falhas: HashMap::new(),
            resolvidos: HashMap::new(),
            vistas: HashMap::new(),
            destacadas: Vec::new(),
            proximo_id: 0,
            dica: None,
            dica_visivel: None,
            aviso: None,
            quadro: 0,
            abertos: Vec::new(),
        }
    }

    /// A pasta do cofre (o `caminho_acervo` dos achados é relativo a ela).
    pub fn definir_pasta(&mut self, pasta: PathBuf) {
        self.pasta = Some(pasta);
        self.resolvidos.clear();
    }

    /// Uma pasta temporária com arquivos fictícios (demonstração): apagada ao sair.
    pub fn definir_pasta_temporaria(&mut self, pasta: PathBuf) {
        self.definir_pasta(pasta);
        self.temporaria = true;
    }

    /// O modo demonstração: gera os arquivos fictícios numa pasta temporária só sua
    /// (`$XDG_RUNTIME_DIR`, em memória, quando existe), apagada ao sair. Devolve quantos.
    pub fn preparar_demonstracao(
        &mut self,
        consulta: &crate::demonstracao::ConsultaDemonstracao,
    ) -> Result<usize, String> {
        let base = std::env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let pasta = base.join(format!("sbf-demonstracao-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&pasta);
        let mut construtor = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            construtor.mode(0o700);
        }
        construtor
            .create(&pasta)
            .map_err(|e| format!("não deu para criar a pasta da demonstração: {e}"))?;
        self.definir_pasta_temporaria(pasta.clone());
        consulta
            .gerar_acervo(&pasta)
            .map_err(|e| format!("não deu para gerar os arquivos fictícios: {e}"))
    }

    pub fn pasta(&self) -> Option<&Path> {
        self.pasta.as_deref()
    }

    /// Quem monta as candidatas de PDF protegido (o inventário real, com o chaveiro).
    pub fn definir_candidatas(&mut self, gerador: GeradorDeCandidatas) {
        self.candidatas = Some(gerador);
    }

    /// Os limites do cache: documentos e bytes.
    pub fn limites(&self) -> (usize, usize) {
        self.limites
    }

    /// Troca os limites do cache (Opções). O cache recomeça vazio.
    pub fn definir_limites(&mut self, documentos: usize, bytes: usize) {
        let documentos = documentos.max(1);
        let bytes = bytes.max(1024 * 1024);
        if (documentos, bytes) == self.limites {
            return;
        }
        self.limites = (documentos, bytes);
        if let Ok(mut c) = self.cache.lock() {
            *c = sbf_previa::Cache::novo(documentos, bytes);
        }
    }

    /// Esquece o que leu do inventário (depois de uma decisão ou de Atualizar). As
    /// prévias abertas continuam no cache: o arquivo não muda.
    pub fn limpar(&mut self) {
        self.resolvidos.clear();
        self.resumos
            .retain(|_, r| !matches!(r, Resumo::Erro { .. }));
        self.falhas.clear();
    }

    /// As prévias destacadas em janela própria.
    pub fn destacadas(&self) -> &[Destacada] {
        &self.destacadas
    }

    /// Destaca a prévia do achado numa janela própria do sistema.
    pub fn destacar(&mut self, achado: &Ref) {
        if self
            .destacadas
            .iter()
            .any(|d| d.achado.chave == achado.chave)
        {
            return;
        }
        self.proximo_id += 1;
        let estado = self.vistas.get(&achado.chave).copied().unwrap_or_default();
        self.destacadas.push(Destacada {
            id: self.proximo_id,
            achado: achado.clone(),
            estado,
        });
    }

    pub fn fechar_destacada(&mut self, id: u64) {
        self.destacadas.retain(|d| d.id != id);
    }

    /// O nome cuja dica está aparecendo agora.
    pub fn dica_visivel(&self) -> Option<&Ref> {
        self.dica_visivel.as_ref()
    }

    /// O que se sabe do conteúdo de um achado.
    pub fn resumo(&self, hash: &str) -> Option<&Resumo> {
        self.resumos.get(hash)
    }

    /// Há pedido na fila ou em andamento.
    pub fn ocupada(&self) -> bool {
        !self.em_voo.is_empty()
            || self
                .fila
                .estado
                .lock()
                .map(|f| f.ocupado || !f.trabalhos.is_empty())
                .unwrap_or(false)
    }

    /// Quantas texturas estão vivas (só as das prévias desenhadas).
    pub fn texturas_vivas(&self) -> usize {
        self.texturas.len()
    }

    /// Tem a página desenhada (em qualquer largura) deste achado.
    pub fn tem_imagem(&self, hash: &str, pagina: usize) -> bool {
        self.texturas
            .keys()
            .any(|(h, p, _)| h == hash && *p == pagina)
    }

    // ------------------------------------------------------------------ fila

    fn ligar_trabalhador(&mut self, ctx: &egui::Context) {
        if self.trabalhador {
            return;
        }
        self.trabalhador = true;
        let fila = Arc::clone(&self.fila);
        let tx = self.tx.clone();
        let cache = Arc::clone(&self.cache);
        let abridor = Arc::clone(&self.abridor);
        let candidatas = self.candidatas.clone();
        let ctx = ctx.clone();
        let r = std::thread::Builder::new()
            .name("sbf-previa".to_owned())
            .spawn(move || trabalhar(&fila, &tx, &cache, &abridor, candidatas.as_ref(), &ctx));
        if r.is_err() {
            self.trabalhador = false;
        }
    }

    fn pedir(
        &mut self,
        ctx: &egui::Context,
        posto: Posto,
        arquivo: &ArquivoDaPrevia,
        pagina: usize,
        largura: u32,
    ) {
        let chave = (arquivo.hash.clone(), pagina, largura);
        if self.em_voo.get(&posto) == Some(&chave) {
            return;
        }
        self.ligar_trabalhador(ctx);
        if let Ok(mut f) = self.fila.estado.lock() {
            // Só o último pedido de cada posto importa: o que não começou sai da fila.
            f.trabalhos.retain(|t| t.posto != posto);
            f.trabalhos.push_back(Trabalho {
                posto: posto.clone(),
                arquivo: arquivo.clone(),
                pagina,
                largura,
            });
        }
        self.fila.sinal.notify_one();
        self.em_voo.insert(posto, chave);
    }

    /// Lê o que o trabalhador mandou.
    fn receber(&mut self, ctx: &egui::Context) {
        while let Ok(r) = self.rx.try_recv() {
            match r {
                Resposta::Comecou(hash) => self.abertos.push(hash),
                Resposta::Conteudo { hash, resumo } => {
                    let sem_imagem = !matches!(resumo, Resumo::Paginas { .. } | Resumo::Imagem);
                    if sem_imagem {
                        self.em_voo.retain(|_, (h, ..)| *h != hash);
                    }
                    self.resumos.insert(hash, resumo);
                }
                Resposta::Imagem { chave, imagem } => {
                    self.em_voo.retain(|_, c| *c != chave);
                    match imagem {
                        Ok(img) => {
                            let cor = egui::ColorImage::from_rgba_unmultiplied(
                                [img.largura as usize, img.altura as usize],
                                &img.rgba,
                            );
                            let t = ctx.load_texture(
                                format!("previa-{}-{}-{}", chave.0, chave.1, chave.2),
                                cor,
                                TextureOptions::LINEAR,
                            );
                            self.texturas.insert(chave, (t, self.quadro));
                        }
                        Err(e) => {
                            self.falhas.insert(chave, e);
                        }
                    }
                }
            }
        }
        if self.ocupada() {
            ctx.request_repaint_after(Duration::from_millis(60));
        }
    }

    // ------------------------------------------------------------- resolução

    /// O arquivo de um item: o achado ou documento direto; a origem, pelo achado dela; o
    /// registro, pelo documento dele.
    pub fn resolver(
        &mut self,
        leitor: &mut Leitor<'_>,
        alvo: &Ref,
    ) -> Result<ArquivoDaPrevia, String> {
        if let Some(r) = self.resolvidos.get(alvo) {
            return r.clone();
        }
        let r = resolver(leitor, alvo, self.pasta.as_deref());
        self.resolvidos.insert(alvo.clone(), r.clone());
        r
    }

    // ---------------------------------------------------------------- vistas

    /// O painel Prévia do dock: a miniatura do item em foco, do tamanho que cabe em
    /// `altura` (a página inteira à vista). Devolve o achado quando você clica na miniatura
    /// (a janela abre a página dele na aba Arquivo).
    pub fn painel_ui(
        &mut self,
        ui: &mut Ui,
        leitor: &mut Leitor<'_>,
        foco: Option<&Ref>,
        altura: f32,
        pedidos: &mut Vec<Pedido>,
    ) -> Option<Ref> {
        let ctx = ui.ctx().clone();
        self.receber(&ctx);
        let p = tema::paleta(&ctx);
        let Some(alvo) = foco else {
            vazio(
                ui,
                "Selecione um achado ou documento para ver a prévia do arquivo.",
            );
            return None;
        };
        let arquivo = match self.resolver(leitor, alvo) {
            Ok(a) => a,
            Err(e) => {
                vazio(ui, &e);
                return None;
            }
        };
        let resumo = self.resumos.get(&arquivo.hash).cloned();
        // Cabeçalho: nome truncado, tipo, página e Destacar.
        let mut destacar = false;
        ui.horizontal(|ui| {
            ui.label(icones::icone_cor(
                icone_da_familia(arquivo.familia()),
                16.0,
                p.tinta_suave,
            ));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if botao_com_id(
                    ui,
                    Id::new(("previa-destacar", Posto::Painel)),
                    "picture_in_picture_alt",
                    "Destacar: abrir a prévia numa janela própria",
                )
                .clicked()
                {
                    destacar = true;
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&arquivo.nome).strong().color(p.tinta))
                            .truncate(),
                    )
                    .on_hover_text(&arquivo.nome);
                });
            });
        });
        ui.label(
            RichText::new(linha_do_tipo(&arquivo, resumo.as_ref(), 1))
                .size(11.5)
                .color(p.tinta_suave),
        );
        ui.add_space(4.0);
        if destacar {
            self.destacar(&arquivo.achado);
        }
        // A página inteira à vista: cabe na largura e na altura do painel.
        let pela_altura = (altura - 80.0) / PROPORCAO_A4;
        let largura = (ui.available_width() - 24.0)
            .min(pela_altura)
            .clamp(80.0, 320.0);
        let mut abrir = None;
        moldura(ui, |ui| {
            let r = self.conteudo(
                ui,
                leitor,
                Posto::Painel,
                &arquivo,
                1,
                largura,
                false,
                pedidos,
            );
            if let Some(r) = r
                && r.clicked()
            {
                abrir = Some(arquivo.achado.clone());
            }
        });
        abrir
    }

    /// A aba Arquivo da página do item: a prévia grande, com páginas, zoom e Destacar.
    /// `teclado`: PageUp/PageDown mudam de página (só no centro).
    pub fn aba_ui(
        &mut self,
        ui: &mut Ui,
        leitor: &mut Leitor<'_>,
        alvo: &Ref,
        pedidos: &mut Vec<Pedido>,
        teclado: bool,
    ) {
        let ctx = ui.ctx().clone();
        self.receber(&ctx);
        let arquivo = match self.resolver(leitor, alvo) {
            Ok(a) => a,
            Err(e) => {
                vazio(ui, &e);
                return;
            }
        };
        let mut estado = self.vistas.get(&arquivo.hash).copied().unwrap_or_default();
        let posto = Posto::Aba(arquivo.hash.clone());
        let destacar = self.vista_grande(
            ui,
            leitor,
            posto,
            &arquivo,
            &mut estado,
            pedidos,
            teclado,
            true,
        );
        self.vistas.insert(arquivo.hash.clone(), estado);
        if destacar {
            self.destacar(&arquivo.achado);
        }
    }

    /// As janelas destacadas e a dica. Chame uma vez por quadro, depois do centro.
    pub fn janelas_ui(
        &mut self,
        ctx: &egui::Context,
        leitor: &mut Leitor<'_>,
        pedidos: &mut Vec<Pedido>,
    ) {
        self.quadro += 1;
        self.receber(ctx);
        self.dica_ui(ctx, leitor, pedidos);
        let p = tema::paleta(ctx);
        let lista = self.destacadas.clone();
        for d in lista {
            let arquivo = self.resolver(leitor, &d.achado);
            let titulo = match &arquivo {
                Ok(a) => format!("Prévia · {}", a.nome),
                Err(_) => "Prévia".to_owned(),
            };
            let vid = egui::ViewportId::from_hash_of(("sbf-previa-destacada", d.id));
            let construtor = egui::ViewportBuilder::default()
                .with_title(titulo)
                .with_inner_size([720.0, 900.0]);
            let mut estado = d.estado;
            let fechar = ctx.show_viewport_immediate(vid, construtor, |ui, classe| {
                let mut corpo = |ui: &mut Ui| match &arquivo {
                    Ok(a) => {
                        egui::ScrollArea::vertical()
                            .id_salt(("previa-destacada", d.id))
                            .auto_shrink([false, false])
                            .show(ui, |ui| {
                                self.vista_grande(
                                    ui,
                                    leitor,
                                    Posto::Destacada(d.id),
                                    a,
                                    &mut estado,
                                    pedidos,
                                    true,
                                    false,
                                );
                            });
                    }
                    Err(e) => vazio(ui, e),
                };
                match classe {
                    egui::ViewportClass::EmbeddedWindow => corpo(ui),
                    _ => {
                        egui::CentralPanel::default()
                            .frame(
                                Frame::new()
                                    .fill(p.superficie_1)
                                    .inner_margin(Margin::same(10)),
                            )
                            .show(ui, |ui| corpo(ui));
                    }
                }
                ui.input(|i| i.viewport().close_requested())
            });
            if let Some(x) = self.destacadas.iter_mut().find(|x| x.id == d.id) {
                x.estado = estado;
            }
            if fechar {
                self.fechar_destacada(d.id);
            }
        }
        // As texturas só das prévias desenhadas: o resto volta ao cache quando precisar.
        let quadro = self.quadro;
        self.texturas
            .retain(|_, (_, usado)| quadro.saturating_sub(*usado) <= 2);
    }

    // ------------------------------------------------------------------ dica

    fn dica_ui(&mut self, ctx: &egui::Context, leitor: &mut Leitor<'_>, pedidos: &mut Vec<Pedido>) {
        self.dica_visivel = None;
        let agora = ctx.input(|i| i.time);
        let passo = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
        let marca = ctx
            .data_mut(|d| d.remove_temp::<Option<(Ref, Rect, u64)>>(id_da_marca()))
            .flatten();
        match marca {
            Some((alvo, ret, quando)) if quando + 1 >= passo => {
                let mesmo = self.dica.as_ref().is_some_and(|d| d.alvo == alvo);
                if mesmo {
                    if let Some(d) = self.dica.as_mut() {
                        d.retangulo = ret;
                    }
                } else {
                    self.dica = Some(Dica {
                        alvo,
                        retangulo: ret,
                        desde: agora,
                    });
                }
            }
            _ => self.dica = None,
        }
        let Some(d) = &self.dica else { return };
        let falta = d.desde + ESPERA_DA_DICA - agora;
        if falta > 0.0 {
            ctx.request_repaint_after(Duration::from_secs_f64(falta.max(0.01)));
            return;
        }
        let (alvo, ret) = (d.alvo.clone(), d.retangulo);
        let Ok(arquivo) = self.resolver(leitor, &alvo) else {
            return;
        };
        self.dica_visivel = Some(alvo.clone());
        let p = tema::paleta(ctx);
        let tela = ctx.content_rect();
        let altura = LARGURA_DA_DICA * PROPORCAO_A4 + 70.0;
        let mut pos = pos2(ret.left(), ret.bottom() + 6.0);
        if pos.y + altura > tela.bottom() {
            pos.y = (ret.top() - altura - 6.0).max(tela.top());
        }
        pos.x = pos
            .x
            .min(tela.right() - LARGURA_DA_DICA - 30.0)
            .max(tela.left());
        egui::Area::new(Id::new("sbf-previa-dica"))
            .order(Order::Tooltip)
            .fixed_pos(pos)
            .interactable(false)
            .show(ctx, |ui| {
                Frame::popup(&ctx.global_style())
                    .fill(p.superficie_1)
                    .show(ui, |ui| {
                        ui.set_max_width(LARGURA_DA_DICA + 8.0);
                        ui.label(RichText::new(&arquivo.nome).strong().color(p.tinta));
                        let resumo = self.resumos.get(&arquivo.hash).cloned();
                        ui.label(
                            RichText::new(linha_do_tipo(&arquivo, resumo.as_ref(), 1))
                                .size(11.5)
                                .color(p.tinta_suave),
                        );
                        ui.add_space(4.0);
                        self.conteudo(
                            ui,
                            leitor,
                            Posto::Dica,
                            &arquivo,
                            1,
                            LARGURA_DA_DICA,
                            false,
                            pedidos,
                        );
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new(format!(
                                "{} · clique abre, Ctrl+clique abre ao lado",
                                alvo.tipo.singular()
                            ))
                            .size(11.0)
                            .color(p.tinta_suave),
                        );
                    });
            });
    }

    // ---------------------------------------------------------- vista grande

    /// A barra (páginas, zoom, caber, abrir no sistema, destacar) e a prévia. Devolve se
    /// você pediu Destacar.
    #[allow(clippy::too_many_arguments)]
    fn vista_grande(
        &mut self,
        ui: &mut Ui,
        leitor: &mut Leitor<'_>,
        posto: Posto,
        arquivo: &ArquivoDaPrevia,
        estado: &mut EstadoDaVista,
        pedidos: &mut Vec<Pedido>,
        teclado: bool,
        com_destacar: bool,
    ) -> bool {
        let p = tema::paleta(ui.ctx());
        let resumo = self.resumos.get(&arquivo.hash).cloned();
        let total = match resumo {
            Some(Resumo::Paginas { total }) => total.max(1),
            _ => 1,
        };
        let paginado = matches!(resumo, Some(Resumo::Paginas { .. }));
        let com_imagem = matches!(resumo, Some(Resumo::Paginas { .. } | Resumo::Imagem));
        estado.pagina = estado.pagina.clamp(1, total);
        if teclado && paginado {
            let (descer, subir) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown),
                    i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp),
                )
            });
            if descer && estado.pagina < total {
                estado.pagina += 1;
            }
            if subir && estado.pagina > 1 {
                estado.pagina -= 1;
            }
        }
        let disponivel = (ui.available_width() - 32.0).max(120.0);
        let base = LARGURA_A4;
        let mut destacar = false;
        // Barra compacta.
        Frame::new()
            .fill(p.superficie_1)
            .stroke(Stroke::new(1.0, p.borda))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::symmetric(8, 4))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.label(icones::icone_cor(
                        icone_da_familia(arquivo.familia()),
                        16.0,
                        p.tinta_suave,
                    ));
                    ui.add(
                        egui::Label::new(RichText::new(&arquivo.nome).strong().color(p.tinta))
                            .truncate(),
                    )
                    .on_hover_text(&arquivo.nome);
                    ui.separator();
                    if componentes::botao_icone(
                        ui,
                        "chevron_left",
                        "Página anterior (PageUp)",
                        paginado && estado.pagina > 1,
                    )
                    .clicked()
                    {
                        estado.pagina -= 1;
                    }
                    ui.label(
                        RichText::new(format!("{} / {}", estado.pagina, total))
                            .monospace()
                            .color(if paginado {
                                p.tinta
                            } else {
                                p.tinta_desabilitada
                            }),
                    );
                    if componentes::botao_icone(
                        ui,
                        "chevron_right",
                        "Próxima página (PageDown)",
                        paginado && estado.pagina < total,
                    )
                    .clicked()
                    {
                        estado.pagina += 1;
                    }
                    ui.separator();
                    let zoom_atual = if estado.caber {
                        disponivel / base
                    } else {
                        estado.zoom
                    };
                    if componentes::botao_icone(ui, "remove", "Diminuir (Ctrl+roda)", com_imagem)
                        .clicked()
                    {
                        estado.zoom = passo_de_zoom(zoom_atual, false);
                        estado.caber = false;
                    }
                    ui.label(
                        RichText::new(format!("{:.0}%", zoom_atual * 100.0))
                            .monospace()
                            .color(if com_imagem {
                                p.tinta
                            } else {
                                p.tinta_desabilitada
                            }),
                    );
                    if componentes::botao_icone(ui, "add", "Aumentar (Ctrl+roda)", com_imagem)
                        .clicked()
                    {
                        estado.zoom = passo_de_zoom(zoom_atual, true);
                        estado.caber = false;
                    }
                    if ui
                        .add_enabled(
                            com_imagem,
                            egui::Button::selectable(estado.caber, "Caber na largura").small(),
                        )
                        .clicked()
                    {
                        estado.caber = true;
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if com_destacar
                            && botao_com_id(
                                ui,
                                Id::new(("previa-destacar", posto.clone())),
                                "picture_in_picture_alt",
                                "Destacar: abrir a prévia numa janela própria",
                            )
                            .clicked()
                        {
                            destacar = true;
                        }
                        let pode = arquivo.caminho.is_ok();
                        if componentes::botao_icone(
                            ui,
                            "open_in_new",
                            "Abrir no programa do sistema",
                            pode,
                        )
                        .clicked()
                            && let Ok(c) = &arquivo.caminho
                        {
                            self.aviso = abrir_no_sistema(c).err();
                        }
                    });
                });
            });
        if let Some(a) = &self.aviso {
            componentes::aviso(ui, "error", a, p.perigo_sutil, p.borda, p.perigo);
        }
        ui.add_space(6.0);
        let largura = if estado.caber {
            disponivel
        } else {
            (base * estado.zoom).clamp(80.0, 4000.0)
        };
        let pagina = estado.pagina;
        let resposta = moldura(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt(("previa-grande", &posto))
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        self.conteudo(
                            ui,
                            leitor,
                            posto.clone(),
                            arquivo,
                            pagina,
                            largura,
                            true,
                            pedidos,
                        );
                    });
                });
        });
        // Ctrl+roda sobre a prévia: zoom.
        if com_imagem && resposta.contains_pointer() {
            let z = ui.input(|i| i.zoom_delta());
            if (z - 1.0).abs() > f32::EPSILON {
                let atual = if estado.caber {
                    disponivel / base
                } else {
                    estado.zoom
                };
                estado.zoom = (atual * z).clamp(0.25, 4.0);
                estado.caber = false;
            }
        }
        destacar
    }

    // -------------------------------------------------------------- conteúdo

    /// O conteúdo da prévia na largura dada (em pontos). Devolve a resposta da página,
    /// quando há uma imagem (clicável).
    #[allow(clippy::too_many_arguments)]
    fn conteudo(
        &mut self,
        ui: &mut Ui,
        leitor: &mut Leitor<'_>,
        posto: Posto,
        arquivo: &ArquivoDaPrevia,
        pagina: usize,
        largura: f32,
        grande: bool,
        pedidos: &mut Vec<Pedido>,
    ) -> Option<egui::Response> {
        let ctx = ui.ctx().clone();
        if let Err(motivo) = &arquivo.caminho {
            metadados(
                ui,
                arquivo,
                "Sem prévia",
                motivo,
                false,
                grande,
                pedidos,
                &mut self.aviso,
            );
            return None;
        }
        let ppp = ctx.pixels_per_point();
        let px = largura_em_px(largura, ppp);
        let resumo = self.resumos.get(&arquivo.hash).cloned();
        match resumo {
            None => {
                self.pedir(&ctx, posto, arquivo, pagina, px);
                Some(folha_carregando(ui, largura, arquivo))
            }
            Some(Resumo::Paginas { .. } | Resumo::Imagem) => {
                let pagina = if matches!(resumo, Some(Resumo::Imagem)) {
                    1
                } else {
                    pagina
                };
                let chave = (arquivo.hash.clone(), pagina, px);
                if let Some(e) = self.falhas.get(&chave).cloned() {
                    metadados(
                        ui,
                        arquivo,
                        "Sem prévia",
                        &e,
                        false,
                        grande,
                        pedidos,
                        &mut self.aviso,
                    );
                    return None;
                }
                let quadro = self.quadro;
                let textura = match self.texturas.get_mut(&chave) {
                    Some((t, usado)) => {
                        *usado = quadro;
                        Some(t.clone())
                    }
                    None => {
                        self.pedir(&ctx, posto, arquivo, pagina, px);
                        // Enquanto a largura nova chega, a mesma página em outra largura.
                        self.texturas
                            .iter_mut()
                            .filter(|((h, p, _), _)| *h == arquivo.hash && *p == pagina)
                            .max_by_key(|((.., l), _)| *l)
                            .map(|(_, (t, usado))| {
                                *usado = quadro;
                                t.clone()
                            })
                    }
                };
                match textura {
                    Some(t) => Some(folha_com_imagem(ui, &t, largura)),
                    None => Some(folha_carregando(ui, largura, arquivo)),
                }
            }
            Some(Resumo::Email(e)) => {
                folha_de_texto(ui, largura, grande, |ui| {
                    email_ui(ui, leitor, &e, grande, pedidos)
                });
                None
            }
            Some(Resumo::Texto(t)) => {
                folha_de_texto(ui, largura, grande, |ui| texto_ui(ui, &t, grande));
                None
            }
            Some(Resumo::ListaDoZip(l)) => {
                folha_de_texto(ui, largura, grande, |ui| zip_ui(ui, &l, grande));
                None
            }
            Some(Resumo::Protegido { motivo }) => {
                metadados(
                    ui,
                    arquivo,
                    "Protegido por senha",
                    &motivo,
                    true,
                    grande,
                    pedidos,
                    &mut self.aviso,
                );
                None
            }
            Some(Resumo::SemPrevia { motivo } | Resumo::Erro { motivo }) => {
                metadados(
                    ui,
                    arquivo,
                    "Sem prévia",
                    &motivo,
                    false,
                    grande,
                    pedidos,
                    &mut self.aviso,
                );
                None
            }
        }
    }
}

// ---------------------------------------------------------------------- trabalhador

fn trabalhar(
    fila: &Fila,
    tx: &Sender<Resposta>,
    cache: &Mutex<sbf_previa::Cache>,
    abridor: &Abridor,
    candidatas: Option<&GeradorDeCandidatas>,
    ctx: &egui::Context,
) {
    loop {
        let trabalho = {
            let Ok(mut f) = fila.estado.lock() else {
                return;
            };
            f.ocupado = false;
            loop {
                if f.parar {
                    return;
                }
                if let Some(t) = f.trabalhos.pop_front() {
                    f.ocupado = true;
                    break t;
                }
                f = match fila.sinal.wait(f) {
                    Ok(f) => f,
                    Err(_) => return,
                };
            }
        };
        // Um pânico no desenho não derruba a janela: vira "sem prévia".
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            fazer(&trabalho, tx, cache, abridor, candidatas)
        }));
        if r.is_err() {
            let _ = tx.send(Resposta::Conteudo {
                hash: trabalho.arquivo.hash.clone(),
                resumo: Resumo::Erro {
                    motivo: "A prévia falhou ao abrir este arquivo.".to_owned(),
                },
            });
        }
        ctx.request_repaint();
    }
}

fn fazer(
    t: &Trabalho,
    tx: &Sender<Resposta>,
    cache: &Mutex<sbf_previa::Cache>,
    abridor: &Abridor,
    candidatas: Option<&GeradorDeCandidatas>,
) {
    let hash = t.arquivo.hash.clone();
    let enviar_resumo = |resumo: Resumo| {
        let _ = tx.send(Resposta::Conteudo {
            hash: hash.clone(),
            resumo,
        });
    };
    let guardado = cache.lock().ok().and_then(|mut c| c.obter(&hash));
    let conteudo = match guardado {
        Some(c) => c,
        None => {
            let _ = tx.send(Resposta::Comecou(hash.clone()));
            let Ok(caminho) = &t.arquivo.caminho else {
                return;
            };
            // PDF marcado como protegido: só com as candidatas das regras. Sem nenhuma, nem
            // tenta abrir (o motivo diz por quê).
            let mut lista = Vec::new();
            if t.arquivo.protegido && t.arquivo.familia() == Familia::Pdf {
                match candidatas.map(|g| g(&hash)) {
                    Some(Ok(v)) if !v.is_empty() => lista = v,
                    Some(Err(motivo)) => {
                        enviar_resumo(Resumo::Protegido { motivo });
                        return;
                    }
                    _ => {
                        enviar_resumo(Resumo::Protegido {
                            motivo: MOTIVO_SEM_REGRA.to_owned(),
                        });
                        return;
                    }
                }
            }
            let pedido = sbf_previa::Pedido {
                arquivo: caminho,
                extensao: &t.arquivo.extensao,
                candidatas: lista,
            };
            match abridor(pedido) {
                Ok(c) => match cache.lock() {
                    Ok(mut cache) => cache.guardar(hash.clone(), c),
                    Err(_) => Arc::new(c),
                },
                Err(e) => {
                    enviar_resumo(Resumo::Erro {
                        motivo: motivo_do_erro(&e),
                    });
                    return;
                }
            }
        }
    };
    enviar_resumo(resumo_de(&conteudo));
    let imagem = match conteudo.as_ref() {
        Conteudo::Paginas(paginas) => {
            let pagina = t.pagina.clamp(1, paginas.total().max(1));
            imagem_em_cache(cache, &hash, pagina, t.largura).map_or_else(
                || {
                    paginas
                        .desenhar(pagina, t.largura)
                        .map(|i| guardar_imagem(cache, &hash, pagina, t.largura, i))
                        .map_err(|e| motivo_do_erro(&e))
                },
                Ok,
            )
        }
        Conteudo::Imagem(original) => Ok(imagem_em_cache(cache, &hash, 1, t.largura)
            .unwrap_or_else(|| {
                let reduzida = match sbf_previa::miniatura(&conteudo, t.largura) {
                    Ok(Some(i)) => i,
                    _ => original.clone(),
                };
                guardar_imagem(cache, &hash, 1, t.largura, reduzida)
            })),
        _ => return,
    };
    let _ = tx.send(Resposta::Imagem {
        chave: (hash, t.pagina, t.largura),
        imagem,
    });
}

fn imagem_em_cache(
    cache: &Mutex<sbf_previa::Cache>,
    hash: &str,
    pagina: usize,
    largura: u32,
) -> Option<Arc<Imagem>> {
    cache
        .lock()
        .ok()
        .and_then(|mut c| c.imagem(hash, pagina, largura))
}

fn guardar_imagem(
    cache: &Mutex<sbf_previa::Cache>,
    hash: &str,
    pagina: usize,
    largura: u32,
    imagem: Imagem,
) -> Arc<Imagem> {
    match cache.lock() {
        Ok(mut c) => c.guardar_imagem(hash, pagina, largura, imagem),
        Err(_) => Arc::new(imagem),
    }
}

fn resumo_de(c: &Conteudo) -> Resumo {
    match c {
        Conteudo::Paginas(p) => Resumo::Paginas { total: p.total() },
        Conteudo::Imagem(_) => Resumo::Imagem,
        Conteudo::Email(e) => Resumo::Email(e.clone()),
        Conteudo::Texto(t) => Resumo::Texto(t.clone()),
        Conteudo::ListaDoZip(l) => Resumo::ListaDoZip(l.clone()),
        Conteudo::Protegido { motivo } => Resumo::Protegido {
            motivo: motivo.clone(),
        },
        Conteudo::SemPrevia { motivo } => Resumo::SemPrevia {
            motivo: motivo.clone(),
        },
    }
}

/// O erro da prévia em palavras, com o que fazer.
pub fn motivo_do_erro(e: &ErroDaPrevia) -> String {
    match e {
        ErroDaPrevia::NaoImplementado => {
            "A prévia deste tipo de arquivo ainda não está pronta nesta versão.".to_owned()
        }
        ErroDaPrevia::SemArquivo => "O arquivo não está no acervo.".to_owned(),
        ErroDaPrevia::GrandeDemais => {
            "Arquivo grande demais para a prévia (mais de 50 MiB ou 64 megapixels).".to_owned()
        }
        ErroDaPrevia::TempoEsgotado => "A conversão passou de 30 s e foi interrompida.".to_owned(),
        ErroDaPrevia::FaltaPrograma(p) => {
            format!("Falta o programa {p} para mostrar este tipo de arquivo.")
        }
        ErroDaPrevia::Ilegivel(m) => format!("Arquivo corrompido ou ilegível: {m}."),
    }
}

// ------------------------------------------------------------------------ resolução

fn primeiro_ligado(leitor: &mut Leitor<'_>, de: &Ref, tipo: Tipo) -> Option<Ref> {
    let pedido = PedidoDeTabela::Ligados {
        de: de.clone(),
        tipo,
    };
    leitor
        .tabela(&pedido)
        .ok()?
        .linhas
        .iter()
        .find_map(|l| l.alvo.clone())
}

/// O achado dono do arquivo de um item (achado, documento, origem ou registro).
pub fn achado_de(leitor: &mut Leitor<'_>, alvo: &Ref) -> Option<Ref> {
    match alvo.tipo {
        Tipo::Achado | Tipo::Documento => Some(alvo.clone()),
        Tipo::Origem => primeiro_ligado(leitor, alvo, Tipo::Achado),
        Tipo::Registro => primeiro_ligado(leitor, alvo, Tipo::Documento)
            .or_else(|| primeiro_ligado(leitor, alvo, Tipo::Achado)),
        _ => None,
    }
}

fn campo(item: &sbf_inventario::consulta::Item, rotulo: &str) -> Option<String> {
    item.campos
        .iter()
        .find(|c| c.rotulo == rotulo)
        .filter(|c| !matches!(c.valor, Celula::Vazia))
        .map(|c| formato::texto_da_celula(&c.valor))
        .filter(|t| !t.trim().is_empty())
}

/// Um caminho relativo só de nomes (sem `..`, sem raiz): o acervo não sai do cofre.
fn relativo_seguro(rel: &str) -> bool {
    let p = Path::new(rel);
    !rel.is_empty()
        && p.components()
            .all(|c| matches!(c, std::path::Component::Normal(_)))
}

fn resolver(
    leitor: &mut Leitor<'_>,
    alvo: &Ref,
    pasta: Option<&Path>,
) -> Result<ArquivoDaPrevia, String> {
    let achado = achado_de(leitor, alvo).ok_or_else(|| match alvo.tipo {
        Tipo::Origem | Tipo::Registro => format!(
            "{} sem documento ligado: nada para mostrar.",
            alvo.tipo.singular()
        ),
        _ => "Selecione um achado, documento, origem ou registro para ver a prévia do arquivo."
            .to_owned(),
    })?;
    let item = leitor.item(&achado)?;
    let item = item
        .as_ref()
        .as_ref()
        .ok_or_else(|| format!("{} não encontrado.", achado.tipo.singular()))?;
    let nome = item.rotulo.clone();
    let extensao = campo(item, "Extensão")
        .or_else(|| nome.rsplit_once('.').map(|(_, e)| e.to_owned()))
        .unwrap_or_default()
        .trim_start_matches('.')
        .to_ascii_lowercase();
    let caminho = match (pasta, campo(item, "No cofre")) {
        (None, _) => Err("Sem a pasta do cofre: abra um inventário para ver o arquivo.".to_owned()),
        (_, None) => Err("Este achado ainda não tem cópia no acervo.".to_owned()),
        (Some(_), Some(rel)) if !relativo_seguro(&rel) => {
            Err("O caminho no acervo não é válido.".to_owned())
        }
        (Some(pasta), Some(rel)) => {
            let c = pasta.join(&rel);
            if c.is_file() {
                Ok(c)
            } else {
                Err(format!("O arquivo não está no acervo ({rel})."))
            }
        }
    };
    let tamanho = caminho
        .as_ref()
        .ok()
        .and_then(|c| std::fs::metadata(c).ok())
        .map(|m| m.len());
    let protegido = campo(item, "Protegido por senha")
        .is_some_and(|t| t.trim().to_lowercase().starts_with("sim"));
    Ok(ArquivoDaPrevia {
        hash: achado.chave.clone(),
        achado,
        nome,
        extensao,
        tipo: campo(item, "Tipo"),
        fornecedor: campo(item, "Fornecedor"),
        data_chave: campo(item, "Data"),
        caminho,
        tamanho,
        protegido,
    })
}

// ------------------------------------------------------------------------- dica

fn id_da_marca() -> Id {
    Id::new("sbf-previa/sob-o-mouse")
}

/// O nome de um achado ou documento está sob o mouse neste quadro (o link chama). A janela
/// mostra a dica com a miniatura depois de [`ESPERA_DA_DICA`].
pub fn marcar_sob_o_mouse(ctx: &egui::Context, alvo: &Ref, retangulo: Rect) {
    let passo = ctx.cumulative_pass_nr_for(egui::ViewportId::ROOT);
    ctx.data_mut(|d| d.insert_temp(id_da_marca(), Some((alvo.clone(), retangulo, passo))));
}

// ----------------------------------------------------------------------- desenho

fn vazio(ui: &mut Ui, texto: &str) {
    let p = tema::paleta(ui.ctx());
    ui.add_space(6.0);
    ui.label(RichText::new(texto).color(p.tinta_suave));
}

/// Um botão de ícone com id fixo (os testes o acham pelo id).
fn botao_com_id(ui: &mut Ui, id: Id, icone: &str, dica: &str) -> egui::Response {
    let p = tema::paleta(ui.ctx());
    let (ret, _) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::hover());
    let r = ui.interact(ret, id, Sense::click());
    if r.hovered() {
        ui.painter()
            .rect_filled(ret, CornerRadius::same(4), p.superficie_3);
    }
    ui.painter().text(
        ret.center(),
        egui::Align2::CENTER_CENTER,
        icones::texto(icone),
        egui::FontId::new(18.0, tema::familia_icones()),
        p.tinta,
    );
    r.on_hover_text(dica)
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// O fundo neutro em volta da página (a moldura segue o tema).
fn moldura(ui: &mut Ui, conteudo: impl FnOnce(&mut Ui)) -> egui::Response {
    let p = tema::paleta(ui.ctx());
    Frame::new()
        .fill(p.superficie_2)
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.vertical_centered(conteudo);
        })
        .response
}

fn frame_da_folha(ui: &Ui) -> Frame {
    let p = tema::paleta(ui.ctx());
    Frame::new()
        .fill(PAPEL)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(2))
        .shadow(Shadow {
            offset: [0, 2],
            blur: 6,
            spread: 0,
            color: Color32::from_black_alpha(if p.escuro { 70 } else { 22 }),
        })
}

fn folha_com_imagem(ui: &mut Ui, t: &TextureHandle, largura: f32) -> egui::Response {
    let [w, h] = t.size();
    let altura = largura * h as f32 / (w.max(1) as f32);
    frame_da_folha(ui)
        .show(ui, |ui| {
            let (ret, r) = ui.allocate_exact_size(vec2(largura, altura), Sense::click());
            ui.painter().image(
                t.id(),
                ret,
                Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                Color32::WHITE,
            );
            r
        })
        .inner
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

/// A folha enquanto a página carrega: linhas fantasma e "carregando…".
fn folha_carregando(ui: &mut Ui, largura: f32, arquivo: &ArquivoDaPrevia) -> egui::Response {
    let largura = largura.min(900.0);
    let altura = largura * PROPORCAO_A4;
    frame_da_folha(ui)
        .show(ui, |ui| {
            let (ret, r) = ui.allocate_exact_size(vec2(largura, altura), Sense::click());
            let pintor = ui.painter_at(ret);
            let m = (largura * 0.07).max(8.0);
            let x0 = ret.left() + m;
            let mut y = ret.top() + m;
            let titulo = arquivo
                .fornecedor
                .clone()
                .unwrap_or_else(|| arquivo.nome.clone())
                .to_uppercase();
            pintor.text(
                pos2(x0, y),
                egui::Align2::LEFT_TOP,
                titulo,
                egui::FontId::new((largura * 0.045).clamp(8.0, 16.0), tema::familia_forte()),
                TINTA_DO_PAPEL,
            );
            y += (largura * 0.09).max(14.0);
            let util = largura - 2.0 * m;
            for (i, f) in [0.7, 1.0, 0.85, 0.6, 1.0, 0.4, 0.9, 0.75]
                .iter()
                .enumerate()
            {
                let alt = (largura * 0.018).max(3.0);
                pintor.rect_filled(
                    Rect::from_min_size(pos2(x0, y), vec2(util * f, alt)),
                    CornerRadius::same(2),
                    LINHA_DO_PAPEL,
                );
                y += alt * if i == 2 { 4.0 } else { 2.6 };
            }
            let centro = pos2(ret.center().x, ret.top() + altura * 0.62);
            let raio = (largura * 0.04).clamp(6.0, 14.0);
            let t = ui.input(|i| i.time) as f32;
            pintor.add(egui::Shape::circle_stroke(
                centro,
                raio,
                Stroke::new(2.0, LINHA_DO_PAPEL),
            ));
            let a0 = t * 4.0;
            let pontos: Vec<egui::Pos2> = (0..=12)
                .map(|k| {
                    let a = a0 + k as f32 * 0.13;
                    centro + vec2(a.cos(), a.sin()) * raio
                })
                .collect();
            pintor.add(egui::Shape::line(
                pontos,
                Stroke::new(2.0, Color32::from_rgb(0x5c, 0x68, 0x63)),
            ));
            pintor.text(
                centro + vec2(0.0, raio + 10.0),
                egui::Align2::CENTER_TOP,
                "carregando…",
                egui::FontId::proportional((largura * 0.04).clamp(10.0, 13.0)),
                Color32::from_rgb(0x5c, 0x68, 0x63),
            );
            ui.ctx().request_repaint_after(Duration::from_millis(50));
            r
        })
        .inner
}

/// Uma folha com texto (e-mail, texto, zip): papel claro, tinta escura.
fn folha_de_texto(ui: &mut Ui, largura: f32, grande: bool, conteudo: impl FnOnce(&mut Ui)) {
    let largura = if grande { largura.min(980.0) } else { largura };
    frame_da_folha(ui)
        .inner_margin(Margin::same(if grande { 18 } else { 8 }))
        .show(ui, |ui| {
            ui.set_width(largura - if grande { 36.0 } else { 16.0 });
            ui.set_max_width(largura - if grande { 36.0 } else { 16.0 });
            if !grande {
                ui.set_max_height(largura * PROPORCAO_A4);
            }
            let v = ui.visuals_mut();
            v.override_text_color = Some(TINTA_DO_PAPEL);
            v.hyperlink_color = Color32::from_rgb(0x2e, 0x6b, 0x52);
            ui.with_layout(Layout::top_down(Align::Min), conteudo);
        });
}

fn texto_ui(ui: &mut Ui, texto: &str, grande: bool) {
    let (mostra, cortado) = cortar(texto, if grande { TETO_DO_TEXTO } else { 1_500 });
    let tamanho = if grande { 12.5 } else { 9.5 };
    ui.add(egui::Label::new(RichText::new(mostra).monospace().size(tamanho)).wrap());
    if cortado {
        ui.label(
            RichText::new("… (cortado na prévia; o original está inteiro)")
                .italics()
                .size(tamanho),
        );
    }
}

fn cortar(texto: &str, teto: usize) -> (&str, bool) {
    if texto.len() <= teto {
        return (texto, false);
    }
    let mut fim = teto;
    while !texto.is_char_boundary(fim) {
        fim -= 1;
    }
    (&texto[..fim], true)
}

fn email_ui(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    e: &Email,
    grande: bool,
    pedidos: &mut Vec<Pedido>,
) {
    let tamanho = if grande { 13.0 } else { 10.0 };
    let suave = Color32::from_rgb(0x5c, 0x68, 0x63);
    egui::Grid::new(("previa-email", grande))
        .num_columns(2)
        .spacing(vec2(8.0, 3.0))
        .show(ui, |ui| {
            for (rotulo, valor) in [
                ("De", &e.de),
                ("Para", &e.para),
                ("Assunto", &e.assunto),
                ("Data", &e.data),
            ] {
                if !grande && rotulo == "Para" {
                    continue;
                }
                ui.label(RichText::new(rotulo).size(tamanho).color(suave));
                let mut t = RichText::new(valor.as_str()).size(tamanho);
                if rotulo == "Assunto" {
                    t = t.strong();
                }
                ui.add(egui::Label::new(t).truncate());
                ui.end_row();
            }
        });
    ui.add_space(4.0);
    ui.separator();
    let (corpo, cortado) = cortar(&e.corpo, if grande { TETO_DO_TEXTO } else { 900 });
    ui.add(egui::Label::new(RichText::new(corpo).size(tamanho)).wrap());
    if cortado {
        ui.label(RichText::new("…").size(tamanho));
    }
    if e.anexos.is_empty() {
        return;
    }
    ui.add_space(6.0);
    ui.separator();
    ui.label(
        RichText::new(format!("Anexos ({})", e.anexos.len()))
            .size(tamanho)
            .strong(),
    );
    for a in &e.anexos {
        ui.horizontal(|ui| {
            ui.label(icones::icone_cor("attach_file", tamanho + 2.0, suave));
            // O anexo que também é um achado (mesmo content_hash) vira link.
            let achado = a.sha256.as_ref().and_then(|h| {
                let r = Ref::nova(Tipo::Achado, h.clone());
                leitor.item(&r).ok().filter(|i| i.is_some()).map(|_| r)
            });
            match achado {
                Some(r) => {
                    componentes::link_item(ui, &r, &a.nome, pedidos);
                }
                None => {
                    ui.label(RichText::new(&a.nome).size(tamanho));
                }
            }
            ui.label(
                RichText::new(tamanho_legivel(a.bytes))
                    .size(tamanho - 1.0)
                    .color(suave),
            );
        });
    }
}

fn zip_ui(ui: &mut Ui, l: &[EntradaDoZip], grande: bool) {
    let tamanho = if grande { 12.5 } else { 9.5 };
    let suave = Color32::from_rgb(0x5c, 0x68, 0x63);
    ui.label(
        RichText::new(format!("{} arquivos no zip (nada é extraído)", l.len()))
            .size(tamanho)
            .strong(),
    );
    ui.add_space(4.0);
    let teto = if grande { l.len() } else { 18 };
    for e in l.iter().take(teto) {
        ui.horizontal(|ui| {
            ui.add(egui::Label::new(RichText::new(&e.nome).size(tamanho).monospace()).truncate());
            ui.label(
                RichText::new(tamanho_legivel(e.bytes))
                    .size(tamanho)
                    .color(suave),
            );
        });
    }
    if l.len() > teto {
        ui.label(
            RichText::new(format!("… e mais {}", l.len() - teto))
                .size(tamanho)
                .color(suave),
        );
    }
}

/// Sem prévia (ou protegido): os metadados, o motivo e o que dá para fazer.
#[allow(clippy::too_many_arguments)]
fn metadados(
    ui: &mut Ui,
    arquivo: &ArquivoDaPrevia,
    titulo: &str,
    motivo: &str,
    protegido: bool,
    grande: bool,
    pedidos: &mut Vec<Pedido>,
    aviso: &mut Option<String>,
) {
    let p = tema::paleta(ui.ctx());
    let (icone, cor) = if protegido {
        ("lock", p.aviso)
    } else {
        ("visibility", p.tinta_suave)
    };
    Frame::new()
        .fill(p.superficie_1)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(if grande { 16 } else { 10 }))
        .show(ui, |ui| {
            ui.set_max_width(if grande { 560.0 } else { ui.available_width() });
            ui.with_layout(Layout::top_down(Align::Min), |ui| {
                ui.horizontal(|ui| {
                    ui.label(icones::icone_cor(icone, if grande { 26.0 } else { 20.0 }, cor));
                    ui.label(RichText::new(titulo).strong().size(if grande { 16.0 } else { 13.5 }).color(p.tinta));
                });
                ui.add(egui::Label::new(RichText::new(motivo).color(p.tinta_suave)).wrap());
                ui.add_space(6.0);
                egui::Grid::new(("previa-metadados", &arquivo.hash, grande))
                    .num_columns(2)
                    .spacing(vec2(10.0, 4.0))
                    .show(ui, |ui| {
                        let linhas = [
                            ("Nome", Some(arquivo.nome.clone())),
                            (
                                "Tipo",
                                Some(match &arquivo.tipo {
                                    Some(t) => format!("{t} · .{}", arquivo.extensao),
                                    None => format!(".{}", arquivo.extensao),
                                }),
                            ),
                            ("Tamanho", arquivo.tamanho.map(tamanho_legivel)),
                            ("Data-chave", arquivo.data_chave.clone()),
                        ];
                        for (rotulo, valor) in linhas {
                            ui.label(RichText::new(rotulo).size(12.0).color(p.tinta_suave));
                            ui.add(egui::Label::new(valor.unwrap_or_else(|| "—".to_owned())).truncate());
                            ui.end_row();
                        }
                    });
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if protegido
                        && ui
                            .button((icones::icone_cor("key", 15.0, p.tinta), "Nova regra"))
                            .on_hover_text("Cria uma regra de senha para o fornecedor e o tipo deste documento")
                            .clicked()
                    {
                        pedidos.push(Pedido::NovaRegra {
                            fornecedor: arquivo.fornecedor.clone().unwrap_or_default(),
                            tipo_de_documento: tipo_cru(arquivo.tipo.as_deref()),
                        });
                    }
                    let pode = arquivo.caminho.is_ok();
                    if ui
                        .add_enabled(
                            pode,
                            egui::Button::new((icones::icone_cor("open_in_new", 15.0, p.tinta), "Abrir no programa do sistema")),
                        )
                        .clicked()
                        && let Ok(c) = &arquivo.caminho
                    {
                        *aviso = abrir_no_sistema(c).err();
                    }
                });
            });
        });
}

/// O tipo de documento como a regra de senha guarda (`fatura_cartao`); o rótulo da
/// demonstração ("Fatura de cartão") cai no padrão.
fn tipo_cru(tipo: Option<&str>) -> String {
    match tipo {
        Some(t) if !t.is_empty() && t.chars().all(|c| c.is_ascii_lowercase() || c == '_') => {
            t.to_owned()
        }
        _ => "fatura".to_owned(),
    }
}

fn linha_do_tipo(arquivo: &ArquivoDaPrevia, resumo: Option<&Resumo>, pagina: usize) -> String {
    let mut partes = vec![arquivo.extensao.to_uppercase()];
    if let Some(t) = &arquivo.tipo {
        partes.push(t.clone());
    }
    if let Some(Resumo::Paginas { total }) = resumo {
        partes.push(format!("pág. {pagina}/{total}"));
    }
    if let Some(b) = arquivo.tamanho {
        partes.push(tamanho_legivel(b));
    }
    partes.join(" · ")
}

fn icone_da_familia(f: Familia) -> &'static str {
    match f {
        Familia::Pdf | Familia::Office => "description",
        Familia::Imagem | Familia::Heic => "image",
        Familia::Email => "mail",
        Familia::Texto => "article",
        Familia::Zip => "folder",
        Familia::Outro => "find_in_page",
    }
}

/// Bytes em palavras: "812 B", "34 KB", "1,2 MB".
pub fn tamanho_legivel(bytes: u64) -> String {
    const KB: f64 = 1024.0;
    let b = bytes as f64;
    if b < KB {
        format!("{bytes} B")
    } else if b < KB * KB {
        format!("{:.0} KB", b / KB)
    } else if b < KB * KB * KB {
        format!("{:.1} MB", b / KB / KB).replace('.', ",")
    } else {
        format!("{:.1} GB", b / KB / KB / KB).replace('.', ",")
    }
}

/// A largura pedida ao desenho, em pixels, em degraus de 64 (redimensionar a janela não
/// redesenha a cada pixel).
pub fn largura_em_px(pontos: f32, pixels_por_ponto: f32) -> u32 {
    let px = (pontos * pixels_por_ponto).clamp(64.0, 4096.0);
    ((px / 64.0).ceil() * 64.0) as u32
}

const DEGRAUS_DE_ZOOM: [f32; 11] = [0.25, 0.33, 0.5, 0.67, 0.75, 1.0, 1.25, 1.5, 2.0, 3.0, 4.0];

fn passo_de_zoom(atual: f32, maior: bool) -> f32 {
    if maior {
        DEGRAUS_DE_ZOOM
            .iter()
            .copied()
            .find(|z| *z > atual + 0.01)
            .unwrap_or(4.0)
    } else {
        DEGRAUS_DE_ZOOM
            .iter()
            .rev()
            .copied()
            .find(|z| *z < atual - 0.01)
            .unwrap_or(0.25)
    }
}

/// Abre o arquivo com o programa do sistema, pelo `sbf-processo` (caminho absoluto).
pub fn abrir_no_sistema(caminho: &Path) -> Result<(), String> {
    let absoluto =
        std::fs::canonicalize(caminho).map_err(|e| format!("Não achei o arquivo: {e}"))?;
    sbf_processo::abrir_no_sistema(&absoluto, false).map_err(|e| format!("Não deu para abrir: {e}"))
}

/// As candidatas do inventário real: as regras de senha do fornecedor e tipo do achado,
/// montadas em memória a partir do chaveiro, **só com ele aberto**.
pub fn gerador_do_inventario(
    banco: PathBuf,
    chaveiro: sbf_chaveiro::Chaveiro,
) -> GeradorDeCandidatas {
    let chaveiro = Mutex::new(chaveiro);
    Arc::new(move |hash: &str| {
        let chaveiro = chaveiro
            .lock()
            .map_err(|_| "O chaveiro não respondeu.".to_owned())?;
        match chaveiro.situacao() {
            sbf_chaveiro::Situacao::ArquivoTrancado => {
                return Err(
                    "O chaveiro está trancado: destranque-o (Arquivo › Chaveiro) para ver a prévia deste PDF protegido."
                        .to_owned(),
                );
            }
            sbf_chaveiro::Situacao::Indisponivel => {
                return Err(
                    "O chaveiro não está disponível: a prévia deste PDF protegido precisa dele."
                        .to_owned(),
                );
            }
            _ => {}
        }
        let conn = sbf_inventario::banco::abrir_leitura(&banco).map_err(|e| e.to_string())?;
        let (fornecedor, tipo): (Option<String>, Option<String>) = conn
            .query_row(
                "SELECT supplier_name_raw, document_type FROM achado WHERE content_hash = ?1",
                [hash],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .map_err(|e| e.to_string())?;
        let regras =
            sbf_inventario::protegidos::regras_para(&conn, fornecedor.as_deref(), tipo.as_deref())
                .map_err(|e| e.to_string())?;
        if regras.is_empty() {
            return Err(MOTIVO_SEM_REGRA.to_owned());
        }
        let v: Vec<Candidata> = regras
            .iter()
            .filter_map(|r| r.estrategia.parse::<sbf_pdf::Estrategia>().ok())
            .filter_map(|e| e.candidata(&*chaveiro).ok())
            .collect();
        if v.is_empty() {
            return Err(
                "As regras deste fornecedor não montaram nenhuma senha: falta o CPF ou a senha própria no chaveiro."
                    .to_owned(),
            );
        }
        Ok(v)
    })
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn largura_em_degraus_de_64() {
        assert_eq!(largura_em_px(100.0, 1.0), 128);
        assert_eq!(largura_em_px(128.0, 1.0), 128);
        assert_eq!(largura_em_px(220.0, 2.0), 448);
        assert_eq!(largura_em_px(1.0, 1.0), 64);
        assert_eq!(largura_em_px(1e6, 1.0), 4096);
    }

    #[test]
    fn zoom_anda_em_degraus() {
        assert_eq!(passo_de_zoom(1.0, true), 1.25);
        assert_eq!(passo_de_zoom(1.0, false), 0.75);
        assert_eq!(passo_de_zoom(0.9, true), 1.0);
        assert_eq!(passo_de_zoom(4.0, true), 4.0);
        assert_eq!(passo_de_zoom(0.25, false), 0.25);
    }

    #[test]
    fn tamanhos_em_palavras() {
        assert_eq!(tamanho_legivel(812), "812 B");
        assert_eq!(tamanho_legivel(34 * 1024), "34 KB");
        assert_eq!(tamanho_legivel(1_258_291), "1,2 MB");
    }

    #[test]
    fn caminho_do_acervo_nao_sai_do_cofre() {
        assert!(relativo_seguro("acervo/ab/abcdef.pdf"));
        assert!(!relativo_seguro("../fora.pdf"));
        assert!(!relativo_seguro("/etc/passwd"));
        assert!(!relativo_seguro(""));
    }

    #[test]
    fn todo_erro_tem_motivo() {
        for e in [
            ErroDaPrevia::NaoImplementado,
            ErroDaPrevia::SemArquivo,
            ErroDaPrevia::GrandeDemais,
            ErroDaPrevia::TempoEsgotado,
            ErroDaPrevia::FaltaPrograma("pdftoppm".into()),
            ErroDaPrevia::Ilegivel("xref".into()),
        ] {
            assert!(motivo_do_erro(&e).ends_with('.'));
        }
    }

    #[test]
    fn tipo_da_regra() {
        assert_eq!(tipo_cru(Some("fatura_cartao")), "fatura_cartao");
        assert_eq!(tipo_cru(Some("Fatura de cartão")), "fatura");
        assert_eq!(tipo_cru(None), "fatura");
    }
}
