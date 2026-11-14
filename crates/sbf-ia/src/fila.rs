//! A fila da triagem (spec 09, "Como gasta pouco"): lotes de até 25 itens, cache por
//! conteúdo, retentativa única, pausa e espera no limite, reserva só se habilitada e abaixo do
//! teto, escalonamento do degrau rápido para o forte e progresso por canal.
//!
//! A fila não conhece o domínio: o chamador diz como montar a entrada de um lote e como ler a
//! confiança de cada item da resposta ([`Formato`]). O texto enviado nunca vai a log nem a
//! disco; o que fica gravado de cada chamada é a [`Saida`] (tamanho, tokens, custo).

use crate::esquema::validar_esquema;
use crate::tempo::agora_rfc3339;
use crate::{
    Consumo, Degrau, ErroDeIa, NivelDeConfianca, Pedido, Provedor, Registro, Saida, chave_do_cache,
};
use serde_json::Value;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, mpsc};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// O máximo de itens por chamada (spec 09).
pub const MAXIMO_POR_LOTE: usize = 25;

/// Como o chamador fala com a fila: monta a entrada de um lote e lê a confiança de cada item.
pub trait Formato<I>: Send + Sync {
    /// A entrada do lote, com os itens na ordem recebida.
    fn montar_entrada(&self, itens: &[&I]) -> String;
    /// Para cada um dos `quantos` itens do lote (na ordem de `montar_entrada`), a confiança
    /// (0 a 1) que a resposta traz, ou `None` se o item faltou ou veio incompleto.
    fn ler_confiancas(&self, resposta: &Value, quantos: usize) -> Vec<Option<f64>>;
}

/// Pausa e cancelamento, de outra thread.
#[derive(Debug, Clone, Default)]
pub struct Controle {
    cancelar: Arc<AtomicBool>,
    pausar: Arc<AtomicBool>,
}

impl Controle {
    pub fn novo() -> Self {
        Self::default()
    }
    pub fn cancelar(&self) {
        self.cancelar.store(true, Ordering::SeqCst);
    }
    pub fn pausar(&self) {
        self.pausar.store(true, Ordering::SeqCst);
    }
    pub fn retomar(&self) {
        self.pausar.store(false, Ordering::SeqCst);
    }
    pub fn cancelado(&self) -> bool {
        self.cancelar.load(Ordering::SeqCst)
    }
    pub fn pausado(&self) -> bool {
        self.pausar.load(Ordering::SeqCst)
    }
}

/// O que a fila conta à janela. Sem texto de item.
#[derive(Debug, Clone, PartialEq)]
pub enum Evento {
    Iniciada {
        itens: usize,
        lotes: usize,
    },
    /// Um lote saiu do cache, sem chamada.
    LoteEmCache,
    /// Um lote terminou (com resposta ou como não classificado).
    LoteConcluido {
        itens_feitos: usize,
        itens_total: usize,
        chamadas: u64,
        acertos_de_cache: u64,
        consumo: Consumo,
    },
    /// Itens abaixo da confiança sobem para o degrau forte.
    Escalonando {
        itens: usize,
    },
    /// Resposta fora do esquema: o lote tenta de novo, uma vez.
    Repetindo {
        itens: usize,
    },
    /// Limite do provedor: a fila espera `espera_segundos` e continua.
    EsperandoLimite {
        espera_segundos: u64,
    },
    TrocouParaReserva,
    Pausada,
    Retomada,
    Terminada {
        interrupcao: Option<Interrupcao>,
    },
}

/// Por que uma rodada parou antes do fim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Interrupcao {
    Cancelada,
    /// Teto de gasto do provedor pago.
    Teto,
    Indisponivel(String),
    /// O limite não passou depois de esperar o máximo combinado.
    LimiteSemFim,
    /// Não foi possível gravar o registro do que saiu (a fila para, para não enviar às cegas).
    FalhaDoRegistro(String),
}

