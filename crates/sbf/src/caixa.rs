//! A caixa de entrada e a edição **de verdade** (spec 08): o adaptador entre a costura da
//! janela (`sbf_janela::caixa::{FonteDaCaixa, Editor}`, view-models mínimos) e a estação
//! (`sbf_estacao::Escritas`, tipos do contrato v2).
//!
//! - Ler e escrever passam pela mesma `EstacaoLocal` que a janela já usa para o livro-caixa
//!   (`Arc<Mutex<_>>`); escrever só enfileira (a sincronização envia) e a leitura já mostra o
//!   efeito, marcado como pendente.
//! - O envio de um lote do inventário bloqueia (rede e disco): roda numa thread, com a sua
//!   própria conexão ao `inventario.sqlite`, e o andamento volta por um canal que o
//!   [`Editor::tique`] esvazia a cada quadro.
//! - Nenhum valor de rascunho, nome de arquivo ou texto de documento vai para log.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant};

use sbf_contrato::{
    Acao, Centavos, Centro, Colecao, Conta, Documento, EdicaoDaMovimentacao, EstadoDoCentro,
    EstadoDoRascunho, Fornecedor, Id, LinhaAgora, LoteDeRascunhos as LoteDoContrato, Movimentacao,
    NovoRateio, OrigemDoDocumento, Rascunho as RascunhoDoContrato, Rateio, TipoDeMovimentacao,
};
use sbf_estacao::{
    AndamentoDoEnvio, ClienteDoServidor, ErroDeEscrita, Escrita as EscritaDaEstacao, Escritas,
    EstacaoLocal, EstadoDaEscrita, ItemDaCaixa, LeitorDeCofre, ResumoDoEnvio, dinheiro,
    documentos_do_lote,
};
use sbf_inventario::alteracao::CampoLadoALado;
use sbf_inventario::banco::Inventario;
use sbf_inventario::{Ref, Tipo};
use sbf_janela::caixa::{
    AlvoDaEscrita, CamposDaMovimentacao, CamposDoRascunho, Direcao, Editor, EnvioDoLote,
    ErroDaCaixa, Escrita, EstadoDeEnvio, FonteDaCaixa, LoteDeRascunhos, MovimentacaoEditavel,
    Opcao, ProgressoDoEnvio, Rascunho, RateioDaMovimentacao, RecusaDaEscrita, ResultadoDaCaixa,
    StatusDoRascunho,
};
use serde_json::{Value, json};

/// O id do lote sintético dos rascunhos que o servidor deixou sem lote.
const SEM_LOTE: &str = "sem-lote";
/// Quanto tempo vale a leitura do estado de um lote no inventário (a janela pergunta a cada
/// quadro).
const VALIDADE_DA_SITUACAO: Duration = Duration::from_secs(1);

fn travar<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

// ───────────────────────────── o envio de lote ─────────────────────────────

/// O que o inventário diz de um lote, para decidir o botão.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SituacaoDoLote {
    pub aprovado: bool,
    pub promovido: bool,
    /// Quantos documentos o lote sobe.
    pub documentos: usize,
}

/// Um envio pronto para rodar numa thread: recebe o callback de andamento.
pub type TrabalhoDeEnvio =
    Box<dyn FnOnce(&mut dyn FnMut(&AndamentoDoEnvio)) -> Result<ResumoDoEnvio, String> + Send>;

/// A ligação com o inventário e o servidor para o envio. A real abre o `inventario.sqlite` e
/// usa o cliente da estação; os testes trocam por um dublê.
pub trait PortaDoEnvio: Send + Sync {
    /// O estado do lote no inventário. Chamado fora do quadro? Não: a janela o chama a cada
    /// quadro, por isso o adaptador guarda a resposta por [`VALIDADE_DA_SITUACAO`].
    ///
    /// # Errors
    /// Texto para a tela.
    fn situacao(&self, lote: &Ref) -> Result<SituacaoDoLote, String>;

    /// Prepara o envio na thread da janela (pega o cliente da estação) e devolve o trabalho
    /// que roda fora do quadro.
    ///
    /// # Errors
    /// Texto para a tela.
    fn preparar(&self, lote: &Ref) -> Result<TrabalhoDeEnvio, String>;
}

/// O envio real: `inventario.sqlite` e cofre em disco, cliente da estação.
pub struct EnvioReal {
    estacao: Arc<Mutex<EstacaoLocal>>,
    banco: PathBuf,
    cofre: PathBuf,
}

impl EnvioReal {
    pub fn novo(estacao: Arc<Mutex<EstacaoLocal>>, cofre: PathBuf) -> Self {
        Self {
            estacao,
            banco: cofre.join("inventario.sqlite"),
            cofre,
        }
    }
}

impl PortaDoEnvio for EnvioReal {
    fn situacao(&self, lote: &Ref) -> Result<SituacaoDoLote, String> {
        let inv = Inventario::abrir(&self.banco).map_err(|e| e.to_string())?;
        let id: i64 = lote
            .chave
            .parse()
            .map_err(|_| "o lote não existe".to_owned())?;
        let (aprovado, promovido): (Option<String>, Option<String>) = inv
            .conexao()
            .query_row(
                "SELECT aprovado_em, promovido_em FROM lote WHERE id = ?1",
                [id],
                |l| Ok((l.get(0)?, l.get(1)?)),
            )
            .map_err(|_| "o lote não existe".to_owned())?;
        let (documentos, _) = documentos_do_lote(inv.conexao(), lote).map_err(|e| e.to_string())?;
        Ok(SituacaoDoLote {
            aprovado: aprovado.is_some(),
            promovido: promovido.is_some(),
            documentos: documentos.len(),
        })
    }

    fn preparar(&self, lote: &Ref) -> Result<TrabalhoDeEnvio, String> {
        let cliente: ClienteDoServidor = travar(&self.estacao).cliente();
        let banco = self.banco.clone();
        let leitor = LeitorDeCofre::novo(self.cofre.clone());
        let lote = lote.clone();
        Ok(Box::new(move |progresso| {
            // A janela guarda o `Inventario` como `Comandos`; o envio abre o seu.
            let mut inventario = Inventario::abrir(&banco).map_err(|e| e.to_string())?;
            sbf_estacao::enviar_lote(&cliente, &mut inventario, &lote, &leitor, progresso)
                .map_err(|e| e.to_string())
        }))
    }
}

enum Evento {
    Andamento(ProgressoDoEnvio),
    Fim(Result<ResumoDoEnvio, String>),
}

enum EstadoLocal {
    Enviando(ProgressoDoEnvio),
    Enviado {
        documentos: usize,
    },
    Falhou {
        progresso: ProgressoDoEnvio,
        motivo: String,
    },
}

struct EnvioEmCurso {
    eventos: Receiver<Evento>,
}

#[derive(Default)]
struct Envios {
    locais: BTreeMap<String, EstadoLocal>,
    em_curso: BTreeMap<String, EnvioEmCurso>,
    situacoes: BTreeMap<String, (Instant, Result<SituacaoDoLote, String>)>,
}

// ───────────────────────────── o adaptador ─────────────────────────────

/// A caixa de entrada ligada à estação real.
pub struct CaixaDaEstacao {
    estacao: Arc<Mutex<EstacaoLocal>>,
    /// Sobe a cada escrita: o quadro relê a cópia do livro (a mesma da `Alteracoes`).
    escritas: Arc<AtomicU64>,
    porta: Option<Arc<dyn PortaDoEnvio>>,
    envios: Mutex<Envios>,
}

impl std::fmt::Debug for CaixaDaEstacao {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CaixaDaEstacao").finish_non_exhaustive()
    }
}

fn id_de(texto: &str, o_que: &str) -> ResultadoDaCaixa<Id> {
    Id::parse_str(texto).map_err(|_| ErroDaCaixa::NaoEncontrado(o_que.to_owned()))
}

