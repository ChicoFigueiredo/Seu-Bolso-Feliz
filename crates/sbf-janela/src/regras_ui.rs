//! A área **Regras de reconhecimento** (spec 13, D12): a lista, a página da regra com o
//! editor, os exemplos, a simulação, as versões e o Rastro. Toda a lógica mora em
//! [`crate::regras`]; aqui só se desenha e se chama a costura.
//!
//! Ativar, consolidar, reverter e desativar são do usuário: cada um confirma antes (D14).
//! Um exemplo que falha bloqueia a ativação com a lista do que falhou (D12).

use egui::{Align, Layout, RichText, ScrollArea, TextEdit, Ui};
use sbf_regras::catalogo::{
    Esperado, ExemploDeRegra, RelatorioDosExemplos, ResumoDaRegra, VersaoDeRegra,
};
use sbf_regras::{Estado, LIMIAR_DE_CONSOLIDACAO};

use crate::regras::{
    FonteDasRegras, LinhaDoRastro, SimulacaoDaRegra, definicao_em_texto, rotulo_do_autor,
    rotulo_do_estado, validar_definicao,
};
use crate::{componentes, icones, tema};

/// O destino da área na navegação.
pub const ROTA: &str = "regras";

/// O título completo (o rótulo da lateral pode abreviar).
pub const TITULO: &str = "Regras de reconhecimento";

/// O que o editor guarda enquanto você digita.
#[derive(Debug, Default, Clone)]
struct Editor {
    nome: String,
    motivo: String,
    definicao: String,
    script: String,
    /// A validação ao vivo: o que está errado, ou `None` se está tudo certo.
    erro_da_definicao: Option<String>,
    /// `Some(true)` verificado pelo executor, `Some(false)` sem executor, `None` sem script.
    script_verificado: Option<bool>,
    erro_do_script: Option<String>,
}

#[derive(Debug, Default, Clone)]
struct NovoExemplo {
    aberto: bool,
    nome: String,
    texto: String,
    esperado: String,
    erro: Option<String>,
}

/// O formulário "Nova regra".
#[derive(Debug, Default, Clone)]
struct NovaRegra {
    id: String,
    nome: String,
    tipo_documental: String,
    erro: Option<String>,
}

/// Uma regra aberta, com tudo o que a página mostra.
#[derive(Debug)]
struct Pagina {
    regra: String,
    resumo: ResumoDaRegra,
    versoes: Vec<VersaoDeRegra>,
    exemplos: Vec<ExemploDeRegra>,
    rastro: Vec<LinhaDoRastro>,
    /// A versão aberta no editor, nos testes, na simulação e na ativação.
    selecionada: u32,
    editor: Editor,
    relatorio: Option<RelatorioDosExemplos>,
    simulacao: Option<Result<SimulacaoDaRegra, String>>,
    /// A versão cuja ativação espera confirmação.
    confirmando_ativar: Option<u32>,
    confirmando_consolidar: bool,
    novo: NovoExemplo,
    /// A versão sugerida para consolidar, se há.
    sugerida: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct Mensagem {
    erro: bool,
    texto: String,
}

/// O estado da área. Fica em [`crate::Janela::regras`].
#[derive(Debug, Default)]
pub struct Regras {
    fonte: Option<FonteDasRegras>,
    lista: Option<Result<Vec<ResumoDaRegra>, String>>,
    sugestoes: Vec<(String, u32)>,
    pagina: Option<Pagina>,
    mensagem: Option<Mensagem>,
    /// O formulário "Nova regra", enquanto aberto.
    nova: Option<NovaRegra>,
    /// Quanto rolar a página no próximo quadro (as capturas usam).
    pub rolar_para: Option<f32>,
}

impl Regras {
    /// Liga o catálogo (e o executor) à janela.
    pub fn ligar(&mut self, fonte: FonteDasRegras) {
        self.fonte = Some(fonte);
        self.lista = None;
        self.pagina = None;
    }

    /// O catálogo ligado, se há (o Reconhecimento fala com ele).
    pub fn fonte(&self) -> Option<&FonteDasRegras> {
        self.fonte.as_ref()
    }

    /// Mostra uma mensagem no alto da área (o Reconhecimento avisa o que criou).
    pub fn avisar(&mut self, erro: bool, texto: &str) {
        self.mensagem = Some(Mensagem {
            erro,
            texto: texto.to_owned(),
        });
    }

    /// O catálogo está ligado?
    pub fn ligada(&self) -> bool {
        self.fonte.is_some()
    }

    /// Abre a página de uma regra (a lista continua um clique atrás).
    pub fn abrir(&mut self, regra: &str) {
        self.mensagem = None;
        self.pagina = self.carregar_pagina(regra, None);
    }

    /// Abre o formulário "Nova regra" (o botão da lista).
    pub fn abrir_nova_regra(&mut self) {
        self.nova = Some(NovaRegra::default());
    }

    /// Volta para a lista.
    pub fn voltar(&mut self) {
        self.pagina = None;
        self.recarregar_lista();
    }

    /// A regra aberta, se há.
    pub fn regra_aberta(&self) -> Option<&str> {
        self.pagina.as_ref().map(|p| p.regra.as_str())
    }

    /// Roda a simulação da versão escolhida (o botão Simular).
    pub fn simular(&mut self) {
        let (Some(f), Some(p)) = (self.fonte.clone(), self.pagina.as_mut()) else {
            return;
        };
        p.simulacao = Some(f.simular(&p.regra, p.selecionada));
    }

    /// Escolhe a versão da página aberta (editor, testes e ativação passam a ela).
    pub fn selecionar_versao(&mut self, versao: u32) {
        let (Some(f), Some(p)) = (self.fonte.clone(), self.pagina.as_mut()) else {
            return;
        };
        p.selecionar(&f, versao);
    }

    fn recarregar_lista(&mut self) {
        let Some(f) = &self.fonte else { return };
        self.lista = Some(f.resumos());
        self.sugestoes = f.sugestoes_de_consolidacao().unwrap_or_default();
    }

