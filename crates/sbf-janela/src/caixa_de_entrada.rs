//! A Caixa de entrada na janela (CONTEXT.md): os documentos que o reconhecimento local viu
//! chegar (Telegram, Gmail, pasta, arrastar, `sbf reconhecer`). Aqui moram o estado e a
//! lógica, sem egui desenhando nada (o desenho está em [`crate::caixa_de_entrada_ui`]).
//!
//! Duas listas saem da mesma tabela (`sbf_inventario::caixa_de_entrada`):
//!
//! - a **Caixa de entrada** lista o que algum leitor ou regra reconheceu (Pronto, Precisa
//!   revisar, Possível duplicidade): é onde se decidem as sugestões de ligação ao
//!   planejamento. Uma leitura que falhou também aparece aqui, como Precisa revisar;
//! - **A triar** ganha a seção "Sem padrão de reconhecimento" com o que nenhuma regra casou
//!   (Não reconhecido, Protegido por senha): é onde se cria a regra e se reconhece de novo.
//!
//! Processando não está em lista nenhuma nem no contador: é um indicador à parte.
//!
//! Escolher uma linha mostra no dock o **mesmo cartão** do painel Reconhecimento e a Prévia
//! do arquivo. O cartão é recalculado na hora **só com o texto que o inventário já guarda**
//! (sem IA, sem extração, sem OCR): é rápido o bastante para a thread da interface. Sem texto
//! guardado o cartão diz isso, e "Reconhecer de novo" (que extrai) roda numa thread à parte.
//! Confirmar uma sugestão segue o caminho de sempre ([`Janela::confirmar_sugestao`]), com
//! Rastro e Desfazer. Resolver, dispensar e reabrir são decisões do usuário no log. O que a
//! janela lê e grava passa pela costura [`FonteDaCaixaDeEntrada`], para os testes e a
//! demonstração usarem uma fictícia.

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use sbf_inventario::caixa_de_entrada::{
    Canal, Contagens, Entrada, Estado, Filtro, Grupo, Situacao,
};
use sbf_inventario::consulta::Area;
use sbf_inventario::{Ref, Tipo};
use sbf_reconhecimento::Resultado;

use crate::janela::{Janela, Mensagem};
use crate::navegacao::Destino;
use crate::reconhecimento::{Cartao, SituacaoDoCartao};

/// De quanto em quanto tempo as listas são relidas (o worker grava de outro processo).
const RELER_A_CADA: Duration = Duration::from_secs(2);

/// O que a janela lê e pede da Caixa de entrada.
pub trait FonteDaCaixaDeEntrada: Send + Sync + std::fmt::Debug {
    /// As linhas da lista, das mais recentes para as mais antigas.
    ///
    /// # Errors
    /// O motivo, sem conteúdo de documento.
    fn listar(&self, filtro: &Filtro) -> Result<Vec<Entrada>, String>;

    /// Os contadores.
    ///
    /// # Errors
    /// O motivo.
    fn contagens(&self) -> Result<Contagens, String>;

    /// Resolver, dispensar ou reabrir: uma decisão do usuário no log (Rastro, Desfazer).
    ///
    /// # Errors
    /// O motivo.
    fn definir_situacao(&self, sha256: &str, situacao: Situacao) -> Result<(), String>;

    /// O cartão do reconhecimento do conteúdo, recalculado agora **só com o texto guardado**
    /// (sem IA, sem extrair, sem OCR) e sem mexer na Caixa de entrada. Roda na thread da
    /// interface: tem de ser rápido.
    ///
    /// # Errors
    /// O motivo (o arquivo não está mais no cofre, o banco travou).
    fn cartao(&self, sha256: &str) -> Result<Resultado, String>;

    /// "Reconhecer de novo": extrai o que faltar, roda as regras de agora e atualiza a linha
    /// (o item pode mudar de lista) **sem reabrir** o que o usuário decidiu. Pode demorar:
    /// a janela o chama numa thread à parte.
    ///
    /// # Errors
    /// O motivo.
    fn reconhecer_de_novo(&self, sha256: &str) -> Result<Resultado, String>;
}

