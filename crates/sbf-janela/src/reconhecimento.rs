//! Arrastar um arquivo para a janela (spec 11): a fila, a thread do reconhecimento e o que
//! acontece quando o usuário confirma uma sugestão.
//!
//! Aqui mora o estado e a lógica, sem egui desenhando nada (o desenho está em
//! [`crate::reconhecimento_ui`]). Regras:
//!
//! - cada arquivo vira um cartão; o reconhecimento roda **um por vez**, numa thread, e a
//!   interface só consulta o canal a cada quadro;
//! - pasta é recusada; arquivo repetido na fila é ignorado; sem [`Reconhecedor`] ligado, nada
//!   entra na fila e a janela diz isso;
//! - **nada muda no planejamento sem o clique**: [`Janela::confirmar_sugestao`] é o único
//!   caminho que escreve, com `Autor::Usuario`.

use std::collections::{BTreeMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::thread::JoinHandle;

use sbf_planejamento::{
    Alteracao, AlvoDoDocumento, Autor, Conciliacao, Planejamento, TipoDeDocumento,
};
use sbf_reconhecimento::descontos::{self, preservar_ligacoes};
use sbf_reconhecimento::{
    Andamento, ErroDoReconhecimento, Etapa, Reconhecedor, Reconhecido, Resultado, Situacao,
    Sugestao,
};

use crate::controle_ui::mensagem_de_erro;
use crate::janela::{Janela, Mensagem};

/// Em que pé está o cartão de um arquivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SituacaoDoCartao {
    /// Na fila, esperando a vez.
    Aguardando,
    Rodando,
    Pronto(Box<Resultado>),
    /// O reconhecimento parou (o motivo, em português, sem conteúdo do documento).
    Falhou(String),
    Cancelado,
}

/// O cartão de um arquivo solto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cartao {
    pub caminho: PathBuf,
    /// Só o nome do arquivo, sem a pasta.
    pub nome: String,
    /// As seis etapas, na ordem; `None` até a etapa anunciar algo.
    pub etapas: Vec<(Etapa, Option<Situacao>)>,
    pub situacao: SituacaoDoCartao,
    /// Os índices das sugestões já confirmadas.
    pub confirmadas: Vec<usize>,
    /// Os índices das sugestões ignoradas.
    pub ignoradas: Vec<usize>,
    /// Os índices das conciliações que a janela fez sozinha (as três provas, `Autor::Regra`):
    /// aparecem como "Conciliado automaticamente · Desfazer". Também estão em `confirmadas`.
    pub automaticas: Vec<usize>,
    /// O erro da última tentativa de confirmar (índice da sugestão, motivo).
    pub erro_de_sugestao: Option<(usize, String)>,
    /// A marcação de eventuais que a pessoa deixou em cada sugestão de rubricas (índice da
    /// sugestão): `true` = só deste mês. Quem não mexeu não aparece: vale a inicial.
    pub marcacoes: BTreeMap<usize, Vec<bool>>,
}

impl Cartao {
    fn novo(caminho: PathBuf) -> Self {
        let nome = caminho.file_name().map_or_else(
            || "arquivo".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        );
        Self {
            caminho,
            nome,
            etapas: Etapa::TODAS.iter().map(|e| (*e, None)).collect(),
            situacao: SituacaoDoCartao::Aguardando,
            confirmadas: Vec::new(),
            ignoradas: Vec::new(),
            automaticas: Vec::new(),
            erro_de_sugestao: None,
            marcacoes: BTreeMap::new(),
        }
    }

    /// A marcação de eventuais da sugestão de rubricas `sugestao`: a da pessoa, ou a inicial.
    pub fn eventuais_da(&self, sugestao: usize) -> Option<Vec<bool>> {
        let Sugestao::UsarRubricas { uso, .. } = self.resultado()?.sugestoes.get(sugestao)? else {
            return None;
        };
        Some(
            self.marcacoes
                .get(&sugestao)
                .cloned()
                .unwrap_or_else(|| uso.eventuais_iniciais()),
        )
    }

    /// O cartão ainda ocupa a fila (espera ou roda).
    pub fn em_andamento(&self) -> bool {
        matches!(
            self.situacao,
            SituacaoDoCartao::Aguardando | SituacaoDoCartao::Rodando
        )
    }

    /// A situação da etapa, se ela já anunciou alguma.
    pub fn situacao_da_etapa(&self, etapa: Etapa) -> Option<&Situacao> {
        self.etapas
            .iter()
            .find(|(e, _)| *e == etapa)
            .and_then(|(_, s)| s.as_ref())
    }

    /// O resultado, quando o reconhecimento terminou.
    pub fn resultado(&self) -> Option<&Resultado> {
        match &self.situacao {
            SituacaoDoCartao::Pronto(r) => Some(r),
            _ => None,
        }
    }
}

enum Aviso {
    Andamento(Andamento),
    Fim(Box<Result<Resultado, ErroDoReconhecimento>>),
}

struct Execucao {
    indice: usize,
    canal: Receiver<Aviso>,
    cancelar: Arc<AtomicBool>,
    // Guardada só para a thread não ficar solta; ela termina sozinha.
    _thread: JoinHandle<()>,
}

impl std::fmt::Debug for Execucao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Execucao")
            .field("indice", &self.indice)
            .finish_non_exhaustive()
    }
}

/// Liga o registro do contracheque no inventário ao crédito do banco depois de uma
/// conciliação confirmada (spec 12, D1; `sbf_inventario::salario::ligar_salario_ao_credito`).
pub trait LigadorDeSalario: Send + Sync + std::fmt::Debug {
    /// `Ok(false)` quando falta o registro do contracheque ou o do crédito.
    ///
    /// # Errors
    /// O motivo, sem conteúdo do documento.
    fn ligar(&self, sha256: &str, movimento: &str) -> Result<bool, String>;
}

/// O ligador real: abre o `inventario.sqlite` das Ferramentas.
#[derive(Debug, Clone)]
pub struct LigadorDoInventario {
    pub banco: PathBuf,
}

impl LigadorDeSalario for LigadorDoInventario {
    fn ligar(&self, sha256: &str, movimento: &str) -> Result<bool, String> {
        let inv =
            sbf_inventario::banco::Inventario::abrir(&self.banco).map_err(|e| e.to_string())?;
        sbf_inventario::salario::ligar_salario_ao_credito(inv.conexao(), sha256, movimento)
    }
}

