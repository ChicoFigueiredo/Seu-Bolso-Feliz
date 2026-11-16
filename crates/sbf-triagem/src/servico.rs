//! O que a janela usa da triagem: um trait pequeno para a rodada
//! ([`RodadaDeTriagem`]: iniciar, pausar, retomar, progresso) e o serviço completo
//! ([`ServicoDeTriagem`]), com duas implementações:
//!
//! - [`TriagemFicticia`]: dados inventados, para o modo demonstração e os testes da janela;
//! - [`TriagemDoInventario`]: sobre o `inventario.sqlite` e a `Fila` do motor (`sbf-ia`),
//!   montada pelo coordenador com o provedor principal, a reserva e o `RegistroDaIa`.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use sbf_ia::{Controle, Degrau, Fila, NivelDeConfianca, Pedido, Provedor};
use sbf_inventario::comando::{Comandos, IdDecisao};
use sbf_inventario::sugestao::RegistroDaIa;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::aceite::{self, ResultadoDoAceite};
use crate::fila::{OpcoesDaFila, selecionar};
use crate::piloto::{self, FAIXA_ALTA, FAIXA_BAIXA, FAIXA_MEDIA, Placar, RelatorioDoPiloto};
use crate::rodada::{ConfigDaRodada, Progresso, SituacaoDaRodada, rodar};
use crate::sugestoes::{
    self, FiltroDeSugestoes, GrupoDeSugestoes, SugestaoListada, agrupar, listar_abertas,
};
use crate::tipos::Alvo;

/// O provedor que a pessoa escolhe em Opções → IA.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvedorEscolhido {
    /// A assinatura Claude Max, pelo Claude Code em modo headless (D1).
    ClaudeCli,
    OpenRouter,
}

impl ProvedorEscolhido {
    pub fn rotulo(self) -> &'static str {
        match self {
            Self::ClaudeCli => "Claude Code (assinatura)",
            Self::OpenRouter => "OpenRouter",
        }
    }
}

/// Opções → IA.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConfiguracaoDeIa {
    pub principal: ProvedorEscolhido,
    /// A reserva: só entra com ela ligada e abaixo do teto (IA-06).
    pub reserva: Option<ProvedorEscolhido>,
    /// Centavos de dólar por rodada. Vale para o OpenRouter.
    pub teto_da_rodada_centavos_usd: u32,
    /// Centavos de dólar por mês.
    pub teto_do_mes_centavos_usd: u32,
    /// O nível do destinatário "triagem" (D2: completo).
    pub nivel: NivelDeConfianca,
}

impl Default for ConfiguracaoDeIa {
    fn default() -> Self {
        Self {
            principal: ProvedorEscolhido::ClaudeCli,
            reserva: None,
            teto_da_rodada_centavos_usd: 500,
            teto_do_mes_centavos_usd: 2000,
            nivel: NivelDeConfianca::Completo,
        }
    }
}

/// O que terminou fora do quadro (a janela lê com [`ServicoDeTriagem::sondar`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Conclusao {
    /// "Testar conexão": o texto a mostrar ou o motivo.
    Teste(Result<String, String>),
    /// "Salvar": gravou ou não.
    Configuracao(Result<(), String>),
}

/// Onde as threads de "Testar conexão" e "Salvar" deixam o resultado. Barato de clonar.
#[derive(Debug, Clone, Default)]
pub struct CaixaDeConclusoes {
    prontas: Arc<Mutex<Vec<Conclusao>>>,
    em_andamento: Arc<std::sync::atomic::AtomicUsize>,
}

