//! A costura da **caixa de entrada e da edição** (spec 08, "Janela"): o que a interface lê
//! ([`FonteDaCaixa`]) e o que ela pede ([`Editor`]), com tipos de view-model próprios e mínimos.
//! Nada aqui conhece contrato, HTTP ou SQLite: o adaptador para a estação real mora fora
//! deste crate. A implementação em memória é [`CaixaFicticia`], para a demonstração e os testes.
//!
//! Regras da costura (as mesmas de `sbf_inventario::comando`):
//! - a janela **lê por `&self`** e **escreve por `&mut self`**; nada daqui bloqueia esperando
//!   a rede. Escrever só põe a alteração na fila local, como [`EstadoDeEnvio::Pendente`]; quem
//!   fala com o servidor roda fora do quadro e a janela acompanha por [`Caixa::tique`];
//! - a aplicação é **otimista**: depois de escrever, a leitura já mostra o efeito, marcado como
//!   pendente, até o servidor aceitar (some a marca) ou recusar (a [`Escrita`] ganha uma
//!   [`RecusaDaEscrita`] com "sua versão" e "no servidor agora");
//! - dinheiro em centavos `i64`, datas `AAAA-MM-DD`.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::{Duration, Instant};

use sbf_inventario::alteracao::CampoLadoALado;
use sbf_inventario::{Consulta, Ref, Tipo};

/// O id de um rascunho (`draft_records.id`).
pub type IdDoRascunho = String;
/// O id de um lote de rascunhos (`draft_batches.id`).
pub type IdDoLote = String;
/// O id de uma escrita na fila local.
pub type IdDaEscrita = String;

/// Por que uma chamada da costura falhou.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErroDaCaixa {
    NaoEncontrado(String),
    /// O pedido não passa nas regras (soma que não bate, campo faltando…). Nada foi gravado.
    Invalido(String),
    Local(String),
}

impl std::fmt::Display for ErroDaCaixa {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErroDaCaixa::NaoEncontrado(o) => write!(f, "não encontrado: {o}"),
            ErroDaCaixa::Invalido(m) => f.write_str(m),
            ErroDaCaixa::Local(m) => write!(f, "erro local: {m}"),
        }
    }
}

impl std::error::Error for ErroDaCaixa {}

pub type ResultadoDaCaixa<T> = Result<T, ErroDaCaixa>;

// ---------------------------------------------------------------------------
// Rascunhos
// ---------------------------------------------------------------------------

/// Onde o rascunho está (`draft_records.status`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusDoRascunho {
    /// `pending_review`.
    EmRevisao,
    /// `corrected`: você mexeu nos campos.
    Corrigido,
    Aprovado,
    Recusado,
}

impl StatusDoRascunho {
    pub fn rotulo(self) -> &'static str {
        match self {
            StatusDoRascunho::EmRevisao => "Em revisão",
            StatusDoRascunho::Corrigido => "Corrigido",
            StatusDoRascunho::Aprovado => "Aprovado",
            StatusDoRascunho::Recusado => "Recusado",
        }
    }

    /// Ainda espera uma decisão sua (a caixa de entrada lista só estes por padrão).
    pub fn aberto(self) -> bool {
        matches!(
            self,
            StatusDoRascunho::EmRevisao | StatusDoRascunho::Corrigido
        )
    }
}

/// Onde a escrita está em relação ao servidor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EstadoDeEnvio {
    /// Feita aqui, o servidor ainda não respondeu. Marca visual de pendente.
    Pendente,
    Aceita,
    /// O servidor não aplicou: fica visível até Refazer ou Descartar.
    Recusada,
}

impl EstadoDeEnvio {
    pub fn rotulo(self) -> &'static str {
        match self {
            EstadoDeEnvio::Pendente => "Pendente",
            EstadoDeEnvio::Aceita => "Aceita",
            EstadoDeEnvio::Recusada => "Recusada",
        }
    }
}

/// A direção do dinheiro (`outflow`/`inflow` do draft v1).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direcao {
    #[default]
    Saida,
    Entrada,
}

impl Direcao {
    pub fn rotulo(self) -> &'static str {
        match self {
            Direcao::Saida => "Saída",
            Direcao::Entrada => "Entrada",
        }
    }
}

/// Os valores de `transactions.type` que o formulário oferece (id do banco, rótulo).
pub const TIPOS_DE_LANCAMENTO: &[(&str, &str)] = &[
    ("expense", "Despesa"),
    ("income", "Receita"),
    ("refund", "Estorno"),
    ("fee", "Tarifa"),
    ("interest_charge", "Juros"),
    ("statement_payment", "Pagamento de fatura"),
    ("liability_payment", "Pagamento de dívida"),
    ("adjustment", "Ajuste de saldo"),
];

/// O rótulo de um tipo de lançamento pelo id do banco.
pub fn rotulo_do_tipo(id: &str) -> &str {
    TIPOS_DE_LANCAMENTO
        .iter()
        .find(|(i, _)| *i == id)
        .map_or(id, |(_, r)| r)
}

/// Os campos do `draft_data` v1 de uma transação que o formulário corrige.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CamposDoRascunho {
    pub direcao: Direcao,
    /// `transaction_type` (id do banco; ver [`TIPOS_DE_LANCAMENTO`]).
    pub tipo: Option<String>,
    /// `amount_cents`, sempre positivo (o sinal vem da direção).
    pub valor_centavos: Option<i64>,
    /// `event_date`.
    pub data_do_evento: Option<String>,
    /// `due_date`.
    pub vencimento: Option<String>,
    pub descricao: Option<String>,
    /// `supplier_name_raw`.
    pub fornecedor: Option<String>,
    /// `category_suggestion` (texto).
    pub categoria: Option<String>,
    /// `financial_product_id`: o id da conta ou cartão (ver [`FonteDaCaixa::contas`]).
    pub conta: Option<String>,
    pub numero_do_documento: Option<String>,
    pub observacoes: Option<String>,
}

impl CamposDoRascunho {
    /// O que falta para **lançar** (a camada "postable" do contrato): conta, tipo, valor maior
    /// que zero e data do evento. Vazio = pode aprovar.
    pub fn faltando(&self) -> Vec<&'static str> {
        let mut f = Vec::new();
        if self.conta.as_deref().is_none_or(|c| c.trim().is_empty()) {
            f.push("Conta ou cartão");
        }
        if self.tipo.as_deref().is_none_or(|c| c.trim().is_empty()) {
            f.push("Tipo");
        }
        if self.valor_centavos.is_none_or(|v| v <= 0) {
            f.push("Valor");
        }
        if self
            .data_do_evento
            .as_deref()
            .is_none_or(|c| c.trim().is_empty())
        {
            f.push("Data do evento");
        }
        f
    }
}

/// Um rascunho da caixa de entrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rascunho {
    pub id: IdDoRascunho,
    pub lote: IdDoLote,
    /// O nome do documento de onde ele veio ("fatura-energia-out.pdf").
    pub rotulo: String,
    /// O achado/documento do inventário, para a prévia ao lado. `None`: sem arquivo.
    pub documento: Option<Ref>,
    pub status: StatusDoRascunho,
    /// A última escrita sobre ele: `Some(Pendente)` mostra a marca de pendente.
    pub envio: Option<EstadoDeEnvio>,
    pub campos: CamposDoRascunho,
    /// Se você recusou: o motivo.
    pub motivo_da_recusa: Option<String>,
}

