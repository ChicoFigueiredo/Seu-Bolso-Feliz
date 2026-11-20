//! A marcação das listas de documentos (spec 15, D3): fundo **verde claro** na linha de um
//! documento que algum padrão de reconhecimento já casou, **âmbar** quando a última decisão
//! foi Ambíguo, **sem cor** quando nenhum casou; um selo "N padrões" e o filtro
//! Verde / Âmbar / Sem cor.
//!
//! Vale para as três listas: a Caixa de entrada, a seção "Sem padrão de reconhecimento" de
//! A triar e a tabela de achados e documentos das áreas. Os dados vêm da consulta em lote do
//! catálogo (`sbf_regras::catalogo::Catalogo::marcas`, via [`FonteDasRegras::marcas`]).
//!
//! **Uma consulta por lote, nunca por linha por quadro.** Cada linha desenhada só pede a
//! marca a um cache; o que falta é anotado e resolvido numa consulta só depois do desenho
//! (`resolver`). O que já veio vale por [`VALIDADE`] e depois é pedido de novo (o worker grava
//! a passada de fundo de outro processo), mostrando o valor antigo enquanto isso. Só o filtro
//! carrega a lista inteira, e também em lote.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Context, Id, RichText, Ui};
pub use sbf_regras::marcacao::{CorDaMarca, Marca};

use crate::componentes;
use crate::regras::FonteDasRegras;
use crate::tema;

/// Quanto tempo uma marca lida vale antes de ser pedida de novo.
pub const VALIDADE: Duration = Duration::from_secs(10);

/// Quais linhas uma lista mostra, pela cor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum FiltroDeCor {
    #[default]
    Todas,
    Verde,
    Ambar,
    SemCor,
}

impl FiltroDeCor {
    pub const TODOS: [FiltroDeCor; 4] = [
        FiltroDeCor::Todas,
        FiltroDeCor::Verde,
        FiltroDeCor::Ambar,
        FiltroDeCor::SemCor,
    ];

    #[must_use]
    pub fn rotulo(self) -> &'static str {
        match self {
            FiltroDeCor::Todas => "Todas",
            FiltroDeCor::Verde => "Verde",
            FiltroDeCor::Ambar => "Âmbar",
            FiltroDeCor::SemCor => "Sem cor",
        }
    }

    /// A linha de cor `cor` passa neste filtro.
    #[must_use]
    pub fn aceita(self, cor: CorDaMarca) -> bool {
        match self {
            FiltroDeCor::Todas => true,
            FiltroDeCor::Verde => cor == CorDaMarca::Verde,
            FiltroDeCor::Ambar => cor == CorDaMarca::Ambar,
            FiltroDeCor::SemCor => cor == CorDaMarca::SemCor,
        }
    }
}

/// O cache das marcas e os filtros das listas. Compartilhado entre a janela e as tabelas
/// (que não conhecem a janela) pelo contexto do egui.
#[derive(Debug, Default)]
pub struct Marcacao {
    fonte: Option<FonteDasRegras>,
    cache: HashMap<String, (Marca, Instant)>,
    faltam: HashSet<String>,
    filtros: HashMap<String, FiltroDeCor>,
    /// O último erro de leitura (sem conteúdo de documento).
    pub erro: Option<String>,
    /// Quantas consultas em lote já foram feitas (os testes conferem que não é uma por linha).
    pub consultas: usize,
}

/// A marcação como a janela e as tabelas a compartilham.
pub type Compartilhada = Arc<Mutex<Marcacao>>;

fn id_no_contexto() -> Id {
    Id::new("sbf-janela/marcacao")
}

/// Põe a marcação no contexto, para as tabelas a acharem.
pub fn publicar(ctx: &Context, m: &Compartilhada) {
    ctx.data_mut(|d| d.insert_temp(id_no_contexto(), Arc::clone(m)));
}

/// A marcação publicada, se há e está ligada.
pub fn do_contexto(ctx: &Context) -> Option<Compartilhada> {
    let m: Compartilhada = ctx.data(|d| d.get_temp(id_no_contexto()))?;
    let ligada = m.lock().is_ok_and(|g| g.ligada());
    ligada.then_some(m)
}

