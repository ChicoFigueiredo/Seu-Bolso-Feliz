//! A **lista única de Ações** do SBF (spec 06, "A janela"; INV-14: "toda Ação vem da
//! lista única"). O Ribbon, a Paleta, os atalhos e a faixa provisória leem daqui.
//!
//! Cada Ação tem id estável, rótulo no infinitivo, ícone Material Symbols, atalho, aba e
//! grupo do Ribbon. Se está habilitada depende do contexto ([`disponibilidade`]): a
//! desabilitada sempre diz o motivo.

use egui::{Key, KeyboardShortcut, Modifiers};
use sbf_inventario::{Ref, Tipo};

/// O id de uma Ação.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum IdAcao {
    // Barra de Acesso Rápido e navegação
    Voltar,
    Avancar,
    Atualizar,
    Pesquisar,
    /// Período (menu): ciclo atual, ciclo anterior ou todos.
    Periodo,
    PeriodoAtual,
    PeriodoAnterior,
    PeriodoTodos,
    /// Período › Intervalo…: do ciclo inicial ao final.
    PeriodoIntervalo,
    /// Conexão (menu, com as conexões do inventário).
    FonteDoAchado,
    /// Estado do achado (menu).
    EstadoDoAchado,
    EstadoTodos,
    EstadoATriar,
    EstadoDocumento,
    EstadoEmDuvida,
    EstadoDescartado,
    /// Tipo de documento (menu).
    TipoDoDocumento,
    TipoTodos,
    TipoBoleto,
    TipoFatura,
    TipoExtrato,
    TipoComprovante,
    TipoNotaFiscal,
    TipoFoto,
    TipoOutro,
    /// Quem decidiu (menu): você, as regras ou todos.
    QuemDecidiu,
    QuemTodos,
    QuemVoce,
    QuemRegras,
    AbrirEmPainelFlutuante,
    // Triagem
    Documento,
    Descartar,
    EmDuvida,
    // Corrigir
    CorrigirFornecedor,
    CorrigirCategoria,
    CorrigirTipo,
    CorrigirDataEValor,
    // Relacionar
    MarcarDuplicidade,
    UnirConciliados,
    ConfirmarRecorrencia,
    // Interpretação
    Aprovar,
    Rejeitar,
    // Pendência
    Sinalizar,
    Responder,
    // Histórico
    Desfazer,
    VerDecisoes,
    // Lotes
    NovoLote,
    AdicionarSelecao,
    RemoverDoLote,
    AprovarLote,
    DesaprovarLote,
    EnviarAoServidor,
    // Ferramentas
    AbrirPdfProtegido,
    RegrasDeSenha,
    NovaRegraDeSenha,
    AtualizarEstante,
    FazerCopiaDeSeguranca,
    // Exibir
    PainelDetalhe,
    PainelOrigens,
    PainelTexto,
    PainelHistorico,
    TrocarTema,
    // Achado (contextual)
    AbrirOriginal,
    MostrarNoCofre,
    VerTexto,
    AbrirProtegido,
    CopiarHash,
    // Página
    CopiarRastro,
}

/// A aba do Ribbon onde a Ação mora.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Aba {
    AcessoRapido,
    Inicio,
    Triagem,
    Lotes,
    Ferramentas,
    Exibir,
    /// Contextual: aparece com um achado (ou documento) selecionado.
    Achado,
    /// Contextual: aparece numa página de item.
    Pagina,
}

impl Aba {
    pub fn rotulo(self) -> &'static str {
        match self {
            Aba::AcessoRapido => "Acesso rápido",
            Aba::Inicio => "Início",
            Aba::Triagem => "Triagem",
            Aba::Lotes => "Lotes",
            Aba::Ferramentas => "Ferramentas",
            Aba::Exibir => "Exibir",
            Aba::Achado => "Achado",
            Aba::Pagina => "Página",
        }
    }
}

/// Um atalho de teclado.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Atalho {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub tecla: Key,
}

impl Atalho {
    const fn ctrl(tecla: Key) -> Self {
        Self {
            ctrl: true,
            shift: false,
            alt: false,
            tecla,
        }
    }
    const fn ctrl_shift(tecla: Key) -> Self {
        Self {
            ctrl: true,
            shift: true,
            alt: false,
            tecla,
        }
    }
    const fn alt(tecla: Key) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: true,
            tecla,
        }
    }
    const fn so(tecla: Key) -> Self {
        Self {
            ctrl: false,
            shift: false,
            alt: false,
            tecla,
        }
    }

    pub fn atalho_egui(&self) -> KeyboardShortcut {
        let mut m = Modifiers::NONE;
        if self.ctrl {
            m |= Modifiers::COMMAND;
        }
        if self.shift {
            m |= Modifiers::SHIFT;
        }
        if self.alt {
            m |= Modifiers::ALT;
        }
        KeyboardShortcut::new(m, self.tecla)
    }

    /// "Ctrl+Shift+F", "Alt+←".
    pub fn texto(&self) -> String {
        let mut s = String::new();
        if self.ctrl {
            s.push_str("Ctrl+");
        }
        if self.shift {
            s.push_str("Shift+");
        }
        if self.alt {
            s.push_str("Alt+");
        }
        let tecla = match self.tecla {
            Key::ArrowLeft => "←".to_owned(),
            Key::ArrowRight => "→".to_owned(),
            Key::Enter => "Enter".to_owned(),
            Key::Delete => "Delete".to_owned(),
            outra => outra.name().to_owned(),
        };
        s.push_str(&tecla);
        s
    }
}

