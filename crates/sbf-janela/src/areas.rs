//! O painel de navegação com o produto inteiro (spec 06, "O produto inteiro na
//! navegação"). Funcionam o grupo Inventário (fatia 1), as áreas de Dinheiro sobre o
//! livro-caixa, Relatórios e a Sincronização (fatia 2); o resto aparece com o selo da
//! fatia em que chega e o motivo.

use egui::{Align, CornerRadius, Frame, Layout, Margin, RichText, ScrollArea, Sense, Ui};
use sbf_inventario::Tipo;
use sbf_inventario::consulta::{Area, Contagens};

use crate::componentes;
use crate::formato;
use crate::icones;
use crate::navegacao::{Destino, Pedido};
use crate::tema;

/// Uma área que chega numa fatia futura.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AreaFutura {
    pub id: &'static str,
    pub rotulo: &'static str,
    pub icone: &'static str,
    pub fatia: u8,
    pub motivo: &'static str,
}

/// As áreas das fatias futuras, na ordem do painel.
pub const AREAS_FUTURAS: &[AreaFutura] = &[
    AreaFutura {
        id: "caixa",
        rotulo: "Caixa de entrada",
        icone: "inbox",
        fatia: 3,
        motivo: "Os rascunhos do pipeline para corrigir, aprovar ou recusar (fatia 3)",
    },
    AreaFutura {
        id: "conexoes",
        rotulo: "Conexões",
        icone: "hub",
        fatia: 4,
        motivo: "Chega na fatia 4, com conexões, alertas e canais",
    },
    AreaFutura {
        id: "alertas",
        rotulo: "Alertas e canais",
        icone: "notifications",
        fatia: 4,
        motivo: "Chega na fatia 4, com conexões, alertas e canais",
    },
    AreaFutura {
        id: "roteiros",
        rotulo: "Roteiros",
        icone: "route",
        fatia: 5,
        motivo: "Chega na fatia 5, com os roteiros",
    },
    AreaFutura {
        id: "pontos",
        rotulo: "Pontos de atenção",
        icone: "lightbulb",
        fatia: 6,
        motivo: "Chega na fatia 6, com a análise",
    },
    AreaFutura {
        id: "destinatarios",
        rotulo: "Destinatários",
        icone: "shield_person",
        fatia: 6,
        motivo: "Destinatários e níveis de confiança chegam na fatia 6, com a análise",
    },
];

pub fn area_futura(id: &str) -> Option<&'static AreaFutura> {
    AREAS_FUTURAS.iter().find(|a| a.id == id)
}

/// As áreas do grupo Inventário, na ordem do painel.
pub const AREAS_DO_INVENTARIO: [Area; 16] = [
    Area::VisaoGeral,
    Area::ATriar,
    Area::EmDuvida,
    Area::Documentos,
    Area::Registros,
    Area::Ciclos,
    Area::Fornecedores,
    Area::Faturas,
    Area::ContasLacunas,
    Area::Emails,
    Area::ArquivosEPastas,
    Area::Coletas,
    Area::Pendencias,
    Area::Lotes,
    Area::SugestoesDeConciliacao,
    Area::TriagemPorIa,
];

/// As áreas de Dinheiro, na ordem do painel (o livro-caixa).
pub const AREAS_DE_DINHEIRO: [Area; 7] = Area::DO_LIVRO_CAIXA;

/// Todas as áreas que funcionam (o Inventário, Dinheiro, Relatórios e Sincronização).
pub fn areas_da_fatia() -> Vec<Area> {
    let mut v = AREAS_DO_INVENTARIO.to_vec();
    v.push(Area::Relatorios);
    v.extend(AREAS_DE_DINHEIRO);
    v.push(Area::Sincronizacao);
    v
}

