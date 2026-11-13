//! A janela do SBF: o estado da interface (navegação, seleção, filtros, lote atual,
//! diálogos, mensagens) e as vistas que a casca põe nos seus lugares.
//!
//! Regra do quadro: as vistas só **leem** (`&dyn Consulta`, com cache) e emitem
//! [`Pedido`]s. As escritas acontecem em [`Janela::processar`], depois do desenho, com
//! `&mut dyn Comandos`; nenhuma transação atravessa um quadro (spec 06, LICOES.md).

use std::collections::HashMap;

use egui::{Align, Context, Id, Layout, PointerButton, RichText, Ui};
use sbf_inventario::comando::{Acao, IdDecisao, PedidoDeDecisao};
use sbf_inventario::consulta::{Area, Contagens, Filtro, PedidoDeTabela};
use sbf_inventario::{Comandos, Consulta, Ref, Tipo};

use crate::acoes::{self, ContextoDasAcoes, IdAcao};
use crate::areas;
use crate::componentes;
use crate::dock::{self, PainelDoDock};
use crate::formato;
use crate::icones;
use crate::leitura::{Cache, Leitor};
use crate::navegacao::{Destino, Navegacao, Pedido};
use crate::pagina::{self, Vista};
use crate::painel::{self, EstadoDosRelatorios};
use crate::tabela;
use crate::tema;

/// O período do filtro (aba Início).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Periodo {
    CicloAtual,
    CicloAnterior,
    #[default]
    TodosOsCiclos,
    /// Do ciclo ao ciclo de [`Janela::intervalo`].
    Intervalo,
}

/// Uma mensagem curta no topo do centro (resultado da última Ação).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mensagem {
    pub texto: String,
    pub erro: bool,
}

/// O que outros crates ligam à janela (o integrador marca o que existe).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Ligacoes {
    pub pdf: bool,
    pub estante: bool,
    pub cofre: bool,
}

/// Um diálogo de Ação que pede dados.
#[derive(Debug, Clone, PartialEq)]
pub enum Dialogo {
    Texto {
        acao: IdAcao,
        alvos: Vec<Ref>,
        rotulo: String,
        valor: String,
    },
    DataEValor {
        alvos: Vec<Ref>,
        data: String,
        valor: String,
        erro: Option<String>,
    },
    NovoLote {
        nome: String,
    },
    /// Período › Intervalo…
    Intervalo {
        de: String,
        ate: String,
        erro: Option<String>,
    },
    /// Livro-caixa › Mudar categoria (uma movimentação).
    MudarCategoria {
        movimentacao: Ref,
        rotulo: String,
        atual: Option<String>,
        opcoes: Vec<sbf_inventario::alteracao::OpcaoDeCategoria>,
        /// O índice escolhido em `opcoes`; `None` = sem categoria.
        escolhida: Option<usize>,
    },
    /// Livro-caixa › Revogar estação (confirmação).
    RevogarEstacao {
        estacao: Ref,
        nome: String,
    },
    /// Livro-caixa › Renomear estação.
    RenomearEstacao {
        estacao: Ref,
        nome: String,
    },
    /// Ferramentas › Regras de senha › Nova regra. Nunca a senha: só como montá-la.
    NovaRegra {
        fornecedor: String,
        tipo_de_documento: String,
        estrategia: String,
        /// Menor é tentada primeiro (padrão 100).
        prioridade: String,
        erro: Option<String>,
    },
}

/// A janela do SBF.
#[derive(Debug)]
pub struct Janela {
    pub navegacao: Navegacao,
    pub centro: Vista,
    flutuantes: HashMap<u64, Vista>,
    cache: Cache,
    pesquisa: String,
    pub periodo: Periodo,
    /// Filtro "Quem decidiu" (aba Início): `None` = todos.
    pub quem_decidiu: Option<sbf_inventario::consulta::Quem>,
    /// Filtros "Estado do achado" e "Tipo" (aba Início): `None` = todos.
    pub estado_do_achado: Option<String>,
    /// Filtro "Conexão": o id da fonte.
    pub fonte: Option<String>,
    /// Período › Intervalo…: do ciclo ao ciclo.
    pub intervalo: Option<(String, String)>,
    pub tipo_do_documento: Option<String>,
    focar_pesquisa: bool,
    relatorios: EstadoDosRelatorios,
    pub lote_atual: Option<Ref>,
    pub paineis_visiveis: Vec<PainelDoDock>,
    pedidos: Vec<Pedido>,
    pub dialogo: Option<Dialogo>,
    pub mensagem: Option<Mensagem>,
    decisoes_da_sessao: Vec<IdDecisao>,
    pub tema: String,
    pub ligacoes: Ligacoes,
    /// "Hoje" em `AAAA-MM-DD` (o ciclo atual sai daqui). Fixável para teste.
    pub hoje: String,
    /// Dentro da `bancada-casca`: a navegação é da casca, e os pedidos de navegação vão
    /// para a fila dela.
    pub na_casca: bool,
    /// As Ferramentas do inventário real (PDF protegido, estante, cópia, chaveiro).
    /// `None` no modo demonstração.
    pub ferramentas: Option<crate::ferramentas::Ferramentas>,
    /// A prévia do arquivo (painel, aba Arquivo, destacadas e a dica).
    pub previas: crate::previa::Previas,
    chaveiro_trancado: bool,
    visao_geral_pendente:
        Option<std::sync::mpsc::Receiver<Result<sbf_inventario::consulta::Painel, String>>>,
    /// Primeiros pedaços de áreas pedidos numa thread (no inventário real), por pedido.
    pedacos_pendentes: HashMap<String, std::sync::mpsc::Receiver<PaginaPendente>>,
    /// A estação (alterações e sincronização do livro-caixa). `None`: esta máquina ainda
    /// não é uma estação; as áreas de Dinheiro mostram o motivo.
    pub estacao: Option<crate::dinheiro::EstacaoLigada>,
    /// O pareamento da estação (login Google). `None`: a janela não sabe parear.
    pub pareamento: Option<Box<dyn crate::dinheiro::Pareamento>>,
}

type PaginaPendente = Result<sbf_inventario::consulta::PaginaDeTabela, String>;

/// No inventário real, o primeiro pedaço de uma área é lido numa thread com a própria
/// `LeitorSqlite` (algumas áreas levam segundos na primeira vez). Devolve `true` quando o
/// pedaço já está no cache e a tabela pode ser desenhada; `false` enquanto carrega.
fn pedaco_pronto(
    pendentes: &mut HashMap<String, std::sync::mpsc::Receiver<PaginaPendente>>,
    banco: Option<&std::path::Path>,
    leitor: &mut Leitor<'_>,
    pedido: &PedidoDeTabela,
) -> bool {
    use std::sync::mpsc::TryRecvError;
    if leitor.tem_pedaco(pedido) {
        return true;
    }
    let Some(banco) = banco else { return true };
    let k = crate::leitura::chave(pedido);
    if let Some(rx) = pendentes.get(&k) {
        return match rx.try_recv() {
            Ok(r) => {
                leitor.guardar_pedaco(pedido, r);
                pendentes.remove(&k);
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                pendentes.remove(&k);
                true
            }
        };
    }
    let (tx, rx) = std::sync::mpsc::channel();
    let banco = banco.to_path_buf();
    let p = pedido.clone();
    std::thread::spawn(move || {
        let r = sbf_inventario::leitor::LeitorSqlite::abrir(&banco)
            .and_then(|l| l.tabela_paginada(&p, 0, crate::leitura::PEDACO))
            .map_err(|e| e.to_string());
        let _ = tx.send(r);
    });
    pendentes.insert(k, rx);
    false
}

fn carregando(ui: &mut Ui, texto: &str) {
    ui.horizontal(|ui| {
        ui.add(egui::Spinner::new());
        ui.label(texto);
    });
    ui.ctx()
        .request_repaint_after(std::time::Duration::from_millis(100));
}

impl Default for Janela {
    fn default() -> Self {
        Self::nova()
    }
}

impl Janela {
    pub fn nova() -> Self {
        Self {
            navegacao: Navegacao::nova(Destino::Area(Area::VisaoGeral)),
            centro: Vista::default(),
            flutuantes: HashMap::new(),
            cache: Cache::default(),
            pesquisa: String::new(),
            periodo: Periodo::default(),
            quem_decidiu: None,
            estado_do_achado: None,
            fonte: None,
            intervalo: None,
            tipo_do_documento: None,
            focar_pesquisa: false,
            relatorios: EstadoDosRelatorios::default(),
            lote_atual: None,
            paineis_visiveis: vec![
                PainelDoDock::Detalhe,
                PainelDoDock::Previa,
                PainelDoDock::Historico,
            ],
            pedidos: Vec::new(),
            dialogo: None,
            mensagem: None,
            decisoes_da_sessao: Vec::new(),
            tema: tema::TEMA_PADRAO.to_owned(),
            ligacoes: Ligacoes::default(),
            hoje: hoje_em_sao_paulo(),
            na_casca: false,
            ferramentas: None,
            previas: crate::previa::Previas::nova(),
            chaveiro_trancado: false,
            visao_geral_pendente: None,
            pedacos_pendentes: HashMap::new(),
            estacao: None,
            pareamento: None,
        }
    }

    /// Liga a estação (a real, ou o livro fictício da demonstração).
    pub fn ligar_estacao(&mut self, estacao: Box<dyn crate::dinheiro::EstacaoDoLivro>) {
        self.estacao = Some(crate::dinheiro::EstacaoLigada(estacao));
    }

    /// Esquece o que a janela guarda dos números e das tabelas: a próxima leitura vai à
    /// `Consulta` (o livro-caixa mudou depois de uma troca da estação).
    pub fn reler(&mut self) {
        self.cache.limpar();
    }

    /// Liga o pareamento (o botão "Parear esta estação" da área Sincronização).
    pub fn ligar_pareamento(&mut self, pareamento: Box<dyn crate::dinheiro::Pareamento>) {
        self.pareamento = Some(pareamento);
    }

    /// O estado da sincronização, se há estação.
    pub fn estado_da_sincronizacao(
        &self,
    ) -> Option<sbf_inventario::alteracao::EstadoDaSincronizacao> {
        self.estacao.as_ref().map(|e| e.0.estado())
    }

    /// Enfileira um pedido (a casca chama isto para os botões do Ribbon e da Paleta).
    pub fn pedir(&mut self, pedido: Pedido) {
        self.pedidos.push(pedido);
    }