fn id_da_movimentacao(alvo: &Ref) -> ResultadoDaCaixa<Id> {
    if alvo.tipo != Tipo::Movimentacao {
        return Err(ErroDaCaixa::Invalido(
            "só movimentações do livro-caixa se editam aqui".to_owned(),
        ));
    }
    id_de(&alvo.chave, "movimentação")
}

fn erro(e: ErroDeEscrita) -> ErroDaCaixa {
    match e {
        ErroDeEscrita::NaoEncontrado(o) => ErroDaCaixa::NaoEncontrado(o),
        ErroDeEscrita::Inexistente(id) => ErroDaCaixa::NaoEncontrado(format!("escrita {id}")),
        ErroDeEscrita::Local(m) => ErroDaCaixa::Local(m),
        outro => ErroDaCaixa::Invalido(outro.to_string()),
    }
}

fn texto(v: &Value, chave: &str) -> Option<String> {
    v.get(chave)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
}

/// O `draft_data` v1 como o formulário o vê.
pub fn campos_do_rascunho(dados: &Value) -> CamposDoRascunho {
    CamposDoRascunho {
        direcao: if dados.get("direction").and_then(Value::as_str) == Some("inflow") {
            Direcao::Entrada
        } else {
            Direcao::Saida
        },
        tipo: texto(dados, "transaction_type"),
        valor_centavos: dados.get("amount_cents").and_then(Value::as_i64),
        data_do_evento: texto(dados, "event_date"),
        vencimento: texto(dados, "due_date"),
        descricao: texto(dados, "description"),
        fornecedor: texto(dados, "supplier_name_raw"),
        categoria: texto(dados, "category_suggestion"),
        conta: texto(dados, "financial_product_id"),
        numero_do_documento: texto(dados, "document_number"),
        observacoes: texto(dados, "notes"),
    }
}

/// Os campos do formulário sobre o `draft_data` que já existe: o que o formulário não
/// conhece (`schema_version`, `draft_type`, `tags`, `barcode_digitable_line`…) fica como está.
pub fn dados_com(base: &Value, c: &CamposDoRascunho) -> Value {
    let mut dados = base.clone();
    if !dados.is_object() {
        dados = json!({});
    }
    let Some(o) = dados.as_object_mut() else {
        return dados;
    };
    let opcional = |t: &Option<String>| {
        t.as_ref()
            .map(|t| t.trim())
            .filter(|t| !t.is_empty())
            .map_or(Value::Null, |t| Value::String(t.to_owned()))
    };
    o.insert(
        "direction".to_owned(),
        Value::String(
            match c.direcao {
                Direcao::Saida => "outflow",
                Direcao::Entrada => "inflow",
            }
            .to_owned(),
        ),
    );
    o.insert("transaction_type".to_owned(), opcional(&c.tipo));
    o.insert(
        "amount_cents".to_owned(),
        c.valor_centavos.map_or(Value::Null, Value::from),
    );
    o.insert("event_date".to_owned(), opcional(&c.data_do_evento));
    o.insert("due_date".to_owned(), opcional(&c.vencimento));
    o.insert("description".to_owned(), opcional(&c.descricao));
    o.insert("supplier_name_raw".to_owned(), opcional(&c.fornecedor));
    o.insert("category_suggestion".to_owned(), opcional(&c.categoria));
    o.insert("financial_product_id".to_owned(), opcional(&c.conta));
    o.insert(
        "document_number".to_owned(),
        opcional(&c.numero_do_documento),
    );
    o.insert("notes".to_owned(), opcional(&c.observacoes));
    dados
}

fn status_do(e: EstadoDoRascunho) -> StatusDoRascunho {
    match e {
        EstadoDoRascunho::PendenteDeRevisao => StatusDoRascunho::EmRevisao,
        EstadoDoRascunho::Corrigido => StatusDoRascunho::Corrigido,
        EstadoDoRascunho::Aprovado | EstadoDoRascunho::Lancado => StatusDoRascunho::Aprovado,
        EstadoDoRascunho::Recusado | EstadoDoRascunho::Arquivado => StatusDoRascunho::Recusado,
    }
}

fn estado_de_envio(e: EstadoDaEscrita) -> Option<EstadoDeEnvio> {
    match e {
        EstadoDaEscrita::Aguardando => Some(EstadoDeEnvio::Pendente),
        EstadoDaEscrita::Aceita => Some(EstadoDeEnvio::Aceita),
        EstadoDaEscrita::Recusada => Some(EstadoDeEnvio::Recusada),
        // Refeita e descartada saem da lista: a refeita é outra escrita.
        EstadoDaEscrita::Refeita | EstadoDaEscrita::Descartada => None,
    }
}

fn o_que_faz(a: &Acao) -> &'static str {
    match a {
        Acao::MudarCategoriaDaMovimentacao { .. } => "Mudar categoria",
        Acao::CorrigirRascunho { .. } => "Corrigir rascunho",
        Acao::AprovarRascunho { .. } => "Aprovar rascunho",
        Acao::RecusarRascunho { .. } => "Recusar rascunho",
        Acao::AprovarLoteDeRascunhos { .. } => "Aprovar lote",
        Acao::EditarMovimentacao { .. } => "Editar movimentação",
        Acao::ExcluirMovimentacao { .. } => "Excluir movimentação",
        Acao::RatearMovimentacao { .. } => "Ratear movimentação",
        Acao::ConciliarRascunho { .. } => "Conciliar rascunho",
    }
}

fn alvo_da(a: &Acao) -> AlvoDaEscrita {
    let mov = |id: &Id| AlvoDaEscrita::Movimentacao(Ref::nova(Tipo::Movimentacao, id.to_string()));
    match a {
        Acao::MudarCategoriaDaMovimentacao { movimentacao, .. }
        | Acao::EditarMovimentacao { movimentacao, .. }
        | Acao::ExcluirMovimentacao { movimentacao }
        | Acao::RatearMovimentacao { movimentacao, .. } => mov(movimentacao),
        Acao::CorrigirRascunho { rascunho, .. }
        | Acao::AprovarRascunho { rascunho }
        | Acao::RecusarRascunho { rascunho, .. }
        | Acao::ConciliarRascunho { rascunho, .. } => AlvoDaEscrita::Rascunho(rascunho.to_string()),
        Acao::AprovarLoteDeRascunhos { lote } => AlvoDaEscrita::Lote(lote.to_string()),
    }
}

/// "Aprovar rascunho · Mercado Delta" → "Mercado Delta".
fn rotulo_do_alvo(rotulo: &str) -> String {
    rotulo
        .split_once(" · ")
        .map_or(rotulo, |(_, resto)| resto)
        .to_owned()
}

fn com_sinal(m: &Movimentacao) -> Centavos {
    let v = m.valor.abs();
    match m.tipo {
        TipoDeMovimentacao::Entrada | TipoDeMovimentacao::Estorno => v,
        TipoDeMovimentacao::Ajuste => m.valor,
        _ => -v,
    }
}

fn valor_ou_traco(v: Option<String>) -> String {
    v.filter(|t| !t.trim().is_empty())
        .unwrap_or_else(|| "—".to_owned())
}

impl CaixaDaEstacao {
    /// `escritas` é o contador que o quadro observa para reler a cópia do livro.
    pub fn nova(
        estacao: Arc<Mutex<EstacaoLocal>>,
        escritas: Arc<AtomicU64>,
        porta: Option<Arc<dyn PortaDoEnvio>>,
    ) -> Self {
        Self {
            estacao,
            escritas,
            porta,
            envios: Mutex::new(Envios::default()),
        }
    }

    fn com_cache<T>(
        &self,
        f: impl FnOnce(&sbf_estacao::CacheDoLivro) -> Result<T, sbf_estacao::ErroDoCache>,
    ) -> ResultadoDaCaixa<Option<T>> {
        let e = travar(&self.estacao);
        match e.cache() {
            Some(c) => f(c)
                .map(Some)
                .map_err(|x| ErroDaCaixa::Local(x.to_string())),
            None => Ok(None),
        }
    }

