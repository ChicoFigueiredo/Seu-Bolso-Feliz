//! A estação real do livro-caixa ligada à janela (spec 07, fatia 2): `EstacaoLocal`
//! (`sbf-estacao`) compartilhada entre a janela (que a usa pelos traits de alteração e de
//! sincronização) e o quadro (que a "tica" e relê a cópia quando uma troca termina).
//!
//! - `SBF_SERVIDOR` (padrão `http://127.0.0.1:8787`) e `SBF_GOOGLE_CLIENT_ID` vêm do
//!   ambiente; a credencial mora só no chaveiro.
//! - Sem servidor ou sem pareamento a janela abre do mesmo jeito: a área Sincronização mostra
//!   o motivo e o botão "Parear esta estação".

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::Duration;

use bancada_rede::{ProvedorOAuth, Relogio, RelogioDoSistema};
use sbf_estacao::{
    ConfiguracaoDaEstacao, ConsultaDoLivro, EstacaoLocal, GuardaDeCredencial, Servidor,
    garantir_fora_do_cofre, pasta_de_dados_padrao, sistema_atual,
};
use sbf_inventario::Consulta;
use sbf_inventario::alteracao::{
    Alteracao, Alteracoes, Estacao, EstadoDaSincronizacao, OpcaoDeCategoria, PedidoDeAlteracao,
    ResultadoDeAlteracao, Sincronizacao, Troca,
};
use sbf_inventario::consulta::{
    Area, Contagens, Filtro, Fonte, Item, Painel, PainelDeArea, PedidoDeTabela, Pergunta, Rastro,
    ResultadoDeConsulta, Tabela, TotalDoCiclo,
};
use sbf_inventario::referencia::Ref;
use sbf_janela::{AndamentoDoPareamento, ConsultaComposta, Janela, Pareamento};

/// O servidor quando `SBF_SERVIDOR` não diz outro.
pub const SERVIDOR_PADRAO: &str = "http://127.0.0.1:8787";
/// Quanto o login no navegador pode demorar.
const PRAZO_DO_LOGIN: Duration = Duration::from_secs(5 * 60);

fn travar<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// O servidor e a rota: `https://<autoridade>` é o nome oficial; um `http://` de loopback
/// vira rota para esse nome (o portão só aceita HTTPS ou loopback).
fn servidor_de(base: &str) -> Result<(Servidor, Option<(String, String)>), String> {
    let base = base.trim().trim_end_matches('/');
    if let Some(autoridade) = base.strip_prefix("http://") {
        let oficial =
            Servidor::novo(&format!("https://{autoridade}")).map_err(|e| e.to_string())?;
        return Ok((oficial, Some((autoridade.to_owned(), base.to_owned()))));
    }
    Ok((Servidor::novo(base).map_err(|e| e.to_string())?, None))
}

/// A configuração da estação deste app.
fn configuracao(
    pasta_de_dados: &Path,
    guarda: Arc<dyn GuardaDeCredencial>,
    servidor: &str,
    google: Option<&str>,
) -> Result<ConfiguracaoDaEstacao, String> {
    let (oficial, rota) = servidor_de(servidor)?;
    let mut config = ConfiguracaoDaEstacao::nova(oficial, pasta_de_dados, guarda)
        .versao_das_regras(env!("CARGO_PKG_VERSION"));
    if let Some((autoridade, base)) = rota {
        config = config.rota(&autoridade, &base);
    }
    if let Some(id) = google.filter(|g| !g.trim().is_empty()) {
        config = config.google(ProvedorOAuth::google(id.trim()));
    }
    Ok(config)
}

/// A leitura do livro-caixa que a janela segura e o quadro troca por baixo.
#[derive(Clone, Default)]
struct ConsultaViva(Arc<Mutex<Option<ConsultaDoLivro>>>);

impl ConsultaViva {
    fn com<T>(
        &self,
        f: impl FnOnce(&ConsultaDoLivro) -> ResultadoDeConsulta<T>,
    ) -> ResultadoDeConsulta<T> {
        match travar(&self.0).as_ref() {
            Some(c) => f(c),
            None => Err(sbf_inventario::consulta::ErroDeConsulta::NaoSuportado(
                sbf_janela::composta::SEM_LIVRO.to_owned(),
            )),
        }
    }
}

