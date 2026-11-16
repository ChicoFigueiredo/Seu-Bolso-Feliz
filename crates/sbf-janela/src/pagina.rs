//! A página de um item (spec 06, "Navegação relacional"): cabeçalho com o resumo,
//! gráfico por ciclo quando faz sentido, abas com tudo o que se liga ao item e a aba
//! Rastro. Tipo de fatia futura: a página com o selo e o motivo.

use std::collections::HashMap;

use egui::{
    Align, CornerRadius, FontId, Frame, IntoAtoms, Layout, Margin, RichText, Sense, Stroke, Ui,
    vec2,
};
use sbf_inventario::consulta::{Celula, Fonte, PedidoDeTabela, Rastro, TotalDoCiclo};
use sbf_inventario::{Ref, Tipo};

use crate::componentes;
use crate::formato::{self, A_CONFERIR};
use crate::icones;
use crate::leitura::Leitor;
use crate::navegacao::{Destino, Pedido};
use crate::tabela::{self, EstadoDaTabela, Selecao};
use crate::tema::{self, familia_forte};

/// O estado de um painel que mostra destinos (o centro ou um flutuante).
#[derive(Debug, Clone, Default)]
pub struct Vista {
    pub selecao: Selecao,
    pub tabelas: HashMap<String, EstadoDaTabela>,
    pub abas: HashMap<Ref, usize>,
}

impl Vista {
    pub fn estado_da_tabela(&mut self, chave: &str) -> &mut EstadoDaTabela {
        self.tabelas.entry(chave.to_owned()).or_default()
    }
}

/// O que uma aba de página mostra.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConteudoDaAba {
    /// Os itens de um tipo ligados ao item (`PedidoDeTabela::Ligados`).
    Ligados(Tipo),
    /// O texto extraído do achado.
    Texto,
    /// A cadeia de proveniência.
    Rastro,
    /// A prévia do arquivo do item (o do achado, ou o documento da origem e do registro).
    Arquivo,
    /// Um recorte do livro-caixa sobre o item (`ItensDoTotal` com o recorte): o pagamento
    /// de uma fatura, os rateios de um centro, a simulação de uma dívida…
    Recorte(&'static str),
    /// As alterações deste item feitas nesta estação (a estação, não a `Consulta`).
    Alteracoes,
}

/// Uma aba de página. Desabilitada traz o motivo (as abas do livro-caixa nesta fatia).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AbaDePagina {
    pub rotulo: String,
    pub conteudo: ConteudoDaAba,
    pub motivo: Option<String>,
}

/// A fatia que esta janela já tem: o inventário (1) e o livro-caixa só leitura (2).
pub const FATIA_DA_JANELA: u8 = 2;

/// O motivo de um tipo que ainda não funciona nesta janela (fatias 3 em diante).
pub fn motivo_do_tipo(tipo: Tipo) -> Option<String> {
    if tipo.fatia() <= FATIA_DA_JANELA {
        None
    } else {
        tipo.motivo_da_fatia()
    }
}

fn aba(rotulo: &str, tipo: Tipo) -> AbaDePagina {
    let rotulo = if tipo == Tipo::Registro && !rotulo.contains(A_CONFERIR) {
        format!("{rotulo} · {A_CONFERIR}")
    } else {
        rotulo.to_owned()
    };
    AbaDePagina {
        rotulo,
        conteudo: ConteudoDaAba::Ligados(tipo),
        motivo: motivo_do_tipo(tipo),
    }
}

fn aba_desabilitada(rotulo: &str, tipo: Tipo, motivo: &str) -> AbaDePagina {
    AbaDePagina {
        motivo: Some(motivo.to_owned()),
        ..aba(rotulo, tipo)
    }
}

fn recorte(rotulo: &str, r: &'static str) -> AbaDePagina {
    aba_especial(rotulo, ConteudoDaAba::Recorte(r))
}

/// A ligação registro → movimentação nasce com a promoção.
const MOTIVO_PROMOCAO: &str =
    "A ligação com os registros do inventário nasce com a promoção, na fatia 3";

fn aba_especial(rotulo: &str, conteudo: ConteudoDaAba) -> AbaDePagina {
    AbaDePagina {
        rotulo: rotulo.to_owned(),
        conteudo,
        motivo: None,
    }
}

