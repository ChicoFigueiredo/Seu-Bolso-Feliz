//! A área **Roteiros** (Conexões › Roteiros, spec 10 C1) e as senhas de sites em Opções ›
//! Chaveiro.
//!
//! A janela lista os roteiros, a última execução de cada um, e deixa **Executar**, **Gravar novo
//! (guiado pela IA)**, ver o **histórico** e aprovar ou recusar o **conserto** que a IA propôs
//! quando um site mudou. Quando um roteiro precisa de você (MFA, CAPTCHA), aparece o aviso
//! **"Sua vez no navegador"** com **Continuar**: o executor espera esse clique (ele passa pelo
//! `roteiros.sqlite`, então vale também para um roteiro que o `sbf worker` está rodando).
//!
//! Nada aqui mostra senha: as senhas de sites mostram só o rótulo.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use egui::{Align, Layout, RichText, ScrollArea, Ui};
use sbf_chaveiro::{Chaveiro, Segredo};
use sbf_ia::{NivelDeConfianca, Provedor, Registro};
use sbf_roteiros::Passo;
use sbf_roteiros::banco::{
    BancoDeRoteiros, EstadoDaExecucao, LinhaDeExecucao, LinhaDeRoteiro, Proposta as Conserto,
};
use sbf_roteiros::executor::Fim;
use sbf_roteiros::gravacao::{Aprovacao, Aprovador, FimDaGravacao, Guia, Proposta};
use sbf_roteiros::passo::Marca;
use sbf_roteiros::servico::{Ambiente, executar_roteiro, gravar_novo};
use zeroize::Zeroize as _;

use crate::roteiros_escritos::Escritos;
use crate::{componentes, tema};

/// O provedor e o registro de `saida_ia` da IA dos roteiros.
pub type ProvedorEmRegistro = (Arc<dyn Provedor>, Arc<dyn Registro>);

/// O provedor de IA e o registro do que sai para ele (o `sbf` monta sobre o motor da triagem).
pub trait IaDosRoteiros: Send + Sync {
    /// O provedor principal e o registro de `saida_ia`.
    ///
    /// # Errors
    /// O motivo de a IA não estar disponível.
    fn pronta(&self) -> Result<ProvedorEmRegistro, String>;
}

/// O que a gravação mostra enquanto a IA conduz.
#[derive(Debug, Default)]
struct Gravando {
    nome: String,
    aceitos: Vec<String>,
    proposta: Option<Proposta>,
    motivo_da_recusa: String,
    terminou: Option<Result<String, String>>,
}

enum DaGravacao {
    Proposta(Proposta),
    Terminou(Result<String, String>),
}

struct AprovadorDaJanela {
    para_a_janela: Sender<DaGravacao>,
    da_janela: Receiver<Aprovacao>,
    cancelar: Arc<AtomicBool>,
}

impl Aprovador for AprovadorDaJanela {
    fn decidir(&mut self, proposta: &Proposta) -> Aprovacao {
        if self
            .para_a_janela
            .send(DaGravacao::Proposta(proposta.clone()))
            .is_err()
        {
            return Aprovacao::Parar;
        }
        loop {
            match self.da_janela.recv_timeout(Duration::from_millis(250)) {
                Ok(a) => return a,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                    if self.cancelar.load(Ordering::Relaxed) {
                        return Aprovacao::Parar;
                    }
                }
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return Aprovacao::Parar,
            }
        }
    }
}

/// O formulário de "Gravar novo".
#[derive(Debug, Default)]
struct Formulario {
    aberto: bool,
    nome: String,
    url: String,
    rotulo_de_senha: String,
    objetivo: String,
}

struct EmCurso {
    roteiro: String,
    cancelar: Arc<AtomicBool>,
    resposta: Receiver<Result<String, String>>,
}

/// O estado da área Roteiros.
#[derive(Default)]
pub struct Roteiros {
    amb: Option<Ambiente>,
    ia: Option<Arc<dyn IaDosRoteiros>>,
    chaveiro: Option<Chaveiro>,
    demonstracao: bool,
    banco: Option<BancoDeRoteiros>,
    linhas: Vec<LinhaDeRoteiro>,
    historico: Vec<LinhaDeExecucao>,
    aguardando: Vec<LinhaDeExecucao>,
    lido_em: Option<Instant>,
    selecionado: Option<i64>,
    conserto: Option<(i64, Conserto, Vec<String>)>,
    em_curso: Vec<EmCurso>,
    formulario: Formulario,
    gravando: Option<Gravando>,
    para_a_gravacao: Option<Sender<Aprovacao>>,
    da_gravacao: Option<Receiver<DaGravacao>>,
    cancelar_a_gravacao: Option<Arc<AtomicBool>>,
    aviso: Option<(String, bool)>,
    /// Rótulos de senha de site, só na demonstração.
    rotulos_da_demonstracao: Vec<String>,
    senhas_da_demonstracao: SenhasDeSites,
    /// Os roteiros escritos (Luau, spec 16).
    escritos: Escritos,
}

impl std::fmt::Debug for Roteiros {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Roteiros")
            .field("ligado", &self.amb.is_some())
            .field("demonstracao", &self.demonstracao)
            .field("roteiros", &self.linhas.len())
            .finish_non_exhaustive()
    }
}

/// O passo como o usuário o aprova: a URL inteira, o texto que será digitado.
fn texto_do_passo(p: &Passo) -> String {
    p.descricao_completa()
}

