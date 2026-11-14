//! [`EstacaoLocal`]: a estação como a janela a usa. Implementa os traits
//! [`Sincronizacao`] e [`Alteracoes`] do `sbf-inventario`; nada aqui bloqueia o quadro.
//!
//! - **Sincronizar** ao abrir ([`EstacaoLocal::abrir`]), por Ação
//!   ([`Sincronizacao::sincronizar_agora`]) e por intervalo ([`EstacaoLocal::tique`], a cada
//!   quadro). A troca roda numa thread própria, com a sua conexão de escrita; a janela lê
//!   pela dela.
//! - **Pareamento** pelo [`Pareador`] (login Google por loopback e `POST
//!   /estacoes/pareamento`), que roda fora do quadro.
//! - **Revogada** (Q11): a troca apaga a credencial e o cache; no quadro seguinte, a janela
//!   larga a conexão e apaga de novo (Windows).

use crate::alteracoes;
use crate::cache::{CacheDoLivro, apagar_arquivos};
use crate::credencial::{CredencialDaEstacao, GuardaDeCredencial};
use crate::escrita::{ErroDeEscrita, Escrita, Escritas, LoteNaCaixa, ResultadoDeEscrita};
use crate::pareamento::Pareador;
use crate::servidor::{ClienteDoServidor, Servidor};
use crate::sincronizacao::{Desfecho, Sincronizador, estacao_da_janela};
use crate::texto::instante_iso;
use bancada_rede::{
    Catalogo, ConfiguracaoDaRede, ErroDeRede, Instante, ProvedorOAuth, Rede, Relogio,
    RelogioDoSistema,
};
use sbf_contrato::{Acao, Colecao, Id, VERSAO_DO_CONTRATO};
use sbf_inventario::alteracao::{
    Alteracao, Alteracoes, ErroDeAlteracao, Estacao, EstadoDaRede, EstadoDaSincronizacao,
    OpcaoDeCategoria, PedidoDeAlteracao, ResultadoDeAlteracao, Sincronizacao, Troca,
};
use sbf_inventario::referencia::{Ref, Tipo};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

/// O intervalo padrão entre sincronizações com a janela aberta (Opções pode trocar).
pub const INTERVALO_PADRAO: Duration = Duration::from_secs(15 * 60);
/// Quantas trocas o histórico mostra.
const TROCAS_NO_HISTORICO: usize = 50;

/// O que a estação precisa para funcionar.
#[derive(Debug, Clone)]
pub struct ConfiguracaoDaEstacao {
    servidor: Servidor,
    pasta_de_dados: PathBuf,
    guarda: Arc<dyn GuardaDeCredencial>,
    relogio: Arc<dyn Relogio>,
    versao_das_regras: String,
    intervalo: Option<Duration>,
    rotulo_do_servidor: String,
    google: Option<ProvedorOAuth>,
    rotas: Vec<(String, String)>,
    sincronizar_ao_abrir: bool,
}

impl ConfiguracaoDaEstacao {
    /// A estação de `servidor`, com o cache em `<pasta_de_dados>/livro-caixa/<host>/` e a
    /// credencial em `guarda` (o chaveiro do app).
    pub fn nova(
        servidor: Servidor,
        pasta_de_dados: impl AsRef<Path>,
        guarda: Arc<dyn GuardaDeCredencial>,
    ) -> Self {
        Self {
            servidor,
            pasta_de_dados: pasta_de_dados.as_ref().to_path_buf(),
            guarda,
            relogio: Arc::new(RelogioDoSistema),
            versao_das_regras: env!("CARGO_PKG_VERSION").to_owned(),
            intervalo: Some(INTERVALO_PADRAO),
            rotulo_do_servidor: "servidor na VPS de São Paulo".to_owned(),
            google: None,
            rotas: Vec::new(),
            sincronizar_ao_abrir: true,
        }
    }