impl Consulta for ConsultaViva {
    fn tabela(&self, pedido: &PedidoDeTabela) -> ResultadoDeConsulta<Tabela> {
        self.com(|c| c.tabela(pedido))
    }
    fn item(&self, alvo: &Ref) -> ResultadoDeConsulta<Option<Item>> {
        self.com(|c| c.item(alvo))
    }
    fn rastro(&self, alvo: &Ref) -> ResultadoDeConsulta<Rastro> {
        self.com(|c| c.rastro(alvo))
    }
    fn por_ciclo(
        &self,
        alvo: &Ref,
        fonte: Fonte,
        ciclos: usize,
    ) -> ResultadoDeConsulta<Vec<TotalDoCiclo>> {
        self.com(|c| c.por_ciclo(alvo, fonte, ciclos))
    }
    fn visao_geral(&self) -> ResultadoDeConsulta<Painel> {
        self.com(|c| c.visao_geral())
    }
    fn relatorio(
        &self,
        pergunta: Pergunta,
        fonte: Fonte,
        filtro: &Filtro,
    ) -> ResultadoDeConsulta<Painel> {
        self.com(|c| c.relatorio(pergunta, fonte, filtro))
    }
    fn contagens(&self) -> ResultadoDeConsulta<Contagens> {
        self.com(|c| c.contagens())
    }
    fn painel_da_area(&self, area: Area, filtro: &Filtro) -> ResultadoDeConsulta<PainelDeArea> {
        self.com(|c| c.painel_da_area(area, filtro))
    }
    fn texto(&self, achado: &Ref) -> ResultadoDeConsulta<Option<String>> {
        self.com(|c| c.texto(achado))
    }
}

/// A estação compartilhada: o que a janela usa, por trás de uma trava.
#[derive(Clone)]
struct EstacaoCompartilhada {
    estacao: Arc<Mutex<EstacaoLocal>>,
    /// Sobe a cada escrita local (propor, refazer, descartar): a cópia lida muda.
    escritas: Arc<AtomicU64>,
}

impl Alteracoes for EstacaoCompartilhada {
    fn propor(&mut self, pedido: &PedidoDeAlteracao) -> ResultadoDeAlteracao<Alteracao> {
        self.escritas.fetch_add(1, Ordering::AcqRel);
        travar(&self.estacao).propor(pedido)
    }
    fn alteracoes(&self, so_em_aberto: bool) -> ResultadoDeAlteracao<Vec<Alteracao>> {
        travar(&self.estacao).alteracoes(so_em_aberto)
    }
    fn alteracao(&self, id: &str) -> ResultadoDeAlteracao<Option<Alteracao>> {
        travar(&self.estacao).alteracao(id)
    }
    fn refazer(&mut self, id: &str) -> ResultadoDeAlteracao<Alteracao> {
        self.escritas.fetch_add(1, Ordering::AcqRel);
        travar(&self.estacao).refazer(id)
    }
    fn descartar(&mut self, id: &str) -> ResultadoDeAlteracao<()> {
        self.escritas.fetch_add(1, Ordering::AcqRel);
        travar(&self.estacao).descartar(id)
    }
    fn categorias(&self) -> ResultadoDeAlteracao<Vec<OpcaoDeCategoria>> {
        travar(&self.estacao).categorias()
    }
}

impl Sincronizacao for EstacaoCompartilhada {
    fn estado(&self) -> EstadoDaSincronizacao {
        travar(&self.estacao).estado()
    }
    fn sincronizar_agora(&mut self) {
        travar(&self.estacao).sincronizar_agora();
    }
    fn estacoes(&self) -> ResultadoDeAlteracao<Vec<Estacao>> {
        travar(&self.estacao).estacoes()
    }
    fn revogar(&mut self, estacao: &Ref) -> ResultadoDeAlteracao<()> {
        travar(&self.estacao).revogar(estacao)
    }
    fn renomear(&mut self, estacao: &Ref, nome: &str) -> ResultadoDeAlteracao<()> {
        travar(&self.estacao).renomear(estacao, nome)
    }
    fn historico(&self) -> ResultadoDeAlteracao<Vec<Troca>> {
        travar(&self.estacao).historico()
    }
}