/// A definição de uma Ação na lista única.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DefinicaoDeAcao {
    pub id: IdAcao,
    /// Id estável em texto (`triagem.documento`), para a casca, a Paleta e o i18n.
    pub codigo: &'static str,
    /// Rótulo no infinitivo, como no Ribbon.
    pub rotulo: &'static str,
    /// Nome do ícone Material Symbols Rounded.
    pub icone: &'static str,
    pub atalho: Option<Atalho>,
    pub aba: Aba,
    /// O grupo dentro da aba ("Triagem", "Corrigir", "Relacionar"…).
    pub grupo: &'static str,
    /// O tooltip.
    pub dica: &'static str,
}

#[allow(clippy::too_many_arguments)]
const fn a(
    id: IdAcao,
    codigo: &'static str,
    rotulo: &'static str,
    icone: &'static str,
    atalho: Option<Atalho>,
    aba: Aba,
    grupo: &'static str,
    dica: &'static str,
) -> DefinicaoDeAcao {
    DefinicaoDeAcao {
        id,
        codigo,
        rotulo,
        icone,
        atalho,
        aba,
        grupo,
        dica,
    }
}

use Aba as B;
use IdAcao as I;

/// A lista única, na ordem do Ribbon.
pub const ACOES: &[DefinicaoDeAcao] = &[
    a(
        I::Voltar,
        "navegar.voltar",
        "Voltar",
        "arrow_back",
        Some(Atalho::alt(Key::ArrowLeft)),
        B::AcessoRapido,
        "Navegar",
        "Volta ao lugar anterior (também o botão lateral do mouse)",
    ),
    a(
        I::Avancar,
        "navegar.avancar",
        "Avançar",
        "arrow_forward",
        Some(Atalho::alt(Key::ArrowRight)),
        B::AcessoRapido,
        "Navegar",
        "Avança de novo no caminho",
    ),
    a(
        I::Desfazer,
        "historico.desfazer",
        "Desfazer decisão",
        "undo",
        Some(Atalho::ctrl(Key::Z)),
        B::AcessoRapido,
        "Histórico",
        "Desfaz a sua última decisão com uma nova decisão",
    ),
    a(
        I::Atualizar,
        "dados.atualizar",
        "Atualizar",
        "refresh",
        Some(Atalho::so(Key::F5)),
        B::AcessoRapido,
        "Dados",
        "Relê o inventário",
    ),
    a(
        I::Pesquisar,
        "inicio.pesquisar",
        "Pesquisar",
        "search",
        Some(Atalho::ctrl(Key::F)),
        B::Inicio,
        "Pesquisa",
        "Pesquisa nesta área",
    ),
    a(
        I::Periodo,
        "inicio.periodo",
        "Período",
        "date_range",
        None,
        B::Inicio,
        "Período",
        "Ciclo atual, ciclo anterior ou todos os ciclos",
    ),
    a(
        I::PeriodoAtual,
        "inicio.periodo.atual",
        "Ciclo atual",
        "today",
        None,
        B::Inicio,
        "Período",
        "Só o ciclo atual",
    ),
    a(
        I::PeriodoAnterior,
        "inicio.periodo.anterior",
        "Ciclo anterior",
        "history",
        None,
        B::Inicio,
        "Período",
        "Só o ciclo anterior",
    ),
    a(
        I::PeriodoTodos,
        "inicio.periodo.todos",
        "Todos os ciclos",
        "calendar_month",
        None,
        B::Inicio,
        "Período",
        "Sem filtro de ciclo",
    ),
    a(
        I::PeriodoIntervalo,
        "inicio.periodo.intervalo",
        "Intervalo…",
        "date_range",
        None,
        B::Inicio,
        "Período",
        "Do ciclo inicial ao ciclo final",
    ),
    a(
        I::FonteDoAchado,
        "inicio.fonte",
        "Conexão",
        "cable",
        None,
        B::Inicio,
        "Filtros",
        "Só o que veio de uma conexão",
    ),
    a(
        I::EstadoDoAchado,
        "inicio.estado",
        "Estado do achado",
        "filter_alt",
        None,
        B::Inicio,
        "Filtros",
        "Só os achados num estado",
    ),
    a(
        I::EstadoTodos,
        "inicio.estado.todos",
        "Todos os estados",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Sem filtro de estado",
    ),
    a(
        I::EstadoATriar,
        "inicio.estado.a-triar",
        "A triar",
        "schedule",
        None,
        B::Inicio,
        "Filtros",
        "Só os achados a triar",
    ),
    a(
        I::EstadoDocumento,
        "inicio.estado.documento",
        "Documento",
        "task_alt",
        None,
        B::Inicio,
        "Filtros",
        "Só os documentos",
    ),
    a(
        I::EstadoEmDuvida,
        "inicio.estado.em-duvida",
        "Em dúvida",
        "help",
        None,
        B::Inicio,
        "Filtros",
        "Só os achados em dúvida",
    ),
    a(
        I::EstadoDescartado,
        "inicio.estado.descartado",
        "Descartado",
        "block",
        None,
        B::Inicio,
        "Filtros",
        "Só os descartados",
    ),
    a(
        I::TipoDoDocumento,
        "inicio.tipo",
        "Tipo",
        "label",
        None,
        B::Inicio,
        "Filtros",
        "Só um tipo de documento",
    ),
    a(
        I::TipoTodos,
        "inicio.tipo.todos",
        "Todos os tipos",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Sem filtro de tipo",
    ),
    a(
        I::TipoBoleto,
        "inicio.tipo.boleto",
        "Boleto",
        "receipt_long",
        None,
        B::Inicio,
        "Filtros",
        "Só boletos",
    ),
    a(
        I::TipoFatura,
        "inicio.tipo.fatura",
        "Fatura de cartão",
        "credit_card",
        None,
        B::Inicio,
        "Filtros",
        "Só faturas de cartão",
    ),
    a(
        I::TipoExtrato,
        "inicio.tipo.extrato",
        "Extrato",
        "account_balance",
        None,
        B::Inicio,
        "Filtros",
        "Só extratos",
    ),
    a(
        I::TipoComprovante,
        "inicio.tipo.comprovante",
        "Comprovante",
        "description",
        None,
        B::Inicio,
        "Filtros",
        "Só comprovantes",
    ),
    a(
        I::TipoNotaFiscal,
        "inicio.tipo.nota-fiscal",
        "Nota fiscal",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Só notas fiscais",
    ),
    a(
        I::TipoFoto,
        "inicio.tipo.foto",
        "Foto",
        "find_in_page",
        None,
        B::Inicio,
        "Filtros",
        "Só fotos",
    ),
    a(
        I::TipoOutro,
        "inicio.tipo.outro",
        "Outro",
        "category",
        None,
        B::Inicio,
        "Filtros",
        "Só os de tipo outro",
    ),
    a(
        I::QuemDecidiu,
        "inicio.quem-decidiu",
        "Quem decidiu",
        "person",
        None,
        B::Inicio,
        "Filtros",
        "Só o que você ou as regras decidiram",
    ),
    a(
        I::QuemTodos,
        "inicio.quem.todos",
        "Todos",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Decisões de todos",
    ),
    a(
        I::QuemVoce,
        "inicio.quem.voce",
        "Você",
        "person",
        None,
        B::Inicio,
        "Filtros",
        "Só o que você decidiu",
    ),
    a(
        I::QuemRegras,
        "inicio.quem.regras",
        "Regras",
        "smart_toy",
        None,
        B::Inicio,
        "Filtros",
        "Só o que as regras decidiram",
    ),
    a(
        I::Documento,
        "triagem.documento",
        "Documento",
        "task_alt",
        Some(Atalho::ctrl(Key::Num1)),
        B::Triagem,
        "Triagem",
        "Marca os achados selecionados como documento",
    ),
    a(
        I::Descartar,
        "triagem.descartar",
        "Descartar",
        "block",
        Some(Atalho::ctrl(Key::Num2)),
        B::Triagem,
        "Triagem",
        "Marca os achados selecionados como descartados",
    ),
    a(
        I::EmDuvida,
        "triagem.em-duvida",
        "Em dúvida",
        "help",
        Some(Atalho::ctrl(Key::Num3)),
        B::Triagem,
        "Triagem",
        "Deixa os achados selecionados em dúvida",
    ),
    a(
        I::CorrigirFornecedor,
        "corrigir.fornecedor",
        "Corrigir fornecedor",
        "storefront",
        Some(Atalho::ctrl_shift(Key::F)),
        B::Triagem,
        "Corrigir",
        "Corrige o fornecedor do achado ou do registro",
    ),
    a(
        I::CorrigirCategoria,
        "corrigir.categoria",
        "Corrigir categoria",
        "category",
        Some(Atalho::ctrl_shift(Key::C)),
        B::Triagem,
        "Corrigir",
        "Corrige a categoria do registro",
    ),
    a(
        I::CorrigirTipo,
        "corrigir.tipo",
        "Corrigir tipo",
        "label",
        Some(Atalho::ctrl_shift(Key::T)),
        B::Triagem,
        "Corrigir",
        "Corrige o tipo do achado ou do registro",
    ),
    a(
        I::CorrigirDataEValor,
        "corrigir.data-e-valor",
        "Corrigir data e valor",
        "edit_note",
        Some(Atalho::ctrl_shift(Key::V)),
        B::Triagem,
        "Corrigir",
        "Corrige a data e o valor do achado; o ciclo é recalculado",
    ),
    a(
        I::MarcarDuplicidade,
        "relacionar.duplicidade",
        "Marcar duplicidade",
        "difference",
        Some(Atalho::ctrl_shift(Key::D)),
        B::Triagem,
        "Relacionar",
        "Marca dois achados como duplicidade um do outro",
    ),
    a(
        I::UnirConciliados,
        "relacionar.conciliar",
        "Unir conciliados",
        "call_merge",
        Some(Atalho::ctrl_shift(Key::U)),
        B::Triagem,
        "Relacionar",
        "Reconhece que dois registros descrevem o mesmo fato",
    ),
    a(
        I::ConfirmarRecorrencia,
        "relacionar.recorrencia",
        "Confirmar recorrência",
        "event_repeat",
        None,
        B::Triagem,
        "Relacionar",
        "Confirma que o fornecedor se repete todo ciclo",
    ),
    a(
        I::Aprovar,
        "interpretacao.aprovar",
        "Aprovar",
        "thumb_up",
        Some(Atalho::ctrl(Key::Enter)),
        B::Triagem,
        "Interpretação",
        "Aprova os registros selecionados",
    ),
    a(
        I::Rejeitar,
        "interpretacao.rejeitar",
        "Rejeitar",
        "thumb_down",
        Some(Atalho::ctrl_shift(Key::Enter)),
        B::Triagem,
        "Interpretação",
        "Rejeita os registros selecionados, com motivo",
    ),
    a(
        I::Sinalizar,
        "pendencia.sinalizar",
        "Sinalizar",
        "flag",
        Some(Atalho::ctrl_shift(Key::S)),
        B::Triagem,
        "Pendência",
        "Abre uma pendência sobre o item",
    ),
    a(
        I::Responder,
        "pendencia.responder",
        "Responder",
        "reply",
        Some(Atalho::ctrl_shift(Key::R)),
        B::Triagem,
        "Pendência",
        "Responde a pendência selecionada",
    ),
    a(
        I::VerDecisoes,
        "historico.ver-decisoes",
        "Ver decisões",
        "history",
        Some(Atalho::ctrl(Key::H)),
        B::Triagem,
        "Histórico",
        "Mostra o Histórico de decisões",
    ),
    a(
        I::NovoLote,
        "lotes.novo",
        "Novo lote",
        "inventory_2",
        Some(Atalho::ctrl_shift(Key::N)),
        B::Lotes,
        "Lote",
        "Cria um lote com nome",
    ),
    a(
        I::AdicionarSelecao,
        "lotes.adicionar",
        "Adicionar seleção",
        "playlist_add",
        Some(Atalho::ctrl_shift(Key::A)),
        B::Lotes,
        "Lote",
        "Põe os documentos e registros selecionados no lote atual",
    ),
    a(
        I::RemoverDoLote,
        "lotes.remover",
        "Remover",
        "playlist_remove",
        None,
        B::Lotes,
        "Lote",
        "Tira os itens selecionados do lote atual",
    ),
    a(
        I::AprovarLote,
        "lotes.aprovar",
        "Aprovar lote",
        "check_circle",
        None,
        B::Lotes,
        "Aprovação",
        "Aprova o lote atual: os registros dele ficam aprovados",
    ),
    a(
        I::DesaprovarLote,
        "lotes.desaprovar",
        "Desaprovar lote",
        "do_not_disturb_on",
        None,
        B::Lotes,
        "Aprovação",
        "Volta o lote atual para editável, por uma nova decisão",
    ),
    a(
        I::EnviarAoServidor,
        "lotes.enviar",
        "Enviar ao servidor",
        "send",
        None,
        B::Lotes,
        "Promoção",
        "Promove o lote aprovado para a caixa de entrada",
    ),
    a(
        I::AbrirPdfProtegido,
        "ferramentas.pdf-protegido",
        "Abrir PDF protegido",
        "lock_open",
        None,
        B::Ferramentas,
        "PDF protegido",
        "Tenta abrir os PDFs protegidos pelas regras de senha",
    ),
    a(
        I::RegrasDeSenha,
        "ferramentas.regras-de-senha",
        "Regras de senha",
        "key",
        None,
        B::Ferramentas,
        "PDF protegido",
        "As regras de senha por fornecedor e tipo",
    ),
    a(
        I::NovaRegraDeSenha,
        "ferramentas.nova-regra",
        "Nova regra",
        "add",
        None,
        B::Ferramentas,
        "PDF protegido",
        "Cria uma regra de senha: fornecedor, tipo e estratégia (nunca a senha)",
    ),
    a(
        I::AtualizarEstante,
        "ferramentas.estante",
        "Atualizar estante",
        "backup",
        None,
        B::Ferramentas,
        "Estante e cópia",
        "Copia os documentos para a estante no Google Drive",
    ),
    a(
        I::FazerCopiaDeSeguranca,
        "ferramentas.copia",
        "Fazer cópia de segurança",
        "enhanced_encryption",
        None,
        B::Ferramentas,
        "Estante e cópia",
        "Cópia cifrada do cofre e do banco",
    ),
    a(
        I::PainelDetalhe,
        "exibir.detalhe",
        "Detalhe",
        "info",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Detalhe",
    ),
    a(
        I::PainelOrigens,
        "exibir.origens",
        "Origens",
        "folder_open",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Origens",
    ),
    a(
        I::PainelTexto,
        "exibir.texto",
        "Texto extraído",
        "description",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Texto extraído",
    ),
    a(
        I::PainelHistorico,
        "exibir.historico",
        "Histórico de decisões",
        "history",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Histórico de decisões",
    ),
    a(
        I::TrocarTema,
        "exibir.tema",
        "Tema",
        "palette",
        None,
        B::Exibir,
        "Aparência",
        "Escolhe um dos 7 temas do SBF",
    ),
    a(
        I::AbrirOriginal,
        "achado.abrir-original",
        "Abrir original",
        "open_in_new",
        None,
        B::Achado,
        "Original",
        "Abre o arquivo original do cofre",
    ),
    a(
        I::MostrarNoCofre,
        "achado.mostrar-no-cofre",
        "Mostrar no cofre",
        "folder",
        None,
        B::Achado,
        "Original",
        "Mostra o arquivo na pasta do cofre",
    ),
    a(
        I::VerTexto,
        "achado.ver-texto",
        "Ver texto",
        "description",
        None,
        B::Achado,
        "Original",
        "Mostra o texto extraído",
    ),
    a(
        I::AbrirProtegido,
        "achado.abrir-protegido",
        "Abrir protegido",
        "lock",
        None,
        B::Achado,
        "Original",
        "Abre o PDF protegido pela regra de senha",
    ),
    a(
        I::CopiarHash,
        "achado.copiar-hash",
        "Copiar hash",
        "fingerprint",
        None,
        B::Achado,
        "Identidade",
        "Copia o hash do conteúdo",
    ),
    a(
        I::AbrirEmPainelFlutuante,
        "pagina.flutuante",
        "Abrir em painel flutuante",
        "picture_in_picture",
        Some(Atalho::ctrl_shift(Key::O)),
        B::Pagina,
        "Janela",
        "Abre o item selecionado num painel flutuante (ou Ctrl+clique)",
    ),
    a(
        I::CopiarRastro,
        "pagina.copiar-rastro",
        "Copiar rastro",
        "content_copy",
        None,
        B::Pagina,
        "Rastro",
        "Copia a cadeia de proveniência do item como texto",
    ),
];