    pub fn relogio(mut self, relogio: Arc<dyn Relogio>) -> Self {
        self.relogio = relogio;
        self
    }

    /// A versão das regras desta estação (`sbf --version`).
    pub fn versao_das_regras(mut self, versao: impl Into<String>) -> Self {
        self.versao_das_regras = versao.into();
        self
    }

    /// Intervalo entre sincronizações com a janela aberta (`None`: só ao abrir e por Ação).
    pub fn intervalo(mut self, intervalo: Option<Duration>) -> Self {
        self.intervalo = intervalo;
        self
    }

    /// O texto do cabeçalho da área Sincronização.
    pub fn rotulo_do_servidor(mut self, rotulo: impl Into<String>) -> Self {
        self.rotulo_do_servidor = rotulo.into();
        self
    }

    /// O cliente OAuth do Google para o pareamento.
    pub fn google(mut self, provedor: ProvedorOAuth) -> Self {
        self.google = Some(provedor);
        self
    }

    /// Manda o host oficial para outro endereço (servidor falso ou local de teste; só
    /// loopback ou HTTPS, como a `bancada-rede` exige).
    pub fn rota(mut self, autoridade: &str, base: &str) -> Self {
        self.rotas.push((autoridade.to_owned(), base.to_owned()));
        self
    }

    /// Sincronizar ao abrir (padrão: sim).
    pub fn sincronizar_ao_abrir(mut self, sim: bool) -> Self {
        self.sincronizar_ao_abrir = sim;
        self
    }

    /// `<pasta de dados>/livro-caixa/<host>/livro-caixa.sqlite` (o `:` da porta vira `_`).
    pub fn caminho_do_cache(&self) -> PathBuf {
        crate::pasta::caminho_do_cache(&self.pasta_de_dados, self.servidor.host())
    }

    pub fn servidor(&self) -> &Servidor {
        &self.servidor
    }

    /// O portão de rede da estação: o servidor (com a credencial) e o endpoint de token do
    /// Google, e nada mais.
    ///
    /// # Errors
    /// Endereço ou rota inválidos.
    pub fn rede(&self) -> Result<Rede, ErroDeRede> {
        let mut catalogo = Catalogo::novo().com(self.servidor.endereco()?);
        if let Some(google) = &self.google {
            catalogo = catalogo.com(google.endereco_do_token()?);
        }
        let mut config = ConfiguracaoDaRede::nova(catalogo)
            .relogio(self.relogio.clone())
            .autorizacao(Arc::new(CredencialDaEstacao::nova(
                self.guarda.clone(),
                self.servidor.host(),
            )))
            .agente_de_usuario(format!("SeuBolsoFeliz/{}", self.versao_das_regras));
        for (autoridade, base) in &self.rotas {
            config = config.rota(autoridade, base);
        }
        Rede::abrir(config)
    }
}

/// O estado que a thread da troca e a janela compartilham.
#[derive(Debug)]
struct Comum {
    rede: EstadoDaRede,
    ultima_troca: Option<String>,
    copia_de: Option<String>,
    esta_estacao: Option<Estacao>,
    revogada: bool,
    /// Sobe quando uma troca termina: a janela relê o que guarda em memória.
    trocas_terminadas: u64,
}

fn travar<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A estação desta máquina.
#[derive(Debug)]
pub struct EstacaoLocal {
    config: Arc<ConfiguracaoDaEstacao>,
    cliente: ClienteDoServidor,
    cache: Option<CacheDoLivro>,
    comum: Arc<Mutex<Comum>>,
    rodando: Arc<AtomicBool>,
    proxima: Option<Instante>,
}

