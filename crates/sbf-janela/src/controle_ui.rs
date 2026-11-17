//! A área **Controle mensal** (spec 11): cabeçalho com período, cartões do resumo, faixa
//! "Em atraso de antes", grade densa de 3 níveis, painel lateral (detalhe, propostas do Claude e
//! arquivados) e os diálogos de edição com a prévia de `Planejamento::simular`.
//!
//! Só lê por `&dyn Planejamento`. Toda escrita vira um [`PedidoDoControle`] que
//! [`crate::Janela::processar`] aplica fora do desenho, com `Autor::Usuario`. A decisão do que
//! mostrar mora em [`crate::controle`], testada sem egui.

use std::collections::{HashMap, HashSet};

use egui::{
    Align, Color32, CornerRadius, FontId, Frame, Id, Layout, Margin, RichText, ScrollArea, Sense,
    Stroke, Ui,
};
use egui_extras::{Column, TableBuilder};
use sbf_inventario::consulta::Consulta;
use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Assinatura, Autor, Competencia, Composicao, Conciliacao, Controle,
    Data, DocumentoVinculado, EntradaDoHistorico, ErroDoPlanejamento, EstadoDaProposta, Excecao,
    Filtro, Grupo, IdOcorrencia, MovimentoCandidato, Natureza, Ocorrencia, Planejamento, Prazo,
    Proposta, Recorrente, Simulacao,
};

use crate::componentes;
use crate::compromisso::{
    FormularioDeCompromisso, FrequenciaEscolhida, NaturezaEscolhida, PrazoEscolhido,
};
use crate::controle::{
    self, AcaoDoControle, AlvoDoDialogo, AtualDaVersao, Cartao, ContextoDaLinha, EscopoEscolhido,
    Formulario, Grade, GrupoVm, ModoDoPeriodo, Navegador, PROXIMAS_DE_INICIO, SubgrupoVm,
    Subtotais, Tom, competencia_texto, data_texto, reais,
};
use crate::formato;
use crate::icones;
use crate::movimentos::{self, OfertaDeAssinatura};
use crate::navegacao::Pedido;
use crate::preferencias_da_janela::PreferenciasDoControle;
use crate::rubricas::{self, SeletorDeDocumento, TabelaDeRubricas};
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
    /// Várias alterações em ordem (um grupo novo e o compromisso; um vínculo de documento).
    /// Para na primeira que falha.
    AplicarVarias(Vec<Alteracao>),
    /// Mostra o documento (pelo `content_hash`) no painel Prévia do dock.
    PreviaDoDocumento(String),
    /// Abre a prévia do documento numa janela própria.
    DestacarDocumento(String),
    /// Concilia à mão com um movimento escolhido na lista. `descricao` é a do banco, para
    /// oferecer a assinatura depois.
    ConciliarManual {
        conciliacao: Conciliacao,
        descricao: String,
    },
    /// Define (ou tira) a assinatura de um recorrente, lendo a versão na hora.
    DefinirAssinatura {
        recorrente: String,
        assinatura: Option<Assinatura>,
    },
    /// Põe a cópia do acervo do contracheque (`content_hash`) na fila do Reconhecimento.
    RevisarContracheque(String),
    FecharDialogo,
    ConfirmarProposta(String),
    RecusarProposta(String, String),
    Recarregar,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AbaLateral {
    Propostas,
    Arquivados,
}

/// As abas do Detalhe. Guardada entre aberturas; valor desconhecido vale Resumo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum AbaDoDetalhe {
    Rubricas,
    Documentos,
    /// Versões, pausas e exceções.
    Vigencia,
    Historico,
    /// Também o que vale para um nome desconhecido (`serde(other)` pede a última variante).
    #[default]
    #[serde(other)]
    Resumo,
}

impl AbaDoDetalhe {
    const TODAS: [AbaDoDetalhe; 5] = [
        AbaDoDetalhe::Resumo,
        AbaDoDetalhe::Rubricas,
        AbaDoDetalhe::Documentos,
        AbaDoDetalhe::Vigencia,
        AbaDoDetalhe::Historico,
    ];