/// A definição de uma Ação. Toda `IdAcao` está na lista (conferido por teste).
pub fn definicao(id: IdAcao) -> &'static DefinicaoDeAcao {
    ACOES.iter().find(|d| d.id == id).unwrap_or(&ACOES[0])
}

/// Se uma Ação pode rodar agora.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Disponibilidade {
    Habilitada,
    Desabilitada(String),
}

impl Disponibilidade {
    pub fn habilitada(&self) -> bool {
        matches!(self, Disponibilidade::Habilitada)
    }

    pub fn motivo(&self) -> Option<&str> {
        match self {
            Disponibilidade::Habilitada => None,
            Disponibilidade::Desabilitada(m) => Some(m),
        }
    }
}

/// O que a disponibilidade precisa saber da janela.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ContextoDasAcoes {
    /// A seleção da tabela (ou o item aberto, quando a tabela não tem seleção).
    pub selecao: Vec<Ref>,
    pub lote_atual: Option<Ref>,
    pub pode_voltar: bool,
    pub pode_avancar: bool,
    /// A página aberta no centro, se houver.
    pub item_aberto: Option<Ref>,
    /// Há um leitor de PDF protegido ligado (fatia 1, `sbf-pdf`).
    pub pdf_ligado: bool,
    /// Há estante e cópia de segurança ligadas (fatia 1, `sbf-estante`).
    pub estante_ligada: bool,
    /// Há um cofre aberto (o modo demonstração não tem).
    pub cofre_aberto: bool,
    /// Destinos configurados em Opções.
    pub destino_da_estante: bool,
    pub destino_da_copia: bool,
    /// Uma ferramenta lenta já está rodando.
    pub ferramenta_ocupada: bool,
}

