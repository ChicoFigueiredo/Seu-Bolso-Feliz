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
use crate::padroes_visiveis::{
    Desfecho, Papel, PesosDasVersoes, PorQue, montar_por_que, rotulo_do_tipo_de_entrada,
};
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
    /// A prioridade e a especificidade das versões que a leitura cita ("Por que este padrão?").
    pub pesos: &'a PesosDasVersoes,
}

/// O que o cartão deixa o desenho mudar: a correção dos campos, o Rastro e o "Por que este
/// padrão?" abertos.
pub(crate) struct EdicaoDoCartao<'a> {
    pub corrigindo: &'a mut bool,
    pub edicao: &'a mut BTreeMap<String, String>,
    pub rastro_aberto: &'a mut bool,
    pub por_que_aberto: &'a mut bool,
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
    format!("Padrão {nome} · versão {versao}")
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
        .on_hover_text("Abrir o padrão na área Padrões de reconhecimento")
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
        ui.label(RichText::new("Reconhecido pelo padrão").strong());
    });
    ui.horizontal_wrapped(|ui| {
        if ui
            .link(RichText::new(extras.nome_da(&r.regra)).strong())
            .on_hover_text("Abrir o padrão na área Padrões de reconhecimento")
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
                    "Mudar um valor que o padrão leu errado. Aceitar depois disso conta como correção para o padrão.",
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
                                "Padrão {} · versão {} {o_que}{motivo}",
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
                                    "outro padrão"
                                } else {
                                    "outros padrões"
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
                "Precisa revisar · {} padrões discordam",
                numero_por_extenso(candidatas.len())
            ))
            .strong()
            .color(p.aviso),
        );
    });
    pequeno(ui, motivo, p.tinta_suave);
    pequeno(
        ui,
        "Nada foi aplicado. Ajuste a prioridade ou as condições de um dos padrões, ou corrija o documento.",
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
                                            "Os padrões divergem neste campo (um não o lê ou lê outro valor)",
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
                            .small_button("Abrir o padrão")
                            .on_hover_text("Abrir o padrão na área Padrões de reconhecimento")
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
            .on_hover_text("Lê este documento com um padrão, sem gravar nada")
            .clicked()
        {
            pedidos.push(Pedido::AbrirTeste(indice));
        }
        let criar = if extras.sem_padrao {
            egui::Button::new(RichText::new("Criar padrão a partir daqui").strong())
                .fill(tema::paleta(ui.ctx()).destaque_sutil)
        } else {
            egui::Button::new("Criar padrão a partir daqui")
        };
        if ui
            .add(criar)
            .on_hover_text("Monta um rascunho de padrão com o que você corrigir; ele só vale depois que você ativar")
            .clicked()
        {
            pedidos.push(Pedido::AbrirCriacao(indice));
        }
    });
}

// ---------------------------------------------------------------------------------------
// "Por que este padrão?" (spec 15, D6)
// ---------------------------------------------------------------------------------------

fn plural(n: usize, singular: &str, plural: &str) -> String {
    format!("{n} {}", if n == 1 { singular } else { plural })
}

/// O campo em que as candidatas de um Ambíguo discordam, como a pessoa lê.
fn campo_que_diverge(campo: &str) -> String {
    if campo == "tipo_documental" {
        "o tipo documental".to_owned()
    } else {
        format!("o campo {}", rotulo_do_campo(campo).to_lowercase())
    }
}

/// A seção recolhível "Por que este padrão?": o tipo da entrada, quantos padrões se aplicam,
/// os que casaram (com prioridade e especificidade), o vencedor e o motivo, ou o Ambíguo com o
/// campo em que discordam. Tudo sai do que o motor devolveu na [`Leitura`].
pub(crate) fn por_que(
    ui: &mut Ui,
    p: &Paleta,
    indice: usize,
    extras: &ExtrasDaRegra<'_>,
    aberto: &mut bool,
    pedidos: &mut Vec<Pedido>,
) {
    let Some(leitura) = extras.leitura else {
        return;
    };
    let q = montar_por_que(leitura, extras.entrada, extras.pesos);
    let resposta = egui::CollapsingHeader::new(
        RichText::new("Por que este padrão?")
            .size(12.0)
            .color(p.tinta_suave),
    )
    .id_salt(("por-que-este-padrao", indice))
    .open(Some(*aberto))
    .show(ui, |ui| corpo_do_por_que(ui, p, &q, extras, pedidos));
    if resposta.header_response.clicked() {
        *aberto = !*aberto;
    }
}