    /// Enfileira uma Ação da lista única.
    pub fn executar(&mut self, id: IdAcao) {
        self.pedidos.push(Pedido::Executar(id));
    }

    /// As decisões que você tomou nesta sessão.
    pub fn decisoes_da_sessao(&self) -> &[IdDecisao] {
        &self.decisoes_da_sessao
    }

    /// A janela das Ferramentas (andamento e resultado). Quando uma tarefa termina, relê o
    /// inventário: a rodada de PDF e a aceitação de uma regra mudam o banco.
    pub fn ferramentas_ui(&mut self, ctx: &Context) {
        let Some(f) = self.ferramentas.as_mut() else {
            return;
        };
        let terminou = f.sondar(ctx);
        // Chaveiro trancado: a prévia esquece o PDF decifrado (memória e texturas).
        let trancado = f
            .chaveiro
            .as_ref()
            .is_some_and(|c| c.situacao() == sbf_chaveiro::Situacao::ArquivoTrancado);
        if trancado && !self.chaveiro_trancado {
            self.previas.esquecer_tudo();
        }
        self.chaveiro_trancado = trancado;
        f.janela_ui(ctx, &mut self.pedidos);
        f.primeira_abertura_ui(ctx);
        if terminou {
            self.atualizar();
        }
    }

    /// As conexões do inventário (para o filtro Conexão), lidas da área Coletas: `(id, nome)`.
    pub fn conexoes(&mut self, consulta: &dyn Consulta) -> Vec<(String, String)> {
        let pedido = PedidoDeTabela::Area {
            area: Area::Coletas,
            filtro: Filtro::default(),
        };
        let mut v: Vec<(String, String)> = Vec::new();
        if let Ok(c) =
            Leitor::novo(consulta, &mut self.cache).carregada(&pedido, crate::leitura::PEDACO)
        {
            for l in &c.tabela.linhas {
                for cel in &l.celulas {
                    if let sbf_inventario::consulta::Celula::Link { alvo, texto } = cel
                        && alvo.tipo == Tipo::Conexao
                        && !v.iter().any(|(id, _)| *id == alvo.chave)
                    {
                        v.push((alvo.chave.clone(), texto.clone()));
                    }
                }
            }
        }
        v
    }

    /// As primeiras `n` linhas de uma tabela, lida em pedaços (a Paleta usa).
    pub fn primeiras(
        &mut self,
        consulta: &dyn Consulta,
        pedido: &PedidoDeTabela,
        n: usize,
    ) -> Result<crate::leitura::Carregada, String> {
        Leitor::novo(consulta, &mut self.cache).carregada(pedido, n)
    }

    /// Lê a Visão geral numa thread, com uma conexão própria, para a janela abrir sem
    /// esperar (no inventário real ela leva meio segundo).
    pub fn pre_carregar_visao_geral(&mut self, banco: std::path::PathBuf) {
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let r = sbf_inventario::leitor::LeitorSqlite::abrir(&banco)
                .and_then(|l| l.visao_geral())
                .map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.visao_geral_pendente = Some(rx);
    }

    /// Uma tabela pela leitura com cache (a Paleta usa).
    pub fn tabela(
        &mut self,
        consulta: &dyn Consulta,
        pedido: &PedidoDeTabela,
    ) -> crate::leitura::Lido<sbf_inventario::consulta::Tabela> {
        Leitor::novo(consulta, &mut self.cache).tabela(pedido)
    }

    /// O nome de um item (trilha, títulos).
    pub fn rotulo(&mut self, consulta: &dyn Consulta, alvo: &Ref) -> String {
        Leitor::novo(consulta, &mut self.cache).rotulo(alvo)
    }

    /// A casca abriu um destino no centro (clique, Voltar, Paleta…): acompanha o histórico
    /// da casca, limpa a seleção e, num lote, faz dele o lote atual.
    pub fn ao_abrir_no_centro(&mut self, destino: Destino) {
        if *self.navegacao.atual() != destino {
            self.navegacao.abrir(destino.clone());
            self.centro.selecao.limpar();
        }
        if let Destino::Item(r) = &destino
            && r.tipo == Tipo::Lote
        {
            self.lote_atual = Some(r.clone());
        }
    }

    /// O nome do lote atual, para a faixa de Lotes.
    pub fn rotulo_do_lote_atual(&mut self, consulta: &dyn Consulta) -> Option<String> {
        let lote = self.lote_atual.clone()?;
        Some(Leitor::novo(consulta, &mut self.cache).rotulo(&lote))
    }

    /// O item aberto no centro, se for uma página.
    pub fn item_aberto(&self) -> Option<&Ref> {
        match self.navegacao.atual() {
            Destino::Item(r) => Some(r),
            _ => None,
        }
    }

    /// O item que o dock mostra: a última linha clicada, ou a página aberta.
    pub fn foco(&self) -> Option<Ref> {
        self.centro
            .selecao
            .foco
            .clone()
            .or_else(|| self.item_aberto().cloned())
    }

    /// A seleção que as Ações usam: a da tabela ou, sem ela, o item aberto.
    pub fn selecao(&self) -> Vec<Ref> {
        if self.centro.selecao.itens.is_empty() {
            self.item_aberto().cloned().into_iter().collect()
        } else {
            self.centro.selecao.itens.clone()
        }
    }

    /// O contexto para habilitar as Ações (Ribbon, Paleta, faixa provisória).
    pub fn contexto_das_acoes(&self) -> ContextoDasAcoes {
        ContextoDasAcoes {
            selecao: self.selecao(),
            lote_atual: self.lote_atual.clone(),
            pode_voltar: self.navegacao.pode_voltar(),
            pode_avancar: self.navegacao.pode_avancar(),
            item_aberto: self.item_aberto().cloned(),
            pdf_ligado: self.ligacoes.pdf || self.ferramentas.is_some(),
            estante_ligada: self.ligacoes.estante || self.ferramentas.is_some(),
            cofre_aberto: self.ligacoes.cofre || self.ferramentas.is_some(),
            destino_da_estante: self
                .ferramentas
                .as_ref()
                .is_some_and(|f| f.tem_destino_da_estante()),
            destino_da_copia: self
                .ferramentas
                .as_ref()
                .is_some_and(|f| !f.destino_da_copia.trim().is_empty()),
            ferramenta_ocupada: self.ferramentas.as_ref().is_some_and(|f| f.ocupada()),
            estacao_ligada: self.estacao.is_some(),
            estacao_revogada: self.estacao.as_ref().is_some_and(|e| {
                e.0.estado().rede == sbf_inventario::alteracao::EstadoDaRede::EstacaoRevogada
            }),
            alteracao_recusada: self.alteracao_recusada_selecionada().is_some(),
            estacao_ativa: self.estacao_ativa_selecionada().is_some(),
        }
    }

    /// A alteração recusada selecionada (ou aberta), se houver.
    fn alteracao_recusada_selecionada(&self) -> Option<String> {
        let sel = self.selecao();
        let [r] = sel.as_slice() else { return None };
        if r.tipo != Tipo::Alteracao {
            return None;
        }
        let est = self.estacao.as_ref()?;
        let a = est.0.alteracao(&r.chave).ok().flatten()?;
        (a.estado == sbf_inventario::alteracao::EstadoDaAlteracao::Recusada).then_some(a.id)
    }

    /// A estação ativa selecionada (ou aberta), com o nome.
    fn estacao_ativa_selecionada(&self) -> Option<(Ref, String)> {
        let sel = self.selecao();
        let [r] = sel.as_slice() else { return None };
        if r.tipo != Tipo::Estacao {
            return None;
        }
        let est = self.estacao.as_ref()?;
        est.0
            .estacoes()
            .ok()?
            .into_iter()
            .find(|e| &e.alvo == r && e.credencial == sbf_inventario::alteracao::Credencial::Ativa)
            .map(|e| (e.alvo, e.nome))
    }

    /// O ciclo atual (`AAAA-MM`), pelo dia de corte 20.
    pub fn ciclo_atual(&self) -> String {
        ciclo_de(&self.hoje)
    }

    /// O filtro da aba Início, montado da pesquisa e do período.
    pub fn filtro(&self) -> Filtro {
        let atual = self.ciclo_atual();
        let ciclos = match self.periodo {
            Periodo::TodosOsCiclos => None,
            Periodo::CicloAtual => Some((atual.clone(), atual)),
            Periodo::CicloAnterior => {
                let a = ciclo_anterior(&atual);
                Some((a.clone(), a))
            }
            Periodo::Intervalo => self.intervalo.clone(),
        };
        Filtro {
            texto: Some(self.pesquisa.trim().to_owned()).filter(|t| !t.is_empty()),
            ciclos,
            quem_decidiu: self.quem_decidiu.clone(),
            estados: self.estado_do_achado.iter().cloned().collect(),
            fontes: self.fonte.iter().cloned().collect(),
            tipos: self.tipo_do_documento.iter().cloned().collect(),
        }
    }

    /// Esquece as leituras: a próxima vez vai à `Consulta` (Atualizar, ou depois de
    /// algo que os outros crates mudaram no banco).
    pub fn atualizar(&mut self) {
        self.cache.limpar();
        self.previas.limpar();
    }

    /// As prévias destacadas em janela própria e a dica com a miniatura. Uma vez por quadro,
    /// depois do centro.
    pub fn previas_ui(&mut self, ctx: &Context, consulta: &dyn Consulta) {
        let mut leitor = Leitor::novo(consulta, &mut self.cache);
        self.previas.janelas_ui(ctx, &mut leitor, &mut self.pedidos);
    }

    /// As contagens da barra de status.
    pub fn contagens(&mut self, consulta: &dyn Consulta) -> Option<Contagens> {
        Leitor::novo(consulta, &mut self.cache)
            .contagens()
            .ok()
            .map(|c| *c)
    }

    /// "Seu Bolso Feliz — Energia Exemplo".
    pub fn titulo(&mut self, consulta: &dyn Consulta) -> String {
        let atual = self.navegacao.atual().clone();
        let nome = match &atual {
            Destino::Item(r) => Leitor::novo(consulta, &mut self.cache).rotulo(r),
            outro => areas::rotulo_do_destino(outro),
        };
        format!("Seu Bolso Feliz — {nome}")
    }

    // ------------------------------------------------------------------ vistas