/// As abas da página de um tipo. Toda página termina na aba Rastro.
pub fn abas_de(tipo: Tipo) -> Vec<AbaDePagina> {
    use Tipo as T;
    let mut v = match tipo {
        T::Fornecedor => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Compromissos e recorrentes", T::Recorrente),
            aba("Registros", T::Registro),
            aba("Documentos", T::Documento),
            aba("Contratos", T::Contrato),
            aba("Regras de senha", T::RegraDeSenha),
        ],
        T::Movimentacao => vec![
            aba("Rateio por centro", T::Centro),
            aba("Documento", T::Documento),
            aba("Ligações", T::Movimentacao),
            aba_desabilitada("Registro de origem", T::Registro, MOTIVO_PROMOCAO),
            aba_especial("Alterações", ConteudoDaAba::Alteracoes),
        ],
        T::Transferencia => vec![
            aba("Contas", T::Conta),
            aba("Movimentações", T::Movimentacao),
            aba("Documentos", T::Documento),
        ],
        T::Instituicao => vec![
            aba("Contas", T::Conta),
            aba("Cartões", T::Cartao),
            aba("Produtos", T::Produto),
            aba("Dívidas", T::Divida),
        ],
        T::Produto => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Instituição e conta", T::Instituicao),
            aba("Dívidas", T::Divida),
            aba("Documentos", T::Documento),
        ],
        T::Cartao => vec![
            aba("Faturas", T::Fatura),
            aba("Itens", T::Movimentacao),
            aba("Documentos", T::Documento),
            recorte("Compromissos e dívidas", "compromissos-e-dividas"),
        ],
        T::Categoria | T::Tag => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Fornecedores", T::Fornecedor),
            aba("Compromissos e recorrentes", T::Recorrente),
            aba("Documentos", T::Documento),
        ],
        T::Centro => vec![
            recorte("Rateios", "rateios"),
            aba("Movimentações", T::Movimentacao),
            aba("Compromissos e recorrentes", T::Recorrente),
            aba("Documentos", T::Documento),
        ],
        T::Compromisso => vec![
            recorte("Origem", "origem"),
            recorte("Pagamento", "pagamento"),
            aba("Documentos", T::Documento),
        ],
        T::Recorrente => vec![
            aba("Ocorrências", T::Compromisso),
            recorte("Próximos compromissos", "proximos"),
            recorte("Fornecedor e contrato", "fornecedor-e-contrato"),
            aba("Documentos", T::Documento),
        ],
        T::Divida => vec![
            aba("Parcelas", T::Parcela),
            recorte("Simulação SAC × Price", "simulacao"),
            aba("Movimentações", T::Movimentacao),
            aba("Contrato", T::Contrato),
            aba("Documentos", T::Documento),
        ],
        T::Parcela => vec![
            recorte("Pagamento", "pagamento"),
            aba("Boleto e documentos", T::Documento),
            aba("Ligações", T::Parcela),
        ],
        T::Contrato => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Compromissos e recorrentes", T::Recorrente),
            aba("Documentos", T::Documento),
            recorte("Dívidas e parcelas", "parcelas-e-dividas"),
        ],
        T::Achado | T::Documento => vec![
            aba_especial("Arquivo", ConteudoDaAba::Arquivo),
            aba("Origens", T::Origem),
            aba_especial("Texto extraído", ConteudoDaAba::Texto),
            aba("Registros que saíram dele", T::Registro),
            aba("Decisões", T::Decisao),
            aba("Duplicidades", T::Achado),
            aba("Pendências", T::Pendencia),
            aba("Lotes", T::Lote),
        ],
        T::Origem => vec![
            aba("Achado", T::Achado),
            aba_especial("Arquivo", ConteudoDaAba::Arquivo),
            aba("Coleta", T::Coleta),
            aba("Conexão", T::Conexao),
        ],
        T::Registro => vec![
            aba("Documento", T::Documento),
            aba_especial("Arquivo", ConteudoDaAba::Arquivo),
            aba("Decisões", T::Decisao),
            aba("Conciliados", T::Registro),
            aba("Lotes", T::Lote),
            aba("Pendências", T::Pendencia),
            aba("Movimentação", T::Movimentacao),
        ],
        T::Decisao => vec![
            aba("Achados", T::Achado),
            aba("Registros", T::Registro),
            aba("Desfaz ou desfeita por", T::Decisao),
        ],
        T::Lote => vec![
            aba("Documentos", T::Documento),
            aba("Registros", T::Registro),
            aba("Decisões", T::Decisao),
            aba("Pendências", T::Pendencia),
        ],
        T::Pendencia => vec![
            aba("Achados", T::Achado),
            aba("Registros", T::Registro),
            aba("Lotes", T::Lote),
            aba("Decisões", T::Decisao),
        ],
        T::RegraDeSenha => vec![
            aba("Fornecedor", T::Fornecedor),
            aba("Documentos", T::Documento),
            aba("Decisões", T::Decisao),
        ],
        T::Conexao => vec![
            aba("Coletas", T::Coleta),
            aba("Origens", T::Origem),
            aba("Achados", T::Achado),
        ],
        T::Coleta => vec![
            aba("Origens", T::Origem),
            aba("Achados", T::Achado),
            aba("Conexão", T::Conexao),
        ],
        T::Ciclo => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Compromissos", T::Compromisso),
            aba("Registros", T::Registro),
            aba("Documentos", T::Documento),
            aba("Fornecedores", T::Fornecedor),
        ],
        T::Conta => vec![
            aba("Movimentações", T::Movimentacao),
            aba("Transferências", T::Transferencia),
            aba("Extratos e documentos", T::Documento),
            aba("Registros", T::Registro),
            aba("Faturas", T::Fatura),
            aba_desabilitada(
                "Conexão de origem",
                T::Conexao,
                "Chega na fatia 4, com conexões, alertas e canais",
            ),
        ],
        T::Fatura => vec![
            recorte("Itens", "itens"),
            recorte("Pagamento", "pagamento"),
            aba("Documento da fatura", T::Documento),
            aba("Registros", T::Registro),
        ],
        // Estação e Alteração têm página própria (`dinheiro`), sem `Consulta`.
        // Fatia futura: só o selo, sem abas além do Rastro.
        _ => Vec::new(),
    };
    v.push(aba_especial("Rastro", ConteudoDaAba::Rastro));
    v
}

