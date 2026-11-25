//! Conexões › Roteiros, parte dos **roteiros escritos** (Luau, spec 16).
//!
//! Um roteiro escrito é um programa em Luau, escrito à mão, que comanda o Chrome por verbos de
//! alto nível. Aqui a janela lista os roteiros, mostra o manifesto, o fonte (só leitura), o
//! histórico de estados, o resultado do último teste e o **histórico de execuções** (lido do
//! banco, então vale também para as que a agenda do worker disparou), e deixa **Executar
//! agora** (só com versão ativa).
//!
//! **Sua vez no navegador.** Quando uma execução (da janela ou do worker, outro processo) para
//! num CAPTCHA, no código do aplicativo autenticador (TOTP) ou num MFA manual, a janela lê isso
//! do banco e mostra o cartão com **Continuar** e **Abortar**; o pedido volta ao executor
//! pelo mesmo banco.
//!
//! **Ativar e desativar não são um clique.** Cada um abre uma confirmação em que o usuário
//! digita o texto pedido (`nome@versão` para ativar, o nome para desativar; para versão
//! escrita pela IA ou pelo MCP o banco acrescenta o host principal). A [`Confirmacao`] do banco
//! só é construída por [`confirmacao_da_janela`] (`Confirmacao::da_janela`), o ponto único onde
//! a janela pede essa prova depois da digitação exata.
//!
//! A lógica de estado (quando cada botão vale, as linhas de texto, a conferência da digitação)
//! é pura e fica separada do desenho, para os testes não precisarem de tela.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};

use egui::{Align, Layout, RichText, ScrollArea, Ui};
use sbf_chaveiro::Chaveiro;
use sbf_roteiros::banco::{
    AutorEscrito, BancoDeRoteiros, Confirmacao, DetalheDoEscrito, EstadoDaExecucao, EstadoEscrito,
    EventoEscrito, VersaoEscrita,
};
use sbf_roteiros::execucao_escrita::{EventoDaExecucaoEscrita, LinhaDeExecucaoEscrita};
use sbf_roteiros::executor::Fim;
use sbf_roteiros::manifesto::{Classe, Manifesto, Mfa};
use sbf_roteiros::servico::{Ambiente, RelatorioEscrito, executar_escrito};

use crate::{componentes, tema};

/// O aviso que acompanha toda ativação.
pub const AVISO_DE_ATIVAR: &str =
    "Um roteiro escrito navega com sua senha; confira o fonte antes de ativar.";

// ---------------------------------------------------------------------------------------
// Lógica pura
// ---------------------------------------------------------------------------------------

/// Texto da classe de ação.
#[must_use]
pub fn rotulo_da_classe(c: Classe) -> &'static str {
    match c {
        Classe::Consulta => "consulta",
        Classe::AtualizarConexao => "atualizar conexão",
    }
}

/// Texto do MFA: `nenhum`, `manual` ou `e-mail`.
#[must_use]
pub fn rotulo_do_mfa(m: &Mfa) -> &'static str {
    match m {
        Mfa::Nenhum => "nenhum",
        Mfa::Manual => "manual",
        Mfa::Email { .. } => "e-mail",
    }
}

/// O texto do estado de uma versão, em português.
#[must_use]
pub fn rotulo_do_estado(e: EstadoEscrito) -> &'static str {
    e.texto()
}

/// A agenda do manifesto.
#[must_use]
pub fn texto_da_agenda(horas: Option<u32>) -> String {
    match horas {
        None => "só pelo botão".to_owned(),
        Some(1) => "a cada 1 hora".to_owned(),
        Some(h) if h % 24 == 0 && h >= 24 => {
            let d = h / 24;
            if d == 1 {
                "a cada 1 dia".to_owned()
            } else {
                format!("a cada {d} dias")
            }
        }
        Some(h) => format!("a cada {h} horas"),
    }
}

/// Uma linha do histórico de estados.
#[must_use]
pub fn linha_do_evento(e: &EventoEscrito) -> String {
    let (quando, versao, de, para, quem, detalhe) = e;
    let transicao = match de {
        Some(d) => format!("{d} → {para}"),
        None => format!("criada como {para}"),
    };
    let mut t = format!(
        "{} · versão {versao} · {transicao} · {quem}",
        quando.replace('T', " ")
    );
    if !detalhe.trim().is_empty() {
        t.push_str(" · ");
        t.push_str(detalhe.trim());
    }
    t
}

/// O que a lista mostra de um roteiro escrito.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDeEscrito {
    pub nome: String,
    pub descricao: String,
    pub classe: &'static str,
    pub mfa: &'static str,
    /// Destaque: o código de MFA vem do e-mail.
    pub mfa_por_email: bool,
    pub movimenta_dinheiro: bool,
    pub hosts: String,
    /// O estado da versão mais nova.
    pub estado_da_ultima: EstadoEscrito,
    pub ultima: u32,
    pub ativa: Option<u32>,
    pub agenda: String,
}

impl ResumoDeEscrito {
    /// Resume o roteiro. O manifesto vem da versão ativa e, sem ela, da mais nova.
    #[must_use]
    pub fn de(d: &DetalheDoEscrito) -> Option<Self> {
        let nova = d.versoes.iter().max_by_key(|v| v.numero)?;
        let ativa = d.versoes.iter().find(|v| v.estado == EstadoEscrito::Ativo);
        let m: &Manifesto = &ativa.unwrap_or(nova).manifesto;
        Some(Self {
            nome: d.nome.clone(),
            descricao: m.descricao.clone(),
            classe: rotulo_da_classe(m.classe),
            mfa: rotulo_do_mfa(&m.mfa),
            mfa_por_email: matches!(m.mfa, Mfa::Email { .. }),
            movimenta_dinheiro: m.movimenta_dinheiro,
            hosts: m.hosts.join(", "),
            estado_da_ultima: nova.estado,
            ultima: nova.numero,
            ativa: ativa.map(|v| v.numero),
            agenda: texto_da_agenda(m.agenda_horas),
        })
    }

    /// A linha de metadados do cartão.
    #[must_use]
    pub fn linha_de_metadados(&self) -> String {
        format!(
            "classe: {} · MFA: {} · hosts: {} · agenda: {}",
            self.classe, self.mfa, self.hosts, self.agenda
        )
    }

    /// A linha das versões: a ativa e a mais nova com o estado.
    #[must_use]
    pub fn linha_de_versoes(&self) -> String {
        let ativa = match self.ativa {
            Some(n) => format!("versão ativa: {n}"),
            None => "sem versão ativa".to_owned(),
        };
        format!(
            "{ativa} · mais nova: {} ({})",
            self.ultima,
            rotulo_do_estado(self.estado_da_ultima)
        )
    }
}

/// Executar vale com versão ativa e sem execução em curso.
#[must_use]
pub fn pode_executar(ativa: Option<u32>, rodando: bool) -> bool {
    ativa.is_some() && !rodando
}

/// Ativar vale só para versão `testado`, sem execução em curso.
#[must_use]
pub fn pode_ativar(estado: EstadoEscrito, rodando: bool) -> bool {
    estado == EstadoEscrito::Testado && !rodando
}

/// Desativar vale com versão ativa e sem execução em curso.
#[must_use]
pub fn pode_desativar(ativa: Option<u32>, rodando: bool) -> bool {
    ativa.is_some() && !rodando
}

/// O que a confirmação mostra da versão antes de o usuário digitar: quem escreveu, para onde
/// ela navega, de onde vêm a senha e o código de MFA. Só rótulos, nunca segredo.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResumoDaConfirmacao {
    /// `usuario`, `mcp` ou `ia:<modelo>`.
    pub autor: String,
    /// O autor não é o usuário: a confirmação ganha destaque.
    pub autor_externo: bool,
    pub hosts: String,
    pub rotulo_senha: String,
    pub mfa: String,
    pub movimenta_dinheiro: bool,
}

impl ResumoDaConfirmacao {
    #[must_use]
    pub fn de(v: &VersaoEscrita) -> Self {
        let m = &v.manifesto;
        Self {
            autor: v.autor.texto(),
            autor_externo: v.autor != AutorEscrito::Usuario,
            hosts: m.hosts.join(", "),
            rotulo_senha: m
                .rotulo_senha
                .clone()
                .unwrap_or_else(|| "nenhum (o roteiro não usa senha)".to_owned()),
            mfa: match &m.mfa {
                Mfa::Nenhum => "nenhum".to_owned(),
                Mfa::Manual => {
                    "manual: o roteiro para e você resolve no navegador (código do aplicativo autenticador, CAPTCHA)".to_owned()
                }
                Mfa::Email { remetente, .. } => {
                    format!("por e-mail: o SBF lê o código no Gmail, só de {remetente}")
                }
            },
            movimenta_dinheiro: m.movimenta_dinheiro,
        }
    }