impl Roteiros {
    /// Liga os roteiros de verdade: onde ficam as coisas, a IA e o chaveiro.
    pub fn ligar(
        &mut self,
        ambiente: Ambiente,
        ia: Option<Arc<dyn IaDosRoteiros>>,
        chaveiro: Option<Chaveiro>,
    ) {
        self.banco = ambiente.banco().ok();
        self.amb = Some(ambiente);
        self.ia = ia;
        self.chaveiro = chaveiro;
        self.demonstracao = false;
    }

    /// Dados fictícios para a demonstração e as capturas (nenhum Chrome, nenhum site).
    pub fn demonstracao(&mut self, cena: &str) {
        self.demonstracao = true;
        let exec = |id: i64,
                    roteiro_id: i64,
                    nome: &str,
                    gatilho: &str,
                    estado: EstadoDaExecucao,
                    quando: &str,
                    passo: i64,
                    motivo: Option<&str>,
                    baixados: i64,
                    novos: i64| LinhaDeExecucao {
            id,
            roteiro_id,
            roteiro: nome.to_owned(),
            versao: 1,
            gatilho: gatilho.to_owned(),
            iniciada_em: quando.to_owned(),
            concluida_em: Some(quando.to_owned()),
            estado,
            passo_atual: passo,
            motivo: motivo.map(str::to_owned),
            baixados,
            novos,
        };
        let h1 = exec(
            41,
            1,
            "CAIXA · extrato do FGTS",
            "agenda",
            EstadoDaExecucao::Concluida,
            "2026-11-14T06:02",
            7,
            None,
            1,
            1,
        );
        let h2 = exec(
            43,
            2,
            "e-CAC · situação fiscal",
            "janela",
            EstadoDaExecucao::AguardandoUsuario,
            "2026-11-14T09:40",
            3,
            Some("o site mostrou um CAPTCHA"),
            0,
            0,
        );
        let h3 = exec(
            40,
            3,
            "Banco Exemplo · extratos",
            "agenda",
            EstadoDaExecucao::Falhou,
            "2026-11-13T06:01",
            5,
            Some("passo 6: o elemento \"#baixar\" não apareceu"),
            0,
            0,
        );
        let h4 = exec(
            38,
            3,
            "Banco Exemplo · extratos",
            "janela",
            EstadoDaExecucao::Bloqueada,
            "2026-11-12T20:15",
            4,
            Some(
                "passo 5: o campo \"texto\" do elemento indica \"pagar\", e roteiro só consulta e baixa",
            ),
            0,
            0,
        );
        let h5 = exec(
            35,
            1,
            "CAIXA · extrato do FGTS",
            "agenda",
            EstadoDaExecucao::Concluida,
            "2026-11-13T06:02",
            7,
            None,
            1,
            0,
        );
        let linha = |id: i64,
                     nome: &str,
                     desc: &str,
                     url: &str,
                     rotulo: &str,
                     agenda: Option<i64>,
                     passos: usize,
                     versao: i64,
                     proposta: bool,
                     ultima: &LinhaDeExecucao| LinhaDeRoteiro {
            id,
            nome: nome.to_owned(),
            descricao: desc.to_owned(),
            url_inicial: url.to_owned(),
            rotulo_senha: Some(rotulo.to_owned()),
            agenda_horas: agenda,
            ativo: true,
            versao,
            passos,
            ha_proposta: proposta,
            ultima: Some(ultima.clone()),
        };
        self.linhas = vec![
            linha(
                1,
                "CAIXA · extrato do FGTS",
                "baixar o extrato do FGTS do mês",
                "https://exemplo.invalid/caixa",
                "caixa-fgts",
                Some(24),
                8,
                1,
                false,
                &h1,
            ),
            linha(
                2,
                "e-CAC · situação fiscal",
                "baixar o relatório de situação fiscal",
                "https://exemplo.invalid/ecac",
                "ecac",
                None,
                9,
                2,
                false,
                &h2,
            ),
            linha(
                3,
                "Banco Exemplo · extratos",
                "baixar os extratos do mês",
                "https://exemplo.invalid/banco",
                "banco-exemplo",
                Some(24),
                8,
                3,
                true,
                &h3,
            ),
        ];
        self.historico = vec![h2.clone(), h3, h4, h1, h5];
        self.aguardando = vec![h2];
        self.rotulos_da_demonstracao =
            vec!["banco-exemplo".into(), "caixa-fgts".into(), "ecac".into()];
        self.conserto = None;
        self.gravando = None;
        self.formulario = Formulario::default();
        self.escritos.demonstracao();
        match cena {
            "roteiros-conserto" => {
                self.aguardando.clear();
                self.linhas.sort_by_key(|l| i32::from(l.id != 3));
                self.selecionado = Some(3);
                self.conserto = Some((
                    3,
                    Conserto {
                        versao_id: 9,
                        roteiro_id: 3,
                        numero: 4,
                        passos: vec![],
                        motivo: "O botão de baixar agora se chama \"#baixar-extrato\" e fica dentro da lista de extratos."
                            .to_owned(),
                    },
                    vec![
                        "Abrir https://exemplo.invalid/banco/login".into(),
                        "Preencher #cpf com o CPF do chaveiro".into(),
                        "Preencher #senha com a senha \"banco-exemplo\" do chaveiro".into(),
                        "Clicar em #entrar".into(),
                        "Clicar em #extratos-link".into(),
                        "−  Baixar por #baixar".into(),
                        "+  Baixar por #baixar-extrato".into(),
                    ],
                ));
            }
            "roteiros-gravar" => {
                self.aguardando.clear();
                self.gravando = Some(Gravando {
                    nome: "Banco Exemplo · extratos".into(),
                    aceitos: vec![
                        "Abrir https://exemplo.invalid/banco/login".into(),
                        "Preencher #cpf com o CPF do chaveiro".into(),
                        "Preencher #senha com a senha \"banco-exemplo\" do chaveiro".into(),
                    ],
                    proposta: Some(Proposta::Passo {
                        passo: Passo::Clicar {
                            seletor: "#entrar".into(),
                        },
                        explicacao: "Enviar o login para entrar na conta.".into(),
                        alvo: "<button>, texto \"Entrar\"".into(),
                    }),
                    ..Gravando::default()
                });
            }
            "roteiros-formulario" => {
                self.aguardando.clear();
                self.formulario = Formulario {
                    aberto: true,
                    nome: "Banco Exemplo · extratos".into(),
                    url: "https://exemplo.invalid/banco".into(),
                    rotulo_de_senha: "banco-exemplo".into(),
                    objetivo: "baixar os extratos do mês".into(),
                };
            }
            _ => {}
        }
    }