/// Tipos cuja página mostra o gráfico de totais por ciclo.
pub fn tem_grafico_por_ciclo(tipo: Tipo) -> bool {
    matches!(
        tipo,
        Tipo::Fornecedor | Tipo::Conta | Tipo::Categoria | Tipo::Centro | Tipo::Cartao
    )
}

/// A fonte do gráfico por ciclo: os tipos só do livro-caixa somam movimentações; os
/// compartilhados continuam somando os registros do inventário (fatia 1).
fn fonte_do_grafico(tipo: Tipo) -> Fonte {
    if crate::composta::so_do_livro(tipo) {
        Fonte::LivroCaixa
    } else {
        Fonte::Inventario
    }
}

/// O texto do rastro para "Copiar rastro".
pub fn texto_do_rastro(alvo: &Ref, rotulo: &str, rastro: &Rastro) -> String {
    let elo = |e: &sbf_inventario::consulta::Elo| {
        let mut s = format!(
            "{} {} ({}) · {}",
            e.alvo.tipo.singular(),
            e.rotulo,
            e.alvo,
            e.papel
        );
        if let Some(q) = &e.quem {
            s.push_str(&format!(" · {}", q.rotulo()));
        }
        if let Some(q) = &e.quando {
            s.push_str(&format!(" · {}", formato::data(q)));
        }
        s
    };
    let mut linhas = vec![format!(
        "Rastro de {} {rotulo} ({alvo})",
        alvo.tipo.singular()
    )];
    for e in rastro.acima.iter().rev() {
        linhas.push(format!("↑ {}", elo(e)));
    }
    linhas.push(format!("● {} {rotulo}", alvo.tipo.singular()));
    for e in &rastro.abaixo {
        linhas.push(format!("↓ {}", elo(e)));
    }
    linhas.join("\n")
}

/// O que a página precisa da estação (as páginas de Estação e de Alteração e a aba
/// Alterações da movimentação).
#[derive(Clone, Copy)]
pub struct ContextoDaPagina<'a> {
    pub estacao: Option<&'a dyn crate::dinheiro::EstacaoDoLivro>,
    /// A caixa de entrada e a edição (fatia 3): Editar, Ratear, Excluir e a marca de pendente.
    pub caixa: Option<&'a dyn crate::caixa::FonteDaCaixa>,
    pub hoje: &'a str,
}

impl std::fmt::Debug for ContextoDaPagina<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ContextoDaPagina")
            .field("estacao", &self.estacao.is_some())
            .field("caixa", &self.caixa.is_some())
            .field("hoje", &self.hoje)
            .finish()
    }
}