    /// As linhas de leitura da confirmação.
    #[must_use]
    pub fn linhas(&self) -> Vec<String> {
        vec![
            format!("Quem escreveu esta versão: {}", self.autor),
            format!("Hosts onde ela navega e digita segredo: {}", self.hosts),
            format!("Senha do chaveiro (rótulo): {}", self.rotulo_senha),
            format!("MFA {}", self.mfa),
            format!(
                "Movimenta dinheiro: {}",
                if self.movimenta_dinheiro {
                    "sim"
                } else {
                    "não"
                }
            ),
        ]
    }

    /// O alerta de quando o autor não é o usuário.
    #[must_use]
    pub fn alerta(&self) -> Option<String> {
        self.autor_externo.then(|| {
            format!(
                "Esta versão foi escrita por {}, não por você. Leia o fonte inteiro e confira os hosts antes de digitar a confirmação.",
                self.autor
            )
        })
    }
}

/// O texto que o usuário tem de digitar para ativar `nome` na versão `numero`.
///
/// Isolado numa função só: o banco é quem sabe o texto (para versão escrita pela IA ou pelo MCP
/// ele inclui o host principal).
///
/// # Errors
/// Versão inexistente ou falha do banco.
pub fn texto_a_digitar_para_ativar(
    banco: &BancoDeRoteiros,
    nome: &str,
    numero: u32,
) -> Result<String, String> {
    banco
        .esperado_para_ativar(nome, numero)
        .map_err(|e| e.to_string())
}

/// O texto que o usuário tem de digitar para desativar `nome`.
#[must_use]
pub fn texto_a_digitar_para_desativar(banco: &BancoDeRoteiros, nome: &str) -> String {
    banco.esperado_para_desativar(nome)
}

/// Um pedido de ativação ou desativação esperando a digitação do usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    pub nome: String,
    /// `Some(versão)` ativa essa versão; `None` desativa o roteiro.
    pub versao: Option<u32>,
    pub digitado: String,
    /// O que o usuário tem de digitar (calculado uma vez, quando o pedido abre).
    esperado: String,
    /// O que mostrar da versão antes da digitação.
    pub resumo: Option<ResumoDaConfirmacao>,
}

impl Pedido {
    #[must_use]
    pub fn ativar(nome: &str, versao: u32) -> Self {
        Self {
            nome: nome.to_owned(),
            versao: Some(versao),
            digitado: String::new(),
            esperado: Confirmacao::texto_de_ativar(nome, versao),
            resumo: None,
        }
    }

    #[must_use]
    pub fn desativar(nome: &str) -> Self {
        Self {
            nome: nome.to_owned(),
            versao: None,
            digitado: String::new(),
            esperado: Confirmacao::texto_de_desativar(nome),
            resumo: None,
        }
    }

    /// Troca o texto a digitar pelo que o banco pediu.
    #[must_use]
    pub fn com_esperado(mut self, esperado: String) -> Self {
        self.esperado = esperado;
        self
    }

    /// Anexa o resumo da versão.
    #[must_use]
    pub fn com_resumo(mut self, resumo: ResumoDaConfirmacao) -> Self {
        self.resumo = Some(resumo);
        self
    }

    /// O que o usuário tem de digitar.
    #[must_use]
    pub fn esperado(&self) -> String {
        self.esperado.clone()
    }

    /// A digitação confere **exatamente** (nada de espaço ou maiúscula a mais).
    #[must_use]
    pub fn confere(&self) -> bool {
        self.digitado == self.esperado
    }
}

/// O único lugar onde a janela pede ao banco a prova de que o usuário confirmou: constrói a
/// [`Confirmacao`] pelo caminho da janela (`Confirmacao::da_janela`) e só se a digitação
/// confere exatamente com o pedido.
///
/// # Errors
/// Digitação diferente do esperado.
pub fn confirmacao_da_janela(digitado: &str, esperado: &str) -> Result<Confirmacao, String> {
    Confirmacao::da_janela(digitado, esperado)
        .map_err(|_| "A confirmação digitada é diferente do pedido; nada foi alterado.".to_owned())
}

/// Aplica o pedido ao banco. Devolve a mensagem de sucesso ou o motivo da recusa.
///
/// # Errors
/// Confirmação recusada ou erro do banco.
pub fn aplicar_pedido(banco: &BancoDeRoteiros, p: &Pedido) -> Result<String, String> {
    let conf = confirmacao_da_janela(&p.digitado, &p.esperado())?;
    match p.versao {
        Some(v) => {
            banco.ativar(&p.nome, v, &conf).map_err(|e| e.to_string())?;
            Ok(format!("{}@{v} ativado.", p.nome))
        }
        None => {
            let v = banco.desativar(&p.nome, &conf).map_err(|e| e.to_string())?;
            Ok(format!("{}@{v} desativado.", p.nome))
        }
    }
}

// ---------------------------------------------------------------------------------------
// Execuções: "Sua vez no navegador" e histórico (lidos do banco)
// ---------------------------------------------------------------------------------------

/// `2026-11-24T09:00:01.123Z` vira `2026-11-24 09:00:01` (UTC, como o banco grava).
#[must_use]
pub fn hora_curta(iso: &str) -> String {
    iso.chars()
        .take(19)
        .map(|c| if c == 'T' { ' ' } else { c })
        .collect()
}

/// O estado de uma execução, em português.
#[must_use]
pub fn rotulo_da_execucao(e: EstadoDaExecucao) -> &'static str {
    match e {
        EstadoDaExecucao::Rodando => "executando",
        EstadoDaExecucao::AguardandoUsuario => "esperando você",
        EstadoDaExecucao::Concluida => "concluída",
        EstadoDaExecucao::Falhou => "falhou",
        EstadoDaExecucao::Bloqueada => "bloqueada pela lista de bloqueio",
        EstadoDaExecucao::Cancelada => "cancelada",
        EstadoDaExecucao::EntradaRecusada => "entrada recusada: agenda suspensa",
    }
}

/// O que disparou a execução: `janela`, `agenda` ou `teste`.
#[must_use]
pub fn rotulo_do_gatilho(g: &str) -> &str {
    match g {
        "janela" => "botão da janela",
        "agenda" => "agenda do worker",
        "teste" => "teste no site falso",
        outro => outro,
    }
}

/// Uma linha do histórico de execuções.
#[must_use]
pub fn linha_da_execucao(e: &LinhaDeExecucaoEscrita) -> String {
    let fim = match &e.concluida_em {
        Some(f) => format!("até {}", hora_curta(f)),
        None => "em andamento".to_owned(),
    };
    let mut t = format!(
        "{} ({fim}) · {} · versão {} · {}",
        hora_curta(&e.iniciada_em),
        rotulo_do_gatilho(&e.gatilho),
        e.versao,
        rotulo_da_execucao(e.estado)
    );
    if let Some(m) = e.motivo.as_deref().filter(|m| !m.trim().is_empty()) {
        t.push_str(": ");
        t.push_str(m.trim());
    }
    if e.estado == EstadoDaExecucao::Concluida {
        t.push_str(&format!(
            " · {} baixado{}, {} novo{}",
            e.baixados,
            if e.baixados == 1 { "" } else { "s" },
            e.novos,
            if e.novos == 1 { "" } else { "s" }
        ));
    }
    t
}

/// Uma linha de evento de uma execução (sem segredo: o banco só guarda texto já limpo).
#[must_use]
pub fn linha_do_evento_da_execucao(e: &EventoDaExecucaoEscrita) -> String {
    let (quando, tipo, mensagem) = e;
    format!("{} · {tipo} · {mensagem}", hora_curta(quando))
}

/// A frase do cartão "Sua vez no navegador".
#[must_use]
pub fn texto_da_pausa(motivo: Option<&str>) -> String {
    let m = motivo
        .map(str::trim)
        .filter(|m| !m.is_empty())
        .unwrap_or("O site precisa de você");
    format!(
        "{m}. Resolva no Chrome que o SBF abriu (CAPTCHA, código do aplicativo autenticador, MFA manual) e depois clique em Continuar: o roteiro escrito retoma de onde parou. Abortar encerra a execução."
    )
}

/// O que a janela diz de uma execução que terminou: o texto e se é erro.
#[must_use]
pub fn texto_do_relatorio(r: &RelatorioEscrito) -> (String, bool) {
    match &r.fim {
        Fim::Concluida => (
            format!(
                "concluída: {} baixado{}, {} novo{} no inventário",
                r.baixados,
                if r.baixados == 1 { "" } else { "s" },
                r.novos,
                if r.novos == 1 { "" } else { "s" }
            ),
            false,
        ),
        Fim::Falhou { motivo, .. } => (format!("falhou: {motivo}"), true),
        Fim::Bloqueada { bloqueio, .. } => (
            format!("bloqueada pela lista de bloqueio: {bloqueio}"),
            true,
        ),
        Fim::Cancelada => ("cancelada".to_owned(), true),
        Fim::PausaExpirou { .. } => ("não concluída: você não voltou a tempo".to_owned(), true),
    }
}