    fn ligado(&self) -> bool {
        self.amb.is_some() || self.demonstracao
    }

    /// Os rótulos de senha de site que a gravação pode usar.
    fn rotulos(&self) -> Vec<String> {
        match &self.chaveiro {
            Some(c) => c.rotulos_de_site(),
            None => self.rotulos_da_demonstracao.clone(),
        }
    }

    fn reler(&mut self) {
        let Some(b) = &self.banco else { return };
        let _ = b.encerrar_orfas();
        if let Ok(l) = b.listar() {
            self.linhas = l;
        }
        if let Ok(h) = b.historico(None, 40) {
            self.historico = h;
        }
        if let Ok(a) = b.aguardando() {
            self.aguardando = a;
        }
        self.escritos.reler(b);
    }

    /// Junta o que as threads devolveram.
    fn andamento(&mut self) {
        self.escritos.andamento();
        let mut feitos = Vec::new();
        for (i, e) in self.em_curso.iter().enumerate() {
            if let Ok(r) = e.resposta.try_recv() {
                feitos.push((i, e.roteiro.clone(), r));
            }
        }
        for (i, nome, r) in feitos.into_iter().rev() {
            self.em_curso.remove(i);
            self.aviso = Some(match r {
                Ok(t) => (format!("{nome}: {t}"), false),
                Err(e) => (format!("{nome}: {e}"), true),
            });
            self.lido_em = None;
        }
        let mut mensagens = Vec::new();
        if let Some(rx) = &self.da_gravacao {
            while let Ok(m) = rx.try_recv() {
                mensagens.push(m);
            }
        }
        for m in mensagens {
            match m {
                DaGravacao::Proposta(p) => {
                    if let Some(g) = &mut self.gravando {
                        g.proposta = Some(p);
                    }
                }
                DaGravacao::Terminou(r) => {
                    if let Some(g) = &mut self.gravando {
                        g.proposta = None;
                        g.terminou = Some(r);
                    }
                    self.para_a_gravacao = None;
                    self.da_gravacao = None;
                    self.lido_em = None;
                }
            }
        }
    }

    fn executar(&mut self, linha: &LinhaDeRoteiro) {
        let Some(amb) = self.amb.clone() else {
            self.aviso = Some((
                "Na demonstração os roteiros não rodam: não há navegador nem site.".into(),
                false,
            ));
            return;
        };
        if self.em_curso.iter().any(|e| e.roteiro == linha.nome) {
            return;
        }
        let Some(chaveiro) = self.chaveiro.clone() else {
            self.aviso = Some((
                "O chaveiro não abriu: sem ele o roteiro não tem a senha do site.".into(),
                true,
            ));
            return;
        };
        let ia = self.ia.clone();
        let cancelar = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();
        let id = linha.id;
        let c2 = Arc::clone(&cancelar);
        std::thread::spawn(move || {
            let pronta = ia.as_ref().and_then(|i| i.pronta().ok());
            let guia = pronta.as_ref().map(|(provedor, registro)| Guia {
                provedor: provedor.as_ref(),
                registro: Some(registro.as_ref()),
                destinatario: "roteiros".to_owned(),
                nivel: NivelDeConfianca::Mascarado,
                rotulos_de_senha: chaveiro.rotulos_de_site(),
                tem_cpf: chaveiro.tem_cpf(),
                segredos: Vec::new(),
            });
            let r = executar_roteiro(&amb, id, "janela", &chaveiro, guia.as_ref(), &c2);
            let _ = tx.send(match r {
                Ok(rel) => match rel.fim {
                    Fim::Concluida => Ok(format!(
                        "concluída: {} baixado{}, {} novo{} no inventário",
                        rel.baixados,
                        if rel.baixados == 1 { "" } else { "s" },
                        rel.novos,
                        if rel.novos == 1 { "" } else { "s" }
                    )),
                    Fim::Falhou { passo, motivo } => {
                        Err(format!("falhou no passo {}: {motivo}", passo + 1))
                    }
                    Fim::Bloqueada { passo, bloqueio } => {
                        Err(format!("bloqueada no passo {}: {bloqueio}", passo + 1))
                    }
                    Fim::Cancelada => Err("cancelada".to_owned()),
                    Fim::PausaExpirou { .. } => Err("você não voltou a tempo".to_owned()),
                },
                Err(e) => Err(e.to_string()),
            });
        });
        self.em_curso.push(EmCurso {
            roteiro: linha.nome.clone(),
            cancelar,
            resposta: rx,
        });
    }

