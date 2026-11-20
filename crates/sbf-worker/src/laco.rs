//! O laço do worker. Um passo faz, nesta ordem, o que estiver na hora:
//!
//! 1. **Vigia de pastas**: o `notify` avisa e o worker indexa só o que mudou
//!    ([`sbf_coleta::pastas::indexar_arquivo`], as mesmas regras de área da varredura);
//! 2. **Reconciliação**: de tempos em tempos, varredura incremental das mesmas pastas, para
//!    pegar o que o `notify` perdeu (fila cheia, disco montado depois, evento descartado). É
//!    barata: origem já vista (caminho e mtime) é pulada sem reler o arquivo;
//! 3. **Nuvem**: se a coleta está ligada em Opções › Nuvens, no intervalo configurado;
//! 4. **Mensageiros**: se ligados em Opções › Mensageiros, WhatsApp e Telegram (só as conversas
//!    que o usuário incluiu), no intervalo de `mensageiros.json`;
//! 5. **Gmail**: a cada `gmail_minutos` (padrão 30; `0` desliga), se o coletor foi ligado (o
//!    `sbf` só liga quando as credenciais OAuth estão no ambiente): anexos para
//!    `<cofre>/entrada/gmail/` e reconhecimento com as regras;
//!    **Telegram**: as conversas seguidas (`telegram.json`), a cada `telegram_minutos` (15);
//! 6. **Roteiros**: se há `roteiros.sqlite`, de 5 em 5 minutos executa os roteiros agendados
//!    que já passaram do intervalo (spec 10, C1);
//! 7. **Triagem**: um pequeno lote do agente, no horário configurado.
//! 8. **Índice local** (spec 10): atualização incremental dos embeddings, de 30 em 30 minutos,
//!    só se o modelo já foi baixado pela janela (o worker nunca baixa modelo);
//! 9. **Pontos de atenção** (spec 10): a análise por regras, de 6 em 6 horas.
//!
//! O que cada etapa **faz** de verdade vem por trait ([`ColetorDeNuvem`], [`Triador`]), para
//! o laço ser testável sem rclone nem `claude`. O estado vai para `worker-estado.json` a cada
//! passo em que algo muda e, no mínimo, a cada 10 s.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use rusqlite::Connection;
use sbf_coleta::pastas::{self, Contadores, Opcoes, Varredor, Vigia};
use sbf_inventario::caixa_de_entrada;

use crate::config::{Config, hora_local};
use crate::estado::{EstadoDoWorker, UltimaColeta, agora};
use crate::registro::{Evento, Falha, Registro};
use crate::{Resultado, banco};

/// Onde ficam os arquivos do worker.
#[derive(Debug, Clone)]
pub struct Dados {
    /// A pasta do cofre (acervo, `inventario.sqlite`).
    pub cofre: PathBuf,
    /// O `inventario.sqlite`.
    pub banco: PathBuf,
    /// A pasta de dados do app (fora do cofre): config, estado, log, lock, `agente.sqlite`.
    pub pasta: PathBuf,
}

impl Dados {
    /// `cofre/inventario.sqlite` e a pasta de dados dada.
    #[must_use]
    pub fn novos(cofre: &Path, pasta: &Path) -> Self {
        Self {
            cofre: cofre.to_path_buf(),
            banco: cofre.join("inventario.sqlite"),
            pasta: pasta.to_path_buf(),
        }
    }
    /// `worker.json`.
    #[must_use]
    pub fn config(&self) -> PathBuf {
        self.pasta.join("worker.json")
    }
    /// `worker-estado.json`.
    #[must_use]
    pub fn estado(&self) -> PathBuf {
        self.pasta.join("worker-estado.json")
    }
    /// `worker.lock`.
    #[must_use]
    pub fn lock(&self) -> PathBuf {
        self.pasta.join("worker.lock")
    }
    /// `worker.log`.
    #[must_use]
    pub fn log(&self) -> PathBuf {
        self.pasta.join("worker.log")
    }
    /// `agente.sqlite`.
    #[must_use]
    pub fn agente(&self) -> PathBuf {
        self.pasta.join("agente.sqlite")
    }
    /// `mensageiros.json`, onde a janela guarda o interruptor e as conversas incluídas
    /// (Opções › Mensageiros).
    #[must_use]
    pub fn mensageiros(&self) -> PathBuf {
        self.pasta.join("mensageiros.json")
    }
    /// `nuvens.json`, onde a janela guarda "coleta ligada" (Opções › Nuvens).
    #[must_use]
    pub fn nuvens(&self) -> PathBuf {
        self.pasta.join("nuvens.json")
    }
}

/// A coleta de nuvem está ligada em Opções › Nuvens? (`nuvens.json`, `coleta_ligada`.)
#[must_use]
pub fn coleta_de_nuvem_ligada(dados: &Dados) -> bool {
    #[derive(serde::Deserialize, Default)]
    struct P {
        #[serde(default)]
        coleta_ligada: bool,
    }
    std::fs::read_to_string(dados.nuvens())
        .ok()
        .and_then(|t| serde_json::from_str::<P>(&t).ok())
        .unwrap_or_default()
        .coleta_ligada
}

