//! A tela "Contas do planejamento": uma linha por código da planilha (`CX-0001`), com a conta
//! do extrato a que ele corresponde, o apelido que o usuário lhe deu e o rótulo que as outras
//! telas mostram no lugar da conta do extrato. O histórico **propõe** o par e o usuário
//! confirma cada um; nada é ligado sozinho. Só lê por `&dyn Planejamento`; escrever é um
//! pedido (`MapearConta`, `DefinirApelidoDaConta`) que a janela aplica fora do desenho.

use std::collections::HashMap;

use egui::{CornerRadius, Frame, Margin, RichText, ScrollArea, Stroke, TextEdit, Ui};
use sbf_planejamento::{Alteracao, ContaDoPlanejamento, ParDeConta, Planejamento, Rotulador};

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
    /// O rótulo que cada conta do extrato ganha com os pares e apelidos de agora.
    pub rotulador: Rotulador,
}

impl DadosDasContas {
    /// A sugestão do histórico para um código ainda sem par.
    pub fn sugestao_de(&self, codigo: &str) -> Option<&ParDeConta> {
        self.sugestoes.iter().find(|s| s.codigo == codigo)
    }
}

pub fn carregar(plan: &dyn Planejamento) -> Result<DadosDasContas, String> {
    let contas = plan.contas().map_err(|e| e.to_string())?;
    let do_extrato = plan.contas_do_extrato().map_err(|e| e.to_string())?;
    Ok(DadosDasContas {
        rotulador: Rotulador::novo(&contas, &do_extrato),
        sugestoes: plan.pares_de_conta_sugeridos().map_err(|e| e.to_string())?,
        contas,
        do_extrato,
    })
}

/// O que a tela guarda entre quadros.
#[derive(Debug, Default)]
pub struct EstadoDasContas {
    cache: Option<(u64, Result<DadosDasContas, String>)>,
    /// O código cuja lista de contas do extrato está aberta ("Escolher…").
    pub escolhendo: Option<String>,
    /// O apelido em edição de cada código (o que está no campo, ainda não salvo).
    pub apelidos_em_edicao: HashMap<String, String>,
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

/// O pedido que confirma (`Some`) ou desliga (`None`) o par de um código.
pub fn pedido_de_par(codigo: &str, conta: Option<&str>) -> Pedido {
    Pedido::Controle(PedidoDoControle::AplicarVarias(vec![
        Alteracao::MapearConta {
            codigo: codigo.to_owned(),
            conta_do_extrato: conta.map(str::to_owned),
        },
    ]))
}

/// O pedido que dá (`Some`) ou tira (`None`, ou texto em branco) o apelido de um código.
pub fn pedido_de_apelido(codigo: &str, apelido: Option<&str>) -> Pedido {
    Pedido::Controle(PedidoDoControle::AplicarVarias(vec![
        Alteracao::DefinirApelidoDaConta {
            codigo: codigo.to_owned(),
            apelido: apelido
                .map(str::trim)
                .filter(|a| !a.is_empty())
                .map(str::to_owned),
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
                "Liga o código da planilha à conta do extrato. O histórico propõe; cada par só vale depois que você confirma.",
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
            ui.label(RichText::new(&c.codigo).strong().size(15.0));
            match &c.conta_do_extrato {
                Some(conta) => {
                    ui.label(icones::icone_cor("sync_alt", 15.0, p.tinta_suave));
                    ui.label(RichText::new(conta).strong())
                        .on_hover_text("A conta do extrato (como o banco a chama)");
                    componentes::selo(ui, "confirmada", p.sucesso_sutil, p.sucesso);
                }
                None => {
                    componentes::selo(ui, "sem par", p.superficie_3, p.tinta_suave);
                }
            }
        });
        apelido_e_rotulo(ui, c, dados, local, pedidos);
        if c.conta_do_extrato.is_none()
            && let Some(s) = dados.sugestao_de(&c.codigo)
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
                .on_hover_text("Liga este código a essa conta do extrato")
                .clicked()
            {
                pedidos.push(pedido_de_par(&c.codigo, Some(&s.conta_do_extrato)));
            }
        }
        ui.add_space(4.0);
        ui.horizontal_wrapped(|ui| {
            let aberto = local.escolhendo.as_deref() == Some(c.codigo.as_str());
            if ui
                .selectable_label(aberto, "Escolher…")
                .on_hover_text("Escolhe a conta do extrato entre as que aparecem nele")
                .clicked()
            {
                local.escolhendo = (!aberto).then(|| c.codigo.clone());
            }
            if c.conta_do_extrato.is_some()
                && ui
                    .button("Desligar")
                    .on_hover_text("Desfaz o par; o código volta a ficar sem conta do extrato")
                    .clicked()
            {
                pedidos.push(pedido_de_par(&c.codigo, None));
            }
        });
        if local.escolhendo.as_deref() == Some(c.codigo.as_str()) {
            ui.add_space(2.0);
            if dados.do_extrato.is_empty() {
                texto_pequeno(ui, "O extrato ainda não trouxe nenhuma conta.");
            }
            for conta in &dados.do_extrato {
                let atual = c.conta_do_extrato.as_deref() == Some(conta.as_str());
                // Todas as contas aparecem; as que têm rótulo mostram o rótulo, com a conta do
                // extrato na dica.
                if ui
                    .selectable_label(atual, dados.rotulador.rotular(conta))
                    .on_hover_text(conta)
                    .clicked()
                {
                    pedidos.push(pedido_de_par(&c.codigo, Some(conta)));
                    local.escolhendo = None;
                }
            }
        }
    });
}