    fn carregar_pagina(&self, regra: &str, versao: Option<u32>) -> Option<Pagina> {
        let f = self.fonte.as_ref()?;
        let resumo = f.resumos().ok()?.into_iter().find(|r| r.id == regra)?;
        let versoes = f.versoes(regra).ok()?;
        let selecionada = versao
            .filter(|v| versoes.iter().any(|x| x.versao == *v))
            .or(resumo.versao_em_vigor)
            .or_else(|| versoes.last().map(|v| v.versao))?;
        let mut p = Pagina {
            regra: regra.to_owned(),
            resumo,
            versoes,
            exemplos: Vec::new(),
            rastro: Vec::new(),
            selecionada,
            editor: Editor::default(),
            relatorio: None,
            simulacao: None,
            confirmando_ativar: None,
            confirmando_consolidar: false,
            novo: NovoExemplo::default(),
            sugerida: None,
        };
        p.atualizar(f);
        p.selecionar(f, selecionada);
        Some(p)
    }

    /// Depois de uma ação: recarrega lista e página, mantendo a versão escolhida.
    fn depois_da_acao(&mut self, resultado: Result<String, String>) {
        self.mensagem = Some(match resultado {
            Ok(texto) => Mensagem { erro: false, texto },
            Err(texto) => Mensagem { erro: true, texto },
        });
        self.recarregar_lista();
        if let Some(p) = &self.pagina {
            let (regra, versao) = (p.regra.clone(), p.selecionada);
            let novo = self.carregar_pagina(&regra, Some(versao));
            if let (Some(mut novo), Some(velho)) = (novo, self.pagina.take()) {
                // Não perde o que o usuário digitou no formulário do exemplo.
                novo.novo = velho.novo;
                if novo.selecionada == velho.selecionada {
                    novo.simulacao = velho.simulacao;
                }
                self.pagina = Some(novo);
            }
        }
    }

    /// Desenha a área.
    pub fn area(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        if self.fonte.is_none() {
            componentes::fatia_futura(
                ui,
                "rule",
                TITULO,
                3,
                "O catálogo de regras (regras.sqlite) ainda não está ligado a esta janela.",
            );
            return;
        }
        if self.lista.is_none() {
            self.recarregar_lista();
        }
        let rolar = self.rolar_para.take();
        let mut area = ScrollArea::vertical()
            .id_salt("regras")
            .auto_shrink([false, false]);
        if let Some(y) = rolar {
            area = area.vertical_scroll_offset(y);
        }
        area.show(ui, |ui| {
            if self.pagina.is_some() {
                self.desenhar_pagina(ui);
            } else {
                self.desenhar_lista(ui);
            }
        });
        let _ = p;
        self.dialogo_da_nova_regra(ui.ctx());
    }

    /// "Nova regra": id, nome, tipo documental e uma definição mínima (rascunho; nada é
    /// ativado). Depois de criar, abre a página para o usuário editar.
    fn dialogo_da_nova_regra(&mut self, ctx: &egui::Context) {
        let (Some(fonte), Some(n)) = (self.fonte.clone(), self.nova.as_mut()) else {
            return;
        };
        let p = tema::paleta(ctx);
        let mut criar = false;
        let mut fechar = false;
        let resposta = egui::Modal::new(egui::Id::new("regras-nova")).show(ctx, |ui| {
            ui.set_width(520.0);
            componentes::titulo(ui, "Nova regra", 17.0);
            texto_pequeno(
                ui,
                "Nasce em rascunho, com uma definição mínima para você completar na página da regra. Nada é ativado.",
            );
            ui.add_space(8.0);
            egui::Grid::new("regras-nova-campos")
                .num_columns(2)
                .spacing([14.0, 6.0])
                .show(ui, |ui| {
                    ui.label("Id");
                    ui.add(
                        TextEdit::singleline(&mut n.id)
                            .hint_text("comprovante-pix-banco-exemplo")
                            .desired_width(300.0),
                    );
                    ui.end_row();
                    ui.label("Nome");
                    ui.add(
                        TextEdit::singleline(&mut n.nome)
                            .hint_text("Comprovante Pix · Banco Exemplo")
                            .desired_width(300.0),
                    );
                    ui.end_row();
                    ui.label("Tipo documental");
                    ui.add(
                        TextEdit::singleline(&mut n.tipo_documental)
                            .hint_text("comprovante-pix")
                            .desired_width(300.0),
                    );
                    ui.end_row();
                });
            if let Some(e) = &n.erro {
                ui.add_space(4.0);
                componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui.button("Criar rascunho").clicked() {
                    criar = true;
                }
                if ui.button("Cancelar").clicked() {
                    fechar = true;
                }
            });
        });
        if resposta.should_close() {
            fechar = true;
        }
        if fechar {
            self.nova = None;
        } else if criar {
            match fonte.criar_regra(&n.id, &n.nome, &n.tipo_documental) {
                Ok(()) => {
                    let id = n.id.trim().to_owned();
                    self.nova = None;
                    self.abrir(&id);
                    self.avisar(
                        false,
                        "Regra criada em rascunho. Complete a definição, teste com exemplos e só então ative.",
                    );
                    self.recarregar_lista();
                }
                Err(e) => n.erro = Some(e),
            }
        }
    }

    fn mostrar_mensagem(&self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        if let Some(m) = &self.mensagem {
            ui.add_space(6.0);
            if m.erro {
                componentes::aviso(ui, "error", &m.texto, p.perigo_sutil, p.borda, p.perigo);
            } else {
                componentes::aviso(
                    ui,
                    "check_circle",
                    &m.texto,
                    p.sucesso_sutil,
                    p.borda,
                    p.sucesso,
                );
            }
        }
    }

    // ---- lista ----

