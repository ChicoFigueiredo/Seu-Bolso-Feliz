//! A área **Controle mensal** (spec 11): cabeçalho com período, cartões do resumo, faixa
//! "Em atraso de antes", grade densa de 3 níveis, painel lateral (detalhe, propostas do Claude e
//! arquivados) e os diálogos de edição com a prévia de `Planejamento::simular`.
//!
//! Só lê por `&dyn Planejamento`. Toda escrita vira um [`PedidoDoControle`] que
//! [`crate::Janela::processar`] aplica fora do desenho, com `Autor::Usuario`. A decisão do que
//! mostrar mora em [`crate::controle`], testada sem egui.

use std::collections::HashSet;

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, RichText, ScrollArea, Sense,
    Stroke, Ui,
};
use egui_extras::{Column, TableBuilder};
use sbf_planejamento::{
    Alteracao, Autor, Competencia, Controle, Data, EntradaDoHistorico, ErroDoPlanejamento,
    EstadoDaProposta, Excecao, Filtro, IdOcorrencia, Natureza, Ocorrencia, Planejamento, Prazo,
    Proposta, Recorrente, Simulacao,
};

use crate::componentes;
use crate::controle::{
    self, AcaoDoControle, AlvoDoDialogo, AtualDaVersao, Cartao, ContextoDaLinha, EscopoEscolhido,
    Formulario, Grade, GrupoVm, ModoDoPeriodo, Navegador, PROXIMAS_DE_INICIO, SubgrupoVm,
    Subtotais, Tom, competencia_texto, data_texto, reais,
};
use crate::formato;
use crate::icones;
use crate::navegacao::Pedido;
use crate::tema::{self, Paleta};

/// O planejamento ligado à janela (o serviço real ou o fictício da demonstração).
pub struct PlanejamentoLigado(pub Box<dyn Planejamento>);

impl std::fmt::Debug for PlanejamentoLigado {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PlanejamentoLigado")
    }
}

/// O que a área pede. Quem aplica é [`crate::Janela::processar`].
#[derive(Debug, Clone, PartialEq)]
pub enum PedidoDoControle {
    /// Uma Ação do Ribbon, do menu da linha ou de um atalho.
    Acao(AcaoDoControle),
    /// Aplicar a alteração de um diálogo (já com a prévia vista).
    Aplicar(Box<Alteracao>),
    FecharDialogo,
    ConfirmarProposta(String),
    RecusarProposta(String, String),
    Recarregar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbaLateral {
    Detalhe,
    Propostas,
    Arquivados,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbaDoDetalhe {
    Resumo,
    Proximas,
    Versoes,
    Pausas,
    Excecoes,
    Historico,
}

impl AbaDoDetalhe {
    const TODAS: [AbaDoDetalhe; 6] = [
        AbaDoDetalhe::Resumo,
        AbaDoDetalhe::Proximas,
        AbaDoDetalhe::Versoes,
        AbaDoDetalhe::Pausas,
        AbaDoDetalhe::Excecoes,
        AbaDoDetalhe::Historico,
    ];

    fn rotulo(self) -> &'static str {
        match self {
            AbaDoDetalhe::Resumo => "Resumo",
            AbaDoDetalhe::Proximas => "Próximas ocorrências",
            AbaDoDetalhe::Versoes => "Versões",
            AbaDoDetalhe::Pausas => "Pausas e cancelamento",
            AbaDoDetalhe::Excecoes => "Exceções",
            AbaDoDetalhe::Historico => "Histórico",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct ChaveDoCache {
    periodo: sbf_planejamento::Periodo,
    filtro: Filtro,
    busca: String,
    hoje: Data,
    geracao: u64,
}

struct CacheDoControle {
    chave: ChaveDoCache,
    controle: Result<Controle, String>,
    grade: Grade,
    cartoes: Vec<Cartao>,
}

/// Uma proposta com o texto em português e o efeito simulado.
#[derive(Debug)]
pub struct VmDaProposta {
    pub proposta: Proposta,
    pub texto: String,
    pub simulacao: Result<Simulacao, String>,
}

struct DetalheEmCache {
    recorrente: String,
    geracao: u64,
    meses: u16,
    dados: Result<DadosDoDetalhe, String>,
}

struct DadosDoDetalhe {
    recorrente: Option<Recorrente>,
    proximas: Vec<Ocorrencia>,
    historico: Vec<EntradaDoHistorico>,
    excecoes: Vec<sbf_planejamento::ExcecaoGravada>,
}

/// O que a área guarda entre quadros.
pub struct EstadoDoControle {
    pub navegador: Navegador,
    pub filtro: Filtro,
    pub busca: String,
    pub selecionada: Option<IdOcorrencia>,
    pub lateral: Option<AbaLateral>,
    pub aba_do_detalhe: AbaDoDetalhe,
    pub dialogo: Option<DialogoDoControle>,
    /// O arquivado ou excluído escolhido na aba Arquivados.
    pub restauravel: Option<String>,
    /// Mensagem no topo: o texto e se é erro.
    pub mensagem: Option<(String, bool)>,
    geracao: u64,
    recolhidos: HashSet<String>,
    proximas_meses: u16,
    ir_para_aberto: bool,
    ir_para_ano: i32,
    ir_para_mes: u8,
    ir_para_erro: Option<String>,
    atrasadas_abertas: bool,
    pub resumo_aberto: bool,
    motivos_de_recusa: std::collections::HashMap<String, String>,
    recusando: Option<String>,
    cache: Option<CacheDoControle>,
    detalhe: Option<DetalheEmCache>,
    propostas: Option<(u64, Vec<VmDaProposta>)>,
    arquivados: Option<(u64, Vec<Recorrente>)>,
    selecao: Option<(IdOcorrencia, u64, Option<Recorrente>)>,
}

impl std::fmt::Debug for EstadoDoControle {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstadoDoControle")
            .field("navegador", &self.navegador)
            .field("filtro", &self.filtro)
            .finish_non_exhaustive()
    }
}

impl EstadoDoControle {
    pub fn novo(hoje: Data) -> Self {
        let navegador = Navegador::novo(hoje);
        let a = navegador.ancora();
        Self {
            navegador,
            filtro: Filtro::Todos,
            busca: String::new(),
            selecionada: None,
            lateral: None,
            aba_do_detalhe: AbaDoDetalhe::Resumo,
            dialogo: None,
            restauravel: None,
            mensagem: None,
            geracao: 0,
            recolhidos: HashSet::new(),
            proximas_meses: PROXIMAS_DE_INICIO,
            ir_para_aberto: false,
            ir_para_ano: a.ano,
            ir_para_mes: a.mes,
            ir_para_erro: None,
            atrasadas_abertas: true,
            resumo_aberto: true,
            motivos_de_recusa: std::collections::HashMap::new(),
            recusando: None,
            cache: None,
            detalhe: None,
            propostas: None,
            arquivados: None,
            selecao: None,
        }
    }

    /// Esquece o que leu: a próxima leitura vai ao serviço (depois de escrever).
    pub fn recarregar(&mut self) {
        self.geracao += 1;
    }

    pub fn geracao(&self) -> u64 {
        self.geracao
    }

    /// Abre ou fecha o painel lateral.
    pub fn abrir_lateral(&mut self, aba: AbaLateral) {
        self.lateral = Some(aba);
    }

    /// A linha escolhida, se ainda está no que se vê.
    pub fn linha_selecionada(&self) -> Option<&Ocorrencia> {
        let id = self.selecionada.as_ref()?;
        let c = self.cache.as_ref()?;
        c.grade.linhas().find(|o| &o.id == id).or_else(|| {
            c.controle
                .as_ref()
                .ok()?
                .atrasadas_de_antes
                .iter()
                .find(|o| &o.id == id)
        })
    }

    /// O recorrente da linha escolhida (lido uma vez por escolha).
    pub fn recorrente_selecionado(&self) -> Option<&Recorrente> {
        let (id, g, r) = self.selecao.as_ref()?;
        (Some(id) == self.selecionada.as_ref() && *g == self.geracao)
            .then_some(r.as_ref())
            .flatten()
    }

    fn restauravel_completo(&self) -> Option<&Recorrente> {
        let id = self.restauravel.as_ref()?;
        self.arquivados.as_ref()?.1.iter().find(|r| &r.id == id)
    }

    /// A linha de cima (`para_cima`) ou de baixo do mesmo grupo, de outro compromisso.
    pub fn vizinha_de(&self, id: &IdOcorrencia, para_cima: bool) -> Option<Ocorrencia> {
        let c = self.cache.as_ref()?;
        let (antes, depois) = c.grade.vizinhas(id);
        (if para_cima { antes } else { depois }).cloned()
    }

    /// O contexto que decide se uma Ação vale agora.
    pub fn contexto(&self) -> ContextoDaLinha<'_> {
        let linha = self.linha_selecionada();
        let (primeira, ultima) = match (linha, self.cache.as_ref()) {
            (Some(o), Some(c)) => {
                let (antes, depois) = c.grade.vizinhas(&o.id);
                (antes.is_none(), depois.is_none())
            }
            _ => (true, true),
        };
        ContextoDaLinha {
            linha,
            recorrente: self.recorrente_selecionado(),
            primeira_do_grupo: primeira,
            ultima_do_grupo: ultima,
            restauravel: self.restauravel_completo(),
        }
    }

    /// Escolhe uma linha pelo id do recorrente (a primeira ocorrência dele na grade).
    pub fn selecionar_recorrente(&mut self, recorrente: &str) {
        let achada = self
            .cache
            .as_ref()
            .and_then(|c| c.grade.linhas().find(|o| o.id.recorrente == recorrente))
            .map(|o| o.id.clone());
        if achada.is_some() {
            self.selecionada = achada;
        }
    }

    fn garantir_cache(&mut self, plan: &dyn Planejamento, hoje: Data) {
        self.navegador.atualizar_hoje(hoje);
        let chave = ChaveDoCache {
            periodo: self.navegador.periodo(),
            filtro: self.filtro,
            busca: self.busca.clone(),
            hoje,
            geracao: self.geracao,
        };
        if self.cache.as_ref().is_some_and(|c| c.chave == chave) {
            return;
        }
        let busca = (!self.busca.trim().is_empty()).then_some(self.busca.as_str());
        let controle = plan
            .controle(&chave.periodo, chave.filtro, busca, hoje)
            .map_err(|e| e.to_string());
        let (grade, cartoes) = match &controle {
            Ok(c) => (controle::montar_grade(c), controle::cartoes(c, hoje)),
            Err(_) => (Grade::default(), Vec::new()),
        };
        self.cache = Some(CacheDoControle {
            chave,
            controle,
            grade,
            cartoes,
        });
    }

    fn garantir_selecao(&mut self, plan: &dyn Planejamento) {
        let Some(id) = self.selecionada.clone() else {
            self.selecao = None;
            return;
        };
        if self
            .selecao
            .as_ref()
            .is_some_and(|(i, g, _)| *i == id && *g == self.geracao)
        {
            return;
        }
        let r = plan.recorrente(&id.recorrente).ok().flatten();
        self.selecao = Some((id, self.geracao, r));
    }

    fn garantir_lateral(&mut self, plan: &dyn Planejamento, hoje: Data) {
        match self.lateral {
            Some(AbaLateral::Propostas) => {
                if self
                    .propostas
                    .as_ref()
                    .is_none_or(|(g, _)| *g != self.geracao)
                {
                    self.propostas = Some((self.geracao, carregar_propostas(plan, hoje)));
                }
            }
            Some(AbaLateral::Arquivados) => {
                if self
                    .arquivados
                    .as_ref()
                    .is_none_or(|(g, _)| *g != self.geracao)
                {
                    let v = plan
                        .recorrentes(true)
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|r| r.arquivado || r.excluido.is_some())
                        .collect();
                    self.arquivados = Some((self.geracao, v));
                }
            }
            Some(AbaLateral::Detalhe) => {
                let Some(id) = self.selecionada.as_ref().map(|i| i.recorrente.clone()) else {
                    return;
                };
                let desatualizado = self.detalhe.as_ref().is_none_or(|d| {
                    d.recorrente != id
                        || d.geracao != self.geracao
                        || d.meses != self.proximas_meses
                });
                if desatualizado {
                    let desde = Competencia::de_data(self.navegador.periodo().inicio());
                    self.detalhe = Some(DetalheEmCache {
                        recorrente: id.clone(),
                        geracao: self.geracao,
                        meses: self.proximas_meses,
                        dados: carregar_detalhe(plan, &id, desde, self.proximas_meses, hoje),
                    });
                }
            }
            None => {}
        }
        // As propostas pendentes alimentam o botão do cabeçalho, mesmo com o painel fechado.
        if self
            .propostas
            .as_ref()
            .is_none_or(|(g, _)| *g != self.geracao)
        {
            self.propostas = Some((self.geracao, carregar_propostas(plan, hoje)));
        }
    }

    fn n_propostas(&self) -> usize {
        self.propostas.as_ref().map_or(0, |(_, v)| {
            v.iter()
                .filter(|p| p.proposta.estado == EstadoDaProposta::Pendente)
                .count()
        })
    }
}

fn carregar_propostas(plan: &dyn Planejamento, hoje: Data) -> Vec<VmDaProposta> {
    let nome = |id: &str| {
        plan.recorrente(id)
            .ok()
            .flatten()
            .map_or_else(|| id.to_owned(), |r| r.nome)
    };
    plan.propostas(true)
        .unwrap_or_default()
        .into_iter()
        .map(|p| VmDaProposta {
            texto: controle::descrever(&p.alteracao, &nome),
            simulacao: plan
                .simular(&p.alteracao, hoje)
                .map_err(|e| mensagem_de_erro(&e)),
            proposta: p,
        })
        .collect()
}

fn carregar_detalhe(
    plan: &dyn Planejamento,
    id: &str,
    desde: Competencia,
    meses: u16,
    hoje: Data,
) -> Result<DadosDoDetalhe, String> {
    let recorrente = plan.recorrente(id).map_err(|e| e.to_string())?;
    let proximas = plan
        .proximas(id, desde, meses, hoje)
        .map_err(|e| e.to_string())?;
    let historico = plan.historico(Some(id), 60).map_err(|e| e.to_string())?;
    let excecoes = plan.excecoes_do_recorrente(id).map_err(|e| e.to_string())?;
    Ok(DadosDoDetalhe {
        recorrente,
        proximas,
        historico,
        excecoes,
    })
}

/// O erro como o usuário lê. `VersaoVelha` vira o aviso pedido pela spec.
pub fn mensagem_de_erro(e: &ErroDoPlanejamento) -> String {
    match e {
        ErroDoPlanejamento::VersaoVelha { .. } => "A linha mudou, recarregue.".to_owned(),
        outro => outro.to_string(),
    }
}

// ---------------------------------------------------------------------------------------
// A grade como lista de itens (testável)
// ---------------------------------------------------------------------------------------

/// Uma linha visual da grade.
#[derive(Debug, Clone, PartialEq)]
pub enum ItemDaGrade<'a> {
    Grupo {
        grupo: &'a GrupoVm,
        recolhido: bool,
    },
    Subgrupo {
        grupo: &'a GrupoVm,
        sub: &'a SubgrupoVm,
        recolhido: bool,
    },
    Linha {
        ocorrencia: &'a Ocorrencia,
        /// 1 = direto no grupo; 2 = dentro de um subgrupo.
        nivel: u8,
    },
    Subtotal(&'a GrupoVm),
    Total(&'a Subtotais),
}

pub fn chave_do_grupo(g: &GrupoVm) -> String {
    format!("g:{}", g.nome)
}

pub fn chave_do_subgrupo(g: &GrupoVm, s: &SubgrupoVm) -> String {
    format!("s:{}/{}", g.nome, s.nome.as_deref().unwrap_or_default())
}

/// Achata a grade nos itens que se veem, respeitando o que está recolhido.
pub fn itens_da_grade<'a>(grade: &'a Grade, recolhidos: &HashSet<String>) -> Vec<ItemDaGrade<'a>> {
    let mut v = Vec::new();
    for g in &grade.grupos {
        let recolhido = recolhidos.contains(&chave_do_grupo(g));
        v.push(ItemDaGrade::Grupo {
            grupo: g,
            recolhido,
        });
        if recolhido {
            continue;
        }
        for s in &g.subgrupos {
            match &s.nome {
                None => {
                    for o in &s.linhas {
                        v.push(ItemDaGrade::Linha {
                            ocorrencia: o,
                            nivel: 1,
                        });
                    }
                }
                Some(_) => {
                    let rec = recolhidos.contains(&chave_do_subgrupo(g, s));
                    v.push(ItemDaGrade::Subgrupo {
                        grupo: g,
                        sub: s,
                        recolhido: rec,
                    });
                    if !rec {
                        for o in &s.linhas {
                            v.push(ItemDaGrade::Linha {
                                ocorrencia: o,
                                nivel: 2,
                            });
                        }
                    }
                }
            }
        }
        v.push(ItemDaGrade::Subtotal(g));
    }
    v.push(ItemDaGrade::Total(&grade.total));
    v
}