/// O campo "Apelido" (com Salvar) e o rótulo que a conta passa a ter nas outras telas.
fn apelido_e_rotulo(
    ui: &mut Ui,
    c: &ContaDoPlanejamento,
    dados: &DadosDasContas,
    local: &mut EstadoDasContas,
    pedidos: &mut Vec<Pedido>,
) {
    let atual = c.apelido.clone().unwrap_or_default();
    let texto = local
        .apelidos_em_edicao
        .entry(c.codigo.clone())
        .or_insert_with(|| atual.clone());
    let mudou = texto.trim() != atual;
    let mut salvar = false;
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label("Apelido");
        let r = ui.add(
            TextEdit::singleline(texto)
                .hint_text("opcional, como Reserva")
                .desired_width(200.0),
        );
        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if ui
            .add_enabled(mudou, egui::Button::new("Salvar"))
            .on_hover_text("Grava o apelido; vazio tira o apelido")
            .clicked()
            || (enter && mudou)
        {
            salvar = true;
        }
    });
    if salvar {
        let novo = texto.clone();
        pedidos.push(pedido_de_apelido(&c.codigo, Some(&novo)));
        // Relê o campo do que for gravado.
        local.apelidos_em_edicao.remove(&c.codigo);
    }
    let rotulo = match &c.conta_do_extrato {
        Some(conta) => dados.rotulador.rotular(conta),
        None => c.codigo.clone(),
    };
    texto_pequeno(ui, format!("Rótulo nas outras telas: {rotulo}"));
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn o_pedido_de_apelido_apara_o_texto_e_branco_tira() {
        let apelido = |p: Pedido| match p {
            Pedido::Controle(PedidoDoControle::AplicarVarias(v)) => match v.into_iter().next() {
                Some(Alteracao::DefinirApelidoDaConta { codigo, apelido }) => (codigo, apelido),
                outra => panic!("alteração inesperada: {outra:?}"),
            },
            _ => panic!("pedido inesperado"),
        };
        assert_eq!(
            apelido(pedido_de_apelido("CX-0001", Some("  Reserva "))),
            ("CX-0001".to_owned(), Some("Reserva".to_owned()))
        );
        assert_eq!(apelido(pedido_de_apelido("CX-0001", Some("   "))).1, None);
        assert_eq!(apelido(pedido_de_apelido("CX-0001", None)).1, None);
    }

    #[test]
    fn a_tela_carrega_o_rotulo_de_toda_conta_do_extrato() {
        let plano = crate::planejamento_ficticio::PlanejamentoFicticio::novo(
            sbf_planejamento::Data::de_iso(crate::planejamento_ficticio::HOJE_DA_DEMONSTRACAO)
                .expect("data"),
        );
        let d = carregar(&plano).expect("dados");
        // Nenhuma conta do extrato some da lista por ter ou não ter rótulo.
        assert_eq!(d.do_extrato, plano.contas_do_extrato().unwrap());
        assert!(
            d.do_extrato.iter().any(|c| d.rotulador.rotular(c) != *c),
            "ao menos uma conta tem rótulo"
        );
    }
}