    fn comecar_a_gravar(&mut self) {
        let Some(amb) = self.amb.clone() else {
            self.aviso = Some(("Na demonstração não há como gravar.".into(), false));
            return;
        };
        let (Some(chaveiro), Some(ia)) = (self.chaveiro.clone(), self.ia.clone()) else {
            self.aviso = Some((
                "Gravar precisa do chaveiro aberto e da IA ligada (Opções › IA).".into(),
                true,
            ));
            return;
        };
        let f = std::mem::take(&mut self.formulario);
        let (para_a_janela, da_gravacao) = channel();
        let (para_a_gravacao, da_janela) = channel();
        let cancelar = Arc::new(AtomicBool::new(false));
        let aprovador = AprovadorDaJanela {
            para_a_janela: para_a_janela.clone(),
            da_janela,
            cancelar: Arc::clone(&cancelar),
        };
        self.gravando = Some(Gravando {
            nome: f.nome.clone(),
            ..Gravando::default()
        });
        self.para_a_gravacao = Some(para_a_gravacao);
        self.da_gravacao = Some(da_gravacao);
        self.cancelar_a_gravacao = Some(Arc::clone(&cancelar));
        std::thread::spawn(move || {
            let mut aprovador = aprovador;
            let resultado = (|| -> Result<String, String> {
                let (provedor, registro) = ia.pronta()?;
                let guia = Guia {
                    provedor: provedor.as_ref(),
                    registro: Some(registro.as_ref()),
                    destinatario: "roteiros".to_owned(),
                    nivel: NivelDeConfianca::Mascarado,
                    rotulos_de_senha: chaveiro.rotulos_de_site(),
                    tem_cpf: chaveiro.tem_cpf(),
                    segredos: Vec::new(),
                };
                let rotulo =
                    (!f.rotulo_de_senha.trim().is_empty()).then_some(f.rotulo_de_senha.trim());
                let (id, fim) = gravar_novo(
                    &amb,
                    f.nome.trim(),
                    f.url.trim(),
                    rotulo,
                    f.objetivo.trim(),
                    &chaveiro,
                    &guia,
                    &mut aprovador,
                    &cancelar,
                )
                .map_err(|e| e.to_string())?;
                Ok(match (id, fim) {
                    (0, _) => "Nada foi gravado além de abrir a página; o roteiro não foi criado."
                        .to_owned(),
                    (_, FimDaGravacao::Cumprido) => {
                        "Roteiro gravado: a IA considerou o objetivo cumprido.".to_owned()
                    }
                    (_, _) => "Roteiro gravado com os passos aceitos até aqui.".to_owned(),
                })
            })();
            let _ = para_a_janela.send(DaGravacao::Terminou(resultado));
        });
    }