impl CaixaDeConclusoes {
    fn travar(&self) -> std::sync::MutexGuard<'_, Vec<Conclusao>> {
        self.prontas
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    /// Roda `trabalho` numa thread e guarda o que ele devolve. Se a thread não nasce, a
    /// conclusão é o erro.
    fn lancar(
        &self,
        nome: &str,
        trabalho: impl FnOnce() -> Conclusao + Send + 'static,
        falha: fn(String) -> Conclusao,
    ) {
        use std::sync::atomic::Ordering;
        self.em_andamento.fetch_add(1, Ordering::AcqRel);
        let caixa = self.clone();
        let iniciada = std::thread::Builder::new()
            .name(nome.to_owned())
            .spawn(move || {
                // Um pânico do trabalho não pode deixar a janela esperando para sempre.
                let c = std::panic::catch_unwind(std::panic::AssertUnwindSafe(trabalho))
                    .unwrap_or_else(|_| falha("a tarefa parou de repente".to_owned()));
                caixa.travar().push(c);
                caixa.em_andamento.fetch_sub(1, Ordering::AcqRel);
            });
        if iniciada.is_err() {
            self.em_andamento.fetch_sub(1, Ordering::AcqRel);
            self.travar()
                .push(falha("não foi possível iniciar a tarefa".to_owned()));
        }
    }

    /// Guarda uma conclusão que já está pronta (a fictícia).
    fn entregar(&self, c: Conclusao) {
        self.travar().push(c);
    }

    fn tomar(&self) -> Vec<Conclusao> {
        std::mem::take(&mut *self.travar())
    }

    fn ocupada(&self) -> bool {
        self.em_andamento.load(std::sync::atomic::Ordering::Acquire) > 0
    }
}

/// O trait pequeno da rodada.
pub trait RodadaDeTriagem {
    /// Começa uma rodada sobre a fila (ou o piloto de 200 itens já decididos).
    ///
    /// # Errors
    /// Texto para a tela (já há rodada rodando, banco, provedor).
    fn iniciar(&mut self, piloto: bool) -> Result<(), String>;
    /// Pede a pausa: vale entre dois lotes.
    fn pausar(&mut self);
    /// Continua do que falta (a fila é recalculada).
    ///
    /// # Errors
    /// Texto para a tela.
    fn retomar(&mut self) -> Result<(), String>;
    fn progresso(&self) -> Progresso;
}

/// O que a área "Triagem por IA" e Opções → IA usam.
pub trait ServicoDeTriagem: RodadaDeTriagem {
    /// As sugestões abertas agrupadas por campo e valor.
    ///
    /// # Errors
    /// Texto para a tela.
    fn grupos(&self, filtro: &FiltroDeSugestoes) -> Result<Vec<GrupoDeSugestoes>, String>;
    /// Aceita: uma decisão do usuário por item, pelo `Comandos`.
    ///
    /// # Errors
    /// Texto para a tela.
    fn aceitar(
        &mut self,
        ids: &[i64],
        comandos: &mut dyn Comandos,
    ) -> Result<ResultadoDoAceite, String>;
    /// Recusa: nenhuma decisão é gravada.
    ///
    /// # Errors
    /// Texto para a tela.
    fn recusar(&mut self, ids: &[i64]) -> Result<usize, String>;
    /// O relatório do último piloto, se houve.
    fn relatorio_do_piloto(&self) -> Option<RelatorioDoPiloto>;
    fn configuracao(&self) -> ConfiguracaoDeIa;
    /// # Errors
    /// Texto para a tela.
    fn salvar_configuracao(&mut self, configuracao: ConfiguracaoDeIa) -> Result<(), String>;
    /// Uma chamada mínima para ver se o provedor responde. Devolve o que mostrar.
    ///
    /// # Errors
    /// Texto para a tela.
    fn testar_conexao(&mut self) -> Result<String, String>;

    /// Como [`Self::testar_conexao`], mas **sem bloquear**: o resultado chega por
    /// [`Self::sondar`] como [`Conclusao::Teste`]. A janela só usa esta.
    fn iniciar_teste_de_conexao(&mut self);
    /// Como [`Self::salvar_configuracao`], sem bloquear: a configuração já vale na memória e o
    /// resultado da gravação chega como [`Conclusao::Configuracao`].
    fn iniciar_salvamento(&mut self, configuracao: ConfiguracaoDeIa);
    /// O que terminou desde a última chamada. Não bloqueia.
    fn sondar(&mut self) -> Vec<Conclusao>;
    /// Há teste ou gravação em andamento (a janela pede repintura).
    fn ocupado(&self) -> bool;