// ---------------------------------------------------------------------------------------
// Diálogo
// ---------------------------------------------------------------------------------------

/// Um diálogo de edição aberto, com a prévia da última alteração válida.
#[derive(Debug, Clone)]
pub struct DialogoDoControle {
    pub acao: AcaoDoControle,
    pub alvo: AlvoDoDialogo,
    pub competencia: Competencia,
    pub formulario: Formulario,
    pub atual: AtualDaVersao,
    pub grupos: Vec<String>,
    pub erro: Option<String>,
    simulada: Option<(Alteracao, Result<Simulacao, String>)>,
}

impl DialogoDoControle {
    /// A alteração que o formulário monta agora, ou o que está errado.
    pub fn alteracao(&self) -> Result<Alteracao, String> {
        controle::montar_alteracao(&self.alvo, self.competencia, &self.formulario, &self.atual)
    }

    pub fn titulo(&self) -> String {
        let d = controle::definicao(self.acao);
        format!("{} · {}", d.rotulo, self.alvo.nome)
    }

    /// A prévia que vale para a alteração atual (`None` se ainda não simulou ou mudou).
    pub fn previa(&self) -> Option<&Result<Simulacao, String>> {
        let atual = self.alteracao().ok()?;
        self.simulada
            .as_ref()
            .filter(|(a, _)| *a == atual)
            .map(|(_, s)| s)
    }

    /// Simula a alteração atual se ela mudou desde a última prévia.
    pub fn atualizar_previa(&mut self, plan: &dyn Planejamento, hoje: Data) {
        let Ok(alt) = self.alteracao() else {
            return;
        };
        if self.simulada.as_ref().is_some_and(|(a, _)| *a == alt) {
            return;
        }
        let r = plan.simular(&alt, hoje).map_err(|e| mensagem_de_erro(&e));
        self.simulada = Some((alt, r));
    }

    /// Pode aplicar: o formulário é válido e a prévia não falhou.
    pub fn pode_aplicar(&self) -> bool {
        matches!(self.previa(), Some(Ok(_)))
    }
}