    /// O painel de navegação (à esquerda).
    pub fn navegacao_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let contagens = self.contagens(consulta);
        let atual = self.navegacao.atual().clone();
        areas::painel_de_navegacao(ui, &atual, contagens.as_ref(), &mut self.pedidos);
    }

    /// A trilha com Voltar e Avançar ("Inventário › Fornecedores › Energia Exemplo").
    pub fn trilha_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        let p = tema::paleta(ui.ctx());
        let atual = self.navegacao.atual().clone();
        let mut leitor = Leitor::novo(consulta, &mut self.cache);
        let pedidos = &mut self.pedidos;
        ui.horizontal(|ui| {
            if componentes::botao_icone(
                ui,
                "arrow_back",
                "Voltar (Alt+←)",
                self.navegacao.pode_voltar(),
            )
            .clicked()
            {
                pedidos.push(Pedido::Voltar);
            }
            if componentes::botao_icone(
                ui,
                "arrow_forward",
                "Avançar (Alt+→)",
                self.navegacao.pode_avancar(),
            )
            .clicked()
            {
                pedidos.push(Pedido::Avancar);
            }
            let separador = |ui: &mut Ui| {
                ui.label(icones::icone_cor("chevron_right", 14.0, p.tinta_suave));
            };
            let destino_da_area = match &atual {
                Destino::Item(r) => areas::destino_do_tipo(r.tipo),
                Destino::Total { .. } => None,
                outro => Some(outro.clone()),
            };
            if let Some(d) = &destino_da_area {
                ui.label(RichText::new(areas::grupo_de(d)).color(p.tinta_suave));
                separador(ui);
                if matches!(atual, Destino::Item(_)) {
                    componentes::link_para(ui, &areas::rotulo_do_destino(d), d.clone(), pedidos);
                } else {
                    ui.label(
                        RichText::new(areas::rotulo_do_destino(d))
                            .strong()
                            .color(p.tinta),
                    );
                }
            }
            match &atual {
                Destino::Item(r) => {
                    separador(ui);
                    let rotulo = leitor.rotulo(r);
                    ui.label(RichText::new(rotulo).strong().color(p.tinta));
                }
                Destino::Total { titulo, .. } => {
                    ui.label(RichText::new("Total").color(p.tinta_suave));
                    separador(ui);
                    ui.label(RichText::new(titulo).strong().color(p.tinta));
                }
                _ => {}
            }
        });
    }

    /// O centro: a área, a página do item ou a lista de um total.
    pub fn centro_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta) {
        self.trilha_ui(ui, consulta);
        self.mensagem_ui(ui);
        let atual = self.navegacao.atual().clone();
        self.desenhar_destino(ui, consulta, &atual, None);
    }

    /// A mensagem da última Ação, no topo da página.
    pub fn mensagem_ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        if let Some(m) = &self.mensagem {
            let (fundo, borda, cor, icone) = if m.erro {
                (p.perigo_sutil, p.borda, p.perigo, "error")
            } else {
                (p.sucesso_sutil, p.destaque_borda, p.sucesso, "check_circle")
            };
            componentes::aviso(ui, icone, &m.texto, fundo, borda, cor);
        }
        ui.add_space(4.0);
    }

    /// Desenha um destino: a área, a página do item ou a lista de um total. `vista` é a de
    /// um painel flutuante; `None` é o centro (cuja seleção as Ações usam).
    pub fn desenhar_destino(
        &mut self,
        ui: &mut Ui,
        consulta: &dyn Consulta,
        destino: &Destino,
        vista: Option<&mut Vista>,
    ) {
        match vista {
            Some(v) => self.desenhar_destino_em(ui, consulta, destino, v),
            None => {
                let mut centro = std::mem::take(&mut self.centro);
                self.desenhar_destino_em(ui, consulta, destino, &mut centro);
                self.centro = centro;
            }
        }
    }

    fn desenhar_destino_em(
        &mut self,
        ui: &mut Ui,
        consulta: &dyn Consulta,
        destino: &Destino,
        vista: &mut Vista,
    ) {
        let p = tema::paleta(ui.ctx());
        let filtro = self.filtro();
        let banco = self.ferramentas.as_ref().map(|f| f.banco.clone());
        let descricao_do_filtro_atual = descricao_do_filtro(&filtro, &self.ciclo_atual());
        let hoje = self.hoje.clone();
        let estacao: Option<&dyn crate::dinheiro::EstacaoDoLivro> =
            self.estacao.as_ref().map(|e| e.0.as_ref());
        let sinc = estacao.map(|e| e.estado());
        let contexto_da_pagina = pagina::ContextoDaPagina {
            estacao,
            hoje: &hoje,
        };
        let mut leitor = Leitor::novo(consulta, &mut self.cache);
        let pedidos = &mut self.pedidos;
        let previas = &mut self.previas;
        let pendentes = &mut self.pedacos_pendentes;
        match destino {
            Destino::Area(Area::Sincronizacao) => {
                cabecalho_da_area(
                    ui,
                    Area::Sincronizacao,
                    None,
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                crate::dinheiro::area_sincronizacao(
                    ui,
                    estacao,
                    self.pareamento.as_deref(),
                    &hoje,
                    vista,
                    pedidos,
                );
            }
            Destino::Area(area) if area.e_do_livro_caixa() => {
                let pesquisa = if *area == Area::Hoje {
                    None
                } else {
                    Some(&mut self.pesquisa)
                };
                cabecalho_da_area(
                    ui,
                    *area,
                    pesquisa,
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                crate::dinheiro::faixa_da_estacao(ui, sinc.as_ref(), &hoje);
                let hora_da_copia = crate::dinheiro::hora_da_copia(sinc.as_ref(), &hoje);
                let painel = match leitor.painel_da_area(*area, &filtro) {
                    Ok(painel) => painel,
                    Err(e) => {
                        crate::dinheiro::motivo_ou_erro(ui, &e);
                        return;
                    }
                };
                crate::dinheiro::indicadores(ui, &painel, hora_da_copia.as_deref(), pedidos);
                let pedido = PedidoDeTabela::Area {
                    area: *area,
                    filtro,
                };
                let chave = format!("area:{area:?}");
                if *area == Area::Hoje {
                    egui::ScrollArea::vertical()
                        .id_salt("hoje")
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            crate::dinheiro::secoes(ui, &painel, *area, vista, pedidos)
                        });
                } else if painel.secoes.is_empty() {
                    if let Err(e) = pagina::tabela_em_pedacos(
                        ui,
                        &mut leitor,
                        &pedido,
                        &chave,
                        vista,
                        pedidos,
                        None,
                        true,
                    ) {
                        crate::dinheiro::motivo_ou_erro(ui, &e);
                    }
                } else {
                    egui::ScrollArea::vertical()
                        .id_salt(("dinheiro", *area as u8))
                        .auto_shrink([false, false])
                        .show(ui, |ui| {
                            let n = leitor.carregada(&pedido, 0).map(|c| c.total).unwrap_or(0);
                            let altura = tabela::altura_para(ui, n, 340.0);
                            if let Err(e) = pagina::tabela_em_pedacos(
                                ui,
                                &mut leitor,
                                &pedido,
                                &chave,
                                vista,
                                pedidos,
                                Some(altura),
                                true,
                            ) {
                                crate::dinheiro::motivo_ou_erro(ui, &e);
                            }
                            crate::dinheiro::secoes(ui, &painel, *area, vista, pedidos);
                        });
                }
            }
            Destino::Area(Area::VisaoGeral) => {
                cabecalho_da_area(
                    ui,
                    Area::VisaoGeral,
                    None,
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                if let Some(rx) = &self.visao_geral_pendente {
                    match rx.try_recv() {
                        Ok(r) => {
                            leitor.guardar_visao_geral(r);
                            self.visao_geral_pendente = None;
                        }
                        Err(std::sync::mpsc::TryRecvError::Empty) => {
                            // As contagens da barra chegam em milissegundos: mostra antes.
                            if let Ok(c) = leitor.contagens() {
                                ui.label(format!(
                                    "{} achados · {} documentos · {} em dúvida · {} a triar",
                                    formato::inteiro(c.achados),
                                    formato::inteiro(c.documentos),
                                    formato::inteiro(c.em_duvida),
                                    formato::inteiro(c.a_triar)
                                ));
                            }
                            carregando(ui, "Carregando a Visão geral…");
                            return;
                        }
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            self.visao_geral_pendente = None;
                        }
                    }
                }
                painel::visao_geral(ui, &mut leitor, vista, pedidos);
            }
            Destino::Area(Area::Relatorios) => {
                cabecalho_da_area(
                    ui,
                    Area::Relatorios,
                    None,
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                painel::relatorios(
                    ui,
                    &mut leitor,
                    &mut self.relatorios,
                    &filtro,
                    vista,
                    pedidos,
                    estacao.is_some(),
                );
            }
            Destino::Area(area) => {
                cabecalho_da_area(
                    ui,
                    *area,
                    Some(&mut self.pesquisa),
                    &mut self.periodo,
                    &mut self.focar_pesquisa,
                );
                if *area == Area::Lotes {
                    faixa_do_lote(ui, &mut leitor, self.lote_atual.as_ref(), pedidos);
                }
                if let Some(outra) = areas::area_complementar(*area) {
                    let pedido = PedidoDeTabela::Area {
                        area: outra,
                        filtro: filtro.clone(),
                    };
                    componentes::rotulo_de_secao(ui, areas::rotulo_da_area(outra));
                    let chave = format!("area:{outra:?}");
                    let pronto = pedaco_pronto(pendentes, banco.as_deref(), &mut leitor, &pedido);
                    let n = if pronto {
                        leitor.carregada(&pedido, 0).map(|c| c.total).unwrap_or(0)
                    } else {
                        0
                    };
                    let altura = tabela::altura_para(ui, n, 220.0);
                    if let Err(e) = pagina::tabela_em_pedacos(
                        ui,
                        &mut leitor,
                        &pedido,
                        &chave,
                        vista,
                        pedidos,
                        Some(altura),
                        false,
                    ) {
                        componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo);
                    }
                    ui.add_space(6.0);
                    componentes::rotulo_de_secao(ui, areas::rotulo_da_area(*area));
                }
                let pedido = PedidoDeTabela::Area {
                    area: *area,
                    filtro,
                };
                if !pedaco_pronto(pendentes, banco.as_deref(), &mut leitor, &pedido) {
                    carregando(ui, "carregando…");
                    return;
                }
                match leitor.carregada(&pedido, 0) {
                    Ok(c) => {
                        ui.horizontal(|ui| {
                            let n = c.total;
                            let sel = vista.selecao.itens.len();
                            let mut texto = format!("{} {}", formato::inteiro(n as i64), if n == 1 { "linha" } else { "linhas" });
                            if sel > 0 {
                                texto.push_str(&format!(" · {sel} selecionada{}", if sel == 1 { "" } else { "s" }));
                            }
                            ui.label(RichText::new(&descricao_do_filtro_atual).size(12.0).color(p.tinta_suave));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(texto).size(12.0).color(p.tinta_suave))
                                    .on_hover_text("Clique seleciona, Ctrl+clique soma à seleção, Shift+clique seleciona o intervalo, duplo clique abre, ↑/↓ movem.");
                            });
                        });
                        let chave = format!("area:{area:?}");
                        if let Err(e) = pagina::tabela_em_pedacos(
                            ui,
                            &mut leitor,
                            &pedido,
                            &chave,
                            vista,
                            pedidos,
                            None,
                            true,
                        ) {
                            componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo);
                        }
                    }
                    Err(e) => {
                        componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo)
                    }
                }
            }
            Destino::AreaFutura(id) => {
                if let Some(a) = areas::area_futura(id) {
                    componentes::fatia_futura(ui, a.icone, a.rotulo, a.fatia, a.motivo);
                }
            }
            Destino::Item(r) => {
                egui::ScrollArea::vertical()
                    .id_salt("pagina")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        pagina::desenhar(
                            ui,
                            &mut leitor,
                            previas,
                            r,
                            vista,
                            pedidos,
                            contexto_da_pagina,
                        )
                    });
            }
            Destino::Total { titulo, pedido } => {
                pagina::desenhar_total(ui, &mut leitor, titulo, pedido, vista, pedidos);
            }
        }
    }

    /// Um painel do dock.
    pub fn dock_ui(&mut self, ui: &mut Ui, consulta: &dyn Consulta, painel: PainelDoDock) {
        let foco = self.foco();
        let mut leitor = Leitor::novo(consulta, &mut self.cache);
        if painel == PainelDoDock::Previa {
            let altura = ui.available_height();
            egui::ScrollArea::vertical()
                .id_salt("dock-previa")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    // Clicar na miniatura abre a página do item na aba Arquivo.
                    if let Some(achado) = self.previas.painel_ui(
                        ui,
                        &mut leitor,
                        foco.as_ref(),
                        altura,
                        &mut self.pedidos,
                    ) {
                        let destino_ref = foco
                            .clone()
                            .filter(|f| {
                                pagina::abas_de(f.tipo)
                                    .iter()
                                    .any(|a| a.conteudo == pagina::ConteudoDaAba::Arquivo)
                            })
                            .unwrap_or(achado);
                        if let Some(i) = pagina::abas_de(destino_ref.tipo)
                            .iter()
                            .position(|a| a.conteudo == pagina::ConteudoDaAba::Arquivo)
                        {
                            self.centro.abas.insert(destino_ref.clone(), i);
                        }
                        crate::navegacao::pedir_abrir(
                            ui.ctx(),
                            &mut self.pedidos,
                            Destino::Item(destino_ref),
                            false,
                        );
                    }
                });
            return;
        }
        dock::desenhar(
            ui,
            painel,
            &mut leitor,
            foco.as_ref(),
            &self.decisoes_da_sessao,
            &mut self.pedidos,
        );
    }

    /// Os painéis flutuantes (Ctrl+clique), como janelas egui dentro da janela.
    pub fn flutuantes_ui(&mut self, ctx: &Context, consulta: &dyn Consulta) {
        let p = tema::paleta(ctx);
        let lista: Vec<_> = self.navegacao.flutuantes().to_vec();
        let hoje = self.hoje.clone();
        for f in lista {
            let contexto_da_pagina = pagina::ContextoDaPagina {
                estacao: self.estacao.as_ref().map(|e| e.0.as_ref()),
                hoje: &hoje,
            };
            let mut leitor = Leitor::novo(consulta, &mut self.cache);
            let titulo = match &f.destino {
                Destino::Item(r) => format!("{} · {}", r.tipo.singular(), leitor.rotulo(r)),
                outro => areas::rotulo_do_destino(outro),
            };
            let mut aberto = true;
            let vista = self.flutuantes.entry(f.id).or_default();
            let pedidos = &mut self.pedidos;
            let previas = &mut self.previas;
            egui::Window::new(RichText::new(titulo).strong())
                .id(Id::new(("flutuante", f.id)))
                .open(&mut aberto)
                .default_size([560.0, 460.0])
                .resizable(true)
                .collapsible(true)
                .frame(egui::Frame::window(&ctx.global_style()).fill(p.superficie_1))
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .show(ui, |ui| match &f.destino {
                            Destino::Item(r) => pagina::desenhar(
                                ui,
                                &mut leitor,
                                previas,
                                r,
                                vista,
                                pedidos,
                                contexto_da_pagina,
                            ),
                            Destino::Total { titulo, pedido } => pagina::desenhar_total(
                                ui,
                                &mut leitor,
                                titulo,
                                pedido,
                                vista,
                                pedidos,
                            ),
                            Destino::Area(a) => {
                                let pedido = PedidoDeTabela::Area {
                                    area: *a,
                                    filtro: Filtro::default(),
                                };
                                if let Ok(t) = leitor.tabela(&pedido) {
                                    let mut estado = tabela::EstadoDaTabela::default();
                                    tabela::desenhar(
                                        ui,
                                        ("flutuante-area", f.id),
                                        &t,
                                        &mut estado,
                                        &mut vista.selecao,
                                        pedidos,
                                        Some(380.0),
                                    );
                                }
                            }
                            Destino::AreaFutura(id) => {
                                if let Some(a) = areas::area_futura(id) {
                                    componentes::fatia_futura(
                                        ui, a.icone, a.rotulo, a.fatia, a.motivo,
                                    );
                                }
                            }
                        });
                });
            if !aberto {
                self.pedidos.push(Pedido::FecharFlutuante(f.id));
            }
        }
    }

    /// Os diálogos das Ações que pedem dados (corrigir, rejeitar, sinalizar, novo lote).
    pub fn dialogos_ui(&mut self, ctx: &Context) {
        let Some(mut dialogo) = self.dialogo.take() else {
            return;
        };
        let mut fechar = false;
        let mut confirmar = false;
        let resposta = egui::Modal::new(Id::new("sbf-dialogo")).show(ctx, |ui| {
            ui.set_width(380.0);
            match &mut dialogo {
                Dialogo::Texto {
                    acao,
                    alvos,
                    rotulo,
                    valor,
                } => {
                    let def = acoes::definicao(*acao);
                    componentes::titulo(ui, def.rotulo, 17.0);
                    ui.label(format!(
                        "{} {}",
                        alvos.len(),
                        if alvos.len() == 1 { "item" } else { "itens" }
                    ));
                    ui.add_space(6.0);
                    ui.label(rotulo.as_str());
                    let r = ui.add(egui::TextEdit::singleline(valor).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirmar = true;
                    }
                }
                Dialogo::DataEValor {
                    alvos,
                    data,
                    valor,
                    erro,
                } => {
                    componentes::titulo(ui, "Corrigir data e valor", 17.0);
                    ui.label(format!(
                        "{} achado. O ciclo é recalculado pela data.",
                        alvos.len()
                    ));
                    ui.add_space(6.0);
                    ui.label("Data (dd/mm/aaaa), vazio para não mudar");
                    ui.add(egui::TextEdit::singleline(data).desired_width(f32::INFINITY));
                    ui.label("Valor em reais (ex.: -312,44), vazio para não mudar");
                    ui.add(egui::TextEdit::singleline(valor).desired_width(f32::INFINITY));
                    if let Some(e) = erro {
                        ui.colored_label(tema::paleta(ui.ctx()).perigo, e.as_str());
                    }
                }
                Dialogo::Intervalo { de, ate, erro } => {
                    componentes::titulo(ui, "Período: intervalo de ciclos", 17.0);
                    ui.label("Do ciclo (AAAA-MM)");
                    ui.add(egui::TextEdit::singleline(de).desired_width(f32::INFINITY));
                    ui.label("Até o ciclo (AAAA-MM), inclusive");
                    ui.add(egui::TextEdit::singleline(ate).desired_width(f32::INFINITY));
                    if let Some(e) = erro {
                        ui.colored_label(tema::paleta(ui.ctx()).perigo, e.as_str());
                    }
                }
                Dialogo::NovaRegra {
                    fornecedor,
                    tipo_de_documento,
                    estrategia,
                    prioridade,
                    erro,
                } => {
                    componentes::titulo(ui, "Nova regra de senha", 17.0);
                    ui.label(
                        RichText::new("A regra diz só como montar a senha. A senha mesmo fica no chaveiro.")
                            .size(12.0)
                            .color(tema::paleta(ui.ctx()).tinta_suave),
                    );
                    ui.label("Fornecedor (como aparece nos registros)");
                    ui.add(egui::TextEdit::singleline(fornecedor).desired_width(f32::INFINITY));
                    ui.label("Tipo de documento (ex.: fatura, boleto, extrato; * para todos)");
                    ui.add(egui::TextEdit::singleline(tipo_de_documento).desired_width(f32::INFINITY));
                    ui.label("Estratégia");
                    egui::ComboBox::from_id_salt("estrategia-da-regra")
                        .selected_text(estrategia.as_str())
                        .show_ui(ui, |ui| {
                            for e in ESTRATEGIAS_SUGERIDAS {
                                ui.selectable_value(estrategia, (*e).to_owned(), *e);
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(estrategia)
                            .hint_text("cpf_primeiros:5, cpf_ultimos:5, cpf_completo, senha_propria:<rótulo>")
                            .desired_width(f32::INFINITY),
                    );
                    ui.label("Prioridade (menor é tentada primeiro; padrão 100)");
                    ui.add(egui::TextEdit::singleline(prioridade).desired_width(120.0));
                    if let Some(e) = erro {
                        ui.colored_label(tema::paleta(ui.ctx()).perigo, e.as_str());
                    }
                }
                Dialogo::MudarCategoria {
                    rotulo,
                    atual,
                    opcoes,
                    escolhida,
                    ..
                } => {
                    componentes::titulo(ui, "Mudar categoria", 17.0);
                    ui.label(rotulo.as_str());
                    ui.label(
                        RichText::new(format!(
                            "Hoje: {}. A mudança fica aguardando e vai ao servidor na próxima sincronização; ele aplica ou recusa.",
                            atual.as_deref().unwrap_or("Sem categoria")
                        ))
                        .size(12.0)
                        .color(tema::paleta(ui.ctx()).tinta_suave),
                    );
                    ui.add_space(6.0);
                    let texto = escolhida
                        .and_then(|i| opcoes.get(i))
                        .map(|o| o.nome.clone())
                        .unwrap_or_else(|| "Sem categoria".to_owned());
                    egui::ComboBox::from_id_salt("mudar-categoria")
                        .selected_text(texto)
                        .width(300.0)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(escolhida, None, "Sem categoria");
                            for (i, o) in opcoes.iter().enumerate() {
                                let rotulo = match &o.grupo {
                                    Some(g) => format!("{} · {g}", o.nome),
                                    None => o.nome.clone(),
                                };
                                ui.selectable_value(escolhida, Some(i), rotulo);
                            }
                        });
                }
                Dialogo::RevogarEstacao { nome, .. } => {
                    componentes::titulo(ui, "Revogar estação", 17.0);
                    ui.label(format!(
                        "Revogar a estação \u{201c}{nome}\u{201d}? Ela deixa de sincronizar e, no próximo contato, apaga a cópia do livro-caixa e a credencial. O cofre e o inventário dela ficam. Nada é apagado do servidor."
                    ));
                }
                Dialogo::RenomearEstacao { nome, .. } => {
                    componentes::titulo(ui, "Renomear estação", 17.0);
                    ui.label("Nome da estação");
                    let r = ui.add(egui::TextEdit::singleline(nome).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirmar = true;
                    }
                }
                Dialogo::NovoLote { nome } => {
                    componentes::titulo(ui, "Novo lote", 17.0);
                    ui.label("Nome do lote");
                    let r = ui.add(egui::TextEdit::singleline(nome).desired_width(f32::INFINITY));
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        confirmar = true;
                    }
                }
            }
            ui.add_space(10.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("Confirmar").clicked() {
                    confirmar = true;
                }
                if ui.button("Cancelar").clicked() {
                    fechar = true;
                }
            });
        });
        if resposta.should_close() {
            fechar = true;
        }
        if confirmar {
            match confirmar_dialogo(&dialogo) {
                Ok(pedido) => {
                    self.pedidos.push(pedido);
                    return;
                }
                Err(e) => {
                    if let Dialogo::DataEValor { erro, .. }
                    | Dialogo::Intervalo { erro, .. }
                    | Dialogo::NovaRegra { erro, .. } = &mut dialogo
                    {
                        *erro = Some(e);
                    } else {
                        self.mensagem = Some(Mensagem {
                            texto: e,
                            erro: true,
                        });
                    }
                }
            }
        }
        if !fechar {
            self.dialogo = Some(dialogo);
        }
    }

    /// Lê os atalhos da lista única e os botões laterais do mouse.
    pub fn atalhos(&mut self, ctx: &Context) {
        if self.dialogo.is_some() {
            return;
        }
        if let Some(id) = acoes::atalho_apertado(ctx)
            && acoes::disponibilidade(id, &self.contexto_das_acoes()).habilitada()
        {
            self.executar(id);
        }
        let (tras, frente) = ctx.input(|i| {
            (
                i.pointer.button_clicked(PointerButton::Extra1),
                i.pointer.button_clicked(PointerButton::Extra2),
            )
        });
        if tras {
            self.pedidos.push(Pedido::Voltar);
        }
        if frente {
            self.pedidos.push(Pedido::Avancar);
        }
    }

    /// Se a pesquisa deve ganhar o foco neste quadro (Ctrl+F).
    pub fn pesquisa(&mut self) -> &mut String {
        &mut self.pesquisa
    }

    // ------------------------------------------------------------------ escrita

    /// Aplica os pedidos do quadro: navegação, Ações e decisões. Chame depois de desenhar.
    pub fn processar(
        &mut self,
        ctx: &Context,
        consulta: &dyn Consulta,
        comandos: &mut dyn Comandos,
    ) {
        // Clicar num lote (na área Lotes ou numa aba) faz dele o lote atual.
        if let Some(r) = &self.centro.selecao.foco
            && r.tipo == Tipo::Lote
            && self.lote_atual.as_ref() != Some(r)
        {
            self.lote_atual = Some(r.clone());
        }
        let pedidos = std::mem::take(&mut self.pedidos);
        for pedido in pedidos {
            if self.na_casca && self.encaminhar_para_a_casca(ctx, &pedido) {
                continue;
            }
            let antes = self.navegacao.atual().clone();
            if self.navegacao.aplicar(&pedido) {
                let depois = self.navegacao.atual().clone();
                if antes != depois {
                    self.centro.selecao.limpar();
                    if let Destino::Item(r) = &depois
                        && r.tipo == Tipo::Lote
                    {
                        self.lote_atual = Some(r.clone());
                    }
                }
                self.flutuantes
                    .retain(|id, _| self.navegacao.flutuantes().iter().any(|f| f.id == *id));
                continue;
            }
            match pedido {
                Pedido::Executar(id) => self.acao(ctx, consulta, comandos, id),
                Pedido::Decidir(d) => self.decidir(comandos, d),
                Pedido::Desfazer(id) => match comandos.desfazer(id) {
                    Ok(nova) => {
                        self.decisoes_da_sessao.push(nova);
                        self.ok(format!("Decisão {id} desfeita pela decisão {nova}."));
                    }
                    Err(e) => self.erro(e.to_string()),
                },
                Pedido::CriarLote(nome) => match comandos.criar_lote(&nome) {
                    Ok(lote) => {
                        self.ok(format!(
                            "Lote \u{201c}{nome}\u{201d} criado. Ele é o lote atual."
                        ));
                        self.lote_atual = Some(lote);
                    }
                    Err(e) => self.erro(e.to_string()),
                },
                Pedido::EscolherLote(r) => self.lote_atual = Some(r),
                Pedido::NovaRegra {
                    fornecedor,
                    tipo_de_documento,
                } => {
                    self.dialogo = Some(Dialogo::NovaRegra {
                        fornecedor,
                        tipo_de_documento,
                        estrategia: ESTRATEGIAS_SUGERIDAS[0].to_owned(),
                        prioridade: "100".to_owned(),
                        erro: None,
                    });
                }
                Pedido::Intervalo(de, ate) => {
                    self.intervalo = Some((de, ate));
                    self.periodo = Periodo::Intervalo;
                }
                Pedido::Copiar(texto) => {
                    ctx.copy_text(texto);
                    self.mensagem = Some(Mensagem {
                        texto: "Copiado.".to_owned(),
                        erro: false,
                    });
                }
                Pedido::Alterar(pedido) => self.alterar(pedido),
                Pedido::RefazerAlteracao(id) => self.refazer(&id),
                Pedido::DescartarAlteracao(id) => self.descartar(&id),
                Pedido::Sincronizar => self.sincronizar(),
                Pedido::Parear => {
                    if let Some(p) = self.pareamento.as_mut() {
                        p.iniciar();
                    }
                }
                Pedido::RevogarEstacao(r) => match self.estacao.as_mut().map(|e| e.0.revogar(&r)) {
                    Some(Ok(())) => self.ok(
                        "Estação revogada. O que ela já sincronizou continua no servidor."
                            .to_owned(),
                    ),
                    Some(Err(e)) => self.erro(e.to_string()),
                    None => self.erro(acoes::MOTIVO_SEM_ESTACAO.to_owned()),
                },
                Pedido::RenomearEstacao(r, nome) => {
                    match self.estacao.as_mut().map(|e| e.0.renomear(&r, &nome)) {
                        Some(Ok(())) => {
                            self.ok(format!("Estação renomeada para \u{201c}{nome}\u{201d}."))
                        }
                        Some(Err(e)) => self.erro(e.to_string()),
                        None => self.erro(acoes::MOTIVO_SEM_ESTACAO.to_owned()),
                    }
                }
                // Já tratados pela navegação.
                Pedido::Abrir(_)
                | Pedido::AbrirFlutuante(_)
                | Pedido::FecharFlutuante(_)
                | Pedido::Voltar
                | Pedido::Avancar => {}
            }
        }
    }

    /// Na casca, os pedidos de navegação vão para a fila dela.
    fn encaminhar_para_a_casca(&mut self, ctx: &Context, pedido: &Pedido) -> bool {
        use bancada_casca::{Local, Pedido as P, Rota};
        let p = match pedido {
            Pedido::Abrir(d) => P::Abrir {
                rota: Rota::nova(d.rota()),
                local: Local::Centro,
            },
            Pedido::AbrirFlutuante(d) => P::AbrirFlutuante(Rota::nova(d.rota())),
            Pedido::Voltar => P::Voltar(Local::Centro),
            Pedido::Avancar => P::Avancar(Local::Centro),
            Pedido::FecharFlutuante(_) => return true,
            _ => return false,
        };
        bancada_casca::pedir(ctx, p);
        true
    }

    fn ok(&mut self, texto: String) {
        self.cache.limpar();
        self.mensagem = Some(Mensagem { texto, erro: false });
    }

    fn erro(&mut self, texto: String) {
        self.mensagem = Some(Mensagem { texto, erro: true });
    }

    fn decidir(&mut self, comandos: &mut dyn Comandos, pedido: PedidoDeDecisao) {
        match comandos.executar(&pedido) {
            Ok(ids) => {
                let n = pedido.alvos.len();
                let rotulo = rotulo_da_acao(&pedido.acao);
                self.decisoes_da_sessao.extend(ids.iter().copied());
                self.ok(format!(
                    "{rotulo}: {n} {} · {} decisão{} gravada{}. Desfazer: Ctrl+Z.",
                    if n == 1 { "item" } else { "itens" },
                    ids.len(),
                    if ids.len() == 1 { "" } else { "ões" },
                    if ids.len() == 1 { "" } else { "s" },
                ));
                if matches!(
                    pedido.acao,
                    Acao::MarcarDocumento | Acao::Descartar | Acao::EmDuvida
                ) {
                    self.centro.selecao.itens.clear();
                }
            }
            Err(e) => self.erro(e.to_string()),
        }
    }

    /// Grava uma alteração na fila da estação (vai na próxima sincronização).
    fn alterar(&mut self, pedido: sbf_inventario::alteracao::PedidoDeAlteracao) {
        let Some(est) = self.estacao.as_mut() else {
            self.erro(acoes::MOTIVO_SEM_ESTACAO.to_owned());
            return;
        };
        match est.0.propor(&pedido) {
            Ok(a) => self.ok(format!(
                "{}: {} → {}. Aguardando o servidor: vai na próxima sincronização.",
                a.rotulo(),
                a.antes,
                a.depois
            )),
            Err(e) => self.erro(e.to_string()),
        }
    }

    fn refazer(&mut self, id: &str) {
        let Some(est) = self.estacao.as_mut() else {
            return;
        };
        match est.0.refazer(id) {
            Ok(nova) => self.ok(format!(
                "Refeita sobre a versão nova do servidor: {} aguardando a próxima sincronização.",
                nova.rotulo()
            )),
            Err(e) => self.erro(e.to_string()),
        }
    }

    fn descartar(&mut self, id: &str) {
        let Some(est) = self.estacao.as_mut() else {
            return;
        };
        match est.0.descartar(id) {
            Ok(()) => self.ok("Alteração descartada: vale a versão do servidor.".to_owned()),
            Err(e) => self.erro(e.to_string()),
        }
    }

    fn sincronizar(&mut self) {
        let Some(est) = self.estacao.as_mut() else {
            self.erro(acoes::MOTIVO_SEM_ESTACAO.to_owned());
            return;
        };
        est.0.sincronizar_agora();
        let estado = est.0.estado();
        let (texto, _, aviso) = crate::dinheiro::texto_da_barra(&estado, &self.hoje);
        // Releitura: a troca pode ter mudado o livro-caixa.
        self.cache.limpar();
        self.mensagem = Some(Mensagem { texto, erro: aviso });
    }

    fn acao(
        &mut self,
        ctx: &Context,
        consulta: &dyn Consulta,
        comandos: &mut dyn Comandos,
        id: IdAcao,
    ) {
        let contexto = self.contexto_das_acoes();
        if let Some(m) = acoes::disponibilidade(id, &contexto).motivo() {
            self.erro(format!("{}: {m}", acoes::definicao(id).rotulo));
            return;
        }
        let sel = contexto.selecao.clone();
        let simples = |acao: Acao| PedidoDeDecisao {
            acao,
            alvos: sel.clone(),
            motivo: None,
        };
        let texto = |acao: IdAcao, rotulo: &str| Dialogo::Texto {
            acao,
            alvos: sel.clone(),
            rotulo: rotulo.to_owned(),
            valor: String::new(),
        };
        match id {
            IdAcao::Voltar => {
                self.navegacao.voltar();
                self.centro.selecao.limpar();
            }
            IdAcao::Avancar => {
                self.navegacao.avancar();
                self.centro.selecao.limpar();
            }
            IdAcao::Atualizar => {
                self.atualizar();
                self.mensagem = Some(Mensagem {
                    texto: "Inventário relido.".to_owned(),
                    erro: false,
                });
            }
            IdAcao::Pesquisar => self.focar_pesquisa = true,
            IdAcao::Periodo
            | IdAcao::QuemDecidiu
            | IdAcao::EstadoDoAchado
            | IdAcao::TipoDoDocumento
            | IdAcao::FonteDoAchado => {}
            IdAcao::PeriodoIntervalo => {
                let atual = self.ciclo_atual();
                let (de, ate) = self
                    .intervalo
                    .clone()
                    .unwrap_or_else(|| (ciclo_anterior(&ciclo_anterior(&atual)), atual));
                self.dialogo = Some(Dialogo::Intervalo {
                    de,
                    ate,
                    erro: None,
                });
            }
            IdAcao::NovaRegraDeSenha => {
                let fornecedor = sel
                    .first()
                    .filter(|r| r.tipo == Tipo::Fornecedor)
                    .map(|r| r.chave.clone())
                    .unwrap_or_default();
                self.dialogo = Some(Dialogo::NovaRegra {
                    fornecedor,
                    tipo_de_documento: "fatura".to_owned(),
                    estrategia: ESTRATEGIAS_SUGERIDAS[0].to_owned(),
                    prioridade: "100".to_owned(),
                    erro: None,
                });
            }
            IdAcao::EstadoTodos
            | IdAcao::EstadoATriar
            | IdAcao::EstadoDocumento
            | IdAcao::EstadoEmDuvida
            | IdAcao::EstadoDescartado
            | IdAcao::TipoTodos
            | IdAcao::TipoBoleto
            | IdAcao::TipoFatura
            | IdAcao::TipoExtrato
            | IdAcao::TipoComprovante
            | IdAcao::TipoNotaFiscal
            | IdAcao::TipoFoto
            | IdAcao::TipoOutro => {
                if let Some(f) = acoes::filtro_de_estado(id) {
                    self.estado_do_achado = f.map(str::to_owned);
                } else if let Some(f) = acoes::filtro_de_tipo(id) {
                    self.tipo_do_documento = f.map(str::to_owned);
                }
            }
            IdAcao::PeriodoAtual => self.periodo = Periodo::CicloAtual,
            IdAcao::PeriodoAnterior => self.periodo = Periodo::CicloAnterior,
            IdAcao::PeriodoTodos => self.periodo = Periodo::TodosOsCiclos,
            IdAcao::QuemTodos => self.quem_decidiu = None,
            IdAcao::QuemVoce => self.quem_decidiu = Some(sbf_inventario::consulta::Quem::Usuario),
            IdAcao::QuemRegras => {
                // Texto vazio: qualquer rodada de regras.
                self.quem_decidiu = Some(sbf_inventario::consulta::Quem::Automatico(String::new()));
            }
            IdAcao::AbrirEmPainelFlutuante => {
                if let Some(r) = sel.first() {
                    if self.na_casca {
                        bancada_casca::abrir_flutuante(ctx, Destino::Item(r.clone()).rota());
                    } else {
                        self.navegacao.abrir_flutuante(Destino::Item(r.clone()));
                    }
                }
            }
            IdAcao::Documento => self.decidir(comandos, simples(Acao::MarcarDocumento)),
            IdAcao::Descartar => self.decidir(comandos, simples(Acao::Descartar)),
            IdAcao::EmDuvida => self.decidir(comandos, simples(Acao::EmDuvida)),
            IdAcao::CorrigirFornecedor => self.dialogo = Some(texto(id, "Fornecedor")),
            IdAcao::CorrigirCategoria => self.dialogo = Some(texto(id, "Categoria")),
            IdAcao::CorrigirTipo => {
                self.dialogo = Some(texto(id, "Tipo (ex.: boleto, fatura_cartao, compra)"))
            }
            IdAcao::CorrigirDataEValor => {
                self.dialogo = Some(Dialogo::DataEValor {
                    alvos: sel,
                    data: String::new(),
                    valor: String::new(),
                    erro: None,
                });
            }
            IdAcao::MarcarDuplicidade | IdAcao::UnirConciliados => {
                if let [a, b] = sel.as_slice() {
                    let acao = if id == IdAcao::MarcarDuplicidade {
                        Acao::MarcarDuplicidade { com: b.clone() }
                    } else {
                        Acao::UnirConciliados { com: b.clone() }
                    };
                    self.decidir(
                        comandos,
                        PedidoDeDecisao {
                            acao,
                            alvos: vec![a.clone()],
                            motivo: None,
                        },
                    );
                }
            }
            IdAcao::ConfirmarRecorrencia => {
                self.decidir(comandos, simples(Acao::ConfirmarRecorrencia))
            }
            IdAcao::Aprovar => self.decidir(comandos, simples(Acao::Aprovar)),
            IdAcao::Rejeitar => self.dialogo = Some(texto(id, "Motivo (obrigatório)")),
            IdAcao::Sinalizar => self.dialogo = Some(texto(id, "A pergunta que trava o item")),
            IdAcao::Responder => self.dialogo = Some(texto(id, "Resposta")),
            IdAcao::Desfazer => match comandos.desfazer_ultima() {
                Ok(Some(nova)) => {
                    self.decisoes_da_sessao.push(nova);
                    self.ok(format!("Última decisão desfeita (decisão {nova})."));
                }
                Ok(None) => self.erro("Nada a desfazer.".to_owned()),
                Err(e) => self.erro(e.to_string()),
            },
            IdAcao::VerDecisoes => {
                if !self.paineis_visiveis.contains(&PainelDoDock::Historico) {
                    self.paineis_visiveis.push(PainelDoDock::Historico);
                }
            }
            IdAcao::NovoLote => {
                self.dialogo = Some(Dialogo::NovoLote {
                    nome: String::new(),
                })
            }
            IdAcao::AdicionarSelecao
            | IdAcao::RemoverDoLote
            | IdAcao::AprovarLote
            | IdAcao::DesaprovarLote => {
                let Some(lote) = self.lote_atual.clone() else {
                    return;
                };
                let r = match id {
                    IdAcao::AdicionarSelecao => comandos.adicionar_ao_lote(&lote, &sel).map(|()| format!("{} itens no lote.", sel.len())),
                    IdAcao::RemoverDoLote => comandos.remover_do_lote(&lote, &sel).map(|()| format!("{} itens tirados do lote.", sel.len())),
                    IdAcao::AprovarLote => comandos.aprovar_lote(&lote).map(|()| "Lote aprovado: os registros dele ficam aprovados. Enviar chega na fatia 3.".to_owned()),
                    _ => comandos.desaprovar_lote(&lote).map(|()| "Lote de volta a editável.".to_owned()),
                };
                match r {
                    Ok(t) => self.ok(t),
                    Err(e) => self.erro(e.to_string()),
                }
            }
            IdAcao::PainelDetalhe
            | IdAcao::PainelOrigens
            | IdAcao::PainelTexto
            | IdAcao::PainelHistorico => {
                let painel = match id {
                    IdAcao::PainelDetalhe => PainelDoDock::Detalhe,
                    IdAcao::PainelOrigens => PainelDoDock::Origens,
                    IdAcao::PainelTexto => PainelDoDock::Texto,
                    _ => PainelDoDock::Historico,
                };
                if let Some(i) = self.paineis_visiveis.iter().position(|x| *x == painel) {
                    self.paineis_visiveis.remove(i);
                } else {
                    self.paineis_visiveis.push(painel);
                }
            }
            IdAcao::VerTexto => {
                if !self.paineis_visiveis.contains(&PainelDoDock::Texto) {
                    self.paineis_visiveis.push(PainelDoDock::Texto);
                }
            }
            IdAcao::TrocarTema => {
                let todos = tema::temas();
                let i = todos
                    .iter()
                    .position(|t| t.id == self.tema)
                    .map_or(0, |i| (i + 1) % todos.len().max(1));
                if let Some(t) = todos.get(i) {
                    self.tema = t.id.clone();
                    tema::aplicar(ctx, &self.tema);
                    self.mensagem = Some(Mensagem {
                        texto: format!("Tema {}.", t.nome),
                        erro: false,
                    });
                }
            }
            IdAcao::CopiarHash => {
                if let Some(r) = sel.first() {
                    ctx.copy_text(r.chave.clone());
                    self.mensagem = Some(Mensagem {
                        texto: "Hash copiado.".to_owned(),
                        erro: false,
                    });
                }
            }
            IdAcao::SincronizarAgora => self.sincronizar(),
            IdAcao::MudarCategoria => {
                let Some(mov) = sel.first().cloned() else {
                    return;
                };
                let opcoes = self
                    .estacao
                    .as_ref()
                    .and_then(|e| e.0.categorias().ok())
                    .unwrap_or_default();
                let mut leitor = Leitor::novo(consulta, &mut self.cache);
                let rotulo = leitor.rotulo(&mov);
                let atual = leitor.item(&mov).ok().and_then(|i| {
                    i.as_ref().as_ref().and_then(|i| {
                        i.campos
                            .iter()
                            .find(|c| c.rotulo == "Categoria")
                            .map(|c| formato::texto_da_celula(&c.valor))
                    })
                });
                let escolhida = atual
                    .as_ref()
                    .and_then(|a| opcoes.iter().position(|o| &o.nome == a));
                self.dialogo = Some(Dialogo::MudarCategoria {
                    movimentacao: mov,
                    rotulo,
                    atual,
                    opcoes,
                    escolhida,
                });
            }
            IdAcao::RefazerAlteracao => {
                if let Some(id) = self.alteracao_recusada_selecionada() {
                    self.refazer(&id);
                }
            }
            IdAcao::DescartarAlteracao => {
                if let Some(id) = self.alteracao_recusada_selecionada() {
                    self.descartar(&id);
                }
            }
            IdAcao::RevogarEstacao => {
                if let Some((estacao, nome)) = self.estacao_ativa_selecionada() {
                    self.dialogo = Some(Dialogo::RevogarEstacao { estacao, nome });
                }
            }
            IdAcao::RenomearEstacao => {
                let Some(r) = sel.first().cloned() else {
                    return;
                };
                let nome = self
                    .estacao
                    .as_ref()
                    .and_then(|e| e.0.estacoes().ok())
                    .and_then(|v| v.into_iter().find(|e| e.alvo == r))
                    .map(|e| e.nome)
                    .unwrap_or_default();
                self.dialogo = Some(Dialogo::RenomearEstacao { estacao: r, nome });
            }
            IdAcao::EditarMovimentacao | IdAcao::RatearMovimentacao => {
                self.erro(format!(
                    "{}: {}",
                    acoes::definicao(id).rotulo,
                    crate::dinheiro::MOTIVO_FATIA_3
                ));
            }
            IdAcao::CopiarRastro => {
                if let Some(r) = self.item_aberto().cloned() {
                    let mut leitor = Leitor::novo(consulta, &mut self.cache);
                    let rotulo = leitor.rotulo(&r);
                    match leitor.rastro(&r) {
                        Ok(rastro) => {
                            ctx.copy_text(pagina::texto_do_rastro(&r, &rotulo, &rastro));
                            self.mensagem = Some(Mensagem {
                                texto: "Rastro copiado.".to_owned(),
                                erro: false,
                            });
                        }
                        Err(e) => self.erro(e),
                    }
                }
            }
            IdAcao::AbrirPdfProtegido
            | IdAcao::AbrirProtegido
            | IdAcao::RegrasDeSenha
            | IdAcao::AtualizarEstante
            | IdAcao::FazerCopiaDeSeguranca
            | IdAcao::AbrirOriginal
            | IdAcao::MostrarNoCofre
                if self.ferramentas.is_some() =>
            {
                let hash = sel.first().map(|r| r.chave.clone()).unwrap_or_default();
                let Some(f) = self.ferramentas.as_mut() else {
                    return;
                };
                let r = match id {
                    IdAcao::AbrirPdfProtegido => {
                        f.abrir_pdf_protegido();
                        Ok(())
                    }
                    IdAcao::AbrirProtegido => {
                        let so: std::collections::BTreeSet<String> =
                            sel.iter().map(|r| r.chave.clone()).collect();
                        f.abrir_pdf_protegido_de(Some(so));
                        Ok(())
                    }
                    IdAcao::RegrasDeSenha => {
                        f.regras_de_senha();
                        Ok(())
                    }
                    IdAcao::AtualizarEstante => {
                        f.atualizar_estante();
                        Ok(())
                    }
                    IdAcao::FazerCopiaDeSeguranca => {
                        f.pedir_copia();
                        Ok(())
                    }
                    IdAcao::AbrirOriginal => f.abrir_original(&hash),
                    _ => f.mostrar_no_cofre(&hash),
                };
                if let Err(e) = r {
                    self.erro(e);
                }
            }
            IdAcao::EnviarAoServidor
            | IdAcao::AbrirPdfProtegido
            | IdAcao::RegrasDeSenha
            | IdAcao::AtualizarEstante
            | IdAcao::FazerCopiaDeSeguranca
            | IdAcao::AbrirOriginal
            | IdAcao::MostrarNoCofre
            | IdAcao::AbrirProtegido => {
                self.erro(format!(
                    "{}: {}",
                    acoes::definicao(id).rotulo,
                    acoes::MOTIVO_NAO_LIGADO
                ));
            }
        }
    }
}