impl EstacaoLocal {
    /// Abre a estação. Um cache sem credencial (estação revogada que não conseguiu apagar o
    /// arquivo, ou credencial apagada à mão) é apagado aqui. Se pareada e a configuração
    /// pede, já agenda a primeira troca.
    ///
    /// # Errors
    /// Rede mal configurada.
    pub fn abrir(config: ConfiguracaoDaEstacao) -> Result<Self, ErroDeRede> {
        let rede = config.rede()?;
        let cliente = ClienteDoServidor::novo(rede, config.servidor.clone());
        let caminho = config.caminho_do_cache();
        let pareada = config.guarda.tem(config.servidor.host());
        if !pareada {
            let _ = apagar_arquivos(&caminho);
        }
        let mut estacao = Self {
            config: Arc::new(config),
            cliente,
            cache: None,
            comum: Arc::new(Mutex::new(Comum {
                rede: if pareada {
                    EstadoDaRede::Sincronizando
                } else {
                    EstadoDaRede::NaoPareada
                },
                ultima_troca: None,
                copia_de: None,
                esta_estacao: None,
                revogada: false,
                trocas_terminadas: 0,
            })),
            rodando: Arc::new(AtomicBool::new(false)),
            proxima: None,
        };
        if pareada {
            estacao.abrir_cache();
            if estacao.config.sincronizar_ao_abrir {
                estacao.disparar();
            } else {
                travar(&estacao.comum).rede = EstadoDaRede::SemRede;
            }
        }
        Ok(estacao)
    }

    fn abrir_cache(&mut self) {
        if self.cache.is_none() && self.config.caminho_do_cache().exists() {
            self.cache = CacheDoLivro::abrir(&self.config.caminho_do_cache()).ok();
        }
        if let Some(cache) = &self.cache {
            let historico = cache.historico(TROCAS_NO_HISTORICO).unwrap_or_default();
            let mut comum = travar(&self.comum);
            comum.copia_de = cache.copia_de().ok().flatten();
            comum.ultima_troca = historico
                .iter()
                .find(|t| t.erro.is_none() && t.fim.is_some())
                .and_then(|t| t.fim.clone());
            comum.esta_estacao = esta_estacao(cache);
        }
    }

    /// O cliente do servidor (clone barato), para rodar fora do quadro o que bloqueia, como
    /// [`crate::enviar_lote`].
    pub fn cliente(&self) -> ClienteDoServidor {
        self.cliente.clone()
    }

    pub fn configuracao(&self) -> &ConfiguracaoDaEstacao {
        &self.config
    }

    /// O cache que a janela lê (`None` antes da primeira troca, ou depois da revogação).
    pub fn cache(&self) -> Option<&CacheDoLivro> {
        self.cache.as_ref()
    }

    /// Quantas trocas terminaram desde que a estação abriu (a janela relê ao mudar).
    pub fn trocas_terminadas(&self) -> u64 {
        travar(&self.comum).trocas_terminadas
    }

    pub fn pareada(&self) -> bool {
        self.config.guarda.tem(self.config.servidor.host())
    }

    /// Há uma troca em andamento.
    pub fn sincronizando(&self) -> bool {
        self.rodando.load(Ordering::Acquire)
    }

    /// O pareamento (login Google e `POST /estacoes/pareamento`), para rodar fora do quadro.
    pub fn pareador(&self) -> Pareador {
        Pareador::novo(
            self.cliente.clone(),
            self.config.guarda.clone(),
            self.config.google.clone(),
            self.config.relogio.clone(),
        )
    }

    /// A cada quadro: larga e apaga o cache de uma estação revogada, abre o cache que a
    /// primeira troca criou, e dispara a troca do intervalo.
    pub fn tique(&mut self) {
        let (revogada, terminou) = {
            let comum = travar(&self.comum);
            (comum.revogada, comum.trocas_terminadas)
        };
        if revogada {
            if self.cache.take().is_some() {
                let _ = apagar_arquivos(&self.config.caminho_do_cache());
            }
            return;
        }
        if terminou > 0 && self.cache.is_none() && !self.sincronizando() {
            self.abrir_cache();
        }
        let agora = self.config.relogio.agora();
        if let (Some(proxima), Some(_)) = (self.proxima, self.config.intervalo)
            && agora >= proxima
            && self.pareada()
        {
            self.disparar();
        }
    }