impl Marcacao {
    #[must_use]
    pub fn ligada(&self) -> bool {
        self.fonte.is_some()
    }

    /// Liga (ou troca) o catálogo de onde as marcas vêm. Trocar de catálogo esquece o cache.
    pub fn definir_fonte(&mut self, fonte: Option<&FonteDasRegras>) {
        let igual = match (&self.fonte, fonte) {
            (Some(a), Some(b)) => Arc::ptr_eq(&a.catalogo, &b.catalogo),
            (None, None) => true,
            _ => false,
        };
        if !igual {
            self.fonte = fonte.cloned();
            self.esquecer();
        }
    }

    /// Esquece as marcas lidas (a próxima leitura vai ao catálogo).
    pub fn esquecer(&mut self) {
        self.cache.clear();
        self.faltam.clear();
    }

    /// A marca de um documento, do cache. O que falta (ou venceu) é anotado para a próxima
    /// consulta em lote; enquanto não chega, a linha sai sem cor.
    pub fn marca(&mut self, sha256: &str) -> Marca {
        match self.cache.get(sha256) {
            Some((m, quando)) => {
                if quando.elapsed() > VALIDADE {
                    self.faltam.insert(sha256.to_owned());
                }
                *m
            }
            None => {
                self.faltam.insert(sha256.to_owned());
                Marca::default()
            }
        }
    }

    /// Há marcas anotadas para ler.
    #[must_use]
    pub fn tem_o_que_ler(&self) -> bool {
        !self.faltam.is_empty()
    }

    /// Lê, numa consulta em lote, as marcas que as linhas desenhadas pediram. Devolve `true`
    /// se leu algo (quem chama pede outro quadro para pintar).
    pub fn resolver(&mut self) -> bool {
        if self.faltam.is_empty() {
            return false;
        }
        let pedidas: Vec<String> = self.faltam.drain().collect();
        self.ler(&pedidas);
        true
    }

    fn ler(&mut self, ids: &[String]) {
        let Some(fonte) = &self.fonte else {
            return;
        };
        self.consultas += 1;
        match fonte.marcas(ids) {
            Ok(mapa) => {
                let agora = Instant::now();
                for id in ids {
                    let m = mapa.get(id).copied().unwrap_or_default();
                    self.cache.insert(id.clone(), (m, agora));
                }
                self.erro = None;
            }
            Err(e) => self.erro = Some(e),
        }
    }

    /// Garante, já, as marcas de toda uma lista (o filtro precisa de todas): lê em lote só o
    /// que falta ou venceu.
    pub fn garantir(&mut self, ids: &[&str]) {
        let a_ler: Vec<String> = ids
            .iter()
            .filter(|id| {
                self.cache
                    .get(**id)
                    .is_none_or(|(_, quando)| quando.elapsed() > VALIDADE)
            })
            .map(|id| (*id).to_owned())
            .collect();
        if !a_ler.is_empty() {
            self.ler(&a_ler);
        }
    }

    /// A cor de um documento (do cache; use depois de [`Marcacao::garantir`]).
    #[must_use]
    pub fn cor(&self, sha256: &str) -> CorDaMarca {
        self.cache
            .get(sha256)
            .map_or(CorDaMarca::SemCor, |(m, _)| m.cor())
    }

    /// O filtro da lista `lista`.
    #[must_use]
    pub fn filtro(&self, lista: &str) -> FiltroDeCor {
        self.filtros.get(lista).copied().unwrap_or_default()
    }

    pub fn definir_filtro(&mut self, lista: &str, filtro: FiltroDeCor) {
        if filtro == FiltroDeCor::Todas {
            self.filtros.remove(lista);
        } else {
            self.filtros.insert(lista.to_owned(), filtro);
        }
    }