fn rotulo_da_acao(acao: &Acao) -> &'static str {
    let id = match acao {
        Acao::MarcarDocumento => IdAcao::Documento,
        Acao::Descartar => IdAcao::Descartar,
        Acao::EmDuvida => IdAcao::EmDuvida,
        Acao::CorrigirFornecedor(_) => IdAcao::CorrigirFornecedor,
        Acao::CorrigirCategoria(_) => IdAcao::CorrigirCategoria,
        Acao::CorrigirTipo(_) => IdAcao::CorrigirTipo,
        Acao::CorrigirDataEValor { .. } => IdAcao::CorrigirDataEValor,
        Acao::MarcarDuplicidade { .. } => IdAcao::MarcarDuplicidade,
        Acao::UnirConciliados { .. } => IdAcao::UnirConciliados,
        Acao::ConfirmarRecorrencia => IdAcao::ConfirmarRecorrencia,
        Acao::Aprovar => IdAcao::Aprovar,
        Acao::Rejeitar => IdAcao::Rejeitar,
        Acao::Sinalizar { .. } => IdAcao::Sinalizar,
        Acao::Responder { .. } => IdAcao::Responder,
        Acao::GuardarRegraDeSenha { .. } => return "Guardar regra de senha",
    };
    acoes::definicao(id).rotulo
}

