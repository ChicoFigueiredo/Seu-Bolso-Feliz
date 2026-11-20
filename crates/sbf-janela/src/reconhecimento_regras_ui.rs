//! O desenho do painel Reconhecimento quando uma **regra de reconhecimento** entra em cena
//! (spec 13): o cartão "Reconhecido pela regra", o caso ambíguo, as sugestões das regras e os
//! diálogos "Testar com este documento" e "Criar regra a partir daqui". O estado e a lógica
//! estão em [`crate::reconhecimento_regras`].

use std::collections::BTreeMap;

use egui::{Context, Id, RichText, ScrollArea, TextEdit, Ui};
use sbf_reconhecimento::{PROVAS_POSSIVEIS, Sugestao};
use sbf_regras::{Decisao, Entrada, Leitura, Proposta, Reconhecimento, ResultadoDaExecucao};

use crate::componentes;
use crate::controle::{data_texto, reais};
use crate::icones;
use crate::navegacao::{self, Destino};
use crate::reconhecimento_regras::{
    CAMPOS_USUAIS, LinhaDeCampo, descrever_de_onde, rotulo_do_campo, valor_do_texto,
};
use crate::reconhecimento_ui::{Pedido, aviso_com_triangulo, pequeno};
use crate::regras::{rotulo_do_autor, rotulo_do_estado};
use crate::tema::{self, Paleta};
use sbf_regras::Autor;

/// O que o desenho de um cartão precisa além do reconhecimento em si.
pub(crate) struct ExtrasDaRegra<'a> {
    pub entrada: Option<&'a Entrada>,
    pub leitura: Option<&'a Leitura>,
    /// id da regra → nome.
    pub nomes: &'a BTreeMap<String, String>,
    pub regras_ligadas: bool,
    /// Nenhum padrão reconheceu o documento: "Criar regra a partir daqui" vira o caminho
    /// principal (em destaque).
    pub sem_padrao: bool,
}

/// O que o cartão deixa o desenho mudar: a correção dos campos e o Rastro aberto.
pub(crate) struct EdicaoDoCartao<'a> {
    pub corrigindo: &'a mut bool,
    pub edicao: &'a mut BTreeMap<String, String>,
    pub rastro_aberto: &'a mut bool,
}

impl ExtrasDaRegra<'_> {
    fn nome_da<'b>(&'b self, id: &'b str) -> &'b str {
        self.nomes.get(id).map_or(id, String::as_str)
    }
}

/// Os campos na ordem de quem lê um comprovante (valor, datas, favorecido, autenticação) e,
/// depois, os demais por nome.
fn campos_em_ordem(
    campos: &BTreeMap<String, sbf_regras::Campo>,
) -> Vec<(&String, &sbf_regras::Campo)> {
    let pos = |n: &str| {
        CAMPOS_USUAIS
            .iter()
            .position(|c| *c == n)
            .unwrap_or(CAMPOS_USUAIS.len())
    };
    let mut v: Vec<_> = campos.iter().collect();
    v.sort_by(|a, b| (pos(a.0), a.0).cmp(&(pos(b.0), b.0)));
    v
}

fn selo_de_provas(ui: &mut Ui, p: &Paleta, provas: u8) {
    let todas = provas >= PROVAS_POSSIVEIS;
    componentes::selo(
        ui,
        &format!("{provas} de {PROVAS_POSSIVEIS} provas batem"),
        if todas { p.sucesso_sutil } else { p.info_sutil },
        if todas { p.sucesso } else { p.info },
    );
}

/// "Regra <nome> · versão N": o texto da linha discreta que aponta para a regra que
/// reconheceu.
pub(crate) fn rotulo_da_regra(nome: &str, versao: u32) -> String {
    format!("Regra {nome} · versão {versao}")
}

/// A regra que reconheceu a entrada, segundo a leitura do motor (no contracheque, já
/// completada pelo leitor nativo e pelo script).
pub(crate) fn regra_da_leitura(leitura: Option<&Leitura>) -> Option<&Reconhecimento> {
    match &leitura?.decisao {
        Decisao::Reconhecido(r) => Some(r),
        _ => None,
    }
}

