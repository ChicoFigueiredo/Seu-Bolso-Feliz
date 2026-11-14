//! A interface da caixa de entrada e da edição (spec 08, "Janela"): a área Caixa de entrada,
//! as ações das páginas de dinheiro (Editar, Ratear, Excluir), os diálogos e a seção da
//! área Sincronização. Só desenha e emite [`PedidoDaCaixa`]; quem escreve é a [`Janela`],
//! fora do quadro, pelo [`Editor`](crate::caixa::Editor).
//!
//! [`Janela`]: crate::janela::Janela

use egui::{
    Align, CornerRadius, FontId, Frame, Key, Layout, Margin, Modifiers, RichText, Sense, Stroke, Ui,
};
use egui_extras::{Size, StripBuilder};
use sbf_inventario::Ref;

use crate::caixa::{
    AlvoDaEscrita, CamposDaMovimentacao, CamposDoRascunho, Direcao, EnvioDoLote, Escrita,
    EstadoDeEnvio, FonteDaCaixa, LoteDeRascunhos, Opcao, ProgressoDoEnvio, Rascunho,
    RateioDaMovimentacao, StatusDoRascunho, TIPOS_DE_LANCAMENTO, validar_rateio,
};
use crate::componentes;
use crate::dinheiro;
use crate::formato;
use crate::icones;
use crate::janela::{centavos_de, data_iso};
use crate::leitura::Leitor;
use crate::navegacao::Pedido;
use crate::previa::Previas;
use crate::tema::{self, familia_forte};

/// Tudo o que a caixa pede à janela.
#[derive(Debug, Clone, PartialEq)]
pub enum PedidoDaCaixa {
    /// Grava os campos corrigidos do rascunho.
    Corrigir {
        id: String,
        campos: CamposDoRascunho,
    },
    Aprovar(String),
    /// Abre o diálogo que pede o motivo.
    PedirRecusa(String),
    Recusar {
        id: String,
        motivo: String,
    },
    /// Abre a confirmação do lote (mostra o que falta nos incompletos).
    PedirAprovarLote(String),
    AprovarLote(String),
    /// Abrem os diálogos de uma movimentação.
    PedirEdicao(Ref),
    PedirRateio(Ref),
    PedirExclusao(Ref),
    Editar {
        alvo: Ref,
        campos: CamposDaMovimentacao,
    },
    Ratear {
        alvo: Ref,
        linhas: Vec<RateioDaMovimentacao>,
    },
    Excluir(Ref),
    RefazerEscrita(String),
    DescartarEscrita(String),
    /// O botão Enviar ao servidor de um lote do inventário.
    EnviarLote(Ref),
}

fn pedir(pedidos: &mut Vec<Pedido>, p: PedidoDaCaixa) {
    pedidos.push(Pedido::Caixa(p));
}

// ---------------------------------------------------------------------------
// Marca de estado de envio
// ---------------------------------------------------------------------------

/// A marca visual de uma escrita: ícone e texto (a cor nunca é o único sinal).
pub fn marca(ui: &mut Ui, estado: EstadoDeEnvio) {
    let p = tema::paleta(ui.ctx());
    let (icone, fundo, cor) = match estado {
        EstadoDeEnvio::Pendente => ("schedule", p.aviso_sutil, p.aviso),
        EstadoDeEnvio::Aceita => ("check_circle", p.sucesso_sutil, p.sucesso),
        EstadoDeEnvio::Recusada => ("block", p.perigo_sutil, p.perigo),
    };
    let dica = match estado {
        EstadoDeEnvio::Pendente => "Feita aqui; o servidor ainda não aceitou",
        EstadoDeEnvio::Aceita => "O servidor aceitou",
        EstadoDeEnvio::Recusada => "O servidor recusou: veja em Sincronização",
    };
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 3.0;
        ui.label(icones::icone_cor(icone, 14.0, cor));
        componentes::selo(ui, estado.rotulo(), fundo, cor);
    })
    .response
    .on_hover_text(dica);
}

// ---------------------------------------------------------------------------
// Estado da área
// ---------------------------------------------------------------------------

/// O formulário de correção: os textos que você digita, e o rascunho que eles corrigem.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FormularioDoRascunho {
    pub id: String,
    original: CamposDoRascunho,
    pub descricao: String,
    pub fornecedor: String,
    pub direcao: Direcao,
    pub tipo: Option<String>,
    pub valor: String,
    pub data_do_evento: String,
    pub vencimento: String,
    pub conta: Option<String>,
    pub categoria: String,
    pub numero_do_documento: String,
    pub observacoes: String,
}

fn texto(o: &Option<String>) -> String {
    o.clone().unwrap_or_default()
}

fn opcional(t: &str) -> Option<String> {
    let t = t.trim();
    (!t.is_empty()).then(|| t.to_owned())
}

/// `31244` → `312,44` (para o campo de texto).
pub fn centavos_em_texto(c: i64) -> String {
    format!(
        "{}{},{:02}",
        if c < 0 { "-" } else { "" },
        c.abs() / 100,
        c.abs() % 100
    )
}

impl FormularioDoRascunho {
    pub fn de(r: &Rascunho) -> Self {
        let c = &r.campos;
        Self {
            id: r.id.clone(),
            original: c.clone(),
            descricao: texto(&c.descricao),
            fornecedor: texto(&c.fornecedor),
            direcao: c.direcao,
            tipo: c.tipo.clone(),
            valor: c.valor_centavos.map(centavos_em_texto).unwrap_or_default(),
            data_do_evento: c
                .data_do_evento
                .as_deref()
                .map(formato::data)
                .unwrap_or_default(),
            vencimento: c
                .vencimento
                .as_deref()
                .map(formato::data)
                .unwrap_or_default(),
            conta: c.conta.clone(),
            categoria: texto(&c.categoria),
            numero_do_documento: texto(&c.numero_do_documento),
            observacoes: texto(&c.observacoes),
        }
    }

    /// Os campos que o texto digitado dá, ou o que está inválido (nada é enviado assim).
    pub fn campos(&self) -> Result<CamposDoRascunho, Vec<&'static str>> {
        let mut erros = Vec::new();
        let valor = if self.valor.trim().is_empty() {
            None
        } else {
            match centavos_de(&self.valor) {
                Some(v) if v >= 0 => Some(v),
                _ => {
                    erros.push("Valor (use 312,44; sem sinal)");
                    None
                }
            }
        };
        let data = |t: &str, rotulo: &'static str, erros: &mut Vec<&'static str>| {
            if t.trim().is_empty() {
                None
            } else {
                match data_iso(t.trim()) {
                    Some(d) => Some(d),
                    None => {
                        erros.push(rotulo);
                        None
                    }
                }
            }
        };
        let data_do_evento = data(
            &self.data_do_evento,
            "Data do evento (dd/mm/aaaa)",
            &mut erros,
        );
        let vencimento = data(&self.vencimento, "Vencimento (dd/mm/aaaa)", &mut erros);
        if !erros.is_empty() {
            return Err(erros);
        }
        Ok(CamposDoRascunho {
            direcao: self.direcao,
            tipo: self.tipo.clone(),
            valor_centavos: valor,
            data_do_evento,
            vencimento,
            descricao: opcional(&self.descricao),
            fornecedor: opcional(&self.fornecedor),
            categoria: opcional(&self.categoria),
            conta: self.conta.clone(),
            numero_do_documento: opcional(&self.numero_do_documento),
            observacoes: opcional(&self.observacoes),
        })
    }

    /// Mudou em relação ao rascunho?
    pub fn sujo(&self) -> bool {
        self.campos().map_or(true, |c| c != self.original)
    }
}