/// As estratégias que o menu da Nova regra sugere (o campo aceita qualquer uma válida).
pub const ESTRATEGIAS_SUGERIDAS: &[&str] = &[
    "cpf_primeiros:5",
    "cpf_primeiros:4",
    "cpf_primeiros:3",
    "cpf_ultimos:5",
    "cpf_ultimos:4",
    "cpf_completo",
    "cpf_pontuado",
];

/// Monta o pedido de um diálogo confirmado.
pub fn confirmar_dialogo(d: &Dialogo) -> Result<Pedido, String> {
    match d {
        Dialogo::MudarCategoria {
            movimentacao,
            atual,
            opcoes,
            escolhida,
            ..
        } => {
            let nova = escolhida.and_then(|i| opcoes.get(i));
            let nome = nova.map(|o| o.nome.as_str()).unwrap_or("Sem categoria");
            if atual.as_deref().unwrap_or("Sem categoria") == nome {
                return Err("A categoria já é essa.".to_owned());
            }
            Ok(Pedido::Alterar(
                sbf_inventario::alteracao::PedidoDeAlteracao::MudarCategoria {
                    movimentacao: movimentacao.clone(),
                    categoria: nova.map(|o| o.alvo.clone()),
                },
            ))
        }
        Dialogo::RevogarEstacao { estacao, .. } => Ok(Pedido::RevogarEstacao(estacao.clone())),
        Dialogo::RenomearEstacao { estacao, nome } => {
            let n = nome.trim();
            if n.is_empty() {
                Err("Dê um nome à estação.".to_owned())
            } else {
                Ok(Pedido::RenomearEstacao(estacao.clone(), n.to_owned()))
            }
        }
        Dialogo::Texto {
            acao, alvos, valor, ..
        } => {
            let v = valor.trim().to_owned();
            if v.is_empty() {
                return Err("Preencha o campo.".to_owned());
            }
            let (acao, motivo) = match acao {
                IdAcao::CorrigirFornecedor => (Acao::CorrigirFornecedor(v), None),
                IdAcao::CorrigirCategoria => (Acao::CorrigirCategoria(v), None),
                IdAcao::CorrigirTipo => (Acao::CorrigirTipo(v), None),
                IdAcao::Rejeitar => (Acao::Rejeitar, Some(v)),
                IdAcao::Sinalizar => (Acao::Sinalizar { pergunta: v }, None),
                IdAcao::Responder => (Acao::Responder { resposta: v }, None),
                outra => return Err(format!("{outra:?} não tem diálogo de texto")),
            };
            Ok(Pedido::Decidir(PedidoDeDecisao {
                acao,
                alvos: alvos.clone(),
                motivo,
            }))
        }
        Dialogo::DataEValor {
            alvos, data, valor, ..
        } => {
            let data = if data.trim().is_empty() {
                None
            } else {
                Some(data_iso(data.trim()).ok_or("Data inválida: use dd/mm/aaaa.")?)
            };
            let valor_centavos = if valor.trim().is_empty() {
                None
            } else {
                Some(
                    centavos_de(valor.trim())
                        .ok_or("Valor inválido: use, por exemplo, -312,44.")?,
                )
            };
            if data.is_none() && valor_centavos.is_none() {
                return Err("Preencha a data, o valor ou os dois.".to_owned());
            }
            Ok(Pedido::Decidir(PedidoDeDecisao {
                acao: Acao::CorrigirDataEValor {
                    data,
                    valor_centavos,
                },
                alvos: alvos.clone(),
                motivo: None,
            }))
        }
        Dialogo::Intervalo { de, ate, .. } => {
            let (de, ate) = (de.trim(), ate.trim());
            let valido = |c: &str| {
                c.len() == 7
                    && c.as_bytes()[4] == b'-'
                    && c[..4].parse::<u32>().is_ok()
                    && c[5..].parse::<u32>().is_ok_and(|m| (1..=12).contains(&m))
            };
            if !valido(de) || !valido(ate) {
                return Err("Use ciclos no formato AAAA-MM, por exemplo 2026-05.".to_owned());
            }
            if de > ate {
                return Err("O ciclo inicial vem depois do final.".to_owned());
            }
            Ok(Pedido::Intervalo(de.to_owned(), ate.to_owned()))
        }
        Dialogo::NovaRegra {
            fornecedor,
            tipo_de_documento,
            estrategia,
            prioridade,
            ..
        } => {
            let prioridade: i64 = if prioridade.trim().is_empty() {
                100
            } else {
                prioridade
                    .trim()
                    .parse()
                    .map_err(|_| "A prioridade é um número inteiro (padrão 100).".to_owned())?
            };
            let fornecedor = fornecedor.trim();
            if fornecedor.is_empty() {
                return Err("Diga o fornecedor.".to_owned());
            }
            let tipo = if tipo_de_documento.trim().is_empty() {
                "*".to_owned()
            } else {
                tipo_de_documento.trim().to_owned()
            };
            let estrategia = estrategia.trim();
            estrategia
                .parse::<sbf_pdf::Estrategia>()
                .map_err(|_| format!("Estratégia desconhecida: {estrategia}"))?;
            Ok(Pedido::Decidir(PedidoDeDecisao {
                acao: Acao::GuardarRegraDeSenha {
                    tipo_de_documento: tipo,
                    prioridade: Some(prioridade),
                    estrategia: estrategia.to_owned(),
                },
                alvos: vec![Ref::nova(Tipo::Fornecedor, fornecedor)],
                motivo: Some("regra criada à mão (Ferramentas › Regras de senha)".to_owned()),
            }))
        }
        Dialogo::NovoLote { nome } => {
            let n = nome.trim();
            if n.is_empty() {
                Err("Dê um nome ao lote.".to_owned())
            } else {
                Ok(Pedido::CriarLote(n.to_owned()))
            }
        }
    }
}