/// A rota da área na casca. Onde dá, é o código do tipo principal, para a navegação
/// acender a área também na página do item (`fornecedor` acende em `fornecedor:x`).
pub fn rota_da_area(area: Area) -> &'static str {
    match area {
        Area::VisaoGeral => "visao-geral",
        Area::ATriar => "a-triar",
        Area::EmDuvida => "em-duvida",
        Area::Documentos => "documento",
        Area::Registros => "registro",
        Area::Ciclos => "ciclo",
        Area::Fornecedores => "fornecedor",
        Area::Faturas => "fatura",
        Area::ContasLacunas => "conta",
        Area::Emails => "emails",
        Area::EmailsPorRemetente => "emails-por-remetente",
        Area::ArquivosEPastas => "origem",
        Area::PastasPessoais => "pastas-pessoais",
        Area::Coletas => "coleta",
        Area::Pendencias => "pendencia",
        Area::Lotes => "lote",
        Area::SugestoesDeConciliacao => "sugestoes-conciliacao",
        Area::TriagemPorIa => "triagem-ia",
        Area::Relatorios => "relatorios",
        Area::Hoje => "hoje",
        Area::Movimentacoes => "mov",
        Area::ContasECartoes => "contas-cartoes",
        Area::Dividas => "divida",
        Area::Recorrentes => "recorrente",
        Area::Centros => "centro",
        Area::FornecedoresDoLivro => "fornecedores-cadastro",
        Area::Sincronizacao => "sincronizacao",
    }
}

/// O inverso de [`rota_da_area`].
pub fn area_da_rota(rota: &str) -> Option<Area> {
    let mut todas = areas_da_fatia();
    todas.push(Area::EmailsPorRemetente);
    todas.push(Area::PastasPessoais);
    todas.into_iter().find(|a| rota_da_area(*a) == rota)
}

pub fn rotulo_da_area(area: Area) -> &'static str {
    match area {
        Area::VisaoGeral => "Visão geral",
        Area::ATriar => "A triar",
        Area::EmDuvida => "Em dúvida",
        Area::Documentos => "Documentos",
        Area::Registros => "Registros",
        Area::Ciclos => "Ciclos",
        Area::Fornecedores => "Fornecedores nos registros",
        Area::Faturas => "Faturas (conferência)",
        Area::ContasLacunas => "Contas (lacunas)",
        Area::Emails => "E-mails",
        Area::EmailsPorRemetente => "E-mails por remetente",
        Area::PastasPessoais => "Pastas pessoais",
        Area::ArquivosEPastas => "Arquivos e pastas",
        Area::Coletas => "Coletas",
        Area::Pendencias => "Pendências",
        Area::Lotes => "Lotes",
        Area::SugestoesDeConciliacao => "Sugestões de conciliação",
        Area::TriagemPorIa => "Triagem por IA",
        Area::Relatorios => "Relatórios",
        Area::Hoje => "Hoje",
        Area::Movimentacoes => "Movimentações",
        Area::ContasECartoes => "Contas e cartões",
        Area::Dividas => "Dívidas",
        Area::Recorrentes => "Recorrentes",
        Area::Centros => "Centros",
        Area::FornecedoresDoLivro => "Fornecedores",
        Area::Sincronizacao => "Sincronização",
    }
}

pub fn icone_da_area(area: Area) -> &'static str {
    match area {
        Area::VisaoGeral => "space_dashboard",
        Area::ATriar => "schedule",
        Area::EmDuvida => "help",
        Area::Documentos => "task_alt",
        Area::Registros => "receipt_long",
        Area::Ciclos => "calendar_month",
        Area::Fornecedores => "storefront",
        Area::Faturas => "credit_card",
        Area::ContasLacunas => "account_balance",
        Area::Emails | Area::EmailsPorRemetente => "mail",
        Area::PastasPessoais => "folder",
        Area::ArquivosEPastas => "folder",
        Area::Coletas => "sync_saved_locally",
        Area::Pendencias => "flag",
        Area::Lotes => "inventory_2",
        Area::SugestoesDeConciliacao => "call_merge",
        Area::TriagemPorIa => "smart_toy",
        Area::Relatorios => "query_stats",
        Area::Hoje => "today",
        Area::Movimentacoes => "swap_vert",
        Area::ContasECartoes => "account_balance_wallet",
        Area::Dividas => "request_quote",
        Area::Recorrentes => "event_repeat",
        Area::Centros => "donut_small",
        Area::FornecedoresDoLivro => "storefront",
        Area::Sincronizacao => "cloud_sync",
    }
}