/// Monta o diálogo de uma Ação sobre a seleção atual.
pub fn abrir_dialogo(
    acao: AcaoDoControle,
    estado: &EstadoDoControle,
    plan: &dyn Planejamento,
    hoje: Data,
) -> Result<DialogoDoControle, String> {
    let grupos: Vec<String> = plan
        .grupos()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|g| g.nome)
        .collect();
    if acao == AcaoDoControle::Restaurar {
        let r = estado
            .restauravel_completo()
            .ok_or("Escolha um compromisso na aba Arquivados.")?;
        return Ok(DialogoDoControle {
            acao,
            alvo: AlvoDoDialogo {
                recorrente: r.id.clone(),
                nome: r.nome.clone(),
                versao_base: r.versao_do_registro,
                ocorrencia: None,
            },
            competencia: Competencia::de_data(hoje),
            formulario: Formulario::Restaurar {
                excluido: r.excluido.is_some(),
            },
            atual: AtualDaVersao {
                valor: None,
                conta: String::new(),
                centro: String::new(),
                certeza: sbf_planejamento::Certeza::Confirmado,
            },
            grupos,
            erro: None,
            simulada: None,
        });
    }
    let o = estado
        .linha_selecionada()
        .ok_or("Escolha uma linha do controle.")?
        .clone();
    let rec = plan
        .recorrente(&o.id.recorrente)
        .map_err(|e| e.to_string())?
        .ok_or("O compromisso não existe mais. Recarregue.")?;
    let comp = o.id.competencia;
    let texto_data = |d: Data| data_texto(d);
    let formulario = match acao {
        AcaoDoControle::Editar => Formulario::Editar {
            escopo: EscopoEscolhido::SoEsteMes,
            valor: o
                .previsto
                .map(|v| crate::caixa_ui::centavos_em_texto(v.0))
                .unwrap_or_default(),
            conta: o.conta.clone().unwrap_or_default(),
            centro: o.centro.clone().unwrap_or_default(),
            estimado: o.certeza == sbf_planejamento::Certeza::Estimado,
        },
        AcaoDoControle::Pausar => Formulario::Pausar {
            desde: competencia_texto(comp),
            ate: String::new(),
            motivo: String::new(),
        },
        AcaoDoControle::Retomar => Formulario::Retomar {
            desde: competencia_texto(comp),
        },
        AcaoDoControle::Cancelar => Formulario::Cancelar {
            desde: competencia_texto(comp),
            motivo: String::new(),
            comprovado: false,
        },
        AcaoDoControle::Adiar => Formulario::Adiar {
            nova_data: texto_data(o.vencimento.somar_dias(7)),
        },
        AcaoDoControle::Dispensar => Formulario::Dispensar {
            motivo: String::new(),
        },
        AcaoDoControle::InformarPago => Formulario::InformarPago {
            valor: o
                .previsto
                .map(|v| crate::caixa_ui::centavos_em_texto(v.0))
                .unwrap_or_default(),
            data: texto_data(hoje),
        },
        AcaoDoControle::Excluir => Formulario::Excluir {
            motivo: String::new(),
            confirmou: false,
        },
        AcaoDoControle::Arquivar => Formulario::Arquivar,
        AcaoDoControle::Mover => Formulario::Mover {
            grupo: rec.grupo.clone(),
        },
        outra => {
            return Err(format!(
                "{} não abre diálogo.",
                controle::definicao(outra).rotulo
            ));
        }
    };
    let mut atual = AtualDaVersao::de(&o);
    atual.certeza = o.certeza;
    Ok(DialogoDoControle {
        acao,
        alvo: AlvoDoDialogo {
            recorrente: rec.id.clone(),
            nome: rec.nome.clone(),
            versao_base: rec.versao_do_registro,
            ocorrencia: Some(o.id.clone()),
        },
        competencia: comp,
        formulario,
        atual,
        grupos,
        erro: None,
        simulada: None,
    })
}

// ---------------------------------------------------------------------------------------
// Cores e peças
// ---------------------------------------------------------------------------------------

fn cores_do_tom(p: &Paleta, tom: Tom) -> (Color32, Color32) {
    match tom {
        Tom::Neutro => (p.superficie_3, p.tinta_suave),
        Tom::Ok => (p.sucesso_sutil, p.sucesso),
        Tom::Aviso => (p.aviso_sutil, p.aviso),
        Tom::Perigo => (p.perigo_sutil, p.perigo),
        Tom::Info => (p.info_sutil, p.info),
    }
}

fn selo_do_estado(ui: &mut Ui, e: sbf_planejamento::EstadoDaOcorrencia) {
    let p = tema::paleta(ui.ctx());
    let s = controle::estilo_do_estado(e);
    let (fundo, cor) = cores_do_tom(&p, s.tom);
    let r = Frame::new()
        .fill(fundo)
        .corner_radius(CornerRadius::same(8))
        .inner_margin(Margin::symmetric(6, 1))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.x = 3.0;
            ui.label(icones::icone_cor(s.icone, 13.0, cor));
            ui.add(
                egui::Label::new(RichText::new(s.rotulo).size(12.0).color(cor).strong()).extend(),
            );
        })
        .response;
    r.on_hover_text(s.rotulo);
}

fn texto_pequeno(ui: &mut Ui, t: impl Into<String>) {
    let p = tema::paleta(ui.ctx());
    ui.label(RichText::new(t.into()).size(11.5).color(p.tinta_suave));
}

// ---------------------------------------------------------------------------------------
// A área
// ---------------------------------------------------------------------------------------

/// Desenha o Controle mensal inteiro.
pub fn area(
    ui: &mut Ui,
    plan: &dyn Planejamento,
    estado: &mut EstadoDoControle,
    hoje: Data,
    pedidos: &mut Vec<Pedido>,
) {
    estado.garantir_cache(plan, hoje);
    estado.garantir_selecao(plan);
    estado.garantir_lateral(plan, hoje);
    let p = tema::paleta(ui.ctx());

    if estado.lateral.is_some() {
        egui::Panel::right("controle-lateral")
            .resizable(true)
            .default_size(400.0)
            .min_size(320.0)
            .frame(
                Frame::new()
                    .fill(p.superficie_1)
                    .inner_margin(Margin::same(10)),
            )
            .show(ui, |ui| lateral(ui, plan, estado, hoje, pedidos));
    }

    teclado(ui, estado);
    let altura_total = ui.available_height();
    let y0 = ui.cursor().top();
    ScrollArea::vertical()
        .id_salt("controle-pagina")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            cabecalho(ui, estado);
            mensagem(ui, estado);
            ui.add_space(6.0);
            let cache = estado.cache.take();
            if let Some(c) = &cache {
                match &c.controle {
                    Err(e) => {
                        componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
                    }
                    Ok(controle) => {
                        if estado.resumo_aberto {
                            cartoes(ui, &c.cartoes);
                            ui.add_space(6.0);
                        }
                        faixa_de_atrasadas(ui, estado, controle, pedidos);
                        // A grade ocupa o que sobra da janela (e nunca menos que o mínimo).
                        let usado = ui.cursor().top() - y0;
                        let altura = (altura_total - usado - 12.0).max(280.0);
                        grade(ui, estado, c, altura, pedidos);
                    }
                }
            }
            estado.cache = cache;
        });
}

/// Setas movem a escolha; Esc limpa. Só quando ninguém está digitando.
fn teclado(ui: &mut Ui, estado: &mut EstadoDoControle) {
    if ui.ctx().egui_wants_keyboard_input() || estado.dialogo.is_some() {
        return;
    }
    let (baixo, cima) = ui.input(|i| {
        (
            i.key_pressed(egui::Key::ArrowDown) && !i.modifiers.alt,
            i.key_pressed(egui::Key::ArrowUp) && !i.modifiers.alt,
        )
    });
    if !(baixo || cima) {
        return;
    }
    let Some(c) = estado.cache.as_ref() else {
        return;
    };
    let ids: Vec<IdOcorrencia> = c.grade.linhas().map(|o| o.id.clone()).collect();
    if ids.is_empty() {
        return;
    }
    let atual = estado
        .selecionada
        .as_ref()
        .and_then(|s| ids.iter().position(|i| i == s));
    let novo = match (atual, baixo) {
        (None, _) => 0,
        (Some(i), true) => (i + 1).min(ids.len() - 1),
        (Some(i), false) => i.saturating_sub(1),
    };
    estado.selecionada = Some(ids[novo].clone());
}

fn cabecalho(ui: &mut Ui, estado: &mut EstadoDoControle) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        componentes::titulo(ui, "Controle mensal", 20.0);
        if estado.navegador.e_projecao() {
            componentes::selo(ui, "projeção", p.info_sutil, p.info).on_hover_text(
                "Este período ainda não começou: o que se vê nasce das regras, não de fatos.",
            );
        }
        ui.separator();
        let modo = estado.navegador.modo;
        for m in [ModoDoPeriodo::Ciclo, ModoDoPeriodo::MesCivil] {
            if ui.selectable_label(modo == m, m.rotulo()).clicked() {
                estado.navegador.trocar_modo(m);
            }
        }
        ui.separator();
        let anterior = estado.navegador.pode_anterior();
        if componentes::botao_icone(ui, "chevron_left", "Período anterior", anterior).clicked() {
            estado.navegador.anterior();
        }
        ui.label(RichText::new(estado.navegador.rotulo()).strong());
        let proximo = estado.navegador.pode_proximo();
        if componentes::botao_icone(ui, "chevron_right", "Próximo período", proximo).clicked() {
            estado.navegador.proximo();
        }
        if ui
            .add_enabled(!estado.navegador.contem_hoje(), egui::Button::new("Hoje"))
            .on_hover_text("Volta ao período de hoje")
            .clicked()
        {
            estado.navegador.ir_para_hoje();
        }
        if ui
            .selectable_label(estado.ir_para_aberto, "Ir para…")
            .on_hover_text("Qualquer mês dos próximos 360")
            .clicked()
        {
            estado.ir_para_aberto = !estado.ir_para_aberto;
            let a = estado.navegador.ancora();
            estado.ir_para_ano = a.ano;
            estado.ir_para_mes = a.mes;
            estado.ir_para_erro = None;
        }
    });
    if estado.ir_para_aberto {
        ui.horizontal_wrapped(|ui| {
            let rotulo = match estado.navegador.modo {
                ModoDoPeriodo::Ciclo => "Mês em que o ciclo começa",
                ModoDoPeriodo::MesCivil => "Mês",
            };
            ui.label(rotulo);
            egui::ComboBox::from_id_salt("controle-ir-mes")
                .selected_text(format!("{:02}", estado.ir_para_mes))
                .width(60.0)
                .show_ui(ui, |ui| {
                    for m in 1..=12u8 {
                        ui.selectable_value(&mut estado.ir_para_mes, m, format!("{m:02}"));
                    }
                });
            ui.add(egui::DragValue::new(&mut estado.ir_para_ano).range(1990..=2100));
            if ui.button("Ir").clicked() {
                match Competencia::nova(estado.ir_para_ano, estado.ir_para_mes) {
                    Some(c) => match estado.navegador.ir_para(c) {
                        Ok(()) => {
                            estado.ir_para_aberto = false;
                            estado.ir_para_erro = None;
                        }
                        Err(e) => estado.ir_para_erro = Some(e),
                    },
                    None => estado.ir_para_erro = Some("Mês inválido.".into()),
                }
            }
            if let Some(e) = &estado.ir_para_erro {
                ui.colored_label(tema::paleta(ui.ctx()).perigo, e);
            }
        });
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut estado.busca)
                .hint_text("Buscar por nome, conta ou centro")
                .desired_width(240.0),
        );
        let rotulo = |f: Filtro| match f {
            Filtro::Todos => "Todos",
            Filtro::APagar => "A pagar",
            Filtro::EmAtraso => "Em atraso",
            Filtro::Pagos => "Pagos",
            Filtro::SemComprovante => "Sem comprovante",
            Filtro::SemValor => "Sem valor",
            Filtro::PausadosECancelados => "Pausados e cancelados",
        };
        egui::ComboBox::from_id_salt("controle-filtro")
            .selected_text(format!("Filtro: {}", rotulo(estado.filtro)))
            .show_ui(ui, |ui| {
                for f in [
                    Filtro::Todos,
                    Filtro::APagar,
                    Filtro::EmAtraso,
                    Filtro::Pagos,
                    Filtro::SemComprovante,
                    Filtro::SemValor,
                    Filtro::PausadosECancelados,
                ] {
                    ui.selectable_value(&mut estado.filtro, f, rotulo(f));
                }
            });
        ui.separator();
        let n = estado.n_propostas();
        let rot = if n > 0 {
            format!("Propostas do Claude ({n})")
        } else {
            "Propostas do Claude".to_owned()
        };
        if ui
            .selectable_label(estado.lateral == Some(AbaLateral::Propostas), rot)
            .clicked()
        {
            estado.lateral = toggle(estado.lateral, AbaLateral::Propostas);
        }
        if ui
            .selectable_label(
                estado.lateral == Some(AbaLateral::Detalhe),
                "Detalhe da linha",
            )
            .clicked()
        {
            estado.lateral = toggle(estado.lateral, AbaLateral::Detalhe);
        }
        if ui
            .selectable_label(estado.lateral == Some(AbaLateral::Arquivados), "Arquivados")
            .clicked()
        {
            estado.lateral = toggle(estado.lateral, AbaLateral::Arquivados);
        }
        ui.separator();
        ui.checkbox(&mut estado.resumo_aberto, "Cartões do resumo");
    });
}