/// O que a área guarda entre quadros.
#[derive(Debug, Default)]
pub struct EstadoDaCaixa {
    pub selecionado: Option<String>,
    pub formulario: Option<FormularioDoRascunho>,
    /// Mostra também os já decididos (aprovados e recusados).
    pub mostrar_decididos: bool,
}

impl EstadoDaCaixa {
    /// Garante uma seleção válida e o formulário dela.
    fn sincronizar(&mut self, lotes: &[LoteDeRascunhos]) {
        let todos = || lotes.iter().flat_map(|l| l.rascunhos.iter());
        let valido = self
            .selecionado
            .as_ref()
            .is_some_and(|s| todos().any(|r| &r.id == s));
        if !valido {
            self.selecionado = todos()
                .find(|r| r.status.aberto())
                .or_else(|| todos().next())
                .map(|r| r.id.clone());
        }
        let Some(r) = self
            .selecionado
            .as_ref()
            .and_then(|s| todos().find(|r| &r.id == s))
        else {
            self.formulario = None;
            return;
        };
        let refazer = match &self.formulario {
            Some(f) => f.id != r.id || (!f.sujo() && f.original != r.campos),
            None => true,
        };
        if refazer {
            self.formulario = Some(FormularioDoRascunho::de(r));
        }
    }

    /// O rascunho selecionado.
    pub fn rascunho<'a>(&self, lotes: &'a [LoteDeRascunhos]) -> Option<&'a Rascunho> {
        let s = self.selecionado.as_ref()?;
        lotes
            .iter()
            .flat_map(|l| l.rascunhos.iter())
            .find(|r| &r.id == s)
    }
}

// ---------------------------------------------------------------------------
// A área Caixa de entrada
// ---------------------------------------------------------------------------

/// Os pedidos que o formulário gera ao aprovar: corrige antes, se você mexeu.
pub fn pedidos_de_aprovar(estado: &EstadoDaCaixa, id: &str) -> Vec<PedidoDaCaixa> {
    let mut v = Vec::new();
    if let Some(f) = estado
        .formulario
        .as_ref()
        .filter(|f| f.id == id && f.sujo())
        && let Ok(campos) = f.campos()
    {
        v.push(PedidoDaCaixa::Corrigir {
            id: id.to_owned(),
            campos,
        });
    }
    v.push(PedidoDaCaixa::Aprovar(id.to_owned()));
    v
}

/// Lê A, R e Shift+A (a Caixa de entrada aberta e nenhum campo de texto com o foco).
fn atalhos(
    ctx: &egui::Context,
    estado: &EstadoDaCaixa,
    lotes: &[LoteDeRascunhos],
    pedidos: &mut Vec<Pedido>,
) {
    if ctx.egui_wants_keyboard_input() {
        return;
    }
    let lote_aprovar = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, Key::A));
    let aprovar = !lote_aprovar && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::A));
    let recusar = ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::R));
    let Some(r) = estado.rascunho(lotes) else {
        return;
    };
    if lote_aprovar {
        pedir(pedidos, PedidoDaCaixa::PedirAprovarLote(r.lote.clone()));
    } else if aprovar && r.status.aberto() {
        for p in pedidos_de_aprovar(estado, &r.id) {
            pedir(pedidos, p);
        }
    } else if recusar && r.status.aberto() {
        pedir(pedidos, PedidoDaCaixa::PedirRecusa(r.id.clone()));
    }
}

fn icone_do_status(s: StatusDoRascunho) -> &'static str {
    match s {
        StatusDoRascunho::EmRevisao => "help",
        StatusDoRascunho::Corrigido => "edit_note",
        StatusDoRascunho::Aprovado => "check_circle",
        StatusDoRascunho::Recusado => "cancel",
    }
}

/// A área Caixa de entrada: a lista agrupada por lote, a prévia ao lado e o formulário.
pub fn area(
    ui: &mut Ui,
    fonte: &dyn FonteDaCaixa,
    estado: &mut EstadoDaCaixa,
    previas: &mut Previas,
    leitor: &mut Leitor<'_>,
    hoje: &str,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let lotes = match fonte.lotes() {
        Ok(l) => l,
        Err(e) => {
            dinheiro::motivo_ou_erro(ui, &e.to_string());
            return;
        }
    };
    estado.sincronizar(&lotes);
    atalhos(ui.ctx(), estado, &lotes, pedidos);

    let abertos: usize = lotes.iter().map(|l| l.n_abertos()).sum();
    ui.horizontal_wrapped(|ui| {
        ui.label(icones::icone_cor("inbox", 20.0, p.tinta));
        componentes::titulo(ui, "Caixa de entrada", 20.0);
        ui.label(
            RichText::new(format!(
                "{abertos} {} esperando você",
                if abertos == 1 {
                    "rascunho"
                } else {
                    "rascunhos"
                }
            ))
            .color(p.tinta_suave),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.checkbox(&mut estado.mostrar_decididos, "Mostrar decididos");
        });
    });
    ui.label(
        RichText::new(
            "Os rascunhos que o pipeline gerou dos documentos. Aprovar lança no livro-caixa pelo servidor. Atalhos: A aprova, R recusa, Shift+A aprova o lote.",
        )
        .size(12.0)
        .color(p.tinta_suave),
    );
    ui.add_space(6.0);
    let (pendentes, recusadas) = crate::caixa::contar(&fonte.escritas().unwrap_or_default());
    if pendentes + recusadas > 0 {
        faixa_de_escritas(ui, pendentes, recusadas, pedidos);
    }

    if lotes.is_empty() {
        componentes::aviso(
            ui,
            "inbox",
            "Nenhum rascunho esperando. Quando o pipeline processar documentos enviados, eles chegam aqui.",
            p.info_sutil,
            p.info_borda,
            p.info,
        );
        return;
    }
    let altura = ui.available_height().max(320.0);
    StripBuilder::new(ui)
        .size(Size::exact(260.0))
        .size(Size::exact(290.0))
        .size(Size::remainder().at_least(300.0))
        .horizontal(|mut tira| {
            tira.cell(|ui| lista(ui, &lotes, estado, pedidos));
            tira.cell(|ui| {
                let doc = estado.rascunho(&lotes).and_then(|r| r.documento.clone());
                if let Some(doc) = doc {
                    if let Some(abrir) =
                        previas.painel_ui(ui, leitor, Some(&doc), altura - 40.0, pedidos)
                    {
                        crate::navegacao::pedir_abrir(
                            ui.ctx(),
                            pedidos,
                            crate::navegacao::Destino::Item(abrir),
                            false,
                        );
                    }
                } else {
                    ui.label(
                        RichText::new("Este rascunho não tem arquivo para mostrar.")
                            .color(p.tinta_suave),
                    );
                }
            });
            tira.cell(|ui| formulario(ui, fonte, &lotes, estado, hoje, pedidos));
        });
}