/// Um lote de rascunhos (`draft_batches`) com os rascunhos dele.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoteDeRascunhos {
    pub id: IdDoLote,
    pub nome: String,
    /// De onde vieram ("Gmail · rótulo Contas").
    pub origem: String,
    /// Instante ISO 8601.
    pub criado_em: String,
    pub rascunhos: Vec<Rascunho>,
}

impl LoteDeRascunhos {
    /// Os que ainda esperam decisão.
    pub fn abertos(&self) -> impl Iterator<Item = &Rascunho> {
        self.rascunhos.iter().filter(|r| r.status.aberto())
    }

    pub fn n_abertos(&self) -> usize {
        self.abertos().count()
    }
}

// ---------------------------------------------------------------------------
// Movimentações editáveis
// ---------------------------------------------------------------------------

/// Uma opção de seletor (conta, centro).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Opcao {
    pub id: String,
    pub nome: String,
}

/// Os campos que `EditarMovimentacao` muda.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CamposDaMovimentacao {
    pub descricao: String,
    /// `AAAA-MM-DD`.
    pub data: String,
    /// Positivo entrou, negativo saiu.
    pub valor_centavos: i64,
    /// O id da conta.
    pub conta: Option<String>,
    pub fornecedor: Option<String>,
    /// O id do centro, quando a movimentação tem um só (sem rateio).
    pub centro: Option<String>,
    pub notas: Option<String>,
}

/// Uma linha de rateio: quanto da movimentação vai para um centro. Sempre positivo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RateioDaMovimentacao {
    pub centro: String,
    pub centavos: i64,
}

/// Uma movimentação como a edição a vê.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MovimentacaoEditavel {
    pub alvo: Ref,
    pub campos: CamposDaMovimentacao,
    pub rateios: Vec<RateioDaMovimentacao>,
}

/// A soma dos rateios tem de bater com o valor (em módulo): confere antes de enviar.
/// Devolve a mensagem de por que não passa.
pub fn validar_rateio(total_centavos: i64, linhas: &[RateioDaMovimentacao]) -> Result<(), String> {
    if linhas.is_empty() {
        return Err("Informe ao menos um centro.".to_owned());
    }
    let mut vistos = std::collections::BTreeSet::new();
    for l in linhas {
        if l.centro.trim().is_empty() {
            return Err("Toda linha precisa de um centro.".to_owned());
        }
        if l.centavos <= 0 {
            return Err("Cada valor do rateio tem de ser maior que zero.".to_owned());
        }
        if !vistos.insert(l.centro.as_str()) {
            return Err("O mesmo centro aparece duas vezes.".to_owned());
        }
    }
    let soma: i64 = linhas.iter().map(|l| l.centavos).sum();
    let alvo = total_centavos.abs();
    if soma != alvo {
        let dif = alvo - soma;
        return Err(format!(
            "A soma ({}) não bate com o valor ({}): {} {}.",
            crate::formato::dinheiro(soma),
            crate::formato::dinheiro(alvo),
            if dif > 0 { "faltam" } else { "passam" },
            crate::formato::dinheiro(dif.abs()),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// A fila de escritas e as recusas
// ---------------------------------------------------------------------------

/// O que a escrita toca.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlvoDaEscrita {
    Rascunho(IdDoRascunho),
    Lote(IdDoLote),
    Movimentacao(Ref),
}

/// Por que o servidor recusou e as duas versões, lado a lado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecusaDaEscrita {
    pub motivo: String,
    /// Instante ISO 8601.
    pub quando: String,
    pub campos: Vec<CampoLadoALado>,
}

/// Uma escrita da fila local (a área Sincronização lista as Pendentes e as Recusadas).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Escrita {
    pub id: IdDaEscrita,
    /// "Aprovar rascunho", "Editar movimentação", "Ratear movimentação"…
    pub o_que: String,
    /// O nome do alvo ("fatura-energia-out.pdf", "Mercado Delta · 04/11").
    pub rotulo_do_alvo: String,
    pub alvo: AlvoDaEscrita,
    /// Instante ISO 8601 em que você fez.
    pub feita_em: String,
    pub estado: EstadoDeEnvio,
    pub recusa: Option<RecusaDaEscrita>,
}

// ---------------------------------------------------------------------------
// Envio de lote do inventário
// ---------------------------------------------------------------------------

/// Em que pé está o envio de um lote do inventário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EnvioDoLote {
    /// O lote ainda não está aprovado (ou não tem o que enviar): o motivo.
    Indisponivel(String),
    /// Aprovado e pronto: quantos documentos subirão.
    Pronto {
        documentos: usize,
    },
    Enviando(ProgressoDoEnvio),
    /// Todos aceitos: os registros do lote ficaram `promovido`.
    Enviado {
        documentos: usize,
    },
    /// Parou no meio: pode reenviar e retoma de onde parou.
    Falhou {
        progresso: ProgressoDoEnvio,
        motivo: String,
    },
}

/// Quantos documentos já subiram.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressoDoEnvio {
    pub enviados: usize,
    pub total: usize,
}

impl ProgressoDoEnvio {
    /// De 0 a 1.
    pub fn fracao(&self) -> f32 {
        if self.total == 0 {
            1.0
        } else {
            self.enviados as f32 / self.total as f32
        }
    }
}

// ---------------------------------------------------------------------------
// As costuras
// ---------------------------------------------------------------------------

/// O que a janela **lê** da caixa de entrada e da edição.
pub trait FonteDaCaixa {
    /// Os lotes de rascunhos do dono, do mais novo ao mais antigo.
    fn lotes(&self) -> ResultadoDaCaixa<Vec<LoteDeRascunhos>>;

    /// Contas e cartões (o seletor "Conta ou cartão").
    fn contas(&self) -> Vec<Opcao>;

    /// Centros (o seletor do rateio).
    fn centros(&self) -> Vec<Opcao>;

    /// A movimentação como a edição a vê, **já com as escritas pendentes aplicadas**.
    fn movimentacao(&self, alvo: &Ref) -> ResultadoDaCaixa<Option<MovimentacaoEditavel>>;

    /// A fila local: Pendentes e Recusadas (e as Aceitas recentes), da mais nova à mais antiga.
    fn escritas(&self) -> ResultadoDaCaixa<Vec<Escrita>>;

    /// O envio de um lote do inventário (o botão Enviar ao servidor e a barra).
    fn envio_do_lote(&self, lote: &Ref) -> EnvioDoLote;
}