    fn enfileirar(&mut self, acao: Acao) -> ResultadoDaCaixa<Escrita> {
        let r = travar(&self.estacao).enfileirar(acao).map_err(erro)?;
        self.escritas.fetch_add(1, Ordering::AcqRel);
        Ok(self.escrita_da(&r))
    }

    fn escrita_da(&self, e: &EscritaDaEstacao) -> Escrita {
        let lado_a_lado = if e.estado == EstadoDaEscrita::Recusada {
            self.lado_a_lado(e)
        } else {
            Vec::new()
        };
        Escrita {
            id: e.id.to_string(),
            o_que: o_que_faz(&e.acao).to_owned(),
            rotulo_do_alvo: rotulo_do_alvo(&e.rotulo),
            alvo: alvo_da(&e.acao),
            feita_em: e.feita_em.clone(),
            estado: estado_de_envio(e.estado).unwrap_or(EstadoDeEnvio::Pendente),
            recusa: e.recusa.as_ref().map(|r| RecusaDaEscrita {
                motivo: r.mensagem.clone(),
                quando: r.quando.clone(),
                campos: lado_a_lado.clone(),
            }),
        }
    }

    /// "Sua versão" × "No servidor agora" da escrita recusada.
    fn lado_a_lado(&self, e: &EscritaDaEstacao) -> Vec<CampoLadoALado> {
        let Some(recusa) = &e.recusa else {
            return Vec::new();
        };
        let nome_da_conta = |id: &str| -> String {
            Id::parse_str(id)
                .ok()
                .and_then(|i| {
                    self.com_cache(|c| c.linha::<Conta>(Colecao::Contas, &i.to_string()))
                        .ok()
                        .flatten()
                        .flatten()
                })
                .map_or_else(|| id.to_owned(), |c| c.nome)
        };
        let agora_mov = recusa.no_servidor_agora.iter().find_map(|l| match l {
            LinhaAgora::Movimentacoes(m) => Some(m.as_ref()),
            _ => None,
        });
        let agora_rasc = recusa.no_servidor_agora.iter().find_map(|l| match l {
            LinhaAgora::Rascunhos(r) => Some(r.as_ref()),
            _ => None,
        });
        match &e.acao {
            Acao::EditarMovimentacao { edicao, .. } => {
                vec![
                    CampoLadoALado::novo(
                        "Descrição",
                        valor_ou_traco(edicao.descricao.clone()),
                        valor_ou_traco(agora_mov.and_then(|m| m.descricao.clone())),
                    ),
                    CampoLadoALado::novo(
                        "Data",
                        edicao.data.clone(),
                        agora_mov.map_or_else(|| "—".to_owned(), |m| m.data.clone()),
                    ),
                    CampoLadoALado::novo(
                        "Valor",
                        dinheiro(edicao.valor),
                        agora_mov.map_or_else(|| "—".to_owned(), |m| dinheiro(m.valor.abs())),
                    ),
                    CampoLadoALado::novo(
                        "Conta",
                        nome_da_conta(&edicao.conta.to_string()),
                        agora_mov.map_or_else(
                            || "—".to_owned(),
                            |m| nome_da_conta(&m.conta.to_string()),
                        ),
                    ),
                    CampoLadoALado::novo(
                        "Notas",
                        valor_ou_traco(edicao.notas.clone()),
                        valor_ou_traco(agora_mov.and_then(|m| m.notas.clone())),
                    ),
                ]
            }
            Acao::CorrigirRascunho { dados, .. } => {
                let sua = campos_do_rascunho(dados);
                let servidor = agora_rasc.map(|r| campos_do_rascunho(&r.dados));
                let dinheiro_de = |v: Option<i64>| v.map_or_else(|| "—".to_owned(), dinheiro);
                let conta_de = |c: &Option<String>| {
                    c.as_deref()
                        .map_or_else(|| "—".to_owned(), &nome_da_conta)
                };
                vec![
                    CampoLadoALado::novo(
                        "Descrição",
                        valor_ou_traco(sua.descricao.clone()),
                        valor_ou_traco(servidor.as_ref().and_then(|s| s.descricao.clone())),
                    ),
                    CampoLadoALado::novo(
                        "Valor",
                        dinheiro_de(sua.valor_centavos),
                        dinheiro_de(servidor.as_ref().and_then(|s| s.valor_centavos)),
                    ),
                    CampoLadoALado::novo(
                        "Data do evento",
                        valor_ou_traco(sua.data_do_evento.clone()),
                        valor_ou_traco(servidor.as_ref().and_then(|s| s.data_do_evento.clone())),
                    ),
                    CampoLadoALado::novo(
                        "Fornecedor",
                        valor_ou_traco(sua.fornecedor.clone()),
                        valor_ou_traco(servidor.as_ref().and_then(|s| s.fornecedor.clone())),
                    ),
                    CampoLadoALado::novo(
                        "Conta ou cartão",
                        conta_de(&sua.conta),
                        servidor
                            .as_ref()
                            .map_or_else(|| "—".to_owned(), |s| conta_de(&s.conta)),
                    ),
                ]
            }
            outra => {
                let situacao = agora_rasc.map_or_else(
                    || {
                        agora_mov.map_or_else(
                            || "não existe mais".to_owned(),
                            |_| "mudou no servidor".to_owned(),
                        )
                    },
                    |r| status_do(r.estado).rotulo().to_owned(),
                );
                vec![CampoLadoALado::novo(
                    "Situação",
                    o_que_faz(outra).to_owned(),
                    situacao,
                )]
            }
        }
    }

    fn rascunho_do_cache(&self, id: Id) -> ResultadoDaCaixa<RascunhoDoContrato> {
        self.com_cache(|c| c.linha::<RascunhoDoContrato>(Colecao::Rascunhos, &id.to_string()))?
            .flatten()
            .ok_or_else(|| ErroDaCaixa::NaoEncontrado("rascunho".to_owned()))
    }

    fn situacao_do_lote(&self, lote: &Ref) -> Result<SituacaoDoLote, String> {
        let Some(porta) = &self.porta else {
            return Err("O envio ao servidor não está ligado neste modo.".to_owned());
        };
        let mut envios = travar(&self.envios);
        if let Some((quando, r)) = envios.situacoes.get(&lote.chave)
            && quando.elapsed() < VALIDADE_DA_SITUACAO
        {
            return r.clone();
        }
        // A consulta é rápida (SQLite local) e fica em cache por um segundo.
        let r = porta.situacao(lote);
        envios
            .situacoes
            .insert(lote.chave.clone(), (Instant::now(), r.clone()));
        r
    }

    fn absorver(&self, chave: &str, evento: Evento, envios: &mut Envios) {
        match evento {
            Evento::Andamento(p) => {
                envios
                    .locais
                    .insert(chave.to_owned(), EstadoLocal::Enviando(p));
            }
            Evento::Fim(resultado) => {
                let ultimo = match envios.locais.get(chave) {
                    Some(EstadoLocal::Enviando(p)) => *p,
                    _ => ProgressoDoEnvio {
                        enviados: 0,
                        total: 0,
                    },
                };
                let novo = match resultado {
                    Ok(r) if r.interrompido => EstadoLocal::Falhou {
                        progresso: ultimo,
                        motivo: "a rede caiu; enviar de novo retoma de onde parou".to_owned(),
                    },
                    Ok(r) if !r.falhas.is_empty() => EstadoLocal::Falhou {
                        progresso: ultimo,
                        motivo: format!("{}: {}", r.falhas[0].nome, r.falhas[0].motivo),
                    },
                    Ok(r) => EstadoLocal::Enviado {
                        documentos: r.total,
                    },
                    Err(m) => EstadoLocal::Falhou {
                        progresso: ultimo,
                        motivo: m,
                    },
                };
                envios.locais.insert(chave.to_owned(), novo);
                // O inventário mudou (registros promovidos): a próxima leitura é nova.
                envios.situacoes.remove(chave);
            }
        }
    }
}