    /// Bloqueia até a troca em andamento terminar (testes e encerramento do app).
    pub fn esperar_troca(&mut self, prazo: Duration) -> bool {
        let inicio = std::time::Instant::now();
        while self.sincronizando() {
            if inicio.elapsed() > prazo {
                return false;
            }
            thread::sleep(Duration::from_millis(5));
        }
        self.tique();
        true
    }

    /// Depois de parear: apaga o estado de "não pareada" e sincroniza.
    pub fn apos_parear(&mut self) {
        {
            let mut comum = travar(&self.comum);
            comum.revogada = false;
            comum.rede = EstadoDaRede::Sincronizando;
        }
        self.disparar();
    }

    fn disparar(&mut self) {
        if self.rodando.swap(true, Ordering::AcqRel) {
            return;
        }
        if let Some(intervalo) = self.config.intervalo {
            self.proxima = Some(self.config.relogio.agora().mais(intervalo));
        }
        travar(&self.comum).rede = EstadoDaRede::Sincronizando;
        let config = self.config.clone();
        let cliente = self.cliente.clone();
        let comum = self.comum.clone();
        let rodando = self.rodando.clone();
        let iniciada = thread::Builder::new()
            .name("sbf-sincronizacao".to_owned())
            .spawn(move || {
                let desfecho = Sincronizador {
                    cliente: &cliente,
                    guarda: config.guarda.as_ref(),
                    relogio: config.relogio.as_ref(),
                    versao_das_regras: &config.versao_das_regras,
                }
                .trocar(&config.caminho_do_cache());
                registrar(&comum, &config, desfecho);
                rodando.store(false, Ordering::Release);
            });
        if iniciada.is_err() {
            self.rodando.store(false, Ordering::Release);
            travar(&self.comum).rede =
                EstadoDaRede::Falhou("não foi possível iniciar a sincronização".to_owned());
        }
    }

    fn cache_para_escrever(&mut self) -> ResultadoDeAlteracao<&mut CacheDoLivro> {
        if travar(&self.comum).revogada {
            return Err(ErroDeAlteracao::EstacaoRevogada);
        }
        self.abrir_cache();
        self.cache
            .as_mut()
            .ok_or_else(|| ErroDeAlteracao::Local("ainda não há cópia do livro-caixa".to_owned()))
    }

    fn agora(&self) -> String {
        instante_iso(self.config.relogio.agora())
    }

    fn ref_desta(&self) -> Ref {
        travar(&self.comum)
            .esta_estacao
            .as_ref()
            .map_or_else(|| Ref::nova(Tipo::Estacao, "esta"), |e| e.alvo.clone())
    }
}

fn esta_estacao(cache: &CacheDoLivro) -> Option<Estacao> {
    if let Ok(Some(lista)) = cache.estacoes()
        && let Some(e) = lista.estacoes.iter().find(|e| e.id == lista.esta)
    {
        return Some(estacao_da_janela(e, true));
    }
    cache
        .estado_do_servidor()
        .ok()
        .flatten()
        .map(|estado| estacao_da_janela(&estado.estacao, true))
}