    fn desenhar_lista(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        ui.horizontal(|ui| {
            componentes::titulo(ui, TITULO, 20.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui
                    .button("Nova regra")
                    .on_hover_text("Cria uma regra em rascunho com uma definição mínima")
                    .clicked()
                {
                    self.nova = Some(NovaRegra::default());
                }
            });
        });
        texto_pequeno(
            ui,
            "Como o sbf reconhece cada tipo de documento. Uma regra só propõe; consolidada, concilia sozinha quando as três provas batem.",
        );
        self.mostrar_mensagem(ui);
        ui.add_space(8.0);
        let mut abrir = None;
        let sugestoes = self.sugestoes.clone();
        let lista = match &self.lista {
            Some(Ok(l)) => l.clone(),
            Some(Err(e)) => {
                componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
                return;
            }
            None => return,
        };
        for (regra, _) in &sugestoes {
            let nome = lista
                .iter()
                .find(|r| &r.id == regra)
                .map_or(regra.as_str(), |r| r.nome.as_str());
            faixa_de_sugestao(ui, nome, regra, &mut abrir);
        }
        if lista.is_empty() {
            texto_pequeno(
                ui,
                "Nenhuma regra ainda. As de fábrica entram ao abrir o cofre.",
            );
        } else {
            componentes::cartao(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::Grid::new("lista-de-regras")
                    .num_columns(6)
                    .striped(true)
                    .spacing([18.0, 8.0])
                    .show(ui, |ui| {
                        for t in [
                            "Regra",
                            "Tipo documental",
                            "Em vigor",
                            "Estado",
                            "Execuções",
                            "Taxa de acerto",
                        ] {
                            componentes::rotulo_de_secao(ui, t);
                        }
                        ui.end_row();
                        for r in &lista {
                            linha_da_lista(ui, r, &mut abrir);
                            ui.end_row();
                        }
                    });
            });
        }
        if let Some(regra) = abrir {
            self.abrir(&regra);
        }
    }

    // ---- página ----

    fn desenhar_pagina(&mut self, ui: &mut Ui) {
        let Some(fonte) = self.fonte.clone() else {
            return;
        };
        let mut pagina = match self.pagina.take() {
            Some(p) => p,
            None => return,
        };
        let mut acao: Option<Acao> = None;
        let mut voltar = false;
        let p = tema::paleta(ui.ctx());

        if ui
            .button(format!("{}  Regras", icones::texto("arrow_back")))
            .clicked()
        {
            voltar = true;
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            componentes::titulo(ui, &pagina.resumo.nome, 20.0);
            selo_do_estado(ui, pagina.resumo.estado);
        });
        texto_pequeno(
            ui,
            format!(
                "{} · tipo documental {} · {} {}",
                pagina.regra,
                pagina.resumo.tipo_documental,
                pagina.resumo.versoes,
                if pagina.resumo.versoes == 1 {
                    "versão"
                } else {
                    "versões"
                }
            ),
        );
        self.mostrar_mensagem(ui);

        if let Some(v) = pagina.sugerida {
            ui.add_space(6.0);
            let texto = format!(
                "A regra {} acertou {LIMIAR_DE_CONSOLIDACAO} seguidas. Consolidar?",
                pagina.resumo.nome
            );
            componentes::aviso(
                ui,
                "verified",
                &texto,
                p.destaque_sutil,
                p.destaque_borda,
                p.destaque_tinta,
            );
            let _ = v;
        }

        ui.add_space(8.0);
        barra_de_acoes(ui, &pagina, &mut acao);
        confirmacoes(ui, &mut pagina, &mut acao);
        bloqueio_de_ativacao(ui, &pagina);

        ui.add_space(10.0);
        secao_de_versoes(ui, &mut pagina, &mut acao);
        ui.add_space(10.0);
        secao_do_editor(ui, &fonte, &mut pagina, &mut acao);
        ui.add_space(10.0);
        secao_de_exemplos(ui, &mut pagina, &mut acao);
        ui.add_space(10.0);
        secao_da_simulacao(ui, &pagina);
        ui.add_space(10.0);
        secao_do_rastro(ui, &pagina);
        ui.add_space(24.0);

        self.pagina = Some(pagina);
        if voltar {
            self.voltar();
            return;
        }
        if let Some(a) = acao {
            self.executar(&fonte, a);
        }
    }

    fn executar(&mut self, f: &FonteDasRegras, acao: Acao) {
        let Some(regra) = self.pagina.as_ref().map(|p| p.regra.clone()) else {
            return;
        };
        match acao {
            Acao::Selecionar(v) => {
                if let Some(p) = self.pagina.as_mut() {
                    p.selecionar(f, v);
                }
            }
            Acao::Testar => {
                let v = self.versao_atual();
                let r = f.testar(&regra, v);
                if let Some(p) = self.pagina.as_mut() {
                    match r {
                        Ok(rel) => {
                            let msg = if rel.aprovado() {
                                format!("Os {} exemplos passaram.", rel.resultados.len())
                            } else {
                                format!("{} exemplo(s) falharam.", rel.falhas().len())
                            };
                            self.mensagem = Some(Mensagem {
                                erro: !rel.aprovado(),
                                texto: msg,
                            });
                            p.relatorio = Some(rel);
                        }
                        Err(e) => {
                            self.mensagem = Some(Mensagem {
                                erro: true,
                                texto: e,
                            });
                        }
                    }
                }
            }
            Acao::Simular => {
                let v = self.versao_atual();
                if let Some(p) = self.pagina.as_mut() {
                    p.simulacao = Some(f.simular(&regra, v));
                }
            }
            Acao::PedirAtivar => {
                let v = self.versao_atual();
                if let Some(p) = self.pagina.as_mut() {
                    p.confirmando_ativar = Some(v);
                }
            }
            Acao::Ativar(v) => {
                if let Some(p) = self.pagina.as_mut() {
                    p.confirmando_ativar = None;
                }
                let r = f.ativar(&regra, v).map(|()| {
                    format!(
                        "Versão {v} ativada. Ela só propõe; nada vira conciliação sem o seu clique."
                    )
                });
                self.depois_da_acao(r);
            }
            Acao::Desativar => {
                let r = f.desativar(&regra).map(|()| "Regra desativada.".to_owned());
                self.depois_da_acao(r);
            }
            Acao::Reverter => {
                let r = f
                    .reverter(&regra)
                    .map(|()| "Voltou para a versão anterior.".to_owned());
                self.depois_da_acao(r);
            }
            Acao::PedirConsolidar => {
                if let Some(p) = self.pagina.as_mut() {
                    p.confirmando_consolidar = true;
                }
            }
            Acao::Consolidar(v) => {
                if let Some(p) = self.pagina.as_mut() {
                    p.confirmando_consolidar = false;
                }
                let r = f
                    .consolidar(&regra, v)
                    .map(|()| format!("Versão {v} consolidada. Concilia sozinha só com as três provas, com Desfazer."));
                self.depois_da_acao(r);
            }
            Acao::Cancelar => {
                if let Some(p) = self.pagina.as_mut() {
                    p.confirmando_ativar = None;
                    p.confirmando_consolidar = false;
                }
            }
            Acao::SalvarRascunho => {
                let Some(p) = self.pagina.as_ref() else {
                    return;
                };
                let e = &p.editor;
                match f.salvar_rascunho(&regra, &e.nome, &e.definicao, &e.script, &e.motivo) {
                    Ok(nova) => {
                        let ja_existia = p.versoes.iter().any(|v| v.versao == nova.versao);
                        let texto = if ja_existia {
                            format!("Nada mudou: é a versão {} de novo.", nova.versao)
                        } else {
                            format!("Rascunho salvo como versão {}.", nova.versao)
                        };
                        self.recarregar_lista();
                        let novo = self.carregar_pagina(&regra, Some(nova.versao));
                        if let Some(mut novo) = novo {
                            if let Some(velho) = self.pagina.take() {
                                novo.novo = velho.novo;
                            }
                            self.pagina = Some(novo);
                        }
                        self.mensagem = Some(Mensagem { erro: false, texto });
                    }
                    Err(e) => {
                        self.mensagem = Some(Mensagem {
                            erro: true,
                            texto: e,
                        });
                    }
                }
            }
            Acao::PreencherEsperado => {
                let v = self.versao_atual();
                if let Some(p) = self.pagina.as_mut() {
                    match f.esperado_de_hoje(&regra, v, &p.novo.texto) {
                        Ok(e) => {
                            p.novo.esperado = serde_json::to_string_pretty(&e).unwrap_or_default();
                            p.novo.erro = None;
                        }
                        Err(e) => p.novo.erro = Some(e),
                    }
                }
            }
            Acao::AdicionarExemplo => {
                let Some(p) = self.pagina.as_ref() else {
                    return;
                };
                let n = &p.novo;
                let r = f
                    .adicionar_exemplo(&regra, &n.nome, &n.texto, &n.esperado)
                    .map(|()| format!("Exemplo «{}» salvo.", n.nome.trim()));
                match r {
                    Ok(texto) => {
                        if let Some(p) = self.pagina.as_mut() {
                            p.novo = NovoExemplo::default();
                        }
                        self.depois_da_acao(Ok(texto));
                        if let Some(p) = self.pagina.as_mut() {
                            p.relatorio = f.testar(&regra, p.selecionada).ok();
                        }
                    }
                    Err(e) => {
                        if let Some(p) = self.pagina.as_mut() {
                            p.novo.erro = Some(e);
                        }
                    }
                }
            }
            Acao::ApagarExemplo(nome) => {
                let r = f
                    .apagar_exemplo(&regra, &nome)
                    .map(|()| format!("Exemplo «{nome}» apagado."));
                self.depois_da_acao(r);
            }
        }
    }

    fn versao_atual(&self) -> u32 {
        self.pagina.as_ref().map_or(0, |p| p.selecionada)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Acao {
    Selecionar(u32),
    Testar,
    Simular,
    PedirAtivar,
    Ativar(u32),
    Desativar,
    Reverter,
    PedirConsolidar,
    Consolidar(u32),
    Cancelar,
    SalvarRascunho,
    PreencherEsperado,
    AdicionarExemplo,
    ApagarExemplo(String),
}

impl Pagina {
    /// Recarrega exemplos, rastro, sugestão e o relatório da versão escolhida.
    fn atualizar(&mut self, f: &FonteDasRegras) {
        self.exemplos = f.exemplos(&self.regra).unwrap_or_default();
        self.rastro = f.rastro(&self.regra).unwrap_or_default();
        self.sugerida = f.consolidacao_sugerida(&self.regra);
        self.relatorio = f.testar(&self.regra, self.selecionada).ok();
    }

    fn versao(&self, v: u32) -> Option<&VersaoDeRegra> {
        self.versoes.iter().find(|x| x.versao == v)
    }

    /// Escolhe a versão: o editor passa a mostrar o conteúdo dela.
    fn selecionar(&mut self, f: &FonteDasRegras, versao: u32) {
        let Some(v) = self.versao(versao).cloned() else {
            return;
        };
        self.selecionada = versao;
        self.simulacao = None;
        self.confirmando_ativar = None;
        self.relatorio = f.testar(&self.regra, versao).ok();
        self.editor = Editor {
            nome: self.resumo.nome.clone(),
            motivo: String::new(),
            definicao: definicao_em_texto(&v.definicao),
            script: v.script.clone().unwrap_or_default(),
            ..Editor::default()
        };
        self.editor.revalidar(f);
    }

    fn estado_da_selecionada(&self) -> Option<Estado> {
        self.versao(self.selecionada).map(|v| v.estado)
    }

    /// O que falhou nos exemplos da versão escolhida.
    fn falhas(&self) -> Vec<String> {
        self.relatorio
            .as_ref()
            .map(|r| {
                r.resultados
                    .iter()
                    .filter(|x| !x.passou)
                    .map(|x| format!("{}: {}", x.nome, x.detalhe))
                    .collect()
            })
            .unwrap_or_default()
    }
}

impl Editor {
    /// A validação ao vivo: JSON e `Definicao::validar`; o script pelo executor.
    fn revalidar(&mut self, f: &FonteDasRegras) {
        self.erro_da_definicao = validar_definicao(&self.definicao).err();
        if self.script.trim().is_empty() {
            self.script_verificado = None;
            self.erro_do_script = None;
        } else {
            match f.verificar_script(&self.script) {
                Ok(v) => {
                    self.script_verificado = Some(v);
                    self.erro_do_script = None;
                }
                Err(e) => {
                    self.script_verificado = None;
                    self.erro_do_script = Some(e);
                }
            }
        }
    }

    fn valido(&self) -> bool {
        self.erro_da_definicao.is_none()
            && self.erro_do_script.is_none()
            && !self.nome.trim().is_empty()
    }
}

// ---------------------------------------------------------------------------------------
// Pedaços da tela
// ---------------------------------------------------------------------------------------

fn texto_pequeno(ui: &mut Ui, texto: impl Into<String>) {
    let p = tema::paleta(ui.ctx());
    ui.label(RichText::new(texto.into()).size(12.5).color(p.tinta_suave));
}

fn cores_do_estado(ui: &Ui, e: Estado) -> (egui::Color32, egui::Color32) {
    let p = tema::paleta(ui.ctx());
    match e {
        Estado::Rascunho => (p.aviso_sutil, p.aviso),
        Estado::Ativa => (p.sucesso_sutil, p.sucesso),
        Estado::Consolidada => (p.destaque_sutil, p.destaque_tinta),
        Estado::Desativada => (p.superficie_3, p.tinta_suave),
    }
}

fn selo_do_estado(ui: &mut Ui, estado: Option<Estado>) {
    match estado {
        Some(e) => {
            let (fundo, cor) = cores_do_estado(ui, e);
            componentes::selo(ui, rotulo_do_estado(e), fundo, cor);
        }
        None => {
            let p = tema::paleta(ui.ctx());
            componentes::selo(ui, "sem versão em vigor", p.superficie_3, p.tinta_suave);
        }
    }
}

fn percentual(r: &ResumoDaRegra) -> String {
    r.taxa_de_acerto()
        .map_or_else(|| "—".to_owned(), |t| format!("{:.0}%", t * 100.0))
}

fn faixa_de_sugestao(ui: &mut Ui, nome: &str, regra: &str, abrir: &mut Option<String>) {
    let p = tema::paleta(ui.ctx());
    egui::Frame::new()
        .fill(p.destaque_sutil)
        .stroke(egui::Stroke::new(1.0, p.destaque_borda))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor("verified", 16.0, p.destaque_tinta));
                ui.label(
                    RichText::new(format!(
                        "A regra {nome} acertou {LIMIAR_DE_CONSOLIDACAO} seguidas. Consolidar?"
                    ))
                    .color(p.destaque_tinta),
                );
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.button("Ver a regra").clicked() {
                        *abrir = Some(regra.to_owned());
                    }
                });
            });
        });
    ui.add_space(6.0);
}