/// O pareamento por thread: login Google no navegador, `POST /estacoes/pareamento`, e
/// então a primeira sincronização.
#[derive(Debug, Clone)]
struct PareamentoPorThread {
    estacao: Arc<Mutex<EstacaoLocal>>,
    andamento: Arc<Mutex<AndamentoDoPareamento>>,
}

impl std::fmt::Debug for EstacaoCompartilhada {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EstacaoCompartilhada")
            .finish_non_exhaustive()
    }
}

impl Pareamento for PareamentoPorThread {
    fn iniciar(&mut self) {
        {
            let mut a = travar(&self.andamento);
            if matches!(*a, AndamentoDoPareamento::Pareando(_)) {
                return;
            }
            *a = AndamentoDoPareamento::Pareando("abrindo o login Google…".to_owned());
        }
        let estacao = self.estacao.clone();
        let andamento = self.andamento.clone();
        let falhou = andamento.clone();
        let iniciada = thread::Builder::new()
            .name("sbf-pareamento".to_owned())
            .spawn(move || {
                let resultado = parear(&estacao, &andamento);
                *travar(&andamento) = match resultado {
                    Ok(()) => AndamentoDoPareamento::Parado,
                    Err(motivo) => AndamentoDoPareamento::Falhou(motivo),
                };
            });
        if iniciada.is_err() {
            *travar(&falhou) =
                AndamentoDoPareamento::Falhou("não foi possível iniciar o pareamento".to_owned());
        }
    }

    fn andamento(&self) -> AndamentoDoPareamento {
        travar(&self.andamento).clone()
    }
}

/// O passo a passo do pareamento, fora do quadro. Nenhuma mensagem carrega token.
fn parear(
    estacao: &Mutex<EstacaoLocal>,
    andamento: &Mutex<AndamentoDoPareamento>,
) -> Result<(), String> {
    let passo =
        |texto: &str| *travar(andamento) = AndamentoDoPareamento::Pareando(texto.to_owned());
    let pareador = travar(estacao).pareador();
    let fluxo = pareador.iniciar_login().map_err(|e| match e {
        sbf_estacao::ErroDePareamento::SemGoogle => {
            "o login Google não está configurado: defina SBF_GOOGLE_CLIENT_ID e abra de novo"
                .to_owned()
        }
        outro => outro.to_string(),
    })?;
    if !fluxo.abrir_no_navegador() {
        return Err("o navegador não abriu para o login Google".to_owned());
    }
    passo("esperando o login no navegador…");
    let prazo = RelogioDoSistema.agora().mais(PRAZO_DO_LOGIN);
    let tokens = pareador
        .concluir_login(fluxo, prazo, &AtomicBool::new(false))
        .map_err(|e| e.to_string())?;
    passo("pareando com o servidor…");
    let nome = std::env::var("HOSTNAME")
        .or_else(|_| std::env::var("COMPUTERNAME"))
        .unwrap_or_else(|_| "Este computador".to_owned());
    pareador
        .parear(&tokens, &nome, sistema_atual())
        .map_err(|e| e.to_string())?;
    travar(estacao).apos_parear();
    Ok(())
}

/// `sbf estacao importar-credencial [--servidor <url>]`: lê a credencial da entrada padrão
/// (nunca de argv, que aparece em `ps`) e a guarda no chaveiro, onde o pareamento normal a
/// guardaria. A estação conta como pareada na próxima abertura (a credencial no chaveiro é
/// o que `pareada()` confere).
pub fn importar_credencial_cli(args: &[String]) -> std::process::ExitCode {
    use std::io::Read;
    let falha = |m: &str| {
        eprintln!(
            "sbf estacao: {m}\nuso: sbf estacao importar-credencial [--servidor <url>] < credencial"
        );
        std::process::ExitCode::from(2)
    };
    let base = match args {
        [c] if c == "importar-credencial" => None,
        [c, f, v] if c == "importar-credencial" && f == "--servidor" => Some(v.clone()),
        _ => return falha("argumentos inválidos"),
    };
    let mut texto = zeroize::Zeroizing::new(String::new());
    if std::io::stdin()
        .take(4096)
        .read_to_string(&mut texto)
        .is_err()
    {
        return falha("não deu para ler a credencial da entrada padrão");
    }
    let chaveiro = match sbf_janela::ferramentas::abrir_chaveiro_com_espera(Duration::from_secs(3))
    {
        Ok(c) => c,
        Err(e) => {
            eprintln!("sbf estacao: {e}");
            return std::process::ExitCode::FAILURE;
        }
    };
    match importar(&chaveiro, base, &texto) {
        Ok(msg) => {
            println!("{msg}");
            std::process::ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("sbf estacao: {e}");
            std::process::ExitCode::FAILURE
        }
    }
}