/// `28/10/2026` ou `2026-10-28` → `2026-10-28`.
pub fn data_iso(texto: &str) -> Option<String> {
    let (a, m, d) = if let Some((d, resto)) = texto.split_once('/') {
        let (m, a) = resto.split_once('/')?;
        (a, m, d)
    } else {
        let mut it = texto.splitn(3, '-');
        (it.next()?, it.next()?, it.next()?)
    };
    let (a, m, d): (u32, u32, u32) = (a.parse().ok()?, m.parse().ok()?, d.parse().ok()?);
    if !(1900..=2200).contains(&a) || !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    Some(format!("{a:04}-{m:02}-{d:02}"))
}

/// `-1.234,56` → `-123456`; `312` → `31200`.
pub fn centavos_de(texto: &str) -> Option<i64> {
    let t = texto.replace("R$", "").replace(['.', ' '], "");
    let negativo = t.starts_with('-');
    let t = t.trim_start_matches(['-', '+']);
    let (inteiro, frac) = t.split_once(',').unwrap_or((t, "0"));
    let frac = format!("{frac:0<2}");
    if frac.len() > 2 {
        return None;
    }
    let v = inteiro.parse::<i64>().ok()? * 100 + frac.parse::<i64>().ok()?;
    Some(if negativo { -v } else { v })
}