/// A linha sob o título da área.
pub fn descricao_da_area(area: Area) -> &'static str {
    match area {
        Area::VisaoGeral => {
            "O inventário em números: resumo, etapas, fontes, os últimos 13 ciclos e avisos."
        }
        Area::ATriar => "Achados que chegaram e ainda não passaram por nenhuma regra.",
        Area::EmDuvida => "Achados que a regra ou você deixou em dúvida.",
        Area::Documentos => "Achados confirmados como documento, com quem decidiu.",
        Area::Registros => {
            "Fatos financeiros que os documentos declaram. Ainda não são movimentações."
        }
        Area::Ciclos => "Os registros por ciclo (do dia 20 à véspera do dia 20 seguinte).",
        Area::Fornecedores => "Os fornecedores como aparecem nos registros.",
        Area::Faturas => "Faturas de cartão: o total declarado contra a soma dos registros.",
        Area::ContasLacunas => "Por conta, os ciclos sem nenhum registro.",
        Area::Emails => "Os e-mails por remetente e cada e-mail com anexo.",
        Area::EmailsPorRemetente => "Quantos e-mails e anexos cada remetente mandou.",
        Area::PastasPessoais => "As pastas pessoais onde os arquivos foram achados.",
        Area::ArquivosEPastas => "Os arquivos achados no disco e nas nuvens, e as pastas pessoais.",
        Area::Coletas => "Cada passagem por uma fonte do inventário.",
        Area::Pendencias => {
            "Perguntas que travam um achado, um registro ou um lote até você responder."
        }
        Area::Lotes => "Conjuntos de documentos e registros que você aprova junto para promoção.",
        Area::SugestoesDeConciliacao => {
            "Pares de registros que parecem o mesmo fato. Nada muda até você confirmar."
        }
        Area::TriagemPorIa => {
            "A IA sugere categoria, fornecedor e tipo. Nada muda até você aceitar."
        }
        Area::Relatorios => "As perguntas do §11 respondidas com os registros do inventário.",
        Area::Hoje => "Quanto eu tenho, quanto é meu, o que vence e como está o ciclo.",
        Area::Movimentacoes => "O que entrou e o que saiu, do livro-caixa.",
        Area::ContasECartoes => {
            "Onde está o dinheiro e quanto devo no cartão. Disponível agora soma só as contas."
        }
        Area::Dividas => "Quanto falta e quando acaba.",
        Area::Recorrentes => "Quanto do ciclo já está prometido.",
        Area::Centros => "Quanto foi para cada centro e quanto não foi alocado.",
        Area::FornecedoresDoLivro => {
            "Quanto e com que frequência sai para cada fornecedor, no livro-caixa."
        }
        Area::Sincronizacao => {
            "As estações, a última troca com o servidor e as alterações. O inventário não sincroniza."
        }
    }
}

/// A outra metade de uma área que mostra duas vistas (spec 06, "Áreas e vistas"): a
/// janela desenha esta tabela acima da principal.
pub fn area_complementar(area: Area) -> Option<Area> {
    match area {
        Area::Emails => Some(Area::EmailsPorRemetente),
        Area::ArquivosEPastas => Some(Area::PastasPessoais),
        _ => None,
    }
}