fn todos_de(selecao: &[Ref], tipos: &[Tipo]) -> bool {
    !selecao.is_empty() && selecao.iter().all(|r| tipos.contains(&r.tipo))
}

const ACHADOS: &[Tipo] = &[Tipo::Achado, Tipo::Documento];

/// O motivo das ferramentas que outros crates ligam.
pub const MOTIVO_NAO_LIGADO: &str = "Ainda não ligado a esta janela";

/// Se a Ação pode rodar no contexto. A desabilitada diz o motivo.
pub fn disponibilidade(id: IdAcao, c: &ContextoDasAcoes) -> Disponibilidade {
    use Disponibilidade::{Desabilitada, Habilitada};
    let exige = |ok: bool, motivo: &str| {
        if ok {
            Habilitada
        } else {
            Desabilitada(motivo.to_owned())
        }
    };
    let sel = &c.selecao;
    match id {
        I::Voltar => exige(c.pode_voltar, "Não há para onde voltar"),
        I::Avancar => exige(c.pode_avancar, "Não há para onde avançar"),
        I::Atualizar | I::Pesquisar | I::Desfazer | I::VerDecisoes | I::NovoLote => Habilitada,
        I::Periodo | I::PeriodoAtual | I::PeriodoAnterior | I::PeriodoTodos => Habilitada,
        I::QuemDecidiu | I::QuemTodos | I::QuemVoce | I::QuemRegras => Habilitada,
        I::PeriodoIntervalo | I::FonteDoAchado | I::NovaRegraDeSenha => Habilitada,
        I::EstadoDoAchado
        | I::EstadoTodos
        | I::EstadoATriar
        | I::EstadoDocumento
        | I::EstadoEmDuvida
        | I::EstadoDescartado
        | I::TipoDoDocumento
        | I::TipoTodos
        | I::TipoBoleto
        | I::TipoFatura
        | I::TipoExtrato
        | I::TipoComprovante
        | I::TipoNotaFiscal
        | I::TipoFoto
        | I::TipoOutro => Habilitada,
        id if e_filtro_de_menu(id) || matches!(id, I::EstadoDoAchado | I::TipoDoDocumento) => {
            Habilitada
        }
        I::PainelDetalhe | I::PainelOrigens | I::PainelTexto | I::PainelHistorico => Habilitada,
        I::TrocarTema => Habilitada,
        I::Documento | I::Descartar | I::EmDuvida => {
            exige(todos_de(sel, ACHADOS), "Selecione um ou mais achados")
        }
        I::CorrigirFornecedor | I::CorrigirTipo => exige(
            todos_de(sel, &[Tipo::Achado, Tipo::Documento, Tipo::Registro]),
            "Selecione achados ou registros",
        ),
        I::CorrigirCategoria => exige(todos_de(sel, &[Tipo::Registro]), "Selecione registros"),
        I::CorrigirDataEValor => exige(
            sel.len() == 1 && todos_de(sel, ACHADOS),
            "Selecione um achado",
        ),
        I::MarcarDuplicidade => exige(
            sel.len() == 2 && todos_de(sel, ACHADOS),
            "Selecione dois achados",
        ),
        I::UnirConciliados => exige(
            sel.len() == 2 && todos_de(sel, &[Tipo::Registro]),
            "Selecione dois registros",
        ),
        I::ConfirmarRecorrencia => exige(
            todos_de(sel, &[Tipo::Fornecedor]),
            "Selecione um fornecedor",
        ),
        I::Aprovar | I::Rejeitar => exige(todos_de(sel, &[Tipo::Registro]), "Selecione registros"),
        I::Sinalizar => exige(
            sel.len() == 1
                && todos_de(
                    sel,
                    &[Tipo::Achado, Tipo::Documento, Tipo::Registro, Tipo::Lote],
                ),
            "Selecione um achado, registro ou lote",
        ),
        I::Responder => exige(
            sel.len() == 1 && todos_de(sel, &[Tipo::Pendencia]),
            "Selecione uma pendência",
        ),
        I::AdicionarSelecao => {
            if c.lote_atual.is_none() {
                Desabilitada("Escolha ou crie um lote".to_owned())
            } else {
                exige(
                    todos_de(sel, &[Tipo::Achado, Tipo::Documento, Tipo::Registro]),
                    "Selecione documentos ou registros",
                )
            }
        }
        I::RemoverDoLote => {
            if c.lote_atual.is_none() {
                Desabilitada("Escolha um lote".to_owned())
            } else {
                exige(!sel.is_empty(), "Selecione itens do lote")
            }
        }
        I::AprovarLote | I::DesaprovarLote => exige(c.lote_atual.is_some(), "Escolha um lote"),
        I::EnviarAoServidor => Desabilitada("Chega na fatia 3, com a caixa de entrada".to_owned()),
        I::RegrasDeSenha => exige(c.pdf_ligado, MOTIVO_NAO_LIGADO),
        I::AbrirPdfProtegido if c.ferramenta_ocupada => {
            Desabilitada("Outra ferramenta está rodando".to_owned())
        }
        I::AbrirPdfProtegido => exige(c.pdf_ligado, MOTIVO_NAO_LIGADO),
        I::AtualizarEstante | I::FazerCopiaDeSeguranca => {
            let destino = if id == I::AtualizarEstante {
                c.destino_da_estante
            } else {
                c.destino_da_copia
            };
            if !c.estante_ligada {
                Desabilitada(MOTIVO_NAO_LIGADO.to_owned())
            } else if !destino {
                Desabilitada(crate::ferramentas::MOTIVO_SEM_DESTINO.to_owned())
            } else if c.ferramenta_ocupada {
                Desabilitada("Outra ferramenta está rodando".to_owned())
            } else {
                Habilitada
            }
        }
        I::AbrirOriginal | I::MostrarNoCofre => {
            if !(sel.len() == 1 && todos_de(sel, ACHADOS)) {
                Desabilitada("Selecione um achado".to_owned())
            } else {
                exige(c.cofre_aberto, "Sem cofre aberto (modo demonstração)")
            }
        }
        I::AbrirProtegido => {
            if !(sel.len() == 1 && todos_de(sel, ACHADOS)) {
                Desabilitada("Selecione um achado".to_owned())
            } else {
                exige(c.pdf_ligado, MOTIVO_NAO_LIGADO)
            }
        }
        I::VerTexto | I::CopiarHash => exige(
            sel.len() == 1 && todos_de(sel, ACHADOS),
            "Selecione um achado",
        ),
        I::AbrirEmPainelFlutuante => exige(sel.len() == 1, "Selecione um item"),
        I::CopiarRastro => exige(c.item_aberto.is_some(), "Abra a página de um item"),
    }
}