    /// Desenha a área.
    pub fn area(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        if !self.ligado() {
            componentes::titulo(ui, "Roteiros", 20.0);
            ui.add_space(6.0);
            ui.label(
                RichText::new("Os roteiros só existem no inventário de verdade (fora da demonstração), com o chaveiro aberto.")
                    .color(p.tinta_suave),
            );
            return;
        }
        self.andamento();
        let mudou_ha = self.lido_em.map(|t| t.elapsed());
        if self.amb.is_some() && mudou_ha.is_none_or(|t| t > Duration::from_secs(1)) {
            self.reler();
            self.lido_em = Some(Instant::now());
        }
        if self.amb.is_some() {
            ui.ctx().request_repaint_after(Duration::from_millis(800));
        }
        ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            ui.horizontal(|ui| {
                componentes::titulo(ui, "Roteiros", 20.0);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui
                        .add_enabled(self.gravando.is_none(), egui::Button::new("Gravar novo (guiado)"))
                        .clicked()
                    {
                        self.formulario.aberto = true;
                    }
                });
            });
            ui.label(
                RichText::new(
                    "Conexões › Roteiros. Um roteiro navega num site sem API, num Chrome com perfil só do SBF, para consultar e baixar documentos. Ele nunca paga, transfere, contrata nem altera cadastro: a lista de bloqueio barra esses botões no navegador e no executor.",
                )
                .size(12.5)
                .color(p.tinta_suave),
            );
            ui.add_space(8.0);
            if let Some((t, erro)) = &self.aviso {
                if *erro {
                    componentes::aviso(ui, "error", t, p.perigo_sutil, p.borda, p.perigo);
                } else {
                    componentes::aviso(ui, "help", t, p.info_sutil, p.info_borda, p.info);
                }
                ui.add_space(6.0);
            }
            self.sua_vez(ui);
            self.escritos.sua_vez_ui(ui, self.banco.as_ref());
            self.formulario_ui(ui);
            self.gravacao_ui(ui);
            self.lista(ui);
            self.escritos
                .ui(ui, self.banco.as_ref(), self.amb.as_ref(), self.chaveiro.as_ref());
            self.historico_ui(ui);
        });
    }

    fn sua_vez(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        let esperando = self.aguardando.clone();
        for e in esperando {
            componentes::cartao(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("Sua vez no navegador")
                            .strong()
                            .color(p.aviso),
                    );
                    ui.label(format!("· {}", e.roteiro));
                });
                ui.label(
                    RichText::new(format!(
                        "{}. Resolva no Chrome que o SBF abriu (digite o código, marque a caixa) e depois clique em Continuar: o roteiro retoma do passo onde parou.",
                        e.motivo.clone().unwrap_or_else(|| "O site precisa de você".into())
                    ))
                    .color(p.tinta),
                );
                ui.horizontal(|ui| {
                    if ui.button("Continuar").clicked() {
                        match &self.banco {
                            Some(b) => {
                                let _ = b.pedir_continuar(e.id, true);
                            }
                            None => self.aguardando.retain(|x| x.id != e.id),
                        }
                    }
                    if ui.button("Cancelar a execução").clicked() {
                        match &self.banco {
                            Some(b) => {
                                let _ = b.pedir_continuar(e.id, false);
                            }
                            None => self.aguardando.retain(|x| x.id != e.id),
                        }
                    }
                });
            });
            ui.add_space(6.0);
        }
    }

    fn formulario_ui(&mut self, ui: &mut Ui) {
        if !self.formulario.aberto {
            return;
        }
        let p = tema::paleta(ui.ctx());
        let rotulos = self.rotulos();
        let mut comecar = false;
        let mut fechar = false;
        componentes::cartao(ui, |ui| {
            componentes::titulo(ui, "Gravar um roteiro novo", 15.0);
            ui.label(
                RichText::new("A IA olha a página (sem senhas nem valores de campos) e propõe um passo de cada vez; você aceita ou recusa. Pronto, o roteiro roda sem IA.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
            egui::Grid::new("form-roteiro")
                .num_columns(2)
                .spacing([12.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Nome");
                    ui.text_edit_singleline(&mut self.formulario.nome);
                    ui.end_row();
                    ui.label("Endereço do site");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.formulario.url).desired_width(360.0),
                    );
                    ui.end_row();
                    ui.label("Senha do chaveiro");
                    egui::ComboBox::from_id_salt("rotulo-de-senha")
                        .selected_text(if self.formulario.rotulo_de_senha.is_empty() {
                            "(sem senha)".to_owned()
                        } else {
                            self.formulario.rotulo_de_senha.clone()
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(
                                &mut self.formulario.rotulo_de_senha,
                                String::new(),
                                "(sem senha)",
                            );
                            for r in &rotulos {
                                ui.selectable_value(
                                    &mut self.formulario.rotulo_de_senha,
                                    r.clone(),
                                    r,
                                );
                            }
                        });
                    ui.end_row();
                    ui.label("O que baixar");
                    ui.add(
                        egui::TextEdit::singleline(&mut self.formulario.objetivo)
                            .desired_width(360.0),
                    );
                    ui.end_row();
                });
            ui.label(
                RichText::new("Guarde antes a senha do site em Opções › Chaveiro › Senhas de sites. O CPF é o do chaveiro.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
            let pronto = !self.formulario.nome.trim().is_empty()
                && self.formulario.url.trim().starts_with("http")
                && !self.formulario.objetivo.trim().is_empty();
            ui.horizontal(|ui| {
                comecar = ui
                    .add_enabled(pronto, egui::Button::new("Começar a gravar"))
                    .clicked();
                fechar = ui.button("Cancelar").clicked();
            });
        });
        ui.add_space(8.0);
        if fechar {
            self.formulario = Formulario::default();
        }
        if comecar {
            self.comecar_a_gravar();
        }
    }

    fn gravacao_ui(&mut self, ui: &mut Ui) {
        let Some(g) = &mut self.gravando else { return };
        let p = tema::paleta(ui.ctx());
        let mut decisao: Option<Aprovacao> = None;
        let mut fechar = false;
        componentes::cartao(ui, |ui| {
            ui.horizontal(|ui| {
                componentes::titulo(ui, &format!("Gravando: {}", g.nome), 15.0);
                if g.terminou.is_none() {
                    ui.spinner();
                }
            });
            if g.aceitos.is_empty() {
                ui.label(RichText::new("Nenhum passo aceito ainda.").color(p.tinta_suave));
            }
            for (i, a) in g.aceitos.iter().enumerate() {
                ui.label(format!("{}. {a}", i + 1));
            }
            match (&g.proposta, &g.terminou) {
                (
                    Some(Proposta::Passo {
                        passo,
                        explicacao,
                        alvo,
                    }),
                    None,
                ) => {
                    ui.add_space(6.0);
                    componentes::aviso(
                        ui,
                        "route",
                        &format!("A IA propõe: {}. {explicacao}", texto_do_passo(passo)),
                        p.info_sutil,
                        p.info_borda,
                        p.info,
                    );
                    if !alvo.is_empty() {
                        ui.label(
                            RichText::new(format!("No elemento da página: {alvo}"))
                                .size(12.5)
                                .color(p.tinta),
                        );
                    }
                    ui.horizontal(|ui| {
                        if ui.button("Aceitar e executar").clicked() {
                            g.aceitos.push(texto_do_passo(passo));
                            decisao = Some(Aprovacao::Aceitar);
                        }
                        ui.add(
                            egui::TextEdit::singleline(&mut g.motivo_da_recusa)
                                .hint_text("por que não (opcional)")
                                .desired_width(220.0),
                        );
                        if ui.button("Recusar").clicked() {
                            let m = if g.motivo_da_recusa.trim().is_empty() {
                                "o usuário recusou este passo".to_owned()
                            } else {
                                g.motivo_da_recusa.trim().to_owned()
                            };
                            g.motivo_da_recusa.clear();
                            decisao = Some(Aprovacao::Recusar(m));
                        }
                        if ui.button("Terminar e salvar").clicked() {
                            decisao = Some(Aprovacao::Parar);
                        }
                    });
                }
                (Some(Proposta::Pausa { motivo }), None) => {
                    ui.add_space(6.0);
                    componentes::aviso(
                        ui,
                        "help",
                        &format!("A IA pede a sua vez no navegador: {motivo}"),
                        p.aviso_sutil,
                        p.aviso_borda,
                        p.aviso,
                    );
                    ui.horizontal(|ui| {
                        if ui.button("Passar a vez ao navegador").clicked() {
                            decisao = Some(Aprovacao::Aceitar);
                        }
                        if ui.button("Terminar e salvar").clicked() {
                            decisao = Some(Aprovacao::Parar);
                        }
                    });
                }
                (_, None) => {
                    ui.label(RichText::new("A IA está olhando a página…").color(p.tinta_suave));
                }
                (_, Some(r)) => {
                    match r {
                        Ok(t) => {
                            componentes::aviso(ui, "help", t, p.info_sutil, p.info_borda, p.info)
                        }
                        Err(e) => {
                            componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo)
                        }
                    }
                    fechar = ui.button("Fechar").clicked();
                }
            }
        });
        ui.add_space(8.0);
        if let Some(d) = decisao {
            if let Some(g) = &mut self.gravando {
                g.proposta = None;
            }
            match &self.para_a_gravacao {
                Some(tx) => {
                    let _ = tx.send(d);
                }
                None => {
                    // demonstração: só recolhe a proposta
                }
            }
        }
        if fechar {
            self.gravando = None;
        }
    }

    fn lista(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        componentes::rotulo_de_secao(ui, "Seus roteiros");
        if self.linhas.is_empty() {
            ui.label(
                RichText::new("Nenhum roteiro ainda. Use Gravar novo (guiado).")
                    .color(p.tinta_suave),
            );
        }
        let linhas = self.linhas.clone();
        for l in linhas {
            let rodando = self.em_curso.iter().any(|e| e.roteiro == l.nome)
                || l.ultima.as_ref().is_some_and(|u| {
                    matches!(
                        u.estado,
                        EstadoDaExecucao::Rodando | EstadoDaExecucao::AguardandoUsuario
                    )
                });
            componentes::cartao(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&l.nome).strong().size(15.0));
                    if let Some(u) = &l.ultima {
                        selo_da_execucao(ui, u.estado);
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if ui
                            .add_enabled(!rodando, egui::Button::new("Executar"))
                            .clicked()
                        {
                            self.executar(&l);
                        }
                        if ui.button("Histórico").clicked() {
                            self.selecionado = if self.selecionado == Some(l.id) {
                                None
                            } else {
                                Some(l.id)
                            };
                        }
                    });
                });
                ui.label(
                    RichText::new(format!(
                        "{} · {} passos · versão {} · senha do chaveiro: {}",
                        l.descricao,
                        l.passos,
                        l.versao,
                        l.rotulo_senha.as_deref().unwrap_or("nenhuma")
                    ))
                    .size(12.5)
                    .color(p.tinta_suave),
                );
                match &l.ultima {
                    Some(u) => {
                        let mut t = format!(
                            "Última execução: {} · {}",
                            u.iniciada_em.replace('T', " "),
                            u.estado.rotulo()
                        );
                        if u.estado == EstadoDaExecucao::Concluida {
                            t.push_str(&format!(
                                " · {} baixado{}, {} novo{}",
                                u.baixados,
                                if u.baixados == 1 { "" } else { "s" },
                                u.novos,
                                if u.novos == 1 { "" } else { "s" }
                            ));
                        }
                        ui.label(t);
                        if let (Some(m), true) =
                            (&u.motivo, !matches!(u.estado, EstadoDaExecucao::Concluida))
                        {
                            ui.label(RichText::new(m).size(12.5).color(p.tinta_suave));
                        }
                    }
                    None => {
                        ui.label(RichText::new("Nunca executado.").color(p.tinta_suave));
                    }
                }
                ui.horizontal(|ui| {
                    let mut agendado = l.agenda_horas.is_some();
                    let mut horas = l.agenda_horas.unwrap_or(24);
                    let a = ui.checkbox(&mut agendado, "Executar sozinho (no sbf worker) a cada");
                    let b = ui.add_enabled(
                        agendado,
                        egui::DragValue::new(&mut horas).range(1..=720).suffix(" h"),
                    );
                    if a.changed() || b.changed() {
                        match &self.banco {
                            Some(banco) => {
                                let _ = banco.definir_agenda(l.id, agendado.then_some(horas));
                                self.lido_em = None;
                            }
                            None => {
                                if let Some(x) = self.linhas.iter_mut().find(|x| x.id == l.id) {
                                    x.agenda_horas = agendado.then_some(horas);
                                }
                            }
                        }
                    }
                });
                if l.ha_proposta {
                    self.conserto_ui(ui, &l);
                }
            });
            ui.add_space(6.0);
        }
    }

    fn conserto_ui(&mut self, ui: &mut Ui, l: &LinhaDeRoteiro) {
        let p = tema::paleta(ui.ctx());
        if self.conserto.as_ref().is_none_or(|(id, _, _)| *id != l.id)
            && let Some(b) = &self.banco
            && let Ok(Some(prop)) = b.proposta(l.id)
            && let Ok((_, _, atual)) = b.versao_ativa(l.id)
        {
            let linhas = diferenca(&atual.passos, &prop.passos);
            self.conserto = Some((l.id, prop, linhas));
        }
        let Some((_, prop, diff)) = self.conserto.clone().filter(|(id, _, _)| *id == l.id) else {
            componentes::aviso(
                ui,
                "help",
                "A IA propôs um conserto para este roteiro.",
                p.aviso_sutil,
                p.aviso_borda,
                p.aviso,
            );
            return;
        };
        componentes::aviso(
            ui,
            "help",
            &format!(
                "O site mudou e a IA propôs um conserto (versão {}): {} Ele só vale se você aprovar.",
                prop.numero, prop.motivo
            ),
            p.aviso_sutil,
            p.aviso_borda,
            p.aviso,
        );
        for linha in &diff {
            let cor = if linha.starts_with('!') {
                p.perigo
            } else if linha.starts_with('+') {
                p.sucesso
            } else if linha.starts_with('−') {
                p.perigo
            } else {
                p.tinta_suave
            };
            ui.label(RichText::new(linha).monospace().size(12.0).color(cor));
        }
        ui.horizontal(|ui| {
            if ui.button("Aprovar o conserto").clicked() {
                match &self.banco {
                    Some(b) => {
                        let _ = b.aprovar_proposta(l.id);
                        self.lido_em = None;
                    }
                    None => {
                        if let Some(x) = self.linhas.iter_mut().find(|x| x.id == l.id) {
                            x.ha_proposta = false;
                            x.versao = prop.numero;
                        }
                    }
                }
                self.conserto = None;
            }
            if ui.button("Recusar").clicked() {
                match &self.banco {
                    Some(b) => {
                        let _ = b.rejeitar_proposta(l.id);
                        self.lido_em = None;
                    }
                    None => {
                        if let Some(x) = self.linhas.iter_mut().find(|x| x.id == l.id) {
                            x.ha_proposta = false;
                        }
                    }
                }
                self.conserto = None;
            }
        });
    }

    fn historico_ui(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        ui.add_space(6.0);
        let titulo = match self
            .selecionado
            .and_then(|id| self.linhas.iter().find(|l| l.id == id))
        {
            Some(l) => format!("Histórico · {}", l.nome),
            None => "Histórico de execuções".to_owned(),
        };
        componentes::rotulo_de_secao(ui, &titulo);
        let itens: Vec<&LinhaDeExecucao> = self
            .historico
            .iter()
            .filter(|h| self.selecionado.is_none_or(|id| h.roteiro_id == id))
            .collect();
        if itens.is_empty() {
            ui.label(RichText::new("Nenhuma execução ainda.").color(p.tinta_suave));
            return;
        }
        egui::Grid::new("historico-roteiros")
            .num_columns(6)
            .striped(true)
            .spacing([14.0, 4.0])
            .show(ui, |ui| {
                for c in [
                    "Quando",
                    "Roteiro",
                    "Como",
                    "Resultado",
                    "Baixados",
                    "Motivo",
                ] {
                    ui.label(RichText::new(c).size(11.5).color(p.tinta_suave));
                }
                ui.end_row();
                for h in itens {
                    ui.label(h.iniciada_em.replace('T', " "));
                    ui.label(&h.roteiro);
                    ui.label(match h.gatilho.as_str() {
                        "agenda" => "agendada",
                        "gravacao" => "gravação",
                        _ => "pela janela",
                    });
                    selo_da_execucao(ui, h.estado);
                    ui.label(format!(
                        "{} ({} novo{})",
                        h.baixados,
                        h.novos,
                        if h.novos == 1 { "" } else { "s" }
                    ));
                    ui.add(
                        egui::Label::new(
                            RichText::new(h.motivo.clone().unwrap_or_default())
                                .size(12.0)
                                .color(p.tinta_suave),
                        )
                        .truncate(),
                    );
                    ui.end_row();
                }
            });
    }
}