/// A área onde um tipo de item mora (a trilha "Fornecedores › Energia Exemplo").
pub fn destino_do_tipo(tipo: Tipo) -> Option<Destino> {
    let area = match tipo {
        Tipo::Achado | Tipo::Documento => Area::Documentos,
        Tipo::Origem => Area::ArquivosEPastas,
        Tipo::Registro => Area::Registros,
        Tipo::Decisao => Area::Registros,
        Tipo::Lote => Area::Lotes,
        Tipo::Pendencia => Area::Pendencias,
        Tipo::Coleta | Tipo::Conexao => Area::Coletas,
        Tipo::Ciclo => Area::Ciclos,
        Tipo::Fornecedor => Area::Fornecedores,
        Tipo::Conta | Tipo::Instituicao | Tipo::Produto | Tipo::Cartao => Area::ContasECartoes,
        Tipo::Fatura => Area::Faturas,
        Tipo::RegraDeSenha => Area::Documentos,
        Tipo::Movimentacao | Tipo::Transferencia | Tipo::Categoria | Tipo::Tag => {
            Area::Movimentacoes
        }
        Tipo::Divida | Tipo::Parcela => Area::Dividas,
        Tipo::Compromisso | Tipo::Recorrente => Area::Recorrentes,
        Tipo::Centro => Area::Centros,
        Tipo::Contrato => Area::FornecedoresDoLivro,
        Tipo::Estacao | Tipo::Alteracao => Area::Sincronizacao,
        Tipo::Alerta | Tipo::Canal => return Some(Destino::AreaFutura("alertas")),
        Tipo::Roteiro => return Some(Destino::AreaFutura("roteiros")),
        Tipo::Ponto => return Some(Destino::AreaFutura("pontos")),
        Tipo::Destinatario => return Some(Destino::AreaFutura("destinatarios")),
    };
    Some(Destino::Area(area))
}

/// O nome curto de um destino de área (trilha, título da janela).
pub fn rotulo_do_destino(destino: &Destino) -> String {
    match destino {
        Destino::Area(a) => rotulo_da_area(*a).to_owned(),
        Destino::AreaFutura(id) => {
            area_futura(id).map_or((*id).to_owned(), |a| a.rotulo.to_owned())
        }
        Destino::Item(r) => format!("{} {}", r.tipo.singular(), r.chave),
        Destino::Total { titulo, .. } => titulo.clone(),
    }
}

fn grupo_do_destino(destino: &Destino) -> &'static str {
    match destino {
        Destino::Area(Area::Relatorios) => "Dinheiro",
        Destino::Area(Area::Sincronizacao) => "Estação",
        Destino::Area(a) if a.e_do_livro_caixa() => "Dinheiro",
        Destino::Area(_) => "Inventário",
        Destino::AreaFutura(id) => match *id {
            "caixa" => "Documentos",
            "conexoes" | "alertas" | "roteiros" => "Fontes",
            "sincronizacao" => "Estação",
            "pontos" | "destinatarios" => "Análise",
            _ => "Dinheiro",
        },
        _ => "",
    }
}

/// O grupo do painel de um destino de área ("Inventário", "Dinheiro"…).
pub fn grupo_de(destino: &Destino) -> &'static str {
    grupo_do_destino(destino)
}

fn contagem_da_area(area: Area, c: &Contagens) -> Option<i64> {
    match area {
        Area::ATriar => Some(c.a_triar),
        Area::EmDuvida => Some(c.em_duvida),
        Area::Documentos => Some(c.documentos),
        _ => None,
    }
}

fn entrada(
    ui: &mut Ui,
    icone: &str,
    rotulo: &str,
    ativa: bool,
    lado: impl FnOnce(&mut Ui),
    dica: Option<&str>,
) -> egui::Response {
    let p = tema::paleta(ui.ctx());
    let fundo = if ativa {
        p.destaque_sutil
    } else {
        egui::Color32::TRANSPARENT
    };
    let quadro = Frame::new()
        .fill(fundo)
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::symmetric(8, 3))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                let cor = if ativa { p.destaque_tinta } else { p.tinta };
                ui.label(icones::icone_cor(icone, 17.0, cor));
                let texto = RichText::new(rotulo).color(cor);
                ui.add(egui::Label::new(if ativa { texto.strong() } else { texto }).truncate());
                ui.with_layout(Layout::right_to_left(Align::Center), lado);
            });
        });
    let r = ui.interact(quadro.response.rect, ui.id().with(rotulo), Sense::click());
    if r.hovered() && !ativa {
        ui.painter().rect_filled(
            quadro.response.rect,
            CornerRadius::same(4),
            p.superficie_3.gamma_multiply(0.5),
        );
    }
    let r = r.on_hover_cursor(egui::CursorIcon::PointingHand);
    match dica {
        Some(d) => r.on_hover_text(d),
        None => r,
    }
}