    fn rotulo(self) -> &'static str {
        match self {
            AbaDoDetalhe::Resumo => "Resumo",
            AbaDoDetalhe::Rubricas => "Rubricas",
            AbaDoDetalhe::Documentos => "Documentos",
            AbaDoDetalhe::Vigencia => "Vigência",
            AbaDoDetalhe::Historico => "Histórico",
        }
    }

    /// A aba que de fato se mostra: a guardada, ou Resumo quando ela não existe para a linha
    /// (Rubricas sem composição). Nunca altera a preferência.
    pub fn efetiva(self, tem_rubricas: bool) -> AbaDoDetalhe {
        if self == AbaDoDetalhe::Rubricas && !tem_rubricas {
            AbaDoDetalhe::Resumo
        } else {
            self
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
    /// As linhas de entrada cujas rubricas não batem com o digitado, com o texto do aviso.
    avisos: HashMap<IdOcorrencia, String>,
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
    competencia: Competencia,
    geracao: u64,
    meses: u16,
    dados: Result<DadosDoDetalhe, String>,
}

struct DadosDoDetalhe {
    recorrente: Option<Recorrente>,
    proximas: Vec<Ocorrencia>,
    historico: Vec<EntradaDoHistorico>,
    excecoes: Vec<sbf_planejamento::ExcecaoGravada>,
    composicao: Option<Composicao>,
    /// Os documentos ligados só a esta ocorrência.
    docs_da_ocorrencia: Vec<DocumentoVinculado>,
    /// Os ligados ao compromisso inteiro.
    docs_do_compromisso: Vec<DocumentoVinculado>,
    /// A conciliação à espera de confirmação, se houver.
    sugestao: Option<Conciliacao>,
    /// Os movimentos do extrato que podem pagar esta ocorrência, do melhor ao pior.
    candidatos: Vec<MovimentoCandidato>,
    /// As conciliações gravadas desta ocorrência, com o movimento do extrato.
    conciliacoes: Vec<sbf_planejamento::ConciliacaoComMovimento>,
}

/// O diálogo "Adicionar compromisso": o formulário e a prévia da última criação válida.
#[derive(Debug, Clone)]
pub struct NovoDialogo {
    pub formulario: FormularioDeCompromisso,
    pub grupos: Vec<Grupo>,
    simulada: Option<(Alteracao, Result<Simulacao, String>)>,
}

impl NovoDialogo {
    pub fn alteracao(&self) -> Result<Alteracao, String> {
        self.formulario.alteracao()
    }

    /// A prévia que vale para o que está no formulário agora.
    pub fn previa(&self) -> Option<&Result<Simulacao, String>> {
        let atual = self.alteracao().ok()?;
        self.simulada
            .as_ref()
            .filter(|(a, _)| *a == atual)
            .map(|(_, s)| s)
    }

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

    pub fn pode_criar(&self) -> bool {
        matches!(self.previa(), Some(Ok(_)))
    }

    /// As alterações em ordem: o grupo novo, se houver, e o compromisso.
    pub fn alteracoes(&self) -> Result<Vec<Alteracao>, String> {
        self.formulario.alteracoes(&self.grupos)
    }
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
    /// O diálogo de criar compromisso.
    pub novo: Option<NovoDialogo>,
    /// O seletor de "Anexar documento".
    pub anexar: Option<SeletorDeDocumento>,
    /// O documento (`content_hash`) mostrado no painel Prévia do dock.
    pub documento_em_previa: Option<String>,
    /// A lista de "Conciliar com outro…" está aberta para esta ocorrência.
    pub conciliar_aberto: Option<IdOcorrencia>,
    /// O que se oferece depois de uma conciliação manual (a assinatura).
    pub oferta_de_assinatura: Option<OfertaDeAssinatura>,
    /// A tela "Contas do planejamento".
    pub contas: crate::contas_ui::EstadoDasContas,
    selecao_da_previa: Option<IdOcorrencia>,
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
        let navegador = Navegador::abertura(hoje);
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
            novo: None,
            anexar: None,
            documento_em_previa: None,
            conciliar_aberto: None,
            oferta_de_assinatura: None,
            contas: crate::contas_ui::EstadoDasContas::default(),
            selecao_da_previa: None,
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

    /// O que a janela guarda entre aberturas.
    pub fn preferencias(&self) -> PreferenciasDoControle {
        let mut recolhidos: Vec<String> = self.recolhidos.iter().cloned().collect();
        recolhidos.sort();
        PreferenciasDoControle {
            resumo_aberto: self.resumo_aberto,
            atrasadas_abertas: self.atrasadas_abertas,
            modo: self.navegador.modo,
            filtro: self.filtro,
            recolhidos,
            aba_do_detalhe: self.aba_do_detalhe,
        }
    }

    /// Restaura o que foi guardado. O modo muda o período mostrado, que continua cobrindo
    /// o mesmo ponto no tempo.
    pub fn aplicar_preferencias(&mut self, p: &PreferenciasDoControle) {
        self.resumo_aberto = p.resumo_aberto;
        self.atrasadas_abertas = p.atrasadas_abertas;
        if self.navegador.modo != p.modo {
            self.navegador.trocar_modo(p.modo);
        }
        self.filtro = p.filtro;
        self.recolhidos = p.recolhidos.iter().cloned().collect();
        self.aba_do_detalhe = p.aba_do_detalhe;
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
        let (grade, mut cartoes) = match &controle {
            Ok(c) => (controle::montar_grade(c), controle::cartoes(c, hoje)),
            Err(_) => (Grade::default(), Vec::new()),
        };
        let mut avisos = HashMap::new();
        let mut do_cartao: Vec<(String, String)> = Vec::new();
        if let Ok(c) = &controle {
            for (i, o) in c
                .linhas
                .iter()
                .chain(c.atrasadas_de_antes.iter())
                .enumerate()
                .filter(|(_, o)| matches!(o.natureza, Natureza::Entrada))
            {
                if let Ok(Some(comp)) = plan.composicao(&o.id.recorrente, o.id.competencia)
                    && let Some(t) = rubricas::aviso_de_divergencia(&comp, o.previsto)
                {
                    if i < c.linhas.len() {
                        do_cartao.push((o.nome.clone(), t.clone()));
                    }
                    avisos.insert(o.id.clone(), t);
                }
            }
        }
        controle::avisar_nos_cartoes(&mut cartoes, &do_cartao);
        self.cache = Some(CacheDoControle {
            chave,
            controle,
            grade,
            cartoes,
            avisos,
        });
    }

    fn garantir_selecao(&mut self, plan: &dyn Planejamento) {
        // A prévia do documento vale para a linha em que foi aberta.
        if self.selecao_da_previa != self.selecionada {
            self.documento_em_previa = None;
            self.selecao_da_previa.clone_from(&self.selecionada);
        }
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
        if self.lateral == Some(AbaLateral::Arquivados)
            && self
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
        // As propostas pendentes alimentam o botão do cabeçalho, mesmo com o painel fechado.
        if self
            .propostas
            .as_ref()
            .is_none_or(|(g, _)| *g != self.geracao)
        {
            self.propostas = Some((self.geracao, carregar_propostas(plan, hoje)));
        }
    }

    /// Lê o detalhe da linha escolhida (uma vez por escolha, por geração e por janela de
    /// próximas ocorrências).
    fn garantir_detalhe(&mut self, plan: &dyn Planejamento, hoje: Data) {
        let Some(id) = self.selecionada.clone() else {
            return;
        };
        let desatualizado = self.detalhe.as_ref().is_none_or(|d| {
            d.recorrente != id.recorrente
                || d.competencia != id.competencia
                || d.geracao != self.geracao
                || d.meses != self.proximas_meses
        });
        if desatualizado {
            let desde = Competencia::de_data(self.navegador.periodo().inicio());
            self.detalhe = Some(DetalheEmCache {
                recorrente: id.recorrente.clone(),
                competencia: id.competencia,
                geracao: self.geracao,
                meses: self.proximas_meses,
                dados: carregar_detalhe(plan, &id, desde, self.proximas_meses, hoje),
            });
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
    ocorrencia: &IdOcorrencia,
    desde: Competencia,
    meses: u16,
    hoje: Data,
) -> Result<DadosDoDetalhe, String> {
    let id = ocorrencia.recorrente.as_str();
    let recorrente = plan.recorrente(id).map_err(|e| e.to_string())?;
    let composicao = plan
        .composicao(id, ocorrencia.competencia)
        .map_err(|e| e.to_string())?;
    let docs_da_ocorrencia = plan
        .documentos(&AlvoDoDocumento::Ocorrencia {
            id: ocorrencia.clone(),
        })
        .map_err(|e| e.to_string())?;
    let docs_do_compromisso = plan
        .documentos(&AlvoDoDocumento::Recorrente { id: id.to_owned() })
        .map_err(|e| e.to_string())?;
    let proximas = plan
        .proximas(id, desde, meses, hoje)
        .map_err(|e| e.to_string())?;
    let historico = plan.historico(Some(id), 60).map_err(|e| e.to_string())?;
    let excecoes = plan.excecoes_do_recorrente(id).map_err(|e| e.to_string())?;
    // O extrato é um extra: se falhar, o resto do detalhe continua valendo.
    let sugestao = plan.sugestao_de_conciliacao(ocorrencia).ok().flatten();
    let candidatos = plan
        .candidatos_de_conciliacao(ocorrencia)
        .unwrap_or_default();
    let conciliacoes = plan
        .conciliacoes_da_ocorrencia(ocorrencia)
        .unwrap_or_default();
    Ok(DadosDoDetalhe {
        sugestao,
        candidatos,
        conciliacoes,
        recorrente,
        proximas,
        historico,
        excecoes,
        composicao,
        docs_da_ocorrencia,
        docs_do_compromisso,
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

/// Os campos da assinatura no diálogo de Editar: o texto da descrição do banco e a janela em
/// dias. Texto vazio tira a assinatura.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CampoDeAssinatura {
    pub texto: String,
    pub janela: String,
    pub original: Option<Assinatura>,
}

impl CampoDeAssinatura {
    pub fn novo(original: Option<Assinatura>) -> Self {
        Self {
            texto: original
                .as_ref()
                .map(|a| a.texto.clone())
                .unwrap_or_default(),
            janela: original
                .as_ref()
                .map_or(movimentos::JANELA_OFERECIDA, |a| a.janela_dias)
                .to_string(),
            original,
        }
    }

    /// O que os campos dizem: `None` = sem assinatura. Erro quando a janela não é um número
    /// de 1 a 60.
    pub fn valor(&self) -> Result<Option<Assinatura>, String> {
        let texto = self.texto.trim();
        if texto.is_empty() {
            return Ok(None);
        }
        match self.janela.trim().parse::<u8>() {
            Ok(j) if (1..=60).contains(&j) => Ok(Some(Assinatura {
                texto: texto.to_owned(),
                janela_dias: j,
            })),
            _ => Err("A janela da assinatura vai de 1 a 60 dias.".to_owned()),
        }
    }

    /// A assinatura a gravar, se ela mudou (`Some(None)` = tirar).
    pub fn a_gravar(&self) -> Option<Option<Assinatura>> {
        let v = self.valor().ok()?;
        (v != self.original).then_some(v)
    }
}

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
    /// Só no Editar: como reconhecer o compromisso no extrato.
    pub assinatura: Option<CampoDeAssinatura>,
    /// O formulário como abriu: sem mexer nele, só a assinatura é gravada.
    pub inicial: Formulario,
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
        let assinatura_boa = self.assinatura.as_ref().is_none_or(|a| a.valor().is_ok());
        // Mexer só na assinatura não tem prévia de valores: a edição em si não muda nada.
        let so_assinatura = self.formulario == self.inicial
            && self
                .assinatura
                .as_ref()
                .and_then(CampoDeAssinatura::a_gravar)
                .is_some();
        assinatura_boa && (so_assinatura || matches!(self.previa(), Some(Ok(_))))
    }

    /// Os pedidos que o Aplicar faz, na ordem: a edição (a menos que só a assinatura mudou)
    /// e a assinatura. A versão desta é lida na hora, depois da edição.
    pub fn pedidos_de_aplicar(&self) -> Vec<Pedido> {
        let nova = self
            .assinatura
            .as_ref()
            .and_then(CampoDeAssinatura::a_gravar);
        let so_assinatura = nova.is_some() && self.formulario == self.inicial;
        let mut v = Vec::new();
        if !so_assinatura && let Ok(alt) = self.alteracao() {
            v.push(Pedido::Controle(PedidoDoControle::Aplicar(Box::new(alt))));
        }
        if let Some(assinatura) = nova {
            v.push(Pedido::Controle(PedidoDoControle::DefinirAssinatura {
                recorrente: self.alvo.recorrente.clone(),
                assinatura,
            }));
        }
        v
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
            inicial: Formulario::Restaurar {
                excluido: r.excluido.is_some(),
            },
            assinatura: None,
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
    let assinatura = (acao == AcaoDoControle::Editar)
        .then(|| CampoDeAssinatura::novo(plan.assinatura(&rec.id).ok().flatten()));
    Ok(DialogoDoControle {
        inicial: formulario.clone(),
        assinatura,
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

/// O triângulo de aviso (⚠): as rubricas do salário não batem com o digitado.
fn triangulo_de_aviso(ui: &mut Ui, texto: &str) -> egui::Response {
    let p = tema::paleta(ui.ctx());
    ui.label(icones::icone_cor("warning", 15.0, p.aviso))
        .on_hover_text(format!("Atenção: {texto}."))
}

pub(crate) fn texto_pequeno(ui: &mut Ui, t: impl Into<String>) {
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
    estado.garantir_detalhe(plan, hoje);
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
            cabecalho(ui, estado, pedidos);
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

fn cabecalho(ui: &mut Ui, estado: &mut EstadoDoControle, pedidos: &mut Vec<Pedido>) {
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
            .selectable_label(estado.lateral == Some(AbaLateral::Arquivados), "Arquivados")
            .clicked()
        {
            estado.lateral = toggle(estado.lateral, AbaLateral::Arquivados);
        }
        ui.separator();
        ui.checkbox(&mut estado.resumo_aberto, "Cartões do resumo");
        ui.separator();
        let def = controle::definicao(AcaoDoControle::AdicionarCompromisso);
        if ui
            .button(format!("{}  {}", icones::texto(def.icone), def.rotulo))
            .on_hover_text(format!("{} ({})", def.dica, atalho_em_texto(def)))
            .clicked()
        {
            pedidos.push(Pedido::Controle(PedidoDoControle::Acao(def.id)));
        }
    });
}

fn toggle(atual: Option<AbaLateral>, alvo: AbaLateral) -> Option<AbaLateral> {
    if atual == Some(alvo) {
        None
    } else {
        Some(alvo)
    }
}

pub(crate) fn mensagem(ui: &mut Ui, estado: &mut EstadoDoControle) {
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
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(&k.valor)
                                        .font(FontId::new(18.0, crate::tema::familia_forte()))
                                        .color(p.tinta),
                                );
                                if let Some(a) = &k.aviso {
                                    triangulo_de_aviso(ui, a);
                                }
                            });
                            for n in &k.notas {
                                ui.add(egui::Label::new(
                                    RichText::new(n).size(11.0).color(p.tinta_suave),
                                ));
                            }
                            if let Some(a) = &k.aviso {
                                ui.add(egui::Label::new(
                                    RichText::new(a).size(11.0).color(p.aviso),
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
                                linha(
                                    &mut row,
                                    o,
                                    *nivel,
                                    projecao,
                                    c.avisos.get(&o.id).map(String::as_str),
                                    &p,
                                );
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
    aviso: Option<&str>,
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
                if let Some(a) = aviso {
                    triangulo_de_aviso(ui, a);
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
    let _ = (plan, hoje);
    ui.horizontal(|ui| {
        for (a, rot) in [
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
        None => {}
    }
}

fn autor_texto(a: &Autor) -> &'static str {
    match a {
        Autor::Usuario => "você",
        Autor::Importacao => "importação",
        Autor::Ia { .. } => "IA",
        Autor::Mcp => "Claude (MCP)",
        Autor::Regra { .. } => "regra automática",
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

/// O detalhe da linha escolhida, no painel Detalhe do dock (o mesmo clique que escolhe a
/// linha da grade). Sem linha escolhida, diz o que fazer.
pub fn detalhe_no_dock(
    ui: &mut Ui,
    plan: &dyn Planejamento,
    estado: &mut EstadoDoControle,
    hoje: Data,
    pedidos: &mut Vec<Pedido>,
) {
    estado.garantir_detalhe(plan, hoje);
    detalhe_ui(ui, estado, hoje, pedidos);
}

/// Os contracheques ligados à ocorrência ou ao compromisso inteiro.
fn contracheques_de(dados: &DadosDoDetalhe) -> Vec<&DocumentoVinculado> {
    dados
        .docs_da_ocorrencia
        .iter()
        .chain(dados.docs_do_compromisso.iter())
        .filter(|d| d.tipo == sbf_planejamento::TipoDeDocumento::Contracheque && !d.desvinculado)
        .collect()
}

/// A seção Movimentos da ocorrência; desconto em folha não passa pelo extrato.
fn movimentos_do_detalhe(
    sel: &Ocorrencia,
    dados: &DadosDoDetalhe,
) -> Option<movimentos::VmDeMovimentos> {
    (sel.natureza != Natureza::DescontoEmFolha).then(|| {
        movimentos::montar(&movimentos::EntradaDosMovimentos {
            conciliado: sel.conciliado,
            gravadas: &dados.conciliacoes,
            sugestao: dados.sugestao.as_ref(),
            candidatos: &dados.candidatos,
            composicao: dados.composicao.as_ref(),
            contracheques: &contracheques_de(dados),
            previsto: sel.previsto,
        })
    })
}

/// O que um clique no detalhe pede; aplicado depois do desenho.
enum EfeitoDoDetalhe {
    IrParaLinha(String),
    Previa(String),
    Destacar(String),
    Desvincular(String),
    Anexar,
    RevisarContracheque(String),
}

fn detalhe_ui(ui: &mut Ui, estado: &mut EstadoDoControle, hoje: Data, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    let Some(sel) = estado.linha_selecionada().cloned() else {
        ui.add_space(6.0);
        ui.label(
            RichText::new(
                "Clique numa linha da grade para ver o detalhe aqui. As Ações dela ficam na faixa de Ações e no menu do botão direito.",
            )
            .color(p.tinta_suave),
        );
        return;
    };
    ui.horizontal_wrapped(|ui| {
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
    let tabela = dados
        .composicao
        .as_ref()
        .map(|c| rubricas::tabela_de_rubricas(c, sel.previsto));
    if let Some(a) = tabela.as_ref().and_then(|t| t.aviso.as_ref()) {
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            ui.label(icones::icone_cor("warning", 16.0, p.aviso));
            ui.label(RichText::new(format!("Atenção: {a}.")).color(p.aviso));
        });
    }
    ui.add_space(4.0);
    let tem_rubricas = tabela.is_some();
    let aba = estado.aba_do_detalhe.efetiva(tem_rubricas);
    let n_docs = dados.docs_da_ocorrencia.len() + dados.docs_do_compromisso.len();
    if let Some(nova) = faixa_de_abas(ui, &p, aba, tem_rubricas, n_docs) {
        estado.aba_do_detalhe = nova;
    }
    let aba = estado.aba_do_detalhe.efetiva(tem_rubricas);
    ui.separator();
    let contracheques = contracheques_de(dados);
    let vm_movimentos = movimentos_do_detalhe(&sel, dados);
    let faixa = movimentos::faixa_do_contracheque(
        dados.composicao.as_ref(),
        &contracheques,
        sel.id.competencia,
    );
    let mut conciliar_aberto = estado.conciliar_aberto.as_ref() == Some(&sel.id);
    let mut dispensou_oferta = false;
    let oferta = estado
        .oferta_de_assinatura
        .as_ref()
        .filter(|o| o.recorrente == sel.id.recorrente);
    let mut mais = false;
    let mut efeitos: Vec<EfeitoDoDetalhe> = Vec::new();
    // O painel do dock já rola na vertical; aqui só as tabelas largas rolam de lado.
    ScrollArea::horizontal()
        .id_salt("controle-detalhe")
        .auto_shrink([false, true])
        .show(ui, |ui| match aba {
            AbaDoDetalhe::Resumo => {
                // Os movimentos primeiro: é o que se decide ali (confirmar, escolher outro).
                if let Some(vm) = &vm_movimentos {
                    dispensou_oferta =
                        movimentos::secao(ui, vm, &sel.id, &mut conciliar_aberto, oferta, pedidos);
                    ui.add_space(8.0);
                }
                aba_resumo(ui, &sel, dados.recorrente.as_ref());
                ui.add_space(8.0);
                secao_do_detalhe(ui, &p, "Próximas ocorrências");
                mais = aba_proximas(ui, &dados.proximas, d.meses, hoje);
            }
            AbaDoDetalhe::Rubricas => {
                if let Some(f) = &faixa {
                    faixa_do_contracheque_ui(ui, f, &mut efeitos);
                }
                if let Some(t) = &tabela {
                    aba_rubricas(ui, t, &mut efeitos);
                }
            }
            AbaDoDetalhe::Documentos => aba_documentos(
                ui,
                &sel,
                &dados.docs_da_ocorrencia,
                &dados.docs_do_compromisso,
                estado.documento_em_previa.as_deref(),
                &mut efeitos,
            ),
            AbaDoDetalhe::Vigencia => {
                secao_do_detalhe(ui, &p, "Versões");
                aba_versoes(ui, dados.recorrente.as_ref());
                ui.add_space(8.0);
                secao_do_detalhe(ui, &p, "Pausas e cancelamento");
                aba_pausas(ui, dados.recorrente.as_ref());
                ui.add_space(8.0);
                secao_do_detalhe(ui, &p, "Exceções");
                aba_excecoes(ui, &dados.excecoes);
            }
            AbaDoDetalhe::Historico => aba_historico(ui, &dados.historico),
        });
    estado.conciliar_aberto = conciliar_aberto.then(|| sel.id.clone());
    if dispensou_oferta {
        estado.oferta_de_assinatura = None;
    }
    if mais {
        estado.proximas_meses =
            (estado.proximas_meses + PROXIMAS_DE_INICIO).min(sbf_planejamento::HORIZONTE_MESES);
    }
    for e in efeitos {
        match e {
            EfeitoDoDetalhe::IrParaLinha(r) => estado.selecionar_recorrente(&r),
            EfeitoDoDetalhe::Previa(h) => {
                pedidos.push(Pedido::Controle(PedidoDoControle::PreviaDoDocumento(h)));
            }
            EfeitoDoDetalhe::Destacar(h) => {
                pedidos.push(Pedido::Controle(PedidoDoControle::DestacarDocumento(h)));
            }
            EfeitoDoDetalhe::Desvincular(id) => {
                pedidos.push(Pedido::Controle(PedidoDoControle::AplicarVarias(vec![
                    Alteracao::DesvincularDocumento { id },
                ])));
            }
            EfeitoDoDetalhe::RevisarContracheque(sha) => {
                pedidos.push(Pedido::Controle(PedidoDoControle::RevisarContracheque(sha)));
            }
            EfeitoDoDetalhe::Anexar => {
                pedidos.push(Pedido::Controle(PedidoDoControle::Acao(
                    AcaoDoControle::AnexarDocumento,
                )));
            }
        }
    }
}

fn celula_de_valor(ui: &mut Ui, v: sbf_planejamento::Centavos, forte: bool) {
    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
        let t = RichText::new(reais(v));
        ui.label(if forte { t.strong() } else { t });
    });
}

/// Título de seção dentro de uma aba do Detalhe.
fn secao_do_detalhe(ui: &mut Ui, p: &Paleta, titulo: &str) {
    ui.label(RichText::new(titulo).strong().size(13.0).color(p.tinta));
    ui.add_space(2.0);
}

/// A faixa de abas do Detalhe: quebra de linha (cabe num dock estreito), sublinhado na ativa
/// e fundo no hover, como as abas de página da casca. Devolve a aba clicada, se houve.
fn faixa_de_abas(
    ui: &mut Ui,
    p: &Paleta,
    ativa: AbaDoDetalhe,
    tem_rubricas: bool,
    n_docs: usize,
) -> Option<AbaDoDetalhe> {
    let mut nova = None;
    let largura = ui.available_rect_before_wrap().x_range();
    let faixa = ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(2.0, 2.0);
        for a in AbaDoDetalhe::TODAS {
            if a == AbaDoDetalhe::Rubricas && !tem_rubricas {
                continue;
            }
            let texto = match a {
                AbaDoDetalhe::Documentos if n_docs > 0 => format!("{}  {n_docs}", a.rotulo()),
                _ => a.rotulo().to_owned(),
            };
            let eh_ativa = a == ativa;
            let cor = if eh_ativa { p.destaque_tinta } else { p.tinta };
            let g = ui
                .painter()
                .layout_no_wrap(texto, egui::FontId::proportional(13.5), cor);
            let (rect, resp) =
                ui.allocate_exact_size(g.size() + egui::vec2(16.0, 10.0), egui::Sense::click());
            if resp.hovered() && !eh_ativa {
                ui.painter().rect_filled(
                    rect,
                    egui::CornerRadius::same(4),
                    ui.visuals().widgets.hovered.weak_bg_fill,
                );
            }
            ui.painter().galley(rect.center() - g.size() / 2.0, g, cor);
            if eh_ativa {
                ui.painter().rect_filled(
                    egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 5.0, rect.bottom() - 2.0),
                        egui::pos2(rect.right() - 5.0, rect.bottom()),
                    ),
                    egui::CornerRadius::same(1),
                    p.destaque,
                );
            }
            if resp
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                nova = Some(a);
            }
        }
    });
    let r = faixa.response.rect;
    ui.painter()
        .hline(largura, r.bottom(), egui::Stroke::new(1.0, p.borda));
    nova
}

/// A faixa da aba Rubricas: o contracheque vinculado tem rubricas que a composição vigente
/// não usa. "Revisar e usar" leva a cópia do acervo ao Reconhecimento.
fn faixa_do_contracheque_ui(
    ui: &mut Ui,
    f: &movimentos::FaixaDoContracheque,
    efeitos: &mut Vec<EfeitoDoDetalhe>,
) {
    let p = tema::paleta(ui.ctx());
    Frame::new()
        .fill(p.aviso_sutil)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor("warning", 16.0, p.aviso));
                ui.label(
                    RichText::new(format!(
                        "O contracheque {} vinculado traz rubricas diferentes.",
                        f.mes_ano
                    ))
                    .color(p.aviso),
                );
                if ui
                    .button("Revisar e usar")
                    .on_hover_text("Abre o contracheque no painel Reconhecimento para você conferir as rubricas")
                    .clicked()
                {
                    efeitos.push(EfeitoDoDetalhe::RevisarContracheque(f.sha256.clone()));
                }
            });
        });
    ui.add_space(6.0);
}