/// Quais linhas uma lista mostra.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Escopo {
    /// O padrão: o que ainda espera uma decisão.
    #[default]
    NaoResolvidos,
    /// Também o que já foi resolvido ou dispensado.
    Todos,
}

impl Escopo {
    pub fn rotulo(self) -> &'static str {
        match self {
            Escopo::NaoResolvidos => "Não resolvidos",
            Escopo::Todos => "Todos",
        }
    }

    fn so_abertas(self) -> bool {
        self == Escopo::NaoResolvidos
    }
}

/// O que a interface pede à Caixa de entrada (aplicado depois do desenho, no tique).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PedidoDaCaixaDeEntrada {
    Selecionar(String),
    Resolver(String),
    Dispensar(String),
    Reabrir(String),
    ReconhecerDeNovo(String),
    Recarregar,
}

/// Um "Reconhecer de novo" que roda numa thread à parte.
#[derive(Debug)]
struct Reconhecendo {
    sha256: String,
    resposta: Receiver<Result<Resultado, String>>,
}

/// O estado da Caixa de entrada na janela.
#[derive(Debug, Default)]
pub struct EstadoDaCaixaDeEntrada {
    fonte: Option<Arc<dyn FonteDaCaixaDeEntrada>>,
    /// A Caixa de entrada: os reconhecidos.
    pub caixa: Vec<Entrada>,
    /// A seção "Sem padrão de reconhecimento" de A triar.
    pub sem_padrao: Vec<Entrada>,
    pub contagens: Contagens,
    /// O filtro Não resolvidos / Todos da Caixa de entrada.
    pub escopo: Escopo,
    /// O mesmo filtro, na seção Sem padrão de A triar.
    pub escopo_sem_padrao: Escopo,
    pub canal: Option<Canal>,
    /// O conteúdo (sha256) da linha escolhida.
    pub selecionada: Option<String>,
    /// A seleção da tabela no momento em que a linha foi escolhida: se ela muda (o usuário
    /// clicou noutra coisa), o dock volta a seguir a tabela.
    selecao_vista: Option<Ref>,
    foco_ativo: bool,
    /// O último erro de leitura ou de ação (sem conteúdo de documento).
    pub erro: Option<String>,
    ultima_leitura: Option<Instant>,
    pub pendentes: Vec<PedidoDaCaixaDeEntrada>,
    /// A seção de A triar está aberta.
    pub secao_aberta: bool,
    reconhecendo: Option<Reconhecendo>,
}

impl EstadoDaCaixaDeEntrada {
    pub fn ligada(&self) -> bool {
        self.fonte.is_some()
    }

    /// O filtro da Caixa de entrada no momento.
    pub fn filtro_da_caixa(&self) -> Filtro {
        Filtro {
            grupo: Grupo::Reconhecidos,
            so_abertas: self.escopo.so_abertas(),
            canal: self.canal,
            limite: 500,
        }
    }

    /// O filtro da seção Sem padrão de A triar no momento.
    pub fn filtro_sem_padrao(&self) -> Filtro {
        Filtro {
            grupo: Grupo::SemPadrao,
            so_abertas: self.escopo_sem_padrao.so_abertas(),
            canal: None,
            limite: 500,
        }
    }

    /// Pede uma releitura no próximo tique.
    pub fn recarregar(&mut self) {
        self.ultima_leitura = None;
    }

    /// O "Reconhecer de novo" deste conteúdo está rodando.
    pub fn esta_reconhecendo(&self, sha256: &str) -> bool {
        self.reconhecendo
            .as_ref()
            .is_some_and(|r| r.sha256 == sha256)
    }

    /// Algum "Reconhecer de novo" está rodando.
    pub fn reconhecendo_algum(&self) -> bool {
        self.reconhecendo.is_some()
    }
}