impl FonteDaCaixa for CaixaDaEstacao {
    fn lotes(&self) -> ResultadoDaCaixa<Vec<LoteDeRascunhos>> {
        let Some((grupos, documentos)) = self.com_cache(|c| {
            let grupos = c.caixa_de_entrada(false)?;
            let docs: Vec<Documento> = c.linhas(Colecao::Documentos)?;
            Ok((grupos, docs))
        })?
        else {
            return Ok(Vec::new());
        };
        let por_id: BTreeMap<Id, &Documento> = documentos.iter().map(|d| (d.id, d)).collect();
        let mut saida = Vec::new();
        for g in grupos {
            let rascunhos: Vec<Rascunho> =
                g.itens.iter().map(|i| rascunho_da(i, &por_id)).collect();
            let origem = g
                .itens
                .iter()
                .find_map(|i| i.rascunho.documento.and_then(|d| por_id.get(&d)))
                .map_or_else(
                    || "Servidor".to_owned(),
                    |d| match d.origem {
                        OrigemDoDocumento::Gmail => "Gmail".to_owned(),
                        OrigemDoDocumento::ArquivoLocal => "Pasta local".to_owned(),
                        OrigemDoDocumento::EnvioManual => "Envio manual".to_owned(),
                    },
                );
            saida.push(lote_da(g.lote.as_ref(), origem, rascunhos));
        }
        Ok(saida)
    }

    fn contas(&self) -> Vec<Opcao> {
        self.com_cache(|c| c.linhas::<Conta>(Colecao::Contas))
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.ativa)
            .map(|c| Opcao {
                id: c.id.to_string(),
                nome: c.nome,
            })
            .collect()
    }

    fn centros(&self) -> Vec<Opcao> {
        self.com_cache(|c| c.linhas::<Centro>(Colecao::Centros))
            .ok()
            .flatten()
            .unwrap_or_default()
            .into_iter()
            .filter(|c| c.estado == EstadoDoCentro::Ativo)
            .map(|c| Opcao {
                id: c.id.to_string(),
                nome: c.nome,
            })
            .collect()
    }

    fn movimentacao(&self, alvo: &Ref) -> ResultadoDaCaixa<Option<MovimentacaoEditavel>> {
        if alvo.tipo != Tipo::Movimentacao {
            return Ok(None);
        }
        let Ok(id) = Id::parse_str(&alvo.chave) else {
            return Ok(None);
        };
        let lido = self.com_cache(|c| {
            let m = c.linha::<Movimentacao>(Colecao::Movimentacoes, &id.to_string())?;
            let Some(m) = m else { return Ok(None) };
            let rateios: Vec<Rateio> = c
                .linhas::<Rateio>(Colecao::Rateios)?
                .into_iter()
                .filter(|r| r.movimentacao == id)
                .collect();
            let fornecedor = match m.fornecedor {
                Some(f) => c
                    .linha::<Fornecedor>(Colecao::Fornecedores, &f.to_string())?
                    .map(|f| f.nome),
                None => None,
            };
            Ok(Some((m, rateios, fornecedor)))
        })?;
        let Some((m, rateios, fornecedor)) = lido.flatten() else {
            return Ok(None);
        };
        let centro = match rateios.as_slice() {
            [um] => Some(um.centro.to_string()),
            _ => None,
        };
        Ok(Some(MovimentacaoEditavel {
            alvo: alvo.clone(),
            campos: CamposDaMovimentacao {
                descricao: m.descricao.clone().unwrap_or_default(),
                data: m.data.clone(),
                valor_centavos: com_sinal(&m),
                conta: Some(m.conta.to_string()),
                fornecedor,
                centro,
                notas: m.notas.clone(),
            },
            rateios: rateios
                .iter()
                .map(|r| RateioDaMovimentacao {
                    centro: r.centro.to_string(),
                    centavos: r.valor.abs(),
                })
                .collect(),
        }))
    }

    fn escritas(&self) -> ResultadoDaCaixa<Vec<Escrita>> {
        let todas = travar(&self.estacao).escritas(false).map_err(erro)?;
        Ok(todas
            .iter()
            .rev()
            .filter(|e| estado_de_envio(e.estado).is_some())
            .map(|e| self.escrita_da(e))
            .collect())
    }

    fn envio_do_lote(&self, lote: &Ref) -> EnvioDoLote {
        {
            let envios = travar(&self.envios);
            match envios.locais.get(&lote.chave) {
                Some(EstadoLocal::Enviando(p)) => return EnvioDoLote::Enviando(*p),
                Some(EstadoLocal::Falhou { progresso, motivo }) => {
                    return EnvioDoLote::Falhou {
                        progresso: *progresso,
                        motivo: motivo.clone(),
                    };
                }
                Some(EstadoLocal::Enviado { documentos }) => {
                    return EnvioDoLote::Enviado {
                        documentos: *documentos,
                    };
                }
                None => {}
            }
        }
        match self.situacao_do_lote(lote) {
            Err(m) => EnvioDoLote::Indisponivel(m),
            Ok(s) if !s.aprovado => EnvioDoLote::Indisponivel(
                "O lote ainda não foi aprovado: aprove antes de enviar.".to_owned(),
            ),
            Ok(s) if s.promovido => EnvioDoLote::Enviado {
                documentos: s.documentos,
            },
            Ok(s) if s.documentos == 0 => EnvioDoLote::Indisponivel(
                "O lote não tem documentos para enviar (conexões bancárias não sobem).".to_owned(),
            ),
            Ok(s) => EnvioDoLote::Pronto {
                documentos: s.documentos,
            },
        }
    }
}

fn lote_da(
    lote: Option<&LoteDoContrato>,
    origem: String,
    rascunhos: Vec<Rascunho>,
) -> LoteDeRascunhos {
    match lote {
        Some(l) => LoteDeRascunhos {
            id: l.id.to_string(),
            nome: l.nome.clone().unwrap_or_else(|| "Lote".to_owned()),
            origem,
            criado_em: l.criado_em.clone().unwrap_or_default(),
            rascunhos,
        },
        None => LoteDeRascunhos {
            id: SEM_LOTE.to_owned(),
            nome: "Sem lote".to_owned(),
            origem,
            criado_em: String::new(),
            rascunhos,
        },
    }
}

fn rascunho_da(item: &ItemDaCaixa, documentos: &BTreeMap<Id, &Documento>) -> Rascunho {
    let r = &item.rascunho;
    let campos = campos_do_rascunho(&r.dados);
    let doc = r.documento.and_then(|d| documentos.get(&d).copied());
    let rotulo = doc
        .map(|d| d.nome_do_arquivo.clone())
        .or_else(|| campos.descricao.clone())
        .unwrap_or_else(|| "Rascunho".to_owned());
    Rascunho {
        id: r.id.to_string(),
        lote: r
            .lote
            .map_or_else(|| SEM_LOTE.to_owned(), |l| l.to_string()),
        rotulo,
        // O achado do inventário que tem o mesmo conteúdo (a prévia ao lado).
        documento: doc
            .and_then(|d| d.hash_do_conteudo.clone())
            .map(|h| Ref::nova(Tipo::Achado, h)),
        status: status_do(r.estado),
        envio: if item.recusado {
            Some(EstadoDeEnvio::Recusada)
        } else if item.pendente {
            Some(EstadoDeEnvio::Pendente)
        } else {
            None
        },
        campos,
        motivo_da_recusa: r
            .correcoes
            .as_ref()
            .and_then(|c| texto(c, "rejection_reason")),
    }
}