/// O interruptor e o intervalo dos mensageiros, lidos de `mensageiros.json` (o resto do
/// arquivo é do crate `sbf-mensageiros`). Sem arquivo: desligado.
#[must_use]
pub fn mensageiros_ligados(dados: &Dados) -> Option<u64> {
    #[derive(serde::Deserialize)]
    struct P {
        #[serde(default)]
        ligado: bool,
        #[serde(default = "trinta")]
        intervalo_minutos: u64,
    }
    fn trinta() -> u64 {
        30
    }
    let p: P = serde_json::from_str(&std::fs::read_to_string(dados.mensageiros()).ok()?).ok()?;
    p.ligado.then_some(p.intervalo_minutos)
}

/// O que uma coleta de mensageiros fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDeMensageiros {
    /// Mensagens vistas.
    pub mensagens: usize,
    /// Origens novas no inventário.
    pub novos: usize,
}

/// Uma coleta de mensageiros (a real é `sbf_mensageiros`, montada no `sbf`).
pub trait ColetorDeMensageiros {
    /// Coleta as conversas incluídas de cada mensageiro pareado ou importado.
    ///
    /// # Errors
    /// A categoria da falha (a mensagem não sai do coletor).
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDeMensageiros, Falha>;
}

/// O que uma coleta do Gmail fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDoGmail {
    /// Mensagens lidas.
    pub mensagens: usize,
    /// Conteúdos novos no inventário.
    pub novos: usize,
}

/// Uma coleta do Gmail (a real é `sbf_gmail::coletar`, montada no `sbf`).
pub trait ColetorDeGmail {
    /// Coleta as mensagens novas que casam com a consulta padrão.
    ///
    /// # Errors
    /// A categoria da falha (a mensagem não sai do coletor).
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDoGmail, Falha>;

    /// Depois de [`Falha::Autorizacao`] o worker para de chamar [`Self::coletar`] e só
    /// pergunta isto (barato, sem rede): `true` quando a pessoa entrou de novo e vale tentar.
    /// O padrão é `true` (sem suspensão que dependa de login).
    fn pode_tentar_de_novo(&mut self) -> bool {
        true
    }

    /// Os ids das contas com a autorização vencida agora (lê só o chaveiro; sem rede). O worker
    /// registra cada uma ao vencer, mostra a contagem na barra e segue com as outras contas.
    fn contas_vencidas(&mut self) -> Vec<String> {
        Vec::new()
    }
}

/// O que uma coleta do Telegram fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDoTelegram {
    /// Conversas consultadas.
    pub conversas: usize,
    /// Arquivos novos no inventário.
    pub novos: usize,
}

/// Uma coleta do Telegram (a real é `sbf_telegram::coleta`, montada no `sbf`).
pub trait ColetorDeTelegram {
    /// Coleta as conversas seguidas. Sem sessão guardada ou sem conversa seguida, é um `Ok`
    /// vazio (não é falha: o usuário ainda não ligou).
    ///
    /// # Errors
    /// A categoria da falha (a mensagem não sai do coletor).
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDoTelegram, Falha>;
}

/// O que uma coleta de nuvem fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDaNuvem {
    /// Itens listados no remoto.
    pub listados: usize,
    /// Itens baixados.
    pub baixados: usize,
}

/// Uma coleta de nuvem (a real é `ColetaDeNuvem`, no `sbf`).
pub trait ColetorDeNuvem {
    /// Coleta todas as nuvens conectadas. `cancelar` vira `true` para parar.
    ///
    /// # Errors
    /// A categoria da falha (a mensagem não sai do coletor).
    fn coletar(&mut self, cancelar: &AtomicBool) -> Result<ResumoDaNuvem, Falha>;
}

/// O que uma passada dos roteiros agendados fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDeRoteiros {
    /// Roteiros executados.
    pub executados: usize,
    /// Arquivos baixados.
    pub baixados: usize,
    /// Achados novos no inventário.
    pub novos: usize,
}

/// A execução dos roteiros agendados (a real é `sbf_roteiros::servico`, montada no `sbf`).
pub trait ColetorDeRoteiros {
    /// Executa os roteiros que já passaram do intervalo em `agora_` (segundos Unix).
    ///
    /// # Errors
    /// A categoria da falha (a mensagem não sai do coletor).
    fn executar_devidos(
        &mut self,
        agora_: u64,
        cancelar: &AtomicBool,
    ) -> Result<ResumoDeRoteiros, Falha>;
}

/// O que um ciclo da triagem fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDaTriagem {
    /// Itens enviados à IA.
    pub enviados: usize,
    /// Sugestões da IA.
    pub sugestoes: usize,
    /// Sugestões de regra.
    pub por_regra: usize,
    /// Itens que ainda esperam.
    pub na_fila: usize,
}

/// Um ciclo do agente de triagem (o real é `sbf_triagem::agente::rodar_ciclo`).
pub trait Triador {
    /// Roda um lote de até `lote` itens.
    ///
    /// # Errors
    /// A categoria da falha.
    fn ciclo(&mut self, lote: usize, cancelar: &AtomicBool) -> Result<ResumoDaTriagem, Falha>;
}

/// Um arquivo novo de uma pasta vigiada, a reconhecer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArquivoNovo {
    /// O sha256 do conteúdo (já indexado no inventário).
    pub sha256: String,
    pub caminho: PathBuf,
    /// A fonte da pasta no `worker.json` (`celular` é o espelho do celular).
    pub fonte: String,
}

