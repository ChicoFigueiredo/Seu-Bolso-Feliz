//! A janela do SBF sobre a `bancada-casca` (ADR-018): o [`AppSbf`] implementa `AppCasca`.
//!
//! - **Ações:** a lista única de [`crate::acoes`] vira a lista de Ações da casca (id =
//!   `codigo`), com os grupos do Ribbon; Voltar e Avançar são os da casca.
//! - **Rotas:** cada [`Destino`] vira uma rota de texto ([`Destino::rota`]); a casca guarda o
//!   histórico do centro e de cada painel flutuante.
//! - **Desenho:** as páginas, as áreas e os painéis do dock são os mesmos da [`Janela`]; as
//!   escritas continuam fora do desenho, em [`Janela::processar`].

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap};

use bancada_casca::{
    Aba as AbaCasca, Acao as AcaoCasca, AppCasca, Atalho as AtalhoCasca, ConfigCasca,
    Destino as DestinoPaleta, Elo, FamiliaDeIcones, Grupo, GrupoNavegacao, ItemNavegacao, Local,
    PainelDock, Preferencias, Rota, SecaoBastidores, SegmentoStatus, Textos, Tom, ids,
};
use egui::{RichText, Ui};
use sbf_inventario::consulta::{Area, Filtro, PedidoDeTabela};
use sbf_inventario::{Comandos, Consulta, Tipo};

use crate::acoes::{self, ACOES, Aba, IdAcao};
use crate::areas;
use crate::componentes;
use crate::dock::PainelDoDock;
use crate::formato;
use crate::janela::{Janela, Mensagem};
use crate::moldura::EstadoDaBarra;
use crate::navegacao::{self, Destino};
use crate::pagina::Vista;
use crate::tema;

const TOKENS: &str = include_str!("../../../docs/design/desktop/tokens.json");
const TEMAS: &str = include_str!("../../../docs/design/desktop/temas.json");
const SIMBOLOS: &[u8] = include_bytes!("../assets/fontes/MaterialSymbolsRounded-subconjunto.ttf");

/// Os nomes dos ícones que a casca usa, ligados aos do Material Symbols Rounded.
const ICONES_DA_CASCA: &[(&str, &str)] = &[
    ("voltar", "arrow_back"),
    ("avancar", "arrow_forward"),
    ("paleta", "search"),
    ("atalhos", "keyboard"),
    ("recolher", "expand_less"),
    ("expandir", "expand_more"),
    ("ribbon-classico", "view_agenda"),
    ("ribbon-simples", "view_stream"),
    ("densidade", "density_medium"),
    ("fechar", "close"),
    ("tela-cheia", "fullscreen"),
    ("foco", "center_focus_strong"),
];

/// A configuração da casca para o SBF: tokens e temas do SBF, sálvia, os ícones Material
/// (todos os do subconjunto, pelo nome, mais os nomes da casca) e o título.
pub fn config_da_casca() -> ConfigCasca {
    let mut config = ConfigCasca::nova("Seu Bolso Feliz", TOKENS, TEMAS);
    config.tema_inicial = tema::TEMA_PADRAO.to_owned();
    let mut glifos: Vec<(String, String)> = crate::icones::todos()
        .map(|(n, c)| (n.to_owned(), c.to_string()))
        .collect();
    for (casca, material) in ICONES_DA_CASCA {
        if let Some(c) = crate::icones::caractere(material) {
            glifos.push(((*casca).to_owned(), c.to_string()));
        }
    }
    config.icones = FamiliaDeIcones::com_fonte(SIMBOLOS.to_vec())
        .glifos(glifos.iter().map(|(a, b)| (a.as_str(), b.as_str())));
    config
}

/// O app do SBF dentro da casca.
pub struct AppSbf {
    janela: RefCell<Janela>,
    consulta: Box<dyn Consulta>,
    comandos: Box<dyn Comandos>,
    pub barra: EstadoDaBarra,
    vistas_flutuantes: HashMap<u64, Vista>,
    paineis_visiveis: BTreeMap<String, bool>,
    tema_atual: String,
    textos: Textos,
}

impl std::fmt::Debug for AppSbf {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AppSbf")
            .field("barra", &self.barra)
            .finish_non_exhaustive()
    }
}

/// Ações geradas dos dados (fora da lista estática): o filtro por conexão e a escolha do tema.
const PREFIXO_FONTE: &str = "inicio.fonte.";
const PREFIXO_FONTE_TODAS: &str = "inicio.fonte-todas";
const PREFIXO_TEMA: &str = "exibir.tema.";

/// O id do painel do dock na casca.
pub fn id_do_painel(p: PainelDoDock) -> &'static str {
    match p {
        PainelDoDock::Detalhe => "detalhe",
        PainelDoDock::Origens => "origens",
        PainelDoDock::Texto => "texto",
        PainelDoDock::Historico => "historico",
        PainelDoDock::Previa => "previa",
    }
}

fn painel_do_id(id: &str) -> Option<PainelDoDock> {
    PainelDoDock::TODOS
        .into_iter()
        .find(|p| id_do_painel(*p) == id)
}