/// Desenha a página de um item.
pub fn desenhar(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    previas: &mut crate::previa::Previas,
    alvo: &Ref,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
    contexto: ContextoDaPagina<'_>,
) {
    let p = tema::paleta(ui.ctx());
    if matches!(alvo.tipo, Tipo::Estacao | Tipo::Alteracao) {
        match contexto.estacao {
            Some(est) if alvo.tipo == Tipo::Estacao => {
                crate::dinheiro::pagina_da_estacao(ui, est, alvo, contexto.hoje, vista, pedidos)
            }
            Some(est) => {
                crate::dinheiro::pagina_da_alteracao(ui, est, alvo, contexto.hoje, vista, pedidos)
            }
            None => crate::dinheiro::motivo_ou_erro(
                ui,
                &format!("ainda não suportado: {}", crate::composta::SEM_LIVRO),
            ),
        }
        return;
    }
    if let Some(motivo) = motivo_do_tipo(alvo.tipo) {
        componentes::fatia_futura(
            ui,
            alvo.tipo.icone(),
            &format!("{} {}", alvo.tipo.singular(), alvo.chave),
            alvo.tipo.fatia(),
            &motivo,
        );
        ui.add_space(12.0);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("A página e o rastro deste item nascem com essa fatia.")
                    .color(p.tinta_suave),
            );
        });
        return;
    }
    let item = match leitor.item(alvo) {
        Ok(i) => i,
        Err(e) if e.starts_with("ainda não suportado: ") => {
            componentes::fatia_futura(
                ui,
                alvo.tipo.icone(),
                &format!("{} {}", alvo.tipo.singular(), alvo.chave),
                2,
                e.trim_start_matches("ainda não suportado: "),
            );
            return;
        }
        Err(e) => {
            componentes::aviso(
                ui,
                "error",
                &format!("Não deu para ler {alvo}: {e}"),
                p.perigo_sutil,
                p.borda,
                p.perigo,
            );
            return;
        }
    };
    let Some(item) = item.as_ref() else {
        componentes::aviso(
            ui,
            "help",
            &format!("{} não encontrado: {}", alvo.tipo.singular(), alvo.chave),
            p.aviso_sutil,
            p.aviso_borda,
            p.aviso,
        );
        return;
    };

    // Cabeçalho: ícone, nome, selo do tipo, botão de abrir ao lado.
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor(alvo.tipo.icone(), 22.0, p.tinta));
        componentes::titulo(ui, &item.rotulo, 20.0);
        componentes::selo(ui, alvo.tipo.singular(), p.superficie_3, p.tinta_suave);
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if componentes::botao_icone(ui, "picture_in_picture", "Abrir em painel flutuante", true)
                .clicked()
            {
                crate::navegacao::pedir_abrir(ui.ctx(), pedidos, Destino::Item(alvo.clone()), true);
            }
            if alvo.tipo == Tipo::Movimentacao
                && let Some(caixa) = contexto.caixa
            {
                crate::caixa_ui::acoes_da_movimentacao(ui, caixa, alvo, pedidos);
            }
        });
    });
    ui.add_space(6.0);

    // Estado em destaque, quando o item tem.
    let (estados, campos): (Vec<_>, Vec<_>) = item
        .campos
        .iter()
        .partition(|c| matches!(c.valor, Celula::Estado { .. }));
    for c in &estados {
        if let Celula::Estado { rotulo, quem } = &c.valor {
            ui.horizontal(|ui| componentes::estado(ui, rotulo, quem.as_ref()));
        }
    }

    // Os valores em cartões (o primeiro em destaque), com o gráfico por ciclo ao lado,
    // como no protótipo; o resto em grade.
    let (valores, campos): (Vec<_>, Vec<_>) = campos
        .into_iter()
        .partition(|c| matches!(c.valor, Celula::Valor { .. }));
    let totais = if tem_grafico_por_ciclo(alvo.tipo) {
        leitor
            .por_ciclo(alvo, fonte_do_grafico(alvo.tipo), 6)
            .ok()
            .filter(|t| !t.is_empty())
    } else {
        None
    };
    if !valores.is_empty() || totais.is_some() {
        ui.horizontal_wrapped(|ui| {
            for (i, c) in valores.iter().take(4).enumerate() {
                cartao_de_valor(ui, c, i == 0, pedidos);
            }
            if let Some(t) = &totais {
                grafico_por_ciclo(ui, t, pedidos);
            }
        });
        ui.add_space(8.0);
    }
    let campos: Vec<_> = valores.iter().skip(4).copied().chain(campos).collect();
    let largura = ui.available_width();
    let por_linha = ((largura / 190.0).floor() as usize).clamp(1, 5);
    egui::Grid::new(("campos", alvo))
        .num_columns(por_linha)
        .spacing(vec2(24.0, 8.0))
        .min_col_width((largura / por_linha as f32 - 24.0).max(80.0))
        .show(ui, |ui| {
            let largura_da_celula = (largura / por_linha as f32 - 24.0).max(80.0);
            for (i, c) in campos.iter().enumerate() {
                ui.vertical(|ui| {
                    ui.set_max_width(largura_da_celula);
                    componentes::rotulo_de_secao(ui, &c.rotulo);
                    ui.horizontal(|ui| {
                        componentes::celula(ui, &c.valor, pedidos);
                        if let Celula::Valor {
                            fonte: Fonte::Inventario,
                            ..
                        } = c.valor
                        {
                            ui.label(
                                RichText::new(format!("· {A_CONFERIR}"))
                                    .size(12.0)
                                    .color(p.tinta_suave),
                            );
                        }
                    });
                });
                if (i + 1) % por_linha == 0 {
                    ui.end_row();
                }
            }
        });

    // Abas.
    ui.add_space(10.0);
    let abas = abas_de(alvo.tipo);
    let escolhida = vista
        .abas
        .get(alvo)
        .copied()
        .unwrap_or_else(|| abas.iter().position(|a| a.motivo.is_none()).unwrap_or(0));
    let escolhida = escolhida.min(abas.len().saturating_sub(1));
    ui.horizontal(|ui| {
        for (i, a) in abas.iter().enumerate() {
            let ativa = i == escolhida;
            let cor = if a.motivo.is_some() {
                p.tinta_desabilitada
            } else if ativa {
                p.tinta
            } else {
                p.tinta_suave
            };
            let mut texto = RichText::new(&a.rotulo).color(cor);
            if ativa {
                texto = texto.strong();
            }
            let icone = match a.conteudo {
                ConteudoDaAba::Rastro => Some("account_tree"),
                ConteudoDaAba::Arquivo => Some("description"),
                _ => None,
            };
            let r = ui.add_enabled(
                a.motivo.is_none(),
                egui::Button::new(match icone {
                    Some(n) => (icones::icone_cor(n, 15.0, cor), texto).into_atoms(),
                    None => texto.into_atoms(),
                })
                .frame(false),
            );
            if ativa {
                ui.painter().hline(
                    r.rect.x_range(),
                    r.rect.bottom() + 2.0,
                    Stroke::new(2.0, p.destaque),
                );
            }
            let r = match &a.motivo {
                Some(m) => r.on_disabled_hover_text(m.as_str()),
                None => r,
            };
            if r.clicked() {
                vista.abas.insert(alvo.clone(), i);
            }
        }
    });
    let linha = ui.available_rect_before_wrap();
    ui.painter().hline(
        linha.x_range(),
        linha.top() + 2.0,
        Stroke::new(1.0, p.borda),
    );
    ui.add_space(8.0);

    let Some(a) = abas.get(escolhida) else { return };
    if let Some(m) = &a.motivo {
        componentes::aviso(ui, "info", m, p.info_sutil, p.info_borda, p.info);
        return;
    }
    match &a.conteudo {
        ConteudoDaAba::Rastro => match leitor.rastro(alvo) {
            Ok(r) => desenhar_rastro(ui, alvo, &item.rotulo, &r, pedidos),
            Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
        },
        ConteudoDaAba::Arquivo => {
            // PageUp/PageDown só no centro e sem campo de texto em foco.
            let teclado = !crate::navegacao::na_casca(ui.ctx())
                || bancada_casca::pedidos::local_atual(ui.ctx()) == bancada_casca::Local::Centro;
            let teclado = teclado && ui.ctx().memory(|m| m.focused().is_none());
            previas.aba_ui(ui, leitor, alvo, pedidos, teclado);
        }
        ConteudoDaAba::Texto => match leitor.texto(alvo) {
            Ok(t) => texto_extraido(ui, t.as_deref()),
            Err(e) => componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo),
        },
        ConteudoDaAba::Alteracoes => crate::dinheiro::aba_alteracoes_do_item(
            ui,
            contexto.estacao,
            alvo,
            contexto.hoje,
            vista,
            pedidos,
        ),
        ConteudoDaAba::Recorte(r) => {
            let pedido = PedidoDeTabela::ItensDoTotal {
                de: alvo.clone(),
                fonte: Fonte::LivroCaixa,
                ciclo: None,
                recorte: Some((*r).to_owned()),
            };
            let chave = format!("recorte:{alvo}:{r}");
            let altura = ui.available_height().max(200.0);
            if let Err(e) = tabela_em_pedacos(
                ui,
                leitor,
                &pedido,
                &chave,
                vista,
                pedidos,
                Some(altura),
                false,
            ) {
                crate::dinheiro::motivo_ou_erro(ui, &e);
            }
        }
        ConteudoDaAba::Ligados(tipo) => {
            let pedido = PedidoDeTabela::Ligados {
                de: alvo.clone(),
                tipo: *tipo,
            };
            let chave = format!("ligados:{alvo}:{tipo}");
            let altura = ui.available_height().max(200.0);
            if let Err(e) = tabela_em_pedacos(
                ui,
                leitor,
                &pedido,
                &chave,
                vista,
                pedidos,
                Some(altura),
                false,
            ) {
                crate::dinheiro::motivo_ou_erro(ui, &e);
            }
        }
    }
}