/// O reconhecimento de um arquivo que chegou numa pasta vigiada (o real é o
/// `ReconhecedorDoCofre` com o canal da pasta, montado no `sbf`): o resultado entra na Caixa
/// de entrada. Quem chama: o vigia (arquivo que acabou de aparecer) e a reconciliação, **só
/// para os achados novos que ela indexou e que ainda não têm linha na Caixa de entrada**
/// (o que chegou com o vigia parado). Nunca o acervo antigo.
pub trait ReconhecedorDePastas {
    /// Reconhece um arquivo novo.
    ///
    /// # Errors
    /// A categoria da falha.
    fn reconhecer(&mut self, arquivo: &ArquivoNovo, cancelar: &AtomicBool) -> Result<(), Falha>;
}

/// Quantos achados novos a reconciliação reconhece por pasta e por volta (o resto fica para a
/// próxima: o laço não pode ficar preso num lote grande).
const LOTE_DE_PENDENTES: u32 = 50;

/// O que uma atualização do índice local fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDoIndice {
    pub embutidos: usize,
    pub mantidos: usize,
    pub removidos: usize,
}

/// Atualiza o índice local (o real é `sbf_indice::fontes::atualizar`, montado no `sbf`).
pub trait Indexador {
    /// `Ok(None)`: o modelo ainda não foi baixado, nada a fazer (não é falha).
    ///
    /// # Errors
    /// A categoria da falha.
    fn atualizar(&mut self, cancelar: &AtomicBool) -> Result<Option<ResumoDoIndice>, Falha>;
}

/// O que uma análise de pontos de atenção fez.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDosPontos {
    pub novos: usize,
    pub reabertos: usize,
    pub fechados: usize,
}

/// A análise dos pontos de atenção (a real é `sbf_pontos::rodar`, montada no `sbf`).
pub trait Analista {
    /// # Errors
    /// A categoria da falha.
    fn analisar(&mut self) -> Result<ResumoDosPontos, Falha>;
}

/// O que uma volta de OCR fez. Só contagens.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResumoDoOcr {
    /// Imagens com texto lido e gravado.
    pub lidas: usize,
    /// Imagens que o OCR leu e não tinham texto.
    pub sem_texto: usize,
    /// Imagens que não puderam ser lidas.
    pub falhas: usize,
    /// Sobraram imagens novas para a próxima volta.
    pub pendentes: bool,
    /// Não há tesseract neste sistema (não é falha: nada foi feito).
    pub indisponivel: bool,
    /// A causa mais comum das falhas da volta (uma palavra, sem caminho nem conteúdo).
    pub causa: Option<&'static str>,
}

/// Lê o texto das imagens novas do inventário (o real é `sbf_ocr::ler_imagens_novas`,
/// montado no `sbf`).
pub trait LeitorDeImagens {
    /// Lê um lote (poucas imagens por volta, para o vigia não ficar parado).
    ///
    /// # Errors
    /// A categoria da falha.
    fn ler_novas(&mut self, cancelar: &AtomicBool) -> Result<ResumoDoOcr, Falha>;
}

/// De quanto em quanto tempo o OCR procura imagens novas, em segundos (se o lote anterior
/// deixou imagens, a próxima volta já roda).
pub const INTERVALO_DO_OCR: u64 = 60;
/// Quanto o OCR espera quando o lote anterior não leu nada (só falhou) e ainda sobraram
/// imagens: sem isto ele giraria a cada volta do laço em cima de um monte de imagens ruins.
pub const INTERVALO_APOS_LOTE_SEM_LEITURA: u64 = 5 * 60;
/// De quanto em quanto tempo o índice é atualizado, em segundos.
pub const INTERVALO_DO_INDICE: u64 = 30 * 60;
/// De quanto em quanto tempo os pontos de atenção são analisados, em segundos.
pub const INTERVALO_DOS_PONTOS: u64 = 6 * 3600;

/// De quanto em quanto tempo, em segundos, o estado é renovado mesmo com o laço preso num
/// passo longo (a varredura inicial de uma pasta grande): bem abaixo de
/// [`crate::estado::VALIDADE_S`].
pub const BATIMENTO_DO_ESTADO: u64 = 10;

/// O worker.
pub struct Worker {
    dados: Dados,
    config: Config,
    nuvem: Option<Box<dyn ColetorDeNuvem>>,
    mensageiros: Option<Box<dyn ColetorDeMensageiros>>,
    gmail: Option<Box<dyn ColetorDeGmail>>,
    telegram: Option<Box<dyn ColetorDeTelegram>>,
    roteiros: Option<Box<dyn ColetorDeRoteiros>>,
    triador: Option<Box<dyn Triador>>,
    indexador: Option<Box<dyn Indexador>>,
    analista: Option<Box<dyn Analista>>,
    leitor: Option<Box<dyn LeitorDeImagens>>,
    reconhecedor_de_pastas: Option<Box<dyn ReconhecedorDePastas>>,
    registro: Registro,
    /// O estado que vai para o arquivo.
    pub estado: EstadoDoWorker,
    vigia: Option<Vigia>,
    varreduras: HashMap<String, i64>,
    conn: Option<Connection>,
    proxima_reconciliacao: u64,
    proxima_nuvem: u64,
    proxima_mensageiros: u64,
    proxima_gmail: u64,
    /// Todas as contas de e-mail venceram (`Autorizacao`): não coleta até `pode_tentar_de_novo`.
    gmail_suspenso: bool,
    /// As contas de e-mail com autorização vencida já registradas no log.
    gmail_vencidas: std::collections::HashSet<String>,
    proxima_telegram: u64,
    proxima_roteiros: u64,
    proxima_triagem: u64,
    proximo_indice: u64,
    proximos_pontos: u64,
    proximo_ocr: u64,
    gravado_em: u64,
    sujo: bool,
    /// Quando o arquivo de estado foi escrito pela última vez (o laço ou o batimento).
    pulso: Arc<AtomicU64>,
    /// De quanto em quanto tempo, em segundos, o batimento renova o estado se o laço está
    /// preso num passo longo.
    batimento_s: u64,
}