/// `2026-11-20T14:05:09.123Z` (UTC) como `20/11 11:05` (UTC−3, o fuso do usuário).
pub fn chegada_em_texto(utc: &str) -> String {
    let (Some(dia), Some(hora)) = (utc.get(0..10), utc.get(11..16)) else {
        return utc.to_owned();
    };
    let nums: Option<Vec<i64>> = dia
        .split('-')
        .chain(hora.split(':'))
        .map(|p| p.parse().ok())
        .collect();
    let Some([a, m, d, h, min]) = nums.as_deref().and_then(|v| <[i64; 5]>::try_from(v).ok()) else {
        return utc.to_owned();
    };
    let minutos = h * 60 + min - 3 * 60;
    let (mes, dia, minutos) = if minutos < 0 {
        let (_, m, d) = dia_anterior(a, m, d);
        (m, d, minutos + 1440)
    } else {
        (m, d, minutos)
    };
    format!("{dia:02}/{mes:02} {:02}:{:02}", minutos / 60, minutos % 60)
}

/// O dia anterior.
fn dia_anterior(a: i64, m: i64, d: i64) -> (i64, i64, i64) {
    if d > 1 {
        return (a, m, d - 1);
    }
    let (pa, pm) = if m == 1 { (a - 1, 12) } else { (a, m - 1) };
    let bissexto = (pa % 4 == 0 && pa % 100 != 0) || pa % 400 == 0;
    let ultimo = match pm {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ if bissexto => 29,
        _ => 28,
    };
    (pa, pm, ultimo)
}

impl Janela {
    /// Liga a Caixa de entrada (a real, sobre o `inventario.sqlite`, ou a fictícia da
    /// demonstração).
    pub fn ligar_caixa_de_entrada(&mut self, fonte: Arc<dyn FonteDaCaixaDeEntrada>) {
        self.caixa_de_entrada.fonte = Some(fonte);
        self.caixa_de_entrada.secao_aberta = true;
        self.caixa_de_entrada.recarregar();
    }

    /// Os contadores (releituras no máximo a cada poucos segundos); `None` sem Caixa de
    /// entrada ligada.
    pub fn contagens_da_caixa_de_entrada(&mut self) -> Option<Contagens> {
        self.caixa_de_entrada.fonte.as_ref()?;
        self.reler_a_caixa_de_entrada(false);
        Some(self.caixa_de_entrada.contagens)
    }

    fn reler_a_caixa_de_entrada(&mut self, forcar: bool) {
        let Some(fonte) = self.caixa_de_entrada.fonte.clone() else {
            return;
        };
        let velha = self
            .caixa_de_entrada
            .ultima_leitura
            .is_none_or(|t| t.elapsed() >= RELER_A_CADA);
        if !forcar && !velha {
            return;
        }
        self.caixa_de_entrada.ultima_leitura = Some(Instant::now());
        let c = &self.caixa_de_entrada;
        let leitura = (|| -> Result<_, String> {
            Ok((
                fonte.listar(&c.filtro_da_caixa())?,
                fonte.listar(&c.filtro_sem_padrao())?,
                fonte.contagens()?,
            ))
        })();
        match leitura {
            Ok((caixa, sem_padrao, contagens)) => {
                let c = &mut self.caixa_de_entrada;
                c.caixa = caixa;
                c.sem_padrao = sem_padrao;
                c.contagens = contagens;
                c.erro = None;
            }
            Err(e) => self.caixa_de_entrada.erro = Some(e),
        }
    }

    /// Chamada a cada quadro (por `arrastar_ui`): aplica o que a interface pediu, recolhe o
    /// "Reconhecer de novo" que terminou, relê as listas quando estão velhas e resolve o item
    /// cujas sugestões acabaram. Devolve `true` enquanto algo roda (quem desenha pede quadro).
    pub fn tique_da_caixa_de_entrada(&mut self, ctx: Option<&egui::Context>) -> bool {
        if !self.caixa_de_entrada.ligada() {
            return false;
        }
        let pedidos = std::mem::take(&mut self.caixa_de_entrada.pendentes);
        let mexeu = !pedidos.is_empty();
        for p in pedidos {
            self.aplicar_pedido_da_caixa_de_entrada(p, ctx);
        }
        let terminou = self.recolher_o_reconhecimento();
        self.resolver_se_acabou();
        self.reler_a_caixa_de_entrada(mexeu || terminou);
        self.caixa_de_entrada.reconhecendo.is_some()
    }