/// A resposta de um lote que valeu.
#[derive(Debug, Clone, PartialEq)]
pub struct LoteResolvido {
    /// Os índices (na lista original) dos itens do lote, na ordem da entrada.
    pub indices: Vec<usize>,
    pub json: Value,
    pub degrau: Degrau,
    pub provedor: String,
    pub do_cache: bool,
}

/// Onde está a resposta de um item: `lotes[lote]`, posição `posicao` dentro do lote.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Atribuicao {
    pub lote: usize,
    pub posicao: usize,
}

/// O resultado de uma rodada.
#[derive(Debug, Clone, PartialEq)]
pub struct Relatorio {
    pub lotes: Vec<LoteResolvido>,
    /// Por item da lista original: de onde ler a resposta (`None`: sem resposta).
    pub atribuicoes: Vec<Option<Atribuicao>>,
    /// A confiança final de cada item com resposta.
    pub confiancas: Vec<Option<f64>>,
    /// Resposta inválida duas vezes, ou falha do provedor: viram "não classificado".
    pub nao_classificados: Vec<usize>,
    /// Nunca chegaram a ser tentados (a rodada parou antes).
    pub nao_processados: Vec<usize>,
    pub chamadas: u64,
    pub acertos_de_cache: u64,
    /// Itens que subiram para o degrau forte.
    pub escalonados: usize,
    pub consumo: Consumo,
    pub interrupcao: Option<Interrupcao>,
}

impl Relatorio {
    /// A resposta de um lote para o item `indice`, com a posição dele nela.
    pub fn resposta_do_item(&self, indice: usize) -> Option<(&LoteResolvido, usize)> {
        let a = (*self.atribuicoes.get(indice)?)?;
        Some((self.lotes.get(a.lote)?, a.posicao))
    }
}

/// Configuração de uma fila.
#[derive(Debug, Clone)]
pub struct ConfiguracaoDaFila {
    /// Vai para `saida_ia.destinatario` (por exemplo `triagem`).
    pub destinatario: String,
    pub nivel: NivelDeConfianca,
    pub prompt_versao: String,
    pub instrucao: String,
    pub esquema: Value,
    pub max_tokens_saida: u32,
    /// Itens por chamada, de 1 a 25.
    pub tamanho_do_lote: usize,
    /// Itens com confiança abaixo disto sobem para o degrau forte.
    pub limiar_de_escalonamento: f64,
    pub escalonar: bool,
    /// A reserva só entra se isto for `true` **e** ela estiver abaixo do teto.
    pub usar_reserva: bool,
    /// Quantas esperas de limite seguidas num lote antes de desistir da rodada.
    pub esperas_maximas_por_lote: u32,
}

impl ConfiguracaoDaFila {
    pub fn nova(
        destinatario: impl Into<String>,
        prompt_versao: impl Into<String>,
        instrucao: impl Into<String>,
        esquema: Value,
    ) -> Self {
        Self {
            destinatario: destinatario.into(),
            nivel: NivelDeConfianca::Completo,
            prompt_versao: prompt_versao.into(),
            instrucao: instrucao.into(),
            esquema,
            max_tokens_saida: 8000,
            tamanho_do_lote: MAXIMO_POR_LOTE,
            limiar_de_escalonamento: 0.7,
            escalonar: true,
            usar_reserva: false,
            esperas_maximas_por_lote: 50,
        }
    }
}

type Espera = dyn Fn(Duration, &Controle) -> bool + Send + Sync;
type Relogio = dyn Fn() -> String + Send + Sync;

/// A fila. Uma instância serve a várias rodadas; cada [`Fila::executar`] é independente.
pub struct Fila {
    principal: Arc<dyn Provedor>,
    reserva: Option<Arc<dyn Provedor>>,
    registro: Arc<dyn Registro>,
    config: ConfiguracaoDaFila,
    esperar: Arc<Espera>,
    relogio: Arc<Relogio>,
}