/// O texto do aviso discreto quando o elo no inventário falha (a conciliação fica).
pub const AVISO_DO_ELO: &str =
    "A conciliação ficou, mas não deu para ligar o registro do salário ao crédito no inventário.";

/// O estado do arrastar e do reconhecimento na janela.
#[derive(Debug, Default)]
pub struct Reconhecimento {
    reconhecedor: Option<Arc<dyn Reconhecedor>>,
    /// Quem liga o registro do salário ao crédito; sem ele, o das Ferramentas (se houver).
    pub ligador: Option<Arc<dyn LigadorDeSalario>>,
    pub cartoes: Vec<Cartao>,
    fila: VecDeque<usize>,
    execucao: Option<Execucao>,
    /// Há arquivo pairando sobre a janela: a camada "Solte para reconhecer".
    pub pairando: bool,
    /// O painel deve aparecer (ao soltar); o app consome isto uma vez.
    pub painel_pedido: bool,
    /// Rola a lista de cartões até este ponto no próximo quadro (as capturas usam, para
    /// mostrar as rubricas); a interface consome uma vez.
    pub rolar_para: Option<f32>,
}

impl Reconhecimento {
    /// Há reconhecedor ligado.
    pub fn ligado(&self) -> bool {
        self.reconhecedor.is_some()
    }

    /// Alguma coisa ainda roda ou espera.
    pub fn ocupado(&self) -> bool {
        self.execucao.is_some() || !self.fila.is_empty()
    }
}

/// O texto quando a janela não tem reconhecedor.
pub const NAO_LIGADO: &str = "O reconhecimento não está ligado nesta janela";

impl Janela {
    /// Liga o reconhecedor (o real, do motor, ou o fictício da demonstração).
    pub fn ligar_reconhecedor(&mut self, reconhecedor: Arc<dyn Reconhecedor>) {
        self.reconhecimento.reconhecedor = Some(reconhecedor);
    }

    /// Arquivos soltos na janela. `ctx` acorda a interface quando a thread anuncia algo.
    pub fn soltar_arquivos(&mut self, caminhos: Vec<PathBuf>, ctx: Option<&egui::Context>) {
        if caminhos.is_empty() {
            return;
        }
        if !self.reconhecimento.ligado() {
            self.mensagem = Some(Mensagem {
                texto: format!("{NAO_LIGADO}."),
                erro: true,
            });
            return;
        }
        let mut pastas = 0usize;
        let mut aceitos = 0usize;
        for caminho in caminhos {
            if caminho.is_dir() {
                pastas += 1;
                continue;
            }
            let repetido = self
                .reconhecimento
                .cartoes
                .iter()
                .any(|c| c.em_andamento() && c.caminho == caminho);
            if repetido {
                continue;
            }
            self.reconhecimento.cartoes.push(Cartao::novo(caminho));
            let indice = self.reconhecimento.cartoes.len() - 1;
            self.reconhecimento.fila.push_back(indice);
            aceitos += 1;
        }
        if pastas > 0 {
            self.mensagem = Some(Mensagem {
                texto: if pastas == 1 {
                    "Pasta não vale: solte arquivos, não pastas.".to_owned()
                } else {
                    format!("{pastas} pastas não valem: solte arquivos, não pastas.")
                },
                erro: true,
            });
        }
        if aceitos > 0 {
            self.reconhecimento.painel_pedido = true;
        }
        self.tique_do_reconhecimento(ctx);
    }

    /// Lê o canal da thread, aplica o que chegou e começa o próximo da fila. Devolve `true`
    /// enquanto ainda há trabalho (quem desenha pede novo quadro).
    pub fn tique_do_reconhecimento(&mut self, ctx: Option<&egui::Context>) -> bool {
        if let Some(exec) = self.reconhecimento.execucao.as_ref() {
            let indice = exec.indice;
            let mut terminou = false;
            let mut chegou = false;
            loop {
                match exec.canal.try_recv() {
                    Ok(Aviso::Andamento(a)) => {
                        if let Some(c) = self.reconhecimento.cartoes.get_mut(indice) {
                            if let Some(slot) = c.etapas.iter_mut().find(|(e, _)| *e == a.etapa) {
                                slot.1 = Some(a.situacao);
                            }
                            c.situacao = SituacaoDoCartao::Rodando;
                        }
                    }
                    Ok(Aviso::Fim(r)) => {
                        let r = *r;
                        chegou = r.is_ok();
                        if let Some(c) = self.reconhecimento.cartoes.get_mut(indice) {
                            c.situacao = match r {
                                Ok(resultado) => SituacaoDoCartao::Pronto(Box::new(resultado)),
                                Err(ErroDoReconhecimento::Cancelado) => SituacaoDoCartao::Cancelado,
                                Err(e) => SituacaoDoCartao::Falhou(e.to_string()),
                            };
                        }
                        terminou = true;
                        break;
                    }
                    Err(TryRecvError::Empty) => break,
                    Err(TryRecvError::Disconnected) => {
                        if let Some(c) = self.reconhecimento.cartoes.get_mut(indice) {
                            c.situacao = SituacaoDoCartao::Falhou(
                                "O reconhecimento parou de repente.".to_owned(),
                            );
                        }
                        terminou = true;
                        break;
                    }
                }
            }
            if terminou {
                self.reconhecimento.execucao = None;
            }
            if chegou {
                self.aplicar_automaticas(indice);
            }
        }
        if self.reconhecimento.execucao.is_none() {
            self.iniciar_o_proximo(ctx);
        }
        self.reconhecimento.ocupado()
    }

    fn iniciar_o_proximo(&mut self, ctx: Option<&egui::Context>) {
        let Some(reconhecedor) = self.reconhecimento.reconhecedor.clone() else {
            return;
        };
        while let Some(indice) = self.reconhecimento.fila.pop_front() {
            let Some(cartao) = self.reconhecimento.cartoes.get_mut(indice) else {
                continue;
            };
            // Cancelado na espera: pula.
            if cartao.situacao != SituacaoDoCartao::Aguardando {
                continue;
            }
            cartao.situacao = SituacaoDoCartao::Rodando;
            let caminho = cartao.caminho.clone();
            let (envio, canal) = channel();
            let cancelar = Arc::new(AtomicBool::new(false));
            let da_thread = Arc::clone(&cancelar);
            let ctx = ctx.cloned();
            let acordar = move || {
                if let Some(c) = &ctx {
                    c.request_repaint();
                }
            };
            let thread = std::thread::spawn(move || {
                let envio_do_andamento = envio.clone();
                let acordar_andamento = acordar.clone();
                let r = reconhecedor.reconhecer(
                    &caminho,
                    &mut |a| {
                        let _ = envio_do_andamento.send(Aviso::Andamento(a));
                        acordar_andamento();
                    },
                    &da_thread,
                );
                let _ = envio.send(Aviso::Fim(Box::new(r)));
                acordar();
            });
            self.reconhecimento.execucao = Some(Execucao {
                indice,
                canal,
                cancelar,
                _thread: thread,
            });
            return;
        }
    }