fn faixa_de_escritas(ui: &mut Ui, pendentes: usize, recusadas: usize, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    let (fundo, borda, cor, icone) = if recusadas > 0 {
        (p.perigo_sutil, p.borda, p.perigo, "block")
    } else {
        (p.aviso_sutil, p.aviso_borda, p.aviso, "schedule")
    };
    Frame::new()
        .fill(fundo)
        .stroke(Stroke::new(1.0, borda))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(10, 6))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor(icone, 16.0, cor));
                let mut t = Vec::new();
                if pendentes > 0 {
                    t.push(format!("{pendentes} aguardando o servidor"));
                }
                if recusadas > 0 {
                    t.push(format!(
                        "{recusadas} {}",
                        if recusadas == 1 {
                            "recusada"
                        } else {
                            "recusadas"
                        }
                    ));
                }
                ui.label(RichText::new(t.join(" · ")).color(cor));
                componentes::link_para(
                    ui,
                    "Abrir Sincronização",
                    crate::navegacao::Destino::Area(sbf_inventario::consulta::Area::Sincronizacao),
                    pedidos,
                );
            });
        });
    ui.add_space(4.0);
}

fn lista(
    ui: &mut Ui,
    lotes: &[LoteDeRascunhos],
    estado: &mut EstadoDaCaixa,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    egui::ScrollArea::vertical()
        .id_salt("caixa-lista")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for l in lotes {
                let visiveis: Vec<&Rascunho> = l
                    .rascunhos
                    .iter()
                    .filter(|r| estado.mostrar_decididos || r.status.aberto())
                    .collect();
                if visiveis.is_empty() && !estado.mostrar_decididos {
                    continue;
                }
                componentes::cartao(ui, |ui| {
                    ui.set_width(ui.available_width());
                    ui.horizontal(|ui| {
                        ui.label(icones::icone_cor("inventory_2", 16.0, p.tinta));
                        ui.add(
                            egui::Label::new(
                                RichText::new(&l.nome)
                                    .font(FontId::new(13.5, familia_forte()))
                                    .color(p.tinta),
                            )
                            .truncate(),
                        );
                    });
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} · {} esperando", l.origem, l.n_abertos()))
                                .size(12.0)
                                .color(p.tinta_suave),
                        );
                    });
                    ui.add_enabled_ui(l.n_abertos() > 0, |ui| {
                        if ui
                            .button((
                                icones::icone_cor("check_circle", 15.0, p.tinta),
                                RichText::new("Aprovar lote"),
                            ))
                            .on_hover_text(
                                "Aprova todos os rascunhos abertos do lote, tudo ou nada (Shift+A)",
                            )
                            .clicked()
                        {
                            pedir(pedidos, PedidoDaCaixa::PedirAprovarLote(l.id.clone()));
                        }
                    });
                    ui.add_space(2.0);
                    for r in visiveis {
                        linha_do_rascunho(ui, r, estado);
                    }
                });
                ui.add_space(6.0);
            }
        });
}

fn linha_do_rascunho(ui: &mut Ui, r: &Rascunho, estado: &mut EstadoDaCaixa) {
    let p = tema::paleta(ui.ctx());
    let selecionada = estado.selecionado.as_deref() == Some(&r.id);
    let fundo = if selecionada {
        p.destaque_sutil
    } else {
        egui::Color32::TRANSPARENT
    };
    let quadro = Frame::new()
        .fill(fundo)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(6, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            let cor = if r.status.aberto() {
                p.tinta
            } else {
                p.tinta_suave
            };
            ui.horizontal(|ui| {
                ui.label(icones::icone_cor(icone_do_status(r.status), 16.0, cor));
                ui.vertical(|ui| {
                    ui.add(
                        egui::Label::new(RichText::new(&r.rotulo).color(cor).strong()).truncate(),
                    );
                    ui.horizontal(|ui| {
                        let valor = r
                            .campos
                            .valor_centavos
                            .map_or_else(|| "sem valor".to_owned(), formato::dinheiro);
                        ui.label(RichText::new(valor).size(12.0).color(p.tinta_suave));
                        let falta = r.campos.faltando();
                        if r.status.aberto() && !falta.is_empty() {
                            ui.label(
                                RichText::new(format!("· falta {}", falta.len()))
                                    .size(12.0)
                                    .color(p.aviso),
                            )
                            .on_hover_text(format!("Falta para lançar: {}", falta.join(", ")));
                        }
                        if !r.status.aberto() {
                            ui.label(
                                RichText::new(format!("· {}", r.status.rotulo()))
                                    .size(12.0)
                                    .color(p.tinta_suave),
                            );
                        }
                        if let Some(e) = r.envio {
                            marca(ui, e);
                        }
                    });
                });
            });
        });
    let resposta = ui
        .interact(
            quadro.response.rect,
            ui.id().with(("rascunho", &r.id)),
            Sense::click(),
        )
        .on_hover_cursor(egui::CursorIcon::PointingHand);
    if resposta.clicked() {
        estado.selecionado = Some(r.id.clone());
    }
}

fn campo_de_texto(ui: &mut Ui, rotulo: &str, valor: &mut String, falta: bool, dica: &str) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        componentes::rotulo_de_secao(ui, rotulo);
        if falta {
            ui.label(icones::icone_cor("warning", 14.0, p.aviso))
                .on_hover_text("Falta para lançar");
        }
    });
    ui.add(
        egui::TextEdit::singleline(valor)
            .hint_text(dica)
            .desired_width(f32::INFINITY),
    );
    ui.add_space(3.0);
}

fn seletor(
    ui: &mut Ui,
    rotulo: &str,
    id: &str,
    falta: bool,
    atual: &mut Option<String>,
    opcoes: &[(String, String)],
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        componentes::rotulo_de_secao(ui, rotulo);
        if falta {
            ui.label(icones::icone_cor("warning", 14.0, p.aviso))
                .on_hover_text("Falta para lançar");
        }
    });
    let texto = atual
        .as_ref()
        .and_then(|a| opcoes.iter().find(|(i, _)| i == a))
        .map_or("Escolha…".to_owned(), |(_, n)| n.clone());
    egui::ComboBox::from_id_salt(id)
        .selected_text(texto)
        .width(ui.available_width() - 8.0)
        .show_ui(ui, |ui| {
            for (i, n) in opcoes {
                ui.selectable_value(atual, Some(i.clone()), n);
            }
        });
    ui.add_space(3.0);
}