/// Um cartão com um valor do cabeçalho; o primeiro da página vem em destaque (hero).
fn cartao_de_valor(
    ui: &mut Ui,
    c: &sbf_inventario::consulta::Campo,
    destaque: bool,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    let Celula::Valor {
        centavos,
        fonte,
        itens,
    } = &c.valor
    else {
        return;
    };
    let (fundo, borda, cor, sub) = if destaque {
        (p.hero_fundo, p.hero_fundo, p.hero_texto, p.hero_sub)
    } else {
        (p.superficie_1, p.borda, p.tinta, p.tinta_suave)
    };
    Frame::new()
        .fill(fundo)
        .stroke(Stroke::new(1.0, borda))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_min_size(vec2(170.0, 74.0));
            ui.vertical(|ui| {
                ui.label(
                    RichText::new(c.rotulo.to_uppercase())
                        .font(FontId::new(11.5, familia_forte()))
                        .color(sub),
                );
                let texto = formato::dinheiro(*centavos);
                componentes::total_rico(
                    ui,
                    RichText::new(&texto)
                        .font(FontId::new(21.0, familia_forte()))
                        .color(cor),
                    &texto,
                    itens.as_deref(),
                    &format!("{} · {}", c.rotulo, texto),
                    pedidos,
                    sub,
                );
                if *fonte == Fonte::Inventario {
                    ui.label(RichText::new(A_CONFERIR).size(12.0).color(sub));
                }
            });
        });
}