/// A aba "Valores e rubricas": proventos, descontos, totais e o líquido contra o digitado.
fn aba_rubricas(ui: &mut Ui, t: &TabelaDeRubricas, efeitos: &mut Vec<EfeitoDoDetalhe>) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new("Origem").color(p.tinta_suave).size(12.0));
        ui.label(RichText::new(&t.origem).strong());
        if let Some(h) = &t.documento
            && ui
                .small_button(format!("{}  Ver documento", icones::texto("visibility")))
                .on_hover_text("Mostra o documento no painel Prévia")
                .clicked()
        {
            efeitos.push(EfeitoDoDetalhe::Previa(h.clone()));
        }
    });
    ui.add_space(6.0);
    for (titulo, linhas, total) in [
        ("Proventos", &t.proventos, t.total_proventos),
        ("Descontos", &t.descontos, t.total_descontos),
    ] {
        ui.label(RichText::new(titulo).strong().size(13.5));
        if linhas.is_empty() {
            texto_pequeno(ui, "Nenhuma rubrica.");
            continue;
        }
        egui::Grid::new(("controle-rubricas", titulo))
            .num_columns(5)
            .spacing([12.0, 4.0])
            .striped(true)
            .show(ui, |ui| {
                // O valor logo depois do nome: o painel do dock é estreito.
                for c in ["Código", "Nome", "Valor", "Competência", "Prazo"] {
                    ui.label(RichText::new(c).size(11.5).color(p.tinta_suave));
                }
                ui.end_row();
                for l in linhas {
                    ui.label(RichText::new(&l.codigo).size(12.0));
                    match &l.recorrente {
                        Some(r) => {
                            if ui
                                .link(&l.nome)
                                .on_hover_text("Abre a linha deste desconto em folha")
                                .clicked()
                            {
                                efeitos.push(EfeitoDoDetalhe::IrParaLinha(r.clone()));
                            }
                        }
                        None => {
                            ui.label(&l.nome);
                        }
                    }
                    celula_de_valor(ui, l.valor, false);
                    ui.label(RichText::new(&l.competencia).size(12.0));
                    ui.label(RichText::new(&l.prazo).size(12.0));
                    ui.end_row();
                }
                ui.label("");
                ui.label(RichText::new(format!("Total de {}", titulo.to_lowercase())).strong());
                celula_de_valor(ui, total, true);
                ui.end_row();
            });
        ui.add_space(8.0);
    }
    egui::Grid::new("controle-rubricas-liquido")
        .num_columns(2)
        .spacing([14.0, 4.0])
        .show(ui, |ui| {
            campo(ui, "Líquido das rubricas", reais(t.liquido));
            campo(
                ui,
                "Valor digitado",
                t.digitado.map_or("sem valor".to_owned(), reais),
            );
        });
    ui.add_space(4.0);
    match &t.aviso {
        Some(a) => {
            componentes::aviso(
                ui,
                "warning",
                &format!("Atenção: {a}."),
                p.aviso_sutil,
                p.borda,
                p.aviso,
            );
        }
        None => {
            if t.digitado.is_some() {
                texto_pequeno(ui, "As rubricas batem com o valor digitado.");
            }
        }
    }
}