/// As linhas de `exibir` que o script quis mostrar, discretas, uma por linha. As que
/// começam por aviso (`⚠`) saem na cor de aviso.
pub(crate) fn linhas_de_exibir(ui: &mut Ui, p: &Paleta, linhas: &[String]) {
    for linha in linhas {
        let cor = if linha.starts_with('⚠') {
            p.aviso
        } else {
            p.tinta
        };
        ui.label(RichText::new(linha).size(12.0).color(cor));
    }
}

/// No cartão do contracheque: a linha "Regra <nome> · versão N" (link para a regra) e,
/// abaixo, o `exibir` do script. Nada quando a leitura não trouxe a regra.
pub(crate) fn regra_do_contracheque(
    ui: &mut Ui,
    p: &Paleta,
    extras: &ExtrasDaRegra<'_>,
    pedidos: &mut Vec<Pedido>,
) {
    let Some(r) = regra_da_leitura(extras.leitura) else {
        return;
    };
    ui.add_space(3.0);
    if ui
        .link(
            RichText::new(rotulo_da_regra(extras.nome_da(&r.regra), r.versao))
                .size(12.0)
                .color(p.tinta_suave),
        )
        .on_hover_text("Abrir a regra na área Regras de reconhecimento")
        .clicked()
    {
        pedidos.push(Pedido::AbrirRegra(r.regra.clone()));
    }
    linhas_de_exibir(ui, p, &r.exibir);
}

fn selo_precisa_revisar(ui: &mut Ui, p: &Paleta) {
    componentes::selo(ui, "Precisa revisar", p.aviso_sutil, p.aviso);
}

// ---------------------------------------------------------------------------------------
// O cartão
// ---------------------------------------------------------------------------------------

/// "Reconhecido pela regra X · versão N": tipo, campos (com o Rastro de cada um ao passar o
/// mouse e numa seção própria), motivos e o "Corrigir campos".
pub(crate) fn pela_regra(
    ui: &mut Ui,
    p: &Paleta,
    indice: usize,
    r: &Reconhecimento,
    extras: &ExtrasDaRegra<'_>,
    estado: &mut EdicaoDoCartao<'_>,
    pedidos: &mut Vec<Pedido>,
) {
    let (corrigindo, edicao) = (&mut *estado.corrigindo, &mut *estado.edicao);
    let texto = extras.entrada.and_then(|e| e.texto.as_deref());
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor("task_alt", 16.0, p.sucesso));
        ui.label(RichText::new("Reconhecido pela regra").strong());
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .link(RichText::new(extras.nome_da(&r.regra)).strong())
            .on_hover_text("Abrir a regra na área Regras de reconhecimento")
            .clicked()
        {
            pedidos.push(Pedido::AbrirRegra(r.regra.clone()));
        }
        ui.label(RichText::new(format!("· versão {}", r.versao)).color(p.tinta_suave));
        if r.revisar {
            selo_precisa_revisar(ui, p);
        }
    });
    linhas_de_exibir(ui, p, &r.exibir);
    pequeno(
        ui,
        format!("Tipo documental: {}", r.tipo_documental),
        p.tinta_suave,
    );
    if !r.campos.is_empty() {
        ui.add_space(2.0);
        egui::Grid::new(("campos-da-regra", indice))
            .num_columns(2)
            .spacing([14.0, 3.0])
            .show(ui, |ui| {
                for (nome, campo) in campos_em_ordem(&r.campos) {
                    let origem = format!("Veio de: {}", descrever_de_onde(&campo.de_onde, texto));
                    ui.label(
                        RichText::new(rotulo_do_campo(nome))
                            .size(12.0)
                            .color(p.tinta_suave),
                    )
                    .on_hover_text(&origem);
                    if *corrigindo {
                        let buffer = edicao
                            .entry(nome.clone())
                            .or_insert_with(|| campo.valor.to_string());
                        ui.horizontal(|ui| {
                            ui.add(TextEdit::singleline(buffer).desired_width(150.0))
                                .on_hover_text(&origem);
                            match valor_do_texto(nome, Some(&campo.valor), buffer) {
                                None => {
                                    ui.label(
                                        RichText::new("não entendi").size(11.5).color(p.perigo),
                                    );
                                }
                                Some(v) if v != campo.valor => {
                                    componentes::selo(ui, "corrigido", p.aviso_sutil, p.aviso);
                                }
                                Some(_) => {}
                            }
                        });
                    } else {
                        ui.label(campo.valor.to_string()).on_hover_text(&origem);
                    }
                    ui.end_row();
                }
            });
        ui.horizontal(|ui| {
            let rotulo = if *corrigindo {
                "Concluir correção"
            } else {
                "Corrigir campos"
            };
            if ui
                .small_button(rotulo)
                .on_hover_text(
                    "Mudar um valor que a regra leu errado. Aceitar depois disso conta como correção para a regra.",
                )
                .clicked()
            {
                pedidos.push(Pedido::AlternarCorrecao(indice));
            }
        });
    }
    for motivo in &r.motivos {
        pequeno(ui, motivo, p.tinta_suave);
    }
    rastro(ui, p, indice, r, extras, texto, estado.rastro_aberto);
}