fn linha_da_lista(ui: &mut Ui, r: &ResumoDaRegra, abrir: &mut Option<String>) {
    let p = tema::paleta(ui.ctx());
    ui.vertical(|ui| {
        ui.set_min_width(300.0);
        if ui
            .link(RichText::new(&r.nome).strong().size(14.0))
            .on_hover_text("Abre a regra: versões, editor, exemplos e Rastro")
            .clicked()
        {
            *abrir = Some(r.id.clone());
        }
        ui.label(RichText::new(&r.id).size(11.5).color(p.tinta_suave));
    });
    ui.label(&r.tipo_documental);
    ui.label(
        r.versao_em_vigor
            .map_or_else(|| "—".to_owned(), |v| format!("v{v}")),
    );
    selo_do_estado(ui, r.estado);
    ui.label(r.execucoes.to_string());
    ui.label(percentual(r));
}

fn barra_de_acoes(ui: &mut Ui, p: &Pagina, acao: &mut Option<Acao>) {
    let pal = tema::paleta(ui.ctx());
    let estado = p.estado_da_selecionada();
    let ativavel = matches!(estado, Some(Estado::Rascunho | Estado::Desativada));
    let falhas = p.falhas();
    let em_vigor = p.resumo.estado.is_some();
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(format!("Versão {}", p.selecionada))
                .strong()
                .size(14.0),
        );
        if let Some(e) = estado {
            let (fundo, cor) = cores_do_estado(ui, e);
            componentes::selo(ui, rotulo_do_estado(e), fundo, cor);
        }
        ui.add_space(8.0);
        if ui
            .button(format!("{}  Testar", icones::texto("fact_check")))
            .on_hover_text("Roda os exemplos desta versão")
            .clicked()
        {
            *acao = Some(Acao::Testar);
        }
        if ui
            .button(format!("{}  Simular", icones::texto("difference")))
            .on_hover_text("Mostra o que muda, entrada por entrada, se esta versão entrasse em vigor. Não grava nada.")
            .clicked()
        {
            *acao = Some(Acao::Simular);
        }
        let motivo_do_bloqueio = if !ativavel {
            "Só um rascunho ou uma versão desativada pode ser ativada"
        } else if !falhas.is_empty() {
            "Um exemplo falha: corrija a regra ou o exemplo antes de ativar"
        } else {
            "Coloca esta versão em vigor; ela só propõe"
        };
        let botao = egui::Button::new(
            RichText::new(format!("{}  Ativar versão {}", icones::texto("task_alt"), p.selecionada))
                .color(if ativavel && falhas.is_empty() {
                    pal.sobre_destaque
                } else {
                    pal.tinta_desabilitada
                }),
        )
        .fill(if ativavel && falhas.is_empty() {
            pal.destaque
        } else {
            pal.superficie_3
        });
        if ui
            .add_enabled(ativavel && falhas.is_empty(), botao)
            .on_hover_text(motivo_do_bloqueio)
            .on_disabled_hover_text(motivo_do_bloqueio)
            .clicked()
        {
            *acao = Some(Acao::PedirAtivar);
        }
        if ui
            .add_enabled(em_vigor, egui::Button::new(format!("{}  Desativar", icones::texto("block"))))
            .on_hover_text("Tira a versão em vigor; o histórico fica")
            .clicked()
        {
            *acao = Some(Acao::Desativar);
        }
        let da_para_reverter = em_vigor
            && p.rastro
                .iter()
                .any(|l| l.para == "ativa" && l.versao != p.resumo.versao_em_vigor.unwrap_or(0));
        if ui
            .add_enabled(da_para_reverter, egui::Button::new(format!("{}  Reverter", icones::texto("undo"))))
            .on_hover_text("Volta para a versão que estava em vigor antes (rollback)")
            .clicked()
        {
            *acao = Some(Acao::Reverter);
        }
        if p.sugerida.is_some()
            && ui
                .button(format!("{}  Consolidar", icones::texto("verified")))
                .on_hover_text("Sugerida: acertou o bastante seguidas")
                .clicked()
        {
            *acao = Some(Acao::PedirConsolidar);
        }
    });
}

