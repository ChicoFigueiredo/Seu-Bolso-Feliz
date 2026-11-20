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
    /// Nas Sugestões de conciliação: confirma, em lote, os pares selecionados.
    ConfirmarConciliacoes,
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
    // Livro-caixa (fatia 2)
    SincronizarAgora,
    MudarCategoria,
    EditarMovimentacao,
    RatearMovimentacao,
    RefazerAlteracao,
    DescartarAlteracao,
    RevogarEstacao,
    RenomearEstacao,
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
    /// As Ações do livro-caixa e da estação (fatia 2).
    LivroCaixa,
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
            Aba::LivroCaixa => "Livro-caixa",
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

/// Uma entrada da lista única (os campos de [`DefinicaoDeAcao`], na ordem da tabela).
#[allow(clippy::too_many_arguments)]
const fn definir_acao(
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
    definir_acao(
        I::Voltar,
        "navegar.voltar",
        "Voltar",
        "arrow_back",
        Some(Atalho::alt(Key::ArrowLeft)),
        B::AcessoRapido,
        "Navegar",
        "Volta ao lugar anterior (também o botão lateral do mouse)",
    ),
    definir_acao(
        I::Avancar,
        "navegar.avancar",
        "Avançar",
        "arrow_forward",
        Some(Atalho::alt(Key::ArrowRight)),
        B::AcessoRapido,
        "Navegar",
        "Avança de novo no caminho",
    ),
    definir_acao(
        I::Desfazer,
        "historico.desfazer",
        "Desfazer decisão",
        "undo",
        Some(Atalho::ctrl(Key::Z)),
        B::AcessoRapido,
        "Histórico",
        "Desfaz a sua última decisão com uma nova decisão",
    ),
    definir_acao(
        I::Atualizar,
        "dados.atualizar",
        "Atualizar",
        "refresh",
        Some(Atalho::so(Key::F5)),
        B::AcessoRapido,
        "Dados",
        "Relê o inventário",
    ),
    definir_acao(
        I::Pesquisar,
        "inicio.pesquisar",
        "Pesquisar",
        "search",
        Some(Atalho::ctrl(Key::F)),
        B::Inicio,
        "Pesquisa",
        "Pesquisa nesta área",
    ),
    definir_acao(
        I::Periodo,
        "inicio.periodo",
        "Período",
        "date_range",
        None,
        B::Inicio,
        "Período",
        "Ciclo atual, ciclo anterior ou todos os ciclos",
    ),
    definir_acao(
        I::PeriodoAtual,
        "inicio.periodo.atual",
        "Ciclo atual",
        "today",
        None,
        B::Inicio,
        "Período",
        "Só o ciclo atual",
    ),
    definir_acao(
        I::PeriodoAnterior,
        "inicio.periodo.anterior",
        "Ciclo anterior",
        "history",
        None,
        B::Inicio,
        "Período",
        "Só o ciclo anterior",
    ),
    definir_acao(
        I::PeriodoTodos,
        "inicio.periodo.todos",
        "Todos os ciclos",
        "calendar_month",
        None,
        B::Inicio,
        "Período",
        "Sem filtro de ciclo",
    ),
    definir_acao(
        I::PeriodoIntervalo,
        "inicio.periodo.intervalo",
        "Intervalo…",
        "date_range",
        None,
        B::Inicio,
        "Período",
        "Do ciclo inicial ao ciclo final",
    ),
    definir_acao(
        I::FonteDoAchado,
        "inicio.fonte",
        "Conexão",
        "cable",
        None,
        B::Inicio,
        "Filtros",
        "Só o que veio de uma conexão",
    ),
    definir_acao(
        I::EstadoDoAchado,
        "inicio.estado",
        "Estado do achado",
        "filter_alt",
        None,
        B::Inicio,
        "Filtros",
        "Só os achados num estado",
    ),
    definir_acao(
        I::EstadoTodos,
        "inicio.estado.todos",
        "Todos os estados",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Sem filtro de estado",
    ),
    definir_acao(
        I::EstadoATriar,
        "inicio.estado.a-triar",
        "A triar",
        "schedule",
        None,
        B::Inicio,
        "Filtros",
        "Só os achados a triar",
    ),
    definir_acao(
        I::EstadoDocumento,
        "inicio.estado.documento",
        "Documento",
        "task_alt",
        None,
        B::Inicio,
        "Filtros",
        "Só os documentos",
    ),
    definir_acao(
        I::EstadoEmDuvida,
        "inicio.estado.em-duvida",
        "Em dúvida",
        "help",
        None,
        B::Inicio,
        "Filtros",
        "Só os achados em dúvida",
    ),
    definir_acao(
        I::EstadoDescartado,
        "inicio.estado.descartado",
        "Descartado",
        "block",
        None,
        B::Inicio,
        "Filtros",
        "Só os descartados",
    ),
    definir_acao(
        I::TipoDoDocumento,
        "inicio.tipo",
        "Tipo",
        "label",
        None,
        B::Inicio,
        "Filtros",
        "Só um tipo de documento",
    ),
    definir_acao(
        I::TipoTodos,
        "inicio.tipo.todos",
        "Todos os tipos",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Sem filtro de tipo",
    ),
    definir_acao(
        I::TipoBoleto,
        "inicio.tipo.boleto",
        "Boleto",
        "receipt_long",
        None,
        B::Inicio,
        "Filtros",
        "Só boletos",
    ),
    definir_acao(
        I::TipoFatura,
        "inicio.tipo.fatura",
        "Fatura de cartão",
        "credit_card",
        None,
        B::Inicio,
        "Filtros",
        "Só faturas de cartão",
    ),
    definir_acao(
        I::TipoExtrato,
        "inicio.tipo.extrato",
        "Extrato",
        "account_balance",
        None,
        B::Inicio,
        "Filtros",
        "Só extratos",
    ),
    definir_acao(
        I::TipoComprovante,
        "inicio.tipo.comprovante",
        "Comprovante",
        "description",
        None,
        B::Inicio,
        "Filtros",
        "Só comprovantes",
    ),
    definir_acao(
        I::TipoNotaFiscal,
        "inicio.tipo.nota-fiscal",
        "Nota fiscal",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Só notas fiscais",
    ),
    definir_acao(
        I::TipoFoto,
        "inicio.tipo.foto",
        "Foto",
        "find_in_page",
        None,
        B::Inicio,
        "Filtros",
        "Só fotos",
    ),
    definir_acao(
        I::TipoOutro,
        "inicio.tipo.outro",
        "Outro",
        "category",
        None,
        B::Inicio,
        "Filtros",
        "Só os de tipo outro",
    ),
    definir_acao(
        I::QuemDecidiu,
        "inicio.quem-decidiu",
        "Quem decidiu",
        "person",
        None,
        B::Inicio,
        "Filtros",
        "Só o que você ou os padrões decidiram",
    ),
    definir_acao(
        I::QuemTodos,
        "inicio.quem.todos",
        "Todos",
        "list_alt",
        None,
        B::Inicio,
        "Filtros",
        "Decisões de todos",
    ),
    definir_acao(
        I::QuemVoce,
        "inicio.quem.voce",
        "Você",
        "person",
        None,
        B::Inicio,
        "Filtros",
        "Só o que você decidiu",
    ),
    definir_acao(
        I::QuemRegras,
        "inicio.quem.regras",
        "Padrões",
        "smart_toy",
        None,
        B::Inicio,
        "Filtros",
        "Só o que os padrões decidiram",
    ),
    definir_acao(
        I::Documento,
        "triagem.documento",
        "Documento",
        "task_alt",
        Some(Atalho::ctrl(Key::Num1)),
        B::Triagem,
        "Triagem",
        "Marca os achados selecionados como documento",
    ),
    definir_acao(
        I::Descartar,
        "triagem.descartar",
        "Descartar",
        "block",
        Some(Atalho::ctrl(Key::Num2)),
        B::Triagem,
        "Triagem",
        "Marca os achados selecionados como descartados",
    ),
    definir_acao(
        I::EmDuvida,
        "triagem.em-duvida",
        "Em dúvida",
        "help",
        Some(Atalho::ctrl(Key::Num3)),
        B::Triagem,
        "Triagem",
        "Deixa os achados selecionados em dúvida",
    ),
    definir_acao(
        I::CorrigirFornecedor,
        "corrigir.fornecedor",
        "Corrigir fornecedor",
        "storefront",
        Some(Atalho::ctrl_shift(Key::F)),
        B::Triagem,
        "Corrigir",
        "Corrige o fornecedor do achado ou do registro",
    ),
    definir_acao(
        I::CorrigirCategoria,
        "corrigir.categoria",
        "Corrigir categoria",
        "category",
        Some(Atalho::ctrl_shift(Key::C)),
        B::Triagem,
        "Corrigir",
        "Corrige a categoria do registro",
    ),
    definir_acao(
        I::CorrigirTipo,
        "corrigir.tipo",
        "Corrigir tipo",
        "label",
        Some(Atalho::ctrl_shift(Key::T)),
        B::Triagem,
        "Corrigir",
        "Corrige o tipo do achado ou do registro",
    ),
    definir_acao(
        I::CorrigirDataEValor,
        "corrigir.data-e-valor",
        "Corrigir data e valor",
        "edit_note",
        Some(Atalho::ctrl_shift(Key::V)),
        B::Triagem,
        "Corrigir",
        "Corrige a data e o valor do achado; o ciclo é recalculado",
    ),
    definir_acao(
        I::MarcarDuplicidade,
        "relacionar.duplicidade",
        "Marcar duplicidade",
        "difference",
        Some(Atalho::ctrl_shift(Key::D)),
        B::Triagem,
        "Relacionar",
        "Marca dois achados como duplicidade um do outro",
    ),
    definir_acao(
        I::UnirConciliados,
        "relacionar.conciliar",
        "Unir conciliados",
        "call_merge",
        Some(Atalho::ctrl_shift(Key::U)),
        B::Triagem,
        "Relacionar",
        "Reconhece que dois registros descrevem o mesmo fato",
    ),
    definir_acao(
        I::ConfirmarConciliacoes,
        "relacionar.confirmar-conciliacoes",
        "Confirmar conciliações",
        "call_merge",
        None,
        B::Triagem,
        "Relacionar",
        "Confirma, em lote, os pares sugeridos selecionados: uma decisão por par",
    ),
    definir_acao(
        I::ConfirmarRecorrencia,
        "relacionar.recorrencia",
        "Confirmar recorrência",
        "event_repeat",
        None,
        B::Triagem,
        "Relacionar",
        "Confirma que o fornecedor se repete todo ciclo",
    ),
    definir_acao(
        I::Aprovar,
        "interpretacao.aprovar",
        "Aprovar",
        "thumb_up",
        Some(Atalho::ctrl(Key::Enter)),
        B::Triagem,
        "Interpretação",
        "Aprova os registros selecionados",
    ),
    definir_acao(
        I::Rejeitar,
        "interpretacao.rejeitar",
        "Rejeitar",
        "thumb_down",
        Some(Atalho::ctrl_shift(Key::Enter)),
        B::Triagem,
        "Interpretação",
        "Rejeita os registros selecionados, com motivo",
    ),
    definir_acao(
        I::Sinalizar,
        "pendencia.sinalizar",
        "Sinalizar",
        "flag",
        Some(Atalho::ctrl_shift(Key::S)),
        B::Triagem,
        "Pendência",
        "Abre uma pendência sobre o item",
    ),
    definir_acao(
        I::Responder,
        "pendencia.responder",
        "Responder",
        "reply",
        Some(Atalho::ctrl_shift(Key::R)),
        B::Triagem,
        "Pendência",
        "Responde a pendência selecionada",
    ),
    definir_acao(
        I::VerDecisoes,
        "historico.ver-decisoes",
        "Ver decisões",
        "history",
        Some(Atalho::ctrl(Key::H)),
        B::Triagem,
        "Histórico",
        "Mostra o Histórico de decisões",
    ),
    definir_acao(
        I::NovoLote,
        "lotes.novo",
        "Novo lote",
        "inventory_2",
        Some(Atalho::ctrl_shift(Key::N)),
        B::Lotes,
        "Lote",
        "Cria um lote com nome",
    ),
    definir_acao(
        I::AdicionarSelecao,
        "lotes.adicionar",
        "Adicionar seleção",
        "playlist_add",
        Some(Atalho::ctrl_shift(Key::A)),
        B::Lotes,
        "Lote",
        "Põe os documentos e registros selecionados no lote atual",
    ),
    definir_acao(
        I::RemoverDoLote,
        "lotes.remover",
        "Remover",
        "playlist_remove",
        None,
        B::Lotes,
        "Lote",
        "Tira os itens selecionados do lote atual",
    ),
    definir_acao(
        I::AprovarLote,
        "lotes.aprovar",
        "Aprovar lote",
        "check_circle",
        None,
        B::Lotes,
        "Aprovação",
        "Aprova o lote atual: os registros dele ficam aprovados",
    ),
    definir_acao(
        I::DesaprovarLote,
        "lotes.desaprovar",
        "Desaprovar lote",
        "do_not_disturb_on",
        None,
        B::Lotes,
        "Aprovação",
        "Volta o lote atual para editável, por uma nova decisão",
    ),
    definir_acao(
        I::EnviarAoServidor,
        "lotes.enviar",
        "Enviar ao servidor",
        "send",
        None,
        B::Lotes,
        "Promoção",
        "Promove o lote aprovado para a caixa de entrada",
    ),
    definir_acao(
        I::AbrirPdfProtegido,
        "ferramentas.pdf-protegido",
        "Abrir PDF protegido",
        "lock_open",
        None,
        B::Ferramentas,
        "PDF protegido",
        "Tenta abrir os PDFs protegidos pelas regras de senha",
    ),
    definir_acao(
        I::RegrasDeSenha,
        "ferramentas.regras-de-senha",
        "Regras de senha",
        "key",
        None,
        B::Ferramentas,
        "PDF protegido",
        "As regras de senha por fornecedor e tipo",
    ),
    definir_acao(
        I::NovaRegraDeSenha,
        "ferramentas.nova-regra",
        "Nova regra",
        "add",
        None,
        B::Ferramentas,
        "PDF protegido",
        "Cria uma regra de senha: fornecedor, tipo e estratégia (nunca a senha)",
    ),
    definir_acao(
        I::AtualizarEstante,
        "ferramentas.estante",
        "Atualizar estante",
        "backup",
        None,
        B::Ferramentas,
        "Estante e cópia",
        "Copia os documentos para a estante no Google Drive",
    ),
    definir_acao(
        I::FazerCopiaDeSeguranca,
        "ferramentas.copia",
        "Fazer cópia de segurança",
        "enhanced_encryption",
        None,
        B::Ferramentas,
        "Estante e cópia",
        "Cópia cifrada do cofre e do banco",
    ),
    definir_acao(
        I::PainelDetalhe,
        "exibir.detalhe",
        "Detalhe",
        "info",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Detalhe",
    ),
    definir_acao(
        I::PainelOrigens,
        "exibir.origens",
        "Origens",
        "folder_open",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Origens",
    ),
    definir_acao(
        I::PainelTexto,
        "exibir.texto",
        "Texto extraído",
        "description",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Texto extraído",
    ),
    definir_acao(
        I::PainelHistorico,
        "exibir.historico",
        "Histórico de decisões",
        "history",
        None,
        B::Exibir,
        "Painéis",
        "Mostra ou esconde o painel Histórico de decisões",
    ),
    definir_acao(
        I::TrocarTema,
        "exibir.tema",
        "Tema",
        "palette",
        None,
        B::Exibir,
        "Aparência",
        "Escolhe um dos 7 temas do SBF",
    ),
    definir_acao(
        I::AbrirOriginal,
        "achado.abrir-original",
        "Abrir original",
        "open_in_new",
        None,
        B::Achado,
        "Original",
        "Abre o arquivo original do cofre",
    ),
    definir_acao(
        I::MostrarNoCofre,
        "achado.mostrar-no-cofre",
        "Mostrar no cofre",
        "folder",
        None,
        B::Achado,
        "Original",
        "Mostra o arquivo na pasta do cofre",
    ),
    definir_acao(
        I::VerTexto,
        "achado.ver-texto",
        "Ver texto",
        "description",
        None,
        B::Achado,
        "Original",
        "Mostra o texto extraído",
    ),
    definir_acao(
        I::AbrirProtegido,
        "achado.abrir-protegido",
        "Abrir protegido",
        "lock",
        None,
        B::Achado,
        "Original",
        "Abre o PDF protegido pela regra de senha",
    ),
    definir_acao(
        I::CopiarHash,
        "achado.copiar-hash",
        "Copiar hash",
        "fingerprint",
        None,
        B::Achado,
        "Identidade",
        "Copia o hash do conteúdo",
    ),
    definir_acao(
        I::AbrirEmPainelFlutuante,
        "pagina.flutuante",
        "Abrir em painel flutuante",
        "picture_in_picture",
        Some(Atalho::ctrl_shift(Key::O)),
        B::Pagina,
        "Janela",
        "Abre o item selecionado num painel flutuante (ou Ctrl+clique)",
    ),
    definir_acao(
        I::CopiarRastro,
        "pagina.copiar-rastro",
        "Copiar rastro",
        "content_copy",
        None,
        B::Pagina,
        "Rastro",
        "Copia a cadeia de proveniência do item como texto",
    ),
    definir_acao(
        I::SincronizarAgora,
        "livro.sincronizar",
        "Sincronizar agora",
        "cloud_sync",
        Some(Atalho::so(Key::F9)),
        B::LivroCaixa,
        "Sincronização",
        "Troca com o servidor agora: manda as alterações e traz o que mudou",
    ),
    definir_acao(
        I::MudarCategoria,
        "livro.mudar-categoria",
        "Mudar categoria",
        "category",
        None,
        B::LivroCaixa,
        "Movimentação",
        "Muda a categoria da movimentação; vai ao servidor na próxima sincronização",
    ),
    definir_acao(
        I::EditarMovimentacao,
        "livro.editar",
        "Editar movimentação",
        "edit",
        None,
        B::LivroCaixa,
        "Movimentação",
        "Data, valor, conta e descrição",
    ),
    definir_acao(
        I::RatearMovimentacao,
        "livro.ratear",
        "Ratear",
        "donut_small",
        None,
        B::LivroCaixa,
        "Movimentação",
        "Divide o valor entre centros",
    ),
    definir_acao(
        I::RefazerAlteracao,
        "livro.refazer",
        "Refazer sobre a nova",
        "redo",
        None,
        B::LivroCaixa,
        "Alterações",
        "Aplica a sua mudança recusada sobre o que está no servidor agora",
    ),
    definir_acao(
        I::DescartarAlteracao,
        "livro.descartar",
        "Descartar alteração",
        "delete",
        None,
        B::LivroCaixa,
        "Alterações",
        "Mantém a versão do servidor e descarta a sua",
    ),
    definir_acao(
        I::RevogarEstacao,
        "livro.revogar",
        "Revogar estação",
        "key_off",
        None,
        B::LivroCaixa,
        "Estações",
        "Corta a credencial da estação; não apaga nada do servidor",
    ),
    definir_acao(
        I::RenomearEstacao,
        "livro.renomear",
        "Renomear estação",
        "edit_note",
        None,
        B::LivroCaixa,
        "Estações",
        "Muda o nome com que a estação aparece",
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
    /// Há uma estação (livro-caixa) ligada a esta janela.
    pub estacao_ligada: bool,
    /// A estação desta máquina está revogada.
    pub estacao_revogada: bool,
    /// A seleção é uma alteração recusada.
    pub alteracao_recusada: bool,
    /// A seleção é uma estação com credencial ativa.
    pub estacao_ativa: bool,
    /// A área aberta é a das Sugestões de conciliação.
    pub nas_sugestoes: bool,
    /// O envio do lote atual ao servidor (fatia 3).
    pub envio_do_lote: EnvioNoContexto,
    /// A caixa de entrada e a edição estão ligadas (Editar e Ratear a movimentação).
    pub edicao_ligada: bool,
}

/// Se Enviar ao servidor pode rodar no lote atual.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum EnvioNoContexto {
    /// A caixa não está ligada a esta janela.
    #[default]
    NaoLigado,
    Pronto,
    /// Por que não: "Escolha um lote", "O lote ainda não está aprovado"…
    Indisponivel(String),
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
        I::ConfirmarConciliacoes => exige(
            c.nas_sugestoes && todos_de(sel, &[Tipo::Registro]),
            "Abra Sugestões de conciliação e selecione os pares",
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
        I::EnviarAoServidor => match &c.envio_do_lote {
            EnvioNoContexto::NaoLigado => Desabilitada(MOTIVO_NAO_LIGADO.to_owned()),
            EnvioNoContexto::Pronto => Habilitada,
            EnvioNoContexto::Indisponivel(m) => Desabilitada(m.clone()),
        },
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
        I::SincronizarAgora => {
            if !c.estacao_ligada {
                Desabilitada(MOTIVO_SEM_ESTACAO.to_owned())
            } else {
                exige(!c.estacao_revogada, "Esta estação foi revogada")
            }
        }
        I::MudarCategoria => {
            if !c.estacao_ligada {
                Desabilitada(MOTIVO_SEM_ESTACAO.to_owned())
            } else if c.estacao_revogada {
                Desabilitada("Esta estação foi revogada".to_owned())
            } else {
                exige(
                    sel.len() == 1 && todos_de(sel, &[Tipo::Movimentacao]),
                    "Selecione uma movimentação",
                )
            }
        }
        I::EditarMovimentacao | I::RatearMovimentacao => {
            if !c.edicao_ligada {
                Desabilitada(crate::dinheiro::MOTIVO_FATIA_3.to_owned())
            } else {
                exige(
                    sel.len() == 1 && todos_de(sel, &[Tipo::Movimentacao]),
                    "Selecione uma movimentação",
                )
            }
        }
        I::RefazerAlteracao | I::DescartarAlteracao => exige(
            c.alteracao_recusada,
            "Selecione uma alteração recusada (área Sincronização)",
        ),
        I::RevogarEstacao => exige(
            c.estacao_ativa,
            "Selecione uma estação com credencial ativa (área Sincronização)",
        ),
        I::RenomearEstacao => exige(
            sel.len() == 1 && todos_de(sel, &[Tipo::Estacao]) && c.estacao_ligada,
            "Selecione uma estação (área Sincronização)",
        ),
    }
}

