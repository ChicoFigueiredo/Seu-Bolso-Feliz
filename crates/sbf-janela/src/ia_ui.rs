//! A aba **IA** de Conexões (spec 15, D12): os provedores de Rita. Local (Ollama), `claude -p`
//! (assinatura), OpenRouter, API da Anthropic e qualquer API compatível com OpenAI. Chaves e
//! endereços ficam no **Chaveiro**; a tela nunca reexibe uma chave (mostra "guardada · termina
//! em …abcd" ou "não guardada") e a chave digitada é zerada da memória logo depois de guardada.

use std::sync::Arc;

use egui::{RichText, Ui};
use sbf_chaveiro::Segredo;
use sbf_rita::{
    ConfiguracaoDeRita, GuardaDeProvedores, ProvedorConfigurado, TipoDeProvedor,
    provedores::id_valido,
};
use zeroize::Zeroize;

use crate::conexoes_ui::PontoDeEstado;
use crate::{componentes, tema};

/// A aba IA.
#[derive(Default)]
pub struct IaUi {
    guarda: Option<Arc<dyn GuardaDeProvedores>>,
    /// A cópia em edição; `None` até a primeira vez que a aba aparece.
    trabalho: Option<ConfiguracaoDeRita>,
    /// A chave digitada de cada provedor (zerada ao guardar).
    chaves: Vec<(String, String)>,
    novo_tipo: Option<TipoDeProvedor>,
    aviso: Option<(String, bool)>,
}

impl std::fmt::Debug for IaUi {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("IaUi")
            .field("ligada", &self.guarda.is_some())
            .finish_non_exhaustive()
    }
}

impl IaUi {
    pub fn ligar(&mut self, guarda: Arc<dyn GuardaDeProvedores>) {
        self.guarda = Some(guarda);
        self.trabalho = None;
    }

    /// O ponto do título da aba: verde com algum provedor pronto, cinza sem provedor.
    pub fn ponto(&self) -> PontoDeEstado {
        let Some(g) = &self.guarda else {
            return PontoDeEstado::Desligado;
        };
        if g.ler().provedores.is_empty() {
            PontoDeEstado::Desligado
        } else {
            PontoDeEstado::Ok
        }
    }

    fn chave_digitada(&mut self, id: &str) -> &mut String {
        if !self.chaves.iter().any(|(i, _)| i == id) {
            self.chaves.push((id.to_owned(), String::new()));
        }
        let pos = self
            .chaves
            .iter()
            .position(|(i, _)| i == id)
            .unwrap_or(self.chaves.len() - 1);
        &mut self.chaves[pos].1
    }

    /// Guarda a configuração em edição.
    pub fn salvar(&mut self) {
        let (Some(g), Some(t)) = (&self.guarda, &self.trabalho) else {
            return;
        };
        self.aviso = Some(match g.gravar(t) {
            Ok(()) => (
                "Provedores guardados. Vale já no painel Rita.".to_owned(),
                false,
            ),
            Err(e) => (e, true),
        });
    }