fn confirmacoes(ui: &mut Ui, p: &mut Pagina, acao: &mut Option<Acao>) {
    let pal = tema::paleta(ui.ctx());
    if let Some(v) = p.confirmando_ativar {
        ui.add_space(6.0);
        componentes::cartao(ui, |ui| {
            ui.label(
                RichText::new(format!("Ativar {} versão {v}?", p.resumo.nome))
                    .strong()
                    .size(14.0),
            );
            texto_pequeno(
                ui,
                "A versão passa a propor para os documentos novos. Nada vira conciliação sem o seu clique. A versão em vigor hoje fica desativada, no histórico.",
            );
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Ativar").color(pal.sobre_destaque))
                            .fill(pal.destaque),
                    )
                    .clicked()
                {
                    *acao = Some(Acao::Ativar(v));
                }
                if ui.button("Cancelar").clicked() {
                    *acao = Some(Acao::Cancelar);
                }
            });
        });
    }
    if p.confirmando_consolidar
        && let Some(v) = p.sugerida
    {
        ui.add_space(6.0);
        componentes::cartao(ui, |ui| {
            ui.label(
                RichText::new(format!("Consolidar {} versão {v}?", p.resumo.nome))
                    .strong()
                    .size(14.0),
            );
            texto_pequeno(
                ui,
                "Consolidada, a regra concilia sozinha só com as três provas (valor, data e conta batendo exatamente), com Desfazer. Uma rejeição, um Desfazer ou uma versão nova a devolvem a ativa.",
            );
            ui.horizontal(|ui| {
                if ui.button("Consolidar").clicked() {
                    *acao = Some(Acao::Consolidar(v));
                }
                if ui.button("Cancelar").clicked() {
                    *acao = Some(Acao::Cancelar);
                }
            });
        });
    }
}