/// O ciclo de uma data `AAAA-MM-DD`: dia 20 em diante é o ciclo do mês; antes, o do mês anterior.
pub fn ciclo_de(data: &str) -> String {
    let mut it = data.splitn(3, '-');
    let (a, m, d) = (
        it.next()
            .and_then(|x| x.parse::<i32>().ok())
            .unwrap_or(2026),
        it.next().and_then(|x| x.parse::<i32>().ok()).unwrap_or(1),
        it.next()
            .and_then(|x| x.get(..2))
            .and_then(|x| x.parse::<i32>().ok())
            .unwrap_or(1),
    );
    let ciclo = format!("{a:04}-{m:02}");
    if d >= 20 {
        ciclo
    } else {
        ciclo_anterior(&ciclo)
    }
}

/// `2026-01` → `2025-12`.
pub fn ciclo_anterior(ciclo: &str) -> String {
    let (a, m) = ciclo.split_once('-').unwrap_or(("2026", "01"));
    let (a, m): (i32, i32) = (a.parse().unwrap_or(2026), m.parse().unwrap_or(1));
    if m == 1 {
        format!("{:04}-12", a - 1)
    } else {
        format!("{a:04}-{:02}", m - 1)
    }
}

/// Hoje em America/Sao_Paulo (UTC−3, sem horário de verão desde 2019).
pub fn hoje_em_sao_paulo() -> String {
    let segundos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
        - 3 * 3600;
    let dias = segundos.div_euclid(86_400);
    // Algoritmo de dias para data civil (Howard Hinnant).
    let z = dias + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// O filtro em uso, em palavras ("Ciclo 2026-10 · decidido por você"), ou o convite.
pub fn descricao_do_filtro(f: &Filtro, ciclo_atual: &str) -> String {
    let mut partes = Vec::new();
    if let Some((de, ate)) = &f.ciclos {
        partes.push(if de == ate {
            if de == ciclo_atual {
                format!("ciclo atual ({de})")
            } else {
                format!("ciclo {de}")
            }
        } else {
            format!("ciclos {de} a {ate}")
        });
    }
    for e in &f.estados {
        partes.push(match e.as_str() {
            "novo" => "a triar".to_owned(),
            "duvida" => "em dúvida".to_owned(),
            outro => outro.to_owned(),
        });
    }
    for t in &f.tipos {
        partes.push(format!("tipo {}", t.replace('_', " ")));
    }
    for c in &f.fontes {
        partes.push(format!("conexão {c}"));
    }
    match &f.quem_decidiu {
        Some(sbf_inventario::consulta::Quem::Usuario) => {
            partes.push("decidido por você".to_owned())
        }
        Some(sbf_inventario::consulta::Quem::Automatico(r)) if r.is_empty() => {
            partes.push("decidido pelas regras".to_owned());
        }
        Some(q) => partes.push(format!("decidido por {}", q.rotulo())),
        None => {}
    }
    if let Some(t) = &f.texto {
        partes.push(format!("\u{201c}{t}\u{201d}"));
    }
    if partes.is_empty() {
        "Sem filtros. Use Início › Período e Filtros, ou pesquise.".to_owned()
    } else {
        format!("Filtro: {}", partes.join(" · "))
    }
}

fn cabecalho_da_area(
    ui: &mut Ui,
    area: Area,
    pesquisa: Option<&mut String>,
    periodo: &mut Periodo,
    focar: &mut bool,
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.vertical(|ui| {
            componentes::titulo(ui, areas::rotulo_da_area(area), 20.0);
            ui.label(
                RichText::new(areas::descricao_da_area(area))
                    .size(12.5)
                    .color(p.tinta_suave),
            );
        });
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if let Some(texto) = pesquisa {
                let r = ui.add(
                    egui::TextEdit::singleline(texto)
                        .hint_text("Pesquisar nesta área (Ctrl+F)")
                        .desired_width(240.0),
                );
                if *focar {
                    r.request_focus();
                    *focar = false;
                }
                ui.label(icones::icone_cor("search", 16.0, p.tinta_suave));
            }
            // Período e filtros ficam no Ribbon (aba Início), como no protótipo.
            let _ = periodo;
        });
    });
    ui.add_space(4.0);
}