/// Uma tabela lida em pedaços (`Consulta::tabela_paginada`): a rolagem tem o tamanho do
/// total, e o pedaço seguinte é pedido quando a rolagem chega perto do fim do que já foi
/// lido. Ordenar uma tabela ainda não toda lida carrega tudo antes, com um aviso.
/// Devolve o total de linhas (para o "N linhas" do cabeçalho), ou o erro.
#[allow(clippy::too_many_arguments)]
pub fn tabela_em_pedacos(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    pedido: &PedidoDeTabela,
    chave: &str,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
    altura: Option<f32>,
    teclado: bool,
) -> Result<usize, String> {
    let p = tema::paleta(ui.ctx());
    let mut estado = vista.tabelas.remove(chave).unwrap_or_default();
    estado.teclado = teclado;
    let mut minimo = 0;
    if estado.ordem_pendente.is_some() {
        if estado.ordenar_agora {
            minimo = usize::MAX;
        } else {
            let total = leitor.carregada(pedido, 0).map(|c| c.total).unwrap_or(0);
            ui.label(
                RichText::new(format!(
                    "ordenando {} linhas…",
                    formato::inteiro(total as i64)
                ))
                .size(12.0)
                .color(p.tinta_suave),
            );
            estado.ordenar_agora = true;
            ui.ctx().request_repaint();
        }
    }
    let resultado = leitor.carregada(pedido, minimo).map(|c| {
        if minimo == usize::MAX {
            if let Some(o) = estado.ordem_pendente.take() {
                estado.ordem = o;
            }
            estado.ordenar_agora = false;
        }
        let falta = tabela::desenhar_parcial(
            ui,
            chave,
            &c.tabela,
            c.total,
            &mut estado,
            &mut vista.selecao,
            pedidos,
            altura,
        );
        (c.total, falta)
    });
    vista.tabelas.insert(chave.to_owned(), estado);
    let (total, falta) = resultado?;
    if let Some(n) = falta {
        // Perto do fim do que foi lido: o pedaço seguinte (mais um de folga).
        leitor.carregada(pedido, n + 1 + crate::leitura::PEDACO)?;
        ui.ctx().request_repaint();
    }
    Ok(total)
}

/// O texto extraído, em fonte mono, rolável.
pub fn texto_extraido(ui: &mut Ui, texto: Option<&str>) {
    let p = tema::paleta(ui.ctx());
    match texto {
        Some(t) => {
            egui::ScrollArea::vertical()
                .id_salt("texto-extraido")
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    Frame::new()
                        .fill(p.superficie_2)
                        .stroke(Stroke::new(1.0, p.borda))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::same(10))
                        .show(ui, |ui| {
                            ui.add(
                                egui::Label::new(RichText::new(t).monospace().size(12.5)).wrap(),
                            );
                        });
                });
        }
        None => {
            ui.label(RichText::new("Sem texto extraído para este achado.").color(p.tinta_suave));
        }
    }
}

/// Altura útil das barras do gráfico por ciclo.
pub const ALTURA_DO_GRAFICO: f32 = 64.0;

/// Altura de uma barra: proporcional ao valor absoluto (o maior ocupa `altura`), com um
/// mínimo visível de 3 px para valor diferente de zero.
pub fn altura_da_barra(centavos: i64, maximo: f32, altura: f32) -> f32 {
    if centavos == 0 {
        return 0.0;
    }
    (centavos.unsigned_abs() as f32 / maximo.max(1.0) * altura).max(3.0)
}