/// A versão que o detalhe mostra por padrão: a mais nova.
#[must_use]
pub fn versao_padrao(d: &DetalheDoEscrito) -> Option<u32> {
    d.versoes.iter().map(|v| v.numero).max()
}

// ---------------------------------------------------------------------------------------
// Estado e desenho
// ---------------------------------------------------------------------------------------

struct EmCursoEscrito {
    roteiro: String,
    cancelar: Arc<AtomicBool>,
    resposta: Receiver<Result<RelatorioEscrito, String>>,
}

/// O último resultado visto nesta sessão da janela.
#[derive(Debug, Clone)]
struct Ultimo {
    texto: String,
    erro: bool,
    registros: Vec<String>,
}

/// O estado da parte "roteiros escritos".
#[derive(Default)]
pub struct Escritos {
    detalhes: Vec<DetalheDoEscrito>,
    selecionado: Option<String>,
    versao_vista: Option<u32>,
    em_curso: Vec<EmCursoEscrito>,
    ultimos: HashMap<String, Ultimo>,
    pedido: Option<Pedido>,
    aviso: Option<(String, bool)>,
    /// Execuções esperando o usuário (do banco, de qualquer processo) que ele ainda não
    /// respondeu.
    aguardando: Vec<LinhaDeExecucaoEscrita>,
    /// As últimas execuções de cada roteiro (as mais novas primeiro), lidas do banco.
    historico: HashMap<String, Vec<LinhaDeExecucaoEscrita>>,
    /// Os eventos das execuções que o usuário expandiu.
    eventos: HashMap<i64, Vec<EventoDaExecucaoEscrita>>,
}

/// Quantas execuções o detalhe mostra por roteiro.
const EXECUCOES_NO_DETALHE: usize = 8;

impl std::fmt::Debug for Escritos {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Escritos")
            .field("roteiros", &self.detalhes.len())
            .field("em_curso", &self.em_curso.len())
            .finish_non_exhaustive()
    }
}

impl Escritos {
    /// Relê os roteiros escritos do banco.
    pub fn reler(&mut self, banco: &BancoDeRoteiros) {
        let Ok(linhas) = banco.listar_escritos() else {
            return;
        };
        self.detalhes = linhas
            .iter()
            .filter_map(|l| banco.mostrar(&l.nome).ok())
            .collect();
        self.reler_execucoes(banco);
    }

    /// Relê do banco as execuções: as que esperam o usuário, o histórico de cada roteiro e os
    /// eventos das que estão expandidas. É o que faz o cartão "Sua vez no navegador" aparecer
    /// também para a execução que a agenda do worker (outro processo) disparou.
    pub fn reler_execucoes(&mut self, banco: &BancoDeRoteiros) {
        if let Ok(espera) = banco.aguardando_escritas() {
            // Quem já respondeu (aqui ou em outro processo) mas ainda não voltou a rodar não
            // pede de novo.
            self.aguardando = espera
                .into_iter()
                .filter(|e| banco.pedido_do_usuario_escrita(e.id).is_ok_and(|p| p == 0))
                .collect();
        }
        let mut historico = HashMap::new();
        for d in &self.detalhes {
            if let Ok(h) = banco.historico_escrito(Some(&d.nome), EXECUCOES_NO_DETALHE) {
                historico.insert(d.nome.clone(), h);
            }
        }
        self.historico = historico;
        let abertas: Vec<i64> = self.eventos.keys().copied().collect();
        for id in abertas {
            match banco.eventos_escritos(id) {
                Ok(ev) => {
                    self.eventos.insert(id, ev);
                }
                Err(_) => {
                    self.eventos.remove(&id);
                }
            }
        }
    }

    /// O usuário respondeu à pausa: `continuar` retoma, senão **aborta**. Grava o pedido no
    /// banco (é por ele que o executor, aqui ou no worker, fica sabendo) e, se a execução roda
    /// nesta janela, também levanta o sinal de cancelar.
    pub fn responder_pausa(
        &mut self,
        banco: Option<&BancoDeRoteiros>,
        execucao: i64,
        roteiro: &str,
        continuar: bool,
    ) {
        if let Some(b) = banco
            && let Err(e) = b.pedir_continuar_escrita(execucao, continuar)
        {
            self.aviso = Some((format!("Não consegui avisar o roteiro: {e}"), true));
            return;
        }
        if !continuar && let Some(e) = self.em_curso.iter().find(|e| e.roteiro == roteiro) {
            e.cancelar.store(true, Ordering::SeqCst);
        }
        self.aguardando.retain(|e| e.id != execucao);
        self.aviso = Some((
            if continuar {
                format!("{roteiro}: você continuou; o roteiro escrito retoma de onde parou.")
            } else {
                format!("{roteiro}: você abortou; a execução termina como cancelada.")
            },
            !continuar,
        ));
    }