impl Editor for CaixaDaEstacao {
    fn corrigir(&mut self, id: &str, campos: CamposDoRascunho) -> ResultadoDaCaixa<Escrita> {
        let rid = id_de(id, "rascunho")?;
        let atual = self.rascunho_do_cache(rid)?;
        let dados = dados_com(&atual.dados, &campos);
        self.enfileirar(Acao::CorrigirRascunho {
            rascunho: rid,
            dados,
        })
    }

    fn aprovar(&mut self, id: &str) -> ResultadoDaCaixa<Escrita> {
        let rid = id_de(id, "rascunho")?;
        let atual = self.rascunho_do_cache(rid)?;
        let falta = campos_do_rascunho(&atual.dados).faltando();
        if !falta.is_empty() {
            return Err(ErroDaCaixa::Invalido(format!(
                "Falta para lançar: {}.",
                falta.join(", ")
            )));
        }
        self.enfileirar(Acao::AprovarRascunho { rascunho: rid })
    }

    fn recusar(&mut self, id: &str, motivo: &str) -> ResultadoDaCaixa<Escrita> {
        if motivo.trim().is_empty() {
            return Err(ErroDaCaixa::Invalido("Diga o motivo da recusa.".to_owned()));
        }
        let rid = id_de(id, "rascunho")?;
        self.enfileirar(Acao::RecusarRascunho {
            rascunho: rid,
            motivo: motivo.trim().to_owned(),
        })
    }

    fn aprovar_lote(&mut self, lote: &str) -> ResultadoDaCaixa<Escrita> {
        let lid = id_de(lote, "lote")?;
        // Tudo ou nada: se um dos abertos não passa, nenhum muda.
        let abertos = self
            .com_cache(|c| c.linhas::<RascunhoDoContrato>(Colecao::Rascunhos))?
            .unwrap_or_default();
        let mut barrados = Vec::new();
        for r in abertos
            .iter()
            .filter(|r| r.lote == Some(lid) && r.estado.pendente())
        {
            let falta = campos_do_rascunho(&r.dados).faltando();
            if !falta.is_empty() {
                barrados.push(format!("{} ({})", rotulo_curto(r), falta.join(", ")));
            }
        }
        if !barrados.is_empty() {
            return Err(ErroDaCaixa::Invalido(format!(
                "{} rascunhos ainda não podem ser lançados: {}. Nada foi aprovado.",
                barrados.len(),
                barrados.join("; ")
            )));
        }
        self.enfileirar(Acao::AprovarLoteDeRascunhos { lote: lid })
    }

    fn editar_movimentacao(
        &mut self,
        alvo: &Ref,
        campos: CamposDaMovimentacao,
    ) -> ResultadoDaCaixa<Escrita> {
        let mid = id_da_movimentacao(alvo)?;
        let conta = campos
            .conta
            .as_deref()
            .ok_or_else(|| ErroDaCaixa::Invalido("Escolha a conta.".to_owned()))
            .and_then(|c| id_de(c, "conta"))?;
        let fornecedor = self.fornecedor_pelo_nome(campos.fornecedor.as_deref())?;
        let centro = campos
            .centro
            .as_deref()
            .filter(|c| !c.trim().is_empty())
            .map(|c| id_de(c, "centro"))
            .transpose()?;
        self.enfileirar(Acao::EditarMovimentacao {
            movimentacao: mid,
            edicao: EdicaoDaMovimentacao {
                descricao: Some(campos.descricao.trim().to_owned()).filter(|d| !d.is_empty()),
                data: campos.data,
                valor: campos.valor_centavos.abs(),
                conta,
                fornecedor,
                centro,
                notas: campos
                    .notas
                    .map(|n| n.trim().to_owned())
                    .filter(|n| !n.is_empty()),
            },
        })
    }

    fn ratear(
        &mut self,
        alvo: &Ref,
        linhas: Vec<RateioDaMovimentacao>,
    ) -> ResultadoDaCaixa<Escrita> {
        let mid = id_da_movimentacao(alvo)?;
        let rateios = linhas
            .iter()
            .map(|l| {
                Ok(NovoRateio {
                    id: Id::new_v4(),
                    centro: id_de(&l.centro, "centro")?,
                    valor: l.centavos,
                    nota: None,
                })
            })
            .collect::<ResultadoDaCaixa<Vec<_>>>()?;
        self.enfileirar(Acao::RatearMovimentacao {
            movimentacao: mid,
            rateios,
        })
    }

    fn excluir(&mut self, alvo: &Ref) -> ResultadoDaCaixa<Escrita> {
        let mid = id_da_movimentacao(alvo)?;
        self.enfileirar(Acao::ExcluirMovimentacao { movimentacao: mid })
    }

    fn refazer(&mut self, id: &str) -> ResultadoDaCaixa<Escrita> {
        let eid = id_de(id, "escrita")?;
        let nova = travar(&self.estacao).refazer_escrita(eid).map_err(erro)?;
        self.escritas.fetch_add(1, Ordering::AcqRel);
        Ok(self.escrita_da(&nova))
    }

    fn descartar(&mut self, id: &str) -> ResultadoDaCaixa<()> {
        let eid = id_de(id, "escrita")?;
        travar(&self.estacao).descartar_escrita(eid).map_err(erro)?;
        self.escritas.fetch_add(1, Ordering::AcqRel);
        Ok(())
    }

    fn enviar_lote(&mut self, lote: &Ref) -> ResultadoDaCaixa<()> {
        let Some(porta) = self.porta.clone() else {
            return Err(ErroDaCaixa::Invalido(
                "O envio ao servidor não está ligado neste modo.".to_owned(),
            ));
        };
        match self.envio_do_lote(lote) {
            EnvioDoLote::Indisponivel(m) => return Err(ErroDaCaixa::Invalido(m)),
            EnvioDoLote::Enviando(_) => {
                return Err(ErroDaCaixa::Invalido(
                    "O envio deste lote já está em andamento.".to_owned(),
                ));
            }
            EnvioDoLote::Enviado { .. } => {
                return Err(ErroDaCaixa::Invalido(
                    "Este lote já foi enviado.".to_owned(),
                ));
            }
            EnvioDoLote::Pronto { .. } | EnvioDoLote::Falhou { .. } => {}
        }
        let trabalho = porta.preparar(lote).map_err(ErroDaCaixa::Local)?;
        let (tx, rx): (Sender<Evento>, Receiver<Evento>) = channel();
        let chave = lote.chave.clone();
        let iniciada = thread::Builder::new()
            .name("sbf-envio-de-lote".to_owned())
            .spawn(move || {
                let tx_andamento = tx.clone();
                let mut progresso = move |a: &AndamentoDoEnvio| {
                    let _ = tx_andamento.send(Evento::Andamento(ProgressoDoEnvio {
                        enviados: a.feitos,
                        total: a.total,
                    }));
                };
                // Um pânico não pode deixar a barra parada para sempre.
                let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    trabalho(&mut progresso)
                }))
                .unwrap_or_else(|_| Err("o envio parou de repente".to_owned()));
                let _ = tx.send(Evento::Fim(r));
            });
        if iniciada.is_err() {
            return Err(ErroDaCaixa::Local(
                "não foi possível iniciar o envio".to_owned(),
            ));
        }
        let mut envios = travar(&self.envios);
        envios.locais.insert(
            chave.clone(),
            EstadoLocal::Enviando(ProgressoDoEnvio {
                enviados: 0,
                total: 0,
            }),
        );
        envios.em_curso.insert(chave, EnvioEmCurso { eventos: rx });
        Ok(())
    }

    fn tique(&mut self) -> bool {
        let mut envios = travar(&self.envios);
        if envios.em_curso.is_empty() {
            return false;
        }
        let mut mudou = false;
        let chaves: Vec<String> = envios.em_curso.keys().cloned().collect();
        for chave in chaves {
            let mut recebidos = Vec::new();
            let mut desligou = false;
            if let Some(c) = envios.em_curso.get(&chave) {
                loop {
                    match c.eventos.try_recv() {
                        Ok(e) => recebidos.push(e),
                        Err(std::sync::mpsc::TryRecvError::Empty) => break,
                        Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                            desligou = true;
                            break;
                        }
                    }
                }
            }
            let mut terminou = false;
            for e in recebidos {
                mudou = true;
                terminou |= matches!(e, Evento::Fim(_));
                self.absorver(&chave, e, &mut envios);
            }
            if desligou && !terminou {
                // A thread morreu sem avisar.
                mudou = true;
                terminou = true;
                self.absorver(
                    &chave,
                    Evento::Fim(Err("o envio parou de repente".to_owned())),
                    &mut envios,
                );
            }
            if terminou {
                envios.em_curso.remove(&chave);
            }
        }
        mudou
    }
}