/// Desenha o painel de navegação. Clique abre a área no centro.
pub fn painel_de_navegacao(
    ui: &mut Ui,
    atual: &Destino,
    contagens: Option<&Contagens>,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let ativo_de = |d: &Destino| -> bool {
        atual == d
            || match atual {
                Destino::Item(r) => destino_do_tipo(r.tipo).as_ref() == Some(d),
                _ => false,
            }
    };
    ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            let futuras = |ui: &mut Ui, ids: &[&str], pedidos: &mut Vec<Pedido>| {
                for id in ids {
                    if let Some(a) = area_futura(id) {
                        let d = Destino::AreaFutura(a.id);
                        let r = entrada(
                            ui,
                            a.icone,
                            a.rotulo,
                            ativo_de(&d),
                            |ui| {
                                componentes::selo_fatia(ui, a.fatia);
                            },
                            Some(a.motivo),
                        );
                        if r.clicked() {
                            pedidos.push(Pedido::Abrir(d));
                        }
                    }
                }
            };
            let cabecalho = |ui: &mut Ui, texto: &str, fatia: Option<u8>| {
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    componentes::rotulo_de_secao(ui, texto);
                    if let Some(f) = fatia {
                        componentes::selo_fatia(ui, f);
                    }
                });
                ui.add_space(3.0);
            };

            let real = |ui: &mut Ui, area: Area, pedidos: &mut Vec<Pedido>| {
                let d = Destino::Area(area);
                let r = entrada(
                    ui,
                    icone_da_area(area),
                    rotulo_da_area(area),
                    ativo_de(&d),
                    |_| {},
                    None,
                );
                if r.clicked() {
                    pedidos.push(Pedido::Abrir(d));
                }
            };
            cabecalho(ui, "Dinheiro", Some(2));
            for area in AREAS_DE_DINHEIRO {
                real(ui, area, pedidos);
            }
            real(ui, Area::Relatorios, pedidos);

            cabecalho(ui, "Documentos", None);
            futuras(ui, &["caixa"], pedidos);

            cabecalho(ui, "Inventário", Some(1));
            for area in AREAS_DO_INVENTARIO {
                let d = Destino::Area(area);
                let n = contagens.and_then(|c| contagem_da_area(area, c));
                let r = entrada(
                    ui,
                    icone_da_area(area),
                    rotulo_da_area(area),
                    ativo_de(&d),
                    |ui| {
                        if let Some(n) = n {
                            ui.label(
                                RichText::new(formato::inteiro(n))
                                    .size(12.0)
                                    .color(p.tinta_suave),
                            );
                        }
                    },
                    None,
                );
                if r.clicked() {
                    pedidos.push(Pedido::Abrir(d));
                }
            }

            cabecalho(ui, "Fontes", None);
            futuras(ui, &["conexoes", "alertas", "roteiros"], pedidos);
            cabecalho(ui, "Estação", Some(2));
            real(ui, Area::Sincronizacao, pedidos);
            cabecalho(ui, "Análise", None);
            futuras(ui, &["pontos", "destinatarios"], pedidos);
            ui.add_space(8.0);
        });
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn todo_tipo_tem_uma_area() {
        for t in Tipo::TODOS {
            assert!(destino_do_tipo(t).is_some(), "{t}");
        }
    }

    #[test]
    fn areas_futuras_tem_icone_e_motivo() {
        for a in AREAS_FUTURAS {
            assert!(icones::caractere(a.icone).is_some(), "{}", a.icone);
            assert!(a.fatia >= 2 && !a.motivo.is_empty());
        }
        for a in areas_da_fatia() {
            assert!(icones::caractere(icone_da_area(a)).is_some());
        }
    }
}