impl std::fmt::Debug for Worker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Worker").finish_non_exhaustive()
    }
}

impl Worker {
    /// Prepara o worker: abre o log, começa a vigiar as pastas e monta o estado inicial.
    ///
    /// # Errors
    /// Falha de disco ao abrir o log.
    pub fn novo(
        dados: Dados,
        config: Config,
        nuvem: Option<Box<dyn ColetorDeNuvem>>,
        triador: Option<Box<dyn Triador>>,
    ) -> Resultado<Self> {
        std::fs::create_dir_all(&dados.pasta)?;
        let mut registro = Registro::abrir(&dados.log())?;
        let raizes: Vec<PathBuf> = config.pastas.iter().map(|p| p.raiz.clone()).collect();
        let vigia = if raizes.is_empty() {
            None
        } else {
            match Vigia::iniciar(&raizes) {
                Ok(v) => Some(v),
                Err(_) => {
                    let _ = registro.registrar(&Evento::Falhou {
                        passo: "vigia",
                        falha: Falha::Disco,
                    });
                    None
                }
            }
        };
        let agora_ = agora();
        let estado = EstadoDoWorker {
            pid: std::process::id(),
            versao: env!("CARGO_PKG_VERSION").to_owned(),
            iniciado_em: agora_,
            vigiando: vigia.as_ref().map_or(0, |v| v.vigiadas.len()),
            triagem_ligada: config.triagem.ligada,
            ..EstadoDoWorker::default()
        };
        let mut w = Self {
            dados,
            config,
            nuvem,
            mensageiros: None,
            gmail: None,
            telegram: None,
            roteiros: None,
            triador,
            indexador: None,
            analista: None,
            leitor: None,
            reconhecedor_de_pastas: None,
            registro,
            estado,
            vigia,
            varreduras: HashMap::new(),
            conn: None,
            // A primeira reconciliação roda logo: é a varredura inicial das pastas.
            proxima_reconciliacao: agora_,
            proxima_nuvem: agora_,
            proxima_mensageiros: agora_,
            proxima_gmail: agora_,
            gmail_suspenso: false,
            gmail_vencidas: std::collections::HashSet::new(),
            proxima_telegram: agora_,
            proxima_roteiros: agora_,
            proxima_triagem: agora_,
            proximo_indice: agora_,
            proximos_pontos: agora_,
            proximo_ocr: agora_,
            gravado_em: 0,
            sujo: true,
            pulso: Arc::new(AtomicU64::new(0)),
            batimento_s: BATIMENTO_DO_ESTADO,
        };
        let _ = w.registro.registrar(&Evento::Iniciou {
            vigiadas: w.estado.vigiando,
        });
        Ok(w)
    }

    /// Liga a coleta de mensageiros (sem mudar a assinatura de [`Worker::novo`]).
    #[must_use]
    pub fn com_mensageiros(mut self, coletor: Box<dyn ColetorDeMensageiros>) -> Self {
        self.mensageiros = Some(coletor);
        self
    }

    /// Liga a coleta do Gmail (sem mudar a assinatura de [`Worker::novo`]).
    #[must_use]
    pub fn com_gmail(mut self, coletor: Box<dyn ColetorDeGmail>) -> Self {
        self.gmail = Some(coletor);
        self
    }

    /// Liga a coleta do Telegram (conversas seguidas), de `telegram_minutos` em `telegram_minutos`.
    #[must_use]
    pub fn com_telegram(mut self, coletor: Box<dyn ColetorDeTelegram>) -> Self {
        self.telegram = Some(coletor);
        self
    }

    /// Liga a execução dos roteiros agendados (sem mudar a assinatura de [`Worker::novo`]).
    #[must_use]
    pub fn com_roteiros(mut self, coletor: Box<dyn ColetorDeRoteiros>) -> Self {
        self.roteiros = Some(coletor);
        self
    }

    /// Liga a atualização do índice local (spec 10).
    #[must_use]
    pub fn com_indice(mut self, indexador: Box<dyn Indexador>) -> Self {
        self.indexador = Some(indexador);
        self
    }

    /// Liga o OCR das imagens novas (ADR-016 do desktop: só lê, nunca envia).
    #[must_use]
    pub fn com_ocr(mut self, leitor: Box<dyn LeitorDeImagens>) -> Self {
        self.leitor = Some(leitor);
        self
    }

    /// Liga o reconhecimento dos arquivos novos das pastas vigiadas (Caixa de entrada).
    #[must_use]
    pub fn com_reconhecimento_das_pastas(mut self, r: Box<dyn ReconhecedorDePastas>) -> Self {
        self.reconhecedor_de_pastas = Some(r);
        self
    }

    /// Liga a análise dos pontos de atenção (spec 10).
    #[must_use]
    pub fn com_pontos(mut self, analista: Box<dyn Analista>) -> Self {
        self.analista = Some(analista);
        self
    }