    fn aplicar_pedido_da_caixa_de_entrada(
        &mut self,
        pedido: PedidoDaCaixaDeEntrada,
        ctx: Option<&egui::Context>,
    ) {
        let Some(fonte) = self.caixa_de_entrada.fonte.clone() else {
            return;
        };
        match pedido {
            PedidoDaCaixaDeEntrada::Selecionar(sha) => self.selecionar_na_caixa_de_entrada(&sha),
            PedidoDaCaixaDeEntrada::Resolver(sha) => {
                self.mudar_situacao(&*fonte, &sha, Situacao::Resolvida, "Resolvido");
            }
            PedidoDaCaixaDeEntrada::Dispensar(sha) => {
                self.mudar_situacao(&*fonte, &sha, Situacao::Dispensada, "Dispensado");
            }
            PedidoDaCaixaDeEntrada::Reabrir(sha) => {
                self.mudar_situacao(&*fonte, &sha, Situacao::Aberta, "Reaberto");
            }
            PedidoDaCaixaDeEntrada::ReconhecerDeNovo(sha) => {
                self.reconhecer_de_novo_em_segundo_plano(fonte, sha, ctx);
            }
            PedidoDaCaixaDeEntrada::Recarregar => self.caixa_de_entrada.recarregar(),
        }
    }

    /// Roda o "Reconhecer de novo" numa thread: ele extrai texto (pdftotext, OCR) e roda
    /// scripts, e nada disso pode travar o quadro.
    fn reconhecer_de_novo_em_segundo_plano(
        &mut self,
        fonte: Arc<dyn FonteDaCaixaDeEntrada>,
        sha: String,
        ctx: Option<&egui::Context>,
    ) {
        if self.caixa_de_entrada.reconhecendo.is_some() {
            return;
        }
        let (envio, resposta) = channel();
        let ctx = ctx.cloned();
        let da_thread = sha.clone();
        std::thread::spawn(move || {
            let r = fonte.reconhecer_de_novo(&da_thread);
            let _ = envio.send(r);
            if let Some(c) = ctx {
                c.request_repaint();
            }
        });
        self.caixa_de_entrada.reconhecendo = Some(Reconhecendo {
            sha256: sha,
            resposta,
        });
    }

    /// Recolhe o "Reconhecer de novo" que terminou. `true` quando recolheu um.
    fn recolher_o_reconhecimento(&mut self) -> bool {
        let Some(r) = self.caixa_de_entrada.reconhecendo.as_ref() else {
            return false;
        };
        let resposta = match r.resposta.try_recv() {
            Ok(resposta) => resposta,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err("O reconhecimento parou de repente.".to_owned()),
        };
        let sha = r.sha256.clone();
        self.caixa_de_entrada.reconhecendo = None;
        match resposta {
            Ok(resultado) => {
                // O dock só troca de cartão se a linha ainda é a escolhida.
                if self.caixa_de_entrada.selecionada.as_deref() == Some(sha.as_str()) {
                    self.mostrar_cartao_da_caixa_de_entrada(&sha, resultado);
                }
                self.mensagem = Some(Mensagem {
                    texto: "Reconhecido de novo.".to_owned(),
                    erro: false,
                });
            }
            Err(e) => self.caixa_de_entrada.erro = Some(e),
        }
        true
    }

    fn mudar_situacao(
        &mut self,
        fonte: &dyn FonteDaCaixaDeEntrada,
        sha: &str,
        s: Situacao,
        feito: &str,
    ) {
        match fonte.definir_situacao(sha, s) {
            Ok(()) => {
                if self.caixa_de_entrada.selecionada.as_deref() == Some(sha)
                    && s != Situacao::Aberta
                {
                    self.soltar_a_selecao_da_caixa_de_entrada();
                }
                self.mensagem = Some(Mensagem {
                    texto: format!("{feito}."),
                    erro: false,
                });
            }
            Err(e) => self.caixa_de_entrada.erro = Some(e),
        }
    }