/// O que a janela **pede**. Escrever só enfileira (não bloqueia a rede) e devolve a escrita.
pub trait Editor {
    /// Grava os campos do rascunho (`status = corrected`).
    fn corrigir(&mut self, id: &str, campos: CamposDoRascunho) -> ResultadoDaCaixa<Escrita>;
    /// Aprova e lança no livro-caixa. Recusa de saída o que ainda tem [`CamposDoRascunho::faltando`].
    fn aprovar(&mut self, id: &str) -> ResultadoDaCaixa<Escrita>;
    /// Recusa o rascunho, com o motivo (obrigatório).
    fn recusar(&mut self, id: &str, motivo: &str) -> ResultadoDaCaixa<Escrita>;
    /// Aprova todos os abertos do lote, **tudo ou nada**: se um não passa, nenhum muda.
    fn aprovar_lote(&mut self, lote: &str) -> ResultadoDaCaixa<Escrita>;
    fn editar_movimentacao(
        &mut self,
        alvo: &Ref,
        campos: CamposDaMovimentacao,
    ) -> ResultadoDaCaixa<Escrita>;
    /// Troca o conjunto de rateios inteiro. A soma tem de bater ([`validar_rateio`]).
    fn ratear(
        &mut self,
        alvo: &Ref,
        linhas: Vec<RateioDaMovimentacao>,
    ) -> ResultadoDaCaixa<Escrita>;
    fn excluir(&mut self, alvo: &Ref) -> ResultadoDaCaixa<Escrita>;
    /// "Refazer sobre a nova" de uma escrita recusada: nasce outra, Pendente.
    fn refazer(&mut self, id: &str) -> ResultadoDaCaixa<Escrita>;
    /// "Descartar" uma escrita recusada: vale a versão do servidor.
    fn descartar(&mut self, id: &str) -> ResultadoDaCaixa<()>;
    /// Começa (ou retoma) o envio de um lote aprovado; o andamento vem de
    /// [`FonteDaCaixa::envio_do_lote`].
    fn enviar_lote(&mut self, lote: &Ref) -> ResultadoDaCaixa<()>;
    /// Chamado uma vez por quadro: traz o que o envio em segundo plano produziu. `true` =
    /// algo mudou (a janela relê). Não bloqueia.
    fn tique(&mut self) -> bool {
        false
    }
}

/// Tudo junto: o que a janela guarda.
pub trait Caixa: FonteDaCaixa + Editor {}
impl<T: FonteDaCaixa + Editor> Caixa for T {}

/// A caixa ligada à janela.
pub struct CaixaLigada(pub Box<dyn Caixa>);

impl std::fmt::Debug for CaixaLigada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CaixaLigada")
    }
}

/// O estado de envio da última escrita sobre uma movimentação (a marca de pendente da página).
pub fn envio_da_movimentacao(escritas: &[Escrita], alvo: &Ref) -> Option<EstadoDeEnvio> {
    // A lista vem da mais nova para a mais antiga: a primeira que toca o alvo.
    escritas
        .iter()
        .find(|e| e.alvo == AlvoDaEscrita::Movimentacao(alvo.clone()))
        .map(|e| e.estado)
}

/// Quantas escritas esperam o servidor e quantas ele recusou.
pub fn contar(escritas: &[Escrita]) -> (usize, usize) {
    (
        escritas
            .iter()
            .filter(|e| e.estado == EstadoDeEnvio::Pendente)
            .count(),
        escritas
            .iter()
            .filter(|e| e.estado == EstadoDeEnvio::Recusada)
            .count(),
    )
}

// ---------------------------------------------------------------------------
// A implementação fictícia
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
enum Carga {
    Corrigir(String, CamposDoRascunho),
    Aprovar(String),
    Recusar(String, String),
    AprovarLote(String),
    Editar(Ref, CamposDaMovimentacao),
    Ratear(Ref, Vec<RateioDaMovimentacao>),
    Excluir(Ref),
}

#[derive(Debug, Clone)]
struct EscritaInterna {
    visivel: Escrita,
    carga: Carga,
    /// O status do rascunho antes (para desfazer a aplicação otimista numa recusa).
    status_anterior: Vec<(String, StatusDoRascunho, CamposDoRascunho)>,
    /// A sobreposição local da movimentação antes da escrita (`None` = não havia).
    mov_anterior: Option<(String, Option<Option<MovimentacaoEditavel>>)>,
    descartada: bool,
}

#[derive(Debug, Clone)]
struct EnvioInterno {
    total: usize,
    enviados: usize,
    falha: Option<String>,
}

struct Estado {
    lotes: Vec<LoteDeRascunhos>,
    contas: Vec<Opcao>,
    centros: Vec<Opcao>,
    /// As movimentações já alteradas; as outras vêm da `base`.
    movs: BTreeMap<String, Option<MovimentacaoEditavel>>,
    base: Option<Box<dyn Consulta>>,
    escritas: Vec<EscritaInterna>,
    proxima: u32,
    envios: BTreeMap<String, EnvioInterno>,
    documentos_por_lote: usize,
    ultimo_passo: Option<Instant>,
    /// Quanto espera entre um documento e outro (0 = só `avancar_envio`).
    passo: Duration,
    agora: String,
}

/// A caixa fictícia em memória: a demonstração e os testes. `Clone` compartilha o estado,
/// para o teste guardar uma ponta e dar a outra à janela.
#[derive(Clone)]
pub struct CaixaFicticia(Rc<RefCell<Estado>>);

impl std::fmt::Debug for CaixaFicticia {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CaixaFicticia")
    }
}

impl CaixaFicticia {
    /// Vazia, com as contas e os centros fictícios.
    pub fn nova() -> Self {
        let op = |i: &str, n: &str| Opcao {
            id: i.to_owned(),
            nome: n.to_owned(),
        };
        Self(Rc::new(RefCell::new(Estado {
            lotes: Vec::new(),
            contas: vec![
                op("conta-corrente", "Conta corrente Exemplo"),
                op("cartao-azul", "Cartão Azul"),
                op("poupanca", "Poupança Exemplo"),
            ],
            centros: vec![
                op("casa", "Casa"),
                op("mercado", "Mercado"),
                op("lazer", "Lazer"),
                op("saude", "Saúde"),
            ],
            movs: BTreeMap::new(),
            base: None,
            escritas: Vec::new(),
            proxima: 1,
            envios: BTreeMap::new(),
            documentos_por_lote: 5,
            ultimo_passo: None,
            passo: Duration::ZERO,
            agora: "2026-11-14T09:00:00Z".to_owned(),
        })))
    }

    /// Lotes e rascunhos prontos.
    pub fn com_lotes(self, lotes: Vec<LoteDeRascunhos>) -> Self {
        self.0.borrow_mut().lotes = lotes;
        self
    }

    /// De onde vêm as movimentações que ainda ninguém editou (a consulta do livro fictício).
    pub fn com_base(self, base: Box<dyn Consulta>) -> Self {
        self.0.borrow_mut().base = Some(base);
        self
    }

    /// Define uma movimentação editável (testes).
    pub fn com_movimentacao(self, m: MovimentacaoEditavel) -> Self {
        self.0
            .borrow_mut()
            .movs
            .insert(m.alvo.chave.clone(), Some(m));
        self
    }

    /// A demonstração tem passo de relógio: um documento a cada `passo`.
    pub fn com_passo_de_envio(self, passo: Duration) -> Self {
        self.0.borrow_mut().passo = passo;
        self
    }