fn aba_documentos(
    ui: &mut Ui,
    sel: &Ocorrencia,
    da_ocorrencia: &[DocumentoVinculado],
    do_compromisso: &[DocumentoVinculado],
    em_previa: Option<&str>,
    efeitos: &mut Vec<EfeitoDoDetalhe>,
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        if ui
            .button(format!(
                "{}  Anexar documento…",
                icones::texto("attach_file")
            ))
            .on_hover_text("Escolhe um achado do inventário e liga a esta linha ou ao compromisso")
            .clicked()
        {
            efeitos.push(EfeitoDoDetalhe::Anexar);
        }
    });
    texto_pequeno(
        ui,
        "Fatura e boleto cobram; só o comprovante prova o pagamento.",
    );
    ui.add_space(6.0);
    for (titulo, docs) in [
        (
            format!("Só {}", competencia_texto(sel.id.competencia)),
            da_ocorrencia,
        ),
        ("Do compromisso inteiro".to_owned(), do_compromisso),
    ] {
        ui.label(RichText::new(titulo).strong().size(13.5));
        let linhas = rubricas::linhas_de_documentos(docs);
        if linhas.is_empty() {
            texto_pequeno(ui, "Nenhum documento vinculado.");
            ui.add_space(6.0);
            continue;
        }
        for l in &linhas {
            let em_foco = em_previa == Some(l.sha256.as_str());
            Frame::new()
                .fill(if em_foco {
                    p.superficie_3
                } else {
                    p.superficie_2
                })
                .stroke(Stroke::new(1.0, p.borda))
                .corner_radius(CornerRadius::same(6))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal_wrapped(|ui| {
                        componentes::selo(ui, l.tipo, p.info_sutil, p.info);
                        ui.label(RichText::new(&l.nome).strong());
                    });
                    ui.horizontal_wrapped(|ui| {
                        if let Some(d) = &l.data {
                            texto_pequeno(ui, d.clone());
                        }
                        if let Some(n) = l.nota {
                            texto_pequeno(ui, format!("· {n}"));
                        }
                    });
                    ui.horizontal(|ui| {
                        if ui.small_button("Prévia").clicked() {
                            efeitos.push(EfeitoDoDetalhe::Previa(l.sha256.clone()));
                        }
                        if ui
                            .small_button("Destacar")
                            .on_hover_text("Abre a prévia numa janela própria")
                            .clicked()
                        {
                            efeitos.push(EfeitoDoDetalhe::Destacar(l.sha256.clone()));
                        }
                        if ui
                            .small_button("Desvincular")
                            .on_hover_text("Tira o vínculo; o documento continua no inventário")
                            .clicked()
                        {
                            efeitos.push(EfeitoDoDetalhe::Desvincular(l.id.clone()));
                        }
                    });
                });
            ui.add_space(4.0);
        }
        ui.add_space(6.0);
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
    consulta: &dyn Consulta,
) {
    desenhar_novo_compromisso(ctx, plan, estado, hoje, pedidos);
    desenhar_anexar(ctx, estado, pedidos, consulta);
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
    if aplicar {
        pedidos.extend(d.pedidos_de_aplicar());
    }
    if !fechar {
        estado.dialogo = Some(d);
    }
}

/// Abre o formulário de compromisso novo. O grupo da linha escolhida vem preenchido.
pub fn abrir_novo_compromisso(
    estado: &EstadoDoControle,
    plan: &dyn Planejamento,
    hoje: Data,
) -> Result<NovoDialogo, String> {
    let grupos = plan.grupos().map_err(|e| e.to_string())?;
    let posicao = plan
        .recorrentes(true)
        .map_err(|e| e.to_string())?
        .iter()
        .map(|r| r.posicao)
        .max()
        .unwrap_or(0)
        + 10;
    let mut f = FormularioDeCompromisso::novo(hoje, &grupos, posicao);
    if let Some(o) = estado.linha_selecionada()
        && grupos.iter().any(|g| g.nome == o.grupo)
    {
        f.grupo.clone_from(&o.grupo);
    }
    Ok(NovoDialogo {
        formulario: f,
        grupos,
        simulada: None,
    })
}

/// Abre o seletor de "Anexar documento" sobre a linha escolhida.
pub fn abrir_anexar(estado: &EstadoDoControle) -> Result<SeletorDeDocumento, String> {
    let o = estado
        .linha_selecionada()
        .ok_or("Escolha uma linha do controle.")?;
    let mut s = SeletorDeDocumento::novo(
        o.id.recorrente.clone(),
        o.nome.clone(),
        Some(o.id.clone()),
        o.id.competencia,
    );
    if matches!(o.natureza, Natureza::Entrada) {
        s.tipo = sbf_planejamento::TipoDeDocumento::Contracheque;
    }
    Ok(s)
}

fn campo_de_texto(ui: &mut Ui, rotulo: &str, valor: &mut String, dica: &str, largura: f32) {
    ui.label(rotulo);
    ui.add(
        egui::TextEdit::singleline(valor)
            .hint_text(dica)
            .desired_width(largura),
    );
    ui.end_row();
}

fn desenhar_novo_compromisso(
    ctx: &egui::Context,
    plan: &dyn Planejamento,
    estado: &mut EstadoDoControle,
    hoje: Data,
    pedidos: &mut Vec<Pedido>,
) {
    let Some(mut d) = estado.novo.take() else {
        return;
    };
    d.atualizar_previa(plan, hoje);
    let mut fechar = false;
    let mut criar = false;
    let resposta = egui::Modal::new(Id::new("controle-novo-compromisso")).show(ctx, |ui| {
        ui.set_width(560.0);
        componentes::titulo(ui, "Adicionar compromisso", 17.0);
        texto_pequeno(
            ui,
            "Um compromisso é uma regra que gera uma linha por mês. Nada é gravado antes de Criar.",
        );
        ui.add_space(8.0);
        ScrollArea::vertical()
            .id_salt("controle-novo-rolagem")
            .max_height((ctx.content_rect().height() - 330.0).max(180.0))
            .show(ui, |ui| formulario_do_compromisso(ui, &mut d));
        ui.add_space(8.0);
        ui.separator();
        previa_do_novo(ui, &d);
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let motivo = match d.alteracao() {
                Err(m) => m,
                Ok(_) => "Sem prévia válida.".into(),
            };
            if ui
                .add_enabled(d.pode_criar(), egui::Button::new("Criar compromisso"))
                .on_disabled_hover_text(motivo)
                .clicked()
            {
                criar = true;
            }
            if ui.button("Cancelar").clicked() {
                fechar = true;
            }
        });
    });
    if resposta.should_close() {
        fechar = true;
    }
    if criar && let Ok(v) = d.alteracoes() {
        pedidos.push(Pedido::Controle(PedidoDoControle::AplicarVarias(v)));
        fechar = true;
    }
    if !fechar {
        estado.novo = Some(d);
    }
}