/// A seção "Rastro": de onde veio cada campo e quais regras rodaram.
fn rastro(
    ui: &mut Ui,
    p: &Paleta,
    indice: usize,
    r: &Reconhecimento,
    extras: &ExtrasDaRegra<'_>,
    texto: Option<&str>,
    aberto: &mut bool,
) {
    let resposta =
        egui::CollapsingHeader::new(RichText::new("Rastro").size(12.0).color(p.tinta_suave))
            .id_salt(("rastro-do-reconhecimento", indice))
            .open(Some(*aberto))
            .show(ui, |ui| {
                for (nome, campo) in campos_em_ordem(&r.campos) {
                    pequeno(
                        ui,
                        format!(
                            "{} ← {}",
                            rotulo_do_campo(nome),
                            descrever_de_onde(&campo.de_onde, texto)
                        ),
                        p.tinta,
                    );
                }
                if let Some(l) = extras.leitura {
                    let mut sem_efeito = 0usize;
                    for e in &l.execucoes {
                        let o_que = match e.resultado {
                            ResultadoDaExecucao::Reconhecido => "leu",
                            ResultadoDaExecucao::Erro => "falhou",
                            ResultadoDaExecucao::NaoSeAplica => {
                                sem_efeito += 1;
                                continue;
                            }
                        };
                        let motivo = e
                            .motivo
                            .as_deref()
                            .map(|m| format!(" ({m})"))
                            .unwrap_or_default();
                        pequeno(
                            ui,
                            format!(
                                "Regra {} · versão {} {o_que}{motivo}",
                                extras.nome_da(&e.regra),
                                e.versao
                            ),
                            p.tinta_suave,
                        );
                    }
                    if sem_efeito > 0 {
                        pequeno(
                            ui,
                            format!(
                                "{sem_efeito} {} não se aplicou",
                                if sem_efeito == 1 {
                                    "outra regra"
                                } else {
                                    "outras regras"
                                }
                            ),
                            p.tinta_desabilitada,
                        );
                    }
                }
            });
    if resposta.header_response.clicked() {
        *aberto = !*aberto;
    }
}