    /// O painel do agente contínuo (chegaram, sugeridas, regras aprendidas, acerto da
    /// semana). `None`: o agente não está ligado nesta máquina.
    fn painel_do_agente(&self) -> Option<crate::agente::Painel> {
        None
    }

    /// Apaga uma regra aprendida (ela só volta com 3 confirmações novas).
    ///
    /// # Errors
    /// Texto para a tela.
    fn apagar_regra_do_agente(&mut self, _id: i64) -> Result<(), String> {
        Err("o agente de triagem não está ligado".to_owned())
    }
}

// ---------------------------------------------------------------------------------------
// Fictícia

/// Dados inventados e uma rodada que anda com o relógio. Nada vai a rede nem ao banco.
#[derive(Debug)]
pub struct TriagemFicticia {
    inicio: Option<Instant>,
    deslocamento: usize,
    pausada: bool,
    total: usize,
    piloto: Option<RelatorioDoPiloto>,
    grupos: Vec<GrupoDeSugestoes>,
    configuracao: ConfiguracaoDeIa,
    proxima_decisao: IdDecisao,
    conclusoes: CaixaDeConclusoes,
    /// Itens por segundo da rodada fictícia.
    pub ritmo: usize,
    /// As regras fictícias que o usuário apagou nesta sessão.
    regras_apagadas: Vec<i64>,
}

impl Default for TriagemFicticia {
    fn default() -> Self {
        Self::nova()
    }
}

fn sugestao_ficticia(
    id: i64,
    alvo: Alvo,
    titulo: &str,
    campo: &str,
    valor: Value,
    conf: f64,
) -> SugestaoListada {
    SugestaoListada {
        id,
        alvo,
        titulo: titulo.to_owned(),
        campo: campo.to_owned(),
        valor,
        confianca: conf,
        justificativa: "o texto cita o fornecedor e o valor bate com o histórico".to_owned(),
        modelo: "falso".to_owned(),
    }
}

impl TriagemFicticia {
    pub fn nova() -> Self {
        let mut id = 0;
        let mut grupo = |campo: &str, valor: Value, n: usize, base: f64, nomes: &[&str]| {
            let sugestoes = (0..n)
                .map(|i| {
                    id += 1;
                    let titulo = format!("{} #{}", nomes[i % nomes.len()], 100 + i);
                    sugestao_ficticia(
                        id,
                        Alvo::Registro(1000 + id),
                        &titulo,
                        campo,
                        valor.clone(),
                        (base - (i % 7) as f64 * 0.02).max(0.5),
                    )
                })
                .collect::<Vec<_>>();
            GrupoDeSugestoes {
                alvo_tipo: "registro",
                campo: campo.to_owned(),
                valor_texto: sugestoes::texto_do_valor(campo, &valor),
                valor,
                sugestoes,
            }
        };
        let grupos = vec![
            grupo(
                "categoria",
                json!("Mercado"),
                142,
                0.96,
                &["MERCADO DELTA", "SUPERMERCADO GAMA"],
            ),
            grupo(
                "categoria",
                json!("Energia"),
                12,
                0.98,
                &["ENERGIA EXEMPLO"],
            ),
            grupo(
                "fornecedor",
                json!("Padaria Beta"),
                31,
                0.88,
                &["PIX PADARIA BETA"],
            ),
            grupo(
                "categoria",
                json!("Saúde"),
                9,
                0.64,
                &["FARMACIA EPSILON", "CLINICA ZETA"],
            ),
        ];
        let mut f = Self {
            inicio: None,
            deslocamento: 0,
            pausada: false,
            total: 200,
            piloto: None,
            grupos,
            configuracao: ConfiguracaoDeIa::default(),
            proxima_decisao: 9000,
            conclusoes: CaixaDeConclusoes::default(),
            ritmo: 20,
            regras_apagadas: Vec::new(),
        };
        f.piloto = Some(f.relatorio_ficticio());
        f
    }