fn rotulo_curto(r: &RascunhoDoContrato) -> String {
    texto(&r.dados, "description")
        .or_else(|| texto(&r.dados, "supplier_name_raw"))
        .unwrap_or_else(|| "Rascunho".to_owned())
}

impl CaixaDaEstacao {
    /// O fornecedor pelo nome digitado. Vazio tira o fornecedor; um nome que ninguém tem é
    /// erro (não cria fornecedor aqui).
    fn fornecedor_pelo_nome(&self, nome: Option<&str>) -> ResultadoDaCaixa<Option<Id>> {
        let Some(nome) = nome.map(str::trim).filter(|n| !n.is_empty()) else {
            return Ok(None);
        };
        let Some(todos) = self.com_cache(|c| c.linhas::<Fornecedor>(Colecao::Fornecedores))? else {
            return Ok(None);
        };
        let procurado = nome.to_lowercase();
        todos
            .iter()
            .find(|f| f.nome.trim().to_lowercase() == procurado)
            .map(|f| Some(f.id))
            .ok_or_else(|| {
                ErroDaCaixa::Invalido(format!(
                    "Não conheço o fornecedor \u{201c}{nome}\u{201d}. Escolha um já cadastrado ou deixe em branco."
                ))
            })
    }
}

#[cfg(test)]
mod testes {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;
    use sbf_contrato::{
        EstadoDaConciliacao, EstadoDoLote, OrigemDaMovimentacao, PropostaDeAlteracao,
        Rascunho as R, TipoDeConta, TipoDeRascunho, Versao,
    };
    use sbf_estacao::{CacheDoLivro, GuardaDeCredencial, GuardaEmMemoria, RecusaDaEscrita as RE};

    fn id(n: u128) -> Id {
        Id::from_u128(n)
    }

    fn conta(n: u128, nome: &str) -> Conta {
        Conta {
            id: id(n),
            versao: Versao("v1".into()),
            instituicao: id(800),
            nome: nome.into(),
            tipo: TipoDeConta::ContaCorrente,
            saldo_atual: None,
            limite_de_credito: None,
            ativa: true,
            ordem: None,
        }
    }

    fn centro(n: u128, nome: &str) -> Centro {
        Centro {
            id: id(n),
            versao: Versao("v1".into()),
            nome: nome.into(),
            tipo: sbf_contrato::TipoDeCentro::Pessoa,
            descricao: None,
            estado: EstadoDoCentro::Ativo,
            icone: None,
            cor: None,
        }
    }

    fn movimentacao(n: u128, descricao: &str, valor: Centavos) -> Movimentacao {
        Movimentacao {
            id: id(n),
            versao: Versao("v1".into()),
            conta: id(900),
            tipo: TipoDeMovimentacao::Saida,
            valor,
            descricao: Some(descricao.into()),
            data: "2026-11-04".into(),
            competencia: None,
            fatura: None,
            parcela_de_divida: None,
            categoria: None,
            prioridade: None,
            confirmada: true,
            notas: None,
            origem: OrigemDaMovimentacao::Manual,
            ocorrencia: None,
            fornecedor: None,
            documento: None,
            tags: vec![],
            criada_em: "2026-11-04T12:00:00Z".into(),
        }
    }

    fn lote(n: u128, nome: &str) -> LoteDoContrato {
        LoteDoContrato {
            id: id(n),
            versao: Versao("v1".into()),
            nome: Some(nome.into()),
            estado: EstadoDoLote::Aberto,
            total: 2,
            aprovados: 0,
            recusados: 0,
            documento: None,
            criado_em: Some("2026-11-13T12:00:00Z".into()),
        }
    }

    fn rascunho(n: u128, lote: Id, dados: Value) -> R {
        R {
            id: id(n),
            versao: Versao("v1".into()),
            lote: Some(lote),
            documento: None,
            tipo: TipoDeRascunho::Movimentacao,
            estado: EstadoDoRascunho::PendenteDeRevisao,
            dados,
            versao_do_esquema: 1,
            correcoes: None,
            confianca_percentual: Some(90),
            conciliacao: EstadoDaConciliacao::NaoVerificado,
            movimentacao_conciliada: None,
            lancado_como: None,
            criado_em: Some("2026-11-13T12:00:00Z".into()),
        }
    }

    /// Um rascunho completo (passa em `faltando`).
    fn completo(descricao: &str) -> Value {
        json!({
            "schema_version": 1, "draft_type": "transaction", "direction": "outflow",
            "transaction_type": "expense", "amount_cents": 12_990, "event_date": "2026-11-10",
            "description": descricao, "financial_product_id": id(900).to_string(),
            "tags": ["x"]
        })
    }

    struct Cena {
        _pasta: tempfile::TempDir,
        estacao: Arc<Mutex<EstacaoLocal>>,
        escritas: Arc<AtomicU64>,
    }

    /// Uma estação pareada de mentira, com o cache já cheio e sem rede (nada sincroniza).
    fn cena(linhas: Vec<LinhaAgora>) -> Cena {
        cena_com(linhas, Vec::new())
    }

    /// `extras`: linhas de coleções que `LinhaAgora` não cobre (contas, centros…), gravadas
    /// direto na tabela `linha` do cache.
    fn cena_com(linhas: Vec<LinhaAgora>, extras: Vec<(Colecao, Id, Value)>) -> Cena {
        let pasta = tempfile::tempdir().unwrap();
        let guarda = Arc::new(GuardaEmMemoria::default());
        let config = crate::estacao::configuracao(
            pasta.path(),
            guarda.clone(),
            crate::estacao::SERVIDOR_PADRAO,
            None,
        )
        .unwrap()
        .sincronizar_ao_abrir(false);
        guarda
            .gravar(
                config.servidor().host(),
                bancada_rede::Segredo::novo("sbfe_ficticia"),
            )
            .unwrap();
        {
            let mut cache = CacheDoLivro::abrir(&config.caminho_do_cache()).unwrap();
            cache.gravar_linhas(&linhas).unwrap();
        }
        if !extras.is_empty() {
            let conn = rusqlite::Connection::open(config.caminho_do_cache()).unwrap();
            for (colecao, id, dados) in extras {
                conn.execute(
                    "INSERT INTO linha (colecao, id, versao, dados) VALUES (?1, ?2, 'v1', ?3)",
                    rusqlite::params![colecao.caminho(), id.to_string(), dados.to_string()],
                )
                .unwrap();
            }
        }
        let estacao = EstacaoLocal::abrir(config).unwrap();
        Cena {
            _pasta: pasta,
            estacao: Arc::new(Mutex::new(estacao)),
            escritas: Arc::new(AtomicU64::new(0)),
        }
    }

    fn caixa(c: &Cena) -> CaixaDaEstacao {
        CaixaDaEstacao::nova(c.estacao.clone(), c.escritas.clone(), None)
    }