    /// Quantos documentos tem um lote do inventário (o padrão é 5).
    pub fn com_documentos_por_lote(self, n: usize) -> Self {
        self.0.borrow_mut().documentos_por_lote = n;
        self
    }

    /// O servidor aceitou tudo o que estava Pendente.
    pub fn servidor_aceita_tudo(&self) {
        let mut e = self.0.borrow_mut();
        let Estado {
            escritas, lotes, ..
        } = &mut *e;
        for x in escritas
            .iter_mut()
            .filter(|x| x.visivel.estado == EstadoDeEnvio::Pendente)
        {
            x.visivel.estado = EstadoDeEnvio::Aceita;
            marcar_rascunhos(lotes, &x.carga, Some(EstadoDeEnvio::Aceita));
        }
    }

    /// O servidor recusou esta escrita: a aplicação otimista volta e a escrita ganha o lado a lado.
    pub fn servidor_recusa(&self, id: &str, motivo: &str, campos: Vec<CampoLadoALado>) {
        let mut e = self.0.borrow_mut();
        let agora = e.agora.clone();
        let Estado {
            escritas,
            lotes,
            movs,
            ..
        } = &mut *e;
        let Some(x) = escritas.iter_mut().find(|x| x.visivel.id == id) else {
            return;
        };
        x.visivel.estado = EstadoDeEnvio::Recusada;
        x.visivel.recusa = Some(RecusaDaEscrita {
            motivo: motivo.to_owned(),
            quando: agora,
            campos,
        });
        for (rid, status, campos) in &x.status_anterior {
            for l in lotes.iter_mut() {
                for r in l.rascunhos.iter_mut().filter(|r| &r.id == rid) {
                    r.status = *status;
                    r.campos = campos.clone();
                    r.envio = Some(EstadoDeEnvio::Recusada);
                }
            }
        }
        if let Some((chave, antes)) = x.mov_anterior.clone() {
            // A movimentação volta ao que o servidor tem: restaura a sobreposição de antes.
            match antes {
                Some(v) => movs.insert(chave, v),
                None => movs.remove(&chave),
            };
        }
    }

    /// Sobe mais um documento do envio em curso (testes e o relógio da demonstração).
    pub fn avancar_envio(&self) {
        let mut e = self.0.borrow_mut();
        for envio in e.envios.values_mut() {
            if envio.falha.is_none() && envio.enviados < envio.total {
                envio.enviados += 1;
                return;
            }
        }
    }

    /// O envio para de dar certo no meio (rede caiu): `enviar_lote` de novo retoma.
    pub fn derrubar_envio(&self, lote: &Ref, motivo: &str) {
        if let Some(x) = self.0.borrow_mut().envios.get_mut(&lote.chave) {
            x.falha = Some(motivo.to_owned());
        }
    }

    /// As escritas como a janela as vê (atalho para testes).
    pub fn escritas_visiveis(&self) -> Vec<Escrita> {
        self.escritas().unwrap_or_default()
    }

    fn registrar(
        e: &mut Estado,
        o_que: &str,
        rotulo: String,
        alvo: AlvoDaEscrita,
        carga: Carga,
        anteriores: Vec<(String, StatusDoRascunho, CamposDoRascunho)>,
    ) -> Escrita {
        let id = format!("escrita-{}", e.proxima);
        e.proxima += 1;
        let visivel = Escrita {
            id,
            o_que: o_que.to_owned(),
            rotulo_do_alvo: rotulo,
            alvo,
            feita_em: e.agora.clone(),
            estado: EstadoDeEnvio::Pendente,
            recusa: None,
        };
        e.escritas.insert(
            0,
            EscritaInterna {
                visivel: visivel.clone(),
                carga,
                status_anterior: anteriores,
                mov_anterior: None,
                descartada: false,
            },
        );
        visivel
    }