    /// O cartão "Sua vez no navegador", um por execução que espera o usuário.
    pub fn sua_vez_ui(&mut self, ui: &mut Ui, banco: Option<&BancoDeRoteiros>) {
        let p = tema::paleta(ui.ctx());
        for e in self.aguardando.clone() {
            let mut resposta = None;
            componentes::cartao(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Sua vez no navegador")
                            .strong()
                            .color(p.aviso),
                    );
                    ui.label(format!("· {} (roteiro escrito)", e.roteiro));
                });
                ui.label(RichText::new(texto_da_pausa(e.motivo.as_deref())).color(p.tinta));
                ui.horizontal(|ui| {
                    if ui.button("Continuar").clicked() {
                        resposta = Some(true);
                    }
                    if ui.button("Abortar").clicked() {
                        resposta = Some(false);
                    }
                });
            });
            ui.add_space(6.0);
            if let Some(continuar) = resposta {
                self.responder_pausa(banco, e.id, &e.roteiro, continuar);
            }
        }
    }

    /// Dados fictícios para a demonstração e as capturas.
    pub fn demonstracao(&mut self) {
        fn manifesto(
            nome: &str,
            desc: &str,
            mfa: Mfa,
            classe: Classe,
            agenda: Option<u32>,
        ) -> Manifesto {
            Manifesto {
                nome: nome.to_owned(),
                descricao: desc.to_owned(),
                url_inicial: "https://portal.exemplo.test/entrar".to_owned(),
                hosts: vec!["portal.exemplo.test".to_owned()],
                rotulo_senha: Some("senha:portal-exemplo".to_owned()),
                rotulo_cpf: None,
                mfa,
                movimenta_dinheiro: false,
                classe,
                agenda_horas: agenda,
                agenda: Vec::new(),
                parametros: Vec::new(),
            }
        }
        fn versao(
            m: &Manifesto,
            numero: u32,
            estado: EstadoEscrito,
            teste: Option<&str>,
        ) -> VersaoEscrita {
            VersaoEscrita {
                roteiro: m.nome.clone(),
                numero,
                manifesto: m.clone(),
                fonte: "-- Roteiro fictício (demonstração)\nnavegador.abrir(\"https://portal.exemplo.test/entrar\")\nnavegador.preencher_senha(\"#senha\", \"senha:portal-exemplo\")\nnavegador.clicar(\"#entrar\")\nif navegador.esperar(\"#demonstrativo\", 20) then\n  navegador.baixar(\"#baixar-pdf\")\nend\n".to_owned(),
                checksum: format!("demo{numero:04}"),
                estado,
                autor: sbf_roteiros::banco::AutorEscrito::Usuario,
                motivo: String::new(),
                criada_em: "2026-11-24 09:00:00".to_owned(),
                resultado_do_teste: teste.map(str::to_owned),
            }
        }
        let a = manifesto(
            "contracheque-exemplo",
            "Baixa o demonstrativo de pagamento do mês no portal fictício.",
            Mfa::Email {
                remetente: "nao-responda@portal.exemplo.test".to_owned(),
                formato: "\\d{6}".to_owned(),
            },
            Classe::Consulta,
            Some(168),
        );
        let b = manifesto(
            "extrato-banco-exemplo",
            "Consulta o extrato no banco fictício; o código do aplicativo autenticador (TOTP) muda por minuto.",
            Mfa::Manual,
            Classe::Consulta,
            None,
        );
        let c = manifesto(
            "atualizar-conexoes-exemplo",
            "Clica em Atualizar nas conexões do portal fictício.",
            Mfa::Nenhum,
            Classe::AtualizarConexao,
            Some(24),
        );
        let ev =
            |q: &str, v: u32, de: Option<&str>, para: &str, quem: &str, d: &str| -> EventoEscrito {
                (
                    q.to_owned(),
                    v,
                    de.map(str::to_owned),
                    para.to_owned(),
                    quem.to_owned(),
                    d.to_owned(),
                )
            };
        self.detalhes = vec![
            DetalheDoEscrito {
                nome: a.nome.clone(),
                versoes: vec![
                    versao(
                        &a,
                        1,
                        EstadoEscrito::Ativo,
                        Some("passou: site falso, 1 arquivo baixado"),
                    ),
                    versao(
                        &a,
                        2,
                        EstadoEscrito::Testado,
                        Some("passou: site falso, 1 arquivo baixado"),
                    ),
                ],
                eventos: vec![
                    ev(
                        "2026-11-20 08:00:00",
                        1,
                        None,
                        "rascunho",
                        "usuario",
                        "primeira versão",
                    ),
                    ev(
                        "2026-11-20 08:05:00",
                        1,
                        Some("rascunho"),
                        "testado",
                        "teste",
                        "passou: site falso, 1 arquivo baixado",
                    ),
                    ev(
                        "2026-11-20 08:10:00",
                        1,
                        Some("testado"),
                        "ativo",
                        "usuario",
                        "ativada com confirmação digitada",
                    ),
                    ev(
                        "2026-11-24 09:00:00",
                        2,
                        None,
                        "rascunho",
                        "usuario",
                        "espera o portal demorar mais",
                    ),
                    ev(
                        "2026-11-24 09:05:00",
                        2,
                        Some("rascunho"),
                        "testado",
                        "teste",
                        "passou: site falso, 1 arquivo baixado",
                    ),
                ],
            },
            DetalheDoEscrito {
                nome: b.nome.clone(),
                versoes: vec![versao(&b, 1, EstadoEscrito::Rascunho, None)],
                eventos: vec![ev(
                    "2026-11-24 10:00:00",
                    1,
                    None,
                    "rascunho",
                    "mcp",
                    "proposta pela IA",
                )],
            },
            DetalheDoEscrito {
                nome: c.nome.clone(),
                versoes: vec![versao(
                    &c,
                    1,
                    EstadoEscrito::Ativo,
                    Some("passou: site falso"),
                )],
                eventos: vec![
                    ev("2026-11-18 08:00:00", 1, None, "rascunho", "usuario", ""),
                    ev(
                        "2026-11-18 08:02:00",
                        1,
                        Some("rascunho"),
                        "testado",
                        "teste",
                        "passou: site falso",
                    ),
                    ev(
                        "2026-11-18 08:03:00",
                        1,
                        Some("testado"),
                        "ativo",
                        "usuario",
                        "ativada com confirmação digitada",
                    ),
                ],
            },
        ];
        let exec = |id: i64,
                    roteiro: &str,
                    versao: u32,
                    gatilho: &str,
                    estado: EstadoDaExecucao,
                    motivo: Option<&str>,
                    baixados: i64| LinhaDeExecucaoEscrita {
            id,
            roteiro: roteiro.to_owned(),
            versao,
            gatilho: gatilho.to_owned(),
            iniciada_em: "2026-11-24T08:30:00.000Z".to_owned(),
            concluida_em: (estado != EstadoDaExecucao::AguardandoUsuario)
                .then(|| "2026-11-24T08:31:12.000Z".to_owned()),
            estado,
            chamada: 9,
            motivo: motivo.map(str::to_owned),
            baixados,
            novos: baixados,
        };
        self.historico = HashMap::from([
            (
                "contracheque-exemplo".to_owned(),
                vec![
                    exec(
                        12,
                        "contracheque-exemplo",
                        1,
                        "agenda",
                        EstadoDaExecucao::Concluida,
                        None,
                        1,
                    ),
                    exec(
                        7,
                        "contracheque-exemplo",
                        1,
                        "janela",
                        EstadoDaExecucao::Falhou,
                        Some("o elemento #demonstrativo não apareceu"),
                        0,
                    ),
                ],
            ),
            (
                "atualizar-conexoes-exemplo".to_owned(),
                vec![exec(
                    13,
                    "atualizar-conexoes-exemplo",
                    1,
                    "agenda",
                    EstadoDaExecucao::AguardandoUsuario,
                    Some("Há um CAPTCHA na página"),
                    0,
                )],
            ),
        ]);
        self.eventos = HashMap::from([(
            12,
            vec![
                (
                    "2026-11-24T08:30:01.000Z".to_owned(),
                    "registro".to_owned(),
                    "entrou no portal fictício".to_owned(),
                ),
                (
                    "2026-11-24T08:31:10.000Z".to_owned(),
                    "download".to_owned(),
                    "demonstrativo.pdf".to_owned(),
                ),
            ],
        )]);
        self.aguardando = vec![exec(
            13,
            "atualizar-conexoes-exemplo",
            1,
            "agenda",
            EstadoDaExecucao::AguardandoUsuario,
            Some("Há um CAPTCHA na página"),
            0,
        )];
        self.selecionado = None;
        self.versao_vista = None;
        self.pedido = None;
    }

    fn rodando(&self, nome: &str) -> bool {
        self.em_curso.iter().any(|e| e.roteiro == nome)
    }

    /// Junta o que as threads devolveram.
    pub fn andamento(&mut self) {
        let mut feitos = Vec::new();
        for (i, e) in self.em_curso.iter().enumerate() {
            if let Ok(r) = e.resposta.try_recv() {
                feitos.push((i, e.roteiro.clone(), r));
            }
        }
        for (i, nome, r) in feitos.into_iter().rev() {
            self.em_curso.remove(i);
            let u = match r {
                Ok(rel) => {
                    let (texto, erro) = texto_do_relatorio(&rel);
                    Ultimo {
                        texto,
                        erro,
                        registros: rel.registros,
                    }
                }
                Err(e) => Ultimo {
                    texto: e,
                    erro: true,
                    registros: Vec::new(),
                },
            };
            self.aviso = Some((format!("{nome}: {}", u.texto), u.erro));
            self.ultimos.insert(nome, u);
        }
    }

    /// Dispara a versão ativa numa thread de trabalho: a janela não trava.
    fn executar(&mut self, amb: Option<&Ambiente>, chaveiro: Option<&Chaveiro>, nome: &str) {
        let Some(amb) = amb.cloned() else {
            self.aviso = Some((
                "Na demonstração os roteiros não rodam: não há navegador nem site.".into(),
                false,
            ));
            return;
        };
        if self.rodando(nome) {
            return;
        }
        let Some(chaveiro) = chaveiro.cloned() else {
            self.aviso = Some((
                "O chaveiro não abriu: sem ele o roteiro não tem a senha do site.".into(),
                true,
            ));
            return;
        };
        let cancelar = Arc::new(AtomicBool::new(false));
        let c2 = Arc::clone(&cancelar);
        let (tx, rx) = channel();
        let n = nome.to_owned();
        std::thread::spawn(move || {
            let r = executar_escrito(&amb, &n, "janela", &chaveiro, &c2).map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.em_curso.push(EmCursoEscrito {
            roteiro: nome.to_owned(),
            cancelar,
            resposta: rx,
        });
    }

    /// Pede o cancelamento de tudo o que roda (ao fechar a janela).
    pub fn cancelar_tudo(&self) {
        for e in &self.em_curso {
            e.cancelar.store(true, Ordering::SeqCst);
        }
    }

    /// Desenha a seção.
    pub fn ui(
        &mut self,
        ui: &mut Ui,
        banco: Option<&BancoDeRoteiros>,
        amb: Option<&Ambiente>,
        chaveiro: Option<&Chaveiro>,
    ) {
        let p = tema::paleta(ui.ctx());
        ui.add_space(10.0);
        componentes::rotulo_de_secao(ui, "Roteiros escritos (Luau)");
        ui.label(
            RichText::new("Um roteiro escrito é um programa em Luau, escrito à mão, que comanda o Chrome por verbos de alto nível. Ele nasce em rascunho, passa por um teste no site falso e só vale depois que você o ativa, digitando a confirmação.")
                .size(12.5)
                .color(p.tinta_suave),
        );
        ui.add_space(4.0);
        if let Some((t, erro)) = &self.aviso {
            if *erro {
                componentes::aviso(ui, "error", t, p.perigo_sutil, p.borda, p.perigo);
            } else {
                componentes::aviso(ui, "help", t, p.info_sutil, p.info_borda, p.info);
            }
            ui.add_space(6.0);
        }
        if self.detalhes.is_empty() {
            ui.label(
                RichText::new("Nenhum roteiro escrito ainda. Importe um pelo terminal: sbf roteiros escrito importar.")
                    .color(p.tinta_suave),
            );
            return;
        }
        let detalhes = self.detalhes.clone();
        for d in &detalhes {
            let Some(r) = ResumoDeEscrito::de(d) else {
                continue;
            };
            self.cartao(ui, d, &r, amb, chaveiro);
            if self.selecionado.as_deref() == Some(d.nome.as_str()) {
                self.detalhe(ui, d, banco);
            }
            ui.add_space(6.0);
        }
    }

    fn cartao(
        &mut self,
        ui: &mut Ui,
        d: &DetalheDoEscrito,
        r: &ResumoDeEscrito,
        amb: Option<&Ambiente>,
        chaveiro: Option<&Chaveiro>,
    ) {
        let p = tema::paleta(ui.ctx());
        let rodando = self.rodando(&r.nome);
        let (fundo, cor) = match r.estado_da_ultima {
            EstadoEscrito::Ativo => (p.sucesso_sutil, p.sucesso),
            EstadoEscrito::Testado => (p.info_sutil, p.info),
            EstadoEscrito::Rascunho => (p.aviso_sutil, p.aviso),
            EstadoEscrito::Aposentado => (p.superficie_3, p.tinta_suave),
        };
        componentes::cartao(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&r.nome).strong().size(15.0));
                componentes::selo(ui, rotulo_do_estado(r.estado_da_ultima), fundo, cor);
                if r.mfa_por_email {
                    componentes::selo(ui, "MFA por e-mail", p.aviso_sutil, p.aviso);
                }
                if r.classe != "consulta" {
                    componentes::selo(ui, r.classe, p.destaque_sutil, p.destaque_tinta);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if rodando {
                        if ui.button("Cancelar").clicked()
                            && let Some(e) = self.em_curso.iter().find(|e| e.roteiro == r.nome)
                        {
                            e.cancelar.store(true, Ordering::SeqCst);
                        }
                        ui.spinner();
                    } else if ui
                        .add_enabled(
                            pode_executar(r.ativa, rodando),
                            egui::Button::new("Executar agora"),
                        )
                        .on_disabled_hover_text(
                            "Só a versão ativa executa. Ative uma versão testada primeiro.",
                        )
                        .clicked()
                    {
                        self.executar(amb, chaveiro, &r.nome);
                    }
                    if ui.button("Detalhes").clicked() {
                        if self.selecionado.as_deref() == Some(r.nome.as_str()) {
                            self.selecionado = None;
                        } else {
                            self.selecionado = Some(r.nome.clone());
                            self.versao_vista = versao_padrao(d);
                            self.pedido = None;
                        }
                    }
                });
            });
            if !r.descricao.is_empty() {
                ui.label(RichText::new(&r.descricao).size(12.5).color(p.tinta_suave));
            }
            ui.label(
                RichText::new(r.linha_de_metadados())
                    .size(12.5)
                    .color(p.tinta_suave),
            );
            ui.label(
                RichText::new(r.linha_de_versoes())
                    .size(12.5)
                    .color(p.tinta_suave),
            );
            match (
                self.historico.get(&r.nome).and_then(|h| h.first()),
                self.ultimos.get(&r.nome),
            ) {
                (Some(e), _) => {
                    ui.label(
                        RichText::new(format!("Última execução: {}", linha_da_execucao(e)))
                            .color(cor_da_execucao(&p, e.estado)),
                    );
                }
                (None, Some(u)) => {
                    let cor = if u.erro { p.perigo } else { p.sucesso };
                    ui.label(RichText::new(format!("Última execução: {}", u.texto)).color(cor));
                }
                (None, None) => {
                    ui.label(RichText::new("Última execução: nenhuma ainda.").color(p.tinta_suave));
                }
            }
        });
    }

    fn detalhe(&mut self, ui: &mut Ui, d: &DetalheDoEscrito, banco: Option<&BancoDeRoteiros>) {
        let p = tema::paleta(ui.ctx());
        let Some(numero) = self.versao_vista.or_else(|| versao_padrao(d)) else {
            return;
        };
        let Some(v) = d.versoes.iter().find(|v| v.numero == numero) else {
            return;
        };
        let ativa = d
            .versoes
            .iter()
            .find(|v| v.estado == EstadoEscrito::Ativo)
            .map(|v| v.numero);
        let rodando = self.rodando(&d.nome);
        let mut pedir_ativar = false;
        let mut pedir_desativar = false;
        let mut carregar: Vec<i64> = Vec::new();
        let mut descartar: Vec<i64> = Vec::new();
        componentes::cartao(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("Versão");
                egui::ComboBox::from_id_salt(("versao-escrita", &d.nome))
                    .selected_text(format!("{} ({})", v.numero, rotulo_do_estado(v.estado)))
                    .show_ui(ui, |ui| {
                        for x in d.versoes.iter().rev() {
                            ui.selectable_value(
                                &mut self.versao_vista,
                                Some(x.numero),
                                format!("{} ({})", x.numero, rotulo_do_estado(x.estado)),
                            );
                        }
                    });
                ui.label(
                    RichText::new(format!(
                        "escrita por {} em {}",
                        v.autor.texto(),
                        v.criada_em.replace('T', " ")
                    ))
                    .size(12.0)
                    .color(p.tinta_suave),
                );
            });
            if !v.motivo.trim().is_empty() {
                ui.label(
                    RichText::new(format!("Motivo: {}", v.motivo))
                        .size(12.0)
                        .color(p.tinta_suave),
                );
            }

            componentes::rotulo_de_secao(ui, "Manifesto");
            for linha in linhas_do_manifesto(&v.manifesto) {
                ui.label(RichText::new(linha).monospace().size(12.0));
            }

            componentes::rotulo_de_secao(ui, "Fonte Luau (só leitura)");
            ScrollArea::vertical()
                .id_salt(("fonte-escrito", &d.nome, v.numero))
                .max_height(260.0)
                .show(ui, |ui| {
                    let mut texto: &str = &v.fonte;
                    ui.add(
                        egui::TextEdit::multiline(&mut texto)
                            .code_editor()
                            .desired_width(f32::INFINITY),
                    );
                });

            componentes::rotulo_de_secao(ui, "Resultado do último teste");
            match &v.resultado_do_teste {
                Some(t) => {
                    ui.label(t);
                }
                None => {
                    ui.label(
                        RichText::new("Esta versão ainda não foi testada no site falso.")
                            .color(p.tinta_suave),
                    );
                }
            }

            if let Some(u) = self
                .ultimos
                .get(&d.nome)
                .filter(|u| !u.registros.is_empty())
            {
                componentes::rotulo_de_secao(ui, "O que o roteiro registrou na última execução");
                for linha in &u.registros {
                    ui.label(RichText::new(linha).monospace().size(12.0));
                }
            }

            componentes::rotulo_de_secao(ui, "Histórico de execuções (horários em UTC)");
            let execucoes = self.historico.get(&d.nome).cloned().unwrap_or_default();
            if execucoes.is_empty() {
                ui.label(RichText::new("Nenhuma execução ainda.").color(p.tinta_suave));
            }
            for x in &execucoes {
                let titulo = RichText::new(linha_da_execucao(x))
                    .size(12.0)
                    .color(cor_da_execucao(&p, x.estado));
                let aberto = egui::CollapsingHeader::new(titulo)
                    .id_salt(("execucao-escrita", x.id))
                    .show(ui, |ui| match self.eventos.get(&x.id) {
                        Some(ev) if ev.is_empty() => {
                            ui.label(RichText::new("Sem eventos.").color(p.tinta_suave));
                        }
                        Some(ev) => {
                            for e in ev {
                                ui.label(
                                    RichText::new(linha_do_evento_da_execucao(e))
                                        .monospace()
                                        .size(12.0),
                                );
                            }
                        }
                        None => carregar.push(x.id),
                    });
                if aberto.body_returned.is_none() {
                    descartar.push(x.id);
                }
            }

            componentes::rotulo_de_secao(ui, "Histórico de estados");
            if d.eventos.is_empty() {
                ui.label(RichText::new("Sem eventos.").color(p.tinta_suave));
            }
            for e in &d.eventos {
                ui.label(RichText::new(linha_do_evento(e)).size(12.0));
            }

            ui.add_space(6.0);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(pode_ativar(v.estado, rodando), egui::Button::new("Ativar…"))
                    .on_disabled_hover_text("Só uma versão testada pode ser ativada.")
                    .clicked()
                {
                    pedir_ativar = true;
                }
                if ui
                    .add_enabled(
                        pode_desativar(ativa, rodando),
                        egui::Button::new("Desativar…"),
                    )
                    .on_disabled_hover_text("Não há versão ativa.")
                    .clicked()
                {
                    pedir_desativar = true;
                }
            });
        });
        if let Some(b) = banco {
            for id in carregar {
                if let Ok(ev) = b.eventos_escritos(id) {
                    self.eventos.insert(id, ev);
                }
            }
        }
        for id in descartar {
            self.eventos.remove(&id);
        }
        if pedir_ativar {
            self.abrir_pedido_de_ativar(banco, v);
        }
        if pedir_desativar {
            let resumo = d
                .versoes
                .iter()
                .find(|x| x.estado == EstadoEscrito::Ativo)
                .map(ResumoDaConfirmacao::de);
            let esperado = match banco {
                Some(b) => texto_a_digitar_para_desativar(b, &d.nome),
                None => Confirmacao::texto_de_desativar(&d.nome),
            };
            let mut ped = Pedido::desativar(&d.nome).com_esperado(esperado);
            if let Some(r) = resumo {
                ped = ped.com_resumo(r);
            }
            self.pedido = Some(ped);
        }
        self.confirmacao(ui, &d.nome, banco);
    }

    fn abrir_pedido_de_ativar(&mut self, banco: Option<&BancoDeRoteiros>, v: &VersaoEscrita) {
        let esperado = match banco {
            Some(b) => match texto_a_digitar_para_ativar(b, &v.roteiro, v.numero) {
                Ok(t) => t,
                Err(e) => {
                    self.aviso = Some((e, true));
                    return;
                }
            },
            None => Confirmacao::texto_de_ativar(&v.roteiro, v.numero),
        };
        self.pedido = Some(
            Pedido::ativar(&v.roteiro, v.numero)
                .com_esperado(esperado)
                .com_resumo(ResumoDaConfirmacao::de(v)),
        );
    }

    fn confirmacao(&mut self, ui: &mut Ui, nome: &str, banco: Option<&BancoDeRoteiros>) {
        let Some(pedido) = self.pedido.as_mut().filter(|p| p.nome == nome) else {
            return;
        };
        let p = tema::paleta(ui.ctx());
        let esperado = pedido.esperado();
        let mut confirmar = false;
        let mut desistir = false;
        componentes::cartao(ui, |ui| {
            let titulo = match pedido.versao {
                Some(v) => format!("Ativar {nome} versão {v}?"),
                None => format!("Desativar {nome}?"),
            };
            componentes::titulo(ui, &titulo, 15.0);
            if let Some(alerta) = pedido.resumo.as_ref().and_then(ResumoDaConfirmacao::alerta) {
                componentes::aviso(ui, "error", &alerta, p.perigo_sutil, p.borda, p.perigo);
            }
            if let Some(r) = &pedido.resumo {
                for linha in r.linhas() {
                    let destaque = r.autor_externo && linha.starts_with("Quem escreveu");
                    let t = RichText::new(linha).size(12.5);
                    ui.label(if destaque {
                        t.strong().color(p.perigo)
                    } else {
                        t
                    });
                }
            }
            if pedido.versao.is_some() {
                componentes::aviso(
                    ui,
                    "help",
                    AVISO_DE_ATIVAR,
                    p.aviso_sutil,
                    p.aviso_borda,
                    p.aviso,
                );
            } else {
                ui.label(
                    "O roteiro fica sem versão ativa: o botão e a agenda deixam de executá-lo.",
                );
            }
            ui.label(format!("Para confirmar, digite exatamente: {esperado}"));
            ui.add(egui::TextEdit::singleline(&mut pedido.digitado).desired_width(360.0));
            let ok = pedido.confere();
            ui.horizontal(|ui| {
                let rotulo = if pedido.versao.is_some() {
                    "Ativar"
                } else {
                    "Desativar"
                };
                confirmar = ui
                    .add_enabled(ok && banco.is_some(), egui::Button::new(rotulo))
                    .clicked();
                desistir = ui.button("Cancelar").clicked();
            });
        });
        if desistir {
            self.pedido = None;
        } else if confirmar && let (Some(b), Some(pedido)) = (banco, self.pedido.clone()) {
            self.aviso = Some(match aplicar_pedido(b, &pedido) {
                Ok(t) => (t, false),
                Err(e) => (e, true),
            });
            self.pedido = None;
            self.reler(b);
        }
    }
}