fn formulario(
    ui: &mut Ui,
    fonte: &dyn FonteDaCaixa,
    lotes: &[LoteDeRascunhos],
    estado: &mut EstadoDaCaixa,
    _hoje: &str,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let Some(r) = estado.rascunho(lotes).cloned() else {
        ui.label(RichText::new("Selecione um rascunho.").color(p.tinta_suave));
        return;
    };
    let contas: Vec<(String, String)> =
        fonte.contas().into_iter().map(|o| (o.id, o.nome)).collect();
    let aberto = r.status.aberto();
    let Some(f) = estado.formulario.as_mut() else {
        return;
    };
    let faltando = r.campos.faltando();
    let validacao = f.campos();
    // O que falta considera o que você digitou, não só o que está gravado.
    let falta_agora = validacao
        .as_ref()
        .map_or(faltando.clone(), |c| c.faltando());
    let sujo = f.sujo();
    egui::ScrollArea::vertical()
        .id_salt("caixa-formulario")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                componentes::titulo(ui, "Corrigir", 16.0);
                ui.label(RichText::new(format!("· {}", r.status.rotulo())).color(p.tinta_suave));
                if let Some(e) = r.envio {
                    marca(ui, e);
                }
            });
            if let Some(m) = &r.motivo_da_recusa {
                ui.label(RichText::new(format!("Recusado: {m}")).color(p.tinta_suave));
            }
            if aberto && !falta_agora.is_empty() {
                componentes::aviso(
                    ui,
                    "warning",
                    &format!("Falta para lançar: {}.", falta_agora.join(", ")),
                    p.aviso_sutil,
                    p.aviso_borda,
                    p.aviso,
                );
                ui.add_space(4.0);
            }
            if let Err(erros) = &validacao {
                componentes::aviso(
                    ui,
                    "error",
                    &format!("Confira: {}.", erros.join("; ")),
                    p.perigo_sutil,
                    p.borda,
                    p.perigo,
                );
                ui.add_space(4.0);
            }
            ui.add_enabled_ui(aberto, |ui| {
                campo_de_texto(ui, "Descrição", &mut f.descricao, false, "o que foi");
                campo_de_texto(ui, "Fornecedor", &mut f.fornecedor, false, "quem cobrou");
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        componentes::rotulo_de_secao(ui, "Direção");
                        egui::ComboBox::from_id_salt("caixa-direcao")
                            .selected_text(f.direcao.rotulo())
                            .width(110.0)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut f.direcao, Direcao::Saida, "Saída");
                                ui.selectable_value(&mut f.direcao, Direcao::Entrada, "Entrada");
                            });
                    });
                    ui.vertical(|ui| {
                        let tipos: Vec<(String, String)> = TIPOS_DE_LANCAMENTO
                            .iter()
                            .map(|(i, n)| ((*i).to_owned(), (*n).to_owned()))
                            .collect();
                        seletor(
                            ui,
                            "Tipo",
                            "caixa-tipo",
                            falta_agora.contains(&"Tipo"),
                            &mut f.tipo,
                            &tipos,
                        );
                    });
                });
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.set_width(130.0);
                        campo_de_texto(
                            ui,
                            "Valor (R$)",
                            &mut f.valor,
                            falta_agora.contains(&"Valor"),
                            "312,44",
                        );
                    });
                    ui.vertical(|ui| {
                        ui.set_width(130.0);
                        campo_de_texto(
                            ui,
                            "Data do evento",
                            &mut f.data_do_evento,
                            falta_agora.contains(&"Data do evento"),
                            "dd/mm/aaaa",
                        );
                    });
                });
                campo_de_texto(ui, "Vencimento", &mut f.vencimento, false, "dd/mm/aaaa");
                seletor(
                    ui,
                    "Conta ou cartão",
                    "caixa-conta",
                    falta_agora.contains(&"Conta ou cartão"),
                    &mut f.conta,
                    &contas,
                );
                campo_de_texto(
                    ui,
                    "Categoria (sugestão)",
                    &mut f.categoria,
                    false,
                    "Casa, Mercado…",
                );
                campo_de_texto(
                    ui,
                    "Número do documento",
                    &mut f.numero_do_documento,
                    false,
                    "",
                );
                componentes::rotulo_de_secao(ui, "Observações");
                ui.add(
                    egui::TextEdit::multiline(&mut f.observacoes)
                        .desired_rows(2)
                        .desired_width(f32::INFINITY),
                );
            });
            ui.add_space(8.0);
            ui.horizontal_wrapped(|ui| {
                let pode_aprovar = aberto && validacao.is_ok() && falta_agora.is_empty();
                let aprovar = ui
                    .add_enabled(
                        pode_aprovar,
                        egui::Button::new((
                            icones::icone_cor("check", 16.0, p.sobre_destaque),
                            RichText::new("Aprovar  (A)").color(p.sobre_destaque),
                        ))
                        .fill(p.destaque),
                    )
                    .on_hover_text("Lança no livro-caixa; o servidor aceita ou recusa")
                    .on_disabled_hover_text(if aberto {
                        "Falta preencher ou corrigir campos"
                    } else {
                        "Já decidido"
                    });
                if aprovar.clicked() {
                    for pedido in pedidos_de_aprovar_com(&r.id, f) {
                        pedir(pedidos, pedido);
                    }
                }
                if ui
                    .add_enabled(
                        aberto && sujo && validacao.is_ok(),
                        egui::Button::new((
                            icones::icone_cor("edit_note", 16.0, p.tinta),
                            RichText::new("Salvar correção"),
                        )),
                    )
                    .clicked()
                    && let Ok(campos) = f.campos()
                {
                    pedir(
                        pedidos,
                        PedidoDaCaixa::Corrigir {
                            id: r.id.clone(),
                            campos,
                        },
                    );
                }
                if ui
                    .add_enabled(
                        aberto,
                        egui::Button::new((
                            icones::icone_cor("close", 16.0, p.perigo),
                            RichText::new("Recusar  (R)"),
                        )),
                    )
                    .clicked()
                {
                    pedir(pedidos, PedidoDaCaixa::PedirRecusa(r.id.clone()));
                }
            });
        });
}

fn pedidos_de_aprovar_com(id: &str, f: &FormularioDoRascunho) -> Vec<PedidoDaCaixa> {
    let mut v = Vec::new();
    if f.sujo()
        && let Ok(campos) = f.campos()
    {
        v.push(PedidoDaCaixa::Corrigir {
            id: id.to_owned(),
            campos,
        });
    }
    v.push(PedidoDaCaixa::Aprovar(id.to_owned()));
    v
}

// ---------------------------------------------------------------------------
// Páginas de dinheiro: Editar, Ratear, Excluir
// ---------------------------------------------------------------------------