/// O motivo das Ações do livro-caixa sem estação ligada.
pub const MOTIVO_SEM_ESTACAO: &str =
    "Esta máquina ainda não é uma estação: pareie com o servidor na área Sincronização";

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
        I::ConfirmarConciliacoes,
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
        I::SincronizarAgora,
        I::MudarCategoria,
        I::EditarMovimentacao,
        I::RatearMovimentacao,
        I::RefazerAlteracao,
        I::DescartarAlteracao,
        I::RevogarEstacao,
        I::RenomearEstacao,
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
    fn mudar_categoria_exige_uma_movimentacao_e_a_estacao() {
        let mov = Ref::nova(Tipo::Movimentacao, "m1");
        let mut c = ContextoDasAcoes {
            selecao: vec![mov.clone()],
            ..Default::default()
        };
        assert_eq!(
            disponibilidade(I::MudarCategoria, &c).motivo(),
            Some(MOTIVO_SEM_ESTACAO)
        );
        c.estacao_ligada = true;
        assert!(disponibilidade(I::MudarCategoria, &c).habilitada());
        c.selecao = vec![mov.clone(), Ref::nova(Tipo::Movimentacao, "m2")];
        assert!(!disponibilidade(I::MudarCategoria, &c).habilitada());
        c.selecao = vec![mov];
        c.estacao_revogada = true;
        assert!(!disponibilidade(I::MudarCategoria, &c).habilitada());
        // A edição do resto do livro-caixa só liga com a caixa, e diz o motivo.
        assert_eq!(
            disponibilidade(I::EditarMovimentacao, &c).motivo(),
            Some(crate::dinheiro::MOTIVO_FATIA_3)
        );
        c.edicao_ligada = true;
        assert!(disponibilidade(I::EditarMovimentacao, &c).habilitada());
        assert!(disponibilidade(I::RatearMovimentacao, &c).habilitada());
        c.selecao.clear();
        assert!(!disponibilidade(I::RatearMovimentacao, &c).habilitada());
    }

    #[test]
    fn enviar_ao_servidor_segue_o_contexto_da_caixa() {
        let mut c = ContextoDasAcoes {
            lote_atual: Some(Ref::nova(Tipo::Lote, "1")),
            ..Default::default()
        };
        assert_eq!(
            disponibilidade(I::EnviarAoServidor, &c),
            Disponibilidade::Desabilitada(MOTIVO_NAO_LIGADO.to_owned())
        );
        c.envio_do_lote = EnvioNoContexto::Indisponivel("O lote ainda não está aprovado".into());
        assert_eq!(
            disponibilidade(I::EnviarAoServidor, &c),
            Disponibilidade::Desabilitada("O lote ainda não está aprovado".to_owned())
        );
        c.envio_do_lote = EnvioNoContexto::Pronto;
        assert!(disponibilidade(I::EnviarAoServidor, &c).habilitada());
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