/// "Precisa revisar · duas regras discordam": as candidatas lado a lado. Nada é aplicado.
pub(crate) fn ambiguo(
    ui: &mut Ui,
    p: &Paleta,
    candidatas: &[Reconhecimento],
    motivo: &str,
    extras: &ExtrasDaRegra<'_>,
    pedidos: &mut Vec<Pedido>,
) {
    ui.horizontal_wrapped(|ui| {
        ui.label(icones::icone_cor("warning", 16.0, p.aviso));
        ui.label(
            RichText::new(format!(
                "Precisa revisar · {} regras discordam",
                numero_por_extenso(candidatas.len())
            ))
            .strong()
            .color(p.aviso),
        );
    });
    pequeno(ui, motivo, p.tinta_suave);
    pequeno(
        ui,
        "Nada foi aplicado. Ajuste a prioridade ou as condições de uma das regras, ou corrija o documento.",
        p.tinta_suave,
    );
    if candidatas.is_empty() {
        return;
    }
    // Os campos em que as candidatas leem valores diferentes ganham o triângulo.
    let diverge = |nome: &str| {
        let valores: Vec<_> = candidatas
            .iter()
            .map(|c| c.campos.get(nome).map(|k| &k.valor))
            .collect();
        valores.windows(2).any(|w| w[0] != w[1])
    };
    ui.add_space(4.0);
    let n = candidatas.len().min(3);
    let folga = ui.spacing().item_spacing.x;
    let largura = ((ui.available_width() - folga * (n as f32 - 1.0)) / n as f32).max(90.0);
    ui.horizontal_top(|ui| {
        for c in candidatas.iter().take(n) {
            ui.allocate_ui_with_layout(
                egui::vec2(largura, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    componentes::cartao(ui, |ui| {
                        ui.set_width(largura - 22.0);
                        ui.label(RichText::new(extras.nome_da(&c.regra)).strong());
                        pequeno(
                            ui,
                            format!("versão {} · tipo {}", c.versao, c.tipo_documental),
                            p.tinta_suave,
                        );
                        ui.add_space(2.0);
                        for (nome, campo) in campos_em_ordem(&c.campos) {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(format!("{}:", rotulo_do_campo(nome)))
                                        .size(12.0)
                                        .color(p.tinta_suave),
                                );
                                ui.label(RichText::new(campo.valor.to_string()).size(12.0));
                                if diverge(nome) {
                                    ui.label(icones::icone_cor("warning", 14.0, p.aviso))
                                        .on_hover_text(
                                            "As regras divergem neste campo (uma não o lê ou lê outro valor)",
                                        );
                                }
                            });
                        }
                        if c.campos.is_empty() {
                            pequeno(ui, "sem campos lidos", p.tinta_desabilitada);
                        }
                        if c.revisar {
                            selo_precisa_revisar(ui, p);
                        }
                        ui.add_space(2.0);
                        if ui
                            .small_button("Abrir a regra")
                            .on_hover_text("Abrir a regra na área Regras de reconhecimento")
                            .clicked()
                        {
                            pedidos.push(Pedido::AbrirRegra(c.regra.clone()));
                        }
                    });
                },
            );
        }
    });
}

fn numero_por_extenso(n: usize) -> &'static str {
    match n {
        2 => "duas",
        3 => "três",
        4 => "quatro",
        _ => "várias",
    }
}

/// Os botões "Testar com este documento" e "Criar regra a partir daqui" (só quando o cartão
/// guarda a entrada que o motor recebeu e o catálogo está ligado).
pub(crate) fn acoes_da_regra(
    ui: &mut Ui,
    indice: usize,
    extras: &ExtrasDaRegra<'_>,
    pedidos: &mut Vec<Pedido>,
) {
    if extras.entrada.is_none() || !extras.regras_ligadas {
        return;
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui
            .button("Testar com este documento")
            .on_hover_text("Lê este documento com uma regra, sem gravar nada")
            .clicked()
        {
            pedidos.push(Pedido::AbrirTeste(indice));
        }
        let criar = if extras.sem_padrao {
            egui::Button::new(RichText::new("Criar regra a partir daqui").strong())
                .fill(tema::paleta(ui.ctx()).destaque_sutil)
        } else {
            egui::Button::new("Criar regra a partir daqui")
        };
        if ui
            .add(criar)
            .on_hover_text("Monta um rascunho de regra com o que você corrigir; ele só vale depois que você ativar")
            .clicked()
        {
            pedidos.push(Pedido::AbrirCriacao(indice));
        }
    });
}

// ---------------------------------------------------------------------------------------
// As sugestões das regras
// ---------------------------------------------------------------------------------------