fn corpo_do_por_que(
    ui: &mut Ui,
    p: &Paleta,
    q: &PorQue,
    extras: &ExtrasDaRegra<'_>,
    pedidos: &mut Vec<Pedido>,
) {
    let tipo = q
        .tipo_da_entrada
        .map_or("documento", rotulo_do_tipo_de_entrada);
    let mime = q
        .mime
        .as_deref()
        .map(|m| format!(" ({m})"))
        .unwrap_or_default();
    pequeno(ui, format!("Entrada: {tipo}{mime}"), p.tinta);
    if q.aplicam == 0 {
        pequeno(
            ui,
            "Nenhum padrão se aplica a este tipo de entrada.",
            p.tinta_suave,
        );
    } else {
        pequeno(
            ui,
            format!(
                "{} a este tipo de entrada; {} {}.",
                plural(q.aplicam, "padrão se aplica", "padrões se aplicam"),
                q.casaram.len(),
                if q.casaram.len() == 1 {
                    "casou"
                } else {
                    "casaram"
                }
            ),
            p.tinta_suave,
        );
    }
    for c in &q.casaram {
        ui.horizontal_wrapped(|ui| {
            match c.papel {
                Papel::Venceu => componentes::selo(ui, "venceu", p.sucesso_sutil, p.sucesso),
                Papel::Discorda => componentes::selo(ui, "discorda", p.aviso_sutil, p.aviso),
                Papel::NaoPrevaleceu => {
                    componentes::selo(ui, "não prevaleceu", p.superficie_3, p.tinta_suave)
                }
            };
            if ui
                .link(RichText::new(extras.nome_da(&c.regra)).size(12.0))
                .on_hover_text("Abrir o padrão na área Padrões de reconhecimento")
                .clicked()
            {
                pedidos.push(Pedido::AbrirRegra(c.regra.clone()));
            }
            let pesos = c.peso.map_or_else(String::new, |w| {
                format!(
                    " · prioridade {} · especificidade {}",
                    w.prioridade, w.especificidade
                )
            });
            pequeno(ui, format!("versão {}{pesos}", c.versao), p.tinta_suave);
            if let Some(nota) = &c.nota {
                pequeno(ui, nota, p.tinta_desabilitada);
            }
        });
    }
    for f in &q.falharam {
        pequeno(
            ui,
            format!(
                "{} · versão {} detectou o documento e falhou ({})",
                extras.nome_da(&f.regra),
                f.versao,
                f.categoria
            ),
            p.aviso,
        );
    }
    ui.add_space(2.0);
    match &q.desfecho {
        Desfecho::Venceu {
            regra,
            versao,
            motivo,
        } => {
            ui.label(
                RichText::new(format!(
                    "Venceu: {} · versão {versao}.",
                    extras.nome_da(regra)
                ))
                .size(12.0)
                .strong(),
            );
            let por = match (motivo, q.casaram.len()) {
                (Some(m), _) => Some(m.clone()),
                (None, 1) => Some("foi o único padrão que casou.".to_owned()),
                (None, _) => None,
            };
            if let Some(m) = por {
                pequeno(ui, format!("Motivo: {m}"), p.tinta_suave);
            }
        }
        Desfecho::Ambiguo {
            motivo,
            divergencia,
        } => {
            ui.horizontal_wrapped(|ui| {
                ui.label(icones::icone_cor("warning", 14.0, p.aviso));
                let onde = divergencia.as_ref().map_or_else(
                    || "em prioridade, especificidade e resultado".to_owned(),
                    |d| format!("em {}", campo_que_diverge(&d.campo)),
                );
                ui.label(
                    RichText::new(format!(
                        "Ambíguo: {} empataram em prioridade e especificidade e discordam {onde}. Nada foi aplicado.",
                        plural(
                            q.casaram
                                .iter()
                                .filter(|c| c.papel == Papel::Discorda)
                                .count(),
                            "padrão",
                            "padrões"
                        )
                    ))
                    .size(12.0)
                    .strong()
                    .color(p.aviso),
                );
            });
            if let Some(d) = divergencia {
                for (regra, versao, valor) in &d.valores {
                    pequeno(
                        ui,
                        format!("{} · versão {versao} lê {valor}", extras.nome_da(regra)),
                        p.tinta_suave,
                    );
                }
            }
            pequeno(ui, motivo, p.tinta_desabilitada);
        }
        Desfecho::NenhumCasou => {
            pequeno(ui, "Nenhum padrão casou com este documento.", p.tinta_suave);
        }
    }
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
        Sugestao::Vincular { data, .. } => {
            pequeno(
                ui,
                format!(
                    "{}Confirmar liga o documento à ocorrência; nada é conciliado nem criado.",
                    data.map(|d| format!("Documento de {}. ", data_texto(d)))
                        .unwrap_or_default()
                ),
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
            format!("Proposta do padrão {} · versão {}", regra.0, regra.1),
            p.tinta_desabilitada,
        );
    }
}