    fn banco(&mut self) -> Resultado<&Connection> {
        if self.conn.is_none() {
            let conn = banco::abrir(&self.dados.banco)?;
            // A Caixa de entrada existe desde aqui: só o que for achado depois passa pelo
            // reconhecimento da reconciliação (o acervo antigo não).
            caixa_de_entrada::garantir_tabela(&conn)?;
            self.conn = Some(conn);
        }
        self.conn.as_ref().ok_or(crate::Erro::Configuracao)
    }

    fn falhou(&mut self, passo: &'static str, falha: Falha) {
        self.estado.falhas += 1;
        self.sujo = true;
        let _ = self.registro.registrar(&Evento::Falhou { passo, falha });
    }

    fn fonte_de(&self, caminho: &Path) -> Option<(String, PathBuf)> {
        self.config
            .pastas
            .iter()
            .filter(|p| caminho.starts_with(&p.raiz))
            .max_by_key(|p| p.raiz.as_os_str().len())
            .map(|p| (p.fonte.clone(), p.raiz.clone()))
    }

    fn varredura_do_vigia(&mut self, fonte: &str, raiz: &Path) -> Resultado<i64> {
        if let Some(id) = self.varreduras.get(fonte) {
            return Ok(*id);
        }
        let id = pastas::abrir_varredura_do_vigia(self.banco()?, fonte, raiz)?;
        self.varreduras.insert(fonte.to_owned(), id);
        Ok(id)
    }

    /// O vigia: espera até `espera` por avisos e indexa o que chegou.
    fn passo_do_vigia(&mut self, espera: Duration, agora_: u64) {
        let Some(vigia) = self.vigia.as_ref() else {
            std::thread::sleep(espera);
            return;
        };
        let avisados = vigia.mudancas(espera);
        if avisados.is_empty() {
            return;
        }
        let mut por_fonte: HashMap<String, (PathBuf, Vec<PathBuf>)> = HashMap::new();
        for a in avisados {
            if let Some((fonte, raiz)) = self.fonte_de(&a) {
                por_fonte
                    .entry(fonte)
                    .or_insert((raiz, Vec::new()))
                    .1
                    .push(a);
            }
        }
        for (fonte, (raiz, arquivos)) in por_fonte {
            let mut novos_arquivos: Vec<ArquivoNovo> = Vec::new();
            let resultado = (|| -> Resultado<Contadores> {
                let varredura = self.varredura_do_vigia(&fonte, &raiz)?;
                let cofre = self.dados.cofre.clone();
                let conn = self.banco()?;
                let mut c = Contadores::default();
                for a in &arquivos {
                    let destino =
                        pastas::indexar_arquivo(conn, &cofre, &fonte, varredura, a, &mut c)?;
                    if destino == pastas::Destino::Novo
                        && let Some(sha256) = caixa_de_entrada::sha_da_origem_local(conn, a)?
                    {
                        novos_arquivos.push(ArquivoNovo {
                            sha256,
                            caminho: a.clone(),
                            fonte: fonte.clone(),
                        });
                    }
                }
                Ok(c)
            })();
            match resultado {
                Ok(c) => {
                    let _ = self.registro.registrar(&Evento::Vigia {
                        fonte: &fonte,
                        novos: c.novos,
                        avisados: arquivos.len(),
                    });
                    self.coletou("pastas", c.novos, agora_);
                    self.reconhecer_os_novos(&novos_arquivos, &AtomicBool::new(false));
                }
                Err(_) => self.falhou("vigia", Falha::Banco),
            }
        }
    }

    /// Os arquivos novos passam pelo reconhecimento (e entram na Caixa de entrada). Uma
    /// falha de um arquivo não para os outros.
    fn reconhecer_os_novos(&mut self, arquivos: &[ArquivoNovo], cancelar: &AtomicBool) {
        let Some(mut r) = self.reconhecedor_de_pastas.take() else {
            return;
        };
        for a in arquivos {
            if cancelar.load(Ordering::Relaxed) {
                break;
            }
            if let Err(f) = r.reconhecer(a, cancelar) {
                self.falhou("reconhecimento", f);
            }
        }
        self.reconhecedor_de_pastas = Some(r);
    }

    /// Os achados novos que a reconciliação indexou e que ainda não têm linha na Caixa de
    /// entrada (o vigia estava parado quando eles chegaram).
    fn reconhecer_os_pendentes(&mut self, fonte: &str, cancelar: &AtomicBool) {
        if self.reconhecedor_de_pastas.is_none() {
            return;
        }
        let pendentes = match self.banco().and_then(|c| {
            Ok(caixa_de_entrada::pendentes_de_reconhecer(
                c,
                fonte,
                LOTE_DE_PENDENTES,
            )?)
        }) {
            Ok(p) => p,
            Err(_) => return self.falhou("reconhecimento", Falha::Banco),
        };
        let arquivos: Vec<ArquivoNovo> = pendentes
            .into_iter()
            .map(|p| ArquivoNovo {
                sha256: p.sha256,
                caminho: PathBuf::from(p.caminho),
                fonte: fonte.to_owned(),
            })
            .collect();
        self.reconhecer_os_novos(&arquivos, cancelar);
    }

    fn coletou(&mut self, origem: &str, novos: u64, agora_: u64) {
        if novos > 0 {
            self.estado.ultima_coleta = Some(UltimaColeta {
                quando: agora_,
                origem: origem.to_owned(),
                novos,
            });
            self.sujo = true;
        }
    }