    /// Cancela o arquivo `indice`: na espera, sai da fila; rodando, a thread é avisada e o
    /// reconhecedor para entre etapas.
    pub fn cancelar_reconhecimento(&mut self, indice: usize) {
        let Some(c) = self.reconhecimento.cartoes.get_mut(indice) else {
            return;
        };
        match c.situacao {
            SituacaoDoCartao::Aguardando => c.situacao = SituacaoDoCartao::Cancelado,
            SituacaoDoCartao::Rodando => {
                if let Some(e) = self
                    .reconhecimento
                    .execucao
                    .as_ref()
                    .filter(|e| e.indice == indice)
                {
                    e.cancelar.store(true, Ordering::Relaxed);
                }
            }
            _ => {}
        }
    }

    /// Tira da lista os cartões que já terminaram (os que esperam ou rodam ficam).
    pub fn limpar_reconhecimento(&mut self) {
        let r = &mut self.reconhecimento;
        // Os índices da fila e da execução mudam quando a lista encolhe: só limpa sem fila.
        if r.execucao.is_some() || !r.fila.is_empty() {
            return;
        }
        r.cartoes.clear();
    }

    /// Ignorar uma sugestão: some o botão, nada muda no planejamento.
    pub fn ignorar_sugestao(&mut self, cartao: usize, sugestao: usize) {
        if let Some(c) = self.reconhecimento.cartoes.get_mut(cartao)
            && !c.ignoradas.contains(&sugestao)
            && !c.confirmadas.contains(&sugestao)
        {
            c.ignoradas.push(sugestao);
        }
    }

    /// Troca a rubrica `rubrica` da sugestão de rubricas entre "fixa" e "só deste mês".
    /// Só vale antes de confirmar; nada é gravado.
    pub fn alternar_eventual(&mut self, cartao: usize, sugestao: usize, rubrica: usize) {
        let Some(c) = self.reconhecimento.cartoes.get_mut(cartao) else {
            return;
        };
        if c.confirmadas.contains(&sugestao) {
            return;
        }
        let Some(mut marcacao) = c.eventuais_da(sugestao) else {
            return;
        };
        let Some(m) = marcacao.get_mut(rubrica) else {
            return;
        };
        *m = !*m;
        c.marcacoes.insert(sugestao, marcacao);
    }

    /// Quem liga o registro do salário ao crédito: o da janela ou, no inventário real, o das
    /// Ferramentas.
    pub(crate) fn ligador_de_salario(&self) -> Option<Arc<dyn LigadorDeSalario>> {
        self.reconhecimento.ligador.clone().or_else(|| {
            self.ferramentas.as_ref().map(|f| {
                Arc::new(LigadorDoInventario {
                    banco: f.banco.clone(),
                }) as Arc<dyn LigadorDeSalario>
            })
        })
    }

    /// As conciliações das três provas que o reconhecimento achou: a janela as grava sozinha
    /// (`Autor::Regra`), exceto o par que já foi desfeito alguma vez. É a única escrita sem
    /// clique (AC-07), e o cartão mostra "Conciliado automaticamente · Desfazer".
    fn aplicar_automaticas(&mut self, cartao: usize) {
        let ligador = self.ligador_de_salario();
        let Some(c) = self.reconhecimento.cartoes.get_mut(cartao) else {
            return;
        };
        let Some(resultado) = c.resultado() else {
            return;
        };
        let sha = resultado.sha256.clone();
        let automaticas: Vec<(usize, Conciliacao)> = resultado
            .sugestoes
            .iter()
            .enumerate()
            .filter_map(|(i, s)| match s {
                Sugestao::Conciliar { conciliacao, .. } if s.e_automatica() => {
                    Some((i, conciliacao.clone()))
                }
                _ => None,
            })
            .collect();
        if automaticas.is_empty() {
            return;
        }
        let Some(pl) = self.planejamento.as_mut() else {
            return;
        };
        let plan = pl.0.as_mut();
        let mut aviso = None;
        let mut feitas = 0usize;
        for (i, conciliacao) in automaticas {
            // O usuário já disse "não" a este par: a regra não o refaz.
            if plan
                .conciliacao_desfeita(&conciliacao.ocorrencia, &conciliacao.movimento)
                .unwrap_or(false)
            {
                continue;
            }
            let confirmada = Conciliacao {
                confirmada: true,
                ..conciliacao
            };
            let ja = plan
                .conciliacoes_da_ocorrencia(&confirmada.ocorrencia)
                .map(|v| {
                    v.iter().any(|g| {
                        g.conciliacao.confirmada && g.conciliacao.movimento == confirmada.movimento
                    })
                })
                .unwrap_or(false);
            if !ja {
                let r = plan.aplicar(
                    Alteracao::Conciliar {
                        conciliacao: confirmada.clone(),
                    },
                    Autor::Regra {
                        regra: "tres-provas".to_owned(),
                    },
                );
                if let Err(e) = r {
                    c.erro_de_sugestao = Some((i, mensagem_de_erro(&e)));
                    continue;
                }
            }
            c.confirmadas.push(i);
            c.automaticas.push(i);
            feitas += 1;
            aviso = aviso.or(ligar_ao_credito(ligador.as_deref(), &sha, &confirmada));
        }
        if feitas > 0 {
            self.controle.recarregar();
            self.controle.mensagem = Some((
                aviso.map_or_else(
                    || {
                        "Conciliado automaticamente: as três provas batem. Dá para desfazer."
                            .to_owned()
                    },
                    str::to_owned,
                ),
                false,
            ));
        }
    }