    /// Quais dos documentos `ids` passam no filtro da lista (na mesma ordem). Sem filtro,
    /// todos, sem consultar nada.
    pub fn passam(&mut self, lista: &str, ids: &[&str]) -> Vec<bool> {
        let filtro = self.filtro(lista);
        if filtro == FiltroDeCor::Todas {
            return vec![true; ids.len()];
        }
        self.garantir(ids);
        ids.iter().map(|id| filtro.aceita(self.cor(id))).collect()
    }
}

/// Cabeçalho de uma lista: o texto "Marcação" e as quatro escolhas. Devolve `true` quando o
/// filtro mudou.
pub fn faixa_do_filtro(ui: &mut Ui, m: &mut Marcacao, lista: &str) -> bool {
    let p = tema::paleta(ui.ctx());
    let mut mudou = false;
    let atual = m.filtro(lista);
    ui.horizontal(|ui| {
        ui.label(RichText::new("Marcação:").size(12.0).color(p.tinta_suave))
            .on_hover_text(
                "Verde: algum padrão de reconhecimento já casou. Âmbar: a última decisão foi \
                 ambígua. Sem cor: nenhum padrão casou.",
            );
        for f in FiltroDeCor::TODOS {
            if ui.selectable_label(atual == f, f.rotulo()).clicked() && atual != f {
                m.definir_filtro(lista, f);
                mudou = true;
            }
        }
    });
    mudou
}

/// O fundo de uma linha pela cor; `None` sem cor.
#[must_use]
pub fn fundo_da_linha(ctx: &Context, cor: CorDaMarca) -> Option<egui::Color32> {
    let p = tema::paleta(ctx);
    match cor {
        CorDaMarca::Verde => Some(p.marca_verde),
        CorDaMarca::Ambar => Some(p.marca_ambar),
        CorDaMarca::SemCor => None,
    }
}

/// Pinta o fundo da célula (e das margens entre células, para a linha ficar inteira). Numa
/// linha selecionada só a faixa da esquerda, para não cobrir a cor da seleção. Chame antes de
/// pôr o conteúdo da célula.
pub fn pintar_celula(ui: &Ui, cor: CorDaMarca, selecionada: bool, primeira: bool) {
    let Some(fundo) = fundo_da_linha(ui.ctx(), cor) else {
        return;
    };
    let p = tema::paleta(ui.ctx());
    let folga = ui.spacing().item_spacing / 2.0;
    let rect = ui.max_rect().expand2(folga);
    if !selecionada {
        ui.painter().rect_filled(rect, 0.0, fundo);
    }
    if primeira {
        let faixa = egui::Rect::from_min_max(rect.min, egui::pos2(rect.min.x + 3.0, rect.max.y));
        let tom = if cor == CorDaMarca::Ambar {
            p.aviso
        } else {
            p.sucesso
        };
        ui.painter().rect_filled(faixa, 0.0, tom);
    }
}

/// "1 padrão" / "N padrões".
#[must_use]
pub fn texto_do_selo(m: Marca) -> String {
    if m.padroes == 1 {
        "1 padrão".to_owned()
    } else {
        format!("{} padrões", m.padroes)
    }
}

/// O selo "N padrões" de uma linha marcada, com a dica do que a cor quer dizer. Sem padrão
/// que casou, não desenha nada.
pub fn selo_de_padroes(ui: &mut Ui, m: Marca) {
    if m.padroes == 0 && !m.ambiguo {
        return;
    }
    let p = tema::paleta(ui.ctx());
    let (fundo, tinta, dica) = if m.ambiguo {
        (
            p.superficie_1,
            p.aviso,
            "Ambíguo: a última decisão dos padrões foi um empate que discorda. Peça revisão.",
        )
    } else {
        (
            p.superficie_1,
            p.sucesso,
            "Casou: este documento já foi reconhecido por esse número de padrões, em algum \
             momento (a marca fica mesmo depois de desativar o padrão).",
        )
    };
    let texto = if m.padroes == 0 {
        "ambíguo".to_owned()
    } else {
        texto_do_selo(m)
    };
    componentes::selo(ui, &texto, fundo, tinta).on_hover_text(dica);
}