    fn relatorio_ficticio(&self) -> RelatorioDoPiloto {
        let placar = |c, a| Placar {
            comparadas: c,
            acertos: a,
        };
        RelatorioDoPiloto {
            rodada: sugestoes::Rodada {
                id: 1,
                iniciada_em: "2026-11-14T09:00:00.000Z".into(),
                terminada_em: Some("2026-11-14T09:04:00.000Z".into()),
                itens: 200,
                chamadas: 11,
                custo_nominal_centavos_usd: 38,
                provedor: "falso".into(),
                situacao: "concluida".into(),
                piloto: true,
            },
            itens: 200,
            nao_classificados: 3,
            sem_gabarito: 41,
            geral: placar(310, 271),
            por_campo: [
                ("categoria".to_owned(), placar(180, 160)),
                ("fornecedor".to_owned(), placar(130, 111)),
            ]
            .into_iter()
            .collect(),
            por_faixa: [
                (FAIXA_BAIXA, placar(40, 22)),
                (FAIXA_MEDIA, placar(110, 93)),
                (FAIXA_ALTA, placar(160, 156)),
            ]
            .into_iter()
            .collect(),
            tokens_entrada: 210_000,
            tokens_saida: 34_000,
            custo_dezmilesimos_usd: 3_800,
            tokens_por_item: Some(1220),
        }
    }

    /// Avança a rodada fictícia `itens` itens (para os testes, sem relógio).
    pub fn avancar(&mut self, itens: usize) {
        self.deslocamento += itens;
    }

    fn feitos(&self) -> usize {
        let por_relogio = if self.pausada {
            0
        } else {
            self.inicio.map_or(0, |i| {
                (i.elapsed().as_secs_f32() * self.ritmo as f32) as usize
            })
        };
        (self.deslocamento + por_relogio).min(self.total)
    }
}

impl RodadaDeTriagem for TriagemFicticia {
    fn iniciar(&mut self, piloto: bool) -> Result<(), String> {
        self.total = if piloto { 200 } else { 10_000 };
        self.deslocamento = 0;
        self.pausada = false;
        self.inicio = Some(Instant::now());
        Ok(())
    }

    fn pausar(&mut self) {
        if !self.pausada {
            self.deslocamento = self.feitos();
            self.pausada = true;
        }
    }

    fn retomar(&mut self) -> Result<(), String> {
        if self.pausada {
            self.pausada = false;
            self.inicio = Some(Instant::now());
        }
        Ok(())
    }

    fn progresso(&self) -> Progresso {
        let feitos = self.feitos();
        let situacao = match (self.inicio, self.pausada) {
            (None, _) => SituacaoDaRodada::Ociosa,
            (Some(_), true) => SituacaoDaRodada::Pausada,
            (Some(_), false) if feitos >= self.total => SituacaoDaRodada::Concluida,
            _ => SituacaoDaRodada::Rodando,
        };
        Progresso {
            situacao,
            rodada_id: self.inicio.map(|_| 1),
            total: self.total,
            processados: feitos,
            chamadas: u32::try_from(feitos / 25).unwrap_or(u32::MAX),
            sugestoes: feitos * 3,
            nao_classificados: feitos / 80,
            subiram_de_degrau: feitos / 9,
            tokens_entrada: feitos as u64 * 1000,
            tokens_saida: feitos as u64 * 170,
            custo_dezmilesimos_usd: feitos as u64 * 19,
            aviso: None,
        }
    }
}