/// A marca de pendente e os botões Editar, Ratear e Excluir da página de uma movimentação.
pub fn acoes_da_movimentacao(
    ui: &mut Ui,
    fonte: &dyn FonteDaCaixa,
    alvo: &Ref,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let existe = fonte.movimentacao(alvo).ok().flatten().is_some();
    let envio = crate::caixa::envio_da_movimentacao(&fonte.escritas().unwrap_or_default(), alvo);
    if let Some(e) = envio {
        marca(ui, e);
    }
    for (icone, rotulo, dica, p_) in [
        (
            "delete",
            "Excluir",
            "Exclui a movimentação e os rateios dela (pede confirmação)",
            PedidoDaCaixa::PedirExclusao(alvo.clone()),
        ),
        (
            "call_split",
            "Ratear",
            "Divide o valor entre centros; a soma tem de bater",
            PedidoDaCaixa::PedirRateio(alvo.clone()),
        ),
        (
            "edit",
            "Editar",
            "Descrição, data, valor, conta, fornecedor, centro e notas",
            PedidoDaCaixa::PedirEdicao(alvo.clone()),
        ),
    ] {
        if ui
            .add_enabled(
                existe,
                egui::Button::new((
                    icones::icone_cor(icone, 15.0, p.tinta),
                    RichText::new(rotulo),
                )),
            )
            .on_hover_text(dica)
            .on_disabled_hover_text("A movimentação não está na cópia local")
            .clicked()
        {
            pedir(pedidos, p_);
        }
    }
}

/// Uma faixa curta nas áreas de Dinheiro: quantas escritas esperam o servidor ou foram recusadas.
pub fn faixa_de_pendentes(ui: &mut Ui, fonte: &dyn FonteDaCaixa, pedidos: &mut Vec<Pedido>) {
    let (pendentes, recusadas) = crate::caixa::contar(&fonte.escritas().unwrap_or_default());
    if pendentes + recusadas > 0 {
        faixa_de_escritas(ui, pendentes, recusadas, pedidos);
    }
}

// ---------------------------------------------------------------------------
// Diálogos
// ---------------------------------------------------------------------------

/// Uma linha do diálogo de rateio.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LinhaDeRateio {
    pub centro: Option<String>,
    pub valor: String,
}

/// Os diálogos da caixa (a janela os desenha dentro do modal dela).
#[derive(Debug, Clone, PartialEq)]
pub enum DialogoDaCaixa {
    Recusar {
        id: String,
        rotulo: String,
        motivo: String,
    },
    AprovarLote {
        id: String,
        nome: String,
        abertos: usize,
        /// "arquivo.pdf: Conta ou cartão, Valor": o que impede o lote de passar.
        incompletos: Vec<String>,
    },
    Editar {
        alvo: Ref,
        descricao: String,
        data: String,
        valor: String,
        conta: Option<String>,
        fornecedor: String,
        centro: Option<String>,
        notas: String,
        contas: Vec<Opcao>,
        centros: Vec<Opcao>,
        tem_rateio: bool,
    },
    Ratear {
        alvo: Ref,
        rotulo: String,
        total: i64,
        linhas: Vec<LinhaDeRateio>,
        centros: Vec<Opcao>,
    },
    Excluir {
        alvo: Ref,
        rotulo: String,
        n_rateios: usize,
    },
}

impl DialogoDaCaixa {
    pub fn titulo(&self) -> &'static str {
        match self {
            DialogoDaCaixa::Recusar { .. } => "Recusar rascunho",
            DialogoDaCaixa::AprovarLote { .. } => "Aprovar lote",
            DialogoDaCaixa::Editar { .. } => "Editar movimentação",
            DialogoDaCaixa::Ratear { .. } => "Ratear movimentação",
            DialogoDaCaixa::Excluir { .. } => "Excluir movimentação",
        }
    }

    /// O rótulo do botão que confirma.
    pub fn rotulo_do_botao(&self) -> &'static str {
        match self {
            DialogoDaCaixa::Excluir { .. } => "Excluir",
            DialogoDaCaixa::Recusar { .. } => "Recusar",
            DialogoDaCaixa::AprovarLote { .. } => "Aprovar lote",
            _ => "Confirmar",
        }
    }

    /// Os pedidos do diálogo, depois de validar (nada inválido sai daqui).
    pub fn confirmar(&self) -> Result<PedidoDaCaixa, String> {
        match self {
            DialogoDaCaixa::Recusar { id, motivo, .. } => {
                if motivo.trim().is_empty() {
                    return Err("Diga por que está recusando.".to_owned());
                }
                Ok(PedidoDaCaixa::Recusar {
                    id: id.clone(),
                    motivo: motivo.trim().to_owned(),
                })
            }
            DialogoDaCaixa::AprovarLote {
                id, incompletos, ..
            } => {
                if !incompletos.is_empty() {
                    return Err(format!(
                        "O lote tem rascunhos incompletos ({}). Corrija-os antes: nenhum seria aprovado.",
                        incompletos.len()
                    ));
                }
                Ok(PedidoDaCaixa::AprovarLote(id.clone()))
            }
            DialogoDaCaixa::Editar {
                alvo,
                descricao,
                data,
                valor,
                conta,
                fornecedor,
                centro,
                notas,
                ..
            } => {
                if descricao.trim().is_empty() {
                    return Err("A descrição não pode ficar vazia.".to_owned());
                }
                let data = data_iso(data.trim()).ok_or("Data inválida (use dd/mm/aaaa).")?;
                let valor = centavos_de(valor.trim())
                    .filter(|v| *v != 0)
                    .ok_or("Valor inválido (ex.: -312,44).")?;
                Ok(PedidoDaCaixa::Editar {
                    alvo: alvo.clone(),
                    campos: CamposDaMovimentacao {
                        descricao: descricao.trim().to_owned(),
                        data,
                        valor_centavos: valor,
                        conta: conta.clone(),
                        fornecedor: opcional(fornecedor),
                        centro: centro.clone(),
                        notas: opcional(notas),
                    },
                })
            }
            DialogoDaCaixa::Ratear {
                alvo,
                total,
                linhas,
                ..
            } => {
                let linhas = linhas_de_rateio(linhas)?;
                validar_rateio(*total, &linhas)?;
                Ok(PedidoDaCaixa::Ratear {
                    alvo: alvo.clone(),
                    linhas,
                })
            }
            DialogoDaCaixa::Excluir { alvo, .. } => Ok(PedidoDaCaixa::Excluir(alvo.clone())),
        }
    }
}

/// Os textos das linhas viram centavos (linha vazia some; texto inválido é erro).
pub fn linhas_de_rateio(linhas: &[LinhaDeRateio]) -> Result<Vec<RateioDaMovimentacao>, String> {
    let mut v = Vec::new();
    for l in linhas {
        if l.centro.is_none() && l.valor.trim().is_empty() {
            continue;
        }
        let centro = l.centro.clone().ok_or("Escolha o centro de cada linha.")?;
        let centavos = centavos_de(l.valor.trim())
            .ok_or_else(|| format!("Valor inválido: \u{201c}{}\u{201d}.", l.valor))?;
        v.push(RateioDaMovimentacao { centro, centavos });
    }
    Ok(v)
}