fn faixa_do_lote(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    lote_atual: Option<&Ref>,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.label(icones::icone_cor("inventory_2", 16.0, p.tinta));
            ui.label(RichText::new("Lote atual").color(p.tinta_suave));
            match lote_atual {
                Some(l) => {
                    let rotulo = leitor.rotulo(l);
                    componentes::link_item(ui, l, &rotulo, pedidos);
                }
                None => {
                    ui.label(RichText::new("nenhum: crie um (Novo lote) ou abra um da lista").color(p.tinta_suave));
                }
            }
            ui.separator();
            ui.label(
                RichText::new("Selecione documentos ou registros em qualquer área e use Lotes › Adicionar seleção. Enviar ao servidor chega na fatia 3.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
        });
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn filtro_em_palavras() {
        assert!(descricao_do_filtro(&Filtro::default(), "2026-10").starts_with("Sem filtros"));
        let f = Filtro {
            ciclos: Some(("2026-10".into(), "2026-10".into())),
            estados: vec!["duvida".into()],
            quem_decidiu: Some(sbf_inventario::consulta::Quem::Usuario),
            ..Filtro::default()
        };
        assert_eq!(
            descricao_do_filtro(&f, "2026-10"),
            "Filtro: ciclo atual (2026-10) · em dúvida · decidido por você"
        );
    }

    #[test]
    fn ciclo_pelo_dia_de_corte() {
        assert_eq!(ciclo_de("2026-11-13"), "2026-10");
        assert_eq!(ciclo_de("2026-11-20"), "2026-11");
        assert_eq!(ciclo_de("2026-01-05"), "2025-12");
        assert_eq!(ciclo_anterior("2026-10"), "2026-09");
    }

    #[test]
    fn hoje_tem_formato_de_data() {
        let h = hoje_em_sao_paulo();
        assert_eq!(h.len(), 10);
        assert!(data_iso(&h).is_some());
    }

    #[test]
    fn valores_e_datas_dos_dialogos() {
        assert_eq!(centavos_de("-312,44"), Some(-31244));
        assert_eq!(centavos_de("1.234,5"), Some(123450));
        assert_eq!(centavos_de("R$ 300"), Some(30000));
        assert_eq!(centavos_de("abc"), None);
        assert_eq!(data_iso("28/10/2026").as_deref(), Some("2026-10-28"));
        assert_eq!(data_iso("2026-10-28").as_deref(), Some("2026-10-28"));
        assert_eq!(data_iso("31/13/2026"), None);
    }

    #[test]
    fn rejeitar_sem_motivo_nao_passa_e_com_motivo_vira_decisao() {
        let alvo = Ref::nova(Tipo::Registro, "7");
        let mut d = Dialogo::Texto {
            acao: IdAcao::Rejeitar,
            alvos: vec![alvo.clone()],
            rotulo: String::new(),
            valor: "  ".into(),
        };
        assert!(confirmar_dialogo(&d).is_err());
        if let Dialogo::Texto { valor, .. } = &mut d {
            *valor = "valor diferente do boleto".into();
        }
        assert_eq!(
            confirmar_dialogo(&d),
            Ok(Pedido::Decidir(PedidoDeDecisao {
                acao: Acao::Rejeitar,
                alvos: vec![alvo],
                motivo: Some("valor diferente do boleto".into())
            }))
        );
    }
}