/// O que a sugestão de regra mostra abaixo do rótulo.
pub(crate) fn detalhe_da_sugestao(ui: &mut Ui, p: &Paleta, sug: &Sugestao, aplicada: bool) {
    match sug {
        Sugestao::ProvarRegistro {
            movimento,
            alvo,
            provas,
            ..
        } => {
            selo_de_provas(ui, p, *provas);
            pequeno(
                ui,
                format!(
                    "Débito de {} em {} no extrato ({}).",
                    reais(sbf_planejamento::Centavos(movimento.valor.0.abs())),
                    data_texto(movimento.data),
                    movimento.descricao
                ),
                p.tinta_suave,
            );
            pequeno(
                ui,
                if alvo.is_some() {
                    "Confirmar liga o comprovante à ocorrência que já tem esse débito."
                } else {
                    "O débito já está no extrato; o comprovante só o prova e nada é criado."
                },
                p.tinta_suave,
            );
        }
        Sugestao::ConciliarOcorrencia {
            conciliacao,
            provas,
            ..
        }
        | Sugestao::PagarFatura {
            conciliacao,
            provas,
            ..
        } => {
            selo_de_provas(ui, p, *provas);
            let fatura = matches!(sug, Sugestao::PagarFatura { .. });
            pequeno(
                ui,
                format!(
                    "Débito de {} em {}. {}",
                    reais(conciliacao.valor),
                    data_texto(conciliacao.data),
                    if aplicada {
                        "A ocorrência ficou ligada a ele."
                    } else if fatura {
                        "Confirmar paga a fatura; os itens dela já são os gastos."
                    } else {
                        "Confirmar liga a ocorrência a ele."
                    }
                ),
                p.tinta_suave,
            );
        }
        _ => {}
    }
    if let Some((regra, _)) = sug.da_regra() {
        pequeno(
            ui,
            format!("Proposta da regra {} · versão {}", regra.0, regra.1),
            p.tinta_desabilitada,
        );
    }
}

/// "Conciliado automaticamente pela regra X ·" (o Desfazer vem em seguida).
pub(crate) fn texto_da_automatica(sug: &Sugestao, nomes: &BTreeMap<String, String>) -> String {
    match sug.da_regra() {
        Some(((id, _), _)) => format!(
            "Conciliado automaticamente pela regra {} ·",
            nomes.get(id).map_or(id.as_str(), String::as_str)
        ),
        None => "Conciliado automaticamente ·".to_owned(),
    }
}

// ---------------------------------------------------------------------------------------
// Os diálogos
// ---------------------------------------------------------------------------------------

fn descrever_proposta(pr: &Proposta) -> String {
    use sbf_planejamento::Centavos;
    let dinheiro = |c: i64| reais(Centavos(c.abs()));
    let dia = |iso: &str| sbf_regras::Valor::Data(iso.to_owned()).to_string();
    match pr {
        Proposta::ProvarRegistro {
            valor_centavos,
            data,
            ..
        } => format!(
            "provar o débito de {} perto de {}",
            dinheiro(*valor_centavos),
            dia(data)
        ),
        Proposta::ConciliarOcorrencia {
            valor_centavos,
            data,
            compromisso,
            ..
        } => format!(
            "conciliar a ocorrência{} com o débito de {} perto de {}",
            compromisso
                .as_deref()
                .map(|c| format!(" de {c}"))
                .unwrap_or_default(),
            dinheiro(*valor_centavos),
            dia(data)
        ),
        Proposta::PagarFatura {
            valor_centavos,
            vencimento,
            ..
        } => format!(
            "pagar a fatura de {} que vence em {}",
            dinheiro(*valor_centavos),
            dia(vencimento)
        ),
    }
}