impl ServicoDeTriagem for TriagemFicticia {
    fn grupos(&self, filtro: &FiltroDeSugestoes) -> Result<Vec<GrupoDeSugestoes>, String> {
        Ok(self
            .grupos
            .iter()
            .filter(|g| filtro.campo.as_ref().is_none_or(|c| *c == g.campo))
            .filter_map(|g| {
                let sugestoes: Vec<_> = g
                    .sugestoes
                    .iter()
                    .filter(|s| s.confianca >= filtro.confianca_minima.unwrap_or(0.0))
                    .cloned()
                    .collect();
                (!sugestoes.is_empty()).then(|| GrupoDeSugestoes {
                    sugestoes,
                    ..g.clone()
                })
            })
            .collect())
    }

    fn aceitar(
        &mut self,
        ids: &[i64],
        _comandos: &mut dyn Comandos,
    ) -> Result<ResultadoDoAceite, String> {
        let mut r = ResultadoDoAceite::default();
        for g in &mut self.grupos {
            g.sugestoes.retain(|s| {
                let aceita = ids.contains(&s.id);
                if aceita {
                    r.aceitas.push(s.id);
                    self.proxima_decisao += 1;
                    r.decisoes.push(self.proxima_decisao);
                }
                !aceita
            });
        }
        self.grupos.retain(|g| !g.sugestoes.is_empty());
        Ok(r)
    }

    fn recusar(&mut self, ids: &[i64]) -> Result<usize, String> {
        let antes: usize = self.grupos.iter().map(|g| g.sugestoes.len()).sum();
        for g in &mut self.grupos {
            g.sugestoes.retain(|s| !ids.contains(&s.id));
        }
        self.grupos.retain(|g| !g.sugestoes.is_empty());
        let depois: usize = self.grupos.iter().map(|g| g.sugestoes.len()).sum();
        Ok(antes - depois)
    }

    fn relatorio_do_piloto(&self) -> Option<RelatorioDoPiloto> {
        self.piloto.clone()
    }

    fn configuracao(&self) -> ConfiguracaoDeIa {
        self.configuracao.clone()
    }

    fn salvar_configuracao(&mut self, configuracao: ConfiguracaoDeIa) -> Result<(), String> {
        self.configuracao = configuracao;
        Ok(())
    }

    fn testar_conexao(&mut self) -> Result<String, String> {
        Ok("conexão de demonstração: nenhum provedor foi chamado".to_owned())
    }

    fn iniciar_teste_de_conexao(&mut self) {
        let r = self.testar_conexao();
        self.conclusoes.entregar(Conclusao::Teste(r));
    }

    fn iniciar_salvamento(&mut self, configuracao: ConfiguracaoDeIa) {
        let r = self.salvar_configuracao(configuracao);
        self.conclusoes.entregar(Conclusao::Configuracao(r));
    }

    fn sondar(&mut self) -> Vec<Conclusao> {
        self.conclusoes.tomar()
    }

    fn ocupado(&self) -> bool {
        self.conclusoes.ocupada()
    }

    fn painel_do_agente(&self) -> Option<crate::agente::Painel> {
        use crate::agente::{Painel, RegraAprendida, TipoDeRegra};
        let regras = [
            (
                1,
                TipoDeRegra::Categoria,
                "mercado delta (compra)",
                "Mercado",
                5,
            ),
            (
                2,
                TipoDeRegra::Categoria,
                "energia exemplo (pagamento)",
                "Energia",
                4,
            ),
            (3, TipoDeRegra::TipoDocumento, "fatura luz #", "fatura", 3),
        ]
        .into_iter()
        .filter(|(id, ..)| !self.regras_apagadas.contains(id))
        .map(|(id, tipo, quando, valor, exemplos)| RegraAprendida {
            id,
            tipo,
            quando: quando.to_owned(),
            valor: valor.to_owned(),
            exemplos,
        })
        .collect();
        Some(Painel {
            chegaram: 42,
            sugeridas: 37,
            abertas: 29,
            prontas: 21,
            regras,
            aceitas_da_semana: 31,
            recusadas_da_semana: 4,
            na_fila: 12,
        })
    }