fn bloqueio_de_ativacao(ui: &mut Ui, p: &Pagina) {
    let pal = tema::paleta(ui.ctx());
    let falhas = p.falhas();
    if falhas.is_empty()
        || !matches!(
            p.estado_da_selecionada(),
            Some(Estado::Rascunho | Estado::Desativada)
        )
    {
        return;
    }
    ui.add_space(6.0);
    egui::Frame::new()
        .fill(pal.perigo_sutil)
        .stroke(egui::Stroke::new(1.0, pal.borda))
        .corner_radius(egui::CornerRadius::same(4))
        .inner_margin(egui::Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(icones::icone_cor("block", 16.0, pal.perigo));
                ui.label(
                    RichText::new(format!(
                        "Ativar está bloqueado: {} {} falha{}.",
                        falhas.len(),
                        if falhas.len() == 1 {
                            "exemplo"
                        } else {
                            "exemplos"
                        },
                        if falhas.len() == 1 { "" } else { "m" }
                    ))
                    .strong()
                    .color(pal.perigo),
                );
            });
            for f in &falhas {
                ui.label(RichText::new(format!("•  {f}")).color(pal.perigo));
            }
        });
}

fn secao_de_versoes(ui: &mut Ui, p: &mut Pagina, acao: &mut Option<Acao>) {
    componentes::rotulo_de_secao(ui, "Versões");
    ui.add_space(2.0);
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        egui::Grid::new("versoes-da-regra")
            .num_columns(6)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                for t in ["Versão", "Estado", "Autor", "Motivo", "Criada em", ""] {
                    componentes::rotulo_de_secao(ui, t);
                }
                ui.end_row();
                for v in p.versoes.iter().rev() {
                    let escolhida = v.versao == p.selecionada;
                    ui.label(RichText::new(format!("v{}", v.versao)).strong());
                    let (fundo, cor) = cores_do_estado(ui, v.estado);
                    componentes::selo(ui, rotulo_do_estado(v.estado), fundo, cor);
                    ui.label(rotulo_do_autor(v.autor));
                    ui.label(&v.motivo);
                    ui.label(data_curta(&v.criada_em));
                    if escolhida {
                        texto_pequeno(ui, "no editor");
                    } else if ui.small_button("Abrir no editor").clicked() {
                        *acao = Some(Acao::Selecionar(v.versao));
                    }
                    ui.end_row();
                }
            });
    });
}

fn data_curta(iso: &str) -> String {
    iso.get(..16)
        .map_or_else(|| iso.to_owned(), |s| s.replace('T', " "))
}

fn secao_do_editor(ui: &mut Ui, f: &FonteDasRegras, p: &mut Pagina, acao: &mut Option<Acao>) {
    let pal = tema::paleta(ui.ctx());
    componentes::rotulo_de_secao(
        ui,
        &format!("Editor · conteúdo da versão {}", p.selecionada),
    );
    ui.add_space(2.0);
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        texto_pequeno(
            ui,
            "Uma versão gravada não muda. Salvar cria uma versão nova em rascunho, com você como autor; ela só entra em vigor quando você ativa.",
        );
        ui.add_space(4.0);
        let mut mudou = false;
        ui.horizontal(|ui| {
            ui.label("Nome");
            ui.add(TextEdit::singleline(&mut p.editor.nome).desired_width(320.0));
            ui.label("Motivo da versão");
            ui.add(
                TextEdit::singleline(&mut p.editor.motivo)
                    .hint_text("o que mudou e por quê")
                    .desired_width(ui.available_width()),
            );
        });
        ui.add_space(4.0);
        componentes::rotulo_de_secao(ui, "Definição (JSON)");
        let r = ScrollArea::vertical()
            .id_salt("editor-da-definicao")
            .max_height(170.0)
            .show(ui, |ui| {
                ui.add(
                    TextEdit::multiline(&mut p.editor.definicao)
                        .code_editor()
                        .desired_rows(6)
                        .desired_width(f32::INFINITY),
                )
            })
            .inner;
        mudou |= r.changed();
        match &p.editor.erro_da_definicao {
            Some(e) => {
                ui.label(RichText::new(format!("Definição inválida: {e}")).color(pal.perigo));
            }
            None => {
                ui.label(RichText::new("Definição válida.").color(pal.sucesso));
            }
        }
        ui.add_space(4.0);
        componentes::rotulo_de_secao(ui, "Script Luau (opcional)");
        let r = ui.add(
            TextEdit::multiline(&mut p.editor.script)
                .code_editor()
                .desired_rows(4)
                .hint_text("function transformar(entrada, parametros, ajuda) ... end")
                .desired_width(f32::INFINITY),
        );
        mudou |= r.changed();
        match (&p.editor.erro_do_script, p.editor.script_verificado) {
            (Some(e), _) => {
                ui.label(RichText::new(format!("Script com erro: {e}")).color(pal.perigo));
            }
            (None, Some(true)) => {
                ui.label(RichText::new("Script verificado pelo executor.").color(pal.sucesso));
            }
            (None, Some(false)) => {
                ui.label(
                    RichText::new(
                        "Script não verificado: não há executor ligado. A ativação fica bloqueada.",
                    )
                    .color(pal.aviso),
                );
            }
            (None, None) => {}
        }
        if mudou {
            p.editor.revalidar(f);
        }
        ui.add_space(6.0);
        let proxima = p.versoes.iter().map(|v| v.versao).max().unwrap_or(0) + 1;
        if ui
            .add_enabled(
                p.editor.valido(),
                egui::Button::new(format!("Salvar rascunho (versão {proxima})")),
            )
            .on_hover_text("Cria uma versão nova em rascunho; nada entra em vigor")
            .clicked()
        {
            *acao = Some(Acao::SalvarRascunho);
        }
    });
}