/// "Conciliado automaticamente pela regra X ·" (o Desfazer vem em seguida).
pub(crate) fn texto_da_automatica(sug: &Sugestao, nomes: &BTreeMap<String, String>) -> String {
    match sug.da_regra() {
        Some(((id, _), _)) => format!(
            "Conciliado automaticamente pelo padrão {} ·",
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
        Proposta::VincularOcorrencia {
            data, compromisso, ..
        } => format!(
            "ligar o documento à ocorrência de {compromisso} perto de {}",
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
                    RichText::new("O padrão não se aplica a este documento.").color(p.tinta_suave),
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
                    "{} · só leitura: nada é gravado e o padrão não muda.",
                    t.documento
                ),
                p.tinta_suave,
            );
            ui.add_space(8.0);
            if t.opcoes.is_empty() {
                pequeno(ui, "Ainda não há padrões no catálogo.", p.tinta_suave);
            } else {
                ui.horizontal(|ui| {
                    ui.label("Padrão e versão");
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
                    None => pequeno(ui, "Escolha um padrão para ler o documento.", p.tinta_suave),
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
                    "O exemplo guarda o que o padrão leu agora (ou \"não se aplica\"). Ele passa a valer nos testes do padrão.",
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
            componentes::titulo(ui, "Criar padrão a partir daqui", 17.0);
            pequeno(
                ui,
                format!(
                    "{} · o sbf monta um rascunho a partir do texto deste documento, sem IA. Ele só reconhece algo depois que você ativar o padrão.",
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
                                ui.label("Nome do padrão");
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
                        componentes::rotulo_de_secao(ui, "Campos que o padrão deve ler");
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
                                RichText::new(format!("Padrão {}", m.nova.regra)).strong(),
                            );
                            pequeno(
                                ui,
                                format!(
                                    "Autor: {} · fica em rascunho; ativar é com você, na página do padrão.",
                                    rotulo_do_autor(Autor::Correcao)
                                ),
                                p.tinta_suave,
                            );
                            if let sbf_regras::catalogo::Esperado::Reconhece { campos, .. } =
                                &m.esperado
                            {
                                ui.add_space(3.0);
                                pequeno(ui, "O padrão aprendeu a ler:", p.tinta_suave);
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
                                "Salvar guarda este documento como o primeiro exemplo do padrão.",
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
                    if ui.button("Salvar rascunho e abrir o padrão").clicked() {
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