    fn soltar_a_selecao_da_caixa_de_entrada(&mut self) {
        self.caixa_de_entrada.selecionada = None;
        self.caixa_de_entrada.foco_ativo = false;
        self.reconhecimento.da_caixa = None;
    }

    /// Escolhe uma linha: o dock passa a mostrar o cartão e a Prévia dela.
    fn selecionar_na_caixa_de_entrada(&mut self, sha: &str) {
        let Some(fonte) = self.caixa_de_entrada.fonte.clone() else {
            return;
        };
        match fonte.cartao(sha) {
            Ok(r) => self.mostrar_cartao_da_caixa_de_entrada(sha, r),
            Err(e) => self.caixa_de_entrada.erro = Some(e),
        }
    }

    fn mostrar_cartao_da_caixa_de_entrada(&mut self, sha: &str, resultado: Resultado) {
        let cartao = Cartao::da_caixa_de_entrada(resultado);
        let cartoes = &mut self.reconhecimento.cartoes;
        // Um lugar só para o cartão da Caixa de entrada: trocar de linha reaproveita o mesmo.
        let vaga = self
            .reconhecimento
            .da_caixa
            .filter(|i| cartoes.get(*i).is_some_and(|c| !c.em_andamento()));
        let indice = match vaga {
            Some(i) => {
                cartoes[i] = cartao;
                i
            }
            None => {
                cartoes.push(cartao);
                cartoes.len() - 1
            }
        };
        self.reconhecimento.da_caixa = Some(indice);
        self.guardar_nomes_das_regras(indice);
        self.caixa_de_entrada.selecionada = Some(sha.to_owned());
        self.caixa_de_entrada.selecao_vista = self.centro.selecao.foco.clone();
        self.caixa_de_entrada.foco_ativo = true;
        self.reconhecimento.painel_pedido = true;
        self.caixa_de_entrada.recarregar();
    }

    /// Todas as sugestões do cartão foram confirmadas ou ignoradas, e alguma foi confirmada:
    /// o item está resolvido e sai da lista padrão.
    fn resolver_se_acabou(&mut self) {
        let (Some(sha), Some(i)) = (
            self.caixa_de_entrada.selecionada.clone(),
            self.reconhecimento.da_caixa,
        ) else {
            return;
        };
        let Some(fonte) = self.caixa_de_entrada.fonte.clone() else {
            return;
        };
        let Some(c) = self.reconhecimento.cartoes.get(i) else {
            return;
        };
        let SituacaoDoCartao::Pronto(r) = &c.situacao else {
            return;
        };
        let todas = !r.sugestoes.is_empty()
            && (0..r.sugestoes.len())
                .all(|s| c.confirmadas.contains(&s) || c.ignoradas.contains(&s))
            && !c.confirmadas.is_empty();
        if todas {
            self.mudar_situacao(&*fonte, &sha, Situacao::Resolvida, "Resolvido");
            self.caixa_de_entrada.recarregar();
        }
    }

    /// O item que o dock mostra quando uma linha da Caixa de entrada está escolhida: o achado
    /// dela.
    pub(crate) fn foco_da_caixa_de_entrada(&self) -> Option<Ref> {
        self.cartao_da_caixa_de_entrada_em_foco()?;
        let sha = self.caixa_de_entrada.selecionada.as_deref()?;
        Some(Ref::nova(Tipo::Achado, sha))
    }