/// A situação do rateio digitado: soma, falta e se bate (para o resumo vivo do diálogo).
pub fn resumo_do_rateio(total: i64, linhas: &[LinhaDeRateio]) -> (i64, Result<(), String>) {
    let soma: i64 = linhas
        .iter()
        .filter_map(|l| centavos_de(l.valor.trim()))
        .sum();
    let r = linhas_de_rateio(linhas).and_then(|v| validar_rateio(total, &v));
    (soma, r)
}

/// Desenha o corpo do diálogo. Devolve `true` se Enter pediu para confirmar.
pub fn desenhar_dialogo(ui: &mut Ui, d: &mut DialogoDaCaixa) -> bool {
    let p = tema::paleta(ui.ctx());
    let mut confirmar = false;
    componentes::titulo(ui, d.titulo(), 17.0);
    match d {
        DialogoDaCaixa::Recusar { rotulo, motivo, .. } => {
            ui.label(rotulo.as_str());
            ui.label("Motivo (obrigatório)");
            let r = ui.add(
                egui::TextEdit::multiline(motivo)
                    .desired_rows(3)
                    .desired_width(f32::INFINITY),
            );
            r.request_focus();
        }
        DialogoDaCaixa::AprovarLote {
            nome,
            abertos,
            incompletos,
            ..
        } => {
            ui.label(format!(
                "Aprovar os {abertos} rascunhos abertos do lote \u{201c}{nome}\u{201d}? É tudo ou nada: se o servidor reprovar um, nenhum é lançado."
            ));
            if !incompletos.is_empty() {
                ui.add_space(4.0);
                componentes::aviso(
                    ui,
                    "warning",
                    &format!(
                        "{} incompleto{}: {}",
                        incompletos.len(),
                        if incompletos.len() == 1 { "" } else { "s" },
                        incompletos.join(" · ")
                    ),
                    p.aviso_sutil,
                    p.aviso_borda,
                    p.aviso,
                );
            }
        }
        DialogoDaCaixa::Editar {
            descricao,
            data,
            valor,
            conta,
            fornecedor,
            centro,
            notas,
            contas,
            centros,
            tem_rateio,
            ..
        } => {
            ui.label(
                RichText::new("A mudança fica pendente e vai ao servidor na próxima sincronização; ele aplica ou recusa.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
            campo_de_texto(ui, "Descrição", descricao, false, "");
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    campo_de_texto(ui, "Data", data, false, "dd/mm/aaaa");
                });
                ui.vertical(|ui| {
                    ui.set_width(150.0);
                    campo_de_texto(ui, "Valor (R$, negativo = saiu)", valor, false, "-312,44");
                });
            });
            if *tem_rateio {
                ui.label(
                    RichText::new("Tem rateio: mudar o valor exige refazer o rateio antes.")
                        .size(12.0)
                        .color(p.aviso),
                );
            }
            let contas: Vec<(String, String)> = contas
                .iter()
                .map(|o| (o.id.clone(), o.nome.clone()))
                .collect();
            seletor(ui, "Conta", "editar-conta", false, conta, &contas);
            campo_de_texto(ui, "Fornecedor", fornecedor, false, "");
            let centros: Vec<(String, String)> = centros
                .iter()
                .map(|o| (o.id.clone(), o.nome.clone()))
                .collect();
            seletor(ui, "Centro", "editar-centro", false, centro, &centros);
            componentes::rotulo_de_secao(ui, "Notas");
            ui.add(
                egui::TextEdit::multiline(notas)
                    .desired_rows(2)
                    .desired_width(f32::INFINITY),
            );
        }
        DialogoDaCaixa::Ratear {
            rotulo,
            total,
            linhas,
            centros,
            ..
        } => {
            ui.label(format!(
                "{rotulo} · valor {}",
                formato::dinheiro(total.abs())
            ));
            ui.label(
                RichText::new("Troca o conjunto de rateios inteiro. A soma tem de bater com o valor; confiro antes de enviar.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
            ui.add_space(4.0);
            let opcoes: Vec<(String, String)> = centros
                .iter()
                .map(|o| (o.id.clone(), o.nome.clone()))
                .collect();
            let mut remover = None;
            for (i, l) in linhas.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    let texto = l
                        .centro
                        .as_ref()
                        .and_then(|c| opcoes.iter().find(|(id, _)| id == c))
                        .map_or("Centro…".to_owned(), |(_, n)| n.clone());
                    egui::ComboBox::from_id_salt(("rateio-centro", i))
                        .selected_text(texto)
                        .width(170.0)
                        .show_ui(ui, |ui| {
                            for (id, n) in &opcoes {
                                ui.selectable_value(&mut l.centro, Some(id.clone()), n);
                            }
                        });
                    ui.add(
                        egui::TextEdit::singleline(&mut l.valor)
                            .hint_text("0,00")
                            .desired_width(100.0),
                    );
                    if ui.small_button("remover").clicked() {
                        remover = Some(i);
                    }
                });
            }
            if let Some(i) = remover {
                linhas.remove(i);
            }
            if ui.button("Adicionar linha").clicked() {
                linhas.push(LinhaDeRateio {
                    centro: None,
                    valor: String::new(),
                });
            }
            ui.add_space(4.0);
            let (soma, r) = resumo_do_rateio(*total, linhas);
            let (icone, cor, t) = match r {
                Ok(()) => (
                    "check_circle",
                    p.sucesso,
                    "A soma bate com o valor.".to_owned(),
                ),
                Err(e) => ("warning", p.aviso, e),
            };
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor(icone, 15.0, cor));
                ui.label(
                    RichText::new(format!("Soma {} · {t}", formato::dinheiro(soma))).color(cor),
                );
            });
        }
        DialogoDaCaixa::Excluir {
            rotulo, n_rateios, ..
        } => {
            ui.label(format!(
                "Excluir \u{201c}{rotulo}\u{201d}?{}",
                if *n_rateios > 0 {
                    format!(" Os {n_rateios} rateios dela também são apagados.")
                } else {
                    String::new()
                }
            ));
            ui.label(
                RichText::new("A exclusão fica pendente até o servidor aceitar. Se ele recusar, a movimentação volta.")
                    .size(12.0)
                    .color(p.tinta_suave),
            );
        }
    }
    if ui.input(|i| i.key_pressed(Key::Enter) && i.modifiers.command) {
        confirmar = true;
    }
    confirmar
}

/// Monta o diálogo de Editar para uma movimentação.
pub fn dialogo_de_edicao(fonte: &dyn FonteDaCaixa, alvo: &Ref) -> Result<DialogoDaCaixa, String> {
    let m = fonte
        .movimentacao(alvo)
        .map_err(|e| e.to_string())?
        .ok_or("A movimentação não está na cópia local.")?;
    Ok(DialogoDaCaixa::Editar {
        alvo: alvo.clone(),
        descricao: m.campos.descricao,
        data: formato::data(&m.campos.data),
        valor: centavos_em_texto(m.campos.valor_centavos),
        conta: m.campos.conta,
        fornecedor: texto(&m.campos.fornecedor),
        centro: m.campos.centro,
        notas: texto(&m.campos.notas),
        contas: fonte.contas(),
        centros: fonte.centros(),
        tem_rateio: !m.rateios.is_empty(),
    })
}