fn registrar(comum: &Mutex<Comum>, config: &ConfiguracaoDaEstacao, desfecho: Desfecho) {
    // Lê o cache fora da trava (a janela consulta `estado()` a cada quadro).
    let esta = match &desfecho {
        Desfecho::Sincronizado { .. } => CacheDoLivro::abrir(&config.caminho_do_cache())
            .ok()
            .and_then(|c| esta_estacao(&c)),
        _ => None,
    };
    let mut comum = travar(comum);
    comum.trocas_terminadas += 1;
    comum.rede = match desfecho {
        Desfecho::Sincronizado {
            copia_de,
            regras_mais_novas,
            ..
        } => {
            comum.ultima_troca = Some(copia_de.clone());
            comum.copia_de = Some(copia_de);
            if esta.is_some() {
                comum.esta_estacao = esta;
            }
            match regras_mais_novas {
                Some(versao_do_servidor) => EstadoDaRede::RegrasMaisNovas { versao_do_servidor },
                None => EstadoDaRede::Sincronizado,
            }
        }
        Desfecho::SemRede => EstadoDaRede::SemRede,
        Desfecho::EstacaoRevogada => {
            comum.revogada = true;
            comum.copia_de = None;
            comum.esta_estacao = None;
            EstadoDaRede::EstacaoRevogada
        }
        Desfecho::AtualizeOApp => EstadoDaRede::AtualizeOApp,
        Desfecho::ServidorDesatualizado => EstadoDaRede::Falhou(
            "o servidor ainda não conhece este app; ele precisa ser atualizado".to_owned(),
        ),
        Desfecho::NaoPareada => EstadoDaRede::NaoPareada,
        Desfecho::Falhou(m) => EstadoDaRede::Falhou(m),
    };
}

impl Sincronizacao for EstacaoLocal {
    fn estado(&self) -> EstadoDaSincronizacao {
        let comum = travar(&self.comum);
        EstadoDaSincronizacao {
            esta_estacao: comum.esta_estacao.clone(),
            servidor: self.config.rotulo_do_servidor.clone(),
            ultima_troca: comum.ultima_troca.clone(),
            copia_de: comum.copia_de.clone(),
            rede: comum.rede.clone(),
            versao_das_regras: self.config.versao_das_regras.clone(),
        }
    }

    fn sincronizar_agora(&mut self) {
        if self.pareada() && !travar(&self.comum).revogada {
            self.disparar();
        }
    }

    fn estacoes(&self) -> ResultadoDeAlteracao<Vec<Estacao>> {
        let Some(cache) = &self.cache else {
            return Ok(Vec::new());
        };
        let Some(lista) = cache.estacoes()? else {
            return Ok(Vec::new());
        };
        let pedidas = cache.revogacoes_pedidas()?;
        let mut saida: Vec<Estacao> = lista
            .estacoes
            .iter()
            .map(|e| {
                let mut da_janela = estacao_da_janela(e, e.id == lista.esta);
                if pedidas.contains(&e.id.to_string()) {
                    da_janela.credencial = sbf_inventario::alteracao::Credencial::Revogada;
                }
                da_janela
            })
            .collect();
        saida.sort_by_key(|e| !e.esta);
        Ok(saida)
    }

    fn revogar(&mut self, estacao: &Ref) -> ResultadoDeAlteracao<()> {
        if estacao.tipo != Tipo::Estacao {
            return Err(ErroDeAlteracao::NaoEncontrado(estacao.clone()));
        }
        let chave = estacao.chave.clone();
        self.cache_para_escrever()?.pedir_revogacao(&chave)?;
        Ok(())
    }

    fn renomear(&mut self, _estacao: &Ref, _nome: &str) -> ResultadoDeAlteracao<()> {
        Err(ErroDeAlteracao::NaoSuportado(
            "o contrato da fatia 2 ainda não tem o recurso de renomear estação".to_owned(),
        ))
    }

    fn historico(&self) -> ResultadoDeAlteracao<Vec<Troca>> {
        match &self.cache {
            Some(c) => Ok(c.historico(TROCAS_NO_HISTORICO)?),
            None => Ok(Vec::new()),
        }
    }
}

impl Alteracoes for EstacaoLocal {
    fn propor(&mut self, pedido: &PedidoDeAlteracao) -> ResultadoDeAlteracao<Alteracao> {
        let agora = self.agora();
        let estacao = self.ref_desta();
        let cache = self.cache_para_escrever()?;
        if cache
            .estado_do_servidor()?
            .is_some_and(|e| e.contrato > VERSAO_DO_CONTRATO)
        {
            return Err(ErroDeAlteracao::ContratoMaisNovo);
        }
        alteracoes::propor(cache, pedido, estacao, &agora)
    }