    /// Desenha a aba.
    pub fn desenhar(&mut self, ui: &mut Ui) {
        let p = tema::paleta(ui.ctx());
        let Some(g) = self.guarda.clone() else {
            ui.label(RichText::new("Os provedores de Rita só existem com o inventário de verdade (fora da demonstração).").color(p.tinta_suave));
            return;
        };
        if self.trabalho.is_none() {
            self.trabalho = Some(g.ler());
        }
        componentes::rotulo_de_secao(ui, "Provedores de Rita");
        ui.label(
            RichText::new(
                "Rita manda o texto completo do item ao provedor escolhido (nível de confiança completo). Endereços e chaves ficam no Chaveiro, nunca no cofre.",
            )
            .size(12.5)
            .color(p.tinta_suave),
        );
        ui.add_space(6.0);
        if let Some((t, erro)) = &self.aviso {
            if *erro {
                componentes::aviso(ui, "error", t, p.perigo_sutil, p.borda, p.perigo);
            } else {
                componentes::aviso(ui, "help", t, p.info_sutil, p.info_borda, p.info);
            }
            ui.add_space(6.0);
        }
        let mut remover: Option<usize> = None;
        let mut guardar_chave: Option<String> = None;
        let mut apagar_chave: Option<String> = None;
        let n = self.trabalho.as_ref().map_or(0, |t| t.provedores.len());
        for i in 0..n {
            let Some((id, tipo, e_padrao)) = self.trabalho.as_ref().and_then(|t| {
                let pr = t.provedores.get(i)?;
                Some((
                    pr.id.clone(),
                    pr.tipo,
                    t.padrao.as_deref() == Some(pr.id.as_str()),
                ))
            }) else {
                break;
            };
            let estado_da_chave = g.final_da_chave(&id);
            let mut digitada = std::mem::take(self.chave_digitada(&id));
            let mut marcar_padrao = false;
            let Some(trabalho) = self.trabalho.as_mut() else {
                break;
            };
            componentes::cartao(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&trabalho.provedores[i].nome).strong());
                    ui.label(RichText::new(tipo.rotulo()).size(12.0).color(p.tinta_suave));
                    if e_padrao {
                        componentes::selo(ui, "padrão", p.destaque_sutil, p.destaque_tinta);
                    }
                });
                egui::Grid::new(("rita-provedor", i))
                    .num_columns(2)
                    .spacing(egui::vec2(10.0, 6.0))
                    .show(ui, |ui| {
                        let pr = &mut trabalho.provedores[i];
                        ui.label("Nome");
                        ui.text_edit_singleline(&mut pr.nome);
                        ui.end_row();
                        if tipo.pede_endereco() || pr.endereco.is_some() {
                            ui.label("Endereço");
                            let mut e = pr.endereco.clone().unwrap_or_default();
                            ui.add(egui::TextEdit::singleline(&mut e).desired_width(320.0));
                            pr.endereco = Some(e);
                            ui.end_row();
                        }
                        ui.label("Modelo");
                        ui.add(egui::TextEdit::singleline(&mut pr.modelo).desired_width(320.0));
                        ui.end_row();
                        if tipo.usa_chave() {
                            ui.label("Chave");
                            ui.horizontal(|ui| {
                                let texto = match &estado_da_chave {
                                    None => "não guardada".to_owned(),
                                    Some(f) if f.is_empty() => "guardada".to_owned(),
                                    Some(f) => format!("guardada · termina em …{f}"),
                                };
                                ui.label(RichText::new(texto).color(p.tinta_suave));
                            });
                            ui.end_row();
                            ui.label("");
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut digitada)
                                        .password(true)
                                        .hint_text("colar a chave aqui")
                                        .desired_width(240.0),
                                );
                                if ui
                                    .add_enabled(
                                        !digitada.trim().is_empty(),
                                        egui::Button::new("Guardar chave"),
                                    )
                                    .clicked()
                                {
                                    guardar_chave = Some(id.clone());
                                }
                                if estado_da_chave.is_some() && ui.button("Apagar chave").clicked()
                                {
                                    apagar_chave = Some(id.clone());
                                }
                            });
                            ui.end_row();
                        }
                    });
                ui.horizontal(|ui| {
                    if !e_padrao && ui.button("Usar como padrão").clicked() {
                        marcar_padrao = true;
                    }
                    if ui.button("Remover").clicked() {
                        remover = Some(i);
                    }
                });
            });
            if marcar_padrao {
                trabalho.padrao = Some(id.clone());
            }
            // Devolve o que foi digitado (ou zera, se a chave vai ser guardada agora).
            if guardar_chave.as_deref() == Some(id.as_str()) {
                match g.guardar_chave(&id, Segredo::novo(digitada.trim().to_owned())) {
                    Ok(()) => {
                        self.aviso = Some(("Chave guardada no Chaveiro.".to_owned(), false));
                    }
                    Err(e) => self.aviso = Some((e, true)),
                }
                digitada.zeroize();
            }
            *self.chave_digitada(&id) = digitada;
            ui.add_space(6.0);
        }
        if let Some(id) = apagar_chave {
            self.aviso = Some(match g.apagar_chave(&id) {
                Ok(()) => ("Chave apagada do Chaveiro.".to_owned(), false),
                Err(e) => (e, true),
            });
        }
        if let (Some(i), Some(t)) = (remover, self.trabalho.as_mut())
            && i < t.provedores.len()
        {
            let r = t.provedores.remove(i);
            if t.padrao.as_deref() == Some(r.id.as_str()) {
                t.padrao = t.provedores.first().map(|p| p.id.clone());
            }
        }
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("rita-novo-tipo")
                .selected_text(
                    self.novo_tipo
                        .map_or("Adicionar provedor…", TipoDeProvedor::rotulo),
                )
                .show_ui(ui, |ui| {
                    for t in TipoDeProvedor::TODOS {
                        if ui
                            .selectable_label(self.novo_tipo == Some(t), t.rotulo())
                            .clicked()
                        {
                            self.novo_tipo = Some(t);
                        }
                    }
                });
            if ui
                .add_enabled(self.novo_tipo.is_some(), egui::Button::new("Adicionar"))
                .clicked()
                && let (Some(tipo), Some(t)) = (self.novo_tipo.take(), self.trabalho.as_mut())
            {
                let mut novo = ProvedorConfigurado::padrao_do_tipo(tipo);
                let base = novo.id.clone();
                let mut k = 2;
                while t.provedores.iter().any(|p| p.id == novo.id) || !id_valido(&novo.id) {
                    novo.id = format!("{base}-{k}");
                    k += 1;
                }
                if t.padrao.is_none() {
                    t.padrao = Some(novo.id.clone());
                }
                t.provedores.push(novo);
            }
        });
        ui.add_space(8.0);
        ui.horizontal(|ui| {
            if ui
                .add(egui::Button::new("Guardar provedores").fill(p.destaque))
                .clicked()
            {
                self.salvar();
            }
            if ui.button("Desfazer alterações").clicked() {
                self.trabalho = Some(g.ler());
            }
        });
    }
}