fn toggle(atual: Option<AbaLateral>, alvo: AbaLateral) -> Option<AbaLateral> {
    if atual == Some(alvo) {
        None
    } else {
        Some(alvo)
    }
}

fn mensagem(ui: &mut Ui, estado: &mut EstadoDoControle) {
    let Some((texto, erro)) = estado.mensagem.clone() else {
        return;
    };
    let p = tema::paleta(ui.ctx());
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        if erro {
            componentes::aviso(ui, "error", &texto, p.perigo_sutil, p.borda, p.perigo);
        } else {
            componentes::aviso(
                ui,
                "check_circle",
                &texto,
                p.sucesso_sutil,
                p.borda,
                p.sucesso,
            );
        }
        if erro && texto.contains("recarregue") && ui.button("Recarregar").clicked() {
            estado.recarregar();
            estado.mensagem = None;
        }
        if ui.small_button("Fechar").clicked() {
            estado.mensagem = None;
        }
    });
}

/// Quantos cartões vão por linha: todos, se cabem com folga; senão quatro (ou menos).
pub fn cartoes_por_linha(largura: f32) -> usize {
    (((largura + 8.0) / (LARGURA_MINIMA + 8.0)) as usize).clamp(1, 7)
}

const LARGURA_MINIMA: f32 = 128.0;

fn cartoes(ui: &mut Ui, v: &[Cartao]) {
    let p = tema::paleta(ui.ctx());
    let por_linha = cartoes_por_linha(ui.available_width());
    let largura = ((ui.available_width() - 8.0 * (por_linha as f32 - 1.0)) / por_linha as f32)
        .clamp(LARGURA_MINIMA, 300.0);
    egui::Grid::new("controle-cartoes")
        .spacing([8.0, 8.0])
        .min_col_width(largura)
        .show(ui, |ui| {
            for (i, k) in v.iter().enumerate() {
                let (fundo, cor) = cores_do_tom(&p, k.tom);
                let quadro = Frame::new()
                    .fill(p.superficie_1)
                    .stroke(Stroke::new(
                        1.0,
                        if k.tom == Tom::Neutro { p.borda } else { cor },
                    ))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(9))
                    .show(ui, |ui| {
                        ui.set_width(largura - 20.0);
                        ui.set_min_height(70.0);
                        ui.vertical(|ui| {
                            ui.label(
                                RichText::new(k.titulo)
                                    .size(12.0)
                                    .color(p.tinta_suave)
                                    .strong(),
                            );
                            if let Some(s) = k.selo {
                                ui.horizontal(|ui| {
                                    componentes::selo(ui, s, fundo, cor);
                                });
                            }
                            ui.label(
                                RichText::new(&k.valor)
                                    .font(FontId::new(18.0, crate::tema::familia_forte()))
                                    .color(p.tinta),
                            );
                            for n in &k.notas {
                                ui.add(egui::Label::new(
                                    RichText::new(n).size(11.0).color(p.tinta_suave),
                                ));
                            }
                        });
                    })
                    .response;
                if let Some(d) = &k.dica {
                    quadro.on_hover_text(d);
                }
                if (i + 1) % por_linha == 0 {
                    ui.end_row();
                }
            }
        });
}

fn faixa_de_atrasadas(
    ui: &mut Ui,
    estado: &mut EstadoDoControle,
    c: &Controle,
    _pedidos: &mut Vec<Pedido>,
) {
    if c.atrasadas_de_antes.is_empty() {
        return;
    }
    let p = tema::paleta(ui.ctx());
    let total: sbf_planejamento::Centavos = c
        .atrasadas_de_antes
        .iter()
        .filter_map(controle::falta)
        .sum();
    Frame::new()
        .fill(p.perigo_sutil)
        .stroke(Stroke::new(1.0, p.perigo))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(icones::icone_cor("error", 16.0, p.perigo));
                ui.label(
                    RichText::new(format!(
                        "Em atraso de antes · {} · {}",
                        c.atrasadas_de_antes.len(),
                        reais(total)
                    ))
                    .strong()
                    .color(p.perigo),
                );
                let rot = if estado.atrasadas_abertas {
                    "Recolher"
                } else {
                    "Mostrar"
                };
                if ui.small_button(rot).clicked() {
                    estado.atrasadas_abertas = !estado.atrasadas_abertas;
                }
                texto_pequeno(ui, "Continuam devidas; vêm de períodos anteriores.");
            });
            if estado.atrasadas_abertas {
                for o in c.atrasadas_de_antes.iter().take(3) {
                    let sel = estado.selecionada.as_ref() == Some(&o.id);
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        let t = format!(
                            "{}   venceu em {}   falta {}",
                            o.nome,
                            data_texto(o.vencimento),
                            controle::falta(o).map_or("sem valor".to_owned(), reais),
                        );
                        if ui.selectable_label(sel, t).clicked() {
                            estado.selecionada = Some(o.id.clone());
                        }
                    });
                }
                if c.atrasadas_de_antes.len() > 3 {
                    ui.horizontal(|ui| {
                        ui.add_space(22.0);
                        texto_pequeno(
                            ui,
                            format!(
                                "… e mais {}. Use o filtro Em atraso para ver todas.",
                                c.atrasadas_de_antes.len() - 3
                            ),
                        );
                    });
                }
            }
        });
    ui.add_space(8.0);
}

// ---------------------------------------------------------------------------------------
// Grade
// ---------------------------------------------------------------------------------------

const COLUNAS: [&str; 10] = [
    "Origem",
    "Nome",
    "Vencimento",
    "Previsto",
    "Pago informado",
    "Conciliado",
    "Falta",
    "Estado",
    "Conta",
    "Centro",
];

fn altura_do_item(i: &ItemDaGrade<'_>) -> f32 {
    match i {
        ItemDaGrade::Grupo { .. } => 28.0,
        ItemDaGrade::Subgrupo { .. } => 24.0,
        ItemDaGrade::Linha { .. } => 32.0,
        ItemDaGrade::Subtotal(g) => 14.0 * g.total.partes_do_previsto().len().max(1) as f32 + 10.0,
        ItemDaGrade::Total(t) => 14.0 * t.partes_do_previsto().len().max(1) as f32 + 12.0,
    }
}

fn numero(ui: &mut Ui, t: impl Into<String>, cor: Color32) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        ui.label(RichText::new(t.into()).color(cor));
    });
}

fn dinheiro_ou_traco(ui: &mut Ui, v: Option<sbf_planejamento::Centavos>) {
    let p = tema::paleta(ui.ctx());
    match v {
        Some(v) => numero(ui, reais(v), p.tinta),
        None => numero(ui, "—", p.tinta_desabilitada),
    }
}

fn subtotais_em_colunas(
    row: &mut egui_extras::TableRow<'_, '_>,
    t: &Subtotais,
    titulo: &str,
    forte: bool,
    p: &Paleta,
) {
    let p = *p;
    row.col(|_| {});
    row.col(|ui| {
        let r = RichText::new(titulo).color(p.tinta);
        ui.label(if forte { r.strong() } else { r.size(12.5) });
    });
    row.col(|_| {});
    row.col(|ui| {
        ui.with_layout(Layout::top_down(Align::Max), |ui| {
            let partes = t.partes_do_previsto();
            if partes.is_empty() {
                ui.label(RichText::new("—").color(p.tinta_desabilitada));
            }
            for (nome, v) in partes {
                ui.label(RichText::new(format!("{nome} {}", reais(v))).size(12.0));
            }
            if t.sem_valor > 0 {
                ui.label(
                    RichText::new(format!("{} sem valor", t.sem_valor))
                        .size(11.0)
                        .color(p.aviso),
                );
            }
        });
    });
    row.col(|ui| {
        numero(ui, reais(t.pago_informado), p.tinta);
    });
    row.col(|ui| {
        numero(ui, reais(t.conciliado), p.tinta);
    });
    row.col(|ui| {
        numero(ui, reais(t.falta), p.tinta);
    });
    for _ in 0..3 {
        row.col(|_| {});
    }
}