    /// Aplica a carga ao estado local (a aplicação otimista) e põe na fila.
    fn aplicar(e: &mut Estado, carga: Carga, o_que: &str) -> ResultadoDaCaixa<Escrita> {
        match carga.clone() {
            Carga::Corrigir(id, campos) => {
                let (rotulo, anterior) = {
                    let r = achar(&mut e.lotes, &id)?;
                    let ant = (r.id.clone(), r.status, r.campos.clone());
                    r.campos = campos;
                    r.status = StatusDoRascunho::Corrigido;
                    r.envio = Some(EstadoDeEnvio::Pendente);
                    (r.rotulo.clone(), ant)
                };
                Ok(Self::registrar(
                    e,
                    o_que,
                    rotulo,
                    AlvoDaEscrita::Rascunho(id),
                    carga,
                    vec![anterior],
                ))
            }
            Carga::Aprovar(id) => {
                let (rotulo, anterior) = {
                    let r = achar(&mut e.lotes, &id)?;
                    if !r.status.aberto() {
                        return Err(ErroDaCaixa::Invalido(format!(
                            "o rascunho já está {}",
                            r.status.rotulo().to_lowercase()
                        )));
                    }
                    let f = r.campos.faltando();
                    if !f.is_empty() {
                        return Err(ErroDaCaixa::Invalido(format!(
                            "falta para lançar: {}",
                            f.join(", ")
                        )));
                    }
                    let ant = (r.id.clone(), r.status, r.campos.clone());
                    r.status = StatusDoRascunho::Aprovado;
                    r.envio = Some(EstadoDeEnvio::Pendente);
                    (r.rotulo.clone(), ant)
                };
                Ok(Self::registrar(
                    e,
                    o_que,
                    rotulo,
                    AlvoDaEscrita::Rascunho(id),
                    carga,
                    vec![anterior],
                ))
            }
            Carga::Recusar(id, motivo) => {
                if motivo.trim().is_empty() {
                    return Err(ErroDaCaixa::Invalido(
                        "Diga por que está recusando.".to_owned(),
                    ));
                }
                let (rotulo, anterior) = {
                    let r = achar(&mut e.lotes, &id)?;
                    if !r.status.aberto() {
                        return Err(ErroDaCaixa::Invalido(format!(
                            "o rascunho já está {}",
                            r.status.rotulo().to_lowercase()
                        )));
                    }
                    let ant = (r.id.clone(), r.status, r.campos.clone());
                    r.status = StatusDoRascunho::Recusado;
                    r.motivo_da_recusa = Some(motivo.trim().to_owned());
                    r.envio = Some(EstadoDeEnvio::Pendente);
                    (r.rotulo.clone(), ant)
                };
                Ok(Self::registrar(
                    e,
                    o_que,
                    rotulo,
                    AlvoDaEscrita::Rascunho(id),
                    carga,
                    vec![anterior],
                ))
            }
            Carga::AprovarLote(id) => {
                let (nome, abertos) = {
                    let l = e
                        .lotes
                        .iter()
                        .find(|l| l.id == id)
                        .ok_or_else(|| ErroDaCaixa::NaoEncontrado(format!("lote {id}")))?;
                    (l.nome.clone(), l.n_abertos())
                };
                if abertos == 0 {
                    return Err(ErroDaCaixa::Invalido(
                        "O lote não tem rascunhos esperando decisão.".to_owned(),
                    ));
                }
                // Tudo ou nada: confere todos antes de mudar qualquer um.
                if let Some(l) = e.lotes.iter().find(|l| l.id == id) {
                    for r in l.abertos() {
                        let f = r.campos.faltando();
                        if !f.is_empty() {
                            return Err(ErroDaCaixa::Invalido(format!(
                                "{}: falta {}. Nenhum rascunho do lote foi aprovado.",
                                r.rotulo,
                                f.join(", ")
                            )));
                        }
                    }
                }
                let mut anteriores = Vec::new();
                if let Some(l) = e.lotes.iter_mut().find(|l| l.id == id) {
                    for r in l.rascunhos.iter_mut().filter(|r| r.status.aberto()) {
                        anteriores.push((r.id.clone(), r.status, r.campos.clone()));
                        r.status = StatusDoRascunho::Aprovado;
                        r.envio = Some(EstadoDeEnvio::Pendente);
                    }
                }
                Ok(Self::registrar(
                    e,
                    o_que,
                    format!("{nome} ({abertos})"),
                    AlvoDaEscrita::Lote(id),
                    carga,
                    anteriores,
                ))
            }
            Carga::Editar(alvo, campos) => {
                if campos.descricao.trim().is_empty() {
                    return Err(ErroDaCaixa::Invalido(
                        "A descrição não pode ficar vazia.".to_owned(),
                    ));
                }
                if campos.valor_centavos == 0 {
                    return Err(ErroDaCaixa::Invalido(
                        "O valor não pode ser zero.".to_owned(),
                    ));
                }
                let mut atual = Self::ler_mov(e, &alvo)?
                    .ok_or_else(|| ErroDaCaixa::NaoEncontrado(alvo.to_string()))?;
                // Mudar o valor desmancha um rateio que já não bate: avisa em vez de gravar torto.
                if !atual.rateios.is_empty()
                    && campos.valor_centavos.abs() != atual.campos.valor_centavos.abs()
                {
                    return Err(ErroDaCaixa::Invalido(
                        "A movimentação tem rateio: refaça o rateio com o novo valor antes de mudar o valor."
                            .to_owned(),
                    ));
                }
                let rotulo = rotulo_da_mov(&campos);
                let antes = e.movs.get(&alvo.chave).cloned();
                atual.campos = campos.clone();
                e.movs.insert(alvo.chave.clone(), Some(atual));
                Ok(Self::com_mov_anterior(
                    Self::registrar(
                        e,
                        o_que,
                        rotulo,
                        AlvoDaEscrita::Movimentacao(alvo.clone()),
                        Carga::Editar(alvo.clone(), campos),
                        Vec::new(),
                    ),
                    e,
                    &alvo,
                    antes,
                ))
            }
            Carga::Ratear(alvo, linhas) => {
                let mut atual = Self::ler_mov(e, &alvo)?
                    .ok_or_else(|| ErroDaCaixa::NaoEncontrado(alvo.to_string()))?;
                validar_rateio(atual.campos.valor_centavos, &linhas)
                    .map_err(ErroDaCaixa::Invalido)?;
                let rotulo = rotulo_da_mov(&atual.campos);
                let antes = e.movs.get(&alvo.chave).cloned();
                atual.rateios = linhas.clone();
                e.movs.insert(alvo.chave.clone(), Some(atual));
                Ok(Self::com_mov_anterior(
                    Self::registrar(
                        e,
                        o_que,
                        rotulo,
                        AlvoDaEscrita::Movimentacao(alvo.clone()),
                        Carga::Ratear(alvo.clone(), linhas),
                        Vec::new(),
                    ),
                    e,
                    &alvo,
                    antes,
                ))
            }
            Carga::Excluir(alvo) => {
                let atual = Self::ler_mov(e, &alvo)?
                    .ok_or_else(|| ErroDaCaixa::NaoEncontrado(alvo.to_string()))?;
                let rotulo = rotulo_da_mov(&atual.campos);
                let antes = e.movs.get(&alvo.chave).cloned();
                e.movs.insert(alvo.chave.clone(), None);
                Ok(Self::com_mov_anterior(
                    Self::registrar(
                        e,
                        o_que,
                        rotulo,
                        AlvoDaEscrita::Movimentacao(alvo.clone()),
                        Carga::Excluir(alvo.clone()),
                        Vec::new(),
                    ),
                    e,
                    &alvo,
                    antes,
                ))
            }
        }
    }

    /// Guarda, na escrita recém-criada (a primeira da fila), o que havia antes da movimentação.
    fn com_mov_anterior(
        escrita: Escrita,
        e: &mut Estado,
        alvo: &Ref,
        antes: Option<Option<MovimentacaoEditavel>>,
    ) -> Escrita {
        if let Some(x) = e.escritas.first_mut() {
            x.mov_anterior = Some((alvo.chave.clone(), antes));
        }
        escrita
    }

    fn ler_mov(e: &Estado, alvo: &Ref) -> ResultadoDaCaixa<Option<MovimentacaoEditavel>> {
        if let Some(m) = e.movs.get(&alvo.chave) {
            return Ok(m.clone());
        }
        let Some(base) = &e.base else {
            return Ok(None);
        };
        let item = base
            .item(alvo)
            .map_err(|x| ErroDaCaixa::Local(x.to_string()))?;
        Ok(item.and_then(|i| mov_do_item(&i)))
    }
}

fn rotulo_da_mov(c: &CamposDaMovimentacao) -> String {
    format!("{} · {}", c.descricao, crate::formato::data(&c.data))
}

fn mov_do_item(i: &sbf_inventario::consulta::Item) -> Option<MovimentacaoEditavel> {
    use sbf_inventario::consulta::Celula;
    if i.alvo.tipo != Tipo::Movimentacao {
        return None;
    }
    let mut c = CamposDaMovimentacao {
        descricao: i
            .rotulo
            .rsplit_once(" · ")
            .map_or(i.rotulo.clone(), |(d, _)| d.to_owned()),
        ..Default::default()
    };
    for campo in &i.campos {
        match (campo.rotulo.as_str(), &campo.valor) {
            ("Valor", Celula::Valor { centavos, .. }) => c.valor_centavos = *centavos,
            ("Data", Celula::Data(d)) => c.data = d.clone(),
            ("Fornecedor", Celula::Link { texto, .. }) => c.fornecedor = Some(texto.clone()),
            _ => {}
        }
    }
    Some(MovimentacaoEditavel {
        alvo: i.alvo.clone(),
        campos: c,
        rateios: Vec::new(),
    })
}

fn achar<'a>(lotes: &'a mut [LoteDeRascunhos], id: &str) -> ResultadoDaCaixa<&'a mut Rascunho> {
    lotes
        .iter_mut()
        .flat_map(|l| l.rascunhos.iter_mut())
        .find(|r| r.id == id)
        .ok_or_else(|| ErroDaCaixa::NaoEncontrado(format!("rascunho {id}")))
}