fn formulario_do_compromisso(ui: &mut Ui, d: &mut NovoDialogo) {
    let grupos = d.grupos.clone();
    let f = &mut d.formulario;
    egui::Grid::new("controle-form-novo")
        .num_columns(2)
        .spacing([10.0, 6.0])
        .show(ui, |ui| {
            campo_de_texto(ui, "Nome", &mut f.nome, "Academia", 300.0);
            ui.label("Grupo");
            ui.horizontal(|ui| {
                egui::ComboBox::from_id_salt("controle-novo-grupo")
                    .selected_text(if f.grupo.is_empty() {
                        "Escolha…"
                    } else {
                        f.grupo.as_str()
                    })
                    .show_ui(ui, |ui| {
                        for g in &grupos {
                            ui.selectable_value(&mut f.grupo, g.nome.clone(), &g.nome);
                        }
                    });
                ui.add(
                    egui::TextEdit::singleline(&mut f.grupo_novo)
                        .hint_text("ou um grupo novo")
                        .desired_width(150.0),
                );
            });
            ui.end_row();
            campo_de_texto(ui, "Subgrupo", &mut f.subgrupo, "opcional", 200.0);
            ui.label("Natureza");
            egui::ComboBox::from_id_salt("controle-novo-natureza")
                .selected_text(f.natureza.rotulo())
                .show_ui(ui, |ui| {
                    for n in NaturezaEscolhida::TODAS {
                        ui.selectable_value(&mut f.natureza, n, n.rotulo())
                            .on_hover_text(n.dica());
                    }
                });
            ui.end_row();
            ui.label("Valor (R$)");
            ui.horizontal(|ui| {
                ui.add_enabled(
                    !f.sem_valor,
                    egui::TextEdit::singleline(&mut f.valor)
                        .hint_text("312,44")
                        .desired_width(110.0),
                );
                ui.checkbox(&mut f.sem_valor, "sem valor")
                    .on_hover_text("O valor ainda não é conhecido. Não conta como zero.");
                ui.checkbox(&mut f.estimado, "estimado");
            });
            ui.end_row();
            ui.label("Frequência");
            egui::ComboBox::from_id_salt("controle-novo-frequencia")
                .selected_text(f.frequencia.rotulo())
                .show_ui(ui, |ui| {
                    for fr in FrequenciaEscolhida::TODAS {
                        ui.selectable_value(&mut f.frequencia, fr, fr.rotulo());
                    }
                });
            ui.end_row();
            match f.frequencia {
                FrequenciaEscolhida::Mensal => {
                    campo_de_texto(ui, "Dia do mês", &mut f.dia, "5", 60.0);
                    campo_de_texto(ui, "Começa em", &mut f.desde, "11/2026", 90.0);
                }
                FrequenciaEscolhida::CadaNMeses => {
                    campo_de_texto(ui, "A cada (meses)", &mut f.n_meses, "3", 60.0);
                    campo_de_texto(ui, "Dia do mês", &mut f.dia, "5", 60.0);
                    campo_de_texto(ui, "Primeira vez em", &mut f.desde, "11/2026", 90.0);
                }
                FrequenciaEscolhida::Anual => {
                    campo_de_texto(ui, "Mês do ano (1–12)", &mut f.mes_do_ano, "12", 60.0);
                    campo_de_texto(ui, "Dia do mês", &mut f.dia, "25", 60.0);
                    campo_de_texto(ui, "Começa em", &mut f.desde, "11/2026", 90.0);
                }
                FrequenciaEscolhida::Unica => {
                    campo_de_texto(ui, "Data", &mut f.data_unica, "15/02/2027", 110.0);
                }
            }
            if f.frequencia != FrequenciaEscolhida::Unica {
                ui.label("Prazo");
                egui::ComboBox::from_id_salt("controle-novo-prazo")
                    .selected_text(f.prazo.rotulo())
                    .show_ui(ui, |ui| {
                        for pz in PrazoEscolhido::TODOS {
                            ui.selectable_value(&mut f.prazo, pz, pz.rotulo());
                        }
                    });
                ui.end_row();
                match f.prazo {
                    PrazoEscolhido::SemFim => {}
                    PrazoEscolhido::AteMes => {
                        campo_de_texto(ui, "Até (não entra)", &mut f.ate_mes, "12/2027", 90.0);
                    }
                    PrazoEscolhido::Parcelas => {
                        campo_de_texto(ui, "Parcelas", &mut f.parcelas, "12", 60.0);
                    }
                }
            }
            campo_de_texto(ui, "Conta", &mut f.conta, "opcional", 200.0);
            campo_de_texto(ui, "Centro", &mut f.centro, "opcional", 200.0);
        });
    texto_pequeno(ui, f.natureza.dica());
}