/// Monta o diálogo de Ratear (começa com os rateios que já existem, ou duas linhas vazias).
pub fn dialogo_de_rateio(fonte: &dyn FonteDaCaixa, alvo: &Ref) -> Result<DialogoDaCaixa, String> {
    let m = fonte
        .movimentacao(alvo)
        .map_err(|e| e.to_string())?
        .ok_or("A movimentação não está na cópia local.")?;
    let mut linhas: Vec<LinhaDeRateio> = m
        .rateios
        .iter()
        .map(|r| LinhaDeRateio {
            centro: Some(r.centro.clone()),
            valor: centavos_em_texto(r.centavos),
        })
        .collect();
    while linhas.len() < 2 {
        linhas.push(LinhaDeRateio {
            centro: None,
            valor: String::new(),
        });
    }
    Ok(DialogoDaCaixa::Ratear {
        alvo: alvo.clone(),
        rotulo: format!("{} · {}", m.campos.descricao, formato::data(&m.campos.data)),
        total: m.campos.valor_centavos,
        linhas,
        centros: fonte.centros(),
    })
}

/// Monta o diálogo de Excluir.
pub fn dialogo_de_exclusao(fonte: &dyn FonteDaCaixa, alvo: &Ref) -> Result<DialogoDaCaixa, String> {
    let m = fonte
        .movimentacao(alvo)
        .map_err(|e| e.to_string())?
        .ok_or("A movimentação não está na cópia local.")?;
    Ok(DialogoDaCaixa::Excluir {
        alvo: alvo.clone(),
        rotulo: format!("{} · {}", m.campos.descricao, formato::data(&m.campos.data)),
        n_rateios: m.rateios.len(),
    })
}

/// Monta o diálogo do lote (com o que falta nos incompletos).
pub fn dialogo_de_lote(fonte: &dyn FonteDaCaixa, id: &str) -> Result<DialogoDaCaixa, String> {
    let lotes = fonte.lotes().map_err(|e| e.to_string())?;
    let l = lotes
        .iter()
        .find(|l| l.id == id)
        .ok_or("Lote não encontrado.")?;
    let incompletos = l
        .abertos()
        .filter_map(|r| {
            let f = r.campos.faltando();
            (!f.is_empty()).then(|| format!("{}: {}", r.rotulo, f.join(", ")))
        })
        .collect();
    Ok(DialogoDaCaixa::AprovarLote {
        id: id.to_owned(),
        nome: l.nome.clone(),
        abertos: l.n_abertos(),
        incompletos,
    })
}

// ---------------------------------------------------------------------------
// Sincronização
// ---------------------------------------------------------------------------

fn rotulo_do_alvo(e: &Escrita) -> &'static str {
    match e.alvo {
        AlvoDaEscrita::Rascunho(_) => "Caixa de entrada",
        AlvoDaEscrita::Lote(_) => "Caixa de entrada · lote",
        AlvoDaEscrita::Movimentacao(_) => "Movimentações",
    }
}

/// As seções da área Sincronização para a caixa e a edição: a fila pendente e as recusas
/// lado a lado, com Refazer e Descartar.
pub fn secoes_da_sincronizacao(
    ui: &mut Ui,
    fonte: &dyn FonteDaCaixa,
    hoje: &str,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let escritas = fonte.escritas().unwrap_or_default();
    let pendentes: Vec<&Escrita> = escritas
        .iter()
        .filter(|e| e.estado == EstadoDeEnvio::Pendente)
        .collect();
    let recusadas: Vec<&Escrita> = escritas
        .iter()
        .filter(|e| e.estado == EstadoDeEnvio::Recusada)
        .collect();
    componentes::rotulo_de_secao(
        ui,
        &format!("Fila de escritas · {} aguardando", pendentes.len()),
    );
    if pendentes.is_empty() {
        ui.label(RichText::new("Nenhuma escrita esperando o servidor.").color(p.tinta_suave));
    } else {
        ui.label(
            RichText::new("Aprovações, correções e edições que vão na próxima sincronização, em ordem. O servidor aplica cada uma inteira ou recusa.")
                .size(12.0)
                .color(p.tinta_suave),
        );
        for e in pendentes.iter().rev() {
            ui.horizontal_wrapped(|ui| {
                marca(ui, e.estado);
                ui.label(RichText::new(&e.o_que).strong());
                ui.label(
                    RichText::new(format!(
                        "· {} · {} · feita {}",
                        e.rotulo_do_alvo,
                        rotulo_do_alvo(e),
                        dinheiro::hora(&e.feita_em, hoje)
                    ))
                    .color(p.tinta_suave),
                );
            });
        }
    }
    ui.add_space(10.0);
    componentes::rotulo_de_secao(ui, &format!("Escritas recusadas · {}", recusadas.len()));
    if recusadas.is_empty() {
        ui.label(RichText::new("Nenhuma escrita recusada.").color(p.tinta_suave));
    }
    for e in recusadas {
        componentes::cartao(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(|ui| {
                marca(ui, e.estado);
                ui.label(RichText::new(&e.o_que).strong());
                ui.label(
                    RichText::new(format!(
                        "· {} · feita {}",
                        e.rotulo_do_alvo,
                        dinheiro::hora(&e.feita_em, hoje)
                    ))
                    .color(p.tinta_suave),
                );
            });
            ui.add_space(4.0);
            if let Some(r) = &e.recusa {
                dinheiro::lado_a_lado_de_campos(
                    ui,
                    &e.id,
                    &r.motivo,
                    &r.campos,
                    Pedido::Caixa(PedidoDaCaixa::RefazerEscrita(e.id.clone())),
                    Pedido::Caixa(PedidoDaCaixa::DescartarEscrita(e.id.clone())),
                    pedidos,
                );
            }
        });
        ui.add_space(6.0);
    }
    ui.add_space(8.0);
}

// ---------------------------------------------------------------------------
// Envio de lote do inventário
// ---------------------------------------------------------------------------

/// A barra de progresso do envio.
pub fn barra_de_progresso(ui: &mut Ui, p: &ProgressoDoEnvio) {
    ui.add(
        egui::ProgressBar::new(p.fracao())
            .desired_width(220.0)
            .text(format!("{} de {} documentos", p.enviados, p.total)),
    );
}