    fn apagar_regra_do_agente(&mut self, id: i64) -> Result<(), String> {
        self.regras_apagadas.push(id);
        Ok(())
    }
}

// ---------------------------------------------------------------------------------------
// Sobre o inventário

const CHAVE_DA_CONFIGURACAO: &str = "triagem.configuracao";

/// A configuração de Opções → IA gravada no banco (o padrão se nunca foi salva). Quem monta
/// a [`Fila`] precisa dela antes do serviço (a reserva só entra se estiver ligada).
pub fn configuracao_salva(caminho: impl AsRef<Path>) -> ConfiguracaoDeIa {
    sugestoes::abrir(caminho.as_ref())
        .ok()
        .and_then(|conn| ler_configuracao(&conn))
        .unwrap_or_default()
}

fn ler_configuracao(conn: &rusqlite::Connection) -> Option<ConfiguracaoDeIa> {
    conn.query_row(
        "SELECT valor FROM meta WHERE chave = ?1",
        [CHAVE_DA_CONFIGURACAO],
        |l| l.get::<_, String>(0),
    )
    .ok()
    .and_then(|t| serde_json::from_str(&t).ok())
}

fn pedido_de_teste() -> Pedido {
    Pedido {
        prompt_versao: "teste-de-conexao".to_owned(),
        instrucao: "Responda com o JSON {\"ok\": true}.".to_owned(),
        entrada: "teste".to_owned(),
        esquema: json!({
            "type": "object",
            "required": ["ok"],
            "properties": {"ok": {"type": "boolean"}}
        }),
        degrau: Degrau::Rapido,
        max_tokens_saida: 20,
    }
}

fn testar(provedor: &dyn Provedor) -> Result<String, String> {
    provedor
        .pedir(&pedido_de_teste())
        .map(|r| format!("{} respondeu ({})", provedor.nome(), r.modelo))
        .map_err(|e| e.to_string())
}

fn gravar_configuracao(caminho: &Path, configuracao: &ConfiguracaoDeIa) -> Result<(), String> {
    let conn = sugestoes::abrir(caminho).map_err(|e| e.to_string())?;
    let texto = serde_json::to_string(configuracao).map_err(|e| e.to_string())?;
    conn.execute(
        "INSERT OR REPLACE INTO meta (chave, valor) VALUES (?1, ?2)",
        rusqlite::params![CHAVE_DA_CONFIGURACAO, texto],
    )
    .map_err(|e| e.to_string())?;
    Ok(())
}

/// O serviço sobre o `inventario.sqlite` (já migrado) e um [`Provedor`]. A rodada roda numa
/// thread própria, com a sua conexão.
pub struct TriagemDoInventario {
    caminho: PathBuf,
    fila: Arc<Fila>,
    registro: Arc<RegistroDaIa>,
    provedor: Arc<dyn Provedor>,
    controle: Controle,
    progresso: Arc<Mutex<Progresso>>,
    relatorio: Arc<Mutex<Option<RelatorioDoPiloto>>>,
    thread: Option<JoinHandle<()>>,
    configuracao: ConfiguracaoDeIa,
    rastro: Option<crate::rodada::RastroDeFalhas>,
    conclusoes: CaixaDeConclusoes,
    /// O `agente.sqlite` do agente contínuo, se o painel dele deve aparecer.
    agente: Option<PathBuf>,
}

impl std::fmt::Debug for TriagemDoInventario {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TriagemDoInventario")
            .field("caminho", &self.caminho)
            .finish_non_exhaustive()
    }
}