fn grade(
    ui: &mut Ui,
    estado: &mut EstadoDoControle,
    c: &CacheDoControle,
    altura: f32,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let projecao = estado.navegador.e_projecao();
    if c.grade.grupos.is_empty() {
        ui.add_space(16.0);
        ui.vertical_centered(|ui| {
            ui.label(icones::icone_cor("calendar_month", 36.0, p.tinta_suave));
            ui.label(
                RichText::new("Nenhuma linha neste período com esse filtro.").color(p.tinta_suave),
            );
        });
        return;
    }
    let itens = itens_da_grade(&c.grade, &estado.recolhidos);
    let alturas: Vec<f32> = itens.iter().map(altura_do_item).collect();
    let mut alternar: Option<String> = None;
    let mut escolhida: Option<IdOcorrencia> = None;
    let mut pedir: Vec<AcaoDoControle> = Vec::new();
    let contexto_base = estado.selecionada.clone();
    let selecionada_ctx: Vec<(AcaoDoControle, Result<(), String>)> = {
        let ctx = estado.contexto();
        crate::controle::ACOES_DO_CONTROLE
            .iter()
            .map(|d| (d.id, controle::disponibilidade(d.id, &ctx)))
            .collect()
    };
    let _ = &contexto_base;
    let larguras = [82.0, 210.0, 98.0, 132.0, 98.0, 98.0, 90.0, 122.0, 80.0];
    let ancho = ui.available_width();
    ui.allocate_ui_with_layout(
        egui::vec2(ancho, altura + 30.0),
        Layout::top_down(Align::Min),
        |ui| {
            ui.set_clip_rect(ui.clip_rect().intersect(ui.max_rect()));
            let mut construtor = TableBuilder::new(ui)
                .id_salt("controle-grade")
                .striped(false)
                .resizable(true)
                .sense(Sense::click())
                .cell_layout(Layout::left_to_right(Align::Center))
                .auto_shrink([false, false])
                .max_scroll_height(altura);
            for (i, l) in larguras.iter().enumerate() {
                let col = if matches!(i, 3..=6) {
                    Column::initial(*l).at_least(60.0).clip(true)
                } else {
                    Column::initial(*l).at_least(48.0).clip(true)
                };
                construtor = construtor.column(col);
            }
            construtor = construtor.column(Column::remainder().at_least(60.0).clip(true));
            construtor
                .header(26.0, |mut cab| {
                    for (i, rotulo) in COLUNAS.iter().enumerate() {
                        cab.col(|ui| {
                            let r = RichText::new(*rotulo)
                                .strong()
                                .size(12.5)
                                .color(p.tinta_suave);
                            if matches!(i, 3..=6) {
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.label(r);
                                });
                            } else {
                                ui.label(r);
                            }
                        });
                    }
                })
                .body(|corpo| {
                    corpo.heterogeneous_rows(alturas.iter().copied(), |mut row| {
                        let item = &itens[row.index()];
                        match item {
                            ItemDaGrade::Grupo { grupo, recolhido } => {
                                row.col(|ui| {
                                    let ic = if *recolhido {
                                        "chevron_right"
                                    } else {
                                        "expand_more"
                                    };
                                    ui.label(icones::icone_cor(ic, 18.0, p.tinta_suave));
                                });
                                row.col(|ui| {
                                    ui.label(RichText::new(&grupo.nome).strong().size(14.0));
                                    ui.label(
                                        RichText::new(format!("{} linhas", grupo.total.linhas))
                                            .size(11.5)
                                            .color(p.tinta_suave),
                                    );
                                });
                                for _ in 2..COLUNAS.len() {
                                    row.col(|_| {});
                                }
                                let r = row.response();
                                if r.clicked() {
                                    alternar = Some(chave_do_grupo(grupo));
                                }
                            }
                            ItemDaGrade::Subgrupo {
                                grupo,
                                sub,
                                recolhido,
                            } => {
                                row.col(|ui| {
                                    ui.add_space(10.0);
                                    let ic = if *recolhido {
                                        "chevron_right"
                                    } else {
                                        "expand_more"
                                    };
                                    ui.label(icones::icone_cor(ic, 16.0, p.tinta_suave));
                                });
                                row.col(|ui| {
                                    ui.label(
                                        RichText::new(sub.nome.as_deref().unwrap_or_default())
                                            .strong()
                                            .color(p.tinta_suave),
                                    );
                                    texto_pequeno(ui, format!("{} linhas", sub.total.linhas));
                                });
                                for _ in 2..COLUNAS.len() {
                                    row.col(|_| {});
                                }
                                if row.response().clicked() {
                                    alternar = Some(chave_do_subgrupo(grupo, sub));
                                }
                            }
                            ItemDaGrade::Linha {
                                ocorrencia: o,
                                nivel,
                            } => {
                                let sel = estado.selecionada.as_ref() == Some(&o.id);
                                row.set_selected(sel);
                                linha(&mut row, o, *nivel, projecao, &p);
                                let r = row.response();
                                if r.clicked() || r.secondary_clicked() {
                                    escolhida = Some(o.id.clone());
                                }
                                r.context_menu(|ui| {
                                    menu_da_linha(ui, &selecionada_ctx, &mut pedir);
                                });
                            }
                            ItemDaGrade::Subtotal(g) => {
                                subtotais_em_colunas(
                                    &mut row,
                                    &g.total,
                                    &format!("Subtotal · {}", g.nome),
                                    false,
                                    &p,
                                );
                            }
                            ItemDaGrade::Total(t) => {
                                subtotais_em_colunas(&mut row, t, "Total do período", true, &p);
                            }
                        }
                    });
                });
        },
    );
    if let Some(k) = alternar
        && !estado.recolhidos.remove(&k)
    {
        estado.recolhidos.insert(k);
    }
    if let Some(id) = escolhida {
        estado.selecionada = Some(id);
    }
    for a in pedir {
        pedidos.push(Pedido::Controle(PedidoDoControle::Acao(a)));
    }
}

fn menu_da_linha(
    ui: &mut Ui,
    disp: &[(AcaoDoControle, Result<(), String>)],
    pedir: &mut Vec<AcaoDoControle>,
) {
    for d in &controle::ACOES_DO_CONTROLE {
        if matches!(d.id, AcaoDoControle::Restaurar | AcaoDoControle::Propostas) {
            continue;
        }
        let r = disp
            .iter()
            .find(|(a, _)| *a == d.id)
            .map(|(_, r)| r.clone())
            .unwrap_or(Ok(()));
        let texto = format!("{}    {}", d.rotulo, atalho_em_texto(d));
        let resp = ui.add_enabled(r.is_ok(), egui::Button::new(texto));
        let resp = match &r {
            Err(m) => resp.on_disabled_hover_text(m.as_str()),
            Ok(()) => resp.on_hover_text(d.dica),
        };
        if resp.clicked() {
            pedir.push(d.id);
            ui.close();
        }
    }
}

pub fn atalho_em_texto(d: &controle::DefinicaoDoControle) -> String {
    let a = d.atalho;
    let mut s = String::new();
    if a.ctrl {
        s.push_str("Ctrl+");
    }
    if a.alt {
        s.push_str("Alt+");
    }
    if a.shift {
        s.push_str("Shift+");
    }
    s.push_str(&format!("{:?}", a.tecla).replace("Arrow", ""));
    s
}

fn linha(
    row: &mut egui_extras::TableRow<'_, '_>,
    o: &Ocorrencia,
    nivel: u8,
    projecao: bool,
    p: &Paleta,
) {
    let p = *p;
    let suprimida = controle::suprimida(o.estado)
        || o.estado == sbf_planejamento::EstadoDaOcorrencia::Dispensada;
    let cor_do_texto = if suprimida {
        p.tinta_desabilitada
    } else {
        p.tinta
    };
    row.col(|ui| {
        let (rotulo, icone) = controle::origem(o, projecao);
        let r = ui
            .horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                ui.label(icones::icone_cor(icone, 14.0, p.tinta_suave));
                ui.label(RichText::new(rotulo).size(12.0).color(p.tinta_suave));
            })
            .response;
        r.on_hover_text(controle::origem_dica(o, projecao));
    });
    row.col(|ui| {
        ui.add_space(f32::from(nivel) * 12.0);
        let mut nome = o.nome.clone();
        if let Some((n, t)) = o.parcela {
            nome.push_str(&format!(" ({n}/{t})"));
        }
        ui.add(egui::Label::new(RichText::new(nome).color(cor_do_texto)).truncate());
        if matches!(o.natureza, Natureza::DentroDaFatura) {
            componentes::selo(ui, "no cartão", p.info_sutil, p.info)
                .on_hover_text("Compra no cartão: o caixa sai pela fatura, que já está no plano. Não soma nas saídas.");
        }
        if o.sugestao_de_conciliacao {
            componentes::selo(ui, "conciliar?", p.aviso_sutil, p.aviso).on_hover_text(
                "Há um lançamento do banco que parece pagar esta linha. Confirme a conciliação.",
            );
        }
        if o.prazo_desconhecido {
            ui.label(icones::icone_cor("help", 14.0, p.aviso))
                .on_hover_text("Prazo desconhecido: projetado até o horizonte, como estimativa.");
        }
    });
    row.col(|ui| {
        let v = controle::vencimento(o);
        let r = ui
            .vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(&v.principal).color(cor_do_texto));
                if let Some(s) = &v.secundario {
                    ui.label(RichText::new(s).size(11.0).color(p.tinta_suave));
                }
            })
            .response;
        if let Some(d) = v.dica {
            r.on_hover_text(d);
        }
    });
    row.col(|ui| match o.previsto {
        None => {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                componentes::selo(ui, "sem valor", p.aviso_sutil, p.aviso)
                    .on_hover_text("O valor ainda não é conhecido. Não conta como zero.");
            });
        }
        Some(v) => {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let mut t = RichText::new(reais(v)).color(cor_do_texto);
                if projecao || o.certeza == sbf_planejamento::Certeza::Estimado {
                    t = t.italics();
                }
                let r = ui.label(t);
                if o.certeza == sbf_planejamento::Certeza::Estimado {
                    ui.label(RichText::new("est.").size(10.5).color(p.tinta_suave));
                    r.on_hover_text("Valor estimado, não confirmado.");
                }
            });
        }
    });
    row.col(|ui| dinheiro_ou_traco(ui, o.pago_informado));
    row.col(|ui| dinheiro_ou_traco(ui, o.conciliado));
    row.col(|ui| {
        let f = controle::falta(o);
        match (f, o.previsto) {
            (Some(f), _) if f.0 > 0 => numero(ui, reais(f), p.tinta),
            (Some(_), _) => numero(ui, reais(sbf_planejamento::Centavos::ZERO), p.tinta_suave),
            _ => numero(ui, "—", p.tinta_desabilitada),
        }
    });
    row.col(|ui| selo_do_estado(ui, o.estado));
    row.col(|ui| {
        ui.add(
            egui::Label::new(RichText::new(o.conta.as_deref().unwrap_or("—")).size(12.0))
                .truncate(),
        );
    });
    row.col(|ui| {
        ui.add(
            egui::Label::new(RichText::new(o.centro.as_deref().unwrap_or("—")).size(12.0))
                .truncate(),
        );
    });
}

// ---------------------------------------------------------------------------------------
// Painel lateral
// ---------------------------------------------------------------------------------------

fn lateral(
    ui: &mut Ui,
    plan: &dyn Planejamento,
    estado: &mut EstadoDoControle,
    hoje: Data,
    pedidos: &mut Vec<Pedido>,
) {
    let _ = plan;
    ui.horizontal(|ui| {
        for (a, rot) in [
            (AbaLateral::Detalhe, "Detalhe"),
            (AbaLateral::Propostas, "Propostas"),
            (AbaLateral::Arquivados, "Arquivados"),
        ] {
            if ui
                .selectable_label(estado.lateral == Some(a), rot)
                .clicked()
            {
                estado.lateral = Some(a);
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if componentes::botao_icone(ui, "close", "Fechar o painel", true).clicked() {
                estado.lateral = None;
            }
        });
    });
    ui.separator();
    match estado.lateral {
        Some(AbaLateral::Propostas) => propostas_ui(ui, estado, pedidos),
        Some(AbaLateral::Arquivados) => arquivados_ui(ui, estado, pedidos),
        Some(AbaLateral::Detalhe) => detalhe_ui(ui, estado, hoje),
        None => {}
    }
}