/// O id da Ação da lista única pelo código (`triagem.documento`).
pub fn acao_do_codigo(codigo: &str) -> Option<IdAcao> {
    ACOES.iter().find(|d| d.codigo == codigo).map(|d| d.id)
}

fn atalho_da_casca(a: acoes::Atalho) -> AtalhoCasca {
    AtalhoCasca {
        ctrl: a.ctrl,
        alt: a.alt,
        shift: a.shift,
        tecla: a.tecla,
    }
}

/// As Ações que a casca já tem (Voltar e Avançar) ou que viram Ações da casca (painéis).
fn fica_de_fora(id: IdAcao) -> bool {
    matches!(
        id,
        IdAcao::Voltar
            | IdAcao::Avancar
            | IdAcao::PainelDetalhe
            | IdAcao::PainelOrigens
            | IdAcao::PainelTexto
            | IdAcao::PainelHistorico
    )
}

impl AppSbf {
    pub fn novo(
        janela: Janela,
        consulta: Box<dyn Consulta>,
        comandos: Box<dyn Comandos>,
        barra: EstadoDaBarra,
    ) -> Self {
        let mut janela = janela;
        janela.na_casca = true;
        let paineis_visiveis = PainelDoDock::TODOS
            .into_iter()
            .map(|p| {
                (
                    id_do_painel(p).to_owned(),
                    matches!(
                        p,
                        PainelDoDock::Detalhe | PainelDoDock::Previa | PainelDoDock::Historico
                    ),
                )
            })
            .collect();
        Self {
            janela: RefCell::new(janela),
            consulta,
            comandos,
            barra,
            vistas_flutuantes: HashMap::new(),
            paineis_visiveis,
            tema_atual: tema::TEMA_PADRAO.to_owned(),
            textos: Textos::pt_br(),
        }
    }

    /// A janela (estado da interface), para testes e para o integrador.
    pub fn janela(&self) -> std::cell::RefMut<'_, Janela> {
        self.janela.borrow_mut()
    }

    pub fn consulta(&self) -> &dyn Consulta {
        self.consulta.as_ref()
    }

    fn processar(&mut self, ctx: &egui::Context) {
        let AppSbf {
            janela,
            consulta,
            comandos,
            ..
        } = self;
        janela
            .get_mut()
            .processar(ctx, consulta.as_ref(), comandos.as_mut());
    }

    fn preparar(&self, ctx: &egui::Context) {
        navegacao::marcar_na_casca(ctx);
        tema::sincronizar_com_a_casca(ctx);
    }

    fn keytips(lista: &[&acoes::DefinicaoDeAcao]) -> HashMap<IdAcao, String> {
        let mut usados: Vec<String> = Vec::new();
        let mut v = HashMap::new();
        for d in lista {
            let letras: Vec<char> = d
                .rotulo
                .chars()
                .filter(|c| c.is_alphabetic())
                .map(|c| tirar_acento(c).to_ascii_uppercase())
                .collect();
            let mut escolhido = None;
            for c in &letras {
                let k = c.to_string();
                if !usados.contains(&k) {
                    escolhido = Some(k);
                    break;
                }
            }
            // Sem letra livre no rótulo: qualquer letra livre (KeyTips de uma letra só, sem
            // prefixo ambíguo).
            if escolhido.is_none() {
                escolhido = ('A'..='Z')
                    .map(|c| c.to_string())
                    .find(|k| !usados.contains(k));
            }
            if let Some(k) = escolhido {
                usados.push(k.clone());
                v.insert(d.id, k);
            }
        }
        v
    }
}

fn tirar_acento(c: char) -> char {
    match c {
        'á' | 'à' | 'ã' | 'â' | 'Á' | 'À' | 'Ã' | 'Â' => 'A',
        'é' | 'ê' | 'É' | 'Ê' => 'E',
        'í' | 'Í' => 'I',
        'ó' | 'ô' | 'õ' | 'Ó' | 'Ô' | 'Õ' => 'O',
        'ú' | 'Ú' => 'U',
        'ç' | 'Ç' => 'C',
        outra => outra,
    }
}

/// As Ações que só aparecem dentro de um menu (Período, Filtros), não como botão.
fn filho_de_menu(id: IdAcao) -> bool {
    matches!(
        id,
        IdAcao::PeriodoAtual
            | IdAcao::PeriodoAnterior
            | IdAcao::PeriodoTodos
            | IdAcao::PeriodoIntervalo
            | IdAcao::QuemTodos
            | IdAcao::QuemVoce
            | IdAcao::QuemRegras
    ) || crate::acoes::e_filtro_de_menu(id)
}

