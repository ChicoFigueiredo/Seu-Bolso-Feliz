//! A tela "Contas do planejamento": uma linha por apelido da planilha (`CX-0001`), com a conta
//! do extrato a que ele corresponde. O histórico **propõe** o par e o usuário confirma cada
//! um; nada é ligado sozinho. Só lê por `&dyn Planejamento`; escrever é um pedido
//! (`MapearConta`) que a janela aplica fora do desenho.

use egui::{CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, Ui};
use sbf_planejamento::{Alteracao, ContaDoPlanejamento, ParDeConta, Planejamento};

use crate::componentes;
use crate::controle_ui::{EstadoDoControle, PedidoDoControle, mensagem, texto_pequeno};
use crate::icones;
use crate::navegacao::Pedido;
use crate::tema;

/// O destino da tela na navegação.
pub const ROTA: &str = "contas-do-planejamento";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DadosDasContas {
    pub contas: Vec<ContaDoPlanejamento>,
    pub sugestoes: Vec<ParDeConta>,
    pub do_extrato: Vec<String>,
}

impl DadosDasContas {
    /// A sugestão do histórico para um apelido ainda sem par.
    pub fn sugestao_de(&self, apelido: &str) -> Option<&ParDeConta> {
        self.sugestoes.iter().find(|s| s.apelido == apelido)
    }
}

pub fn carregar(plan: &dyn Planejamento) -> Result<DadosDasContas, String> {
    Ok(DadosDasContas {
        contas: plan.contas().map_err(|e| e.to_string())?,
        sugestoes: plan.pares_de_conta_sugeridos().map_err(|e| e.to_string())?,
        do_extrato: plan.contas_do_extrato().map_err(|e| e.to_string())?,
    })
}

/// O que a tela guarda entre quadros.
#[derive(Debug, Default)]
pub struct EstadoDasContas {
    cache: Option<(u64, Result<DadosDasContas, String>)>,
    /// O apelido cuja lista de contas do extrato está aberta ("Escolher…").
    pub escolhendo: Option<String>,
}

impl EstadoDasContas {
    fn garantir(&mut self, plan: &dyn Planejamento, geracao: u64) {
        if self.cache.as_ref().is_none_or(|(g, _)| *g != geracao) {
            self.cache = Some((geracao, carregar(plan)));
        }
    }

    pub fn dados(&self) -> Option<&Result<DadosDasContas, String>> {
        self.cache.as_ref().map(|(_, d)| d)
    }
}

/// O pedido que confirma (`Some`) ou desliga (`None`) o par de um apelido.
pub fn pedido_de_par(apelido: &str, conta: Option<&str>) -> Pedido {
    Pedido::Controle(PedidoDoControle::AplicarVarias(vec![
        Alteracao::MapearConta {
            apelido: apelido.to_owned(),
            conta_do_extrato: conta.map(str::to_owned),
        },
    ]))
}

/// Desenha a tela.
pub fn area(
    ui: &mut Ui,
    plan: &dyn Planejamento,
    estado: &mut EstadoDoControle,
    pedidos: &mut Vec<Pedido>,
) {
    let geracao = estado.geracao();
    let mut local = std::mem::take(&mut estado.contas);
    local.garantir(plan, geracao);
    let p = tema::paleta(ui.ctx());
    ScrollArea::vertical()
        .id_salt("contas-do-planejamento")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            componentes::titulo(ui, "Contas do planejamento", 20.0);
            texto_pequeno(
                ui,
                "Liga o apelido da planilha à conta do extrato. O histórico propõe; cada par só vale depois que você confirma.",
            );
            mensagem(ui, estado);
            ui.add_space(8.0);
            let dados = match local.cache.as_ref().map(|(_, d)| d) {
                Some(Ok(d)) => d.clone(),
                Some(Err(e)) => {
                    componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
                    return;
                }
                None => return,
            };
            if dados.contas.is_empty() {
                texto_pequeno(ui, "Nenhum compromisso usa uma conta ainda.");
            }
            for c in &dados.contas {
                linha_da_conta(ui, c, &dados, &mut local, pedidos);
            }
        });
    estado.contas = local;
}

fn moldura(ui: &mut Ui, conteudo: impl FnOnce(&mut Ui)) {
    let p = tema::paleta(ui.ctx());
    Frame::new()
        .fill(p.superficie_2)
        .stroke(Stroke::new(1.0, p.borda))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            conteudo(ui);
        });
    ui.add_space(6.0);
}

fn linha_da_conta(
    ui: &mut Ui,
    c: &ContaDoPlanejamento,
    dados: &DadosDasContas,
    local: &mut EstadoDasContas,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    moldura(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.label(RichText::new(&c.apelido).strong().size(15.0));
            match &c.conta_do_extrato {
                Some(conta) => {
                    ui.label(icones::icone_cor("sync_alt", 15.0, p.tinta_suave));
                    ui.label(RichText::new(conta).strong());
                    componentes::selo(ui, "confirmada", p.sucesso_sutil, p.sucesso);
                }
                None => {
                    componentes::selo(ui, "sem par", p.superficie_3, p.tinta_suave);
                }
            }
        });
        if c.conta_do_extrato.is_none()
            && let Some(s) = dados.sugestao_de(&c.apelido)
        {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor("lightbulb", 15.0, p.info));
                ui.label(RichText::new(format!("Sugestão: {}", s.conta_do_extrato)).strong());
                componentes::selo(
                    ui,
                    &format!(
                        "{} {}",
                        s.evidencias,
                        if s.evidencias == 1 {
                            "evidência"
                        } else {
                            "evidências"
                        }
                    ),
                    p.info_sutil,
                    p.info,
                );
            });
            texto_pequeno(ui, s.motivo.clone());
            if ui
                .button(format!("{}  Confirmar", icones::texto("task_alt")))
                .on_hover_text("Liga este apelido a essa conta do extrato")
                .clicked()
            {
                pedidos.push(pedido_de_par(&c.apelido, Some(&s.conta_do_extrato)));
            }
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let aberto = local.escolhendo.as_deref() == Some(c.apelido.as_str());
            if ui
                .selectable_label(aberto, "Escolher…")
                .on_hover_text("Escolhe a conta do extrato entre as que aparecem nele")
                .clicked()
            {
                local.escolhendo = (!aberto).then(|| c.apelido.clone());
            }
            if c.conta_do_extrato.is_some()
                && ui
                    .button("Desligar")
                    .on_hover_text("Desfaz o par; o apelido volta a ficar sem conta do extrato")
                    .clicked()
            {
                pedidos.push(pedido_de_par(&c.apelido, None));
            }
        });
        if local.escolhendo.as_deref() == Some(c.apelido.as_str()) {
            ui.add_space(2.0);
            if dados.do_extrato.is_empty() {
                texto_pequeno(ui, "O extrato ainda não trouxe nenhuma conta.");
            }
            for conta in &dados.do_extrato {
                let atual = c.conta_do_extrato.as_deref() == Some(conta.as_str());
                if ui.selectable_label(atual, conta).clicked() {
                    pedidos.push(pedido_de_par(&c.apelido, Some(conta)));
                    local.escolhendo = None;
                }
            }
        }
    });
}