impl std::fmt::Debug for Fila {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Fila")
            .field("principal", &self.principal.nome())
            .field("reserva", &self.reserva.as_ref().map(|r| r.nome()))
            .finish_non_exhaustive()
    }
}

/// Espera `duracao` em fatias, acordando se cancelarem. `false` se cancelada.
fn esperar_de_verdade(duracao: Duration, controle: &Controle) -> bool {
    let fim = Instant::now() + duracao;
    while Instant::now() < fim {
        if controle.cancelado() {
            return false;
        }
        std::thread::sleep(Duration::from_millis(50).min(fim - Instant::now()));
    }
    !controle.cancelado()
}

impl Fila {
    pub fn nova(
        principal: Arc<dyn Provedor>,
        reserva: Option<Arc<dyn Provedor>>,
        registro: Arc<dyn Registro>,
        config: ConfiguracaoDaFila,
    ) -> Self {
        Self {
            principal,
            reserva,
            registro,
            config,
            esperar: Arc::new(esperar_de_verdade),
            relogio: Arc::new(agora_rfc3339),
        }
    }

    /// Troca a espera do limite (os testes não dormem de verdade). Devolve `false` se
    /// foi cancelada.
    #[must_use]
    pub fn com_espera(
        mut self,
        espera: impl Fn(Duration, &Controle) -> bool + Send + Sync + 'static,
    ) -> Self {
        self.esperar = Arc::new(espera);
        self
    }

    /// Troca a fonte do `quando` (RFC 3339 UTC) das saídas.
    #[must_use]
    pub fn com_relogio(mut self, relogio: impl Fn() -> String + Send + Sync + 'static) -> Self {
        self.relogio = Arc::new(relogio);
        self
    }

    /// Roda a rodada na thread de quem chama.
    pub fn executar<I, F: Formato<I>>(
        &self,
        itens: &[I],
        formato: &F,
        controle: &Controle,
        eventos: Option<&Sender<Evento>>,
    ) -> Relatorio {
        self.principal.nova_rodada();
        if let Some(r) = &self.reserva {
            r.nova_rodada();
        }
        let mut e = Execucao::nova(self, itens, formato, controle, eventos);
        e.rodar();
        e.fechar()
    }

    /// Roda a rodada numa thread própria; a janela lê o canal a cada quadro.
    pub fn iniciar<I, F>(
        self: &Arc<Self>,
        itens: Vec<I>,
        formato: Arc<F>,
        controle: Controle,
    ) -> (mpsc::Receiver<Evento>, JoinHandle<Relatorio>)
    where
        I: Send + 'static,
        F: Formato<I> + 'static,
    {
        let (tx, rx) = mpsc::channel();
        let fila = Arc::clone(self);
        let fio =
            std::thread::spawn(move || fila.executar(&itens, &*formato, &controle, Some(&tx)));
        (rx, fio)
    }
}

/// O resultado de falar com o provedor sobre um lote.
enum DeUmLote {
    Valeu {
        json: Value,
        degrau: Degrau,
        provedor: String,
        do_cache: bool,
    },
    /// Resposta inválida duas vezes ou falha do provedor.
    Falhou,
}

struct Execucao<'a, I, F: Formato<I>> {
    fila: &'a Fila,
    itens: &'a [I],
    formato: &'a F,
    controle: &'a Controle,
    eventos: Option<&'a Sender<Evento>>,
    lotes: Vec<LoteResolvido>,
    atribuicoes: Vec<Option<Atribuicao>>,
    confiancas: Vec<Option<f64>>,
    falhos: Vec<bool>,
    chamadas: u64,
    acertos_de_cache: u64,
    escalonados: usize,
    consumo: Consumo,
    feitos: usize,
    principal_limitado_ate: Option<Instant>,
    /// A reserva respondeu `Teto` nesta rodada: não se volta a ela.
    reserva_esgotada: bool,
    interrupcao: Option<Interrupcao>,
}