    /// Desfaz a conciliação que a regra fez sozinha: exclusão lógica, com histórico. A sugestão
    /// volta ao cartão para ser confirmada à mão, e a regra não a refaz.
    pub fn desfazer_automatica(&mut self, cartao: usize, sugestao: usize) {
        let Some(c) = self.reconhecimento.cartoes.get_mut(cartao) else {
            return;
        };
        if !c.automaticas.contains(&sugestao) {
            return;
        }
        let Some(Sugestao::Conciliar { conciliacao, .. }) = c
            .resultado()
            .and_then(|r| r.sugestoes.get(sugestao))
            .cloned()
        else {
            return;
        };
        let Some(pl) = self.planejamento.as_mut() else {
            return;
        };
        let r = pl.0.as_mut().aplicar(
            Alteracao::Desconciliar {
                ocorrencia: conciliacao.ocorrencia,
                movimento: conciliacao.movimento,
            },
            Autor::Usuario,
        );
        self.controle.recarregar();
        match r {
            Ok(_) => {
                c.confirmadas.retain(|i| *i != sugestao);
                c.automaticas.retain(|i| *i != sugestao);
                self.controle.mensagem = Some((
                    "Conciliação desfeita. Ela volta a ser uma sugestão.".to_owned(),
                    false,
                ));
            }
            Err(e) => c.erro_de_sugestao = Some((sugestao, mensagem_de_erro(&e))),
        }
    }

    /// Confirmar uma sugestão: o único caminho (com as três provas) que escreve no
    /// planejamento (`Autor::Usuario`).
    /// Confirmar de novo a mesma sugestão não faz nada. As rubricas aplicam uma composição
    /// por vez, cada uma com a versão do recorrente lida logo antes.
    pub fn confirmar_sugestao(&mut self, cartao: usize, sugestao: usize) {
        let ligador = self.ligador_de_salario();
        let Some(c) = self.reconhecimento.cartoes.get_mut(cartao) else {
            return;
        };
        if c.confirmadas.contains(&sugestao) {
            return;
        }
        c.erro_de_sugestao = None;
        let marcacao = c.eventuais_da(sugestao);
        let marcacao_das_rubricas = c
            .resultado()
            .and_then(|r| {
                r.sugestoes
                    .iter()
                    .position(|s| matches!(s, Sugestao::UsarRubricas { .. }))
            })
            .and_then(|i| c.eventuais_da(i));
        let Some(resultado) = c.resultado() else {
            return;
        };
        let Some(s) = resultado.sugestoes.get(sugestao).cloned() else {
            return;
        };
        let (sha, nome) = (resultado.sha256.clone(), resultado.nome.clone());
        let Some(pl) = self.planejamento.as_mut() else {
            c.erro_de_sugestao = Some((
                sugestao,
                "O planejamento ainda não está ligado a esta janela.".to_owned(),
            ));
            return;
        };
        let plan = pl.0.as_mut();
        let mut resultado_final = Ok(());
        if let Sugestao::CriarDescontosEmFolha {
            uso, descontos: ds, ..
        } = &s
        {
            // As rubricas ligadas seguem a marcação de fixas e eventuais que a pessoa deixou.
            let m = marcacao_das_rubricas.unwrap_or_else(|| uso.eventuais_iniciais());
            if let Err(e) = descontos::aplicar_descontos(plan, uso, ds, &m) {
                resultado_final = Err(mensagem_de_erro(&e));
            }
        } else {
            for mut alteracao in s.alteracoes(&sha, &nome, 0, marcacao.as_deref()) {
                // A versão do recorrente lida agora: se mudou desde o reconhecimento, o
                // conflito aparece no cartão em vez de sobrescrever.
                if let (Alteracao::DefinirComposicao { versao_base, .. }, Some(id)) =
                    (&mut alteracao, s.recorrente_para_versao())
                {
                    match plan.recorrente(id) {
                        Ok(Some(r)) => *versao_base = r.versao_do_registro,
                        Ok(None) => *versao_base = 0,
                        Err(e) => {
                            resultado_final = Err(mensagem_de_erro(&e));
                            break;
                        }
                    }
                }
                // Usar as rubricas de novo não desliga o que "criar compromissos" ligou.
                if let Alteracao::DefinirComposicao { composicao, .. } = &mut alteracao
                    && let Ok(Some(anterior)) =
                        plan.composicao(&composicao.recorrente, composicao.desde)
                    && anterior.desde == composicao.desde
                {
                    preservar_ligacoes(composicao, &anterior);
                }
                if let Err(e) = plan.aplicar(alteracao, Autor::Usuario) {
                    resultado_final = Err(mensagem_de_erro(&e));
                    break;
                }
            }
        }
        let mut aviso = None;
        if resultado_final.is_ok() {
            if let Sugestao::Conciliar { conciliacao, .. } = &s {
                aviso = ligar_ao_credito(
                    ligador.as_deref(),
                    &sha,
                    &Conciliacao {
                        confirmada: true,
                        ..conciliacao.clone()
                    },
                );
            }
            if matches!(
                s,
                Sugestao::UsarRubricas { .. } | Sugestao::CriarDescontosEmFolha { .. }
            ) {
                atualizar_baixa(c, plan);
            }
        }
        self.controle.recarregar();
        match resultado_final {
            Ok(()) => {
                c.confirmadas.push(sugestao);
                self.controle.mensagem = Some((
                    aviso.map_or_else(|| format!("Feito: {}.", s.rotulo()), str::to_owned),
                    false,
                ));
            }
            Err(e) => c.erro_de_sugestao = Some((sugestao, e)),
        }
    }
}

/// Depois de ligar as rubricas aos compromissos, as baixas passam a existir: o cartão ganha
/// (ou renova) "Dar baixa nos N descontos em folha".
fn atualizar_baixa(c: &mut Cartao, plan: &dyn Planejamento) {
    let Some((sha, pagamento)) = c.resultado().and_then(|r| match &r.reconhecido {
        Reconhecido::Contracheque { contracheque, .. } => {
            Some((r.sha256.clone(), contracheque.pagamento))
        }
        _ => None,
    }) else {
        return;
    };
    let Ok(nova) = descontos::sugestao_de_baixa(plan, pagamento, &sha) else {
        return;
    };
    let SituacaoDoCartao::Pronto(r) = &mut c.situacao else {
        return;
    };
    let existente = r
        .sugestoes
        .iter()
        .position(|s| matches!(s, Sugestao::DarBaixa { .. }));
    match (existente, nova) {
        (Some(i), Some(n)) if !c.confirmadas.contains(&i) => r.sugestoes[i] = n,
        (None, Some(n)) => r.sugestoes.push(n),
        _ => {}
    }
}

/// O elo do registro do salário com o crédito, depois de uma conciliação confirmada.
/// Devolve o aviso discreto quando falha (a conciliação não se desfaz).
pub(crate) fn ligar_ao_credito(
    ligador: Option<&dyn LigadorDeSalario>,
    sha256: &str,
    conciliacao: &Conciliacao,
) -> Option<&'static str> {
    if !conciliacao.confirmada || conciliacao.movimento.starts_with("contracheque:") {
        return None;
    }
    match ligador?.ligar(sha256, &conciliacao.movimento) {
        Ok(_) => None,
        Err(_) => Some(AVISO_DO_ELO),
    }
}