/// O filtro de estado de uma Ação do menu "Estado do achado": `Some(None)` = todos.
pub fn filtro_de_estado(id: IdAcao) -> Option<Option<&'static str>> {
    Some(match id {
        I::EstadoTodos => None,
        I::EstadoATriar => Some("novo"),
        I::EstadoDocumento => Some("documento"),
        I::EstadoEmDuvida => Some("duvida"),
        I::EstadoDescartado => Some("descartado"),
        _ => return None,
    })
}

/// O filtro de tipo de uma Ação do menu "Tipo": `Some(None)` = todos.
pub fn filtro_de_tipo(id: IdAcao) -> Option<Option<&'static str>> {
    Some(match id {
        I::TipoTodos => None,
        I::TipoBoleto => Some("boleto"),
        I::TipoFatura => Some("fatura_cartao"),
        I::TipoExtrato => Some("extrato"),
        I::TipoComprovante => Some("comprovante"),
        I::TipoNotaFiscal => Some("nota_fiscal"),
        I::TipoFoto => Some("foto"),
        I::TipoOutro => Some("outro"),
        _ => return None,
    })
}

/// As Ações de dentro dos menus de filtro (Estado do achado e Tipo).
pub fn e_filtro_de_menu(id: IdAcao) -> bool {
    filtro_de_estado(id).is_some() || filtro_de_tipo(id).is_some()
}