fn marcar_rascunhos(lotes: &mut [LoteDeRascunhos], carga: &Carga, estado: Option<EstadoDeEnvio>) {
    let ids: Vec<String> = match carga {
        Carga::Corrigir(id, _) | Carga::Aprovar(id) | Carga::Recusar(id, _) => vec![id.clone()],
        Carga::AprovarLote(l) => lotes
            .iter()
            .filter(|x| &x.id == l)
            .flat_map(|x| x.rascunhos.iter().map(|r| r.id.clone()))
            .collect(),
        _ => return,
    };
    for l in lotes.iter_mut() {
        for r in l.rascunhos.iter_mut().filter(|r| ids.contains(&r.id)) {
            // Só quem estava pendente muda de marca.
            if r.envio == Some(EstadoDeEnvio::Pendente) {
                r.envio = estado;
            }
        }
    }
}

impl FonteDaCaixa for CaixaFicticia {
    fn lotes(&self) -> ResultadoDaCaixa<Vec<LoteDeRascunhos>> {
        Ok(self.0.borrow().lotes.clone())
    }

    fn contas(&self) -> Vec<Opcao> {
        self.0.borrow().contas.clone()
    }

    fn centros(&self) -> Vec<Opcao> {
        self.0.borrow().centros.clone()
    }

    fn movimentacao(&self, alvo: &Ref) -> ResultadoDaCaixa<Option<MovimentacaoEditavel>> {
        Self::ler_mov(&self.0.borrow(), alvo)
    }

    fn escritas(&self) -> ResultadoDaCaixa<Vec<Escrita>> {
        Ok(self
            .0
            .borrow()
            .escritas
            .iter()
            .filter(|x| !x.descartada)
            .map(|x| x.visivel.clone())
            .collect())
    }

    fn envio_do_lote(&self, lote: &Ref) -> EnvioDoLote {
        let e = self.0.borrow();
        match e.envios.get(&lote.chave) {
            Some(x) => {
                let p = ProgressoDoEnvio {
                    enviados: x.enviados,
                    total: x.total,
                };
                match &x.falha {
                    Some(m) => EnvioDoLote::Falhou {
                        progresso: p,
                        motivo: m.clone(),
                    },
                    None if x.enviados >= x.total => EnvioDoLote::Enviado {
                        documentos: x.total,
                    },
                    None => EnvioDoLote::Enviando(p),
                }
            }
            None => EnvioDoLote::Pronto {
                documentos: e.documentos_por_lote,
            },
        }
    }
}

impl Editor for CaixaFicticia {
    fn corrigir(&mut self, id: &str, campos: CamposDoRascunho) -> ResultadoDaCaixa<Escrita> {
        Self::aplicar(
            &mut self.0.borrow_mut(),
            Carga::Corrigir(id.to_owned(), campos),
            "Corrigir rascunho",
        )
    }

    fn aprovar(&mut self, id: &str) -> ResultadoDaCaixa<Escrita> {
        Self::aplicar(
            &mut self.0.borrow_mut(),
            Carga::Aprovar(id.to_owned()),
            "Aprovar rascunho",
        )
    }

    fn recusar(&mut self, id: &str, motivo: &str) -> ResultadoDaCaixa<Escrita> {
        Self::aplicar(
            &mut self.0.borrow_mut(),
            Carga::Recusar(id.to_owned(), motivo.to_owned()),
            "Recusar rascunho",
        )
    }

    fn aprovar_lote(&mut self, lote: &str) -> ResultadoDaCaixa<Escrita> {
        Self::aplicar(
            &mut self.0.borrow_mut(),
            Carga::AprovarLote(lote.to_owned()),
            "Aprovar lote de rascunhos",
        )
    }

    fn editar_movimentacao(
        &mut self,
        alvo: &Ref,
        campos: CamposDaMovimentacao,
    ) -> ResultadoDaCaixa<Escrita> {
        Self::aplicar(
            &mut self.0.borrow_mut(),
            Carga::Editar(alvo.clone(), campos),
            "Editar movimentação",
        )
    }

    fn ratear(
        &mut self,
        alvo: &Ref,
        linhas: Vec<RateioDaMovimentacao>,
    ) -> ResultadoDaCaixa<Escrita> {
        Self::aplicar(
            &mut self.0.borrow_mut(),
            Carga::Ratear(alvo.clone(), linhas),
            "Ratear movimentação",
        )
    }

    fn excluir(&mut self, alvo: &Ref) -> ResultadoDaCaixa<Escrita> {
        Self::aplicar(
            &mut self.0.borrow_mut(),
            Carga::Excluir(alvo.clone()),
            "Excluir movimentação",
        )
    }

    fn refazer(&mut self, id: &str) -> ResultadoDaCaixa<Escrita> {
        let mut e = self.0.borrow_mut();
        let x = e
            .escritas
            .iter_mut()
            .find(|x| x.visivel.id == id)
            .ok_or_else(|| ErroDaCaixa::NaoEncontrado(format!("escrita {id}")))?;
        if x.visivel.estado != EstadoDeEnvio::Recusada {
            return Err(ErroDaCaixa::Invalido(
                "Só uma escrita recusada pode ser refeita.".to_owned(),
            ));
        }
        x.descartada = true;
        let (carga, o_que) = (x.carga.clone(), x.visivel.o_que.clone());
        Self::aplicar(&mut e, carga, &o_que)
    }

    fn descartar(&mut self, id: &str) -> ResultadoDaCaixa<()> {
        let mut e = self.0.borrow_mut();
        let x = e
            .escritas
            .iter_mut()
            .find(|x| x.visivel.id == id)
            .ok_or_else(|| ErroDaCaixa::NaoEncontrado(format!("escrita {id}")))?;
        if x.visivel.estado != EstadoDeEnvio::Recusada {
            return Err(ErroDaCaixa::Invalido(
                "Só uma escrita recusada pode ser descartada.".to_owned(),
            ));
        }
        x.descartada = true;
        Ok(())
    }

    fn enviar_lote(&mut self, lote: &Ref) -> ResultadoDaCaixa<()> {
        let mut e = self.0.borrow_mut();
        let total = e.documentos_por_lote;
        match e.envios.get_mut(&lote.chave) {
            Some(x) if x.falha.is_some() => x.falha = None,
            Some(_) => {}
            None => {
                e.envios.insert(
                    lote.chave.clone(),
                    EnvioInterno {
                        total,
                        enviados: 0,
                        falha: None,
                    },
                );
            }
        }
        e.ultimo_passo = Some(Instant::now());
        Ok(())
    }

    fn tique(&mut self) -> bool {
        let (passo, ultimo) = {
            let e = self.0.borrow();
            (e.passo, e.ultimo_passo)
        };
        let Some(ultimo) = ultimo else { return false };
        if passo.is_zero() || ultimo.elapsed() < passo {
            return false;
        }
        self.0.borrow_mut().ultimo_passo = Some(Instant::now());
        let antes: usize = self.0.borrow().envios.values().map(|x| x.enviados).sum();
        self.avancar_envio();
        let depois: usize = self.0.borrow().envios.values().map(|x| x.enviados).sum();
        antes != depois
    }
}