    /// O índice do cartão da Caixa de entrada quando ele é o que o dock deve mostrar: a linha
    /// segue escolhida, a tabela não mudou de seleção e o centro ainda é a Caixa de entrada ou
    /// A triar.
    pub(crate) fn cartao_da_caixa_de_entrada_em_foco(&self) -> Option<usize> {
        let c = &self.caixa_de_entrada;
        if !c.foco_ativo || c.selecionada.is_none() {
            return None;
        }
        if c.selecao_vista != self.centro.selecao.foco {
            return None;
        }
        let atual = self.navegacao.atual();
        if !matches!(
            atual,
            Destino::AreaFutura("caixa") | Destino::Area(Area::ATriar)
        ) {
            return None;
        }
        self.reconhecimento.da_caixa
    }

    /// Para os testes de fora do crate: o índice do cartão em foco.
    #[doc(hidden)]
    pub fn cartao_da_caixa_de_entrada_em_foco_para_testes(&self) -> Option<usize> {
        self.cartao_da_caixa_de_entrada_em_foco()
    }

    /// Soltar um arquivo na janela volta o dock para os cartões do arrastar.
    pub(crate) fn largar_o_foco_da_caixa_de_entrada(&mut self) {
        self.caixa_de_entrada.foco_ativo = false;
    }
}

// ---------------------------------------------------------------------------------------
// A Caixa de entrada fictícia (demonstração e testes)
// ---------------------------------------------------------------------------------------

/// Uma Caixa de entrada em memória: as linhas e, para cada conteúdo, o resultado que o
/// cartão mostra.
#[derive(Debug, Default)]
pub struct CaixaDeEntradaFicticia {
    dentro: Mutex<Dentro>,
}

#[derive(Debug, Default)]
struct Dentro {
    entradas: Vec<Entrada>,
    resultados: Vec<(String, Resultado)>,
    /// O que "Reconhecer de novo" passa a devolver, e o novo estado.
    depois: Vec<(String, Resultado, Estado)>,
}

impl CaixaDeEntradaFicticia {
    pub fn nova() -> Self {
        Self::default()
    }

    /// Acrescenta uma linha e o resultado do cartão dela.
    #[must_use]
    pub fn com(self, entrada: Entrada, resultado: Resultado) -> Self {
        if let Ok(mut d) = self.dentro.lock() {
            d.resultados.push((entrada.content_hash.clone(), resultado));
            d.entradas.push(entrada);
        }
        self
    }

    /// Define o que "Reconhecer de novo" passa a devolver para o conteúdo (e o novo estado).
    #[must_use]
    pub fn reconhecendo_depois_como(self, sha: &str, resultado: Resultado, estado: Estado) -> Self {
        if let Ok(mut d) = self.dentro.lock() {
            d.depois.push((sha.to_owned(), resultado, estado));
        }
        self
    }

    /// A situação atual da linha (para os testes).
    pub fn situacao_de(&self, sha: &str) -> Option<Situacao> {
        let d = self.dentro.lock().ok()?;
        d.entradas
            .iter()
            .find(|e| e.content_hash == sha)
            .map(|e| e.situacao)
    }

    /// O estado atual da linha (para os testes).
    pub fn estado_de(&self, sha: &str) -> Option<Estado> {
        let d = self.dentro.lock().ok()?;
        d.entradas
            .iter()
            .find(|e| e.content_hash == sha)
            .map(|e| e.estado)
    }
}

/// Uma linha fictícia da Caixa de entrada.
pub fn entrada_ficticia(
    sha: &str,
    nome: &str,
    chegou_em: &str,
    canal: Canal,
    tipo: Option<&str>,
    resumo: &str,
    estado: Estado,
) -> Entrada {
    Entrada {
        content_hash: sha.to_owned(),
        nome: nome.to_owned(),
        chegou_em: chegou_em.to_owned(),
        atualizado_em: chegou_em.to_owned(),
        canal,
        tipo: tipo.map(str::to_owned),
        resumo: Some(resumo.to_owned()),
        estado,
        situacao: Situacao::Aberta,
        vezes: 1,
    }
}