/// A Ação cujo atalho foi apertado neste quadro, se houver. Consome a tecla.
///
/// Com um campo de texto em foco não faz nada: Ctrl+Z ali desfaz a digitação, não uma
/// decisão.
pub fn atalho_apertado(ctx: &egui::Context) -> Option<IdAcao> {
    if ctx.egui_wants_keyboard_input() {
        return None;
    }
    ctx.input_mut(|i| {
        // Os mais específicos primeiro (Ctrl+Shift+F antes de Ctrl+F).
        let mut lista: Vec<&DefinicaoDeAcao> =
            ACOES.iter().filter(|d| d.atalho.is_some()).collect();
        lista.sort_by_key(|d| {
            let at = d
                .atalho
                .map(|a| u8::from(a.ctrl) + u8::from(a.shift) + u8::from(a.alt));
            std::cmp::Reverse(at.unwrap_or(0))
        });
        lista.into_iter().find_map(|d| {
            let atalho = d.atalho?;
            i.consume_shortcut(&atalho.atalho_egui()).then_some(d.id)
        })
    })
}

#[cfg(test)]
mod testes {
    use std::collections::HashSet;

    use super::*;

    const TODAS: &[IdAcao] = &[
        I::Voltar,
        I::Avancar,
        I::Atualizar,
        I::Pesquisar,
        I::Periodo,
        I::PeriodoAtual,
        I::PeriodoAnterior,
        I::PeriodoTodos,
        I::PeriodoIntervalo,
        I::FonteDoAchado,
        I::EstadoDoAchado,
        I::EstadoTodos,
        I::EstadoATriar,
        I::EstadoDocumento,
        I::EstadoEmDuvida,
        I::EstadoDescartado,
        I::TipoDoDocumento,
        I::TipoTodos,
        I::TipoBoleto,
        I::TipoFatura,
        I::TipoExtrato,
        I::TipoComprovante,
        I::TipoNotaFiscal,
        I::TipoFoto,
        I::TipoOutro,
        I::QuemDecidiu,
        I::QuemTodos,
        I::QuemVoce,
        I::QuemRegras,
        I::AbrirEmPainelFlutuante,
        I::Documento,
        I::Descartar,
        I::EmDuvida,
        I::CorrigirFornecedor,
        I::CorrigirCategoria,
        I::CorrigirTipo,
        I::CorrigirDataEValor,
        I::MarcarDuplicidade,
        I::UnirConciliados,
        I::ConfirmarRecorrencia,
        I::Aprovar,
        I::Rejeitar,
        I::Sinalizar,
        I::Responder,
        I::Desfazer,
        I::VerDecisoes,
        I::NovoLote,
        I::AdicionarSelecao,
        I::RemoverDoLote,
        I::AprovarLote,
        I::DesaprovarLote,
        I::EnviarAoServidor,
        I::AbrirPdfProtegido,
        I::RegrasDeSenha,
        I::NovaRegraDeSenha,
        I::AtualizarEstante,
        I::FazerCopiaDeSeguranca,
        I::PainelDetalhe,
        I::PainelOrigens,
        I::PainelTexto,
        I::PainelHistorico,
        I::TrocarTema,
        I::AbrirOriginal,
        I::MostrarNoCofre,
        I::VerTexto,
        I::AbrirProtegido,
        I::CopiarHash,
        I::CopiarRastro,
    ];