fn autor_texto(a: &Autor) -> &'static str {
    match a {
        Autor::Usuario => "você",
        Autor::Importacao => "importação",
        Autor::Ia { .. } => "IA",
        Autor::Mcp => "Claude (MCP)",
    }
}

fn propostas_ui(ui: &mut Ui, estado: &mut EstadoDoControle, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    componentes::titulo(ui, "Propostas do Claude", 16.0);
    texto_pequeno(
        ui,
        "O Claude só propõe. Nada muda até você confirmar aqui; recusar pede um motivo.",
    );
    ui.add_space(6.0);
    let Some((_, lista)) = estado.propostas.take() else {
        return;
    };
    if lista.is_empty() {
        ui.add_space(12.0);
        ui.label(RichText::new("Nenhuma proposta pendente.").color(p.tinta_suave));
    }
    ScrollArea::vertical()
        .id_salt("controle-propostas")
        .show(ui, |ui| {
            for v in &lista {
                let id = v.proposta.id.clone();
                Frame::new()
                    .fill(p.superficie_0)
                    .stroke(Stroke::new(1.0, p.borda))
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(Margin::same(10))
                    .show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.horizontal(|ui| {
                            ui.label(icones::icone_cor("smart_toy", 16.0, p.destaque_tinta));
                            ui.label(
                                RichText::new(controle::origem_da_proposta(&v.proposta))
                                    .strong()
                                    .size(12.5),
                            );
                            texto_pequeno(
                                ui,
                                format!("em {}", formato::data(&v.proposta.criada_em)),
                            );
                        });
                        ui.add_space(4.0);
                        ui.label(RichText::new(&v.texto).size(13.5));
                        ui.add_space(4.0);
                        match &v.simulacao {
                            Ok(s) => resumo_da_simulacao(ui, s),
                            Err(e) => {
                                componentes::aviso(
                                    ui,
                                    "warning",
                                    e,
                                    p.aviso_sutil,
                                    p.aviso_borda,
                                    p.aviso,
                                );
                            }
                        }
                        texto_pequeno(
                            ui,
                            format!("Vence em {}", formato::data(&v.proposta.expira_em)),
                        );
                        ui.add_space(6.0);
                        if estado.recusando.as_deref() == Some(id.as_str()) {
                            let motivo = estado.motivos_de_recusa.entry(id.clone()).or_default();
                            ui.add(
                                egui::TextEdit::singleline(motivo)
                                    .hint_text("Motivo da recusa")
                                    .desired_width(f32::INFINITY),
                            );
                            let vazio = motivo.trim().is_empty();
                            let texto = motivo.trim().to_owned();
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        !vazio,
                                        egui::Button::new("Recusar com este motivo"),
                                    )
                                    .on_disabled_hover_text("Diga o motivo.")
                                    .clicked()
                                {
                                    pedidos.push(Pedido::Controle(
                                        PedidoDoControle::RecusarProposta(
                                            id.clone(),
                                            texto.clone(),
                                        ),
                                    ));
                                    estado.recusando = None;
                                }
                                if ui.button("Voltar").clicked() {
                                    estado.recusando = None;
                                }
                            });
                        } else {
                            ui.horizontal(|ui| {
                                let ok = v.simulacao.is_ok();
                                if ui
                                    .add_enabled(ok, egui::Button::new("Confirmar"))
                                    .on_hover_text("Aplica agora, como você")
                                    .on_disabled_hover_text("A linha mudou, recarregue.")
                                    .clicked()
                                {
                                    pedidos.push(Pedido::Controle(
                                        PedidoDoControle::ConfirmarProposta(id.clone()),
                                    ));
                                }
                                if ui.button("Recusar…").clicked() {
                                    estado.recusando = Some(id.clone());
                                }
                            });
                        }
                    });
                ui.add_space(8.0);
            }
        });
    estado.propostas = Some((estado.geracao, lista));
}

/// "Afeta 3 meses (12/2026 a 02/2027)" com as mudanças e os conflitos.
fn resumo_da_simulacao(ui: &mut Ui, s: &Simulacao) {
    let p = tema::paleta(ui.ctx());
    if s.meses_afetados.is_empty() {
        texto_pequeno(ui, "Não muda nenhum valor mensal.");
    } else {
        let a = s
            .meses_afetados
            .first()
            .copied()
            .map(competencia_texto)
            .unwrap_or_default();
        let b = s
            .meses_afetados
            .last()
            .copied()
            .map(competencia_texto)
            .unwrap_or_default();
        ui.label(
            RichText::new(format!(
                "Meses afetados: {} ({a} a {b})",
                s.meses_afetados.len()
            ))
            .size(12.5)
            .strong(),
        );
        egui::Grid::new(ui.next_auto_id())
            .num_columns(3)
            .spacing([14.0, 2.0])
            .min_row_height(16.0)
            .show(ui, |ui| {
                for (comp, antes, depois) in s.mudancas.iter().take(6) {
                    ui.label(RichText::new(competencia_texto(*comp)).size(12.0));
                    ui.label(
                        RichText::new(format!("antes {}", antes.map_or("—".to_owned(), reais)))
                            .size(12.0)
                            .color(p.tinta_suave),
                    );
                    ui.label(
                        RichText::new(format!("depois {}", depois.map_or("—".to_owned(), reais)))
                            .size(12.0),
                    );
                    ui.end_row();
                }
            });
        if s.mudancas.len() > 6 {
            texto_pequeno(ui, format!("… e mais {} meses.", s.mudancas.len() - 6));
        }
    }
    for c in &s.conflitos {
        ui.horizontal_wrapped(|ui| {
            ui.label(icones::icone_cor("warning", 14.0, p.aviso));
            ui.label(RichText::new(c).size(12.0).color(p.aviso));
        });
    }
}

fn arquivados_ui(ui: &mut Ui, estado: &mut EstadoDoControle, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    componentes::titulo(ui, "Arquivados e excluídos", 16.0);
    texto_pequeno(
        ui,
        "Nada é apagado: dá para restaurar. Escolha um e use Restaurar.",
    );
    ui.add_space(6.0);
    let lista: Vec<Recorrente> = estado
        .arquivados
        .as_ref()
        .map(|(_, v)| v.clone())
        .unwrap_or_default();
    if lista.is_empty() {
        ui.label(RichText::new("Nada arquivado nem excluído.").color(p.tinta_suave));
    }
    for r in &lista {
        let sel = estado.restauravel.as_deref() == Some(r.id.as_str());
        let (rotulo, icone) = match &r.excluido {
            Some(e) => (format!("Excluído · {}", e.motivo), "delete"),
            None => ("Arquivado".to_owned(), "folder_special"),
        };
        let resp = Frame::new()
            .fill(if sel {
                p.destaque_sutil
            } else {
                p.superficie_0
            })
            .stroke(Stroke::new(1.0, p.borda))
            .corner_radius(CornerRadius::same(6))
            .inner_margin(Margin::same(8))
            .show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    ui.label(icones::icone_cor(icone, 15.0, p.tinta_suave));
                    ui.label(RichText::new(&r.nome).strong());
                });
                texto_pequeno(ui, format!("{} · {}", r.grupo, rotulo));
                if ui.small_button("Restaurar…").clicked() {
                    pedidos.push(Pedido::Controle(PedidoDoControle::Acao(
                        AcaoDoControle::Restaurar,
                    )));
                }
            })
            .response;
        if resp.interact(Sense::click()).clicked() {
            estado.restauravel = Some(r.id.clone());
        }
        ui.add_space(4.0);
    }
}

fn detalhe_ui(ui: &mut Ui, estado: &mut EstadoDoControle, hoje: Data) {
    let p = tema::paleta(ui.ctx());
    let Some(sel) = estado.linha_selecionada().cloned() else {
        ui.add_space(20.0);
        ui.label(
            RichText::new("Escolha uma linha da grade para ver o detalhe.").color(p.tinta_suave),
        );
        return;
    };
    ui.horizontal(|ui| {
        componentes::titulo(ui, &sel.nome, 16.0);
        selo_do_estado(ui, sel.estado);
    });
    texto_pequeno(
        ui,
        format!(
            "{} · {} · {}",
            sel.grupo,
            controle::natureza_texto(sel.natureza),
            competencia_texto(sel.id.competencia)
        ),
    );
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        for a in AbaDoDetalhe::TODAS {
            if ui
                .selectable_label(estado.aba_do_detalhe == a, a.rotulo())
                .clicked()
            {
                estado.aba_do_detalhe = a;
            }
        }
    });
    ui.separator();
    let aba = estado.aba_do_detalhe;
    let Some(d) = estado.detalhe.as_ref() else {
        return;
    };
    let dados = match &d.dados {
        Ok(v) => v,
        Err(e) => {
            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
            return;
        }
    };
    let mut mais = false;
    ScrollArea::vertical()
        .id_salt("controle-detalhe")
        .show(ui, |ui| match aba {
            AbaDoDetalhe::Resumo => aba_resumo(ui, &sel, dados.recorrente.as_ref()),
            AbaDoDetalhe::Proximas => {
                mais = aba_proximas(ui, &dados.proximas, d.meses, hoje);
            }
            AbaDoDetalhe::Versoes => aba_versoes(ui, dados.recorrente.as_ref()),
            AbaDoDetalhe::Pausas => aba_pausas(ui, dados.recorrente.as_ref()),
            AbaDoDetalhe::Excecoes => aba_excecoes(ui, &dados.excecoes),
            AbaDoDetalhe::Historico => aba_historico(ui, &dados.historico),
        });
    if mais {
        estado.proximas_meses =
            (estado.proximas_meses + PROXIMAS_DE_INICIO).min(sbf_planejamento::HORIZONTE_MESES);
    }
}

fn campo(ui: &mut Ui, rotulo: &str, valor: impl Into<String>) {
    let p = tema::paleta(ui.ctx());
    ui.label(RichText::new(rotulo).color(p.tinta_suave).size(12.0));
    ui.label(valor.into());
    ui.end_row();
}