/// A demonstração: seis rascunhos em dois lotes (alguns incompletos, de propósito), ligados
/// aos documentos do acervo fictício, mais as movimentações do livro como base de edição.
pub fn demonstracao(documentos: &[Ref], livro: Box<dyn Consulta>) -> CaixaFicticia {
    let doc = |i: usize| documentos.get(i % documentos.len().max(1)).cloned();
    let campos = |desc: &str,
                  forn: &str,
                  valor: Option<i64>,
                  data: Option<&str>,
                  conta: Option<&str>,
                  cat: &str| CamposDoRascunho {
        direcao: Direcao::Saida,
        tipo: Some("expense".to_owned()),
        valor_centavos: valor,
        data_do_evento: data.map(str::to_owned),
        vencimento: data.map(str::to_owned),
        descricao: Some(desc.to_owned()),
        fornecedor: Some(forn.to_owned()),
        categoria: Some(cat.to_owned()),
        conta: conta.map(str::to_owned),
        numero_do_documento: None,
        observacoes: None,
    };
    let r = |id: &str, lote: &str, nome: &str, i: usize, c: CamposDoRascunho| Rascunho {
        id: id.to_owned(),
        lote: lote.to_owned(),
        rotulo: nome.to_owned(),
        documento: doc(i),
        status: StatusDoRascunho::EmRevisao,
        envio: None,
        campos: c,
        motivo_da_recusa: None,
    };
    let lotes = vec![
        LoteDeRascunhos {
            id: "lote-gmail-nov".to_owned(),
            nome: "Gmail · contas de novembro".to_owned(),
            origem: "Gmail · rótulo Contas".to_owned(),
            criado_em: "2026-11-13T20:15:00Z".to_owned(),
            rascunhos: vec![
                r(
                    "r1",
                    "lote-gmail-nov",
                    "fatura-energia-out.pdf",
                    0,
                    campos(
                        "Fatura de energia, outubro",
                        "Energia Exemplo",
                        Some(31_244),
                        Some("2026-11-05"),
                        Some("conta-corrente"),
                        "Casa",
                    ),
                ),
                r(
                    "r2",
                    "lote-gmail-nov",
                    "boleto-internet.pdf",
                    1,
                    campos(
                        "Internet fibra",
                        "Internet Ltda Exemplo",
                        Some(12_990),
                        Some("2026-11-10"),
                        None,
                        "Casa",
                    ),
                ),
                r(
                    "r3",
                    "lote-gmail-nov",
                    "recibo-farmacia.jpg",
                    2,
                    campos(
                        "Farmácia",
                        "Farmácia Sigma",
                        Some(8_730),
                        Some("2026-11-07"),
                        Some("cartao-azul"),
                        "Saúde",
                    ),
                ),
            ],
        },
        LoteDeRascunhos {
            id: "lote-pasta-out".to_owned(),
            nome: "Pasta local · outubro".to_owned(),
            origem: "Pasta local · Documentos/Contas".to_owned(),
            criado_em: "2026-11-12T18:40:00Z".to_owned(),
            rascunhos: vec![
                r(
                    "r4",
                    "lote-pasta-out",
                    "extrato-mercado.pdf",
                    3,
                    campos(
                        "Compras do mês",
                        "Mercado Delta",
                        Some(45_810),
                        Some("2026-10-28"),
                        Some("cartao-azul"),
                        "Mercado",
                    ),
                ),
                r(
                    "r5",
                    "lote-pasta-out",
                    "nota-assinatura.pdf",
                    4,
                    campos(
                        "Assinatura de vídeo",
                        "Streaming Exemplo",
                        None,
                        Some("2026-10-30"),
                        Some("cartao-azul"),
                        "Lazer",
                    ),
                ),
            ],
        },
    ];
    CaixaFicticia::nova()
        .com_lotes(lotes)
        .com_base(livro)
        .com_passo_de_envio(Duration::from_millis(450))
}

#[cfg(test)]
mod testes {
    use super::*;

    fn mov(chave: &str, valor: i64) -> MovimentacaoEditavel {
        MovimentacaoEditavel {
            alvo: Ref::nova(Tipo::Movimentacao, chave),
            campos: CamposDaMovimentacao {
                descricao: "Mercado Delta".into(),
                data: "2026-10-28".into(),
                valor_centavos: valor,
                ..Default::default()
            },
            rateios: Vec::new(),
        }
    }

    fn completo() -> CamposDoRascunho {
        CamposDoRascunho {
            tipo: Some("expense".into()),
            valor_centavos: Some(1000),
            data_do_evento: Some("2026-11-01".into()),
            conta: Some("conta-corrente".into()),
            ..Default::default()
        }
    }

    fn rascunho(id: &str, lote: &str, c: CamposDoRascunho) -> Rascunho {
        Rascunho {
            id: id.into(),
            lote: lote.into(),
            rotulo: format!("{id}.pdf"),
            documento: None,
            status: StatusDoRascunho::EmRevisao,
            envio: None,
            campos: c,
            motivo_da_recusa: None,
        }
    }

    fn caixa_com(rascunhos: Vec<Rascunho>) -> CaixaFicticia {
        CaixaFicticia::nova().com_lotes(vec![LoteDeRascunhos {
            id: "L".into(),
            nome: "Lote".into(),
            origem: "teste".into(),
            criado_em: "2026-11-14T00:00:00Z".into(),
            rascunhos,
        }])
    }

    fn rasc(c: &CaixaFicticia, id: &str) -> Rascunho {
        c.lotes().unwrap()[0]
            .rascunhos
            .iter()
            .find(|r| r.id == id)
            .unwrap()
            .clone()
    }

    #[test]
    fn faltando_lista_o_que_impede_de_lancar() {
        let vazio = CamposDoRascunho::default();
        assert_eq!(
            vazio.faltando(),
            vec!["Conta ou cartão", "Tipo", "Valor", "Data do evento"]
        );
        assert!(completo().faltando().is_empty());
        let zero = CamposDoRascunho {
            valor_centavos: Some(0),
            ..completo()
        };
        assert_eq!(zero.faltando(), vec!["Valor"]);
    }

    #[test]
    fn rateio_so_passa_com_a_soma_exata() {
        let l = |c: &str, v| RateioDaMovimentacao {
            centro: c.into(),
            centavos: v,
        };
        assert!(validar_rateio(-10_000, &[l("a", 6_000), l("b", 4_000)]).is_ok());
        let erro = validar_rateio(-10_000, &[l("a", 6_000), l("b", 3_999)]).unwrap_err();
        assert!(erro.contains("faltam"), "{erro}");
        let erro = validar_rateio(10_000, &[l("a", 6_000), l("b", 4_001)]).unwrap_err();
        assert!(erro.contains("passam"), "{erro}");
        assert!(validar_rateio(100, &[]).is_err());
        assert!(validar_rateio(100, &[l("a", 100), l("a", 0)]).is_err());
        assert!(validar_rateio(200, &[l("a", 100), l("a", 100)]).is_err());
        assert!(validar_rateio(100, &[l("a", -100)]).is_err());
    }