    #[test]
    fn toda_acao_esta_na_lista_uma_vez() {
        let ids: HashSet<_> = ACOES.iter().map(|d| d.id).collect();
        assert_eq!(ids.len(), ACOES.len(), "id repetido na lista");
        for id in TODAS {
            assert!(ids.contains(id), "{id:?} fora da lista");
        }
        assert_eq!(TODAS.len(), ACOES.len());
    }

    #[test]
    fn codigos_unicos_e_rotulos_preenchidos() {
        let codigos: HashSet<_> = ACOES.iter().map(|d| d.codigo).collect();
        assert_eq!(codigos.len(), ACOES.len());
        for d in ACOES {
            assert!(!d.rotulo.is_empty() && !d.dica.is_empty() && !d.grupo.is_empty());
            assert!(
                crate::icones::caractere(d.icone).is_some(),
                "ícone {}",
                d.icone
            );
        }
    }

    #[test]
    fn atalhos_sem_conflito() {
        let mut vistos = HashSet::new();
        for d in ACOES {
            if let Some(at) = d.atalho {
                assert!(vistos.insert(at), "atalho repetido: {}", at.texto());
            }
        }
    }

    #[test]
    fn enviar_ao_servidor_sempre_desabilitado_com_o_motivo() {
        let c = ContextoDasAcoes {
            lote_atual: Some(Ref::nova(Tipo::Lote, "1")),
            ..Default::default()
        };
        assert_eq!(
            disponibilidade(I::EnviarAoServidor, &c),
            Disponibilidade::Desabilitada("Chega na fatia 3, com a caixa de entrada".to_owned())
        );
    }

    #[test]
    fn triagem_exige_achados_e_aprovar_exige_registros() {
        let achado = Ref::nova(Tipo::Achado, "abc");
        let registro = Ref::nova(Tipo::Registro, "7");
        let c = ContextoDasAcoes {
            selecao: vec![achado.clone()],
            ..Default::default()
        };
        assert!(disponibilidade(I::Documento, &c).habilitada());
        assert!(!disponibilidade(I::Aprovar, &c).habilitada());
        let c = ContextoDasAcoes {
            selecao: vec![registro, achado],
            ..Default::default()
        };
        assert!(!disponibilidade(I::Documento, &c).habilitada());
        assert!(disponibilidade(I::CorrigirFornecedor, &c).habilitada());
        assert!(
            disponibilidade(I::Aprovar, &ContextoDasAcoes::default())
                .motivo()
                .is_some()
        );
    }
}