impl FonteDaCaixaDeEntrada for CaixaDeEntradaFicticia {
    fn listar(&self, f: &Filtro) -> Result<Vec<Entrada>, String> {
        let d = self.dentro.lock().map_err(|_| "travada".to_owned())?;
        let mut v: Vec<Entrada> = d
            .entradas
            .iter()
            .filter(|e| match f.grupo {
                Grupo::Reconhecidos => e.estado.e_reconhecido(),
                Grupo::SemPadrao => e.estado.sem_padrao(),
                Grupo::Todos => true,
            })
            .filter(|e| !f.so_abertas || e.situacao == Situacao::Aberta)
            .filter(|e| f.canal.is_none_or(|o| o == e.canal))
            .cloned()
            .collect();
        v.sort_by(|a, b| b.chegou_em.cmp(&a.chegou_em));
        Ok(v)
    }

    fn contagens(&self) -> Result<Contagens, String> {
        let d = self.dentro.lock().map_err(|_| "travada".to_owned())?;
        let mut c = Contagens::default();
        for e in d.entradas.iter().filter(|e| e.situacao == Situacao::Aberta) {
            if e.estado.e_reconhecido() {
                c.reconhecidos += 1;
            } else if e.estado.sem_padrao() {
                c.sem_padrao += 1;
            } else if e.estado.e_processando() {
                c.processando += 1;
            }
        }
        Ok(c)
    }

    fn definir_situacao(&self, sha: &str, s: Situacao) -> Result<(), String> {
        let mut d = self.dentro.lock().map_err(|_| "travada".to_owned())?;
        match d.entradas.iter_mut().find(|e| e.content_hash == sha) {
            Some(e) => {
                e.situacao = s;
                Ok(())
            }
            None => Err("o item não está na Caixa de entrada".to_owned()),
        }
    }

    fn cartao(&self, sha: &str) -> Result<Resultado, String> {
        let d = self.dentro.lock().map_err(|_| "travada".to_owned())?;
        d.resultados
            .iter()
            .find(|(s, _)| s == sha)
            .map(|(_, r)| r.clone())
            .ok_or_else(|| "o arquivo não está mais no cofre".to_owned())
    }

    fn reconhecer_de_novo(&self, sha: &str) -> Result<Resultado, String> {
        let mut d = self.dentro.lock().map_err(|_| "travada".to_owned())?;
        let (r, estado) = match d.depois.iter().find(|(s, _, _)| s == sha) {
            Some((_, r, e)) => (r.clone(), Some(*e)),
            None => (
                d.resultados
                    .iter()
                    .find(|(s, _)| s == sha)
                    .map(|(_, r)| r.clone())
                    .ok_or_else(|| "o arquivo não está mais no cofre".to_owned())?,
                None,
            ),
        };
        if let Some(e) = d.entradas.iter_mut().find(|e| e.content_hash == sha) {
            if let Some(estado) = estado {
                e.estado = estado;
            }
            e.vezes += 1;
        }
        if let Some(slot) = d.resultados.iter_mut().find(|(s, _)| s == sha) {
            slot.1 = r.clone();
        }
        Ok(r)
    }
}

impl Cartao {
    /// O cartão de um item da Caixa de entrada: o resultado já pronto, sem etapas a anunciar.
    pub(crate) fn da_caixa_de_entrada(resultado: Resultado) -> Self {
        let mut c = Cartao::novo(std::path::PathBuf::from(&resultado.nome));
        c.nome = resultado.nome.clone();
        c.situacao = SituacaoDoCartao::Pronto(Box::new(resultado));
        c
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn a_chegada_vai_para_o_fuso_do_usuario() {
        assert_eq!(chegada_em_texto("2026-11-20T14:05:09.123Z"), "20/11 11:05");
        // Antes das 03:00 UTC vira o dia anterior.
        assert_eq!(chegada_em_texto("2026-11-20T01:30:00.000Z"), "19/11 22:30");
        assert_eq!(chegada_em_texto("2026-03-01T02:00:00.000Z"), "28/02 23:00");
        assert_eq!(chegada_em_texto("2026-01-01T00:10:00.000Z"), "31/12 21:10");
        assert_eq!(chegada_em_texto("lixo"), "lixo");
    }
}