/// O contracheque vinculado à ocorrência (ou ao compromisso), se houver: o da composição
/// vigente, senão o mais recente.
pub(crate) fn contracheque_da_ocorrencia(
    plan: &dyn Planejamento,
    id: &sbf_planejamento::IdOcorrencia,
) -> Option<String> {
    // Os da ocorrência e os do compromisso inteiro (o contracheque costuma estar ligado à regra).
    let mut docs = plan
        .documentos(&AlvoDoDocumento::Ocorrencia { id: id.clone() })
        .ok()?;
    for d in plan
        .documentos(&AlvoDoDocumento::Recorrente {
            id: id.recorrente.clone(),
        })
        .ok()?
    {
        if docs.iter().all(|x| x.id != d.id) {
            docs.push(d);
        }
    }
    let contracheques: Vec<_> = docs
        .iter()
        .filter(|d| d.tipo == TipoDeDocumento::Contracheque && !d.desvinculado)
        .collect();
    let da_composicao = plan
        .composicao(&id.recorrente, id.competencia)
        .ok()
        .flatten()
        .and_then(|k| k.documento);
    da_composicao
        .filter(|sha| contracheques.iter().any(|d| &d.sha256 == sha))
        .or_else(|| {
            contracheques
                .iter()
                .max_by_key(|d| d.data)
                .map(|d| d.sha256.clone())
        })
}

#[cfg(test)]
mod testes {
    use super::*;
    use crate::planejamento_ficticio::{HOJE_DA_DEMONSTRACAO, PlanejamentoFicticio};
    use crate::reconhecimento_ficticio::{ID_DO_SALARIO, reconhecedor_do_contracheque};
    use sbf_planejamento::{AlvoDoDocumento, Competencia, Data, IdOcorrencia};
    use sbf_reconhecimento::{ReconhecedorFicticio, Reconhecido, Sugestao};
    use std::time::{Duration, Instant};

    fn hoje() -> Data {
        Data::de_iso(HOJE_DA_DEMONSTRACAO).expect("data")
    }

    fn janela_com_tudo() -> (Janela, Arc<ReconhecedorFicticio>) {
        let mut j = Janela::nova();
        j.hoje = HOJE_DA_DEMONSTRACAO.to_owned();
        j.ligar_planejamento(Box::new(PlanejamentoFicticio::novo(hoje())));
        let r = Arc::new(reconhecedor_do_contracheque());
        j.ligar_reconhecedor(r.clone());
        (j, r)
    }

    fn esperar(j: &mut Janela) {
        let limite = Instant::now() + Duration::from_secs(10);
        while j.tique_do_reconhecimento(None) {
            assert!(Instant::now() < limite, "o reconhecimento não terminou");
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    fn arquivo(dir: &tempfile::TempDir, nome: &str) -> PathBuf {
        let p = dir.path().join(nome);
        std::fs::write(&p, b"ficticio").expect("escrever");
        p
    }

    fn alvo_de_outubro() -> AlvoDoDocumento {
        AlvoDoDocumento::Ocorrencia {
            id: IdOcorrencia {
                recorrente: ID_DO_SALARIO.to_owned(),
                competencia: Competencia { ano: 2026, mes: 10 },
                parcela: None,
            },
        }
    }

    fn documentos_do_alvo(j: &Janela) -> usize {
        j.planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .documentos(&alvo_de_outubro())
            .expect("documentos")
            .iter()
            .filter(|d| !d.desvinculado)
            .count()
    }

    #[test]
    fn soltar_passa_pela_fila_pelas_etapas_e_chega_ao_resultado() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        let a = arquivo(&dir, "contracheque.pdf");
        j.soltar_arquivos(vec![a.clone()], None);
        assert!(j.reconhecimento.painel_pedido);
        assert_eq!(j.reconhecimento.cartoes.len(), 1);
        esperar(&mut j);
        let c = &j.reconhecimento.cartoes[0];
        assert_eq!(c.nome, "contracheque.pdf");
        for etapa in Etapa::TODAS {
            assert!(
                matches!(
                    c.situacao_da_etapa(etapa),
                    Some(Situacao::Feita | Situacao::Pulada(_))
                ),
                "etapa sem resposta: {etapa:?}"
            );
        }
        let res = c.resultado().expect("resultado");
        assert!(matches!(res.reconhecido, Reconhecido::Contracheque { .. }));
        assert_eq!(res.sugestoes.len(), 4);
        assert_eq!(r.recebidos.lock().expect("trava").as_slice(), &[a]);
    }

    #[test]
    fn a_fila_e_sequencial_e_ignora_repetido() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        let a = arquivo(&dir, "a.pdf");
        let b = arquivo(&dir, "b.pdf");
        j.soltar_arquivos(vec![a.clone(), b.clone(), a.clone()], None);
        assert_eq!(j.reconhecimento.cartoes.len(), 2);
        esperar(&mut j);
        assert!(
            j.reconhecimento
                .cartoes
                .iter()
                .all(|c| c.resultado().is_some())
        );
        assert_eq!(r.recebidos.lock().expect("trava").as_slice(), &[a, b]);
    }