fn secao_de_exemplos(ui: &mut Ui, p: &mut Pagina, acao: &mut Option<Acao>) {
    let pal = tema::paleta(ui.ctx());
    componentes::rotulo_de_secao(ui, &format!("Exemplos · versão {}", p.selecionada));
    ui.add_space(2.0);
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        if p.exemplos.is_empty() {
            texto_pequeno(
                ui,
                "Sem exemplos. Uma regra sem exemplos ativa sem nada que a proteja: cole um texto abaixo.",
            );
        }
        for ex in &p.exemplos {
            let resultado = p
                .relatorio
                .as_ref()
                .and_then(|r| r.resultados.iter().find(|x| x.nome == ex.nome));
            ui.horizontal_wrapped(|ui| {
                match resultado {
                    Some(r) if r.passou => {
                        ui.label(icones::icone_cor("check_circle", 16.0, pal.sucesso));
                        componentes::selo(ui, "passou", pal.sucesso_sutil, pal.sucesso);
                    }
                    Some(_) => {
                        ui.label(icones::icone_cor("cancel", 16.0, pal.perigo));
                        componentes::selo(ui, "falhou", pal.perigo_sutil, pal.perigo);
                    }
                    None => {
                        componentes::selo(ui, "não rodou", pal.superficie_3, pal.tinta_suave);
                    }
                }
                ui.label(RichText::new(&ex.nome).strong());
                texto_pequeno(ui, resumo_do_esperado(&ex.esperado));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if componentes::botao_icone(
                        ui,
                        "delete",
                        "Apaga o exemplo (fica no histórico)",
                        true,
                    )
                    .clicked()
                    {
                        *acao = Some(Acao::ApagarExemplo(ex.nome.clone()));
                    }
                });
            });
            if let Some(r) = resultado
                && !r.passou
            {
                ui.label(RichText::new(format!("    {}", r.detalhe)).color(pal.perigo));
            }
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            if ui
                .selectable_label(
                    p.novo.aberto,
                    format!("{}  Adicionar exemplo", icones::texto("add")),
                )
                .clicked()
            {
                p.novo.aberto = !p.novo.aberto;
            }
        });
        if p.novo.aberto {
            formulario_de_exemplo(ui, p, acao);
        }
    });
}

fn formulario_de_exemplo(ui: &mut Ui, p: &mut Pagina, acao: &mut Option<Acao>) {
    let pal = tema::paleta(ui.ctx());
    ui.add_space(4.0);
    texto_pequeno(
        ui,
        "Cole o texto de um documento (o texto fica só neste catálogo, com o exemplo). Depois diga o que a regra deve devolver.",
    );
    ui.horizontal(|ui| {
        ui.label("Nome");
        ui.add(TextEdit::singleline(&mut p.novo.nome).desired_width(320.0));
    });
    ui.add(
        TextEdit::multiline(&mut p.novo.texto)
            .code_editor()
            .desired_rows(5)
            .hint_text("texto do documento")
            .desired_width(f32::INFINITY),
    );
    ui.horizontal(|ui| {
        componentes::rotulo_de_secao(ui, "Resultado esperado (JSON)");
        if ui
            .add_enabled(!p.novo.texto.trim().is_empty(), egui::Button::new("Usar o que a versão lê hoje"))
            .on_hover_text("Preenche com o que esta versão devolve para o texto colado; confira antes de salvar")
            .clicked()
        {
            *acao = Some(Acao::PreencherEsperado);
        }
    });
    ui.add(
        TextEdit::multiline(&mut p.novo.esperado)
            .code_editor()
            .desired_rows(4)
            .hint_text(r#"{"esperado":"nao_se_aplica"}"#)
            .desired_width(f32::INFINITY),
    );
    if let Some(e) = &p.novo.erro {
        ui.label(RichText::new(e).color(pal.perigo));
    }
    if ui.button("Salvar exemplo").clicked() {
        *acao = Some(Acao::AdicionarExemplo);
    }
}

fn resumo_do_esperado(e: &Esperado) -> String {
    match e {
        Esperado::NaoSeAplica => "espera: a regra não se aplica".to_owned(),
        Esperado::Reconhece {
            tipo_documental,
            campos,
        } => format!(
            "espera: {tipo_documental} com {}",
            campos.keys().cloned().collect::<Vec<_>>().join(", ")
        ),
    }
}

fn secao_da_simulacao(ui: &mut Ui, p: &Pagina) {
    let pal = tema::paleta(ui.ctx());
    let Some(s) = &p.simulacao else {
        return;
    };
    componentes::rotulo_de_secao(ui, &format!("Simulação · versão {}", p.selecionada));
    ui.add_space(2.0);
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        match s {
            Err(e) => {
                ui.label(RichText::new(e).color(pal.perigo));
            }
            Ok(s) => {
                texto_pequeno(
                    ui,
                    format!(
                        "{} entradas lidas (exemplos e recentes); {} iguais, {} mudam. Nada foi gravado.",
                        s.total,
                        s.diferencas.iguais,
                        s.diferencas.mudaram.len()
                    ),
                );
                for m in &s.diferencas.mudaram {
                    let rotulo = s
                        .rotulos
                        .get(&m.entrada)
                        .cloned()
                        .unwrap_or_else(|| "entrada".to_owned());
                    ui.add_space(4.0);
                    ui.label(RichText::new(rotulo).strong());
                    ui.horizontal_wrapped(|ui| {
                        componentes::selo(ui, "antes", pal.superficie_3, pal.tinta_suave);
                        ui.label(descrever(&m.antes));
                    });
                    ui.horizontal_wrapped(|ui| {
                        componentes::selo(ui, "depois", pal.info_sutil, pal.info);
                        ui.label(descrever(&m.depois));
                    });
                }
            }
        }
    });
}