impl TriagemDoInventario {
    /// `fila` e `registro` são os que o coordenador montou (o `registro` é o mesmo que a
    /// fila usa: a rodada o marca com o id dela); `provedor` serve ao teste de conexão.
    ///
    /// # Errors
    /// Falha ao abrir o banco.
    pub fn nova(
        caminho: impl AsRef<Path>,
        fila: Arc<Fila>,
        registro: Arc<RegistroDaIa>,
        provedor: Arc<dyn Provedor>,
    ) -> Result<Self, String> {
        let caminho = caminho.as_ref().to_path_buf();
        let conn = sugestoes::abrir(&caminho).map_err(|e| e.to_string())?;
        let configuracao = ler_configuracao(&conn).unwrap_or_default();
        Ok(Self {
            caminho,
            fila,
            registro,
            provedor,
            controle: Controle::novo(),
            progresso: Arc::new(Mutex::new(Progresso::ocioso())),
            relatorio: Arc::new(Mutex::new(None)),
            thread: None,
            configuracao,
            rastro: None,
            conclusoes: CaixaDeConclusoes::default(),
            agente: None,
        })
    }

    /// O rastro que embrulha o provedor da `Fila`: falha transitória adia o item em vez de
    /// marcá-lo "não classificado".
    #[must_use]
    pub fn com_rastro(mut self, rastro: crate::rodada::RastroDeFalhas) -> Self {
        self.rastro = Some(rastro);
        self
    }

    /// O painel do agente contínuo lê o estado dele em `caminho` (o `agente.sqlite`).
    #[must_use]
    pub fn com_agente(mut self, caminho: impl Into<PathBuf>) -> Self {
        self.agente = Some(caminho.into());
        self
    }

    fn rodando(&self) -> bool {
        self.thread.as_ref().is_some_and(|t| !t.is_finished())
    }