#[cfg(test)]
mod testes {
    use super::*;
    use sbf_rita::GuardaEmMemoria;

    #[test]
    fn sem_guarda_a_aba_fica_desligada_e_com_guarda_mostra_o_padrao() {
        let mut ia = IaUi::default();
        assert_eq!(ia.ponto(), PontoDeEstado::Desligado);
        ia.ligar(Arc::new(GuardaEmMemoria::default()));
        assert_eq!(ia.ponto(), PontoDeEstado::Ok);
    }

    #[test]
    fn salvar_grava_a_copia_de_trabalho_e_recusa_o_que_nao_confere() {
        let guarda = Arc::new(GuardaEmMemoria::default());
        let mut ia = IaUi::default();
        ia.ligar(guarda.clone());
        ia.trabalho = Some(guarda.ler());
        if let Some(t) = ia.trabalho.as_mut() {
            t.provedores[0].modelo = "outro-modelo".into();
        }
        ia.salvar();
        assert_eq!(guarda.ler().provedores[0].modelo, "outro-modelo");
        assert_eq!(ia.aviso.as_ref().map(|a| a.1), Some(false));
        if let Some(t) = ia.trabalho.as_mut() {
            t.provedores[0].endereco = Some("http://exemplo.com/v1".into());
        }
        ia.salvar();
        assert_eq!(ia.aviso.as_ref().map(|a| a.1), Some(true));
        assert_eq!(guarda.ler().provedores[0].modelo, "outro-modelo");
    }
}