fn selo_da_execucao(ui: &mut Ui, estado: EstadoDaExecucao) {
    let p = tema::paleta(ui.ctx());
    let (fundo, cor) = match estado {
        EstadoDaExecucao::Concluida => (p.sucesso_sutil, p.sucesso),
        EstadoDaExecucao::Falhou | EstadoDaExecucao::Bloqueada => (p.perigo_sutil, p.perigo),
        EstadoDaExecucao::AguardandoUsuario => (p.aviso_sutil, p.aviso),
        EstadoDaExecucao::Rodando => (p.info_sutil, p.info),
        EstadoDaExecucao::Cancelada => (p.superficie_3, p.tinta_suave),
    };
    componentes::selo(ui, estado.rotulo(), fundo, cor);
}

/// As linhas do "antes e depois" de um conserto: o que sai com `−`, o que entra com `+` e, com
/// `!`, o passo que leva a um **site novo** (o usuário confere o endereço inteiro antes de
/// aprovar).
fn diferenca(antes: &[Passo], depois: &[Passo]) -> Vec<String> {
    sbf_roteiros::passo::diferenca(antes, depois)
        .into_iter()
        .map(|l| match (l.marca, l.host_novo) {
            (Marca::Igual, _) => format!("   {}", l.texto),
            (Marca::Removido, _) => format!("−  {}", l.texto),
            (Marca::Novo, Some(host)) => format!("!  SITE NOVO ({host}): {}", l.texto),
            (Marca::Novo, None) => format!("+  PASSO NOVO: {}", l.texto),
        })
        .collect()
}