/// O que a versão leria do documento (decisão, campos, motivos). Só mostra.
fn leitura_em_cartao(ui: &mut Ui, p: &Paleta, leitura: &Leitura, texto: Option<&str>) {
    match &leitura.decisao {
        Decisao::Reconhecido(r) => {
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor("task_alt", 16.0, p.sucesso));
                ui.label(
                    RichText::new(format!("Reconheceria como {}", r.tipo_documental)).strong(),
                );
                if r.revisar {
                    selo_precisa_revisar(ui, p);
                }
            });
            if r.campos.is_empty() {
                pequeno(ui, "Nenhum campo lido.", p.tinta_suave);
            }
            egui::Grid::new("teste-campos")
                .num_columns(2)
                .spacing([14.0, 3.0])
                .show(ui, |ui| {
                    for (nome, campo) in campos_em_ordem(&r.campos) {
                        let origem =
                            format!("Veio de: {}", descrever_de_onde(&campo.de_onde, texto));
                        ui.label(
                            RichText::new(rotulo_do_campo(nome))
                                .size(12.0)
                                .color(p.tinta_suave),
                        )
                        .on_hover_text(&origem);
                        ui.label(campo.valor.to_string()).on_hover_text(&origem);
                        ui.end_row();
                    }
                });
            for m in &r.motivos {
                pequeno(ui, m, p.tinta_suave);
            }
            for pr in &r.propostas {
                pequeno(
                    ui,
                    format!("Proporia: {}.", descrever_proposta(pr)),
                    p.tinta_suave,
                );
            }
        }
        Decisao::Ambiguo { motivo, .. } => {
            aviso_com_triangulo(ui, p, &format!("Precisa revisar: {motivo}"));
        }
        Decisao::Nenhuma => {
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor("block", 16.0, p.tinta_suave));
                ui.label(
                    RichText::new("A regra não se aplica a este documento.").color(p.tinta_suave),
                );
            });
            for e in &leitura.execucoes {
                if let Some(m) = &e.motivo {
                    pequeno(ui, m, p.tinta_suave);
                }
            }
        }
    }
}

impl crate::janela::Janela {
    /// Desenha os diálogos abertos do Reconhecimento (teste e criação).
    pub(crate) fn dialogos_das_regras(&mut self, ctx: &Context) {
        self.dialogo_de_teste(ctx);
        self.dialogo_de_criacao(ctx);
    }