    #[test]
    fn pasta_e_recusada_com_mensagem() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![dir.path().to_path_buf()], None);
        assert!(j.reconhecimento.cartoes.is_empty());
        assert!(!j.reconhecimento.painel_pedido);
        let m = j.mensagem.as_ref().expect("mensagem");
        assert!(m.erro && m.texto.contains("Pasta"));
        assert!(r.recebidos.lock().expect("trava").is_empty());
    }

    #[test]
    fn sem_reconhecedor_diz_que_nao_esta_ligado() {
        let mut j = Janela::nova();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "a.pdf")], None);
        assert!(j.reconhecimento.cartoes.is_empty());
        let m = j.mensagem.as_ref().expect("mensagem");
        assert!(m.erro && m.texto.starts_with(NAO_LIGADO));
    }

    #[test]
    fn confirmar_vincular_cria_o_documento_no_alvo_e_duas_vezes_nao_duplica() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let antes = documentos_do_alvo(&j);
        let i = j.reconhecimento.cartoes[0]
            .resultado()
            .expect("resultado")
            .sugestoes
            .iter()
            .position(|s| matches!(s, Sugestao::Vincular { .. }))
            .expect("vincular");
        j.confirmar_sugestao(0, i);
        assert_eq!(documentos_do_alvo(&j), antes + 1);
        assert!(j.reconhecimento.cartoes[0].confirmadas.contains(&i));
        j.confirmar_sugestao(0, i);
        assert_eq!(documentos_do_alvo(&j), antes + 1);
        assert!(j.reconhecimento.cartoes[0].erro_de_sugestao.is_none());
    }

    fn indice_de_rubricas(j: &Janela) -> usize {
        j.reconhecimento.cartoes[0]
            .resultado()
            .expect("resultado")
            .sugestoes
            .iter()
            .position(|s| matches!(s, Sugestao::UsarRubricas { .. }))
            .expect("usar rubricas")
    }

    fn uso_do_cartao(j: &Janela) -> sbf_reconhecimento::UsoDeRubricas {
        match &j.reconhecimento.cartoes[0]
            .resultado()
            .expect("r")
            .sugestoes[indice_de_rubricas(j)]
        {
            Sugestao::UsarRubricas { uso, .. } => (**uso).clone(),
            _ => unreachable!(),
        }
    }

    fn composicao_em(j: &Janela, desde: Competencia) -> Option<sbf_planejamento::Composicao> {
        j.planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .composicao(ID_DO_SALARIO, desde)
            .expect("composição")
    }

    #[test]
    fn confirmar_usar_rubricas_define_as_duas_composicoes() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let uso = uso_do_cartao(&j);
        let esperadas = uso.composicoes(&uso.eventuais_iniciais());
        assert_eq!(esperadas.len(), 2);
        let i = indice_de_rubricas(&j);
        j.confirmar_sugestao(0, i);
        assert_eq!(
            j.reconhecimento.cartoes[0].erro_de_sugestao, None,
            "a confirmação falhou"
        );
        for e in &esperadas {
            let gravada = composicao_em(&j, e.desde).expect("existe");
            assert_eq!(gravada.desde, e.desde);
            assert_eq!(gravada.rubricas, e.rubricas);
            assert_eq!(gravada.documento, e.documento);
        }
    }

    #[test]
    fn trocar_a_marcacao_antes_de_confirmar_muda_o_que_e_gravado() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let uso = uso_do_cartao(&j);
        let i = indice_de_rubricas(&j);
        let inicial = uso.eventuais_iniciais();
        let fixas_antes = uso.composicoes(&inicial)[1].rubricas.len();
        // O prêmio (eventual) passa a fixo.
        let premio = uso
            .contracheque
            .rubricas
            .iter()
            .position(|r| r.nome == "PREMIO CAIXA")
            .expect("prêmio");
        assert!(inicial[premio]);
        j.alternar_eventual(0, i, premio);
        assert!(
            !j.reconhecimento.cartoes[0]
                .eventuais_da(i)
                .expect("marcação")[premio]
        );
        j.confirmar_sugestao(0, i);
        assert_eq!(j.reconhecimento.cartoes[0].erro_de_sugestao, None);
        let fixas = composicao_em(&j, Competencia { ano: 2026, mes: 11 }).expect("fixas");
        assert_eq!(fixas.rubricas.len(), fixas_antes + 1);
        assert!(fixas.rubricas.iter().any(|r| r.nome == "PREMIO CAIXA"));
    }

    #[test]
    fn alternar_nao_grava_nada_e_nao_vale_depois_de_confirmar() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let i = indice_de_rubricas(&j);
        let antes = composicao_em(&j, Competencia { ano: 2026, mes: 10 });
        j.alternar_eventual(0, i, 0);
        assert_eq!(composicao_em(&j, Competencia { ano: 2026, mes: 10 }), antes);
        j.confirmar_sugestao(0, i);
        let marcacoes = j.reconhecimento.cartoes[0].marcacoes.clone();
        j.alternar_eventual(0, i, 1);
        assert_eq!(j.reconhecimento.cartoes[0].marcacoes, marcacoes);
    }

    #[test]
    fn confirmar_conciliar_grava_a_conciliacao_confirmada_do_mes() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let i = j.reconhecimento.cartoes[0]
            .resultado()
            .expect("resultado")
            .sugestoes
            .iter()
            .position(|s| matches!(s, Sugestao::Conciliar { .. }))
            .expect("conciliar");
        j.confirmar_sugestao(0, i);
        assert_eq!(j.reconhecimento.cartoes[0].erro_de_sugestao, None);
        assert!(j.reconhecimento.cartoes[0].confirmadas.contains(&i));
        let o = j
            .planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .proximas(ID_DO_SALARIO, Competencia { ano: 2026, mes: 10 }, 1, hoje())
            .expect("proximas");
        assert_eq!(o[0].conciliado, Some(sbf_planejamento::Centavos(2_460_522)));
        let c = j
            .planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .sugestao_de_conciliacao(&o[0].id)
            .expect("sugestao");
        assert!(c.is_none(), "a conciliação confirmada não é mais sugestão");
    }

    #[test]
    fn conflito_de_versao_aparece_no_cartao_e_nao_marca_feita() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        // A sugestão traz um recorrente que não existe: a escrita é recusada.
        if let SituacaoDoCartao::Pronto(r) = &mut j.reconhecimento.cartoes[0].situacao {
            for s in &mut r.sugestoes {
                if let Sugestao::UsarRubricas { uso, .. } = s {
                    uso.recorrente = "rec_que_nao_existe".to_owned();
                }
            }
        }
        let i = indice_de_rubricas(&j);
        j.confirmar_sugestao(0, i);
        let c = &j.reconhecimento.cartoes[0];
        assert!(!c.confirmadas.contains(&i));
        assert!(matches!(&c.erro_de_sugestao, Some((k, m)) if *k == i && !m.is_empty()));
    }

    #[test]
    fn ignorar_nao_escreve_nada() {
        let (mut j, _) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let antes = documentos_do_alvo(&j);
        j.ignorar_sugestao(0, 0);
        assert_eq!(documentos_do_alvo(&j), antes);
        assert_eq!(j.reconhecimento.cartoes[0].ignoradas, vec![0]);
        // Ignorada, a confirmação por engano ainda é possível via API, mas a interface
        // esconde os botões; o que importa: ignorar sozinho nunca grava.
    }

    #[test]
    fn falha_do_reconhecedor_vira_cartao_falhou() {
        let mut j = Janela::nova();
        j.ligar_reconhecedor(Arc::new(ReconhecedorFicticio::novo(Err(
            ErroDoReconhecimento::Arquivo("sumiu".to_owned()),
        ))));
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "a.pdf")], None);
        esperar(&mut j);
        assert!(matches!(
            &j.reconhecimento.cartoes[0].situacao,
            SituacaoDoCartao::Falhou(m) if m.contains("sumiu")
        ));
    }

    #[test]
    fn cancelar_na_espera_tira_da_fila() {
        let (mut j, r) = janela_com_tudo();
        let dir = tempfile::tempdir().expect("dir");
        let (a, b) = (arquivo(&dir, "a.pdf"), arquivo(&dir, "b.pdf"));
        j.soltar_arquivos(vec![a, b.clone()], None);
        // O segundo ainda espera (a thread do primeiro pode terminar a qualquer momento, mas
        // o tique só avança quando chamado).
        j.cancelar_reconhecimento(1);
        esperar(&mut j);
        assert_eq!(
            j.reconhecimento.cartoes[1].situacao,
            SituacaoDoCartao::Cancelado
        );
        assert!(!r.recebidos.lock().expect("trava").contains(&b));
    }

    /// Um ligador que anota as chamadas e responde o que se mandar.
    #[derive(Debug, Default)]
    struct LigadorFalso {
        chamadas: std::sync::Mutex<Vec<(String, String)>>,
        falha: bool,
    }

    impl LigadorDeSalario for LigadorFalso {
        fn ligar(&self, sha256: &str, movimento: &str) -> Result<bool, String> {
            self.chamadas
                .lock()
                .expect("trava")
                .push((sha256.to_owned(), movimento.to_owned()));
            if self.falha {
                Err("banco ocupado".to_owned())
            } else {
                Ok(true)
            }
        }
    }

    fn janela_com_ligador(falha: bool) -> (Janela, Arc<LigadorFalso>) {
        let (mut j, _) = janela_com_tudo();
        let ligador = Arc::new(LigadorFalso {
            falha,
            ..LigadorFalso::default()
        });
        j.reconhecimento.ligador = Some(ligador.clone());
        (j, ligador)
    }

    fn indice_de(j: &Janela, procura: impl Fn(&Sugestao) -> bool) -> usize {
        j.reconhecimento.cartoes[0]
            .resultado()
            .expect("resultado")
            .sugestoes
            .iter()
            .position(procura)
            .expect("sugestão")
    }

    fn ocorrencia_de_outubro() -> sbf_planejamento::IdOcorrencia {
        IdOcorrencia {
            recorrente: ID_DO_SALARIO.to_owned(),
            competencia: Competencia { ano: 2026, mes: 10 },
            parcela: None,
        }
    }

    fn conciliado_de_outubro(j: &Janela) -> Option<sbf_planejamento::Centavos> {
        j.planejamento
            .as_ref()
            .expect("planejamento")
            .0
            .proximas(ID_DO_SALARIO, Competencia { ano: 2026, mes: 10 }, 1, hoje())
            .expect("proximas")[0]
            .conciliado
    }

    #[test]
    fn as_tres_provas_conciliam_sozinhas_com_autor_regra_e_ligam_o_salario() {
        let (mut j, ligador) = janela_com_ligador(false);
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let i = indice_de(&j, |s| matches!(s, Sugestao::Conciliar { .. }));
        let c = &j.reconhecimento.cartoes[0];
        assert!(c.automaticas.contains(&i) && c.confirmadas.contains(&i));
        assert_eq!(
            conciliado_de_outubro(&j),
            Some(sbf_planejamento::Centavos(2_460_522))
        );
        let h = j
            .planejamento
            .as_ref()
            .expect("p")
            .0
            .historico(Some(&ocorrencia_de_outubro().recorrente), 20)
            .expect("historico");
        assert!(
            h.iter()
                .any(|e| matches!(&e.autor, Autor::Regra { regra } if regra == "tres-provas"))
        );
        // O registro do salário foi ligado ao crédito (sha do PDF, movimento do extrato).
        let chamadas = ligador.chamadas.lock().expect("trava").clone();
        assert_eq!(chamadas.len(), 1);
        assert_eq!(
            chamadas[0].1,
            crate::reconhecimento_ficticio::MOVIMENTO_DO_SALARIO
        );
    }

    #[test]
    fn so_as_tres_provas_agem_sozinhas_duas_esperam_o_clique() {
        let (mut j, ligador) = janela_com_ligador(false);
        // Com duas provas só, o cartão não grava nada sem o clique.
        let resposta = {
            let mut r = reconhecedor_do_contracheque()
                .reconhecer(
                    std::path::Path::new("a.pdf"),
                    &mut |_| {},
                    &AtomicBool::new(false),
                )
                .expect("resposta");
            for s in &mut r.sugestoes {
                if let Sugestao::Conciliar { provas, .. } = s {
                    *provas = 2;
                }
            }
            r
        };
        j.ligar_reconhecedor(Arc::new(ReconhecedorFicticio::novo(Ok(resposta))));
        // O planejamento fictício já traz R$ 8.199,00 conciliados em outubro (o extrato dele).
        let antes = conciliado_de_outubro(&j);
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        assert_eq!(conciliado_de_outubro(&j), antes);
        assert!(j.reconhecimento.cartoes[0].automaticas.is_empty());
        assert!(ligador.chamadas.lock().expect("trava").is_empty());
        // O clique grava com o usuário e liga o salário.
        let i = indice_de(&j, |s| matches!(s, Sugestao::Conciliar { .. }));
        j.confirmar_sugestao(0, i);
        assert_eq!(
            conciliado_de_outubro(&j),
            Some(sbf_planejamento::Centavos(2_460_522))
        );
        assert_eq!(ligador.chamadas.lock().expect("trava").len(), 1);
    }

    #[test]
    fn desfazer_a_automatica_devolve_a_sugestao_e_a_regra_nao_refaz() {
        let (mut j, _) = janela_com_ligador(false);
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let i = indice_de(&j, |s| matches!(s, Sugestao::Conciliar { .. }));
        assert!(conciliado_de_outubro(&j).is_some());
        j.desfazer_automatica(0, i);
        let c = &j.reconhecimento.cartoes[0];
        assert!(!c.confirmadas.contains(&i) && !c.automaticas.contains(&i));
        assert_eq!(conciliado_de_outubro(&j), None);
        // Soltar o mesmo arquivo de novo não refaz a conciliação sozinho.
        j.limpar_reconhecimento();
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        assert_eq!(conciliado_de_outubro(&j), None);
        assert!(j.reconhecimento.cartoes[0].automaticas.is_empty());
        // Mas o clique confirma, como qualquer sugestão.
        let i = indice_de(&j, |s| matches!(s, Sugestao::Conciliar { .. }));
        j.confirmar_sugestao(0, i);
        assert!(conciliado_de_outubro(&j).is_some());
    }

    #[test]
    fn erro_ao_ligar_o_salario_vira_aviso_discreto_e_a_conciliacao_fica() {
        let (mut j, ligador) = janela_com_ligador(true);
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        assert!(conciliado_de_outubro(&j).is_some());
        assert_eq!(ligador.chamadas.lock().expect("trava").len(), 1);
        let (texto, erro) = j.controle.mensagem.clone().expect("aviso");
        assert_eq!(texto, AVISO_DO_ELO);
        assert!(!erro, "o aviso é discreto");
    }

    #[test]
    fn conciliar_a_mao_no_detalhe_liga_o_salario_quando_ha_contracheque_vinculado() {
        let (mut j, ligador) = janela_com_ligador(false);
        let id = IdOcorrencia {
            recorrente: ID_DO_SALARIO.to_owned(),
            competencia: Competencia { ano: 2026, mes: 11 },
            parcela: None,
        };
        j.controle_pedido(crate::controle_ui::PedidoDoControle::ConciliarManual {
            conciliacao: Conciliacao {
                ocorrencia: id,
                movimento: "pluggy:txn_ficticia_sal_manual".to_owned(),
                valor: sbf_planejamento::Centavos(822_600),
                data: Data::de_iso("2026-11-05").expect("data"),
                confirmada: true,
            },
            descricao: "SALARIO".to_owned(),
        });
        let chamadas = ligador.chamadas.lock().expect("trava").clone();
        assert_eq!(chamadas.len(), 1);
        assert_eq!(
            chamadas[0].0,
            crate::demonstracao::hash_do_nome("contracheque-2026-11.pdf")
        );
        assert_eq!(chamadas[0].1, "pluggy:txn_ficticia_sal_manual");
        // Um compromisso sem contracheque vinculado não liga nada.
        j.controle_pedido(crate::controle_ui::PedidoDoControle::ConciliarManual {
            conciliacao: Conciliacao {
                ocorrencia: IdOcorrencia {
                    recorrente: "rec_energia".to_owned(),
                    competencia: Competencia { ano: 2026, mes: 11 },
                    parcela: None,
                },
                movimento: "pluggy:txn_ficticia_luz".to_owned(),
                valor: sbf_planejamento::Centavos(18_000),
                data: Data::de_iso("2026-11-05").expect("data"),
                confirmada: true,
            },
            descricao: "ENERGIA".to_owned(),
        });
        assert_eq!(ligador.chamadas.lock().expect("trava").len(), 1);
    }

    #[test]
    fn criar_compromissos_cria_o_grupo_liga_as_rubricas_e_oferece_a_baixa() {
        let (mut j, ligador) = janela_com_ligador(false);
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let criar = indice_de(&j, |s| matches!(s, Sugestao::CriarDescontosEmFolha { .. }));
        assert!(
            j.reconhecimento.cartoes[0]
                .resultado()
                .expect("r")
                .sugestoes
                .iter()
                .all(|s| !matches!(s, Sugestao::DarBaixa { .. })),
            "a baixa só existe depois de ligar"
        );
        let antes = j
            .planejamento
            .as_ref()
            .expect("p")
            .0
            .recorrentes(false)
            .expect("l")
            .len();
        j.confirmar_sugestao(0, criar);
        assert_eq!(j.reconhecimento.cartoes[0].erro_de_sugestao, None);
        let novos: Vec<_> = j
            .planejamento
            .as_ref()
            .expect("p")
            .0
            .recorrentes(false)
            .expect("l")
            .into_iter()
            .filter(|r| r.grupo == "Descontos em folha")
            .collect();
        assert_eq!(novos.len(), 4);
        assert_eq!(
            j.planejamento
                .as_ref()
                .expect("p")
                .0
                .recorrentes(false)
                .expect("l")
                .len(),
            antes + 4
        );
        assert!(
            novos
                .iter()
                .all(|r| r.natureza == sbf_planejamento::Natureza::DescontoEmFolha)
        );
        // As rubricas ficaram ligadas na composição do mês.
        let k = composicao_em(&j, Competencia { ano: 2026, mes: 10 }).expect("composição");
        assert_eq!(
            k.rubricas.iter().filter(|r| r.recorrente.is_some()).count(),
            5
        );
        // E a baixa apareceu no cartão, sem ninguém ter conciliado nada.
        let baixa = indice_de(&j, |s| matches!(s, Sugestao::DarBaixa { .. }));
        let o_inss = novos.iter().find(|r| r.nome == "INSS").expect("inss");
        let previsto = |j: &Janela| {
            j.planejamento
                .as_ref()
                .expect("p")
                .0
                .proximas(&o_inss.id, Competencia { ano: 2026, mes: 10 }, 1, hoje())
                .expect("proximas")[0]
                .clone()
        };
        assert_eq!(previsto(&j).conciliado, None, "dar baixa pede o clique");
        j.confirmar_sugestao(0, baixa);
        assert_eq!(j.reconhecimento.cartoes[0].erro_de_sugestao, None);
        assert!(previsto(&j).conciliado.is_some());
        // A baixa pelo contracheque não liga o salário ao crédito (não há crédito nela).
        assert!(
            ligador
                .chamadas
                .lock()
                .expect("trava")
                .iter()
                .all(|(_, m)| !m.starts_with("contracheque:"))
        );
    }

    #[test]
    fn confirmar_as_rubricas_depois_dos_compromissos_nao_desliga_nada() {
        let (mut j, _) = janela_com_ligador(false);
        let dir = tempfile::tempdir().expect("dir");
        j.soltar_arquivos(vec![arquivo(&dir, "contracheque.pdf")], None);
        esperar(&mut j);
        let criar = indice_de(&j, |s| matches!(s, Sugestao::CriarDescontosEmFolha { .. }));
        j.confirmar_sugestao(0, criar);
        let rubricas = indice_de_rubricas(&j);
        j.confirmar_sugestao(0, rubricas);
        assert_eq!(j.reconhecimento.cartoes[0].erro_de_sugestao, None);
        let k = composicao_em(&j, Competencia { ano: 2026, mes: 10 }).expect("composição");
        assert!(k.rubricas.iter().filter(|r| r.recorrente.is_some()).count() >= 4);
    }
}