/// Os grupos de uma aba do Ribbon, com as Ações do SBF, na ordem da lista única. É a mesma
/// composição para montar as abas e para gerar os KeyTips (únicos e sem prefixo por aba).
pub fn grupos_da_aba(aba: Aba) -> Vec<(&'static str, Vec<IdAcao>)> {
    let mut grupos: Vec<(&'static str, Vec<IdAcao>)> = Vec::new();
    for d in ACOES
        .iter()
        .filter(|d| d.aba == aba && !fica_de_fora(d.id) && !filho_de_menu(d.id))
    {
        match grupos.iter_mut().find(|(g, _)| *g == d.grupo) {
            Some((_, ids)) => ids.push(d.id),
            None => grupos.push((d.grupo, vec![d.id])),
        }
    }
    if aba == Aba::Triagem
        && let Some((_, ids)) = grupos.iter_mut().find(|(g, _)| *g == "Histórico")
    {
        ids.insert(0, IdAcao::Desfazer);
    }
    if aba == Aba::Inicio {
        grupos.push(("Dados", vec![IdAcao::Atualizar]));
    }
    grupos
}

/// Os KeyTips das Ações do SBF: uma letra, única dentro de cada aba.
pub fn keytips_das_acoes() -> HashMap<IdAcao, String> {
    let mut v = HashMap::new();
    for (aba, ..) in ABAS {
        let ids: Vec<&acoes::DefinicaoDeAcao> = grupos_da_aba(aba)
            .into_iter()
            .flat_map(|(_, ids)| ids)
            .map(acoes::definicao)
            .collect();
        v.extend(AppSbf::keytips(&ids));
    }
    v
}

const ABAS: [(Aba, &str, &str, &str); 8] = [
    (Aba::Inicio, "inicio", "Início", "I"),
    (Aba::LivroCaixa, "livro", "Livro-caixa", "K"),
    (Aba::Triagem, "triagem", "Triagem", "T"),
    (Aba::Lotes, "lotes", "Lotes", "L"),
    (Aba::Ferramentas, "ferramentas", "Ferramentas", "F"),
    (Aba::Exibir, "exibir", "Exibir", "E"),
    (Aba::Achado, "achado", "Achado", "C"),
    (Aba::Pagina, "pagina", "Página", "P"),
];

impl AppCasca for AppSbf {
    fn acoes(&self) -> Vec<AcaoCasca> {
        let contexto = self.janela.borrow().contexto_das_acoes();
        let mut v = Vec::new();
        let conexoes = self.janela.borrow_mut().conexoes(self.consulta.as_ref());
        let todas: Vec<&acoes::DefinicaoDeAcao> =
            ACOES.iter().filter(|d| !fica_de_fora(d.id)).collect();
        let keytips = keytips_das_acoes();
        for d in todas {
            let disp = acoes::disponibilidade(d.id, &contexto);
            let mut a = AcaoCasca::nova(d.codigo, d.rotulo)
                .icone(d.icone)
                .dica(d.dica);
            if let Some(at) = d.atalho {
                a = a.atalho(atalho_da_casca(at));
            }
            if let Some(k) = keytips.get(&d.id) {
                a = a.keytip(k.clone());
            }
            if let Some(m) = disp.motivo() {
                a = a.desabilitada(m);
            }
            let j = self.janela.borrow();
            a = match d.id {
                IdAcao::Periodo => a.menu(&[
                    "inicio.periodo.atual",
                    "inicio.periodo.anterior",
                    "inicio.periodo.todos",
                    "inicio.periodo.intervalo",
                ]),
                IdAcao::PeriodoIntervalo => {
                    a.alternancia(j.periodo == crate::janela::Periodo::Intervalo)
                }
                IdAcao::FonteDoAchado => {
                    let menu: Vec<String> = std::iter::once(PREFIXO_FONTE_TODAS.to_owned())
                        .chain(
                            conexoes
                                .iter()
                                .map(|(id, _)| format!("{PREFIXO_FONTE}{id}")),
                        )
                        .collect();
                    let refs: Vec<&str> = menu.iter().map(String::as_str).collect();
                    a.menu(&refs)
                }
                IdAcao::TrocarTema => {
                    let menu: Vec<String> = tema::temas()
                        .iter()
                        .map(|t| format!("{PREFIXO_TEMA}{}", t.id))
                        .collect();
                    let refs: Vec<&str> = menu.iter().map(String::as_str).collect();
                    a.menu(&refs)
                }
                IdAcao::EstadoDoAchado => a.menu(&[
                    "inicio.estado.todos",
                    "inicio.estado.a-triar",
                    "inicio.estado.documento",
                    "inicio.estado.em-duvida",
                    "inicio.estado.descartado",
                ]),
                IdAcao::TipoDoDocumento => a.menu(&[
                    "inicio.tipo.todos",
                    "inicio.tipo.boleto",
                    "inicio.tipo.fatura",
                    "inicio.tipo.extrato",
                    "inicio.tipo.comprovante",
                    "inicio.tipo.nota-fiscal",
                    "inicio.tipo.foto",
                    "inicio.tipo.outro",
                ]),
                id if acoes::filtro_de_estado(id).is_some() => a.alternancia(
                    acoes::filtro_de_estado(id).flatten() == j.estado_do_achado.as_deref(),
                ),
                id if acoes::filtro_de_tipo(id).is_some() => a.alternancia(
                    acoes::filtro_de_tipo(id).flatten() == j.tipo_do_documento.as_deref(),
                ),
                IdAcao::QuemDecidiu => a.menu(&[
                    "inicio.quem.todos",
                    "inicio.quem.voce",
                    "inicio.quem.regras",
                ]),
                IdAcao::PeriodoAtual => {
                    a.alternancia(j.periodo == crate::janela::Periodo::CicloAtual)
                }
                IdAcao::PeriodoAnterior => {
                    a.alternancia(j.periodo == crate::janela::Periodo::CicloAnterior)
                }
                IdAcao::PeriodoTodos => {
                    a.alternancia(j.periodo == crate::janela::Periodo::TodosOsCiclos)
                }
                IdAcao::QuemTodos => a.alternancia(j.quem_decidiu.is_none()),
                IdAcao::QuemVoce => {
                    a.alternancia(j.quem_decidiu == Some(sbf_inventario::consulta::Quem::Usuario))
                }
                IdAcao::QuemRegras => a.alternancia(matches!(
                    j.quem_decidiu,
                    Some(sbf_inventario::consulta::Quem::Automatico(_))
                )),
                _ => a,
            };
            drop(j);
            v.push(a);
        }
        // As Ações geradas dos dados: uma por conexão (filtro) e uma por tema.
        let fonte_atual = self.janela.borrow().fonte.clone();
        v.push(
            AcaoCasca::nova(PREFIXO_FONTE_TODAS, "Todas as conexões")
                .icone("list_alt")
                .alternancia(fonte_atual.is_none()),
        );
        for (id, nome) in &conexoes {
            v.push(
                AcaoCasca::nova(format!("{PREFIXO_FONTE}{id}"), nome.clone())
                    .icone("cable")
                    .dica(format!("Só o que veio de {nome}"))
                    .alternancia(fonte_atual.as_deref() == Some(id.as_str())),
            );
        }
        for t in tema::temas() {
            v.push(
                AcaoCasca::nova(
                    format!("{PREFIXO_TEMA}{}", t.id),
                    format!("{} · {}", t.nome, t.familia),
                )
                .icone("palette")
                .alternancia(t.id == self.tema_atual),
            );
        }
        v
    }

    fn abas(&self) -> Vec<AbaCasca> {
        let contexto = self.janela.borrow().contexto_das_acoes();
        let achado = contexto.selecao.len() == 1
            && matches!(contexto.selecao[0].tipo, Tipo::Achado | Tipo::Documento);
        let pagina = contexto.item_aberto.is_some();
        let mut abas = Vec::new();
        for (aba, id, rotulo, keytip) in ABAS {
            let mut a = AbaCasca::nova(id, rotulo, keytip);
            match aba {
                Aba::Achado => a = a.contextual(achado),
                Aba::Pagina => a = a.contextual(pagina),
                _ => {}
            }
            let grupos: Vec<(&str, Vec<&str>)> = grupos_da_aba(aba)
                .into_iter()
                .map(|(g, ids)| {
                    (
                        g,
                        ids.into_iter()
                            .map(|i| acoes::definicao(i).codigo)
                            .collect(),
                    )
                })
                .collect();
            let mut painel_ids: Vec<String> = Vec::new();
            let mut ribbon_ids: Vec<&str> = Vec::new();
            if aba == Aba::Exibir {
                painel_ids = PainelDoDock::TODOS
                    .iter()
                    .map(|p| ids::painel(id_do_painel(*p)))
                    .collect();
                ribbon_ids = vec![
                    ids::RIBBON_CLASSICO,
                    ids::RIBBON_SIMPLES,
                    ids::RIBBON_RECOLHER,
                    ids::DENSIDADE_COMPACTA,
                    ids::TELA_CHEIA,
                    ids::MODO_FOCO,
                ];
            }
            if !painel_ids.is_empty() {
                let refs: Vec<&str> = painel_ids.iter().map(String::as_str).collect();
                a = a.grupo(Grupo::novo("exibir.paineis", "Painéis", &refs));
                a = a.grupo(Grupo::novo("exibir.layout", "Layout de tela", &ribbon_ids));
            }
            for (g, ids_do_grupo) in &grupos {
                let id_grupo = format!("{id}.{}", g.to_lowercase().replace(' ', "-"));
                a = a.grupo(Grupo::novo(id_grupo, *g, ids_do_grupo));
            }
            abas.push(a);
        }
        abas
    }

    fn acesso_rapido(&self) -> Vec<String> {
        vec![
            acoes::definicao(IdAcao::Desfazer).codigo.to_owned(),
            acoes::definicao(IdAcao::Atualizar).codigo.to_owned(),
            ids::PALETA.to_owned(),
        ]
    }

    fn navegacao(&self) -> Vec<GrupoNavegacao> {
        let contagens = {
            let mut j = self.janela.borrow_mut();
            j.contagens(self.consulta.as_ref())
        };
        let futura = |id: &str| {
            areas::area_futura(id).map(|a| {
                ItemNavegacao::novo(a.id, a.rotulo, a.icone)
                    .indisponivel(format!("fatia {}", a.fatia), a.motivo)
            })
        };
        let futuras = |ids: &[&str]| ids.iter().filter_map(|i| futura(i)).collect::<Vec<_>>();
        let real = |a: Area| {
            ItemNavegacao::novo(
                areas::rota_da_area(a),
                areas::rotulo_da_area(a),
                areas::icone_da_area(a),
            )
        };
        let mut dinheiro: Vec<ItemNavegacao> =
            areas::AREAS_DE_DINHEIRO.iter().map(|a| real(*a)).collect();
        dinheiro.push(real(Area::Relatorios));
        let estacao = {
            let mut item = real(Area::Sincronizacao);
            let recusadas = self
                .janela
                .borrow()
                .estacao
                .as_ref()
                .and_then(|e| e.0.alteracoes(true).ok())
                .map(|v| {
                    v.iter()
                        .filter(|a| {
                            a.estado == sbf_inventario::alteracao::EstadoDaAlteracao::Recusada
                        })
                        .count()
                })
                .unwrap_or(0);
            if recusadas > 0 {
                item = item.contagem(recusadas as u64);
            }
            vec![item]
        };
        let sugestoes = {
            let mut j = self.janela.borrow_mut();
            j.primeiras(
                self.consulta.as_ref(),
                &PedidoDeTabela::Area {
                    area: Area::SugestoesDeConciliacao,
                    filtro: Filtro::default(),
                },
                1000,
            )
            .map(|c| c.total as i64)
            .unwrap_or(0)
        };
        let inventario = areas::AREAS_DO_INVENTARIO
            .iter()
            .map(|a| {
                let mut item = ItemNavegacao::novo(
                    areas::rota_da_area(*a),
                    areas::rotulo_da_area(*a),
                    areas::icone_da_area(*a),
                );
                let n = contagens.as_ref().and_then(|c| match a {
                    Area::ATriar => Some(c.a_triar),
                    Area::EmDuvida => Some(c.em_duvida),
                    Area::Documentos => Some(c.documentos),
                    _ => None,
                });
                let n = n.or((*a == Area::SugestoesDeConciliacao).then_some(sugestoes));
                if let Some(n) = n {
                    item = item.contagem(n.max(0) as u64);
                }
                item
            })
            .collect();
        vec![
            GrupoNavegacao::novo("Dinheiro", dinheiro).selo("fatia 2"),
            GrupoNavegacao::novo("Documentos", futuras(&["caixa"])),
            GrupoNavegacao::novo("Inventário", inventario).selo("fatia 1"),
            GrupoNavegacao::novo("Fontes", futuras(&["conexoes", "alertas", "roteiros"])),
            GrupoNavegacao::novo("Estação", estacao),
            GrupoNavegacao::novo("Análise", futuras(&["pontos", "destinatarios"])),
        ]
    }

    fn destinos(&self) -> Vec<DestinoPaleta> {
        // A Paleta acha também fornecedores e lotes pelo nome.
        let mut v = Vec::new();
        let mut j = self.janela.borrow_mut();
        let consulta = self.consulta.as_ref();
        for (area, tipo) in [
            (Area::Fornecedores, "Fornecedor"),
            (Area::Lotes, "Lote"),
            (Area::Pendencias, "Pendência"),
        ] {
            let Ok(c) = j.primeiras(
                consulta,
                &PedidoDeTabela::Area {
                    area,
                    filtro: Filtro::default(),
                },
                500,
            ) else {
                continue;
            };
            for l in c.tabela.linhas.iter().take(500) {
                if let Some(alvo) = &l.alvo {
                    let rotulo = l
                        .celulas
                        .first()
                        .map(formato::texto_da_celula)
                        .unwrap_or_else(|| alvo.chave.clone());
                    let mut d =
                        DestinoPaleta::novo(Destino::Item(alvo.clone()).rota(), rotulo, tipo);
                    d.icone = alvo.tipo.icone().to_owned();
                    v.push(d);
                }
            }
        }
        v
    }

    fn rota_inicial(&self) -> Rota {
        Rota::nova(Destino::Area(Area::VisaoGeral).rota())
    }

    fn trilha(&self, rota: &Rota) -> Vec<Elo> {
        let Some(destino) = Destino::da_rota(rota.como_texto()) else {
            return vec![Elo::texto(rota.como_texto())];
        };
        match &destino {
            Destino::Item(r) => {
                let mut v = Vec::new();
                if let Some(d) = areas::destino_do_tipo(r.tipo) {
                    v.push(Elo::texto(areas::grupo_de(&d)));
                    v.push(Elo::link(areas::rotulo_do_destino(&d), d.rota()));
                }
                let rotulo = self.janela.borrow_mut().rotulo(self.consulta.as_ref(), r);
                v.push(Elo::texto(rotulo));
                v
            }
            Destino::Total { titulo, .. } => vec![Elo::texto("Total"), Elo::texto(titulo.clone())],
            outro => vec![
                Elo::texto(areas::grupo_de(outro)),
                Elo::texto(areas::rotulo_do_destino(outro)),
            ],
        }
    }

    fn desenhar_pagina(&mut self, ui: &mut Ui, rota: &Rota, local: Local) {
        self.preparar(ui.ctx());
        let ctx = ui.ctx().clone();
        if local == Local::Centro {
            self.processar(&ctx);
        }
        let Some(destino) = Destino::da_rota(rota.como_texto()) else {
            componentes::aviso(
                ui,
                "help",
                &format!("Página desconhecida: {rota}"),
                tema::paleta(&ctx).aviso_sutil,
                tema::paleta(&ctx).aviso_borda,
                tema::paleta(&ctx).aviso,
            );
            return;
        };
        let AppSbf {
            janela,
            consulta,
            vistas_flutuantes,
            ..
        } = self;
        let janela = janela.get_mut();
        match local {
            Local::Centro => {
                janela.mensagem_ui(ui);
                janela.desenhar_destino(ui, consulta.as_ref(), &destino, None);
                janela.dialogos_ui(&ctx);
                janela.ferramentas_ui(&ctx);
                janela.previas_ui(&ctx, consulta.as_ref());
            }
            Local::Flutuante(id) => {
                let vista = vistas_flutuantes.entry(id).or_default();
                janela.desenhar_destino(ui, consulta.as_ref(), &destino, Some(vista));
            }
        }
        if local == Local::Centro {
            self.processar(&ctx);
        }
    }

    fn paineis(&self) -> Vec<PainelDock> {
        PainelDoDock::TODOS
            .into_iter()
            .map(|p| {
                // Três pilhas, uma embaixo da outra: Detalhe (com Origens e Texto), Prévia
                // e Histórico.
                let pilha = match p {
                    PainelDoDock::Previa => 1,
                    PainelDoDock::Historico => 2,
                    _ => 0,
                };
                let mut d = PainelDock::novo(id_do_painel(p), p.titulo(), pilha).icone(p.icone());
                if matches!(p, PainelDoDock::Origens | PainelDoDock::Texto) {
                    d = d.escondido();
                }
                d
            })
            .collect()
    }

    fn desenhar_painel(&mut self, ui: &mut Ui, id: &str) {
        self.preparar(ui.ctx());
        if let Some(p) = painel_do_id(id) {
            let AppSbf {
                janela, consulta, ..
            } = self;
            janela.get_mut().dock_ui(ui, consulta.as_ref(), p);
        }
    }

    fn status(&self) -> Vec<SegmentoStatus> {
        let contagens = self.janela.borrow_mut().contagens(self.consulta.as_ref());
        let traco = |v: &Option<String>| v.clone().unwrap_or_else(|| "—".to_owned());
        let mut v = vec![
            SegmentoStatus::novo("inventario", self.barra.inventario.clone()).icone("inventory_2"),
        ];
        if let Some(c) = contagens {
            v.push(
                SegmentoStatus::novo(
                    "achados",
                    format!("{} achados", formato::inteiro(c.achados)),
                )
                .icone("find_in_page"),
            );
            v.push(
                SegmentoStatus::novo(
                    "documentos",
                    format!("{} documentos", formato::inteiro(c.documentos)),
                )
                .icone("task_alt"),
            );
            v.push(
                SegmentoStatus::novo(
                    "em-duvida",
                    format!("{} em dúvida", formato::inteiro(c.em_duvida)),
                )
                .icone("help"),
            );
            v.push(
                SegmentoStatus::novo(
                    "a-triar",
                    format!("{} a triar", formato::inteiro(c.a_triar)),
                )
                .icone("schedule")
                .clicavel(),
            );
        }
        let (sinc, hoje) = {
            let j = self.janela.borrow();
            (j.estado_da_sincronizacao(), j.hoje.clone())
        };
        let segmento = match &sinc {
            Some(e) => {
                let (texto, icone, aviso) = crate::dinheiro::texto_da_barra(e, &hoje);
                let mut s = SegmentoStatus::novo("sincronizacao", texto)
                    .icone(icone)
                    .clicavel();
                if aviso {
                    s = s.tom(Tom::Aviso);
                }
                if let Some(x) = crate::dinheiro::explicacao(e) {
                    s = s.dica(x);
                }
                s
            }
            None => SegmentoStatus::novo("sincronizacao", "Estação não pareada")
                .icone("cloud_off")
                .clicavel()
                .dica(crate::acoes::MOTIVO_SEM_ESTACAO),
        };
        v.push(segmento);
        let (chaveiro, ultima_copia) = {
            let j = self.janela.borrow();
            match &j.ferramentas {
                Some(f) => (
                    f.texto_do_chaveiro()
                        .or_else(|| self.barra.chaveiro.clone()),
                    f.ultima_copia
                        .clone()
                        .or_else(|| self.barra.ultima_copia.clone()),
                ),
                None => (self.barra.chaveiro.clone(), self.barra.ultima_copia.clone()),
            }
        };
        v.push(
            SegmentoStatus::novo("chaveiro", format!("Chaveiro {}", traco(&chaveiro))).icone("key"),
        );
        let disco = format!("Disco cifrado: {}", traco(&self.barra.disco_cifrado));
        v.push(match &self.barra.aviso_de_disco {
            Some(aviso) => SegmentoStatus::novo("disco", disco)
                .icone("warning")
                .tom(Tom::Aviso)
                .dica(aviso.clone()),
            None => SegmentoStatus::novo("disco", disco).icone("hard_drive"),
        });
        v.push(
            SegmentoStatus::novo("copia", format!("Última cópia: {}", traco(&ultima_copia)))
                .icone("backup"),
        );
        if let Some(s) = &self.barra.selo {
            v.push(
                SegmentoStatus::novo("selo", s.clone())
                    .icone("info")
                    .tom(Tom::Info)
                    .a_direita(),
            );
        }
        v.push(
            SegmentoStatus::novo("versao", format!("sbf {}", env!("CARGO_PKG_VERSION")))
                .a_direita(),
        );
        v
    }

    fn secoes_bastidores(&self) -> Vec<SecaoBastidores> {
        vec![
            SecaoBastidores::nova("inventario", "Inventário", "inventory_2"),
            SecaoBastidores::nova("chaveiro", "Chaveiro", "key"),
            SecaoBastidores::nova("estante", "Estante e cópia de segurança", "backup"),
            SecaoBastidores::nova("opcoes", "Opções", "tune"),
            SecaoBastidores::nova("sobre", "Sobre", "info"),
        ]
    }

    fn desenhar_bastidores(&mut self, ui: &mut Ui, id: &str) {
        self.preparar(ui.ctx());
        let p = tema::paleta(ui.ctx());
        let linha = |ui: &mut Ui, rotulo: &str, valor: String| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(rotulo).color(p.tinta_suave));
                ui.label(valor);
            });
        };
        match id {
            "inventario" => {
                componentes::titulo(ui, "Inventário", 18.0);
                linha(ui, "Aberto", self.barra.inventario.clone());
                if let Some(c) = self.janela.get_mut().contagens(self.consulta.as_ref()) {
                    linha(ui, "Achados", formato::inteiro(c.achados));
                    linha(ui, "Documentos", formato::inteiro(c.documentos));
                }
            }
            "chaveiro" if self.janela.get_mut().ferramentas.is_some() => {
                componentes::titulo(ui, "Chaveiro", 18.0);
                if let Some(f) = self.janela.get_mut().ferramentas.as_mut() {
                    f.chaveiro_ui(ui);
                }
            }
            "estante" if self.janela.get_mut().ferramentas.is_some() => {
                componentes::titulo(ui, "Estante e cópia de segurança", 18.0);
                if let Some(f) = self.janela.get_mut().ferramentas.as_mut() {
                    f.destinos_ui(ui);
                    linha(
                        ui,
                        "Última cópia",
                        f.ultima_copia.clone().unwrap_or_else(|| "—".into()),
                    );
                }
            }
            "chaveiro" => {
                componentes::titulo(ui, "Chaveiro", 18.0);
                linha(
                    ui,
                    "Estado",
                    self.barra.chaveiro.clone().unwrap_or_else(|| "—".into()),
                );
                ui.label(RichText::new("Os rótulos dos segredos aparecem aqui; os valores, nunca. Importar o cofre.enc chega quando o chaveiro for ligado a esta janela.").color(p.tinta_suave));
            }
            "estante" => {
                componentes::titulo(ui, "Estante e cópia de segurança", 18.0);
                linha(
                    ui,
                    "Última cópia",
                    self.barra
                        .ultima_copia
                        .clone()
                        .unwrap_or_else(|| "—".into()),
                );
                ui.label(RichText::new("Configure o destino no Google Drive para habilitar Atualizar estante e Fazer cópia de segurança.").color(p.tinta_suave));
            }
            "opcoes" => {
                componentes::titulo(ui, "Opções", 18.0);
                bancada_casca::vista::opcoes_de_aparencia(ui, &self.textos);
                ui.add_space(12.0);
                opcoes_da_previa(ui, &mut self.janela.get_mut().previas);
                ui.add_space(12.0);
                let janela = self.janela.get_mut();
                let mut pedidos = Vec::new();
                crate::triagem::opcoes_ia(ui, &mut janela.triagem, &mut pedidos);
                for p in pedidos {
                    janela.pedir(p);
                }
            }
            _ => {
                componentes::titulo(ui, "Seu Bolso Feliz", 18.0);
                linha(ui, "Versão", env!("CARGO_PKG_VERSION").to_owned());
                ui.label(RichText::new("Fatia 2: o inventário e o livro-caixa (só leitura, mais mudar a categoria) no desktop. Fontes Manrope e Instrument Serif (OFL) e Material Symbols Rounded (Apache-2.0).").color(p.tinta_suave));
            }
        }
    }

    fn ao_executar(&mut self, ctx: &egui::Context, id: &str) {
        if id == PREFIXO_FONTE_TODAS {
            self.janela.get_mut().fonte = None;
            return;
        }
        if let Some(fonte) = id.strip_prefix(PREFIXO_FONTE) {
            self.janela.get_mut().fonte = Some(fonte.to_owned());
            return;
        }
        if let Some(t) = id.strip_prefix(PREFIXO_TEMA) {
            bancada_casca::pedir(ctx, bancada_casca::Pedido::Tema(t.to_owned()));
            return;
        }
        let Some(acao) = acao_do_codigo(id) else {
            return;
        };
        match acao {
            IdAcao::VerDecisoes | IdAcao::VerTexto => {
                let painel = if acao == IdAcao::VerDecisoes {
                    "historico"
                } else {
                    "texto"
                };
                if !self.paineis_visiveis.get(painel).copied().unwrap_or(false) {
                    bancada_casca::executar(ctx, ids::painel(painel));
                }
            }
            IdAcao::TrocarTema => {
                let todos = tema::temas();
                let i = todos
                    .iter()
                    .position(|t| t.id == self.tema_atual)
                    .map_or(0, |i| (i + 1) % todos.len().max(1));
                if let Some(t) = todos.get(i) {
                    bancada_casca::pedir(ctx, bancada_casca::Pedido::Tema(t.id.clone()));
                }
            }
            outra => {
                self.janela.get_mut().executar(outra);
                self.processar(ctx);
            }
        }
    }

    fn ao_abrir_rota(&mut self, ctx: &egui::Context, rota: &Rota, local: Local) {
        if local != Local::Centro {
            return;
        }
        if let Some(d) = Destino::da_rota(rota.como_texto()) {
            // O Ribbon mostra as Ações da área aberta (ADR-018).
            let aba = match &d {
                Destino::Area(
                    Area::ATriar | Area::EmDuvida | Area::Documentos | Area::Registros,
                ) => Some("triagem"),
                Destino::Area(Area::Lotes) => Some("lotes"),
                Destino::Area(a) if a.e_do_livro_caixa() || *a == Area::Sincronizacao => {
                    Some("livro")
                }
                Destino::Item(r)
                    if matches!(r.tipo, Tipo::Movimentacao | Tipo::Estacao | Tipo::Alteracao) =>
                {
                    Some("livro")
                }
                Destino::Item(r)
                    if matches!(r.tipo, Tipo::Achado | Tipo::Documento | Tipo::Registro) =>
                {
                    Some("triagem")
                }
                Destino::Item(r) if r.tipo == Tipo::Lote => Some("lotes"),
                _ => None,
            };
            if let Some(aba) = aba {
                bancada_casca::pedir(ctx, bancada_casca::Pedido::AbrirAba(aba.to_owned()));
            }
            self.janela.get_mut().ao_abrir_no_centro(d);
        }
    }

    fn ao_mudar_preferencias(&mut self, preferencias: &Preferencias) {
        self.paineis_visiveis = preferencias.paineis_visiveis.clone();
        self.tema_atual = preferencias.tema.clone();
    }

    fn ao_clicar_status(&mut self, ctx: &egui::Context, id: &str) {
        if id == "a-triar" {
            bancada_casca::abrir(ctx, Destino::Area(Area::ATriar).rota());
        }
        if id == "sincronizacao" {
            bancada_casca::abrir(ctx, Destino::Area(Area::Sincronizacao).rota());
        }
    }
}