impl<'a, I, F: Formato<I>> Execucao<'a, I, F> {
    fn nova(
        fila: &'a Fila,
        itens: &'a [I],
        formato: &'a F,
        controle: &'a Controle,
        eventos: Option<&'a Sender<Evento>>,
    ) -> Self {
        let n = itens.len();
        Self {
            fila,
            itens,
            formato,
            controle,
            eventos,
            lotes: Vec::new(),
            atribuicoes: vec![None; n],
            confiancas: vec![None; n],
            falhos: vec![false; n],
            chamadas: 0,
            acertos_de_cache: 0,
            escalonados: 0,
            consumo: Consumo::default(),
            feitos: 0,
            principal_limitado_ate: None,
            reserva_esgotada: false,
            interrupcao: None,
        }
    }

    fn emitir(&self, evento: Evento) {
        if let Some(canal) = self.eventos {
            let _ = canal.send(evento);
        }
    }

    fn tamanho(&self) -> usize {
        self.fila.config.tamanho_do_lote.clamp(1, MAXIMO_POR_LOTE)
    }

    fn rodar(&mut self) {
        let n = self.itens.len();
        let todos: Vec<usize> = (0..n).collect();
        self.emitir(Evento::Iniciada {
            itens: n,
            lotes: n.div_ceil(self.tamanho()),
        });
        if let Err(i) = self.passar(&todos, Degrau::Rapido, false) {
            self.interrupcao = Some(i);
            return;
        }
        if self.fila.config.escalonar {
            let limiar = self.fila.config.limiar_de_escalonamento;
            let baixos: Vec<usize> = (0..n)
                .filter(|&i| self.confiancas[i].is_some_and(|c| c < limiar))
                .collect();
            if !baixos.is_empty() {
                self.emitir(Evento::Escalonando {
                    itens: baixos.len(),
                });
                self.escalonados = baixos.len();
                if let Err(i) = self.passar(&baixos, Degrau::Forte, true) {
                    self.interrupcao = Some(i);
                }
            }
        }
    }

    fn fechar(self) -> Relatorio {
        let n = self.itens.len();
        let nao_classificados = (0..n)
            .filter(|&i| self.atribuicoes[i].is_none() && self.falhos[i])
            .collect();
        let nao_processados = (0..n)
            .filter(|&i| self.atribuicoes[i].is_none() && !self.falhos[i])
            .collect();
        self.emitir(Evento::Terminada {
            interrupcao: self.interrupcao.clone(),
        });
        Relatorio {
            lotes: self.lotes,
            atribuicoes: self.atribuicoes,
            confiancas: self.confiancas,
            nao_classificados,
            nao_processados,
            chamadas: self.chamadas,
            acertos_de_cache: self.acertos_de_cache,
            escalonados: self.escalonados,
            consumo: self.consumo,
            interrupcao: self.interrupcao,
        }
    }

    /// Uma passada sobre `indices`, em lotes, no `degrau`.
    fn passar(
        &mut self,
        indices: &[usize],
        degrau: Degrau,
        escalonamento: bool,
    ) -> Result<(), Interrupcao> {
        for lote in indices.chunks(self.tamanho()) {
            self.lote(lote, degrau, escalonamento, 0)?;
            if !escalonamento {
                self.feitos += lote.len();
            }
            self.emitir(Evento::LoteConcluido {
                itens_feitos: self.feitos.min(self.itens.len()),
                itens_total: self.itens.len(),
                chamadas: self.chamadas,
                acertos_de_cache: self.acertos_de_cache,
                consumo: self.consumo,
            });
        }
        Ok(())
    }