/// O estado do formulário de senhas de sites (Opções › Chaveiro). Os campos são zerados assim
/// que a senha é guardada.
#[derive(Debug, Default)]
pub struct SenhasDeSites {
    rotulo: String,
    senha: String,
    aviso: Option<String>,
}

impl SenhasDeSites {
    /// A seção "Senhas de sites" do chaveiro: lista os rótulos (nunca os valores), guarda e
    /// apaga.
    pub fn ui(&mut self, ui: &mut Ui, chaveiro: &Chaveiro) {
        let p = tema::paleta(ui.ctx());
        componentes::titulo(ui, "Senhas de sites (roteiros)", 15.0);
        let rotulos = chaveiro.rotulos_de_site();
        ui.label(format!(
            "Senhas de sites ({}): {}",
            rotulos.len(),
            if rotulos.is_empty() {
                "nenhuma".to_owned()
            } else {
                rotulos.join(", ")
            }
        ));
        ui.label(
            RichText::new("Só os rótulos aparecem. A senha vai do chaveiro direto para o campo do site, em memória; nunca para o roteiro, a IA ou o log.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        ui.label("Rótulo (por exemplo, caixa-fgts)");
        ui.text_edit_singleline(&mut self.rotulo);
        ui.label("Senha do site");
        ui.add(egui::TextEdit::singleline(&mut self.senha).password(true));
        ui.horizontal(|ui| {
            let pronto = !self.rotulo.trim().is_empty() && !self.senha.is_empty();
            if ui
                .add_enabled(pronto, egui::Button::new("Guardar senha do site"))
                .clicked()
            {
                let r = chaveiro
                    .guardar_senha_de_site(self.rotulo.trim(), Segredo::novo(self.senha.clone()));
                self.senha.zeroize();
                self.aviso = Some(match r {
                    Ok(()) => format!("Guardada: {}.", self.rotulo.trim()),
                    Err(e) => format!("Não deu para guardar: {e}"),
                });
                self.rotulo.clear();
            }
            let apagavel = rotulos.iter().any(|r| *r == self.rotulo.trim());
            if ui
                .add_enabled(apagavel, egui::Button::new("Apagar esse rótulo"))
                .clicked()
            {
                self.aviso = Some(match chaveiro.apagar_senha_de_site(self.rotulo.trim()) {
                    Ok(()) => "Apagada.".to_owned(),
                    Err(e) => format!("Não deu para apagar: {e}"),
                });
                self.senha.zeroize();
            }
        });
        if let Some(a) = &self.aviso {
            ui.label(RichText::new(a).size(12.0));
        }
    }

    /// A mesma seção na demonstração: só os rótulos inventados.
    pub fn ui_demonstracao(&mut self, ui: &mut Ui, rotulos: &[String]) {
        let p = tema::paleta(ui.ctx());
        componentes::titulo(ui, "Senhas de sites (roteiros)", 15.0);
        ui.label(format!(
            "Senhas de sites ({}): {}",
            rotulos.len(),
            rotulos.join(", ")
        ));
        ui.label(
            RichText::new("Só os rótulos aparecem. A senha vai do chaveiro direto para o campo do site, em memória; nunca para o roteiro, a IA ou o log.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        ui.label("Rótulo");
        ui.add_enabled(false, egui::TextEdit::singleline(&mut self.rotulo));
        ui.label("Senha do site");
        ui.add_enabled(
            false,
            egui::TextEdit::singleline(&mut self.senha).password(true),
        );
        ui.add_enabled(false, egui::Button::new("Guardar senha do site"));
    }
}

impl Roteiros {
    /// A seção "Senhas de sites" do chaveiro na demonstração.
    pub fn senhas_da_demonstracao_ui(&mut self, ui: &mut Ui) {
        let rotulos = self.rotulos_da_demonstracao.clone();
        self.senhas_da_demonstracao.ui_demonstracao(ui, &rotulos);
    }

    /// Os rótulos da demonstração (para a seção do chaveiro).
    #[must_use]
    pub fn rotulos_da_demonstracao(&self) -> &[String] {
        &self.rotulos_da_demonstracao
    }

    /// Cancela o que estiver em curso (ao fechar a janela).
    pub fn cancelar_tudo(&mut self) {
        for e in &self.em_curso {
            e.cancelar.store(true, Ordering::SeqCst);
        }
        if let Some(c) = &self.cancelar_a_gravacao {
            c.store(true, Ordering::SeqCst);
        }
        self.escritos.cancelar_tudo();
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_conserto_destaca_o_site_novo_e_mostra_a_url_inteira() {
        let antes = vec![Passo::Navegar {
            url: "https://banco.example/login".into(),
        }];
        let depois = vec![
            Passo::Navegar {
                url: "https://banco.example/login".into(),
            },
            Passo::Navegar {
                url: "https://outro.example/x?token=1".into(),
            },
            Passo::Clicar {
                seletor: "#baixar".into(),
            },
        ];
        let linhas = diferenca(&antes, &depois);
        assert!(linhas[0].starts_with("   "));
        assert!(linhas[1].starts_with('!'), "{linhas:?}");
        assert!(linhas[1].contains("SITE NOVO (outro.example)"));
        assert!(linhas[1].contains("token=1"), "a URL vai inteira");
        assert!(linhas[2].starts_with("+  PASSO NOVO"));
    }
}