fn cor_da_execucao(p: &tema::Paleta, e: EstadoDaExecucao) -> egui::Color32 {
    match e {
        EstadoDaExecucao::Concluida => p.sucesso,
        EstadoDaExecucao::Rodando | EstadoDaExecucao::AguardandoUsuario => p.aviso,
        EstadoDaExecucao::Falhou
        | EstadoDaExecucao::Bloqueada
        | EstadoDaExecucao::Cancelada
        | EstadoDaExecucao::EntradaRecusada => p.perigo,
    }
}

/// O manifesto como linhas de leitura (sem segredo: só rótulos).
#[must_use]
pub fn linhas_do_manifesto(m: &Manifesto) -> Vec<String> {
    let mut l = vec![
        format!("nome: {}", m.nome),
        format!("url inicial: {}", m.url_inicial),
        format!("hosts permitidos: {}", m.hosts.join(", ")),
        format!(
            "senha do chaveiro: {}",
            m.rotulo_senha.as_deref().unwrap_or("nenhuma")
        ),
        format!(
            "CPF do chaveiro: {}",
            if m.rotulo_cpf.is_some() {
                "sim"
            } else {
                "não"
            }
        ),
        format!("classe de ação: {}", rotulo_da_classe(m.classe)),
    ];
    match &m.mfa {
        Mfa::Nenhum => l.push("MFA: nenhum".to_owned()),
        Mfa::Manual => l.push("MFA: manual (você resolve no navegador)".to_owned()),
        Mfa::Email { remetente, formato } => {
            l.push(format!(
                "MFA por e-mail: remetente {remetente}, formato {formato}"
            ));
        }
    }
    l.push(format!(
        "movimenta dinheiro: {}",
        if m.movimenta_dinheiro { "sim" } else { "não" }
    ));
    l.push(format!("agenda: {}", texto_da_agenda(m.agenda_horas)));
    l
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_roteiros::banco::AutorEscrito;

    fn manifesto(nome: &str, mfa: Mfa) -> Manifesto {
        Manifesto {
            nome: nome.to_owned(),
            descricao: "baixa o demonstrativo".to_owned(),
            url_inicial: "https://portal.exemplo.test/entrar".to_owned(),
            hosts: vec!["portal.exemplo.test".to_owned()],
            rotulo_senha: Some("senha:portal-exemplo".to_owned()),
            rotulo_cpf: None,
            mfa,
            movimenta_dinheiro: false,
            classe: Classe::Consulta,
            agenda_horas: Some(168),
            agenda: Vec::new(),
            parametros: Vec::new(),
        }
    }

    fn email() -> Mfa {
        Mfa::Email {
            remetente: "nao-responda@portal.exemplo.test".to_owned(),
            formato: "\\d{6}".to_owned(),
        }
    }

    fn banco_com_testado() -> BancoDeRoteiros {
        let b = BancoDeRoteiros::em_memoria().expect("banco");
        let m = manifesto("contracheque-exemplo", Mfa::Nenhum);
        let i = b
            .importar_escrito(
                &m,
                "navegador.registrar(\"oi\")",
                &AutorEscrito::Usuario,
                "",
            )
            .expect("importa");
        b.marcar_testado(
            "contracheque-exemplo",
            i.numero,
            &sbf_roteiros::banco::ResultadoDoTeste {
                passou: true,
                resumo: "ok".into(),
            },
        )
        .expect("testa");
        b
    }

    #[test]
    fn executar_so_com_versao_ativa_e_sem_execucao_em_curso() {
        assert!(pode_executar(Some(1), false));
        assert!(!pode_executar(None, false));
        assert!(!pode_executar(Some(1), true));
    }

    #[test]
    fn ativar_so_versao_testada_e_sem_execucao_em_curso() {
        assert!(pode_ativar(EstadoEscrito::Testado, false));
        assert!(!pode_ativar(EstadoEscrito::Rascunho, false));
        assert!(!pode_ativar(EstadoEscrito::Ativo, false));
        assert!(!pode_ativar(EstadoEscrito::Aposentado, false));
        assert!(!pode_ativar(EstadoEscrito::Testado, true));
    }

    #[test]
    fn desativar_so_com_versao_ativa() {
        assert!(pode_desativar(Some(2), false));
        assert!(!pode_desativar(None, false));
        assert!(!pode_desativar(Some(2), true));
    }

    #[test]
    fn a_confirmacao_exige_a_digitacao_exata() {
        let mut p = Pedido::ativar("contracheque-exemplo", 2);
        assert_eq!(p.esperado(), "contracheque-exemplo@2");
        assert!(!p.confere(), "vazio não confere");
        p.digitado = "contracheque-exemplo@1".into();
        assert!(!p.confere(), "outra versão não confere");
        p.digitado = "contracheque-exemplo@2 ".into();
        assert!(!p.confere(), "espaço sobrando não confere");
        p.digitado = "Contracheque-exemplo@2".into();
        assert!(!p.confere(), "maiúscula não confere");
        p.digitado = "contracheque-exemplo@2".into();
        assert!(p.confere());
        let mut d = Pedido::desativar("contracheque-exemplo");
        assert_eq!(d.esperado(), "contracheque-exemplo");
        d.digitado = "contracheque-exemplo".into();
        assert!(d.confere());
    }

    #[test]
    fn digitacao_errada_nao_altera_o_banco() {
        let b = banco_com_testado();
        let mut p = Pedido::ativar("contracheque-exemplo", 1);
        p.digitado = "outra-coisa".into();
        let e = aplicar_pedido(&b, &p).expect_err("recusa");
        assert!(e.contains("diferente"), "{e}");
        let v = b.versao_escrita("contracheque-exemplo", 1).expect("versão");
        assert_eq!(v.estado, EstadoEscrito::Testado);
        assert!(
            b.versao_ativa_escrita("contracheque-exemplo")
                .expect("lê")
                .is_none()
        );
    }

    #[test]
    fn a_janela_ativa_so_com_a_digitacao_exata() {
        let b = banco_com_testado();
        let mut p = Pedido::ativar("contracheque-exemplo", 1);
        p.digitado = "contracheque-exemplo".into();
        let e = aplicar_pedido(&b, &p).expect_err("digitação errada");
        assert!(e.contains("diferente do pedido"), "{e}");
        assert_eq!(
            b.versao_escrita("contracheque-exemplo", 1)
                .expect("versão")
                .estado,
            EstadoEscrito::Testado,
            "nada muda"
        );
        p.digitado = p.esperado();
        let ok = aplicar_pedido(&b, &p).expect("digitação certa ativa");
        assert!(ok.contains("ativado"), "{ok}");
        assert_eq!(
            b.versao_escrita("contracheque-exemplo", 1)
                .expect("versão")
                .estado,
            EstadoEscrito::Ativo
        );
    }

    #[test]
    fn o_resumo_destaca_mfa_por_email_classe_agenda_e_versoes() {
        let b = BancoDeRoteiros::em_memoria().expect("banco");
        let m = manifesto("contracheque-exemplo", email());
        b.importar_escrito(&m, "navegador.registrar(\"a\")", &AutorEscrito::Usuario, "")
            .expect("importa");
        let d = b.mostrar("contracheque-exemplo").expect("mostra");
        let r = ResumoDeEscrito::de(&d).expect("resumo");
        assert!(r.mfa_por_email);
        assert_eq!(r.mfa, "e-mail");
        assert_eq!(r.classe, "consulta");
        assert_eq!(r.agenda, "a cada 7 dias");
        assert_eq!(r.ativa, None);
        assert_eq!(r.estado_da_ultima, EstadoEscrito::Rascunho);
        assert_eq!(
            r.linha_de_versoes(),
            "sem versão ativa · mais nova: 1 (rascunho)"
        );
        assert_eq!(
            r.linha_de_metadados(),
            "classe: consulta · MFA: e-mail · hosts: portal.exemplo.test · agenda: a cada 7 dias"
        );
    }

    #[test]
    fn o_resumo_sem_mfa_por_email_nao_destaca() {
        let b = banco_com_testado();
        let d = b.mostrar("contracheque-exemplo").expect("mostra");
        let r = ResumoDeEscrito::de(&d).expect("resumo");
        assert!(!r.mfa_por_email);
        assert_eq!(r.mfa, "nenhum");
        assert_eq!(r.estado_da_ultima, EstadoEscrito::Testado);
    }

    #[test]
    fn textos_da_agenda() {
        assert_eq!(texto_da_agenda(None), "só pelo botão");
        assert_eq!(texto_da_agenda(Some(1)), "a cada 1 hora");
        assert_eq!(texto_da_agenda(Some(6)), "a cada 6 horas");
        assert_eq!(texto_da_agenda(Some(24)), "a cada 1 dia");
        assert_eq!(texto_da_agenda(Some(48)), "a cada 2 dias");
    }

    #[test]
    fn o_historico_formata_criacao_e_transicao() {
        let criada: EventoEscrito = (
            "2026-11-24T09:00:00".into(),
            1,
            None,
            "rascunho".into(),
            "usuario".into(),
            String::new(),
        );
        assert_eq!(
            linha_do_evento(&criada),
            "2026-11-24 09:00:00 · versão 1 · criada como rascunho · usuario"
        );
        let teste: EventoEscrito = (
            "2026-11-24 09:05:00".into(),
            1,
            Some("rascunho".into()),
            "testado".into(),
            "teste".into(),
            "passou: ok".into(),
        );
        assert_eq!(
            linha_do_evento(&teste),
            "2026-11-24 09:05:00 · versão 1 · rascunho → testado · teste · passou: ok"
        );
    }

    #[test]
    fn o_manifesto_na_tela_mostra_so_rotulos() {
        let m = manifesto("contracheque-exemplo", email());
        let t = linhas_do_manifesto(&m).join("\n");
        assert!(t.contains("senha do chaveiro: senha:portal-exemplo"));
        assert!(t.contains("MFA por e-mail: remetente nao-responda@portal.exemplo.test"));
        assert!(t.contains("movimenta dinheiro: não"));
    }

    #[test]
    fn o_relatorio_vira_texto_e_marca_erro() {
        let rel = |fim| RelatorioEscrito {
            fim,
            baixados: 1,
            novos: 1,
            descartados: 0,
            registros: vec![],
            entrada_recusada: None,
        };
        let (t, erro) = texto_do_relatorio(&rel(Fim::Concluida));
        assert!(!erro);
        assert_eq!(t, "concluída: 1 baixado, 1 novo no inventário");
        let (t, erro) = texto_do_relatorio(&rel(Fim::Falhou {
            passo: 0,
            motivo: "o elemento não apareceu".into(),
        }));
        assert!(erro);
        assert_eq!(t, "falhou: o elemento não apareceu");
        assert!(texto_do_relatorio(&rel(Fim::Cancelada)).1);
        assert!(texto_do_relatorio(&rel(Fim::PausaExpirou { passo: 0 })).1);
    }

    #[test]
    fn a_versao_padrao_e_a_mais_nova() {
        let b = banco_com_testado();
        let m = manifesto("contracheque-exemplo", Mfa::Manual);
        b.importar_escrito(&m, "navegador.registrar(\"b\")", &AutorEscrito::Usuario, "")
            .expect("v2");
        let d = b.mostrar("contracheque-exemplo").expect("mostra");
        assert_eq!(versao_padrao(&d), Some(2));
    }

    #[test]
    fn a_demonstracao_cobre_os_casos_da_tela() {
        let mut e = Escritos::default();
        e.demonstracao();
        let resumos: Vec<_> = e.detalhes.iter().filter_map(ResumoDeEscrito::de).collect();
        assert_eq!(resumos.len(), 3);
        assert!(resumos.iter().any(|r| r.mfa_por_email));
        assert!(resumos.iter().any(|r| r.classe == "atualizar conexão"));
        assert!(resumos.iter().any(|r| r.ativa.is_none()));
    }

    fn execucao_esperando(b: &BancoDeRoteiros, gatilho: &str, motivo: &str) -> i64 {
        let id = b
            .iniciar_execucao_escrita("contracheque-exemplo", 1, gatilho)
            .expect("inicia");
        b.marcar_aguardando_escrita(id, motivo).expect("aguarda");
        id
    }

    fn desenhar(e: &mut Escritos, banco: Option<&BancoDeRoteiros>) {
        let ctx = egui::Context::default();
        crate::tema::instalar_fontes(&ctx);
        crate::tema::aplicar(&ctx, "claro");
        for _ in 0..2 {
            let entrada = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                ..egui::RawInput::default()
            };
            let mut saida = ctx.run_ui(entrada, |ui| {
                e.sua_vez_ui(ui, banco);
                e.ui(ui, banco, None, None);
            });
            saida.textures_delta.clear();
        }
    }

    #[test]
    fn o_cartao_sua_vez_aparece_para_execucao_do_worker_e_some_ao_voltar_a_rodar() {
        let b = banco_com_testado();
        let mut e = Escritos::default();
        e.reler(&b);
        assert!(e.aguardando.is_empty(), "nada espera ainda");
        // O worker (outro processo) disparou pela agenda e parou num CAPTCHA.
        let id = execucao_esperando(&b, "agenda", "Há um CAPTCHA na página");
        e.reler(&b);
        assert_eq!(e.aguardando.len(), 1);
        assert_eq!(e.aguardando[0].roteiro, "contracheque-exemplo");
        assert_eq!(e.aguardando[0].gatilho, "agenda");
        let t = texto_da_pausa(e.aguardando[0].motivo.as_deref());
        assert!(t.starts_with("Há um CAPTCHA na página."), "{t}");
        assert!(t.contains("código do aplicativo autenticador"), "{t}");
        assert!(!t.to_lowercase().contains("token"), "{t}");
        desenhar(&mut e, Some(&b));
        b.marcar_rodando_escrita(id).expect("volta");
        e.reler(&b);
        assert!(e.aguardando.is_empty());
    }

    #[test]
    fn continuar_grava_o_pedido_e_esconde_o_cartao() {
        let b = banco_com_testado();
        let id = execucao_esperando(&b, "janela", "MFA manual");
        let mut e = Escritos::default();
        e.reler(&b);
        assert_eq!(e.aguardando.len(), 1);
        e.responder_pausa(Some(&b), id, "contracheque-exemplo", true);
        assert_eq!(b.pedido_do_usuario_escrita(id).expect("lê"), 1);
        assert!(e.aguardando.is_empty());
        assert!(matches!(&e.aviso, Some((_, false))));
        // O executor ainda não voltou a rodar: a próxima leitura não mostra de novo.
        e.reler(&b);
        assert!(e.aguardando.is_empty(), "já respondida");
    }

    #[test]
    fn abortar_grava_o_cancelamento_e_levanta_o_sinal_da_thread_da_janela() {
        let b = banco_com_testado();
        let id = execucao_esperando(&b, "janela", "CAPTCHA");
        let mut e = Escritos::default();
        let cancelar = Arc::new(AtomicBool::new(false));
        let (_tx, rx) = channel();
        e.em_curso.push(EmCursoEscrito {
            roteiro: "contracheque-exemplo".to_owned(),
            cancelar: Arc::clone(&cancelar),
            resposta: rx,
        });
        e.reler(&b);
        e.responder_pausa(Some(&b), id, "contracheque-exemplo", false);
        assert_eq!(b.pedido_do_usuario_escrita(id).expect("lê"), 2);
        assert!(
            cancelar.load(Ordering::SeqCst),
            "a thread da janela cancela"
        );
        assert!(e.aguardando.is_empty());
        assert!(matches!(&e.aviso, Some((t, true)) if t.contains("abortou")));
        e.reler(&b);
        assert!(e.aguardando.is_empty());
    }

    #[test]
    fn abortar_de_execucao_do_worker_so_grava_o_pedido() {
        let b = banco_com_testado();
        let id = execucao_esperando(&b, "agenda", "CAPTCHA");
        let mut e = Escritos::default();
        e.responder_pausa(Some(&b), id, "contracheque-exemplo", false);
        assert_eq!(b.pedido_do_usuario_escrita(id).expect("lê"), 2);
    }

    #[test]
    fn o_historico_vem_do_banco_com_gatilho_estado_e_numeros() {
        let b = banco_com_testado();
        let id = b
            .iniciar_execucao_escrita("contracheque-exemplo", 1, "agenda")
            .expect("inicia");
        b.evento_escrito(id, "registro", "entrou no portal fictício")
            .expect("evento");
        b.finalizar_escrita(id, EstadoDaExecucao::Concluida, None, 2, 1)
            .expect("fecha");
        let id2 = b
            .iniciar_execucao_escrita("contracheque-exemplo", 1, "janela")
            .expect("inicia");
        b.finalizar_escrita(
            id2,
            EstadoDaExecucao::Falhou,
            Some("o elemento não apareceu"),
            0,
            0,
        )
        .expect("fecha");
        let mut e = Escritos::default();
        e.reler(&b);
        let h = &e.historico["contracheque-exemplo"];
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].id, id2, "a mais nova primeiro");
        let l0 = linha_da_execucao(&h[0]);
        assert!(l0.contains("botão da janela"), "{l0}");
        assert!(
            l0.contains("versão 1 · falhou: o elemento não apareceu"),
            "{l0}"
        );
        let l1 = linha_da_execucao(&h[1]);
        assert!(l1.contains("agenda do worker"), "{l1}");
        assert!(l1.contains("concluída · 2 baixados, 1 novo"), "{l1}");
        assert!(!l1.contains('Z'), "{l1}");
        // Expandir carrega os eventos (sem segredo: só o texto que o banco guardou).
        e.eventos
            .insert(id, b.eventos_escritos(id).expect("eventos"));
        let ev = linha_do_evento_da_execucao(&e.eventos[&id][0]);
        assert!(
            ev.ends_with("· registro · entrou no portal fictício"),
            "{ev}"
        );
        desenhar(&mut e, Some(&b));
    }

    #[test]
    fn a_linha_da_execucao_em_andamento_e_a_hora_curta() {
        assert_eq!(
            hora_curta("2026-11-24T09:00:01.123Z"),
            "2026-11-24 09:00:01"
        );
        let x = LinhaDeExecucaoEscrita {
            id: 1,
            roteiro: "contracheque-exemplo".into(),
            versao: 3,
            gatilho: "teste".into(),
            iniciada_em: "2026-11-24T09:00:01.123Z".into(),
            concluida_em: None,
            estado: EstadoDaExecucao::AguardandoUsuario,
            chamada: 4,
            motivo: Some("CAPTCHA".into()),
            baixados: 0,
            novos: 0,
        };
        assert_eq!(
            linha_da_execucao(&x),
            "2026-11-24 09:00:01 (em andamento) · teste no site falso · versão 3 · esperando você: CAPTCHA"
        );
    }

    #[test]
    fn a_confirmacao_destaca_autor_que_nao_e_o_usuario() {
        let b = BancoDeRoteiros::em_memoria().expect("banco");
        let m = manifesto("contracheque-exemplo", email());
        b.importar_escrito(
            &m,
            "navegador.registrar(\"a\")",
            &AutorEscrito::Mcp,
            "proposta",
        )
        .expect("mcp");
        b.importar_escrito(
            &m,
            "navegador.registrar(\"b\")",
            &AutorEscrito::Ia("claude-x".into()),
            "",
        )
        .expect("ia");
        b.importar_escrito(&m, "navegador.registrar(\"c\")", &AutorEscrito::Usuario, "")
            .expect("usuario");
        let r =
            |n| ResumoDaConfirmacao::de(&b.versao_escrita("contracheque-exemplo", n).expect("v"));
        let mcp = r(1);
        assert!(mcp.autor_externo);
        assert_eq!(mcp.autor, "mcp");
        let alerta = mcp.alerta().expect("alerta");
        assert!(alerta.contains("escrita por mcp, não por você"), "{alerta}");
        assert!(r(2).alerta().expect("ia").contains("ia:claude-x"));
        let eu = r(3);
        assert!(!eu.autor_externo);
        assert!(eu.alerta().is_none());
        let t = eu.linhas().join("\n");
        assert!(t.contains("Quem escreveu esta versão: usuario"), "{t}");
        assert!(
            t.contains("Hosts onde ela navega e digita segredo: portal.exemplo.test"),
            "{t}"
        );
        assert!(
            t.contains("Senha do chaveiro (rótulo): senha:portal-exemplo"),
            "{t}"
        );
        assert!(
            t.contains(
                "MFA por e-mail: o SBF lê o código no Gmail, só de nao-responda@portal.exemplo.test"
            ),
            "{t}"
        );
        assert!(!t.to_lowercase().contains("token"), "{t}");
    }

    #[test]
    fn abrir_o_pedido_de_ativar_anexa_o_resumo_e_o_texto_a_digitar() {
        let b = banco_com_testado();
        let v = b.versao_escrita("contracheque-exemplo", 1).expect("v");
        let mut e = Escritos::default();
        e.abrir_pedido_de_ativar(Some(&b), &v);
        let p = e.pedido.as_ref().expect("pedido");
        assert_eq!(p.esperado(), "contracheque-exemplo@1");
        assert_eq!(p.resumo.as_ref().map(|r| r.autor.as_str()), Some("usuario"));
        e.selecionado = Some("contracheque-exemplo".to_owned());
        e.reler(&b);
        desenhar(&mut e, Some(&b));
    }

    #[test]
    fn a_secao_desenha_com_detalhe_e_confirmacao_sem_travar() {
        let ctx = egui::Context::default();
        crate::tema::instalar_fontes(&ctx);
        crate::tema::aplicar(&ctx, "claro");
        let mut e = Escritos::default();
        e.demonstracao();
        e.selecionado = Some("contracheque-exemplo".to_owned());
        e.versao_vista = Some(2);
        e.pedido = Some(Pedido::ativar("contracheque-exemplo", 2));
        for _ in 0..2 {
            let entrada = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                ..egui::RawInput::default()
            };
            let mut saida = ctx.run_ui(entrada, |ui| {
                e.sua_vez_ui(ui, None);
                e.ui(ui, None, None, None);
            });
            saida.textures_delta.clear();
        }
        assert!(e.pedido.is_some(), "sem clique, o pedido continua aberto");
        assert_eq!(e.aguardando.len(), 1, "a demonstração mostra a pausa");
    }
}