/// As barras dos últimos ciclos, cada uma clicável (abre o que soma o ciclo).
pub fn grafico_por_ciclo(ui: &mut Ui, totais: &[TotalDoCiclo], pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    componentes::cartao(ui, |ui| {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                componentes::rotulo_de_secao(ui, "Total por ciclo");
                ui.label(
                    RichText::new(format!(
                        "últimos {} ciclos · inventário · {A_CONFERIR}",
                        totais.len()
                    ))
                    .size(12.0)
                    .color(p.tinta_suave),
                );
            });
            ui.add_space(4.0);
            let maximo = totais
                .iter()
                .map(|t| t.centavos.unsigned_abs())
                .max()
                .unwrap_or(1)
                .max(1) as f32;
            let largura_barra = 54.0;
            ui.horizontal(|ui| {
                for (i, t) in totais.iter().enumerate() {
                    let ultimo = i + 1 == totais.len();
                    ui.vertical(|ui| {
                        ui.set_width(largura_barra);
                        ui.add(
                            egui::Label::new(
                                RichText::new(formato::dinheiro(t.centavos).replace("R$ ", ""))
                                    .size(11.0)
                                    .color(p.tinta_suave),
                            )
                            .extend(),
                        );
                        let (rect, r) = ui.allocate_exact_size(
                            vec2(largura_barra - 12.0, ALTURA_DO_GRAFICO),
                            Sense::click(),
                        );
                        let h = altura_da_barra(t.centavos, maximo, ALTURA_DO_GRAFICO);
                        let barra = egui::Rect::from_min_max(
                            egui::pos2(rect.left(), rect.bottom() - h),
                            rect.max,
                        );
                        let cor = if ultimo || r.hovered() {
                            p.destaque
                        } else {
                            p.destaque_borda
                        };
                        ui.painter().rect_filled(barra, CornerRadius::same(2), cor);
                        let mut texto = RichText::new(&t.ciclo).size(11.0).color(p.tinta_suave);
                        if ultimo {
                            texto = texto.strong().color(p.tinta);
                        }
                        ui.label(texto);
                        if let Some(itens) = &t.itens {
                            let r = r
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .on_hover_text(format!(
                                    "Ciclo {}: {} · {A_CONFERIR}\nClique abre os registros",
                                    t.ciclo,
                                    formato::dinheiro(t.centavos)
                                ));
                            if r.clicked() {
                                let flutuante = componentes::ctrl(ui);
                                crate::navegacao::pedir_abrir(
                                    ui.ctx(),
                                    pedidos,
                                    Destino::Total {
                                        titulo: format!(
                                            "Ciclo {} · {}",
                                            t.ciclo,
                                            formato::dinheiro(t.centavos)
                                        ),
                                        pedido: (**itens).clone(),
                                    },
                                    flutuante,
                                );
                            }
                        }
                    });
                }
            });
        });
    });
}

fn desenhar_elo(ui: &mut Ui, e: &sbf_inventario::consulta::Elo, pedidos: &mut Vec<Pedido>) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor(e.alvo.tipo.icone(), 18.0, p.tinta_suave));
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(e.alvo.tipo.singular())
                        .size(12.0)
                        .color(p.tinta_suave),
                );
                componentes::link_item(ui, &e.alvo, &e.rotulo, pedidos);
            });
            let mut meta = e.papel.clone();
            if let Some(q) = &e.quem {
                meta.push_str(&format!(" · {}", q.rotulo()));
            }
            if let Some(q) = &e.quando {
                meta.push_str(&format!(" · {}", formato::data(q)));
            }
            ui.label(RichText::new(meta).size(12.0).color(p.tinta_suave));
        });
    });
}

/// A aba Rastro: a cadeia vertical, de onde veio até no que virou.
pub fn desenhar_rastro(
    ui: &mut Ui,
    alvo: &Ref,
    rotulo: &str,
    rastro: &Rastro,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(
            RichText::new("Cada elo é um link e diz quem decidiu e quando.").color(p.tinta_suave),
        );
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            if ui
                .button((
                    icones::icone_cor("content_copy", 16.0, p.tinta),
                    "Copiar rastro",
                ))
                .clicked()
            {
                pedidos.push(Pedido::Copiar(texto_do_rastro(alvo, rotulo, rastro)));
            }
        });
    });
    ui.add_space(6.0);
    egui::ScrollArea::vertical()
        .id_salt(("rastro", alvo))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            let trilho = |ui: &mut Ui| {
                let (rect, _) = ui.allocate_exact_size(vec2(18.0, 14.0), Sense::hover());
                ui.painter()
                    .vline(rect.center().x, rect.y_range(), Stroke::new(2.0, p.borda));
            };
            componentes::rotulo_de_secao(ui, "De onde veio");
            if rastro.acima.is_empty() {
                let frase = sbf_inventario::rastro::motivo_sem_rastro(alvo.tipo)
                    .unwrap_or("Nada acima: este item é a origem.");
                ui.label(RichText::new(frase).color(p.tinta_suave));
            }
            for e in rastro.acima.iter().rev() {
                desenhar_elo(ui, e, pedidos);
                trilho(ui);
            }
            Frame::new()
                .fill(p.destaque_sutil)
                .stroke(Stroke::new(1.0, p.destaque_borda))
                .corner_radius(CornerRadius::same(4))
                .inner_margin(Margin::symmetric(8, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(icones::icone_cor(alvo.tipo.icone(), 18.0, p.destaque_tinta));
                        ui.label(
                            RichText::new(alvo.tipo.singular())
                                .size(12.0)
                                .color(p.tinta_suave),
                        );
                        ui.label(
                            RichText::new(rotulo)
                                .font(FontId::new(14.0, familia_forte()))
                                .color(p.tinta),
                        );
                        ui.label(RichText::new("este item").size(12.0).color(p.tinta_suave));
                    });
                });
            ui.add_space(4.0);
            componentes::rotulo_de_secao(ui, "No que virou");
            if rastro.abaixo.is_empty() {
                ui.label(
                    RichText::new("Ainda não virou nada: nenhum registro, lote ou movimentação.")
                        .color(p.tinta_suave),
                );
            }
            for (i, e) in rastro.abaixo.iter().enumerate() {
                if i > 0 {
                    trilho(ui);
                }
                desenhar_elo(ui, e, pedidos);
            }
            if rastro
                .abaixo
                .iter()
                .all(|e| e.alvo.tipo != Tipo::Movimentacao)
            {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    componentes::selo_fatia(ui, 2);
                    ui.label(
                        RichText::new(
                            "Movimentação, rateio e centros entram no rastro com o livro-caixa.",
                        )
                        .size(12.0)
                        .color(p.tinta_suave),
                    );
                });
            }
        });
}

