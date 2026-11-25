//! Conexões › Roteiros, parte dos **roteiros escritos** (Luau, spec 14).
//!
//! Um roteiro escrito é um programa em Luau, escrito à mão, que comanda o Chrome por verbos de
//! alto nível. Aqui a janela lista os roteiros, mostra o manifesto, o fonte (só leitura), o
//! histórico de estados e o resultado do último teste, e deixa **Executar agora** (só com
//! versão ativa).
//!
//! **Ativar e desativar não são um clique.** Cada um abre uma confirmação em que o usuário
//! digita `nome@versão` (ativar) ou o nome (desativar). A [`Confirmacao`] do banco só é
//! construída por [`confirmacao_da_janela`], o ponto único onde a janela pede essa prova.
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
    BancoDeRoteiros, Confirmacao, DetalheDoEscrito, EstadoEscrito, EventoEscrito, VersaoEscrita,
};
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

/// Um pedido de ativação ou desativação esperando a digitação do usuário.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pedido {
    pub nome: String,
    /// `Some(versão)` ativa essa versão; `None` desativa o roteiro.
    pub versao: Option<u32>,
    pub digitado: String,
}

impl Pedido {
    #[must_use]
    pub fn ativar(nome: &str, versao: u32) -> Self {
        Self {
            nome: nome.to_owned(),
            versao: Some(versao),
            digitado: String::new(),
        }
    }

    #[must_use]
    pub fn desativar(nome: &str) -> Self {
        Self {
            nome: nome.to_owned(),
            versao: None,
            digitado: String::new(),
        }
    }

    /// O que o usuário tem de digitar: `nome@versão` ou o nome.
    #[must_use]
    pub fn esperado(&self) -> String {
        match self.versao {
            Some(v) => Confirmacao::texto_de_ativar(&self.nome, v),
            None => Confirmacao::texto_de_desativar(&self.nome),
        }
    }

    /// A digitação confere **exatamente** (nada de espaço ou maiúscula a mais).
    #[must_use]
    pub fn confere(&self) -> bool {
        self.digitado == self.esperado()
    }
}

/// O único lugar onde a janela pede ao banco a prova de que o usuário confirmou.
///
/// LACUNA: `Confirmacao` só tem o construtor `do_terminal(terminal_interativo, ...)`, feito
/// para o CLI. A janela não é um terminal, e passar `true` ali seria mentir ao guarda do
/// banco. Enquanto o banco não oferecer um construtor próprio para a janela, a digitação é
/// conferida aqui, mas a ativação é recusada com o caminho do terminal e nada muda.
///
/// # Errors
/// Digitação diferente do esperado ou ausência do construtor da janela.
pub fn confirmacao_da_janela(
    digitado: &str,
    esperado: &str,
    comando_no_terminal: &str,
) -> Result<Confirmacao, String> {
    if digitado != esperado {
        return Err("A confirmação digitada é diferente do pedido; nada foi alterado.".to_owned());
    }
    Err(format!(
        "Esta janela ainda não consegue ativar nem desativar roteiro escrito: o banco só aceita a confirmação de um terminal. Nada foi alterado. No terminal, rode: {comando_no_terminal}"
    ))
}

/// Aplica o pedido ao banco. Devolve a mensagem de sucesso ou o motivo da recusa.
///
/// # Errors
/// Confirmação recusada ou erro do banco.
pub fn aplicar_pedido(banco: &BancoDeRoteiros, p: &Pedido) -> Result<String, String> {
    let esperado = p.esperado();
    let comando = match p.versao {
        Some(v) => format!("sbf roteiros escrito ativar {} {v}", p.nome),
        None => format!("sbf roteiros escrito desativar {}", p.nome),
    };
    let conf = confirmacao_da_janela(&p.digitado, &esperado, &comando)?;
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
}

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
            "Consulta o extrato no banco fictício; o token muda por minuto.",
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
            match self.ultimos.get(&r.nome) {
                Some(u) => {
                    let cor = if u.erro { p.perigo } else { p.sucesso };
                    ui.label(
                        RichText::new(format!("Última execução (nesta sessão): {}", u.texto))
                            .color(cor),
                    );
                }
                None => {
                    ui.label(
                        RichText::new("Última execução: nenhuma nesta sessão da janela.")
                            .color(p.tinta_suave),
                    );
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
        if pedir_ativar {
            self.pedido = Some(Pedido::ativar(&d.nome, v.numero));
        }
        if pedir_desativar {
            self.pedido = Some(Pedido::desativar(&d.nome));
        }
        self.confirmacao(ui, &d.nome, banco);
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
    fn a_janela_nao_burla_o_guarda_do_terminal() {
        // Registra a lacuna: sem um construtor de `Confirmacao` para a janela, nada é ativado,
        // mesmo com a digitação certa, e a mensagem aponta o comando do terminal.
        let b = banco_com_testado();
        let mut p = Pedido::ativar("contracheque-exemplo", 1);
        p.digitado = p.esperado();
        let e = aplicar_pedido(&b, &p).expect_err("ainda não há construtor da janela");
        assert!(
            e.contains("sbf roteiros escrito ativar contracheque-exemplo 1"),
            "{e}"
        );
        let v = b.versao_escrita("contracheque-exemplo", 1).expect("versão");
        assert_eq!(v.estado, EstadoEscrito::Testado, "nada muda");
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
            registros: vec![],
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
            let mut saida = ctx.run_ui(entrada, |ui| e.ui(ui, None, None, None));
            saida.textures_delta.clear();
        }
        assert!(e.pedido.is_some(), "sem clique, o pedido continua aberto");
    }
}