    fn alteracoes(&self, so_em_aberto: bool) -> ResultadoDeAlteracao<Vec<Alteracao>> {
        let Some(cache) = &self.cache else {
            return Ok(Vec::new());
        };
        Ok(cache
            .alteracoes()?
            .into_iter()
            .map(|r| r.alteracao)
            .filter(|a| !so_em_aberto || a.estado.em_aberto())
            .collect())
    }

    fn alteracao(&self, id: &str) -> ResultadoDeAlteracao<Option<Alteracao>> {
        let Some(cache) = &self.cache else {
            return Ok(None);
        };
        Ok(cache.alteracao(id)?.map(|r| r.alteracao))
    }

    fn refazer(&mut self, id: &str) -> ResultadoDeAlteracao<Alteracao> {
        let agora = self.agora();
        let cache = self.cache_para_escrever()?;
        alteracoes::refazer(cache, id, &agora)
    }

    fn descartar(&mut self, id: &str) -> ResultadoDeAlteracao<()> {
        let cache = self.cache_para_escrever()?;
        alteracoes::descartar(cache, id)
    }

    fn categorias(&self) -> ResultadoDeAlteracao<Vec<OpcaoDeCategoria>> {
        match &self.cache {
            Some(c) => alteracoes::categorias(c),
            None => Ok(Vec::new()),
        }
    }
}

impl EstacaoLocal {
    fn cache_para_escrever_v2(&mut self) -> ResultadoDeEscrita<&mut CacheDoLivro> {
        if travar(&self.comum).revogada {
            return Err(ErroDeEscrita::EstacaoRevogada);
        }
        let versao = self.config.servidor.versao_do_contrato();
        self.abrir_cache();
        let cache = self
            .cache
            .as_mut()
            .ok_or_else(|| ErroDeEscrita::Local("ainda não há cópia do livro-caixa".to_owned()))?;
        if cache
            .estado_do_servidor()?
            .is_some_and(|e| e.contrato > versao)
        {
            return Err(ErroDeEscrita::ContratoMaisNovo);
        }
        Ok(cache)
    }
}

impl Escritas for EstacaoLocal {
    fn caixa_de_entrada(&self, so_a_decidir: bool) -> ResultadoDeEscrita<Vec<LoteNaCaixa>> {
        match &self.cache {
            Some(c) => Ok(c.caixa_de_entrada(so_a_decidir)?),
            None => Ok(Vec::new()),
        }
    }

    fn enfileirar(&mut self, acao: Acao) -> ResultadoDeEscrita<Escrita> {
        let agora = self.agora();
        self.cache_para_escrever_v2()?.enfileirar(acao, &agora)
    }

    fn escritas(&self, so_em_aberto: bool) -> ResultadoDeEscrita<Vec<Escrita>> {
        let Some(cache) = &self.cache else {
            return Ok(Vec::new());
        };
        Ok(cache
            .escritas()?
            .into_iter()
            .filter(|e| !so_em_aberto || e.estado.em_aberto())
            .collect())
    }

    fn pendentes(&self) -> ResultadoDeEscrita<std::collections::HashSet<(Colecao, Id)>> {
        match &self.cache {
            Some(c) => Ok(c.itens_pendentes()?),
            None => Ok(Default::default()),
        }
    }

    fn refazer_escrita(&mut self, id: Id) -> ResultadoDeEscrita<Escrita> {
        let agora = self.agora();
        self.cache_para_escrever_v2()?.refazer_escrita(id, &agora)
    }

    fn descartar_escrita(&mut self, id: Id) -> ResultadoDeEscrita<()> {
        self.cache_para_escrever_v2()?.descartar_escrita(id)
    }
}