/// Opções › Prévia do arquivo: os limites do cache (só em memória).
pub fn opcoes_da_previa(ui: &mut Ui, previas: &mut crate::previa::Previas) {
    let p = tema::paleta(ui.ctx());
    componentes::rotulo_de_secao(ui, "Prévia do arquivo");
    ui.label(
        RichText::new(
            "As prévias abertas ficam só na memória, nunca em disco. Guarda as últimas N ou até o limite de memória, o que vier primeiro.",
        )
        .color(p.tinta_suave),
    );
    let (mut documentos, bytes) = previas.limites();
    let mut mb = (bytes / (1024 * 1024)) as u32;
    let mut mudou = false;
    egui::Grid::new("opcoes-da-previa")
        .num_columns(2)
        .spacing(egui::vec2(12.0, 6.0))
        .show(ui, |ui| {
            ui.label("Prévias guardadas");
            mudou |= ui
                .add(
                    egui::DragValue::new(&mut documentos)
                        .range(1..=500)
                        .suffix(" documentos"),
                )
                .changed();
            ui.end_row();
            ui.label("Memória das prévias");
            mudou |= ui
                .add(egui::DragValue::new(&mut mb).range(16..=4096).suffix(" MB"))
                .changed();
            ui.end_row();
        });
    if mudou {
        previas.definir_limites(documentos, mb as usize * 1024 * 1024);
    }
}

/// Mensagem curta no topo da página (para o integrador mostrar um aviso seu).
pub fn avisar(app: &AppSbf, texto: impl Into<String>, erro: bool) {
    app.janela().mensagem = Some(Mensagem {
        texto: texto.into(),
        erro,
    });
}