    /// Espera a rodada acabar (testes e encerramento).
    pub fn esperar(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl RodadaDeTriagem for TriagemDoInventario {
    fn iniciar(&mut self, piloto: bool) -> Result<(), String> {
        if self.rodando() {
            return Err("já há uma rodada em andamento".to_owned());
        }
        self.esperar();
        self.controle.retomar();
        let conn = sugestoes::abrir(&self.caminho).map_err(|e| e.to_string())?;
        let fila_do_motor = Arc::clone(&self.fila);
        let registro = Arc::clone(&self.registro);
        let nome = self.provedor.nome();
        let controle = self.controle.clone();
        let rastro = self.rastro.clone();
        let progresso = Arc::clone(&self.progresso);
        let relatorio = Arc::clone(&self.relatorio);
        self.thread = Some(std::thread::spawn(move || {
            let falhou = |motivo: String| {
                if let Ok(mut p) = progresso.lock() {
                    p.situacao = SituacaoDaRodada::Falhou;
                    p.aviso = Some(motivo);
                }
            };
            let ao_abrir = |id: i64| registro.definir_rodada(Some(id));
            if piloto {
                match piloto::rodar_piloto(
                    &conn,
                    &fila_do_motor,
                    nome,
                    piloto::TAMANHO_DO_PILOTO,
                    &controle,
                    &progresso,
                    &ao_abrir,
                ) {
                    Ok(r) => {
                        if let Ok(mut slot) = relatorio.lock() {
                            *slot = Some(r);
                        }
                    }
                    Err(e) => falhou(e.to_string()),
                }
                registro.definir_rodada(None);
                return;
            }
            let alvos = match selecionar(&conn, &OpcoesDaFila::padrao()) {
                Ok(f) => f,
                Err(e) => return falhou(e.to_string()),
            };
            if let Err(e) = sugestoes::gravar_regras(&conn, &alvos.regras) {
                return falhou(e.to_string());
            }
            if let Err(e) = rodar(
                &conn,
                &fila_do_motor,
                nome,
                &alvos.alvos,
                &ConfigDaRodada {
                    rastro,
                    ..ConfigDaRodada::default()
                },
                &controle,
                &progresso,
                &ao_abrir,
            ) {
                falhou(e.to_string());
            }
            registro.definir_rodada(None);
        }));
        Ok(())
    }

    fn pausar(&mut self) {
        self.controle.pausar();
    }

    fn retomar(&mut self) -> Result<(), String> {
        if self.rodando() {
            self.controle.retomar();
            Ok(())
        } else {
            // A rodada terminou (pausada por limite ou teto): recalcula a fila e segue.
            self.iniciar(false)
        }
    }

    fn progresso(&self) -> Progresso {
        self.progresso
            .lock()
            .map_or_else(|e| e.into_inner().clone(), |p| p.clone())
    }
}

impl ServicoDeTriagem for TriagemDoInventario {
    fn grupos(&self, filtro: &FiltroDeSugestoes) -> Result<Vec<GrupoDeSugestoes>, String> {
        let conn = sugestoes::abrir(&self.caminho).map_err(|e| e.to_string())?;
        Ok(agrupar(
            listar_abertas(&conn, filtro).map_err(|e| e.to_string())?,
        ))
    }

    fn aceitar(
        &mut self,
        ids: &[i64],
        comandos: &mut dyn Comandos,
    ) -> Result<ResultadoDoAceite, String> {
        let conn = sugestoes::abrir(&self.caminho).map_err(|e| e.to_string())?;
        aceite::aceitar(&conn, comandos, ids).map_err(|e| e.to_string())
    }

    fn recusar(&mut self, ids: &[i64]) -> Result<usize, String> {
        let conn = sugestoes::abrir(&self.caminho).map_err(|e| e.to_string())?;
        aceite::recusar(&conn, ids).map_err(|e| e.to_string())
    }

    fn relatorio_do_piloto(&self) -> Option<RelatorioDoPiloto> {
        self.relatorio.lock().ok().and_then(|r| r.clone())
    }

    fn configuracao(&self) -> ConfiguracaoDeIa {
        self.configuracao.clone()
    }

    fn salvar_configuracao(&mut self, configuracao: ConfiguracaoDeIa) -> Result<(), String> {
        gravar_configuracao(&self.caminho, &configuracao)?;
        self.configuracao = configuracao;
        Ok(())
    }

    fn testar_conexao(&mut self) -> Result<String, String> {
        testar(self.provedor.as_ref())
    }

    fn iniciar_teste_de_conexao(&mut self) {
        let provedor = Arc::clone(&self.provedor);
        self.conclusoes.lancar(
            "sbf-teste-de-ia",
            move || Conclusao::Teste(testar(provedor.as_ref())),
            |m| Conclusao::Teste(Err(m)),
        );
    }

    fn iniciar_salvamento(&mut self, configuracao: ConfiguracaoDeIa) {
        // Vale já na tela; a gravação (que pode esperar o banco) roda fora do quadro.
        self.configuracao = configuracao.clone();
        let caminho = self.caminho.clone();
        self.conclusoes.lancar(
            "sbf-salvar-opcoes-de-ia",
            move || Conclusao::Configuracao(gravar_configuracao(&caminho, &configuracao)),
            |m| Conclusao::Configuracao(Err(m)),
        );
    }

    fn sondar(&mut self) -> Vec<Conclusao> {
        self.conclusoes.tomar()
    }

    fn ocupado(&self) -> bool {
        self.conclusoes.ocupada()
    }

    fn painel_do_agente(&self) -> Option<crate::agente::Painel> {
        let estado = self.agente.as_ref()?;
        let agente = crate::agente::Agente::abrir(estado).ok()?;
        let conn = sugestoes::abrir(&self.caminho).ok()?;
        agente.painel(&conn).ok()
    }

    fn apagar_regra_do_agente(&mut self, id: i64) -> Result<(), String> {
        let estado = self
            .agente
            .as_ref()
            .ok_or_else(|| "o agente de triagem não está ligado".to_owned())?;
        crate::agente::Agente::abrir(estado)
            .and_then(|a| a.apagar_regra(id))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }
}