fn prazo_texto(p: &Prazo) -> String {
    match p {
        Prazo::SemFim => "Sem fim".into(),
        Prazo::AteCompetencia(c) => format!("Até {} (exclusivo)", competencia_texto(*c)),
        Prazo::Parcelas { total, primeira } => {
            format!(
                "{total} parcelas, a primeira em {}",
                competencia_texto(*primeira)
            )
        }
        Prazo::Desconhecido => "Desconhecido (projeção como estimativa)".into(),
    }
}

fn aba_resumo(ui: &mut Ui, o: &Ocorrencia, r: Option<&Recorrente>) {
    egui::Grid::new("controle-resumo")
        .num_columns(2)
        .spacing([14.0, 6.0])
        .show(ui, |ui| {
            campo(ui, "Vencimento", {
                let v = controle::vencimento(o);
                match v.secundario {
                    Some(s) => format!("{} · {s}", v.principal),
                    None => v.principal,
                }
            });
            campo(ui, "Previsto", controle::dinheiro_ou_sem_valor(o.previsto));
            campo(
                ui,
                "Pago informado",
                o.pago_informado.map_or("—".to_owned(), reais),
            );
            campo(ui, "Conciliado", o.conciliado.map_or("—".to_owned(), reais));
            campo(
                ui,
                "Falta",
                controle::falta(o).map_or("—".to_owned(), reais),
            );
            campo(ui, "Origem do valor", controle::origem_dica(o, false));
            campo(ui, "Conta", o.conta.clone().unwrap_or_else(|| "—".into()));
            campo(ui, "Centro", o.centro.clone().unwrap_or_else(|| "—".into()));
            if let Some(r) = r {
                campo(ui, "Prazo", prazo_texto(&r.prazo));
                campo(
                    ui,
                    "Situação",
                    match (&r.excluido, r.arquivado, &r.cancelamento) {
                        (Some(_), _, _) => "Excluído",
                        (_, true, _) => "Arquivado",
                        (_, _, Some(_)) => "Cancelado",
                        _ => "Ativo",
                    },
                );
                campo(ui, "Versão do registro", r.versao_do_registro.to_string());
            }
        });
}

fn aba_proximas(ui: &mut Ui, v: &[Ocorrencia], meses: u16, _hoje: Data) -> bool {
    let p = tema::paleta(ui.ctx());
    texto_pequeno(ui, format!("Próximos {meses} meses a partir do período."));
    egui::Grid::new("controle-proximas")
        .num_columns(4)
        .spacing([12.0, 3.0])
        .striped(true)
        .show(ui, |ui| {
            for t in ["Mês", "Vencimento", "Previsto", "Estado"] {
                ui.label(RichText::new(t).strong().size(12.0).color(p.tinta_suave));
            }
            ui.end_row();
            for o in v {
                ui.label(competencia_texto(o.id.competencia));
                ui.label(data_texto(o.vencimento));
                ui.label(controle::dinheiro_ou_sem_valor(o.previsto));
                ui.label(controle::estilo_do_estado(o.estado).rotulo);
                ui.end_row();
            }
        });
    if v.is_empty() {
        texto_pequeno(ui, "Sem ocorrências neste intervalo.");
    }
    meses < sbf_planejamento::HORIZONTE_MESES
        && ui
            .button(format!("Mostrar mais {PROXIMAS_DE_INICIO} meses"))
            .clicked()
}

fn aba_versoes(ui: &mut Ui, r: Option<&Recorrente>) {
    let Some(r) = r else { return };
    for v in r.versoes.iter().rev() {
        componentes::cartao(ui, |ui| {
            ui.label(RichText::new(format!("Desde {}", competencia_texto(v.desde))).strong());
            texto_pequeno(
                ui,
                format!(
                    "{} · {} · {}",
                    controle::dinheiro_ou_sem_valor(v.valor),
                    match v.certeza {
                        sbf_planejamento::Certeza::Confirmado => "confirmado",
                        sbf_planejamento::Certeza::Estimado => "estimado",
                    },
                    match v.fonte {
                        sbf_planejamento::Fonte::Contrato => "contrato",
                        sbf_planejamento::Fonte::Planilha => "planilha",
                        sbf_planejamento::Fonte::Pluggy => "Pluggy",
                        sbf_planejamento::Fonte::Usuario => "você",
                        sbf_planejamento::Fonte::Ia => "IA",
                    }
                ),
            );
            texto_pequeno(
                ui,
                format!(
                    "Conta {} · centro {}",
                    v.conta.as_deref().unwrap_or("—"),
                    v.centro.as_deref().unwrap_or("—")
                ),
            );
        });
        ui.add_space(4.0);
    }
}

fn aba_pausas(ui: &mut Ui, r: Option<&Recorrente>) {
    let Some(r) = r else { return };
    if r.pausas.is_empty() && r.cancelamento.is_none() {
        texto_pequeno(ui, "Sem pausas nem cancelamento.");
    }
    for p in &r.pausas {
        componentes::cartao(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "Pausa de {} {}",
                    competencia_texto(p.desde),
                    p.ate.map_or("até retomar".to_owned(), |a| format!(
                        "até {} (exclusivo)",
                        competencia_texto(a)
                    ))
                ))
                .strong(),
            );
            texto_pequeno(ui, format!("Motivo: {}", p.motivo));
        });
        ui.add_space(4.0);
    }
    if let Some(c) = &r.cancelamento {
        componentes::cartao(ui, |ui| {
            ui.label(
                RichText::new(format!("Cancelado desde {}", competencia_texto(c.desde))).strong(),
            );
            texto_pequeno(
                ui,
                format!(
                    "Motivo: {} · {}",
                    c.motivo,
                    if c.comprovado {
                        "com comprovante do fornecedor"
                    } else {
                        "sem comprovante do fornecedor (o app não afirma o cancelamento)"
                    }
                ),
            );
        });
    }
}

/// As exceções do compromisso (`excecoes_do_recorrente`), com as desfeitas marcadas.
fn aba_excecoes(ui: &mut Ui, v: &[sbf_planejamento::ExcecaoGravada]) {
    if v.is_empty() {
        texto_pequeno(
            ui,
            "Nenhuma exceção: adiamentos, dispensas, valores só de um mês e pagamentos informados aparecem aqui.",
        );
    }
    for e in v {
        let tipo = match &e.excecao {
            Excecao::Adiar { .. } => "Adiada",
            Excecao::Dispensar { .. } => "Dispensada",
            Excecao::ValorSoDesteMes { .. } => "Valor só deste mês",
            Excecao::PagoInformado { .. } => "Pago informado",
        };
        componentes::cartao(ui, |ui| {
            ui.label(
                RichText::new(format!(
                    "{tipo} · {}{}",
                    competencia_texto(e.ocorrencia.competencia),
                    if e.desfeita { " · desfeita" } else { "" }
                ))
                .strong(),
            );
            texto_pequeno(
                ui,
                format!(
                    "{} · por {}",
                    formato::data(&e.quando),
                    autor_texto(&e.autor)
                ),
            );
        });
        ui.add_space(4.0);
    }
}

fn aba_historico(ui: &mut Ui, h: &[EntradaDoHistorico]) {
    if h.is_empty() {
        texto_pequeno(ui, "Sem alterações registradas.");
    }
    for e in h {
        componentes::cartao(ui, |ui| {
            ui.label(&e.resumo);
            texto_pequeno(
                ui,
                format!(
                    "{} · por {}",
                    formato::data(&e.quando),
                    autor_texto(&e.autor)
                ),
            );
        });
        ui.add_space(4.0);
    }
}

// ---------------------------------------------------------------------------------------
// Diálogo (desenho)
// ---------------------------------------------------------------------------------------

/// Desenha o diálogo aberto, se houver, e devolve os pedidos que ele fez.
pub fn desenhar_dialogo(
    ctx: &egui::Context,
    plan: &dyn Planejamento,
    estado: &mut EstadoDoControle,
    hoje: Data,
    pedidos: &mut Vec<Pedido>,
) {
    let Some(mut d) = estado.dialogo.take() else {
        return;
    };
    d.atualizar_previa(plan, hoje);
    let mut fechar = false;
    let mut aplicar = false;
    let resposta = egui::Modal::new(Id::new("controle-dialogo")).show(ctx, |ui| {
        ui.set_width(520.0);
        componentes::titulo(ui, &d.titulo(), 17.0);
        texto_pequeno(
            ui,
            format!("Mês da linha: {}", competencia_texto(d.competencia)),
        );
        ui.add_space(8.0);
        formulario_ui(ui, &mut d);
        ui.add_space(8.0);
        ui.separator();
        previa_ui(ui, &d);
        if let Some(e) = &d.erro {
            let p = tema::paleta(ui.ctx());
            ui.add_space(4.0);
            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
        }
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let pode = d.pode_aplicar();
            if ui
                .add_enabled(pode, egui::Button::new("Aplicar"))
                .on_disabled_hover_text(match d.alteracao() {
                    Err(m) => m,
                    Ok(_) => "Sem prévia válida.".into(),
                })
                .clicked()
            {
                aplicar = true;
            }
            if ui.button("Cancelar").clicked() {
                fechar = true;
            }
        });
    });
    if resposta.should_close() {
        fechar = true;
    }
    if aplicar && let Ok(alt) = d.alteracao() {
        pedidos.push(Pedido::Controle(PedidoDoControle::Aplicar(Box::new(alt))));
    }
    if !fechar {
        estado.dialogo = Some(d);
    }
}