    fn base() -> Vec<LinhaAgora> {
        vec![
            LinhaAgora::Rateios(Rateio {
                id: id(70),
                movimentacao: id(2),
                centro: id(51),
                valor: 2500,
                nota: None,
                criado_em: "2026-11-04T12:00:00Z".into(),
            }),
            LinhaAgora::Movimentacoes(Box::new(movimentacao(2, "Padaria", 2500))),
            LinhaAgora::LotesDeRascunhos(lote(10, "Gmail · contas")),
            LinhaAgora::Rascunhos(Box::new(rascunho(20, id(10), completo("Internet fibra")))),
            LinhaAgora::Rascunhos(Box::new(rascunho(
                21,
                id(10),
                json!({"schema_version": 1, "draft_type": "transaction", "description": "Sem conta"}),
            ))),
        ]
    }

    #[test]
    fn dados_com_preserva_o_que_o_formulario_nao_conhece_e_volta_igual() {
        let base = completo("Internet");
        let campos = campos_do_rascunho(&base);
        assert_eq!(campos.valor_centavos, Some(12_990));
        assert_eq!(campos.direcao, Direcao::Saida);
        assert!(campos.faltando().is_empty());
        let volta = dados_com(&base, &campos);
        assert_eq!(volta["tags"], json!(["x"]));
        assert_eq!(volta["schema_version"], json!(1));
        assert_eq!(campos_do_rascunho(&volta), campos);
        // Entrada vira `inflow`; limpar um campo grava nulo.
        let mut c = campos;
        c.direcao = Direcao::Entrada;
        c.descricao = None;
        let v = dados_com(&base, &c);
        assert_eq!(v["direction"], json!("inflow"));
        assert!(v["description"].is_null());
    }

    #[test]
    fn le_lotes_contas_e_centros_do_cache() {
        let c = cena({
            let mut l = base();
            l.push(LinhaAgora::Movimentacoes(Box::new(movimentacao(3, "x", 1))));
            l
        });
        let cx = caixa(&c);
        let lotes = cx.lotes().unwrap();
        assert_eq!(lotes.len(), 1);
        assert_eq!(lotes[0].nome, "Gmail · contas");
        assert_eq!(lotes[0].rascunhos.len(), 2);
        assert_eq!(lotes[0].n_abertos(), 2);
        let r = lotes[0]
            .rascunhos
            .iter()
            .find(|r| r.id == id(20).to_string())
            .unwrap();
        assert_eq!(r.rotulo, "Internet fibra");
        assert_eq!(r.status, StatusDoRascunho::EmRevisao);
        assert_eq!(r.envio, None);
        assert!(r.campos.faltando().is_empty());
        let sem_conta = lotes[0]
            .rascunhos
            .iter()
            .find(|r| r.id == id(21).to_string())
            .unwrap();
        assert_eq!(
            sem_conta.campos.faltando(),
            ["Conta ou cartão", "Tipo", "Valor", "Data do evento"]
        );
    }

    #[test]
    fn corrigir_aparece_na_hora_como_pendente_e_entra_na_fila() {
        let c = cena(base());
        let mut cx = caixa(&c);
        let mut campos = campos_do_rascunho(&completo("Internet fibra"));
        campos.descricao = Some("Internet fibra 500".into());
        let e = cx.corrigir(&id(20).to_string(), campos).unwrap();
        assert_eq!(e.o_que, "Corrigir rascunho");
        assert_eq!(e.estado, EstadoDeEnvio::Pendente);
        assert_eq!(e.alvo, AlvoDaEscrita::Rascunho(id(20).to_string()));
        assert_eq!(c.escritas.load(Ordering::Acquire), 1);

        let lotes = cx.lotes().unwrap();
        let r = lotes[0]
            .rascunhos
            .iter()
            .find(|r| r.id == id(20).to_string())
            .unwrap();
        assert_eq!(r.campos.descricao.as_deref(), Some("Internet fibra 500"));
        assert_eq!(r.envio, Some(EstadoDeEnvio::Pendente));
        assert_eq!(r.status, StatusDoRascunho::Corrigido);
        let fila = cx.escritas().unwrap();
        assert_eq!(fila.len(), 1);
        assert_eq!(fila[0].rotulo_do_alvo, "Internet fibra");
    }

    #[test]
    fn aprovar_barra_o_que_falta_e_passa_o_completo() {
        let c = cena(base());
        let mut cx = caixa(&c);
        let e = cx.aprovar(&id(21).to_string()).unwrap_err();
        let ErroDaCaixa::Invalido(m) = e else {
            panic!("esperava Invalido")
        };
        assert!(m.starts_with("Falta para lançar: Conta ou cartão"), "{m}");
        assert!(cx.escritas().unwrap().is_empty());

        let ok = cx.aprovar(&id(20).to_string()).unwrap();
        assert_eq!(ok.o_que, "Aprovar rascunho");
        assert_eq!(ok.estado, EstadoDeEnvio::Pendente);
    }

    #[test]
    fn aprovar_lote_e_tudo_ou_nada() {
        let c = cena(base());
        let mut cx = caixa(&c);
        // O rascunho 21 não passa: o lote inteiro fica como está.
        let e = cx.aprovar_lote(&id(10).to_string()).unwrap_err();
        let ErroDaCaixa::Invalido(m) = e else {
            panic!("esperava Invalido")
        };
        assert!(m.contains("Nada foi aprovado"), "{m}");
        assert!(cx.escritas().unwrap().is_empty());
    }

    #[test]
    fn aprovar_lote_completo_enfileira_uma_escrita() {
        let c = cena(vec![
            LinhaAgora::LotesDeRascunhos(lote(10, "Gmail")),
            LinhaAgora::Rascunhos(Box::new(rascunho(20, id(10), completo("A")))),
            LinhaAgora::Rascunhos(Box::new(rascunho(22, id(10), completo("B")))),
        ]);
        let mut cx = caixa(&c);
        let e = cx.aprovar_lote(&id(10).to_string()).unwrap();
        assert_eq!(e.o_que, "Aprovar lote");
        assert_eq!(e.alvo, AlvoDaEscrita::Lote(id(10).to_string()));
    }

    #[test]
    fn recusar_pede_motivo_e_grava_a_escrita() {
        let c = cena(base());
        let mut cx = caixa(&c);
        assert!(matches!(
            cx.recusar(&id(20).to_string(), "  "),
            Err(ErroDaCaixa::Invalido(_))
        ));
        let e = cx.recusar(&id(20).to_string(), "duplicado").unwrap();
        assert_eq!(e.o_que, "Recusar rascunho");
        assert!(matches!(
            cx.recusar("nao-e-uuid", "x"),
            Err(ErroDaCaixa::NaoEncontrado(_))
        ));
    }

    #[test]
    fn movimentacao_editar_ratear_e_excluir() {
        let c = cena_com(
            base(),
            vec![
                (
                    Colecao::Contas,
                    id(900),
                    serde_json::to_value(conta(900, "Conta corrente")).unwrap(),
                ),
                (
                    Colecao::Contas,
                    id(901),
                    serde_json::to_value(conta(901, "Cartão Azul")).unwrap(),
                ),
                (
                    Colecao::Centros,
                    id(51),
                    serde_json::to_value(centro(51, "Mercado")).unwrap(),
                ),
            ],
        );
        let mut cx = caixa(&c);
        let alvo = Ref::nova(Tipo::Movimentacao, id(2).to_string());
        let m = cx.movimentacao(&alvo).unwrap().unwrap();
        assert_eq!(m.campos.valor_centavos, -2500, "saída com sinal");
        assert_eq!(m.campos.descricao, "Padaria");
        assert_eq!(m.campos.centro, Some(id(51).to_string()), "um rateio só");
        assert_eq!(cx.contas().len(), 2);
        assert_eq!(cx.centros().len(), 1);
        assert!(
            cx.movimentacao(&Ref::nova(Tipo::Conta, "x"))
                .unwrap()
                .is_none()
        );

        let mut campos = m.campos.clone();
        campos.descricao = "Padaria nova".into();
        campos.valor_centavos = -3000;
        let e = cx.editar_movimentacao(&alvo, campos).unwrap();
        assert_eq!(e.o_que, "Editar movimentação");
        assert_eq!(e.alvo, AlvoDaEscrita::Movimentacao(alvo.clone()));
        let depois = cx.movimentacao(&alvo).unwrap().unwrap();
        assert_eq!(depois.campos.descricao, "Padaria nova");
        assert_eq!(depois.campos.valor_centavos, -3000);

        // Rateio que não soma o valor é recusado antes de entrar na fila.
        let e = cx
            .ratear(
                &alvo,
                vec![RateioDaMovimentacao {
                    centro: id(51).to_string(),
                    centavos: 1,
                }],
            )
            .unwrap_err();
        assert!(matches!(e, ErroDaCaixa::Invalido(_)), "{e:?}");
        assert_eq!(cx.escritas().unwrap().len(), 1);

        let e = cx.excluir(&alvo).unwrap();
        assert_eq!(e.o_que, "Excluir movimentação");
        assert_eq!(cx.escritas().unwrap().len(), 2);
        // A mais nova primeiro.
        assert_eq!(cx.escritas().unwrap()[0].o_que, "Excluir movimentação");
    }