    /// Varredura incremental de todas as pastas configuradas.
    fn reconciliar(&mut self, cancelar: &AtomicBool, agora_: u64) {
        for p in self.config.pastas.clone() {
            if cancelar.load(Ordering::Relaxed) {
                return;
            }
            let cofre = self.dados.cofre.clone();
            let resultado = (|| -> Resultado<Contadores> {
                let conn = self.banco()?;
                let mut v = Varredor::novo(
                    conn,
                    &p.fonte,
                    &p.raiz,
                    Opcoes {
                        cofre,
                        ..Opcoes::default()
                    },
                )?;
                let completa = v.varrer(cancelar)?;
                Ok(v.finalizar(if completa {
                    "concluida"
                } else {
                    "interrompida"
                })?)
            })();
            match resultado {
                Ok(c) => {
                    let _ = self.registro.registrar(&Evento::Reconciliou {
                        fonte: &p.fonte,
                        novos: c.novos,
                        erros: c.erros,
                    });
                    self.coletou("pastas", c.novos, agora_);
                    self.reconhecer_os_pendentes(&p.fonte, cancelar);
                }
                Err(_) => self.falhou("reconciliacao", Falha::Banco),
            }
        }
    }

    /// Um passo do laço. `espera` é quanto o vigia pode esperar por avisos (faz de "sono").
    pub fn passo(&mut self, agora_: u64, espera: Duration, cancelar: &AtomicBool) {
        self.passo_do_vigia(espera, agora_);
        if cancelar.load(Ordering::Relaxed) {
            return;
        }
        if agora_ >= self.proxima_reconciliacao {
            self.proxima_reconciliacao = agora_ + self.config.reconciliar_minutos.max(1) * 60;
            if !self.config.pastas.is_empty() {
                self.reconciliar(cancelar, agora_);
            }
        }
        let nuvem_ligada = coleta_de_nuvem_ligada(&self.dados);
        if nuvem_ligada != self.estado.nuvem_ligada {
            self.estado.nuvem_ligada = nuvem_ligada;
            self.sujo = true;
        }
        if nuvem_ligada && agora_ >= self.proxima_nuvem {
            self.proxima_nuvem = agora_ + self.config.nuvem_minutos.max(1) * 60;
            self.passo_da_nuvem(cancelar, agora_);
        }
        let mensageiros = mensageiros_ligados(&self.dados);
        if mensageiros.is_some() != self.estado.mensageiros_ligados {
            self.estado.mensageiros_ligados = mensageiros.is_some();
            self.sujo = true;
        }
        if let Some(minutos) = mensageiros
            && agora_ >= self.proxima_mensageiros
            && !cancelar.load(Ordering::Relaxed)
        {
            self.proxima_mensageiros = agora_ + minutos.max(1) * 60;
            self.passo_dos_mensageiros(cancelar, agora_);
        }
        if self.config.gmail_minutos > 0
            && self.gmail.is_some()
            && agora_ >= self.proxima_gmail
            && !cancelar.load(Ordering::Relaxed)
        {
            self.passo_do_gmail(cancelar, agora_);
        }
        if self.config.telegram_minutos > 0
            && self.telegram.is_some()
            && agora_ >= self.proxima_telegram
            && !cancelar.load(Ordering::Relaxed)
        {
            self.proxima_telegram = agora_ + self.config.telegram_minutos * 60;
            self.passo_do_telegram(cancelar, agora_);
        }
        if self.dados.pasta.join("roteiros.sqlite").exists()
            && agora_ >= self.proxima_roteiros
            && !cancelar.load(Ordering::Relaxed)
        {
            self.proxima_roteiros = agora_ + 5 * 60;
            self.passo_dos_roteiros(cancelar, agora_);
        }
        if self.config.triagem.ligada
            && agora_ >= self.proxima_triagem
            && self
                .config
                .triagem
                .na_janela(hora_local(agora_, self.config.fuso_horas))
            && !cancelar.load(Ordering::Relaxed)
        {
            self.proxima_triagem = agora_ + self.config.triagem.intervalo_minutos.max(1) * 60;
            self.passo_da_triagem(cancelar, agora_);
        }
        if agora_ >= self.proximo_ocr && !cancelar.load(Ordering::Relaxed) {
            self.proximo_ocr = agora_ + INTERVALO_DO_OCR;
            self.passo_do_ocr(cancelar, agora_);
        }
        if agora_ >= self.proximo_indice && !cancelar.load(Ordering::Relaxed) {
            self.proximo_indice = agora_ + INTERVALO_DO_INDICE;
            self.passo_do_indice(cancelar);
        }
        if agora_ >= self.proximos_pontos && !cancelar.load(Ordering::Relaxed) {
            self.proximos_pontos = agora_ + INTERVALO_DOS_PONTOS;
            self.passo_dos_pontos();
        }
        if self.sujo || agora_.saturating_sub(self.gravado_em) >= 10 {
            self.gravar_estado();
        }
    }

    fn passo_do_ocr(&mut self, cancelar: &AtomicBool, agora_: u64) {
        let Some(mut l) = self.leitor.take() else {
            return;
        };
        match l.ler_novas(cancelar) {
            Ok(r) => {
                if r.lidas + r.sem_texto + r.falhas > 0 {
                    let _ = self.registro.registrar(&Evento::Ocr {
                        lidas: r.lidas,
                        sem_texto: r.sem_texto,
                        falhas: r.falhas,
                        causa: r.causa,
                    });
                }
                if r.pendentes && !r.indisponivel {
                    self.proximo_ocr = if r.lidas + r.sem_texto > 0 {
                        // Sobrou imagem e o lote rendeu: a próxima volta já lê o seguinte.
                        agora_
                    } else {
                        // O lote só falhou: não gira em cima das mesmas imagens ruins.
                        agora_ + INTERVALO_APOS_LOTE_SEM_LEITURA
                    };
                }
            }
            Err(f) => self.falhou("ocr", f),
        }
        self.leitor = Some(l);
    }