    /// Um lote. Item que a resposta deixou sem confiança volta uma vez (`tentativa` 0 → 1).
    fn lote(
        &mut self,
        indices: &[usize],
        degrau: Degrau,
        escalonamento: bool,
        tentativa: u8,
    ) -> Result<(), Interrupcao> {
        match self.falar(indices, degrau)? {
            DeUmLote::Falhou => {
                if !escalonamento {
                    for &i in indices {
                        self.falhos[i] = true;
                    }
                }
            }
            DeUmLote::Valeu {
                json,
                degrau,
                provedor,
                do_cache,
            } => {
                let confs = self.formato.ler_confiancas(&json, indices.len());
                let id = self.lotes.len();
                let mut incompletos = Vec::new();
                for (posicao, &i) in indices.iter().enumerate() {
                    match confs.get(posicao).copied().flatten() {
                        Some(c) => {
                            self.atribuicoes[i] = Some(Atribuicao { lote: id, posicao });
                            self.confiancas[i] = Some(c);
                        }
                        None => incompletos.push(i),
                    }
                }
                self.lotes.push(LoteResolvido {
                    indices: indices.to_vec(),
                    json,
                    degrau,
                    provedor,
                    do_cache,
                });
                if !incompletos.is_empty() {
                    if tentativa == 0 {
                        self.emitir(Evento::Repetindo {
                            itens: incompletos.len(),
                        });
                        self.lote(&incompletos, degrau, escalonamento, 1)?;
                    } else if !escalonamento {
                        for i in incompletos {
                            if self.atribuicoes[i].is_none() {
                                self.falhos[i] = true;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn reserva_utilizavel(&self) -> Option<&'a Arc<dyn Provedor>> {
        let fila: &'a Fila = self.fila;
        fila.reserva
            .as_ref()
            .filter(|r| fila.config.usar_reserva && !self.reserva_esgotada && r.dentro_do_teto())
    }

    /// Cuida da pausa e do cancelamento antes de cada tentativa.
    fn ponto_de_controle(&self) -> Result<(), Interrupcao> {
        if self.controle.pausado() && !self.controle.cancelado() {
            self.emitir(Evento::Pausada);
            while self.controle.pausado() && !self.controle.cancelado() {
                std::thread::sleep(Duration::from_millis(10));
            }
            if !self.controle.cancelado() {
                self.emitir(Evento::Retomada);
            }
        }
        if self.controle.cancelado() {
            return Err(Interrupcao::Cancelada);
        }
        Ok(())
    }

    fn registrar(
        &mut self,
        provedor: &str,
        modelo: String,
        bytes: u64,
        consumo: Consumo,
    ) -> Result<(), Interrupcao> {
        let c = &self.fila.config;
        let saida = Saida {
            destinatario: c.destinatario.clone(),
            provedor: provedor.to_owned(),
            modelo,
            nivel: c.nivel,
            bytes_enviados: bytes,
            consumo,
            quando: (self.fila.relogio)(),
        };
        self.fila
            .registro
            .registrar_saida(&saida)
            .map_err(|e| Interrupcao::FalhaDoRegistro(e.to_string()))
    }

    fn somar(&mut self, c: Consumo) {
        self.consumo.tokens_entrada += c.tokens_entrada;
        self.consumo.tokens_saida += c.tokens_saida;
        self.consumo.custo_dezmilesimos_usd += c.custo_dezmilesimos_usd;
        self.consumo.nominal |= c.nominal;
    }

    /// Cache, depois o provedor, com retentativa, espera e reserva.
    fn falar(&mut self, indices: &[usize], degrau: Degrau) -> Result<DeUmLote, Interrupcao> {
        let c = &self.fila.config;
        let refs: Vec<&I> = indices.iter().map(|&i| &self.itens[i]).collect();
        let pedido = Pedido {
            prompt_versao: c.prompt_versao.clone(),
            instrucao: c.instrucao.clone(),
            entrada: self.formato.montar_entrada(&refs),
            esquema: c.esquema.clone(),
            degrau,
            max_tokens_saida: c.max_tokens_saida,
        };
        let bytes = (pedido.instrucao.len()
            + pedido.entrada.len()
            + pedido.esquema.to_string().len()) as u64;

        // Cache: o do principal e, se a reserva está ligada, o dela.
        let mut nomes = vec![self.fila.principal.nome()];
        if let (true, Some(r)) = (c.usar_reserva, &self.fila.reserva) {
            nomes.push(r.nome());
        }
        for nome in nomes {
            let chave = chave_do_cache(&pedido, nome);
            if let Ok(Some(json)) = self.fila.registro.ler_cache(&chave)
                && validar_esquema(&pedido.esquema, &json).is_ok()
            {
                self.acertos_de_cache += 1;
                self.emitir(Evento::LoteEmCache);
                return Ok(DeUmLote::Valeu {
                    json,
                    degrau,
                    provedor: nome.to_owned(),
                    do_cache: true,
                });
            }
        }

        let mut invalidas = 0_u8;
        let mut esperas = 0_u32;
        loop {
            self.ponto_de_controle()?;
            let reserva = self.reserva_utilizavel();
            let usar_reserva = reserva.is_some()
                && self
                    .principal_limitado_ate
                    .is_some_and(|ate| Instant::now() < ate);
            let provedor: Arc<dyn Provedor> = match (usar_reserva, reserva) {
                (true, Some(r)) => Arc::clone(r),
                _ => Arc::clone(&self.fila.principal),
            };
            let nome = provedor.nome();
            self.chamadas += 1;
            match provedor.pedir(&pedido) {
                Ok(resposta) => {
                    self.registrar(nome, resposta.modelo.clone(), bytes, resposta.consumo)?;
                    self.somar(resposta.consumo);
                    let chave = chave_do_cache(&pedido, nome);
                    let _ = self.fila.registro.gravar_cache(&chave, &resposta.json);
                    return Ok(DeUmLote::Valeu {
                        json: resposta.json,
                        degrau,
                        provedor: nome.to_owned(),
                        do_cache: false,
                    });
                }
                Err(ErroDeIa::RespostaInvalida(_)) => {
                    self.registrar(nome, rotulo(degrau), bytes, Consumo::default())?;
                    invalidas += 1;
                    if invalidas >= 2 {
                        return Ok(DeUmLote::Falhou);
                    }
                    self.emitir(Evento::Repetindo {
                        itens: indices.len(),
                    });
                }
                Err(ErroDeIa::Limite { espera_segundos }) => {
                    // Nada foi processado: esta chamada não conta.
                    self.chamadas -= 1;
                    let espera = Duration::from_secs(espera_segundos);
                    if !usar_reserva && self.reserva_utilizavel().is_some() {
                        self.principal_limitado_ate = Some(Instant::now() + espera);
                        self.emitir(Evento::TrocouParaReserva);
                        continue;
                    }
                    esperas += 1;
                    if esperas > self.fila.config.esperas_maximas_por_lote {
                        return Err(Interrupcao::LimiteSemFim);
                    }
                    self.emitir(Evento::EsperandoLimite { espera_segundos });
                    if !(self.fila.esperar)(espera, self.controle) {
                        return Err(Interrupcao::Cancelada);
                    }
                }
                Err(ErroDeIa::Teto) => {
                    self.chamadas -= 1;
                    if usar_reserva {
                        // A reserva estourou o teto: volta a esperar o principal.
                        self.principal_limitado_ate = None;
                        self.reserva_esgotada = true;
                        continue;
                    }
                    return Err(Interrupcao::Teto);
                }
                Err(ErroDeIa::Indisponivel(m)) => {
                    self.chamadas -= 1;
                    return Err(Interrupcao::Indisponivel(m));
                }
                Err(ErroDeIa::Outro(_)) => {
                    self.registrar(nome, rotulo(degrau), bytes, Consumo::default())?;
                    return Ok(DeUmLote::Falhou);
                }
            }
        }
    }
}

fn rotulo(degrau: Degrau) -> String {
    match degrau {
        Degrau::Rapido => "rapido".to_owned(),
        Degrau::Forte => "forte".to_owned(),
    }
}