    #[test]
    fn lado_a_lado_da_edicao_recusada() {
        let c = cena(base());
        let cx = caixa(&c);
        let proposta = PropostaDeAlteracao {
            id: id(500),
            acao: Acao::EditarMovimentacao {
                movimentacao: id(2),
                edicao: EdicaoDaMovimentacao {
                    descricao: Some("Padaria nova".into()),
                    data: "2026-11-04".into(),
                    valor: 3000,
                    conta: id(900),
                    fornecedor: None,
                    centro: None,
                    notas: None,
                },
            },
            item: sbf_contrato::ItemAlterado {
                colecao: Colecao::Movimentacoes,
                id: id(2),
            },
            versoes_de_base: vec![],
            feita_em: "2026-11-14T12:00:00Z".into(),
        };
        let e = EscritaDaEstacao {
            id: id(500),
            acao: proposta.acao.clone(),
            rotulo: "Editar · Padaria".into(),
            feita_em: proposta.feita_em.clone(),
            estado: EstadoDaEscrita::Recusada,
            recusa: Some(RE {
                motivo: sbf_contrato::CodigoDeErro::VersaoDeBaseDivergente,
                mensagem: "Mudou no servidor".into(),
                quando: "2026-11-14T12:05:00Z".into(),
                no_servidor_agora: vec![LinhaAgora::Movimentacoes(Box::new(movimentacao(
                    2,
                    "Padaria do Zé",
                    2500,
                )))],
            }),
            refeita_em: None,
            proposta,
        };
        let v = cx.escrita_da(&e);
        assert_eq!(v.estado, EstadoDeEnvio::Recusada);
        assert_eq!(v.rotulo_do_alvo, "Padaria");
        let r = v.recusa.unwrap();
        assert_eq!(r.motivo, "Mudou no servidor");
        let desc = r.campos.iter().find(|c| c.rotulo == "Descrição").unwrap();
        assert_eq!(desc.sua, "Padaria nova");
        assert_eq!(desc.servidor, "Padaria do Zé");
        assert!(desc.difere());
        let valor = r.campos.iter().find(|c| c.rotulo == "Valor").unwrap();
        assert_eq!(valor.sua, dinheiro(3000));
        assert_eq!(valor.servidor, dinheiro(2500));
    }

    // ───────────── o envio, com um dublê da porta ─────────────

    struct PortaFalsa {
        situacao: Mutex<SituacaoDoLote>,
        interromper: bool,
    }

    impl PortaDoEnvio for PortaFalsa {
        fn situacao(&self, _: &Ref) -> Result<SituacaoDoLote, String> {
            Ok(self.situacao.lock().unwrap().clone())
        }
        fn preparar(&self, _: &Ref) -> Result<TrabalhoDeEnvio, String> {
            let total = self.situacao.lock().unwrap().documentos;
            let interromper = self.interromper;
            Ok(Box::new(move |progresso| {
                let feitos_ate = if interromper { total - 1 } else { total };
                for feitos in 0..=feitos_ate {
                    progresso(&AndamentoDoEnvio {
                        total,
                        feitos,
                        atual: None,
                        falhas: 0,
                    });
                }
                Ok(ResumoDoEnvio {
                    total,
                    enviados: feitos_ate,
                    lote_promovido: !interromper,
                    interrompido: interromper,
                    ..ResumoDoEnvio::default()
                })
            }))
        }
    }

    fn caixa_com_porta(
        c: &Cena,
        aprovado: bool,
        documentos: usize,
        interromper: bool,
    ) -> CaixaDaEstacao {
        CaixaDaEstacao::nova(
            c.estacao.clone(),
            c.escritas.clone(),
            Some(Arc::new(PortaFalsa {
                situacao: Mutex::new(SituacaoDoLote {
                    aprovado,
                    promovido: false,
                    documentos,
                }),
                interromper,
            })),
        )
    }

    fn esperar_fim(cx: &mut CaixaDaEstacao, lote: &Ref) -> EnvioDoLote {
        for _ in 0..400 {
            cx.tique();
            let e = cx.envio_do_lote(lote);
            if !matches!(e, EnvioDoLote::Enviando(_)) {
                return e;
            }
            thread::sleep(Duration::from_millis(10));
        }
        panic!("o envio não terminou");
    }

    #[test]
    fn lote_nao_aprovado_e_indisponivel_e_nao_envia() {
        let c = cena(base());
        let mut cx = caixa_com_porta(&c, false, 3, false);
        let lote = Ref::nova(Tipo::Lote, "1");
        match cx.envio_do_lote(&lote) {
            EnvioDoLote::Indisponivel(m) => assert!(m.contains("não foi aprovado"), "{m}"),
            outro => panic!("{outro:?}"),
        }
        assert!(matches!(
            cx.enviar_lote(&lote),
            Err(ErroDaCaixa::Invalido(_))
        ));
    }

    #[test]
    fn sem_porta_o_envio_diz_que_nao_esta_ligado() {
        let c = cena(base());
        let cx = caixa(&c);
        assert!(matches!(
            cx.envio_do_lote(&Ref::nova(Tipo::Lote, "1")),
            EnvioDoLote::Indisponivel(_)
        ));
    }

    #[test]
    fn envio_anda_por_tique_e_termina_enviado() {
        let c = cena(base());
        let mut cx = caixa_com_porta(&c, true, 4, false);
        let lote = Ref::nova(Tipo::Lote, "1");
        assert_eq!(
            cx.envio_do_lote(&lote),
            EnvioDoLote::Pronto { documentos: 4 }
        );
        cx.enviar_lote(&lote).unwrap();
        // Logo depois de pedir, já aparece como em andamento; segundo pedido é recusado.
        assert!(matches!(cx.envio_do_lote(&lote), EnvioDoLote::Enviando(_)));
        let fim = esperar_fim(&mut cx, &lote);
        assert_eq!(fim, EnvioDoLote::Enviado { documentos: 4 });
        assert!(matches!(
            cx.enviar_lote(&lote),
            Err(ErroDaCaixa::Invalido(_))
        ));
    }

    #[test]
    fn rede_que_cai_deixa_falhou_e_reenviar_retoma() {
        let c = cena(base());
        let mut cx = caixa_com_porta(&c, true, 3, true);
        let lote = Ref::nova(Tipo::Lote, "1");
        cx.enviar_lote(&lote).unwrap();
        match esperar_fim(&mut cx, &lote) {
            EnvioDoLote::Falhou { progresso, motivo } => {
                assert_eq!(progresso.total, 3);
                assert_eq!(progresso.enviados, 2);
                assert!(motivo.contains("rede"), "{motivo}");
            }
            outro => panic!("{outro:?}"),
        }
        // Reenviar é permitido (retoma de onde parou).
        cx.enviar_lote(&lote).unwrap();
        assert!(matches!(
            esperar_fim(&mut cx, &lote),
            EnvioDoLote::Falhou { .. }
        ));
    }
}