    fn passo_do_indice(&mut self, cancelar: &AtomicBool) {
        let Some(mut i) = self.indexador.take() else {
            return;
        };
        match i.atualizar(cancelar) {
            Ok(Some(r)) => {
                let _ = self.registro.registrar(&Evento::Indice {
                    embutidos: r.embutidos,
                    mantidos: r.mantidos,
                    removidos: r.removidos,
                });
            }
            Ok(None) => {}
            Err(f) => self.falhou("indice", f),
        }
        self.indexador = Some(i);
    }

    fn passo_dos_pontos(&mut self) {
        let Some(mut a) = self.analista.take() else {
            return;
        };
        match a.analisar() {
            Ok(r) => {
                let _ = self.registro.registrar(&Evento::Pontos {
                    novos: r.novos,
                    reabertos: r.reabertos,
                    fechados: r.fechados,
                });
            }
            Err(f) => self.falhou("pontos", f),
        }
        self.analista = Some(a);
    }

    fn passo_da_nuvem(&mut self, cancelar: &AtomicBool, agora_: u64) {
        let Some(mut n) = self.nuvem.take() else {
            return;
        };
        match n.coletar(cancelar) {
            Ok(r) => {
                let _ = self.registro.registrar(&Evento::Nuvem {
                    listados: r.listados,
                    baixados: r.baixados,
                });
                self.coletou("nuvem", r.baixados as u64, agora_);
            }
            Err(f) => self.falhou("nuvem", f),
        }
        self.nuvem = Some(n);
    }

    fn passo_dos_mensageiros(&mut self, cancelar: &AtomicBool, agora_: u64) {
        let Some(mut m) = self.mensageiros.take() else {
            return;
        };
        match m.coletar(cancelar) {
            Ok(r) => {
                let _ = self.registro.registrar(&Evento::Mensageiros {
                    mensagens: r.mensagens,
                    novos: r.novos,
                });
                self.coletou("mensageiros", r.novos as u64, agora_);
            }
            Err(f) => self.falhou("mensageiros", f),
        }
        self.mensageiros = Some(m);
    }

    /// Quanto esperar para olhar de novo se a pessoa entrou no Gmail (sem rede, só o chaveiro).
    const ESPERA_DA_SUSPENSAO_S: u64 = 60;

    fn passo_do_gmail(&mut self, cancelar: &AtomicBool, agora_: u64) {
        let Some(mut g) = self.gmail.take() else {
            return;
        };
        if self.gmail_suspenso {
            if !g.pode_tentar_de_novo() {
                // Continua vencido: nada de rede, só olha de novo daqui a pouco.
                self.proxima_gmail = agora_ + Self::ESPERA_DA_SUSPENSAO_S;
                self.gmail = Some(g);
                return;
            }
            self.gmail_suspenso = false;
        }
        self.proxima_gmail = agora_ + self.config.gmail_minutos * 60;
        match g.coletar(cancelar) {
            Ok(r) => {
                let _ = self.registro.registrar(&Evento::Gmail {
                    mensagens: r.mensagens,
                    novos: r.novos,
                });
                self.coletou("gmail", r.novos as u64, agora_);
            }
            Err(f) => {
                if f == Falha::Autorizacao {
                    // Todas as contas venceram: não martela o Google, suspende até a pessoa
                    // entrar de novo.
                    self.gmail_suspenso = true;
                    self.proxima_gmail = agora_ + Self::ESPERA_DA_SUSPENSAO_S;
                }
                self.falhou("gmail", f);
            }
        }
        // Cada conta com a autorização vencida é registrada uma vez (quando vence); as outras
        // contas seguem coletando normalmente.
        let vencidas = g.contas_vencidas();
        for id in vencidas
            .iter()
            .filter(|i| !self.gmail_vencidas.contains(*i))
        {
            let _ = self.registro.registrar(&Evento::FalhouConta {
                passo: "gmail",
                falha: Falha::Autorizacao,
                conta: id,
            });
        }
        let n = u32::try_from(vencidas.len()).unwrap_or(u32::MAX);
        if n != self.estado.gmail_contas_vencidas {
            self.estado.gmail_contas_vencidas = n;
            self.sujo = true;
        }
        self.gmail_vencidas = vencidas.into_iter().collect();
        self.gmail = Some(g);
    }

    fn passo_do_telegram(&mut self, cancelar: &AtomicBool, agora_: u64) {
        let Some(mut t) = self.telegram.take() else {
            return;
        };
        match t.coletar(cancelar) {
            Ok(r) => {
                if r.conversas > 0 {
                    let _ = self.registro.registrar(&Evento::Telegram {
                        conversas: r.conversas,
                        novos: r.novos,
                    });
                }
                self.coletou("telegram", r.novos as u64, agora_);
            }
            Err(f) => self.falhou("telegram", f),
        }
        self.telegram = Some(t);
    }