/// O miolo testável: valida, guarda e devolve a mensagem de sucesso.
fn importar(
    guarda: &dyn GuardaDeCredencial,
    servidor: Option<String>,
    texto: &str,
) -> Result<String, String> {
    let base = servidor
        .or_else(|| std::env::var("SBF_SERVIDOR").ok())
        .unwrap_or_else(|| SERVIDOR_PADRAO.to_owned());
    let (oficial, _) = servidor_de(&base)?;
    sbf_estacao::importar_credencial(guarda, oficial.host(), texto).map_err(|e| e.to_string())?;
    Ok(format!(
        "estação pareada com {}",
        base.trim().trim_end_matches('/')
    ))
}

/// O que o quadro faz com a estação: "ticar" e reler a cópia.
pub struct Ligacao {
    estacao: Arc<Mutex<EstacaoLocal>>,
    livro: ConsultaViva,
    escritas: Arc<AtomicU64>,
    visto: Option<(u64, u64)>,
}

impl Ligacao {
    /// A cada quadro: `tique()` da estação e, se uma troca terminou (ou houve escrita
    /// local), relê a cópia. Devolve `true` quando o livro mudou (a janela esquece o cache).
    pub fn tique(&mut self) -> bool {
        let mut e = travar(&self.estacao);
        e.tique();
        let agora = (e.trocas_terminadas(), self.escritas.load(Ordering::Acquire));
        if self.visto == Some(agora) {
            return false;
        }
        self.visto = Some(agora);
        let lido = e.cache().and_then(|c| match ConsultaDoLivro::do_cache(c) {
            Ok(c) => Some(c),
            Err(erro) => {
                eprintln!("sbf: a cópia do livro-caixa não pôde ser lida: {erro}");
                None
            }
        });
        drop(e);
        *travar(&self.livro.0) = lido;
        true
    }

    /// Há troca em andamento (o quadro pede repintura para vê-la terminar).
    pub fn sincronizando(&self) -> bool {
        travar(&self.estacao).sincronizando()
    }
}

/// Abre a estação do ambiente e a liga à `janela`. Devolve a leitura composta (inventário +
/// livro-caixa) e a ligação que o quadro tica. Se a estação não abre, a janela abre sem o
/// livro-caixa e mostra o motivo.
pub fn ligar(
    janela: &mut Janela,
    inventario: Box<dyn Consulta>,
    cofre: Option<&Path>,
) -> (Box<dyn Consulta>, Option<Ligacao>) {
    match abrir_do_ambiente(cofre) {
        Ok(estacao) => {
            let estacao = Arc::new(Mutex::new(estacao));
            let escritas = Arc::new(AtomicU64::new(0));
            let livro = ConsultaViva::default();
            janela.ligar_estacao(Box::new(EstacaoCompartilhada {
                estacao: estacao.clone(),
                escritas: escritas.clone(),
            }));
            janela.ligar_pareamento(Box::new(PareamentoPorThread {
                estacao: estacao.clone(),
                andamento: Arc::new(Mutex::new(AndamentoDoPareamento::Parado)),
            }));
            let composta = ConsultaComposta::nova(inventario, Some(Box::new(livro.clone())));
            (
                Box::new(composta),
                Some(Ligacao {
                    estacao,
                    livro,
                    escritas,
                    visto: None,
                }),
            )
        }
        Err(motivo) => {
            if janela.mensagem.is_none() {
                janela.mensagem = Some(sbf_janela::janela::Mensagem {
                    texto: format!("Livro-caixa indisponível: {motivo}"),
                    erro: true,
                });
            }
            (Box::new(ConsultaComposta::nova(inventario, None)), None)
        }
    }
}