    #[test]
    fn aprovar_marca_pendente_e_o_aceite_tira_a_marca() {
        let mut c = caixa_com(vec![rascunho("a", "L", completo())]);
        let e = c.aprovar("a").unwrap();
        assert_eq!(e.estado, EstadoDeEnvio::Pendente);
        let r = rasc(&c, "a");
        assert_eq!(r.status, StatusDoRascunho::Aprovado);
        assert_eq!(r.envio, Some(EstadoDeEnvio::Pendente));
        c.servidor_aceita_tudo();
        assert_eq!(rasc(&c, "a").envio, Some(EstadoDeEnvio::Aceita));
        assert_eq!(c.escritas().unwrap()[0].estado, EstadoDeEnvio::Aceita);
    }

    #[test]
    fn aprovar_com_campo_faltando_nao_grava_nada() {
        let mut c = caixa_com(vec![rascunho("a", "L", CamposDoRascunho::default())]);
        let erro = c.aprovar("a").unwrap_err().to_string();
        assert!(erro.contains("Conta ou cartão"), "{erro}");
        assert_eq!(rasc(&c, "a").status, StatusDoRascunho::EmRevisao);
        assert!(c.escritas().unwrap().is_empty());
    }

    #[test]
    fn lote_e_tudo_ou_nada() {
        let mut c = caixa_com(vec![
            rascunho("ok", "L", completo()),
            rascunho("ruim", "L", CamposDoRascunho::default()),
        ]);
        assert!(c.aprovar_lote("L").is_err());
        assert_eq!(rasc(&c, "ok").status, StatusDoRascunho::EmRevisao);
        // Corrige o ruim: agora passa, os dois de uma vez.
        c.corrigir("ruim", completo()).unwrap();
        c.aprovar_lote("L").unwrap();
        assert_eq!(rasc(&c, "ok").status, StatusDoRascunho::Aprovado);
        assert_eq!(rasc(&c, "ruim").status, StatusDoRascunho::Aprovado);
    }

    #[test]
    fn recusar_exige_motivo_e_guarda() {
        let mut c = caixa_com(vec![rascunho("a", "L", completo())]);
        assert!(c.recusar("a", "  ").is_err());
        c.recusar("a", "não é meu").unwrap();
        let r = rasc(&c, "a");
        assert_eq!(r.status, StatusDoRascunho::Recusado);
        assert_eq!(r.motivo_da_recusa.as_deref(), Some("não é meu"));
    }

    #[test]
    fn recusa_do_servidor_desfaz_o_otimismo_e_refazer_ou_descartar() {
        let mut c = caixa_com(vec![rascunho("a", "L", completo())]);
        let e = c.aprovar("a").unwrap();
        c.servidor_recusa(
            &e.id,
            "o rascunho mudou no servidor",
            vec![CampoLadoALado::novo("Valor", "R$ 10,00", "R$ 12,00")],
        );
        let r = rasc(&c, "a");
        assert_eq!(r.status, StatusDoRascunho::EmRevisao, "volta ao que era");
        assert_eq!(r.envio, Some(EstadoDeEnvio::Recusada));
        let fila = c.escritas().unwrap();
        assert_eq!(contar(&fila), (0, 1));
        assert!(fila[0].recusa.as_ref().unwrap().campos[0].difere());
        // Refazer: nasce outra Pendente e a recusada sai da lista.
        let nova = c.refazer(&e.id).unwrap();
        assert_eq!(nova.estado, EstadoDeEnvio::Pendente);
        assert_eq!(contar(&c.escritas().unwrap()), (1, 0));
        assert_eq!(rasc(&c, "a").status, StatusDoRascunho::Aprovado);
        // Descartar uma que não está recusada não pode.
        assert!(c.descartar(&nova.id).is_err());
        c.servidor_recusa(&nova.id, "de novo", vec![]);
        c.descartar(&nova.id).unwrap();
        assert!(c.escritas().unwrap().is_empty());
    }

    #[test]
    fn editar_ratear_e_excluir_movimentacao() {
        let alvo = Ref::nova(Tipo::Movimentacao, "m1");
        let mut c = CaixaFicticia::nova().com_movimentacao(mov("m1", -10_000));
        let mut campos = c.movimentacao(&alvo).unwrap().unwrap().campos;
        campos.descricao = "Mercado Delta (compra grande)".into();
        c.editar_movimentacao(&alvo, campos).unwrap();
        assert_eq!(
            c.movimentacao(&alvo).unwrap().unwrap().campos.descricao,
            "Mercado Delta (compra grande)"
        );
        assert_eq!(
            envio_da_movimentacao(&c.escritas().unwrap(), &alvo),
            Some(EstadoDeEnvio::Pendente)
        );
        let linha = |c: &str, v| RateioDaMovimentacao {
            centro: c.into(),
            centavos: v,
        };
        let antes = c.escritas().unwrap().len();
        assert!(
            c.ratear(&alvo, vec![linha("casa", 7_000), linha("lazer", 2_000)])
                .is_err()
        );
        assert_eq!(
            c.escritas().unwrap().len(),
            antes,
            "rateio inválido não grava"
        );
        c.ratear(&alvo, vec![linha("casa", 7_000), linha("lazer", 3_000)])
            .unwrap();
        assert_eq!(c.movimentacao(&alvo).unwrap().unwrap().rateios.len(), 2);
        c.excluir(&alvo).unwrap();
        assert!(c.movimentacao(&alvo).unwrap().is_none());
        // O servidor recusa a exclusão: a movimentação volta, com o rateio de antes.
        let id = c.escritas().unwrap()[0].id.clone();
        c.servidor_recusa(&id, "mudou", vec![]);
        assert_eq!(c.movimentacao(&alvo).unwrap().unwrap().rateios.len(), 2);
    }

    #[test]
    fn envio_de_lote_anda_e_retoma_de_onde_parou() {
        let lote = Ref::nova(Tipo::Lote, "7");
        let mut c = CaixaFicticia::nova().com_documentos_por_lote(3);
        assert_eq!(
            c.envio_do_lote(&lote),
            EnvioDoLote::Pronto { documentos: 3 }
        );
        c.enviar_lote(&lote).unwrap();
        c.avancar_envio();
        assert_eq!(
            c.envio_do_lote(&lote),
            EnvioDoLote::Enviando(ProgressoDoEnvio {
                enviados: 1,
                total: 3
            })
        );
        c.derrubar_envio(&lote, "sem rede");
        assert!(matches!(c.envio_do_lote(&lote), EnvioDoLote::Falhou { .. }));
        c.enviar_lote(&lote).unwrap();
        c.avancar_envio();
        c.avancar_envio();
        assert_eq!(
            c.envio_do_lote(&lote),
            EnvioDoLote::Enviado { documentos: 3 }
        );
    }

    #[test]
    fn progresso_em_fracao() {
        assert_eq!(
            ProgressoDoEnvio {
                enviados: 1,
                total: 4
            }
            .fracao(),
            0.25
        );
        assert_eq!(
            ProgressoDoEnvio {
                enviados: 0,
                total: 0
            }
            .fracao(),
            1.0
        );
    }
}