/// O botão Enviar ao servidor e o andamento, na faixa do lote.
pub fn envio_do_lote(ui: &mut Ui, fonte: &dyn FonteDaCaixa, lote: &Ref, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    let envio = fonte.envio_do_lote(lote);
    let (habilitado, dica) = match &envio {
        EnvioDoLote::Indisponivel(m) => (false, m.clone()),
        EnvioDoLote::Pronto { documentos } => (
            true,
            format!(
                "Sobe {documentos} documentos; o servidor cria os registros e o pipeline os processa"
            ),
        ),
        EnvioDoLote::Enviando(_) => (false, "Enviando".to_owned()),
        EnvioDoLote::Enviado { .. } => (false, "Já enviado".to_owned()),
        EnvioDoLote::Falhou { .. } => (true, "Retoma de onde parou".to_owned()),
    };
    let rotulo = if matches!(envio, EnvioDoLote::Falhou { .. }) {
        "Retomar envio"
    } else {
        "Enviar ao servidor"
    };
    if ui
        .add_enabled(
            habilitado,
            egui::Button::new((
                icones::icone_cor(
                    "send",
                    15.0,
                    if habilitado {
                        p.sobre_destaque
                    } else {
                        p.tinta_desabilitada
                    },
                ),
                RichText::new(rotulo).color(if habilitado {
                    p.sobre_destaque
                } else {
                    p.tinta_desabilitada
                }),
            ))
            .fill(if habilitado {
                p.destaque
            } else {
                p.superficie_2
            }),
        )
        .on_hover_text(&dica)
        .on_disabled_hover_text(&dica)
        .clicked()
    {
        pedir(pedidos, PedidoDaCaixa::EnviarLote(lote.clone()));
    }
    match &envio {
        EnvioDoLote::Enviando(prog) => {
            barra_de_progresso(ui, prog);
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_millis(150));
        }
        EnvioDoLote::Enviado { documentos } => {
            ui.label(icones::icone_cor("check_circle", 15.0, p.sucesso));
            ui.label(
                RichText::new(format!(
                    "{documentos} documentos aceitos; os registros ficaram promovidos"
                ))
                .color(p.sucesso),
            );
        }
        EnvioDoLote::Falhou { progresso, motivo } => {
            barra_de_progresso(ui, progresso);
            ui.label(icones::icone_cor("error", 15.0, p.perigo));
            ui.label(RichText::new(format!("Parou: {motivo}")).color(p.perigo));
        }
        EnvioDoLote::Indisponivel(m) => {
            ui.label(RichText::new(m).size(12.0).color(p.tinta_suave));
        }
        EnvioDoLote::Pronto { .. } => {}
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    fn r(id: &str, c: CamposDoRascunho) -> Rascunho {
        Rascunho {
            id: id.into(),
            lote: "L".into(),
            rotulo: "a.pdf".into(),
            documento: None,
            status: StatusDoRascunho::EmRevisao,
            envio: None,
            campos: c,
            motivo_da_recusa: None,
        }
    }

    fn completo() -> CamposDoRascunho {
        CamposDoRascunho {
            tipo: Some("expense".into()),
            valor_centavos: Some(31_244),
            data_do_evento: Some("2026-11-05".into()),
            conta: Some("conta-corrente".into()),
            descricao: Some("Energia".into()),
            ..Default::default()
        }
    }

    #[test]
    fn formulario_ida_e_volta_sem_mudar_nada() {
        let rasc = r("a", completo());
        let f = FormularioDoRascunho::de(&rasc);
        assert_eq!(f.valor, "312,44");
        assert_eq!(f.data_do_evento, "05/11/2026");
        assert_eq!(f.campos().unwrap(), rasc.campos);
        assert!(!f.sujo());
    }

    #[test]
    fn formulario_com_valor_ou_data_invalidos_nao_gera_campos() {
        let mut f = FormularioDoRascunho::de(&r("a", completo()));
        f.valor = "abc".into();
        f.data_do_evento = "31/31/2026".into();
        let erros = f.campos().unwrap_err();
        assert_eq!(erros.len(), 2);
        assert!(f.sujo());
        f.valor = "-5,00".into();
        assert!(f.campos().is_err(), "o valor do rascunho é sem sinal");
    }

    #[test]
    fn aprovar_corrige_antes_so_se_mexeu() {
        let rasc = r("a", completo());
        let mut e = EstadoDaCaixa {
            formulario: Some(FormularioDoRascunho::de(&rasc)),
            ..Default::default()
        };
        assert_eq!(
            pedidos_de_aprovar(&e, "a"),
            vec![PedidoDaCaixa::Aprovar("a".into())]
        );
        e.formulario.as_mut().unwrap().valor = "400,00".into();
        let v = pedidos_de_aprovar(&e, "a");
        assert_eq!(v.len(), 2);
        assert!(
            matches!(&v[0], PedidoDaCaixa::Corrigir { campos, .. } if campos.valor_centavos == Some(40_000))
        );
        assert_eq!(v[1], PedidoDaCaixa::Aprovar("a".into()));
    }

    #[test]
    fn dialogo_de_rateio_valida_antes_de_enviar() {
        let alvo = Ref::nova(sbf_inventario::Tipo::Movimentacao, "m");
        let mut d = DialogoDaCaixa::Ratear {
            alvo: alvo.clone(),
            rotulo: "x".into(),
            total: -10_000,
            linhas: vec![
                LinhaDeRateio {
                    centro: Some("casa".into()),
                    valor: "60,00".into(),
                },
                LinhaDeRateio {
                    centro: Some("lazer".into()),
                    valor: "39,99".into(),
                },
            ],
            centros: vec![],
        };
        let erro = d.confirmar().unwrap_err();
        assert!(erro.contains("faltam"), "{erro}");
        if let DialogoDaCaixa::Ratear { linhas, .. } = &mut d {
            linhas[1].valor = "40,00".into();
        }
        assert!(matches!(d.confirmar(), Ok(PedidoDaCaixa::Ratear { .. })));
    }

    #[test]
    fn recusar_exige_motivo_e_lote_incompleto_nao_confirma() {
        let d = DialogoDaCaixa::Recusar {
            id: "a".into(),
            rotulo: "a.pdf".into(),
            motivo: " ".into(),
        };
        assert!(d.confirmar().is_err());
        let d = DialogoDaCaixa::AprovarLote {
            id: "L".into(),
            nome: "n".into(),
            abertos: 2,
            incompletos: vec!["b.pdf: Valor".into()],
        };
        assert!(d.confirmar().is_err());
    }

    #[test]
    fn editar_pede_data_e_valor_validos() {
        let alvo = Ref::nova(sbf_inventario::Tipo::Movimentacao, "m");
        let mut d = DialogoDaCaixa::Editar {
            alvo,
            descricao: "Mercado".into(),
            data: "28/10/2026".into(),
            valor: "-458,10".into(),
            conta: None,
            fornecedor: String::new(),
            centro: None,
            notas: String::new(),
            contas: vec![],
            centros: vec![],
            tem_rateio: false,
        };
        let PedidoDaCaixa::Editar { campos, .. } = d.confirmar().unwrap() else {
            panic!()
        };
        assert_eq!(campos.valor_centavos, -45_810);
        assert_eq!(campos.data, "2026-10-28");
        if let DialogoDaCaixa::Editar { data, .. } = &mut d {
            *data = "ontem".into();
        }
        assert!(d.confirmar().is_err());
    }

    #[test]
    fn centavos_em_texto_nao_usa_ponto_flutuante() {
        assert_eq!(centavos_em_texto(5), "0,05");
        assert_eq!(centavos_em_texto(-45_810), "-458,10");
        assert_eq!(crate::caixa::rotulo_do_tipo("income"), "Receita");
    }
}