fn descrever(r: &sbf_regras::simulacao::ResumoDaDecisao) -> String {
    use sbf_regras::simulacao::ResumoDaDecisao::{Ambiguo, Nenhuma, Reconhecido};
    match r {
        Nenhuma => "nenhuma regra se aplica".to_owned(),
        Ambiguo { regras } => format!("ambíguo entre {}", regras.join(", ")),
        Reconhecido {
            regra,
            tipo_documental,
            campos,
            revisar,
            propostas,
        } => format!(
            "{regra} reconhece {tipo_documental} · {} · {} {} · {}",
            campos
                .iter()
                .map(|(n, v)| format!("{n} = {}", valor(v)))
                .collect::<Vec<_>>()
                .join(", "),
            propostas,
            if *propostas == 1 {
                "proposta"
            } else {
                "propostas"
            },
            if *revisar {
                "precisa revisar"
            } else {
                "sem revisão"
            }
        ),
    }
}

fn valor(v: &sbf_regras::Valor) -> String {
    v.to_string()
}

fn secao_do_rastro(ui: &mut Ui, p: &Pagina) {
    componentes::rotulo_de_secao(ui, "Rastro da regra");
    ui.add_space(2.0);
    componentes::cartao(ui, |ui| {
        ui.set_width(ui.available_width());
        if p.rastro.is_empty() {
            texto_pequeno(ui, "Sem histórico ainda.");
        }
        egui::Grid::new("rastro-da-regra")
            .num_columns(5)
            .striped(true)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                for l in p.rastro.iter().rev() {
                    ui.label(data_curta(&l.em));
                    ui.label(RichText::new(format!("v{}", l.versao)).strong());
                    ui.label(format!(
                        "{} → {}",
                        l.de.as_deref().unwrap_or("nova"),
                        l.para
                    ));
                    ui.label(autor_do_rastro(&l.quem));
                    ui.label(&l.motivo);
                    ui.end_row();
                }
            });
    });
}

fn autor_do_rastro(quem: &str) -> String {
    match quem {
        "usuario" => "você".to_owned(),
        "ia" => "IA (MCP)".to_owned(),
        "fabrica" => "fábrica".to_owned(),
        "correcao" => "correção".to_owned(),
        "sistema" => "sbf".to_owned(),
        outro => outro.to_owned(),
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn regras() -> Regras {
        let mut r = Regras::default();
        r.ligar(FonteDasRegras::ficticia(None).expect("fonte"));
        r.recarregar_lista();
        r
    }

    #[test]
    fn abrir_a_regra_em_rascunho_escolhe_a_ultima_versao_e_mostra_a_falha() {
        let mut r = regras();
        r.abrir("fatura-cartao-banco-exemplo");
        let p = r.pagina.as_ref().expect("página");
        assert_eq!(p.selecionada, 1);
        assert_eq!(p.falhas().len(), 1);
        assert!(p.editor.erro_da_definicao.is_none());
    }

    #[test]
    fn a_lista_traz_a_sugestao_de_consolidar() {
        let r = regras();
        assert_eq!(
            r.sugestoes,
            vec![("comprovante-pix-banco-exemplo".to_owned(), 5)]
        );
    }

    #[test]
    fn reverter_e_ativar_pela_pagina_trocam_a_versao_em_vigor() {
        let mut r = regras();
        let f = r.fonte.clone().expect("fonte");
        r.abrir("comprovante-pix-banco-exemplo");
        r.executar(&f, Acao::Reverter);
        assert_eq!(
            r.pagina.as_ref().expect("p").resumo.versao_em_vigor,
            Some(1)
        );
        r.executar(&f, Acao::Selecionar(2));
        r.executar(&f, Acao::PedirAtivar);
        assert_eq!(r.pagina.as_ref().expect("p").confirmando_ativar, Some(2));
        r.executar(&f, Acao::Ativar(2));
        assert_eq!(
            r.pagina.as_ref().expect("p").resumo.versao_em_vigor,
            Some(2)
        );
    }

    #[test]
    fn ativar_com_exemplo_falhando_mostra_o_erro_e_nao_muda_nada() {
        let mut r = regras();
        let f = r.fonte.clone().expect("fonte");
        r.abrir("fatura-cartao-banco-exemplo");
        r.executar(&f, Acao::Ativar(1));
        assert!(r.mensagem.as_ref().expect("mensagem").erro);
        assert_eq!(r.pagina.as_ref().expect("p").resumo.estado, None);
    }

    #[test]
    fn salvar_rascunho_pela_pagina_abre_a_versao_nova() {
        let mut r = regras();
        let f = r.fonte.clone().expect("fonte");
        r.abrir("comprovante-pix-banco-exemplo");
        {
            let p = r.pagina.as_mut().expect("p");
            p.editor.motivo = "teste".to_owned();
            p.editor.definicao = p
                .editor
                .definicao
                .replace("\"prioridade\": 0", "\"prioridade\": 3");
            p.editor.revalidar(&f);
        }
        r.executar(&f, Acao::SalvarRascunho);
        let p = r.pagina.as_ref().expect("p");
        assert_eq!(p.versoes.len(), 3);
        assert_eq!(p.selecionada, 3);
        assert_eq!(p.estado_da_selecionada(), Some(Estado::Rascunho));
        // A versão em vigor não mudou.
        assert_eq!(p.resumo.versao_em_vigor, Some(2));
    }

    #[test]
    fn consolidar_pela_pagina_so_com_a_sugestao() {
        let mut r = regras();
        let f = r.fonte.clone().expect("fonte");
        r.abrir("conta-de-energia-distribuidora-exemplo");
        assert!(r.pagina.as_ref().expect("p").sugerida.is_none());
        r.abrir("comprovante-pix-banco-exemplo");
        assert_eq!(r.pagina.as_ref().expect("p").sugerida, Some(2));
        r.executar(&f, Acao::PedirConsolidar);
        r.executar(&f, Acao::Consolidar(2));
        let p = r.pagina.as_ref().expect("p");
        assert_eq!(p.resumo.estado, Some(Estado::Consolidada));
        assert!(p.sugerida.is_none());
    }

    #[test]
    fn json_quebrado_aparece_ao_vivo() {
        let mut r = regras();
        let f = r.fonte.clone().expect("fonte");
        r.abrir("comprovante-pix-banco-exemplo");
        let p = r.pagina.as_mut().expect("p");
        p.editor.definicao.push('}');
        p.editor.revalidar(&f);
        assert!(p.editor.erro_da_definicao.is_some());
        assert!(!p.editor.valido());
    }
}