    fn passo_dos_roteiros(&mut self, cancelar: &AtomicBool, agora_: u64) {
        let Some(mut r) = self.roteiros.take() else {
            return;
        };
        match r.executar_devidos(agora_, cancelar) {
            Ok(res) => {
                if res.executados > 0 {
                    let _ = self.registro.registrar(&Evento::Roteiros {
                        executados: res.executados,
                        baixados: res.baixados,
                    });
                }
                self.coletou("roteiros", res.novos as u64, agora_);
            }
            Err(f) => self.falhou("roteiros", f),
        }
        self.roteiros = Some(r);
    }

    fn passo_da_triagem(&mut self, cancelar: &AtomicBool, agora_: u64) {
        let Some(mut t) = self.triador.take() else {
            return;
        };
        match t.ciclo(self.config.triagem.lote.max(1), cancelar) {
            Ok(r) => {
                let _ = self.registro.registrar(&Evento::Triagem {
                    enviados: r.enviados,
                    sugestoes: r.sugestoes,
                    por_regra: r.por_regra,
                    na_fila: r.na_fila,
                });
                self.estado.fila_triagem = r.na_fila;
                self.estado.ultima_triagem = Some(agora_);
                self.sujo = true;
                // Ainda há fila: o próximo lote não espera o intervalo inteiro.
                if r.na_fila > 0 && r.enviados > 0 {
                    self.proxima_triagem = agora_ + 60;
                }
            }
            Err(f) => self.falhou("triagem", f),
        }
        self.triador = Some(t);
    }

    /// Troca o intervalo do batimento do estado (o padrão é [`BATIMENTO_DO_ESTADO`]).
    #[must_use]
    pub fn com_batimento(mut self, segundos: u64) -> Self {
        self.batimento_s = segundos.max(1);
        self
    }

    fn gravar_estado(&mut self) {
        match self.estado.gravar(&self.dados.estado()) {
            Ok(()) => {
                self.gravado_em = self.estado.atualizado_em;
                self.pulso
                    .store(self.estado.atualizado_em, Ordering::Relaxed);
                self.sujo = false;
            }
            Err(_) => {
                self.gravado_em = agora();
                let _ = self.registro.registrar(&Evento::Falhou {
                    passo: "estado",
                    falha: Falha::Disco,
                });
            }
        }
    }

    /// Roda até `cancelar` virar `true`. Cada volta dura ~1 s (o vigia faz de sono).
    ///
    /// O estado (pid novo, `iniciado_em`, pastas vigiadas) é gravado já na largada, e uma thread
    /// de batimento o renova enquanto o laço estiver preso num passo longo (a varredura inicial
    /// de uma pasta grande leva minutos): sem isto a janela mostrava "parado" com o serviço ativo.
    pub fn rodar(&mut self, cancelar: &AtomicBool) {
        self.gravar_estado();
        let parar = AtomicBool::new(false);
        let caminho = self.dados.estado();
        let pulso = Arc::clone(&self.pulso);
        let (pid, batimento_s) = (self.estado.pid, self.batimento_s);
        std::thread::scope(|s| {
            s.spawn(|| batimento(&caminho, pid, &pulso, batimento_s, &parar));
            while !cancelar.load(Ordering::Relaxed) {
                self.passo(agora(), Duration::from_secs(1), cancelar);
            }
            parar.store(true, Ordering::Relaxed);
        });
        self.encerrar();
    }

    /// Fecha as varreduras do vigia e grava o estado "parado".
    pub fn encerrar(&mut self) {
        let ids: Vec<i64> = self.varreduras.values().copied().collect();
        if let Ok(conn) = self.banco() {
            for id in ids {
                let _ = conn.execute(
                    "UPDATE varredura SET status = 'concluida',
                       concluida_em = strftime('%Y-%m-%dT%H:%M:%fZ', 'now')
                     WHERE id = ?1 AND status = 'rodando'",
                    [id],
                );
            }
        }
        let _ = self.registro.registrar(&Evento::Parou);
        // Um estado antigo (atualizado_em no passado) faz a janela mostrar "parado".
        self.estado.atualizado_em = 0;
        let _ = crate::estado::escrever_atomico(
            &self.dados.estado(),
            &serde_json::to_vec_pretty(&self.estado).unwrap_or_default(),
        );
    }
}

/// Renova o `atualizado_em` do estado quando o laço não o escreve há `cada` segundos. Só mexe
/// em arquivo do próprio processo (`pid`) e só quando o laço está parado, então não disputa
/// com a escrita dele.
fn batimento(caminho: &Path, pid: u32, pulso: &AtomicU64, cada: u64, parar: &AtomicBool) {
    while !parar.load(Ordering::Relaxed) {
        std::thread::sleep(Duration::from_millis(100));
        let agora_ = agora();
        if agora_.saturating_sub(pulso.load(Ordering::Relaxed)) < cada {
            continue;
        }
        if let Some(mut e) = EstadoDoWorker::ler(caminho)
            && e.pid == pid
            && !parar.load(Ordering::Relaxed)
        {
            e.atualizado_em = agora_;
            if let Ok(b) = serde_json::to_vec_pretty(&e)
                && crate::estado::escrever_atomico(caminho, &b).is_ok()
            {
                pulso.store(agora_, Ordering::Relaxed);
            }
        }
    }
}

/// Recarrega a configuração de [`Dados::config`] (o worker lê de novo ao subir; a janela pode
/// pedir reinício depois de mudar).
///
/// # Errors
/// [`crate::Erro::Configuracao`] se o arquivo existe e é inválido.
pub fn carregar_config(dados: &Dados) -> Resultado<Config> {
    Config::carregar(&dados.config())
}