fn previa_do_novo(ui: &mut Ui, d: &NovoDialogo) {
    let p = tema::paleta(ui.ctx());
    ui.label(RichText::new("Prévia").strong().size(13.5));
    texto_pequeno(
        ui,
        "Nada foi gravado. Isto é o que passa a aparecer se você criar.",
    );
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

fn desenhar_anexar(
    ctx: &egui::Context,
    estado: &mut EstadoDoControle,
    pedidos: &mut Vec<Pedido>,
    consulta: &dyn Consulta,
) {
    let Some(mut s) = estado.anexar.take() else {
        return;
    };
    s.buscar(consulta);
    let mut fechar = false;
    let mut anexar = false;
    let mut ver: Option<String> = None;
    let mut escolher: Option<String> = None;
    let resposta = egui::Modal::new(Id::new("controle-anexar")).show(ctx, |ui| {
        ui.set_width(600.0);
        componentes::titulo(
            ui,
            &format!("Anexar documento · {}", s.nome_do_compromisso),
            17.0,
        );
        texto_pequeno(
            ui,
            "Escolha um achado do inventário. Anexar não move nem apaga o arquivo.",
        );
        ui.add_space(8.0);
        ui.horizontal_wrapped(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut s.texto)
                    .hint_text("Buscar por nome, fornecedor ou tipo")
                    .desired_width(260.0),
            );
            ui.checkbox(
                &mut s.filtrar_por_periodo,
                format!("só perto de {}", competencia_texto(s.competencia)),
            )
            .on_hover_text("O mês da linha, os dois anteriores e o seguinte");
            ui.checkbox(&mut s.filtrar_por_tipo, "só do tipo escolhido");
        });
        ui.horizontal(|ui| {
            ui.label("Tipo do vínculo");
            let antes = s.tipo;
            egui::ComboBox::from_id_salt("controle-anexar-tipo")
                .selected_text(rubricas::rotulo_do_tipo(s.tipo))
                .show_ui(ui, |ui| {
                    for t in rubricas::TIPOS_DE_DOCUMENTO {
                        ui.selectable_value(&mut s.tipo, t, rubricas::rotulo_do_tipo(t));
                    }
                });
            if s.tipo != antes {
                s.tipo_escolhido_a_mao = true;
            }
            if let Some(n) = rubricas::nota_do_tipo(s.tipo) {
                texto_pequeno(ui, n);
            }
        });
        ui.add_space(4.0);
        if let Some(e) = &s.erro {
            let p = tema::paleta(ui.ctx());
            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
        }
        let visiveis = s.visiveis();
        texto_pequeno(
            ui,
            format!("{} de {} achados", visiveis.len(), s.candidatos.len()),
        );
        ScrollArea::vertical()
            .id_salt("controle-anexar-lista")
            .max_height(220.0)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                if visiveis.is_empty() {
                    texto_pequeno(
                        ui,
                        "Nenhum achado com esses filtros. Solte o período ou o tipo.",
                    );
                }
                for c in &visiveis {
                    let marcado = s.escolhido.as_deref() == Some(c.sha256.as_str());
                    let t = format!(
                        "{}   ·   {}   ·   {}{}",
                        c.nome,
                        if c.tipo.is_empty() {
                            "sem tipo"
                        } else {
                            c.tipo.as_str()
                        },
                        c.data
                            .as_deref()
                            .and_then(Data::de_iso)
                            .map_or("sem data".to_owned(), data_texto),
                        c.fornecedor
                            .as_deref()
                            .map(|f| format!("   ·   {f}"))
                            .unwrap_or_default(),
                    );
                    if ui.selectable_label(marcado, t).clicked() {
                        escolher = Some(c.sha256.clone());
                    }
                }
            });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Vincular a");
            if s.ocorrencia.is_some() {
                ui.radio_value(
                    &mut s.ao_compromisso_inteiro,
                    false,
                    format!("só {}", competencia_texto(s.competencia)),
                );
            }
            ui.radio_value(&mut s.ao_compromisso_inteiro, true, "o compromisso inteiro");
        });
        ui.add_space(10.0);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let motivo = match s.alteracao() {
                Err(m) => m,
                Ok(_) => String::new(),
            };
            if ui
                .add_enabled(s.alteracao().is_ok(), egui::Button::new("Anexar"))
                .on_disabled_hover_text(motivo)
                .clicked()
            {
                anexar = true;
            }
            if ui.button("Cancelar").clicked() {
                fechar = true;
            }
            if let Some(h) = &s.escolhido
                && ui
                    .button("Ver prévia")
                    .on_hover_text("Mostra o achado no painel Prévia, sem fechar o seletor")
                    .clicked()
            {
                ver = Some(h.clone());
            }
        });
    });
    if resposta.should_close() {
        fechar = true;
    }
    if let Some(h) = escolher {
        s.escolher(&h);
    }
    if let Some(h) = ver {
        pedidos.push(Pedido::Controle(PedidoDoControle::PreviaDoDocumento(h)));
    }
    if anexar && let Ok(alt) = s.alteracao() {
        pedidos.push(Pedido::Controle(PedidoDoControle::AplicarVarias(vec![alt])));
        fechar = true;
    }
    if !fechar {
        estado.anexar = Some(s);
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
                    ui.add_enabled(
                        daqui,
                        egui::TextEdit::singleline(conta).hint_text("apelido, como CX-0630"),
                    );
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
    if let Some(a) = &mut d.assinatura {
        ui.add_space(6.0);
        ui.label(RichText::new("Assinatura no extrato").strong());
        egui::Grid::new("controle-form-assinatura")
            .num_columns(2)
            .spacing([10.0, 6.0])
            .show(ui, |ui| {
                ui.label("Texto do banco");
                ui.add(
                    egui::TextEdit::singleline(&mut a.texto)
                        .hint_text("SALARIO EMPRESA")
                        .desired_width(240.0),
                );
                ui.end_row();
                ui.label("Janela (dias)");
                ui.add(egui::TextEdit::singleline(&mut a.janela).desired_width(60.0));
                ui.end_row();
            });
        match a.valor() {
            Err(e) => {
                ui.label(RichText::new(e).color(tema::paleta(ui.ctx()).aviso));
            }
            Ok(_) => texto_pequeno(
                ui,
                "O que o banco descreve com esse texto, perto do vencimento, vira candidato com qualquer valor. Vazio = sem assinatura.",
            ),
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

    // ----------------------------------------------------------------- detalhe e rubricas

    #[test]
    fn clicar_no_salario_carrega_o_detalhe_com_rubricas_e_documentos() {
        let (mut e, s) = estado_e_servico();
        e.selecionar_recorrente("rec_salario");
        assert_eq!(
            e.selecionada.as_ref().map(|i| i.recorrente.as_str()),
            Some("rec_salario")
        );
        e.garantir_selecao(&s);
        e.garantir_detalhe(&s, hoje());
        let d = e.detalhe.as_ref().expect("detalhe");
        assert_eq!(d.recorrente, "rec_salario");
        let dados = d.dados.as_ref().expect("dados");
        let c = dados.composicao.as_ref().expect("composição do salário");
        assert!(c.rubricas.len() >= 4);
        assert!(c.rubricas.iter().any(|r| r.recorrente.is_some()));
        // O contracheque está ligado ao compromisso inteiro.
        assert_eq!(dados.docs_do_compromisso.len(), 1);
        assert_eq!(
            dados.docs_do_compromisso[0].tipo,
            sbf_planejamento::TipoDeDocumento::Contracheque
        );
    }

    #[test]
    fn trocar_a_linha_troca_o_detalhe_e_larga_a_previa() {
        let (mut e, s) = estado_e_servico();
        e.selecionar_recorrente("rec_salario");
        e.garantir_selecao(&s);
        e.documento_em_previa = Some("abc".into());
        e.garantir_selecao(&s);
        assert_eq!(e.documento_em_previa.as_deref(), Some("abc"));
        e.selecionar_recorrente("rec_internet");
        e.garantir_selecao(&s);
        assert_eq!(e.documento_em_previa, None);
        e.garantir_detalhe(&s, hoje());
        assert_eq!(e.detalhe.as_ref().unwrap().recorrente, "rec_internet");
        assert!(
            e.detalhe
                .as_ref()
                .unwrap()
                .dados
                .as_ref()
                .unwrap()
                .composicao
                .is_none()
        );
    }

    #[test]
    fn rubricas_que_nao_batem_avisam_na_linha_e_no_cartao_de_entradas() {
        let (e, _) = estado_e_servico();
        let c = e.cache.as_ref().unwrap();
        let (id, texto) = c
            .avisos
            .iter()
            .find(|(i, _)| i.recorrente == "rec_salario")
            .expect("aviso do salário");
        assert!(texto.starts_with("as rubricas somam R$ 8.226,00, o digitado é"));
        assert_eq!(id.recorrente, "rec_salario");
        let entradas = c.cartoes.iter().find(|k| k.id == "entradas").unwrap();
        assert!(
            entradas
                .aviso
                .as_deref()
                .unwrap()
                .contains("as rubricas somam")
        );
        // Os outros cartões não ganham triângulo.
        assert!(c.cartoes.iter().filter(|k| k.aviso.is_some()).count() == 1);
    }

    #[test]
    fn composicao_que_bate_tira_o_aviso() {
        let (mut e, mut s) = estado_e_servico();
        let rec = s.recorrente("rec_salario").unwrap().unwrap();
        let mut c = s
            .composicao("rec_salario", Competencia::de_data(hoje()))
            .unwrap()
            .unwrap();
        // Ajusta o desconto até o líquido bater com os R$ 8.450,00 digitados.
        let dif = c
            .divergencia(Some(sbf_planejamento::Centavos(845_000)))
            .unwrap();
        c.rubricas[2].valor = sbf_planejamento::Centavos(c.rubricas[2].valor.0 + dif.0);
        c.desde = Competencia::de_data(hoje());
        c.origem = "contracheque 11/2026 (revisado)".into();
        s.aplicar(
            Alteracao::DefinirComposicao {
                composicao: c,
                versao_base: rec.versao_do_registro,
            },
            Autor::Usuario,
        )
        .expect("define a composição");
        e.recarregar();
        e.garantir_cache(&s, hoje());
        assert!(
            e.cache
                .as_ref()
                .unwrap()
                .avisos
                .keys()
                .all(|i| i.recorrente != "rec_salario")
        );
        let entradas = e
            .cache
            .as_ref()
            .unwrap()
            .cartoes
            .iter()
            .find(|k| k.id == "entradas")
            .unwrap();
        assert_eq!(entradas.aviso, None);
    }

    // ----------------------------------------------------------------- documentos

    #[test]
    fn anexar_e_desvincular_um_documento_na_ocorrencia() {
        let (mut e, mut s) = estado_e_servico();
        e.selecionar_recorrente("rec_internet");
        let mut seletor = abrir_anexar(&e).expect("seletor");
        seletor.candidatos = vec![rubricas::Candidato {
            sha256: "hash-do-boleto".into(),
            nome: "boleto-internet.pdf".into(),
            tipo: "boleto".into(),
            data: Some("2026-11-12".into()),
            fornecedor: None,
        }];
        seletor.escolher("hash-do-boleto");
        seletor.ao_compromisso_inteiro = false;
        let alvo = seletor.alvo();
        assert!(matches!(alvo, AlvoDoDocumento::Ocorrencia { .. }));
        s.aplicar(seletor.alteracao().unwrap(), Autor::Usuario)
            .expect("vincula");
        let lista = s.documentos(&alvo).unwrap();
        assert_eq!(lista.len(), 1);
        assert_eq!(lista[0].tipo, sbf_planejamento::TipoDeDocumento::Boleto);
        // O mesmo documento não vincula duas vezes ao mesmo alvo.
        assert!(
            s.aplicar(seletor.alteracao().unwrap(), Autor::Usuario)
                .is_err()
        );
        // Não aparece no compromisso inteiro.
        let inteiro = AlvoDoDocumento::Recorrente {
            id: "rec_internet".into(),
        };
        assert!(s.documentos(&inteiro).unwrap().is_empty());
        s.aplicar(
            Alteracao::DesvincularDocumento {
                id: lista[0].id.clone(),
            },
            Autor::Usuario,
        )
        .expect("desvincula");
        assert!(s.documentos(&alvo).unwrap().is_empty());
        assert!(
            s.aplicar(
                Alteracao::DesvincularDocumento {
                    id: lista[0].id.clone()
                },
                Autor::Usuario
            )
            .is_err()
        );
    }

    #[test]
    fn seletor_acha_o_contracheque_do_inventario_da_demonstracao() {
        let (consulta, _) = crate::demonstracao::nova();
        let (mut e, _) = estado_e_servico();
        e.selecionar_recorrente("rec_salario");
        let mut sel = abrir_anexar(&e).expect("seletor");
        assert_eq!(sel.tipo, sbf_planejamento::TipoDeDocumento::Contracheque);
        sel.buscar(&consulta);
        let visiveis = sel.visiveis();
        let n_visiveis = visiveis.len();
        assert!(
            visiveis
                .iter()
                .any(|c| c.nome == "contracheque-2026-11.pdf"),
            "{:?}",
            sel.candidatos.iter().map(|c| &c.nome).collect::<Vec<_>>()
        );
        // O filtro por tipo tira a fatura da lista de contracheques.
        assert!(visiveis.iter().all(|c| !c.nome.starts_with("fatura")));
        sel.filtrar_por_tipo = false;
        assert!(sel.visiveis().len() > n_visiveis);
        // Procurar por texto estreita a busca.
        sel.texto = "contracheque".into();
        sel.buscar(&consulta);
        assert!(!sel.candidatos.is_empty());
        assert!(sel.candidatos.len() < 10);
    }

    // ----------------------------------------------------------------- compromisso novo

    #[test]
    fn adicionar_compromisso_mostra_a_previa_e_cria_o_plano_avulso() {
        let (mut e, mut s) = estado_e_servico();
        e.selecionar_recorrente("rec_aluguel");
        let mut d = abrir_novo_compromisso(&e, &s, hoje()).expect("diálogo");
        // O grupo da linha escolhida já vem preenchido.
        assert_eq!(d.formulario.grupo, "Lar");
        assert!(!d.pode_criar());
        d.formulario.nome = "Presente da Ana".into();
        d.formulario.valor = "250,00".into();
        d.formulario.frequencia = FrequenciaEscolhida::Unica;
        d.formulario.data_unica = "15/02/2027".into();
        d.formulario.subgrupo = "Eventos".into();
        d.atualizar_previa(&s, hoje());
        assert!(d.pode_criar(), "{:?}", d.previa());
        let sim = d.previa().unwrap().as_ref().unwrap();
        assert_eq!(sim.meses_afetados, vec![Competencia { ano: 2027, mes: 2 }]);
        assert_eq!(
            sim.mudancas[0],
            (
                Competencia { ano: 2027, mes: 2 },
                None,
                Some(sbf_planejamento::Centavos(25_000))
            )
        );
        // Nada foi gravado antes de criar.
        assert!(
            s.recorrentes(false)
                .unwrap()
                .iter()
                .all(|r| r.nome != "Presente da Ana")
        );
        for a in d.alteracoes().unwrap() {
            s.aplicar(a, Autor::Usuario).expect("cria");
        }
        let criada = s
            .recorrentes(false)
            .unwrap()
            .into_iter()
            .find(|r| r.nome == "Presente da Ana")
            .expect("criado");
        assert_eq!(criada.subgrupo.as_deref(), Some("Eventos"));
        let fev = s
            .proximas(&criada.id, Competencia { ano: 2027, mes: 2 }, 1, hoje())
            .unwrap();
        assert_eq!(fev.len(), 1);
        assert_eq!(fev[0].previsto, Some(sbf_planejamento::Centavos(25_000)));
    }

    #[test]
    fn formulario_invalido_nao_tem_previa_nem_cria() {
        let (e, s) = estado_e_servico();
        let mut d = abrir_novo_compromisso(&e, &s, hoje()).expect("diálogo");
        d.formulario.nome = "Curso".into();
        d.formulario.valor = "12,345".into();
        d.atualizar_previa(&s, hoje());
        assert!(d.alteracao().unwrap_err().contains("Valor inválido"));
        assert!(d.previa().is_none());
        assert!(!d.pode_criar());
    }

    #[test]
    fn grupo_novo_e_criado_antes_do_compromisso() {
        let (e, mut s) = estado_e_servico();
        let mut d = abrir_novo_compromisso(&e, &s, hoje()).expect("diálogo");
        d.formulario.nome = "Ração".into();
        d.formulario.valor = "89,90".into();
        d.formulario.grupo_novo = "Pets".into();
        let v = d.alteracoes().unwrap();
        assert_eq!(v.len(), 2);
        for a in v {
            s.aplicar(a, Autor::Usuario).expect("aplica");
        }
        assert!(s.grupos().unwrap().iter().any(|g| g.nome == "Pets"));
        assert!(
            s.recorrentes(false)
                .unwrap()
                .iter()
                .any(|r| r.nome == "Ração" && r.grupo == "Pets")
        );
    }

    // ----------------------------------------------------------------- preferências

    #[test]
    fn aba_sem_composicao_mostra_resumo_sem_mudar_a_preferencia() {
        let (mut e, _) = estado_e_servico();
        e.aba_do_detalhe = AbaDoDetalhe::Rubricas;
        assert_eq!(e.aba_do_detalhe.efetiva(false), AbaDoDetalhe::Resumo);
        assert_eq!(e.aba_do_detalhe.efetiva(true), AbaDoDetalhe::Rubricas);
        // A preferência continua a guardada.
        assert_eq!(e.aba_do_detalhe, AbaDoDetalhe::Rubricas);
        assert_eq!(e.preferencias().aba_do_detalhe, AbaDoDetalhe::Rubricas);
        assert_eq!(
            AbaDoDetalhe::Vigencia.efetiva(false),
            AbaDoDetalhe::Vigencia
        );
    }

    #[test]
    fn preferencias_do_controle_vao_e_voltam() {
        let (mut e, _) = estado_e_servico();
        e.resumo_aberto = false;
        e.atrasadas_abertas = false;
        e.navegador.trocar_modo(ModoDoPeriodo::MesCivil);
        e.filtro = Filtro::SemValor;
        let g = e.cache.as_ref().unwrap().grade.grupos[0].clone();
        e.recolhidos.insert(chave_do_grupo(&g));
        let p = e.preferencias();
        // Vai para o arquivo e volta numa janela nova.
        let pasta = tempfile::tempdir().unwrap();
        let arquivo = pasta.path().join("p.json");
        crate::preferencias_da_janela::PreferenciasDaJanela {
            zoom: Some(1.2),
            controle: p.clone(),
            ..Default::default()
        }
        .salvar(&arquivo)
        .unwrap();
        let lidas = crate::preferencias_da_janela::PreferenciasDaJanela::carregar(&arquivo);
        assert_eq!(lidas.zoom_valido(), Some(1.2));
        let mut novo = EstadoDoControle::novo(hoje());
        assert!(novo.resumo_aberto && novo.atrasadas_abertas);
        novo.aplicar_preferencias(&lidas.controle);
        assert!(!novo.resumo_aberto);
        assert!(!novo.atrasadas_abertas);
        assert_eq!(novo.navegador.modo, ModoDoPeriodo::MesCivil);
        assert_eq!(novo.filtro, Filtro::SemValor);
        assert!(novo.recolhidos.contains(&chave_do_grupo(&g)));
        assert_eq!(novo.preferencias(), p);
    }

    #[test]
    fn janela_guarda_ao_mudar_e_restaura_na_proxima_abertura() {
        use crate::janela::Janela;
        let pasta = tempfile::tempdir().unwrap();
        let arquivo = pasta.path().join("dados").join("p.json");
        let mut j = Janela::nova();
        j.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        j.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje())));
        assert_eq!(j.restaurar_preferencias(Some(arquivo.clone())), None);
        assert_eq!(j.guardador.quadro(&j.preferencias_atuais(1.0)), Ok(true));
        // Sem mudar nada, não grava de novo.
        assert_eq!(j.guardador.quadro(&j.preferencias_atuais(1.0)), Ok(false));
        j.controle.resumo_aberto = false;
        assert_eq!(j.guardador.quadro(&j.preferencias_atuais(1.0)), Ok(true));
        assert_eq!(j.guardador.quadro(&j.preferencias_atuais(1.3)), Ok(true));
        // Janela nova, mesmo arquivo.
        let mut k = Janela::nova();
        k.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        k.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje())));
        assert_eq!(k.restaurar_preferencias(Some(arquivo)), Some(1.3));
        assert!(!k.controle.resumo_aberto);
    }

    #[test]
    fn previa_de_documento_vira_o_foco_do_dock_so_no_controle() {
        use crate::janela::Janela;
        let mut j = Janela::nova();
        j.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        j.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje())));
        assert!(j.controle_no_centro());
        assert_eq!(j.foco(), None);
        j.controle_pedido(PedidoDoControle::PreviaDoDocumento("h1".into()));
        assert_eq!(j.foco(), Some(rubricas::ref_do_documento("h1")));
    }

    #[test]
    fn adicionar_e_anexar_estao_na_lista_de_acoes() {
        use crate::controle::{ContextoDaLinha, disponibilidade};
        let c = ContextoDaLinha::default();
        assert!(disponibilidade(AcaoDoControle::AdicionarCompromisso, &c).is_ok());
        assert!(disponibilidade(AcaoDoControle::AnexarDocumento, &c).is_err());
        assert_eq!(
            controle::acao_do_codigo("controle.adicionar"),
            Some(AcaoDoControle::AdicionarCompromisso)
        );
    }

    // -----------------------------------------------------------------------------------
    // Conciliação com o extrato (contas, movimentos, assinatura, faixa do contracheque)
    // -----------------------------------------------------------------------------------

    fn janela_ligada() -> crate::janela::Janela {
        let mut j = crate::janela::Janela::nova();
        j.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        j.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje())));
        j
    }

    /// Escolhe o salário do mês do ciclo aberto e lê o detalhe.
    fn selecionar_salario(j: &mut crate::janela::Janela) -> Ocorrencia {
        let pl = j.planejamento.as_ref().expect("planejamento").0.as_ref();
        // A tela abre no ciclo seguinte (últimos dias do ciclo); o de hoje tem o de novembro.
        j.controle.navegador.ir_para_hoje();
        j.controle.garantir_cache(pl, hoje());
        // O salário de novembro (já caiu no banco; o de dezembro ainda vem): está em atraso de
        // antes, à espera da confirmação.
        j.controle.selecionada = Some(IdOcorrencia {
            recorrente: "rec_salario".into(),
            competencia: Competencia::de_data(hoje()),
            parcela: None,
        });
        j.controle.garantir_selecao(pl);
        j.controle.garantir_detalhe(pl, hoje());
        j.controle.linha_selecionada().cloned().expect("linha")
    }

    fn vm_do_salario(j: &mut crate::janela::Janela) -> (Ocorrencia, movimentos::VmDeMovimentos) {
        let sel = selecionar_salario(j);
        let dados = j
            .controle
            .detalhe
            .as_ref()
            .and_then(|d| d.dados.as_ref().ok())
            .expect("dados");
        let vm = movimentos_do_detalhe(&sel, dados).expect("movimentos");
        (sel, vm)
    }

    fn mapear_salario(j: &mut crate::janela::Janela) {
        j.controle_pedido(PedidoDoControle::AplicarVarias(vec![
            Alteracao::MapearConta {
                apelido: "CX-0630".into(),
                conta_do_extrato: Some(crate::planejamento_ficticio::CONTA_DO_SALARIO.into()),
            },
        ]));
        assert_eq!(j.controle.mensagem.as_ref().map(|m| m.1), Some(false));
    }

    #[test]
    fn contas_mostram_o_par_confirmado_e_a_sugestao_para_o_resto() {
        let j = janela_ligada();
        let pl = j.planejamento.as_ref().unwrap().0.as_ref();
        let d = crate::contas_ui::carregar(pl).expect("contas");
        let cx = d.contas.iter().find(|c| c.apelido == "CX-0630").unwrap();
        assert!(cx.conta_do_extrato.is_none());
        let s = d.sugestao_de("CX-0630").expect("sugestão");
        assert_eq!(
            s.conta_do_extrato,
            crate::planejamento_ficticio::CONTA_DO_SALARIO
        );
        assert!(!s.motivo.is_empty());
        assert!(d.sugestao_de("Nubank").is_none());
        assert!(d.do_extrato.len() >= 2);
    }

    #[test]
    fn confirmar_o_par_pelo_pedido_grava_e_diz_em_portugues() {
        let mut j = janela_ligada();
        let Pedido::Controle(p) = crate::contas_ui::pedido_de_par(
            "CX-0630",
            Some(crate::planejamento_ficticio::CONTA_DO_SALARIO),
        ) else {
            panic!("pedido do controle");
        };
        j.controle_pedido(p);
        assert_eq!(
            j.controle.mensagem,
            Some(("Par de conta confirmado.".to_owned(), false))
        );
        let pl = j.planejamento.as_ref().unwrap().0.as_ref();
        let d = crate::contas_ui::carregar(pl).unwrap();
        assert_eq!(
            d.contas
                .iter()
                .find(|c| c.apelido == "CX-0630")
                .unwrap()
                .conta_do_extrato
                .as_deref(),
            Some(crate::planejamento_ficticio::CONTA_DO_SALARIO)
        );
        // Desligar.
        let Pedido::Controle(p) = crate::contas_ui::pedido_de_par("CX-0630", None) else {
            panic!("pedido do controle");
        };
        j.controle_pedido(p);
        assert_eq!(
            j.controle.mensagem,
            Some(("Par de conta desligado.".to_owned(), false))
        );
    }

    #[test]
    fn o_detalhe_do_salario_traz_a_sugestao_e_os_candidatos_ordenados() {
        let mut j = janela_ligada();
        let (sel, vm) = vm_do_salario(&mut j);
        assert!(sel.sugestao_de_conciliacao, "{sel:?}");
        let sug = vm.sugestao.as_ref().expect("sugestão");
        // Sem o par confirmado, a conta ainda não prova nada.
        assert!(!sug.tres_provas);
        assert!(vm.candidatos.len() >= 2);
        assert!(vm.candidatos[0].bate_assinatura);
        let mut dias: Vec<i64> = Vec::new();
        for c in vm.candidatos.iter().filter(|c| !c.bate_assinatura) {
            dias.push(sel.vencimento.dias_ate(c.movimento.data).abs());
        }
        assert!(dias.windows(2).all(|w| w[0] <= w[1]), "{dias:?}");
    }

    #[test]
    fn com_o_par_confirmado_a_sugestao_do_salario_ganha_tres_provas() {
        let mut j = janela_ligada();
        mapear_salario(&mut j);
        let (_, vm) = vm_do_salario(&mut j);
        let sug = vm.sugestao.expect("sugestão");
        assert!(sug.tres_provas, "{sug:?}");
        assert_eq!(sug.diferenca, None);
        assert_eq!(sug.valor, crate::planejamento_ficticio::LIQUIDO_DO_SALARIO);
    }

    #[test]
    fn confirmar_a_sugestao_vira_conciliacao_confirmada() {
        let mut j = janela_ligada();
        mapear_salario(&mut j);
        selecionar_salario(&mut j);
        j.controle_pedido(PedidoDoControle::Acao(AcaoDoControle::ConfirmarConciliacao));
        let (sel, vm) = vm_do_salario(&mut j);
        assert!(!sel.sugestao_de_conciliacao);
        assert_eq!(
            sel.conciliado,
            Some(crate::planejamento_ficticio::LIQUIDO_DO_SALARIO)
        );
        let m = vm.confirmadas.first().expect("confirmada");
        assert!(m.tres_provas);
        assert!(
            m.descricao
                .as_deref()
                .is_some_and(|d| d.contains("SALARIO"))
        );
    }

    #[test]
    fn conciliar_com_outro_grava_confirmada_e_oferece_a_assinatura_so_sem_ela() {
        let mut j = janela_ligada();
        let (sel, vm) = vm_do_salario(&mut j);
        let outro = vm
            .candidatos
            .iter()
            .find(|c| !c.bate_assinatura)
            .expect("outro")
            .movimento
            .clone();
        let pedido = |descricao: &str| PedidoDoControle::ConciliarManual {
            conciliacao: Conciliacao {
                ocorrencia: sel.id.clone(),
                movimento: outro.movimento.clone(),
                valor: outro.valor,
                data: outro.data,
                confirmada: true,
            },
            descricao: descricao.to_owned(),
        };
        // O salário já tem assinatura: nada a oferecer.
        j.controle_pedido(pedido(&outro.descricao));
        let (sel2, _) = vm_do_salario(&mut j);
        assert_eq!(sel2.conciliado, Some(outro.valor));
        assert!(!sel2.sugestao_de_conciliacao);
        assert_eq!(j.controle.oferta_de_assinatura, None);
        // Sem assinatura, a oferta aparece com o texto do banco.
        let base = {
            let pl = j.planejamento.as_ref().unwrap().0.as_ref();
            pl.recorrente("rec_salario")
                .unwrap()
                .unwrap()
                .versao_do_registro
        };
        j.controle_pedido(PedidoDoControle::AplicarVarias(vec![
            Alteracao::DefinirAssinatura {
                recorrente: "rec_salario".into(),
                assinatura: None,
                versao_base: base,
            },
        ]));
        j.controle_pedido(pedido(&outro.descricao));
        let oferta = j.controle.oferta_de_assinatura.clone().expect("oferta");
        assert_eq!(oferta.recorrente, "rec_salario");
        assert_eq!(oferta.texto, outro.descricao);
        // Aceitar lê a versão na hora (a conciliação não a mexe, mas a edição de antes sim) e
        // grava com janela de 5 dias.
        j.controle_pedido(PedidoDoControle::DefinirAssinatura {
            recorrente: oferta.recorrente.clone(),
            assinatura: Some(oferta.assinatura()),
        });
        assert_eq!(j.controle.oferta_de_assinatura, None);
        let pl = j.planejamento.as_ref().unwrap().0.as_ref();
        assert_eq!(
            pl.assinatura("rec_salario").unwrap(),
            Some(Assinatura {
                texto: outro.descricao,
                janela_dias: 5
            })
        );
    }

    #[test]
    fn buscar_no_extrato_diz_quantas_sugestoes_novas() {
        let mut j = janela_ligada();
        // O salário do mês já nasce sugerido; a busca não repete.
        j.controle_pedido(PedidoDoControle::Acao(AcaoDoControle::BuscarNoExtrato));
        let (texto, erro) = j.controle.mensagem.clone().expect("mensagem");
        assert!(!erro);
        assert!(
            texto == "Nenhuma sugestão nova."
                || texto.ends_with("sugestões novas.")
                || texto == "1 sugestão nova.",
            "{texto}"
        );
        assert!(
            j.controle_disponibilidade(AcaoDoControle::BuscarNoExtrato)
                .is_ok()
        );
    }

    #[test]
    fn buscar_no_extrato_acha_a_sugestao_que_foi_tirada() {
        let mut j = janela_ligada();
        let (sel, _) = vm_do_salario(&mut j);
        // Dispensar a sugestão semeada não é uma Alteração; refaz o fictício sem ela.
        let s = PlanejamentoFicticio::novo(hoje());
        let _ = s.sugestao_de_conciliacao(&sel.id);
        j.ligar_planejamento(Box::new(s));
        j.controle_pedido(PedidoDoControle::Acao(AcaoDoControle::BuscarNoExtrato));
        assert!(!j.controle.mensagem.as_ref().unwrap().1);
    }

    #[test]
    fn editar_o_salario_tem_a_conta_e_a_assinatura() {
        let mut j = janela_ligada();
        selecionar_salario(&mut j);
        let pl = j.planejamento.as_ref().unwrap().0.as_ref();
        let mut d =
            abrir_dialogo(AcaoDoControle::Editar, &j.controle, pl, hoje()).expect("diálogo");
        let Formulario::Editar { conta, .. } = &d.formulario else {
            panic!("formulário de editar");
        };
        assert_eq!(conta, "CX-0630");
        let campo = d.assinatura.clone().expect("campo da assinatura");
        assert_eq!(campo.texto, crate::planejamento_ficticio::TEXTO_DO_SALARIO);
        assert_eq!(campo.janela, "5");
        // Sem mexer em nada, não há o que aplicar.
        assert!(d.pedidos_de_aplicar().is_empty());
        assert!(!d.pode_aplicar());
        // Só a assinatura mudou: só ela é gravada.
        let a = d.assinatura.as_mut().unwrap();
        a.texto = "SALARIO OUTRO".into();
        a.janela = "7".into();
        assert!(d.pode_aplicar());
        let v = d.pedidos_de_aplicar();
        assert_eq!(
            v,
            vec![Pedido::Controle(PedidoDoControle::DefinirAssinatura {
                recorrente: "rec_salario".into(),
                assinatura: Some(Assinatura {
                    texto: "SALARIO OUTRO".into(),
                    janela_dias: 7
                }),
            })]
        );
        // Janela inválida bloqueia o Aplicar.
        d.assinatura.as_mut().unwrap().janela = "0".into();
        d.atualizar_previa(pl, hoje());
        assert!(!d.pode_aplicar());
        // Texto vazio tira a assinatura.
        let a = d.assinatura.as_mut().unwrap();
        a.texto = "  ".into();
        assert_eq!(a.a_gravar(), Some(None));
    }

    #[test]
    fn faixa_das_rubricas_aparece_quando_a_composicao_nao_veio_do_contracheque() {
        let mut j = janela_ligada();
        // A composição semeada vem do contracheque vinculado: sem faixa.
        selecionar_salario(&mut j);
        let dados = j.controle.detalhe.as_ref().unwrap().dados.as_ref().unwrap();
        let docs = contracheques_de(dados);
        assert_eq!(docs.len(), 1);
        let comp = j.controle.selecionada.as_ref().unwrap().competencia;
        assert_eq!(
            movimentos::faixa_do_contracheque(dados.composicao.as_ref(), &docs, comp),
            None
        );
        // Um contracheque novo vinculado, de outro mês, traz rubricas que a composição não usa.
        j.controle_pedido(PedidoDoControle::AplicarVarias(vec![
            Alteracao::VincularDocumento {
                alvo: AlvoDoDocumento::Recorrente {
                    id: "rec_salario".into(),
                },
                sha256: "sha-do-contracheque-novo".into(),
                tipo_documento: sbf_planejamento::TipoDeDocumento::Contracheque,
                nome: "contracheque-2026-12.pdf".into(),
                data: Data::de_iso("2026-12-05"),
            },
        ]));
        selecionar_salario(&mut j);
        let dados = j.controle.detalhe.as_ref().unwrap().dados.as_ref().unwrap();
        let docs = contracheques_de(dados);
        let f = movimentos::faixa_do_contracheque(dados.composicao.as_ref(), &docs, comp)
            .expect("faixa");
        assert_eq!(f.sha256, "sha-do-contracheque-novo");
        assert_eq!(f.mes_ano, "12/2026");
    }

    #[test]
    fn revisar_contracheque_enfileira_no_reconhecimento_e_pede_o_painel() {
        let mut j = janela_ligada();
        // Sem reconhecedor, avisa em vez de calar.
        j.revisar_contracheque("/acervo/contracheque.pdf".into());
        assert_eq!(j.controle.mensagem.as_ref().map(|m| m.1), Some(true));
        assert!(j.reconhecimento.cartoes.is_empty());
        j.ligar_reconhecedor(std::sync::Arc::new(
            crate::reconhecimento_ficticio::reconhecedor_do_contracheque(),
        ));
        j.revisar_contracheque("/acervo/contracheque.pdf".into());
        assert_eq!(j.reconhecimento.cartoes.len(), 1);
        assert!(j.reconhecimento.painel_pedido);
        assert_eq!(j.controle.mensagem.as_ref().map(|m| m.1), Some(false));
        // Pelo pedido, sem inventário ligado, é um aviso (não há cópia do acervo).
        j.controle_pedido(PedidoDoControle::RevisarContracheque("sha".into()));
        assert_eq!(j.controle.mensagem.as_ref().map(|m| m.1), Some(true));
        assert_eq!(j.reconhecimento.cartoes.len(), 1);
    }

    #[test]
    fn a_tela_de_contas_e_o_buscar_estao_na_navegacao_e_na_lista_de_acoes() {
        use crate::controle::acao_do_codigo;
        assert_eq!(
            acao_do_codigo("controle.buscar-no-extrato"),
            Some(AcaoDoControle::BuscarNoExtrato)
        );
        let a = crate::areas::area_futura(crate::contas_ui::ROTA).expect("área");
        assert_eq!(
            crate::areas::grupo_de(&crate::navegacao::Destino::AreaFutura(a.id)),
            "Controle"
        );
    }
}