    fn dialogo_de_teste(&mut self, ctx: &Context) {
        let p = tema::paleta(ctx);
        let Some(t) = self.reconhecimento.teste.as_mut() else {
            return;
        };
        let mut mudou = false;
        let mut guardar = false;
        let mut fechar = false;
        let resposta = egui::Modal::new(Id::new("reconhecimento-teste")).show(ctx, |ui| {
            ui.set_width(620.0);
            componentes::titulo(ui, "Testar com este documento", 17.0);
            pequeno(
                ui,
                format!(
                    "{} · só leitura: nada é gravado e a regra não muda.",
                    t.documento
                ),
                p.tinta_suave,
            );
            ui.add_space(8.0);
            if t.opcoes.is_empty() {
                pequeno(ui, "Ainda não há regras no catálogo.", p.tinta_suave);
            } else {
                ui.horizontal(|ui| {
                    ui.label("Regra e versão");
                    let atual = t.opcao().map_or_else(String::new, |o| {
                        format!("{} · versão {} ({})", o.nome, o.versao, rotulo_do_estado_opcao(o))
                    });
                    egui::ComboBox::from_id_salt("reconhecimento-teste-versao")
                        .width(380.0)
                        .selected_text(atual)
                        .show_ui(ui, |ui| {
                            for (i, o) in t.opcoes.iter().enumerate() {
                                let rotulo = format!(
                                    "{} · versão {} ({})",
                                    o.nome,
                                    o.versao,
                                    rotulo_do_estado_opcao(o)
                                );
                                if ui.selectable_value(&mut t.escolhida, i, rotulo).changed() {
                                    mudou = true;
                                }
                            }
                        });
                });
            }
            ui.add_space(6.0);
            componentes::cartao(ui, |ui| {
                ui.set_width(ui.available_width());
                match &t.leitura {
                    Some(Ok(l)) => leitura_em_cartao(ui, &p, l, t.entrada.texto.as_deref()),
                    Some(Err(e)) => {
                        componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
                    }
                    None => pequeno(ui, "Escolha uma regra para ler o documento.", p.tinta_suave),
                }
            });
            if let Some((erro, m)) = &t.mensagem {
                ui.add_space(4.0);
                if *erro {
                    componentes::aviso(ui, "error", m, p.perigo_sutil, p.borda, p.perigo);
                } else {
                    componentes::aviso(ui, "check_circle", m, p.sucesso_sutil, p.borda, p.sucesso);
                }
            }
            ui.add_space(8.0);
            if t.guardando {
                ui.horizontal(|ui| {
                    ui.label("Nome do exemplo");
                    ui.add(
                        TextEdit::singleline(&mut t.nome_do_exemplo)
                            .hint_text("Pix de R$ 187,50")
                            .desired_width(260.0),
                    );
                    if ui.button("Guardar").clicked() {
                        guardar = true;
                    }
                    if ui.button("Cancelar").clicked() {
                        t.guardando = false;
                    }
                });
                pequeno(
                    ui,
                    "O exemplo guarda o que a regra leu agora (ou \"não se aplica\"). Ele passa a valer nos testes da regra.",
                    p.tinta_suave,
                );
            } else {
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(t.leitura.is_some(), egui::Button::new("Guardar como exemplo"))
                        .clicked()
                    {
                        t.guardando = true;
                        t.mensagem = None;
                    }
                    if ui.button("Fechar").clicked() {
                        fechar = true;
                    }
                });
            }
        });
        if resposta.should_close() {
            fechar = true;
        }
        if fechar {
            self.fechar_teste();
        } else if mudou {
            self.rodar_teste();
        } else if guardar {
            self.guardar_teste_como_exemplo();
        }
    }

    fn dialogo_de_criacao(&mut self, ctx: &Context) {
        let p = tema::paleta(ctx);
        let Some(c) = self.reconhecimento.criacao.as_mut() else {
            return;
        };
        let mut montar = false;
        let mut salvar = false;
        let mut fechar = false;
        let resposta = egui::Modal::new(Id::new("reconhecimento-criacao")).show(ctx, |ui| {
            ui.set_width(620.0);
            componentes::titulo(ui, "Criar regra a partir daqui", 17.0);
            pequeno(
                ui,
                format!(
                    "{} · o sbf monta um rascunho a partir do texto deste documento, sem IA. Ele só reconhece algo depois que você ativar a regra.",
                    c.documento
                ),
                p.tinta_suave,
            );
            ui.add_space(8.0);
            let editavel = c.montado.is_none();
            ScrollArea::vertical()
                .id_salt("reconhecimento-criacao-rolagem")
                .max_height(420.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if !editavel {
                        pequeno(
                            ui,
                            format!("{} · tipo {}", c.nome.trim(), c.tipo_documental.trim()),
                            p.tinta_suave,
                        );
                    }
                    if editavel {
                      ui.scope(|ui| {
                        egui::Grid::new("criacao-campos")
                            .num_columns(2)
                            .spacing([14.0, 5.0])
                            .show(ui, |ui| {
                                ui.label("Nome da regra");
                                ui.add(
                                    TextEdit::singleline(&mut c.nome)
                                        .hint_text("Comprovante Pix · Banco Exemplo")
                                        .desired_width(340.0),
                                );
                                ui.end_row();
                                ui.label("Tipo documental");
                                ui.add(
                                    TextEdit::singleline(&mut c.tipo_documental)
                                        .hint_text("comprovante-pix")
                                        .desired_width(340.0),
                                );
                                ui.end_row();
                            });
                        ui.add_space(8.0);
                        componentes::rotulo_de_secao(ui, "Campos que a regra deve ler");
                        pequeno(
                            ui,
                            "Preencha o que está certo neste documento; deixe em branco o que não interessa. Valores: R$ 1.250,00, 12/10/2026, texto.",
                            p.tinta_suave,
                        );
                        ui.add_space(2.0);
                        egui::Grid::new("criacao-linhas")
                            .num_columns(2)
                            .spacing([14.0, 5.0])
                            .show(ui, |ui| {
                                for l in &mut c.linhas {
                                    linha(ui, l);
                                    ui.end_row();
                                }
                            });
                        if ui.small_button("Adicionar campo livre").clicked() {
                            c.linhas.push(LinhaDeCampo {
                                nome: String::new(),
                                texto: String::new(),
                                livre: true,
                            });
                        }
                    });
                    }
                    if let Some(m) = &c.montado {
                        ui.add_space(10.0);
                        componentes::rotulo_de_secao(ui, "Rascunho montado");
                        componentes::cartao(ui, |ui| {
                            ui.set_width(ui.available_width());
                            ui.label(
                                RichText::new(format!("Regra {}", m.nova.regra)).strong(),
                            );
                            pequeno(
                                ui,
                                format!(
                                    "Autor: {} · fica em rascunho; ativar é com você, na página da regra.",
                                    rotulo_do_autor(Autor::Correcao)
                                ),
                                p.tinta_suave,
                            );
                            if let sbf_regras::catalogo::Esperado::Reconhece { campos, .. } =
                                &m.esperado
                            {
                                ui.add_space(3.0);
                                pequeno(ui, "A regra aprendeu a ler:", p.tinta_suave);
                                let mut ordem: Vec<_> = campos.iter().collect();
                                ordem.sort_by_key(|(n, _)| {
                                    (
                                        CAMPOS_USUAIS
                                            .iter()
                                            .position(|c| c == &n.as_str())
                                            .unwrap_or(CAMPOS_USUAIS.len()),
                                        (*n).clone(),
                                    )
                                });
                                for (n, v) in ordem {
                                    pequeno(
                                        ui,
                                        format!("{}: {v}", rotulo_do_campo(n)),
                                        p.tinta,
                                    );
                                }
                            }
                            if m.motivos.is_empty() {
                                ui.add_space(3.0);
                                pequeno(
                                    ui,
                                    "Todos os campos foram achados no texto.",
                                    p.sucesso,
                                );
                            } else {
                                ui.add_space(3.0);
                                pequeno(ui, "Ficaram de fora:", p.aviso);
                                for motivo in &m.motivos {
                                    aviso_com_triangulo(ui, &p, motivo);
                                }
                            }
                            ui.add_space(3.0);
                            pequeno(
                                ui,
                                "Salvar guarda este documento como o primeiro exemplo da regra.",
                                p.tinta_suave,
                            );
                        });
                    }
                });
            if let Some(e) = &c.erro {
                ui.add_space(4.0);
                componentes::aviso(ui, "error", e, p.perigo_sutil, p.borda, p.perigo);
            }
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if c.montado.is_some() {
                    if ui.button("Salvar rascunho e abrir a regra").clicked() {
                        salvar = true;
                    }
                    if ui.button("Voltar e editar").clicked() {
                        c.montado = None;
                    }
                } else if ui.button("Montar rascunho").clicked() {
                    montar = true;
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
            self.fechar_criacao();
        } else if montar {
            self.montar_criacao();
        } else if salvar && self.salvar_criacao().is_some() {
            navegacao::pedir_abrir(
                ctx,
                &mut self.pedidos,
                Destino::AreaFutura(crate::regras_ui::ROTA),
                false,
            );
        }
    }
}

fn rotulo_do_estado_opcao(o: &crate::regras::OpcaoDeTeste) -> &'static str {
    rotulo_do_estado(o.estado)
}

fn linha(ui: &mut Ui, l: &mut LinhaDeCampo) {
    if l.livre {
        ui.add(
            TextEdit::singleline(&mut l.nome)
                .hint_text("campo livre (ex.: multa)")
                .desired_width(150.0),
        );
    } else {
        debug_assert!(CAMPOS_USUAIS.contains(&l.nome.as_str()));
        ui.label(rotulo_do_campo(&l.nome));
    }
    ui.add(TextEdit::singleline(&mut l.texto).desired_width(340.0));
}