fn abrir_do_ambiente(cofre: Option<&Path>) -> Result<EstacaoLocal, String> {
    let servidor = std::env::var("SBF_SERVIDOR").unwrap_or_else(|_| SERVIDOR_PADRAO.to_owned());
    let google = std::env::var("SBF_GOOGLE_CLIENT_ID").ok();
    let pasta = pasta_de_dados_padrao().map_err(|e| e.to_string())?;
    if let Some(cofre) = cofre {
        garantir_fora_do_cofre(&pasta, cofre).map_err(|e| e.to_string())?;
    }
    let chaveiro = sbf_janela::ferramentas::abrir_chaveiro_com_espera(Duration::from_secs(3))?;
    let config = configuracao(&pasta, Arc::new(chaveiro), &servidor, google.as_deref())?;
    EstacaoLocal::abrir(config).map_err(|e| e.to_string())
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_estacao::GuardaEmMemoria;

    const CRED: &str =
        "sbfe_0123456789abcdef0123456789abcdef_AbCdEfGhIjKlMnOpQrStUvWxYz0123456789-_AbCde";

    #[test]
    fn importar_guarda_onde_o_pareamento_guarda_e_so_diz_o_servidor() {
        let guarda = GuardaEmMemoria::default();
        let msg = importar(&guarda, Some("http://127.0.0.1:8788".into()), CRED).unwrap();
        assert_eq!(msg, "estação pareada com http://127.0.0.1:8788");
        assert!(!msg.contains("sbfe_"));
        assert!(guarda.tem("127.0.0.1:8788"));
    }

    #[test]
    fn importar_recusa_credencial_invalida() {
        let guarda = GuardaEmMemoria::default();
        let e = importar(
            &guarda,
            Some("http://127.0.0.1:8788".into()),
            "lixo-secreto",
        )
        .unwrap_err();
        assert!(!e.contains("lixo-secreto"));
        assert!(!guarda.tem("127.0.0.1:8788"));
    }
    use sbf_inventario::alteracao::EstadoDaRede;

    #[test]
    fn http_de_loopback_vira_rota_do_nome_oficial() {
        let (s, rota) = servidor_de("http://127.0.0.1:8787/").unwrap();
        assert_eq!(s.host(), "127.0.0.1:8787");
        assert_eq!(
            rota,
            Some((
                "127.0.0.1:8787".to_owned(),
                "http://127.0.0.1:8787".to_owned()
            ))
        );
        let (s, rota) = servidor_de("https://sbf.exemplo.com.br").unwrap();
        assert_eq!(s.host(), "sbf.exemplo.com.br");
        assert!(rota.is_none());
        assert!(servidor_de("ftp://x").is_err());
    }

    #[test]
    fn sem_servidor_a_estacao_abre_nao_pareada_e_a_janela_continua() {
        let pasta = tempfile::tempdir().unwrap();
        let config = configuracao(
            pasta.path(),
            Arc::new(GuardaEmMemoria::default()),
            SERVIDOR_PADRAO,
            Some("cliente-ficticio.apps.googleusercontent.com"),
        )
        .unwrap();
        let estacao = EstacaoLocal::abrir(config).unwrap();
        assert_eq!(estacao.estado().rede, EstadoDaRede::NaoPareada);
        let estacao = Arc::new(Mutex::new(estacao));
        let mut ligacao = Ligacao {
            estacao,
            livro: ConsultaViva::default(),
            escritas: Arc::new(AtomicU64::new(0)),
            visto: None,
        };
        // O primeiro quadro lê (nada); o segundo não muda nada.
        assert!(ligacao.tique());
        assert!(!ligacao.tique());
        let motivo = ligacao.livro.painel_da_area(Area::Hoje, &Filtro::default());
        assert!(motivo.unwrap_err().to_string().contains("Sincronização"));
    }
}