fn formulario_ui(ui: &mut Ui, d: &mut DialogoDoControle) {
    let grupos = d.grupos.clone();
    match &mut d.formulario {
        Formulario::Editar {
            escopo,
            valor,
            conta,
            centro,
            estimado,
        } => {
            ui.label("Onde a mudança vale");
            ui.horizontal(|ui| {
                ui.radio_value(escopo, EscopoEscolhido::SoEsteMes, "Só este mês");
                ui.radio_value(escopo, EscopoEscolhido::DaquiEmDiante, "Daqui em diante");
            });
            egui::Grid::new("controle-form-editar")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Valor (R$)");
                    ui.add(egui::TextEdit::singleline(valor).hint_text("312,44"));
                    ui.end_row();
                    let daqui = *escopo == EscopoEscolhido::DaquiEmDiante;
                    ui.label("Conta");
                    ui.add_enabled(daqui, egui::TextEdit::singleline(conta));
                    ui.end_row();
                    ui.label("Centro");
                    ui.add_enabled(daqui, egui::TextEdit::singleline(centro));
                    ui.end_row();
                    ui.label("");
                    ui.add_enabled(daqui, egui::Checkbox::new(estimado, "Valor estimado"));
                    ui.end_row();
                });
            if *escopo == EscopoEscolhido::SoEsteMes {
                texto_pequeno(
                    ui,
                    "Só este mês muda apenas o valor, como uma exceção deste mês.",
                );
            }
        }
        Formulario::Pausar { desde, ate, motivo } => {
            egui::Grid::new("controle-form-pausar")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Pausar a partir de");
                    ui.add(egui::TextEdit::singleline(desde).hint_text("12/2026"));
                    ui.end_row();
                    ui.label("Até (não entra)");
                    ui.add(egui::TextEdit::singleline(ate).hint_text("vazio = até retomar"));
                    ui.end_row();
                    ui.label("Motivo");
                    ui.add(egui::TextEdit::singleline(motivo).desired_width(280.0));
                    ui.end_row();
                });
        }
        Formulario::Retomar { desde } => {
            egui::Grid::new("controle-form-retomar")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Voltar em");
                    ui.add(egui::TextEdit::singleline(desde).hint_text("03/2027"));
                    ui.end_row();
                });
            texto_pequeno(
                ui,
                "Os meses anteriores seguem pausados; este mês e os seguintes voltam.",
            );
        }
        Formulario::Cancelar {
            desde,
            motivo,
            comprovado,
        } => {
            egui::Grid::new("controle-form-cancelar")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Cancelar a partir de");
                    ui.add(egui::TextEdit::singleline(desde).hint_text("02/2027"));
                    ui.end_row();
                    ui.label("Motivo");
                    ui.add(egui::TextEdit::singleline(motivo).desired_width(280.0));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(comprovado, "Tenho o comprovante do fornecedor");
                    ui.end_row();
                });
            texto_pequeno(
                ui,
                "Nada antes muda e o que já era devido continua. Sem comprovante, o app não afirma o cancelamento.",
            );
        }
        Formulario::Adiar { nova_data } => {
            egui::Grid::new("controle-form-adiar")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Nova data");
                    ui.add(
                        egui::TextEdit::singleline(nova_data)
                            .hint_text("dd/mm/aaaa; vazio = sem data"),
                    );
                    ui.end_row();
                });
            texto_pequeno(
                ui,
                "Adiar não paga: continua devido. Sem data, passa a contar em atraso depois do vencimento original.",
            );
        }
        Formulario::Dispensar { motivo } => {
            egui::Grid::new("controle-form-dispensar")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Motivo");
                    ui.add(egui::TextEdit::singleline(motivo).desired_width(300.0));
                    ui.end_row();
                });
            texto_pequeno(
                ui,
                "Dispensar tira só este mês do que é devido. Não é pagar.",
            );
        }
        Formulario::InformarPago { valor, data } => {
            egui::Grid::new("controle-form-pago")
                .num_columns(2)
                .spacing([10.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Valor pago (R$)");
                    ui.add(egui::TextEdit::singleline(valor));
                    ui.end_row();
                    ui.label("Data");
                    ui.add(egui::TextEdit::singleline(data).hint_text("dd/mm/aaaa"));
                    ui.end_row();
                });
            texto_pequeno(
                ui,
                "É o que você informa. A conciliação com o banco é outro passo.",
            );
        }
        Formulario::Excluir { motivo, confirmou } => {
            egui::Grid::new("controle-form-excluir")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label("Motivo");
                    ui.add(egui::TextEdit::singleline(motivo).desired_width(300.0));
                    ui.end_row();
                });
            ui.checkbox(
                confirmou,
                "Sei que o compromisso some do controle (dá para restaurar)",
            );
        }
        Formulario::Arquivar => {
            ui.label(
                "O compromisso sai da grade, sem apagar nada. Dá para tirar do arquivo depois.",
            );
        }
        Formulario::Restaurar { excluido } => {
            ui.label(if *excluido {
                "Restaura o compromisso excluído, com o histórico."
            } else {
                "Tira o compromisso do arquivo."
            });
        }
        Formulario::Mover { grupo } => {
            egui::ComboBox::from_id_salt("controle-form-mover")
                .selected_text(grupo.as_str())
                .show_ui(ui, |ui| {
                    for g in &grupos {
                        ui.selectable_value(grupo, g.clone(), g);
                    }
                });
            texto_pequeno(
                ui,
                "Vai para o fim do grupo escolhido. Os valores dos meses não mudam.",
            );
        }
    }
}

fn previa_ui(ui: &mut Ui, d: &DialogoDoControle) {
    let p = tema::paleta(ui.ctx());
    ui.label(RichText::new("Prévia").strong().size(13.5));
    texto_pequeno(ui, "Nada foi gravado. Isto é o que muda se você aplicar.");
    ui.add_space(4.0);
    match d.alteracao() {
        Err(m) => {
            ui.label(RichText::new(m).color(p.aviso));
        }
        Ok(_) => match d.previa() {
            None => {
                ui.label(RichText::new("Calculando…").color(p.tinta_suave));
            }
            Some(Err(e)) => {
                componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
            }
            Some(Ok(s)) => resumo_da_simulacao(ui, s),
        },
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};

    fn hoje() -> Data {
        Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data")
    }

    fn estado_e_servico() -> (EstadoDoControle, PlanejamentoFicticio) {
        let s = PlanejamentoFicticio::novo(hoje());
        let mut e = EstadoDoControle::novo(hoje());
        e.garantir_cache(&s, hoje());
        (e, s)
    }

    #[test]
    fn itens_respeitam_o_recolhido() {
        let (e, _) = estado_e_servico();
        let g = &e.cache.as_ref().unwrap().grade;
        let todos = itens_da_grade(g, &HashSet::new());
        let linhas = todos
            .iter()
            .filter(|i| matches!(i, ItemDaGrade::Linha { .. }))
            .count();
        assert_eq!(linhas, g.linhas().count());
        assert!(matches!(todos.last(), Some(ItemDaGrade::Total(_))));
        // Recolher o primeiro grupo some com as linhas dele, mas deixa o cabeçalho.
        let primeiro = &g.grupos[0];
        let mut rec = HashSet::new();
        rec.insert(chave_do_grupo(primeiro));
        let menos = itens_da_grade(g, &rec);
        let linhas_menos = menos
            .iter()
            .filter(|i| matches!(i, ItemDaGrade::Linha { .. }))
            .count();
        assert_eq!(linhas - linhas_menos, primeiro.linhas().count());
        assert!(matches!(
            menos[0],
            ItemDaGrade::Grupo {
                recolhido: true,
                ..
            }
        ));
        // Cada grupo termina no subtotal, e o total fecha tudo.
        let subtotais = todos
            .iter()
            .filter(|i| matches!(i, ItemDaGrade::Subtotal(_)))
            .count();
        assert_eq!(subtotais, g.grupos.len());
    }

    #[test]
    fn recolher_subgrupo_esconde_so_as_linhas_dele() {
        let (e, _) = estado_e_servico();
        let g = &e.cache.as_ref().unwrap().grade;
        let lar = g.grupos.iter().find(|x| x.nome == "Lar").unwrap();
        let sub = lar.subgrupos.iter().find(|s| s.nome.is_some()).unwrap();
        let mut rec = HashSet::new();
        rec.insert(chave_do_subgrupo(lar, sub));
        let v = itens_da_grade(g, &rec);
        let total = itens_da_grade(g, &HashSet::new());
        assert_eq!(total.len() - v.len(), sub.linhas.len());
    }

    #[test]
    fn dialogo_de_editar_tem_previa_antes_de_aplicar() {
        let (mut e, s) = estado_e_servico();
        e.selecionar_recorrente("rec_internet");
        assert!(e.selecionada.is_some());
        let mut d = abrir_dialogo(AcaoDoControle::Editar, &e, &s, hoje()).expect("diálogo");
        // Sem mudar nada, "só este mês" não monta alteração.
        assert!(d.alteracao().is_err());
        assert!(!d.pode_aplicar());
        if let Formulario::Editar { valor, escopo, .. } = &mut d.formulario {
            *valor = "139,90".into();
            *escopo = EscopoEscolhido::DaquiEmDiante;
        }
        d.atualizar_previa(&s, hoje());
        assert!(d.pode_aplicar());
        let Some(Ok(sim)) = d.previa() else {
            panic!("sem prévia")
        };
        assert!(!sim.meses_afetados.is_empty());
        // Mudar o texto invalida a prévia até simular de novo.
        if let Formulario::Editar { valor, .. } = &mut d.formulario {
            *valor = "140,00".into();
        }
        assert!(d.previa().is_none());
    }

    #[test]
    fn aplicar_com_versao_velha_diz_recarregue() {
        let e = ErroDoPlanejamento::VersaoVelha { atual: 7 };
        assert_eq!(mensagem_de_erro(&e), "A linha mudou, recarregue.");
        let (mut est, mut s) = estado_e_servico();
        est.selecionar_recorrente("rec_internet");
        let d = abrir_dialogo(AcaoDoControle::Pausar, &est, &s, hoje()).unwrap();
        // Outra alteração chega antes e a base do diálogo fica velha.
        s.aplicar(
            Alteracao::Arquivar {
                recorrente: "rec_internet".into(),
                arquivado: false,
                versao_base: d.alvo.versao_base,
            },
            Autor::Usuario,
        )
        .unwrap();
        let mut d = d;
        if let Formulario::Pausar { motivo, .. } = &mut d.formulario {
            *motivo = "viagem".into();
        }
        d.atualizar_previa(&s, hoje());
        match d.previa() {
            Some(Err(m)) => assert_eq!(m, "A linha mudou, recarregue."),
            _ => panic!("esperava a prévia com erro"),
        }
        assert!(!d.pode_aplicar());
    }

    #[test]
    fn contexto_da_linha_para_subir_e_descer() {
        let (mut e, _) = estado_e_servico();
        let primeiro = e.cache.as_ref().unwrap().grade.grupos[0]
            .linhas()
            .next()
            .unwrap()
            .id
            .clone();
        e.selecionada = Some(primeiro);
        assert!(e.contexto().primeira_do_grupo);
        assert!(controle::disponibilidade(AcaoDoControle::Subir, &e.contexto()).is_err());
        assert!(controle::disponibilidade(AcaoDoControle::Descer, &e.contexto()).is_ok());
    }

    #[test]
    fn propostas_vem_em_portugues_e_com_previa() {
        let s = PlanejamentoFicticio::novo(hoje());
        let v = carregar_propostas(&s, hoje());
        assert_eq!(v.len(), 2);
        for p in &v {
            assert!(!p.texto.contains("rec_"));
            assert!(p.simulacao.is_ok());
        }
    }
}