/// A página de um total: a lista do que soma (ou conta) o número.
pub fn desenhar_total(
    ui: &mut Ui,
    leitor: &mut Leitor<'_>,
    titulo: &str,
    pedido: &PedidoDeTabela,
    vista: &mut Vista,
    pedidos: &mut Vec<Pedido>,
) {
    let p = tema::paleta(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(icones::icone_cor("functions", 22.0, p.tinta));
        componentes::titulo(ui, titulo, 20.0);
    });
    if let PedidoDeTabela::ItensDoTotal {
        de, fonte, ciclo, ..
    } = pedido
    {
        ui.horizontal(|ui| {
            ui.label(RichText::new("Itens que compõem o total de").color(p.tinta_suave));
            let rotulo = leitor.rotulo(de);
            componentes::link_item(ui, de, &rotulo, pedidos);
            if let Some(c) = ciclo {
                ui.label(RichText::new(format!("no ciclo {c}")).color(p.tinta_suave));
            }
            if *fonte == Fonte::Inventario {
                ui.label(
                    RichText::new(format!("· registros do inventário · {A_CONFERIR}"))
                        .color(p.tinta_suave),
                );
            }
        });
    }
    ui.add_space(8.0);
    let chave = format!(
        "total:{}",
        serde_json::to_string(pedido).unwrap_or_default()
    );
    if let Err(e) = tabela_em_pedacos(ui, leitor, pedido, &chave, vista, pedidos, None, false) {
        componentes::aviso(ui, "error", &e, p.perigo_sutil, p.borda, p.perigo);
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    #[test]
    fn toda_pagina_termina_no_rastro() {
        for t in Tipo::TODOS {
            let abas = abas_de(t);
            assert_eq!(
                abas.last().map(|a| &a.conteudo),
                Some(&ConteudoDaAba::Rastro)
            );
        }
    }

    #[test]
    fn fornecedor_tem_as_abas_da_spec_com_o_livro_caixa_aceso() {
        let abas = abas_de(Tipo::Fornecedor);
        for rotulo in ["Movimentações", "Compromissos e recorrentes", "Documentos"] {
            let a = abas.iter().find(|a| a.rotulo == rotulo).expect("aba");
            assert!(a.motivo.is_none(), "{rotulo}");
        }
        let reg = abas
            .iter()
            .find(|a| a.rotulo.starts_with("Registros"))
            .expect("aba");
        assert_eq!(reg.rotulo, "Registros · a conferir");
        // A movimentação diz por que ainda não liga ao registro (promoção, fatia 3).
        let mov = abas_de(Tipo::Movimentacao);
        assert!(
            mov.iter()
                .any(|a| a.rotulo.starts_with("Registro de origem") && a.motivo.is_some())
        );
        assert!(mov.iter().any(|a| a.conteudo == ConteudoDaAba::Alteracoes));
    }

    #[test]
    fn barras_proporcionais_ao_valor_absoluto() {
        let maximo = 31244.0;
        let a = altura_da_barra(-22870, maximo, 64.0);
        let b = altura_da_barra(-28710, maximo, 64.0);
        let c = altura_da_barra(-31244, maximo, 64.0);
        assert!((c - 64.0).abs() < 0.01);
        assert!((a / c - 22870.0 / 31244.0).abs() < 0.01);
        assert!(
            b - a > 10.0,
            "a diferença de R$ 58 tem que aparecer: {a} {b}"
        );
        assert_eq!(altura_da_barra(-1, maximo, 64.0), 3.0);
        assert_eq!(altura_da_barra(0, maximo, 64.0), 0.0);
    }

    #[test]
    fn rastro_vira_texto_com_cada_elo() {
        use sbf_inventario::consulta::{Elo, Quem};
        let alvo = Ref::nova(Tipo::Registro, "7");
        let r = Rastro {
            acima: vec![Elo {
                alvo: Ref::nova(Tipo::Documento, "abc"),
                rotulo: "conta.pdf".into(),
                papel: "extraído de".into(),
                quem: Some(Quem::Automatico("regras@2".into())),
                quando: Some("2026-10-02T21:41:00".into()),
            }],
            abaixo: vec![],
        };
        let t = texto_do_rastro(&alvo, "Conta de energia", &r);
        assert!(t.contains(
            "↑ Documento conta.pdf (documento:abc) · extraído de · regras@2 · 02/10/2026 21:41"
        ));
        assert!(t.contains("● Registro Conta de energia"));
    }
}
